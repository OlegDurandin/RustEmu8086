//! Debug windows: memory, stack, flags, variables, symbols, listing, log.

use egui::{Color32, FontId, RichText, Ui, pos2};
use rustemu_asm::SymbolKind;
use rustemu_core::cpu::{self, flags};
use rustemu_core::font::cp437_to_char;
use rustemu_core::linear;

use crate::app::App;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum NumFormat {
    #[default]
    Hex,
    Dec,
    Signed,
    Bin,
    Char,
}

fn window(title: &str, open: &mut bool, pos: [f32; 2], ctx: &egui::Context, f: impl FnOnce(&mut Ui)) {
    egui::Window::new(title).open(open).default_pos(pos2(pos[0], pos[1])).show(ctx, f);
}

pub fn debug_windows(app: &mut App, ctx: &egui::Context) {
    let w = &mut app.windows;
    let (mut mem, mut stack, mut fl, mut vars, mut sym, mut lst, mut log) =
        (w.memory, w.stack, w.flags, w.vars, w.symbols, w.listing, w.log);
    window("memory", &mut mem, [80.0, 420.0], ctx, |ui| memory_ui(app, ui));
    window("stack", &mut stack, [680.0, 110.0], ctx, |ui| stack_ui(app, ui));
    window("flags", &mut fl, [880.0, 110.0], ctx, |ui| flags_ui(app, ui));
    window("variables", &mut vars, [300.0, 300.0], ctx, |ui| vars_ui(app, ui));
    window("symbol table", &mut sym, [320.0, 160.0], ctx, |ui| symbols_ui(app, ui));
    window("listing", &mut lst, [200.0, 140.0], ctx, |ui| listing_ui(app, ui));
    window("log", &mut log, [120.0, 500.0], ctx, |ui| log_ui(app, ui));
    let w = &mut app.windows;
    (w.memory, w.stack, w.flags, w.vars, w.symbols, w.listing, w.log) = (mem, stack, fl, vars, sym, lst, log);
    let mut an = app.windows.flag_analyser;
    window("lexical flag analyser", &mut an, [1000.0, 110.0], ctx, |ui| flag_analyser_ui(app, ui));
    app.windows.flag_analyser = an;
}

/// Parses `seg:off` (hex) or a linear hex address; segment may be a register name.
fn parse_addr(app: &App, s: &str) -> Option<u32> {
    let s = s.trim();
    let hex = |t: &str| -> Option<u32> {
        let t = t.trim().trim_end_matches(['h', 'H']);
        let u = t.to_ascii_uppercase();
        let c = &app.session.machine.cpu;
        if let Some(i) = cpu::SREG_NAMES.iter().position(|r| *r == u) {
            return Some(c.sregs[i] as u32);
        }
        if let Some(i) = cpu::REG16_NAMES.iter().position(|r| *r == u) {
            return Some(c.regs[i] as u32);
        }
        if u == "IP" {
            return Some(c.ip as u32);
        }
        u32::from_str_radix(t, 16).ok()
    };
    match s.split_once(':') {
        Some((a, b)) => Some(linear(hex(a)? as u16, hex(b)? as u16)),
        None => hex(s).map(|v| v & 0xFFFFF),
    }
}

fn memory_ui(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("address:");
        let r = ui
            .add(egui::TextEdit::singleline(&mut app.mem_addr_text).desired_width(110.0).hint_text("DS:0000 or 07100"));
        if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("go").clicked() {
            let text = app.mem_addr_text.clone();
            match parse_addr(app, &text) {
                Some(a) => app.mem_view_addr = a & !0xF,
                None => app.status = format!("bad address '{text}'"),
            }
        }
        for (label, seg) in [("DS", cpu::DS), ("ES", cpu::ES), ("SS:SP", cpu::SS), ("CS:IP", cpu::CS)] {
            if ui.small_button(label).clicked() {
                let c = &app.session.machine.cpu;
                let off = match seg {
                    cpu::SS => c.regs[cpu::SP],
                    cpu::CS => c.ip,
                    _ => 0,
                };
                app.mem_view_addr = linear(c.sregs[seg], off) & !0xF;
            }
        }
        if ui.small_button("B800").clicked() {
            app.mem_view_addr = 0xB8000;
        }
    });
    let font = FontId::monospace(12.5);
    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 1.0;
    let base = app.mem_view_addr & !0xF;
    let pc = app.session.machine.cpu.pc();
    let sp = linear(app.session.machine.cpu.sregs[cpu::SS], app.session.machine.cpu.regs[cpu::SP]);
    if base != app.mem_last_base {
        app.mem_last_base = base;
        app.mem_scroll_pending = true;
    }
    let mut sa = egui::ScrollArea::vertical().max_height(320.0);
    if app.mem_scroll_pending {
        let step = row_h + ui.spacing().item_spacing.y;
        sa = sa.vertical_scroll_offset((base >> 4) as f32 * step);
        app.mem_scroll_pending = false;
    }
    sa.show_rows(ui, row_h, 0x10000, |ui, range| {
        for row in range {
            let a = (row as u32) << 4;
            let mut job = egui::text::LayoutJob::default();
            let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: c, ..Default::default() };
            let txt = ui.visuals().text_color();
            let weak = ui.visuals().weak_text_color();
            job.append(&format!("{:05X}  ", a), 0.0, fmt(weak));
            let mem = &app.session.machine.sys.mem;
            for i in 0..16 {
                let b = mem.read8(a + i);
                let col = if a + i == pc {
                    crate::colors::info(ui)
                } else if a + i == sp {
                    crate::colors::warn(ui)
                } else {
                    txt
                };
                job.append(&format!("{b:02X}{}", if i == 7 { "  " } else { " " }), 0.0, fmt(col));
            }
            let chars: String = (0..16)
                .map(|i| {
                    let b = mem.read8(a + i);
                    if b < 32 { '.' } else { cp437_to_char(b) }
                })
                .collect();
            job.append(&format!(" {chars}"), 0.0, fmt(weak));
            ui.add(egui::Label::new(job).extend());
        }
    });
    ui.weak("blue = CS:IP, yellow = SS:SP");
}

fn stack_ui(app: &mut App, ui: &mut Ui) {
    let c = &app.session.machine.cpu;
    let (ss, sp, bp) = (c.sregs[cpu::SS], c.regs[cpu::SP], c.regs[cpu::BP]);
    let font = FontId::monospace(12.5);
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    // Like Emu8086: from the bottom of the stack segment (SS:FFFE) up to the top, SP selected.
    let rows = 0x8000usize;
    let sp_row = (0xFFFEu16.wrapping_sub(sp) / 2) as usize;
    let id = ui.make_persistent_id("stack_last_sp");
    let last = ui.data(|d| d.get_temp::<(u16, u16)>(id));
    let mut sa = egui::ScrollArea::vertical().id_salt("stack_rows").max_height(420.0).auto_shrink([false, true]);
    if last != Some((ss, sp)) {
        ui.data_mut(|d| d.insert_temp(id, (ss, sp)));
        let step = row_h + ui.spacing().item_spacing.y;
        sa = sa.vertical_scroll_offset((sp_row as f32 * step - 200.0).max(0.0));
    }
    let sel_bg = ui.visuals().selection.bg_fill;
    let sel_fg = ui.visuals().selection.stroke.color;
    sa.show_rows(ui, row_h, rows, |ui, range| {
        for i in range {
            let off = 0xFFFEu16.wrapping_sub(i as u16 * 2);
            let v = app.session.machine.sys.mem.read16(linear(ss, off));
            let mut text = format!("{ss:04X}:{off:04X}  {v:04X}");
            if i == sp_row {
                text.push_str(" <");
            }
            if off == bp && bp != sp {
                text.push_str("   BP");
            }
            let t = RichText::new(text).font(font.clone());
            ui.label(if i == sp_row { t.background_color(sel_bg).color(sel_fg) } else { t });
        }
    });
    ui.weak("SS:SP is selected (<); the stack grows towards smaller offsets");
}

/// The flags of the flags window, in Emu8086's order, with their descriptions.
const FLAG_ROWS: [(u16, &str, &str); 8] = [
    (flags::CF, "CF", "Carry Flag"),
    (flags::ZF, "ZF", "Zero Flag (0-not zero, 1-zero)"),
    (flags::SF, "SF", "Sign Flag (0-positive, 1-negative)"),
    (flags::OF, "OF", "Overflow Flag"),
    (flags::PF, "PF", "Parity Flag (0-odd, 1-even)"),
    (flags::AF, "AF", "Auxiliary Flag"),
    (flags::IF, "IF", "Interrupt enable Flag (0-disabled, 1-enabled)"),
    (flags::DF, "DF", "Direction Flag (0-forward, 1-backward)"),
];

fn flags_ui(app: &mut App, ui: &mut Ui) {
    let prev = app.prev_cpu.flags;
    let c = &mut app.session.machine.cpu;
    let font = FontId::monospace(14.0);
    egui::Grid::new("flags").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
        for (bit, name, desc) in FLAG_ROWS {
            ui.label(RichText::new(name).monospace()).on_hover_text(desc);
            let on = c.flag(bit);
            let color = if on { Color32::from_rgb(220, 40, 40) } else { Color32::from_rgb(30, 80, 220) };
            let mut t = RichText::new(if on { "1" } else { "0" }).font(font.clone()).color(color).strong();
            if (c.flags ^ prev) & bit != 0 {
                t = t.underline();
            }
            if ui.add(egui::Button::new(t).min_size(egui::vec2(36.0, 0.0))).on_hover_text(desc).clicked() {
                c.set_flag(bit, !on);
            }
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    if ui.add(egui::Button::new("analyse").min_size(egui::vec2(120.0, 0.0))).clicked() {
        app.windows.flag_analyser = true;
    }
}

/// Text of the "lexical flag analyser" (which conditions the flags satisfy), as in Emu8086.
pub fn flag_analysis(f: u16) -> String {
    let on = |b: u16| f & b != 0;
    let (cf, zf, sf, of) = (on(flags::CF), on(flags::ZF), on(flags::SF), on(flags::OF));
    let line = |text: &str, cond: &str| format!("{text:<16}({cond})\n");
    let mut s = String::from("=== unsigned ===\n");
    s += &if cf { line("below", "CF=1") } else { line("not below", "CF=0") };
    s += &if !cf && !zf { line("    above", "CF=0 and ZF=0") } else { line("    not above", "CF=1 or ZF=1") };
    s += "\n=== signed ===\n";
    s += &if sf == of { line("not less", "SF=OF") } else { line("less", "SF<>OF") };
    s += &if sf == of && !zf {
        line("    greater", "SF=OF and ZF=0")
    } else {
        line("    not greater", "SF<>OF or ZF=1")
    };
    s += "\n";
    s += &if zf { line("equal", "ZF=1") } else { line("not equal", "ZF=0") };
    s += &if sf { line("negative", "SF=1") } else { line("positive", "SF=0") };
    s += &if of { line("overflow", "OF=1") } else { line("no overflow", "OF=0") };
    s += &if on(flags::PF) { line("parity even", "PF=1") } else { line("parity odd", "PF=0") };
    s += &if on(flags::AF) { line("aux carry", "AF=1") } else { line("not aux carry", "AF=0") };
    s += &if on(flags::IF) { line("hw-int enabled", "IF=1") } else { line("hw-int disabled", "IF=0") };
    s += &if on(flags::DF) { line("backward dir", "DF=1") } else { line("forward dir", "DF=0") };
    s
}

fn flag_analyser_ui(app: &mut App, ui: &mut Ui) {
    let text = flag_analysis(app.session.machine.cpu.flags);
    ui.label(RichText::new(text.trim_end()).monospace().size(13.0));
}

fn format_value(v: u32, size: u8, f: NumFormat) -> String {
    let bits = size as u32 * 8;
    match f {
        NumFormat::Hex => format!("{:0w$X}h", v, w = size as usize * 2),
        NumFormat::Dec => format!("{v}"),
        NumFormat::Signed => {
            let sv = ((v as i64) << (64 - bits)) >> (64 - bits);
            format!("{sv}")
        }
        NumFormat::Bin => format!("{:0w$b}b", v, w = bits as usize),
        NumFormat::Char => (0..size)
            .map(|i| {
                let b = (v >> (i * 8)) as u8;
                if b < 32 { '.' } else { cp437_to_char(b) }
            })
            .collect(),
    }
}

fn vars_ui(app: &mut App, ui: &mut Ui) {
    let Some(prog) = &app.session.program else {
        ui.weak("no program");
        return;
    };
    let Some(base) = app.session.image_base() else { return };
    ui.horizontal(|ui| {
        for (f, n) in [
            (NumFormat::Hex, "hex"),
            (NumFormat::Dec, "dec"),
            (NumFormat::Signed, "signed"),
            (NumFormat::Bin, "bin"),
            (NumFormat::Char, "ascii"),
        ] {
            ui.selectable_value(&mut app.vars_format, f, n);
        }
    });
    let load_seg = app.session.machine.loaded.as_ref().map(|l| l.load_segment).unwrap_or(0);
    let is_exe = app.session.machine.loaded.as_ref().is_some_and(|l| l.kind == rustemu_core::LoadKind::Exe);
    let font = FontId::monospace(12.5);
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        egui::Grid::new("vars").num_columns(3).striped(true).spacing([10.0, 1.0]).show(ui, |ui| {
            for s in prog.symbols.iter().filter(|s| s.kind == SymbolKind::Variable && !s.is_macro_local()) {
                let size = s.size.unwrap_or(1).min(4);
                // Physical address of the variable.
                let addr = if is_exe {
                    linear(load_seg.wrapping_add(s.segment.unwrap_or(0)), s.value as u16)
                } else {
                    base.wrapping_add((s.value as u32).wrapping_sub(prog.origin as u32))
                };
                let mem = &app.session.machine.sys.mem;
                let mut vals = Vec::new();
                for k in 0..4u32 {
                    let mut v = 0u32;
                    for i in 0..size as u32 {
                        v |= (mem.read8(addr + k * size as u32 + i) as u32) << (i * 8);
                    }
                    vals.push(format_value(v, size, app.vars_format));
                }
                ui.label(RichText::new(&s.name).font(font.clone()).strong());
                ui.label(RichText::new(["", "b", "w", "", "d"][size as usize]).weak());
                ui.label(RichText::new(vals.join(", ") + ", …").font(font.clone()));
                ui.end_row();
            }
        });
        let consts: Vec<_> =
            prog.symbols.iter().filter(|s| s.kind == SymbolKind::Constant && !s.is_macro_local()).collect();
        if !consts.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("constants").weak());
            egui::Grid::new("consts").num_columns(3).striped(true).spacing([10.0, 1.0]).show(ui, |ui| {
                for s in consts {
                    ui.label(RichText::new(&s.name).font(font.clone()).strong());
                    ui.label(RichText::new("equ").weak());
                    let v = s.value as u32;
                    let size = if s.value > 0xFFFF || s.value < -0x8000 { 4 } else { 2 };
                    ui.label(RichText::new(format_value(v, size, app.vars_format)).font(font.clone()));
                    ui.end_row();
                }
            });
        }
    });
}

fn symbols_ui(app: &mut App, ui: &mut Ui) {
    let Some(prog) = &app.session.program else {
        ui.weak("no program");
        return;
    };
    egui::ScrollArea::both().max_height(360.0).show(ui, |ui| {
        ui.label(RichText::new(prog.symbols_text()).monospace());
    });
}

fn listing_ui(app: &mut App, ui: &mut Ui) {
    let Some(prog) = &app.session.program else {
        ui.weak("no program");
        return;
    };
    if app.listing_cache.as_ref().map(|(n, _)| *n) != Some(prog.listing.len()) {
        app.listing_cache = Some((prog.listing.len(), prog.listing_text()));
    }
    let text = &app.listing_cache.as_ref().unwrap().1;
    egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
        ui.label(RichText::new(text).monospace().size(12.0));
    });
}

fn log_ui(app: &mut App, ui: &mut Ui) {
    let m = &app.session.machine;
    ui.label(RichText::new("console output").strong());
    egui::ScrollArea::vertical().id_salt("out").max_height(160.0).stick_to_bottom(true).show(ui, |ui| {
        let text: String = m
            .output
            .iter()
            .filter(|&&b| b != b'\r')
            .map(|&b| if b == b'\n' { '\n' } else { cp437_to_char(b) })
            .collect();
        ui.label(RichText::new(text).monospace());
    });
    ui.separator();
    ui.label(RichText::new("emulator messages").strong());
    egui::ScrollArea::vertical().id_salt("log").max_height(120.0).stick_to_bottom(true).show(ui, |ui| {
        if m.log.is_empty() {
            ui.weak("(none)");
        }
        for l in &m.log {
            ui.label(RichText::new(l).monospace().color(crate::colors::warn(ui)));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_flag_analysis_like_emu8086() {
        // Only IF set (state right after loading a program).
        let text = flag_analysis(0xF202);
        let expected = "=== unsigned ===
not below       (CF=0)
    above       (CF=0 and ZF=0)

=== signed ===
not less        (SF=OF)
    greater     (SF=OF and ZF=0)

not equal       (ZF=0)
positive        (SF=0)
no overflow     (OF=0)
parity odd      (PF=0)
not aux carry   (AF=0)
hw-int enabled  (IF=1)
forward dir     (DF=0)
";
        assert_eq!(text, expected);
        let t = flag_analysis(flags::CF | flags::ZF | flags::SF);
        assert!(
            t.contains("below           (CF=1)") && t.contains("not above") && t.contains("less            (SF<>OF)")
        );
        assert!(t.contains("equal           (ZF=1)") && t.contains("hw-int disabled"));
    }
}
