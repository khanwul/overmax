//! 플레이어용 업로드 기록 창. 디버그 로그와 별개로 이번 실행 중 업로드 결과를 최신순으로 보여준다.
//! 메인 오버레이와 같은 카드 룩을 쓰며, 제목표시줄 없이(프레임리스) 헤더에서 드래그하고 헤더의 ✕로 닫는다.

use crate::ui::overlay_theme::Theme;
use eframe::egui::{
    self, Align, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, ViewportCommand,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_ENTRIES: usize = 200;
pub const WINDOW_SIZE: [f32; 2] = [360.0, 480.0];

#[derive(Clone, Debug)]
pub struct UploadHistoryEntry {
    pub at: Instant,
    pub song: String,
    pub pattern: String,
    pub rate: f64,
    pub max_combo: bool,
    pub success: bool,
    pub message: String,
}

/// 업로드 결과 기록 (메모리 전용, 앱 실행 중에만 유지). 최신 항목이 앞에 온다.
#[derive(Clone, Default)]
pub struct SharedUploadHistory {
    entries: Arc<Mutex<VecDeque<UploadHistoryEntry>>>,
}

impl SharedUploadHistory {
    pub fn push(&self, entry: UploadHistoryEntry) {
        let mut guard = overmax_core::lock_or_recover(&self.entries);
        guard.push_front(entry);
        guard.truncate(MAX_ENTRIES);
    }

    pub fn snapshot(&self) -> Vec<UploadHistoryEntry> {
        Vec::from(overmax_core::lock_clone_or_default(&self.entries))
    }
}

fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        crate::t!("upload-history-just-now").to_string()
    } else if secs < 3600 {
        crate::t!("upload-history-minutes-ago", n = (secs / 60) as i64)
    } else {
        crate::t!("upload-history-hours-ago", n = (secs / 3600) as i64)
    }
}

/// Esc 또는 OS 닫기 요청 시 창을 닫는다.
pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

/// 창 본문. 헤더(제목 드래그 · ✕ 닫기)와 기록 카드 목록을 그린다.
pub fn render_upload_history(
    ui: &mut egui::Ui,
    entries: &[UploadHistoryEntry],
    open: &Arc<AtomicBool>,
    scale: f32,
) {
    let ctx = ui.ctx().clone();
    Frame::new()
        .fill(Theme::PANEL_BG)
        .corner_radius(CornerRadius::same((14.0 * scale) as u8))
        .inner_margin(Margin::same((8.0 * scale) as i8))
        .show(ui, |ui| {
            Frame::new()
                .fill(Theme::HEADER_BG)
                .corner_radius(CornerRadius::same((10.0 * scale) as u8))
                .inner_margin(Margin::symmetric((12.0 * scale) as i8, (8.0 * scale) as i8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // 제목 라벨만 드래그 영역으로 써서 ✕ 버튼 클릭과 충돌하지 않게 한다.
                        let title = ui.add(
                            egui::Label::new(
                                RichText::new(crate::t!("upload-history-title"))
                                    .color(Theme::TEXT_PRIMARY)
                                    .font(FontId::proportional(14.0 * scale))
                                    .strong(),
                            )
                            .sense(Sense::drag()),
                        );
                        if title.drag_started() {
                            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let close = egui::Button::new(
                                RichText::new("✕")
                                    .color(Theme::TEXT_SECONDARY)
                                    .font(FontId::proportional(12.0 * scale)),
                            )
                            .fill(Theme::SECTION_BG)
                            .corner_radius(CornerRadius::same((3.5 * scale) as u8));
                            if ui.add_sized([18.0 * scale, 18.0 * scale], close).clicked() {
                                open.store(false, Ordering::Relaxed);
                            }
                        });
                    });
                });

            ui.add_space(6.0 * scale);

            if entries.is_empty() {
                ui.label(
                    RichText::new(crate::t!("upload-history-empty"))
                        .color(Theme::TEXT_MUTED)
                        .font(FontId::proportional(12.0 * scale)),
                );
                return;
            }

            let now = Instant::now();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in entries {
                        draw_row(ui, entry, now, scale);
                        ui.add_space(6.0 * scale);
                    }
                });
        });
}

fn draw_row(ui: &mut egui::Ui, entry: &UploadHistoryEntry, now: Instant, scale: f32) {
    Frame::new()
        .fill(Theme::SECTION_BG)
        .corner_radius(CornerRadius::same((8.0 * scale) as u8))
        .inner_margin(Margin::symmetric((10.0 * scale) as i8, (6.0 * scale) as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&entry.song)
                            .color(Theme::TEXT_PRIMARY)
                            .font(FontId::proportional(13.0 * scale))
                            .strong(),
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&entry.pattern)
                                .color(Theme::TEXT_SECONDARY)
                                .font(FontId::proportional(11.0 * scale)),
                        );
                        ui.label(
                            RichText::new(format!("{:.2}%", entry.rate))
                                .color(Theme::TEXT_ACCENT)
                                .font(FontId::proportional(11.0 * scale))
                                .strong(),
                        );
                        if entry.max_combo {
                            ui.label(
                                RichText::new("MAX COMBO")
                                    .color(Theme::OK)
                                    .font(FontId::proportional(10.5 * scale))
                                    .strong(),
                            );
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (text, color) = if entry.success {
                        (crate::t!("upload-history-result-ok"), Theme::OK)
                    } else {
                        (crate::t!("upload-history-result-failed"), Theme::WARN)
                    };
                    ui.vertical(|ui| {
                        ui.with_layout(Layout::top_down(Align::Max), |ui| {
                            ui.label(
                                RichText::new(text)
                                    .color(color)
                                    .font(FontId::proportional(11.0 * scale))
                                    .strong(),
                            )
                            .on_hover_text(entry.message.as_str());
                            ui.label(
                                RichText::new(format_elapsed(
                                    now.saturating_duration_since(entry.at),
                                ))
                                .color(Theme::TEXT_MUTED)
                                .font(FontId::proportional(10.5 * scale)),
                            );
                        });
                    });
                });
            });
        });
}
