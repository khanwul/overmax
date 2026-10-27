//! 플레이어용 업로드 기록 창. 디버그 로그와 별개로 이번 실행 중 업로드 결과를 최신순으로 보여준다.
//! 메인 오버레이와 같은 카드 룩을 쓰며, 제목표시줄 없이(프레임리스) 헤더에서 드래그하고 헤더의 ✕로 닫는다.

use crate::ui::overlay_theme::Theme;
use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, Stroke,
    ViewportCommand,
};
use overmax_data::{UploadHistoryRecord, UPLOAD_HISTORY_LIMIT};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const WINDOW_SIZE: [f32; 2] = [360.0, 480.0];

/// 현재 시각 (Unix 초). 시계가 뒤로 가도 0을 반환해 패닉하지 않는다.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 업로드 기록 목록의 메모리 사본. DB(`record.db`)가 원본이고, 여기서는 창 표시용으로만 든다.
/// 최신 항목이 앞에 온다.
#[derive(Clone, Default)]
pub struct SharedUploadHistory {
    entries: Arc<Mutex<VecDeque<UploadHistoryRecord>>>,
}

impl SharedUploadHistory {
    pub fn push(&self, record: UploadHistoryRecord) {
        let mut guard = overmax_core::lock_or_recover(&self.entries);
        guard.push_front(record);
        guard.truncate(UPLOAD_HISTORY_LIMIT);
    }

    /// DB에서 다시 읽어 전체를 교체한다 (앱 기동 시, 사용자가 바뀔 때).
    pub fn replace(&self, records: Vec<UploadHistoryRecord>) {
        let mut guard = overmax_core::lock_or_recover(&self.entries);
        *guard = VecDeque::from(records);
    }

    pub fn snapshot(&self) -> Vec<UploadHistoryRecord> {
        Vec::from(overmax_core::lock_clone_or_default(&self.entries))
    }
}

fn format_elapsed(secs: i64) -> String {
    if secs < 60 {
        crate::t!("upload-history-just-now").to_string()
    } else if secs < 3600 {
        crate::t!("upload-history-minutes-ago", n = secs / 60)
    } else {
        crate::t!("upload-history-hours-ago", n = secs / 3600)
    }
}

/// Esc 또는 OS 닫기 요청 시 창을 닫는다.
pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

/// 창 본문. 창 전체를 패널 색으로 채운 뒤 헤더(제목 드래그 · ✕ 닫기)와 기록 카드 목록을 그린다.
pub fn render_upload_history(
    ui: &mut egui::Ui,
    entries: &[UploadHistoryRecord],
    open: &Arc<AtomicBool>,
    scale: f32,
) {
    // 내용이 짧아도 창 바닥이 검게 남지 않도록 창 전체 영역을 먼저 채운다.
    let window_rect = ui.max_rect();
    ui.painter().rect_filled(
        window_rect,
        CornerRadius::same((14.0 * scale) as u8),
        Theme::PANEL_BG,
    );

    Frame::new()
        .inner_margin(Margin::same((8.0 * scale) as i8))
        .show(ui, |ui| {
            render_header(ui, open, scale);
            ui.add_space(6.0 * scale);

            if entries.is_empty() {
                ui.label(
                    RichText::new(crate::t!("upload-history-empty"))
                        .color(Theme::TEXT_MUTED)
                        .font(FontId::proportional(12.0 * scale)),
                );
                return;
            }

            let now = now_unix();
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

/// 헤더: 제목 영역 전체가 드래그 핸들이고, 오른쪽 ✕ 버튼만 닫기 동작을 가진다.
fn render_header(ui: &mut egui::Ui, open: &Arc<AtomicBool>, scale: f32) {
    let ctx = ui.ctx().clone();
    let button_size = 20.0 * scale;
    Frame::new()
        .fill(Theme::HEADER_BG)
        .corner_radius(CornerRadius::same((10.0 * scale) as u8))
        .inner_margin(Margin::symmetric((12.0 * scale) as i8, (8.0 * scale) as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let drag_width = (ui.available_width() - button_size).max(0.0);
                let (drag_rect, drag) =
                    ui.allocate_exact_size(egui::vec2(drag_width, button_size), Sense::drag());
                ui.painter().text(
                    drag_rect.left_center(),
                    Align2::LEFT_CENTER,
                    crate::t!("upload-history-title").to_string(),
                    FontId::proportional(14.0 * scale),
                    Theme::TEXT_PRIMARY,
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }

                let close = egui::Button::new(
                    RichText::new("✕")
                        .color(Theme::TEXT_SECONDARY)
                        .font(FontId::proportional(12.0 * scale)),
                )
                .fill(Theme::SECTION_BG)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same((4.0 * scale) as u8));
                if ui
                    .add_sized([button_size, button_size], close)
                    .on_hover_text(crate::t!("upload-history-close"))
                    .clicked()
                {
                    open.store(false, Ordering::Relaxed);
                }
            });
        });
}

fn draw_row(ui: &mut egui::Ui, entry: &UploadHistoryRecord, now: i64, scale: f32) {
    Frame::new()
        .fill(Theme::SECTION_BG)
        .corner_radius(CornerRadius::same((8.0 * scale) as u8))
        .inner_margin(Margin::symmetric((10.0 * scale) as i8, (6.0 * scale) as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&entry.song_name)
                            .color(Theme::TEXT_PRIMARY)
                            .font(FontId::proportional(13.0 * scale))
                            .strong(),
                    );
                    ui.horizontal_wrapped(|ui| {
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
                        if let Some(delta) = entry.rate_delta {
                            ui.label(
                                RichText::new(format!("{delta:+.2}%"))
                                    .color(Theme::OK)
                                    .font(FontId::proportional(11.0 * scale))
                                    .strong(),
                            );
                        }
                        if entry.is_max_combo {
                            ui.label(
                                RichText::new("MAX COMBO")
                                    .color(Theme::OK)
                                    .font(FontId::proportional(10.5 * scale))
                                    .strong(),
                            );
                        }
                    });
                    if let Some(rank) = entry.top50_rank {
                        ui.label(
                            RichText::new(crate::t!("upload-history-top50-rank", rank = rank))
                                .color(Theme::TEXT_ACCENT)
                                .font(FontId::proportional(10.5 * scale))
                                .strong(),
                        );
                    }
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
                                RichText::new(format_elapsed((now - entry.uploaded_at).max(0)))
                                    .color(Theme::TEXT_MUTED)
                                    .font(FontId::proportional(10.5 * scale)),
                            );
                        });
                    });
                });
            });
        });
}
