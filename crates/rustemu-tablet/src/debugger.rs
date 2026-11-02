//! The Debugger tab's own cards: the visual step (what the last instruction did and why), the code
//! (source, source mixed with the instructions, or the instructions alone) and the tools that open
//! one at a time (watch, breakpoints, call stack, trace, ALU).
//!
//! Two lines are told apart everywhere: the last executed instruction (a calm blue bar) and the
//! next one at CS:IP (an amber arrow). While the BIOS serves an `INT`, the code shows the `INT` of
//! the program that called it; "BIOS internals" shows the ROM code itself.

use egui::{Align2, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use rustemu_core::linear;
use rustemu_session::step::StepView;
use rustemu_ui::app::App;

use crate::TabletState;
use crate::cards::{breakpoints_card, call_stack_card, trace_card, watch_card};
use crate::theme::pal;
use crate::widgets::{HEAD_H, ROW_H, card, split_y};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum CodeMode {
    /// The program's source (the default).
    #[default]
    Source,
    /// The instructions, each source line above its own.
    Mixed,
    Assembly,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DebugTool {
    /// The program's output, when the CPU leaves it no room beside the code.
    Output,
    Watch,
    Breakpoints,
    CallStack,
    Trace,
    Alu,
    /// The DEBUG.EXE-style console with the log of every stop.
    Console,
}

impl DebugTool {
    const ALL: [(DebugTool, &'static str); 6] = [
        (DebugTool::Watch, "Watch"),
        (DebugTool::Breakpoints, "Breakpoints"),
        (DebugTool::CallStack, "Call stack"),
        (DebugTool::Trace, "Trace"),
        (DebugTool::Alu, "ALU"),
        (DebugTool::Console, "Debug log"),
    ];
}

/// State of the Debugger tab.
#[derive(Default)]
pub(crate) struct DebugView {
    pub(crate) mode: CodeMode,
    /// Show the ROM code of the BIOS while it serves an `INT` (otherwise: the calling `INT`).
    pub(crate) bios: bool,
    /// The explanation of the visual step is open.
    pub(crate) why: bool,
    /// The open tool (one at a time; kept while switching tabs).
    pub(crate) tool: Option<DebugTool>,
    /// The output is one of the tools (set by the layout when it has no room of its own).
    pub(crate) output_tool: bool,
    /// The next instruction the source view last followed.
    followed: Option<u32>,
    /// What the source view drew in the last frame: the file, its first line drawn and the rows in
    /// view.
    pub(crate) shown: Option<(usize, u32, f32)>,
}

fn into_macros(app: &App) -> bool {
    app.settings.show_macro_code
}

// ---- visual step -------------------------------------------------------------------------------

/// Height of the visual step card.
pub(crate) fn step_height(t: &TabletState) -> f32 {
    HEAD_H + if t.debug.why { 196.0 } else { 140.0 }
}

/// What the last instruction did: the instruction and its line, the changed registers, flags and
/// memory; "why" explains the flags, "ALU details" opens the ALU tool.
pub(crate) fn step_card(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "LAST STEP");
    if let Some(v) = app.session.current_view(into_macros(app))
        && app.loaded()
    {
        let next = rustemu_ui::debugger::file_line(app, v.file, v.line);
        head.label(RichText::new(format!("next → {next}")).color(pal::AMBER));
    }
    if !app.loaded() {
        b.label(RichText::new("Press Step to assemble the program and run its first instruction.").color(pal::WEAK));
        b.label(RichText::new("What each instruction changes is shown here.").color(pal::WEAK));
        return;
    }
    let waiting = app.session.target.is_waiting_input();
    let Some(step) = app.session.last_step(into_macros(app)) else {
        b.label(RichText::new("Nothing executed yet.").size(17.0).color(pal::TEXT));
        b.label(RichText::new("Step (F8) runs one instruction; what it changed is shown here.").color(pal::WEAK));
        return;
    };
    b.horizontal(|ui| {
        ui.label(RichText::new(&step.text).monospace().size(19.0).strong().color(pal::TEXT));
        ui.label(RichText::new(format!("{:04X}:{:04X}", step.cs, step.ip)).monospace().color(pal::WEAK));
        if let Some(src) = &step.source {
            let text = rustemu_ui::debugger::file_line(app, src.file, src.line);
            if ui.link(RichText::new(text).monospace()).on_hover_text("show the line in the source").clicked() {
                t.debug.mode = CodeMode::Source;
            }
        }
    });
    b.add_space(4.0);
    changes(&mut b, &step);
    b.add_space(6.0);
    b.horizontal(|ui| {
        if waiting {
            ui.label(RichText::new("Waiting for a key").color(pal::AMBER).strong());
            if !t.keypad.open && ui.button("Open keyboard").clicked() {
                t.keypad.open = true;
            }
        }
        let why = if t.debug.why { "Hide the explanation" } else { "Why did this happen?" };
        if ui.button(why).clicked() {
            t.debug.why = !t.debug.why;
        }
        if step.alu.is_some() && ui.button("Open ALU details").clicked() {
            t.debug.tool = Some(DebugTool::Alu);
        }
    });
    if t.debug.why {
        b.add_space(4.0);
        for line in step.why() {
            b.label(RichText::new(line).monospace().color(pal::TEXT));
        }
    }
}

/// The changes of a step as chips: `BL 0Fh → 0Eh`, `CF 0 → 1`, `[07200h] 00 → 34`.
fn changes(ui: &mut Ui, step: &StepView) {
    let mut chips: Vec<(String, bool)> = Vec::new();
    for r in &step.regs {
        match r.byte() {
            Some((name, b, a)) => {
                chips.push((format!("{name} {b:02X}h → {a:02X}h"), true));
                chips.push((format!("{} {:04X}h → {:04X}h", r.name, r.before, r.after), false));
            }
            None => chips.push((format!("{} {:04X}h → {:04X}h", r.name, r.before, r.after), true)),
        }
    }
    for f in &step.flags {
        chips.push((format!("{} {} → {}", f.name, f.before as u8, f.after as u8), true));
    }
    const MAX_WRITES: usize = 4;
    for &(a, b, n) in step.writes.iter().take(MAX_WRITES) {
        chips.push((format!("[{a:05X}h] {b:02X} → {n:02X}"), true));
    }
    if step.writes.len() > MAX_WRITES {
        chips.push((format!("+{} bytes", step.writes.len() - MAX_WRITES), false));
    }
    if chips.is_empty() {
        ui.label(RichText::new("No register, flag or memory changed (only IP moved on).").color(pal::WEAK));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
        for (text, main) in chips {
            let color = if main { pal::GREEN } else { pal::WEAK };
            egui::Frame::new()
                .fill(if main { pal::GREEN.gamma_multiply(0.12) } else { pal::FIELD })
                .stroke(Stroke::new(1.0, if main { pal::GREEN } else { pal::BORDER }))
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| ui.label(RichText::new(text).monospace().color(color)));
        }
    });
}

// ---- code --------------------------------------------------------------------------------------

/// Where the code view starts and what it marks.
struct Marks {
    /// The next instruction (CS:IP, or the calling `INT` while the BIOS is hidden).
    next: Option<u32>,
    /// The last executed instruction.
    last: Option<u32>,
}

fn marks(app: &App, t: &TabletState) -> Marks {
    let next = if !t.debug.bios { app.session.effective_pc() } else { app.session.target.pc() };
    // As the last step card shows it: a step that began in the BIOS is the INT that called it.
    let last = app.session.last_step(into_macros(app)).map(|v| linear(v.cs, v.ip)).filter(|&a| a != next);
    Marks { next: Some(next), last }
}

pub(crate) fn code_card(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (mut head, mut b) = card(ui, rect, "CODE");
    let in_bios = app.session.bios_caller().is_some();
    if in_bios && app.loaded() {
        head.checkbox(&mut t.debug.bios, "BIOS internals")
            .on_hover_text("The BIOS serves an INT of the program: show its ROM code instead of the calling INT");
    }
    for (mode, name) in [(CodeMode::Assembly, "Assembly"), (CodeMode::Mixed, "Mixed"), (CodeMode::Source, "Source")] {
        if head.selectable_label(t.debug.mode == mode, name).clicked() {
            t.debug.mode = mode;
        }
    }
    if !app.loaded() {
        b.label(RichText::new("Press Run or Step to assemble and load the program.").color(pal::WEAK));
        return;
    }
    if let Some(code) = app.session.target.terminated() {
        b.label(RichText::new(format!("The program has terminated (exit code {code}).")).color(pal::TEXT).strong());
        b.label(RichText::new("Step back to see its last instructions, or Reset to run it again.").color(pal::WEAK));
        return;
    }
    if in_bios {
        let int = app.session.bios_caller().map(|(cs, ip)| {
            let a = linear(cs, ip);
            app.session.target.disassemble(a, ip as u32).text
        });
        let text = match (&int, t.debug.bios) {
            (Some(i), false) => format!("The BIOS is serving {i}: the code shows the calling instruction."),
            (Some(i), true) => format!("BIOS code serving {i} (ROM at F000)."),
            _ => String::new(),
        };
        b.label(RichText::new(text).color(pal::AMBER));
    }
    let mk = marks(app, t);
    let mode =
        if t.debug.bios && in_bios && t.debug.mode == CodeMode::Source { CodeMode::Assembly } else { t.debug.mode };
    match mode {
        CodeMode::Source if app.program.files.is_empty() => {
            b.label(RichText::new("No source (a binary was loaded): see Assembly.").color(pal::WEAK));
        }
        CodeMode::Source => source_rows(app, t, &mut b, &mk),
        CodeMode::Mixed | CodeMode::Assembly => asm_rows(app, &mut b, &mk, mode == CodeMode::Mixed, t.debug.bios),
    }
}

/// The source file of the next instruction, its line marked; the last executed line too.
fn source_rows(app: &mut App, t: &mut TabletState, b: &mut Ui, mk: &Marks) {
    let into = into_macros(app);
    let next = mk.next.and_then(|a| app.session.view_at(a, into));
    let last = mk.last.and_then(|a| app.session.view_at(a, into));
    let file = next.as_ref().or(last.as_ref()).map_or(0, |v| v.file);
    let Some((name, lines)) = app.program.files.get(file).cloned() else { return };
    if file != 0 {
        b.label(RichText::new(format!("in {name}")).color(pal::WEAK));
    }
    let next_line = next.as_ref().filter(|v| v.file == file).map(|v| v.line);
    let last_line = last.as_ref().filter(|v| v.file == file).map(|v| v.line).filter(|l| Some(*l) != next_line);
    let font = FontId::monospace(14.5);
    let digits = lines.len().max(1).ilog10() as usize + 1;
    let cw = b.fonts_mut(|f| f.glyph_width(&font, '0'));
    let x_text = 30.0 + cw * (digits as f32 + 1.0);
    // A scroll area per file: a file's scroll offset is clamped by its own length (one area for all
    // files kept the program scrolled no further than the library allowed).
    let mut area = egui::ScrollArea::vertical().id_salt(("t_source", file)).auto_shrink([false, false]);
    // Follow the next line like an editor: when the file changes, or when the line is out of view
    // after execution moved or the view got smaller (the PC keyboard opened), it goes a third of
    // the way down. Scrolling by hand is left alone while nothing changes.
    let visible = (b.available_height() / ROW_H).floor().max(1.0);
    if let Some(l) = next_line {
        let moved = t.debug.followed != mk.next;
        let shown_file = t.debug.shown.map(|s| s.0);
        let resized = t.debug.shown.is_some_and(|s| s.2 != visible);
        let first = t.debug.shown.map_or(1.0, |s| s.1 as f32);
        let out_of_view = (l as f32) < first || (l as f32) > first + visible - 2.0;
        if shown_file != Some(file) || ((moved || resized) && out_of_view) {
            area = area.vertical_scroll_offset((l as f32 - 1.0 - (visible / 3.0).floor()).max(0.0) * ROW_H);
        }
        t.debug.followed = mk.next;
    }
    let waiting = app.session.target.is_waiting_input();
    let mut toggle = None;
    // Rows touch each other: show_rows places them by the spacing of `b`, so it must be zero there
    // too (with the theme's 6 points it counted 32-point rows and showed lines far above).
    b.spacing_mut().item_spacing.y = 0.0;
    let mut first_drawn = 1;
    area.show_rows(b, ROW_H, lines.len(), |ui, range| {
        first_drawn = range.start as u32 + 1;
        for i in range {
            let line = i as u32 + 1;
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
            let p = ui.painter();
            let y = r.center().y;
            if Some(line) == next_line {
                p.rect_filled(r, 2.0, pal::AMBER_BG);
                paint_arrow(p, pos2(r.left() + 12.0, y), pal::AMBER);
                if waiting {
                    p.text(
                        r.right_center() - vec2(8.0, 0.0),
                        Align2::RIGHT_CENTER,
                        "waiting for a key",
                        FontId::proportional(13.0),
                        pal::AMBER,
                    );
                }
            } else if Some(line) == last_line {
                p.rect_filled(r, 2.0, pal::LINK.gamma_multiply(0.12));
                p.rect_filled(Rect::from_min_size(r.min, vec2(4.0, r.height())), 1.0, pal::LINK);
            }
            if app.debugger.bp_lines.contains_key(&(file, line)) {
                p.circle_filled(pos2(r.left() + 12.0, y), 6.0, pal::RED);
                if Some(line) == next_line {
                    paint_arrow(p, pos2(r.left() + 12.0, y), pal::AMBER);
                }
            }
            p.text(pos2(r.left() + 26.0, y), Align2::LEFT_CENTER, format!("{line:>digits$}"), font.clone(), pal::WEAK);
            p.text(pos2(r.left() + x_text, y), Align2::LEFT_CENTER, &lines[i], font.clone(), pal::TEXT);
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x < r.left() + 24.0) {
                toggle = Some(line);
            }
        }
    });
    t.debug.shown = Some((file, first_drawn, visible));
    if let Some(l) = toggle {
        app.toggle_breakpoint_at_line(file, l);
    }
}

/// The instructions from the one before the next; `mixed` puts each source line above its first
/// instruction.
fn asm_rows(app: &mut App, b: &mut Ui, mk: &Marks, mixed: bool, bios: bool) {
    let font = FontId::monospace(14.5);
    let char_w = b.fonts_mut(|f| f.glyph_width(&font, '0'));
    let width = b.available_width();
    let x_addr = 28.0;
    let x_bytes = x_addr + char_w * 6.0 + 14.0;
    // The bytes of an instruction: up to 6 (all of most), or 4 and an ellipsis when narrow, or none.
    let fits = |n: f32| width >= x_bytes + char_w * (n * 3.0 + 6.0 + 22.0) + 20.0;
    let max_bytes: usize = if fits(6.0) {
        6
    } else if fits(4.0) {
        4
    } else {
        0
    };
    let show_bytes = max_bytes > 0;
    // Each byte takes 3 characters ("2E "), the ellipsis of a longer one the last space.
    let x_mnem = if show_bytes { x_bytes + char_w * (max_bytes as f32 * 3.0 + 1.0) + 10.0 } else { x_bytes };
    let x_ops = x_mnem + char_w * 6.0 + 6.0;
    b.spacing_mut().item_spacing.y = 0.0;
    let rows = ((b.available_height() / ROW_H).floor() as usize).max(1);
    let next = mk.next.unwrap_or(0);
    // Where the list starts: the disassembly follows CS:IP; with the BIOS hidden it starts at the
    // calling INT.
    let (mut a, mut ip, cs) = match (app.session.bios_caller(), bios) {
        (Some((cs, ip)), false) => (linear(cs, ip), ip, cs),
        _ => {
            let start = rustemu_ui::inspect::disasm_start(app, if mixed { rows / 2 } else { rows });
            let Some(c) = app.session.x86().map(|m| &m.cpu) else { return };
            (start, c.ip.wrapping_sub(c.pc().wrapping_sub(start) as u16), c.cs())
        }
    };
    let mut shown = 0;
    let mut last_line = None;
    let mut toggle = None;
    let mut select = None;
    while shown < rows {
        let text_row = |b: &mut Ui, text: String, color| {
            let (r, _) = b.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
            b.painter().text(pos2(r.left() + x_addr, r.center().y), Align2::LEFT_CENTER, text, font.clone(), color);
        };
        if mixed && let Some((f, l)) = app.session.line_at_addr(a) {
            if last_line != Some((f, l)) {
                last_line = Some((f, l));
                let src = rustemu_ui::debugger::source_text(app, f, l);
                text_row(b, format!("{l:>4}│ {}", src.trim()), pal::LABEL);
                shown += 1;
                if shown >= rows {
                    break;
                }
            }
        } else if let Some(name) = app.session.symbol_at(a) {
            text_row(b, format!("{name}:"), pal::LABEL);
            shown += 1;
            if shown >= rows {
                break;
            }
        }
        let m = &*app.session.target;
        let bytes = m.read_bytes(a, 8);
        let ins = app.session.code_row(a, ip as u32, app.settings.disasm_data);
        let (r, resp) = b.allocate_exact_size(vec2(width, ROW_H), Sense::click());
        let p = b.painter();
        let y = r.center().y;
        if a == next {
            p.rect_filled(r, 2.0, pal::AMBER_BG);
        } else if Some(a) == mk.last {
            p.rect_filled(r, 2.0, pal::LINK.gamma_multiply(0.12));
            p.rect_filled(Rect::from_min_size(r.min, vec2(4.0, r.height())), 1.0, pal::LINK);
        }
        if m.breakpoints().contains(&a) {
            p.circle_filled(pos2(r.left() + 12.0, y), 6.0, pal::RED);
        }
        if a == next {
            paint_arrow(p, pos2(r.left() + 12.0, y), pal::AMBER);
        }
        let addr_color = if a == next { pal::AMBER } else { pal::WEAK };
        p.text(pos2(r.left() + x_addr, y), Align2::LEFT_CENTER, format!("{ip:04X}:"), font.clone(), addr_color);
        if show_bytes {
            let n = if ins.data { max_bytes.min(3) } else { max_bytes };
            let mut hex: String = bytes[..(ins.len as usize).min(n)].iter().map(|b| format!("{b:02X} ")).collect();
            if ins.len as usize > n {
                hex.push('…');
            }
            p.text(pos2(r.left() + x_bytes, y), Align2::LEFT_CENTER, hex.trim_end(), font.clone(), pal::TEXT);
        }
        let (mnem, ops) = ins.text.split_once(' ').unwrap_or((&ins.text, ""));
        let (mnem_color, ops_color) = if ins.data { (pal::WEAK, pal::TEAL) } else { (pal::LINK, pal::TEXT) };
        p.text(pos2(r.left() + x_mnem, y), Align2::LEFT_CENTER, mnem, font.clone(), mnem_color);
        let ops =
            match rustemu_ui::inspect::jump_target(&ins.text).and_then(|off| app.session.symbol_at(linear(cs, off))) {
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
        shown += 1;
        if ip as u32 + ins.len > 0xFFFF {
            break;
        }
        a += ins.len;
        ip = ip.wrapping_add(ins.len as u16);
    }
    if let Some(s) = select {
        app.view.code_selected = Some(s);
    }
    if let Some(a) = toggle {
        app.toggle_breakpoint_at_addr(a);
    }
}

pub(crate) fn paint_arrow(p: &egui::Painter, c: egui::Pos2, color: egui::Color32) {
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

// ---- tools -------------------------------------------------------------------------------------

/// Height of the tool buttons row.
pub(crate) const TOOLS_ROW_H: f32 = 44.0;

/// The tool buttons and, below them, the open tool in the rest of `rect`.
pub(crate) fn tools_area(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (row, rest) = split_y(rect, TOOLS_ROW_H);
    let mut r = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("t_tools")
            .max_rect(row)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    r.spacing_mut().item_spacing.x = 8.0;
    if !t.debug.output_tool && t.debug.tool == Some(DebugTool::Output) {
        t.debug.tool = None;
    }
    let output = t.debug.output_tool.then_some((DebugTool::Output, "Output"));
    for (tool, name) in output.into_iter().chain(DebugTool::ALL) {
        let on = t.debug.tool == Some(tool);
        let resp = r.add(
            egui::Button::new(RichText::new(name).size(15.0).color(if on { egui::Color32::WHITE } else { pal::TEXT }))
                .fill(if on { pal::BLUE } else { pal::BUTTON })
                .min_size(vec2(64.0, TOOLS_ROW_H)),
        );
        if resp.clicked() {
            t.debug.tool = if on { None } else { Some(tool) };
        }
    }
    let Some(tool) = t.debug.tool else { return };
    if rest.height() < HEAD_H + 40.0 {
        return;
    }
    match tool {
        DebugTool::Output => crate::cards::output_card(app, t, ui, rest),
        DebugTool::Watch => watch_card(app, ui, rest),
        DebugTool::Breakpoints => breakpoints_card(app, t, ui, rest),
        DebugTool::CallStack => call_stack_card(app, ui, rest),
        DebugTool::Trace => trace_card(app, ui, rest),
        DebugTool::Console => {
            let (_, mut b) = card(ui, rest, "DEBUG LOG");
            rustemu_ui::console::console_ui(app, &mut b);
        }
        DebugTool::Alu => {
            let (_, mut b) = card(ui, rest, "ALU");
            egui::ScrollArea::vertical().id_salt("t_alu").auto_shrink([false, false]).show(&mut b, |ui| {
                rustemu_ui::units::alu_ui(app, ui);
            });
        }
    }
}
