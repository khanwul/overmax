//! Recommendation Provider Fetcher — network requests for external recommendation sources.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::service::recommend::{RecommendContext, VaryDim};
use serde::{Deserialize, Serialize};

/// 외부 추천 Provider 규격 식별자 (docs/guides/recommend-provider-protocol.md).
/// IPC 계층(`overmax-ipc/1`)과 별개 프로토콜이지만 버저닝 문화(`x/1`)를 공유한다.
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

pub fn test_provider_connection_blocking(provider_url: &str) -> Result<ProviderManifest, String> {
    let clean_url = provider_url.trim_end_matches('/');
    let manifest_url = format!("{}/manifest", clean_url);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP Client 생성 실패: {}", e))?;

    let response = client
        .get(&manifest_url)
        .send()
        .map_err(|e| format!("연결 실패: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("서버 응답 오류 (HTTP {})", response.status()));
    }

    let manifest: ProviderManifest = response
        .json()
        .map_err(|e| format!("Manifest JSON 파싱 실패: {}", e))?;

    if manifest.protocol != RECOMMEND_PROTOCOL_ID {
        return Err(format!(
            "지원하지 않는 프로토콜 버전: {}",
            manifest.protocol
        ));
    }

    Ok(manifest)
}

static MANIFEST_CACHE: Mutex<Option<HashMap<String, (ProviderManifest, Instant)>>> =
    Mutex::new(None);

pub fn fetch_manifest_blocking(provider_url: &str) -> ProviderManifest {
    let clean_url = provider_url.trim_end_matches('/').to_string();

    {
        let mut guard = overmax_core::lock_or_recover(&MANIFEST_CACHE);
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some((manifest, fetched_at)) = cache.get(&clean_url) {
            if fetched_at.elapsed().as_secs() < 3600 {
                return manifest.clone();
            }
        }
    }

    let manifest = test_provider_connection_blocking(&clean_url).unwrap_or_default();

    {
        let mut guard = overmax_core::lock_or_recover(&MANIFEST_CACHE);
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(clean_url, (manifest.clone(), Instant::now()));
    }

    manifest
}

pub fn get_cached_manifest(provider_url: &str) -> ProviderManifest {
    let clean_url = provider_url.trim_end_matches('/').to_string();
    let mut guard = overmax_core::lock_or_recover(&MANIFEST_CACHE);
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some((manifest, _)) = cache.get(&clean_url) {
        manifest.clone()
    } else {
        ProviderManifest::default()
    }
}

pub fn fetch_recommend_blocking(
    provider_url: &str,
    manifest: &ProviderManifest,
    ctx: &RecommendContext,
    save_path: &Path,
) -> Result<(), String> {
    let clean_url = provider_url.trim_end_matches('/');
    let endpoint = if manifest.endpoint.starts_with('/') {
        format!("{}{}", clean_url, manifest.endpoint)
    } else if manifest.endpoint.starts_with("http://") || manifest.endpoint.starts_with("https://")
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

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP Client 생성 실패: {}", e))?;

    let response = client
        .get(&full_url)
        .send()
        .map_err(|e| format!("추천 요청 실패: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("서버 응답 오류 (HTTP {})", response.status()));
    }

    let body = response
        .text()
        .map_err(|e| format!("응답 읽기 실패: {}", e))?;

    if let Some(parent) = save_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    fs::write(save_path, body).map_err(|e| format!("캐시 파일 저장 실패: {}", e))?;

    Ok(())
}
