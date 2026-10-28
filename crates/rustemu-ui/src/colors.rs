//! Accent colours that stay readable in both light and dark themes.

use egui::{Color32, Ui};

fn pick(ui: &Ui, dark: Color32, light: Color32) -> Color32 {
    if ui.visuals().dark_mode { dark } else { light }
}

/// Warnings, "waiting", the stack pointer.
pub fn warn(ui: &Ui) -> Color32 {
    pick(ui, Color32::from_rgb(230, 180, 60), Color32::from_rgb(170, 100, 0))
}

/// Success / "condition true".
pub fn ok(ui: &Ui) -> Color32 {
    pick(ui, Color32::from_rgb(90, 200, 90), Color32::from_rgb(20, 130, 30))
}

/// Errors.
pub fn error(ui: &Ui) -> Color32 {
    pick(ui, Color32::from_rgb(240, 90, 90), Color32::from_rgb(200, 30, 30))
}

/// Links and the instruction pointer marker.
pub fn info(ui: &Ui) -> Color32 {
    pick(ui, Color32::from_rgb(90, 160, 255), Color32::from_rgb(20, 90, 200))
}
