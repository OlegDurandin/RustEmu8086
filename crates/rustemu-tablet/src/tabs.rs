//! The Editor / Debugger / Memory tabs: cards laid out for the window size.

use egui::{Rect, Ui};
use rustemu_ui::app::App;

use crate::TabletState;
use crate::cards::{
    breakpoints_card, call_stack_card, disasm_card, editor_card, flags_card, output_card, registers_card, trace_card,
    watch_card,
};
use crate::memory::{hex_editor_card, inspector_card, memory_card, memory_map_card, segments_card, stack_card};
use crate::widgets::{GAP, HEAD_H, split_x, split_y};

/// Height of the registers card.
const REGS_H: f32 = HEAD_H + 7.0 * 34.0 + 22.0;

/// Width of the registers + flags cards.
/// Width of the left column (editor, disassembly) beside the registers; None when the window is
/// too narrow for two columns (iPadOS windows, Split View) and the cards are stacked.
fn left_width(w: f32) -> Option<f32> {
    let right = (w * 0.42).clamp(400.0, 560.0);
    (w - right - GAP >= 380.0).then_some(w - right - GAP)
}

fn regs_and_flags(app: &mut App, ui: &mut Ui, rect: Rect) {
    let flags_w = 120.0;
    let (r, f) = split_x(rect, rect.width() - flags_w - GAP);
    registers_card(app, ui, r);
    flags_card(app, ui, f);
}

/// Splits off the bottom row of a tab, which gives way to the on-screen keyboard (its screen shows
/// the output then).
fn bottom_row(t: &TabletState, rect: Rect, h: f32) -> (Rect, Option<Rect>) {
    if t.keypad.open {
        return (rect, None);
    }
    let (top, bottom) = split_y(rect, rect.height() - h - GAP);
    (top, Some(bottom))
}

pub(crate) fn editor_tab(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (top, bottom) = bottom_row(t, rect, (rect.height() * 0.3).clamp(200.0, 300.0));
    let Some(left_w) = left_width(top.width()) else {
        // Narrow window: editor, registers and output (the disassembly is on the Debugger tab).
        let (editor, regs) = split_y(top, (top.height() - REGS_H - GAP).max(150.0));
        editor_card(app, ui, editor);
        regs_and_flags(app, ui, regs);
        if let Some(bottom) = bottom {
            output_card(app, t, ui, bottom);
        }
        return;
    };
    let (editor, right) = split_x(top, left_w);
    editor_card(app, ui, editor);
    let (regs, code) = split_y(right, REGS_H.min(right.height() * 0.6));
    regs_and_flags(app, ui, regs);
    disasm_card(app, ui, code, false);
    if let Some(bottom) = bottom {
        let (mem, out) = split_x(bottom, left_w);
        memory_card(app, &mut t.mem, ui, mem);
        output_card(app, t, ui, out);
    }
}

pub(crate) fn debugger_tab(app: &mut App, t: &mut TabletState, ui: &mut Ui, rect: Rect) {
    let (top, bottom) = bottom_row(t, rect, (rect.height() * 0.32).clamp(220.0, 320.0));
    match left_width(top.width()) {
        Some(w) => {
            let (code, right) = split_x(top, w);
            disasm_card(app, ui, code, true);
            let (regs, calls) = split_y(right, REGS_H.min(right.height() * 0.6));
            regs_and_flags(app, ui, regs);
            call_stack_card(app, ui, calls);
        }
        None => {
            // Narrow window: no call stack.
            let (code, regs) = split_y(top, (top.height() - REGS_H - GAP).max(150.0));
            disasm_card(app, ui, code, true);
            regs_and_flags(app, ui, regs);
        }
    }
    let Some(bottom) = bottom else { return };
    if bottom.width() < 900.0 {
        // Narrow window: no room for the trace.
        let (bps, watch) = split_x(bottom, (bottom.width() - GAP) * 0.5);
        breakpoints_card(app, t, ui, bps);
        watch_card(app, ui, watch);
        return;
    }
    let w = bottom.width() - 2.0 * GAP;
    let (bps, rest) = split_x(bottom, w * 0.32);
    let (watch, trace) = split_x(rest, w * 0.28);
    breakpoints_card(app, t, ui, bps);
    watch_card(app, ui, watch);
    trace_card(app, ui, trace);
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
