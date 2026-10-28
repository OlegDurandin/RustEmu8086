//! Top bar (run controls), tab row with the main menu, status bar.

use egui::{Align, Align2, Color32, FontId, Layout, RichText, Sense, Ui, vec2};
use rustemu_core::linear;
use rustemu_ui::app::{App, PendingAction, RunMode};

use crate::Tab;
use crate::TabletState;
use crate::theme::pal;
use crate::widgets::{Icon, badge, icon_button, paint_icon, small_icon};

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
        let (r, _) = ui.allocate_exact_size(vec2(26.0, 32.0), Sense::hover());
        paint_icon(ui.painter(), Icon::File, r.center(), 26.0, pal::WEAK);
        let dirty = if app.dirty() { " •" } else { "" };
        ui.label(RichText::new(format!("{}{dirty}", app.file_name())).size(19.0).color(pal::TEXT));
        ui.add_space(16.0);
        if ui.available_width() > 640.0 {
            badge(ui, "8086 REAL MODE");
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let running = app.run != RunMode::Paused;
            let compact = ui.available_width() < 560.0;
            let l = |s: &'static str| if compact { "" } else { s };
            if icon_button(ui, Icon::Reset, l("Reset"), None, true)
                .on_hover_text("Reload the program (assembles the editor text when it changed)")
                .clicked()
            {
                reset(app);
            }
            if icon_button(ui, Icon::Step, l("Step"), None, !running).on_hover_text("One instruction (F8)").clicked() {
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
            egui::Popup::menu(&r).show(|ui| main_menu(app, ui));
            if small_icon(ui, Icon::Keyboard, "on-screen keyboard").clicked() {
                t.keypad.open = !t.keypad.open;
            }
            if app.is_stale() {
                ui.colored_label(pal::AMBER, "edited since the last build — Run / Step / Reset assemble again");
            }
        });
    });
}

fn main_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(240.0);
    if ui.button("New…").clicked() {
        app.guarded(PendingAction::New);
        ui.close();
    }
    if rustemu_ui::platform::has_file_dialogs() && ui.button("Open…").clicked() {
        app.guarded(PendingAction::Open(None));
        ui.close();
    }
    ui.menu_button("Examples", |ui| {
        for (i, (name, _)) in rustemu_ui::examples::EXAMPLES.iter().enumerate() {
            if ui.button(*name).clicked() {
                app.guarded(PendingAction::Example(i));
                ui.close();
            }
        }
    });
    if rustemu_ui::platform::has_file_dialogs() {
        if ui.button("Save").clicked() {
            app.save();
            ui.close();
        }
        if ui.button("Save as…").clicked() {
            app.save_as();
            ui.close();
        }
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
    let w = &mut app.windows;
    ui.checkbox(&mut w.devices, "Virtual devices");
    ui.checkbox(&mut w.vars, "Variables");
    ui.checkbox(&mut w.symbols, "Symbol table");
    ui.checkbox(&mut w.listing, "Listing");
    ui.checkbox(&mut w.alu, "ALU");
    ui.checkbox(&mut w.stop_condition, "Stop on condition");
    ui.checkbox(&mut w.console, "Debug console");
    ui.separator();
    if ui.button("Reference").clicked() {
        app.reference.open = true;
        ui.close();
    }
    if ui.button("Calculator").clicked() {
        app.calculator.open = true;
        ui.close();
    }
    let w = &mut app.windows;
    ui.checkbox(&mut w.converter, "Base converter");
    ui.checkbox(&mut w.ascii, "ASCII codes");
    if ui.button("Options").clicked() {
        app.options_open = true;
        ui.close();
    }
    if ui.button("About").clicked() {
        app.windows.about = true;
        ui.close();
    }
}

pub(crate) fn status_bar(app: &mut App, t: &mut TabletState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        let m = &app.session.machine;
        let (state, color) = if !app.loaded() {
            ("Not loaded", pal::WEAK)
        } else if m.terminated.is_some() {
            ("Terminated", pal::WEAK)
        } else if app.run != RunMode::Paused {
            if m.is_waiting_input() { ("Waiting for input", pal::AMBER) } else { ("Running", pal::TEAL) }
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
            let c = &m.cpu;
            ui.label(RichText::new(format!("CS:IP {:04X}:{:04X}", c.cs(), c.ip)).size(16.0).color(pal::TEXT));
            sep(ui);
            ui.label(RichText::new(format!("{} instructions", c.instr_count)).size(16.0).color(pal::TEXT));
        }
        let status = app.status.clone();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let size = app.session.machine.loaded.as_ref().map_or(0, |l| l.image_len);
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
