//! Krypteia · MTP Sync visual theme.
//!
//! Aesthetic: dark, restrained console — macOS-class clean lines, deep scarlet
//! accents used sparingly. Monospace primary for the file-listing density;
//! sans for chrome/labels. No gradients, no animations beyond progress.

use eframe::egui;
use egui::{Color32, FontFamily, FontId, Rounding, Stroke, TextStyle};

// ── palette ────────────────────────────────────────────────────────────
// Calibrated for macOS-class restraint: deep void background, gently warmed
// panels, hairlines that feel structural rather than drawn-on. Scarlet is
// reserved for state and identity, never decoration.

pub const VOID: Color32 = Color32::from_rgb(0x09, 0x09, 0x0c);
pub const PANEL: Color32 = Color32::from_rgb(0x10, 0x11, 0x14);
pub const ELEVATED: Color32 = Color32::from_rgb(0x18, 0x19, 0x1d);
pub const HOVER: Color32 = Color32::from_rgb(0x1f, 0x21, 0x26);
pub const HAIRLINE: Color32 = Color32::from_rgb(0x26, 0x28, 0x2d);
pub const HAIRLINE_FAINT: Color32 = Color32::from_rgb(0x18, 0x1a, 0x1e);

pub const SCARLET: Color32 = Color32::from_rgb(0xc4, 0x14, 0x28);
pub const SCARLET_BRIGHT: Color32 = Color32::from_rgb(0xe6, 0x3a, 0x4c);
pub const SCARLET_WASH: Color32 = Color32::from_rgb(0x32, 0x0c, 0x14);
pub const SCARLET_DEEP: Color32 = Color32::from_rgb(0x5a, 0x0e, 0x1c);

pub const BONE: Color32 = Color32::from_rgb(0xeb, 0xe7, 0xdb);
pub const BONE_DIM: Color32 = Color32::from_rgb(0xb0, 0xac, 0xa2);
pub const ASH: Color32 = Color32::from_rgb(0x6e, 0x6e, 0x76);
pub const ASH_DIM: Color32 = Color32::from_rgb(0x44, 0x46, 0x4e);

pub const SUCCESS: Color32 = Color32::from_rgb(0x6c, 0xb3, 0x7e);
pub const WARN: Color32 = Color32::from_rgb(0xd0, 0xa3, 0x4e);

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Typography — proportional primary (chrome, labels, file names),
    // monospace reserved for paths and byte counts. macOS-class hierarchy:
    // generous body size, restrained heading, tiny secondary captions.
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(12.5, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(10.5, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );

    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(BONE);
    v.window_fill = VOID;
    v.panel_fill = PANEL;
    v.extreme_bg_color = VOID;
    v.faint_bg_color = ELEVATED;
    v.code_bg_color = ELEVATED;
    v.window_stroke = Stroke::new(1.0, HAIRLINE);
    v.hyperlink_color = SCARLET_BRIGHT;

    v.selection.bg_fill = SCARLET_WASH;
    v.selection.stroke = Stroke::new(1.0, SCARLET);

    // Widgets — soft pill shapes, no hard borders by default. Hover lifts to
    // a slightly brighter fill, no border flash. Active is a quiet scarlet.
    let r = Rounding::same(5.0);

    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.weak_bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE_FAINT);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.noninteractive.rounding = r;

    v.widgets.inactive.bg_fill = ELEVATED;
    v.widgets.inactive.weak_bg_fill = ELEVATED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, HAIRLINE_FAINT);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, BONE_DIM);
    v.widgets.inactive.rounding = r;
    v.widgets.inactive.expansion = 0.0;

    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, HAIRLINE);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.hovered.rounding = r;
    v.widgets.hovered.expansion = 0.0;

    v.widgets.active.bg_fill = SCARLET_DEEP;
    v.widgets.active.weak_bg_fill = SCARLET_DEEP;
    v.widgets.active.bg_stroke = Stroke::new(1.0, SCARLET);
    v.widgets.active.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.active.rounding = r;
    v.widgets.active.expansion = 0.0;

    v.widgets.open.bg_fill = ELEVATED;
    v.widgets.open.bg_stroke = Stroke::new(1.0, SCARLET);
    v.widgets.open.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.open.rounding = r;

    v.menu_rounding = Rounding::same(6.0);
    v.window_rounding = Rounding::same(0.0);
    v.window_shadow.color = Color32::TRANSPARENT;
    v.popup_shadow.color = Color32::from_black_alpha(80);
    v.popup_shadow.offset = egui::vec2(0.0, 4.0);
    v.popup_shadow.blur = 16.0;
    v.popup_shadow.spread = 0.0;

    style.spacing.item_spacing = egui::vec2(10.0, 6.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(0.0);
    style.spacing.menu_margin = egui::Margin::same(6.0);
    style.spacing.indent = 16.0;
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.handle_min_length = 12.0;

    ctx.set_style(style);
}

pub fn hairline(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, HAIRLINE);
}

pub fn faint_line(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, HAIRLINE_FAINT);
}
