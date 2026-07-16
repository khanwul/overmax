//! V-Archive score registration — mirrors `data/varchive_uploader.py`.

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

pub fn upload_score_blocking(
    account: &AccountInfo,
    song_name: &str,
    button_mode: &str,
    difficulty: &str,
    score: f64,
    is_max_combo: bool,
    composer: &str,
) -> UploadResult {
    let pattern = match difficulty {
        "NM" => "NORMAL",
        "HD" => "HARD",
        "MX" => "MAXIMUM",
        "SC" => "SC",
        _ => {
            return UploadResult {
                success: false,
                updated: false,
                message: format!("unsupported difficulty: {difficulty}"),
            };
        }
    };
    let button = match button_mode {
        "4B" => 4,
        "5B" => 5,
        "6B" => 6,
        "8B" => 8,
        _ => {
            return UploadResult {
                success: false,
                updated: false,
                message: format!("unsupported button mode: {button_mode}"),
            };
        }
    };

    let url = BASE_URL.replace("{user_no}", &account.user_no.to_string());
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return UploadResult {
                success: false,
                updated: false,
                message: e.to_string(),
            };
        }
    };

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

    let resp = match client
        .post(&url)
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

pub fn fetch_records_blocking(v_id: &str, button: i32) -> Result<serde_json::Value, String> {
    let url = format!(
        "https://v-archive.net/api/v2/archive/{}/button/{}",
        v_id, button
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&url).send().map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        resp.json().map_err(|e| e.to_string())
    } else {
        Err(format!(
            "HTTP request failed with status: {}",
            resp.status()
        ))
    }
}

pub fn fetch_single_song_records_blocking(
    v_id: &str,
    button: i32,
    song_id: i32,
) -> Result<serde_json::Value, String> {
    let url = format!(
        "https://v-archive.net/api/v2/archive/{}/button/{}?title={}",
        v_id, button, song_id
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&url).send().map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        resp.json().map_err(|e| e.to_string())
    } else {
        Err(format!(
            "HTTP request failed with status: {}",
            resp.status()
        ))
    }
}
