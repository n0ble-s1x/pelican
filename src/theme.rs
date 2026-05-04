//! Krypteia · Pelican visual theme.
//!
//! Aesthetic: **UNSC tactical** — gunmetal navy hull, single deep red
//! accent for state, HUD teal for success, amber for warning. Flat
//! surfaces, sharp 1-pixel hairlines, monospace data. Zero translucency
//! theatre — egui is a software rasteriser and fake glass reads as
//! pixelated grime. Lean into what the renderer does well: crisp fills,
//! tight typography, code-density information design.
//!
//! Two type families: **Geist Sans** for chrome/body, **Geist Mono** for
//! paths and byte counts. Both embedded once at startup so the look is
//! consistent across distros.

use eframe::egui;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

// ── palette ─────────────────────────────────────────────────────────────
// UNSC dropship: dark gunmetal navy hull, with a single saturated alert
// red used sparingly for primary action and identity. Teal echoes the
// Halo HUD; amber is the warning lamp. Foreground is a cool steel.

pub const HULL: Color32 = Color32::from_rgb(0x08, 0x0a, 0x0e); // canvas
pub const DECK: Color32 = Color32::from_rgb(0x0d, 0x10, 0x16); // pane backdrop
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x16, 0x1d); // cards, popovers
pub const ELEVATED: Color32 = Color32::from_rgb(0x18, 0x1d, 0x26); // chips, inputs
pub const HOVER: Color32 = Color32::from_rgb(0x21, 0x28, 0x33); // interactive lift
pub const HAIRLINE: Color32 = Color32::from_rgb(0x22, 0x29, 0x35); // structural rule
pub const HAIRLINE_FAINT: Color32 = Color32::from_rgb(0x16, 0x1b, 0x23); // whisper

// Accent — UNSC alert red. Deeper and more orange than the prior pink
// scarlet, so it reads as Pelican-thruster glow rather than candy.
pub const ACCENT: Color32 = Color32::from_rgb(0xd8, 0x3a, 0x2c);
pub const ACCENT_BRIGHT: Color32 = Color32::from_rgb(0xff, 0x5b, 0x46);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x4a, 0x16, 0x10);
pub const ACCENT_WASH: Color32 = Color32::from_rgb(0x2c, 0x10, 0x0d);

// Backwards-compatible aliases (callers still reference SCARLET_*).
pub const SCARLET: Color32 = ACCENT;
pub const SCARLET_BRIGHT: Color32 = ACCENT_BRIGHT;
pub const SCARLET_DEEP: Color32 = ACCENT_DEEP;
pub const SCARLET_WASH: Color32 = ACCENT_WASH;

// HUD status hues.
pub const TEAL: Color32 = Color32::from_rgb(0x5a, 0xb9, 0xc2); // success/info
pub const AMBER: Color32 = Color32::from_rgb(0xd4, 0xa9, 0x3a); // warning
pub const SUCCESS: Color32 = TEAL;
pub const WARN: Color32 = AMBER;

// Foreground — cool steel, three steps of dim.
pub const BONE: Color32 = Color32::from_rgb(0xdc, 0xe1, 0xea);
pub const BONE_DIM: Color32 = Color32::from_rgb(0x94, 0x9c, 0xab);
pub const ASH: Color32 = Color32::from_rgb(0x60, 0x67, 0x76);
pub const ASH_DIM: Color32 = Color32::from_rgb(0x3a, 0x40, 0x4d);

// ── radii ───────────────────────────────────────────────────────────────
pub const R_INPUT: CornerRadius = CornerRadius::same(5);
pub const R_BUTTON: CornerRadius = CornerRadius::same(6);
pub const R_CARD: CornerRadius = CornerRadius::same(8);

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut style = (*ctx.global_style()).clone();

    // Tight macOS-class type ramp. Geist's Medium weight reads as the
    // body voice; Bold is reserved for the wordmark + section headers.
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(12.5, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(10.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(11.5, FontFamily::Monospace),
    );

    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(BONE);
    v.window_fill = HULL;
    v.panel_fill = DECK;
    v.extreme_bg_color = HULL;
    v.faint_bg_color = ELEVATED;
    v.code_bg_color = ELEVATED;
    v.window_stroke = Stroke::new(1.0, HAIRLINE);
    v.hyperlink_color = ACCENT_BRIGHT;

    v.selection.bg_fill = ACCENT_WASH;
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    v.widgets.noninteractive.bg_fill = DECK;
    v.widgets.noninteractive.weak_bg_fill = DECK;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE_FAINT);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.noninteractive.corner_radius = R_INPUT;

    v.widgets.inactive.bg_fill = ELEVATED;
    v.widgets.inactive.weak_bg_fill = ELEVATED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, BONE_DIM);
    v.widgets.inactive.corner_radius = R_INPUT;
    v.widgets.inactive.expansion = 0.0;

    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, HAIRLINE);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.hovered.corner_radius = R_INPUT;
    v.widgets.hovered.expansion = 0.0;

    v.widgets.active.bg_fill = ACCENT_DEEP;
    v.widgets.active.weak_bg_fill = ACCENT_DEEP;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.active.corner_radius = R_INPUT;
    v.widgets.active.expansion = 0.0;

    v.widgets.open.bg_fill = ELEVATED;
    v.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.open.fg_stroke = Stroke::new(1.0, BONE);
    v.widgets.open.corner_radius = R_INPUT;

    v.menu_corner_radius = R_CARD;
    v.window_corner_radius = R_CARD;
    v.window_shadow.color = Color32::from_black_alpha(120);
    v.window_shadow.offset = [0, 8];
    v.window_shadow.blur = 24;
    v.window_shadow.spread = 0;
    v.popup_shadow.color = Color32::from_black_alpha(110);
    v.popup_shadow.offset = [0, 4];
    v.popup_shadow.blur = 18;
    v.popup_shadow.spread = 0;

    style.spacing.item_spacing = egui::vec2(10.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 16.0;
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.handle_min_length = 18.0;
    style.spacing.scroll.bar_inner_margin = 2.0;
    style.spacing.scroll.bar_outer_margin = 0.0;

    ctx.set_global_style(style);
}

/// Embed Geist Sans + Mono and make them the defaults. Replaces egui's
/// stock Ubuntu-Light, which reads as weak/generic against the HUD aesthetic.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "geist".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Geist-Regular.otf")).into(),
    );
    fonts.font_data.insert(
        "geist-medium".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Geist-Medium.otf")).into(),
    );
    fonts.font_data.insert(
        "geist-bold".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Geist-Bold.otf")).into(),
    );
    fonts.font_data.insert(
        "geist-mono".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/GeistMono-Regular.otf")).into(),
    );

    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "geist-medium".to_owned());
    prop.insert(1, "geist".to_owned());

    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.insert(0, "geist-mono".to_owned());

    let display = fonts
        .families
        .entry(FontFamily::Name("display".into()))
        .or_default();
    display.push("geist-bold".to_owned());
    display.push("geist".to_owned());

    ctx.set_fonts(fonts);
}

pub fn font_display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}

// ── primitives (flat, no theatre) ───────────────────────────────────────

/// Structural divider between major regions.
pub fn hairline(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, HAIRLINE);
}

/// Whisper divider used inside a panel between sections.
pub fn faint_line(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, HAIRLINE_FAINT);
}

// ── interactive widgets ─────────────────────────────────────────────────

/// Visual tier of an action button.
#[derive(Copy, Clone)]
pub enum BtnKind {
    /// Filled accent — single primary action per surface.
    Primary,
    /// Outlined accent — destructive secondary.
    Danger,
    /// Plain panel — tertiary.
    Quiet,
}

/// Flat button — filled rect, sharp 6-px squircle, no rim/shadow theatre.
/// The accent variant is the only place we paint a saturated colour, so it
/// reads as "this is the action" without needing a halo.
pub fn button(
    ui: &mut egui::Ui,
    label: &str,
    kind: BtnKind,
    enabled: bool,
    min_size: egui::Vec2,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(min_size, sense);
    let painter = ui.painter_at(rect);

    let pressed = response.is_pointer_button_down_on() && enabled;
    let hovered = response.hovered() && enabled;

    let (fill, stroke_col, text_col) = match (kind, enabled, pressed, hovered) {
        (_, false, _, _) => (PANEL, HAIRLINE_FAINT, ASH_DIM),

        (BtnKind::Primary, true, true, _) => (ACCENT_DEEP, ACCENT_BRIGHT, BONE),
        (BtnKind::Primary, true, false, true) => (ACCENT, ACCENT_BRIGHT, BONE),
        (BtnKind::Primary, true, false, false) => (ACCENT, ACCENT, BONE),

        (BtnKind::Danger, true, true, _) => (ACCENT_WASH, ACCENT, ACCENT_BRIGHT),
        (BtnKind::Danger, true, false, true) => (PANEL, ACCENT, ACCENT_BRIGHT),
        (BtnKind::Danger, true, false, false) => (HULL, HAIRLINE, ACCENT_BRIGHT),

        (BtnKind::Quiet, true, true, _) => (HOVER, HAIRLINE, BONE),
        (BtnKind::Quiet, true, false, true) => (ELEVATED, HAIRLINE, BONE),
        (BtnKind::Quiet, true, false, false) => (PANEL, HAIRLINE_FAINT, BONE_DIM),
    };

    painter.rect_filled(rect, R_BUTTON, fill);
    painter.rect_stroke(
        rect,
        R_BUTTON,
        Stroke::new(1.0, stroke_col),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::new(11.5, FontFamily::Name("display".into())),
        text_col,
    );

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Small chip — toolbar utility (Up / Home / Music / Clear). Hover-only
/// fill, no border at rest — sits like a system toolbar button.
pub fn chip(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let font = FontId::new(11.0, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), font.clone(), BONE_DIM);
    let pad = egui::vec2(9.0, 4.0);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);

    let hovered = response.hovered();
    let pressed = response.is_pointer_button_down_on();
    let (fill, text_col) = if pressed {
        (HOVER, BONE)
    } else if hovered {
        (ELEVATED, BONE)
    } else {
        (Color32::TRANSPARENT, BONE_DIM)
    };

    if fill != Color32::TRANSPARENT {
        painter.rect_filled(rect, R_INPUT, fill);
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        text_col,
    );

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Status line for the topbar: small filled dot + label. `animate` makes
/// the dot breathe (used during connect / busy).
pub fn status_indicator(ui: &mut egui::Ui, label: &str, accent: Color32, animate: bool) {
    let font = FontId::new(11.5, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), font.clone(), BONE_DIM);
    let dot_w = 8.0;
    let gap = 8.0;
    let size = egui::vec2(galley.size().x + dot_w + gap, galley.size().y);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let dot_color = if animate {
        let t = ui.ctx().input(|i| i.time);
        let pulse = ((t * std::f64::consts::TAU / 1.6).sin() * 0.5 + 0.5) as f32;
        let factor = 0.55 + pulse * 0.45;
        accent.linear_multiply(factor)
    } else {
        accent
    };
    let dot_center = rect.left_center() + egui::vec2(dot_w * 0.5, 0.0);
    painter.circle_filled(dot_center, 5.0, accent.linear_multiply(0.18));
    painter.circle_filled(dot_center, 3.5, dot_color);

    painter.text(
        rect.left_center() + egui::vec2(dot_w + gap, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        BONE_DIM,
    );

    if animate {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
    }
}

/// Thin progress bar with rounded ends. `fill` is 0..1; `None` → marquee.
pub fn progress_bar(ui: &mut egui::Ui, width: f32, fill: Option<f32>, accent: Color32, time: f64) {
    let h = 3.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let painter = ui.painter();
    let r = CornerRadius::same(2);
    painter.rect_filled(rect, r, ELEVATED);

    let fill_rect = match fill {
        Some(f) => {
            let f = f.clamp(0.0, 1.0);
            let w = (f * width).max(2.0);
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(w, h))
        }
        None => {
            let cycle = 1.6_f64;
            let phase = ((time % cycle) / cycle) as f32;
            let slug_w = (width * 0.22).max(36.0);
            let max_x = (width - slug_w).max(0.0);
            let tri = if phase < 0.5 {
                phase * 2.0
            } else {
                (1.0 - phase) * 2.0
            };
            let x = tri * max_x;
            egui::Rect::from_min_size(rect.left_top() + egui::vec2(x, 0.0), egui::vec2(slug_w, h))
        }
    };
    painter.rect_filled(fill_rect, r, accent);
}

/// Section header — small accent bar + uppercase label in display weight.
pub fn section_header(ui: &mut egui::Ui, label: &str, accent: Color32) {
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(2.0, 11.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, accent);
        ui.add_space(9.0);
        ui.label(
            egui::RichText::new(label)
                .color(BONE)
                .family(FontFamily::Name("display".into()))
                .size(10.5)
                .extra_letter_spacing(2.4),
        );
    });
}
