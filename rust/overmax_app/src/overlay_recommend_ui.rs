use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Label, Layout, Margin, RichText, Vec2,
};
use overmax_core::GameSessionState;
use overmax_data::{RecommendEntry, RecommendResult};

const TAB_WIDTH: f32 = 52.0;
const TAB_HEIGHT: f32 = 46.0;
const TAB_GAP: f32 = 4.0;
const TAB_PAD_Y: f32 = 6.0;
const BODY_HEIGHT: f32 = TAB_PAD_Y * 2.0 + TAB_HEIGHT * 4.0 + TAB_GAP * 3.0;
const RECOMMEND_WIDTH: f32 = 286.0;
const RECOMMEND_PAD_Y: f32 = 8.0;
const RECOMMEND_ROW_GAP: f32 = 4.0;

#[derive(Clone, Debug, PartialEq)]
pub struct PatternTabInfo {
    pub diff: String,
    pub level: Option<u32>,
    pub floor_name: Option<String>,
}

pub fn draw_diff_tabs(ui: &mut egui::Ui, active: Option<&str>, patterns: &[PatternTabInfo]) {
    ui.set_width(TAB_WIDTH);
    ui.vertical(|ui| {
        ui.add_space(TAB_PAD_Y);
        ui.spacing_mut().item_spacing.y = TAB_GAP;
        for diff in ["NM", "HD", "MX", "SC"] {
            draw_diff_tab(ui, diff, active, patterns);
        }
        ui.add_space(TAB_PAD_Y);
    });
}

pub fn draw_recommendations(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    recommendations: &RecommendResult,
) {
    ui.allocate_ui_with_layout(
        Vec2::new(RECOMMEND_WIDTH, BODY_HEIGHT),
        Layout::top_down(Align::Min),
        |ui| {
            ui.add_space(RECOMMEND_PAD_Y);
            draw_recommend_content(ui, state, recommendations);
            ui.add_space(RECOMMEND_PAD_Y);
        },
    );
}

pub fn avg_rate_text(result: &RecommendResult, confidence: f32) -> String {
    if result.avg_rate >= 0.0 {
        format!("{:.2}%", result.avg_rate)
    } else {
        format!("신뢰도 {:.0}%", confidence * 100.0)
    }
}

pub fn pattern_count_text(result: &RecommendResult) -> String {
    format!("{}/{}개 패턴", result.has_record_count, result.total_count)
}

fn draw_diff_tab(ui: &mut egui::Ui, diff: &str, active: Option<&str>, patterns: &[PatternTabInfo]) {
    let pattern = patterns.iter().find(|item| item.diff == diff);
    let exists = pattern.is_some();
    Frame::new()
        .fill(tab_fill(active == Some(diff), exists))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(TAB_WIDTH, TAB_HEIGHT));
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(6.0);
                ui.add(diff_label(diff));
                ui.add(pattern_floor_label(pattern, active == Some(diff), exists));
            });
        });
}

fn draw_recommend_content(
    ui: &mut egui::Ui,
    state: &GameSessionState,
    recommendations: &RecommendResult,
) {
    if state.song_id.is_none() || state.mode.is_none() || state.diff.is_none() {
        draw_empty_recommend(ui, "패턴을 감지하는 중...");
    } else if recommendations.entries.is_empty() {
        draw_empty_recommend(ui, "추천 결과 없음");
    } else {
        ui.spacing_mut().item_spacing.y = RECOMMEND_ROW_GAP;
        for entry in recommendations.entries.iter().take(6) {
            draw_recommend_row(ui, entry);
        }
    }
}

fn draw_empty_recommend(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
        ui.add(
            Label::new(
                RichText::new(text)
                    .color(Color32::from_rgb(80, 88, 112))
                    .font(FontId::proportional(11.0)),
            )
            .selectable(false),
        );
    });
}

fn draw_recommend_row(ui: &mut egui::Ui, entry: &RecommendEntry) {
    Frame::new()
        .fill(Color32::from_rgb(36, 46, 70))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 0))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(RECOMMEND_WIDTH, 30.0));
            ui.horizontal(|ui| {
                draw_entry_badge(ui, entry);
                ui.label(song_name_text(entry));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    draw_rate(ui, entry)
                });
            });
        });
}

fn draw_entry_badge(ui: &mut egui::Ui, entry: &RecommendEntry) {
    let text = badge_text(entry);
    let width = if entry.floor_name.is_none() {
        28.0
    } else {
        36.0
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), diff_color(&entry.difficulty));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(10.0),
        Color32::WHITE,
    );
}

fn draw_rate(ui: &mut egui::Ui, entry: &RecommendEntry) {
    let Some(rate) = entry.rate else {
        ui.label(RichText::new("——").color(Color32::from_rgb(80, 88, 112)));
        return;
    };
    ui.label(
        RichText::new(format!("{rate:.2}%"))
            .color(rate_color(rate))
            .font(FontId::proportional(11.0))
            .strong(),
    );
    if rate >= 100.0 {
        draw_status_badge(ui, "P", Color32::from_rgb(160, 54, 210));
    } else if entry.is_max_combo {
        draw_status_badge(ui, "M", Color32::from_rgb(48, 200, 255));
    }
}

fn draw_status_badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 8.0, color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(9.0),
        Color32::WHITE,
    );
}

fn diff_label(diff: &str) -> Label {
    Label::new(
        RichText::new(diff.to_string())
            .color(diff_color(diff))
            .font(FontId::proportional(11.0))
            .strong(),
    )
}

fn pattern_floor_label(pattern: Option<&PatternTabInfo>, active: bool, exists: bool) -> Label {
    Label::new(
        RichText::new(pattern_label(pattern))
            .color(pattern_text_color(active, exists))
            .font(FontId::proportional(10.0))
            .strong(),
    )
}

fn song_name_text(entry: &RecommendEntry) -> RichText {
    RichText::new(&entry.song_name)
        .color(Color32::from_rgb(232, 238, 255))
        .font(FontId::proportional(11.0))
        .strong()
}

fn badge_text(entry: &RecommendEntry) -> String {
    if entry.floor_name.is_none() {
        entry.difficulty.clone()
    } else {
        format!("{} {}", entry.difficulty, entry.level.unwrap_or_default())
    }
}

fn tab_fill(active: bool, exists: bool) -> Color32 {
    if !exists {
        Color32::from_rgb(20, 26, 40)
    } else if active {
        Color32::from_rgb(63, 80, 117)
    } else {
        Color32::from_rgb(28, 36, 54)
    }
}

fn pattern_label(pattern: Option<&PatternTabInfo>) -> String {
    let Some(pattern) = pattern else {
        return "—".into();
    };
    pattern
        .floor_name
        .clone()
        .or_else(|| pattern.level.map(|level| format!("Lv{level}")))
        .unwrap_or_else(|| "—".into())
}

fn pattern_text_color(active: bool, exists: bool) -> Color32 {
    if active {
        Color32::from_rgb(180, 203, 255)
    } else if exists {
        Color32::from_rgb(136, 145, 167)
    } else {
        Color32::from_rgb(80, 88, 112)
    }
}

fn diff_color(diff: &str) -> Color32 {
    match diff {
        "NM" => Color32::from_rgb(0x4A, 0x90, 0xD9),
        "HD" => Color32::from_rgb(0xF5, 0xA6, 0x23),
        "MX" => Color32::from_rgb(0xD0, 0x02, 0x1B),
        "SC" => Color32::from_rgb(0x9B, 0x59, 0xB6),
        _ => Color32::WHITE,
    }
}

fn rate_color(rate: f64) -> Color32 {
    if rate >= 100.0 {
        Color32::from_rgb(255, 215, 0)
    } else if rate >= 99.0 {
        Color32::from_rgb(184, 220, 255)
    } else if rate >= 95.0 {
        Color32::from_rgb(126, 200, 227)
    } else if rate >= 90.0 {
        Color32::from_rgb(181, 234, 215)
    } else {
        Color32::from_rgb(255, 153, 153)
    }
}

#[cfg(test)]
mod tests {
    use super::{pattern_label, PatternTabInfo};

    #[test]
    fn formats_pattern_tab_label() {
        let pattern = PatternTabInfo {
            diff: "SC".into(),
            level: Some(12),
            floor_name: Some("12.3".into()),
        };

        assert_eq!(pattern_label(Some(&pattern)), "12.3");
    }
}
