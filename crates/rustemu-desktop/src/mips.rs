//! The emulator window of the MIPS workspace: the same controls as the 8086's, MIPS registers
//! grouped by their use and the code list (rustemu_ui::mips_ui), and the windows MIPS has.

use egui::Ui;

use rustemu_ui::app::{App, RunMode};
use rustemu_ui::tools::Tool;

use crate::emulator::{controls, item, stale_banner};

pub(crate) fn emulator_ui(app: &mut App, ui: &mut Ui) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("file", |ui| file_menu(app, ui));
        ui.menu_button("debug", |ui| debug_menu(app, ui));
        ui.menu_button("view", |ui| view_menu(app, ui));
        ui.menu_button("help", |ui| {
            if item(ui, true, "about…", "") {
                app.windows[Tool::About] = true;
            }
        });
    });
    stale_banner(app, ui);
    controls(app, ui, |_, _| {});
    ui.separator();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| rustemu_ui::mips_ui::registers_ui(app, ui));
        ui.add_space(14.0);
        ui.vertical(|ui| rustemu_ui::mips_ui::code_ui(app, ui));
    });
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        let w = &mut app.windows;
        ui.toggle_value(&mut w[Tool::RunIo], "Run I/O").on_hover_text("the program's console: output and input");
        ui.toggle_value(&mut w[Tool::Source], "source").on_hover_text("original source code");
        if ui
            .add_enabled(app.loaded(), egui::Button::new("reset"))
            .on_hover_text("load the program again: registers, memory and console as at the start")
            .clicked()
        {
            app.reload();
        }
        let w = &mut app.windows;
        ui.toggle_value(&mut w[Tool::Memory], "memory").on_hover_text("data segment, heap and stack");
        ui.toggle_value(&mut w[Tool::Vars], "vars").on_hover_text("the program's data labels");
        ui.toggle_value(&mut w[Tool::Watch], "watch");
        ui.toggle_value(&mut w[Tool::CallStack], "call stack");
    });
}

fn file_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(240.0);
    if item(ui, app.loaded(), "reload", "") {
        app.reload();
    }
    ui.menu_button("examples", |ui| rustemu_ui::examples_ui::examples_menu(app, ui, true));
    ui.separator();
    if item(ui, true, "reset emulator and memory", "") {
        app.reset_emulator_and_ram();
    }
    ui.separator();
    if item(ui, app.loaded(), "save the emulator's state", "") {
        app.save_emulator_state();
    }
    if item(ui, app.program.saved_state.is_some(), "load from previous state", "") {
        app.load_emulator_state();
    }
    ui.separator();
    if item(ui, true, "close the emulator", "") {
        app.close_emulator();
    }
}

fn debug_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(300.0);
    let loaded = app.loaded();
    let running = app.run != RunMode::Paused;
    let sel = app.mips.selected;
    let at = sel.map_or("(no selection)".to_string(), |a| format!("(0x{a:08x})"));
    let has_bp = sel.is_some_and(|a| app.session.target.breakpoints().contains(&a));
    if item(ui, loaded, "single step", "F8") {
        app.single_step();
    }
    if item(ui, loaded, "step over", "F10") {
        app.step_over();
    }
    if item(ui, loaded, "step out", "⇧F11") {
        app.step_out();
    }
    if item(ui, loaded && app.session.target.history_len() > 0, "step back", "⇧F8 / F6") {
        app.step_back();
    }
    ui.separator();
    if item(ui, true, "stop on condition…", "") {
        app.windows[Tool::StopCondition] = true;
    }
    if item(ui, loaded && !running, "run", "F5") {
        app.start_run();
    }
    if item(ui, running, "stop", "F5") {
        app.stop();
    }
    if item(ui, loaded && sel.is_some(), &format!("run until {at}"), "") {
        app.begin_exec();
        app.run = RunMode::RunTo(vec![sel.unwrap()]);
    }
    ui.separator();
    if item(ui, sel.is_some(), &format!("{} break point {at}", if has_bp { "clear" } else { "set" }), "") {
        app.toggle_breakpoint_at_addr(sel.unwrap());
    }
    if item(ui, loaded, "show current instruction (at PC)", "F7") {
        app.show_current_instruction();
    }
}

fn view_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(220.0);
    let w = &mut app.windows;
    ui.checkbox(&mut w[Tool::RunIo], "Run I/O console");
    ui.checkbox(&mut w[Tool::Source], "original source code");
    ui.separator();
    ui.checkbox(&mut w[Tool::Memory], "memory");
    ui.checkbox(&mut w[Tool::Vars], "variables");
    ui.checkbox(&mut w[Tool::Symbols], "symbol table");
    ui.checkbox(&mut w[Tool::Listing], "listing");
    ui.separator();
    ui.checkbox(&mut w[Tool::Watch], "watch");
    ui.checkbox(&mut w[Tool::Breakpoints], "breakpoints");
    ui.checkbox(&mut w[Tool::CallStack], "call stack");
    ui.checkbox(&mut w[Tool::StopCondition], "stop on condition");
    ui.separator();
    ui.checkbox(&mut app.dialogs.options, "options");
}
