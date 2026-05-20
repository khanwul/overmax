//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use crate::overlay_theme::{apply_secondary_window_style, Theme};
use eframe::egui::{
    self, CornerRadius, Frame, Margin, RichText, Slider, Stroke, TextEdit, ViewportClass,
};
use overmax_data::{diff_settings, load_merged_settings, normalize_settings, save_user_settings};
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct SettingsUiContext {
    pub current_steam_id: String,
    pub sync_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
    pub sync_steam_id: Arc<Mutex<String>>,
    pub fetch_tx: Sender<(String, String, i32)>,
}

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    apply_secondary_window_style(ui.ctx());
    ui.add_space(8.0);
    ui.heading(
        RichText::new("Overmax 설정")
            .color(Theme::TEXT_PRIMARY)
            .size(Theme::FONT_HEADING)
            .strong(),
    );
    ui.add_space(20.0);
    let tab = settings_tabs(ui);
    ui.add_space(16.0);
    match tab {
        0 => ui_tab(ui, draft),
        1 => varchive_tab(ui, draft, ctx),
        _ => system_tab(ui, draft),
    }
}

fn settings_tabs(ui: &mut egui::Ui) -> usize {
    let id = ui.id().with("settings_tab");
    let mut active = ui.data(|d| d.get_temp::<usize>(id).unwrap_or(0));
    ui.horizontal(|ui| {
        for (idx, label) in ["UI", "V-Archive", "System"].iter().enumerate() {
            if ui
                .selectable_label(
                    active == idx,
                    RichText::new(*label).size(Theme::FONT_BODY),
                )
                .clicked()
            {
                active = idx;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, active));
    active
}

fn ui_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_frame(ui, "오버레이", |ui| overlay_section(ui, draft));
}

fn overlay_section(ui: &mut egui::Ui, draft: &mut Value) {
    let Some(Value::Object(overlay)) = draft.get_mut("overlay") else {
        return;
    };
    
    ui.horizontal(|ui| {
        ui.label(RichText::new("크기").color(Theme::TEXT_PRIMARY).size(Theme::FONT_BODY));
        let current_scale = overlay.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
        for (label, val) in [("S", 0.75), ("M", 1.0), ("L", 1.25), ("XL", 1.5)] {
            if ui
                .selectable_label(
                    (current_scale - val).abs() < 0.01,
                    RichText::new(label).size(Theme::FONT_SMALL),
                )
                .clicked()
            {
                overlay.insert("scale".into(), serde_json::json!(val));
            }
        }
    });

    ui.add_space(12.0);

    let mut opacity = overlay
        .get("base_opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8);
    if ui
        .add(
            Slider::new(&mut opacity, 0.1..=1.0)
                .step_by(0.1)
                .text(RichText::new("기본 투명도").size(Theme::FONT_SMALL)),
        )
        .changed()
    {
        overlay.insert("base_opacity".into(), serde_json::json!(opacity));
    }
}

fn varchive_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_frame(ui, "계정", |ui| {
        ui.label(
            RichText::new(current_steam_label(ctx))
                .color(Theme::TEXT_MUTED)
                .size(Theme::FONT_SMALL),
        );
        ui.add_space(8.0);
        auto_refresh_row(ui, draft);
        if ctx.current_steam_id.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("발견된 Steam 계정이 없습니다.")
                    .color(Theme::TEXT_MUTED)
                    .size(Theme::FONT_SMALL),
            );
            return;
        }
        ui.add_space(12.0);
        steam_account_rows(ui, draft, ctx);
    });
}

fn current_steam_label(ctx: &SettingsUiContext) -> String {
    if ctx.current_steam_id.is_empty() {
        "현재 Steam: -".into()
    } else {
        format!("현재 Steam: {}", ctx.current_steam_id)
    }
}

fn auto_refresh_row(ui: &mut egui::Ui, draft: &mut Value) {
    let varchive = object_section_mut(draft, "varchive");
    let mut enabled = varchive
        .get("auto_refresh")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if ui
        .checkbox(&mut enabled, RichText::new("시작 시 자동 갱신").size(Theme::FONT_BODY))
        .changed()
    {
        varchive.insert("auto_refresh".into(), json!(enabled));
    }
}

fn steam_account_rows(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);
    text_row(ui, entry, "V-Archive ID", "v_id", 180.0);
    
    ui.horizontal(|ui| {
        ui.add_space(100.0); // Align with text_row label
        for b in [4, 5, 6, 8] {
            if ui.button(RichText::new(format!("{b}B")).size(Theme::FONT_SMALL)).clicked() {
                let v_id = entry.get("v_id").and_then(|v| v.as_str()).unwrap_or("");
                if !v_id.is_empty() {
                    let _ = ctx.fetch_tx.send((ctx.current_steam_id.clone(), v_id.to_string(), b));
                }
            }
        }
        if ui.button(RichText::new("All").size(Theme::FONT_SMALL)).clicked() {
            let v_id = entry.get("v_id").and_then(|v| v.as_str()).unwrap_or("");
            if !v_id.is_empty() {
                let _ = ctx.fetch_tx.send((ctx.current_steam_id.clone(), v_id.to_string(), 0));
            }
        }
    });

    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("account.txt").color(Theme::TEXT_PRIMARY).size(Theme::FONT_SMALL));
        let mut path_str = entry
            .get("account_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if ui
            .add(TextEdit::singleline(&mut path_str).desired_width(200.0))
            .changed()
        {
            entry.insert("account_path".into(), json!(path_str.trim()));
        }
        if ui.button(RichText::new("찾아보기").size(Theme::FONT_SMALL)).clicked() {
            if let Some(file_path) = rfd::FileDialog::new()
                .add_filter("Text Files", &["txt"])
                .pick_file()
            {
                let path_str = file_path.to_string_lossy().to_string();
                entry.insert("account_path".into(), json!(path_str));
            }
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(100.0);
        if ui.button(RichText::new("동기화 후보 찾기").size(Theme::FONT_SMALL)).clicked() {
            if let Ok(mut sid) = ctx.sync_steam_id.lock() {
                *sid = ctx.current_steam_id.clone();
            }
            ctx.sync_open.store(true, Ordering::Relaxed);
            ctx.scan_pending.store(true, Ordering::Relaxed);
        }
    });
}

fn text_row(ui: &mut egui::Ui, entry: &mut Map<String, Value>, label: &str, key: &str, width: f32) {
    let mut text = entry
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(Theme::TEXT_PRIMARY).size(Theme::FONT_BODY));
        ui.add_space(4.0);
        if ui
            .add(TextEdit::singleline(&mut text).desired_width(width))
            .changed()
        {
            entry.insert(key.into(), json!(text.trim()));
        }
    });
}

fn system_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_frame(ui, "업데이트", |ui| update_section(ui, draft));
}

fn update_section(ui: &mut egui::Ui, draft: &mut Value) {
    let app_update = object_section_mut(draft, "app_update");
    let mut enabled = app_update
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if ui
        .checkbox(&mut enabled, RichText::new("자동 업데이트").size(Theme::FONT_BODY))
        .changed()
    {
        app_update.insert("enabled".into(), json!(enabled));
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(format!("현재 버전: {}", env!("CARGO_PKG_VERSION")))
            .color(Theme::TEXT_PRIMARY)
            .size(Theme::FONT_SMALL),
    );
}

pub fn render_settings_deferred(
    ctx: &egui::Context,
    class: ViewportClass,
    title: &str,
    draft: &mut Value,
    settings_ctx: &SettingsUiContext,
) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| render_settings_form(ui, draft, settings_ctx));
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::PANEL_BG).inner_margin(Margin::same(24)))
            .show(ctx, |ui| render_settings_form(ui, draft, settings_ctx));
    }
}

/// Applies normalize + delta save vs `base`, reloads merged into `merged_out`.
pub fn save_settings_to_disk(
    root: &Path,
    defaults: &Value,
    base: &Value,
    draft: &mut Value,
    merged_out: &mut Value,
) -> Result<(), String> {
    normalize_settings(draft);
    let diff = diff_settings(base, draft);
    save_user_settings(root, &diff).map_err(|e| e.to_string())?;
    *merged_out = load_merged_settings(root, defaults.clone());
    Ok(())
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

fn section_frame(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .color(Theme::TEXT_PRIMARY)
                    .size(Theme::FONT_BODY)
                    .strong(),
            );
            ui.add_space(12.0);
            add(ui);
        });
    ui.add_space(12.0);
}

fn object_section_mut<'a>(draft: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    let root = draft.as_object_mut().expect("draft object initialized");
    let entry = root
        .entry(section)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("settings section object")
}

fn user_entry_mut<'a>(draft: &'a mut Value, steam_id: &str) -> &'a mut Map<String, Value> {
    let varchive = object_section_mut(draft, "varchive");
    let user_map_value = varchive
        .entry("user_map")
        .or_insert_with(|| Value::Object(Map::new()));
    if !user_map_value.is_object() {
        *user_map_value = Value::Object(Map::new());
    }
    let user_map = user_map_value.as_object_mut().expect("user_map object");
    let entry = user_map
        .entry(steam_id)
        .or_insert_with(|| json!({"v_id": "", "account_path": ""}));
    if let Some(v_id) = entry.as_str().map(str::to_string) {
        *entry = json!({"v_id": v_id, "account_path": ""});
    }
    entry.as_object_mut().expect("user_map entry object")
}

#[cfg(test)]
mod tests {
    use super::save_settings_to_disk;
    use overmax_data::load_merged_settings;
    use serde_json::json;
    use std::fs;

    #[test]
    fn save_user_roundtrip_matches_python_delta_policy() {
        let root =
            std::env::temp_dir().join(format!("overmax-app-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("settings.json"),
            r#"{"overlay":{"scale":1.0,"base_opacity":0.8},"window_tracker":{"poll_interval_sec":0.5}}"#,
        )
        .unwrap();
        fs::write(root.join("settings.user.json"), "{}").unwrap();

        let defaults = json!({
            "overlay": {"scale": 1.0, "base_opacity": 0.8},
            "window_tracker": {"poll_interval_sec": 0.5}
        });
        let base = overmax_data::load_base_settings(&root, defaults.clone());
        let mut merged = load_merged_settings(&root, defaults.clone());
        let mut draft = merged.clone();
        draft["overlay"]["base_opacity"] = json!(0.55);

        save_settings_to_disk(&root, &defaults, &base, &mut draft, &mut merged).unwrap();

        let reloaded = load_merged_settings(&root, defaults);
        assert_eq!(reloaded["overlay"]["base_opacity"], json!(0.55));
        assert_eq!(reloaded["overlay"]["scale"], json!(1.0));

        let user_text = fs::read_to_string(root.join("settings.user.json")).unwrap();
        assert!(user_text.contains("base_opacity"));

        let _ = fs::remove_dir_all(root);
    }
}
