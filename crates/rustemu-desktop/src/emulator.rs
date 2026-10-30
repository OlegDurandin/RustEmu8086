//! Emulator window (controls, registers, memory, disassembly) and the source window.

use egui::{Color32, DragValue, FontId, RichText, Ui, pos2, vec2};
use rustemu_core::cpu::{self, REG16_NAMES, SREG_NAMES};

use rustemu_ui::app::{App, RunMode};

const CHANGED: Color32 = Color32::from_rgb(0xFF, 0x50, 0x50);

pub fn emulator_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.windows.emulator;
    egui::Window::new("emulator")
        .id(egui::Id::unique("emulator_window"))
        .open(&mut open)
        .default_pos(pos2(8.0, 64.0))
        .default_width(660.0)
        .show(ctx, |ui| emulator_ui(app, ui));
    app.windows.emulator = open;
}

/// Warning shown while the loaded program is older than the editor text.
pub fn stale_banner(app: &mut App, ui: &mut Ui) {
    if !app.is_stale() {
        return;
    }
    egui::Frame::new().fill(ui.visuals().warn_fg_color.gamma_multiply(0.18)).inner_margin(4.0).corner_radius(3.0).show(
        ui,
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(
                        "⚠ The source was changed after the program was loaded - the running code is outdated.",
                    )
                    .color(rustemu_ui::colors::warn(ui)),
                );
                if ui.button("Reassemble and load (F9)").clicked() {
                    app.emulate();
                }
            });
        },
    );
}

fn emulator_ui(app: &mut App, ui: &mut Ui) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("file", |ui| file_menu(app, ui));
        ui.menu_button("math", |ui| {
            if ui.button("multi base calculator").clicked() {
                app.calculator.open = true;
            }
            if ui.button("base converter").clicked() {
                app.windows.converter = true;
            }
        });
        ui.menu_button("debug", |ui| debug_menu(app, ui));
        ui.menu_button("view", |ui| view_menu(app, ui));
        ui.menu_button("virtual devices", |ui| virtual_devices_menu(app, ui));
        ui.menu_button("virtual drive", |ui| rustemu_ui::windows::virtual_drive_menu(app, ui));
        ui.menu_button("help", |ui| help_menu(app, ui));
    });
    let loaded = app.loaded();
    stale_banner(app, ui);
    ui.horizontal_wrapped(|ui| {
        if ui.button("📂 Load").on_hover_text("Open an executable or source file").clicked() {
            app.open_dialog();
        }
        if ui.add_enabled(loaded, egui::Button::new("⟲ Reload")).clicked() {
            app.reload();
        }
        if ui
            .add_enabled(loaded && app.session.machine.history_len() > 0, egui::Button::new("⏮ Step back"))
            .on_hover_text("Undo the last instruction (Shift+F8)")
            .clicked()
        {
            app.step_back();
        }
        if ui.add_enabled(loaded, egui::Button::new("⏭ Single step")).on_hover_text("F8").clicked() {
            app.single_step();
        }
        if ui
            .add_enabled(loaded, egui::Button::new("↷ Step over"))
            .on_hover_text("Run the current line to completion: macros, CALL, INT (F10)")
            .clicked()
        {
            app.step_over();
        }
        if ui
            .add_enabled(loaded, egui::Button::new("⏏ Step out"))
            .on_hover_text("Run until the current macro or procedure returns to the caller (⇧F11)")
            .clicked()
        {
            app.step_out();
        }
        let running = app.run != RunMode::Paused;
        let label = if running { "⏹ Stop" } else { "▶ Run" };
        if ui.add_enabled(loaded, egui::Button::new(label)).on_hover_text("F5").clicked() {
            if running { app.stop() } else { app.start_run() }
        }
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut app.settings.step_delay_ms, 0..=1000).suffix(" ms").text("step delay"));
        if app.settings.step_delay_ms == 0 {
            ui.weak("(full speed)");
        }
        if ui
            .checkbox(&mut app.settings.timer_irq, "timer IRQ")
            .on_hover_text(
                "Deliver IRQ0 18.2 times a second (INT 08h → INT 1Ch). The BIOS tick count advances either way.",
            )
            .changed()
        {
            app.session.machine.timer_irq = app.settings.timer_irq;
        }
    });
    ui.separator();

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| registers(app, ui));
        // A plain gap: a vertical separator would stretch the window to the bottom of the screen.
        ui.add_space(14.0);
        ui.vertical(|ui| code_view(app, ui));
    });
    ui.separator();
    bottom_buttons(app, ui);
}

/// A menu item with a right-aligned shortcut.
fn item(ui: &mut Ui, enabled: bool, text: &str, shortcut: &str) -> bool {
    ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(shortcut)).clicked()
}

/// "file" menu of the emulator window (Emu8086).
fn file_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(250.0);
    if item(ui, true, "load executable…", "") {
        app.open_dialog();
    }
    if item(ui, app.loaded(), "reload", "") {
        app.reload();
    }
    ui.separator();
    ui.menu_button("examples", |ui| {
        for (i, (name, _)) in rustemu_ui::examples::EXAMPLES.iter().enumerate() {
            if ui.button(*name).clicked() {
                app.emulate_example(i);
            }
        }
    });
    ui.separator();
    if item(ui, true, "reset emulator and ram", "") {
        app.reset_emulator_and_ram();
    }
    ui.separator();
    if item(ui, true, "set command line parameters", "") {
        app.cmdline_dialog = Some(app.session.command_line.clone());
    }
    ui.separator();
    if item(ui, app.loaded(), "save the emulator's state", "") {
        app.save_emulator_state();
    }
    if item(ui, app.saved_state.is_some(), "load from previous state", "") {
        app.load_emulator_state();
    }
    let recent: Vec<std::path::PathBuf> = app.settings.recent.iter().take(5).cloned().collect();
    if !recent.is_empty() {
        ui.separator();
        for p in recent {
            if ui.button(p.display().to_string()).clicked() {
                app.emulate_file(&p);
            }
        }
    }
    ui.separator();
    if item(ui, true, "close the emulator", "") {
        app.close_emulator();
    }
}

/// "view" menu of the emulator window (Emu8086).
fn view_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(260.0);
    let w = &mut app.windows;
    ui.checkbox(&mut w.console, "log and debug.exe emulation   F11");
    ui.separator();
    ui.checkbox(&mut w.value_viewer, "extended value viewer");
    ui.separator();
    ui.checkbox(&mut w.stack, "stack");
    ui.separator();
    ui.checkbox(&mut w.vars, "variables");
    ui.checkbox(&mut w.memory, "memory");
    ui.separator();
    ui.checkbox(&mut w.symbols, "symbol table");
    ui.checkbox(&mut w.listing, "listing");
    ui.separator();
    ui.checkbox(&mut w.source, "original source code");
    ui.separator();
    ui.checkbox(&mut app.options_open, "options");
    ui.separator();
    let w = &mut app.windows;
    ui.checkbox(&mut w.alu, "arithmetic & logical unit");
    ui.separator();
    ui.checkbox(&mut w.flags, "flags");
    ui.separator();
    ui.checkbox(&mut w.flag_analyser, "lexical flag analyzer");
    ui.separator();
    ui.checkbox(&mut w.ascii, "ascii codes");
    ui.separator();
    ui.checkbox(&mut w.screen, "emulator screen");
}

/// "virtual devices" menu: the device programs of Emu8086, shown as tabs of the devices window.
pub fn virtual_devices_menu(app: &mut App, ui: &mut Ui) {
    for (d, name) in rustemu_ui::devices_ui::Device::EXE {
        if ui.button(name).clicked() {
            app.device_tab = d;
            app.windows.devices = true;
        }
    }
}

/// "help" menu of the emulator window (Emu8086).
fn help_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(240.0);
    if item(ui, true, "documentation and tutorials", "F1") {
        app.reference.open = true;
    }
    ui.separator();
    if item(ui, true, "check for an update…", "") {
        // The project page lists the releases; there is no automatic updater.
        rustemu_ui::platform::open_url(rustemu_ui::app::RELEASES_URL);
    }
    ui.separator();
    if item(ui, true, "about…", "") {
        app.windows.about = true;
    }
}

/// "debug" menu of the emulator window (Emu8086); shortcuts are RustEmu8086's.
fn debug_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(330.0);
    let loaded = app.loaded();
    let running = app.run != RunMode::Paused;
    let sel = app.selected_addr();
    let at = sel.map_or("(no selection)".to_string(), |a| format!("({a:05X}h)"));
    let has_bp = sel.is_some_and(|a| app.session.machine.breakpoints.contains(&a));
    if item(ui, loaded, "single step", "F8") {
        app.single_step();
    }
    if item(ui, loaded, "step over", "F10") {
        app.step_over();
    }
    if item(ui, loaded, "step out", "⇧F11") {
        app.step_out();
    }
    ui.separator();
    if item(ui, loaded && app.session.machine.history_len() > 0, "step back", "⇧F8 / F6") {
        app.step_back();
    }
    ui.separator();
    if item(ui, true, "stop on condition…", "") {
        app.windows.stop_condition = true;
    }
    ui.separator();
    if item(ui, loaded && sel.is_some(), &format!("run until {at}"), "⌃F8") {
        app.run_until_selected();
    }
    if item(ui, loaded && !running, "run", "F5") {
        app.start_run();
    }
    if item(ui, running, "stop", "F5") {
        app.stop();
    }
    ui.separator();
    if item(ui, sel.is_some() && !has_bp, &format!("set break point {at}"), "") {
        app.toggle_breakpoint_at_addr(sel.unwrap());
    }
    if item(ui, has_bp, "clear break point", "") {
        app.toggle_breakpoint_at_addr(sel.unwrap());
    }
    if item(ui, !app.session.machine.breakpoints.is_empty(), "show current break point", "") {
        app.show_next_breakpoint();
    }
    ui.separator();
    if item(ui, loaded, "show current instruction (at CS:IP)", "F7") {
        app.show_current_instruction();
    }
    ui.separator();
    if item(ui, sel.is_some(), &format!("set CS:IP to selected position {at}"), "⌃⇧F7") {
        app.set_cs_ip_to_selected();
    }
}

/// The row of buttons at the bottom of the emulator window, as in Emu8086.
fn bottom_buttons(app: &mut App, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        let w = &mut app.windows;
        ui.toggle_value(&mut w.screen, "screen").on_hover_text("emulator screen");
        ui.toggle_value(&mut w.source, "source").on_hover_text("original source code");
        if ui
            .add_enabled(app.loaded(), egui::Button::new("reset"))
            .on_hover_text(
                "reset emulator: restore interrupt vector, close all file handles, clear screen and reload the program",
            )
            .clicked()
        {
            app.reload();
        }
        ui.menu_button("aux", |ui| {
            let w = &mut app.windows;
            ui.checkbox(&mut w.memory, "memory");
            ui.checkbox(&mut w.alu, "ALU");
            ui.add_enabled(false, egui::Checkbox::new(&mut false, "FPU"))
                .on_disabled_hover_text("the 8087 FPU is not emulated");
            ui.checkbox(&mut w.stop_condition, "stop on condition");
            ui.separator();
            ui.checkbox(&mut w.symbols, "symbol table");
            ui.checkbox(&mut w.listing, "listing");
            ui.separator();
            ui.checkbox(&mut w.breakpoints, "breakpoints");
            ui.checkbox(&mut w.watch, "watch");
            ui.checkbox(&mut w.call_stack, "call stack");
            ui.checkbox(&mut w.devices, "virtual devices");
            ui.checkbox(&mut w.log, "log");
        })
        .response
        .on_hover_text("auxiliary tools");
        let w = &mut app.windows;
        ui.toggle_value(&mut w.vars, "vars").on_hover_text("show defined variables, arrays and strings");
        ui.toggle_value(&mut w.console, "debug").on_hover_text("log and debug.exe emulation");
        ui.toggle_value(&mut w.stack, "stack").on_hover_text("memory at SS:SP from bottom to top");
        ui.toggle_value(&mut w.flags, "flags").on_hover_text("show flags register");
    });
}

/// One register row; returns true when its name was double-clicked (opens the value viewer).
fn reg_row(ui: &mut Ui, name: &str, v: &mut u16, prev: u16, split: bool) -> bool {
    let color = if *v != prev { CHANGED } else { ui.visuals().text_color() };
    let name_resp = ui
        .add(egui::Label::new(RichText::new(name).monospace().strong()).sense(egui::Sense::click()))
        .on_hover_text("double-click: extended value viewer");
    let label = name_resp.id;
    if split {
        let mut hi = (*v >> 8) as u8;
        let mut lo = *v as u8;
        let ch = ui.add(DragValue::new(&mut hi).hexadecimal(2, false, true).speed(0.0)).labelled_by(label);
        let cl = ui.add(DragValue::new(&mut lo).hexadecimal(2, false, true).speed(0.0)).labelled_by(label);
        if ch.changed() || cl.changed() {
            *v = ((hi as u16) << 8) | lo as u16;
        }
        ui.label(RichText::new(format!("{:5}", *v)).monospace().color(color).weak());
    } else {
        ui.add(DragValue::new(v).hexadecimal(4, false, true).speed(0.0)).labelled_by(label);
        ui.label("");
        ui.label(RichText::new(format!("{:5}", *v)).monospace().color(color).weak());
    }
    ui.end_row();
    name_resp.double_clicked()
}

fn registers(app: &mut App, ui: &mut Ui) {
    let prev = app.prev_cpu.clone();
    let c = &mut app.session.machine.cpu;
    let mut view: Option<&str> = None;
    egui::Grid::new("regs").num_columns(4).spacing([6.0, 2.0]).striped(true).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new("H").weak());
        ui.label(RichText::new("L").weak());
        ui.label(RichText::new("dec").weak());
        ui.end_row();
        for i in [cpu::AX, cpu::BX, cpu::CX, cpu::DX] {
            if reg_row(ui, REG16_NAMES[i], &mut c.regs[i], prev.regs[i], true) {
                view = Some(REG16_NAMES[i]);
            }
        }
        // Same order as in Emu8086: CS IP SS SP BP SI DI DS ES.
        if reg_row(ui, SREG_NAMES[cpu::CS], &mut c.sregs[cpu::CS], prev.sregs[cpu::CS], false) {
            view = Some("CS");
        }
        if reg_row(ui, "IP", &mut c.ip, prev.ip, false) {
            view = Some("IP");
        }
        if reg_row(ui, SREG_NAMES[cpu::SS], &mut c.sregs[cpu::SS], prev.sregs[cpu::SS], false) {
            view = Some("SS");
        }
        for i in [cpu::SP, cpu::BP, cpu::SI, cpu::DI] {
            if reg_row(ui, REG16_NAMES[i], &mut c.regs[i], prev.regs[i], false) {
                view = Some(REG16_NAMES[i]);
            }
        }
        for i in [cpu::DS, cpu::ES] {
            if reg_row(ui, SREG_NAMES[i], &mut c.sregs[i], prev.sregs[i], false) {
                view = Some(SREG_NAMES[i]);
            }
        }
    });
    ui.add_space(4.0);
    let f = c.flags;
    ui.horizontal(|ui| {
        use cpu::flags::*;
        for (n, b) in [("C", CF), ("Z", ZF), ("S", SF), ("O", OF), ("P", PF), ("A", AF), ("I", IF), ("D", DF)] {
            let on = f & b != 0;
            let changed = (f ^ prev.flags) & b != 0;
            let t = RichText::new(format!("{n}{}", on as u8)).monospace();
            ui.label(if changed {
                t.color(CHANGED)
            } else if on {
                t.strong()
            } else {
                t.weak()
            });
        }
    });
    ui.add_space(4.0);
    ui.label(RichText::new(format!("instructions: {}", c.instr_count)).weak().small());
    if let Some(name) = view {
        rustemu_ui::debugger::view_register(app, name);
    }
}

fn code_view(app: &mut App, ui: &mut Ui) {
    let rows = 22;
    let font = FontId::monospace(12.5);
    if let Some(code) = app.session.machine.terminated
        && !app.code_view_pinned
    {
        // CS:IP is left in the PSP or the DOS stub: nothing there is the program's code. The
        // banner takes the place of the list, so that the window keeps its size.
        let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 1.0;
        ui.vertical(|ui| {
            ui.set_min_size(vec2(420.0, rows as f32 * row_h));
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(format!("The program has terminated (exit code {code}).")).strong());
                ui.weak("Step back to see its last instructions, or reload it to run it again.");
            });
        });
        return;
    }
    let start = rustemu_ui::debug::disasm_start(app, rows);
    let m = &app.session.machine;
    let pc = m.cpu.pc();
    // Addresses are shown relative to CS, or to the segment of a shown breakpoint.
    let (cs, mut ip) = match app.code_selected {
        Some((s, o)) if app.code_view_pinned && rustemu_core::linear(s, o) == start => (s, o),
        _ => (m.cpu.cs(), m.cpu.ip.wrapping_sub(pc.wrapping_sub(start) as u16)),
    };
    let mut a = start;
    let mut clicked_bp: Option<u32> = None;
    let mut select: Option<(u16, u16)> = None;
    let selected = app.selected_addr();
    let sel_bg = ui.visuals().selection.bg_fill.gamma_multiply(0.45);
    let label_color =
        if ui.visuals().dark_mode { Color32::from_rgb(220, 220, 150) } else { Color32::from_rgb(120, 90, 20) };
    let data_color =
        if ui.visuals().dark_mode { Color32::from_rgb(130, 200, 170) } else { Color32::from_rgb(20, 120, 90) };
    let show_data = app.settings.disasm_data;
    let session = &app.session;
    egui::Grid::new("disasm").num_columns(4).spacing([8.0, 1.0]).show(ui, |ui| {
        for _ in 0..rows {
            if let Some(name) = session.symbol_at(a) {
                ui.label("");
                ui.label("");
                ui.label("");
                ui.label(RichText::new(format!("{name}:")).font(font.clone()).color(label_color));
                ui.end_row();
            }
            let bytes = m.sys.mem.slice(a, 8).to_vec();
            let ins = session.code_row(a, ip, show_data);
            let current = a == pc;
            let bp = m.breakpoints.contains(&a);
            let marker = if bp { "●" } else { " " };
            if ui.label(RichText::new(marker).color(Color32::from_rgb(220, 40, 40)).font(font.clone())).clicked() {
                clicked_bp = Some(a);
            }
            // Data rows can be long: their first bytes and an ellipsis fit the column.
            let n = if ins.data { 4 } else { 6 };
            let mut hex: String = bytes[..(ins.len as usize).min(n)].iter().map(|b| format!("{b:02X}")).collect();
            if ins.len as usize > n {
                hex.push('…');
            }
            let addr = format!("{cs:04X}:{ip:04X}");
            let is_selected = selected == Some(a) && !current;
            let mk = |s: String| {
                let t = RichText::new(s).font(font.clone());
                if current {
                    t.background_color(Color32::from_rgb(40, 90, 160)).color(Color32::WHITE)
                } else if is_selected {
                    t.background_color(sel_bg)
                } else {
                    t
                }
            };
            let row_ip = ip;
            if ui.add(egui::Label::new(mk(addr).weak()).sense(egui::Sense::click())).clicked() {
                select = Some((cs, row_ip));
            }
            if ui.add(egui::Label::new(mk(format!("{hex:<10}"))).sense(egui::Sense::click())).clicked() {
                select = Some((cs, row_ip));
            }
            let text = match rustemu_ui::debug::jump_target(&ins.text)
                .and_then(|off| session.symbol_at(rustemu_core::linear(cs, off)))
            {
                Some(name) => format!("{}  ; {name}", ins.text),
                None => ins.text.clone(),
            };
            let text = if ins.data && !current { mk(text).color(data_color) } else { mk(text) };
            let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
            if r.double_clicked() {
                clicked_bp = Some(a);
            } else if r.clicked() {
                select = Some((cs, row_ip));
            }
            ui.end_row();
            a += ins.len;
            ip = ip.wrapping_add(ins.len as u16);
        }
    });
    ui.horizontal(|ui| {
        ui.weak("click: select (debug menu), double-click: toggle a breakpoint");
        ui.checkbox(&mut app.settings.disasm_data, "data as DB").on_hover_text(
            "Show the program's data (DB, DW, ...) as data and the zeros after the program as DUP, \
             not as instructions",
        );
    });
    if let Some(s) = select {
        app.code_selected = Some(s);
    }
    if let Some(addr) = clicked_bp {
        app.toggle_breakpoint_at_addr(addr);
    }
}

pub fn source_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.windows.source;
    egui::Window::new("original source code")
        .id(egui::Id::unique("source_window"))
        .open(&mut open)
        .default_pos(pos2(700.0, 540.0))
        .default_size([700.0, 300.0])
        .show(ctx, |ui| source_ui(app, ui));
    app.windows.source = open;
}

/// Innermost macro/procedure of `entries` that contains `line`.
fn enclosing(entries: &[rustemu_asm::library::Entry], line: u32) -> Option<&rustemu_asm::library::Entry> {
    entries.iter().filter(|e| e.line <= line && line <= e.end_line).min_by_key(|e| e.end_line - e.line)
}

fn source_ui(app: &mut App, ui: &mut Ui) {
    if app.source_files.is_empty() {
        ui.weak("No source (binary loaded).");
        return;
    }
    let nfiles = app.source_files.len();
    stale_banner(app, ui);
    // Inside macro code the position is shown in the macro definition (e.g. emu8086.inc).
    let view = app.session.current_view(app.show_macro_code).filter(|v| v.file < nfiles);
    let shown = view.as_ref().map(|v| (v.file, v.line));
    if let Some((f, _)) = shown
        && app.follow_source
        && f < nfiles
    {
        app.source_file_idx = f;
    }
    ui.horizontal(|ui| {
        let idx = app.source_file_idx.min(nfiles - 1);
        egui::ComboBox::from_label("file").selected_text(app.source_files[idx].0.clone()).show_ui(ui, |ui| {
            for (i, (n, _)) in app.source_files.iter().enumerate() {
                ui.selectable_value(&mut app.source_file_idx, i, n);
            }
        });
        ui.checkbox(&mut app.follow_source, "follow execution");
        ui.checkbox(&mut app.show_macro_code, "step into macros").on_hover_text(
            "Show code generated by macros (PRINT, PUTC, ...) and library procedures in their definition",
        );
    });

    // Banner: which macro / procedure the current instruction belongs to.
    if let Some(v) = &view
        && v.level > 0
    {
        let (of, ol) = (v.file, v.line);
        let (sf, sl) = v.statement;
        let entry = app.source_index.get(of).and_then(|idx| enclosing(idx, ol)).cloned();
        let what = match &entry {
            Some(e) if e.kind == rustemu_asm::library::EntryKind::Proc => format!("procedure {}", e.name),
            Some(e) => format!("macro {}", e.name),
            None => "macro code".to_string(),
        };
        let user_file = app.source_files.get(sf).map(|f| f.0.clone()).unwrap_or_default();
        egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(4.0).corner_radius(3.0).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("▸ inside {what}")).strong().color(rustemu_ui::colors::info(ui)));
                ui.weak(format!("({} line {ol})", app.source_files[of].0));
                if ui.link(format!("statement: {user_file} line {sl}")).clicked() {
                    app.follow_source = false;
                    app.source_file_idx = sf;
                    app.source_scroll_to = Some(sl);
                }
                ui.weak("— ⇧F11 step out");
                if let Some(e) = &entry
                    && !e.doc.is_empty()
                {
                    ui.label(RichText::new("ℹ").color(rustemu_ui::colors::info(ui))).on_hover_text(&e.doc);
                }
            });
        });
    }

    let fidx = app.source_file_idx.min(nfiles - 1);
    let font = FontId::monospace(13.0);
    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 2.0;
    let n = app.source_files[fidx].1.len();
    let cur_line = shown.filter(|(f, _)| *f == fidx).map(|(_, l)| l);
    // The statement that invoked the macro, highlighted more softly.
    let call_line = view
        .as_ref()
        .filter(|v| v.level > 0 && v.statement.0 == fidx && cur_line != Some(v.statement.1))
        .map(|v| v.statement.1);
    let pos = cur_line.map(|l| (fidx, l));
    if app.last_source_pos != pos
        && let Some(l) = cur_line
    {
        app.source_scroll_to = Some(l);
    }
    app.last_source_pos = pos;
    // Re-applied for a few frames: after switching to a longer file the scroll area only knows
    // the new content height from the next frame on.
    if let Some(l) = app.source_scroll_to.take() {
        app.source_scroll_pending = Some((l, 3));
    }
    let mut scroll_to = None;
    if let Some((l, n)) = app.source_scroll_pending {
        scroll_to = Some(l);
        app.source_scroll_pending = (n > 1).then_some((l, n - 1));
        ui.ctx().request_repaint();
    }
    let mut toggle: Option<u32> = None;
    let mut run_to: Option<u32> = None;
    // One scroll state per file, so a short file does not limit scrolling in a long one.
    let mut sa = egui::ScrollArea::both().id_salt(("source_scroll", fidx)).auto_shrink([false, false]);
    if scroll_to.is_some() {
        // No rubber-band spring while jumping: it would pull the view back to stale bounds.
        let k = &mut ui.spacing_mut().scroll.kinetic;
        k.rubber_band = false;
        k.decay_time = 0.0;
    }
    if let Some(l) = scroll_to {
        let h = ui.available_height();
        let step = row_h + ui.spacing().item_spacing.y;
        sa = sa.vertical_scroll_offset(((l as f32 - 1.0) * step - h / 2.0).max(0.0));
    }
    let soft = if ui.visuals().dark_mode { Color32::from_rgb(45, 60, 85) } else { Color32::from_rgb(205, 222, 245) };
    sa.show_rows(ui, row_h, n, |ui, range| {
        for i in range {
            let line_no = i as u32 + 1;
            let text = &app.source_files[fidx].1[i];
            let bp = app.bp_lines.get(&(fidx, line_no));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let dot = if let Some(bp) = bp {
                    RichText::new("●").color(rustemu_ui::debugger::marker_color(bp))
                } else {
                    RichText::new("○").weak()
                };
                if ui
                    .add(egui::Label::new(dot.font(font.clone())).sense(egui::Sense::click()))
                    .on_hover_text("toggle breakpoint")
                    .clicked()
                {
                    toggle = Some(line_no);
                }
                ui.label(RichText::new(format!("{line_no:>4}")).font(font.clone()).weak());
                let t = RichText::new(text.replace('\t', "        ")).font(font.clone());
                let t = if cur_line == Some(line_no) {
                    t.background_color(Color32::from_rgb(40, 90, 160)).color(Color32::WHITE)
                } else if call_line == Some(line_no) {
                    t.background_color(soft)
                } else {
                    t
                };
                ui.add(egui::Label::new(t).sense(egui::Sense::click())).context_menu(|ui| {
                    if ui.button("Run to this line").clicked() {
                        run_to = Some(line_no);
                    }
                    if ui.button("Toggle breakpoint").clicked() {
                        toggle = Some(line_no);
                    }
                });
            });
        }
    });
    if let Some(l) = toggle {
        app.toggle_breakpoint_at_line(fidx, l);
    }
    if let Some(l) = run_to {
        app.run_to_line(fidx, l);
    }
}
