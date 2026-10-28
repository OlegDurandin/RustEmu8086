//! Font setup: egui defaults plus macOS system fonts as fallbacks for CP437 symbols.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// System fonts tried as fallbacks (path, face index inside a collection).
const FALLBACKS: &[(&str, &str, u32)] =
    &[("menlo", "/System/Library/Fonts/Menlo.ttc", 0), ("apple-symbols", "/System/Library/Fonts/Apple Symbols.ttf", 0)];

pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    for (name, path, index) in FALLBACKS {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let mut data = FontData::from_owned(bytes);
        data.index = *index;
        defs.font_data.insert((*name).to_string(), Arc::new(data));
        for fam in [FontFamily::Monospace, FontFamily::Proportional] {
            defs.families.entry(fam).or_default().push((*name).to_string());
        }
    }
    ctx.set_fonts(defs);
}
