//! The MIPS disassembler: machine words back to instructions, by the table the assembler uses
//! (MARS's basic instructions, see [`crate::asm::BASIC`]).
//!
//! A word is the first basic form whose fixed bits it has (MARS decodes the same way: word 0 is
//! `nop`, a `jalr` with rd = 31 is `jalr $31,$9`); the `f` / `s` / `t` fields of the mask give the
//! operands. `clz` / `clo` are recognized with any rt (MARS writes 0, MIPS32 rt = rd). Two notations:
//!
//! - [`Style::Mars`]: MARS's Basic column, `addi $8,$9,-100`, `lw $8,-100($9)`, a branch with its
//!   word offset (`beq $8,$9,3`), a jump with its address in decimal;
//! - [`Style::Named`]: for reading, `addi $t0, $t1, -100`, branch and jump targets as addresses
//!   (`beq $t0, $t1, 0x00400010`), logical immediates in hex (`ori $t0, $at, 0x1001`).
//!
//! A word that is not an instruction is shown as data: `.word 0xfc000000`. Floating-point
//! registers are `$f0` in both notations.

use std::sync::OnceLock;

use crate::asm::{BASIC, Basic, Format, IntClass, Kind, tokenize};
use crate::cpu::REG_NAMES;

/// How the text is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// MARS's Basic column: register numbers, values in decimal, branch offsets.
    Mars,
    /// Register names, branch and jump targets as addresses, logical immediates in hex.
    Named,
}

/// What an operand of a decoded instruction is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    /// A general-purpose register.
    Reg(u32),
    /// A coprocessor 0 register (`mfc0` / `mtc0`).
    Cp0(u32),
    /// A floating-point (coprocessor 1) register.
    Fpr(u32),
    /// An immediate, sign- or zero-extended by its class (a shift amount, a code).
    Imm(i32),
    /// A branch: the word offset from the next instruction, and the target address.
    Branch(i32, u32),
    /// A jump target address.
    Jump(u32),
}

/// A decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The mnemonic (lower case).
    pub mnemonic: &'static str,
    /// The operands in source order (parentheses not included: see [`Decoded::memory`]).
    pub args: Vec<Arg>,
    /// The form is `rt, offset(base)`: the last two operands are written `offset(base)`.
    pub memory: bool,
    /// MARS's basic instruction.
    pub basic: &'static Basic,
}

/// What a form needs to decode a word.
struct Pattern {
    /// Bits fixed by the mask, and their values.
    fixed: u32,
    bits: u32,
    /// Bit positions (MSB first) of the f, s, t fields.
    fields: [Vec<u32>; 3],
    /// The operand kinds of the example (registers, integers with their class, labels).
    operands: Vec<Kind>,
    /// The memory form `x, offset(base)`.
    memory: bool,
    mnemonic: &'static str,
    basic: &'static Basic,
}

fn patterns() -> &'static [Pattern] {
    static PATTERNS: OnceLock<Vec<Pattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        BASIC
            .iter()
            .map(|b| {
                let (mut fixed, mut bits) = (0u32, 0u32);
                let mut fields: [Vec<u32>; 3] = Default::default();
                for (i, c) in b.mask.bytes().enumerate() {
                    let bit = 31 - i as u32;
                    match c {
                        b'0' => fixed |= 1 << bit,
                        b'1' => {
                            fixed |= 1 << bit;
                            bits |= 1 << bit;
                        }
                        b'f' => fields[0].push(bit),
                        b's' => fields[1].push(bit),
                        b't' => fields[2].push(bit),
                        _ => {}
                    }
                }
                let tokens = tokenize(b.example).expect("basic examples are valid");
                let memory = tokens.iter().any(|t| t.kind == Kind::LParen);
                let operands: Vec<Kind> = tokens[1..]
                    .iter()
                    .filter(|t| !matches!(t.kind, Kind::LParen | Kind::RParen))
                    .map(|t| t.kind.clone())
                    .collect();
                let mnemonic = b.example.split(' ').next().unwrap_or(b.example);
                // clz / clo: MARS encodes rt = 0, MIPS32 rt = rd; the processor ignores it.
                if matches!(mnemonic, "clz" | "clo") {
                    fixed &= !(31 << 16);
                }
                Pattern { fixed, bits, fields, operands, memory, mnemonic, basic: b }
            })
            .collect()
    })
}

fn field(word: u32, positions: &[u32]) -> u32 {
    positions.iter().fold(0, |v, &p| (v << 1) | (word >> p & 1))
}

/// Decodes the word at `addr`; None when it is not a (supported) instruction.
pub fn decode(word: u32, addr: u32) -> Option<Decoded> {
    let p = patterns().iter().find(|p| word & p.fixed == p.bits)?;
    let args = p
        .operands
        .iter()
        .enumerate()
        .map(|(k, kind)| {
            let positions = &p.fields[k.min(2)];
            let raw = field(word, positions);
            let width = positions.len() as u32;
            match kind {
                Kind::RegName(_) | Kind::RegNum(_) if k == 1 && matches!(p.mnemonic, "mfc0" | "mtc0") => Arg::Cp0(raw),
                Kind::RegName(_) | Kind::RegNum(_) => Arg::Reg(raw),
                Kind::FpReg(_) => Arg::Fpr(raw),
                Kind::Ident if p.basic.format == Format::J => {
                    Arg::Jump((addr.wrapping_add(4) & 0xF000_0000) | (raw << 2))
                }
                Kind::Ident => {
                    let off = raw as u16 as i16 as i32;
                    Arg::Branch(off, addr.wrapping_add(4).wrapping_add((off << 2) as u32))
                }
                Kind::Int(v) => match IntClass::of(*v) {
                    IntClass::I16 if width == 16 => Arg::Imm(raw as u16 as i16 as i32),
                    _ => Arg::Imm(raw as i32),
                },
                _ => Arg::Imm(raw as i32),
            }
        })
        .collect();
    Some(Decoded { mnemonic: p.mnemonic, args, memory: p.memory, basic: p.basic })
}

/// Whether the mnemonic's immediate is a bit pattern (shown in hex in [`Style::Named`]).
fn logical(mnemonic: &str) -> bool {
    matches!(mnemonic, "andi" | "ori" | "xori" | "lui")
}

/// The text of a decoded instruction.
pub fn text(d: &Decoded, style: Style) -> String {
    let named = style == Style::Named;
    let arg = |a: &Arg| match (*a, named) {
        (Arg::Reg(r), false) | (Arg::Cp0(r), _) => format!("${r}"),
        (Arg::Reg(r), true) => format!("${}", REG_NAMES[r as usize]),
        (Arg::Fpr(r), _) => format!("$f{r}"),
        (Arg::Imm(v), true) if logical(d.mnemonic) => format!("{:#x}", v as u32),
        (Arg::Imm(v), _) => v.to_string(),
        (Arg::Branch(off, _), false) => off.to_string(),
        (Arg::Branch(_, t), true) | (Arg::Jump(t), true) => format!("0x{t:08x}"),
        (Arg::Jump(t), false) => (t as i32).to_string(),
    };
    let sep = if named { ", " } else { "," };
    let parts: Vec<String> = d.args.iter().map(arg).collect();
    let ops = if d.memory && parts.len() >= 2 {
        let n = parts.len();
        let mut head = parts[..n - 2].to_vec();
        head.push(format!("{}({})", parts[n - 2], parts[n - 1]));
        head.join(sep)
    } else {
        parts.join(sep)
    };
    if ops.is_empty() { d.mnemonic.to_string() } else { format!("{} {ops}", d.mnemonic) }
}

/// The word at `addr` as text (`.word 0x...` when it is not an instruction).
pub fn disassemble(word: u32, addr: u32, style: Style) -> String {
    match decode(word, addr) {
        Some(d) => text(&d, style),
        None => format!(".word 0x{word:08x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mars_and_named_notation() {
        let at = 0x0040_0000;
        let cases: [(u32, &str, &str); 12] = [
            (0x0000_0000, "nop", "nop"),
            (0x2149_ff9c, "addi $9,$10,-100", "addi $t1, $t2, -100"),
            (0x8d49_ff9c, "lw $9,-100($10)", "lw $t1, -100($t2)"),
            (0x112a_fffb, "beq $9,$10,-5", "beq $t1, $t2, 0x003ffff0"),
            (0x0810_0000, "j 4194304", "j 0x00400000"),
            (0x3c01_1001, "lui $1,4097", "lui $at, 0x1001"),
            (0x000a_4a80, "sll $9,$10,10", "sll $t1, $t2, 10"),
            (0x4009_4000, "mfc0 $9,$8", "mfc0 $t1, $8"),
            (0x0000_000c, "syscall", "syscall"),
            (0x0000_190d, "break 100", "break 100"),
            (0x7140_4820, "clz $9,$10", "clz $t1, $t2"),
            (0x4200_0018, "eret", "eret"),
        ];
        for (w, mars, named) in cases {
            assert_eq!(disassemble(w, at, Style::Mars), mars, "{w:08x}");
            assert_eq!(disassemble(w, at, Style::Named), named, "{w:08x}");
        }
        assert_eq!(disassemble(0x4600_0000, at, Style::Mars), "add.s $f0,$f0,$f0", "coprocessor 1");
        assert_eq!(disassemble(0x4601_0132, at, Style::Named), "c.eq.s 1, $f0, $f1");
        assert_eq!(disassemble(0x4505_0004, at, Style::Named), "bc1t 1, 0x00400014");
        assert_eq!(disassemble(0xFC00_0000, at, Style::Named), ".word 0xfc000000");
        let d = decode(0x0c10_0004, at).unwrap();
        assert_eq!((d.mnemonic, d.args.as_slice()), ("jal", &[Arg::Jump(0x0040_0010)][..]));
    }
}
