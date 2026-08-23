//! Settings editor: overlay scale/opacity and capture/matcher intervals.

use crate::ui::dialog_theme::{
    apply_dialog_style, field_row, render_dialog_tabs, section_card, segmented_row, setting_row,
    DialogTheme,
};
use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, Slider, Stroke, TextEdit, ViewportClass,
};
use overmax_data::{diff_settings, load_merged_settings, normalize_settings, save_user_settings};
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct SettingsUiContext {
    pub root: Arc<std::path::PathBuf>,
    pub current_steam_id: String,
    pub sync_open: Arc<AtomicBool>,
    pub debug_open: Arc<AtomicBool>,
    pub scan_pending: Arc<AtomicBool>,
    pub sync_steam_id: Arc<Mutex<String>>,
    pub fetch_tx: Sender<(String, String, i32)>,
    pub steam_users:
        Arc<Mutex<std::collections::HashMap<String, crate::system::steam_session::SteamUser>>>,
}

pub fn render_settings_form(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    apply_dialog_style(ui.ctx());

    ui.add_space(DialogTheme::GAP_SM);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Overmax")
                .color(DialogTheme::TEXT_ACCENT)
                .size(DialogTheme::FONT_TITLE)
                .strong(),
        );
        ui.label(
            RichText::new(crate::t!("settings-title"))
                .color(DialogTheme::TEXT_PRIMARY)
                .size(DialogTheme::FONT_TITLE)
                .strong(),
        );
    });

    ui.add_space(DialogTheme::GAP_LG);

    let id = ui.id().with("settings_tab");
    let mut active = ui.data(|d| d.get_temp::<usize>(id).unwrap_or(0));
    render_dialog_tabs(
        ui,
        "settings_tabs",
        &[
            crate::t!("settings-tab-general"),
            crate::t!("settings-tab-recommend"),
            crate::t!("settings-tab-varchive"),
            crate::t!("settings-tab-advanced"),
        ],
        &mut active,
    );
    ui.data_mut(|d| d.insert_temp(id, active));

    ui.add_space(DialogTheme::GAP_XL);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match active {
            0 => general_tab(ui, draft),
            1 => recommend_tab(ui, draft),
            2 => varchive_tab(ui, draft, ctx),
            _ => advanced_tab(ui, draft, ctx),
        });
}

fn general_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_card(ui, crate::t!("settings-general"), |ui| {
        general_section(ui, draft);
    });
    section_card(ui, crate::t!("settings-overlay-section"), |ui| {
        overlay_section(ui, draft);
    });
}

fn recommend_tab(ui: &mut egui::Ui, draft: &mut Value) {
    section_card(ui, crate::t!("settings-recommend-section"), |ui| {
        recommend_section(ui, draft);
    });
    section_card(ui, crate::t!("settings-recommend-provider"), |ui| {
        recommend_provider_section(ui, draft);
    });
}

fn recommend_section(ui: &mut egui::Ui, draft: &mut Value) {
    let mut smart_reco = draft
        .get("recommend")
        .and_then(|r| r.get("smart_recommend"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    setting_row(
        ui,
        crate::t!("settings-smart-recommend"),
        crate::t!("settings-smart-recommend-hint"),
        |ui| {
            let response = ui
                .checkbox(&mut smart_reco, crate::t!("settings-enable"))
                .on_hover_text(crate::t!("settings-smart-recommend-desc"));

            if response.changed() {
                if !draft.get("recommend").is_some_and(|v| v.is_object()) {
                    draft["recommend"] = serde_json::json!({});
                }
                draft["recommend"]["smart_recommend"] = serde_json::json!(smart_reco);
                ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
            }
        },
    );

    ui.add_space(DialogTheme::GAP_MD);

    let current_target = draft
        .get("recommend")
        .and_then(|r| r.get("target_rate"))
        .and_then(|v| v.as_f64())
        .unwrap_or(99.0);

    segmented_row(
        ui,
        crate::t!("settings-target-rate"),
        crate::t!("settings-target-rate-hint"),
        |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
                ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);

                for (label, val) in [
                    ("97%", 97.0),
                    ("99%", 99.0),
                    ("99.5%", 99.5),
                    ("100%", 100.0),
                ] {
                    let is_active = (current_target - val).abs() < 0.001;
                    let btn = egui::Button::new(
                        RichText::new(label).size(DialogTheme::FONT_BODY).strong(),
                    )
                    .fill(if is_active {
                        DialogTheme::BG_CONTROL_ACTIVE
                    } else {
                        DialogTheme::BG_CONTROL
                    })
                    .stroke(Stroke::new(1.0_f32, DialogTheme::BG_CARD_STROKE))
                    .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                    .wrap();

                    if ui
                        .add_sized(egui::vec2(60.0, DialogTheme::CONTROL_HEIGHT), btn)
                        .on_hover_text(crate::t!("settings-target-rate-desc"))
                        .clicked()
                    {
                        if !draft.get("recommend").is_some_and(|v| v.is_object()) {
                            draft["recommend"] = serde_json::json!({});
                        }
                        draft["recommend"]["target_rate"] = serde_json::json!(val);
                        ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                    }
                }
            });
        },
    );
}

fn overlay_section(ui: &mut egui::Ui, draft: &mut Value) {
    let Some(Value::Object(overlay)) = draft.get_mut("overlay") else {
        return;
    };

    ui.label(
        RichText::new(crate::t!("settings-overlay-hint"))
            .color(DialogTheme::TEXT_MUTED)
            .size(DialogTheme::FONT_HINT),
    );
    ui.add_space(DialogTheme::GAP_MD);

    let current_scale = overlay.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    segmented_row(ui, crate::t!("settings-size"), "", |ui| {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
            for (label, val) in [("S", 0.75), ("M", 1.0), ("L", 1.25), ("XL", 1.5)] {
                let is_active = (current_scale - val).abs() < 0.01;
                let btn =
                    egui::Button::new(RichText::new(label).size(DialogTheme::FONT_BODY).strong())
                        .fill(if is_active {
                            DialogTheme::BG_CONTROL_ACTIVE
                        } else {
                            DialogTheme::BG_CONTROL
                        })
                        .stroke(Stroke::new(1.0_f32, DialogTheme::BG_CARD_STROKE))
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .wrap();

                if ui
                    .add_sized(egui::vec2(44.0, DialogTheme::CONTROL_HEIGHT), btn)
                    .clicked()
                {
                    overlay.insert("scale".into(), serde_json::json!(val));
                    ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
                }
            }
        });
    });

    ui.add_space(DialogTheme::GAP_MD);

    let mut opacity = overlay
        .get("base_opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8);
    setting_row(ui, crate::t!("settings-opacity"), "", |ui| {
        let slider = Slider::new(&mut opacity, 0.1..=1.0)
            .step_by(0.1)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
            .trailing_fill(true);

        if ui
            .add_sized(egui::vec2(180.0, DialogTheme::CONTROL_HEIGHT), slider)
            .changed()
        {
            overlay.insert("base_opacity".into(), serde_json::json!(opacity));
            ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
        }
    });

    ui.add_space(DialogTheme::GAP_MD);

    let mut lite_mode = overlay
        .get("lite_mode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    setting_row(
        ui,
        crate::t!("settings-lite-mode"),
        crate::t!("settings-lite-mode-desc"),
        |ui| {
            let response = ui.checkbox(&mut lite_mode, crate::t!("settings-enable"));
            if response.changed() {
                overlay.insert("lite_mode".into(), serde_json::json!(lite_mode));
                ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
            }
        },
    );

    ui.add_space(DialogTheme::GAP_MD);

    let mut always_visible = overlay
        .get("always_visible")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    setting_row(
        ui,
        crate::t!("settings-overlay-display"),
        crate::t!("settings-always-show-desc"),
        |ui| {
            let response = ui.checkbox(&mut always_visible, crate::t!("settings-always-show"));
            if response.changed() {
                overlay.insert("always_visible".into(), serde_json::json!(always_visible));
                ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
            }
        },
    );

    ui.add_space(DialogTheme::GAP_MD);

    field_row(ui, crate::t!("settings-snap-position"), "", |ui| {
        let mut position = overlay
            .get("position")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut snap = position
            .get("snap")
            .and_then(|v| v.as_str())
            .unwrap_or("manual")
            .to_string();

        let mut changed = false;

        // Render a visual monitor layout frame
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(280.0, 120.0), egui::Sense::hover());
        ui.painter().rect(
            rect,
            CornerRadius::same(DialogTheme::R_SM),
            Color32::from_black_alpha(220),
            Stroke::new(1.0_f32, DialogTheme::BG_CARD_STROKE),
            egui::StrokeKind::Inside,
        );

        let btn_size = egui::vec2(52.0, 32.0);
        let margin = 10.0;

        let rect_tl = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + margin, rect.min.y + margin),
            btn_size,
        );
        let rect_tr = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - btn_size.x - margin, rect.min.y + margin),
            btn_size,
        );
        let rect_bl = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + margin, rect.max.y - btn_size.y - margin),
            btn_size,
        );
        let rect_br = egui::Rect::from_min_size(
            egui::pos2(
                rect.max.x - btn_size.x - margin,
                rect.max.y - btn_size.y - margin,
            ),
            btn_size,
        );
        let rect_manual = egui::Rect::from_center_size(rect.center(), btn_size);

        for (icon, val, target_rect) in [
            ("↖", "top_left", rect_tl),
            ("↗", "top_right", rect_tr),
            ("↙", "bottom_left", rect_bl),
            ("↘", "bottom_right", rect_br),
            ("✥", "manual", rect_manual),
        ] {
            let is_active = snap == val;
            let btn = egui::Button::new(RichText::new(icon).size(DialogTheme::FONT_BODY).strong())
                .fill(if is_active {
                    DialogTheme::BG_CONTROL_ACTIVE
                } else {
                    DialogTheme::BG_CONTROL
                })
                .stroke(Stroke::new(1.0_f32, DialogTheme::BG_CARD_STROKE))
                .corner_radius(CornerRadius::same(DialogTheme::R_SM));
            if ui.put(target_rect, btn).clicked() {
                snap = val.to_string();
                changed = true;
            }
        }

        if changed {
            position.insert("snap".into(), serde_json::json!(snap));
            overlay.insert("position".into(), serde_json::json!(position));
            ui.ctx().request_repaint_of(ui.ctx().parent_viewport_id());
        }
    });
}

fn varchive_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_card(ui, crate::t!("settings-varchive-connect"), |ui| {
        setting_row(ui, crate::t!("settings-link-status"), "", |ui| {
            ui.label(
                RichText::new(current_steam_label(ctx))
                    .color(DialogTheme::TEXT_MUTED)
                    .size(DialogTheme::FONT_BODY),
            );
        });

        if ctx.current_steam_id.is_empty() {
            ui.add_space(DialogTheme::GAP_SM);
            ui.label(
                RichText::new(crate::t!("settings-no-steam-account"))
                    .color(DialogTheme::TEXT_WARN)
                    .size(DialogTheme::FONT_HINT),
            );
            return;
        }

        ui.add_space(DialogTheme::GAP_MD);
        v_archive_id_row(ui, draft, ctx);
    });

    if ctx.current_steam_id.is_empty() {
        return;
    }

    section_card(ui, crate::t!("settings-varchive-upload"), |ui| {
        ui.label(
            RichText::new(crate::t!("settings-varchive-upload-desc"))
                .color(DialogTheme::TEXT_MUTED)
                .size(DialogTheme::FONT_HINT),
        );
        ui.add_space(DialogTheme::GAP_MD);
        account_path_row(ui, draft, ctx);
        ui.add_space(DialogTheme::GAP_LG);
        scan_candidates_row(ui, draft, ctx);
    });
}

fn current_steam_label(ctx: &SettingsUiContext) -> String {
    if ctx.current_steam_id.is_empty() {
        "Steam: -".to_string()
    } else {
        if let Ok(users) = ctx.steam_users.lock() {
            if let Some(user) = users.get(&ctx.current_steam_id) {
                if !user.persona_name.is_empty() && !user.account_name.is_empty() {
                    return format!(
                        "Steam: {} ({}) [{}]",
                        user.persona_name, user.account_name, ctx.current_steam_id
                    );
                } else if !user.persona_name.is_empty() {
                    return format!("Steam: {} [{}]", user.persona_name, ctx.current_steam_id);
                }
            }
        }
        format!("Steam: {}", ctx.current_steam_id)
    }
}

fn v_archive_id_row(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);
    let mut text = entry
        .get("v_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    field_row(ui, "V-Archive ID", "", |ui| {
        ui.horizontal(|ui| {
            let text_res = ui.add(
                TextEdit::singleline(&mut text)
                    .font(egui::FontId::proportional(DialogTheme::FONT_BODY))
                    .vertical_align(egui::Align::Center)
                    .margin(egui::Margin::symmetric(8, 0))
                    .desired_width(ui.available_width() - 90.0)
                    .min_size(egui::vec2(0.0, DialogTheme::CONTROL_HEIGHT)),
            );
            if text_res.changed() {
                entry.insert("v_id".into(), json!(text.trim()));
            }

            ui.add_space(DialogTheme::GAP_XS);

            let has_id = !text.trim().is_empty();
            ui.add_enabled_ui(has_id, |ui| {
                let refresh_btn = egui::Button::new(
                    RichText::new(crate::t!("sync-refresh")).size(DialogTheme::FONT_BODY),
                )
                .fill(if has_id {
                    DialogTheme::BG_CONTROL_ACTIVE
                } else {
                    DialogTheme::BG_CONTROL
                })
                .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                .wrap();

                if ui
                    .add_sized(egui::vec2(84.0, DialogTheme::CONTROL_HEIGHT), refresh_btn)
                    .clicked()
                {
                    let _ = ctx.fetch_tx.send((
                        ctx.current_steam_id.clone(),
                        text.trim().to_string(),
                        0,
                    ));
                }
            });
        });
    });
}

fn account_path_row(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);
    field_row(ui, "account.txt", "", |ui| {
        let mut path_str = entry
            .get("account_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        ui.horizontal(|ui| {
            let text_res = ui.add(
                TextEdit::singleline(&mut path_str)
                    .font(egui::FontId::proportional(DialogTheme::FONT_BODY))
                    .vertical_align(egui::Align::Center)
                    .margin(egui::Margin::symmetric(8, 0))
                    .desired_width(ui.available_width() - 90.0)
                    .min_size(egui::vec2(0.0, DialogTheme::CONTROL_HEIGHT)),
            );
            if text_res.changed() {
                entry.insert("account_path".into(), json!(path_str.trim()));
            }

            ui.add_space(DialogTheme::GAP_XS);

            let find_btn = egui::Button::new(
                RichText::new(crate::t!("settings-browse")).size(DialogTheme::FONT_BODY),
            )
            .fill(DialogTheme::BG_CONTROL_ACTIVE)
            .corner_radius(CornerRadius::same(DialogTheme::R_SM))
            .wrap();
            if ui
                .add_sized(egui::vec2(84.0, DialogTheme::CONTROL_HEIGHT), find_btn)
                .clicked()
            {
                if let Some(file_path) = rfd::FileDialog::new()
                    .add_filter("Text Files", &["txt"])
                    .pick_file()
                {
                    let path_str = file_path.to_string_lossy().to_string();
                    entry.insert("account_path".into(), json!(path_str));
                }
            }
        });
    });
}

fn scan_candidates_row(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let entry = user_entry_mut(draft, &ctx.current_steam_id);
    let account_path = entry
        .get("account_path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let has_account = !account_path.is_empty();

    ui.vertical(|ui| {
        ui.add_enabled_ui(has_account, |ui| {
            let scan_btn = egui::Button::new(
                RichText::new(crate::t!("settings-find-sync-candidates-btn"))
                    .size(DialogTheme::FONT_BODY)
                    .strong(),
            )
            .min_size(egui::vec2(ui.available_width(), 40.0))
            .fill(if has_account {
                DialogTheme::PRIMARY
            } else {
                DialogTheme::BG_CONTROL
            })
            .corner_radius(CornerRadius::same(DialogTheme::R_SM));

            if ui.add(scan_btn).clicked() {
                if let Ok(mut sid) = ctx.sync_steam_id.lock() {
                    *sid = ctx.current_steam_id.clone();
                }
                ctx.sync_open.store(true, Ordering::Relaxed);
                ctx.scan_pending.store(true, Ordering::Relaxed);
            }
        });

        if !has_account {
            ui.add_space(DialogTheme::GAP_XS);
            ui.label(
                RichText::new(crate::t!("settings-account-path-required"))
                    .color(DialogTheme::TEXT_MUTED)
                    .size(DialogTheme::FONT_HINT),
            );
        }
    });
}

fn advanced_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_card(ui, crate::t!("settings-diagnostics"), |ui| {
        debug_section(ui, draft, ctx);
    });
    section_card(ui, crate::t!("settings-screen-capture"), |ui| {
        capture_section(ui, draft);
    });
    section_card(ui, crate::t!("settings-update-section"), |ui| {
        update_section(ui, draft);
    });
    #[cfg(target_os = "linux")]
    {
        section_card(ui, crate::t!("Linux 앱 실행"), |ui| {
            setting_row(ui, crate::t!("앱 메뉴"), "", |ui| {
                if ui.button(crate::t!("바로가기 생성")).clicked() {
                    let result = crate::system::desktop_entry_linux::install(ctx.root.as_path());
                    let (title, description, level) = match result {
                        Ok(path) => (
                            "Overmax",
                            crate::t!("sys-shortcut-create-success", path = path.display()),
                            rfd::MessageLevel::Info,
                        ),
                        Err(error) => (
                            "Overmax",
                            crate::t!("sys-shortcut-create-failed", error = error),
                            rfd::MessageLevel::Error,
                        ),
                    };
                    let _ = rfd::MessageDialog::new()
                        .set_title(title)
                        .set_description(description)
                        .set_level(level)
                        .set_buttons(rfd::MessageButtons::Ok)
                        .show();
                }
            });
        });
    }
}

fn debug_section(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    let mut is_open = ctx.debug_open.load(Ordering::Relaxed);

    setting_row(
        ui,
        crate::t!("settings-debug-window"),
        crate::t!("settings-debug-window-hint"),
        |ui| {
            if ui
                .checkbox(&mut is_open, crate::t!("settings-show-debug-window"))
                .on_hover_text(crate::t!("settings-debug-window-desc"))
                .changed()
            {
                let debug_obj = object_section_mut(draft, "debug");
                debug_obj.insert("enabled".to_string(), Value::Bool(is_open));
                ctx.debug_open.store(is_open, Ordering::Relaxed);
                ui.ctx().request_repaint();
            }
        },
    );
}

fn capture_section(ui: &mut egui::Ui, draft: &mut Value) {
    let screen_capture = object_section_mut(draft, "screen_capture");

    #[cfg(target_os = "windows")]
    {
        let engine = screen_capture
            .get("engine")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .to_string();

        segmented_row(
            ui,
            crate::t!("settings-capture-method-win"),
            crate::t!("settings-capture-engine-hint"),
            |ui| {
                ui.horizontal(|ui| {
                    ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);

                    for (label, val) in [
                        (crate::t!("settings-capture-mode-auto"), "auto"),
                        (crate::t!("settings-capture-mode-dxgi"), "dxgi"),
                        (crate::t!("settings-capture-mode-gdi"), "gdi"),
                    ] {
                        let is_active = engine == val;
                        let btn = egui::Button::new(
                            RichText::new(label).size(DialogTheme::FONT_BODY).strong(),
                        )
                        .fill(if is_active {
                            DialogTheme::BG_CONTROL_ACTIVE
                        } else {
                            DialogTheme::BG_CONTROL
                        })
                        .stroke(Stroke::new(1.0_f32, DialogTheme::BG_CARD_STROKE))
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .wrap();

                        if ui
                            .add_sized(egui::vec2(90.0, DialogTheme::CONTROL_HEIGHT), btn)
                            .clicked()
                        {
                            screen_capture.insert("engine".into(), json!(val));
                        }
                    }
                });
            },
        );

        ui.add_space(DialogTheme::GAP_MD);
    }

    let mut content_protected = screen_capture
        .get("content_protected")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    setting_row(
        ui,
        crate::t!("settings-protect-overlay"),
        crate::t!("settings-protect-overlay-hint"),
        |ui| {
            let response = ui
                .checkbox(
                    &mut content_protected,
                    crate::t!("settings-prevent-screen-capture"),
                )
                .on_hover_text(crate::t!("settings-protect-overlay-desc"));

            if response.changed() {
                screen_capture.insert("content_protected".into(), json!(content_protected));
            }
        },
    );
}

fn general_section(ui: &mut egui::Ui, draft: &mut Value) {
    let current_lang = draft
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("ko")
        .to_string();

    segmented_row(ui, crate::t!("settings-language"), "", |ui| {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
            for (label, val) in [("한국어", "ko"), ("English", "en")] {
                let is_active = current_lang == val;
                let btn =
                    egui::Button::new(RichText::new(label).size(DialogTheme::FONT_BODY).strong())
                        .fill(if is_active {
                            DialogTheme::BG_CONTROL_ACTIVE
                        } else {
                            DialogTheme::BG_CONTROL
                        })
                        .stroke(Stroke::new(1.0, DialogTheme::BG_CARD_STROKE))
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .wrap();

                if ui
                    .add_sized(egui::vec2(84.0, DialogTheme::CONTROL_HEIGHT), btn)
                    .clicked()
                {
                    root_map_mut(draft).insert("language".into(), json!(val));
                }
            }
        });
    });
}

fn update_section(ui: &mut egui::Ui, draft: &mut Value) {
    let app_update = object_section_mut(draft, "app_update");
    let mut enabled = app_update
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    setting_row(
        ui,
        crate::t!("settings-auto-update"),
        crate::t!("settings-auto-update-hint"),
        |ui| {
            if ui
                .checkbox(
                    &mut enabled,
                    RichText::new(crate::t!("settings-use")).size(DialogTheme::FONT_BODY),
                )
                .changed()
            {
                app_update.insert("enabled".into(), json!(enabled));
            }
        },
    );

    ui.add_space(DialogTheme::GAP_MD);

    setting_row(ui, crate::t!("settings-version-info"), "", |ui| {
        ui.label(
            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .color(DialogTheme::TEXT_PRIMARY)
                .size(DialogTheme::FONT_BODY),
        );
    });
}

fn recommend_provider_section(ui: &mut egui::Ui, draft: &mut Value) {
    let rec_provider = object_section_mut(draft, "recommend_provider");

    ui.label(
        RichText::new(crate::t!("settings-recommend-provider-desc"))
            .color(DialogTheme::TEXT_MUTED)
            .size(DialogTheme::FONT_HINT),
    );
    ui.add_space(DialogTheme::GAP_MD);

    let mut enabled = rec_provider
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    setting_row(ui, crate::t!("settings-use-external-provider"), "", |ui| {
        if ui
            .checkbox(
                &mut enabled,
                RichText::new(crate::t!("settings-use")).size(DialogTheme::FONT_BODY),
            )
            .changed()
        {
            rec_provider.insert("enabled".into(), json!(enabled));
        }
    });

    ui.add_space(DialogTheme::GAP_MD);

    let mut url = rec_provider
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    field_row(ui, "Provider URL", "", |ui| {
        if ui
            .add(
                TextEdit::singleline(&mut url)
                    .hint_text("http://127.0.0.1:8080")
                    .desired_width(ui.available_width()),
            )
            .changed()
        {
            rec_provider.insert("url".into(), json!(url.trim()));
        }
    });

    ui.add_space(DialogTheme::GAP_MD);

    let mut name = rec_provider
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    field_row(ui, crate::t!("settings-display-name"), "", |ui| {
        if ui
            .add(
                TextEdit::singleline(&mut name)
                    .hint_text(crate::t!("settings-eg-hint", domain = "djmax.gg"))
                    .desired_width(ui.available_width()),
            )
            .changed()
        {
            rec_provider.insert("name".into(), json!(name.trim()));
        }
    });
}

pub fn render_settings_deferred(
    ui: &mut egui::Ui,
    class: ViewportClass,
    title: &str,
    draft: &mut Value,
    settings_ctx: &SettingsUiContext,
) {
    if class == ViewportClass::EmbeddedWindow {
        egui::Window::new(title).show(ui.ctx(), |ui| render_settings_form(ui, draft, settings_ctx));
    } else {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(DialogTheme::BG_WINDOW)
                    .inner_margin(Margin::same(DialogTheme::PANEL_PADDING as i8)),
            )
            .show(ui, |ui| render_settings_form(ui, draft, settings_ctx));
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
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        crate::ui::native_app_viewports::log_close_request("settings_ui::close_if_requested");
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

fn root_map_mut(draft: &mut Value) -> &mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    draft
        .as_object_mut()
        .expect("draft must be verified as a JSON Object")
}

fn object_section_mut<'a>(draft: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    if !draft.is_object() {
        *draft = Value::Object(Map::new());
    }
    let root = draft
        .as_object_mut()
        .expect("draft must be verified as a JSON Object");
    let entry = root
        .entry(section)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry
        .as_object_mut()
        .expect("entry must be verified as a JSON Object")
}

fn user_entry_mut<'a>(draft: &'a mut Value, steam_id: &str) -> &'a mut Map<String, Value> {
    let varchive = object_section_mut(draft, "varchive");
    let user_map_value = varchive
        .entry("user_map")
        .or_insert_with(|| Value::Object(Map::new()));
    if !user_map_value.is_object() {
        *user_map_value = Value::Object(Map::new());
    }
    let user_map = user_map_value
        .as_object_mut()
        .expect("user_map must be verified as a JSON Object");
    let entry = user_map
        .entry(steam_id)
        .or_insert_with(|| json!({"v_id": "", "account_path": ""}));
    if let Some(v_id) = entry.as_str().map(str::to_string) {
        *entry = json!({"v_id": v_id, "account_path": ""});
    }
    if !entry.is_object() {
        *entry = json!({"v_id": "", "account_path": ""});
    }
    entry
        .as_object_mut()
        .expect("entry must be verified as a JSON Object")
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
