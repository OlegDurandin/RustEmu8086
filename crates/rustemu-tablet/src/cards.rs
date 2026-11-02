//! Cards of the Editor and Debugger tabs: editor, output, call stack, breakpoints, watch, execution
//! trace (the CPU is in `cpu.rs`, the step and the code in `debugger.rs`).

use egui::{Align, Align2, FontId, Layout, Rect, RichText, Sense, TextStyle, Ui, pos2, vec2};
use rustemu_core::disasm::disassemble;
use rustemu_ui::app::App;

use crate::TabletState;
use crate::theme::pal;
use crate::widgets::{
    HEAD_H, Icon, ROW_H, card, field_style, plain_card, small_icon, table_header, toggle_switch, value_box,
};
use crate::{Output, Tab};

// ---- cards -------------------------------------------------------------------------------------

pub(crate) fn editor_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let mut b = plain_card(ui, rect, "editor");
    if app.editor.find.open {
        b.horizontal(|ui| rustemu_ui::find::find_bar(app, ui));
    }
    rustemu_ui::editor::editor(app, &mut b);
}

pub(crate) fn output_card(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "OUTPUT");
    let tab = t.output;
    // MIPS: the screen is the Run I/O console, with a line to type the input.
    let mips = app.session.mips().is_some();
    if !(mips && tab == Output::Screen)
        && small_icon(
            &mut head,
            Icon::Trash,
            if tab == Output::Screen { "clear the screen" } else { "clear the messages" },
        )
        .clicked()
    {
        match tab {
            Output::Screen => {
                if let Some(m) = app.session.x86_mut() {
                    let m = &mut m.sys.mem;
                    let mode = rustemu_core::video::Video::mode(m);
                    rustemu_core::video::Video::set_mode(m, mode);
                }
            }
            Output::Messages => app.build.messages.clear(),
        }
    }
    let errors = app.build.messages.iter().filter(|m| m.error).count();
    let msg = if errors > 0 { format!("messages ({errors})") } else { "messages".into() };
    for (o, name) in [(Output::Messages, msg.as_str()), (Output::Screen, if mips { "Run I/O" } else { "screen" })] {
        if head.selectable_label(t.output == o, name).clicked() {
            t.output = o;
        }
    }
    match tab {
        Output::Screen if mips => rustemu_ui::mips_ui::console_ui(app, &mut b),
        Output::Screen => {
            let waiting = app.session.target.is_waiting_input();
            let avail = b.available_size() - vec2(0.0, if waiting { 48.0 } else { 26.0 });
            let resp = b
                .vertical_centered(|ui| rustemu_ui::screen::screen_view(app, ui, rustemu_ui::screen::Fit::Whole(avail)))
                .inner;
            if resp.clicked() {
                t.keypad.open = true;
            }
            if app.session.target.is_waiting_input() {
                waiting_banner(app, t, &mut b);
            } else if resp.has_focus() {
                b.label(RichText::new("keys go to the emulated PC").color(pal::WEAK));
            } else {
                b.label(RichText::new("tap the screen to type").color(pal::WEAK));
            }
        }
        Output::Messages => {
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(&mut b, |ui| {
                if app.build.messages.is_empty() {
                    ui.label(
                        RichText::new("No messages. Run assembles the program; errors are listed here.")
                            .color(pal::WEAK),
                    );
                }
                let mut jump = None;
                for (i, m) in app.build.messages.iter().enumerate() {
                    let color = if m.error { pal::RED } else { pal::TEXT };
                    let r =
                        ui.add(egui::Label::new(RichText::new(&m.text).monospace().color(color)).sense(Sense::click()));
                    if m.loc.is_some() && r.clicked() {
                        jump = Some(i);
                    }
                }
                if let Some(i) = jump {
                    app.jump_to_message(i);
                }
            });
        }
    }
}

/// Why the program stopped: it waits for a key in `INT nn` of a line of the program; the PC
/// keyboard types it. Waiting is not a debugger pause: Run / Pause do not end it.
fn waiting_banner(app: &App, t: &mut TabletState, ui: &mut Ui) {
    let s = &app.session;
    let int = s.bios_caller().map(|(cs, ip)| {
        let a = rustemu_core::linear(cs, ip);
        s.target.disassemble(a, ip as u32).text
    });
    let at = s.current_view(app.settings.show_macro_code).map(|v| rustemu_ui::debugger::file_line(app, v.file, v.line));
    let mut text = String::from("Waiting for a key");
    if let Some(i) = int {
        text.push_str(&format!(" — {i}"));
    }
    if let Some(at) = at {
        text.push_str(&format!(" at {at}"));
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).color(pal::AMBER).strong());
        if !t.keypad.open && ui.add(egui::Button::new("Open keyboard").min_size(vec2(0.0, 36.0))).clicked() {
            t.keypad.open = true;
        }
    });
}

pub(crate) fn call_stack_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    if rect.height() < HEAD_H + 30.0 {
        return;
    }
    let (_, mut b) = card(ui, rect, "CALL STACK");
    egui::ScrollArea::vertical().id_salt("t_calls").auto_shrink([false, false]).show(&mut b, |ui| {
        rustemu_ui::debugger::call_stack_ui(app, ui);
    });
}

pub(crate) fn breakpoints_card(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "BREAKPOINTS");
    if small_icon(&mut head, Icon::Plus, "breakpoint at the selected instruction or the editor line").clicked() {
        match app.selected_addr() {
            Some(a) if !app.session.target.breakpoints().contains(&a) => app.toggle_breakpoint_at_addr(a),
            _ if !app.debugger.bp_lines.contains_key(&(0, app.editor.cursor_line)) => {
                app.toggle_breakpoint_at_line(0, app.editor.cursor_line)
            }
            _ => {}
        }
    }
    table_header(&mut b, &[(0.0, "Addr"), (70.0, "Enabled"), (140.0, "Description")]);
    let mut resync = false;
    let mut remove_line = None;
    let mut remove_addr = None;
    let mut goto = None;
    egui::ScrollArea::vertical().id_salt("t_bps").auto_shrink([false, false]).show(&mut b, |ui| {
        let cs = app.session.x86().map(|m| m.cpu.cs());
        let keys: Vec<(usize, u32)> = app.debugger.bp_lines.keys().copied().collect();
        if keys.is_empty() && app.session.target.breakpoints().is_empty() {
            ui.label(
                RichText::new("Tap the left margin of the editor or the disassembly to add one.").color(pal::WEAK),
            );
        }
        for key in keys {
            let addr = app.debugger.bp_addr.iter().find(|(_, v)| **v == key).map(|(a, _)| *a);
            let (f, l) = key;
            let text = if f == 0 && (app.program.files.is_empty() || app.is_stale()) {
                app.doc.text.lines().nth(l as usize - 1).unwrap_or("").trim().to_string()
            } else {
                rustemu_ui::debugger::source_text(app, f, l)
            };
            let desc = format!("{}  {text}", rustemu_ui::debugger::file_line(app, f, l));
            let bp = app.debugger.bp_lines.get_mut(&key).unwrap();
            ui.horizontal(|ui| {
                ui.set_min_height(ROW_H + 6.0);
                let a = addr.map_or("—".to_string(), |a| offset_text(cs, a));
                ui.add_sized([62.0, 24.0], egui::Label::new(RichText::new(a).monospace().color(pal::TEXT)));
                if toggle_switch(ui, &mut bp.enabled).changed() {
                    resync = true;
                }
                ui.add_space(12.0);
                let w = ui.available_width() - 40.0;
                ui.allocate_ui_with_layout(vec2(w, 28.0), Layout::left_to_right(Align::Center), |ui| {
                    let r =
                        ui.add(egui::Label::new(RichText::new(desc).color(pal::TEXT)).truncate().sense(Sense::click()));
                    if r.on_hover_text("tap: show in the editor").clicked() {
                        goto = Some(key);
                    }
                });
                if small_icon(ui, Icon::Trash, "delete").clicked() {
                    remove_line = Some(key);
                }
            });
        }
        let mut addrs: Vec<u32> = app
            .session
            .target
            .breakpoints()
            .iter()
            .copied()
            .filter(|a| !app.debugger.bp_addr.contains_key(a))
            .collect();
        addrs.sort_unstable();
        for a in addrs {
            let desc = match app.session.view_at(a, true) {
                Some(v) => format!(
                    "{}  {}",
                    rustemu_ui::debugger::file_line(app, v.file, v.line),
                    rustemu_ui::debugger::source_text(app, v.file, v.line)
                ),
                None => format!("address {a:05X}h"),
            };
            ui.horizontal(|ui| {
                ui.set_min_height(ROW_H + 6.0);
                ui.add_sized(
                    [62.0, 24.0],
                    egui::Label::new(RichText::new(offset_text(cs, a)).monospace().color(pal::TEXT)),
                );
                let mut on = true;
                if toggle_switch(ui, &mut on).changed() {
                    remove_addr = Some(a);
                }
                ui.add_space(12.0);
                let w = ui.available_width() - 40.0;
                ui.allocate_ui_with_layout(vec2(w, 28.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(desc).color(pal::TEXT)).truncate());
                });
                if small_icon(ui, Icon::Trash, "delete").clicked() {
                    remove_addr = Some(a);
                }
            });
        }
    });
    if let Some(k) = remove_line {
        app.debugger.bp_lines.remove(&k);
        resync = true;
    }
    if let Some(a) = remove_addr {
        app.session.target.breakpoints_mut().remove(&a);
    }
    if resync {
        app.sync_breakpoints();
    }
    if let Some((f, l)) = goto {
        rustemu_ui::debugger::show_location(app, f, l);
        if f == 0 {
            t.tab = Tab::Editor;
        }
    }
}

/// Offset of linear address `a` in segment `cs`, or the linear address when it is outside.
fn offset_text(cs: Option<u16>, a: u32) -> String {
    // A flat address space (MIPS): the address itself.
    let Some(cs) = cs else { return format!("{a:08x}") };
    match a.checked_sub((cs as u32) << 4) {
        Some(off) if off <= 0xFFFF => format!("{off:04X}"),
        _ => format!("{a:05X}"),
    }
}

pub(crate) fn watch_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "WATCH");
    if small_icon(&mut head, Icon::Plus, "add an expression").clicked() {
        app.debugger.watches.push(rustemu_ui::app::Watch { text: String::new(), compiled: None, before: None });
    }
    let half = b.available_width() * 0.5;
    table_header(&mut b, &[(0.0, "Expression"), (half, "Value")]);
    let loaded = app.loaded() && app.session.program.is_some();
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt("t_watch").auto_shrink([false, false]).show(&mut b, |ui| {
        if app.debugger.watches.is_empty() {
            ui.label(RichText::new("Tap + and type an expression: ax, [si], table[bx], cx * 2").color(pal::WEAK));
        }
        let half = ui.available_width() * 0.5;
        for i in 0..app.debugger.watches.len() {
            ui.horizontal(|ui| {
                ui.set_min_height(ROW_H + 6.0);
                let r = ui.scope(|ui| {
                    field_style(ui, None);
                    ui.add_sized(
                        [half - 12.0, 28.0],
                        egui::TextEdit::singleline(&mut app.debugger.watches[i].text)
                            .hint_text("expression")
                            .font(TextStyle::Monospace),
                    )
                });
                if r.inner.changed() {
                    app.debugger.watches[i].compiled = None;
                    app.debugger.watches[i].before = None;
                }
                let value = if !loaded || app.debugger.watches[i].text.trim().is_empty() {
                    ("—".to_string(), None)
                } else {
                    match app.watch_value(i) {
                        Ok(v) => {
                            let bits = app.debugger.watches[i]
                                .compiled
                                .as_ref()
                                .and_then(|c| c.as_ref().ok())
                                .map_or(16, |e| e.bits());
                            let digits = (bits as usize).div_ceil(4).clamp(2, 8);
                            let changed = app.debugger.watches[i].before.is_some_and(|b| b != v);
                            (
                                format!("{:0digits$X}", v as u64 & ((1u64 << bits.min(63)) - 1)),
                                changed.then_some(pal::AMBER),
                            )
                        }
                        Err(e) => (e.chars().take(24).collect(), Some(pal::RED)),
                    }
                };
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if small_icon(ui, Icon::Trash, "delete").clicked() {
                        remove = Some(i);
                    }
                    value_box(ui, &value.0, value.1, (ui.available_width() - 8.0).clamp(60.0, 140.0));
                });
            });
        }
    });
    if let Some(i) = remove {
        app.debugger.watches.remove(i);
    }
}

pub(crate) fn trace_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "EXECUTION TRACE");
    let Some(m) = app.session.x86() else { return };
    head.label(RichText::new(format!("{} steps back", m.history_len())).color(pal::WEAK));
    let font = FontId::monospace(14.0);
    let cw = b.fonts_mut(|f| f.glyph_width(&font, '0'));
    // Columns: step number, CS:IP, bytes (left out when the card is narrow), instruction.
    let digits = m.cpu.instr_count.max(1).ilog10() as f32 + 1.0;
    let x_cs = cw * (digits.max(2.0) + 2.0);
    let x_bytes = x_cs + cw * 11.0;
    let show_bytes = b.available_width() >= x_bytes + cw * (12.0 + 6.0 + 18.0);
    let x_ins = if show_bytes { x_bytes + cw * 12.0 } else { x_bytes };
    let mut cols = vec![(0.0, "#"), (x_cs, "CS:IP"), (x_ins, "Instruction")];
    if show_bytes {
        cols.push((x_bytes, "Bytes"));
    }
    table_header(&mut b, &cols);
    if !app.loaded() {
        return;
    }
    b.spacing_mut().item_spacing.y = 0.0;
    let rows = ((b.available_height() / ROW_H).floor() as usize).max(1);
    let past: Vec<&rustemu_core::Cpu> = m.history_cpus().rev().take(rows - 1).collect();
    let current = &m.cpu;
    let list: Vec<(&rustemu_core::Cpu, bool)> =
        past.into_iter().rev().map(|c| (c, false)).chain([(current, true)]).collect();
    for (c, now) in list {
        let (r, _) = b.allocate_exact_size(vec2(b.available_width(), ROW_H), Sense::hover());
        let p = b.painter();
        if now {
            p.rect_filled(r, 2.0, pal::AMBER_BG);
        }
        let y = r.center().y;
        let color = if now { pal::AMBER } else { pal::TEXT };
        let bytes = m.sys.mem.slice(c.pc(), 8);
        let ins = disassemble(bytes, c.ip);
        let (mnem, ops) = ins.text.split_once(' ').unwrap_or((&ins.text, ""));
        p.text(pos2(r.left(), y), Align2::LEFT_CENTER, c.instr_count.to_string(), font.clone(), color);
        p.text(
            pos2(r.left() + x_cs, y),
            Align2::LEFT_CENTER,
            format!("{:04X}:{:04X}", c.cs(), c.ip),
            font.clone(),
            color,
        );
        if show_bytes {
            let hex: String = bytes[..ins.len.min(4)].iter().map(|b| format!("{b:02X} ")).collect();
            p.text(pos2(r.left() + x_bytes, y), Align2::LEFT_CENTER, hex.trim_end(), font.clone(), pal::TEXT);
        }
        p.text(pos2(r.left() + x_ins, y), Align2::LEFT_CENTER, mnem, font.clone(), pal::LINK);
        p.text(pos2(r.left() + x_ins + cw * 6.0, y), Align2::LEFT_CENTER, ops, font.clone(), pal::TEXT);
    }
}
