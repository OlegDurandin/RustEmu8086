//! Colours, visuals and touch-sized spacing of the tablet theme (always dark).

use egui::{Stroke, TextStyle, vec2};

/// Colours of the tablet theme.
pub(crate) mod pal {
    use egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(0x0B, 0x12, 0x20);
    pub const BAR: Color32 = Color32::from_rgb(0x0E, 0x16, 0x27);
    pub const CARD: Color32 = Color32::from_rgb(0x10, 0x19, 0x2B);
    pub const HEADER: Color32 = Color32::from_rgb(0x17, 0x22, 0x38);
    pub const BUTTON: Color32 = Color32::from_rgb(0x1C, 0x29, 0x42);
    pub const FIELD: Color32 = Color32::from_rgb(0x0A, 0x11, 0x1E);
    pub const BORDER: Color32 = Color32::from_rgb(0x26, 0x35, 0x52);
    pub const TEXT: Color32 = Color32::from_rgb(0xD8, 0xE0, 0xEC);
    pub const WEAK: Color32 = Color32::from_rgb(0x7D, 0x8C, 0xA6);
    pub const BLUE: Color32 = Color32::from_rgb(0x2F, 0x6F, 0xD6);
    pub const LINK: Color32 = Color32::from_rgb(0x5E, 0xA2, 0xF0);
    pub const TEAL: Color32 = Color32::from_rgb(0x12, 0xA8, 0x9A);
    pub const AMBER: Color32 = Color32::from_rgb(0xF2, 0xB6, 0x30);
    pub const AMBER_BG: Color32 = Color32::from_rgb(0x4A, 0x3C, 0x16);
    pub const RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
    pub const LABEL: Color32 = Color32::from_rgb(0xE6, 0xC0, 0x6A);
}

/// Visuals of the tablet theme (always dark).
pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = pal::BG;
    v.window_fill = pal::CARD;
    v.window_stroke = Stroke::new(1.0, pal::BORDER);
    v.extreme_bg_color = pal::FIELD;
    v.faint_bg_color = pal::HEADER;
    v.code_bg_color = pal::FIELD;
    v.hyperlink_color = pal::LINK;
    v.selection.bg_fill = pal::BLUE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal::BORDER);
    v.widgets.noninteractive.fg_stroke.color = pal::TEXT;
    v.widgets.inactive.weak_bg_fill = pal::BUTTON;
    v.widgets.inactive.bg_fill = pal::BUTTON;
    v.widgets.inactive.fg_stroke.color = pal::TEXT;
    v.widgets.hovered.weak_bg_fill = pal::BORDER;
    v.widgets.hovered.bg_fill = pal::BORDER;
    v
}

/// Larger controls for touch.
pub fn style(s: &mut egui::Style) {
    s.spacing.button_padding = vec2(10.0, 6.0);
    s.spacing.interact_size.y = 28.0;
    s.spacing.item_spacing = vec2(8.0, 6.0);
    for (ts, size) in [(TextStyle::Body, 15.0), (TextStyle::Button, 15.0), (TextStyle::Monospace, 14.5)] {
        if let Some(f) = s.text_styles.get_mut(&ts) {
            f.size = size;
        }
    }
}

/// Installs the theme (`App::fixed_theme`: the theme setting does not apply to the tablet layout).
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.style_mut_of(egui::Theme::Dark, style);
    ctx.set_theme(egui::ThemePreference::Dark);
}
