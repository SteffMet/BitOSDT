use crate::build::{default_simple_publish_path, default_simple_runtime_url};
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use std::path::{Component, Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn serve_lightweight_tree(
    staging_path: &Path,
    bind_address: &str,
    base_url: &str,
) -> BitOSDTResult<()> {
    if !staging_path.is_dir() {
        return Err(BitOSDTError::NotFound(format!(
            "Lightweight staging path does not exist: {}",
            staging_path.display()
        )));
    }

    let listener = TcpListener::bind(bind_address)
        .await
        .map_err(|e| BitOSDTError::Network(format!("Failed to bind {}: {}", bind_address, e)))?;

    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.map_err(|e| BitOSDTError::Unknown(format!("Failed to listen for Ctrl+C: {}", e)))?;
                return Ok(());
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.map_err(|e| {
                    BitOSDTError::Network(format!("Failed to accept lightweight host connection: {}", e))
                })?;
                let staging = staging_path.to_path_buf();
                let base = base_url.to_string();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, &staging, &base).await;
                });
            }
        }
    }
}

pub fn default_lightweight_bind_address() -> &'static str {
    "0.0.0.0:8080"
}

pub fn resolve_default_lightweight_host_settings() -> BitOSDTResult<(PathBuf, String, String)> {
    Ok((
        default_simple_publish_path()
            .map_err(|e| BitOSDTError::Config(crate::core::errors::ConfigError::LoadFailed(e)))?,
        default_lightweight_bind_address().to_string(),
        default_simple_runtime_url(),
    ))
}

async fn handle_connection(
    mut stream: TcpStream,
    staging_path: &Path,
    base_url: &str,
) -> BitOSDTResult<()> {
    let mut buffer = vec![0u8; 8192];
    let bytes_read = stream.read(&mut buffer).await?;
    if bytes_read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_line = request.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/").split('?').next().unwrap_or("/");

    if method != "GET" {
        write_http_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
        )
        .await?;
        return Ok(());
    }

    if path == "/health" {
        let body = serde_json::json!({
            "status": "ok",
            "mode": "simple",
            "baseUrl": base_url,
            "stagingPath": staging_path.to_string_lossy(),
        })
        .to_string();
        write_http_response(
            &mut stream,
            200,
            "OK",
            "application/json; charset=utf-8",
            body.as_bytes(),
        )
        .await?;
        return Ok(());
    }

    if path == "/api/manifest" {
        let manifest_path = staging_path.join("manifest.json");
        let body = if manifest_path.is_file() {
            tokio::fs::read(&manifest_path).await?
        } else {
            serde_json::json!({
                "name": "BitOSDT Lightweight Runtime",
                "mode": "simple",
                "baseUrl": base_url,
                "healthUrl": format!("{}/health", base_url),
                "downloadUrl": format!("{}/download/bitosdt.exe", base_url),
                "generatedAtUtc": chrono::Utc::now().to_rfc3339(),
            })
            .to_string()
            .into_bytes()
        };
        write_http_response(
            &mut stream,
            200,
            "OK",
            "application/json; charset=utf-8",
            &body,
        )
        .await?;
        return Ok(());
    }

    let relative_path = if path == "/" {
        PathBuf::from("manifest.json")
    } else {
        sanitize_relative_request_path(path)?
    };
    let file_path = staging_path.join(relative_path);

    if !file_path.is_file() {
        write_http_response(
            &mut stream,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            b"Not Found",
        )
        .await?;
        return Ok(());
    }

    let content_type = if path.ends_with(".json") {
        "application/json; charset=utf-8"
    } else {
        "application/octet-stream"
    };
    let body = tokio::fs::read(&file_path).await?;
    write_http_response(&mut stream, 200, "OK", content_type, &body).await?;
    Ok(())
}

fn sanitize_relative_request_path(path: &str) -> BitOSDTResult<PathBuf> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(BitOSDTError::InvalidInput(
            "Request path cannot be empty".to_string(),
        ));
    }

    let candidate = Path::new(trimmed);
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        return Err(BitOSDTError::InvalidInput(format!(
            "Invalid request path: {}",
            path
        )));
    }

    Ok(candidate.to_path_buf())
}

async fn write_http_response(
    stream: &mut TcpStream,
    status_code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> BitOSDTResult<()> {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_code,
        reason,
        content_type,
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.shutdown().await?;
    Ok(())
}
