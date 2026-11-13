//! Portable MARS assembly and execution settings.
use rustemu_mips::{MemoryConfig, Settings};
use serde::{Deserialize, Serialize};

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
