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
use crate::tools::Windows;

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
    /// Language of the reference and of its tooltips in the editor.
    pub reference_lang: crate::reference::Lang,
    pub vars_format: crate::inspect::NumFormat,
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
            reference_lang: Default::default(),
            vars_format: Default::default(),
            syntax: SyntaxChoice::Auto,
            sessions: Vec::new(),
            drive_c_source_folder: false,
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
    /// Write the program loaded in the emulator, not a file.
    pub loaded: bool,
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

/// The program being edited: where it comes from and its text.
pub struct Document {
    pub path: Option<PathBuf>,
    /// Name of a file without a path (opened in the browser), shown and used for downloads.
    pub name: Option<String>,
    /// Session key of a built-in example (it has no path).
    pub example_key: Option<String>,
    /// The program of "my programs" (the browser) shown in the editor.
    pub program: Option<String>,
    /// The program was opened from a link (see [`App::from_link`]).
    pub from_link: bool,
    pub text: String,
    /// The text as last saved (or opened): differs while there are unsaved changes.
    pub saved_text: String,
    /// The file's debugging session may be saved (set once it was restored).
    pub session_started: bool,
}

/// The editor widget: cursor, selection, search, bookmarks and requests for the next frame.
pub struct EditorState {
    /// Syntax highlighting of the text, kept between frames.
    pub(crate) highlighter: crate::highlight::Highlighter,
    pub goto_line: Option<u32>,
    pub cursor_line: u32,
    /// Selection as sorted char indices.
    pub selection: (usize, usize),
    pub pending_select: Option<(usize, usize)>,
    pub find: crate::find::FindState,
    /// Bookmarked lines (1-based).
    pub bookmarks: std::collections::BTreeSet<u32>,
    /// Undo (`true`) / redo (`false`) requested from the Edit menu.
    pub pending_undo: Option<bool>,
    /// Line of the current instruction in the last frame (to scroll it into view).
    pub exec_line: Option<u32>,
    /// Colour of the current instruction (default: [`crate::colors::info`]).
    pub current_line_color: Option<egui::Color32>,
}

impl Default for EditorState {
    fn default() -> Self {
        EditorState {
            highlighter: Default::default(),
            goto_line: None,
            cursor_line: 1,
            selection: (0, 0),
            pending_select: None,
            find: Default::default(),
            bookmarks: Default::default(),
            pending_undo: None,
            exec_line: None,
            current_line_color: None,
        }
    }
}

/// Result of the last build or load.
#[derive(Default)]
pub struct Build {
    pub messages: Vec<Message>,
    /// Error lines of the current file (1-based).
    pub error_lines: Vec<u32>,
    /// Parts of lines of the current file the errors are about: (line, column, length), underlined
    /// in the editor.
    pub error_spans: Vec<(u32, usize, usize)>,
    /// A build/load has failed since the last successful emulator load. The previous machine
    /// remains available for inspection, but Run must not start it by mistake.
    pub(crate) failed: bool,
}

/// The program loaded in the emulator: its sources.
#[derive(Default)]
pub struct Program {
    /// Source files (lines); the first is the main one.
    pub files: Vec<(String, Vec<String>)>,
    /// Macro/procedure index of each source file.
    pub index: Vec<Vec<rustemu_asm::library::Entry>>,
    /// Source text it was assembled from (to warn when it is outdated).
    pub text: Option<String>,
    /// Incremented on every load (compiled expressions refer to the program's symbols).
    pub generation: u64,
    /// "Save the emulator's state" (File menu of the emulator).
    pub saved_state: Option<SavedState>,
}

/// The run loop and the machine before the last step.
pub struct Exec {
    pub last_step: f64,
    pub instr_per_frame: u64,
    /// CPU state before the last step (to highlight changed registers).
    pub prev_cpu: Cpu,
    /// The 8087 before the last step / run (its changes are red in the FPU window).
    pub prev_fpu: rustemu_core::fpu::Fpu,
}

impl Default for Exec {
    fn default() -> Self {
        Exec { last_step: 0.0, instr_per_frame: 50_000, prev_cpu: Cpu::new(), prev_fpu: Default::default() }
    }
}

/// Breakpoints, watches and conditions.
#[derive(Default)]
pub struct Debugger {
    /// Line breakpoints by (file index, line).
    pub bp_lines: std::collections::BTreeMap<(usize, u32), Breakpoint>,
    /// Address of each line breakpoint in the loaded program.
    pub bp_addr: std::collections::HashMap<u32, (usize, u32)>,
    /// Breakpoints of included files waiting for the program to be loaded (file names are
    /// resolved to file indices then).
    pub pending_breakpoints: Vec<SavedBreakpoint>,
    pub stop_condition: StopCondition,
    pub watches: Vec<Watch>,
    pub value_viewer: crate::debugger::ValueViewer,
}

/// Where the views of the machine are: memory, disassembly, source, devices.
pub struct Views {
    pub mem_view_addr: u32,
    pub mem_addr_text: String,
    pub mem_scroll_pending: bool,
    pub mem_last_base: u32,
    /// First address of the disassembly list.
    pub code_view_start: u32,
    /// The disassembly shows `code_view_start` instead of following CS:IP ("show current break
    /// point"); released by the next execution command or "show current instruction".
    pub code_view_pinned: bool,
    /// Instruction selected in the disassembly (segment, offset) for the Debug menu.
    pub code_selected: Option<(u16, u16)>,
    /// Mouse wheel movement over the disassembly not yet turned into whole rows.
    pub code_wheel: f32,
    /// Source file shown in the source window.
    pub source_file_idx: usize,
    pub last_source_pos: Option<(usize, u32)>,
    pub source_scroll_pending: Option<(u32, u8)>,
    pub source_scroll_to: Option<u32>,
    pub listing_cache: Option<(usize, String)>,
    pub device_tab: crate::devices_ui::Device,
}

impl Default for Views {
    fn default() -> Self {
        Views {
            mem_view_addr: 0,
            mem_addr_text: String::new(),
            mem_scroll_pending: true,
            mem_last_base: u32::MAX,
            code_view_start: 0,
            code_view_pinned: false,
            code_selected: None,
            code_wheel: 0.0,
            source_file_idx: 0,
            last_source_pos: None,
            source_scroll_pending: None,
            source_scroll_to: None,
            listing_cache: None,
            device_tab: Default::default(),
        }
    }
}

/// The dialogs that are open and what is typed in them.
#[derive(Default)]
pub struct Dialogs {
    pub notice: Option<(String, String)>,
    /// "Save changes?" before this action.
    pub confirm_discard: Option<PendingAction>,
    /// Command line parameters: the edited text.
    pub cmdline: Option<String>,
    /// "choose code template": the selected option.
    pub new_file: Option<usize>,
    /// "use NASM syntax" in the choose code template dialog.
    pub new_nasm: bool,
    pub options: bool,
    pub bin_to_floppy: Option<BinToFloppy>,
    /// "more examples..." is open: `Some(true)` from the emulator (the example is emulated).
    pub more_examples: Option<bool>,
    /// Filter of the "more examples..." list.
    pub examples_filter: String,
    /// "save in this browser": the name being typed.
    pub save_name: Option<crate::programs::SaveName>,
    /// A program in the "my programs" window waiting for its deletion to be confirmed.
    pub program_delete: Option<String>,
    /// "share link".
    pub share: Option<crate::share::ShareDialog>,
}

/// What lives in the browser (the web version): programs, the memory drive, uploads.
#[derive(Default)]
pub struct Web {
    /// Programs saved in the browser, by name.
    pub programs: crate::programs::Programs,
    /// The user's memory drive C: while a program from a link runs on a temporary one.
    pub(crate) own_drive: Option<rustemu_core::memdrive::MemDrive>,
    /// The notice about the temporary drive C: is still to be shown for the program from a link.
    pub(crate) link_notice_pending: bool,
    /// A file to ask the browser for at the next frame (see `platform::request_upload`).
    pub pending_upload: Option<crate::platform::UploadFor>,
    /// File of the memory drive shown in the "drive C:" window.
    pub drive_preview: Option<String>,
    /// "delete all" in the "drive C:" window waits for confirmation.
    pub drive_confirm_clear: bool,
}

/// What the front end sets: its kind and its look.
pub struct FrontEnd {
    /// Files live in browser storage (the web version): Save keeps the program in "my programs",
    /// Open shows them, DOS programs get a drive C: in memory.
    pub browser: bool,
    /// The tablet layout is shown (set by the tablet front end).
    pub tablet_layout: bool,
    /// A theme of its own (the tablet layout); it replaces the theme setting.
    pub fixed_theme: Option<fn(&egui::Context)>,
    /// The front end can switch between the desktop and the tablet layout (the web version); the
    /// options window and the tablet menu offer it then and set `switch_layout`.
    pub can_switch_layout: bool,
    pub switch_layout: bool,
    /// Address share links point to (the web version: its own; default [`WEB_URL`]).
    pub share_base: Option<String>,
}

impl Default for FrontEnd {
    fn default() -> Self {
        FrontEnd {
            browser: crate::platform::is_web(),
            tablet_layout: false,
            fixed_theme: None,
            can_switch_layout: false,
            switch_layout: false,
            share_base: None,
        }
    }
}

/// Writes to the storage (see [`App::persist`]).
#[derive(Default)]
pub struct Store {
    /// Values last written, to skip unchanged ones.
    pub(crate) persisted: std::collections::HashMap<&'static str, u64>,
    /// Keys whose last write did not reach the storage (the browser's quota is full).
    pub(crate) unsaved_keys: std::collections::BTreeSet<&'static str>,
    /// The storage refused data and the user was told; set again once everything is written.
    pub warned: bool,
}

pub struct App {
    pub settings: Settings,
    /// The emulator with the loaded program.
    pub session: Session,
    pub run: RunMode,
    pub status: String,
    /// The open tools.
    pub windows: Windows,
    /// The program's tools that were open last time: they open with the emulator, at the first
    /// emulate (Emu8086 starts with the source editor alone).
    pub deferred_tools: Option<Windows>,
    /// The arrangement of the panels (`None`: the default one; saved in `Settings::panels`).
    pub panels: Option<egui_tiles::Tree<crate::tools::Tool>>,
    pub doc: Document,
    pub editor: EditorState,
    pub build: Build,
    pub program: Program,
    pub exec: Exec,
    pub debugger: Debugger,
    pub view: Views,
    pub dialogs: Dialogs,
    pub web: Web,
    pub front: FrontEnd,
    pub store: Store,
    // ---- state of the tools
    pub screen: crate::screen::ScreenState,
    pub console: crate::console::ConsoleState,
    pub library: crate::library_ui::LibraryState,
    pub reference: crate::reference_ui::RefState,
    pub calculator: crate::calc::CalcState,
    pub converter_text: String,
    /// Drive C: of DOS programs on the desktop (default [`crate::platform::vdrive_dir`]).
    pub drive_dir: PathBuf,
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
        self.settings.layout == crate::workspace::Layout::Panels && !self.front.tablet_layout
    }

    pub fn with_settings(settings: Settings) -> Self {
        let mut app = App {
            settings,
            session: Session::new(),
            run: RunMode::Paused,
            status: "Ready".into(),
            windows: Windows::default(),
            deferred_tools: None,
            panels: None,
            doc: Document {
                path: None,
                name: None,
                example_key: None,
                program: None,
                from_link: false,
                text: String::new(),
                saved_text: String::new(),
                session_started: false,
            },
            editor: Default::default(),
            build: Default::default(),
            program: Default::default(),
            exec: Default::default(),
            debugger: Default::default(),
            view: Default::default(),
            dialogs: Default::default(),
            web: Default::default(),
            front: Default::default(),
            store: Default::default(),
            screen: crate::screen::ScreenState::new(),
            console: Default::default(),
            library: Default::default(),
            reference: Default::default(),
            calculator: Default::default(),
            converter_text: String::new(),
            drive_dir: crate::platform::vdrive_dir(),
        };
        app.windows = app.settings.windows.clone();
        app.deferred_tools = Some(app.windows.take_program_tools());
        app.panels = app.settings.panels.as_deref().and_then(|s| ron::from_str(s).ok());
        app.reference.lang = app.settings.reference_lang;
        let last = app.settings.last_file.clone();
        match last {
            Some(p) if p.exists() => app.open_path(&p),
            // The first item of the Examples menu: Hello, world.
            _ => app.load_example(crate::examples::index(crate::examples::MENU[0].1).unwrap_or(0)),
        }
        // The browser has no file system: DOS programs get a drive in memory.
        if app.front.browser {
            app.session.machine.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
        }
        app
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        if let Some(apply) = self.front.fixed_theme {
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
        self.doc.text != self.doc.saved_text
    }

    pub fn file_name(&self) -> String {
        self.doc
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| self.doc.name.clone())
            .or_else(|| {
                self.doc.example_key.as_ref().and_then(|k| k.strip_prefix(files::EXAMPLE_KEY)).map(str::to_string)
            })
            .unwrap_or_else(|| "untitled.asm".into())
    }

    /// Runs `action` now, or asks to discard unsaved changes first.
    pub fn guarded(&mut self, action: PendingAction) {
        if self.dirty() {
            self.dialogs.confirm_discard = Some(action);
        } else {
            self.perform(action);
        }
    }

    pub fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::New => self.dialogs.new_file = Some(0),
            PendingAction::Open(None) => self.open_dialog(),
            PendingAction::Open(Some(p)) => self.open_path(&p),
            PendingAction::Example(i) => self.load_example(i),
            PendingAction::Program(name) => self.open_program(&name),
            PendingAction::Upload => self.web.pending_upload = Some(crate::platform::UploadFor::Open),
            PendingAction::Shared(s) => self.open_shared(s),
            PendingAction::Quit => {}
        }
    }

    pub fn notice(&mut self, title: &str, text: &str) {
        self.dialogs.notice = Some((title.to_string(), text.to_string()));
    }
}
