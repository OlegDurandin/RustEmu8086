//! "my programs": the browser has no files, so the sources are kept in its local storage. Save puts
//! the editor's program here, Open lists them; what the editor shows, unsaved changes included,
//! is restored on the next visit (the draft). Written by [`App::persist`].

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::app::{App, PendingAction};

pub(crate) const PROGRAMS_KEY: &str = "programs";
pub(crate) const DRAFT_KEY: &str = "draft";

/// A program saved in the browser.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StoredProgram {
    pub text: String,
    /// Time of the last save, seconds since 1970.
    pub modified: f64,
}

pub type Programs = BTreeMap<String, StoredProgram>;

/// The "save in this browser" dialog.
pub struct SaveName {
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
        self.programs.insert(name.to_string(), StoredProgram { text: self.text.clone(), modified: now() });
        self.program = Some(name.to_string());
        self.doc_name = Some(name.to_string());
        self.example_key = None;
        self.saved_text = self.text.clone();
        self.status = format!("Saved {name} in this browser");
    }

    /// Opens program `name` of "my programs" in the editor.
    pub fn open_program(&mut self, name: &str) {
        let Some(text) = self.programs.get(name).map(|p| p.text.clone()) else { return };
        self.store_session();
        self.path = None;
        self.example_key = None;
        self.program = Some(name.to_string());
        self.doc_name = Some(name.to_string());
        self.opened_from_link = false;
        self.text = text;
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.clear_errors();
        self.restore_session();
        self.status = format!("Opened {name}");
    }

    /// Opens a source uploaded from the computer and keeps it in "my programs" (as `name (2).asm`
    /// when a different program has its name).
    pub fn import_program(&mut self, name: &str, text: String) {
        let base = program_name(name).unwrap_or_else(|| "upload.asm".into());
        let mut name = base.clone();
        for n in 2.. {
            if self.programs.get(&name).is_none_or(|p| p.text == text) {
                break;
            }
            name = match base.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                None => format!("{base} ({n})"),
            };
        }
        self.programs.insert(name.clone(), StoredProgram { text, modified: now() });
        self.open_program(&name);
    }

    pub fn delete_program(&mut self, name: &str) {
        self.programs.remove(name);
        if self.program.as_deref() == Some(name) {
            self.program = None;
        }
        self.status = format!("Deleted {name}");
    }

    /// Names of the programs, last saved first.
    pub fn recent_programs(&self) -> Vec<String> {
        let mut list: Vec<(&String, f64)> = self.programs.iter().map(|(n, p)| (n, p.modified)).collect();
        list.sort_by(|a, b| b.1.total_cmp(&a.1));
        list.into_iter().map(|(n, _)| n.clone()).collect()
    }

    pub(crate) fn persist_browser(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(programs) = ron::ser::to_string(&self.programs) {
            self.put(storage, PROGRAMS_KEY, programs);
        }
        let draft = Draft {
            name: self.doc_name.clone(),
            program: self.program.clone(),
            example: self.example_key.clone(),
            text: self.text.clone(),
            saved: (self.saved_text != self.text).then(|| self.saved_text.clone()),
            from_link: self.from_link(),
        };
        if let Ok(draft) = ron::ser::to_string(&draft) {
            self.put(storage, DRAFT_KEY, draft);
        }
    }

    /// Restores "my programs" and the editor of the last visit (the browser).
    pub fn restore_browser(&mut self, storage: &dyn eframe::Storage) {
        if !self.browser {
            return;
        }
        if let Some(programs) = eframe::get_value(storage, PROGRAMS_KEY) {
            self.programs = programs;
        }
        let Some(d) = eframe::get_value::<Draft>(storage, DRAFT_KEY) else { return };
        self.path = None;
        self.program = d.program.filter(|p| self.programs.contains_key(p));
        self.doc_name = d.name;
        self.opened_from_link = d.from_link;
        self.link_notice_pending = d.from_link;
        self.example_key = d.example;
        self.saved_text = d.saved.unwrap_or_else(|| d.text.clone());
        self.text = d.text;
        self.messages.clear();
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
pub fn programs_window(app: &mut App, ctx: &egui::Context) {
    if !app.browser {
        app.windows.programs = false;
        return;
    }
    let mut open = app.windows.programs;
    egui::Window::new("my programs")
        .open(&mut open)
        .default_size([460.0, 360.0])
        .default_pos(egui::pos2(480.0, 120.0))
        .show(ctx, |ui| programs_ui(app, ui));
    app.windows.programs &= open;
}

fn ago(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

fn programs_ui(app: &mut App, ui: &mut Ui) {
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
    if app.programs.is_empty() {
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
                let p = &app.programs[&name];
                let current = app.program.as_deref() == Some(name.as_str());
                if ui.selectable_label(current, RichText::new(&name).monospace()).on_hover_text("open").clicked()
                    && !current
                {
                    act = Some(Act::Open(name.clone()));
                }
                ui.weak(format!("{} lines, {}", p.text.lines().count(), ago(now - p.modified)));
                if ui.small_button("download").clicked() {
                    act = Some(Act::Download(name.clone()));
                }
                if app.program_delete.as_deref() == Some(name.as_str()) {
                    ui.horizontal(|ui| {
                        ui.colored_label(crate::colors::error(ui), "delete?");
                        if ui.small_button("yes").clicked() {
                            act = Some(Act::Delete(name.clone()));
                        }
                        if ui.small_button("no").clicked() {
                            app.program_delete = None;
                        }
                    });
                } else if ui.small_button("delete…").clicked() {
                    app.program_delete = Some(name.clone());
                }
                ui.end_row();
            }
        });
    });
    match act {
        Some(Act::Open(name)) => app.guarded(PendingAction::Program(name)),
        Some(Act::Download(name)) => crate::platform::download(&name, app.programs[&name].text.as_bytes()),
        Some(Act::Delete(name)) => {
            app.program_delete = None;
            app.delete_program(&name);
        }
        None => {}
    }
}

/// "save in this browser": asks for the name of a new program.
pub fn save_name_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.save_name.take() else { return };
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
                Some(n) if app.programs.contains_key(n) && app.program.as_ref() != Some(n) => {
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
        app.save_name = Some(d);
    }
}
