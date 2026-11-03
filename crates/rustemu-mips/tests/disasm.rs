//! The MIPS disassembler: back to the assembler's text for every form, and the same instructions
//! and operand values as GNU objdump (when `mipsel-linux-gnu-objdump` is installed).

use std::process::Command;

use rustemu_mips::asm::{BASIC, IntClass, Kind, assemble, tokenize};
use rustemu_mips::cpu::REG_NAMES;
use rustemu_mips::disasm::{Arg, Style, decode, disassemble};
use rustemu_mips::memory::TEXT_BASE;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A source line of a basic form with random operands (labels L0..L3).
fn instance(g: &mut Rng, example: &str) -> String {
    let tokens = tokenize(example).unwrap();
    let mnemonic = &tokens[0].text;
    let mut out = format!("{mnemonic} ");
    let mut prev_paren = true;
    for (k, t) in tokens[1..].iter().enumerate() {
        let piece = match &t.kind {
            Kind::LParen | Kind::RParen => {
                out += &t.text;
                prev_paren = true;
                continue;
            }
            Kind::RegName(_) | Kind::RegNum(_) if k == 1 && matches!(mnemonic.as_str(), "mfc0" | "mtc0") => {
                format!("${}", g.below(32))
            }
            Kind::RegName(_) | Kind::RegNum(_) => format!("${}", REG_NAMES[g.below(32) as usize]),
            Kind::FpReg(_) => format!("$f{}", g.below(32)),
            // The condition flag of coprocessor 1 (3 bits).
            Kind::Int(_)
                if mnemonic.starts_with("c.") || mnemonic.starts_with("bc1") || mnemonic.starts_with("mov") =>
            {
                g.below(8).to_string()
            }
            Kind::Int(v) => match IntClass::of(*v) {
                IntClass::I5 => g.below(32).to_string(),
                IntClass::I16U => g.below(65536).to_string(),
                _ => (g.below(65536) as i64 - 32768).to_string(),
            },
            _ => format!("L{}", g.below(4)),
        };
        if !prev_paren {
            out += ",";
        }
        out += &piece;
        prev_paren = false;
    }
    out
}

/// Every basic form, `n` times, in one program with four labels; (source line, word, Basic text).
fn program(n: usize, seed: u64) -> Vec<(String, u32, String)> {
    let mut g = Rng(seed);
    let mut lines = Vec::new();
    for b in BASIC {
        for _ in 0..n {
            lines.push(instance(&mut g, b.example));
        }
    }
    let step = lines.len().div_ceil(4);
    let mut src = String::from(".text\n");
    for (i, l) in lines.iter().enumerate() {
        if i % step == 0 {
            src += &format!("L{}:\n", i / step);
        }
        src += &format!("    {l}\n");
    }
    let p = assemble(&src).unwrap_or_else(|e| panic!("{:?}", &e[..e.len().min(3)]));
    p.statements.iter().zip(lines).map(|(s, l)| (l, s.words[0], s.basic[0].clone())).collect()
}

#[test]
fn disassembly_is_the_basic_column() {
    for (k, (line, word, basic)) in program(30, 0xD15A).into_iter().enumerate() {
        let addr = TEXT_BASE + 4 * k as u32;
        let ours = disassemble(word, addr, Style::Mars);
        // Words two forms share are shown as the first one, as in MARS.
        let alias = (word == 0 && ours == "nop")
            || (ours.starts_with("jalr $31,") && basic.starts_with("jalr $"))
            || (ours == "break 0" && basic == "break")
            // A condition flag 0 is the form without the flag (bc1t, c.eq.s, movf...).
            || cc0_alias(&ours, &basic);
        assert!(ours == basic || alias, "{line}: {word:08x} -> {ours}, the assembler says {basic}");
        // The named notation decodes the same instruction.
        let named = disassemble(word, addr, Style::Named);
        assert_eq!(named.split(' ').next(), ours.split(' ').next());
    }
}

/// `ours` is the form without a condition flag of `basic`, whose flag is 0: `movf $31,$12` for
/// `movf $31,$12,0`, `c.eq.s $f0,$f1` for `c.eq.s 0,$f0,$f1`, `bc1t 5` for `bc1t 0,5`.
fn cc0_alias(ours: &str, basic: &str) -> bool {
    let Some((m, args)) = basic.split_once(' ') else { return false };
    let without = if let Some(rest) = args.strip_prefix("0,") {
        format!("{m} {rest}")
    } else if let Some(rest) = args.strip_suffix(",0") {
        format!("{m} {rest}")
    } else {
        return false;
    };
    ours == without
}

/// Numbers of an objdump operand string: registers, decimal and hex values.
fn objdump_values(ops: &str) -> Vec<i64> {
    ops.split([',', '(', ')'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if let Some(cc) = s.strip_prefix("$fcc") {
                cc.parse().unwrap()
            } else if let Some(r) = s.strip_prefix("$f") {
                r.parse().unwrap()
            } else if let Some(r) = s.strip_prefix('$') {
                r.parse().unwrap()
            } else if let Some(h) = s.strip_prefix("0x") {
                i64::from_str_radix(h, 16).unwrap()
            } else if let Some(h) = s.strip_prefix("-0x") {
                -i64::from_str_radix(h, 16).unwrap()
            } else {
                s.parse().unwrap_or_else(|_| panic!("objdump operand {s}"))
            }
        })
        .collect()
}

#[test]
fn decoding_matches_gnu_objdump() {
    let objdump = "mipsel-linux-gnu-objdump";
    if !Command::new(objdump).arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("{objdump} not found: skipping the comparison with binutils");
        return;
    }
    let mut words: Vec<(String, u32)> = program(40, 0x0B1D).into_iter().map(|(l, w, _)| (l, w)).collect();
    // clz / clo as GNU encodes them (rt = rd); the decoder does not look at rt.
    for (l, w) in &mut words {
        if l.starts_with("clz") || l.starts_with("clo") {
            *w |= (*w >> 11 & 31) << 16;
        }
    }
    let dir = std::env::temp_dir().join(format!("rustemu_objdump_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("code.bin");
    std::fs::write(&bin, words.iter().flat_map(|(_, w)| w.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    let out = Command::new(objdump)
        .args([
            "-D",
            "-z",
            "-b",
            "binary",
            "-m",
            "mips:isa32r2",
            "-EL",
            "-M",
            "no-aliases,gpr-names=numeric,cp0-names=numeric",
        ])
        .arg(format!("--adjust-vma={TEXT_BASE:#x}"))
        .arg(&bin)
        .output()
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut gnu = std::collections::HashMap::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 3
            && let Some(addr) = cols[0].trim().strip_suffix(':').and_then(|a| u32::from_str_radix(a, 16).ok())
        {
            gnu.insert(addr, (cols[2].trim().to_string(), cols.get(3).map_or("", |s| s.trim()).to_string()));
        }
    }
    let mut failures = 0;
    for (k, (line, word)) in words.iter().enumerate() {
        let addr = TEXT_BASE + 4 * k as u32;
        let d = decode(*word, addr).unwrap();
        let (mut mnemonic, ops) = gnu.get(&addr).cloned().unwrap_or_default();
        let mut theirs = objdump_values(&ops);
        let ours: Vec<i64> = d
            .args
            .iter()
            .map(|a| match *a {
                Arg::Reg(r) | Arg::Cp0(r) | Arg::Fpr(r) => r as i64,
                Arg::Imm(v) => v as i64,
                Arg::Branch(_, t) | Arg::Jump(t) => t as i64,
            })
            .collect();
        // Other spellings of objdump: nop is sll $0,$0,0; jalr $31,rs is jalr rs; break's
        // 20-bit code is printed as two 10-bit halves; unsigned immediates of andi/ori/xori, lui
        // and the logical forms are in hex (the values compare equal).
        if d.mnemonic == "nop" && mnemonic == "sll" {
            mnemonic = "nop".into();
            theirs.clear();
        }
        // objdump writes div / divu with a $0 destination, and sub / subu from $0 as neg / negu.
        if matches!(mnemonic.as_str(), "div" | "divu") && theirs.len() == 3 && theirs[0] == 0 {
            theirs.remove(0);
        }
        if let Some(sub) = mnemonic.strip_prefix("neg").filter(|_| matches!(mnemonic.as_str(), "neg" | "negu")) {
            mnemonic = format!("sub{sub}");
            theirs.insert(1, 0);
        }
        // objdump names condition flag 0, which the forms without a flag leave out.
        if theirs.len() == ours.len() + 1 && ops.contains("$fcc0") {
            if ops.starts_with("$fcc0") {
                theirs.remove(0);
            } else {
                theirs.pop();
            }
        }
        if mnemonic == "jalr" && theirs.len() == 1 {
            theirs.insert(0, 31);
        }
        if mnemonic == "break" {
            let code = theirs.first().copied().unwrap_or(0) << 10 | theirs.get(1).copied().unwrap_or(0);
            theirs = if d.args.is_empty() { vec![] } else { vec![code] };
        }
        if mnemonic != d.mnemonic || theirs != ours {
            failures += 1;
            if failures <= 20 {
                eprintln!("{line:<32} {word:08x}: ours {} {ours:?}, objdump {mnemonic} {ops}", d.mnemonic);
            }
        }
    }
    assert_eq!(failures, 0, "{failures} of {} instructions decode differently from objdump", words.len());
}
