//! Viewer for the emu8086.inc macro library: list of macros/procedures, docs and code.

use egui::{FontId, RichText, Ui};
use rustemu_asm::library::{Entry, EntryKind};

use crate::app::App;
use crate::highlight::{Palette, highlight};

#[derive(Default)]
pub struct LibraryState {
    pub open: bool,
    /// Bring the library to the front (the panels layout shows it as a tab).
    pub raise: bool,
    pub selected: Option<String>,
    scroll_to: Option<u32>,
    index: Vec<Entry>,
}

impl LibraryState {
    pub fn index(&mut self) -> &[Entry] {
        if self.index.is_empty() {
            self.index = rustemu_asm::library::index(rustemu_asm::EMU8086_INC);
        }
        &self.index
    }

    /// Opens the viewer at the definition of `name` (case-insensitive).
    pub fn show_entry(&mut self, name: &str) {
        let u = name.to_ascii_uppercase();
        let line = self.index().iter().find(|e| e.name == u).map(|e| e.line);
        if let Some(l) = line {
            self.open = true;
            self.raise = true;
            self.selected = Some(u);
            self.scroll_to = Some(l);
        }
    }

    pub fn find(&mut self, name: &str) -> Option<Entry> {
        let u = name.to_ascii_uppercase();
        self.index().iter().find(|e| e.name == u).cloned()
    }
}

/// Short title like `macro PRINT sdat` or `procedure SCAN_NUM`.
pub fn title(e: &Entry) -> String {
    match e.kind {
        EntryKind::Macro if e.params.is_empty() => format!("macro {}", e.name),
        EntryKind::Macro => format!("macro {} {}", e.name, e.params),
        EntryKind::Proc => format!("procedure {}  (CALL {})", e.name, e.name),
    }
}

pub fn library_window(app: &mut App, ctx: &egui::Context) {
    if app.panels() {
        return;
    }
    let mut open = app.library.open;
    egui::Window::new("emu8086.inc library")
        .open(&mut open)
        .default_pos(egui::pos2(160.0, 100.0))
        .default_size([860.0, 560.0])
        .show(ctx, |ui| library_ui(app, ui));
    app.library.open = open;
}

pub(crate) fn library_ui(app: &mut App, ui: &mut Ui) {
    let entries = app.library.index().to_vec();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            egui::ScrollArea::vertical().id_salt("lib_list").max_height(ui.available_height()).show(ui, |ui| {
                for (kind, header) in [(EntryKind::Macro, "Macros"), (EntryKind::Proc, "Procedures (CALL)")] {
                    ui.label(RichText::new(header).strong());
                    for e in entries.iter().filter(|e| e.kind == kind) {
                        let sel = app.library.selected.as_deref() == Some(e.name.as_str());
                        if ui.selectable_label(sel, RichText::new(&e.name).monospace()).clicked() {
                            app.library.selected = Some(e.name.clone());
                            app.library.scroll_to = Some(e.line);
                        }
                    }
                    ui.add_space(6.0);
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            match app.library.selected.as_ref().and_then(|n| entries.iter().find(|e| &e.name == n)) {
                Some(e) => {
                    ui.label(RichText::new(title(e)).strong().monospace());
                    ui.label(&e.doc);
                    if e.kind == EntryKind::Proc {
                        let define = entries
                            .iter()
                            .filter(|m| m.kind == EntryKind::Macro && m.line < e.line && e.end_line <= m.end_line)
                            .map(|m| m.name.clone())
                            .next();
                        if let Some(d) = define {
                            ui.weak(format!(
                                "Put {d} once in your program (e.g. after the final RET), then CALL {}.",
                                e.name
                            ));
                        }
                    }
                }
                None => {
                    ui.label(
                        "Write  include 'emu8086.inc'  at the top of the program to use these macros and procedures.",
                    );
                    ui.weak("Select a name on the left to see its description and code.");
                }
            }
            ui.separator();
            code_view(app, ui);
        });
    });
}

fn code_view(app: &mut App, ui: &mut Ui) {
    let font = FontId::monospace(12.5);
    let pal = Palette::for_theme(ui.visuals().dark_mode);
    let lines: Vec<&str> = rustemu_asm::EMU8086_INC.lines().collect();
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    let (sel_start, sel_end) = app
        .library
        .selected
        .as_ref()
        .and_then(|n| app.library.index.iter().find(|e| &e.name == n))
        .map(|e| (e.line, e.end_line))
        .unwrap_or((0, 0));
    let mut sa = egui::ScrollArea::both().id_salt("lib_code").auto_shrink([false, false]);
    if let Some(l) = app.library.scroll_to.take() {
        let step = row_h + ui.spacing().item_spacing.y;
        sa = sa.vertical_scroll_offset(((l as f32 - 4.0) * step).max(0.0));
    }
    let band = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(38, 46, 60)
    } else {
        egui::Color32::from_rgb(232, 240, 252)
    };
    sa.show_rows(ui, row_h, lines.len(), |ui, range| {
        for i in range {
            let n = i as u32 + 1;
            let mut job = highlight(lines[i], font.clone(), &pal);
            if (sel_start..=sel_end).contains(&n) {
                for s in &mut job.sections {
                    s.format.background = band;
                }
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{n:>4} ")).font(font.clone()).weak());
                ui.add(egui::Label::new(job).extend());
            });
        }
    });
}
