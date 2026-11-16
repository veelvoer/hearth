//! Update checking against GitHub releases.

use reqwest::Client;
use serde_json::Value;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, UpdateError>;

/// Release info from GitHub.
#[derive(Debug, Clone)]
pub struct Release {
    pub version: String,
    pub download_url: Option<String>,
    pub published_at: Option<String>,
}

/// Check for updates on GitHub.
pub struct UpdateChecker {
    repo: String, // e.g., "voer/hearth"
    client: Client,
}

impl UpdateChecker {
    /// Create a new update checker.
    pub fn new(repo: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(UpdateChecker {
            repo: repo.to_string(),
            client,
        })
    }

    /// Get the latest release.
    pub async fn latest(&self) -> Result<Option<Release>> {
        let url = format!("https://api.github.com/repos/{}/releases/latest", self.repo);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "hearth-client")
            .send()
            .await?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }

        let data: Value = resp.json().await?;
        let version = data
            .get("tag_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let download_url = data
            .get("assets")
            .and_then(|a| a.as_array())
            .and_then(|arr| {
                arr.first()
                    .and_then(|a| a.get("browser_download_url"))
                    .and_then(|u| u.as_str())
            })
            .map(|s| s.to_string());

        let published_at = data
            .get("published_at")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string());

        Ok(version.map(|v| Release {
            version: v,
            download_url,
            published_at,
        }))
    }

    /// Compare two version strings (simple semver comparison).
    pub fn is_newer(current: &str, latest: &str) -> bool {
        let parse_version = |s: &str| -> (u32, u32, u32) {
            let s = s.trim_start_matches('v');
            let parts: Vec<&str> = s.split('.').collect();
            let major = parts.get(0).and_then(|p| p.parse().ok()).unwrap_or(0);
            let minor = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
            let patch = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
            (major, minor, patch)
        };

        let (c_maj, c_min, c_pat) = parse_version(current);
        let (l_maj, l_min, l_pat) = parse_version(latest);

        (l_maj, l_min, l_pat) > (c_maj, c_min, c_pat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(UpdateChecker::is_newer("1.0.0", "1.1.0"));
        assert!(UpdateChecker::is_newer("1.0.0", "2.0.0"));
        assert!(UpdateChecker::is_newer("1.5.0", "1.5.1"));
        assert!(!UpdateChecker::is_newer("1.1.0", "1.0.0"));
        assert!(!UpdateChecker::is_newer("2.0.0", "1.0.0"));
        assert!(!UpdateChecker::is_newer("1.5.1", "1.5.0"));
        assert!(!UpdateChecker::is_newer("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_with_v_prefix() {
        assert!(UpdateChecker::is_newer("v1.0.0", "v1.1.0"));
        assert!(UpdateChecker::is_newer("1.0.0", "v1.1.0"));
    }
}
