//! What the last executed instruction did (the tablet debugger's "visual step"): the instruction,
//! its source line, the registers, flags and memory it changed, and why the ALU set the flags.
//! Everything comes from the step-back history: the CPU before the step and the bytes it wrote.

use rustemu_core::cpu::{self, AluOp, AluTrace, Cpu, flags};
use rustemu_core::disasm::disassemble;
use rustemu_core::linear;

use rustemu_core::machine::ROM_SEG;

use crate::{Session, View};

/// The general registers AX..DX: their names and the names of their bytes.
pub const PAIRS: [(usize, &str, &str, &str); 4] = [
    (cpu::AX, "AX", "AH", "AL"),
    (cpu::BX, "BX", "BH", "BL"),
    (cpu::CX, "CX", "CH", "CL"),
    (cpu::DX, "DX", "DH", "DL"),
];

/// The other word registers, as the CPU panel shows them.
pub const WORDS: [(usize, &str); 4] = [(cpu::SP, "SP"), (cpu::BP, "BP"), (cpu::SI, "SI"), (cpu::DI, "DI")];

/// The segment registers in the order the CPU panel shows them.
pub const SEGMENTS: [(usize, &str); 4] = [(cpu::CS, "CS"), (cpu::DS, "DS"), (cpu::ES, "ES"), (cpu::SS, "SS")];

/// The nine flags of the 8086 with what they mean.
pub const FLAGS: [(u16, &str, &str); 9] = [
    (flags::CF, "CF", "Carry: unsigned overflow, the bit shifted out, a borrow"),
    (flags::PF, "PF", "Parity: the low byte of the result has an even number of 1 bits"),
    (flags::AF, "AF", "Auxiliary carry: carry out of bit 3 (for BCD arithmetic)"),
    (flags::ZF, "ZF", "Zero: the result is zero"),
    (flags::SF, "SF", "Sign: the top bit of the result (negative as a signed number)"),
    (flags::TF, "TF", "Trap: an interrupt after every instruction (single step)"),
    (flags::IF, "IF", "Interrupt: hardware interrupts are enabled"),
    (flags::DF, "DF", "Direction: string instructions go down (STD) instead of up (CLD)"),
    (flags::OF, "OF", "Overflow: the signed result does not fit"),
];

/// The byte of a general register that changed when the other did not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Half {
    /// AH, BH, CH, DH.
    High,
    /// AL, BL, CL, DL.
    Low,
}

/// A register the step changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegChange {
    /// The word register (`BX`).
    pub name: &'static str,
    /// Its value before the step.
    pub before: u16,
    /// Its value after the step.
    pub after: u16,
    /// Only one byte of AX..DX changed (`SUB BL, 1` changes BL, not BH).
    pub only: Option<Half>,
    /// The name of that byte (`BL`).
    pub byte_name: Option<&'static str>,
}

impl RegChange {
    /// The changed byte: (name, before, after).
    pub fn byte(&self) -> Option<(&'static str, u8, u8)> {
        let shift = if self.only? == Half::High { 8 } else { 0 };
        Some((self.byte_name?, (self.before >> shift) as u8, (self.after >> shift) as u8))
    }
}

/// A flag the step changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagChange {
    /// Its name (`CF`).
    pub name: &'static str,
    /// Before the step.
    pub before: bool,
    /// After the step.
    pub after: bool,
}

/// The last executed instruction and what it changed.
#[derive(Clone, Debug)]
pub struct StepView {
    /// CS of the instruction.
    pub cs: u16,
    /// IP of the instruction.
    pub ip: u16,
    /// Its bytes.
    pub bytes: Vec<u8>,
    /// The instruction (`SUB BL, 01h`).
    pub text: String,
    /// Its source line.
    pub source: Option<View>,
    /// The registers it changed (IP excluded).
    pub regs: Vec<RegChange>,
    /// The flags it changed.
    pub flags: Vec<FlagChange>,
    /// Bytes it wrote: (physical address, before, after), each address once.
    pub writes: Vec<(u32, u8, u8)>,
    /// What the ALU did, when this instruction used it.
    pub alu: Option<AluTrace>,
}

impl StepView {
    /// Builds the view from the CPU before and after the step and the bytes written.
    pub fn new(before: &Cpu, after: &Cpu, undo: &[(u32, u8)], mem: &rustemu_core::bus::Memory) -> StepView {
        let pc = before.pc();
        let bytes = mem.slice(pc, 8);
        let ins = disassemble(bytes, before.ip);
        let mut regs = Vec::new();
        for (i, name, h, l) in PAIRS {
            let (b, a) = (before.regs[i], after.regs[i]);
            let d = b ^ a;
            if d == 0 {
                continue;
            }
            let (only, byte_name) = match (d & 0xFF00 != 0, d & 0x00FF != 0) {
                (true, false) => (Some(Half::High), Some(h)),
                (false, true) => (Some(Half::Low), Some(l)),
                _ => (None, None),
            };
            regs.push(RegChange { name, before: b, after: a, only, byte_name });
        }
        let words = WORDS.iter().map(|&(i, n)| (n, before.regs[i], after.regs[i]));
        let segs = SEGMENTS.iter().map(|&(i, n)| (n, before.sregs[i], after.sregs[i]));
        for (name, b, a) in words.chain(segs) {
            if b != a {
                regs.push(RegChange { name, before: b, after: a, only: None, byte_name: None });
            }
        }
        let flags = FLAGS
            .iter()
            .filter(|(bit, ..)| (before.flags ^ after.flags) & bit != 0)
            .map(|&(bit, name, _)| FlagChange { name, before: before.flags & bit != 0, after: after.flags & bit != 0 })
            .collect();
        let mut writes: Vec<(u32, u8, u8)> = Vec::new();
        for &(addr, old) in undo {
            if !writes.iter().any(|w| w.0 == addr) {
                writes.push((addr, old, mem.read8(addr)));
            }
        }
        writes.retain(|w| w.1 != w.2);
        writes.sort_unstable_by_key(|w| w.0);
        let alu = (after.alu_trace.op != AluOp::None && after.alu_trace.instr == before.instr_count)
            .then_some(after.alu_trace);
        StepView {
            cs: before.cs(),
            ip: before.ip,
            bytes: bytes[..ins.len.min(8)].to_vec(),
            text: ins.text,
            source: None,
            regs,
            flags,
            writes,
            alu,
        }
    }

    /// Why the flags are what they are, in a sentence or two (empty when the instruction did not
    /// use the ALU and changed no flag).
    pub fn why(&self) -> Vec<String> {
        let Some(t) = self.alu else {
            return if self.flags.is_empty() {
                vec!["This instruction does not compute anything: the flags keep their values.".into()]
            } else {
                vec![format!(
                    "The instruction sets {} directly.",
                    self.flags.iter().map(|f| f.name).collect::<Vec<_>>().join(", ")
                )]
            };
        };
        explain(&t)
    }
}

/// A value as the assembler writes it: `0Eh`, `000Fh`.
fn hex(v: u32, bits: u32) -> String {
    let digits = (bits / 4) as usize;
    let s = format!("{:0digits$X}h", v & ((1u64 << bits) - 1) as u32);
    if s.starts_with(|c: char| c.is_ascii_alphabetic()) { format!("0{s}") } else { s }
}

/// The ALU operation as a formula and the reasons of its flags.
pub fn explain(t: &AluTrace) -> Vec<String> {
    let bits = if t.wide { 16 } else { 8 };
    let top = 1u32 << (bits - 1);
    let (a, b, r) = (hex(t.a, bits), hex(t.b as u32, bits), hex(t.result, bits));
    let flag = |f: u16| t.flags_out & f != 0;
    let bit = |f: u16| if flag(f) { 1 } else { 0 };
    let zero = format!("ZF = {} ({})", bit(flags::ZF), if flag(flags::ZF) { "result is 0" } else { "result is not 0" });
    let sign = format!("SF = {} (bit {} of the result is {})", bit(flags::SF), bits - 1, (t.result & top != 0) as u8);
    let mut out = Vec::new();
    let mut reasons = Vec::new();
    match t.op {
        AluOp::Add | AluOp::Adc | AluOp::Inc => {
            let carry_in = if t.op == AluOp::Adc && t.flags_in & flags::CF != 0 { " + CF" } else { "" };
            let b = if t.op == AluOp::Inc { hex(1, bits) } else { b };
            out.push(format!("{} {a} + {b}{carry_in} = {r}", t.op.name()));
            reasons.push(zero);
            if t.op != AluOp::Inc {
                reasons.push(format!(
                    "CF = {} ({})",
                    bit(flags::CF),
                    if flag(flags::CF) { "the sum does not fit: carry out" } else { "no carry out" }
                ));
            } else {
                reasons.push("CF unchanged (INC does not touch it)".into());
            }
            reasons.push(sign);
            reasons.push(format!(
                "OF = {} ({})",
                bit(flags::OF),
                if flag(flags::OF) { "signed overflow" } else { "no signed overflow" }
            ));
        }
        AluOp::Sub | AluOp::Sbb | AluOp::Cmp | AluOp::Dec | AluOp::Neg => {
            let (x, y) = match t.op {
                AluOp::Dec => (a, hex(1, bits)),
                AluOp::Neg => (hex(0, bits), a),
                _ => (a, b),
            };
            let borrow_in = if t.op == AluOp::Sbb && t.flags_in & flags::CF != 0 { " − CF" } else { "" };
            let what = if t.op == AluOp::Cmp { " (CMP keeps only the flags)" } else { "" };
            out.push(format!("{} {x} − {y}{borrow_in} = {r}{what}", t.op.name()));
            reasons.push(zero);
            if t.op == AluOp::Dec {
                reasons.push("CF unchanged (DEC does not touch it)".into());
            } else {
                reasons.push(format!(
                    "CF = {} ({})",
                    bit(flags::CF),
                    if flag(flags::CF) { "a borrow: the subtrahend is larger" } else { "no borrow" }
                ));
            }
            reasons.push(sign);
            reasons.push(format!(
                "OF = {} ({})",
                bit(flags::OF),
                if flag(flags::OF) { "signed overflow" } else { "no signed overflow" }
            ));
        }
        AluOp::And | AluOp::Or | AluOp::Xor | AluOp::Test => {
            let op = match t.op {
                AluOp::Or => "OR",
                AluOp::Xor => "XOR",
                _ => "AND",
            };
            let what = if t.op == AluOp::Test { " (TEST keeps only the flags)" } else { "" };
            out.push(format!("{a} {op} {b} = {r}{what}"));
            reasons.push(zero);
            reasons.push(sign);
            reasons.push("CF = 0 and OF = 0 (logic instructions clear them)".into());
        }
        AluOp::Not => {
            out.push(format!("NOT {a} = {r} (every bit inverted)"));
            reasons.push("no flags change".into());
        }
        AluOp::Rol | AluOp::Ror | AluOp::Rcl | AluOp::Rcr | AluOp::Shl | AluOp::Shr | AluOp::Sar | AluOp::Setmo => {
            out.push(format!("{} {a} by {} = {r}", t.op.name(), t.b));
            if t.b != 0 {
                reasons.push(format!("CF = {} (the last bit shifted out)", bit(flags::CF)));
            }
            if matches!(t.op, AluOp::Shl | AluOp::Shr | AluOp::Sar) && t.b != 0 {
                reasons.push(zero);
                reasons.push(sign);
            }
        }
        AluOp::Mul | AluOp::Imul => {
            let (src, dst) = if t.wide { ("AX", "DX:AX") } else { ("AL", "AX") };
            out.push(format!("{} {src} {a} × {b} = {dst} {}", t.op.name(), hex(t.result, bits * 2)));
            reasons.push(format!(
                "CF = OF = {} ({})",
                bit(flags::CF),
                if flag(flags::CF) {
                    "the upper half holds part of the product"
                } else {
                    "the product fits the lower half"
                }
            ));
        }
        AluOp::Div | AluOp::Idiv => {
            let (src, q, rem) = if t.wide { ("DX:AX", "AX", "DX") } else { ("AX", "AL", "AH") };
            out.push(format!(
                "{} {src} {} ÷ {b} = {q} {} remainder {rem} {}",
                t.op.name(),
                hex(t.a, bits * 2),
                hex(t.result & 0xFFFF, bits),
                hex(t.result >> 16, bits)
            ));
            reasons.push("the flags are undefined after a division".into());
        }
        AluOp::None => {}
    }
    if !reasons.is_empty() {
        out.push(reasons.join(" · "));
    }
    out
}

impl Session {
    /// The last executed instruction and its changes; `None` when nothing has been executed since
    /// the load (or the history was cleared).
    ///
    /// A step that began in the BIOS (it finished an `INT` that waited for a key) is shown as the
    /// program's `INT` that called it.
    pub fn last_step(&self, into_macros: bool) -> Option<StepView> {
        let m = self.x86()?;
        let (before, undo) = m.last_step()?;
        let mut v = StepView::new(before, &m.cpu, undo, &m.sys.mem);
        if v.cs == ROM_SEG {
            let (ss, sp) = (before.sregs[cpu::SS], before.regs[cpu::SP]);
            let ip = m.sys.mem.read16(linear(ss, sp)).wrapping_sub(2);
            let cs = m.sys.mem.read16(linear(ss, sp.wrapping_add(2)));
            let a = linear(cs, ip);
            let ins = disassemble(m.sys.mem.slice(a, 8), ip);
            v.bytes = m.sys.mem.slice(a, ins.len.min(8)).to_vec();
            (v.cs, v.ip, v.text) = (cs, ip, ins.text);
        }
        v.source = self.view_at(linear(v.cs, v.ip), into_macros);
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, steps: usize) -> (Session, StepView) {
        let p = rustemu_asm::assemble_str(src).unwrap();
        let mut s = Session::new();
        s.load_program(p).unwrap();
        for _ in 0..steps {
            s.target.step();
        }
        let v = s.last_step(true).unwrap();
        (s, v)
    }

    #[test]
    fn sub_bl_changes_only_bl() {
        let (_, v) = run("org 100h\nmov bx, 000Fh\nsub bl, 1\nret\n", 2);
        assert_eq!(v.text, "SUB BL, 01h");
        assert_eq!(v.source.as_ref().map(|s| s.line), Some(3));
        assert_eq!(v.regs.len(), 1);
        let r = &v.regs[0];
        assert_eq!((r.name, r.before, r.after), ("BX", 0x000F, 0x000E));
        assert_eq!(r.byte(), Some(("BL", 0x0F, 0x0E)));
        // Flags before were those of the load (all clear): 0Fh - 1 sets none, PF stays 0 (0Eh has
        // three 1 bits).
        assert!(v.flags.is_empty(), "{:?}", v.flags);
        let why = v.why();
        assert_eq!(why[0], "SUB 0Fh − 01h = 0Eh");
        assert!(why[1].contains("ZF = 0 (result is not 0)") && why[1].contains("CF = 0 (no borrow)"), "{why:?}");
    }

    #[test]
    fn mov_ax_changes_both_bytes() {
        let (_, v) = run("org 100h\nmov ax, 0BEEFh\nret\n", 1);
        let r = &v.regs[0];
        assert_eq!((r.name, r.after, r.only), ("AX", 0xBEEF, None));
        assert_eq!(v.why()[0], "This instruction does not compute anything: the flags keep their values.");
    }

    #[test]
    fn mov_ah_changes_only_ah_and_memory_writes_are_listed() {
        let (_, v) = run("org 100h\nmov ax, 1234h\nmov ah, 0FFh\nret\n", 2);
        assert_eq!(v.regs[0].byte(), Some(("AH", 0x12, 0xFF)));
        let (_, v) = run("org 100h\nmov word ptr [200h], 1234h\nret\n", 1);
        assert_eq!(v.writes, [(0x7200, 0, 0x34), (0x7201, 0, 0x12)]);
    }

    #[test]
    fn flags_and_their_reasons() {
        let (_, v) = run("org 100h\nmov al, 1\ncmp al, 2\nret\n", 2);
        let names: Vec<&str> = v.flags.iter().map(|f| f.name).collect();
        assert_eq!(names, ["CF", "PF", "AF", "SF"]);
        let why = v.why();
        assert!(why[0].starts_with("CMP 01h − 02h = 0FFh"), "{why:?}");
        assert!(why[1].contains("CF = 1 (a borrow"), "{why:?}");
    }
}
