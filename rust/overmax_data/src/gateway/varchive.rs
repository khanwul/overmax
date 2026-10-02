//! V-Archive Gateway.
//!
//! Handles V-Archive API communication (score upload, record query, song DB fetch)
//! using `GatewayHttpClient`.

use crate::gateway::error::{GatewayError, GatewayResult};
use crate::gateway::http_client::{GatewayHttpClient, DEFAULT_TIMEOUT};
use overmax_core::{Difficulty, Mode};
use std::path::Path;

const BASE_URL: &str = "https://v-archive.net/client/open/{user_no}/score";
const ARCHIVE_BASE: &str = "https://v-archive.net/api/v2/archive/";

/// 계정 정보. `Debug` 출력에 인증 토큰이 노출되지 않도록 수동 구현한다
/// (`RecordDB::masked_steam_id` 와 같은 마스킹 관례).
#[derive(Clone)]
pub struct AccountInfo {
    pub user_no: i64,
    pub token: String,
}

impl std::fmt::Debug for AccountInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // user_no 는 토큰과 짝을 이루는 계정 식별자이므로 함께 숨긴다.
        f.debug_struct("AccountInfo")
            .field("user_no", &"<redacted>")
            .field("token", &"<redacted>")
            .finish()
    }
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
        // URL 조립은 `build_records_url` 이 담당한다. v_id(경로 세그먼트)와
        // since(쿼리 값)을 값 단위로 조립해, 값 안의 `/` `?` `#` `&` 가 URL
        // 구조로 해석되는 것을 막는다.
        let url = build_records_url(v_id, button, since)?;

        let resp = self.client.get(url, Some(DEFAULT_TIMEOUT)).send()?;
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
        let mut url = build_records_url(v_id, button, None)?;
        url.query_pairs_mut()
            .append_pair("title", &song_id.to_string());

        let resp = self.client.get(url, Some(DEFAULT_TIMEOUT)).send()?;
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

/// `fetch_records` 가 요청할 URL 을 구성한다.
///
/// `v_id` 는 경로 세그먼트, `since` 은 쿼리 값이다. 둘 다 값 단위로 조립해야
/// `format!` 보간에서 URL 구조가 조작되지 않는다(아래 테스트 참조).
/// v_id 는 설정 UI(`settings_ui.rs:448`)가 자유 입력이라 화이트리스트 필터로
/// 좁히지 않고, 조립 단계에서 인코딩한다.
fn build_records_url(
    v_id: &str,
    button: i32,
    since: Option<&str>,
) -> Result<reqwest::Url, GatewayError> {
    let mut url = reqwest::Url::parse(ARCHIVE_BASE).map_err(|e| GatewayError::HttpError {
        status: 0,
        message: format!("invalid V-Archive url: {e}"),
    })?;
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.pop_if_empty();
        segments.push(v_id);
        segments.push("button");
        segments.push(&button.to_string());
    }
    if let Some(s) = since {
        url.query_pairs_mut().append_pair("since", s);
    }
    Ok(url)
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

#[cfg(test)]
mod tests {
    use super::build_records_url;

    /// `since` 값에 쿼리 구분자가 들어가도 파라미터가 복제되면 안 된다.
    /// 서버가 준 `updatedAt` 원본 문자열이 그대로 값으로 들어오기 때문이다.
    #[test]
    fn since_value_cannot_inject_extra_query_params() {
        let url = build_records_url("og", 4, Some("abc&since=evil")).unwrap();
        let keys: Vec<String> = url.query_pairs().map(|(k, _)| k.to_string()).collect();
        assert_eq!(keys, vec!["since".to_string()], "쿼리 키가 복제됨");
        let value = url
            .query_pairs()
            .find(|(k, _)| k == "since")
            .map(|(_, v)| v.to_string())
            .unwrap();
        assert_eq!(value, "abc&since=evil", "값이 원본 그대로 보존되어야 함");
    }

    #[test]
    fn since_value_with_hash_and_question_mark_is_escaped() {
        let url = build_records_url("og", 4, Some("2026-08-21T00:00:00.000Z?x#y")).unwrap();
        let keys: Vec<String> = url.query_pairs().map(|(k, _)| k.to_string()).collect();
        assert_eq!(keys, vec!["since".to_string()]);
        assert!(
            url.query().unwrap().contains("%3F"),
            "물음표가 이스케이프되어야 함"
        );
        assert!(
            url.query().unwrap().contains("%23"),
            "샵이 이스케이프되어야 함"
        );
    }

    /// `since` 이 없으면 쿼리 없이 경로만 유지된다(기존 동작).
    #[test]
    fn no_since_produces_plain_path() {
        let url = build_records_url("og", 4, None).unwrap();
        assert!(url.query().is_none());
        assert_eq!(url.path(), "/api/v2/archive/og/button/4");
    }

    /// v_id 는 경로 세그먼트이므로 한글도 그대로 실려 나간다
    /// (설정 UI 가 자유 입력이라 화이트리스트 필터로 좁히면 안 된다).
    #[test]
    fn v_id_keeps_non_ascii_verbatim() {
        let url = build_records_url("한글아이디", 4, None).unwrap();
        assert_eq!(
            url.path(),
            "/api/v2/archive/%ED%95%9C%EA%B8%80%EC%95%84%EC%9D%B4%EB%94%94/button/4"
        );
        // percent-encoding 만 적용되고 값 자체는 보존된다.
        assert!(url.path().ends_with("/button/4"));
    }

    /// v_id 안의 슬래시가 경로를 추가로 쪼개지 않아야 한다.
    /// `format!` 보간이었다면 `archive/a/b/button/4` 로 경로가 2단 분리된다.
    #[test]
    fn v_id_slash_stays_single_segment() {
        let url = build_records_url("a/b", 4, None).unwrap();
        assert_eq!(url.path(), "/api/v2/archive/a%2Fb/button/4");
    }

    /// v_id 안의 물음표/샵이 쿼리와 프래그먼트를 만들지 않아야 한다.
    /// `format!` 보간이었다면 `keys` 가 ["x","since"] 로 오염되고,
    /// `#` 는 `since` 파라미터째 소실시켰다.
    #[test]
    fn v_id_question_mark_and_hash_stay_in_segment() {
        let q = build_records_url("a?x=1", 4, Some("s")).unwrap();
        assert_eq!(q.path(), "/api/v2/archive/a%3Fx=1/button/4");
        assert_eq!(
            q.query_pairs()
                .map(|(k, _)| k.to_string())
                .collect::<Vec<_>>(),
            ["since"]
        );

        let h = build_records_url("a#f", 4, Some("s")).unwrap();
        assert_eq!(h.path(), "/api/v2/archive/a%23f/button/4");
        assert_eq!(
            h.query_pairs()
                .map(|(k, _)| k.to_string())
                .collect::<Vec<_>>(),
            ["since"]
        );
    }

    /// v_id 안의 앰퍼샌드가 쿼리 구분자로 쓰이지 않아야 한다.
    ///
    /// `path_segments_mut` 는 `&` 를 이스케이프하지 않는다(경로에서 합법 문자라
    /// 크레이트가 그대로 둔다). 다만 쿼리가 없는 경로 위치에 있으므로 URL 구조는
    /// 조작되지 않으며, 값도 원본 그대로 보존된다.
    #[test]
    fn v_id_ampersand_stays_in_segment() {
        let url = build_records_url("a&b", 4, None).unwrap();
        assert_eq!(url.path(), "/api/v2/archive/a&b/button/4");
        assert!(url.query().is_none(), "쿼리가 새로 생기면 안 됨");
    }
}
