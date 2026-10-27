//! Instruction encoding.

use crate::expr::{Dist, Env, Expr, Reg, SegId, Val, eval};
use crate::parse::Operand;

/// Classified operand.
#[derive(Debug, Clone)]
pub enum Op {
    R8(u8),
    R16(u8),
    Sr(u8),
    Mem(Mem),
    Imm(Val),
    Far { seg: Val, off: Val },
}

#[derive(Debug, Clone)]
pub struct Mem {
    pub regs: Vec<u8>,
    pub disp: i64,
    pub unknown: bool,
    pub seg_ov: Option<u8>,
    pub size: Option<u8>,
    pub dist: Option<Dist>,
}

impl Op {
    fn size(&self) -> Option<u8> {
        match self {
            Op::R8(_) => Some(1),
            Op::R16(_) | Op::Sr(_) => Some(2),
            Op::Mem(m) => m.size,
            _ => None,
        }
    }
    fn is_rm(&self) -> bool {
        matches!(self, Op::R8(_) | Op::R16(_) | Op::Mem(_))
    }
}

/// Context of the instruction being encoded.
pub struct Ctx {
    /// Offset of the first byte of the instruction.
    pub loc: i64,
    pub seg: SegId,
    /// All code lives in one segment whose paragraph is unknown at assembly time (COM/BIN/BOOT).
    pub flat: bool,
    pub in_far_proc: bool,
}

/// Encoded instruction: bytes plus offsets of 16-bit fields that hold segment values.
#[derive(Debug, Default, Clone)]
pub struct Enc {
    pub bytes: Vec<u8>,
    pub relocs: Vec<usize>,
}

impl Enc {
    fn b(&mut self, v: u8) {
        self.bytes.push(v);
    }
    fn w(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
}

type R<T> = Result<T, String>;

pub fn classify(op: &Operand, env: &dyn Env) -> R<Op> {
    match op {
        Operand::Far(s, o) => Ok(Op::Far { seg: eval(s, env)?, off: eval(o, env)? }),
        Operand::Expr(e) => {
            let (inner, size) = match e {
                Expr::Ptr(sz, x) => (&**x, Some(*sz)),
                _ => (e, None),
            };
            match inner {
                Expr::Reg(Reg::R8(r)) => return Ok(Op::R8(*r)),
                Expr::Reg(Reg::R16(r)) => return Ok(Op::R16(*r)),
                Expr::Reg(Reg::Seg(r)) => return Ok(Op::Sr(*r)),
                _ => {}
            }
            let mut v = eval(e, env)?;
            if size.is_some() {
                v.size = size;
            }
            if v.mem || !v.regs.is_empty() || v.seg_ov.is_some() {
                if v.seg_base.is_some() {
                    return Err("segment value cannot be used as an address".into());
                }
                Ok(Op::Mem(Mem {
                    regs: v.regs,
                    disp: v.num,
                    unknown: v.unknown,
                    seg_ov: v.seg_ov,
                    size: v.size,
                    dist: v.dist,
                }))
            } else {
                Ok(Op::Imm(v))
            }
        }
    }
}

const SEG_PREFIX: [u8; 4] = [0x26, 0x2E, 0x36, 0x3E];

/// Encodes ModR/M (+ displacement). Returns (segment prefix, bytes).
fn modrm(reg: u8, op: &Op) -> R<(Option<u8>, Vec<u8>)> {
    match op {
        Op::R8(r) | Op::R16(r) => Ok((None, vec![0xC0 | (reg << 3) | r])),
        Op::Mem(m) => {
            let mut rs = m.regs.clone();
            rs.sort();
            // 3=BX 5=BP 6=SI 7=DI
            let rm = match rs.as_slice() {
                [] => None,
                [3, 6] => Some(0),
                [3, 7] => Some(1),
                [5, 6] => Some(2),
                [5, 7] => Some(3),
                [6] => Some(4),
                [7] => Some(5),
                [5] => Some(6),
                [3] => Some(7),
                _ => return Err("invalid combination of address registers".into()),
            };
            let prefix = m.seg_ov.map(|s| SEG_PREFIX[s as usize]);
            let disp = m.disp as u16;
            let mut out = Vec::new();
            match rm {
                None => {
                    out.push((reg << 3) | 6);
                    out.extend_from_slice(&disp.to_le_bytes());
                }
                Some(rm) => {
                    let d = disp as i16;
                    if !m.unknown && d == 0 && rm != 6 {
                        out.push((reg << 3) | rm);
                    } else if !m.unknown && (-128..=127).contains(&d) {
                        out.push(0x40 | (reg << 3) | rm);
                        out.push(d as u8);
                    } else {
                        out.push(0x80 | (reg << 3) | rm);
                        out.extend_from_slice(&disp.to_le_bytes());
                    }
                }
            }
            Ok((prefix, out))
        }
        _ => Err("register or memory operand expected".into()),
    }
}

/// Byte operand: -128..255 (signed or unsigned), as for DB data.
fn check8(v: &Val) -> R<u8> {
    if !v.unknown && !(-128..=255).contains(&v.num) {
        return Err(format!("value {} out of byte range", v.num));
    }
    if v.seg_base.is_some() {
        return Err("segment value does not fit in a byte".into());
    }
    Ok(v.num as u8)
}

/// Word operand: -32768..65535 (signed or unsigned), as for DW data.
fn check16(v: &Val) -> R<u16> {
    if !v.unknown && !(-32768..=65535).contains(&v.num) {
        return Err(format!("value {} out of word range", v.num));
    }
    Ok(v.num as u16)
}

fn imm_w(e: &mut Enc, v: &Val) -> R<()> {
    let x = check16(v)?;
    if v.seg_base.is_some() {
        e.relocs.push(e.bytes.len());
    }
    e.w(x);
    Ok(())
}

fn fits_i8(v: &Val) -> bool {
    !v.unknown
        && v.seg_base.is_none()
        && (-32_768..=65_535).contains(&v.num)
        && (-128..=127).contains(&(v.num as u16 as i16))
}

/// Resolves the operand width (true = 16-bit) of a two-operand instruction.
fn width(a: &Op, b: &Op) -> R<bool> {
    let is_reg = |o: &Op| matches!(o, Op::R8(_) | Op::R16(_) | Op::Sr(_));
    match (a.size(), b.size()) {
        // Emu8086: the register decides when it disagrees with a variable's type or PTR
        // (`mov ax, dword_var`, `mov w. byte_var, al`).
        (Some(x), Some(y)) if x != y && is_reg(a) && !is_reg(b) => width_of(x),
        (Some(x), Some(y)) if x != y && is_reg(b) && !is_reg(a) => width_of(y),
        (Some(x), Some(y)) if x != y => Err(format!("operand size mismatch ({x} vs {y} bytes)")),
        (None, None) if matches!(b, Op::Imm(_)) => Ok(!imm_fits_byte(b)),
        (None, None) if matches!(a, Op::Imm(_)) => Ok(!imm_fits_byte(a)),
        (Some(1), _) | (_, Some(1)) => Ok(false),
        (Some(2), _) | (_, Some(2)) => Ok(true),
        (Some(s), _) | (_, Some(s)) => Err(format!("invalid operand size {s}")),
        (None, None) => Err("operand size not specified (use BYTE PTR or WORD PTR)".into()),
    }
}

fn width_of(size: u8) -> R<bool> {
    match size {
        1 => Ok(false),
        2 => Ok(true),
        s => Err(format!("invalid operand size {s}")),
    }
}

fn imm_fits_byte(o: &Op) -> bool {
    match o {
        Op::Imm(v) => v.seg_base.is_none() && v.label_seg.is_none() && (-128..=255).contains(&v.num),
        _ => false,
    }
}

/// Width of `dst` used with an immediate: an untyped memory operand is a byte when the value
/// fits in 8 bits and a word otherwise (Emu8086 infers the size like this: `mov [bx], 0`).
fn width_with_imm(dst: &Op, v: &Val) -> R<bool> {
    match dst.size() {
        Some(s) => width_of(s),
        None => Ok(!imm_fits_byte(&Op::Imm(v.clone()))),
    }
}

fn single_width(a: &Op) -> R<bool> {
    match a.size() {
        Some(1) => Ok(false),
        Some(2) => Ok(true),
        Some(s) => Err(format!("invalid operand size {s}")),
        None => Err("operand size not specified (use BYTE PTR or WORD PTR)".into()),
    }
}

fn emit_rm(e: &mut Enc, opcode: &[u8], reg: u8, rm: &Op) -> R<()> {
    let (prefix, m) = modrm(reg, rm)?;
    if let Some(p) = prefix {
        e.b(p);
    }
    e.bytes.extend_from_slice(opcode);
    e.bytes.extend(m);
    Ok(())
}

pub fn jcc_code(m: &str) -> Option<u8> {
    Some(match m {
        "JO" => 0,
        "JNO" => 1,
        "JB" | "JC" | "JNAE" => 2,
        "JNB" | "JNC" | "JAE" => 3,
        "JE" | "JZ" => 4,
        "JNE" | "JNZ" => 5,
        "JBE" | "JNA" => 6,
        "JA" | "JNBE" => 7,
        "JS" => 8,
        "JNS" => 9,
        "JP" | "JPE" => 10,
        "JNP" | "JPO" => 11,
        "JL" | "JNGE" => 12,
        "JGE" | "JNL" => 13,
        "JLE" | "JNG" => 14,
        "JG" | "JNLE" => 15,
        _ => return None,
    })
}

const ALU: [&str; 8] = ["ADD", "OR", "ADC", "SBB", "AND", "SUB", "XOR", "CMP"];
const SHIFT: [&str; 8] = ["ROL", "ROR", "RCL", "RCR", "SHL", "SHR", "SAL", "SAR"];
const GRP3: [&str; 6] = ["NOT", "NEG", "MUL", "IMUL", "DIV", "IDIV"];

fn simple(m: &str) -> Option<&'static [u8]> {
    Some(match m {
        "AAA" => &[0x37],
        "AAS" => &[0x3F],
        "CBW" => &[0x98],
        "CWD" => &[0x99],
        "CLC" => &[0xF8],
        "CLD" => &[0xFC],
        "CLI" => &[0xFA],
        "CMC" => &[0xF5],
        "DAA" => &[0x27],
        "DAS" => &[0x2F],
        "HLT" => &[0xF4],
        "INTO" => &[0xCE],
        "IRET" => &[0xCF],
        "LAHF" => &[0x9F],
        "SAHF" => &[0x9E],
        "NOP" => &[0x90],
        "PUSHF" => &[0x9C],
        "POPF" => &[0x9D],
        "WAIT" | "FWAIT" => &[0x9B],
        "STC" => &[0xF9],
        "STD" => &[0xFD],
        "STI" => &[0xFB],
        "MOVSB" => &[0xA4],
        "MOVSW" => &[0xA5],
        "CMPSB" => &[0xA6],
        "CMPSW" => &[0xA7],
        "STOSB" => &[0xAA],
        "STOSW" => &[0xAB],
        "LODSB" => &[0xAC],
        "LODSW" => &[0xAD],
        "SCASB" => &[0xAE],
        "SCASW" => &[0xAF],
        "SALC" => &[0xD6],
        // 80186 instructions supported by Emu8086.
        "PUSHA" => &[0x60],
        "POPA" => &[0x61],
        "LEAVE" => &[0xC9],
        _ => return None,
    })
}

/// Whether `m` is a known instruction mnemonic.
pub fn is_mnemonic(m: &str) -> bool {
    simple(m).is_some()
        || jcc_code(m).is_some()
        || ALU.contains(&m)
        || SHIFT.contains(&m)
        || GRP3.contains(&m)
        || matches!(
            m,
            "MOV"
                | "TEST"
                | "XCHG"
                | "INC"
                | "DEC"
                | "PUSH"
                | "POP"
                | "LEA"
                | "LDS"
                | "LES"
                | "IN"
                | "OUT"
                | "INT"
                | "JMP"
                | "CALL"
                | "RET"
                | "RETN"
                | "RETF"
                | "LOOP"
                | "LOOPE"
                | "LOOPZ"
                | "LOOPNE"
                | "LOOPNZ"
                | "JCXZ"
                | "AAM"
                | "AAD"
                | "XLAT"
                | "XLATB"
                | "MOVS"
                | "CMPS"
                | "STOS"
                | "LODS"
                | "SCAS"
                | "ESC"
                | "REP"
                | "REPE"
                | "REPZ"
                | "REPNE"
                | "REPNZ"
                | "LOCK"
        )
}

pub fn encode(prefixes: &[String], mnem: &str, ops: &[Operand], env: &dyn Env, ctx: &Ctx) -> R<Enc> {
    let mut e = Enc::default();
    for p in prefixes {
        e.b(match p.as_str() {
            "LOCK" => 0xF0,
            "REPNE" | "REPNZ" => 0xF2,
            _ => 0xF3,
        });
    }
    // Prefix used as a standalone instruction.
    match mnem {
        "LOCK" => {
            e.b(0xF0);
            return Ok(e);
        }
        "REP" | "REPE" | "REPZ" => {
            e.b(0xF3);
            return Ok(e);
        }
        "REPNE" | "REPNZ" => {
            e.b(0xF2);
            return Ok(e);
        }
        _ => {}
    }
    let ops: Vec<Op> = ops.iter().map(|o| classify(o, env)).collect::<R<_>>()?;
    let n = ops.len();
    let want =
        |k: usize| -> R<()> { if n != k { Err(format!("{mnem} expects {k} operand(s), got {n}")) } else { Ok(()) } };

    if let Some(code) = simple(mnem) {
        want(0)?;
        e.bytes.extend_from_slice(code);
        return Ok(e);
    }

    if let Some(i) = ALU.iter().position(|a| *a == mnem) {
        want(2)?;
        alu(&mut e, i as u8, &ops[0], &ops[1])?;
        return Ok(e);
    }

    if let Some(i) = SHIFT.iter().position(|a| *a == mnem) {
        let sub = if i == 6 { 4 } else { i as u8 };
        if n == 1 {
            let w = single_width(&ops[0])?;
            emit_rm(&mut e, &[0xD0 | w as u8], sub, &ops[0])?;
            return Ok(e);
        }
        want(2)?;
        if !ops[0].is_rm() {
            return Err(format!("{mnem}: register or memory operand expected"));
        }
        let w = single_width(&ops[0])?;
        match &ops[1] {
            Op::R8(1) => emit_rm(&mut e, &[0xD2 | w as u8], sub, &ops[0])?,
            Op::Imm(v) => {
                let count = if v.unknown { 1 } else { v.num };
                if !(0..=31).contains(&count) {
                    return Err("shift count out of range".into());
                }
                // 8086 has no immediate count: repeat the 1-bit form.
                for _ in 0..count {
                    emit_rm(&mut e, &[0xD0 | w as u8], sub, &ops[0])?;
                }
            }
            _ => return Err(format!("{mnem}: count must be 1, a constant or CL")),
        }
        return Ok(e);
    }

    if mnem == "IMUL" && matches!(n, 2 | 3) {
        let (reg, src, imm) = match ops.as_slice() {
            [Op::R16(reg), Op::Imm(imm)] => (*reg, &ops[0], imm),
            [Op::R16(reg), src, Op::Imm(imm)] => (*reg, src, imm),
            _ => return Err("IMUL immediate form expects a word register, r/m16 and an immediate".into()),
        };
        if !src.is_rm() || !width(&ops[0], src)? {
            return Err("IMUL immediate form requires a word r/m operand".into());
        }
        if fits_i8(imm) {
            emit_rm(&mut e, &[0x6B], reg, src)?;
            e.b(imm.num as u8);
        } else {
            emit_rm(&mut e, &[0x69], reg, src)?;
            imm_w(&mut e, imm)?;
        }
        return Ok(e);
    }

    if let Some(i) = GRP3.iter().position(|a| *a == mnem) {
        if n != 1 {
            return Err(format!("{mnem} expects 1 operand"));
        }
        if !ops[0].is_rm() {
            return Err(format!("{mnem}: register or memory operand expected"));
        }
        let w = single_width(&ops[0])?;
        emit_rm(&mut e, &[0xF6 | w as u8], i as u8 + 2, &ops[0])?;
        return Ok(e);
    }

    if let Some(cc) = jcc_code(mnem) {
        want(1)?;
        let (target, dist, unknown) = rel_target(&ops[0], ctx)?;
        let short = target - (ctx.loc + e.bytes.len() as i64 + 2);
        if !unknown && (-128..=127).contains(&short) {
            e.b(0x70 | cc);
            e.b(short as u8);
        } else if dist == Some(Dist::Short) {
            return Err(format!("jump out of range by {} byte(s)", short.abs() - 127));
        } else {
            // Out-of-range conditional jump: inverted short jump over a near JMP.
            e.b(0x70 | (cc ^ 1));
            e.b(3);
            let near = target - (ctx.loc + e.bytes.len() as i64 + 3);
            e.b(0xE9);
            e.w(near as u16);
        }
        return Ok(e);
    }

    match mnem {
        "LOOP" | "LOOPE" | "LOOPZ" | "LOOPNE" | "LOOPNZ" | "JCXZ" => {
            want(1)?;
            let op = match mnem {
                "LOOP" => 0xE2,
                "LOOPE" | "LOOPZ" => 0xE1,
                "LOOPNE" | "LOOPNZ" => 0xE0,
                _ => 0xE3,
            };
            let (target, _, unknown) = rel_target(&ops[0], ctx)?;
            let d = target - (ctx.loc + e.bytes.len() as i64 + 2);
            if (unknown || !(-128..=127).contains(&d)) && op == 0xE3 {
                // Emu8086 assembles a far JCXZ as OR CX, CX + JZ (inverted short jump + near JMP).
                e.bytes.extend_from_slice(&[0x0B, 0xC9, 0x75, 0x03, 0xE9]);
                let near = target - (ctx.loc + e.bytes.len() as i64 + 2);
                e.w(near as u16);
                return Ok(e);
            }
            if !unknown && !(-128..=127).contains(&d) {
                return Err(format!("{mnem} target out of range by {} byte(s)", d.abs() - 127));
            }
            e.b(op);
            e.b(d as u8);
        }
        "JMP" | "CALL" => {
            want(1)?;
            let call = mnem == "CALL";
            match &ops[0] {
                Op::R16(_) | Op::Mem(_) => {
                    let far = match &ops[0] {
                        Op::Mem(m) => m.dist == Some(Dist::Far) || m.size == Some(4),
                        _ => false,
                    };
                    let sub = match (call, far) {
                        (true, false) => 2,
                        (true, true) => 3,
                        (false, false) => 4,
                        (false, true) => 5,
                    };
                    emit_rm(&mut e, &[0xFF], sub, &ops[0])?;
                }
                Op::Far { seg, off } => {
                    e.b(if call { 0x9A } else { 0xEA });
                    e.w(check16(off)?);
                    imm_w(&mut e, seg)?;
                }
                Op::Imm(v) => {
                    let other_seg = v.label_seg.is_some_and(|s| s != ctx.seg && s != usize::MAX) && !ctx.flat;
                    let far = v.dist == Some(Dist::Far) || (v.far_label && v.dist.is_none()) || other_seg;
                    if far && !ctx.flat {
                        if v.label_seg.is_none() && !v.unknown {
                            return Err("FAR target needs a label or seg:offset".into());
                        }
                        e.b(if call { 0x9A } else { 0xEA });
                        e.w(check16(v)?);
                        // Segment of the label: paragraph + relocation.
                        e.relocs.push(e.bytes.len());
                        e.w(v.label_para as u16);
                        return Ok(e);
                    }
                    if far && call {
                        // Flat model: PUSH CS + near CALL is equivalent to a far call.
                        e.b(0x0E);
                    }
                    let (target, dist, unknown) = rel_target(&ops[0], ctx)?;
                    if call {
                        let d = target - (ctx.loc + e.bytes.len() as i64 + 3);
                        e.b(0xE8);
                        e.w(d as u16);
                    } else {
                        let short = target - (ctx.loc + e.bytes.len() as i64 + 2);
                        let can_short = !unknown && (-128..=127).contains(&short);
                        if dist == Some(Dist::Short) || (dist.is_none() && can_short) {
                            if !can_short && !unknown {
                                return Err(format!("short jump out of range by {} byte(s)", short.abs() - 127));
                            }
                            e.b(0xEB);
                            e.b(short as u8);
                        } else {
                            let d = target - (ctx.loc + e.bytes.len() as i64 + 3);
                            e.b(0xE9);
                            e.w(d as u16);
                        }
                    }
                }
                _ => return Err(format!("invalid {mnem} operand")),
            }
        }
        "RET" | "RETN" | "RETF" => {
            let far = mnem == "RETF" || (mnem == "RET" && ctx.in_far_proc);
            match ops.as_slice() {
                [] => e.b(if far { 0xCB } else { 0xC3 }),
                [Op::Imm(v)] => {
                    e.b(if far { 0xCA } else { 0xC2 });
                    e.w(check16(v)?);
                }
                _ => return Err("RET expects an optional constant".into()),
            }
        }
        "INT" => {
            want(1)?;
            let Op::Imm(v) = &ops[0] else { return Err("INT expects a constant".into()) };
            let n = check8(v)?;
            if n == 3 && !v.unknown {
                e.b(0xCC);
            } else {
                e.b(0xCD);
                e.b(n);
            }
        }
        "AAM" | "AAD" => {
            let base = match ops.as_slice() {
                [] => 10,
                [Op::Imm(v)] => check8(v)?,
                _ => return Err(format!("{mnem} expects an optional constant")),
            };
            e.b(if mnem == "AAM" { 0xD4 } else { 0xD5 });
            e.b(base);
        }
        "MOV" => {
            want(2)?;
            mov(&mut e, &ops[0], &ops[1])?;
        }
        "TEST" => {
            want(2)?;
            let (a, b) = match (&ops[0], &ops[1]) {
                (Op::Imm(_), x) => (x, &ops[0]),
                (x, y) => (x, y),
            };
            let w = width(a, b)?;
            match (a, b) {
                (Op::R8(0), Op::Imm(v)) | (Op::R16(0), Op::Imm(v)) => {
                    e.b(0xA8 | w as u8);
                    imm(&mut e, v, w)?;
                }
                (rm, Op::Imm(v)) if rm.is_rm() => {
                    emit_rm(&mut e, &[0xF6 | w as u8], 0, rm)?;
                    imm(&mut e, v, w)?;
                }
                (rm, Op::R8(r)) | (rm, Op::R16(r)) | (Op::R8(r), rm) | (Op::R16(r), rm) if rm.is_rm() => {
                    emit_rm(&mut e, &[0x84 | w as u8], *r, rm)?;
                }
                _ => return Err("invalid TEST operands".into()),
            }
        }
        "XCHG" => {
            want(2)?;
            let w = width(&ops[0], &ops[1])?;
            match (&ops[0], &ops[1]) {
                (Op::R16(0), Op::R16(r)) | (Op::R16(r), Op::R16(0)) => e.b(0x90 | r),
                (Op::R8(r), rm) | (Op::R16(r), rm) | (rm, Op::R8(r)) | (rm, Op::R16(r)) if rm.is_rm() => {
                    emit_rm(&mut e, &[0x86 | w as u8], *r, rm)?;
                }
                _ => return Err("invalid XCHG operands".into()),
            }
        }
        "INC" | "DEC" => {
            want(1)?;
            let sub = (mnem == "DEC") as u8;
            match &ops[0] {
                Op::R16(r) => e.b(0x40 | (sub << 3) | r),
                rm if rm.is_rm() => {
                    let w = single_width(rm)?;
                    emit_rm(&mut e, &[0xFE | w as u8], sub, rm)?;
                }
                _ => return Err(format!("invalid {mnem} operand")),
            }
        }
        "PUSH" | "POP" => {
            want(1)?;
            let pop = mnem == "POP";
            match &ops[0] {
                Op::R16(r) => e.b(if pop { 0x58 } else { 0x50 } | r),
                Op::Sr(s) => {
                    if pop && *s == 1 {
                        return Err("POP CS is not allowed".into());
                    }
                    e.b((s << 3) | if pop { 0x07 } else { 0x06 });
                }
                Op::Mem(m) => {
                    if m.size.is_some_and(|s| s != 2) {
                        return Err(format!("{mnem} requires a word operand"));
                    }
                    if pop {
                        emit_rm(&mut e, &[0x8F], 0, &ops[0])?;
                    } else {
                        emit_rm(&mut e, &[0xFF], 6, &ops[0])?;
                    }
                }
                Op::Imm(v) if !pop => {
                    // 80186 PUSH imm (supported by Emu8086).
                    if fits_i8(v) {
                        e.b(0x6A);
                        e.b(v.num as u8);
                    } else {
                        e.b(0x68);
                        imm_w(&mut e, v)?;
                    }
                }
                _ => return Err(format!("{mnem} requires a 16-bit operand")),
            }
        }
        "LEA" | "LDS" | "LES" => {
            want(2)?;
            let Op::R16(r) = ops[0] else { return Err(format!("{mnem}: 16-bit register expected")) };
            if !matches!(ops[1], Op::Mem(_)) {
                return Err(format!("{mnem}: memory operand expected"));
            }
            let opc = match mnem {
                "LEA" => 0x8D,
                "LDS" => 0xC5,
                _ => 0xC4,
            };
            emit_rm(&mut e, &[opc], r, &ops[1])?;
        }
        "IN" => {
            want(2)?;
            let w = match ops[0] {
                Op::R8(0) => false,
                Op::R16(0) => true,
                _ => return Err("IN: destination must be AL or AX".into()),
            };
            match &ops[1] {
                Op::R16(2) => e.b(0xEC | w as u8),
                Op::Imm(v) => {
                    e.b(0xE4 | w as u8);
                    e.b(port(v)?);
                }
                _ => return Err("IN: port must be a constant 0..255 or DX".into()),
            }
        }
        "OUT" => {
            want(2)?;
            let w = match ops[1] {
                Op::R8(0) => false,
                Op::R16(0) => true,
                _ => return Err("OUT: source must be AL or AX".into()),
            };
            match &ops[0] {
                Op::R16(2) => e.b(0xEE | w as u8),
                Op::Imm(v) => {
                    e.b(0xE6 | w as u8);
                    e.b(port(v)?);
                }
                _ => return Err("OUT: port must be a constant 0..255 or DX".into()),
            }
        }
        "XLAT" | "XLATB" => {
            if let Some(Op::Mem(m)) = ops.first()
                && let Some(s) = m.seg_ov
            {
                e.b(SEG_PREFIX[s as usize]);
            }
            e.b(0xD7);
        }
        "MOVS" | "CMPS" | "STOS" | "LODS" | "SCAS" => {
            let base = match mnem {
                "MOVS" => 0xA4,
                "CMPS" => 0xA6,
                "STOS" => 0xAA,
                "LODS" => 0xAC,
                _ => 0xAE,
            };
            let mut size = None;
            let mut seg = None;
            for (k, o) in ops.iter().enumerate() {
                if let Some(s) = o.size() {
                    size.get_or_insert(s);
                }
                // The source operand (DS:SI) may have a segment override.
                if let Op::Mem(m) = o {
                    let is_src =
                        matches!(mnem, "LODS") || (matches!(mnem, "MOVS") && k == 1) || (mnem == "CMPS" && k == 0);
                    if is_src && m.seg_ov.is_some_and(|s| s != 3) {
                        seg = m.seg_ov;
                    }
                }
            }
            let w = match size {
                Some(1) => false,
                Some(2) => true,
                _ => return Err(format!("{mnem}: operand size not specified")),
            };
            if let Some(s) = seg {
                e.b(SEG_PREFIX[s as usize]);
            }
            e.b(base | w as u8);
        }
        "ESC" => {
            want(2)?;
            let Op::Imm(v) = &ops[0] else { return Err("ESC expects a constant".into()) };
            let code = check8(v)? & 0x3F;
            emit_rm(&mut e, &[0xD8 | (code >> 3)], code & 7, &ops[1])?;
        }
        _ => return Err(format!("unknown instruction '{mnem}'")),
    }
    Ok(e)
}

fn port(v: &Val) -> R<u8> {
    if !v.unknown && !(0..=255).contains(&v.num) {
        return Err("port number must be 0..255 (use DX for larger ports)".into());
    }
    Ok(v.num as u8)
}

fn rel_target(op: &Op, ctx: &Ctx) -> R<(i64, Option<Dist>, bool)> {
    match op {
        Op::Imm(v) => {
            if v.seg_base.is_some() {
                return Err("invalid jump target".into());
            }
            if let Some(s) = v.label_seg
                && s != ctx.seg
                && s != usize::MAX
                && !ctx.flat
            {
                return Err("near jump to a label in another segment".into());
            }
            Ok((v.num, v.dist, v.unknown))
        }
        _ => Err("label expected".into()),
    }
}

fn imm(e: &mut Enc, v: &Val, w: bool) -> R<()> {
    if w {
        imm_w(e, v)
    } else {
        let b = check8(v)?;
        e.b(b);
        Ok(())
    }
}

fn alu(e: &mut Enc, n: u8, a: &Op, b: &Op) -> R<()> {
    match (a, b) {
        (Op::Imm(_), _) => Err("destination cannot be a constant".into()),
        (Op::Mem(_), Op::Mem(_)) => Err("memory-to-memory operation is not allowed".into()),
        (dst, Op::Imm(v)) if dst.is_rm() => {
            let w = width_with_imm(dst, v)?;
            match dst {
                Op::R8(0) | Op::R16(0) if !(w && fits_i8(v)) => {
                    e.b((n << 3) | 4 | w as u8);
                    imm(e, v, w)
                }
                _ if w && fits_i8(v) => {
                    emit_rm(e, &[0x83], n, dst)?;
                    e.b(v.num as u8);
                    Ok(())
                }
                _ => {
                    emit_rm(e, &[0x80 | w as u8], n, dst)?;
                    imm(e, v, w)
                }
            }
        }
        (rm, Op::R8(r)) | (rm, Op::R16(r)) if rm.is_rm() => {
            let w = width(a, b)?;
            emit_rm(e, &[(n << 3) | w as u8], *r, rm)
        }
        (Op::R8(r), rm) | (Op::R16(r), rm) if rm.is_rm() => {
            let w = width(a, b)?;
            emit_rm(e, &[(n << 3) | 2 | w as u8], *r, rm)
        }
        _ => Err(format!("invalid operands for {}", ALU[n as usize])),
    }
}

fn mov(e: &mut Enc, a: &Op, b: &Op) -> R<()> {
    match (a, b) {
        (Op::Imm(_), _) => Err("destination cannot be a constant".into()),
        (Op::Mem(_), Op::Mem(_)) => Err("memory-to-memory MOV is not allowed".into()),
        (Op::Sr(_), Op::Sr(_)) => Err("MOV between segment registers is not allowed".into()),
        (Op::Sr(_), Op::Imm(_)) => {
            Err("cannot load a segment register with a constant (use a general register)".into())
        }
        (Op::Sr(1), _) => Err("MOV CS is not allowed".into()),
        (Op::Sr(s), rm) => {
            if rm.size() == Some(1) {
                return Err("operand size mismatch".into());
            }
            emit_rm(e, &[0x8E], *s, rm)
        }
        (rm, Op::Sr(s)) if rm.is_rm() => {
            if rm.size() == Some(1) {
                return Err("operand size mismatch".into());
            }
            emit_rm(e, &[0x8C], *s, rm)
        }
        (Op::R8(r), Op::Imm(v)) => {
            e.b(0xB0 | r);
            e.b(check8(v)?);
            Ok(())
        }
        (Op::R16(r), Op::Imm(v)) => {
            e.b(0xB8 | r);
            imm_w(e, v)
        }
        (Op::Mem(_), Op::Imm(v)) => {
            let w = width_with_imm(a, v)?;
            emit_rm(e, &[0xC6 | w as u8], 0, a)?;
            imm(e, v, w)
        }
        // Accumulator <-> direct address short forms.
        (Op::R8(0), Op::Mem(m)) | (Op::R16(0), Op::Mem(m)) if m.regs.is_empty() => {
            let w = width(a, b)?;
            if let Some(s) = m.seg_ov {
                e.b(SEG_PREFIX[s as usize]);
            }
            e.b(0xA0 | w as u8);
            e.w(m.disp as u16);
            Ok(())
        }
        (Op::Mem(m), Op::R8(0)) | (Op::Mem(m), Op::R16(0)) if m.regs.is_empty() => {
            let w = width(a, b)?;
            if let Some(s) = m.seg_ov {
                e.b(SEG_PREFIX[s as usize]);
            }
            e.b(0xA2 | w as u8);
            e.w(m.disp as u16);
            Ok(())
        }
        (rm, Op::R8(r)) | (rm, Op::R16(r)) if rm.is_rm() => {
            let w = width(a, b)?;
            emit_rm(e, &[0x88 | w as u8], *r, rm)
        }
        (Op::R8(r), rm) | (Op::R16(r), rm) if rm.is_rm() => {
            let w = width(a, b)?;
            emit_rm(e, &[0x8A | w as u8], *r, rm)
        }
        _ => Err("invalid MOV operands".into()),
    }
}
