//! Overmax Unified Gateway Layer.
//!
//! Centralizes all external outbound network I/O (HTTP clients, V-Archive API,
//! pattern asset downloaders, and custom recommendation providers).

pub mod asset_download;
pub mod error;
pub mod http_client;
pub mod recommend_provider;
pub mod varchive;

pub use asset_download::{
    download_asset_bytes, download_asset_text, fetch_github_release_asset_url,
    AssetDownloadGateway,
};
pub use error::{GatewayError, GatewayResult};
pub use http_client::{GatewayHttpClient, DEFAULT_TIMEOUT, DOWNLOAD_TIMEOUT, FAST_TIMEOUT};
pub use recommend_provider::{
    fetch_manifest_blocking, fetch_recommend_blocking, get_cached_manifest,
    test_provider_connection_blocking, ProviderManifest, RecommendProviderGateway,
    RECOMMEND_PROTOCOL_ID,
};
pub use varchive::{
    download_songs_json_blocking, fetch_records_blocking, fetch_single_song_records_blocking,
    parse_account_file, upload_score_blocking, AccountInfo, UploadResult, VArchiveGateway,
};

/// Unified top-level gateway providing access to all external outbound services.
#[derive(Debug, Default)]
pub struct OvermaxGateway {
    pub varchive: VArchiveGateway,
    pub assets: AssetDownloadGateway,
    pub provider: RecommendProviderGateway,
}

impl OvermaxGateway {
    /// Creates a new unified gateway with a shared HTTP client.
    pub fn new() -> Self {
        let client = GatewayHttpClient::new();
        Self {
            varchive: VArchiveGateway::new(client.clone()),
            assets: AssetDownloadGateway::new(client.clone()),
            provider: RecommendProviderGateway::new(client),
        }
    }
}
