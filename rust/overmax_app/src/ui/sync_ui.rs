//! V-Archive sync window: list candidates and trigger scan / upload.

use crate::ui::overlay_theme::{apply_secondary_window_style, Theme};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, ViewportClass};
use overmax_data::{matches_filter, RecordKey, SyncCandidate, SyncFilterSettings, LEVEL_LABELS};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SyncProps<'a, F1, F2, F3, F4>
where
    F1: Fn(),
    F2: Fn(RecordKey) + Copy,
    F3: Fn(RecordKey) + Copy,
    F4: Fn(SyncFilterSettings) + Copy,
{
    pub steam_id: &'a mut String,
    pub status: &'a str,
    pub candidates: &'a [SyncCandidate],
    pub steam_users: &'a std::collections::HashMap<String, crate::system::steam_session::SteamUser>,
    pub initial_filter: &'a SyncFilterSettings,
    pub on_scan: F1,
    pub on_upload: F2,
    pub on_delete: F3,
    pub on_filter_change: F4,
}

pub fn render_sync<F1, F2, F3, F4>(
    ui: &mut egui::Ui,
    class: ViewportClass,
    props: SyncProps<F1, F2, F3, F4>,
) where
    F1: Fn(),
    F2: Fn(RecordKey) + Copy,
    F3: Fn(RecordKey) + Copy,
    F4: Fn(SyncFilterSettings) + Copy,
{
    let mut body = |ui: &mut egui::Ui| {
        apply_secondary_window_style(ui.ctx());

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("V-Archive")
                    .color(Theme::TEXT_ACCENT)
                    .size(Theme::FONT_HEADING)
                    .strong(),
            );
            ui.label(
                RichText::new(crate::t!("sync-title"))
                    .color(Theme::TEXT_PRIMARY)
                    .size(Theme::FONT_HEADING)
                    .strong(),
            );
        });

        ui.add_space(4.0);
        ui.label(
            RichText::new(crate::t!("sync-desc"))
                .color(Theme::TEXT_SECONDARY)
                .size(Theme::FONT_BODY),
        );
        ui.add_space(16.0);

        steam_account_card(
            ui,
            props.steam_id,
            props.steam_users,
            props.status,
            &props.on_scan,
        );

        ui.add_space(12.0);

        let filter = filter_card(ui, props.initial_filter, props.on_filter_change);

        let total_count = props.candidates.len();
        let filtered_candidates: Vec<&SyncCandidate> = props
            .candidates
            .iter()
            .filter(|c| matches_filter(c, &filter))
            .collect();
        let filtered_count = filtered_candidates.len();

        let sort_mode = candidates_header(ui, total_count, filtered_count);
        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.style_mut().spacing.item_spacing.y = 12.0;
                for c in sort_candidates(filtered_candidates, sort_mode) {
                    candidate_row(ui, c, props.on_upload, props.on_delete);
                }
            });
    };

    if class == ViewportClass::EmbeddedWindow {
        egui::Window::new(crate::t!("sync-varchive-sync")).show(ui.ctx(), |ui| body(ui));
    } else {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Theme::PANEL_BG)
                    .inner_margin(Margin::same(24)),
            )
            .show(ui, |ui| body(ui));
    }
}

/// 상단 Steam 계정 카드: ID 표시/입력, 스캔 버튼, 상태 메시지.
fn steam_account_card<F: Fn()>(
    ui: &mut egui::Ui,
    steam_id: &mut String,
    steam_users: &std::collections::HashMap<String, crate::system::steam_session::SteamUser>,
    status: &str,
    on_scan: F,
) {
    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0_f32, Theme::STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let mut label_text = "Steam ID".to_string();
                if let Some(user) = steam_users.get(steam_id.as_str()) {
                    if !user.persona_name.is_empty() {
                        label_text = format!("{} ({})", user.persona_name, user.account_name);
                    }
                }

                ui.add_sized(
                    egui::vec2(160.0, Theme::CONTROL_HEIGHT),
                    egui::Label::new(
                        RichText::new(label_text)
                            .color(Theme::TEXT_PRIMARY)
                            .size(Theme::FONT_BODY),
                    )
                    .truncate(),
                );
                ui.add_space(8.0);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let scan_btn = egui::Button::new(
                        RichText::new(crate::t!("sync-scan"))
                            .size(Theme::FONT_BODY)
                            .strong(),
                    )
                    .fill(Theme::PRIMARY)
                    .corner_radius(egui::CornerRadius::same(Theme::R_SM));

                    if ui
                        .add_sized(egui::vec2(80.0, Theme::CONTROL_HEIGHT), scan_btn)
                        .clicked()
                    {
                        on_scan();
                    }

                    ui.add_space(8.0);

                    let text_edit = egui::TextEdit::singleline(steam_id)
                        .font(egui::FontId::proportional(Theme::FONT_BODY))
                        .vertical_align(egui::Align::Center)
                        .margin(egui::Margin::symmetric(8, 6));

                    ui.add_sized(
                        egui::vec2(ui.available_width(), Theme::CONTROL_HEIGHT),
                        text_edit,
                    );
                });
            });
            if !status.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(status)
                        .size(Theme::FONT_SMALL)
                        .color(Theme::TEXT_MUTED),
                );
            }
        });
}

/// 필터 카드: 접기/펼치기 토글과 필터 폼. 상태는 egui temp storage로 유지되며,
/// 변경 시 콜백이 호출된다. 최종 필터를 반환한다.
fn filter_card<F: Fn(SyncFilterSettings)>(
    ui: &mut egui::Ui,
    initial_filter: &SyncFilterSettings,
    on_filter_change: F,
) -> SyncFilterSettings {
    let filter_id = ui.make_persistent_id("sync_filter_settings");
    let mut filter = ui
        .data_mut(|d| d.get_temp::<SyncFilterSettings>(filter_id))
        .unwrap_or_else(|| initial_filter.clone());
    let mut filter_changed = false;

    Frame::new()
        .fill(Theme::CARD)
        .stroke(Stroke::new(1.0_f32, Theme::STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let icon = if filter.open { "▼" } else { "▶" };

                let toggle_btn = egui::Button::new(
                    RichText::new(crate::t!("sync-filter-btn", icon = icon))
                        .color(Theme::TEXT_ACCENT)
                        .size(Theme::FONT_BODY)
                        .strong(),
                )
                .fill(Color32::TRANSPARENT);

                if ui.add(toggle_btn).clicked() {
                    filter.open = !filter.open;
                    filter_changed = true;
                }

                if filter.open {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let reset_btn = egui::Button::new(
                            RichText::new(crate::t!("sync-reset-btn")).size(Theme::FONT_SMALL),
                        )
                        .fill(Theme::SECONDARY)
                        .corner_radius(CornerRadius::same(Theme::R_SM));
                        if ui.add(reset_btn).clicked() {
                            filter = SyncFilterSettings::default();
                            filter_changed = true;
                        }
                    });
                }
            });

            if filter.open {
                render_filter_form(ui, &mut filter, &mut filter_changed);
            }
        });

    if filter_changed {
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        on_filter_change(filter.clone());
    }
    filter
}

/// 필터 폼 그리드: 모드 / 난이도 / 레벨 슬라이더 / Rate 슬라이더 / 체크박스 행.
fn render_filter_form(
    ui: &mut egui::Ui,
    filter: &mut SyncFilterSettings,
    filter_changed: &mut bool,
) {
    ui.add_space(8.0);

    egui::Grid::new("sync_filter_form_grid")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .min_col_width(170.0)
        .show(ui, |ui| {
            // Row 1: Mode
            ui.add_sized(
                [170.0, 20.0],
                egui::Label::new(
                    RichText::new(crate::t!("sync-mode"))
                        .color(Theme::TEXT_SECONDARY)
                        .size(Theme::FONT_SMALL)
                        .strong(),
                ),
            );
            ui.horizontal(|ui| {
                for mode in [
                    overmax_core::Mode::B4,
                    overmax_core::Mode::B5,
                    overmax_core::Mode::B6,
                    overmax_core::Mode::B8,
                ] {
                    *filter_changed |= toggle_btn(
                        ui,
                        mode.as_str(),
                        match mode {
                            overmax_core::Mode::B4 => &mut filter.mode_4b,
                            overmax_core::Mode::B5 => &mut filter.mode_5b,
                            overmax_core::Mode::B6 => &mut filter.mode_6b,
                            _ => &mut filter.mode_8b,
                        },
                        crate::ui::components::ModeBadge::mode_color(mode),
                    );
                }
            });
            ui.end_row();

            // Row 2: Difficulty
            ui.add_sized(
                [170.0, 20.0],
                egui::Label::new(
                    RichText::new(crate::t!("sync-difficulty"))
                        .color(Theme::TEXT_SECONDARY)
                        .size(Theme::FONT_SMALL)
                        .strong(),
                ),
            );
            ui.horizontal(|ui| {
                for diff in [
                    overmax_core::Difficulty::NM,
                    overmax_core::Difficulty::HD,
                    overmax_core::Difficulty::MX,
                    overmax_core::Difficulty::SC,
                ] {
                    *filter_changed |= toggle_btn(
                        ui,
                        diff.as_str(),
                        match diff {
                            overmax_core::Difficulty::NM => &mut filter.diff_nm,
                            overmax_core::Difficulty::HD => &mut filter.diff_hd,
                            overmax_core::Difficulty::MX => &mut filter.diff_mx,
                            _ => &mut filter.diff_sc,
                        },
                        crate::ui::overlay_ui::diff_color(diff),
                    );
                }
            });
            ui.end_row();

            // Row 3: Level Slider
            let min_lbl = LEVEL_LABELS.get(filter.min_level_idx).unwrap_or(&"1");
            let max_lbl = LEVEL_LABELS.get(filter.max_level_idx).unwrap_or(&"SC15");
            ui.add_sized(
                [170.0, 20.0],
                egui::Label::new(
                    RichText::new(crate::t!("sync-level-range", min = min_lbl, max = max_lbl))
                        .color(Theme::TEXT_SECONDARY)
                        .size(Theme::FONT_SMALL)
                        .strong(),
                ),
            );
            let mut min_f = filter.min_level_idx as f64;
            let mut max_f = filter.max_level_idx as f64;
            if dual_thumb_range_slider(ui, "level_range_slider", &mut min_f, &mut max_f, 0.0, 29.0)
            {
                filter.min_level_idx = min_f.round() as usize;
                filter.max_level_idx = max_f.round() as usize;
                *filter_changed = true;
            }
            ui.end_row();

            // Row 4: Rate Slider
            ui.add_sized(
                [170.0, 20.0],
                egui::Label::new(
                    RichText::new(format!(
                        "Rate ({:.1}% ~ {:.1}%)",
                        filter.min_rate, filter.max_rate
                    ))
                    .color(Theme::TEXT_SECONDARY)
                    .size(Theme::FONT_SMALL)
                    .strong(),
                ),
            );
            if dual_thumb_range_slider(
                ui,
                "rate_range_slider",
                &mut filter.min_rate,
                &mut filter.max_rate,
                0.0,
                100.0,
            ) {
                *filter_changed = true;
            }
            ui.end_row();

            // Row 5: Checkboxes
            ui.add_sized([170.0, 20.0], egui::Label::new(""));
            ui.horizontal(|ui| {
                if ui
                    .checkbox(
                        &mut filter.require_mc_not_on_varchive,
                        RichText::new(crate::t!("sync-max-combo-only"))
                            .size(Theme::FONT_SMALL)
                            .color(Theme::TEXT_PRIMARY),
                    )
                    .changed()
                {
                    *filter_changed = true;
                }

                ui.add_space(20.0);

                if ui
                    .checkbox(
                        &mut filter.exclude_unuploaded,
                        RichText::new(crate::t!("sync-exclude-unuploaded"))
                            .size(Theme::FONT_SMALL)
                            .color(Theme::TEXT_PRIMARY),
                    )
                    .changed()
                {
                    *filter_changed = true;
                }
            });
            ui.end_row();
        });
}

/// 후보 리스트 헤더: 건수 표시와 정렬 토글. 선택된 정렬 모드를 반환한다.
fn candidates_header(ui: &mut egui::Ui, total_count: usize, filtered_count: usize) -> SyncSortMode {
    let sort_mode_id = ui.make_persistent_id("sync_sort_mode");
    let mut sort_mode =
        ui.data_mut(|d| d.get_temp::<SyncSortMode>(sort_mode_id).unwrap_or_default());

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::t!("sync-upload-candidates"))
                .color(Theme::TEXT_PRIMARY)
                .size(Theme::FONT_BODY)
                .strong(),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("{} / {}", filtered_count, total_count))
                .color(Theme::TEXT_ACCENT)
                .size(Theme::FONT_BODY)
                .strong(),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let diff_btn_fill = if sort_mode == SyncSortMode::RateDiff {
                Theme::PRIMARY
            } else {
                Theme::SECONDARY
            };
            let diff_btn = egui::Button::new(
                RichText::new(crate::t!("sync-sort-by-change")).size(Theme::FONT_SMALL),
            )
            .fill(diff_btn_fill)
            .corner_radius(CornerRadius::same(Theme::R_SM));
            if ui.add(diff_btn).clicked() {
                sort_mode = SyncSortMode::RateDiff;
                ui.data_mut(|d| d.insert_temp(sort_mode_id, sort_mode));
            }

            ui.add_space(4.0);

            let title_btn_fill = if sort_mode == SyncSortMode::Title {
                Theme::PRIMARY
            } else {
                Theme::SECONDARY
            };
            let title_btn = egui::Button::new(
                RichText::new(crate::t!("sync-sort-by-title")).size(Theme::FONT_SMALL),
            )
            .fill(title_btn_fill)
            .corner_radius(CornerRadius::same(Theme::R_SM));
            if ui.add(title_btn).clicked() {
                sort_mode = SyncSortMode::Title;
                ui.data_mut(|d| d.insert_temp(sort_mode_id, sort_mode));
            }
        });
    });
    sort_mode
}

/// 정렬 모드에 따라 후보 목록을 정렬해 반환한다.
fn sort_candidates(
    mut candidates: Vec<&SyncCandidate>,
    sort_mode: SyncSortMode,
) -> Vec<&SyncCandidate> {
    match sort_mode {
        SyncSortMode::Title => {
            candidates.sort_by(|a, b| {
                let mode_cmp = a.button_mode.cmp(&b.button_mode);
                if mode_cmp != std::cmp::Ordering::Equal {
                    return mode_cmp;
                }
                a.song_name.cmp(&b.song_name)
            });
        }
        SyncSortMode::RateDiff => {
            candidates.sort_by(|a, b| {
                let diff_a = match a.varchive_rate {
                    None => a.overmax_rate,
                    Some(vr) => a.overmax_rate - vr,
                };
                let diff_b = match b.varchive_rate {
                    None => b.overmax_rate,
                    Some(vr) => b.overmax_rate - vr,
                };
                diff_b
                    .partial_cmp(&diff_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
    candidates
}

fn toggle_btn(ui: &mut egui::Ui, text: &str, active: &mut bool, active_color: Color32) -> bool {
    let fill = if *active {
        active_color
    } else {
        Theme::SECONDARY
    };
    let text_color = if *active {
        Theme::TEXT_BRIGHT
    } else {
        Theme::TEXT_MUTED
    };
    let btn = egui::Button::new(
        RichText::new(text)
            .size(Theme::FONT_TINY)
            .color(text_color)
            .strong(),
    )
    .fill(fill)
    .corner_radius(CornerRadius::same(Theme::R_SM));

    if ui.add(btn).clicked() {
        *active = !*active;
        true
    } else {
        false
    }
}

fn dual_thumb_range_slider(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + std::fmt::Debug,
    min_val: &mut f64,
    max_val: &mut f64,
    min_limit: f64,
    max_limit: f64,
) -> bool {
    let desired_size = egui::vec2(ui.available_width().clamp(120.0, 240.0), 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

    if !ui.is_rect_visible(rect) {
        return false;
    }

    let val_range = (max_limit - min_limit).max(1e-6);
    let thumb_radius = 6.0_f32;
    let track_margin = thumb_radius + 2.0;

    let track_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + track_margin, rect.center().y - 3.0),
        egui::pos2(rect.max.x - track_margin, rect.center().y + 3.0),
    );
    let track_width = track_rect.width();

    let min_frac = ((*min_val - min_limit) / val_range).clamp(0.0, 1.0);
    let max_frac = ((*max_val - min_limit) / val_range).clamp(0.0, 1.0);

    let min_thumb_pos = egui::pos2(
        track_rect.min.x + (min_frac as f32) * track_width,
        track_rect.center().y,
    );
    let max_thumb_pos = egui::pos2(
        track_rect.min.x + (max_frac as f32) * track_width,
        track_rect.center().y,
    );

    let id = ui.make_persistent_id(id_source);
    let active_thumb_id = id.with("active_thumb");

    let mut active_thumb: Option<u8> = ui.data_mut(|d| d.get_temp(active_thumb_id));
    let mut changed = false;

    let pointer_pos = ui.input(|i| i.pointer.interact_pos());

    if response.drag_started() || (response.clicked() && active_thumb.is_none()) {
        if let Some(pos) = pointer_pos {
            let dist_min = (pos.x - min_thumb_pos.x).abs();
            let dist_max = (pos.x - max_thumb_pos.x).abs();
            if dist_min <= dist_max {
                active_thumb = Some(0);
            } else {
                active_thumb = Some(1);
            }
            ui.data_mut(|d| d.insert_temp(active_thumb_id, active_thumb));
        }
    }

    if response.dragged() || response.clicked() {
        if active_thumb.is_none() {
            if let Some(pos) = pointer_pos {
                let dist_min = (pos.x - min_thumb_pos.x).abs();
                let dist_max = (pos.x - max_thumb_pos.x).abs();
                active_thumb = if dist_min <= dist_max {
                    Some(0)
                } else {
                    Some(1)
                };
                ui.data_mut(|d| d.insert_temp(active_thumb_id, active_thumb));
            }
        }

        if let Some(pos) = pointer_pos {
            let norm_x = ((pos.x - track_rect.min.x) / track_width).clamp(0.0, 1.0) as f64;
            let target_val = min_limit + norm_x * val_range;

            match active_thumb {
                Some(0) => {
                    let new_min = target_val.min(*max_val);
                    if (new_min - *min_val).abs() > 1e-4 {
                        *min_val = new_min;
                        changed = true;
                    }
                }
                Some(1) => {
                    let new_max = target_val.max(*min_val);
                    if (new_max - *max_val).abs() > 1e-4 {
                        *max_val = new_max;
                        changed = true;
                    }
                }
                _ => {}
            }
        }
    }

    if (response.drag_stopped() || (!response.dragged() && !response.clicked()))
        && active_thumb.is_some()
    {
        active_thumb = None;
        ui.data_mut(|d| d.insert_temp(active_thumb_id, active_thumb));
    }

    ui.painter().rect_filled(track_rect, 3.0, Theme::SECONDARY);

    let active_track = egui::Rect::from_min_max(
        egui::pos2(min_thumb_pos.x, track_rect.min.y),
        egui::pos2(max_thumb_pos.x, track_rect.max.y),
    );
    ui.painter().rect_filled(active_track, 3.0, Theme::PRIMARY);

    let min_hovered = response
        .hover_pos()
        .is_some_and(|p| p.distance(min_thumb_pos) <= thumb_radius * 1.8);
    let min_fill = if active_thumb == Some(0) || min_hovered {
        Theme::TEXT_BRIGHT
    } else {
        Theme::TEXT_PRIMARY
    };
    ui.painter()
        .circle_filled(min_thumb_pos, thumb_radius, min_fill);
    ui.painter().circle_stroke(
        min_thumb_pos,
        thumb_radius,
        egui::Stroke::new(1.5_f32, Theme::PRIMARY),
    );

    let max_hovered = response
        .hover_pos()
        .is_some_and(|p| p.distance(max_thumb_pos) <= thumb_radius * 1.8);
    let max_fill = if active_thumb == Some(1) || max_hovered {
        Theme::TEXT_BRIGHT
    } else {
        Theme::TEXT_PRIMARY
    };
    ui.painter()
        .circle_filled(max_thumb_pos, thumb_radius, max_fill);
    ui.painter().circle_stroke(
        max_thumb_pos,
        thumb_radius,
        egui::Stroke::new(1.5_f32, Theme::PRIMARY),
    );

    if changed {
        response.mark_changed();
    }

    changed
}

fn candidate_row<F: Fn(RecordKey), D: Fn(RecordKey)>(
    ui: &mut egui::Ui,
    c: &SyncCandidate,
    on_upload: F,
    on_delete: D,
) {
    Frame::new()
        .fill(Theme::ROW_BG)
        .stroke(Stroke::new(1.0_f32, Theme::STROKE))
        .corner_radius(CornerRadius::same(Theme::R_MD))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&c.song_name)
                            .color(Theme::TEXT_PRIMARY)
                            .size(Theme::FONT_BODY)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        // Button Mode Badge
                        let mode_color =
                            crate::ui::components::ModeBadge::mode_color(c.button_mode);
                        badge(ui, c.button_mode.as_str(), mode_color, Theme::TEXT_PRIMARY);
                        ui.add_space(4.0);
                        // Difficulty Badge
                        let diff_color = crate::ui::overlay_ui::diff_color(c.difficulty);
                        badge(ui, c.difficulty.as_str(), diff_color, Theme::TEXT_BRIGHT);
                        ui.add_space(8.0);
                        if let Some(lvl) = c.pattern_level {
                            ui.label(
                                RichText::new(format!("Lv.{}", lvl))
                                    .size(Theme::FONT_SMALL)
                                    .color(Theme::TEXT_SECONDARY),
                            );
                            ui.add_space(8.0);
                        }
                        ui.label(
                            RichText::new(format!("{:.2}%", c.overmax_rate))
                                .size(Theme::FONT_SMALL)
                                .color(Theme::TEXT_ACCENT)
                                .strong(),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format_candidate_reason(c))
                                .size(Theme::FONT_SMALL)
                                .color(Theme::TEXT_MUTED),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let upload_btn = egui::Button::new(
                        RichText::new(crate::t!("sync-register"))
                            .size(Theme::FONT_SMALL)
                            .strong(),
                    )
                    .min_size(egui::vec2(60.0, Theme::CONTROL_HEIGHT))
                    .fill(Theme::PRIMARY)
                    .stroke(Stroke::new(1.0_f32, Theme::STROKE))
                    .corner_radius(CornerRadius::same(Theme::R_SM));
                    if ui.add(upload_btn).clicked() {
                        if let Some(key) = c.key() {
                            on_upload(key);
                        }
                    }

                    ui.add_space(4.0);

                    let del_btn = egui::Button::new(
                        RichText::new(crate::t!("sync-delete")).size(Theme::FONT_SMALL),
                    )
                    .min_size(egui::vec2(60.0, Theme::CONTROL_HEIGHT))
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0_f32, Theme::STROKE))
                    .corner_radius(CornerRadius::same(Theme::R_SM));
                    if ui.add(del_btn).clicked() {
                        if let Some(key) = c.key() {
                            on_delete(key);
                        }
                    }
                });
            });
            if !c.upload_status.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("{} {}", c.upload_status, c.upload_message))
                        .size(Theme::FONT_SMALL)
                        .color(Theme::TEXT_ACCENT),
                );
            }
        });
}

fn badge(ui: &mut egui::Ui, text: &str, bg: Color32, text_color: Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .color(text_color)
                    .size(Theme::FONT_TINY)
                    .strong(),
            );
        });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
enum SyncSortMode {
    #[default]
    Title,
    RateDiff,
}

fn format_candidate_reason(c: &SyncCandidate) -> String {
    let mut parts = Vec::new();
    if c.varchive_rate.is_none() {
        parts.push(crate::t!("sync-reason-not-registered").to_string());
    } else if c.overmax_rate > c.varchive_rate.unwrap_or(0.0) {
        parts.push(format!(
            "+{:.2}%",
            c.overmax_rate - c.varchive_rate.unwrap_or(0.0)
        ));
    }
    if c.overmax_rate >= 100.0 {
        parts.push("P".to_string());
    } else if c.overmax_mc && !c.varchive_mc.unwrap_or(false) {
        parts.push("M".to_string());
    }
    parts.join(" · ")
}
