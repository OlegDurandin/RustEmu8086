//! The execution units: what the ALU did last and the 8087 registers.

use egui::{FontId, RichText, Ui};
use rustemu_core::cpu::{AluOp, flags};

use crate::app::App;
use crate::debugger::{file_line, show_location, source_text};
use crate::inspect::bin;

/// The 8087: ST(0)..ST(7) (value, tag, 80-bit hex), the condition codes, TOP, the exceptions and
/// the control word. What changed during the last step / run is red.
pub(crate) fn fpu_ui(app: &mut App, ui: &mut Ui) {
    use rustemu_core::fpu::{Tag, sw, to_f64};
    let changed = egui::Color32::from_rgb(220, 40, 40);
    let Some(m) = app.session.x86() else { return };
    let f = &m.sys.fpu;
    // Without an 8086 checkpoint there is nothing to compare with: nothing is highlighted.
    let prev = app.exec.prev_x86().map_or(f, |p| &p.fpu);
    egui::Grid::new("fpu_regs").striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
        for h in ["", "value", "tag", "80-bit hex"] {
            ui.label(RichText::new(h).weak());
        }
        ui.end_row();
        for i in 0..8 {
            let (v, tag) = (f.st(i), f.st_tag(i));
            let hot = v != prev.st(i) || tag != prev.st_tag(i);
            let text = |s: String| {
                let t = RichText::new(s).monospace();
                if hot { t.color(changed) } else { t }
            };
            ui.label(text(format!("ST({i})")));
            let value = match tag {
                Tag::Empty => "empty".to_string(),
                _ => {
                    let x = to_f64(v);
                    if x.is_nan() { "NaN".into() } else { format!("{x}") }
                }
            };
            ui.label(text(value));
            ui.label(RichText::new(format!("{tag:?}").to_lowercase()).weak());
            let hex: String = v.iter().rev().map(|b| format!("{b:02X}")).collect();
            ui.label(text(format!("{} {}", &hex[..4], &hex[4..])));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    let bit = |w: u16, b: u16| (w & b != 0) as u8;
    let s = f.status;
    ui.horizontal(|ui| {
        ui.monospace(format!("TOP = {}", f.top()));
        ui.separator();
        let cc = format!("C3 C2 C1 C0 = {} {} {} {}", bit(s, sw::C3), bit(s, sw::C2), bit(s, sw::C1), bit(s, sw::C0));
        let cc_changed = (s ^ prev.status) & (sw::C0 | sw::C1 | sw::C2 | sw::C3) != 0;
        let t = RichText::new(cc).monospace();
        ui.label(if cc_changed { t.color(changed) } else { t }).on_hover_text(
            "Condition codes: FCOM / FTST / FXAM results (C3 = ZF, C2 = PF, C0 = CF after FSTSW + SAHF)",
        );
    });
    ui.horizontal(|ui| {
        ui.monospace(format!("status {s:04X}h"));
        for (b, name, what) in [
            (sw::IE, "IE", "invalid operation"),
            (sw::DE, "DE", "denormal operand"),
            (sw::ZE, "ZE", "zero divide"),
            (sw::OE, "OE", "overflow"),
            (sw::UE, "UE", "underflow"),
            (sw::PE, "PE", "precision (inexact)"),
        ] {
            let t = RichText::new(name).monospace();
            ui.label(if s & b != 0 { t.color(changed).strong() } else { t.weak() }).on_hover_text(what);
        }
    });
    let c = f.control;
    let precision = ["24 bits", "-", "53 bits", "64 bits"][((c >> 8) & 3) as usize];
    let rounding = ["to nearest", "down", "up", "toward zero"][((c >> 10) & 3) as usize];
    ui.monospace(format!("control {c:04X}h  precision {precision}, rounding {rounding}, masks {:02X}h", c & 0x3F));
    // The bytes as Emu8086's FPU window shows them: the environment (FSTENV) and ST(0)..ST(7) as in
    // memory (FSAVE), lowest byte first.
    egui::CollapsingHeader::new("raw bytes (as in Emu8086)").id_salt("fpu_raw").show(ui, |ui| {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ");
        ui.weak("control register:").on_hover_text("The environment as FSTENV stores it: control word, status word, tag word, instruction and operand addresses");
        ui.monospace(hex(&f.env_bytes()));
        ui.weak("FPU registers:").on_hover_text("ST(0)..ST(7), 10 bytes each, as FSAVE stores them");
        for i in 0..8 {
            ui.monospace(hex(&f.st(i)));
        }
    });
}

/// The last ALU operation: operands, result and flags in all bases.
pub fn alu_ui(app: &mut App, ui: &mut Ui) {
    if app.session.mips().is_some() {
        return crate::mips_ui::alu_ui(app, ui);
    }
    let Some(t) = app.session.x86().map(|m| m.cpu.alu_trace) else { return };
    if t.op == AluOp::None {
        ui.weak("No arithmetic or logic instruction executed yet.");
        return;
    }
    let addr = rustemu_core::linear(t.cs, t.ip);
    let ago = app.session.target.instr_count().saturating_sub(t.instr + 1);
    ui.horizontal_wrapped(|ui| {
        let w = if t.wide { "word" } else { "byte" };
        ui.label(RichText::new(format!("{} ({w})", t.op.name())).strong().monospace().size(15.0));
        ui.weak(format!("at {:04X}:{:04X}", t.cs, t.ip));
        if let Some(v) = app.session.view_at(addr, app.settings.show_macro_code) {
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
