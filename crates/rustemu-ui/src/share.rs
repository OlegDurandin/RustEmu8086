//! Links that carry a program: `<site>#name=hello.asm&code=…` (the text deflated, in base64url)
//! opens it in the web version, `&run` also runs it. Nothing is stored on a server: the program is
//! in the link, after `#`, which browsers do not send.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use rustemu_session::Arch;

use crate::app::{App, PendingAction};

/// Links longer than this may be cut by messengers and mail programs.
const LONG_LINK: usize = 8000;
/// The most text a link may unpack to.
const MAX_TEXT: usize = 1 << 20;

/// A program from a link.
#[derive(Clone, Debug, PartialEq)]
pub struct Shared {
    /// Versioned portable project (None for legacy links).
    pub project: Option<rustemu_build::project::Project>,
    /// File name.
    pub name: String,
    /// Source text.
    pub text: String,
    /// Run it after opening (`&run`).
    pub run: bool,
    /// The processor it is for (`&arch=MIPS32`; none: the 8086).
    pub arch: Arch,
}

/// The "share link" dialog.
pub struct ShareDialog {
    /// The link also runs the program.
    pub run: bool,
    /// Project snapshot captured once when the dialog opens.
    pub project: Option<Result<rustemu_build::project::Project, String>>,
}

impl ShareDialog {
    /// Opens a sharing dialog; dependencies are captured on its first frame.
    pub fn new(run: bool) -> Self {
        Self { run, project: None }
    }
}

/// Encodes the same package as a file into a bounded compressed link.
pub fn project_fragment(project: &rustemu_build::project::Project, run: bool) -> Result<String, String> {
    let bytes = project.encode()?;
    let code = URL_SAFE_NO_PAD.encode(miniz_oxide::deflate::compress_to_vec(&bytes, 9));
    Ok(format!("project={code}{}", if run { "&run" } else { "" }))
}

/// The part after `#` of a link to 8086 program `text` named `name`.
pub fn fragment(name: &str, text: &str, run: bool) -> String {
    fragment_for(Arch::I8086, name, text, run)
}

/// The part after `#` of a link to program `text` named `name` for processor `arch`.
pub fn fragment_for(arch: Arch, name: &str, text: &str, run: bool) -> String {
    let code = URL_SAFE_NO_PAD.encode(miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 9));
    let mut f = format!("name={}&code={code}", percent_encode(name));
    if arch != Arch::I8086 {
        f.push_str(&format!("&arch={}", arch.name()));
    }
    if run {
        f.push_str("&run");
    }
    f
}

/// The program of a link's `#…` part (with or without the `#`); None when it has none.
pub fn parse(fragment: &str) -> Option<Shared> {
    let mut shared =
        Shared { project: None, name: "shared.asm".into(), text: String::new(), run: false, arch: Arch::I8086 };
    if fragment.len() > rustemu_build::project::MAX_PACKAGE_BYTES * 2 {
        return None;
    }
    let parts: Vec<_> = fragment.trim_start_matches('#').split('&').collect();
    if let Some(value) = parts.iter().find_map(|p| p.strip_prefix("project=")) {
        let compressed = URL_SAFE_NO_PAD.decode(value).ok()?;
        let bytes =
            miniz_oxide::inflate::decompress_to_vec_with_limit(&compressed, rustemu_build::project::MAX_PACKAGE_BYTES)
                .ok()?;
        let p = rustemu_build::project::Project::decode(&bytes).ok()?;
        return Some(Shared {
            name: p.entry.clone(),
            text: p.files[&p.entry].clone(),
            arch: Arch::from_name(&p.architecture)?,
            run: parts.contains(&"run"),
            project: Some(p),
        });
    }
    let mut code = None;
    for part in fragment.trim_start_matches('#').split('&') {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        match key {
            "name" => {
                if let Some(name) = percent_decode(value).as_deref().and_then(crate::programs::program_name) {
                    shared.name = name;
                }
            }
            "code" => code = Some(value),
            "run" => shared.run = true,
            "arch" => shared.arch = Arch::from_name(value)?,
            _ => {}
        }
    }
    let deflated = URL_SAFE_NO_PAD.decode(code?).ok()?;
    let text = miniz_oxide::inflate::decompress_to_vec_with_limit(&deflated, MAX_TEXT).ok()?;
    shared.text = String::from_utf8_lossy(&text).replace("\r\n", "\n");
    Some(shared)
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(if b[i] == b'+' { b' ' } else { b[i] });
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl App {
    /// Opens a program from a link as a new document (asks first about unsaved changes).
    pub fn open_link(&mut self, mut shared: Shared) {
        if let Some(p) = &shared.project {
            if let Err(e) = p.validate() {
                self.notice("Open project", &e);
                return;
            }
            shared.name = p.entry.clone();
            shared.text = p.files[&p.entry].clone();
            shared.arch = Arch::from_name(&p.architecture).unwrap();
        }
        self.guarded(PendingAction::Shared(shared));
    }

    pub(crate) fn open_shared(&mut self, s: Shared) {
        self.dialogs.open_in = Some(s.arch);
        if !self.enter_workspace_for(&PendingAction::Shared(s.clone()), None, "") {
            return;
        }
        self.store_session();
        self.doc.path = None;
        self.doc.program = None;
        self.doc.example_key = None;
        self.doc.name = Some(s.name.clone());
        self.doc.from_link = true;
        self.web.link_notice_pending = true;
        self.source_changed();
        self.doc.project = s.project;
        self.apply_project_settings();
        self.doc.text = s.text;
        self.doc.saved_text = self.doc.text.clone();
        self.build.messages.clear();
        self.clear_errors();
        self.restore_session();
        self.status = if self.doc.project.is_some() {
            format!("Opened project {}", s.name)
        } else {
            format!("Opened {} from a link", s.name)
        };
        if s.run {
            self.emulate();
            self.start_run();
        }
    }

    /// The program in the editor came from a link and is not saved: it runs on an empty temporary
    /// drive C:, so that somebody else's program can neither read nor change the user's files.
    pub fn from_link(&self) -> bool {
        self.doc.from_link && self.doc.path.is_none() && self.doc.program.is_none() && self.doc.example_key.is_none()
    }

    /// Puts an empty temporary drive C: (for a program from a link) or the user's own one back
    /// into the machine. Only the memory drive of the browser is swapped.
    pub(crate) fn use_temporary_drive(&mut self, temporary: bool) {
        if temporary && self.web.own_drive.is_none() {
            let Some(own) = self.session.dos_memory_drive().cloned() else { return };
            self.web.own_drive = Some(own);
            self.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
        } else if !temporary && let Some(own) = self.web.own_drive.take() {
            self.session.set_dos_memory_drive(own);
        }
    }

    /// Restores the package's assembly configuration after opening or restoring storage.
    pub(crate) fn apply_project_settings(&mut self) {
        if let Some(p) = &self.doc.project {
            self.settings.mars = p.mars.clone();
            self.settings.syntax = match p.dialect {
                None => crate::app::SyntaxChoice::Auto,
                Some(rustemu_asm::Dialect::Masm) => crate::app::SyntaxChoice::Masm,
                Some(rustemu_asm::Dialect::Nasm) => crate::app::SyntaxChoice::Nasm,
            };
        }
    }

    /// Storage keeps the last valid inventory plus current configuration; the editor text
    /// is stored separately, including oversized or temporarily broken edits.
    pub(crate) fn stored_project(&self) -> Option<rustemu_build::project::Project> {
        self.doc.project.clone().map(|mut p| {
            p.mars = self.settings.mars.clone();
            p.dialect = self.settings.syntax.dialect();
            p
        })
    }

    /// Captures the project and all dependencies without executing the program.
    pub fn portable_project(&self) -> Result<rustemu_build::project::Project, String> {
        if let Some(mut p) = self.doc.project.clone() {
            p.files.insert(p.entry.clone(), self.doc.text.clone());
            p.mars = self.settings.mars.clone();
            p.dialect = self.settings.syntax.dialect();
            p.validate()?;
            return Ok(p);
        }
        rustemu_build::project::Project::capture(self.build_request(0), self.settings.mars.clone(), String::new())
    }

    /// Link containing sources, settings and prepared input.
    pub fn share_link(&self, run: bool) -> Result<String, String> {
        let base = self.front.share_base.as_deref().unwrap_or(crate::app::WEB_URL);
        Ok(format!("{base}#{}", project_fragment(&self.portable_project()?, run)?))
    }

    /// Exports a project file, or downloads it in the browser.
    pub fn export_project(&mut self, project: &rustemu_build::project::Project) -> bool {
        let bytes = match project.encode() {
            Ok(b) => b,
            Err(e) => {
                self.notice("Export project", &e);
                return false;
            }
        };
        let file_name = self.file_name();
        let stem = std::path::Path::new(&file_name).file_stem().and_then(|s| s.to_str()).unwrap_or("project");
        let name = format!("{stem}.remu");
        if self.front.browser {
            crate::platform::download(&name, &bytes);
        } else if let Some(path) = crate::platform::save_file(&[("RustEmu project", &["remu"])], None, &name) {
            if let Err(e) = std::fs::write(path, bytes) {
                self.notice("Export project", &e.to_string());
                return false;
            }
        } else {
            return false;
        }
        self.status = format!("Exported {name}");
        true
    }

    /// Validates an imported package before the unsaved-work guard or any state changes.
    pub fn import_project(&mut self, bytes: &[u8]) {
        match rustemu_build::project::Project::decode(bytes) {
            Ok(p) => self.open_link(Shared {
                name: p.entry.clone(),
                text: p.files[&p.entry].clone(),
                arch: Arch::from_name(&p.architecture).unwrap(),
                run: false,
                project: Some(p),
            }),
            Err(e) => self.notice("Open project", &e),
        }
    }
}

/// Portable projects always use temporary runtime files, independently of source saving.
pub fn project_drive_text() -> &'static str {
    "This portable project runs with an empty temporary drive C:. Its source files are used only for assembly; your personal files are not available to the program. Runtime file changes are not saved in the project."
}

/// What a program from a link may do with drive C: (the notice on its first run, the drive window).
pub fn temporary_drive_text(name: &str) -> String {
    format!(
        "{name} was opened from a link, so it runs with an empty temporary drive C: instead of yours. \
         It can neither see nor change your files, and what it writes there is not kept. To run it \
         with your drive C:, save it first (File -> Save) and run it again."
    )
}

/// "share link": the link to the program in the editor, to copy.
pub fn share_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.dialogs.share.take() else { return };
    if d.project.is_none() {
        d.project = Some(app.portable_project());
    }
    let mut keep = true;
    egui::Window::new("share link")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.label(format!(
                "A link that opens {} in the web version of RustEmu8086. The program is inside the link \
                 itself: nothing is uploaded anywhere.",
                app.file_name()
            ));
            let project = match d.project.as_mut().unwrap() {
                Ok(p) => p,
                Err(e) => {
                    ui.colored_label(crate::colors::warn(ui), e.as_str());
                    if ui.button("close").clicked() {
                        keep = false;
                    }
                    return;
                }
            };
            ui.label(format!("{} source files; MARS settings and arguments included", project.files.len()));
            let input_label = ui.label("Prepared console input (Enter = new line):");
            ui.add(
                egui::TextEdit::multiline(&mut project.input)
                    .desired_rows(3)
                    .char_limit(rustemu_build::project::MAX_INPUT_BYTES),
            )
            .labelled_by(input_label.id);
            ui.checkbox(&mut d.run, "run the program when the link is opened");
            let base = app.front.share_base.as_deref().unwrap_or(crate::app::WEB_URL);
            let link = match project_fragment(project, d.run) {
                Ok(f) => format!("{base}#{f}"),
                Err(e) => {
                    ui.colored_label(crate::colors::warn(ui), e);
                    if ui.button("close").clicked() {
                        keep = false;
                    }
                    return;
                }
            };
            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut link.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(3),
                );
            });
            if link.len() > LONG_LINK {
                ui.colored_label(
                    crate::colors::warn(ui),
                    format!(
                        "The link is {} characters long: some messengers cut long links; exporting the project file \
                         may be safer.",
                        link.len()
                    ),
                );
            } else {
                ui.weak(format!("{} characters", link.len()));
            }
            ui.horizontal(|ui| {
                if ui.button("export project…").clicked() {
                    app.export_project(project);
                }
                if ui.button("copy").clicked() {
                    ui.ctx().copy_text(link.clone());
                    app.status = "Link copied".into();
                    keep = false;
                }
                if ui.button("close").clicked() {
                    keep = false;
                }
            });
        });
    if keep {
        app.dialogs.share = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_carry_the_program() {
        let text = "; Привет\norg 100h\n    mov ah, 9 ; 'x' & \"y\"\n    ret\n";
        let f = fragment("my prog.asm", text, true);
        assert!(f.starts_with("name=my%20prog.asm&code="), "{f}");
        assert!(!f.contains(['+', '/', '#']) && f.matches('=').count() == 2, "{f}");
        let s = parse(&format!("#{f}")).unwrap();
        assert_eq!(
            s,
            Shared { project: None, name: "my prog.asm".into(), text: text.into(), run: true, arch: Arch::I8086 }
        );
        assert!(!parse(&fragment("a", "b", false)).unwrap().run);
        let f = fragment_for(Arch::Mips32, "hello.s", "syscall\n", false);
        assert!(f.ends_with("&arch=MIPS32"), "{f}");
        assert_eq!(parse(&f).unwrap().arch, Arch::Mips32);
        assert_eq!(parse(&f.replace("MIPS32", "Z80")), None, "an unknown processor");
    }

    #[test]
    fn bad_links_are_ignored() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("#section-2"), None);
        assert_eq!(parse("#code=***"), None);
        assert_eq!(parse("#code=AAAA"), None, "not deflate data");
        // A bad name falls back to the default.
        let f = fragment("../x", "nop", false);
        assert_eq!(parse(&f).unwrap().name, "shared.asm");
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn package(mips: bool) -> rustemu_build::project::Project {
        rustemu_build::project::Project {
            version: 1, architecture: if mips { "MIPS32" } else { "8086" }.into(),
            entry: "main.asm".into(), dialect: None,
            mars: crate::mars_settings::MarsSettings {
                delayed_branching: true, memory: "CompactDataAtZero".into(),
                program_arguments_enabled: true, program_arguments: "one two".into(),
                ..Default::default()
            },
            files: BTreeMap::from([("main.asm".into(), if mips {
                ".include \"lib.inc\"\n.text\n.globl main\nmain: li $v0, 5\nsyscall\nmove $a0, $v0\nli $v0, 1\nsyscall\nli $v0, 10\nsyscall\n"
            } else {
                "org 100h\ninclude 'lib.inc'\nmov ah, 1\nint 21h\nmov ax, 4c00h\nint 21h\n"
            }.into()), ("lib.inc".into(), if mips { ".eqv VALUE 42\n" } else { "VALUE equ 42\n" }.into())]),
            input: if mips { "42\n" } else { "X" }.into(),
        }
    }

    #[test]
    fn portable_link_and_file_run_in_a_clean_app_with_the_same_settings_and_input() {
        for mips in [false, true] {
            let p = package(mips);
            let shared = parse(&project_fragment(&p, false).unwrap()).unwrap();
            assert_eq!(shared.project.as_ref(), Some(&p));
            let mut app = App::with_settings(crate::app::Settings { last_file: None, ..Default::default() });
            app.doc.saved_text = app.doc.text.clone();
            app.open_link(shared);
            app.emulate();
            assert!(!app.build.failed, "{:?}", app.status);
            if mips {
                let m = app.session.mips().unwrap();
                assert!(m.settings.delayed_branching);
                assert_eq!(m.settings.memory, rustemu_mips::MemoryConfig::CompactDataAtZero);
                assert_eq!(m.program_arguments, ["one", "two"]);
                assert!(matches!(m.files.storage, rustemu_mips::files::Storage::Memory(_)));
            } else {
                assert!(app.session.dos_memory_drive().is_some());
            }
            for _ in 0..100 {
                if app.session.target.terminated().is_some() {
                    break;
                }
                app.session.target.step();
            }
            assert!(app.session.target.terminated().is_some());
            assert_eq!(app.session.target.output(), if mips { b"42\n42".as_slice() } else { b"X".as_slice() });
            app.reload();
            assert!(app.session.target.has_pending_input());
            let exported = app.portable_project().unwrap();
            assert_eq!(exported, p);
        }
    }

    #[test]
    fn invalid_package_does_not_replace_or_cancel_current_work() {
        let mut app = App::with_settings(crate::app::Settings { last_file: None, ..Default::default() });
        let original = app.doc.text.clone();
        let mut p = package(true);
        p.version = 99;
        app.import_project(&serde_json::to_vec(&p).unwrap());
        assert_eq!(app.doc.text, original);
        assert!(app.doc.project.is_none());
        assert!(app.dialogs.confirm_discard.is_none());
    }

    #[test]
    fn saving_and_reassembling_a_browser_project_preserves_the_personal_drive() {
        let mut app = App::with_settings(crate::app::Settings { last_file: None, ..Default::default() });
        app.front.browser = true;
        let mut drive = rustemu_core::memdrive::MemDrive::new();
        drive.write("personal.txt", b"keep".to_vec()).unwrap();
        app.session.set_dos_memory_drive(drive);
        app.import_project(&package(false).encode().unwrap());
        app.emulate();
        app.save_program("lesson.asm");
        app.emulate();
        assert_eq!(app.web.own_drive.as_ref().unwrap().read("personal.txt"), Some(b"keep".as_slice()));
        assert!(app.session.dos_memory_drive().unwrap().read("personal.txt").is_none());
        app.load_example_text("normal.asm", "org 100h\nret\n");
        app.emulate();
        assert_eq!(app.session.dos_memory_drive().unwrap().read("personal.txt"), Some(b"keep".as_slice()));
    }

    #[test]
    fn browser_storage_keeps_project_dependencies_settings_and_edited_source() {
        #[derive(Default)]
        struct Storage(std::collections::HashMap<String, String>);
        impl eframe::Storage for Storage {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.into(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }
        let mut app = App::with_settings(crate::app::Settings { last_file: None, ..Default::default() });
        app.front.browser = true;
        let mut p = package(true);
        p.dialect = Some(rustemu_asm::Dialect::Nasm);
        app.import_project(&p.encode().unwrap());
        app.doc.text.push_str("; edited source\n");
        app.settings.mars.self_modifying = true;
        app.save_program("lesson.s");
        let (key, draft) = app.draft_record().unwrap();
        let mut storage = Storage::default();
        eframe::Storage::set_string(&mut storage, key, draft);
        let mut restored = App::with_settings(crate::app::Settings { last_file: None, ..Default::default() });
        restored.front.browser = true;
        restored.restore_browser(&storage);
        assert_eq!(restored.arch(), Arch::I8086);
        restored.switch_arch(Arch::Mips32);
        assert_eq!(restored.doc.text, app.doc.text);
        assert!(restored.settings.mars.self_modifying);
        assert_eq!(restored.settings.syntax, crate::app::SyntaxChoice::Nasm);
        assert_eq!(restored.doc.project.as_ref().unwrap().files["lib.inc"], p.files["lib.inc"]);
        restored.web.programs = app.web.programs.clone();
        restored.open_program("lesson.s");
        assert_eq!(restored.portable_project().unwrap(), app.portable_project().unwrap());
    }

    #[test]
    fn oversized_inflated_project_link_is_rejected() {
        let bytes = vec![b' '; rustemu_build::project::MAX_PACKAGE_BYTES + 1];
        let code = URL_SAFE_NO_PAD.encode(miniz_oxide::deflate::compress_to_vec(&bytes, 1));
        assert!(parse(&format!("project={code}")).is_none());
    }
}
