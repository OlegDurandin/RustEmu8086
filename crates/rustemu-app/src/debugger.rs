//! IDE debugger windows: breakpoints (with conditions and "stop on condition"), watches,
//! call stack and the ALU.

use egui::{Color32, FontId, RichText, Ui, pos2};
use rustemu_core::cpu::{AluOp, flags};
use rustemu_session::FrameKind;

use crate::app::{App, Breakpoint, Watch};

pub fn debugger_windows(app: &mut App, ctx: &egui::Context) {
    let w = &app.windows;
    let (mut bps, mut watch, mut calls, mut alu, mut cond) =
        (w.breakpoints, w.watch, w.call_stack, w.alu, w.stop_condition);
    egui::Window::new("breakpoints")
        .open(&mut bps)
        .default_pos(pos2(700.0, 560.0))
        .default_width(560.0)
        .show(ctx, |ui| breakpoints_ui(app, ui));
    egui::Window::new("watch")
        .open(&mut watch)
        .default_pos(pos2(1060.0, 110.0))
        .default_width(460.0)
        .show(ctx, |ui| watch_ui(app, ui));
    egui::Window::new("call stack")
        .open(&mut calls)
        .default_pos(pos2(1060.0, 360.0))
        .default_width(520.0)
        .show(ctx, |ui| call_stack_ui(app, ui));
    egui::Window::new("ALU")
        .open(&mut alu)
        .default_pos(pos2(240.0, 600.0))
        .default_width(520.0)
        .show(ctx, |ui| alu_ui(app, ui));
    egui::Window::new("stop on condition")
        .open(&mut cond)
        .default_pos(pos2(700.0, 420.0))
        .default_width(460.0)
        .show(ctx, |ui| stop_condition_ui(app, ui));
    let w = &mut app.windows;
    (w.breakpoints, w.watch, w.call_stack, w.alu, w.stop_condition) = (bps, watch, calls, alu, cond);
}

/// Colour of a breakpoint marker: red, orange with a condition, grey when disabled.
pub fn marker_color(bp: &Breakpoint) -> Color32 {
    if !bp.enabled {
        Color32::from_rgb(150, 150, 150)
    } else if !bp.condition.trim().is_empty() {
        Color32::from_rgb(235, 140, 20)
    } else {
        Color32::from_rgb(220, 40, 40)
    }
}

/// Shows a source location: editor for the program's own file, source window for others.
pub fn show_location(app: &mut App, file: usize, line: u32) {
    if file == 0 && !app.is_stale() {
        app.goto_line = Some(line);
    }
    if file != 0 || app.windows.source {
        app.windows.source = true;
        app.follow_source = false;
        app.source_file_idx = file;
        app.source_scroll_to = Some(line);
    }
}

fn file_line(app: &App, file: usize, line: u32) -> String {
    let name = app.source_files.get(file).map_or("?", |f| f.0.as_str());
    format!("{name}:{line}")
}

fn source_text(app: &App, file: usize, line: u32) -> String {
    app.source_files
        .get(file)
        .and_then(|f| f.1.get(line as usize - 1))
        .map(|t| t.trim().replace('\t', " "))
        .unwrap_or_default()
}

// ---- breakpoints ----------------------------------------------------------------------------

fn breakpoints_ui(app: &mut App, ui: &mut Ui) {
    if app.bp_lines.is_empty() {
        ui.weak("No breakpoints. Click the margin left of a line number in the editor (or press ⌘F8).");
    }
    let keys: Vec<(usize, u32)> = app.bp_lines.keys().copied().collect();
    let mut remove = None;
    let mut resync = false;
    let font = FontId::monospace(12.5);
    egui::Grid::new("bp_grid").num_columns(6).spacing([8.0, 4.0]).striped(true).show(ui, |ui| {
        if !keys.is_empty() {
            for h in ["", "line", "source", "condition", "hits", ""] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
        }
        for key in keys {
            let (f, l) = key;
            let loc = if app.source_files.is_empty() && f == 0 {
                format!("{}:{l}", app.file_name())
            } else {
                file_line(app, f, l)
            };
            let text = if f == 0 && (app.source_files.is_empty() || app.is_stale()) {
                app.text.lines().nth(l as usize - 1).unwrap_or("").trim().to_string()
            } else {
                source_text(app, f, l)
            };
            let loaded = app.loaded() && app.session.program.is_some();
            let bp = app.bp_lines.get_mut(&key).unwrap();
            let color = marker_color(bp);
            if ui.checkbox(&mut bp.enabled, RichText::new("●").color(color)).changed() {
                resync = true;
            }
            let goto = ui.link(RichText::new(loc).font(font.clone())).clicked();
            ui.label(RichText::new(text).font(font.clone()).weak());
            let r = ui.add_sized(
                [170.0, 20.0],
                egui::TextEdit::singleline(&mut bp.condition)
                    .hint_text("always (or e.g. cx == 0)")
                    .font(egui::TextStyle::Monospace),
            );
            if r.changed() {
                bp.compiled = None;
            }
            if loaded && bp.compiled.is_none() && !bp.condition.trim().is_empty() {
                bp.compiled = Some(app.session.compile(&bp.condition));
            }
            if let Some(Err(e)) = &bp.compiled {
                r.on_hover_text(e.clone());
            }
            ui.horizontal(|ui| {
                ui.label(bp.hits.to_string());
                if let Some(Err(_)) = &bp.compiled {
                    ui.colored_label(crate::colors::error(ui), "⚠").on_hover_text("the condition has an error");
                }
            });
            if ui.small_button("✖").on_hover_text("delete").clicked() {
                remove = Some(key);
            }
            ui.end_row();
            if goto {
                show_location(app, f, l);
            }
        }
    });
    if let Some(k) = remove {
        app.bp_lines.remove(&k);
        resync = true;
    }
    // Address breakpoints set in the disassembly (macro code).
    let mut addrs: Vec<u32> =
        app.session.machine.breakpoints.iter().copied().filter(|a| !app.bp_addr.contains_key(a)).collect();
    addrs.sort_unstable();
    if !addrs.is_empty() {
        ui.separator();
        ui.label(RichText::new("address breakpoints (set in the disassembly)").weak());
        for a in addrs {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{a:05X}h")).font(font.clone()));
                if let Some(v) = app.session.view_at(a, true) {
                    ui.weak(format!("{}  {}", file_line(app, v.file, v.line), source_text(app, v.file, v.line)));
                }
                if ui.small_button("✖").clicked() {
                    app.session.machine.breakpoints.remove(&a);
                }
            });
        }
    }
    ui.horizontal(|ui| {
        if ui.button("enable all").clicked() {
            app.bp_lines.values_mut().for_each(|b| b.enabled = true);
            resync = true;
        }
        if ui.button("disable all").clicked() {
            app.bp_lines.values_mut().for_each(|b| b.enabled = false);
            resync = true;
        }
        if ui.button("reset hits").clicked() {
            app.bp_lines.values_mut().for_each(|b| b.hits = 0);
        }
        if ui.button("delete all").clicked() {
            app.bp_lines.clear();
            app.session.machine.breakpoints.clear();
            app.bp_addr.clear();
        }
    });
    if resync {
        app.sync_breakpoints();
    }
    if ui.link("stop on condition…").clicked() {
        app.windows.stop_condition = true;
    }
}

fn stop_condition_ui(app: &mut App, ui: &mut Ui) {
    let loaded = app.loaded() && app.session.program.is_some();
    let sc = &mut app.stop_condition;
    ui.horizontal(|ui| {
        ui.checkbox(&mut sc.enabled, "");
        let r = ui.add(
            egui::TextEdit::singleline(&mut sc.text)
                .hint_text("expression, e.g.  ax == 5,  [bx] > 10,  value")
                .font(egui::TextStyle::Monospace)
                .desired_width(260.0),
        );
        if r.changed() {
            sc.compiled = None;
            sc.last = None;
            sc.enabled = !sc.text.trim().is_empty();
        }
        ui.radio_value(&mut sc.on_change, false, "becomes true");
        ui.radio_value(&mut sc.on_change, true, "changes");
    });
    if loaded && sc.compiled.is_none() && !sc.text.trim().is_empty() {
        sc.compiled = Some(app.session.compile(&sc.text));
    }
    match &sc.compiled {
        Some(Err(e)) => {
            ui.colored_label(crate::colors::error(ui), e);
        }
        Some(Ok(e)) => {
            let v = e.eval(&app.session.machine);
            ui.weak(format!("now: {v} ({})", hex(v, e.bits())));
        }
        None => {}
    }
    ui.weak(
        "Checked after every instruction while running (slower than full speed). Registers, flags (ZF, CF...), \
         variables, [mem], es:[di], word [..];  + - * / % << >> & | ^ ~ == != < > && ||",
    );
}

// ---- watch ------------------------------------------------------------------------------------

/// `v` in hexadecimal with the digits of a `bits`-wide value.
fn hex(v: i64, bits: u32) -> String {
    let bits = if bits == 16 && !(-0x8000..=0xFFFF).contains(&v) { 32 } else { bits };
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    format!("{:0w$X}h", v as u64 & mask, w = (bits / 4) as usize)
}

fn signed(v: i64, bits: u32) -> i64 {
    match bits {
        8 => v as i8 as i64,
        16 => v as i16 as i64,
        32 => v as i32 as i64,
        _ => v,
    }
}

fn watch_ui(app: &mut App, ui: &mut Ui) {
    let loaded = app.loaded() && app.session.program.is_some();
    let font = FontId::monospace(12.5);
    let changed_color = Color32::from_rgb(0xE0, 0x40, 0x40);
    let mut remove = None;
    egui::Grid::new("watch_grid").num_columns(5).spacing([10.0, 3.0]).striped(true).show(ui, |ui| {
        for h in ["expression", "hex", "decimal", "char", ""] {
            ui.label(RichText::new(h).weak());
        }
        ui.end_row();
        for i in 0..app.watches.len() {
            let r = ui.add_sized(
                [170.0, 20.0],
                egui::TextEdit::singleline(&mut app.watches[i].text).font(egui::TextStyle::Monospace),
            );
            if r.changed() {
                app.watches[i].compiled = None;
                app.watches[i].before = None;
            }
            if !loaded {
                ui.weak("-");
                ui.label("");
                ui.label("");
            } else {
                match app.watch_value(i) {
                    Ok(v) => {
                        let bits =
                            app.watches[i].compiled.as_ref().and_then(|c| c.as_ref().ok()).map_or(16, |e| e.bits());
                        let changed = app.watches[i].before.is_some_and(|b| b != v);
                        let style = |t: String| {
                            let t = RichText::new(t).font(font.clone());
                            if changed { t.color(changed_color) } else { t }
                        };
                        ui.label(style(hex(v, bits)));
                        let s = signed(v, bits);
                        let dec = if s < 0 && bits <= 32 { format!("{v}  ({s})") } else { v.to_string() };
                        ui.label(style(dec));
                        let ch = (32..127).contains(&(v & 0xFF)).then(|| format!("'{}'", (v & 0xFF) as u8 as char));
                        ui.label(style(ch.unwrap_or_default()));
                    }
                    Err(e) => {
                        ui.colored_label(crate::colors::error(ui), "error").on_hover_text(e);
                        ui.label("");
                        ui.label("");
                    }
                }
            }
            if ui.small_button("✖").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        app.watches.remove(i);
    }
    let id = ui.make_persistent_id("new_watch");
    let mut text = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
    let r = ui.add(
        egui::TextEdit::singleline(&mut text)
            .hint_text("add: ax, [si], value, table[bx], word es:[di], cx * 2")
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    );
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
        add_watch(app, text.trim());
        text.clear();
        r.request_focus();
    }
    ui.data_mut(|d| d.insert_temp(id, text));
    ui.weak("Values that changed during the last step / run are red.");
}

pub fn add_watch(app: &mut App, text: &str) {
    app.watches.push(Watch { text: text.to_string(), compiled: None, before: None });
    app.windows.watch = true;
}

// ---- call stack -------------------------------------------------------------------------------

/// Innermost macro/procedure of `entries` that contains `line`.
fn enclosing(entries: &[rustemu_asm::library::Entry], line: u32) -> Option<&rustemu_asm::library::Entry> {
    entries.iter().filter(|e| e.line <= line && line <= e.end_line).min_by_key(|e| e.end_line - e.line)
}

struct Row {
    func: String,
    loc: Option<(usize, u32)>,
    note: String,
}

fn call_stack_rows(app: &App) -> Vec<Row> {
    let s = &app.session;
    let frames = s.call_stack();
    let mut rows = Vec::new();
    let into = app.show_macro_code;
    let pc = s.effective_pc();
    // Macro nesting of the current instruction (innermost first).
    if let (Some(v), Some(info)) = (s.view_at(pc, into), s.line_info_at(pc)) {
        for k in (0..v.level).rev() {
            let (f, l) = info.chain[k];
            let name = app.source_index.get(f).and_then(|idx| enclosing(idx, l)).map(|e| {
                let kind = if e.kind == rustemu_asm::library::EntryKind::Proc { "proc" } else { "macro" };
                format!("{kind} {}", e.name)
            });
            rows.push(Row { func: name.unwrap_or_else(|| "macro".into()), loc: Some((f, l)), note: String::new() });
        }
    }
    let func_of = |k: usize| -> String {
        match frames.get(k) {
            Some(f) => match f.kind {
                FrameKind::Interrupt(n) => format!("INT {n:02X}h handler"),
                FrameKind::Call => {
                    f.target.and_then(|t| s.symbol_at(t)).map(str::to_string).unwrap_or_else(|| "?".into())
                }
            },
            None => "(program)".into(),
        }
    };
    let positions = std::iter::once(pc).chain(frames.iter().map(|f| f.call_addr));
    for (k, p) in positions.enumerate() {
        let view = s.view_at(p, into);
        // The innermost position was already listed with its macros.
        let loc = if k == 0 && view.as_ref().is_some_and(|v| v.level > 0) {
            view.map(|v| v.statement)
        } else {
            view.map(|v| (v.file, v.line))
        };
        let note = if k == 0 { "current".to_string() } else { format!("{:05X}h", p) };
        rows.push(Row { func: func_of(k), loc, note });
    }
    rows
}

fn call_stack_ui(app: &mut App, ui: &mut Ui) {
    if !app.loaded() {
        ui.weak("No program loaded.");
        return;
    }
    let rows = call_stack_rows(app);
    let font = FontId::monospace(12.5);
    let mut goto = None;
    egui::Grid::new("calls").num_columns(4).spacing([10.0, 3.0]).striped(true).show(ui, |ui| {
        for (i, r) in rows.iter().enumerate() {
            let t = RichText::new(format!("{i}")).font(font.clone()).weak();
            ui.label(t);
            let name = RichText::new(&r.func).font(font.clone());
            ui.label(if i == 0 { name.strong().color(crate::colors::info(ui)) } else { name });
            match r.loc {
                Some((f, l)) => {
                    if ui.link(RichText::new(file_line(app, f, l)).font(font.clone())).clicked() {
                        goto = Some((f, l));
                    }
                    ui.label(RichText::new(source_text(app, f, l)).font(font.clone()).weak());
                }
                None => {
                    ui.weak(&r.note);
                    ui.label("");
                }
            }
            ui.end_row();
        }
    });
    if let Some((f, l)) = goto {
        show_location(app, f, l);
    }
    ui.weak("Return addresses found on the stack (CALL / INT) plus the macros of the current line.");
}

// ---- ALU --------------------------------------------------------------------------------------

fn bin(v: u32, bits: u32) -> String {
    let s = format!("{v:0w$b}", w = bits as usize);
    s.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(" ")
}

fn alu_ui(app: &mut App, ui: &mut Ui) {
    let t = app.session.machine.cpu.alu_trace;
    if t.op == AluOp::None {
        ui.weak("No arithmetic or logic instruction executed yet.");
        return;
    }
    let addr = rustemu_core::linear(t.cs, t.ip);
    let ago = app.session.machine.cpu.instr_count.saturating_sub(t.instr + 1);
    ui.horizontal_wrapped(|ui| {
        let w = if t.wide { "word" } else { "byte" };
        ui.label(RichText::new(format!("{} ({w})", t.op.name())).strong().monospace().size(15.0));
        ui.weak(format!("at {:04X}:{:04X}", t.cs, t.ip));
        if let Some(v) = app.session.view_at(addr, app.show_macro_code) {
            let txt = format!("{}  {}", file_line(app, v.file, v.line), source_text(app, v.file, v.line));
            if ui.link(RichText::new(txt).monospace()).clicked() {
                show_location(app, v.file, v.line);
            }
        }
        if ago > 0 {
            ui.weak(format!("({ago} instructions ago)"));
        }
    });
    let bits = if t.wide { 16 } else { 8 };
    let shift = matches!(
        t.op,
        AluOp::Rol | AluOp::Ror | AluOp::Rcl | AluOp::Rcr | AluOp::Shl | AluOp::Shr | AluOp::Sar | AluOp::Setmo
    );
    let (mul, div) = (matches!(t.op, AluOp::Mul | AluOp::Imul), matches!(t.op, AluOp::Div | AluOp::Idiv));
    let signed_op = matches!(t.op, AluOp::Imul | AluOp::Idiv);
    // (label, value, width in bits)
    let mut rows: Vec<(&str, u32, u32)> = Vec::new();
    let (a_name, b_name) = match t.op {
        _ if mul => (if t.wide { "AX" } else { "AL" }, "operand"),
        _ if div => (if t.wide { "DX:AX" } else { "AX" }, "divisor"),
        _ if shift => ("value", "count"),
        AluOp::Inc | AluOp::Dec | AluOp::Neg | AluOp::Not => ("operand", ""),
        _ => ("operand 1", "operand 2"),
    };
    rows.push((a_name, t.a, if div { bits * 2 } else { bits }));
    if !b_name.is_empty() {
        rows.push((b_name, t.b as u32, if shift { 8 } else { bits }));
    }
    if div {
        rows.push(("quotient", t.result & 0xFFFF, bits));
        rows.push(("remainder", t.result >> 16, bits));
    } else {
        let label = if matches!(t.op, AluOp::Cmp) {
            "difference"
        } else if t.op == AluOp::Test {
            "AND result"
        } else {
            "result"
        };
        rows.push((label, t.result, if mul { bits * 2 } else { bits }));
    }
    let font = FontId::monospace(13.0);
    egui::Grid::new("alu").num_columns(5).spacing([14.0, 3.0]).striped(true).show(ui, |ui| {
        for h in ["", "hex", "binary", "unsigned", "signed"] {
            ui.label(RichText::new(h).weak());
        }
        ui.end_row();
        let n = rows.len();
        for (i, (name, v, w)) in rows.into_iter().enumerate() {
            let last = i + 1 == n || (div && i + 2 == n);
            let v = if w >= 32 { v } else { v & ((1u32 << w) - 1) };
            let style = |s: String| {
                let t = RichText::new(s).font(font.clone());
                if last { t.strong() } else { t }
            };
            ui.label(RichText::new(name).weak());
            ui.label(style(format!("{:0w$X}h", v, w = (w / 4) as usize)));
            ui.label(style(bin(v, w)));
            ui.label(style(v.to_string()));
            let s = match w {
                8 => v as u8 as i8 as i64,
                16 => v as u16 as i16 as i64,
                _ => v as i32 as i64,
            };
            ui.label(if signed_op || s < 0 { style(s.to_string()) } else { RichText::new("") });
            ui.end_row();
        }
    });
    if t.op == AluOp::Cmp {
        ui.weak("CMP does not store the difference; only the flags are kept.");
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("flags").weak());
        for (n, f) in [
            ("CF", flags::CF),
            ("ZF", flags::ZF),
            ("SF", flags::SF),
            ("OF", flags::OF),
            ("PF", flags::PF),
            ("AF", flags::AF),
        ] {
            let (before, after) = (t.flags_in & f != 0, t.flags_out & f != 0);
            let txt = RichText::new(format!("{n} {}→{}", before as u8, after as u8)).monospace();
            ui.label(if before != after { txt.strong().color(crate::colors::info(ui)) } else { txt.weak() });
        }
    });
}
