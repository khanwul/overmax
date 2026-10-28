//! V-Archive 계정 설정 조회 (Steam ID → 계정 파일 경로 / V-ID).

use crate::Settings;

pub fn first_steam_from_settings(settings: &Settings) -> String {
    let varchive = settings.varchive();
    varchive.user_map.keys().next().cloned().unwrap_or_default()
}

pub fn account_path_for_steam(settings: &Settings, steam: &str) -> String {
    settings
        .varchive()
        .user_map
        .get(steam)
        .and_then(|entry| entry.account_path.clone())
        .unwrap_or_default()
}

/// `steam`에 연결된 계정 파일이 실제로 존재하는지.
pub fn is_account_configured(settings: &Settings, steam: &str) -> bool {
    let account_path = account_path_for_steam(settings, steam);
    if account_path.is_empty() {
        return false;
    }
    std::path::Path::new(&account_path).exists()
}

pub fn varchive_user_id(settings: &Settings, steam: &str) -> Option<String> {
    if steam.is_empty() {
        return None;
    }
    let varchive = settings.varchive();
    varchive
        .user_map
        .get(steam)
        .and_then(|u| u.v_id.clone())
        .filter(|id| !id.is_empty())
}

pub fn button_num(mode: &str) -> i32 {
    crate::community::client::Mode::from_str(mode)
        .map(|m| m.button_count())
        .unwrap_or(4)
}
