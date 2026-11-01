//! The Editor / Debugger / Memory tabs: cards laid out for the window size.
//!
//! Each tab has one main area and its context: the source with the program's output and a CPU
//! preview (Editor); the last step and the code with the full CPU and the output (Debugger); the
//! bytes with the inspector, segments and stack (Memory). Landscape: two columns; portrait and
//! narrow windows: the main area above its context.

use egui::{Rect, Ui};
use rustemu_ui::app::App;

use crate::cards::{editor_card, output_card};
use crate::debugger::{TOOLS_ROW_H, code_card, step_card, step_height, tools_area};
use crate::memory::{hex_editor_card, inspector_card, memory_map_card, segments_card, stack_card};
use crate::widgets::{GAP, HEAD_H, split_x, split_y};
use crate::{Tab, TabletState, cpu};

/// Two columns fit: a landscape window wide enough for the source beside its context.
fn landscape(r: Rect) -> bool {
    r.width() >= r.height() * 1.15 && r.width() >= 900.0
}

/// Splits off the bottom row of a tab, which gives way to the on-screen keyboard.
fn bottom_row(t: &TabletState, rect: Rect, h: f32) -> (Rect, Option<Rect>) {
    if t.keypad.open {
        return (rect, None);
    }
    let (top, bottom) = split_y(rect, rect.height() - h - GAP);
    (top, Some(bottom))
}

fn cpu_preview(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    if cpu::cpu_preview_card(app, &mut t.cpu, ui, rect) {
        t.tab = Tab::Debugger;
    }
}

/// The source (about 60% of a landscape window) with the output above a CPU preview; no
/// disassembly here (it is on the Debugger tab).
pub(crate) fn editor_tab(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    if landscape(rect) {
        let right_w = (rect.width() * 0.41).clamp(380.0, 600.0);
        let (editor, right) = split_x(rect, rect.width() - right_w - GAP);
        editor_card(app, ui, editor);
        if t.keypad.open {
            // The keyboard is below: the output keeps the whole column.
            output_card(app, t, ui, right);
            return;
        }
        let cpu_h = cpu::preview_height(right.width(), &t.cpu).min(right.height() * 0.62);
        let (out, regs) = split_y(right, right.height() - cpu_h - GAP);
        output_card(app, t, ui, out);
        cpu_preview(app, t, ui, regs);
        return;
    }
    // Portrait and narrow windows: the source above, the output beside the CPU below.
    let half = (rect.width() - GAP) / 2.0;
    let bottom_h = if t.keypad.open {
        (rect.height() * 0.4).max(220.0)
    } else {
        cpu::preview_height(half, &t.cpu).max(260.0).min(rect.height() * 0.55)
    };
    let (editor, bottom) = split_y(rect, rect.height() - bottom_h - GAP);
    editor_card(app, ui, editor);
    if t.keypad.open {
        output_card(app, t, ui, bottom);
        return;
    }
    let (out, regs) = split_x(bottom, half);
    output_card(app, t, ui, out);
    cpu_preview(app, t, ui, regs);
}

/// The code with the open tool below it.
fn code_and_tools(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let tools_h =
        if t.debug.tool.is_some() { (rect.height() * 0.45).max(TOOLS_ROW_H + HEAD_H + 80.0) } else { TOOLS_ROW_H };
    let (code, tools) = split_y(rect, (rect.height() - tools_h - GAP).max(HEAD_H + 60.0));
    code_card(app, t, ui, code);
    tools_area(app, t, ui, tools);
}

/// The last step above the code (about 55% of a landscape window), the full CPU and the output
/// beside them; the tools open one at a time below the code. The output becomes one of the tools
/// when there is no room for it beside the CPU.
pub(crate) fn debugger_tab(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let step_h = step_height(t);
    const MIN_OUTPUT: f32 = HEAD_H + 150.0;
    if landscape(rect) {
        let (left, right) = split_x(rect, rect.width() * 0.55);
        let w = right.width();
        let cpu_h = cpu::full_height(w, &t.cpu).min(right.height());
        let (regs, out) = split_y(right, cpu_h);
        t.debug.output_tool = out.height() < MIN_OUTPUT;
        let (step, rest) = split_y(left, step_h);
        step_card(app, t, ui, step);
        code_and_tools(app, t, ui, rest);
        cpu::cpu_card(app, &mut t.cpu, ui, regs);
        if !t.debug.output_tool {
            output_card(app, t, ui, out);
        }
        return;
    }
    let (step, rest) = split_y(rect, step_h);
    step_card(app, t, ui, step);
    let wide = rect.width() >= 900.0;
    // Portrait: the CPU beside the output; narrow: the CPU alone (the output is a tool).
    let cpu_w = if wide { rect.width() * 0.62 } else { rect.width() };
    let cpu_h = cpu::full_height(cpu_w, &t.cpu).min(rest.height() * 0.6);
    t.debug.output_tool = !wide;
    let (code, bottom) = split_y(rest, rest.height() - cpu_h - GAP);
    code_and_tools(app, t, ui, code);
    if wide {
        let (regs, out) = split_x(bottom, cpu_w);
        cpu::cpu_card(app, &mut t.cpu, ui, regs);
        output_card(app, t, ui, out);
    } else {
        cpu::cpu_card(app, &mut t.cpu, ui, bottom);
    }
}

pub(crate) fn memory_tab(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (top, map) = bottom_row(t, rect, 118.0);
    let side_w = (top.width() * 0.3).clamp(260.0, 330.0);
    let (hex, side) = split_x(top, top.width() - side_w - GAP);
    hex_editor_card(app, &mut t.mem, ui, hex);
    let inspector_h = HEAD_H + 6.0 * 36.0 + 16.0;
    let (insp, rest) = split_y(side, inspector_h.min(side.height() * 0.45));
    let seg_h = HEAD_H + 4.0 * 36.0 + 16.0;
    let (segs, stack) = split_y(rest, seg_h.min(rest.height() * 0.5));
    inspector_card(app, &mut t.mem, ui, insp);
    segments_card(app, &mut t.mem, ui, segs);
    stack_card(app, ui, stack);
    if let Some(map) = map {
        memory_map_card(app, &mut t.mem, ui, map);
    }
}
