//! Cards of the Editor and Debugger tabs: editor, registers, flags, disassembly, output, call
//! stack, breakpoints, watch, execution trace.

use egui::{
    Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, TextStyle, Ui, pos2, vec2,
};
use rustemu_core::cpu::{self, flags};
use rustemu_core::disasm::disassemble;
use rustemu_core::linear;
use rustemu_ui::app::App;

use crate::TabletState;
use crate::theme::pal;
use crate::widgets::{
    HEAD_H, Icon, ROW_H, card, field_style, hex_box, plain_card, small_icon, table_header, toggle_switch, value_box,
};
use crate::{Output, Tab};

// ---- cards -------------------------------------------------------------------------------------

pub(crate) fn editor_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let mut b = plain_card(ui, rect, "editor");
    if app.find.open {
        b.horizontal(|ui| rustemu_ui::find::find_bar(app, ui));
    }
    rustemu_ui::editor::editor(app, &mut b);
}

pub(crate) fn registers_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let (_, mut b) = card(ui, rect, "REGISTERS");
    let prev = app.prev_cpu.clone();
    let c = &mut app.session.machine.cpu;
    let left = [("AX", 0), ("BX", 1), ("CX", 2), ("DX", 3), ("SP", 4), ("BP", 5)];
    let right = [("SI", 6), ("DI", 7), ("CS", 8), ("DS", 9), ("ES", 10), ("SS", 11), ("IP", 12)];
    let reg = reg_mut;
    let mut prev_c = prev;
    let box_w = ((b.available_width() - 2.0 * 34.0 - 40.0) / 2.0).clamp(56.0, 90.0);
    egui::Grid::new("t_regs").num_columns(4).spacing([10.0, 6.0]).show(&mut b, |ui| {
        for row in 0..7 {
            for (names, col) in [(&left[..], 0), (&right[..], 1)] {
                match names.get(row) {
                    Some(&(name, i)) => {
                        let label_text = if col == 1 { format!("  {name}") } else { name.to_string() };
                        let label = ui.label(RichText::new(label_text).monospace().size(15.0).color(pal::TEXT)).id;
                        let old = *reg(&mut prev_c, i);
                        let v = reg(c, i);
                        if i < 4 {
                            // AX..DX as their high and low bytes (AH | AL), like the desktop emulator.
                            byte_pair(ui, v, old, name, box_w, label);
                        } else {
                            let accent = (*v != old).then_some(pal::AMBER);
                            hex_box(ui, v, 4, accent, box_w, label);
                        }
                    }
                    None => {
                        ui.label("");
                        ui.label("");
                    }
                }
            }
            ui.end_row();
        }
    });
}

/// A general register as two byte boxes (`AX` -> AH, AL), each marked when it changed.
fn byte_pair(ui: &mut Ui, v: &mut u16, old: u16, name: &str, width: f32, label: egui::Id) {
    let r = &name[..1];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let w = (width - 4.0) / 2.0;
        let (mut hi, mut lo) = (*v >> 8, *v & 0xFF);
        let changed = |mask: u16| ((*v ^ old) & mask != 0).then_some(pal::AMBER);
        hex_box(ui, &mut hi, 2, changed(0xFF00), w, label).on_hover_text(format!("{r}H"));
        hex_box(ui, &mut lo, 2, changed(0x00FF), w, label).on_hover_text(format!("{r}L"));
        *v = hi << 8 | lo;
    });
}

/// Register `i` of the registers card (AX BX CX DX SP BP SI DI CS DS ES SS IP).
fn reg_mut(c: &mut rustemu_core::Cpu, i: usize) -> &mut u16 {
    match i {
        0 => &mut c.regs[cpu::AX],
        1 => &mut c.regs[cpu::BX],
        2 => &mut c.regs[cpu::CX],
        3 => &mut c.regs[cpu::DX],
        4 => &mut c.regs[cpu::SP],
        5 => &mut c.regs[cpu::BP],
        6 => &mut c.regs[cpu::SI],
        7 => &mut c.regs[cpu::DI],
        8 => &mut c.sregs[cpu::CS],
        9 => &mut c.sregs[cpu::DS],
        10 => &mut c.sregs[cpu::ES],
        11 => &mut c.sregs[cpu::SS],
        _ => &mut c.ip,
    }
}

pub(crate) fn flags_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let (_, mut b) = card(ui, rect, "FLAGS");
    let prev = app.prev_cpu.flags;
    let rows = [
        ("CF", flags::CF),
        ("ZF", flags::ZF),
        ("SF", flags::SF),
        ("OF", flags::OF),
        ("PF", flags::PF),
        ("AF", flags::AF),
        ("IF", flags::IF),
        ("DF", flags::DF),
        ("TF", flags::TF),
    ];
    // One column when it fits, otherwise two (the card is wider then).
    let (box_h, gap) = (24.0, 3.0);
    // Grid rows are at least `interact_size.y` high.
    b.spacing_mut().interact_size.y = box_h;
    let fit = ((b.available_height() + gap) / (box_h + gap)).floor().max(1.0) as usize;
    let cols = rows.len().div_ceil(fit).max(1);
    let f = &mut app.session.machine.cpu.flags;
    egui::Grid::new("t_flags").num_columns(cols * 2).spacing([10.0, gap]).show(&mut b, |ui| {
        for r in 0..rows.len().div_ceil(cols) {
            for col in 0..cols {
                let Some(&(name, bit)) = rows.get(col * rows.len().div_ceil(cols) + r) else { continue };
                ui.label(RichText::new(name).monospace().size(15.0).color(pal::TEXT));
                let on = *f & bit != 0;
                let accent = if (*f ^ prev) & bit != 0 {
                    Some(pal::AMBER)
                } else if on {
                    Some(pal::TEAL)
                } else {
                    None
                };
                let (r, resp) = ui.allocate_exact_size(vec2(40.0, box_h), Sense::click());
                let fill = accent.map_or(pal::FIELD, |c| c.gamma_multiply(0.16));
                ui.painter().rect(r, 4.0, fill, Stroke::new(1.0, accent.unwrap_or(pal::BORDER)), StrokeKind::Inside);
                let text = if on { "1" } else { "0" };
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::monospace(14.0),
                    accent.unwrap_or(pal::TEXT),
                );
                let resp = resp.on_hover_text("tap to toggle");
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name} = {text}")));
                if resp.clicked() {
                    *f ^= bit;
                }
            }
            ui.end_row();
        }
    });
}

/// Disassembly from the instruction before CS:IP; `full` adds the column titles.
pub(crate) fn disasm_card(app: &mut App, ui: &mut Ui, rect: Rect, full: bool) {
    let (mut head, mut b) = card(ui, rect, "DISASSEMBLY");
    if app.loaded() {
        let c = &app.session.machine.cpu;
        head.label(RichText::new(format!("CS {:04X}", c.cs())).monospace().color(pal::WEAK));
    }
    if !app.loaded() {
        b.label(RichText::new("Press Run or Step to assemble and load the program.").color(pal::WEAK));
        return;
    }
    if let Some(code) = app.session.machine.terminated
        && !app.code_view_pinned
    {
        // CS:IP is left in the PSP or the DOS stub: nothing there is the program's code.
        b.label(RichText::new(format!("The program has terminated (exit code {code}).")).color(pal::TEXT).strong());
        b.label(RichText::new("Step back to see its last instructions, or Reset to run it again.").color(pal::WEAK));
        return;
    }
    let font = FontId::monospace(14.5);
    let char_w = b.fonts_mut(|f| f.glyph_width(&font, '0'));
    let width = b.available_width();
    let x_addr = 28.0;
    let x_bytes = x_addr + char_w * 6.0 + 14.0;
    // A narrow card (three columns on an 11-inch iPad) leaves the bytes out for the operands.
    let show_bytes = width >= x_bytes + char_w * (12.0 + 6.0 + 20.0) + 20.0;
    let x_mnem = if show_bytes { x_bytes + char_w * 12.0 + 14.0 } else { x_bytes };
    let x_ops = x_mnem + char_w * 6.0 + 6.0;
    if full {
        let (r, _) = b.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
        let p = b.painter();
        let mut titles = vec![(x_addr, "Addr"), (x_mnem, "Instruction")];
        if show_bytes {
            titles.push((x_bytes, "Bytes"));
        }
        for (x, t) in titles {
            p.text(pos2(r.left() + x, r.center().y), Align2::LEFT_CENTER, t, FontId::monospace(14.0), pal::WEAK);
        }
    }
    b.spacing_mut().item_spacing.y = 0.0;
    let rows = ((b.available_height() / ROW_H).floor() as usize).max(1);
    let start = rustemu_ui::debug::disasm_start(app, rows);
    let m = &app.session.machine;
    let pc = m.cpu.pc();
    let (cs, mut ip) = match app.code_selected {
        Some((s, o)) if app.code_view_pinned && linear(s, o) == start => (s, o),
        _ => (m.cpu.cs(), m.cpu.ip.wrapping_sub(pc.wrapping_sub(start) as u16)),
    };
    let mut a = start;
    let selected = app.selected_addr();
    let mut toggle = None;
    let mut select = None;
    let mut shown = 0;
    while shown < rows {
        if let Some(name) = app.session.symbol_at(a) {
            let (r, _) = b.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
            b.painter().text(
                pos2(r.left() + x_addr, r.center().y),
                Align2::LEFT_CENTER,
                format!("{name}:"),
                font.clone(),
                pal::LABEL,
            );
            shown += 1;
            if shown >= rows {
                break;
            }
        }
        let bytes = m.sys.mem.slice(a, 8).to_vec();
        let ins = app.session.code_row(a, ip, app.settings.disasm_data);
        let (r, resp) = b.allocate_exact_size(vec2(width, ROW_H), Sense::click());
        let p = b.painter();
        let current = a == pc;
        if current {
            p.rect_filled(r, 2.0, pal::AMBER_BG);
        } else if selected == Some(a) {
            p.rect_filled(r, 2.0, pal::BLUE.gamma_multiply(0.3));
        }
        let y = r.center().y;
        if m.breakpoints.contains(&a) {
            p.circle_filled(pos2(r.left() + 12.0, y), 6.0, pal::RED);
        }
        if current {
            paint_arrow(p, pos2(r.left() + 12.0, y), pal::AMBER);
        }
        let addr_color = if current { pal::AMBER } else { pal::WEAK };
        p.text(pos2(r.left() + x_addr, y), Align2::LEFT_CENTER, format!("{ip:04X}:"), font.clone(), addr_color);
        if show_bytes {
            // Data rows can be long: their first bytes and an ellipsis fit the column.
            let n = if ins.data { 3 } else { 6 };
            let mut hex: String = bytes[..(ins.len as usize).min(n)].iter().map(|b| format!("{b:02X} ")).collect();
            if ins.len as usize > n {
                hex.push('…');
            }
            p.text(pos2(r.left() + x_bytes, y), Align2::LEFT_CENTER, hex.trim_end(), font.clone(), pal::TEXT);
        }
        let (mnem, ops) = ins.text.split_once(' ').unwrap_or((&ins.text, ""));
        let (mnem_color, ops_color) = if ins.data { (pal::WEAK, pal::TEAL) } else { (pal::LINK, pal::TEXT) };
        p.text(pos2(r.left() + x_mnem, y), Align2::LEFT_CENTER, mnem, font.clone(), mnem_color);
        let ops = match rustemu_ui::debug::jump_target(&ins.text).and_then(|off| app.session.symbol_at(linear(cs, off)))
        {
            Some(name) => format!("{ops}  ; {name}"),
            None => ops.to_string(),
        };
        p.text(pos2(r.left() + x_ops, y), Align2::LEFT_CENTER, ops, font.clone(), ops_color);
        if resp.clicked() {
            match resp.interact_pointer_pos() {
                Some(pos) if pos.x < r.left() + x_addr => toggle = Some(a),
                _ => select = Some((cs, ip)),
            }
        }
        if resp.double_clicked() {
            toggle = Some(a);
        }
        shown += 1;
        a += ins.len;
        ip = ip.wrapping_add(ins.len as u16);
    }
    if let Some(s) = select {
        app.code_selected = Some(s);
    }
    if let Some(a) = toggle {
        app.toggle_breakpoint_at_addr(a);
    }
}

fn paint_arrow(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    p.add(egui::Shape::convex_polygon(
        vec![
            c + vec2(-8.0, -3.0),
            c + vec2(0.0, -3.0),
            c + vec2(0.0, -7.0),
            c + vec2(8.0, 0.0),
            c + vec2(0.0, 7.0),
            c + vec2(0.0, 3.0),
            c + vec2(-8.0, 3.0),
        ],
        color,
        Stroke::NONE,
    ));
}

pub(crate) fn output_card(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "OUTPUT");
    let tab = t.output;
    if small_icon(&mut head, Icon::Trash, if tab == Output::Screen { "clear the screen" } else { "clear the messages" })
        .clicked()
    {
        match tab {
            Output::Screen => {
                let m = &mut app.session.machine.sys.mem;
                let mode = rustemu_core::video::Video::mode(m);
                rustemu_core::video::Video::set_mode(m, mode);
            }
            Output::Messages => app.messages.clear(),
        }
    }
    let errors = app.messages.iter().filter(|m| m.error).count();
    let msg = if errors > 0 { format!("messages ({errors})") } else { "messages".into() };
    for (o, name) in [(Output::Messages, msg.as_str()), (Output::Screen, "screen")] {
        if head.selectable_label(t.output == o, name).clicked() {
            t.output = o;
        }
    }
    match tab {
        Output::Screen => {
            let avail = b.available_size() - vec2(0.0, 26.0);
            let resp = b
                .vertical_centered(|ui| rustemu_ui::screen::screen_view(app, ui, rustemu_ui::screen::Fit::Whole(avail)))
                .inner;
            if resp.clicked() {
                t.keypad.open = true;
            }
            let focused = resp.has_focus();
            let hint = if app.session.machine.is_waiting_input() {
                RichText::new("waiting for input: tap the screen and type").color(pal::AMBER)
            } else if focused {
                RichText::new("keyboard goes to the program").color(pal::WEAK)
            } else {
                RichText::new("tap the screen to type").color(pal::WEAK)
            };
            b.label(hint);
        }
        Output::Messages => {
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(&mut b, |ui| {
                if app.messages.is_empty() {
                    ui.label(
                        RichText::new("No messages. Run assembles the program; errors are listed here.")
                            .color(pal::WEAK),
                    );
                }
                let mut jump = None;
                for (i, m) in app.messages.iter().enumerate() {
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
            Some(a) if !app.session.machine.breakpoints.contains(&a) => app.toggle_breakpoint_at_addr(a),
            _ if !app.bp_lines.contains_key(&(0, app.cursor_line)) => app.toggle_breakpoint_at_line(0, app.cursor_line),
            _ => {}
        }
    }
    table_header(&mut b, &[(0.0, "Addr"), (70.0, "Enabled"), (140.0, "Description")]);
    let mut resync = false;
    let mut remove_line = None;
    let mut remove_addr = None;
    let mut goto = None;
    egui::ScrollArea::vertical().id_salt("t_bps").auto_shrink([false, false]).show(&mut b, |ui| {
        let cs = app.session.machine.cpu.cs();
        let keys: Vec<(usize, u32)> = app.bp_lines.keys().copied().collect();
        if keys.is_empty() && app.session.machine.breakpoints.is_empty() {
            ui.label(
                RichText::new("Tap the left margin of the editor or the disassembly to add one.").color(pal::WEAK),
            );
        }
        for key in keys {
            let addr = app.bp_addr.iter().find(|(_, v)| **v == key).map(|(a, _)| *a);
            let (f, l) = key;
            let text = if f == 0 && (app.source_files.is_empty() || app.is_stale()) {
                app.text.lines().nth(l as usize - 1).unwrap_or("").trim().to_string()
            } else {
                rustemu_ui::debugger::source_text(app, f, l)
            };
            let desc = format!("{}  {text}", rustemu_ui::debugger::file_line(app, f, l));
            let bp = app.bp_lines.get_mut(&key).unwrap();
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
        let mut addrs: Vec<u32> =
            app.session.machine.breakpoints.iter().copied().filter(|a| !app.bp_addr.contains_key(a)).collect();
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
        app.bp_lines.remove(&k);
        resync = true;
    }
    if let Some(a) = remove_addr {
        app.session.machine.breakpoints.remove(&a);
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
fn offset_text(cs: u16, a: u32) -> String {
    match a.checked_sub((cs as u32) << 4) {
        Some(off) if off <= 0xFFFF => format!("{off:04X}"),
        _ => format!("{a:05X}"),
    }
}

pub(crate) fn watch_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "WATCH");
    if small_icon(&mut head, Icon::Plus, "add an expression").clicked() {
        app.watches.push(rustemu_ui::app::Watch { text: String::new(), compiled: None, before: None });
    }
    let half = b.available_width() * 0.5;
    table_header(&mut b, &[(0.0, "Expression"), (half, "Value")]);
    let loaded = app.loaded() && app.session.program.is_some();
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt("t_watch").auto_shrink([false, false]).show(&mut b, |ui| {
        if app.watches.is_empty() {
            ui.label(RichText::new("Tap + and type an expression: ax, [si], table[bx], cx * 2").color(pal::WEAK));
        }
        let half = ui.available_width() * 0.5;
        for i in 0..app.watches.len() {
            ui.horizontal(|ui| {
                ui.set_min_height(ROW_H + 6.0);
                let r = ui.scope(|ui| {
                    field_style(ui, None);
                    ui.add_sized(
                        [half - 12.0, 28.0],
                        egui::TextEdit::singleline(&mut app.watches[i].text)
                            .hint_text("expression")
                            .font(TextStyle::Monospace),
                    )
                });
                if r.inner.changed() {
                    app.watches[i].compiled = None;
                    app.watches[i].before = None;
                }
                let value = if !loaded || app.watches[i].text.trim().is_empty() {
                    ("—".to_string(), None)
                } else {
                    match app.watch_value(i) {
                        Ok(v) => {
                            let bits =
                                app.watches[i].compiled.as_ref().and_then(|c| c.as_ref().ok()).map_or(16, |e| e.bits());
                            let digits = (bits as usize).div_ceil(4).clamp(2, 8);
                            let changed = app.watches[i].before.is_some_and(|b| b != v);
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
        app.watches.remove(i);
    }
}

pub(crate) fn trace_card(app: &mut App, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "EXECUTION TRACE");
    let m = &app.session.machine;
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
