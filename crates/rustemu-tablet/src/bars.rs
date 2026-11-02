//! Top bar (run controls), tab row with the main menu, status bar.

use egui::{Align, Align2, Color32, FontId, Layout, RichText, Sense, Ui, vec2};
use rustemu_core::linear;
use rustemu_ui::app::{App, PendingAction, RunMode};

use crate::Tab;
use crate::TabletState;
use crate::theme::pal;
use crate::widgets::{Icon, badge, icon_button, paint_icon, small_icon};
use rustemu_ui::tools::Tool;

// ---- execution commands ------------------------------------------------------------------------

/// The editor text is assembled again before running when it changed.
fn ensure_built(app: &mut App) -> bool {
    if !app.loaded() || app.is_stale() {
        app.emulate();
        return false;
    }
    true
}

fn run(app: &mut App) {
    if !app.loaded() || app.is_stale() {
        app.emulate();
        if !app.loaded() || app.is_stale() {
            return;
        }
    }
    app.start_run();
}

fn step(app: &mut App) {
    if ensure_built(app) {
        app.single_step();
    }
}

fn reset(app: &mut App) {
    if !app.loaded() || app.is_stale() {
        app.emulate();
    } else {
        app.reload();
    }
}

// ---- top bar, tabs, status bar -----------------------------------------------------------------

pub(crate) fn top_bar(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.set_min_height(44.0);
        let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
        paint_icon(ui.painter(), Icon::Chip, r.center(), 36.0, pal::TEXT);
        ui.label(RichText::new("RustEmu8086").size(24.0).strong().color(pal::TEXT));
        ui.add_space(8.0);
        let (r, _) = ui.allocate_exact_size(vec2(2.0, 32.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, pal::BORDER);
        ui.add_space(8.0);
        project_button(app, ui);
        ui.add_space(16.0);
        if ui.available_width() > 640.0 {
            badge(ui, "8086 REAL MODE");
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let running = app.run != RunMode::Paused;
            // A step while the program waits for a key would wait again: type the key first.
            let waiting = app.waiting_for_key();
            let compact = ui.available_width() < 560.0;
            let l = |s: &'static str| if compact { "" } else { s };
            if icon_button(ui, Icon::Reset, l("Reset"), None, true)
                .on_hover_text("Reload the program (assembles the editor text when it changed)")
                .clicked()
            {
                reset(app);
            }
            let step_tip = if waiting { "The program waits for a key: type it first" } else { "One instruction (F8)" };
            if icon_button(ui, Icon::Step, l("Step"), None, !running && !waiting).on_hover_text(step_tip).clicked() {
                step(app);
            }
            if icon_button(ui, Icon::Pause, l("Pause"), None, running).on_hover_text("Stop (F5 / Esc)").clicked() {
                app.stop();
            }
            if icon_button(ui, Icon::Play, l("Run"), Some(pal::TEAL), !running)
                .on_hover_text("Assemble when needed and run (F5)")
                .clicked()
            {
                run(app);
            }
        });
    });
}

/// The file name in the top bar: it opens the project menu.
fn project_button(app: &mut App, ui: &mut Ui) {
    let dirty = if app.dirty() { " •" } else { "" };
    let name = format!("{}{dirty}", app.file_name());
    let font = FontId::proportional(19.0);
    let text_w = ui.fonts_mut(|f| f.layout_no_wrap(name.clone(), font.clone(), pal::TEXT).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(12.0 + 26.0 + 8.0 + text_w + 34.0, 44.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "project menu"));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let p = ui.painter();
    let stroke = if open || resp.hovered() { pal::LINK } else { pal::BORDER };
    p.rect(rect, 6.0, pal::FIELD, egui::Stroke::new(1.5, stroke), egui::StrokeKind::Inside);
    paint_icon(p, Icon::File, egui::pos2(rect.left() + 25.0, rect.center().y), 24.0, pal::WEAK);
    p.text(egui::pos2(rect.left() + 46.0, rect.center().y), Align2::LEFT_CENTER, name, font, pal::TEXT);
    // A chevron: the name is a menu.
    let c = egui::pos2(rect.right() - 17.0, rect.center().y + 2.0);
    p.add(egui::Shape::line(
        vec![c + vec2(-5.0, -3.0), c + vec2(0.0, 2.0), c + vec2(5.0, -3.0)],
        egui::Stroke::new(2.0, pal::WEAK),
    ));
    egui::Popup::menu(&resp).show(|ui| project_menu(app, ui));
}

/// A row of the project menu: an icon, the action and what it does.
fn menu_row(ui: &mut Ui, icon: Icon, title: &str, hint: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(440.0, 52.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let p = ui.painter();
    if resp.hovered() || resp.is_pointer_button_down_on() {
        p.rect_filled(rect, 6.0, pal::BORDER);
    }
    paint_icon(p, icon, egui::pos2(rect.left() + 22.0, rect.center().y), 22.0, pal::TEXT);
    p.text(
        egui::pos2(rect.left() + 46.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(17.0),
        pal::TEXT,
    );
    p.text(
        egui::pos2(rect.right() - 12.0, rect.center().y),
        Align2::RIGHT_CENTER,
        hint,
        FontId::proportional(14.0),
        pal::WEAK,
    );
    resp.clicked()
}

/// The project menu: a new file, a file from the device, an example, and keeping, exporting or
/// sharing the source.
fn project_menu(app: &mut App, ui: &mut Ui) {
    let browser = app.front.browser;
    ui.add_space(4.0);
    ui.label(RichText::new(format!("  PROJECT · {}", app.file_name())).monospace().color(pal::WEAK));
    ui.add_space(4.0);
    let mut done = false;
    if menu_row(ui, Icon::Plus, "New ASM file", "Blank or a template") {
        app.guarded(PendingAction::New);
        done = true;
    }
    let open_hint = "ASM · COM · EXE · BIN";
    if menu_row(ui, Icon::Folder, "Open from Files…", open_hint) {
        app.guarded(if browser { PendingAction::Upload } else { PendingAction::Open(None) });
        done = true;
    }
    if menu_row(ui, Icon::Book, "Example library", "Explore & load code") {
        app.dialogs.more_examples = Some(false);
        done = true;
    }
    ui.separator();
    if menu_row(ui, Icon::Floppy, "Save draft", if browser { "In this browser" } else { "To its file" }) {
        app.save();
        done = true;
    }
    if browser && menu_row(ui, Icon::File, "My drafts", "Saved in this browser") {
        app.windows[Tool::Programs] = true;
        done = true;
    }
    if menu_row(ui, Icon::Download, "Export .asm", if browser { "Download source" } else { "Save a copy as…" }) {
        if browser {
            app.download_source();
        } else {
            app.save_as();
        }
        done = true;
    }
    let share_hint = if browser { "Share sheet" } else { "Link to the web version" };
    if menu_row(ui, Icon::Share, "Share source", share_hint) {
        app.share_source();
        done = true;
    }
    if done {
        ui.close();
    }
}

pub(crate) fn tabs_row(app: &mut App, t: &mut TabletState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for (tab, name) in [(Tab::Editor, "Editor"), (Tab::Debugger, "Debugger"), (Tab::Memory, "Memory")] {
            let on = t.tab == tab;
            let font = FontId::monospace(15.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(124.0, 40.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, name));
            let fill = if on {
                pal::BLUE
            } else if resp.hovered() {
                pal::BORDER
            } else {
                pal::HEADER
            };
            ui.painter().rect_filled(rect, 4.0, fill);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                name,
                font,
                if on { Color32::WHITE } else { pal::TEXT },
            );
            if resp.clicked() {
                t.tab = tab;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let r = small_icon(ui, Icon::Menu, "file, examples, tools");
            egui::Popup::menu(&r).show(|ui| main_menu(app, t, ui));
            if small_icon(ui, Icon::Keyboard, "on-screen keyboard").clicked() {
                t.keypad.open = !t.keypad.open;
            }
            if small_icon(ui, Icon::Share, "share link").clicked() {
                app.dialogs.share = Some(rustemu_ui::share::ShareDialog { run: false });
            }
            if app.is_stale() {
                ui.colored_label(pal::AMBER, "edited since the last build — Run / Step / Reset assemble again");
            }
        });
    });
}

fn main_menu(app: &mut App, t: &mut TabletState, ui: &mut Ui) {
    ui.set_min_width(240.0);
    // Files are in the project menu (the file name in the top bar).
    ui.menu_button("Examples", |ui| rustemu_ui::examples_ui::examples_menu(app, ui, false));
    if ui.button("Share link…").clicked() {
        app.dialogs.share = Some(rustemu_ui::share::ShareDialog { run: false });
        ui.close();
    }
    ui.separator();
    if ui.button("Assemble and load").clicked() {
        app.emulate();
        ui.close();
    }
    let loaded = app.loaded();
    if ui.add_enabled(loaded, egui::Button::new("Step back")).clicked() {
        app.step_back();
        ui.close();
    }
    if ui.add_enabled(loaded, egui::Button::new("Step over")).clicked() {
        app.step_over();
        ui.close();
    }
    if ui.add_enabled(loaded, egui::Button::new("Step out")).clicked() {
        app.step_out();
        ui.close();
    }
    if ui.button("Run to cursor").clicked() {
        app.run_to_cursor();
        ui.close();
    }
    ui.separator();
    // Drive C: (its files in the browser, its folder on the desktop) and the floppies, as in the
    // desktop layout.
    ui.menu_button("Virtual drive", |ui| rustemu_ui::drive_ui::virtual_drive_menu(app, ui));
    let memory_drive = app.session.dos_memory_drive().is_some();
    let w = &mut app.windows;
    if memory_drive {
        ui.checkbox(&mut w[Tool::Drive], "Drive C: files");
    }
    ui.checkbox(&mut w[Tool::Devices], "Virtual devices");
    ui.checkbox(&mut w[Tool::Vars], "Variables");
    ui.checkbox(&mut w[Tool::Symbols], "Symbol table");
    ui.checkbox(&mut w[Tool::Listing], "Listing");
    ui.checkbox(&mut w[Tool::Alu], "ALU");
    ui.checkbox(&mut w[Tool::Fpu], "FPU");
    ui.checkbox(&mut w[Tool::StopCondition], "Stop on condition");
    // A tool of the Debugger tab, not a window over everything.
    if ui.button("Debug log").clicked() {
        t.tab = Tab::Debugger;
        t.debug.tool = Some(crate::debugger::DebugTool::Console);
        ui.close();
    }
    ui.separator();
    if ui.button("Reference").clicked() {
        app.windows[Tool::Reference] = true;
        ui.close();
    }
    if ui.button("Calculator").clicked() {
        app.windows[Tool::Calculator] = true;
        ui.close();
    }
    let w = &mut app.windows;
    ui.checkbox(&mut w[Tool::Converter], "Base converter");
    ui.checkbox(&mut w[Tool::Ascii], "ASCII codes");
    if app.front.can_switch_layout && ui.button("Desktop layout").clicked() {
        app.front.switch_layout = true;
        ui.close();
    }
    ui.menu_button("Options", |ui| rustemu_ui::dialogs::options_menu(app, ui));
    if ui.button("About").clicked() {
        app.windows[Tool::About] = true;
        ui.close();
    }
}

pub(crate) fn status_bar(app: &mut App, t: &mut TabletState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        let m = &*app.session.target;
        let (state, color) = if !app.loaded() {
            ("Not loaded", pal::WEAK)
        } else if m.terminated().is_some() {
            ("Terminated", pal::WEAK)
        } else if m.is_waiting_input() {
            ("Waiting for input", pal::AMBER)
        } else if app.run != RunMode::Paused {
            ("Running", pal::TEAL)
        } else {
            ("Paused", pal::TEXT)
        };
        ui.label(RichText::new(state).size(16.0).color(color));
        let sep = |ui: &mut Ui| ui.label(RichText::new("·").size(16.0).color(pal::WEAK));
        if t.tab == Tab::Memory {
            sep(ui);
            let a = t.mem.selected.unwrap_or(linear(t.mem.seg.value(app), t.mem.mem_off));
            let seg = t.mem.seg.value(app);
            let off = a.wrapping_sub((seg as u32) << 4) as u16;
            ui.label(RichText::new(format!("Address {seg:04X}:{off:04X}")).size(16.0).color(pal::TEXT));
            sep(ui);
            ui.label(RichText::new("1 MB address space").size(16.0).color(pal::TEXT));
        } else if app.loaded() {
            sep(ui);
            let at = format!("{} {}", m.pc_name(), m.pc_text());
            ui.label(RichText::new(at).size(16.0).color(pal::TEXT));
            sep(ui);
            ui.label(RichText::new(format!("{} instructions", m.instr_count())).size(16.0).color(pal::TEXT));
        }
        let status = app.status.clone();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let size = app.session.x86().and_then(|m| m.loaded.as_ref()).map_or(0, |l| l.image_len);
            badge(ui, &format!("{size:X}h bytes"));
            let speed = t.speed;
            let s = if app.run == RunMode::Paused || speed == 0.0 {
                "paused".to_string()
            } else if speed >= 1e6 {
                format!("{:.1} M instr/s", speed / 1e6)
            } else {
                format!("{:.0} K instr/s", speed / 1e3)
            };
            badge(ui, &s);
            if ui.available_width() > 560.0 {
                badge(ui, "REAL MODE");
                badge(ui, "8086");
            }
            // The last message (stepped back, not found, assembly failed...) in the space left.
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if !status.is_empty() {
                    sep(ui);
                    let c = if status.contains("failed") || status.starts_with("Error") { pal::RED } else { pal::WEAK };
                    ui.add(egui::Label::new(RichText::new(status).size(15.0).color(c)).truncate());
                }
            });
        });
    });
}
