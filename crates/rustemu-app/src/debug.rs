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
    window("stack", &mut stack, [600.0, 120.0], ctx, |ui| stack_ui(app, ui));
    window("flags", &mut fl, [600.0, 300.0], ctx, |ui| flags_ui(app, ui));
    window("variables", &mut vars, [300.0, 300.0], ctx, |ui| vars_ui(app, ui));
    window("symbol table", &mut sym, [320.0, 160.0], ctx, |ui| symbols_ui(app, ui));
    window("listing", &mut lst, [200.0, 140.0], ctx, |ui| listing_ui(app, ui));
    window("log", &mut log, [120.0, 500.0], ctx, |ui| log_ui(app, ui));
    let w = &mut app.windows;
    (w.memory, w.stack, w.flags, w.vars, w.symbols, w.listing, w.log) = (mem, stack, fl, vars, sym, lst, log);
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
    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        egui::Grid::new("stack").num_columns(3).spacing([10.0, 1.0]).striped(true).show(ui, |ui| {
            for i in 0..32u16 {
                let off = sp.wrapping_add(i * 2);
                let v = app.session.machine.sys.mem.read16(linear(ss, off));
                let mut t = RichText::new(format!("{ss:04X}:{off:04X}")).font(font.clone());
                if i == 0 {
                    t = t.strong().color(crate::colors::warn(ui));
                }
                ui.label(t);
                ui.label(RichText::new(format!("{v:04X}")).font(font.clone()));
                let mut notes = Vec::new();
                if i == 0 {
                    notes.push("← SP");
                }
                if off == bp {
                    notes.push("← BP");
                }
                ui.label(RichText::new(notes.join(" ")).weak());
                ui.end_row();
                if off >= 0xFFFE {
                    break;
                }
            }
        });
    });
}

fn flags_ui(app: &mut App, ui: &mut Ui) {
    let c = &mut app.session.machine.cpu;
    let list = [
        (flags::CF, "CF", "carry"),
        (flags::ZF, "ZF", "zero"),
        (flags::SF, "SF", "sign"),
        (flags::OF, "OF", "overflow"),
        (flags::PF, "PF", "parity"),
        (flags::AF, "AF", "auxiliary carry"),
        (flags::IF, "IF", "interrupt enable"),
        (flags::DF, "DF", "direction"),
        (flags::TF, "TF", "trap"),
    ];
    egui::Grid::new("flags").num_columns(2).show(ui, |ui| {
        for (bit, name, desc) in list {
            let mut on = c.flag(bit);
            if ui.checkbox(&mut on, RichText::new(name).monospace().strong()).changed() {
                c.set_flag(bit, on);
            }
            ui.weak(desc);
            ui.end_row();
        }
    });
    ui.separator();
    ui.label(RichText::new(format!("FLAGS = {:04X}h = {:016b}b", c.flags, c.flags)).monospace());
    // Condition evaluation helper (like Emu8086's "flags" window analysis).
    let f = |b| c.flag(b);
    let conds = [
        ("JZ/JE", f(flags::ZF)),
        ("JC/JB", f(flags::CF)),
        ("JA", !f(flags::CF) && !f(flags::ZF)),
        ("JL", f(flags::SF) != f(flags::OF)),
        ("JG", !f(flags::ZF) && f(flags::SF) == f(flags::OF)),
        ("JS", f(flags::SF)),
        ("JO", f(flags::OF)),
        ("JP", f(flags::PF)),
    ];
    ui.horizontal_wrapped(|ui| {
        for (n, t) in conds {
            let txt = RichText::new(n).monospace();
            ui.label(if t { txt.color(crate::colors::ok(ui)) } else { txt.weak() });
        }
    });
    ui.weak("green = jump would be taken");
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
