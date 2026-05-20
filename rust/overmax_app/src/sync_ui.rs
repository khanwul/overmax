//! V-Archive sync window: list candidates and trigger scan / upload.

use crate::overlay_theme::{apply_secondary_window_style, Theme};
use eframe::egui::{
    self, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, ViewportClass,
};
use overmax_data::SyncCandidate;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn render_sync(
    ctx: &egui::Context,
    class: ViewportClass,
    steam_id: &mut String,
    status: &str,
    candidates: &[SyncCandidate],
    on_scan: impl Fn(),
    on_upload: impl Fn(usize) + Copy,
    on_delete: impl Fn(usize) + Copy,
) {
    let mut body = |ui: &mut egui::Ui| {
        apply_secondary_window_style(ui.ctx());
        ui.heading(
            RichText::new("V-Archive 동기화")
                .color(Theme::TEXT_PRIMARY)
                .size(Theme::FONT_HEADING)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Steam 계정 기준으로 업로드 후보를 확인합니다.")
                .color(Theme::TEXT_SECONDARY)
                .size(Theme::FONT_BODY),
        );
        ui.add_space(16.0);

        Frame::new()
            .fill(Theme::CARD)
            .stroke(Stroke::new(1.0, Theme::STROKE))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Steam ID").color(Theme::TEXT_PRIMARY).size(Theme::FONT_BODY));
                    ui.text_edit_singleline(steam_id);
                    if ui.button(RichText::new("스캔").size(Theme::FONT_BODY)).clicked() {
                        on_scan();
                    }
                });
                if !status.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new(status).size(Theme::FONT_SMALL).color(Theme::TEXT_MUTED));
                }
            });

        ui.add_space(16.0);
        ui.label(
            RichText::new(format!("후보 {}개", candidates.len()))
                .color(Theme::TEXT_PRIMARY)
                .size(Theme::FONT_BODY)
                .strong(),
        );
        ui.add_space(8.0);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, c) in candidates.iter().enumerate() {
                    candidate_row(ui, i, c, on_upload, on_delete);
                    ui.add_space(8.0);
                }
            });
    };

    if class == ViewportClass::Embedded {
        egui::Window::new("V-Archive 동기화").show(ctx, |ui| body(ui));
    } else {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Theme::PANEL_BG).inner_margin(Margin::same(24)))
            .show(ctx, |ui| body(ui));
    }
}

fn candidate_row<F: Fn(usize), D: Fn(usize)>(ui: &mut egui::Ui, index: usize, c: &SyncCandidate, on_upload: F, on_delete: D) {
    Frame::new()
        .fill(Theme::ROW_BG)
        .stroke(Stroke::new(1.0, Theme::STROKE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&c.song_name).color(Theme::TEXT_PRIMARY).size(Theme::FONT_BODY).strong());
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "{} {} · {:.1}% · {}",
                            c.button_mode,
                            c.difficulty,
                            c.overmax_rate,
                            c.reason_label()
                        ))
                        .size(Theme::FONT_SMALL)
                        .color(Theme::TEXT_SECONDARY),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new("등록").size(Theme::FONT_BODY)).clicked() {
                        on_upload(index);
                    }
                    ui.add_space(4.0);
                    if ui.button(RichText::new("삭제").size(Theme::FONT_BODY)).clicked() {
                        on_delete(index);
                    }
                });
            });
            if !c.upload_status.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("{} {}", c.upload_status, c.upload_message))
                        .size(Theme::FONT_SMALL)
                        .color(Theme::TEXT_ACCENT),
                );
            }
        });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}
