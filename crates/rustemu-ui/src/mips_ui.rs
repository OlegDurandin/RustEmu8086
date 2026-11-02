//! The tools of the MIPS workspace, as MARS shows the machine: the registers grouped by their use
//! (PC, HI and LO first), the Run I/O console with a line to type the program's input, the flat
//! 32-bit memory (addresses outside every segment are shown as unmapped, not as zeros) and the
//! code list — address, machine word, instruction and source line.

use egui::{Color32, FontId, RichText, Ui};
use rustemu_mips::cpu::{Cpu, REG_NAMES};
use rustemu_mips::disasm::{Style, disassemble};
use rustemu_mips::memory::{self, Segment};
use rustemu_mips::target::Checkpoint;

use crate::app::{App, RunMode};

/// What the MIPS tools keep between frames.
#[derive(Default)]
pub struct MipsState {
    /// The input line of the console.
    pub input: String,
    /// The input line takes the keyboard at the next frame (the program waits for input).
    pub focus_request: bool,
    /// A register being edited: its index ([`rustemu_mips::target`]'s numbering) and the text.
    pub edit: Option<(usize, String)>,
    /// The instruction selected in the code list.
    pub selected: Option<u32>,
    /// Registers in signed decimal (otherwise hex).
    pub decimal: bool,
    /// The group of registers shown.
    pub group: RegGroup,
}

/// The registers the panel shows, one group at a time.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum RegGroup {
    /// `$zero..$ra`.
    #[default]
    Gpr,
    /// PC, HI, LO.
    Special,
    /// Coprocessor 0: the exception registers.
    Cp0,
    /// Coprocessor 1 (floating point): not emulated yet.
    Fpu,
}

impl RegGroup {
    /// The groups: tab name and title.
    pub const ALL: [(RegGroup, &'static str, &'static str); 4] = [
        (RegGroup::Gpr, "GPR", "General purpose registers"),
        (RegGroup::Special, "Special", "Program counter, multiply and divide results"),
        (RegGroup::Cp0, "CP0", "Coprocessor 0: exceptions"),
        (RegGroup::Fpu, "FPU", "Coprocessor 1: floating point"),
    ];
}

/// What a general purpose register is for (by its name).
fn role(name: &str) -> &'static str {
    match name.trim_end_matches(|c: char| c.is_ascii_digit()) {
        "zero" => "always 0",
        "at" => "assembler temporary (pseudo-instructions use it)",
        "v" => "results of a procedure, the system call number",
        "a" => "arguments of a procedure or a system call",
        "t" => "temporary: a procedure may change it",
        "s" => "saved: a procedure keeps it",
        "k" => "kernel: the exception handler's",
        "gp" => "global pointer",
        "sp" => "stack pointer",
        "fp" => "frame pointer",
        "ra" => "return address of jal",
        _ => "",
    }
}

/// A register of the panel.
struct Reg {
    /// Its index among the target's registers; None: read only.
    index: Option<usize>,
    name: String,
    /// The tooltip of its name.
    hint: String,
    now: u32,
    prev: u32,
}

/// The registers of a group (empty for the FPU).
fn group_regs(group: RegGroup, now: &Cpu, prev: &Cpu) -> Vec<Reg> {
    use rustemu_mips::cpu::cp0;
    match group {
        RegGroup::Gpr => (0..32)
            .map(|r| Reg {
                // $zero is wired to 0.
                index: (r != 0).then_some(r),
                name: format!("${}", REG_NAMES[r]),
                hint: format!("${r}: {}", role(REG_NAMES[r])),
                now: now.regs[r],
                prev: prev.regs[r],
            })
            .collect(),
        RegGroup::Special => [
            (PC, "PC", "program counter: the next instruction", now.pc, prev.pc),
            (HI, "HI", "mult: the high word; div: the remainder", now.hi, prev.hi),
            (LO, "LO", "mult: the low word; div: the quotient", now.lo, prev.lo),
        ]
        .into_iter()
        .map(|(i, name, hint, now, prev)| Reg { index: Some(i), name: name.into(), hint: hint.into(), now, prev })
        .collect(),
        RegGroup::Cp0 => [
            (cp0::VADDR, "BadVAddr", "the address of the last address exception"),
            (cp0::STATUS, "Status", "bit 1 (EXL): in the exception handler; bits 8..15: interrupt mask"),
            (cp0::CAUSE, "Cause", "bits 2..6: the code of the last exception"),
            (cp0::EPC, "EPC", "the address of the instruction that caused the exception"),
        ]
        .into_iter()
        .map(|(r, name, hint)| Reg {
            index: None,
            name: name.into(),
            hint: format!("CP0 ${r}: {hint} (mfc0 / mtc0)"),
            now: now.cp0[r],
            prev: prev.cp0[r],
        })
        .collect(),
        RegGroup::Fpu => Vec::new(),
    }
}

/// A register's name and value (the value can be changed while the program is paused).
fn reg_cell(app: &mut App, ui: &mut Ui, r: &Reg) {
    let font = FontId::monospace(13.0);
    let changed = r.now != r.prev;
    let color = if changed { crate::colors::error(ui) } else { ui.visuals().strong_text_color() };
    let shown = |v: u32| if app.mips.decimal { (v as i32).to_string() } else { format!("{v:08X}") };
    ui.label(RichText::new(&r.name).font(font.clone()).color(ui.visuals().weak_text_color())).on_hover_text(&r.hint);
    let editing = r.index.is_some() && app.mips.edit.as_ref().is_some_and(|(i, _)| Some(*i) == r.index);
    if editing {
        let (_, text) = app.mips.edit.as_mut().unwrap();
        let resp = ui.add(egui::TextEdit::singleline(text).font(font.clone()).desired_width(88.0));
        resp.request_focus();
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.mips.edit = None;
        } else if resp.lost_focus() {
            let (index, text) = app.mips.edit.take().unwrap();
            match parse_value(&text) {
                Some(v) if app.cpu_editable() => {
                    app.session.target.set_register(index, v as u64);
                    app.status = format!("{} = {} (changed by hand)", r.name, hex(v));
                }
                Some(_) => {}
                None => app.status = format!("'{text}' is not a 32-bit value"),
            }
        }
        return;
    }
    let editable = r.index.is_some() && app.cpu_editable();
    let mut text = RichText::new(shown(r.now)).font(font).color(color);
    if changed {
        text = text.strong();
    }
    let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    let other = if app.mips.decimal { hex(r.now) } else { (r.now as i32).to_string() };
    let mut tip = if changed { format!("{other}; was {}", shown(r.prev)) } else { other };
    if editable {
        tip.push_str("\nclick to change");
    } else if r.index.is_none() {
        tip.push_str("\nread only");
    }
    let resp = resp.on_hover_text(tip);
    if editable && resp.clicked() {
        let start = if app.mips.decimal { (r.now as i32).to_string() } else { hex(r.now) };
        app.mips.edit = r.index.map(|i| (i, start));
    }
}

/// The registers, one group at a time (GPR, Special, CP0, FPU), as pairs in two columns. Changed
/// ones are red; values can be changed while the program is paused.
pub fn registers_ui(app: &mut App, ui: &mut Ui) {
    let Some((now, prev)) = cpus(app) else {
        ui.weak("no MIPS machine");
        return;
    };
    ui.horizontal(|ui| {
        for (g, name, title) in RegGroup::ALL {
            ui.selectable_value(&mut app.mips.group, g, name).on_hover_text(title);
        }
    });
    let group = app.mips.group;
    let title = RegGroup::ALL.iter().find(|g| g.0 == group).map_or("", |g| g.2);
    ui.horizontal(|ui| {
        // Not aligned to the right: that would stretch a window to the whole screen.
        ui.label(RichText::new(title).strong());
        ui.add_space(12.0);
        ui.selectable_value(&mut app.mips.decimal, false, "HEX");
        ui.selectable_value(&mut app.mips.decimal, true, "DEC");
    });
    ui.add_space(2.0);
    if group == RegGroup::Fpu {
        // As wide as the registers would be: wrapped text would stretch a window to the screen.
        ui.scope(|ui| {
            ui.set_max_width(300.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Coprocessor 1 is not emulated yet: floating-point instructions stop the program with a \
                         reserved-instruction exception.",
                    )
                    .weak(),
                )
                .wrap(),
            );
        });
        return;
    }
    let regs = group_regs(group, &now, &prev);
    let mut grid = egui::Grid::new(("mips_regs", group as u8)).num_columns(4).spacing([14.0, 4.0]).striped(true);
    // The tablet's card: the columns take its width, the rows are taller for fingers. (A window
    // would grow to the screen.)
    if app.front.tablet_layout {
        grid = grid.min_col_width(((ui.available_width() - 3.0 * 14.0) / 4.0).max(60.0)).spacing([14.0, 6.0]);
    }
    grid.show(ui, |ui| {
        for pair in regs.chunks(2) {
            for r in pair {
                reg_cell(app, ui, r);
            }
            ui.end_row();
        }
    });
    if group == RegGroup::Cp0 && now.cp0[rustemu_mips::cpu::cp0::EPC] != 0 {
        let code = now.cp0[rustemu_mips::cpu::cp0::CAUSE] >> 2 & 31;
        let what = match code {
            4 => "address error on a load or fetch",
            5 => "address error on a store",
            8 => "syscall",
            9 => "break",
            10 => "reserved instruction",
            12 => "arithmetic overflow",
            13 => "trap",
            _ => "",
        };
        ui.weak(format!("last exception: code {code} {what}"));
    }
}

/// Index of PC, HI and LO among the target's registers (after `$zero..$ra` and `$0..$31`).
const PC: usize = 64;
const HI: usize = 65;
const LO: usize = 66;

/// The processor now and before the last step (the same when nothing ran yet).
fn cpus(app: &App) -> Option<(Cpu, Cpu)> {
    let now = app.session.mips()?.cpu.clone();
    let prev = app.exec.prev.get::<Checkpoint>().map_or_else(|| now.clone(), |c| c.cpu.clone());
    Some((now, prev))
}

/// A value as MARS shows it in hex.
fn hex(v: u32) -> String {
    format!("0x{v:08x}")
}

/// Parses a typed register value: `0x…` hex, or a decimal number (negative ones too).
fn parse_value(text: &str) -> Option<u32> {
    let t = text.trim().replace('_', "");
    if let Some(h) = t.strip_prefix("0x").or(t.strip_prefix("0X")) {
        return u32::from_str_radix(h, 16).ok();
    }
    t.parse::<i64>().ok().filter(|v| (-(1i64 << 31)..1i64 << 32).contains(v)).map(|v| v as u32)
}

/// The Run I/O console: the program's output, the input it has not read yet, and a line to type
/// more (Enter sends it with a new line, as MARS's Run I/O does).
pub fn console_ui(app: &mut App, ui: &mut Ui) {
    let Some(m) = app.session.mips() else {
        ui.weak("no MIPS machine");
        return;
    };
    let output = m.console.text();
    let pending = rustemu_mips::console::text(&m.pending_input());
    let waiting = m.is_waiting_input() && pending.is_empty();
    let finished = m.finished;
    let font = FontId::monospace(13.0);
    let input_h = 64.0;
    egui::ScrollArea::vertical()
        .id_salt("run_io")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .max_height((ui.available_height() - input_h).max(80.0))
        .show(ui, |ui| {
            let mut job = egui::text::LayoutJob::default();
            let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: c, ..Default::default() };
            job.append(&output, 0.0, fmt(ui.visuals().text_color()));
            if !pending.is_empty() {
                job.append(&pending, 0.0, fmt(ui.visuals().weak_text_color()));
            }
            if output.is_empty() && pending.is_empty() {
                job.append("(the program's output appears here)", 0.0, fmt(ui.visuals().weak_text_color()));
            }
            ui.add(egui::Label::new(job).wrap().selectable(true));
        });
    ui.separator();
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.mips.input)
                .font(font.clone())
                .desired_width((ui.available_width() - 70.0).max(80.0))
                .hint_text(if waiting { "the program waits: type here, Enter" } else { "input for the program" }),
        );
        if std::mem::take(&mut app.mips.focus_request) {
            r.request_focus();
        }
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("send").on_hover_text("Send the line and a new line (Enter)").clicked() || enter)
            && let Some(m) = app.session.mips_mut()
        {
            let line = std::mem::take(&mut app.mips.input);
            m.push_text(&format!("{line}\n"));
            if enter {
                r.request_focus();
            }
        }
    });
    let state = match finished {
        Some(f) => RichText::new(format!("-- program is finished running (exit code {}) --", f.code())).weak(),
        None if waiting => RichText::new("waiting for input").color(crate::colors::warn(ui)),
        None if app.run != RunMode::Paused => RichText::new("running").weak(),
        None => RichText::new(" ").weak(),
    };
    ui.label(state);
}

/// Parses an address of the memory window: hex (`0x10010000`, `10010000`) or an expression of
/// the debugger (`$sp`, `array + 8`).
pub fn parse_addr(app: &App, text: &str) -> Option<u32> {
    let t = text.trim();
    if let Some(h) = t.strip_prefix("0x").or(t.strip_prefix("0X")) {
        return u32::from_str_radix(h, 16).ok();
    }
    if let Ok(v) = u32::from_str_radix(t, 16)
        && t.len() >= 5
    {
        return Some(v);
    }
    app.session.compile(t).ok().map(|e| e.eval(&*app.session.target) as u32)
}

/// The places of the memory window's buttons: name, hover text, address (None: no machine).
fn quick_links(app: &App) -> Vec<(&'static str, &'static str, u32)> {
    let Some(m) = app.session.mips() else { return Vec::new() };
    vec![
        (".text", "user code (0x00400000)", memory::TEXT_BASE),
        (".data", "user data (0x10010000)", memory::DATA_BASE),
        ("heap", "memory of sbrk (0x10040000)", memory::HEAP_BASE),
        ("$gp", "global pointer", m.cpu.regs[28]),
        ("$sp", "the stack, from the stack pointer", m.cpu.regs[29]),
        ("PC", "the current instruction", m.cpu.pc),
        (".kdata", "kernel data (0x90000000)", memory::KDATA_BASE),
    ]
}

/// Rows of the memory window.
const MEM_ROWS: u32 = 16;

/// The memory as words: 16 bytes a row, each word's value (little-endian) and the bytes as text.
/// Addresses outside every segment are unmapped: an access there is an exception, so they are
/// shown as such, not as zeros.
pub fn memory_ui(app: &mut App, ui: &mut Ui) {
    let Some(m) = app.session.mips() else {
        ui.weak("no MIPS machine");
        return;
    };
    let (pc, sp) = (m.cpu.pc, m.cpu.regs[29]);
    ui.horizontal_wrapped(|ui| {
        ui.label("address:");
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.view.mem_addr_text).desired_width(120.0).hint_text("0x10010000, $sp"),
        );
        if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("go").clicked() {
            let text = app.view.mem_addr_text.clone();
            match parse_addr(app, &text) {
                Some(a) => app.view.mem_view_addr = a & !0xF,
                None => app.status = format!("bad address '{text}'"),
            }
        }
        for (name, hover, addr) in quick_links(app) {
            if ui.small_button(name).on_hover_text(hover).clicked() {
                app.view.mem_view_addr = addr & !0xF;
            }
        }
    });
    let base = app.view.mem_view_addr & !0xF;
    let font = FontId::monospace(12.5);
    let r = ui
        .vertical(|ui| {
            let t = &*app.session.target;
            for row in 0..MEM_ROWS {
                let a = base.wrapping_add(row * 16);
                let mut job = egui::text::LayoutJob::default();
                let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: c, ..Default::default() };
                let txt = ui.visuals().text_color();
                let weak = ui.visuals().weak_text_color();
                job.append(&format!("{a:08x}  "), 0.0, fmt(weak));
                let mut chars = String::new();
                for w in 0..4u32 {
                    let wa = a.wrapping_add(w * 4);
                    if Segment::of(wa).is_none() {
                        job.append("········ ", 0.0, fmt(weak));
                        chars.push_str("    ");
                        continue;
                    }
                    let bytes = t.read_bytes(wa, 4);
                    let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    let col = if wa == pc {
                        crate::colors::info(ui)
                    } else if wa == sp {
                        crate::colors::warn(ui)
                    } else if v == 0 {
                        weak
                    } else {
                        txt
                    };
                    job.append(&format!("{v:08x} "), 0.0, fmt(col));
                    chars.extend(bytes.iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }));
                }
                let label = match Segment::of(a) {
                    None => "  unmapped".to_string(),
                    Some(_) => format!(" {chars}"),
                };
                job.append(&label, 0.0, fmt(weak));
                ui.add(egui::Label::new(job).extend());
            }
        })
        .response;
    // The wheel moves a row at a time over the 4 GiB address space.
    if r.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy.abs() >= 1.0 {
            let rows = (-dy / 18.0).round() as i32;
            let rows = if rows == 0 { -dy.signum() as i32 } else { rows };
            app.view.mem_view_addr = base.wrapping_add_signed(rows * 16);
        }
    }
    ui.horizontal(|ui| {
        if ui.small_button("⏶ page").clicked() {
            app.view.mem_view_addr = base.wrapping_sub(MEM_ROWS * 16);
        }
        if ui.small_button("⏷ page").clicked() {
            app.view.mem_view_addr = base.wrapping_add(MEM_ROWS * 16);
        }
        ui.weak("words, little-endian · blue = PC, yellow = $sp · ········ = unmapped");
    });
}

/// Rows of the code list.
const CODE_ROWS: u32 = 18;

/// The code around the PC (MARS's text segment): breakpoint, address, machine word, instruction
/// and the source line. A click selects an instruction, a click in the first column sets or
/// clears a breakpoint there; the wheel scrolls (until the next step).
pub fn code_ui(app: &mut App, ui: &mut Ui) {
    let Some(m) = app.session.mips() else {
        ui.weak("no MIPS machine");
        return;
    };
    let pc = m.cpu.pc;
    if !app.loaded() {
        ui.weak("Nothing is loaded: emulate (F9) assembles the program in the editor.");
        return;
    }
    if !app.view.code_view_pinned {
        let start = app.view.code_view_start;
        if pc < start || pc >= start.wrapping_add(CODE_ROWS * 4) || !start.is_multiple_of(4) {
            // A few instructions before the PC, within its segment.
            let back = (1..=4u32).take_while(|k| Segment::of(pc.wrapping_sub(4 * k)) == Segment::of(pc)).count();
            app.view.code_view_start = pc.wrapping_sub(4 * back as u32);
        }
    }
    let start = app.view.code_view_start & !3;
    let font = FontId::monospace(12.5);
    let source = app.program.files.first().map(|f| &f.1);
    let mut toggle = None;
    let mut select = None;
    let r = egui::Grid::new("mips_code")
        .num_columns(5)
        .spacing([10.0, 1.0])
        .striped(true)
        .show(ui, |ui| {
            for col in ["", "address", "code", "instruction", "source"] {
                ui.label(RichText::new(col).small().weak());
            }
            ui.end_row();
            let t = &*app.session.target;
            for row in 0..CODE_ROWS {
                let a = start.wrapping_add(row * 4);
                let mapped = Segment::of(a).is_some();
                let word = if mapped { u32::from_le_bytes(t.read_bytes(a, 4).try_into().unwrap()) } else { 0 };
                let bp = t.breakpoints().contains(&a);
                let current = a == pc;
                let selected = app.mips.selected == Some(a);
                let dot = ui
                    .add(
                        egui::Label::new(RichText::new(if bp { "●" } else { "○" }).color(if bp {
                            Color32::from_rgb(0xE0, 0x40, 0x40)
                        } else {
                            ui.visuals().weak_text_color()
                        }))
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_text("set / clear a breakpoint");
                if dot.clicked() {
                    toggle = Some(a);
                }
                let color = if current { crate::colors::info(ui) } else { ui.visuals().text_color() };
                let strong = |s: String| {
                    let t = RichText::new(s).font(font.clone()).color(color);
                    if current || selected { t.strong() } else { t }
                };
                let marker = if current { "▶" } else { " " };
                let addr = ui.add(egui::Label::new(strong(format!("{marker}{a:08x}"))).sense(egui::Sense::click()));
                if !mapped {
                    ui.label(RichText::new("unmapped").font(font.clone()).weak());
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                    continue;
                }
                ui.label(RichText::new(format!("{word:08x}")).font(font.clone()).weak());
                let text = if m.mem.has_code(a) { disassemble(word, a, Style::Named) } else { String::new() };
                let ins = ui.add(egui::Label::new(strong(text)).sense(egui::Sense::click()));
                let line = app.session.line_at_addr(a).filter(|(f, _)| *f == 0).map(|(_, l)| l);
                let src = match (line, source) {
                    (Some(l), Some(lines)) if app.session.addr_of_line(0, l) == Some(a) => {
                        // Short: a long line would stretch the window.
                        let text = lines.get(l as usize - 1).map_or("", |s| s.trim());
                        let mut text: String = text.chars().take(32).collect();
                        if text.len() < lines.get(l as usize - 1).map_or(0, |s| s.trim().len()) {
                            text.push('…');
                        }
                        format!("{l}: {text}")
                    }
                    _ => String::new(),
                };
                ui.label(RichText::new(src).font(font.clone()).weak());
                ui.end_row();
                if addr.clicked() || ins.clicked() {
                    select = Some(a);
                }
            }
        })
        .response;
    if let Some(a) = toggle {
        app.toggle_breakpoint_at_addr(a);
    }
    if let Some(a) = select {
        app.mips.selected = Some(a);
    }
    if r.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy.abs() >= 1.0 {
            let rows = (-dy / 18.0).round() as i32;
            let rows = if rows == 0 { -dy.signum() as i32 } else { rows };
            app.view.code_view_start = start.wrapping_add_signed(rows * 4);
            app.view.code_view_pinned = true;
        }
    }
}

/// What the last instruction did, from the step-back history.
pub struct MipsStep {
    /// Its address.
    pub addr: u32,
    /// The instruction (register names, targets as addresses).
    pub text: String,
    /// Changed registers: name, before, after.
    pub regs: Vec<(String, u32, u32)>,
    /// Changed bytes of memory: address, before, after.
    pub memory: Vec<(u32, u8, u8)>,
}

/// The last executed instruction and what it changed (None: nothing in the history).
pub fn last_step(app: &App) -> Option<MipsStep> {
    let m = app.session.mips()?;
    let (before, undo) = m.last_step()?;
    let addr = before.pc;
    let text = disassemble(m.mem.peek32(addr), addr, Style::Named);
    let mut regs: Vec<(String, u32, u32)> = (1..32)
        .filter(|&i| before.regs[i] != m.cpu.regs[i])
        .map(|i| (format!("${}", REG_NAMES[i]), before.regs[i], m.cpu.regs[i]))
        .collect();
    for (name, a, b) in [("HI", before.hi, m.cpu.hi), ("LO", before.lo, m.cpu.lo)] {
        if a != b {
            regs.push((name.to_string(), a, b));
        }
    }
    let mut memory: Vec<(u32, u8, u8)> = Vec::new();
    for &(a, old) in undo {
        if !memory.iter().any(|&(x, _, _)| x == a) {
            memory.push((a, old, m.mem.peek8(a)));
        }
    }
    memory.retain(|&(_, old, new)| old != new);
    memory.sort_by_key(|&(a, _, _)| a);
    Some(MipsStep { addr, text, regs, memory })
}

/// Text of the listing: per instruction its address, machine word, MARS's basic form and the
/// source line it came from (MARS's text segment window), then the data lines.
pub fn listing_text(app: &App) -> Option<String> {
    let prog = app.session.mips_program.as_ref()?;
    let lines = &app.program.files.first()?.1;
    let src = |l: usize| lines.get(l.wrapping_sub(1)).map_or("", |s| s.trim_end());
    let mut out = String::from("address    code       basic                      source\n");
    for s in &prog.statements {
        for (k, (w, b)) in s.words.iter().zip(&s.basic).enumerate() {
            let source = if k == 0 { format!("{:>4}: {}", s.line, src(s.line)) } else { String::new() };
            out += &format!("{:08x}   {w:08x}   {b:<26} {source}\n", s.addr + 4 * k as u32);
        }
    }
    if !prog.data.is_empty() {
        out += "\ndata\n";
        for d in prog.data.iter().filter(|d| d.len > 0) {
            out += &format!("{:08x}   {:>5} bytes  {:>4}: {}\n", d.addr, d.len, d.line, src(d.line));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        assert_eq!(parse_value("0x10"), Some(16));
        assert_eq!(parse_value("-1"), Some(u32::MAX));
        assert_eq!(parse_value("4294967295"), Some(u32::MAX));
        assert_eq!(parse_value("4294967296"), None);
        assert_eq!(parse_value("x"), None);
    }
}
