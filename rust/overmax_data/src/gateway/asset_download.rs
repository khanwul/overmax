//! Asset Download Gateway.
//!
//! Handles external asset downloads (Google Sheets pattern CSVs, GitHub Release image DBs)
//! using `GatewayHttpClient`.

use crate::gateway::error::{GatewayError, GatewayResult};
use crate::gateway::http_client::{GatewayHttpClient, DEFAULT_TIMEOUT, DOWNLOAD_TIMEOUT};
use serde_json::Value;
use std::time::Duration;

/// Gateway for downloading external static assets and databases.
#[derive(Clone, Debug, Default)]
pub struct AssetDownloadGateway {
    client: GatewayHttpClient,
}

impl AssetDownloadGateway {
    pub fn new(client: GatewayHttpClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &GatewayHttpClient {
        &self.client
    }

    /// Downloads raw bytes of an external asset with specified timeout.
    pub fn download_bytes(&self, url: &str, timeout: Option<Duration>) -> GatewayResult<Vec<u8>> {
        self.client
            .download_bytes(url, timeout.unwrap_or(DOWNLOAD_TIMEOUT))
    }

    /// Downloads text content (e.g. CSV, version file) with UTF-8 BOM stripping.
    pub fn download_text(&self, url: &str, timeout: Option<Duration>) -> GatewayResult<String> {
        self.client
            .download_text(url, timeout.unwrap_or(DEFAULT_TIMEOUT))
    }

    /// Fetches latest GitHub release info and extracts asset download URL.
    pub fn fetch_github_release_asset_url(
        &self,
        owner: &str,
        repo: &str,
        asset_name: &str,
    ) -> GatewayResult<(String, String)> {
        let api_url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");

        let resp = self
            .client
            .get(&api_url, Some(DEFAULT_TIMEOUT))
            .send()?
            .error_for_status()?;

        let data: Value = resp.json()?;
        let tag = data
            .get("tag_name")
            .and_then(Value::as_str)
            .ok_or_else(|| GatewayError::AssetNotFound("tag_name 없음".into()))?;

        let assets = data
            .get("assets")
            .and_then(Value::as_array)
            .ok_or_else(|| GatewayError::AssetNotFound("assets 없음".into()))?;

        for asset in assets {
            if asset.get("name").and_then(Value::as_str) != Some(asset_name) {
                continue;
            }
            let Some(download_url) = asset.get("browser_download_url").and_then(Value::as_str) else {
                continue;
            };
            return Ok((tag.to_string(), download_url.to_string()));
        }

        Err(GatewayError::AssetNotFound(format!(
            "asset 없음: {asset_name}"
        )))
    }
}

// ── Backward-compatible top-level helper functions ───────────────────────────

pub fn download_asset_bytes(
    url: &str,
    timeout: Option<Duration>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    AssetDownloadGateway::default()
        .download_bytes(url, timeout)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}

pub fn download_asset_text(
    url: &str,
    timeout: Option<Duration>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    AssetDownloadGateway::default()
        .download_text(url, timeout)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}

pub fn fetch_github_release_asset_url(
    owner: &str,
    repo: &str,
    asset_name: &str,
) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    AssetDownloadGateway::default()
        .fetch_github_release_asset_url(owner, repo, asset_name)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}
