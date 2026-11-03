//! Tools to inspect the machine: memory, stack, flags, variables, symbols, listing, log.

use egui::{Color32, FontId, RichText, Ui};
use rustemu_asm::SymbolKind;
use rustemu_core::cpu::{self, flags};
use rustemu_core::font::cp437_to_char;
use rustemu_core::linear;

use crate::app::App;
use crate::tools::Tool;

/// How the variables window shows values.
#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum NumFormat {
    /// Hexadecimal.
    #[default]
    Hex,
    /// Unsigned decimal.
    Dec,
    /// Signed decimal.
    Signed,
    /// Binary.
    Bin,
    /// Characters (CP437).
    Char,
}

/// Parses `seg:off` (hex) or a linear hex address; segment may be a register name (8086).
pub fn parse_addr(app: &App, s: &str) -> Option<u32> {
    let s = s.trim();
    let c = &app.session.x86()?.cpu;
    let hex = |t: &str| -> Option<u32> {
        let t = t.trim().trim_end_matches(['h', 'H']);
        let u = t.to_ascii_uppercase();
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

pub(crate) fn memory_ui(app: &mut App, ui: &mut Ui) {
    if app.session.mips().is_some() {
        return crate::mips_ui::memory_ui(app, ui);
    }
    ui.horizontal(|ui| {
        ui.label("address:");
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.view.mem_addr_text).desired_width(110.0).hint_text("DS:0000 or 07100"),
        );
        if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("go").clicked() {
            let text = app.view.mem_addr_text.clone();
            match parse_addr(app, &text) {
                Some(a) => app.view.mem_view_addr = a & !0xF,
                None => app.status = format!("bad address '{text}'"),
            }
        }
        for (label, seg) in [("DS", cpu::DS), ("ES", cpu::ES), ("SS:SP", cpu::SS), ("CS:IP", cpu::CS)] {
            if ui.small_button(label).clicked()
                && let Some(m) = app.session.x86()
            {
                let c = &m.cpu;
                let off = match seg {
                    cpu::SS => c.regs[cpu::SP],
                    cpu::CS => c.ip,
                    _ => 0,
                };
                app.view.mem_view_addr = linear(c.sregs[seg], off) & !0xF;
            }
        }
        if ui.small_button("B800").clicked() {
            app.view.mem_view_addr = 0xB8000;
        }
    });
    let font = FontId::monospace(12.5);
    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 1.0;
    let base = app.view.mem_view_addr & !0xF;
    let pc = app.session.target.pc();
    let sp = app.session.x86().map_or(u32::MAX, |m| linear(m.cpu.sregs[cpu::SS], m.cpu.regs[cpu::SP]));
    if base != app.view.mem_last_base {
        app.view.mem_last_base = base;
        app.view.mem_scroll_pending = true;
    }
    let mut sa = egui::ScrollArea::vertical().max_height(320.0);
    if app.view.mem_scroll_pending {
        let step = row_h + ui.spacing().item_spacing.y;
        sa = sa.vertical_scroll_offset((base >> 4) as f32 * step);
        app.view.mem_scroll_pending = false;
    }
    sa.show_rows(ui, row_h, 0x10000, |ui, range| {
        for row in range {
            let a = (row as u32) << 4;
            let mut job = egui::text::LayoutJob::default();
            let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: c, ..Default::default() };
            let txt = ui.visuals().text_color();
            let weak = ui.visuals().weak_text_color();
            job.append(&format!("{:05X}  ", a), 0.0, fmt(weak));
            let t = &*app.session.target;
            for i in 0..16 {
                let b = t.read8(a + i);
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
                    let b = t.read8(a + i);
                    if b < 32 { '.' } else { cp437_to_char(b) }
                })
                .collect();
            job.append(&format!(" {chars}"), 0.0, fmt(weak));
            ui.add(egui::Label::new(job).extend());
        }
    });
    ui.weak("blue = CS:IP, yellow = SS:SP");
}

pub(crate) fn stack_ui(app: &mut App, ui: &mut Ui) {
    let Some(m) = app.session.x86() else { return };
    let c = &m.cpu;
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
            let v = m.sys.mem.read16(linear(ss, off));
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
pub const FLAG_ROWS: [(u16, &str, &str); 8] = [
    (flags::CF, "CF", "Carry Flag"),
    (flags::ZF, "ZF", "Zero Flag (0-not zero, 1-zero)"),
    (flags::SF, "SF", "Sign Flag (0-positive, 1-negative)"),
    (flags::OF, "OF", "Overflow Flag"),
    (flags::PF, "PF", "Parity Flag (0-odd, 1-even)"),
    (flags::AF, "AF", "Auxiliary Flag"),
    (flags::IF, "IF", "Interrupt enable Flag (0-disabled, 1-enabled)"),
    (flags::DF, "DF", "Direction Flag (0-forward, 1-backward)"),
];

pub(crate) fn flags_ui(app: &mut App, ui: &mut Ui) {
    let Some(m) = app.session.x86_mut() else { return };
    let prev = app.exec.prev_x86().map_or(m.cpu.flags, |p| p.cpu.flags);
    let c = &mut m.cpu;
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
        app.windows[Tool::FlagAnalyser] = true;
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

pub(crate) fn flag_analyser_ui(app: &mut App, ui: &mut Ui) {
    let Some(f) = app.session.x86().map(|m| m.cpu.flags) else { return };
    let text = flag_analysis(f);
    ui.label(RichText::new(text.trim_end()).monospace().size(13.0));
}

/// `v` in binary, in groups of four digits.
pub(crate) fn bin(v: u32, bits: u32) -> String {
    let s = format!("{v:0w$b}", w = bits as usize);
    s.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(" ")
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

pub(crate) fn vars_ui(app: &mut App, ui: &mut Ui) {
    let Some(prog) = &app.session.program else {
        ui.weak("no program");
        return;
    };
    if app.session.image_base().is_none() {
        return;
    }
    ui.horizontal(|ui| {
        for (f, n) in [
            (NumFormat::Hex, "hex"),
            (NumFormat::Dec, "dec"),
            (NumFormat::Signed, "signed"),
            (NumFormat::Bin, "bin"),
            (NumFormat::Char, "ascii"),
        ] {
            ui.selectable_value(&mut app.settings.vars_format, f, n);
        }
    });
    let font = FontId::monospace(12.5);
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        egui::Grid::new("vars").num_columns(3).striped(true).spacing([10.0, 1.0]).show(ui, |ui| {
            for s in prog.symbols.iter().filter(|s| s.kind == SymbolKind::Variable && !s.is_macro_local()) {
                let size = s.size.unwrap_or(1).min(4);
                let Some(addr) = app.session.symbol_addr(s) else { continue };
                let t = &*app.session.target;
                let mut vals = Vec::new();
                for k in 0..4u32 {
                    let mut v = 0u32;
                    for i in 0..size as u32 {
                        v |= (t.read8(addr + k * size as u32 + i) as u32) << (i * 8);
                    }
                    vals.push(format_value(v, size, app.settings.vars_format));
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
                    ui.label(RichText::new(format_value(v, size, app.settings.vars_format)).font(font.clone()));
                    ui.end_row();
                }
            });
        }
    });
}

pub(crate) fn symbols_ui(app: &mut App, ui: &mut Ui) {
    let Some(prog) = &app.session.program else {
        ui.weak("no program");
        return;
    };
    egui::ScrollArea::both().max_height(360.0).show(ui, |ui| {
        ui.label(RichText::new(prog.symbols_text()).monospace());
    });
}

pub(crate) fn listing_ui(app: &mut App, ui: &mut Ui) {
    let Some(prog) = &app.session.program else {
        ui.weak("no program");
        return;
    };
    // The MIPS listing is made per load (`generation`), the 8086's when its lines change.
    let key = if app.session.mips().is_some() { app.program.generation as usize } else { prog.listing.len() };
    if app.view.listing_cache.as_ref().map(|(n, _)| *n) != Some(key) {
        let text = crate::mips_ui::listing_text(app).unwrap_or_else(|| prog.listing_text());
        app.view.listing_cache = Some((key, text));
    }
    let text = &app.view.listing_cache.as_ref().unwrap().1;
    egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
        ui.label(RichText::new(text).monospace().size(12.0));
    });
}

/// The end of the console output a window lays out each frame (the machine keeps about 1 MB).
pub const OUTPUT_SHOWN: usize = 64 * 1024;

/// The last [`OUTPUT_SHOWN`] bytes of `bytes`, and whether earlier ones are not shown (also those
/// the machine no longer keeps: `dropped`).
pub fn output_tail(bytes: &[u8], dropped: usize) -> (&[u8], bool) {
    let start = bytes.len().saturating_sub(OUTPUT_SHOWN);
    (&bytes[start..], start > 0 || dropped > 0)
}

pub(crate) fn log_ui(app: &mut App, ui: &mut Ui) {
    let t = &*app.session.target;
    let log: Vec<String> = app.session.x86().map(|m| m.log.lines().collect()).unwrap_or_default();
    ui.label(RichText::new("console output").strong());
    egui::ScrollArea::vertical().id_salt("out").max_height(160.0).stick_to_bottom(true).show(ui, |ui| {
        let (tail, cut) = output_tail(t.output(), t.output_dropped());
        if cut {
            ui.weak("(earlier output not shown)");
        }
        let text: String =
            tail.iter().filter(|&&b| b != b'\r').map(|&b| if b == b'\n' { '\n' } else { cp437_to_char(b) }).collect();
        ui.label(RichText::new(text).monospace());
    });
    ui.separator();
    ui.label(RichText::new("emulator messages").strong());
    if log.is_empty() {
        ui.weak("(none)");
        return;
    }
    // Only the rows in view are laid out (the log keeps up to 1000 messages).
    let row = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::vertical().id_salt("log").max_height(120.0).stick_to_bottom(true).show_rows(
        ui,
        row,
        log.len(),
        |ui, rows| {
            for l in &log[rows] {
                ui.label(RichText::new(l).monospace().color(crate::colors::warn(ui)));
            }
        },
    );
}

/// Near target offset of a disassembled `CALL`/`JMP`/`Jcc`/`LOOP`/`JCXZ` (`"CALL 0150h"` -> 0x150).
pub fn jump_target(text: &str) -> Option<u16> {
    let (mnem, arg) = text.split_once(' ')?;
    if !(mnem.starts_with('J') || mnem == "CALL" || mnem.starts_with("LOOP")) || arg.contains(['[', ':', ' ']) {
        return None;
    }
    u16::from_str_radix(arg.strip_suffix('h')?, 16).ok()
}

/// Finds the address to start the disassembly at so that the list stays stable while stepping.
pub fn disasm_start(app: &mut App, rows: usize) -> u32 {
    if app.view.code_view_pinned {
        return app.view.code_view_start;
    }
    let t = &*app.session.target;
    let pc = t.pc();
    let (base, last) = t.code_segment();
    let mut a = app.view.code_view_start;
    // Offsets wrap within the code segment ([`Target::code_segment`]).
    let mut ip = pc.wrapping_sub(base).wrapping_sub(pc.wrapping_sub(a)) & last;
    // Rows of the list, label rows included; CS:IP stays at least 3 rows above the bottom.
    let mut shown = 0;
    while shown + 3 < rows {
        shown += 1 + app.session.symbol_at(a).is_some() as usize;
        if a == pc {
            return app.view.code_view_start;
        }
        let len = app.session.code_row(a, ip, app.settings.disasm_data).len;
        a += len;
        ip = ip.wrapping_add(len) & last;
        if a > pc {
            break;
        }
    }
    app.view.code_view_start = pc;
    pc
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
