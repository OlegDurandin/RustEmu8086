//! Workspaces of the processors: each architecture has its own document, debugging session,
//! emulator and windows. Switching parks the current workspace and brings back (or creates) the
//! other one, so that nothing of either is lost; execution is paused on the way out.
//!
//! The architecture belongs to the document: a file remembers it in its [`FileSession`], a program
//! of the browser in its record, a link in its `arch` parameter. A file without one opens in the
//! current workspace, unless its text clearly is the other processor's code ([`detect_arch`]):
//! then the user chooses.

use rustemu_session::Arch;

use super::*;
use crate::tools::Tool;

/// The state of one processor's workspace while another one is shown (the fields of [`App`] of
/// the same names).
pub struct Workspace {
    session: Session,
    run: RunMode,
    status: String,
    windows: Windows,
    deferred_tools: Option<Windows>,
    panels: Option<egui_tiles::Tree<Tool>>,
    doc: Document,
    editor: EditorState,
    build: Build,
    program: Program,
    exec: Exec,
    debugger: Debugger,
    view: Views,
}

impl Workspace {
    /// An empty workspace of `arch` with the windows its settings remember; the program's tools
    /// open at the first emulate.
    pub(super) fn new(arch: Arch, settings: &mut Settings, browser: bool) -> Workspace {
        let session = match arch {
            Arch::I8086 => {
                let mut s = Session::new();
                // The browser has no file system: DOS programs get a drive in memory.
                if browser {
                    s.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
                }
                s
            }
            Arch::Mips32 => Session::new_mips(),
        };
        let (windows, panels) = settings.layout_mut(arch);
        let mut windows = windows.clone();
        let deferred_tools = Some(windows.take_program_tools());
        let panels = panels.as_deref().and_then(|s| ron::from_str(s).ok());
        Workspace {
            session,
            run: RunMode::Paused,
            status: "Ready".into(),
            windows,
            deferred_tools,
            panels,
            doc: Document::default(),
            editor: Default::default(),
            build: Default::default(),
            program: Default::default(),
            exec: Default::default(),
            debugger: Default::default(),
            view: Default::default(),
        }
    }

    /// The processor of the workspace.
    pub fn arch(&self) -> Arch {
        self.session.target.arch()
    }
}

/// "Open as": a document that looks like code of the other processor.
pub struct OpenAs {
    /// What opens it.
    pub action: PendingAction,
    /// The processor its text looks written for.
    pub detected: Arch,
}

/// The processor `text` is clearly written for: MIPS registers (`$t0`, `$sp`, `$4`) and `syscall`
/// against 8086 registers (`ax`, `dl`, `si`) outside comments. None when it is not clear (an empty
/// file, a few lines of data) — the file then opens in the current workspace.
pub fn detect_arch(text: &str) -> Option<Arch> {
    const MIPS_REGS: [&str; 30] = [
        "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "s0", "s1",
        "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "gp", "sp", "fp", "ra",
    ];
    const X86_REGS: [&str; 20] = [
        "ax", "bx", "cx", "dx", "al", "ah", "bl", "bh", "cl", "ch", "dl", "dh", "si", "di", "bp", "sp", "cs", "ds",
        "es", "ss",
    ];
    let (mut mips, mut x86) = (0usize, 0usize);
    for line in text.lines() {
        // Comments: `#` (MIPS) and `;` (8086); `#` also starts Emu8086's directives `#make_com#`.
        let line = line.split(';').next().unwrap_or("");
        if line.trim_start().starts_with("#make_") {
            x86 += 5;
            continue;
        }
        let code = line.split('#').next().unwrap_or("");
        let mut in_string = None;
        let mut word = String::new();
        let mut dollar = false;
        for c in code.chars().chain(std::iter::once(' ')) {
            if let Some(q) = in_string {
                if c == q {
                    in_string = None;
                }
                continue;
            }
            if c.is_ascii_alphanumeric() || c == '_' {
                word.push(c.to_ascii_lowercase());
                continue;
            }
            if !word.is_empty() {
                if dollar {
                    if MIPS_REGS.contains(&word.as_str()) || word.parse::<u8>().is_ok_and(|n| n < 32) {
                        mips += 1;
                    }
                } else if word == "syscall" {
                    mips += 3;
                } else if X86_REGS.contains(&word.as_str()) {
                    x86 += 1;
                }
                word.clear();
            }
            dollar = c == '$';
            if c == '\'' || c == '"' {
                in_string = Some(c);
            }
        }
    }
    if mips >= 2 && mips > 2 * x86 {
        Some(Arch::Mips32)
    } else if x86 >= 2 && x86 > 2 * mips {
        Some(Arch::I8086)
    } else {
        None
    }
}

impl App {
    /// The processor of the workspace shown.
    pub fn arch(&self) -> Arch {
        self.session.target.arch()
    }

    /// Shows the workspace of `arch`: the current one is paused and kept, the other comes back as
    /// it was left (the first time: with the file it had open at the last start).
    pub fn switch_arch(&mut self, arch: Arch) {
        if arch == self.arch() {
            return;
        }
        self.stop();
        self.store_settings();
        let fresh = self.activate(arch);
        self.settings.arch = arch.name().to_string();
        if fresh {
            self.open_start_document();
        }
        self.status = format!("{} - {}", arch.title(), self.status);
    }

    /// Puts the workspace of `arch` in place of the shown one, which is parked; true when it was
    /// created.
    fn activate(&mut self, arch: Arch) -> bool {
        let (mut w, fresh) = match self.parked.iter().position(|w| w.arch() == arch) {
            Some(i) => (self.parked.remove(i), false),
            None => (Workspace::new(arch, &mut self.settings, self.front.browser), true),
        };
        self.swap_workspace(&mut w);
        self.parked.push(w);
        fresh
    }

    pub(super) fn swap_workspace(&mut self, w: &mut Workspace) {
        use std::mem::swap;
        swap(&mut self.session, &mut w.session);
        swap(&mut self.run, &mut w.run);
        swap(&mut self.status, &mut w.status);
        swap(&mut self.windows, &mut w.windows);
        swap(&mut self.deferred_tools, &mut w.deferred_tools);
        swap(&mut self.panels, &mut w.panels);
        swap(&mut self.doc, &mut w.doc);
        swap(&mut self.editor, &mut w.editor);
        swap(&mut self.build, &mut w.build);
        swap(&mut self.program, &mut w.program);
        swap(&mut self.exec, &mut w.exec);
        swap(&mut self.debugger, &mut w.debugger);
        swap(&mut self.view, &mut w.view);
    }

    /// Runs `f` with the workspace of `arch` shown (created when there is none yet), then shows
    /// the current one again. Used to save and restore what the parked workspaces keep.
    pub(crate) fn in_workspace<R>(&mut self, arch: Arch, f: impl FnOnce(&mut App) -> R) -> R {
        let current = self.arch();
        if arch == current {
            return f(self);
        }
        if self.activate(arch) {
            self.open_start_document();
        }
        let r = f(self);
        self.activate(current);
        r
    }

    /// A parked workspace with unsaved changes, by its processor.
    pub(crate) fn parked_dirty(&self) -> Option<Arch> {
        self.parked.iter().find(|w| w.doc.text != w.doc.saved_text).map(Workspace::arch)
    }

    /// The processors of the parked workspaces.
    pub(crate) fn parked_archs(&self) -> Vec<Arch> {
        self.parked.iter().map(Workspace::arch).collect()
    }

    /// The 8086 session, shown or parked (it has the drive C: of DOS programs).
    pub(crate) fn x86_session(&self) -> Option<&Session> {
        if self.arch() == Arch::I8086 {
            return Some(&self.session);
        }
        self.parked.iter().find(|w| w.arch() == Arch::I8086).map(|w| &w.session)
    }

    /// The 8086 session, shown or parked, to change it.
    pub(crate) fn x86_session_mut(&mut self) -> Option<&mut Session> {
        if self.arch() == Arch::I8086 {
            return Some(&mut self.session);
        }
        self.parked.iter_mut().find(|w| w.arch() == Arch::I8086).map(|w| &mut w.session)
    }

    /// Opens what a workspace shows when it is first shown: its last file, or its first example.
    pub(super) fn open_start_document(&mut self) {
        let arch = self.arch();
        match self.settings.last_file_mut(arch).clone() {
            Some(p) if p.exists() => {
                self.dialogs.open_in = Some(arch);
                self.open_path(&p);
            }
            _ => match arch {
                // The first item of the Examples menu: Hello, world.
                Arch::I8086 => self.load_example(crate::examples::index(crate::examples::MENU[0].1).unwrap_or(0)),
                Arch::Mips32 => self.load_mips_example(0),
            },
        }
    }

    /// Chooses the workspace for a document about to be opened by `action`: the processor asked
    /// for ([`Dialogs::open_in`]), the one the document remembers (`saved`), or the one its `text`
    /// looks written for (asked first when it is not the current one). Switches to it. False when
    /// the document cannot be opened now: the user is asked which processor it is for, or about
    /// the unsaved changes of the workspace switched to (`action` is done again after that).
    pub(crate) fn enter_workspace_for(&mut self, action: &PendingAction, saved: Option<Arch>, text: &str) -> bool {
        let current = self.arch();
        let arch = match self.dialogs.open_in.take().or(saved) {
            Some(a) => a,
            None => match detect_arch(text) {
                Some(d) if d != current => {
                    self.dialogs.open_as = Some(OpenAs { action: action.clone(), detected: d });
                    return false;
                }
                _ => current,
            },
        };
        if arch == current {
            return true;
        }
        self.switch_arch(arch);
        if self.dirty() {
            self.dialogs.open_in = Some(arch);
            self.dialogs.confirm_discard = Some(action.clone());
            return false;
        }
        true
    }

    /// The processor a file's saved session says it is for (`key`: [`App::session_key`]).
    pub(crate) fn saved_arch(&self, key: &str) -> Option<Arch> {
        let (_, s) = self.settings.sessions.iter().find(|(k, _)| k == key)?;
        Some(Arch::from_name(&s.arch).unwrap_or(Arch::I8086))
    }

    /// The tool is one of this processor's: the 8086 has a screen, devices, flags, the 8087,
    /// DOS's drive C: and the DEBUG.EXE console; MIPS has its Run I/O console.
    pub fn has_tool(&self, tool: Tool) -> bool {
        let x86 = self.arch() == Arch::I8086;
        match tool {
            Tool::Screen
            | Tool::Devices
            | Tool::Drive
            | Tool::Alu
            | Tool::Fpu
            | Tool::Flags
            | Tool::FlagAnalyser
            | Tool::Stack
            | Tool::Console
            | Tool::Log
            | Tool::ValueViewer
            | Tool::Library
            | Tool::Reference => x86,
            Tool::RunIo => !x86,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_processor() {
        let mips = ".data\nmsg: .asciiz \"ax bx\"\n.text\nmain: li $v0, 4  # print ax\n  la $a0, msg\n  syscall\n";
        assert_eq!(detect_arch(mips), Some(Arch::Mips32));
        let x86 = "org 100h ; $t0\nmov ax, 5\nmov dx, offset msg\nint 21h\nmsg db 'syscall $'\n";
        assert_eq!(detect_arch(x86), Some(Arch::I8086));
        assert_eq!(detect_arch("#make_bin#\nnop\n"), Some(Arch::I8086));
        assert_eq!(detect_arch("bits 16\nmov ax, $ - start\nmov sp, ax\n"), Some(Arch::I8086));
        assert_eq!(detect_arch("addi $8, $9, 1\nsw $8, 0($29)\n"), Some(Arch::Mips32));
        assert_eq!(detect_arch(""), None);
        assert_eq!(detect_arch("nop\n"), None);
    }

    #[test]
    fn workspaces_keep_their_documents() {
        let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
        app.doc.text.push_str("; edited\n");
        app.windows[Tool::Memory] = true;
        app.switch_arch(Arch::Mips32);
        assert_eq!(app.arch(), Arch::Mips32);
        assert!(app.file_name().ends_with(".s"), "{}", app.file_name());
        assert!(!app.windows[Tool::Memory]);
        assert_eq!(app.settings.arch, "MIPS32");
        app.emulate();
        assert!(app.loaded(), "{:?}", app.build.messages.iter().map(|m| &m.text).collect::<Vec<_>>());
        app.switch_arch(Arch::I8086);
        assert!(app.doc.text.ends_with("; edited\n"));
        assert!(app.windows[Tool::Memory]);
        assert!(!app.loaded());
        app.switch_arch(Arch::Mips32);
        assert!(app.loaded(), "the MIPS program stays loaded");
    }
}
