//! V-Archive Gateway.
//!
//! Handles V-Archive API communication (score upload, record query, song DB fetch)
//! using `GatewayHttpClient`.

use crate::gateway::error::{GatewayError, GatewayResult};
use crate::gateway::http_client::{GatewayHttpClient, DEFAULT_TIMEOUT};
use overmax_core::{Difficulty, Mode};
use std::path::Path;

const BASE_URL: &str = "https://v-archive.net/client/open/{user_no}/score";

#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub user_no: i64,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct UploadResult {
    pub success: bool,
    pub updated: bool,
    pub message: String,
}

pub fn parse_account_file(path: &Path) -> Option<AccountInfo> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let user_no = parts.next()?.parse().ok()?;
    let token = parts.next()?.to_string();
    Some(AccountInfo { user_no, token })
}

/// Gateway for interacting with V-Archive web APIs.
#[derive(Clone, Debug, Default)]
pub struct VArchiveGateway {
    client: GatewayHttpClient,
}

impl VArchiveGateway {
    pub fn new(client: GatewayHttpClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &GatewayHttpClient {
        &self.client
    }

    /// Uploads a play score to V-Archive.
    #[allow(clippy::too_many_arguments)]
    pub fn upload_score(
        &self,
        account: &AccountInfo,
        song_name: &str,
        button_mode: Mode,
        difficulty: Difficulty,
        score: f64,
        is_max_combo: bool,
        composer: &str,
    ) -> UploadResult {
        let pattern = difficulty.as_full_name();
        let button = button_mode.button_count();

        let url = BASE_URL.replace("{user_no}", &account.user_no.to_string());

        let mut body = serde_json::json!({
            "name": song_name,
            "button": button,
            "pattern": pattern,
            "score": score,
            "maxCombo": if is_max_combo { 1 } else { 0 },
        });
        if !composer.is_empty() {
            body["composer"] = serde_json::Value::String(composer.to_string());
        }

        let resp = match self
            .client
            .post(&url, Some(DEFAULT_TIMEOUT))
            .header("Authorization", &account.token)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                return UploadResult {
                    success: false,
                    updated: false,
                    message: e.to_string(),
                };
            }
        };

        let status = resp.status();
        let data: serde_json::Value = resp.json().unwrap_or(serde_json::json!({}));
        if status == 200 {
            return UploadResult {
                success: true,
                updated: data
                    .get("update")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                message: String::new(),
            };
        }

        let msg = data
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("request failed")
            .to_string();
        UploadResult {
            success: false,
            updated: false,
            message: msg,
        }
    }

    /// Fetches all user records for a button mode.
    pub fn fetch_records(
        &self,
        v_id: &str,
        button: i32,
        since: Option<&str>,
    ) -> GatewayResult<serde_json::Value> {
        let url = if let Some(s) = since {
            format!(
                "https://v-archive.net/api/v2/archive/{}/button/{}?since={}",
                v_id, button, s
            )
        } else {
            format!(
                "https://v-archive.net/api/v2/archive/{}/button/{}",
                v_id, button
            )
        };

        let resp = self.client.get(&url, Some(DEFAULT_TIMEOUT)).send()?;
        if resp.status().is_success() {
            Ok(resp.json()?)
        } else {
            Err(GatewayError::HttpError {
                status: resp.status().as_u16(),
                message: format!("HTTP request failed with status: {}", resp.status()),
            })
        }
    }

    /// Fetches user records for a single song.
    pub fn fetch_single_song_records(
        &self,
        v_id: &str,
        button: i32,
        song_id: i32,
    ) -> GatewayResult<serde_json::Value> {
        let url = format!(
            "https://v-archive.net/api/v2/archive/{}/button/{}?title={}",
            v_id, button, song_id
        );

        let resp = self.client.get(&url, Some(DEFAULT_TIMEOUT)).send()?;
        if resp.status().is_success() {
            Ok(resp.json()?)
        } else {
            Err(GatewayError::HttpError {
                status: resp.status().as_u16(),
                message: format!("HTTP request failed with status: {}", resp.status()),
            })
        }
    }

    /// Downloads the V-Archive `songs.json` master database.
    pub fn download_songs_json(&self, url: &str) -> GatewayResult<String> {
        self.client.download_text(url, DEFAULT_TIMEOUT)
    }
}

// ── Backward-compatible top-level helper functions ───────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn upload_score_blocking(
    account: &AccountInfo,
    song_name: &str,
    button_mode: Mode,
    difficulty: Difficulty,
    score: f64,
    is_max_combo: bool,
    composer: &str,
) -> UploadResult {
    VArchiveGateway::default().upload_score(
        account,
        song_name,
        button_mode,
        difficulty,
        score,
        is_max_combo,
        composer,
    )
}

pub fn fetch_records_blocking(
    v_id: &str,
    button: i32,
    since: Option<&str>,
) -> Result<serde_json::Value, String> {
    VArchiveGateway::default()
        .fetch_records(v_id, button, since)
        .map_err(|e| e.to_string())
}

pub fn fetch_single_song_records_blocking(
    v_id: &str,
    button: i32,
    song_id: i32,
) -> Result<serde_json::Value, String> {
    VArchiveGateway::default()
        .fetch_single_song_records(v_id, button, song_id)
        .map_err(|e| e.to_string())
}

pub fn download_songs_json_blocking(url: &str) -> Result<String, String> {
    VArchiveGateway::default()
        .download_songs_json(url)
        .map_err(|e| e.to_string())
}
