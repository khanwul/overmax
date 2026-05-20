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

pub fn first_steam_from_settings(settings: Value) -> String {
    let Some(Value::Object(map)) = settings.get("varchive").and_then(|v| v.get("user_map")) else {
        return String::new();
    };
    map.keys().next().cloned().unwrap_or_default()
}

pub fn account_path_for_steam(settings: &Value, steam: &str) -> String {
    settings
        .get("varchive")
        .and_then(|v| v.get("user_map"))
        .and_then(|m| m.get(steam))
        .and_then(|entry| {
            if let Some(s) = entry.as_str() {
                return Some(s.to_string());
            }
            entry
                .get("account_path")
                .and_then(|p| p.as_str())
                .map(|s| s.to_string())
        })
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
