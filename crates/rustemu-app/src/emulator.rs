//! Emulator window (controls, registers, memory, disassembly) and the source window.

use egui::{Color32, DragValue, FontId, RichText, Ui, pos2};
use rustemu_core::cpu::{self, REG16_NAMES, SREG_NAMES};
use rustemu_core::disasm::disassemble;

use crate::app::{App, RunMode};

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

fn emulator_ui(app: &mut App, ui: &mut Ui) {
    let loaded = app.loaded();
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
        ui.separator();
        ui.vertical(|ui| code_view(app, ui));
    });
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        let w = &mut app.windows;
        ui.toggle_value(&mut w.screen, "screen");
        ui.toggle_value(&mut w.source, "source");
        ui.toggle_value(&mut w.memory, "memory");
        ui.toggle_value(&mut w.stack, "stack");
        ui.toggle_value(&mut w.flags, "flags");
        ui.toggle_value(&mut w.vars, "vars");
        ui.toggle_value(&mut w.symbols, "symbols");
        ui.toggle_value(&mut w.listing, "listing");
        ui.toggle_value(&mut w.log, "log");
        ui.toggle_value(&mut w.devices, "devices");
        ui.toggle_value(&mut w.console, "debug");
    });
}

fn reg_row(ui: &mut Ui, name: &str, v: &mut u16, prev: u16, split: bool) {
    let color = if *v != prev { CHANGED } else { ui.visuals().text_color() };
    let label = ui.label(RichText::new(name).monospace().strong()).id;
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
}

fn registers(app: &mut App, ui: &mut Ui) {
    let prev = app.prev_cpu.clone();
    let c = &mut app.session.machine.cpu;
    egui::Grid::new("regs").num_columns(4).spacing([6.0, 2.0]).striped(true).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new("H").weak());
        ui.label(RichText::new("L").weak());
        ui.label(RichText::new("dec").weak());
        ui.end_row();
        for i in [cpu::AX, cpu::BX, cpu::CX, cpu::DX] {
            reg_row(ui, REG16_NAMES[i], &mut c.regs[i], prev.regs[i], true);
        }
        for i in [cpu::CS, cpu::DS, cpu::ES, cpu::SS] {
            reg_row(ui, SREG_NAMES[i], &mut c.sregs[i], prev.sregs[i], false);
        }
        reg_row(ui, "IP", &mut c.ip, prev.ip, false);
        for i in [cpu::SP, cpu::BP, cpu::SI, cpu::DI] {
            reg_row(ui, REG16_NAMES[i], &mut c.regs[i], prev.regs[i], false);
        }
        let mut f = c.flags;
        reg_row(ui, "FL", &mut f, prev.flags, false);
        c.set_flags_word(f);
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
}

/// Near target offset of a disassembled `CALL`/`JMP`/`Jcc`/`LOOP`/`JCXZ` (`"CALL 0150h"` -> 0x150).
fn jump_target(text: &str) -> Option<u16> {
    let (mnem, arg) = text.split_once(' ')?;
    if !(mnem.starts_with('J') || mnem == "CALL" || mnem.starts_with("LOOP")) || arg.contains(['[', ':', ' ']) {
        return None;
    }
    u16::from_str_radix(arg.strip_suffix('h')?, 16).ok()
}

/// Finds the address to start the disassembly at so that the list stays stable while stepping.
fn disasm_start(app: &mut App, rows: usize) -> u32 {
    let m = &app.session.machine;
    let pc = m.cpu.pc();
    let mut a = app.code_view_start;
    let ip0 = m.cpu.ip.wrapping_sub(pc.wrapping_sub(a) as u16);
    let mut ip = ip0;
    for _ in 0..rows.saturating_sub(3) {
        if a == pc {
            return app.code_view_start;
        }
        let ins = disassemble(m.sys.mem.slice(a, 8), ip);
        a += ins.len as u32;
        ip = ip.wrapping_add(ins.len as u16);
        if a > pc {
            break;
        }
    }
    app.code_view_start = pc;
    pc
}

fn code_view(app: &mut App, ui: &mut Ui) {
    let rows = 22;
    let start = disasm_start(app, rows);
    let m = &app.session.machine;
    let pc = m.cpu.pc();
    let cs = m.cpu.cs();
    let mut ip = m.cpu.ip.wrapping_sub(pc.wrapping_sub(start) as u16);
    let mut a = start;
    let font = FontId::monospace(12.5);
    let mut clicked_bp: Option<u32> = None;
    let label_color =
        if ui.visuals().dark_mode { Color32::from_rgb(220, 220, 150) } else { Color32::from_rgb(120, 90, 20) };
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
            let ins = disassemble(&bytes, ip);
            let current = a == pc;
            let bp = m.breakpoints.contains(&a);
            let marker = if bp { "●" } else { " " };
            if ui.label(RichText::new(marker).color(Color32::from_rgb(220, 40, 40)).font(font.clone())).clicked() {
                clicked_bp = Some(a);
            }
            let hex: String = bytes[..ins.len.min(6)].iter().map(|b| format!("{b:02X}")).collect();
            let addr = format!("{cs:04X}:{ip:04X}");
            let mk = |s: String| {
                let t = RichText::new(s).font(font.clone());
                if current { t.background_color(Color32::from_rgb(40, 90, 160)).color(Color32::WHITE) } else { t }
            };
            ui.label(mk(addr).weak());
            ui.label(mk(format!("{hex:<10}")));
            let text = match jump_target(&ins.text).and_then(|off| session.symbol_at(rustemu_core::linear(cs, off))) {
                Some(name) => format!("{}  ; {name}", ins.text),
                None => ins.text.clone(),
            };
            let r = ui.add(egui::Label::new(mk(text)).sense(egui::Sense::click()));
            if r.double_clicked() {
                clicked_bp = Some(a);
            }
            ui.end_row();
            a += ins.len as u32;
            ip = ip.wrapping_add(ins.len as u16);
        }
    });
    ui.weak("double-click an instruction to toggle a breakpoint");
    if let Some(addr) = clicked_bp {
        // Map to a source line when possible, so the breakpoint survives reloads.
        // Macro code keeps an address breakpoint (its line may be expanded in many places).
        if let Some(loc) = app.session.location_at_addr(addr).filter(|l| l.origin.is_none()) {
            app.toggle_breakpoint_at_line(loc.file, loc.line);
        } else {
            let bps = &mut app.session.machine.breakpoints;
            if !bps.remove(&addr) {
                bps.insert(addr);
            }
        }
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
                ui.label(RichText::new(format!("▸ inside {what}")).strong().color(crate::colors::info(ui)));
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
                    ui.label(RichText::new("ℹ").color(crate::colors::info(ui))).on_hover_text(&e.doc);
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
            let bp = app.bp_lines.contains(&(fidx, line_no));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let dot = if bp {
                    RichText::new("●").color(Color32::from_rgb(220, 40, 40))
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
                ui.label(t);
            });
        }
    });
    if let Some(l) = toggle {
        app.toggle_breakpoint_at_line(fidx, l);
    }
}
