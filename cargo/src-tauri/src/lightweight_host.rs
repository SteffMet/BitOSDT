use serde::{Deserialize, Serialize};
use std::net::UdpSocket;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tracing::{info, warn};

const SIMPLE_BIND_HOST: &str = "0.0.0.0";
const SIMPLE_PORT: u16 = 8080;
pub const SIMPLE_BIND_ADDRESS: &str = "0.0.0.0:8080";
pub const SIMPLE_PUBLISH_FOLDER: &str = "pxe-simple";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleDeliveryDefaults {
    pub runtime_url: String,
    pub publish_path: String,
    pub bind_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightweightHostStatus {
    pub running: bool,
    pub base_url: String,
    pub bind_address: String,
    pub staging_path: String,
    pub last_error: Option<String>,
}

struct LightweightHostInner {
    running: bool,
    base_url: String,
    bind_address: String,
    staging_path: PathBuf,
    last_error: Option<String>,
    shutdown: Option<Arc<Notify>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Clone)]
pub struct LightweightHostState {
    inner: Arc<Mutex<LightweightHostInner>>,
}

impl LightweightHostState {
    pub fn new() -> Self {
        let defaults = resolve_simple_delivery_defaults().unwrap_or(SimpleDeliveryDefaults {
            runtime_url: format!("http://127.0.0.1:{SIMPLE_PORT}"),
            publish_path: "C:\\BitOSDT\\Workspace\\pxe-simple".to_string(),
            bind_address: SIMPLE_BIND_ADDRESS.to_string(),
        });

        Self {
            inner: Arc::new(Mutex::new(LightweightHostInner {
                running: false,
                base_url: defaults.runtime_url.clone(),
                bind_address: defaults.bind_address,
                staging_path: PathBuf::from(defaults.publish_path),
                last_error: None,
                shutdown: None,
                task: None,
            })),
        }
    }

    pub fn status(&self) -> LightweightHostStatus {
        let defaults = resolve_simple_delivery_defaults().ok();
        let inner = self.inner.lock().expect("lightweight host state poisoned");
        LightweightHostStatus {
            running: inner.running,
            base_url: if inner.base_url.trim().is_empty() {
                defaults
                    .as_ref()
                    .map(|value| value.runtime_url.clone())
                    .unwrap_or_default()
            } else {
                inner.base_url.clone()
            },
            bind_address: if inner.bind_address.trim().is_empty() {
                defaults
                    .as_ref()
                    .map(|value| value.bind_address.clone())
                    .unwrap_or_else(|| SIMPLE_BIND_ADDRESS.to_string())
            } else {
                inner.bind_address.clone()
            },
            staging_path: if inner.staging_path.as_os_str().is_empty() {
                defaults
                    .as_ref()
                    .map(|value| value.publish_path.clone())
                    .unwrap_or_default()
            } else {
                inner.staging_path.to_string_lossy().to_string()
            },
            last_error: inner.last_error.clone(),
        }
    }
}

impl Default for LightweightHostState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn resolve_simple_delivery_defaults() -> Result<SimpleDeliveryDefaults, String> {
    let publish_path = default_simple_publish_path()?;
    Ok(SimpleDeliveryDefaults {
        runtime_url: default_simple_runtime_url(),
        publish_path: publish_path.to_string_lossy().to_string(),
        bind_address: SIMPLE_BIND_ADDRESS.to_string(),
    })
}

pub fn default_simple_publish_path() -> Result<PathBuf, String> {
    let workspace = bitosdt::core::Config::configured_workspace_path()
        .map_err(|e| format!("Failed to resolve BitOSDT workspace path: {}", e))?;
    Ok(workspace.join(SIMPLE_PUBLISH_FOLDER))
}

pub fn default_simple_runtime_url() -> String {
    let host = preferred_hostname()
        .or_else(local_ipv4_fallback)
        .unwrap_or_else(|| "127.0.0.1".to_string());
    format!("http://{}:{}", host, SIMPLE_PORT)
}

pub async fn ensure_lightweight_host_running(
    state: &LightweightHostState,
    staging_path: &Path,
    base_url: &str,
) -> Result<LightweightHostStatus, String> {
    if !staging_path.is_dir() {
        return Err(format!(
            "PXE staging path does not exist: {}",
            staging_path.display()
        ));
    }

    stop_lightweight_host(state).await?;

    let listener = TcpListener::bind(SIMPLE_BIND_ADDRESS).await.map_err(|e| {
        format!(
            "Failed to bind embedded lightweight host on {}: {}",
            SIMPLE_BIND_ADDRESS, e
        )
    })?;
    let shutdown = Arc::new(Notify::new());
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let state_clone = state.clone();
    let shutdown_clone = shutdown.clone();
    let staging_clone = staging_path.to_path_buf();
    let base_url_string = base_url.to_string();
    let bind_address = SIMPLE_BIND_ADDRESS.to_string();

    let task = tokio::spawn(async move {
        // Signal ready after TcpListener::bind succeeds (already bound before spawn)
        let _ = ready_tx.send(());

        let result = run_lightweight_host(
            listener,
            shutdown_clone,
            staging_clone.clone(),
            base_url_string.clone(),
        )
        .await;
        let mut inner = state_clone
            .inner
            .lock()
            .expect("lightweight host state poisoned");
        inner.running = false;
        inner.shutdown = None;
        inner.task = None;
        if let Err(err) = result {
            inner.last_error = Some(err);
        }
    });

    // Wait for the ready signal before returning
    ready_rx
        .await
        .map_err(|e| format!("Lightweight host ready signal failed: {}", e))?;

    let mut inner = state.inner.lock().expect("lightweight host state poisoned");
    inner.running = true;
    inner.base_url = base_url.to_string();
    inner.bind_address = bind_address;
    inner.staging_path = staging_path.to_path_buf();
    inner.last_error = None;
    inner.shutdown = Some(shutdown);
    inner.task = Some(task);
    drop(inner);

    info!(
        "Lightweight HTTP server started on {} serving {}",
        SIMPLE_BIND_ADDRESS,
        staging_path.display()
    );

    Ok(state.status())
}

pub async fn stop_lightweight_host(state: &LightweightHostState) -> Result<(), String> {
    let (shutdown, task) = {
        let mut inner = state.inner.lock().expect("lightweight host state poisoned");
        inner.running = false;
        (inner.shutdown.take(), inner.task.take())
    };

    if let Some(shutdown) = shutdown {
        shutdown.notify_waiters();
    }

    if let Some(task) = task {
        let _ = task.await;
    }

    Ok(())
}

async fn run_lightweight_host(
    listener: TcpListener,
    shutdown: Arc<Notify>,
    staging_path: PathBuf,
    base_url: String,
) -> Result<(), String> {
    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                return Ok(());
            }
            accept_result = listener.accept() => {
                let (stream, _) = accept_result.map_err(|e| format!("Embedded lightweight host accept failed: {}", e))?;
                let staging_path = staging_path.clone();
                let base_url = base_url.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, &staging_path, &base_url).await;
                });
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    staging_path: &Path,
    base_url: &str,
) -> Result<(), String> {
    let mut buffer = vec![0u8; 8192];
    let bytes_read = stream
        .read(&mut buffer)
        .await
        .map_err(|e| format!("Failed to read host request: {}", e))?;
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
            "bindAddress": SIMPLE_BIND_ADDRESS,
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
            tokio::fs::read(&manifest_path).await.map_err(|e| {
                format!("Failed to read manifest {}: {}", manifest_path.display(), e)
            })?
        } else {
            build_manifest_json(base_url).into_bytes()
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

    if path == "/download/bitosdt.exe" {
        let exe_path = staging_path.join("download").join("bitosdt.exe");
        if !exe_path.is_file() {
            write_http_response(
                &mut stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"bitosdt.exe not found",
            )
            .await?;
        } else {
            let body = tokio::fs::read(&exe_path).await.map_err(|e| {
                format!(
                    "Failed to read runtime executable {}: {}",
                    exe_path.display(),
                    e
                )
            })?;
            write_http_response(&mut stream, 200, "OK", "application/octet-stream", &body).await?;
        }
        return Ok(());
    }

    // Serve arbitrary files from the staging directory (e.g., DriverCache, etc.)
    let relative_path = if path == "/" {
        PathBuf::from("manifest.json")
    } else {
        sanitize_relative_request_path(path)?
    };
    let file_path = staging_path.join(&relative_path);

    info!("HTTP request: {} -> {}", path, file_path.display());

    if !file_path.is_file() {
        warn!("HTTP 404: {} (resolved to {})", path, file_path.display());
        write_http_response(
            &mut stream,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            format!("Not Found: {}", path).as_bytes(),
        )
        .await?;
        return Ok(());
    }

    let content_type = guess_content_type(&relative_path);
    let body = tokio::fs::read(&file_path)
        .await
        .map_err(|e| format!("Failed to read file {}: {}", file_path.display(), e))?;
    info!(
        "HTTP 200: {} -> {} ({} bytes)",
        path,
        file_path.display(),
        body.len()
    );
    write_http_response(&mut stream, 200, "OK", content_type, &body).await?;
    Ok(())
}

fn sanitize_relative_request_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Err("Request path cannot be empty".to_string());
    }

    let candidate = Path::new(trimmed);
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        return Err(format!("Invalid request path: {}", path));
    }

    Ok(candidate.to_path_buf())
}

fn guess_content_type(path: &Path) -> &'static str {
    let path_str = path.to_string_lossy().to_lowercase();
    if path_str.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if path_str.ends_with(".xml") {
        "application/xml; charset=utf-8"
    } else if path_str.ends_with(".cab") || path_str.ends_with(".zip") || path_str.ends_with(".7z")
    {
        "application/octet-stream"
    } else if path_str.ends_with(".ps1") {
        "text/plain; charset=utf-8"
    } else if path_str.ends_with(".cmd") || path_str.ends_with(".bat") {
        "text/plain; charset=utf-8"
    } else if path_str.ends_with(".exe") {
        "application/octet-stream"
    } else {
        "application/octet-stream"
    }
}

async fn write_http_response(
    stream: &mut TcpStream,
    status_code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_code,
        reason,
        content_type,
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .await
        .map_err(|e| format!("Failed to write host response header: {}", e))?;
    stream
        .write_all(body)
        .await
        .map_err(|e| format!("Failed to write host response body: {}", e))?;
    stream
        .shutdown()
        .await
        .map_err(|e| format!("Failed to close host response stream: {}", e))?;
    Ok(())
}

pub fn build_manifest_json(base_url: &str) -> String {
    serde_json::json!({
        "name": "BitOSDT Lightweight Runtime",
        "mode": "simple",
        "baseUrl": base_url,
        "healthUrl": format!("{}/health", base_url),
        "downloadUrl": format!("{}/download/bitosdt.exe", base_url),
        "generatedAtUtc": chrono::Utc::now().to_rfc3339(),
    })
    .to_string()
}

fn preferred_hostname() -> Option<String> {
    for key in ["BITOSDT_SIMPLE_HOSTNAME", "COMPUTERNAME", "HOSTNAME"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

fn local_ipv4_fallback() -> Option<String> {
    let socket = UdpSocket::bind((SIMPLE_BIND_HOST, 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    let address = socket.local_addr().ok()?;
    let ip = address.ip();
    if ip.is_ipv4() {
        Some(ip.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_manifest_json_contains_expected_routes() {
        let manifest = build_manifest_json("http://bitosdt-host:8080");
        assert!(manifest.contains("\"mode\":\"simple\""));
        assert!(manifest.contains("http://bitosdt-host:8080/health"));
        assert!(manifest.contains("http://bitosdt-host:8080/download/bitosdt.exe"));
    }

    #[test]
    fn resolve_simple_delivery_defaults_uses_workspace_publish_path() {
        let defaults = resolve_simple_delivery_defaults().expect("resolve defaults");
        assert!(defaults.runtime_url.starts_with("http://"));
        assert_eq!(defaults.bind_address, SIMPLE_BIND_ADDRESS);
        assert!(defaults.publish_path.ends_with(SIMPLE_PUBLISH_FOLDER));
    }
}
