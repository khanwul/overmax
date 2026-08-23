//! Desktop Dialog & Secondary Window Design System (ODDS).
//!
//! Provides typography, color tokens, 8pt spacing grid, and standard layout helpers
//! exclusively for desktop secondary windows (Settings, Sync, Debug).
//!
//! Isolated from `overlay_theme.rs` (which governs the in-game HUD overlay).

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke};

pub struct DialogTheme;

impl DialogTheme {
    // Surface & Elevation Colors
    pub const BG_WINDOW: Color32 = Color32::from_rgb(14, 18, 28);
    pub const BG_CARD: Color32 = Color32::from_rgb(22, 28, 44);
    pub const BG_CARD_STROKE: Color32 = Color32::from_rgb(32, 42, 66);
    pub const BG_CONTROL: Color32 = Color32::from_rgb(28, 36, 56);
    pub const BG_CONTROL_ACTIVE: Color32 = Color32::from_rgb(60, 80, 120);
    pub const BG_CONTROL_HOVER: Color32 = Color32::from_rgb(45, 60, 92);
    pub const BG_ACCENT_BTN: Color32 = Color32::from_rgb(255, 209, 102);

    // Text Colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 244, 255);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 160, 185);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(100, 110, 135);
    pub const TEXT_ACCENT: Color32 = Color32::from_rgb(255, 209, 102);
    pub const TEXT_WARN: Color32 = Color32::from_rgb(255, 75, 75);

    // Action & Status Colors
    pub const PRIMARY: Color32 = Color32::from_rgb(255, 209, 102);
    pub const SECONDARY: Color32 = Color32::from_rgb(60, 80, 120);
    pub const DANGER: Color32 = Color32::from_rgb(255, 75, 75);

    // Unified Corner Radius
    pub const R_SM: u8 = 6;
    pub const R_MD: u8 = 10;
    pub const R_LG: u8 = 14;

    // 8pt Spacing Grid
    pub const GAP_XS: f32 = 4.0;
    pub const GAP_SM: f32 = 8.0;
    pub const GAP_MD: f32 = 12.0;
    pub const GAP_LG: f32 = 16.0;
    pub const GAP_XL: f32 = 20.0;
    pub const PANEL_PADDING: f32 = 20.0;
    pub const CONTROL_HEIGHT: f32 = 32.0;

    // Typography Scale (pt)
    pub const FONT_TITLE: f32 = 18.0;
    pub const FONT_SECTION: f32 = 14.5;
    pub const FONT_BODY: f32 = 13.0;
    pub const FONT_HINT: f32 = 11.5;
    pub const FONT_TINY: f32 = 10.5;
}

/// Applies unified secondary window styling.
pub fn apply_dialog_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|s| {
        let mut families = std::collections::BTreeMap::new();
        families.insert(
            egui::TextStyle::Body,
            egui::FontId::new(DialogTheme::FONT_BODY, egui::FontFamily::Proportional),
        );
        families.insert(
            egui::TextStyle::Button,
            egui::FontId::new(DialogTheme::FONT_BODY, egui::FontFamily::Proportional),
        );
        families.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(DialogTheme::FONT_TITLE, egui::FontFamily::Proportional),
        );
        families.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(DialogTheme::FONT_BODY, egui::FontFamily::Monospace),
        );
        families.insert(
            egui::TextStyle::Small,
            egui::FontId::new(DialogTheme::FONT_HINT, egui::FontFamily::Proportional),
        );
        s.text_styles = families;

        s.visuals.widgets.inactive.bg_fill = DialogTheme::BG_CONTROL;
        s.visuals.widgets.hovered.bg_fill = DialogTheme::BG_CONTROL_HOVER;
        s.visuals.widgets.active.bg_fill = DialogTheme::PRIMARY;
        s.visuals.selection.bg_fill = DialogTheme::SECONDARY;

        s.visuals.window_corner_radius = DialogTheme::R_LG.into();
        s.visuals.window_stroke = egui::Stroke::NONE;
        s.visuals.window_shadow = egui::Shadow::NONE;

        s.spacing.item_spacing = egui::vec2(DialogTheme::GAP_MD, DialogTheme::GAP_MD);
        s.spacing.button_padding = egui::vec2(10.0, 6.0);
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.bar_inner_margin = 2.0;
    });
}

/// Renders a standardized Card Frame with title accent bar.
pub fn section_card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(DialogTheme::BG_CARD)
        .stroke(Stroke::new(1.0, DialogTheme::BG_CARD_STROKE))
        .corner_radius(CornerRadius::same(DialogTheme::R_MD))
        .inner_margin(Margin::same(DialogTheme::GAP_XL as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_at_least(egui::vec2(3.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 1.5, DialogTheme::PRIMARY);
                ui.add_space(DialogTheme::GAP_SM);
                ui.label(
                    RichText::new(title)
                        .color(DialogTheme::TEXT_PRIMARY)
                        .size(DialogTheme::FONT_SECTION)
                        .strong(),
                );
            });
            ui.add_space(DialogTheme::GAP_LG);
            add_contents(ui);
        });
    ui.add_space(DialogTheme::GAP_LG);
}

/// Standard Layout Pattern 1: Setting Row (Title & Hint on left, Control right-aligned).
pub fn setting_row(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(title)
                        .color(DialogTheme::TEXT_PRIMARY)
                        .size(DialogTheme::FONT_BODY)
                        .strong(),
                );
                if !hint.is_empty() {
                    ui.label(
                        RichText::new(hint)
                            .color(DialogTheme::TEXT_MUTED)
                            .size(DialogTheme::FONT_HINT),
                    );
                }
            });
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            add_control(ui);
        });
    });
}

/// Standard Layout Pattern 2: Segmented Option Row (Title & Hint, followed by options).
pub fn segmented_row(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    add_buttons: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .color(DialogTheme::TEXT_PRIMARY)
                    .size(DialogTheme::FONT_BODY)
                    .strong(),
            );
            if !hint.is_empty() {
                ui.label(
                    RichText::new(hint)
                        .color(DialogTheme::TEXT_MUTED)
                        .size(DialogTheme::FONT_HINT),
                );
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            add_buttons(ui);
        });
    });
}

/// Standard Layout Pattern 3: Full-width / Multi-line Input Row.
pub fn field_row(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    add_input: impl FnOnce(&mut egui::Ui),
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(label)
                    .color(DialogTheme::TEXT_PRIMARY)
                    .size(DialogTheme::FONT_BODY)
                    .strong(),
            );
            if !hint.is_empty() {
                ui.label(
                    RichText::new(hint)
                        .color(DialogTheme::TEXT_MUTED)
                        .size(DialogTheme::FONT_HINT),
                );
            }
        });
        ui.add_space(DialogTheme::GAP_XS);
        add_input(ui);
    });
}

/// Standardized Pill Tabs for Secondary Dialogs.
pub fn render_dialog_tabs(
    ui: &mut egui::Ui,
    _id_source: &str,
    labels: &[&str],
    active: &mut usize,
) {
    ui.horizontal(|ui| {
        ui.style_mut().spacing.item_spacing.x = DialogTheme::GAP_XS;

        Frame::new()
            .fill(DialogTheme::BG_CONTROL)
            .corner_radius(CornerRadius::same(DialogTheme::R_SM))
            .inner_margin(Margin::same(4))
            .show(ui, |ui| {
                for (idx, label) in labels.iter().enumerate() {
                    let is_active = *active == idx;
                    let text_color = if is_active {
                        DialogTheme::TEXT_ACCENT
                    } else {
                        DialogTheme::TEXT_SECONDARY
                    };
                    let bg_fill = if is_active {
                        DialogTheme::BG_CONTROL_ACTIVE
                    } else {
                        Color32::TRANSPARENT
                    };

                    let response = ui.add(
                        egui::Button::new(
                            RichText::new(*label)
                                .size(DialogTheme::FONT_BODY)
                                .color(text_color)
                                .strong(),
                        )
                        .fill(bg_fill)
                        .corner_radius(CornerRadius::same(DialogTheme::R_SM))
                        .stroke(Stroke::NONE),
                    );

                    if response.clicked() {
                        *active = idx;
                    }
                }
            });
    });
}
