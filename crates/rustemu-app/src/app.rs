//! Application state, menus, toolbar and the emulation loop.

use std::path::{Path, PathBuf};

use egui::{Key, KeyboardShortcut, Modifiers};
use rustemu_core::cpu::Cpu;
use rustemu_core::{Event, disasm};
use rustemu_session::expr::Expr;
use rustemu_session::{Session, StepGoal, StepKind};
use serde::{Deserialize, Serialize};

use crate::examples::EXAMPLES;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
    System,
}

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
        }
    }
}

#[derive(Default)]
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

pub struct App {
    pub settings: Settings,
    // ---- editor
    pub path: Option<PathBuf>,
    pub text: String,
    pub saved_text: String,
    pub messages: Vec<Message>,
    /// Error lines of the current file (1-based).
    pub error_lines: Vec<u32>,
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
    /// First address of the disassembly list.
    pub code_view_start: u32,
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
        let app = Self::with_settings(settings);
        crate::fonts::install(&cc.egui_ctx);
        app.apply_theme(&cc.egui_ctx);
        app
    }

    pub fn with_settings(settings: Settings) -> Self {
        let mut app = App {
            settings,
            path: None,
            text: String::new(),
            saved_text: String::new(),
            messages: Vec::new(),
            error_lines: Vec::new(),
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
            code_view_start: 0,
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
        };
        let last = app.settings.last_file.clone();
        match last {
            Some(p) if p.exists() => app.open_path(&p),
            _ => app.load_example(0),
        }
        app
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
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
            .unwrap_or_else(|| "untitled.asm".into())
    }

    // ---- files ------------------------------------------------------------------

    pub fn new_from_template(&mut self, i: usize) {
        self.path = None;
        self.bookmarks.clear();
        self.text = crate::examples::TEMPLATES.get(i).map(|t| t.1).unwrap_or("").to_string();
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.error_lines.clear();
        self.status = "New file".into();
    }

    pub fn open_dialog(&mut self) {
        let mut dlg =
            rfd::FileDialog::new().add_filter("Assembly", &["asm", "inc", "txt"]).add_filter("All files", &["*"]);
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        }
        if let Some(p) = dlg.pick_file() {
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
                self.text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                self.saved_text = self.text.clone();
                self.path = Some(p.to_path_buf());
                self.messages.clear();
                self.error_lines.clear();
                self.add_recent(p);
                self.settings.last_file = Some(p.to_path_buf());
                self.status = format!("Opened {}", p.display());
            }
            Err(e) => self.notice("Open", &format!("Cannot open {}:\n{e}", p.display())),
        }
    }

    fn add_recent(&mut self, p: &Path) {
        self.settings.recent.retain(|r| r != p);
        self.settings.recent.insert(0, p.to_path_buf());
        self.settings.recent.truncate(10);
    }

    pub fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        let mut dlg = rfd::FileDialog::new().add_filter("Assembly", &["asm"]).set_file_name(self.file_name());
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        }
        match dlg.save_file() {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    fn save_to(&mut self, p: &Path) -> bool {
        match std::fs::write(p, &self.text) {
            Ok(()) => {
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
        self.path = None;
        self.text = src.to_string();
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.error_lines.clear();
        self.status = format!("Example: {name}");
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
        match rustemu_asm::assemble(&name, &self.text, &mut loader) {
            Ok(p) => {
                self.messages.push(Message {
                    text: format!(
                        "{}: assembled successfully - {} format, {} bytes",
                        self.file_name(),
                        p.format.name(),
                        p.file_bytes().len()
                    ),
                    loc: None,
                    error: false,
                });
                Some(p)
            }
            Err(e) => {
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
        let out = match &self.path {
            Some(src) => src.with_extension(p.format.extension()),
            None => {
                let dlg = rfd::FileDialog::new().set_file_name(format!("program.{}", p.format.extension()));
                match dlg.save_file() {
                    Some(f) => f,
                    None => return,
                }
            }
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
        self.source_files = p
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
        self.source_index =
            self.source_files.iter().map(|(_, lines)| rustemu_asm::library::index(&lines.join("\n"))).collect();
        let devices = p.emu.start.clone();
        self.loaded_text = Some(self.text.clone());
        let root = self.vdrive();
        self.session.machine.set_dos_root(root);
        if let Err(e) = self.session.load_program(p) {
            self.notice("Load", &e);
            return;
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
        self.loaded_text = None;
        self.source_files.clear();
        self.source_index.clear();
        self.after_load();
    }

    fn after_load(&mut self) {
        self.session.machine.timer_irq = self.settings.timer_irq;
        self.load_gen += 1;
        for w in &mut self.watches {
            w.compiled = None;
        }
        self.stop_condition.compiled = None;
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

    /// The directory DOS file functions see as drive C:.
    fn vdrive(&self) -> PathBuf {
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            return dir.to_path_buf();
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let d = home.join("Documents").join("RustEmu8086").join("vdrive");
        std::fs::create_dir_all(&d).ok();
        d
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

    pub fn single_step(&mut self) {
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
        self.begin_exec();
        self.run = RunMode::Source(self.session.step_goal(StepKind::Out, self.show_macro_code));
        self.status = "Stepping out...".into();
    }

    pub fn start_run(&mut self) {
        if !self.loaded() {
            self.emulate();
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
        if !self.loaded() || self.session.program.is_none() {
            self.emulate();
            if !self.loaded() {
                return;
            }
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
        let start = std::time::Instant::now();
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
                    let t = std::time::Instant::now();
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
        self.bp_lines.entry((file, line)).or_insert_with(Breakpoint::new);
        self.sync_breakpoints();
    }

    /// Breakpoint on the editor line with the text cursor.
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

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        self.tick(&ctx);
        crate::ui::main_ui(self, ui);
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && self.confirm_discard.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_discard = Some(PendingAction::Quit);
        }
    }
}
