//! Application state shared by the front ends: settings, the editor and the emulator. The
//! commands are in the submodules: files, per-file sessions, building, execution, emulator
//! window commands, shortcuts and persistence.

use std::path::{Path, PathBuf};

use egui::{Key, KeyboardShortcut, Modifiers};
use rustemu_core::Event;
use rustemu_core::cpu::Cpu;
use rustemu_session::expr::Expr;
use rustemu_session::{Arch, Target};
use rustemu_session::{Session, StepGoal, StepKind};
use serde::{Deserialize, Serialize};

use crate::examples::EXAMPLES;
use crate::tools::Windows;

mod arch;
mod build;
mod emulator_cmds;
mod exec;
mod files;
mod session;
mod shortcuts;
mod storage;

pub use arch::{OpenAs, Workspace};
pub use rustemu_session::detect_arch;

/// Assembler syntax (options window).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum SyntaxChoice {
    /// Detected from the source (`bits 16`, `section`, `%define`, ...).
    #[default]
    Auto,
    /// Emu8086 / MASM.
    Masm,
    /// NASM (`-f bin`).
    Nasm,
}

impl SyntaxChoice {
    /// The dialect to assemble with (`None`: detect it).
    pub fn dialect(self) -> Option<rustemu_asm::Dialect> {
        match self {
            SyntaxChoice::Auto => None,
            SyntaxChoice::Masm => Some(rustemu_asm::Dialect::Masm),
            SyntaxChoice::Nasm => Some(rustemu_asm::Dialect::Nasm),
        }
    }
}

/// Colour theme of the desktop layout (the tablet has its own).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    /// Light (the default).
    #[default]
    Light,
    /// Dark.
    Dark,
    /// As the system.
    System,
}

/// Releases of RustEmu8086 (help -> check for an update).
pub const RELEASES_URL: &str = "https://github.com/OlegDurandin/RustEmu8086/releases";
/// The source code on GitHub.
pub const REPO_URL: &str = "https://github.com/OlegDurandin/RustEmu8086";
/// MARS, the MIPS simulator the MIPS workspace follows (its documentation and syntax).
pub const MARS_URL: &str = "https://github.com/dpetersanderson/MARS";
/// The web version (share links point there by default).
pub const WEB_URL: &str = "https://olegdurandin.github.io/RustEmu8086/";

/// Settings persisted between runs.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// Recently opened files, the latest first.
    pub recent: Vec<PathBuf>,
    /// Colour theme.
    pub theme: ThemeChoice,
    /// Font size of the editor.
    pub font_size: f32,
    /// Pause between instructions while running (0: full speed).
    pub step_delay_ms: u32,
    /// The file opened at the last start (reopened at the next).
    pub last_file: Option<PathBuf>,
    /// The timer interrupt (IRQ0 -> INT 08h -> INT 1Ch) is delivered.
    pub timer_irq: bool,
    /// Open windows (positions and sizes are kept by egui itself).
    pub windows: Windows,
    /// Tools as panels or as floating windows (classic, like Emu8086).
    pub layout: crate::workspace::Layout,
    /// The arrangement of the panels in RON (`None`: the default one). Kept as text, so that one
    /// saved by another version that cannot be read is dropped, not the whole settings.
    pub panels: Option<String>,
    /// Step into macros (show the code of `emu8086.inc` while stepping inside it).
    pub show_macro_code: bool,
    /// The disassembler shows the program's data as `DB`/`DW` and the zeros after it as `DUP`.
    pub disasm_data: bool,
    /// The source window follows execution.
    pub follow_source: bool,
    /// Language of the reference and of its tooltips in the editor.
    pub reference_lang: crate::reference::Lang,
    /// Number format of the variables window.
    pub vars_format: crate::inspect::NumFormat,
    /// Assembler syntax: detected, Emu8086/MASM or NASM.
    pub syntax: SyntaxChoice,
    /// Debugging session of recently used files, most recent first.
    pub sessions: Vec<(String, FileSession)>,
    /// Drive C: of DOS programs is the folder of the program instead of its own folder (the
    /// desktop; programs can then change and delete the files next to the source).
    pub drive_c_source_folder: bool,
    /// What the MIPS workspace remembers (the 8086 one keeps it in the fields above).
    pub mips: ArchSettings,
}

/// What the workspace of a processor other than the 8086 remembers between runs.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ArchSettings {
    /// The file opened at the last start.
    pub last_file: Option<PathBuf>,
    /// Open windows.
    pub windows: Windows,
    /// The arrangement of the panels in RON (see [`Settings::panels`]).
    pub panels: Option<String>,
}

/// What is remembered per source file between runs.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct FileSession {
    /// Line breakpoints with their conditions.
    pub breakpoints: Vec<SavedBreakpoint>,
    /// Watch expressions.
    pub watches: Vec<String>,
    /// Bookmarked lines.
    pub bookmarks: Vec<u32>,
    /// Expression of "stop on condition".
    pub stop_condition: String,
    /// "Stop on condition" is on.
    pub stop_enabled: bool,
    /// Stop when the value changes (otherwise when it becomes non-zero).
    pub stop_on_change: bool,
    /// The editor line of the cursor.
    pub cursor_line: u32,
    /// The processor the file is for ([`Arch::name`]; empty: the 8086).
    pub arch: String,
}

/// A line breakpoint as saved in a [`FileSession`].
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct SavedBreakpoint {
    /// Empty for the edited file, otherwise the name of an included file (e.g. `emu8086.inc`).
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// Stop only when this expression is not zero (empty: always).
    pub condition: String,
    /// The breakpoint is on.
    pub enabled: bool,
}

/// How many file sessions are remembered.
const MAX_SESSIONS: usize = 30;

impl Settings {
    /// The file opened at the last start of the workspace of `arch`.
    pub fn last_file_mut(&mut self, arch: Arch) -> &mut Option<PathBuf> {
        match arch {
            Arch::I8086 => &mut self.last_file,
            Arch::Mips32 => &mut self.mips.last_file,
        }
    }

    /// Open windows and panels of the workspace of `arch`.
    pub(crate) fn layout_mut(&mut self, arch: Arch) -> (&mut Windows, &mut Option<String>) {
        match arch {
            Arch::I8086 => (&mut self.windows, &mut self.panels),
            Arch::Mips32 => (&mut self.mips.windows, &mut self.mips.panels),
        }
    }
}

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
            mips: ArchSettings::default(),
        }
    }
}

/// A line of the messages panel (assembler errors, notes from loading and running).
pub struct Message {
    /// The text.
    pub text: String,
    /// (file name, line) the message refers to.
    pub loc: Option<(String, u32)>,
    /// The part of that line: column and length in characters.
    pub span: Option<(usize, usize)>,
    /// An error (shown in red, its line marked in the editor).
    pub error: bool,
}

/// How the emulator runs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RunMode {
    /// Stopped: commands execute one step at a time.
    Paused,
    /// Running freely until a breakpoint, an error or Stop.
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
    /// The breakpoint is on.
    pub enabled: bool,
    /// Times execution stopped here.
    pub hits: u32,
    /// Compiled condition, or why it does not compile (None: no condition / not compiled yet).
    pub compiled: Option<Result<Expr, String>>,
}

impl Breakpoint {
    /// An enabled breakpoint without a condition.
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
    /// The expression.
    pub text: String,
    /// It is checked.
    pub enabled: bool,
    /// Stop when the value changes (otherwise: when it becomes non-zero).
    pub on_change: bool,
    /// Compiled expression, or why it does not compile (`None`: not compiled yet).
    pub compiled: Option<Result<Expr, String>>,
    /// Value after the previous instruction.
    pub last: Option<i64>,
}

/// An expression in the watch window.
pub struct Watch {
    /// The expression.
    pub text: String,
    /// Compiled expression, or why it does not compile (`None`: not compiled yet).
    pub compiled: Option<Result<Expr, String>>,
    /// Value when execution was last started (to show what changed).
    pub before: Option<i64>,
}

/// State of the 'write .bin file to floppy' dialog.
pub struct BinToFloppy {
    /// Write the program loaded in the emulator, not a file.
    pub loaded: bool,
    /// File to write (when not the loaded program).
    pub path: String,
    /// Floppy drive (FLOPPY_0..3).
    pub drive: usize,
    /// Cylinder of the first sector.
    pub cyl: usize,
    /// Head of the first sector.
    pub head: usize,
    /// First sector (1-based).
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
#[derive(Default)]
pub struct Document {
    /// The file on disk (`None`: new, an example or a program of the browser).
    pub path: Option<PathBuf>,
    /// Name of a file without a path (opened in the browser), shown and used for downloads.
    pub name: Option<String>,
    /// Session key of a built-in example (it has no path).
    pub example_key: Option<String>,
    /// The program of "my programs" (the browser) shown in the editor.
    pub program: Option<String>,
    /// The program was opened from a link (see [`App::from_link`]).
    pub from_link: bool,
    /// The text in the editor.
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
    /// A line to scroll to and put the cursor on at the next frame.
    pub goto_line: Option<u32>,
    /// The line of the cursor (1-based).
    pub cursor_line: u32,
    /// Selection as sorted char indices.
    pub selection: (usize, usize),
    /// A selection to make at the next frame (char indices).
    pub pending_select: Option<(usize, usize)>,
    /// Find / replace.
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
    /// Messages of the last build, load and run.
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
    /// Time of the last instruction while running with a step delay.
    pub last_step: f64,
    /// Instructions per frame while running freely (adapted to keep the frame rate).
    pub instr_per_frame: u64,
    /// The machine before the last step / run ([`Target::checkpoint`]): changed registers, flags
    /// and 8087 registers are highlighted against it.
    pub prev: rustemu_target::State,
    /// Why execution last stopped or what it is doing (`None`: nothing ran since the load).
    pub last_event: Option<Event>,
}

impl Exec {
    /// The 8086 before the last step / run, when the machine is an 8086.
    pub fn prev_x86(&self) -> Option<&rustemu_core::Checkpoint> {
        self.prev.get()
    }

    /// Remembers `target` as the state before the next step / run.
    pub fn remember(&mut self, target: &dyn Target) {
        self.prev = target.checkpoint();
    }
}

impl Default for Exec {
    fn default() -> Self {
        Exec {
            last_step: 0.0,
            instr_per_frame: 50_000,
            prev: rustemu_target::State::new(rustemu_core::Checkpoint { cpu: Cpu::new(), fpu: Default::default() }),
            last_event: None,
        }
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
    /// "Stop on condition".
    pub stop_condition: StopCondition,
    /// Watch expressions.
    pub watches: Vec<Watch>,
    /// The extended value viewer.
    pub value_viewer: crate::debugger::ValueViewer,
}

/// Where the views of the machine are: memory, disassembly, source, devices.
pub struct Views {
    /// First address of the memory window (16-byte aligned).
    pub mem_view_addr: u32,
    /// The address typed in the memory window.
    pub mem_addr_text: String,
    /// The memory window should scroll to `mem_view_addr` at the next frame.
    pub mem_scroll_pending: bool,
    /// `mem_view_addr` of the last frame (a change scrolls the window).
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
    /// Source line the source window last followed: (file, line).
    pub last_source_pos: Option<(usize, u32)>,
    /// Scroll the source window to a line during the next few frames: (line, frames left).
    pub source_scroll_pending: Option<(u32, u8)>,
    /// A line to scroll the source window to.
    pub source_scroll_to: Option<u32>,
    /// Text of the listing and the number of its lines it was made from.
    pub listing_cache: Option<(usize, String)>,
    /// Tab of the virtual devices window.
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
    /// A message box: (title, text).
    pub notice: Option<(String, String)>,
    /// "Save changes?" before this action.
    pub confirm_discard: Option<PendingAction>,
    /// Command line parameters: the edited text.
    pub cmdline: Option<String>,
    /// "choose code template": the selected option.
    pub new_file: Option<usize>,
    /// "use NASM syntax" in the choose code template dialog.
    pub new_nasm: bool,
    /// The options window is open.
    pub options: bool,
    /// "write .bin file to floppy".
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
    /// "open as": which processor a document is for.
    pub open_as: Option<OpenAs>,
    /// The next document opens in the workspace of this processor (chosen, or remembered by the
    /// document), without asking.
    pub open_in: Option<Arch>,
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
    /// [`rustemu_mips::files::Files::generation`] of the MIPS files last copied to drive C:.
    pub(crate) mips_files_synced: u64,
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
    /// Set by the front end to switch to the other layout at the next frame.
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

/// All the state of the application, shared by the front ends: the emulator session, the edited
/// document, the open tools and their state. Commands are its methods (in `app/`).
pub struct App {
    /// Settings saved between runs.
    pub settings: Settings,
    /// The emulator with the loaded program.
    pub session: Session,
    /// How the emulator runs.
    pub run: RunMode,
    /// The status line.
    pub status: String,
    /// The open tools.
    pub windows: Windows,
    /// The program's tools that were open last time: they open with the emulator, at the first
    /// emulate (Emu8086 starts with the source editor alone).
    pub deferred_tools: Option<Windows>,
    /// The arrangement of the panels (`None`: the default one; saved in `Settings::panels`).
    pub panels: Option<egui_tiles::Tree<crate::tools::Tool>>,
    /// The program being edited.
    pub doc: Document,
    /// The editor widget.
    pub editor: EditorState,
    /// Result of the last build.
    pub build: Build,
    /// The loaded program's sources.
    pub program: Program,
    /// The run loop.
    pub exec: Exec,
    /// Breakpoints, watches, conditions.
    pub debugger: Debugger,
    /// Positions of the views.
    pub view: Views,
    /// Open dialogs.
    pub dialogs: Dialogs,
    /// The browser's programs and drive.
    pub web: Web,
    /// What the front end sets.
    pub front: FrontEnd,
    /// Writes to the storage.
    pub store: Store,
    // ---- state of the tools
    /// The emulator screen (focus, keyboard).
    pub screen: crate::screen::ScreenState,
    /// The debug console.
    pub console: crate::console::ConsoleState,
    /// The emu8086.inc library viewer.
    pub library: crate::library_ui::LibraryState,
    /// The reference.
    pub reference: crate::reference_ui::RefState,
    /// The calculator.
    pub calculator: crate::calc::CalcState,
    /// The number in the base converter.
    pub converter_text: String,
    /// Drive C: of DOS programs on the desktop (default [`crate::platform::vdrive_dir`]).
    pub drive_dir: PathBuf,
    /// State of the MIPS tools (console input, register being edited, selection).
    pub mips: crate::mips_ui::MipsState,
    /// The workspaces of the other processors (see [`App::switch_arch`]).
    pub(crate) parked: Vec<Workspace>,
}

/// An action that replaces the program in the editor (asked about unsaved changes first).
#[derive(Clone)]
pub enum PendingAction {
    /// New file: the "choose code template" dialog.
    New,
    /// Open a file (`None`: ask for it).
    Open(Option<PathBuf>),
    /// A built-in example by index.
    Example(usize),
    /// A built-in MIPS example by index ([`crate::examples::MIPS_EXAMPLES`]).
    MipsExample(usize),
    /// A program saved in the browser.
    Program(String),
    /// A file from the computer (the browser asks for it).
    Upload,
    /// A source uploaded from the computer, to keep in "my programs": its name and text.
    Import(String, String),
    /// A program from a link.
    Shared(crate::share::Shared),
    /// Quit the application.
    Quit,
}

impl App {
    /// The application with the settings and the browser storage of eframe.
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

    /// The application with `settings` (opens the last file or the first example).
    pub fn with_settings(mut settings: Settings) -> Self {
        let browser = FrontEnd::default().browser;
        let mut w = Workspace::new(Arch::I8086, &mut settings, browser);
        let mut app = App {
            settings,
            session: Session::new(),
            run: RunMode::Paused,
            status: String::new(),
            windows: Windows::default(),
            deferred_tools: None,
            panels: None,
            doc: Document::default(),
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
            mips: Default::default(),
            parked: Vec::new(),
        };
        app.swap_workspace(&mut w);
        app.reference.lang = app.settings.reference_lang;
        // The 8086 at every start (the MIPS workspace opens when asked for: the switch, a MIPS
        // file, link or example).
        app.open_start_document();
        app
    }

    /// Applies the theme setting (or the front end's own theme).
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

    /// The editor text differs from the saved one.
    pub fn dirty(&self) -> bool {
        self.doc.text != self.doc.saved_text
    }

    /// Name of the edited file (`untitled.asm` for a new one).
    pub fn file_name(&self) -> String {
        self.doc
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| self.doc.name.clone())
            .or_else(|| {
                let key = self.doc.example_key.as_ref()?;
                key.strip_prefix(files::EXAMPLE_KEY).or(key.strip_prefix(files::MIPS_EXAMPLE_KEY)).map(str::to_string)
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

    /// Runs `action` (see [`App::guarded`]).
    pub fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::New if self.arch() == Arch::Mips32 => self.new_mips_file(),
            PendingAction::New => self.dialogs.new_file = Some(0),
            PendingAction::Open(None) => self.open_dialog(),
            PendingAction::Open(Some(p)) => self.open_path(&p),
            PendingAction::Example(i) => self.load_example(i),
            PendingAction::MipsExample(i) => self.load_mips_example(i),
            PendingAction::Program(name) => self.open_program(&name),
            PendingAction::Upload => self.web.pending_upload = Some(crate::platform::UploadFor::Open),
            PendingAction::Import(name, text) => self.import_program(&name, text),
            PendingAction::Shared(s) => self.open_shared(s),
            PendingAction::Quit => {}
        }
    }

    /// Shows a message box.
    pub fn notice(&mut self, title: &str, text: &str) {
        self.dialogs.notice = Some((title.to_string(), text.to_string()));
    }
}
