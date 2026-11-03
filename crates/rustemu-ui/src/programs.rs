//! "my programs": the browser has no files, so the sources are kept in its local storage. Save puts
//! the editor's program here, Open lists them; what the editor shows, unsaved changes included,
//! is restored on the next visit (the draft). Written by [`App::persist`].

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use rustemu_session::Arch;

use crate::app::{App, PendingAction};

pub(crate) const PROGRAMS_KEY: &str = "programs";
pub(crate) const DRAFT_KEY: &str = "draft";
/// The draft of the MIPS workspace.
pub(crate) const MIPS_DRAFT_KEY: &str = "draft_mips";

/// The storage key of the draft of the workspace of `arch`.
fn draft_key(arch: Arch) -> &'static str {
    match arch {
        Arch::I8086 => DRAFT_KEY,
        Arch::Mips32 => MIPS_DRAFT_KEY,
    }
}

/// A program saved in the browser.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StoredProgram {
    /// The source.
    pub text: String,
    /// Time of the last save, seconds since 1970.
    pub modified: f64,
    /// The processor it is for ([`Arch::name`]; empty: the 8086).
    #[serde(default)]
    pub arch: String,
}

impl StoredProgram {
    /// The processor the program is for.
    pub fn arch(&self) -> Arch {
        Arch::from_name(&self.arch).unwrap_or(Arch::I8086)
    }
}

/// How a program record names `arch`.
fn arch_field(arch: Arch) -> String {
    if arch == Arch::I8086 { String::new() } else { arch.name().to_string() }
}

/// Programs saved in the browser, by name.
pub type Programs = BTreeMap<String, StoredProgram>;

/// The "save in this browser" dialog.
pub struct SaveName {
    /// The name being typed.
    pub text: String,
    /// Done after saving (opening another program from the unsaved changes dialog).
    pub then: Option<PendingAction>,
}

/// What the editor shows, unsaved changes included.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Draft {
    name: Option<String>,
    program: Option<String>,
    example: Option<String>,
    text: String,
    /// The text as last saved, when it differs.
    saved: Option<String>,
    /// Opened from a link (and not saved since): runs on a temporary drive C:.
    from_link: bool,
}

fn now() -> f64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// The name a program is saved under: `name` trimmed, `.asm` added when it has no extension; None
/// when it is empty or has path separators.
pub fn program_name(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        return None;
    }
    Some(if name.contains('.') { name.to_string() } else { format!("{name}.asm") })
}

impl App {
    /// Saves the editor text as program `name` of "my programs" (replacing one of that name).
    pub fn save_program(&mut self, name: &str) {
        let arch = arch_field(self.arch());
        self.web
            .programs
            .insert(name.to_string(), StoredProgram { text: self.doc.text.clone(), modified: now(), arch });
        self.doc.program = Some(name.to_string());
        self.doc.name = Some(name.to_string());
        self.doc.example_key = None;
        self.doc.saved_text = self.doc.text.clone();
        self.status = format!("Saved {name} in this browser");
    }

    /// Opens program `name` of "my programs" in the editor.
    pub fn open_program(&mut self, name: &str) {
        let Some((text, arch)) = self.web.programs.get(name).map(|p| (p.text.clone(), p.arch())) else { return };
        if !self.enter_workspace_for(&PendingAction::Program(name.to_string()), Some(arch), &text) {
            return;
        }
        self.store_session();
        self.doc.path = None;
        self.doc.example_key = None;
        self.doc.program = Some(name.to_string());
        self.doc.name = Some(name.to_string());
        self.doc.from_link = false;
        self.doc.text = text;
        self.doc.saved_text = self.doc.text.clone();
        self.build.messages.clear();
        self.clear_errors();
        self.restore_session();
        self.status = format!("Opened {name}");
    }

    /// Opens a source uploaded from the computer and keeps it in "my programs" (as `name (2).asm`
    /// when a different program has its name).
    pub fn import_program(&mut self, name: &str, text: String) {
        // The processor: the current one, unless the text clearly is the other's (then asked).
        let arch = match self.dialogs.open_in {
            Some(a) => a,
            None => match crate::app::detect_arch(&text) {
                Some(d) if d != self.arch() => {
                    let action = PendingAction::Import(name.to_string(), text);
                    self.dialogs.open_as = Some(crate::app::OpenAs { action, detected: d });
                    return;
                }
                _ => self.arch(),
            },
        };
        let base = program_name(name).unwrap_or_else(|| "upload.asm".into());
        let mut name = base.clone();
        for n in 2.. {
            if self.web.programs.get(&name).is_none_or(|p| p.text == text) {
                break;
            }
            name = match base.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                None => format!("{base} ({n})"),
            };
        }
        self.dialogs.open_in = Some(arch);
        self.web.programs.insert(name.clone(), StoredProgram { text, modified: now(), arch: arch_field(arch) });
        self.open_program(&name);
    }

    /// Deletes a program of "my programs".
    pub fn delete_program(&mut self, name: &str) {
        self.web.programs.remove(name);
        if self.doc.program.as_deref() == Some(name) {
            self.doc.program = None;
        }
        self.status = format!("Deleted {name}");
    }

    /// Names of the programs, last saved first.
    pub fn recent_programs(&self) -> Vec<String> {
        let mut list: Vec<(&String, f64)> = self.web.programs.iter().map(|(n, p)| (n, p.modified)).collect();
        list.sort_by(|a, b| b.1.total_cmp(&a.1));
        list.into_iter().map(|(n, _)| n.clone()).collect()
    }

    pub(crate) fn persist_browser(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(programs) = ron::ser::to_string(&self.web.programs) {
            self.put(storage, PROGRAMS_KEY, programs);
        }
        self.persist_draft(storage);
        for arch in self.parked_archs() {
            self.in_workspace(arch, |app| app.persist_draft(storage));
        }
    }

    /// Writes the draft of the shown workspace.
    fn persist_draft(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some((key, draft)) = self.draft_record() {
            self.put(storage, key, draft);
        }
    }

    /// The draft of the shown workspace as it is stored: its key and value. The web page keeps
    /// a copy, to store it when the application stops on a panic.
    pub fn draft_record(&self) -> Option<(&'static str, String)> {
        let draft = Draft {
            name: self.doc.name.clone(),
            program: self.doc.program.clone(),
            example: self.doc.example_key.clone(),
            text: self.doc.text.clone(),
            saved: (self.doc.saved_text != self.doc.text).then(|| self.doc.saved_text.clone()),
            from_link: self.from_link(),
        };
        Some((draft_key(self.arch()), ron::ser::to_string(&draft).ok()?))
    }

    /// Restores "my programs" and the editor of the last visit (the browser).
    pub fn restore_browser(&mut self, storage: &dyn eframe::Storage) {
        if !self.front.browser {
            return;
        }
        if let Some(programs) = eframe::get_value(storage, PROGRAMS_KEY) {
            self.web.programs = programs;
        }
        for arch in Arch::ALL {
            if arch == self.arch() || storage.get_string(draft_key(arch)).is_none() {
                continue;
            }
            self.in_workspace(arch, |app| app.restore_draft(storage));
        }
        self.restore_draft(storage);
    }

    /// Restores the draft of the shown workspace.
    fn restore_draft(&mut self, storage: &dyn eframe::Storage) {
        let Some(d) = eframe::get_value::<Draft>(storage, draft_key(self.arch())) else { return };
        self.doc.path = None;
        self.doc.program = d.program.filter(|p| self.web.programs.contains_key(p));
        self.doc.name = d.name;
        self.doc.from_link = d.from_link;
        self.web.link_notice_pending = d.from_link;
        self.doc.example_key = d.example;
        self.doc.saved_text = d.saved.unwrap_or_else(|| d.text.clone());
        self.doc.text = d.text;
        self.build.messages.clear();
        self.clear_errors();
        self.restore_session();
        self.status = if self.dirty() {
            format!("{} with the unsaved changes of the last visit", self.file_name())
        } else {
            format!("Opened {}", self.file_name())
        };
    }
}

/// Window "my programs" (the browser).
fn ago(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

pub(crate) fn programs_ui(app: &mut App, ui: &mut Ui) {
    ui.label("Programs saved in this browser. Save (⌘S) puts the program in the editor here.");
    ui.horizontal(|ui| {
        if ui
            .button("upload from computer…")
            .on_hover_text(
                "An .asm file opens in the editor and is kept here; .com / .exe / .bin are loaded into the emulator",
            )
            .clicked()
        {
            app.guarded(PendingAction::Upload);
        }
        if ui.button("download").on_hover_text("Save the program in the editor as a file on the computer").clicked() {
            app.download_source();
        }
    });
    ui.separator();
    if app.web.programs.is_empty() {
        ui.weak("No programs yet: save the program in the editor or upload one.");
        return;
    }
    enum Act {
        Open(String),
        Download(String),
        Delete(String),
    }
    let mut act = None;
    let now = now();
    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("programs_grid").num_columns(4).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
            for name in app.recent_programs() {
                let p = &app.web.programs[&name];
                let current = app.doc.program.as_deref() == Some(name.as_str());
                if ui.selectable_label(current, RichText::new(&name).monospace()).on_hover_text("open").clicked()
                    && !current
                {
                    act = Some(Act::Open(name.clone()));
                }
                // The processor, when it is not the 8086 (opening switches to its workspace).
                let arch = match p.arch() {
                    Arch::I8086 => String::new(),
                    a => format!("{}, ", a.name()),
                };
                ui.weak(format!("{arch}{} lines, {}", p.text.lines().count(), ago(now - p.modified)));
                if ui.small_button("download").clicked() {
                    act = Some(Act::Download(name.clone()));
                }
                if app.dialogs.program_delete.as_deref() == Some(name.as_str()) {
                    ui.horizontal(|ui| {
                        ui.colored_label(crate::colors::error(ui), "delete?");
                        if ui.small_button("yes").clicked() {
                            act = Some(Act::Delete(name.clone()));
                        }
                        if ui.small_button("no").clicked() {
                            app.dialogs.program_delete = None;
                        }
                    });
                } else if ui.small_button("delete…").clicked() {
                    app.dialogs.program_delete = Some(name.clone());
                }
                ui.end_row();
            }
        });
    });
    match act {
        Some(Act::Open(name)) => app.guarded(PendingAction::Program(name)),
        Some(Act::Download(name)) => crate::platform::download(&name, app.web.programs[&name].text.as_bytes()),
        Some(Act::Delete(name)) => {
            app.dialogs.program_delete = None;
            app.delete_program(&name);
        }
        None => {}
    }
}

/// "save in this browser": asks for the name of a new program.
pub fn save_name_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.dialogs.save_name.take() else { return };
    let mut keep = true;
    egui::Window::new("save in this browser")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Name of the program in \"my programs\":");
            ui.add(egui::TextEdit::singleline(&mut d.text).hint_text("file name").desired_width(320.0)).request_focus();
            let name = program_name(&d.text);
            match &name {
                None => {
                    ui.colored_label(crate::colors::error(ui), "enter a file name");
                }
                Some(n) if app.web.programs.contains_key(n) && app.doc.program.as_ref() != Some(n) => {
                    ui.colored_label(crate::colors::warn(ui), format!("replaces the saved {n}"));
                }
                Some(_) => {
                    ui.weak("download it as a file from the \"my programs\" window");
                }
            }
            ui.horizontal(|ui| {
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(n) = &name
                    && (ui.button("Save").clicked() || enter)
                {
                    app.save_program(n);
                    keep = false;
                    if let Some(then) = d.then.take() {
                        app.perform(then);
                    }
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
    if keep {
        app.dialogs.save_name = Some(d);
    }
}
