//! The tools: every window of the interface except the dialogs. Everything a tool needs — its name
//! in the settings, tab title, group of the panels layout, window and body — is here, so adding one
//! means adding a variant and filling the matches below.
//!
//! [`Windows`] says which tools are open; it is the only such state. In the panels layout the tools
//! with a group are tabs ([`crate::workspace`]); the others, and all of them in the classic layout
//! and on the tablet, are floating windows drawn by [`windows`].

use egui::{Ui, pos2};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::workspace::Group;

/// A tool that can be a tab of the workspace or a floating window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    /// The source editor (always shown).
    Editor,
    /// Registers, disassembly and the run controls (the desktop's own body).
    Emulator,
    /// Hex dump of memory.
    Memory,
    /// The stack from SS:SP.
    Stack,
    /// The program's variables and constants.
    Vars,
    /// Watch expressions.
    Watch,
    /// The last ALU operation.
    Alu,
    /// The 8087 registers.
    Fpu,
    /// The emulator screen.
    Screen,
    /// The virtual devices of Emu8086.
    Devices,
    /// Files of drive C: in memory (the browser).
    Drive,
    /// The source with the current line (the desktop's own body).
    Source,
    /// The listing.
    Listing,
    /// The symbol table.
    Symbols,
    /// The emu8086.inc library.
    Library,
    /// The reference.
    Reference,
    /// Breakpoints with conditions.
    Breakpoints,
    /// The call stack.
    CallStack,
    /// The DEBUG.EXE-style console and the log of every stop.
    Console,
    /// The emulator's log.
    Log,
    /// The flags.
    Flags,
    /// The lexical flag analyser (which conditional jumps are taken).
    FlagAnalyser,
    /// Stop on condition.
    StopCondition,
    /// The extended value viewer.
    ValueViewer,
    /// The ASCII table.
    Ascii,
    /// The base converter.
    Converter,
    /// About.
    About,
    /// The multi-base calculator.
    Calculator,
    /// "My programs" saved in the browser.
    Programs,
}

/// What a tool is, apart from its window and body.
struct Spec {
    /// Name in the saved settings (`None`: not saved, closed at start).
    key: Option<&'static str>,
    /// Title of its tab in the panels layout.
    title: &'static str,
    /// Its tabs in the panels layout (`None`: a floating window in both layouts, as in Emu8086).
    group: Option<Group>,
    /// The body scrolls by itself (otherwise its tab gets a scroll area).
    scrolls: bool,
    /// A tool of the loaded program: it opens with the emulator (see [`Windows::take_program_tools`]).
    program: bool,
}

impl Tool {
    /// Number of tools (the size of [`Windows`]).
    pub const COUNT: usize = 29;

    /// All tools; panels are placed in this order by the default layout, windows drawn in it.
    pub const ALL: [Tool; Tool::COUNT] = [
        Tool::Editor,
        Tool::Source,
        Tool::Listing,
        Tool::Symbols,
        Tool::Library,
        Tool::Reference,
        Tool::Emulator,
        Tool::Memory,
        Tool::Watch,
        Tool::Alu,
        Tool::Fpu,
        Tool::Screen,
        Tool::Vars,
        Tool::Stack,
        Tool::Console,
        Tool::Log,
        Tool::Devices,
        Tool::Drive,
        Tool::Breakpoints,
        Tool::CallStack,
        // Floating in both layouts. The flags come before their analyser: "analyse" opens it.
        Tool::Flags,
        Tool::FlagAnalyser,
        Tool::StopCondition,
        Tool::ValueViewer,
        Tool::Programs,
        Tool::Ascii,
        Tool::Converter,
        Tool::Calculator,
        Tool::About,
    ];

    fn spec(self) -> Spec {
        use Group::*;
        // (saved as, tab title, group, scrolls by itself, opens with the program)
        let (key, title, group, scrolls, program) = match self {
            Tool::Editor => (None, "editor", Some(Text), true, false),
            Tool::Source => (Some("source"), "source", Some(Text), true, true),
            Tool::Listing => (Some("listing"), "listing", Some(Text), true, true),
            Tool::Symbols => (Some("symbols"), "symbol table", Some(Text), true, true),
            Tool::Library => (None, "emu8086.inc", Some(Text), true, false),
            Tool::Reference => (None, "reference", Some(Text), true, false),
            Tool::Emulator => (Some("emulator"), "emulator", Some(Emulator), false, true),
            Tool::Memory => (Some("memory"), "memory", Some(Inspect), true, true),
            Tool::Watch => (Some("watch"), "watch", Some(Inspect), false, true),
            Tool::Alu => (Some("alu"), "ALU", Some(Inspect), false, true),
            Tool::Fpu => (Some("fpu"), "FPU", Some(Inspect), false, true),
            Tool::Screen => (Some("screen"), "screen", Some(Screen), true, true),
            Tool::Vars => (Some("vars"), "variables", Some(Aside), true, true),
            Tool::Stack => (Some("stack"), "stack", Some(Aside), true, true),
            Tool::Console => (Some("console"), "debug log", Some(Aside), true, true),
            Tool::Log => (Some("log"), "log", Some(Aside), true, true),
            Tool::Devices => (Some("devices"), "virtual devices", Some(Aside), false, true),
            Tool::Drive => (Some("drive"), "drive C:", Some(Aside), true, false),
            Tool::Breakpoints => (Some("breakpoints"), "breakpoints", Some(Debug), false, true),
            Tool::CallStack => (Some("call_stack"), "call stack", Some(Debug), false, true),
            Tool::Flags => (Some("flags"), "flags", None, false, true),
            Tool::FlagAnalyser => (Some("flag_analyser"), "lexical flag analyser", None, false, true),
            Tool::StopCondition => (Some("stop_condition"), "stop on condition", None, false, true),
            Tool::ValueViewer => (Some("value_viewer"), "extended value viewer", None, false, true),
            Tool::Programs => (Some("programs"), "my programs", None, false, false),
            Tool::Ascii => (Some("ascii"), "ASCII codes", None, false, false),
            Tool::Converter => (Some("converter"), "base converter", None, false, false),
            Tool::Calculator => (None, "multi base calculator", None, false, false),
            Tool::About => (None, "About", None, false, false),
        };
        Spec { key, title, group, scrolls, program }
    }

    /// Title of the tool's tab in the panels layout.
    pub fn title(self) -> &'static str {
        self.spec().title
    }

    /// Its tabs in the panels layout (`None`: always a floating window).
    pub(crate) fn group(self) -> Option<Group> {
        self.spec().group
    }

    pub(crate) fn scrolls(self) -> bool {
        self.spec().scrolls
    }

    /// The tool can be shown here: the drive C: in memory exists, the programs are in the browser.
    fn available(self, app: &App) -> bool {
        match self {
            Tool::Drive => app.session.dos_memory_drive().is_some(),
            Tool::Programs => app.front.browser,
            _ => true,
        }
    }

    /// The tool's request to be brought to the front (set when it shows an article or an entry).
    pub(crate) fn raise(self, app: &mut App) -> Option<&mut bool> {
        match self {
            Tool::Reference => Some(&mut app.reference.raise),
            Tool::Library => Some(&mut app.library.raise),
            _ => None,
        }
    }

    /// Its floating window: title, where it first appears and its size.
    fn window<'a>(self, app: &App) -> egui::Window<'a> {
        let at = |title: &str, x: f32, y: f32| egui::Window::new(title.to_owned()).default_pos(pos2(x, y));
        // Some windows keep an id of their own: their title changes, or egui remembers them by it.
        let unique = |w: egui::Window<'a>, id: &str| w.id(egui::Id::unique(id));
        match self {
            Tool::Emulator => unique(at("emulator", 8.0, 64.0), "emulator_window").default_width(660.0),
            Tool::Screen => unique(at(&crate::screen::title(app), 700.0, 64.0), "screen_window")
                // The screen fills the window: drag its edge to make it larger or smaller.
                .default_size([660.0, 440.0])
                .min_size([240.0, 160.0]),
            Tool::Source => {
                unique(at("original source code", 700.0, 540.0), "source_window").default_size([700.0, 300.0])
            }
            Tool::Memory => at("memory", 80.0, 420.0),
            Tool::Stack => at("stack", 680.0, 110.0),
            Tool::Vars => at("variables", 300.0, 300.0),
            Tool::Symbols => at("symbol table", 320.0, 160.0),
            Tool::Listing => at("listing", 200.0, 140.0),
            Tool::Log => at("log", 120.0, 500.0),
            Tool::Flags => at("flags", 880.0, 110.0).resizable(false).default_width(140.0),
            Tool::FlagAnalyser => at("lexical flag analyser", 1000.0, 110.0),
            Tool::Breakpoints => at("breakpoints", 700.0, 560.0).default_width(560.0),
            Tool::Watch => at("watch", 1060.0, 110.0).default_width(460.0),
            Tool::CallStack => at("call stack", 1060.0, 360.0).default_width(520.0),
            Tool::Alu => at("ALU", 240.0, 600.0).default_width(520.0),
            Tool::Fpu => at("FPU", 760.0, 380.0).default_width(520.0),
            Tool::StopCondition => at("stop on condition", 700.0, 420.0).default_width(460.0),
            Tool::ValueViewer => at("extended value viewer", 420.0, 200.0).resizable(false),
            Tool::Devices => unique(at("virtual devices", 760.0, 120.0), "devices_window").resizable(false),
            Tool::Drive => at("drive C:", 520.0, 140.0).default_size([480.0, 380.0]),
            Tool::Console => at("debug log", 680.0, 470.0).default_size([620.0, 360.0]),
            Tool::Library => at("emu8086.inc library", 160.0, 100.0).default_size([860.0, 560.0]),
            Tool::Reference => at("reference", 140.0, 80.0).default_size([880.0, 580.0]),
            Tool::Programs => at("my programs", 480.0, 120.0).default_size([460.0, 360.0]),
            Tool::Ascii => at("ASCII codes", 160.0, 100.0).default_width(520.0),
            Tool::Converter => egui::Window::new("base converter"),
            Tool::Calculator => at("multi base calculator", 260.0, 110.0).default_width(460.0),
            Tool::About => egui::Window::new("About")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]),
            Tool::Editor => unreachable!("the editor is never a window"),
        }
    }
}

/// What a front end adds to the shared tools.
pub trait Host {
    /// Draws the body of `tool` if this front end has its own (the desktop: the emulator and the
    /// source); returns false for the others.
    fn tool_ui(&mut self, _app: &mut App, _ui: &mut Ui, _tool: Tool) -> bool {
        false
    }

    /// `tool` is a floating window of this front end (the tablet shows the screen in its layout).
    fn floats(&self, _tool: Tool) -> bool {
        true
    }
}

/// A front end without tools of its own.
pub struct NoHost;

impl Host for NoHost {}

/// The body of a tool.
pub fn tool_ui(app: &mut App, ui: &mut Ui, tool: Tool, host: &mut dyn Host) {
    if host.tool_ui(app, ui, tool) {
        return;
    }
    match tool {
        Tool::Editor => crate::editor::editor(app, ui),
        Tool::Screen => crate::screen::screen_ui(app, ui),
        Tool::Memory => crate::inspect::memory_ui(app, ui),
        Tool::Stack => crate::inspect::stack_ui(app, ui),
        Tool::Flags => crate::inspect::flags_ui(app, ui),
        Tool::FlagAnalyser => crate::inspect::flag_analyser_ui(app, ui),
        Tool::Vars => crate::inspect::vars_ui(app, ui),
        Tool::Symbols => crate::inspect::symbols_ui(app, ui),
        Tool::Listing => crate::inspect::listing_ui(app, ui),
        Tool::Log => crate::inspect::log_ui(app, ui),
        Tool::Watch => crate::debugger::watch_ui(app, ui),
        Tool::Breakpoints => crate::debugger::breakpoints_ui(app, ui),
        Tool::StopCondition => crate::debugger::stop_condition_ui(app, ui),
        Tool::CallStack => crate::debugger::call_stack_ui(app, ui),
        Tool::ValueViewer => crate::debugger::value_viewer_ui(app, ui),
        Tool::Alu => crate::units::alu_ui(app, ui),
        Tool::Fpu => crate::units::fpu_ui(app, ui),
        Tool::Devices => crate::devices_ui::devices_ui(app, ui),
        Tool::Drive => crate::drive_ui::drive_ui(app, ui),
        Tool::Console => crate::console::console_ui(app, ui),
        Tool::Library => crate::library_ui::library_ui(app, ui),
        Tool::Reference => crate::reference_ui::reference_ui(&mut app.reference, ui),
        Tool::Programs => crate::programs::programs_ui(app, ui),
        Tool::Ascii => crate::utility::ascii_ui(ui),
        Tool::Converter => crate::utility::converter_ui(&mut app.converter_text, ui),
        Tool::Calculator => crate::calc::calc_ui(&mut app.calculator, ui),
        Tool::About => crate::utility::about_ui(ui),
        Tool::Emulator | Tool::Source => {
            ui.weak("not available here");
        }
    }
}

/// The open tools that are floating windows: all of them in the classic layout and on the tablet,
/// the ones without a group in the panels layout.
pub fn windows(app: &mut App, ctx: &egui::Context, host: &mut dyn Host) {
    let before = app.windows.clone();
    // The emulator, its screen and the source first: the other windows open on top of them.
    const MAIN: [Tool; 3] = [Tool::Emulator, Tool::Screen, Tool::Source];
    for tool in MAIN.into_iter().chain(Tool::ALL.into_iter().filter(|t| !MAIN.contains(t))) {
        if tool == Tool::Editor || !host.floats(tool) {
            continue;
        }
        if !tool.available(app) {
            app.windows[tool] = false;
            continue;
        }
        if app.panels() && tool.group().is_some() {
            continue;
        }
        let raise = tool.raise(app).is_some_and(std::mem::take);
        if raise {
            app.windows[tool] = true;
        }
        if !app.windows[tool] {
            continue;
        }
        let mut open = true;
        let shown = tool.window(app).open(&mut open).show(ctx, |ui| tool_ui(app, ui, tool, host));
        if raise && let Some(r) = shown {
            ctx.move_to_top(r.response.layer_id);
        }
        // The body may have closed it as well.
        app.windows[tool] &= open;
    }
    // A body opened a window drawn before it: show it now, not at the next input.
    if app.windows != before {
        ctx.request_repaint();
    }
}

/// The open tools, saved in the settings by name (the fields of the former struct).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Windows([bool; Tool::COUNT]);

impl Default for Windows {
    fn default() -> Self {
        Windows([false; Tool::COUNT])
    }
}

impl std::ops::Index<Tool> for Windows {
    type Output = bool;
    fn index(&self, tool: Tool) -> &bool {
        &self.0[tool as usize]
    }
}

impl std::ops::IndexMut<Tool> for Windows {
    fn index_mut(&mut self, tool: Tool) -> &mut bool {
        &mut self.0[tool as usize]
    }
}

impl Windows {
    /// Whether the tool is shown (the editor always is).
    pub fn is_open(&self, tool: Tool) -> bool {
        tool == Tool::Editor || self[tool]
    }

    /// Closes the tools of a loaded program and returns them.
    pub fn take_program_tools(&mut self) -> Windows {
        let mut tools = Windows::default();
        for t in Tool::ALL.into_iter().filter(|t| t.spec().program) {
            tools[t] = std::mem::take(&mut self[t]);
        }
        tools
    }

    /// Opens the tools that are open in `other` as well.
    pub fn open_also(&mut self, other: &Windows) {
        for t in Tool::ALL {
            self[t] |= other[t];
        }
    }
}

const KEYS: [&str; 25] = [
    "emulator",
    "screen",
    "source",
    "memory",
    "stack",
    "flags",
    "vars",
    "symbols",
    "listing",
    "log",
    "devices",
    "ascii",
    "converter",
    "console",
    "breakpoints",
    "watch",
    "call_stack",
    "alu",
    "fpu",
    "stop_condition",
    "flag_analyser",
    "value_viewer",
    "drive",
    "programs",
    "about",
];

impl Serialize for Windows {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let saved: Vec<(&'static str, bool)> =
            Tool::ALL.into_iter().filter_map(|t| t.spec().key.map(|k| (k, self[t]))).collect();
        let mut st = s.serialize_struct("Windows", saved.len())?;
        for (k, v) in saved {
            st.serialize_field(k, &v)?;
        }
        st.end()
    }
}

impl<'de> Deserialize<'de> for Windows {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Windows;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("open windows by name")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Windows, A::Error> {
                let mut w = Windows::default();
                while let Some(key) = map.next_key::<String>()? {
                    let open: bool = map.next_value()?;
                    // Unknown names (a newer or older version) are skipped.
                    if let Some(t) = Tool::ALL.into_iter().find(|t| t.spec().key == Some(key.as_str())) {
                        w[t] = open;
                    }
                }
                Ok(w)
            }
        }
        d.deserialize_struct("Windows", &KEYS, Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_listed_once_with_a_distinct_key() {
        for (i, t) in Tool::ALL.into_iter().enumerate() {
            assert!(!Tool::ALL[..i].contains(&t), "{t:?} twice");
        }
        let keys: Vec<&str> = Tool::ALL.into_iter().filter_map(|t| t.spec().key).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len());
        // KEYS (for the deserializer) lists the saved ones and the former `about`.
        for k in &keys {
            assert!(KEYS.contains(k), "{k} missing from KEYS");
        }
    }

    #[test]
    fn windows_are_saved_by_name_and_read_from_the_former_struct() {
        let mut w = Windows::default();
        (w[Tool::Memory], w[Tool::CallStack], w[Tool::Reference]) = (true, true, true);
        let text = ron::to_string(&w).unwrap();
        assert!(text.contains("memory:true") && text.contains("call_stack:true"), "{text}");
        let back: Windows = ron::from_str(&text).unwrap();
        // The reference is not saved.
        assert!(back[Tool::Memory] && back[Tool::CallStack] && !back[Tool::Reference]);
        // Settings of the version with a struct of flags (some fields, an unknown one).
        let old: Windows = ron::from_str("(emulator:true,flags:true,about:true,gone:true)").unwrap();
        assert!(old[Tool::Emulator] && old[Tool::Flags] && !old[Tool::About] && !old[Tool::Memory]);
    }
}
