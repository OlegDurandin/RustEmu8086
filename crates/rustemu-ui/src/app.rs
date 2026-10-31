//! Application state shared by the front ends: settings, the editor and the emulator. The
//! commands are in the submodules: files, per-file sessions, building, execution, emulator
//! window commands, shortcuts and persistence.

use std::path::{Path, PathBuf};

use egui::{Key, KeyboardShortcut, Modifiers};
use rustemu_core::cpu::Cpu;
use rustemu_core::{Event, disasm};
use rustemu_session::expr::Expr;
use rustemu_session::{Session, StepGoal, StepKind};
use serde::{Deserialize, Serialize};

use crate::examples::EXAMPLES;

mod build;
mod emulator_cmds;
mod exec;
mod files;
mod session;
mod shortcuts;
mod storage;

/// Assembler syntax (options window).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum SyntaxChoice {
    /// Detected from the source (`bits 16`, `section`, `%define`, ...).
    #[default]
    Auto,
    Masm,
    Nasm,
}

impl SyntaxChoice {
    pub fn dialect(self) -> Option<rustemu_asm::Dialect> {
        match self {
            SyntaxChoice::Auto => None,
            SyntaxChoice::Masm => Some(rustemu_asm::Dialect::Masm),
            SyntaxChoice::Nasm => Some(rustemu_asm::Dialect::Nasm),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
    System,
}

/// Releases of RustEmu8086 (help -> check for an update).
pub const RELEASES_URL: &str = "https://github.com/OlegDurandin/RustEmu8086/releases";
pub const REPO_URL: &str = "https://github.com/OlegDurandin/RustEmu8086";
pub const WEB_URL: &str = "https://olegdurandin.github.io/RustEmu8086/";

/// Settings persisted between runs.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<PathBuf>,
    pub theme: ThemeChoice,
    pub font_size: f32,
    pub step_delay_ms: u32,
    pub last_file: Option<PathBuf>,
    pub timer_irq: bool,
    /// Open windows (positions and sizes are kept by egui itself).
    pub windows: Windows,
    /// Tools as panels or as floating windows (classic, like Emu8086).
    pub layout: crate::workspace::Layout,
    /// The arrangement of the panels in RON (`None`: the default one). Kept as text, so that one
    /// saved by another version that cannot be read is dropped, not the whole settings.
    pub panels: Option<String>,
    pub show_macro_code: bool,
    /// The disassembler shows the program's data as `DB`/`DW` and the zeros after it as `DUP`.
    pub disasm_data: bool,
    pub follow_source: bool,
    pub vars_format: crate::debug::NumFormat,
    /// Assembler syntax: detected, Emu8086/MASM or NASM.
    pub syntax: SyntaxChoice,
    /// Debugging session of recently used files, most recent first.
    pub sessions: Vec<(String, FileSession)>,
    /// Drive C: of DOS programs is the folder of the program instead of its own folder (the
    /// desktop; programs can then change and delete the files next to the source).
    pub drive_c_source_folder: bool,
}

/// What is remembered per source file between runs.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct FileSession {
    pub breakpoints: Vec<SavedBreakpoint>,
    pub watches: Vec<String>,
    pub bookmarks: Vec<u32>,
    pub stop_condition: String,
    pub stop_enabled: bool,
    pub stop_on_change: bool,
    pub cursor_line: u32,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct SavedBreakpoint {
    /// Empty for the edited file, otherwise the name of an included file (e.g. `emu8086.inc`).
    pub file: String,
    pub line: u32,
    pub condition: String,
    pub enabled: bool,
}

/// How many file sessions are remembered.
const MAX_SESSIONS: usize = 30;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            recent: Vec::new(),
            theme: ThemeChoice::Light,
            font_size: 14.0,
            step_delay_ms: 0,
            last_file: None,
            timer_irq: false,
            windows: Windows::default(),
            layout: Default::default(),
            panels: None,
            show_macro_code: true,
            disasm_data: true,
            follow_source: true,
            vars_format: Default::default(),
            syntax: SyntaxChoice::Auto,
            sessions: Vec::new(),
            drive_c_source_folder: false,
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Windows {
    pub emulator: bool,
    pub screen: bool,
    pub source: bool,
    pub memory: bool,
    pub stack: bool,
    pub flags: bool,
    pub vars: bool,
    pub symbols: bool,
    pub listing: bool,
    pub log: bool,
    pub devices: bool,
    pub about: bool,
    pub ascii: bool,
    pub converter: bool,
    pub console: bool,
    pub breakpoints: bool,
    pub watch: bool,
    pub call_stack: bool,
    pub alu: bool,
    pub stop_condition: bool,
    pub flag_analyser: bool,
    pub value_viewer: bool,
    /// Files of the memory drive C: (the browser).
    pub drive: bool,
    /// "my programs": programs saved in the browser.
    pub programs: bool,
}

/// The tools of a loaded program: they open with the emulator.
const PROGRAM_TOOLS: [fn(&mut Windows) -> &mut bool; 19] = [
    |w| &mut w.emulator,
    |w| &mut w.screen,
    |w| &mut w.source,
    |w| &mut w.memory,
    |w| &mut w.stack,
    |w| &mut w.flags,
    |w| &mut w.vars,
    |w| &mut w.symbols,
    |w| &mut w.listing,
    |w| &mut w.log,
    |w| &mut w.devices,
    |w| &mut w.console,
    |w| &mut w.breakpoints,
    |w| &mut w.watch,
    |w| &mut w.call_stack,
    |w| &mut w.alu,
    |w| &mut w.stop_condition,
    |w| &mut w.flag_analyser,
    |w| &mut w.value_viewer,
];

impl Windows {
    /// Closes the tools of a loaded program and returns them.
    pub fn take_program_tools(&mut self) -> Windows {
        let mut tools = Windows::default();
        for f in PROGRAM_TOOLS {
            *f(&mut tools) = std::mem::take(f(self));
        }
        tools
    }

    /// Opens the tools that are open in `other` as well.
    pub fn open_also(&mut self, mut other: Windows) {
        for f in PROGRAM_TOOLS {
            *f(self) |= *f(&mut other);
        }
    }
}

pub struct Message {
    pub text: String,
    /// (file name, line) the message refers to.
    pub loc: Option<(String, u32)>,
    /// The part of that line: column and length in characters.
    pub span: Option<(usize, usize)>,
    pub error: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RunMode {
    Paused,
    Running,
    /// Run until execution reaches one of these addresses (Run to cursor, `g addr` in the console,
    /// Step Over in binaries without source).
    RunTo(Vec<u32>),
    /// Source-level Step Over / Step Out in progress.
    Source(StepGoal),
}

/// A line breakpoint.
#[derive(Clone)]
pub struct Breakpoint {
    /// Stop only when this expression is not zero (empty: always).
    pub condition: String,
    pub enabled: bool,
    /// Times execution stopped here.
    pub hits: u32,
    /// Compiled condition, or why it does not compile (None: no condition / not compiled yet).
    pub compiled: Option<Result<Expr, String>>,
}

impl Breakpoint {
    pub fn new() -> Self {
        Breakpoint { condition: String::new(), enabled: true, hits: 0, compiled: None }
    }
}

impl Default for Breakpoint {
    fn default() -> Self {
        Self::new()
    }
}

/// "Stop on condition" (checked after every instruction while running).
#[derive(Default)]
pub struct StopCondition {
    pub text: String,
    pub enabled: bool,
    /// Stop when the value changes (otherwise: when it becomes non-zero).
    pub on_change: bool,
    pub compiled: Option<Result<Expr, String>>,
    /// Value after the previous instruction.
    pub last: Option<i64>,
}

/// An expression in the watch window.
pub struct Watch {
    pub text: String,
    pub compiled: Option<Result<Expr, String>>,
    /// Value when execution was last started (to show what changed).
    pub before: Option<i64>,
}

/// State of the 'write .bin file to floppy' dialog.
pub struct BinToFloppy {
    pub path: String,
    pub drive: usize,
    pub cyl: usize,
    pub head: usize,
    pub sector: usize,
}

/// The emulator with the sources it was loaded from.
#[derive(Clone)]
pub struct SavedState {
    session: rustemu_session::SessionState,
    source_files: Vec<(String, Vec<String>)>,
    source_index: Vec<Vec<rustemu_asm::library::Entry>>,
    loaded_text: Option<String>,
}

pub struct App {
    pub settings: Settings,
    // ---- editor
    pub path: Option<PathBuf>,
    /// Name of a file without a path (opened in the browser), shown and used for downloads.
    pub doc_name: Option<String>,
    /// The program in the editor was opened from a link (see [`App::from_link`]).
    pub(crate) opened_from_link: bool,
    /// The notice about the temporary drive C: is still to be shown for the program from a link.
    pub(crate) link_notice_pending: bool,
    /// The user's memory drive C: while a program from a link runs on a temporary one.
    pub(crate) own_drive: Option<rustemu_core::memdrive::MemDrive>,
    /// A file to ask the browser for at the next frame (see `platform::request_upload`).
    pub pending_upload: Option<crate::platform::UploadFor>,
    /// File of the memory drive shown in the "drive C:" window.
    pub drive_preview: Option<String>,
    /// "delete all" in the "drive C:" window waits for confirmation.
    pub drive_confirm_clear: bool,
    pub text: String,
    pub saved_text: String,
    /// Syntax highlighting of `text`, kept between frames.
    pub(crate) highlighter: crate::highlight::Highlighter,
    pub messages: Vec<Message>,
    /// Error lines of the current file (1-based).
    pub error_lines: Vec<u32>,
    /// Parts of lines of the current file the errors are about: (line, column, length), underlined
    /// in the editor.
    pub error_spans: Vec<(u32, usize, usize)>,
    /// A build/load has failed since the last successful emulator load. The
    /// previous machine remains available for inspection, but Run must not
    /// start it by mistake.
    build_failed: bool,
    pub goto_line: Option<u32>,
    pub cursor_line: u32,
    // ---- emulator
    pub session: Session,
    pub windows: Windows,
    /// The program's tools that were open last time: they open with the emulator, at the first
    /// emulate (Emu8086 starts with the source editor alone).
    pub deferred_tools: Option<Windows>,
    pub run: RunMode,
    pub last_step: f64,
    pub status: String,
    /// CPU state before the last step (to highlight changed registers).
    pub prev_cpu: Cpu,
    /// Source files of the loaded program (lines).
    pub source_files: Vec<(String, Vec<String>)>,
    pub source_file_idx: usize,
    pub follow_source: bool,
    pub mem_view_addr: u32,
    pub mem_addr_text: String,
    pub screen: crate::screen::ScreenState,
    pub instr_per_frame: u64,
    pub notice: Option<(String, String)>,
    pub confirm_discard: Option<PendingAction>,
    pub converter_text: String,
    /// Line breakpoints by (file index, line).
    pub bp_lines: std::collections::BTreeMap<(usize, u32), Breakpoint>,
    /// Address of each line breakpoint in the loaded program.
    pub bp_addr: std::collections::HashMap<u32, (usize, u32)>,
    pub stop_condition: StopCondition,
    pub watches: Vec<Watch>,
    /// Source text the loaded program was assembled from (to warn when it is outdated).
    pub loaded_text: Option<String>,
    /// Incremented on every load (compiled expressions refer to the program's symbols).
    pub load_gen: u64,
    /// Editor line of the current instruction in the last frame (to scroll it into view).
    pub editor_exec_line: Option<u32>,
    /// Breakpoints of included files waiting for the program to be loaded (file names are
    /// resolved to file indices then).
    pub pending_breakpoints: Vec<SavedBreakpoint>,
    /// The current file's session may be saved (set once it was restored).
    pub session_started: bool,
    /// Session key of a built-in example (it has no path).
    pub example_key: Option<String>,
    /// First address of the disassembly list.
    pub code_view_start: u32,
    /// The disassembly shows `code_view_start` instead of following CS:IP ("show current break
    /// point"); released by the next execution command or "show current instruction".
    pub code_view_pinned: bool,
    /// Instruction selected in the disassembly (segment, offset) for the Debug menu.
    pub code_selected: Option<(u16, u16)>,
    /// Mouse wheel movement over the disassembly not yet turned into whole rows.
    pub code_wheel: f32,
    /// "more examples..." is open: `Some(true)` from the emulator (the example is emulated).
    pub more_examples: Option<bool>,
    /// Filter of the "more examples..." list.
    pub examples_filter: String,
    /// "Save the emulator's state" (File menu of the emulator).
    pub saved_state: Option<SavedState>,
    pub bin_to_floppy: Option<BinToFloppy>,
    /// "use NASM syntax" in the choose code template dialog.
    pub new_nasm: bool,
    pub value_viewer: crate::debugger::ValueViewer,
    pub last_source_pos: Option<(usize, u32)>,
    pub source_scroll_pending: Option<(u32, u8)>,
    pub vars_format: crate::debug::NumFormat,
    pub mem_scroll_pending: bool,
    pub mem_last_base: u32,
    pub listing_cache: Option<(usize, String)>,
    pub device_tab: crate::devices_ui::Device,
    /// Editor selection as sorted char indices.
    pub selection: (usize, usize),
    pub pending_select: Option<(usize, usize)>,
    pub find: crate::find::FindState,
    pub console: crate::console::ConsoleState,
    /// Macro/procedure index of each source file of the loaded program.
    pub source_index: Vec<Vec<rustemu_asm::library::Entry>>,
    pub show_macro_code: bool,
    pub source_scroll_to: Option<u32>,
    pub library: crate::library_ui::LibraryState,
    pub reference: crate::reference_ui::RefState,
    /// Command line parameters dialog: (open, edited text).
    pub cmdline_dialog: Option<String>,
    /// "choose code template" dialog: selected option while open.
    pub new_dialog: Option<usize>,
    pub options_open: bool,
    pub calculator: crate::calc::CalcState,
    /// Bookmarked lines (1-based).
    pub bookmarks: std::collections::BTreeSet<u32>,
    /// Undo (`true`) / redo (`false`) requested from the Edit menu.
    pub pending_undo: Option<bool>,
    /// A front end with a fixed theme of its own (the tablet layout) sets it here; it replaces the
    /// theme setting.
    pub fixed_theme: Option<fn(&egui::Context)>,
    /// Files live in browser storage (the web version): Save keeps the program in "my programs",
    /// Open shows them, DOS programs get a drive C: in memory.
    pub browser: bool,
    /// Programs saved in the browser, by name.
    pub programs: crate::programs::Programs,
    /// The program of `programs` shown in the editor.
    pub program: Option<String>,
    /// "save in this browser" dialog: the name being typed.
    pub save_name: Option<crate::programs::SaveName>,
    /// A program in the "my programs" window waiting for its deletion to be confirmed.
    pub program_delete: Option<String>,
    /// "share link" dialog.
    pub share: Option<crate::share::ShareDialog>,
    /// Address share links point to (the web version: its own; default [`WEB_URL`]).
    pub share_base: Option<String>,
    /// Values last written to the storage, to skip unchanged ones (see [`App::persist`]).
    persisted: std::collections::HashMap<&'static str, u64>,
    /// Drive C: of DOS programs on the desktop (default [`crate::platform::vdrive_dir`]).
    pub drive_dir: PathBuf,
    /// Keys whose last write did not reach the storage (the browser's quota is full).
    unsaved_keys: std::collections::BTreeSet<&'static str>,
    /// The storage refused data and the user was told; set again once everything is written.
    pub storage_warned: bool,
    /// Colour of the current instruction in the editor (default: [`crate::colors::info`]).
    pub current_line_color: Option<egui::Color32>,
    /// The tablet layout is shown (set by the tablet front end).
    pub tablet_layout: bool,
    /// The arrangement of the panels (`None`: the default one; saved in `Settings::panels`).
    pub panels: Option<egui_tiles::Tree<crate::workspace::Tool>>,
    /// The front end can switch between the desktop and the tablet layout (the web version); the
    /// options window and the tablet menu offer it then and set `switch_layout`.
    pub can_switch_layout: bool,
    pub switch_layout: bool,
}

#[derive(Clone)]
pub enum PendingAction {
    New,
    Open(Option<PathBuf>),
    Example(usize),
    /// A program saved in the browser.
    Program(String),
    /// A file from the computer (the browser asks for it).
    Upload,
    /// A program from a link.
    Shared(crate::share::Shared),
    Quit,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
        let mut app = Self::with_settings(settings);
        if let Some(storage) = cc.storage {
            app.restore_drive(storage);
            app.restore_browser(storage);
        }
        crate::fonts::install(&cc.egui_ctx);
        app.apply_theme(&cc.egui_ctx);
        app
    }

    /// Tools are tabs of the workspace, not floating windows (never in the tablet layout).
    pub fn panels(&self) -> bool {
        self.settings.layout == crate::workspace::Layout::Panels && !self.tablet_layout
    }

    pub fn with_settings(settings: Settings) -> Self {
        let mut app = App {
            settings,
            path: None,
            doc_name: None,
            opened_from_link: false,
            link_notice_pending: false,
            own_drive: None,
            pending_upload: None,
            drive_preview: None,
            drive_confirm_clear: false,
            text: String::new(),
            saved_text: String::new(),
            highlighter: Default::default(),
            messages: Vec::new(),
            error_lines: Vec::new(),
            error_spans: Vec::new(),
            build_failed: false,
            goto_line: None,
            cursor_line: 1,
            session: Session::new(),
            windows: Windows::default(),
            deferred_tools: None,
            run: RunMode::Paused,
            last_step: 0.0,
            status: "Ready".into(),
            prev_cpu: Cpu::new(),
            source_files: Vec::new(),
            source_file_idx: 0,
            follow_source: true,
            mem_view_addr: 0,
            mem_addr_text: String::new(),
            screen: crate::screen::ScreenState::new(),
            instr_per_frame: 50_000,
            notice: None,
            confirm_discard: None,
            converter_text: String::new(),
            bp_lines: Default::default(),
            bp_addr: Default::default(),
            stop_condition: Default::default(),
            watches: Vec::new(),
            loaded_text: None,
            load_gen: 0,
            editor_exec_line: None,
            pending_breakpoints: Vec::new(),
            session_started: false,
            example_key: None,
            code_view_start: 0,
            code_view_pinned: false,
            code_selected: None,
            code_wheel: 0.0,
            more_examples: None,
            examples_filter: String::new(),
            saved_state: None,
            bin_to_floppy: None,
            new_nasm: false,
            value_viewer: Default::default(),
            last_source_pos: None,
            source_scroll_pending: None,
            vars_format: Default::default(),
            mem_scroll_pending: true,
            mem_last_base: u32::MAX,
            listing_cache: None,
            device_tab: Default::default(),
            selection: (0, 0),
            pending_select: None,
            find: Default::default(),
            console: Default::default(),
            source_index: Vec::new(),
            show_macro_code: true,
            source_scroll_to: None,
            library: Default::default(),
            reference: Default::default(),
            cmdline_dialog: None,
            new_dialog: None,
            options_open: false,
            calculator: Default::default(),
            bookmarks: Default::default(),
            pending_undo: None,
            fixed_theme: None,
            browser: crate::platform::is_web(),
            programs: Default::default(),
            program: None,
            save_name: None,
            program_delete: None,
            share: None,
            share_base: None,
            persisted: Default::default(),
            drive_dir: crate::platform::vdrive_dir(),
            unsaved_keys: Default::default(),
            storage_warned: false,
            current_line_color: None,
            tablet_layout: false,
            panels: None,
            can_switch_layout: false,
            switch_layout: false,
        };
        app.windows = app.settings.windows.clone();
        app.deferred_tools = Some(app.windows.take_program_tools());
        app.panels = app.settings.panels.as_deref().and_then(|s| ron::from_str(s).ok());
        app.windows.about = false;
        app.show_macro_code = app.settings.show_macro_code;
        app.follow_source = app.settings.follow_source;
        app.vars_format = app.settings.vars_format;
        let last = app.settings.last_file.clone();
        match last {
            Some(p) if p.exists() => app.open_path(&p),
            // The first item of the Examples menu: Hello, world.
            _ => app.load_example(crate::examples::index(crate::examples::MENU[0].1).unwrap_or(0)),
        }
        // The browser has no file system: DOS programs get a drive in memory.
        if app.browser {
            app.session.machine.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
        }
        app
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        if let Some(apply) = self.fixed_theme {
            apply(ctx);
            return;
        }
        ctx.set_theme(match self.settings.theme {
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::System => egui::ThemePreference::System,
        });
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved_text
    }

    pub fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| self.doc_name.clone())
            .or_else(|| self.example_key.as_ref().and_then(|k| k.strip_prefix(files::EXAMPLE_KEY)).map(str::to_string))
            .unwrap_or_else(|| "untitled.asm".into())
    }

    /// Runs `action` now, or asks to discard unsaved changes first.
    pub fn guarded(&mut self, action: PendingAction) {
        if self.dirty() {
            self.confirm_discard = Some(action);
        } else {
            self.perform(action);
        }
    }

    pub fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::New => self.new_dialog = Some(0),
            PendingAction::Open(None) => self.open_dialog(),
            PendingAction::Open(Some(p)) => self.open_path(&p),
            PendingAction::Example(i) => self.load_example(i),
            PendingAction::Program(name) => self.open_program(&name),
            PendingAction::Upload => self.pending_upload = Some(crate::platform::UploadFor::Open),
            PendingAction::Shared(s) => self.open_shared(s),
            PendingAction::Quit => {}
        }
    }

    pub fn notice(&mut self, title: &str, text: &str) {
        self.notice = Some((title.to_string(), text.to_string()));
    }
}
