//! 8086 disassembler producing Intel/MASM-style text (`MOV AX, [BX+SI+04h]`).

use crate::cpu::{REG8_NAMES, REG16_NAMES, SREG_NAMES};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Length in bytes, including prefixes.
    pub len: usize,
    /// Mnemonic with operands, e.g. `MOV AX, 0005h`.
    pub text: String,
}

/// Formats a number in Emu8086 style: `0Ah`, `1234h`, decimal for small values is not used.
pub fn hex8(v: u8) -> String {
    let s = format!("{v:02X}h");
    if s.starts_with(|c: char| c.is_ascii_alphabetic()) { format!("0{s}") } else { s }
}

pub fn hex16(v: u16) -> String {
    let s = format!("{v:04X}h");
    if s.starts_with(|c: char| c.is_ascii_alphabetic()) { format!("0{s}") } else { s }
}

const ALU: [&str; 8] = ["ADD", "OR", "ADC", "SBB", "AND", "SUB", "XOR", "CMP"];
const SHIFTS: [&str; 8] = ["ROL", "ROR", "RCL", "RCR", "SHL", "SHR", "SHL", "SAR"];
const JCC: [&str; 16] =
    ["JO", "JNO", "JB", "JNB", "JZ", "JNZ", "JBE", "JA", "JS", "JNS", "JP", "JNP", "JL", "JGE", "JLE", "JG"];
const EA_BASE: [&str; 8] = ["BX+SI", "BX+DI", "BP+SI", "BP+DI", "SI", "DI", "BP", "BX"];

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> u8 {
        let v = self.bytes.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        let lo = self.u8() as u16;
        lo | ((self.u8() as u16) << 8)
    }
}

struct ModRm {
    md: u8,
    reg: u8,
    rm: u8,
    mem: String,
}

impl ModRm {
    fn operand(&self, w: bool) -> String {
        if self.md == 3 { reg(self.rm, w).to_string() } else { self.mem.clone() }
    }
    /// Operand with explicit size when it's ambiguous (memory without register operand).
    fn sized(&self, w: bool) -> String {
        if self.md == 3 {
            reg(self.rm, w).to_string()
        } else {
            format!("{} {}", if w { "WORD PTR" } else { "BYTE PTR" }, self.mem)
        }
    }
}

fn reg(r: u8, w: bool) -> &'static str {
    if w { REG16_NAMES[r as usize] } else { REG8_NAMES[r as usize] }
}

fn read_modrm(r: &mut Reader, seg: Option<&str>) -> ModRm {
    let b = r.u8();
    let md = b >> 6;
    let reg = (b >> 3) & 7;
    let rm = b & 7;
    let prefix = seg.map(|s| format!("{s}:")).unwrap_or_default();
    let mem = match md {
        3 => String::new(),
        0 if rm == 6 => format!("{prefix}[{}]", hex16(r.u16())),
        0 => format!("{prefix}[{}]", EA_BASE[rm as usize]),
        1 => {
            let d = r.u8() as i8;
            if d < 0 {
                format!("{prefix}[{}-{}]", EA_BASE[rm as usize], hex8(d.unsigned_abs()))
            } else {
                format!("{prefix}[{}+{}]", EA_BASE[rm as usize], hex8(d as u8))
            }
        }
        _ => format!("{prefix}[{}+{}]", EA_BASE[rm as usize], hex16(r.u16())),
    };
    ModRm { md, reg, rm, mem }
}

/// Disassembles one instruction from `bytes`, located at offset `ip` (used for jump targets).
/// A sign-extended byte immediate (`83 /n ib`, `6A ib`, `6B /r ib`): `-01h` rather than `0FFh`.
fn simm8(b: u8) -> String {
    let v = b as i8;
    if v < 0 { format!("-{}", hex8(v.unsigned_abs())) } else { hex8(b) }
}

pub fn disassemble(bytes: &[u8], ip: u16) -> Instruction {
    let mut r = Reader { bytes, pos: 0 };
    let mut seg: Option<&str> = None;
    let mut prefix = String::new();
    let mut op;
    loop {
        op = r.u8();
        match op {
            0x26 | 0x2E | 0x36 | 0x3E => seg = Some(SREG_NAMES[((op >> 3) & 3) as usize]),
            0xF0 | 0xF1 => prefix.push_str("LOCK "),
            0xF2 => prefix.push_str("REPNE "),
            0xF3 => prefix.push_str("REP "),
            _ => break,
        }
        if r.pos >= 6 {
            break;
        }
    }
    let rel8 = |r: &mut Reader| -> String {
        let d = r.u8() as i8 as u16;
        hex16(ip.wrapping_add(r.pos as u16).wrapping_add(d))
    };
    let rel16 = |r: &mut Reader| -> String {
        let d = r.u16();
        hex16(ip.wrapping_add(r.pos as u16).wrapping_add(d))
    };
    let segp = seg.map(|s| format!("{s}:")).unwrap_or_default();

    let text = match op {
        0x00..=0x3F if op & 7 < 6 => {
            let name = ALU[(op >> 3) as usize];
            let w = op & 1 != 0;
            match op & 7 {
                0 | 1 => {
                    let m = read_modrm(&mut r, seg);
                    format!("{name} {}, {}", m.operand(w), reg(m.reg, w))
                }
                2 | 3 => {
                    let m = read_modrm(&mut r, seg);
                    format!("{name} {}, {}", reg(m.reg, w), m.operand(w))
                }
                4 => format!("{name} AL, {}", hex8(r.u8())),
                _ => format!("{name} AX, {}", hex16(r.u16())),
            }
        }
        0x06 | 0x0E | 0x16 | 0x1E => format!("PUSH {}", SREG_NAMES[(op >> 3) as usize]),
        0x07 | 0x17 | 0x1F => format!("POP {}", SREG_NAMES[(op >> 3) as usize]),
        0x0F => {
            if bytes.get(r.pos) == Some(&0xFF) {
                r.u8();
                format!("BIOS {}", hex8(r.u8()))
            } else {
                "POP CS".into()
            }
        }
        0x27 => "DAA".into(),
        0x2F => "DAS".into(),
        0x37 => "AAA".into(),
        0x3F => "AAS".into(),
        0x40..=0x47 => format!("INC {}", REG16_NAMES[(op & 7) as usize]),
        0x48..=0x4F => format!("DEC {}", REG16_NAMES[(op & 7) as usize]),
        0x50..=0x57 => format!("PUSH {}", REG16_NAMES[(op & 7) as usize]),
        0x58..=0x5F => format!("POP {}", REG16_NAMES[(op & 7) as usize]),
        // 80186 forms (the emulated machine is an 8086 with 80186 extensions).
        0x60 => "PUSHA".into(),
        0x61 => "POPA".into(),
        0x62 => {
            let m = read_modrm(&mut r, seg);
            format!("BOUND {}, {}", reg(m.reg, true), m.operand(true))
        }
        0x68 => format!("PUSH {}", hex16(r.u16())),
        0x6A => format!("PUSH {}", simm8(r.u8())),
        0x69 | 0x6B => {
            let m = read_modrm(&mut r, seg);
            let imm = if op == 0x69 { hex16(r.u16()) } else { simm8(r.u8()) };
            format!("IMUL {}, {}, {imm}", reg(m.reg, true), m.operand(true))
        }
        0x6C => "INSB".into(),
        0x6D => "INSW".into(),
        0x6E => format!("OUTSB{}", seg_suffix(seg)),
        0x6F => format!("OUTSW{}", seg_suffix(seg)),
        0x63..=0x67 | 0x70..=0x7F => format!("{} {}", JCC[(op & 0xF) as usize], rel8(&mut r)),
        0x80..=0x83 => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            let imm = match op {
                0x81 => hex16(r.u16()),
                0x83 => simm8(r.u8()),
                _ => hex8(r.u8()),
            };
            format!("{} {}, {imm}", ALU[m.reg as usize], m.sized(w))
        }
        0x84 | 0x85 => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            format!("TEST {}, {}", m.operand(w), reg(m.reg, w))
        }
        0x86 | 0x87 => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            format!("XCHG {}, {}", m.operand(w), reg(m.reg, w))
        }
        0x88..=0x8B => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            if op & 2 == 0 {
                format!("MOV {}, {}", m.operand(w), reg(m.reg, w))
            } else {
                format!("MOV {}, {}", reg(m.reg, w), m.operand(w))
            }
        }
        0x8C => {
            let m = read_modrm(&mut r, seg);
            format!("MOV {}, {}", m.operand(true), SREG_NAMES[(m.reg & 3) as usize])
        }
        0x8D => {
            let m = read_modrm(&mut r, seg);
            format!("LEA {}, {}", reg(m.reg, true), m.operand(true))
        }
        0x8E => {
            let m = read_modrm(&mut r, seg);
            format!("MOV {}, {}", SREG_NAMES[(m.reg & 3) as usize], m.operand(true))
        }
        0x8F => {
            let m = read_modrm(&mut r, seg);
            format!("POP {}", m.sized(true))
        }
        0x90 => "NOP".into(),
        0x91..=0x97 => format!("XCHG AX, {}", REG16_NAMES[(op & 7) as usize]),
        0x98 => "CBW".into(),
        0x99 => "CWD".into(),
        0x9A => {
            let off = r.u16();
            let s = r.u16();
            format!("CALL {}:{}", hex16(s), hex16(off))
        }
        0x9B => "WAIT".into(),
        0x9C => "PUSHF".into(),
        0x9D => "POPF".into(),
        0x9E => "SAHF".into(),
        0x9F => "LAHF".into(),
        0xA0 => format!("MOV AL, {segp}[{}]", hex16(r.u16())),
        0xA1 => format!("MOV AX, {segp}[{}]", hex16(r.u16())),
        0xA2 => format!("MOV {segp}[{}], AL", hex16(r.u16())),
        0xA3 => format!("MOV {segp}[{}], AX", hex16(r.u16())),
        0xA4 => format!("MOVSB{}", seg_suffix(seg)),
        0xA5 => format!("MOVSW{}", seg_suffix(seg)),
        0xA6 => format!("CMPSB{}", seg_suffix(seg)),
        0xA7 => format!("CMPSW{}", seg_suffix(seg)),
        0xA8 => format!("TEST AL, {}", hex8(r.u8())),
        0xA9 => format!("TEST AX, {}", hex16(r.u16())),
        0xAA => "STOSB".into(),
        0xAB => "STOSW".into(),
        0xAC => format!("LODSB{}", seg_suffix(seg)),
        0xAD => format!("LODSW{}", seg_suffix(seg)),
        0xAE => "SCASB".into(),
        0xAF => "SCASW".into(),
        0xB0..=0xB7 => format!("MOV {}, {}", REG8_NAMES[(op & 7) as usize], hex8(r.u8())),
        0xB8..=0xBF => format!("MOV {}, {}", REG16_NAMES[(op & 7) as usize], hex16(r.u16())),
        0xC0 | 0xC1 => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            format!("{} {}, {}", SHIFTS[m.reg as usize], m.sized(w), hex8(r.u8()))
        }
        0xC2 => format!("RET {}", hex16(r.u16())),
        0xC3 => "RET".into(),
        0xC4 | 0xC5 => {
            let m = read_modrm(&mut r, seg);
            let n = if op == 0xC4 { "LES" } else { "LDS" };
            format!("{n} {}, {}", reg(m.reg, true), m.operand(true))
        }
        0xC6 | 0xC7 => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            let imm = if w { hex16(r.u16()) } else { hex8(r.u8()) };
            format!("MOV {}, {imm}", m.sized(w))
        }
        0xC8 => {
            let size = r.u16();
            format!("ENTER {}, {}", hex16(size), r.u8())
        }
        0xC9 => "LEAVE".into(),
        0xCA => format!("RETF {}", hex16(r.u16())),
        0xCB => "RETF".into(),
        0xCC => "INT 3".into(),
        0xCD => format!("INT {}", hex8(r.u8())),
        0xCE => "INTO".into(),
        0xCF => "IRET".into(),
        0xD0..=0xD3 => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            let cnt = if op & 2 != 0 { "CL" } else { "1" };
            format!("{} {}, {cnt}", SHIFTS[m.reg as usize], m.sized(w))
        }
        0xD4 => {
            let b = r.u8();
            if b == 10 { "AAM".into() } else { format!("AAM {}", hex8(b)) }
        }
        0xD5 => {
            let b = r.u8();
            if b == 10 { "AAD".into() } else { format!("AAD {}", hex8(b)) }
        }
        0xD6 => "SALC".into(),
        0xD7 => format!("XLATB{}", seg_suffix(seg)),
        0xD8..=0xDF => {
            let m = read_modrm(&mut r, seg);
            format!("ESC {}, {}", ((op & 7) << 3) | m.reg, m.operand(true))
        }
        0xE0 => format!("LOOPNE {}", rel8(&mut r)),
        0xE1 => format!("LOOPE {}", rel8(&mut r)),
        0xE2 => format!("LOOP {}", rel8(&mut r)),
        0xE3 => format!("JCXZ {}", rel8(&mut r)),
        0xE4 => format!("IN AL, {}", hex8(r.u8())),
        0xE5 => format!("IN AX, {}", hex8(r.u8())),
        0xE6 => format!("OUT {}, AL", hex8(r.u8())),
        0xE7 => format!("OUT {}, AX", hex8(r.u8())),
        0xE8 => format!("CALL {}", rel16(&mut r)),
        0xE9 => format!("JMP {}", rel16(&mut r)),
        0xEA => {
            let off = r.u16();
            let s = r.u16();
            format!("JMP {}:{}", hex16(s), hex16(off))
        }
        0xEB => format!("JMP {}", rel8(&mut r)),
        0xEC => "IN AL, DX".into(),
        0xED => "IN AX, DX".into(),
        0xEE => "OUT DX, AL".into(),
        0xEF => "OUT DX, AX".into(),
        0xF4 => "HLT".into(),
        0xF5 => "CMC".into(),
        0xF6 | 0xF7 => {
            let w = op & 1 != 0;
            let m = read_modrm(&mut r, seg);
            const G3: [&str; 8] = ["TEST", "TEST", "NOT", "NEG", "MUL", "IMUL", "DIV", "IDIV"];
            if m.reg < 2 {
                let imm = if w { hex16(r.u16()) } else { hex8(r.u8()) };
                format!("TEST {}, {imm}", m.sized(w))
            } else {
                format!("{} {}", G3[m.reg as usize], m.sized(w))
            }
        }
        0xF8 => "CLC".into(),
        0xF9 => "STC".into(),
        0xFA => "CLI".into(),
        0xFB => "STI".into(),
        0xFC => "CLD".into(),
        0xFD => "STD".into(),
        0xFE => {
            let m = read_modrm(&mut r, seg);
            match m.reg {
                0 => format!("INC {}", m.sized(false)),
                1 => format!("DEC {}", m.sized(false)),
                _ => "DB 0FEh".into(),
            }
        }
        0xFF => {
            let m = read_modrm(&mut r, seg);
            match m.reg {
                0 => format!("INC {}", m.sized(true)),
                1 => format!("DEC {}", m.sized(true)),
                2 => format!("CALL {}", m.operand(true)),
                3 => format!("CALL FAR {}", m.operand(true)),
                4 => format!("JMP {}", m.operand(true)),
                5 => format!("JMP FAR {}", m.operand(true)),
                _ => format!("PUSH {}", m.sized(true)),
            }
        }
        _ => format!("DB {}", hex8(op)),
    };
    let len = r.pos.min(bytes.len().max(1));
    Instruction { len, text: format!("{prefix}{text}") }
}

fn seg_suffix(seg: Option<&str>) -> String {
    // Shown in the MASM manner as a segment prefix on the source operand.
    seg.map(|s| format!(" {s}:")).unwrap_or_default()
}
