//! Small tools: the ASCII table, the base converter and the About window.

use egui::{RichText, Ui};

/// The table of characters (code page 437).
pub(crate) fn ascii_ui(ui: &mut Ui) {
    egui::Grid::new("ascii").num_columns(16).spacing([4.0, 2.0]).show(ui, |ui| {
        for row in 0..16u8 {
            for col in 0..16u8 {
                let b = row * 16 + col;
                let c = rustemu_core::font::cp437_to_char(b);
                ui.label(RichText::new(c.to_string()).monospace().size(16.0))
                    .on_hover_text(format!("{b} = {b:02X}h = {b:08b}b"));
            }
            ui.end_row();
        }
    });
    ui.weak("hover a character to see its code (code page 437)");
}

/// Shows a number in all bases (Math menu).
pub(crate) fn converter_ui(text: &mut String, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("value:");
        ui.add(egui::TextEdit::singleline(text).hint_text("number"));
    });
    ui.weak("examples: 255, 0FFh, 0x1F, 1010b, 17o, -5, 'A'");
    let t = text.trim();
    let v = if t.len() == 3 && t.starts_with('\'') && t.ends_with('\'') {
        Some(t.as_bytes()[1] as i64)
    } else if let Some(n) = t.strip_prefix('-') {
        rustemu_asm::lexer::parse_number(n).map(|v| -v)
    } else {
        rustemu_asm::lexer::parse_number(t)
    };
    match v {
        Some(v) => {
            let u16v = v as u16;
            egui::Grid::new("conv").num_columns(2).show(ui, |ui| {
                let row = |ui: &mut Ui, k: &str, s: String| {
                    ui.label(k);
                    ui.label(RichText::new(s).monospace());
                    ui.end_row();
                };
                row(ui, "decimal (unsigned 16)", u16v.to_string());
                row(ui, "decimal (signed 16)", (u16v as i16).to_string());
                row(ui, "decimal (signed 8)", (u16v as u8 as i8).to_string());
                row(ui, "hex", format!("{u16v:04X}h"));
                row(ui, "binary", format!("{:08b} {:08b}b", u16v >> 8, u16v & 0xFF));
                row(ui, "octal", format!("{u16v:o}o"));
                let c = rustemu_core::font::cp437_to_char(u16v as u8);
                row(ui, "char (low byte)", c.to_string());
            });
        }
        None if t.is_empty() => {}
        None => {
            ui.colored_label(crate::colors::error(ui), "not a number");
        }
    }
}

/// A list item whose wrapped lines stay to the right of the bullet.
pub fn bullet(ui: &mut Ui, text: &str) {
    ui.horizontal_top(|ui| {
        ui.label("•");
        ui.add(egui::Label::new(text).wrap());
    });
}

/// Where this build runs.
fn platform_name() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web version"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

pub(crate) fn about_ui(ui: &mut Ui) {
    ui.set_max_width(440.0);
    ui.heading("RustEmu8086");
    ui.label(format!("version {} · {}", env!("CARGO_PKG_VERSION"), platform_name()));
    ui.add_space(6.0);
    ui.label("8086 assembler, emulator and debugger, compatible with Emu8086 programs.");
    ui.add_space(4.0);
    for line in [
        "Emu8086 (MASM) and NASM syntax, COM / EXE / BIN / boot sector output",
        "source-level debugger: breakpoints with conditions, watch, call stack, step back",
        "BIOS and DOS services, a virtual drive, floppies and the Emu8086 virtual devices",
        "the desktop layout of Emu8086 and a touch layout for tablets",
        "the CPU is checked against 2.9 million tests recorded on a real 8088",
    ] {
        bullet(ui, line);
    }
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        for (name, url) in [
            ("source code (GitHub)", crate::app::REPO_URL),
            ("web version", crate::app::WEB_URL),
            ("releases", crate::app::RELEASES_URL),
        ] {
            if ui.link(name).on_hover_text(url).clicked() {
                crate::platform::open_url(url);
            }
        }
    });
    ui.add_space(8.0);
    ui.label("MIT License · © 2026 Oleg Durandin");
    ui.weak(
        "Written in Rust with egui. RustEmu8086 is an independent program, not affiliated with the authors of Emu8086.",
    );
}
