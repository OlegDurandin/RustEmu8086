//! The CPU panel: AX..DX as a word and its two bytes (AH | AL), SP BP SI DI, the segment
//! registers and IP, the nine flags, and an inspector of the tapped value (hex, decimal, binary;
//! editable while the program is paused). The full panel is on the Debugger tab, a preview (AX..DX,
//! CF ZF SF OF, segments) on the Editor tab.
//!
//! The bytes are views of the word, never values of their own: AH = AX >> 8, AL = AX & FF. A
//! value is marked (green, with a dot) when it differs from the state before the last step / run.

use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use rustemu_core::Cpu;
use rustemu_session::step::{FLAGS, Half, PAIRS, SEGMENTS, WORDS};
use rustemu_ui::app::App;

use crate::theme::pal;
use crate::widgets::{HEAD_H, card};

/// A value of the CPU panel that can be inspected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Sel {
    /// A word register by its index in `Cpu::regs`.
    Word(usize),
    /// A byte of AX..DX.
    Byte(usize, Half),
    /// A segment register by its index in `Cpu::sregs`.
    Seg(usize),
    Ip,
    Flag(u16),
}

impl Sel {
    fn name(self) -> &'static str {
        match self {
            Sel::Word(i) => PAIRS.iter().map(|p| (p.0, p.1)).chain(WORDS).find(|w| w.0 == i).map_or("?", |w| w.1),
            Sel::Byte(i, h) => PAIRS.iter().find(|p| p.0 == i).map_or("?", |p| if h == Half::High { p.2 } else { p.3 }),
            Sel::Seg(i) => SEGMENTS.iter().find(|s| s.0 == i).map_or("?", |s| s.1),
            Sel::Ip => "IP",
            Sel::Flag(bit) => FLAGS.iter().find(|f| f.0 == bit).map_or("?", |f| f.1),
        }
    }

    fn bits(self) -> u32 {
        match self {
            Sel::Byte(..) => 8,
            Sel::Flag(_) => 1,
            _ => 16,
        }
    }

    pub(crate) fn get(self, c: &Cpu) -> u16 {
        match self {
            Sel::Word(i) => c.regs[i],
            Sel::Byte(i, Half::High) => c.regs[i] >> 8,
            Sel::Byte(i, Half::Low) => c.regs[i] & 0xFF,
            Sel::Seg(i) => c.sregs[i],
            Sel::Ip => c.ip,
            Sel::Flag(bit) => (c.flags & bit != 0) as u16,
        }
    }

    /// Writes `v`; a byte replaces only its half of the word.
    pub(crate) fn set(self, c: &mut Cpu, v: u16) {
        match self {
            Sel::Word(i) => c.regs[i] = v,
            Sel::Byte(i, Half::High) => c.regs[i] = (c.regs[i] & 0x00FF) | (v & 0xFF) << 8,
            Sel::Byte(i, Half::Low) => c.regs[i] = (c.regs[i] & 0xFF00) | (v & 0xFF),
            Sel::Seg(i) => c.sregs[i] = v,
            Sel::Ip => c.ip = v,
            Sel::Flag(bit) => {
                if v != 0 {
                    c.flags |= bit
                } else {
                    c.flags &= !bit
                }
            }
        }
    }
}

/// State of the CPU panels: the inspected value and its edit field.
#[derive(Default)]
pub(crate) struct CpuView {
    pub(crate) sel: Option<Sel>,
    pub(crate) edit: String,
    error: Option<String>,
}

impl CpuView {
    fn select(&mut self, s: Sel, c: &Cpu) {
        if self.sel == Some(s) {
            self.sel = None;
            return;
        }
        self.sel = Some(s);
        self.edit = format!("{:0w$X}", s.get(c), w = (s.bits() as usize).div_ceil(4));
        self.error = None;
    }
}

const CELL_H: f32 = 44.0;
const GAP: f32 = 8.0;
const FLAG_W: f32 = 46.0;

/// How a cell looks.
struct Look {
    changed: bool,
    selected: bool,
}

/// A value cell: a tap target with a small name in its corner and the value in the middle.
fn cell(ui: &mut Ui, rect: Rect, name: &str, value: &str, look: Look) -> egui::Response {
    let id = ui.scope_id().with(("cpu_cell", name));
    let resp = ui.interact(rect, id, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name} = {value}")));
    let accent = if look.changed { Some(pal::GREEN) } else { None };
    let fill = accent.map_or(pal::FIELD, |c| c.gamma_multiply(0.14));
    let stroke =
        if look.selected { Stroke::new(2.0, pal::LINK) } else { Stroke::new(1.0, accent.unwrap_or(pal::BORDER)) };
    let p = ui.painter();
    p.rect(rect, 5.0, fill, stroke, StrokeKind::Inside);
    p.text(rect.left_top() + vec2(7.0, 4.0), Align2::LEFT_TOP, name, FontId::proportional(11.5), pal::WEAK);
    let size = 16.0;
    let color = accent.unwrap_or(pal::TEXT);
    p.text(rect.center() + vec2(0.0, 5.0), Align2::CENTER_CENTER, value, FontId::monospace(size), color);
    if look.changed {
        // Not by colour alone: a dot marks the changed value.
        p.circle_filled(rect.right_top() + vec2(-8.0, 8.0), 3.5, pal::GREEN);
    }
    resp
}

/// Height of a register cell: the word on top, its bytes below.
const PAIR_H: f32 = 30.0 + CELL_H;
/// Height of the word strip at the top of a register cell (tapping it selects the word).
const WORD_H: f32 = 30.0;

/// One cell for a general register: `AX  001F` on top, its bytes `AH 00 │ AL 1F` below. The word,
/// and each byte, is its own tap target and is marked on its own when it changed.
fn pair_cell(ui: &mut Ui, rect: Rect, cpu: &Cpu, prev: &Cpu, i: usize, view: &mut CpuView) {
    let (name, hi, lo) = PAIRS.iter().find(|p| p.0 == i).map(|p| (p.1, p.2, p.3)).unwrap();
    let (v, old) = (cpu.regs[i], prev.regs[i]);
    let sel = view.sel;
    let word_rect = Rect::from_min_size(rect.min, vec2(rect.width(), WORD_H));
    let half_w = rect.width() / 2.0;
    let halves = [(Half::High, hi, 8u16, 0.0), (Half::Low, lo, 0u16, half_w)].map(|(h, n, shift, x)| {
        (h, n, shift, Rect::from_min_size(pos2(rect.left() + x, word_rect.bottom()), vec2(half_w, CELL_H)))
    });
    let mut tapped = None;
    let word_resp = ui.interact(word_rect, ui.scope_id().with(("cpu_word", name)), Sense::click());
    word_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name} = {v:04X}")));
    if word_resp.clicked() {
        tapped = Some(Sel::Word(i));
    }
    let p = ui.painter().clone();
    let changed = v != old;
    let stroke = if sel == Some(Sel::Word(i)) {
        Stroke::new(2.0, pal::LINK)
    } else {
        Stroke::new(1.0, if changed { pal::GREEN } else { pal::BORDER })
    };
    p.rect(rect, 6.0, pal::FIELD, stroke, StrokeKind::Inside);
    // The word: its name and value.
    p.text(word_rect.left_center() + vec2(9.0, 1.0), Align2::LEFT_CENTER, name, FontId::proportional(14.0), pal::WEAK);
    let word_color = if changed { pal::GREEN } else { pal::TEXT };
    p.text(
        word_rect.right_center() + vec2(-12.0, 1.0),
        Align2::RIGHT_CENTER,
        format!("{v:04X}"),
        FontId::monospace(18.0),
        word_color,
    );
    if changed {
        // Not by colour alone: a dot marks the changed value.
        p.circle_filled(word_rect.right_top() + vec2(-6.0, 6.0), 3.0, pal::GREEN);
    }
    p.line_segment(
        [pos2(rect.left() + 6.0, word_rect.bottom()), pos2(rect.right() - 6.0, word_rect.bottom())],
        Stroke::new(1.0, pal::BORDER),
    );
    p.line_segment(
        [pos2(rect.center().x, word_rect.bottom() + 6.0), pos2(rect.center().x, rect.bottom() - 6.0)],
        Stroke::new(1.0, pal::BORDER),
    );
    for (half, bname, shift, r) in halves {
        let (b, ob) = ((v >> shift) & 0xFF, (old >> shift) & 0xFF);
        let resp = ui.interact(r, ui.scope_id().with(("cpu_byte", bname)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{bname} = {b:02X}")));
        if resp.clicked() {
            tapped = Some(Sel::Byte(i, half));
        }
        let byte_changed = b != ob;
        let inner = r.shrink(3.0);
        if sel == Some(Sel::Byte(i, half)) {
            p.rect_stroke(inner, 4.0, Stroke::new(2.0, pal::LINK), StrokeKind::Inside);
        } else if byte_changed {
            p.rect_filled(inner, 4.0, pal::GREEN.gamma_multiply(0.14));
        }
        p.text(inner.left_center() + vec2(7.0, 0.0), Align2::LEFT_CENTER, bname, FontId::proportional(12.0), pal::WEAK);
        let color = if byte_changed { pal::GREEN } else { pal::TEXT };
        p.text(
            inner.center() + vec2(8.0, 0.0),
            Align2::CENTER_CENTER,
            format!("{b:02X}"),
            FontId::monospace(17.0),
            color,
        );
    }
    if let Some(s) = tapped {
        view.select(s, cpu);
    }
}

/// Columns of the register cells: 4 in a row when wide, 2 x 2 otherwise.
fn pair_cols(w: f32) -> usize {
    if w >= 4.0 * 128.0 + 3.0 * GAP { 4 } else { 2 }
}

/// AX..DX, one cell each.
fn pairs(ui: &mut Ui, top: f32, rect: Rect, cpu: &Cpu, prev: &Cpu, view: &mut CpuView) -> f32 {
    let cols = pair_cols(rect.width());
    let w = (rect.width() - GAP * (cols - 1) as f32) / cols as f32;
    for (k, p) in PAIRS.iter().enumerate() {
        let (row, col) = (k / cols, k % cols);
        let r = Rect::from_min_size(
            pos2(rect.left() + col as f32 * (w + GAP), top + row as f32 * (PAIR_H + GAP)),
            vec2(w, PAIR_H),
        );
        pair_cell(ui, r, cpu, prev, p.0, view);
    }
    top + pairs_height(rect.width())
}

fn pairs_height(w: f32) -> f32 {
    (4 / pair_cols(w)) as f32 * (PAIR_H + GAP)
}

/// A row of equal cells.
fn cell_row(
    ui: &mut Ui,
    top: f32,
    rect: Rect,
    items: &[(Sel, String, bool)],
    view: &mut CpuView,
    cpu: &Cpu,
    max_w: f32,
) -> f32 {
    let n = items.len() as f32;
    let w = ((rect.width() - GAP * (n - 1.0)) / n).min(max_w);
    let mut tapped = None;
    for (k, (s, value, changed)) in items.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + k as f32 * (w + GAP), top), vec2(w, CELL_H));
        let look = Look { changed: *changed, selected: view.sel == Some(*s) };
        if cell(ui, r, s.name(), value, look).clicked() {
            tapped = Some(*s);
        }
    }
    if let Some(s) = tapped {
        view.select(s, cpu);
    }
    top + CELL_H + GAP
}

fn flag_items(cpu: &Cpu, prev: &Cpu, only: &[&str]) -> Vec<(Sel, String, bool)> {
    FLAGS
        .iter()
        .filter(|f| only.is_empty() || only.contains(&f.1))
        .map(|&(bit, ..)| {
            (Sel::Flag(bit), ((cpu.flags & bit != 0) as u8).to_string(), (cpu.flags ^ prev.flags) & bit != 0)
        })
        .collect()
}

/// Rows of `n` flag cells in width `w` and the cells per row (balanced).
fn flag_layout(w: f32, n: usize) -> (usize, usize) {
    let fit = (((w + GAP) / (FLAG_W + GAP)).floor() as usize).max(1);
    let rows = n.div_ceil(fit);
    (rows, n.div_ceil(rows))
}

/// Flag cells: one row when they fit, more otherwise.
fn flag_rows(ui: &mut Ui, top: f32, rect: Rect, items: Vec<(Sel, String, bool)>, view: &mut CpuView, cpu: &Cpu) -> f32 {
    let (_, per_row) = flag_layout(rect.width(), items.len());
    let mut y = top;
    for chunk in items.chunks(per_row) {
        y = cell_row(ui, y, rect, chunk, view, cpu, 64.0);
    }
    y
}

/// Height of the inspector strip: a hint line (`hint`), or the value and its edit field.
fn inspector_h(view: &CpuView, hint: bool) -> f32 {
    if view.sel.is_some() {
        2.0 * CELL_H + GAP
    } else if hint {
        24.0
    } else {
        0.0
    }
}

/// Height of the full CPU card for a width.
pub(crate) fn full_height(w: f32, view: &CpuView) -> f32 {
    let inner = w - 24.0;
    HEAD_H
        + 12.0
        + pairs_height(inner)
        + 2.0 * (CELL_H + GAP)
        + flag_layout(inner, 9).0 as f32 * (CELL_H + GAP)
        + inspector_h(view, true)
        + 8.0
}

/// Height of the MIPS card: the PC, the tabs, the title and the 16 rows of the general purpose
/// registers.
pub(crate) fn mips_height() -> f32 {
    HEAD_H + 12.0 + 26.0 + 2.0 * 36.0 + 16.0 * 24.0 + 8.0
}

/// Height of the preview card for a width.
pub(crate) fn preview_height(w: f32, view: &CpuView) -> f32 {
    let inner = w - 24.0;
    HEAD_H + 12.0 + pairs_height(inner) + CELL_H + GAP + 24.0 + inspector_h(view, false) + 8.0
}

/// The 8086 registers now and before the last step / run (the panel shows the 8086).
fn cpus(app: &App) -> Option<(Cpu, Cpu)> {
    let cpu = app.session.x86()?.cpu.clone();
    let prev = app.exec.prev_x86().map_or_else(|| cpu.clone(), |p| p.cpu.clone());
    Some((cpu, prev))
}

/// The MIPS registers by their use (PC, HI, LO first); the preview links to the debugger.
/// Returns true when "Open debugger" was tapped.
fn mips_card(app: &mut App, ui: &mut Ui, rect: Rect, preview: bool) -> bool {
    let (mut head, mut b) = card(ui, rect, "CPU · MIPS32");
    let open = preview
        && head
            .add(egui::Button::new(egui::RichText::new("Open debugger ›").color(pal::LINK)).frame(false))
            .on_hover_text("the last step, the code and the tools")
            .clicked();
    egui::ScrollArea::vertical()
        .id_salt(("t_mips_regs", preview))
        .auto_shrink([false, false])
        .show(&mut b, |ui| rustemu_ui::mips_ui::registers_ui(app, ui));
    open
}

/// The full CPU panel (Debugger).
pub(crate) fn cpu_card(app: &mut App, view: &mut CpuView, ui: &mut Ui, rect: Rect) {
    if app.session.mips().is_some() {
        mips_card(app, ui, rect, false);
        return;
    }
    let (_, b) = card(ui, rect, "CPU");
    let area = b.max_rect();
    let Some((cpu, prev)) = cpus(app) else { return };
    let mut y = pairs(ui, area.top() + 4.0, area, &cpu, &prev, view);
    let words: Vec<(Sel, String, bool)> = WORDS
        .iter()
        .map(|&(i, _)| (Sel::Word(i), format!("{:04X}", cpu.regs[i]), cpu.regs[i] != prev.regs[i]))
        .collect();
    y = cell_row(ui, y, area, &words, view, &cpu, f32::INFINITY);
    let mut segs: Vec<(Sel, String, bool)> = SEGMENTS
        .iter()
        .map(|&(i, _)| (Sel::Seg(i), format!("{:04X}", cpu.sregs[i]), cpu.sregs[i] != prev.sregs[i]))
        .collect();
    segs.push((Sel::Ip, format!("{:04X}", cpu.ip), cpu.ip != prev.ip));
    y = cell_row(ui, y, area, &segs, view, &cpu, f32::INFINITY);
    y = flag_rows(ui, y, area, flag_items(&cpu, &prev, &[]), view, &cpu);
    inspector(app, view, ui, Rect::from_min_max(pos2(area.left(), y), area.max), true);
}

/// The CPU preview (Editor): AX..DX with their bytes, CF ZF SF OF, the segments as text, and a way
/// to the debugger. Returns true when "Open debugger" was tapped.
pub(crate) fn cpu_preview_card(app: &mut App, view: &mut CpuView, ui: &mut Ui, rect: Rect) -> bool {
    if app.session.mips().is_some() {
        return mips_card(app, ui, rect, true);
    }
    let (mut head, b) = card(ui, rect, "CPU");
    let open = head
        .add(egui::Button::new(egui::RichText::new("Open debugger ›").color(pal::LINK)).frame(false))
        .on_hover_text("all registers, flags and the last step")
        .clicked();
    let area = b.max_rect();
    let Some((cpu, prev)) = cpus(app) else { return open };
    let mut y = pairs(ui, area.top() + 4.0, area, &cpu, &prev, view);
    y = flag_rows(ui, y, area, flag_items(&cpu, &prev, &["CF", "ZF", "SF", "OF"]), view, &cpu);
    let segs: Vec<String> = SEGMENTS.iter().map(|&(i, n)| format!("{n} {:04X}", cpu.sregs[i])).collect();
    ui.painter().text(
        pos2(area.left(), y + 10.0),
        Align2::LEFT_CENTER,
        segs.join("  ·  "),
        FontId::monospace(14.0),
        pal::WEAK,
    );
    y += 24.0;
    inspector(app, view, ui, Rect::from_min_max(pos2(area.left(), y), area.max), false);
    open
}

/// The tapped value in hex, decimal and binary, with an edit field while the program is paused.
fn inspector(app: &mut App, view: &mut CpuView, ui: &mut Ui, rect: Rect, hint: bool) {
    let mut s = ui.new_child(egui::UiBuilder::new().id_salt("cpu_inspector").max_rect(rect));
    let Some(sel) = view.sel else {
        if hint {
            s.label(
                egui::RichText::new("Tap a register or a flag: its value in hex, decimal and binary.").color(pal::WEAK),
            );
        }
        return;
    };
    let Some(cpu) = app.session.x86().map(|m| &m.cpu) else { return };
    let v = sel.get(cpu);
    let bits = sel.bits();
    let name = sel.name();
    if let Sel::Flag(bit) = sel {
        let desc = FLAGS.iter().find(|f| f.0 == bit).map_or("", |f| f.2);
        s.label(egui::RichText::new(format!("{name} = {v}")).monospace().size(16.0).color(pal::TEXT));
        s.label(egui::RichText::new(desc).color(pal::WEAK));
        if app.cpu_editable() {
            if s.button(if v != 0 { "Clear the flag" } else { "Set the flag" }).clicked() {
                app.edit_cpu(|c| sel.set(c, (v == 0) as u16));
            }
        } else {
            s.label(egui::RichText::new(read_only_reason(app)).color(pal::WEAK));
        }
        return;
    }
    let digits = (bits / 4) as usize;
    let signed = if bits == 8 { v as u8 as i8 as i32 } else { v as i16 as i32 };
    let bin: String = format!("{v:0w$b}", w = bits as usize)
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let mut line = format!("{name} = {v:0digits$X}h · {v}");
    if signed < 0 {
        line.push_str(&format!(" ({signed} signed)"));
    }
    line.push_str(&format!(" · {bin}b"));
    s.label(egui::RichText::new(line).monospace().size(15.0).color(pal::TEXT));
    if !app.cpu_editable() {
        s.label(egui::RichText::new(read_only_reason(app)).color(pal::WEAK));
        return;
    }
    s.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("new {name} (hex)")).color(pal::WEAK));
        let r = ui.add(
            egui::TextEdit::singleline(&mut view.edit)
                .char_limit(digits)
                .desired_width(70.0)
                .font(egui::TextStyle::Monospace),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Set").clicked() || enter {
            match u16::from_str_radix(view.edit.trim(), 16) {
                Ok(n) if (n as u32) < (1 << bits) => {
                    app.edit_cpu(|c| sel.set(c, n));
                    view.error = None;
                }
                _ => view.error = Some(format!("{digits} hex digits: 0..{:X}", (1u32 << bits) - 1)),
            }
        }
        if let Some(e) = &view.error {
            ui.colored_label(pal::RED, e);
        }
    });
}

fn read_only_reason(app: &App) -> &'static str {
    if !app.loaded() {
        "Load the program (Run or Step) to change registers."
    } else if app.session.target.is_waiting_input() {
        "Read-only while the program waits for a key."
    } else {
        "Read-only while the program runs: pause it to change values."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_views_of_the_word() {
        let mut c = Cpu::new();
        c.regs[rustemu_core::cpu::AX] = 0x1234;
        Sel::Byte(rustemu_core::cpu::AX, Half::Low).set(&mut c, 0xAA);
        assert_eq!(c.regs[rustemu_core::cpu::AX], 0x12AA);
        c.regs[rustemu_core::cpu::AX] = 0x1234;
        Sel::Byte(rustemu_core::cpu::AX, Half::High).set(&mut c, 0xFF);
        assert_eq!(c.regs[rustemu_core::cpu::AX], 0xFF34);
        assert_eq!(Sel::Byte(rustemu_core::cpu::AX, Half::Low).get(&c), 0x34);
        Sel::Word(rustemu_core::cpu::AX).set(&mut c, 0xABCD);
        assert_eq!(Sel::Byte(rustemu_core::cpu::AX, Half::High).get(&c), 0xAB);
        assert_eq!(Sel::Byte(rustemu_core::cpu::AX, Half::Low).get(&c), 0xCD);
        assert_eq!(Sel::Byte(rustemu_core::cpu::BX, Half::Low).name(), "BL");
        assert_eq!(Sel::Word(rustemu_core::cpu::SI).name(), "SI");
    }
}
