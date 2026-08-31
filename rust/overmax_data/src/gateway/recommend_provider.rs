//! External Recommendation Provider Gateway.
//!
//! Implements `overmax-recommend/1` protocol client using `GatewayHttpClient`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use crate::gateway::error::{GatewayError, GatewayResult};
use crate::gateway::http_client::{GatewayHttpClient, FAST_TIMEOUT};
use crate::service::recommend::{RecommendContext, VaryDim};
use serde::{Deserialize, Serialize};

pub const RECOMMEND_PROTOCOL_ID: &str = "overmax-recommend/1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderManifest {
    pub protocol: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub vary: Vec<VaryDim>,
    #[serde(default = "default_ttl")]
    pub ttl_sec: u64,
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
}

fn default_ttl() -> u64 {
    3600
}

fn default_endpoint() -> String {
    "/recommend".to_string()
}

impl Default for ProviderManifest {
    fn default() -> Self {
        Self {
            protocol: RECOMMEND_PROTOCOL_ID.to_string(),
            name: None,
            vary: vec![VaryDim::SongId, VaryDim::Mode, VaryDim::Diff],
            ttl_sec: default_ttl(),
            endpoint: default_endpoint(),
        }
    }
}

/// Gateway for interacting with external recommendation provider servers.
#[derive(Debug)]
pub struct RecommendProviderGateway {
    client: GatewayHttpClient,
    manifest_cache: Mutex<HashMap<String, (ProviderManifest, Instant)>>,
}

impl RecommendProviderGateway {
    pub fn new(client: GatewayHttpClient) -> Self {
        Self {
            client,
            manifest_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn client(&self) -> &GatewayHttpClient {
        &self.client
    }

    /// Tests the connection to the provider and verifies the manifest protocol version.
    pub fn test_connection(&self, provider_url: &str) -> GatewayResult<ProviderManifest> {
        let clean_url = provider_url.trim_end_matches('/');
        let manifest_url = format!("{}/manifest", clean_url);

        let response = self.client.get(&manifest_url, Some(FAST_TIMEOUT)).send()?;

        if !response.status().is_success() {
            return Err(GatewayError::HttpError {
                status: response.status().as_u16(),
                message: format!("서버 응답 오류 (HTTP {})", response.status()),
            });
        }

        let manifest: ProviderManifest = response.json()?;

        if manifest.protocol != RECOMMEND_PROTOCOL_ID {
            return Err(GatewayError::InvalidProtocol {
                expected: RECOMMEND_PROTOCOL_ID,
                actual: manifest.protocol,
            });
        }

        Ok(manifest)
    }

    /// Fetches the manifest from the provider, caching it in memory.
    pub fn fetch_manifest(&self, provider_url: &str) -> ProviderManifest {
        let clean_url = provider_url.trim_end_matches('/').to_string();

        {
            let guard = overmax_core::lock_or_recover(&self.manifest_cache);
            if let Some((manifest, fetched_at)) = guard.get(&clean_url) {
                if fetched_at.elapsed().as_secs() < 3600 {
                    return manifest.clone();
                }
            }
        }

        let manifest = self.test_connection(&clean_url).unwrap_or_default();

        {
            let mut guard = overmax_core::lock_or_recover(&self.manifest_cache);
            guard.insert(clean_url, (manifest.clone(), Instant::now()));
        }

        manifest
    }

    /// Returns the cached manifest if available, or default.
    pub fn get_cached_manifest(&self, provider_url: &str) -> ProviderManifest {
        let clean_url = provider_url.trim_end_matches('/').to_string();
        let guard = overmax_core::lock_or_recover(&self.manifest_cache);
        if let Some((manifest, _)) = guard.get(&clean_url) {
            manifest.clone()
        } else {
            ProviderManifest::default()
        }
    }

    /// Fetches recommendation results from the provider for the given context and saves to file.
    pub fn fetch_recommendations(
        &self,
        provider_url: &str,
        manifest: &ProviderManifest,
        ctx: &RecommendContext,
        save_path: &Path,
    ) -> GatewayResult<()> {
        let clean_url = provider_url.trim_end_matches('/');
        let endpoint = if manifest.endpoint.starts_with('/') {
            format!("{}{}", clean_url, manifest.endpoint)
        } else if manifest.endpoint.starts_with("http://")
            || manifest.endpoint.starts_with("https://")
        {
            manifest.endpoint.clone()
        } else {
            format!("{}/{}", clean_url, manifest.endpoint)
        };

        let mode_str = ctx.button_mode.as_str();
        let diff_str = ctx.difficulty.as_str();
        let v_id_str = ctx.v_id.as_deref().unwrap_or("");

        let full_url = format!(
            "{}?song_id={}&mode={}&diff={}&v_id={}",
            endpoint, ctx.song_id, mode_str, diff_str, v_id_str
        );

        let response = self.client.get(&full_url, Some(FAST_TIMEOUT)).send()?;

        if !response.status().is_success() {
            return Err(GatewayError::HttpError {
                status: response.status().as_u16(),
                message: format!("서버 응답 오류 (HTTP {})", response.status()),
            });
        }

        let body = response.text()?;

        if let Some(parent) = save_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::write(save_path, body)?;

        Ok(())
    }
}

impl Default for RecommendProviderGateway {
    fn default() -> Self {
        Self::new(GatewayHttpClient::default())
    }
}

// ── Backward-compatible top-level helper functions ───────────────────────────

pub fn test_provider_connection_blocking(provider_url: &str) -> Result<ProviderManifest, String> {
    RecommendProviderGateway::default()
        .test_connection(provider_url)
        .map_err(|e| e.to_string())
}

pub fn fetch_manifest_blocking(provider_url: &str) -> ProviderManifest {
    RecommendProviderGateway::default().fetch_manifest(provider_url)
}

pub fn get_cached_manifest(provider_url: &str) -> ProviderManifest {
    RecommendProviderGateway::default().get_cached_manifest(provider_url)
}

pub fn fetch_recommend_blocking(
    provider_url: &str,
    manifest: &ProviderManifest,
    ctx: &RecommendContext,
    save_path: &Path,
) -> Result<(), String> {
    RecommendProviderGateway::default()
        .fetch_recommendations(provider_url, manifest, ctx, save_path)
        .map_err(|e| e.to_string())
}
