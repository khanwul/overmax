//! V-Archive sync window — list candidates and trigger scan / upload (wired from `native_app`).

use eframe::egui::{self, RichText, ScrollArea, ViewportClass};
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
) {
    let mut body = |ui: &mut egui::Ui| {
        ui.horizontal(|ui| {
            ui.label("Steam ID");
            ui.text_edit_singleline(steam_id);
            if ui.button("스캔").clicked() {
                on_scan();
            }
        });
        ui.label(RichText::new(status).small());
        ui.separator();
        ScrollArea::vertical().show(ui, |ui| {
            for (i, c) in candidates.iter().enumerate() {
                row(ui, i, c, on_upload);
            }
        });
    };

    if class == ViewportClass::Embedded {
        egui::Window::new("V-Archive 동기화").show(ctx, |ui| body(ui));
    } else {
        egui::CentralPanel::default().show(ctx, |ui| body(ui));
    }
}

fn row<F: Fn(usize)>(ui: &mut egui::Ui, index: usize, c: &SyncCandidate, on_upload: F) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.monospace(format!(
                "{} {} {} {:.1}%",
                c.song_name, c.button_mode, c.difficulty, c.overmax_rate
            ));
            ui.label(RichText::new(c.reason_label()).weak());
            if ui.button("등록").clicked() {
                on_upload(index);
            }
        });
        if !c.upload_status.is_empty() {
            ui.label(
                RichText::new(format!("{} {}", c.upload_status, c.upload_message))
                    .small()
                    .color(egui::Color32::LIGHT_BLUE),
            );
        }
    });
}

pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested()) {
        open.store(false, Ordering::Relaxed);
    }
}
