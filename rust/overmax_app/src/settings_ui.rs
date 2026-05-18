//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use eframe::egui::{self, Slider, ViewportClass};
use overmax_data::{diff_settings, load_merged_settings, normalize_settings, save_user_settings};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value) {
    ui.heading("설정");
    ui.add_space(8.0);
    overlay_section(ui, draft);
    ui.add_space(12.0);
    intervals_section(ui, draft);
}

fn overlay_section(ui: &mut egui::Ui, draft: &mut Value) {
    ui.label("오버레이");
    let Some(Value::Object(overlay)) = draft.get_mut("overlay") else {
        return;
    };
    let mut scale = overlay.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    if ui
        .add(Slider::new(&mut scale, 0.75..=1.5).text("scale"))
        .changed()
    {
        overlay.insert("scale".into(), serde_json::json!(scale));
    }
    let mut opacity = overlay.get("base_opacity").and_then(|v| v.as_f64()).unwrap_or(0.8);
    if ui
        .add(Slider::new(&mut opacity, 0.1..=1.0).text("base_opacity"))
        .changed()
    {
        overlay.insert("base_opacity".into(), serde_json::json!(opacity));
    }
}

fn intervals_section(ui: &mut egui::Ui, draft: &mut Value) {
    ui.label("간격 (초)");
    interval_row(ui, draft, "window_tracker", "poll_interval_sec", "창 폴링");
    interval_row(ui, draft, "screen_capture", "ocr_interval_sec", "OCR");
    interval_row(ui, draft, "jacket_matcher", "match_interval_sec", "재킷 매칭");
}

fn interval_row(ui: &mut egui::Ui, draft: &mut Value, section: &str, key: &str, label: &str) {
    let Some(Value::Object(sec)) = draft.get_mut(section) else {
        return;
    };
    let mut v = sec.get(key).and_then(|x| x.as_f64()).unwrap_or(0.5);
    if ui
        .add(Slider::new(&mut v, 0.05..=5.0).text(format!("{label} ({section}/{key})")))
        .changed()
    {
        sec.insert(key.to_string(), serde_json::json!(v));
    }
}

pub fn render_settings_deferred(ctx: &egui::Context, class: ViewportClass, title: &str, draft: &mut Value) {
    if class == ViewportClass::Embedded {
        egui::Window::new(title).show(ctx, |ui| render_settings_form(ui, draft));
    } else {
        egui::CentralPanel::default().show(ctx, |ui| render_settings_form(ui, draft));
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
    }
}

#[cfg(test)]
mod tests {
    use super::save_settings_to_disk;
    use overmax_data::load_merged_settings;
    use serde_json::json;
    use std::fs;

    #[test]
    fn save_user_roundtrip_matches_python_delta_policy() {
        let root = std::env::temp_dir().join(format!("overmax-app-settings-{}", std::process::id()));
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
