use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Download progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub eta_seconds: u64,
    pub percent: f32,
    pub status: DownloadStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

impl DownloadProgress {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            bytes_downloaded: 0,
            total_bytes,
            speed_bps: 0,
            eta_seconds: 0,
            percent: 0.0,
            status: DownloadStatus::Pending,
        }
    }

    pub fn update(&mut self, bytes_downloaded: u64, elapsed: Duration) {
        self.bytes_downloaded = bytes_downloaded;

        if self.total_bytes > 0 {
            self.percent = (bytes_downloaded as f64 / self.total_bytes as f64 * 100.0) as f32;
        }

        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs > 0.0 {
            self.speed_bps = (bytes_downloaded as f64 / elapsed_secs) as u64;

            if self.speed_bps > 0 {
                let remaining = self.total_bytes.saturating_sub(bytes_downloaded);
                self.eta_seconds = (remaining as f64 / self.speed_bps as f64) as u64;
            }
        }
    }

    pub fn format_speed(&self) -> String {
        format_bytes_per_second(self.speed_bps)
    }

    pub fn format_eta(&self) -> String {
        if self.eta_seconds == 0 && self.status == DownloadStatus::Downloading {
            "Calculating...".to_string()
        } else {
            format_duration(self.eta_seconds)
        }
    }

    pub fn format_progress(&self) -> String {
        format!(
            "{} / {} ({:.1}%)",
            format_bytes(self.bytes_downloaded),
            format_bytes(self.total_bytes),
            self.percent
        )
    }
}

/// Progress tracker for managing download state
pub struct ProgressTracker {
    start_time: Option<Instant>,
    last_update: Instant,
    last_bytes: u64,
    smoothed_speed: f64,
    first_speed_update: bool,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            start_time: None,
            last_update: Instant::now(),
            last_bytes: 0,
            smoothed_speed: 0.0,
            first_speed_update: true,
        }
    }

    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
        self.last_update = Instant::now();
        self.last_bytes = 0;
        self.smoothed_speed = 0.0;
        self.first_speed_update = true;
    }

    pub fn update(&mut self, progress: &mut DownloadProgress, bytes_downloaded: u64) {
        let now = Instant::now();
        let interval = now.duration_since(self.last_update);

        // Update speed calculation every 100ms (reduced from 500ms for faster initial feedback)
        if interval.as_millis() >= 100 {
            let bytes_diff = bytes_downloaded.saturating_sub(self.last_bytes);
            let instant_speed = bytes_diff as f64 / interval.as_secs_f64();

            // Use immediate speed for first update, then exponential moving average
            if self.first_speed_update {
                self.smoothed_speed = instant_speed;
                self.first_speed_update = false;
            } else {
                // Exponential moving average for smooth speed display (adjusted for faster updates)
                self.smoothed_speed = self.smoothed_speed * 0.6 + instant_speed * 0.4;
            }
            progress.speed_bps = self.smoothed_speed as u64;

            // Update ETA based on smoothed speed
            if self.smoothed_speed > 0.0 {
                let remaining = progress.total_bytes.saturating_sub(bytes_downloaded);
                progress.eta_seconds = (remaining as f64 / self.smoothed_speed) as u64;
            }

            self.last_update = now;
            self.last_bytes = bytes_downloaded;
        }

        progress.bytes_downloaded = bytes_downloaded;
        if progress.total_bytes > 0 {
            progress.percent =
                (bytes_downloaded as f64 / progress.total_bytes as f64 * 100.0) as f32;
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time
            .map(|start| start.elapsed())
            .unwrap_or_default()
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_bytes_per_second(bps: u64) -> String {
    format!("{}/s", format_bytes(bps))
}

fn format_duration(seconds: u64) -> String {
    if seconds >= 3600 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        format!("{}h {}m", hours, minutes)
    } else if seconds >= 60 {
        let minutes = seconds / 60;
        let secs = seconds % 60;
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
    }

    #[test]
    fn test_progress_percent() {
        let mut progress = DownloadProgress::new(1000);
        progress.update(500, Duration::from_secs(1));
        assert!((progress.percent - 50.0).abs() < 0.1);
    }
}
