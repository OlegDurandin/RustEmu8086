//! Font setup: egui defaults plus macOS system fonts as fallbacks for CP437 symbols. In the
//! browser egui's emoji fonts are replaced by subsets with the symbols of the UI (the fonts are
//! downloaded with the app): `fonts/*-subset.ttf`, made by `scripts/subset_emoji_fonts.py`.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// System fonts tried as fallbacks (path, face index inside a collection).
const FALLBACKS: &[(&str, &str, u32)] =
    &[("menlo", "/System/Library/Fonts/Menlo.ttc", 0), ("apple-symbols", "/System/Library/Fonts/Apple Symbols.ttf", 0)];

/// The emoji fonts of egui (names and sizes as in `FontDefinitions::default`), cut down.
#[cfg(any(target_arch = "wasm32", test))]
const EMOJI_SUBSETS: &[(&str, &[u8], f32)] = &[
    ("NotoEmoji-Regular", include_bytes!("../fonts/NotoEmoji-Regular-subset.ttf"), 0.81),
    ("emoji-icon-font", include_bytes!("../fonts/emoji-icon-font-subset.ttf"), 0.90),
];

/// The fonts of the web version: egui's text fonts and the emoji subsets.
#[cfg(any(target_arch = "wasm32", test))]
fn web_definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    for (name, bytes, scale) in EMOJI_SUBSETS {
        let tweak = egui::FontTweak { scale: *scale, ..Default::default() };
        defs.font_data.insert((*name).to_string(), Arc::new(FontData::from_static(bytes).tweak(tweak)));
        for fam in [FontFamily::Monospace, FontFamily::Proportional] {
            let list = defs.families.entry(fam).or_default();
            if !list.iter().any(|n| n == name) {
                list.push((*name).to_string());
            }
        }
    }
    defs
}

pub fn install(ctx: &egui::Context) {
    #[cfg(target_arch = "wasm32")]
    let mut defs = web_definitions();
    #[cfg(not(target_arch = "wasm32"))]
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

#[cfg(test)]
mod tests {
    use egui::FontId;
    use egui::epaint::text::{Fonts, TextOptions};

    /// Non-ASCII characters of the sources of the app.
    fn ui_chars() -> std::collections::BTreeSet<char> {
        fn walk(dir: &std::path::Path, chars: &mut std::collections::BTreeSet<char>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, chars);
                } else if path.extension().is_some_and(|e| e == "rs" || e == "asm" || e == "inc") {
                    chars.extend(std::fs::read_to_string(&path).unwrap_or_default().chars().filter(|c| !c.is_ascii()));
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut chars = Default::default();
        for krate in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
            walk(&krate.path().join("src"), &mut chars);
        }
        walk(&root.join("examples"), &mut chars);
        walk(&root.join("crates/rustemu-asm/inc"), &mut chars);
        chars
    }

    /// Every symbol of the UI that the full fonts have is in the web fonts too (otherwise run
    /// `scripts/subset_emoji_fonts.py`).
    #[test]
    fn web_fonts_cover_the_ui() {
        let mut full = Fonts::new(TextOptions::default(), egui::FontDefinitions::default());
        let mut web = Fonts::new(TextOptions::default(), super::web_definitions());
        let mut missing = String::new();
        for c in ui_chars() {
            for id in [FontId::monospace(14.0), FontId::proportional(14.0)] {
                if full.has_glyph(&id, c) && !web.has_glyph(&id, c) && !missing.contains(c) {
                    missing.push(c);
                }
            }
        }
        assert!(missing.is_empty(), "not in the web fonts: {missing}");
        // A symbol the UI does not use (a rainbow; not written here, the test reads this file).
        let unused = char::from_u32(0x1F308).unwrap();
        let id = FontId::proportional(14.0);
        assert!(full.has_glyph(&id, unused) && !web.has_glyph(&id, unused));
    }
}
