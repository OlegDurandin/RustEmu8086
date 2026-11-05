//! MARS's settings in the MIPS workspace (its Settings menu): delayed branching, self-modifying
//! code, start at `main`, pseudo-instructions, warnings as errors, the memory configuration,
//! program arguments, an exception handler file and assembling all files of the folder (the
//! last two read files from disk: not in the browser).
//! They are saved, used by the next assembly ([`MarsSettings::mips`]) and recorded in the
//! program it makes: a loaded program keeps running with its own until it is assembled again.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use rustemu_mips::{MemoryConfig, Settings};

use crate::app::App;

/// MARS's settings (saved), by MARS's defaults.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct MarsSettings {
    /// Delayed branching.
    pub delayed_branching: bool,
    /// Self-modifying code.
    pub self_modifying: bool,
    /// Start at the global label `main`.
    pub start_at_main: bool,
    /// Pseudo-instructions and extended formats are allowed.
    pub pseudo_instructions: bool,
    /// Assembler warnings are errors.
    pub warnings_are_errors: bool,
    /// The memory configuration: MARS's identifier ([`MemoryConfig::id`]).
    pub memory: String,
    /// Program arguments are given to the program.
    pub program_arguments_enabled: bool,
    /// The program arguments, separated by spaces (MARS's field above its text segment).
    pub program_arguments: String,
    /// The `.asm` and `.s` files of the program's folder are assembled with it.
    pub assemble_all: bool,
    /// The exception handler file is assembled with every program.
    pub exception_handler_enabled: bool,
    /// The path of the exception handler file.
    pub exception_handler: String,
    /// The reading services take their input from a dialog window.
    pub popup_input: bool,
}

impl Default for MarsSettings {
    fn default() -> Self {
        MarsSettings::from(&Settings::default())
    }
}

impl From<&Settings> for MarsSettings {
    fn from(s: &Settings) -> Self {
        MarsSettings {
            delayed_branching: s.delayed_branching,
            self_modifying: s.self_modifying,
            start_at_main: s.start_at_main,
            pseudo_instructions: s.pseudo_instructions,
            warnings_are_errors: s.warnings_are_errors,
            memory: s.memory.id().to_string(),
            program_arguments_enabled: false,
            program_arguments: String::new(),
            assemble_all: false,
            exception_handler_enabled: false,
            exception_handler: String::new(),
            popup_input: s.popup_input,
        }
    }
}

impl MarsSettings {
    /// The settings for the assembler (an unknown memory configuration is the default one).
    pub fn mips(&self) -> Settings {
        Settings {
            delayed_branching: self.delayed_branching,
            self_modifying: self.self_modifying,
            start_at_main: self.start_at_main,
            pseudo_instructions: self.pseudo_instructions,
            warnings_are_errors: self.warnings_are_errors,
            memory: MemoryConfig::from_id(&self.memory).unwrap_or_default(),
            popup_input: self.popup_input,
        }
    }

    /// The program arguments (split at spaces, as MARS splits its field), none when off.
    pub fn arguments(&self) -> Vec<String> {
        if !self.program_arguments_enabled {
            return Vec::new();
        }
        self.program_arguments.split_whitespace().map(str::to_string).collect()
    }
}

/// A menu of the settings that stays open while they are clicked (several are usually changed
/// together).
pub fn menu_config() -> egui::containers::menu::MenuConfig {
    egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

/// The loaded MIPS program was assembled with other settings than the current ones.
pub fn changed_since_load(app: &App) -> bool {
    let s = &app.settings.mars;
    app.session.mips().is_some_and(|m| m.loaded && (m.settings != s.mips() || m.program_arguments != s.arguments()))
}

/// The settings as checkboxes and the memory configurations, with MARS's names (a menu, the
/// options window). A change while a program is loaded says how to apply it.
pub fn settings_ui(app: &mut App, ui: &mut Ui) {
    let before = app.settings.mars.clone();
    let s = &mut app.settings.mars;
    ui.checkbox(&mut s.delayed_branching, "Delayed branching").on_hover_text(
        "The instruction after a branch or jump (its delay slot) executes before the branch is taken, as on a \
         real MIPS. Pseudo-instructions get the nops of their delay slots.",
    );
    ui.checkbox(&mut s.self_modifying, "Self-modifying code").on_hover_text(
        "The program may load and store in the text segment (a store makes an instruction) and run code from \
         the data segment.",
    );
    ui.checkbox(&mut s.start_at_main, "Initialize Program Counter to global 'main' if defined")
        .on_hover_text("Execution starts at main when the program declares it with .globl main.");
    ui.checkbox(&mut s.pseudo_instructions, "Permit extended (pseudo) instructions and formats")
        .on_hover_text("Off: li, la, move, blt, lw label… are errors; only machine instructions assemble.");
    ui.checkbox(&mut s.warnings_are_errors, "Assembler warnings are considered errors");
    ui.checkbox(&mut s.popup_input, "Popup dialog for input syscalls (5,6,7,8,12)")
        .on_hover_text("The reading services ask in a window instead of the Run I/O console.");
    ui.checkbox(&mut s.program_arguments_enabled, "Program arguments provided to MIPS program").on_hover_text(
        "The arguments are stored under the stack when the program is loaded: $a0 = their number, $a1 = the \
         address of an array of their addresses.",
    );
    if s.program_arguments_enabled {
        let l = ui.label("arguments:");
        ui.add(
            egui::TextEdit::singleline(&mut s.program_arguments).hint_text("separated by spaces").desired_width(300.0),
        )
        .labelled_by(l.id);
    }
    // These read files of the computer.
    if !app.front.browser {
        let s = &mut app.settings.mars;
        ui.checkbox(&mut s.assemble_all, "Assemble all files in directory")
            .on_hover_text("The .asm and .s files of the program's folder are assembled with it; labels declared .globl are seen by all.");
        ui.checkbox(
            &mut s.exception_handler_enabled,
            "Exception handler: include this file in all assemble operations",
        )
        .on_hover_text(
            "The file is assembled before the program (its .ktext code at 0x80000180 handles the exceptions).",
        );
        if s.exception_handler_enabled {
            ui.horizontal(|ui| {
                let l = ui.label("file:");
                ui.add(egui::TextEdit::singleline(&mut s.exception_handler).desired_width(240.0)).labelled_by(l.id);
                if ui.button("choose…").clicked()
                    && let Some(p) = crate::platform::pick_file(&[("MIPS source", &["asm", "s"])], None)
                {
                    s.exception_handler = p.display().to_string();
                }
            });
        }
    }
    let s = &mut app.settings.mars;
    ui.label(RichText::new("memory configuration").weak());
    for c in MemoryConfig::ALL {
        let l = c.layout();
        let hover = format!(
            ".text 0x{:08x}, .data 0x{:08x}, $sp 0x{:08x}, MMIO 0x{:08x}",
            l.text_base, l.data_base, l.sp, l.mmio_base
        );
        if ui.radio(s.memory.eq_ignore_ascii_case(c.id()), c.title()).on_hover_text(hover).clicked() {
            s.memory = c.id().to_string();
        }
    }
    if app.settings.mars != before && changed_since_load(app) {
        app.status = "MARS settings changed: emulate (F9) assembles the program with them".into();
    }
}

/// Warning shown while the loaded program was assembled with other settings, with a button
/// to assemble and load it again.
pub fn changed_banner(app: &mut App, ui: &mut Ui) {
    if !changed_since_load(app) {
        return;
    }
    egui::Frame::new().fill(ui.visuals().warn_fg_color.gamma_multiply(0.18)).inner_margin(4.0).corner_radius(3.0).show(
        ui,
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(
                        "⚠ MARS settings were changed after the program was loaded - it runs with the old ones.",
                    )
                    .color(crate::colors::warn(ui)),
                );
                if ui.button("Reassemble and load (F9)").clicked() {
                    app.emulate();
                }
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_settings_are_mars_settings() {
        assert_eq!(MarsSettings::default().mips(), Settings::default());
        let s = Settings {
            delayed_branching: true,
            self_modifying: true,
            memory: MemoryConfig::CompactTextAtZero,
            ..Settings::default()
        };
        assert_eq!(MarsSettings::from(&s).mips(), s);
        let unknown = MarsSettings { memory: "nonsense".into(), ..MarsSettings::default() };
        assert_eq!(unknown.mips().memory, MemoryConfig::Default);
    }
}
