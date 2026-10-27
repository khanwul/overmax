//! 플레이어용 업로드 기록 창. 디버그 로그와 별개로 이번 실행 중 업로드 결과를 최신순으로 보여준다.
//! 메인 오버레이와 같은 헤더 룩(Theme, 같은 버튼 크기·라운드)을 쓰며, 제목표시줄 없이(프레임리스)
//! 헤더에서 드래그하고 헤더의 ✕로 닫는다.

use crate::ui::overlay_theme::Theme;
use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, TextWrapMode,
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

/// 기록 시각을 로컬 시간의 `YYYY-MM-DD HH:MM`으로 표시한다. 로컬 오프셋을 못 얻으면 UTC로 표시한다.
fn format_timestamp(unix_secs: i64, offset: time::UtcOffset) -> String {
    let utc = time::OffsetDateTime::from_unix_timestamp(unix_secs)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let local = utc.to_offset(offset);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.year(),
        u8::from(local.month()),
        local.day(),
        local.hour(),
        local.minute()
    )
}

/// Esc 또는 OS 닫기 요청 시 창을 닫는다.
pub fn close_if_requested(ctx: &egui::Context, open: &Arc<AtomicBool>) {
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        open.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ctx.parent_viewport_id());
    }
}

/// 창 본문. 창 전체를 패널 색으로 채운 뒤 헤더(제목 드래그 · ✕ 닫기)와 기록 카드 목록을 그린다.
/// 반환값은 이번 프레임에 헤더를 드래그 중인지 여부이다 (창 이동은 호출 쪽에서 처리한다).
pub fn render_upload_history(
    ui: &mut egui::Ui,
    entries: &[UploadHistoryRecord],
    open: &Arc<AtomicBool>,
    scale: f32,
) -> bool {
    // 내용이 짧아도 창 바닥이 검게 남지 않도록 창 전체 영역을 먼저 채운다.
    let window_rect = ui.max_rect();
    ui.painter().rect_filled(
        window_rect,
        CornerRadius::same((14.0 * scale) as u8),
        Theme::PANEL_BG,
    );

    let mut dragging = false;
    Frame::new()
        .inner_margin(Margin::same((8.0 * scale) as i8))
        .show(ui, |ui| {
            dragging = render_header(ui, open, scale);
            ui.add_space(6.0 * scale);

            if entries.is_empty() {
                ui.label(
                    RichText::new(crate::t!("upload-history-empty"))
                        .color(Theme::TEXT_MUTED)
                        .font(FontId::proportional(12.0 * scale)),
                );
                return;
            }

            // 로컬 오프셋은 프레임당 한 번만 구한다 (행마다 OS를 조회하지 않도록).
            let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in entries {
                        draw_row(ui, entry, offset, scale);
                        ui.add_space(6.0 * scale);
                    }
                });
        });
    dragging
}

/// 헤더: 메인 오버레이 헤더와 같은 색·라운드·버튼 크기를 쓴다. 제목 영역 전체가 드래그 핸들이고,
/// 오른쪽 ✕ 버튼만 닫기 동작을 가진다 (드래그 영역은 버튼을 덮지 않도록 버튼 왼쪽까지만 잡는다).
fn render_header(ui: &mut egui::Ui, open: &Arc<AtomicBool>, scale: f32) -> bool {
    let button_size = 18.0 * scale;
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
                let dragging = drag.dragged();

                let close = egui::Button::new(
                    RichText::new("✕")
                        .color(Theme::TEXT_PRIMARY)
                        .font(FontId::proportional(10.0 * scale)),
                )
                .fill(Theme::SECTION_BG)
                .corner_radius(CornerRadius::same((3.5 * scale) as u8))
                .wrap();
                if ui
                    .add_sized([button_size, button_size], close.sense(Sense::click()))
                    .on_hover_text(crate::t!("upload-history-close"))
                    .clicked()
                {
                    open.store(false, Ordering::Relaxed);
                }
                dragging
            })
            .inner
        })
        .inner
}

/// 퍼펙트(Rate 100.00%)인지 판정한다. 저장된 Rate는 소수 둘째 자리까지 맞춰 쓰이므로 여유 오차만 허용한다.
fn is_perfect(rate: f64) -> bool {
    rate >= 100.0 - 1e-6
}

/// 퍼펙트 배지: 강조색(TEXT_ACCENT) 알약 위에 어두운 글자를 올린다.
fn perfect_badge(ui: &mut egui::Ui, scale: f32) {
    Frame::new()
        .fill(Theme::TEXT_ACCENT)
        .corner_radius(CornerRadius::same((4.0 * scale) as u8))
        .inner_margin(Margin::symmetric((5.0 * scale) as i8, (1.0 * scale) as i8))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new("PERFECT")
                        .color(Theme::PANEL_BG)
                        .font(FontId::proportional(9.5 * scale))
                        .strong(),
                )
                .wrap_mode(TextWrapMode::Extend),
            );
        });
}

fn draw_row(ui: &mut egui::Ui, entry: &UploadHistoryRecord, offset: time::UtcOffset, scale: f32) {
    Frame::new()
        .fill(Theme::SECTION_BG)
        .corner_radius(CornerRadius::same((8.0 * scale) as u8))
        .inner_margin(Margin::symmetric((10.0 * scale) as i8, (6.0 * scale) as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // 오른쪽 결과/시각 칸의 폭을 먼저 고정하고, 왼쪽 정보는 남은 폭 안에서만 줄바꿈한다.
            let right_w = (120.0 * scale).min(ui.available_width() * 0.4);
            let left_w = (ui.available_width() - right_w - ui.spacing().item_spacing.x).max(0.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left_w, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        // 내용 폭으로 줄어들면 오른쪽 칸 시작 위치가 행마다 달라지므로 폭을 고정한다.
                        ui.set_width(left_w);
                        // 곡명이 길면 잘라서 오른쪽 칸을 밀어내지 않는다.
                        ui.add(
                            egui::Label::new(
                                RichText::new(&entry.song_name)
                                    .color(Theme::TEXT_PRIMARY)
                                    .font(FontId::proportional(13.0 * scale))
                                    .strong(),
                            )
                            .truncate(),
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
                            // 퍼펙트는 맥스 콤보를 포함하므로, 둘 중 퍼펙트 배지 하나만 보여준다.
                            if is_perfect(entry.rate) {
                                perfect_badge(ui, scale);
                            } else if entry.is_max_combo {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("MAX COMBO")
                                            .color(Theme::OK)
                                            .font(FontId::proportional(10.5 * scale))
                                            .strong(),
                                    )
                                    .wrap_mode(TextWrapMode::Extend),
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
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(right_w, 0.0),
                    Layout::top_down(Align::Max),
                    |ui| {
                        let (text, color) = if entry.success {
                            (crate::t!("upload-history-result-ok"), Theme::OK)
                        } else {
                            (crate::t!("upload-history-result-failed"), Theme::WARN)
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(text)
                                    .color(color)
                                    .font(FontId::proportional(11.0 * scale))
                                    .strong(),
                            )
                            .wrap_mode(TextWrapMode::Extend),
                        )
                        .on_hover_text(entry.message.as_str());
                        ui.add(
                            egui::Label::new(
                                RichText::new(format_timestamp(entry.uploaded_at, offset))
                                    .color(Theme::TEXT_MUTED)
                                    .font(FontId::proportional(10.5 * scale)),
                            )
                            .wrap_mode(TextWrapMode::Extend),
                        );
                    },
                );
            });
        });
}
