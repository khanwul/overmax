use eframe::egui::ViewportId;
use serde_json::Value;

pub fn vp_debug() -> ViewportId {
    ViewportId::from_hash_of("overmax_debug_vp")
}

pub fn vp_settings() -> ViewportId {
    ViewportId::from_hash_of("overmax_settings_vp")
}

pub fn vp_sync() -> ViewportId {
    ViewportId::from_hash_of("overmax_sync_vp")
}

pub fn first_steam_from_settings(settings: &overmax_data::Settings) -> String {
    let varchive = settings.varchive();
    varchive.user_map.keys().next().cloned().unwrap_or_default()
}

pub fn account_path_for_steam(settings: &overmax_data::Settings, steam: &str) -> String {
    settings
        .varchive()
        .user_map
        .get(steam)
        .and_then(|entry| entry.account_path.clone())
        .unwrap_or_default()
}

pub fn button_num(mode: &str) -> i32 {
    match mode {
        "4B" => 4,
        "5B" => 5,
        "6B" => 6,
        "8B" => 8,
        _ => 4,
    }
}
