//! Application state, menus, toolbar and the emulation loop.

use std::path::{Path, PathBuf};

use egui::{Key, KeyboardShortcut, Modifiers};
use rustemu_core::cpu::Cpu;
use rustemu_core::{Event, disasm};
use rustemu_session::expr::Expr;
use rustemu_session::{Session, StepGoal, StepKind};
use serde::{Deserialize, Serialize};

use crate::examples::EXAMPLES;

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
/// Storage key of the memory drive C: (see [`App::persist`]).
const DRIVE_KEY: &str = "dos_drive";

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
    pub screen_zoom: f32,
    pub show_macro_code: bool,
    pub follow_source: bool,
    pub vars_format: crate::debug::NumFormat,
    /// Assembler syntax: detected, Emu8086/MASM or NASM.
    pub syntax: SyntaxChoice,
    /// Debugging session of recently used files, most recent first.
    pub sessions: Vec<(String, FileSession)>,
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
            screen_zoom: 13.0,
            show_macro_code: true,
            follow_source: true,
            vars_format: Default::default(),
            syntax: SyntaxChoice::Auto,
            sessions: Vec::new(),
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
}

pub struct Message {
    pub text: String,
    /// (file name, line) the message refers to.
    pub loc: Option<(String, u32)>,
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
    /// A file to ask the browser for at the next frame (see `platform::request_upload`).
    pub pending_upload: Option<crate::platform::UploadFor>,
    /// File of the memory drive shown in the "drive C:" window.
    pub drive_preview: Option<String>,
    /// "delete all" in the "drive C:" window waits for confirmation.
    pub drive_confirm_clear: bool,
    pub text: String,
    pub saved_text: String,
    pub messages: Vec<Message>,
    /// Error lines of the current file (1-based).
    pub error_lines: Vec<u32>,
    /// A build/load has failed since the last successful emulator load. The
    /// previous machine remains available for inspection, but Run must not
    /// start it by mistake.
    build_failed: bool,
    pub goto_line: Option<u32>,
    pub cursor_line: u32,
    // ---- emulator
    pub session: Session,
    pub windows: Windows,
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
    /// Colour of the current instruction in the editor (default: [`crate::colors::info`]).
    pub current_line_color: Option<egui::Color32>,
    /// The tablet layout is shown (set by the tablet front end).
    pub tablet_layout: bool,
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
    Quit,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
        let mut app = Self::with_settings(settings);
        if let Some(storage) = cc.storage {
            app.restore_drive(storage);
        }
        crate::fonts::install(&cc.egui_ctx);
        app.apply_theme(&cc.egui_ctx);
        app
    }

    pub fn with_settings(settings: Settings) -> Self {
        let mut app = App {
            settings,
            path: None,
            doc_name: None,
            pending_upload: None,
            drive_preview: None,
            drive_confirm_clear: false,
            text: String::new(),
            saved_text: String::new(),
            messages: Vec::new(),
            error_lines: Vec::new(),
            build_failed: false,
            goto_line: None,
            cursor_line: 1,
            session: Session::new(),
            windows: Windows::default(),
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
            current_line_color: None,
            tablet_layout: false,
            can_switch_layout: false,
            switch_layout: false,
        };
        app.windows = app.settings.windows.clone();
        app.windows.about = false;
        app.screen.font_size = app.settings.screen_zoom.clamp(10.0, 22.0);
        app.show_macro_code = app.settings.show_macro_code;
        app.follow_source = app.settings.follow_source;
        app.vars_format = app.settings.vars_format;
        let last = app.settings.last_file.clone();
        match last {
            Some(p) if p.exists() => app.open_path(&p),
            _ => app.load_example(0),
        }
        // The browser has no file system: DOS programs get a drive in memory.
        if crate::platform::is_web() {
            app.session.machine.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
        }
        app
    }

    /// Restores the memory drive C: saved by [`App::persist`] (the browser keeps it in local
    /// storage).
    pub fn restore_drive(&mut self, storage: &dyn eframe::Storage) {
        use base64::Engine;
        if self.session.machine.dos_memory_drive().is_none() {
            return;
        }
        let saved = storage
            .get_string(DRIVE_KEY)
            .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
            .and_then(|b| rustemu_core::memdrive::MemDrive::from_bytes(&b));
        if let Some(drive) = saved {
            self.session.machine.set_dos_memory_drive(drive);
        }
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
            .unwrap_or_else(|| "untitled.asm".into())
    }

    // ---- files ------------------------------------------------------------------

    pub fn new_from_template(&mut self, i: usize) {
        self.new_from(crate::examples::TEMPLATES.get(i).map(|t| t.1).unwrap_or(""));
    }

    /// File -> New with "NASM syntax": COM (0), BIN (2) or boot sector (3) template in NASM.
    pub fn new_from_nasm_template(&mut self, i: usize) {
        let t = match i {
            0 => crate::examples::NASM_TEMPLATES[0].1,
            2 => crate::examples::NASM_TEMPLATES[1].1,
            3 => crate::examples::NASM_TEMPLATES[2].1,
            _ => "bits 16\n",
        };
        self.new_from(t);
    }

    fn new_from(&mut self, template: &str) {
        self.store_session();
        self.path = None;
        self.doc_name = None;
        self.example_key = None;
        self.clear_session();
        self.bookmarks.clear();
        self.text = template.to_string();
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.error_lines.clear();
        self.status = "New file".into();
    }

    pub fn open_dialog(&mut self) {
        if crate::platform::is_web() {
            self.pending_upload = Some(crate::platform::UploadFor::Open);
            return;
        }
        let dir = self.path.as_ref().and_then(|p| p.parent());
        let filters: [crate::platform::Filter; 2] =
            [("Assembly", &["asm", "nasm", "inc", "txt"]), ("All files", &["*"])];
        if let Some(p) = crate::platform::pick_file(&filters, dir) {
            let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "com" | "exe" | "bin" | "boot") {
                self.load_binary(&p);
            } else {
                self.open_path(&p);
            }
        }
    }

    pub fn open_path(&mut self, p: &Path) {
        match std::fs::read(p) {
            Ok(bytes) => {
                self.store_session();
                self.text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                self.saved_text = self.text.clone();
                self.path = Some(p.to_path_buf());
                self.doc_name = None;
                self.messages.clear();
                self.error_lines.clear();
                self.add_recent(p);
                self.settings.last_file = Some(p.to_path_buf());
                self.example_key = None;
                self.restore_session();
                self.status = format!("Opened {}", p.display());
            }
            Err(e) => self.notice("Open", &format!("Cannot open {}:\n{e}", p.display())),
        }
    }

    /// Opens a file the user uploaded (the browser): an executable is loaded into the emulator,
    /// anything else goes into the editor.
    fn open_upload(&mut self, u: crate::platform::Upload) {
        let ext = Path::new(&u.name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "com" | "exe" | "bin" | "boot" | "img") {
            match self.session.load_named(&u.name, u.bytes) {
                Ok(()) => self.after_binary_load(),
                Err(e) => self.notice("Load", &e),
            }
            return;
        }
        self.store_session();
        self.text = String::from_utf8_lossy(&u.bytes).replace("\r\n", "\n");
        self.saved_text = self.text.clone();
        self.path = None;
        self.doc_name = Some(u.name.clone());
        self.example_key = None;
        self.messages.clear();
        self.error_lines.clear();
        self.status = format!("Opened {}", u.name);
    }

    /// Asks the browser for the requested file and opens the files that arrived.
    fn handle_uploads(&mut self, ctx: &egui::Context) {
        use crate::platform::UploadFor;
        if let Some(purpose) = self.pending_upload.take() {
            let filters: &[crate::platform::Filter] = match purpose {
                UploadFor::Open => &[
                    ("Assembly or executable", &["asm", "nasm", "inc", "txt", "com", "exe", "bin"]),
                    ("All files", &["*"]),
                ],
                UploadFor::DriveFiles => &[],
            };
            crate::platform::request_upload(ctx, purpose, filters);
        }
        for u in crate::platform::take_uploads() {
            match u.purpose {
                UploadFor::Open => self.open_upload(u),
                UploadFor::DriveFiles => self.add_drive_file(&u.name, u.bytes),
            }
        }
    }

    /// Puts a file on the memory drive C: (an upload for DOS programs).
    pub fn add_drive_file(&mut self, name: &str, bytes: Vec<u8>) {
        let Some(drive) = self.session.machine.dos_memory_drive_mut() else { return };
        match drive.write(name, bytes) {
            Ok(key) => self.status = format!("C:\\{} added to drive C:", key.replace('/', "\\")),
            Err(e) => self.notice("Drive C:", &e),
        }
    }

    fn add_recent(&mut self, p: &Path) {
        self.settings.recent.retain(|r| r != p);
        self.settings.recent.insert(0, p.to_path_buf());
        self.settings.recent.truncate(10);
    }

    pub fn save(&mut self) -> bool {
        if crate::platform::is_web() {
            return self.download_source();
        }
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        if crate::platform::is_web() {
            return self.download_source();
        }
        let dir = self.path.as_ref().and_then(|p| p.parent());
        match crate::platform::save_file(&[("Assembly", &["asm"])], dir, &self.file_name()) {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    /// Saves the editor text as a download (the browser).
    fn download_source(&mut self) -> bool {
        let name = self.file_name();
        crate::platform::download(&name, self.text.as_bytes());
        self.saved_text = self.text.clone();
        self.status = format!("Downloaded {name}");
        true
    }

    fn save_to(&mut self, p: &Path) -> bool {
        match std::fs::write(p, &self.text) {
            Ok(()) => {
                self.example_key = None;
                self.saved_text = self.text.clone();
                self.path = Some(p.to_path_buf());
                self.add_recent(p);
                self.settings.last_file = Some(p.to_path_buf());
                self.status = format!("Saved {}", p.display());
                true
            }
            Err(e) => {
                self.notice("Save", &format!("Cannot save {}:\n{e}", p.display()));
                false
            }
        }
    }

    pub fn load_example(&mut self, i: usize) {
        let (name, src) = EXAMPLES[i];
        self.store_session();
        self.path = None;
        self.doc_name = None;
        self.example_key = Some(format!("example:{name}"));
        self.text = src.to_string();
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.error_lines.clear();
        self.restore_session();
        self.status = format!("Example: {name}");
    }

    // ---- session (per-file debugging state) ---------------------------------------------

    /// Key under which the current file's session is stored (None for a new unsaved file).
    pub fn session_key(&self) -> Option<String> {
        match &self.path {
            Some(p) => Some(p.to_string_lossy().into_owned()),
            None => self.example_key.clone(),
        }
    }

    /// The current file's breakpoints, watches, bookmarks and stop condition.
    pub fn capture_session(&self) -> FileSession {
        let name = |f: usize| {
            if f == 0 { String::new() } else { self.source_files.get(f).map(|s| s.0.clone()).unwrap_or_default() }
        };
        let mut breakpoints: Vec<SavedBreakpoint> = self
            .bp_lines
            .iter()
            .filter(|&(&(f, _), _)| f == 0 || f < self.source_files.len())
            .map(|(&(f, line), bp)| SavedBreakpoint {
                file: name(f),
                line,
                condition: bp.condition.clone(),
                enabled: bp.enabled,
            })
            .collect();
        breakpoints.extend(self.pending_breakpoints.iter().cloned());
        let sc = &self.stop_condition;
        FileSession {
            breakpoints,
            watches: self.watches.iter().map(|w| w.text.clone()).collect(),
            bookmarks: self.bookmarks.iter().copied().collect(),
            stop_condition: sc.text.clone(),
            stop_enabled: sc.enabled,
            stop_on_change: sc.on_change,
            cursor_line: self.cursor_line,
        }
    }

    /// Remembers the current file's session in the settings.
    pub fn store_session(&mut self) {
        if !self.session_started {
            return;
        }
        let Some(key) = self.session_key() else { return };
        let s = self.capture_session();
        let list = &mut self.settings.sessions;
        list.retain(|(k, _)| *k != key);
        let empty = s == FileSession { cursor_line: s.cursor_line, ..Default::default() };
        if !empty || s.cursor_line > 1 {
            list.insert(0, (key, s));
            list.truncate(MAX_SESSIONS);
        }
    }

    fn clear_session(&mut self) {
        self.bp_lines.clear();
        self.pending_breakpoints.clear();
        self.watches.clear();
        self.bookmarks.clear();
        self.stop_condition = StopCondition::default();
        self.sync_breakpoints();
        self.session_started = true;
    }

    /// Restores the session saved for the current file (or starts an empty one).
    pub fn restore_session(&mut self) {
        self.clear_session();
        let Some(key) = self.session_key() else { return };
        let Some(s) = self.settings.sessions.iter().find(|(k, _)| *k == key).map(|(_, s)| s.clone()) else { return };
        for b in s.breakpoints {
            if b.file.is_empty() {
                let bp = Breakpoint { condition: b.condition, enabled: b.enabled, hits: 0, compiled: None };
                self.bp_lines.insert((0, b.line), bp);
            } else {
                self.pending_breakpoints.push(b);
            }
        }
        self.watches = s.watches.into_iter().map(|text| Watch { text, compiled: None, before: None }).collect();
        self.bookmarks = s.bookmarks.into_iter().collect();
        self.stop_condition = StopCondition {
            text: s.stop_condition,
            enabled: s.stop_enabled,
            on_change: s.stop_on_change,
            ..Default::default()
        };
        if s.cursor_line > 1 {
            self.goto_line = Some(s.cursor_line);
        }
        self.resolve_pending_breakpoints();
        self.sync_breakpoints();
    }

    /// Turns saved breakpoints of included files into line breakpoints once the program is loaded.
    fn resolve_pending_breakpoints(&mut self) {
        let pending = std::mem::take(&mut self.pending_breakpoints);
        for b in pending {
            match self.source_files.iter().position(|(n, _)| n.eq_ignore_ascii_case(&b.file)) {
                Some(f) if f > 0 => {
                    let bp = Breakpoint { condition: b.condition, enabled: b.enabled, hits: 0, compiled: None };
                    self.bp_lines.insert((f, b.line), bp);
                }
                _ => self.pending_breakpoints.push(b),
            }
        }
    }

    /// Copies the window and debugging state into the settings (before they are saved).
    pub fn store_settings(&mut self) {
        self.store_session();
        self.settings.windows = self.windows.clone();
        self.settings.screen_zoom = self.screen.font_size;
        self.settings.show_macro_code = self.show_macro_code;
        self.settings.follow_source = self.follow_source;
        self.settings.vars_format = self.vars_format;
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
            PendingAction::Quit => {}
        }
    }

    pub fn notice(&mut self, title: &str, text: &str) {
        self.notice = Some((title.to_string(), text.to_string()));
    }

    // ---- compile / emulate ----------------------------------------------------------------

    fn assemble(&mut self) -> Option<rustemu_asm::Program> {
        let name = self.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| self.file_name());
        let mut loader = rustemu_asm::FsLoader { include_dirs: Vec::new() };
        self.messages.clear();
        self.error_lines.clear();
        match rustemu_asm::assemble_dialect(&name, &self.text, &mut loader, self.settings.syntax.dialect()) {
            Ok(p) => {
                self.messages.push(Message {
                    text: format!(
                        "{}: assembled successfully - {} syntax, {} format, {} bytes",
                        self.file_name(),
                        p.dialect.name(),
                        p.format.name(),
                        p.file_bytes().len()
                    ),
                    loc: None,
                    error: false,
                });
                Some(p)
            }
            Err(e) => {
                self.build_failed = true;
                for err in &e.errors {
                    let file = e.files.get(err.file).cloned().unwrap_or_default();
                    let short =
                        Path::new(&file).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or(file.clone());
                    if err.file == 0 && err.line > 0 {
                        self.error_lines.push(err.line);
                    }
                    self.messages.push(Message {
                        text: if err.line > 0 {
                            format!("{short} ({}): {}", err.line, err.msg)
                        } else {
                            err.msg.clone()
                        },
                        loc: (err.line > 0 && err.file == 0).then_some((file, err.line)),
                        error: true,
                    });
                }
                self.messages.push(Message { text: format!("{} error(s)", e.errors.len()), loc: None, error: true });
                if let Some(l) = self.error_lines.first() {
                    self.goto_line = Some(*l);
                }
                self.status = "Assembly failed".into();
                None
            }
        }
    }

    /// Compile and write the output file next to the source.
    pub fn compile(&mut self) {
        let Some(p) = self.assemble() else { return };
        if crate::platform::is_web() {
            let stem = self.file_name();
            let stem = stem.rsplit_once('.').map_or(stem.as_str(), |(s, _)| s);
            let name = format!("{stem}.{}", p.format.extension());
            crate::platform::download(&name, &p.file_bytes());
            self.messages.push(Message { text: format!("downloaded: {name}"), loc: None, error: false });
            self.status = format!("Compiled to {name}");
            return;
        }
        let out = match &self.path {
            Some(src) => src.with_extension(p.format.extension()),
            None => match crate::platform::save_file(&[], None, &format!("program.{}", p.format.extension())) {
                Some(f) => f,
                None => return,
            },
        };
        let mut written = vec![];
        let res = std::fs::write(&out, p.file_bytes()).and_then(|_| {
            written.push(out.clone());
            let lst = out.with_extension("lst");
            std::fs::write(&lst, p.listing_text())?;
            written.push(lst);
            let sym = out.with_extension("symbol");
            std::fs::write(&sym, p.symbols_text())?;
            written.push(sym);
            Ok(())
        });
        match res {
            Ok(()) => {
                for w in written {
                    self.messages.push(Message { text: format!("written: {}", w.display()), loc: None, error: false });
                }
                self.status = format!("Compiled to {}", out.display());
            }
            Err(e) => self.notice("Compile", &format!("Cannot write {}:\n{e}", out.display())),
        }
    }

    /// Assemble and load into the emulator.
    pub fn emulate(&mut self) {
        let Some(p) = self.assemble() else { return };
        let source_files: Vec<(String, Vec<String>)> = p
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let text = if i == 0 {
                    self.text.clone()
                } else if f == "emu8086.inc" {
                    rustemu_asm::EMU8086_INC.to_string()
                } else {
                    std::fs::read_to_string(f).unwrap_or_default()
                };
                let name = Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(f.clone());
                (name, text.lines().map(str::to_string).collect())
            })
            .collect();
        let source_index =
            source_files.iter().map(|(_, lines)| rustemu_asm::library::index(&lines.join("\n"))).collect();
        let devices = p.emu.start.clone();
        let root = self.vdrive();
        let same_image = self.session.program.as_ref().is_some_and(|old| old.file_bytes() == p.file_bytes());
        if let Err(e) = self.session.load_program(p) {
            self.build_failed = true;
            self.notice("Load", &e);
            return;
        }
        self.source_files = source_files;
        self.source_index = source_index;
        self.loaded_text = Some(self.text.clone());
        if let Some(root) = root {
            self.session.machine.set_dos_root(root);
        }
        if !same_image {
            self.clear_address_breakpoints();
        }
        self.after_load();
        if let Some(d) = devices.iter().find_map(|n| crate::devices_ui::Device::from_start_name(n)) {
            self.device_tab = d;
            self.windows.devices = true;
        }
    }

    /// Loads a binary executable without source.
    pub fn load_binary(&mut self, p: &Path) {
        match self.session.load_path(p) {
            Ok(()) => self.after_binary_load(),
            Err(e) => self.notice("Load", &e),
        }
    }

    /// After loading code without debug information (binary file, boot from floppy).
    pub fn after_binary_load(&mut self) {
        self.clear_address_breakpoints();
        self.loaded_text = None;
        self.source_files.clear();
        self.source_index.clear();
        self.after_load();
    }

    /// Removes breakpoints set by address in the disassembly: they belong to the previous image.
    /// Line breakpoints are mapped again by [`App::sync_breakpoints`].
    fn clear_address_breakpoints(&mut self) {
        let lines = &self.bp_addr;
        self.session.machine.breakpoints.retain(|a| lines.contains_key(a));
    }

    fn after_load(&mut self) {
        self.build_failed = false;
        self.session.machine.timer_irq = self.settings.timer_irq;
        self.load_gen += 1;
        for w in &mut self.watches {
            w.compiled = None;
        }
        self.stop_condition.compiled = None;
        self.resolve_pending_breakpoints();
        self.sync_breakpoints();
        self.run = RunMode::Paused;
        self.begin_exec();
        self.source_file_idx = 0;
        self.windows.emulator = true;
        self.windows.screen = true;
        self.windows.source = !self.source_files.is_empty();
        self.screen.focus_request = true;
        let c = &self.session.machine.cpu;
        self.mem_view_addr = rustemu_core::linear(c.sregs[rustemu_core::cpu::DS], 0);
        self.status = format!("Loaded - CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// The directory DOS file functions see as drive C:; None for the memory drive (the browser).
    fn vdrive(&self) -> Option<PathBuf> {
        if self.session.machine.dos_memory_drive().is_some() {
            return None;
        }
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            return Some(dir.to_path_buf());
        }
        let d = crate::platform::data_dir().join("vdrive");
        std::fs::create_dir_all(&d).ok();
        Some(d)
    }

    pub fn reload(&mut self) {
        match self.session.reload() {
            Ok(()) => self.after_load(),
            Err(e) => self.notice("Reload", &e),
        }
    }

    pub fn loaded(&self) -> bool {
        self.session.machine.loaded.is_some()
    }

    /// The loaded program was assembled from a different version of the editor text.
    pub fn is_stale(&self) -> bool {
        self.loaded() && self.loaded_text.as_ref().is_some_and(|t| *t != self.text)
    }

    // ---- execution -----------------------------------------------------------------------

    /// Execution commands run the current build: after a failed build they try to build again
    /// instead of running the old image (it stays loaded for inspection).
    fn ensure_current_build(&mut self) -> bool {
        if self.loaded() && !self.build_failed {
            return true;
        }
        self.emulate();
        // `emulate` leaves the assembly error in `status`.
        self.loaded() && !self.build_failed
    }

    pub fn single_step(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        self.run = RunMode::Paused;
        self.begin_exec();
        let ev = self.session.machine.step();
        self.handle_event(ev);
    }

    pub fn step_back(&mut self) {
        self.run = RunMode::Paused;
        self.begin_exec();
        if self.session.machine.step_back() {
            self.status = "Stepped back".into();
        } else {
            self.status = "No more history".into();
        }
    }

    /// Step Over: runs the current source line (including macros and called procedures).
    /// Without debug information: runs CALL / INT / LOOP / REP to the next instruction.
    pub fn step_over(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        if self.session.program.is_some() {
            self.begin_exec();
            self.run = RunMode::Source(self.session.step_goal(StepKind::Over, self.show_macro_code));
            self.status = "Stepping over...".into();
            return;
        }
        let m = &self.session.machine;
        let pc = m.cpu.pc();
        let bytes = m.sys.mem.slice(pc, 8).to_vec();
        let ins = disasm::disassemble(&bytes, m.cpu.ip);
        let t = ins.text.as_str();
        let over = t.starts_with("CALL") || t.starts_with("INT") || t.starts_with("LOOP") || t.starts_with("REP");
        if over {
            let next = rustemu_core::linear(m.cpu.cs(), m.cpu.ip.wrapping_add(ins.len as u16));
            self.begin_exec();
            self.run = RunMode::RunTo(vec![next]);
        } else {
            self.single_step();
        }
    }

    /// Step Out: runs until the current macro or procedure returns to its caller.
    pub fn step_out(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        self.begin_exec();
        self.run = RunMode::Source(self.session.step_goal(StepKind::Out, self.show_macro_code));
        self.status = "Stepping out...".into();
    }

    pub fn start_run(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        if self.session.machine.terminated.is_some() {
            self.status = "Program has terminated - press Reload to run again".into();
            return;
        }
        self.begin_exec();
        self.run = RunMode::Running;
        self.status = "Running...".into();
    }

    /// F4: runs until execution reaches the line with the text cursor (or a breakpoint).
    pub fn run_to_cursor(&mut self) {
        self.run_to_line(0, self.cursor_line);
    }

    /// Runs until execution reaches `line` of source file `file` (the next line with code).
    pub fn run_to_line(&mut self, file: usize, line: u32) {
        // A binary without source is (re)built from the editor text first.
        let ok = if self.session.program.is_none() {
            self.emulate();
            self.loaded() && !self.build_failed
        } else {
            self.ensure_current_build()
        };
        if !ok || self.session.program.is_none() {
            return;
        }
        if file == 0 && self.is_stale() {
            self.status = "The source was changed - reassemble (F9) before running to a line".into();
            return;
        }
        if self.session.machine.terminated.is_some() {
            self.status = "Program has terminated - press Reload to run again".into();
            return;
        }
        let Some(l) = (line..line + 20).find(|&l| self.session.addr_of_line(file, l).is_some()) else {
            self.status = format!("No code at line {line}");
            return;
        };
        self.begin_exec();
        self.run = RunMode::RunTo(self.session.addrs_of_line(file, l));
        self.status = format!("Running to line {l}...");
    }

    /// Remembers the state before an execution command (changed registers and watches are highlighted).
    pub fn begin_exec(&mut self) {
        self.prev_cpu = self.session.machine.cpu.clone();
        self.code_view_pinned = false;
        for i in 0..self.watches.len() {
            self.watches[i].before = self.watch_value(i).ok();
        }
        let v = self.stop_value();
        self.stop_condition.last = v;
    }

    /// Current value of watch `i`.
    pub fn watch_value(&mut self, i: usize) -> Result<i64, String> {
        let w = &mut self.watches[i];
        if w.compiled.is_none() {
            w.compiled = Some(self.session.compile(&w.text));
        }
        match w.compiled.as_ref().unwrap() {
            Ok(e) => Ok(e.eval(&self.session.machine)),
            Err(e) => Err(e.clone()),
        }
    }

    /// Value of the stop condition, when it is enabled and valid.
    fn stop_value(&mut self) -> Option<i64> {
        let sc = &mut self.stop_condition;
        if !sc.enabled || sc.text.trim().is_empty() || self.session.machine.loaded.is_none() {
            return None;
        }
        if sc.compiled.is_none() {
            sc.compiled = Some(self.session.compile(&sc.text));
        }
        sc.compiled.as_ref().unwrap().as_ref().ok().map(|e| e.eval(&self.session.machine))
    }

    fn stop_condition_active(&self) -> bool {
        let sc = &self.stop_condition;
        sc.enabled && !sc.text.trim().is_empty() && !matches!(sc.compiled, Some(Err(_)))
    }

    /// Checks the stop condition after an instruction; stops execution when it is met.
    fn check_stop_condition(&mut self) -> bool {
        let Some(v) = self.stop_value() else { return false };
        let last = self.stop_condition.last.replace(v);
        let met = if self.stop_condition.on_change { last.is_some_and(|l| l != v) } else { v != 0 && last == Some(0) };
        if met {
            self.run = RunMode::Paused;
            let c = &self.session.machine.cpu;
            self.status = if self.stop_condition.on_change {
                format!(
                    "Stopped: {} changed from {} to {v} at {:04X}:{:04X}",
                    self.stop_condition.text,
                    last.unwrap_or(0),
                    c.cs(),
                    c.ip
                )
            } else {
                format!("Stopped: {} is true at {:04X}:{:04X}", self.stop_condition.text, c.cs(), c.ip)
            };
        }
        met
    }

    /// Decides whether execution that reached a breakpoint address should stop there: the line
    /// breakpoint's condition must hold. Counts the hit.
    fn breakpoint_hit(&mut self, pc: u32) -> bool {
        let Some(key) = self.bp_addr.get(&pc).copied() else { return true };
        let Some(bp) = self.bp_lines.get_mut(&key) else { return true };
        if bp.compiled.is_none() && !bp.condition.trim().is_empty() {
            bp.compiled = Some(self.session.compile(&bp.condition));
        }
        let stop = match &bp.compiled {
            Some(Ok(e)) if !bp.condition.trim().is_empty() => e.eval(&self.session.machine) != 0,
            _ => true,
        };
        if stop {
            bp.hits += 1;
        }
        stop
    }

    /// Is there a breakpoint at the current position that should stop execution?
    fn at_breakpoint(&mut self) -> bool {
        let pc = self.session.machine.cpu.pc();
        self.session.machine.breakpoints.contains(&pc) && self.breakpoint_hit(pc)
    }

    pub fn stop(&mut self) {
        if self.run != RunMode::Paused {
            self.run = RunMode::Paused;
            self.status =
                format!("Stopped at {:04X}:{:04X}", self.session.machine.cpu.cs(), self.session.machine.cpu.ip);
        }
    }

    pub fn handle_event(&mut self, ev: Event) {
        let m = &self.session.machine;
        let at = format!("{:04X}:{:04X}", m.cpu.cs(), m.cpu.ip);
        match ev {
            Event::Continue | Event::Budget => {
                if self.run == RunMode::Paused {
                    self.status = format!("CS:IP = {at}");
                }
            }
            Event::Sleeping => {
                self.status = "Waiting (INT 15h/86h)...".into();
            }
            Event::WaitingInput => {
                self.status = "Waiting for keyboard input - type in the emulator screen".into();
                self.screen.focus_request = true;
                self.windows.screen = true;
            }
            Event::Halted => {
                self.run = RunMode::Paused;
                self.status = format!("HLT executed at {at}");
            }
            Event::Terminated(code) => {
                self.run = RunMode::Paused;
                self.status = format!("Program terminated (exit code {code})");
                self.messages.push(Message { text: self.status.clone(), loc: None, error: false });
            }
            Event::Breakpoint => {
                self.run = RunMode::Paused;
                self.status = format!("Breakpoint at {at}");
            }
            Event::Error(e) => {
                self.run = RunMode::Paused;
                self.status = format!("Error at {at}: {e}");
                self.notice("Emulator", &format!("{e}\n\nat {at}"));
            }
        }
    }

    /// Advances the emulation for this frame.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.handle_uploads(ctx);
        if self.run == RunMode::Paused || !self.loaded() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let waiting = self.session.machine.is_waiting_input() && self.session.machine.sys.keys.is_empty();
        if waiting {
            // Nothing to do until a key arrives; keep repainting slowly.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        // Emulation time per frame.
        let frame = std::time::Duration::from_millis(12);
        let start = web_time::Instant::now();
        match self.run.clone() {
            RunMode::Running if self.settings.step_delay_ms > 0 => {
                if (now - self.last_step) * 1000.0 >= self.settings.step_delay_ms as f64 {
                    self.last_step = now;
                    self.prev_cpu = self.session.machine.cpu.clone();
                    let ev = self.session.machine.step();
                    self.handle_event(ev);
                    if self.run != RunMode::Paused && self.at_breakpoint() {
                        self.handle_event(Event::Breakpoint);
                    } else if self.run != RunMode::Paused {
                        self.check_stop_condition();
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(self.settings.step_delay_ms.min(30) as u64));
            }
            RunMode::Running if self.stop_condition_active() => {
                // The condition is checked after every instruction.
                'run: while start.elapsed() < frame {
                    for _ in 0..4096 {
                        let ev = self.session.machine.step();
                        if ev != Event::Continue {
                            self.handle_event(ev);
                            break 'run;
                        }
                        if self.at_breakpoint() {
                            self.handle_event(Event::Breakpoint);
                            break 'run;
                        }
                        if self.check_stop_condition() {
                            break 'run;
                        }
                    }
                }
                ctx.request_repaint();
            }
            RunMode::Running => {
                loop {
                    let t = web_time::Instant::now();
                    let ev = self.session.machine.run(self.instr_per_frame);
                    let el = t.elapsed().as_secs_f64();
                    if ev == Event::Budget {
                        // Aim at ~12 ms of emulation per frame.
                        if el < 0.008 {
                            self.instr_per_frame = (self.instr_per_frame * 3 / 2).min(20_000_000);
                        } else if el > 0.016 {
                            self.instr_per_frame = (self.instr_per_frame * 2 / 3).max(1000);
                        }
                    }
                    // A conditional breakpoint whose condition is false: keep going.
                    if ev == Event::Breakpoint && !self.breakpoint_hit(self.session.machine.cpu.pc()) {
                        if start.elapsed() < frame {
                            continue;
                        }
                        break;
                    }
                    self.handle_event(ev);
                    break;
                }
                ctx.request_repaint();
            }
            RunMode::RunTo(targets) => {
                'run: while start.elapsed() < frame {
                    for _ in 0..4096 {
                        let ev = self.session.machine.step();
                        if ev != Event::Continue {
                            self.handle_event(ev);
                            break 'run;
                        }
                        let pc = self.session.machine.cpu.pc();
                        if targets.contains(&pc) {
                            self.run = RunMode::Paused;
                            let c = &self.session.machine.cpu;
                            self.status = format!("CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
                            break 'run;
                        }
                        if self.at_breakpoint() {
                            self.handle_event(Event::Breakpoint);
                            break 'run;
                        }
                        if self.stop_condition.enabled && self.check_stop_condition() {
                            break 'run;
                        }
                    }
                }
                ctx.request_repaint();
            }
            RunMode::Source(mut goal) => {
                loop {
                    match self.session.run_step_goal(&mut goal, self.instr_per_frame.max(10_000)) {
                        None => self.run = RunMode::Source(goal.clone()),
                        Some(Event::Continue) => {
                            self.run = RunMode::Paused;
                            let c = &self.session.machine.cpu;
                            self.status = format!("CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
                        }
                        Some(Event::Breakpoint) if !self.breakpoint_hit(self.session.machine.cpu.pc()) => {
                            self.run = RunMode::Source(goal.clone());
                            if start.elapsed() < frame {
                                continue;
                            }
                        }
                        Some(ev @ (Event::WaitingInput | Event::Sleeping)) => {
                            self.run = RunMode::Source(goal.clone());
                            self.handle_event(ev);
                        }
                        Some(ev) => self.handle_event(ev),
                    }
                    break;
                }
                ctx.request_repaint();
            }
            RunMode::Paused => {}
        }
    }

    /// Adds or removes a breakpoint. With a program loaded, a line without code puts the
    /// breakpoint on the next line that has code.
    pub fn toggle_breakpoint_at_line(&mut self, file: usize, line: u32) {
        if self.bp_lines.remove(&(file, line)).is_some() {
            self.sync_breakpoints();
            return;
        }
        let mapped = self.loaded() && self.session.program.is_some() && !(file == 0 && self.is_stale());
        let line = if mapped {
            match (line..line + 20).find(|&l| self.session.addr_of_line(file, l).is_some()) {
                Some(l) => l,
                None => {
                    self.status = format!("No code on line {line}");
                    return;
                }
            }
        } else {
            line
        };
        self.bp_lines.entry((file, line)).or_default();
        self.sync_breakpoints();
    }

    /// Breakpoint on the editor line with the text cursor.
    /// Breakpoint at an instruction of the disassembly: a line breakpoint when the address maps to a
    /// program line (it survives reloads); macro code keeps an address breakpoint (its line may be
    /// expanded in many places).
    pub fn toggle_breakpoint_at_addr(&mut self, addr: u32) {
        if let Some(loc) = self.session.location_at_addr(addr).filter(|l| l.origin.is_none())
            && self.session.addr_of_line(loc.file, loc.line) == Some(addr)
        {
            self.toggle_breakpoint_at_line(loc.file, loc.line);
        } else {
            let bps = &mut self.session.machine.breakpoints;
            if !bps.remove(&addr) {
                bps.insert(addr);
            }
        }
    }

    // ---- emulator window menus (Emu8086) ---------------------------------------------------

    /// Linear address of the instruction selected in the disassembly.
    pub fn selected_addr(&self) -> Option<u32> {
        self.code_selected.map(|(s, o)| rustemu_core::linear(s, o))
    }

    /// Debug -> run until: runs to the selected instruction (or a breakpoint).
    pub fn run_until_selected(&mut self) {
        let Some(addr) = self.selected_addr() else { return };
        if !self.ensure_current_build() {
            return;
        }
        if self.session.machine.terminated.is_some() {
            self.status = "Program has terminated - press Reload to run again".into();
            return;
        }
        self.begin_exec();
        self.run = RunMode::RunTo(vec![addr]);
        self.status = format!("Running until {addr:05X}h...");
    }

    /// Debug -> set CS:IP to selected position.
    pub fn set_cs_ip_to_selected(&mut self) {
        let Some((seg, off)) = self.code_selected else { return };
        self.stop();
        let c = &mut self.session.machine.cpu;
        c.sregs[rustemu_core::cpu::CS] = seg;
        c.ip = off;
        c.halted = false;
        self.code_view_pinned = false;
        self.status = format!("CS:IP = {seg:04X}:{off:04X}");
    }

    /// Debug -> show current instruction: the disassembly follows CS:IP again.
    pub fn show_current_instruction(&mut self) {
        self.code_view_pinned = false;
        self.code_view_start = self.session.machine.cpu.pc();
    }

    /// Debug -> show current break point: shows the next breakpoint after the shown one.
    pub fn show_next_breakpoint(&mut self) {
        let mut bps: Vec<u32> = self.session.machine.breakpoints.iter().copied().collect();
        bps.sort_unstable();
        let from = if self.code_view_pinned { self.code_view_start + 1 } else { 0 };
        let Some(&a) = bps.iter().find(|&&a| a >= from).or(bps.first()) else { return };
        self.code_view_start = a;
        self.code_view_pinned = true;
        let cs = self.session.machine.cpu.cs();
        let off = a.wrapping_sub(rustemu_core::linear(cs, 0));
        self.code_selected = Some(if off <= 0xFFFF { (cs, off as u16) } else { ((a >> 4) as u16, (a & 15) as u16) });
    }

    /// File -> save the emulator's state (kept until the application is closed).
    pub fn save_emulator_state(&mut self) {
        self.saved_state = Some(SavedState {
            session: self.session.save_state(),
            source_files: self.source_files.clone(),
            source_index: self.source_index.clone(),
            loaded_text: self.loaded_text.clone(),
        });
        let c = &self.session.machine.cpu;
        self.status = format!("Emulator state saved at {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// File -> load from previous state.
    pub fn load_emulator_state(&mut self) {
        let Some(s) = self.saved_state.clone() else { return };
        self.stop();
        self.session.restore_state(s.session);
        self.source_files = s.source_files;
        self.source_index = s.source_index;
        self.loaded_text = s.loaded_text;
        self.build_failed = false;
        self.load_gen += 1;
        for w in &mut self.watches {
            w.compiled = None;
        }
        self.stop_condition.compiled = None;
        self.sync_breakpoints();
        self.prev_cpu = self.session.machine.cpu.clone();
        self.code_view_pinned = false;
        let c = &self.session.machine.cpu;
        self.status = format!("Emulator state restored - CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// File -> reset emulator and RAM: nothing loaded, memory cleared.
    pub fn reset_emulator_and_ram(&mut self) {
        self.run = RunMode::Paused;
        self.session.unload();
        self.source_files.clear();
        self.source_index.clear();
        self.loaded_text = None;
        self.bp_addr.clear();
        self.prev_cpu = self.session.machine.cpu.clone();
        self.code_selected = None;
        self.code_view_pinned = false;
        self.status = "Emulator and RAM reset".into();
    }

    /// File -> examples / recent files: opens the source and loads it into the emulator.
    pub fn emulate_example(&mut self, i: usize) {
        if self.dirty() {
            self.guarded(PendingAction::Example(i));
        } else {
            self.load_example(i);
            self.emulate();
        }
    }

    pub fn emulate_file(&mut self, p: &Path) {
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "com" | "exe" | "bin" | "boot") {
            self.load_binary(p);
        } else if self.dirty() {
            self.guarded(PendingAction::Open(Some(p.to_path_buf())));
        } else {
            self.open_path(p);
            self.emulate();
        }
    }

    /// File -> close the emulator.
    pub fn close_emulator(&mut self) {
        self.stop();
        self.windows.emulator = false;
    }

    pub fn toggle_breakpoint_at_cursor(&mut self) {
        self.toggle_breakpoint_at_line(0, self.cursor_line);
    }

    /// Maps line breakpoints to addresses of the loaded program (address breakpoints set in the
    /// disassembly are kept). Outdated programs keep the old mapping until they are reassembled.
    pub fn sync_breakpoints(&mut self) {
        if self.is_stale() {
            return;
        }
        let bps = &mut self.session.machine.breakpoints;
        bps.retain(|a| !self.bp_addr.contains_key(a));
        self.bp_addr.clear();
        for (&(f, l), bp) in &mut self.bp_lines {
            bp.compiled = None;
            if !bp.enabled {
                continue;
            }
            for a in self.session.addrs_of_line(f, l) {
                self.bp_addr.insert(a, (f, l));
                self.session.machine.breakpoints.insert(a);
            }
        }
    }

    // ---- shortcuts ------------------------------------------------------------------------------

    /// F1: opens the reference at the instruction / directive / interrupt under the text cursor.
    pub fn help_at_cursor(&mut self) {
        match crate::reference::topic_at_cursor(&self.text, self.selection.0) {
            Some(t) => self.reference.show(t),
            None => self.reference.open = true,
        }
    }

    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let fire = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if fire(sc(cmd, Key::N)) {
            self.guarded(PendingAction::New);
        }
        if fire(sc(cmd, Key::O)) {
            self.guarded(PendingAction::Open(None));
        }
        if fire(sc(cmd | Modifiers::SHIFT, Key::S)) {
            self.save_as();
        } else if fire(sc(cmd, Key::S)) {
            self.save();
        }
        if fire(sc(Modifiers::NONE, Key::F5)) || fire(sc(cmd, Key::R)) {
            if self.run == RunMode::Paused { self.start_run() } else { self.stop() }
        }
        if fire(sc(Modifiers::NONE, Key::F9)) || fire(sc(cmd, Key::E)) {
            self.emulate();
        }
        if fire(sc(cmd, Key::B)) {
            self.compile();
        }
        if fire(sc(Modifiers::NONE, Key::F8)) && self.loaded() {
            self.single_step();
        }
        if fire(sc(Modifiers::SHIFT, Key::F8)) && self.loaded() {
            self.step_back();
        }
        if fire(sc(Modifiers::NONE, Key::F10)) && self.loaded() {
            self.step_over();
        }
        if fire(sc(Modifiers::SHIFT, Key::F11)) && self.loaded() {
            self.step_out();
        }
        // Editor commands are not taken while the program is typed into the emulator screen.
        if self.screen.focused {
            return;
        }
        let ctrl = Modifiers::CTRL;
        if fire(sc(cmd, Key::F)) || fire(sc(cmd, Key::H)) || fire(sc(ctrl, Key::H)) {
            self.find.open = true;
            self.find.focus = true;
        }
        if fire(sc(cmd | Modifiers::SHIFT, Key::G)) || fire(sc(Modifiers::SHIFT, Key::F3)) {
            crate::find::find_next(self, true);
        } else if fire(sc(cmd, Key::G)) || fire(sc(Modifiers::NONE, Key::F3)) {
            crate::find::find_next(self, false);
        }
        if fire(sc(cmd, Key::L)) || fire(sc(ctrl, Key::G)) {
            self.find.goto_open = true;
        }
        // F1: reference article for the word at the text cursor.
        if fire(sc(Modifiers::NONE, Key::F1)) {
            self.help_at_cursor();
        }
        // Emu8086 emulator keys that do not clash with ours.
        if fire(sc(Modifiers::NONE, Key::F11)) {
            self.windows.console = !self.windows.console;
        }
        if fire(sc(Modifiers::NONE, Key::F6)) && self.loaded() {
            self.step_back();
        }
        if fire(sc(ctrl | Modifiers::SHIFT, Key::F7)) {
            self.set_cs_ip_to_selected();
        } else if fire(sc(Modifiers::NONE, Key::F7)) {
            self.show_current_instruction();
        }
        if fire(sc(ctrl, Key::F8)) {
            self.run_until_selected();
        }
        if fire(sc(Modifiers::NONE, Key::F4)) {
            self.run_to_cursor();
        }
        if fire(sc(cmd, Key::F8)) {
            self.toggle_breakpoint_at_cursor();
        }
        if fire(sc(cmd, Key::Slash)) {
            crate::find::toggle_comment(self);
        }
        // Emu8086 editor keys (with the Control key).
        if fire(sc(ctrl, Key::Q)) {
            crate::editcmd::comment(self);
        }
        if fire(sc(ctrl, Key::W)) {
            crate::editcmd::uncomment(self);
        }
        if fire(sc(ctrl, Key::L)) {
            crate::editcmd::lowercase(self);
        }
        if fire(sc(ctrl, Key::U)) {
            crate::editcmd::uppercase(self);
        }
        if fire(sc(ctrl, Key::F2)) || fire(sc(cmd, Key::F2)) {
            crate::editcmd::toggle_bookmark(self);
        } else if fire(sc(Modifiers::SHIFT, Key::F2)) {
            crate::editcmd::prev_bookmark(self);
        } else if fire(sc(Modifiers::NONE, Key::F2)) {
            crate::editcmd::next_bookmark(self);
        }
    }
}

impl App {
    /// Saves the settings (`eframe::App::save` of the front ends).
    pub fn persist(&mut self, storage: &mut dyn eframe::Storage) {
        use base64::Engine;
        self.store_settings();
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
        if let Some(drive) = self.session.machine.dos_memory_drive() {
            storage.set_string(DRIVE_KEY, base64::engine::general_purpose::STANDARD.encode(drive.to_bytes()));
        }
    }

    /// Asks to save unsaved changes when the window is being closed.
    pub fn confirm_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && self.confirm_discard.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_discard = Some(PendingAction::Quit);
        }
    }
}
