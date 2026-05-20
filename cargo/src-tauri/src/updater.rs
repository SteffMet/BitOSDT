use serde::{Deserialize, Serialize};

pub const DEFAULT_FORUM_URL: &str = "https://bitosdt.com/forum/";
pub const DEFAULT_UPDATE_ENDPOINT: &str = "https://bitosdt.com/forum/downloadable_build_status.php";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Release,
    Experimental,
}

impl ReleaseChannel {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "experimental" => Self::Experimental,
            _ => Self::Release,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Experimental => "experimental",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppReleaseMetadata {
    pub version: String,
    pub channel: ReleaseChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadableBuild {
    pub channel: ReleaseChannel,
    pub version: String,
    pub title: String,
    pub forum_url: String,
    pub download_available: bool,
    pub published_at: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub build_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub current_channel: ReleaseChannel,
    pub latest_version: Option<String>,
    pub latest_channel: Option<ReleaseChannel>,
    pub forum_url: String,
    pub title: Option<String>,
    pub published_at: Option<String>,
    pub update_available: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadableBuildStatusEnvelope {
    ok: bool,
    build: Option<DownloadableBuild>,
}

pub fn current_app_release_metadata() -> AppReleaseMetadata {
    AppReleaseMetadata {
        version: env!("CARGO_PKG_VERSION").to_string(),
        channel: ReleaseChannel::parse(env!("BITOSDT_RELEASE_CHANNEL")),
    }
}

pub async fn check_for_update(
    endpoint: &str,
    metadata: &AppReleaseMetadata,
) -> Result<UpdateCheckResponse, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(endpoint)
        .query(&[
            ("channel", metadata.channel.as_str()),
            ("version", metadata.version.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("Failed to query update endpoint: {e}"))?;

    let status = response.status();
    let envelope = response
        .json::<DownloadableBuildStatusEnvelope>()
        .await
        .map_err(|e| format!("Failed to parse update endpoint response: {e}"))?;

    if !status.is_success() || !envelope.ok {
        return Err(format!(
            "Update endpoint returned an error status: {}",
            status
        ));
    }

    Ok(build_update_response(metadata, envelope.build))
}

fn build_update_response(
    metadata: &AppReleaseMetadata,
    build: Option<DownloadableBuild>,
) -> UpdateCheckResponse {
    let Some(build) = build else {
        return UpdateCheckResponse {
            current_version: metadata.version.clone(),
            current_channel: metadata.channel.clone(),
            latest_version: None,
            latest_channel: None,
            forum_url: DEFAULT_FORUM_URL.to_string(),
            title: None,
            published_at: None,
            update_available: false,
        };
    };

    let same_channel = build.channel == metadata.channel;
    let newer = compare_versions(&build.version, &metadata.version).is_gt();
    let available = build.download_available && same_channel && newer;

    UpdateCheckResponse {
        current_version: metadata.version.clone(),
        current_channel: metadata.channel.clone(),
        latest_version: Some(build.version),
        latest_channel: Some(build.channel),
        forum_url: if build.forum_url.trim().is_empty() {
            DEFAULT_FORUM_URL.to_string()
        } else {
            build.forum_url
        },
        title: Some(build.title),
        published_at: Some(build.published_at),
        update_available: available,
    }
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = parse_version_parts(left);
    let right_parts = parse_version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());

    for index in 0..max_len {
        let l = *left_parts.get(index).unwrap_or(&0);
        let r = *right_parts.get(index).unwrap_or(&0);
        match l.cmp(&r) {
            std::cmp::Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }

    std::cmp::Ordering::Equal
}

fn parse_version_parts(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|segment| {
            let numeric: String = segment
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            numeric.parse::<u64>().unwrap_or(0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_detects_newer_release() {
        assert!(compare_versions("2.0.4", "2.0.3").is_gt());
        assert!(compare_versions("2.1.0", "2.0.99").is_gt());
        assert!(compare_versions("2.0.3", "2.0.3").is_eq());
    }

    #[test]
    fn build_update_response_ignores_other_channel() {
        let metadata = AppReleaseMetadata {
            version: "2.0.3".to_string(),
            channel: ReleaseChannel::Release,
        };

        let response = build_update_response(
            &metadata,
            Some(DownloadableBuild {
                channel: ReleaseChannel::Experimental,
                version: "2.0.4".to_string(),
                title: "BitOSDT 2.0.4 Experimental".to_string(),
                forum_url: DEFAULT_FORUM_URL.to_string(),
                download_available: true,
                published_at: "2026-03-12 10:00:00".to_string(),
                notes: None,
                build_id: None,
            }),
        );

        assert!(!response.update_available);
    }

    #[test]
    fn build_update_response_requires_downloadable_newer_build() {
        let metadata = AppReleaseMetadata {
            version: "2.0.3".to_string(),
            channel: ReleaseChannel::Experimental,
        };

        let response = build_update_response(
            &metadata,
            Some(DownloadableBuild {
                channel: ReleaseChannel::Experimental,
                version: "2.0.4".to_string(),
                title: "BitOSDT 2.0.4 Experimental".to_string(),
                forum_url: DEFAULT_FORUM_URL.to_string(),
                download_available: true,
                published_at: "2026-03-12 10:00:00".to_string(),
                notes: None,
                build_id: None,
            }),
        );

        assert!(response.update_available);
        assert_eq!(response.latest_version.as_deref(), Some("2.0.4"));
    }
}
