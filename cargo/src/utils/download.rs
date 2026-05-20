use crate::core::errors::{BitOSDTError, BitOSDTResult};
use futures::StreamExt;
use reqwest::Client;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::info;

pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

pub struct DownloadManager {
    client: Client,
    download_dir: PathBuf,
}

impl DownloadManager {
    pub async fn new(download_dir: PathBuf) -> BitOSDTResult<Self> {
        tokio::fs::create_dir_all(&download_dir).await?;

        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| BitOSDTError::Network(e.to_string()))?;

        Ok(Self {
            client,
            download_dir,
        })
    }

    pub async fn download_file(
        &self,
        url: &str,
        filename: &str,
        progress_callback: Option<ProgressCallback>,
    ) -> BitOSDTResult<PathBuf> {
        let file_path = self.download_dir.join(filename);

        // Check if file already exists
        if file_path.exists() {
            info!("File already exists: {}", file_path.display());
            return Ok(file_path);
        }

        info!("Starting download from: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| BitOSDTError::Network(format!("Failed to start download: {}", e)))?;

        if !response.status().is_success() {
            return Err(BitOSDTError::Network(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        let total_size = response.content_length().unwrap_or(0);

        // Create temp file for atomic download
        let temp_path = file_path.with_extension("tmp");
        let mut file = File::create(&temp_path).await?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| BitOSDTError::Network(format!("Download error: {}", e)))?;

            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            if let Some(ref callback) = progress_callback {
                callback(downloaded, total_size);
            }
        }

        file.flush().await?;
        drop(file);

        // Rename temp to final
        tokio::fs::rename(&temp_path, &file_path).await?;

        info!(
            "Download complete: {} ({} bytes)",
            file_path.display(),
            downloaded
        );

        Ok(file_path)
    }

    pub async fn download_with_resume(
        &self,
        url: &str,
        filename: &str,
        progress_callback: Option<ProgressCallback>,
    ) -> BitOSDTResult<PathBuf> {
        let file_path = self.download_dir.join(filename);

        // Check existing partial download
        let start_byte = if file_path.exists() {
            let metadata = tokio::fs::metadata(&file_path).await?;
            metadata.len()
        } else {
            0
        };

        let mut request = self.client.get(url);

        if start_byte > 0 {
            info!("Resuming download from byte: {}", start_byte);
            request = request.header("Range", format!("bytes={}-", start_byte));
        }

        let response = request
            .send()
            .await
            .map_err(|e| BitOSDTError::Network(format!("Failed to start download: {}", e)))?;

        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(BitOSDTError::Network(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        let total_size = response.content_length().unwrap_or(0) + start_byte;

        // Open file for appending
        let mut file = if start_byte > 0 {
            File::options().append(true).open(&file_path).await?
        } else {
            File::create(&file_path).await?
        };

        let mut downloaded = start_byte;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| BitOSDTError::Network(format!("Download error: {}", e)))?;

            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            if let Some(ref callback) = progress_callback {
                callback(downloaded, total_size);
            }
        }

        file.flush().await?;

        info!(
            "Download complete: {} ({} bytes)",
            file_path.display(),
            downloaded
        );

        Ok(file_path)
    }
}
