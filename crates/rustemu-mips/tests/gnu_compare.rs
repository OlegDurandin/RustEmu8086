//! Encodings of the basic instructions compared with the GNU assembler (binutils), an independent
//! reference besides MARS.
//!
//! Runs when `mipsel-linux-gnu-as` (or `mips-linux-gnu-as`, used with `-EL`) is on the PATH, with
//! its `ld` and `objcopy` (Homebrew: `brew install mipsel-linux-gnu-binutils`); otherwise the test
//! passes without doing anything. `GNU_ITERATIONS` sets the instances of each form (default 40).
//!
//! Every basic instruction form gets random operands; the program is assembled by the core's
//! assembler and by gas (`.set noreorder`, `noat`, `nomacro`, linked at 0x00400000 as MARS loads
//! programs) and compared word by word. Two encodings of MARS differ from GNU's and are checked
//! as MARS's: `clz` / `clo` have rt = 0 (GNU and MIPS32: rt = rd), and `break n` keeps the 20-bit
//! code whole (GNU's `break n` is MARS's `break n << 10`; the GNU source here says `break n >> 10, n & 1023`).
//! The GNU source also writes `div rs, rt` as `div $zero, rs, rt` (GNU's two-operand `div` is a
//! macro) and the coprocessor 0 register of `mfc0` / `mtc0` by number.
//! Pseudo-instructions are MARS's own (GNU's macros differ) and are compared with MARS instead
//! (`mars_compare.rs`).

use std::path::Path;
use std::process::Command;

use rustemu_mips::asm::{Def, IntClass, Operand, Table, assemble};
use rustemu_mips::cpu::REG_NAMES;

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

/// The GNU tools: the prefix (`mipsel-linux-gnu-`) and whether `-EL` is needed.
fn gnu_tools() -> Option<(String, bool)> {
    for (prefix, el) in [("mipsel-linux-gnu-", false), ("mips-linux-gnu-", true)] {
        if Command::new(format!("{prefix}as")).arg("--version").output().is_ok_and(|o| o.status.success()) {
            return Some((prefix.to_string(), el));
        }
    }
    None
}

/// A register: by name or by number, at random.
fn reg(g: &mut Rng) -> String {
    let r = g.below(32) as usize;
    if g.below(2) == 0 { format!("${}", REG_NAMES[r]) } else { format!("${r}") }
}

fn int(g: &mut Rng, class: IntClass) -> i64 {
    match class {
        IntClass::I5 => g.below(32) as i64,
        IntClass::I16U => g.below(65536) as i64,
        IntClass::I16 => g.below(65536) as i64 - 32768,
        IntClass::I32 => g.next() as i32 as i64,
    }
}

/// Instructions whose integer operand is a condition flag of coprocessor 1 (GNU: `$fcc1`).
fn takes_cc(mnemonic: &str) -> bool {
    mnemonic.starts_with("c.")
        || mnemonic.starts_with("bc1")
        || mnemonic.starts_with("movf")
        || mnemonic.starts_with("movt")
}

/// One instance of a form: MARS source and GNU source.
fn instance(g: &mut Rng, mnemonic: &str, ops: &[Operand], labels: usize) -> (String, String) {
    let mut parts = Vec::new();
    // The same operands in GNU's syntax, where they differ.
    let mut gnu_parts = Vec::new();
    for (i, &op) in ops.iter().enumerate() {
        let (part, gnu_part) = match op {
            // GNU wants even registers in double operations; MARS checks them only when running.
            Operand::FReg => {
                let r = format!("$f{}", g.below(16) * 2);
                (r.clone(), r)
            }
            Operand::Int(_) if takes_cc(mnemonic) => {
                let cc = g.below(8);
                (cc.to_string(), format!("$fcc{cc}"))
            }
            op => {
                let p = match op {
                    // The second operand of mfc0 / mtc0 is a coprocessor 0 register: GNU wants its number.
                    Operand::Reg if i == 1 && matches!(mnemonic, "mfc0" | "mtc0") => format!("${}", g.below(32)),
                    Operand::Reg => reg(g),
                    Operand::Int(c) => int(g, c).to_string(),
                    Operand::Label => format!("L{}", g.below(labels as u64)),
                    Operand::LParen => "(".into(),
                    Operand::RParen => ")".into(),
                    Operand::Plus => "+".into(),
                    Operand::FReg => unreachable!(),
                };
                (p.clone(), p)
            }
        };
        parts.push(part);
        gnu_parts.push(gnu_part);
    }
    let join = |parts: &[String]| {
        let mut s = mnemonic.to_string();
        for (i, p) in parts.iter().enumerate() {
            let sep = if i == 0 {
                " "
            } else if p == "(" || p == ")" || parts[i - 1] == "(" {
                ""
            } else {
                ", "
            };
            s += sep;
            s += p;
        }
        s
    };
    let mars = join(&parts);
    // GNU's movf / movt always name the condition flag.
    if (mnemonic.starts_with("movf") || mnemonic.starts_with("movt"))
        && !gnu_parts.iter().any(|p| p.starts_with("$fcc"))
    {
        gnu_parts.push("$fcc0".into());
    }
    let gnu_text = join(&gnu_parts);
    let gnu = if mnemonic == "break" && !parts.is_empty() {
        let n: u32 = parts[0].parse().unwrap();
        format!("break {}, {}", n >> 10, n & 1023)
    } else if matches!(mnemonic, "div" | "divu") {
        // GNU's two-operand div is a macro (with a zero check); the instruction is `div $zero, rs, rt`.
        format!("{mnemonic} $zero, {}, {}", parts[0], parts[1])
    } else {
        gnu_text
    };
    (mars, gnu)
}

/// `jalr` with rd = rs or rs = $ra, `bgezal` / `bltzal` with $ra: unpredictable in MIPS32.
fn unpredictable(mnemonic: &str, src: &str) -> bool {
    let regs: Vec<u32> = rustemu_mips::asm::tokenize(src)
        .unwrap()
        .iter()
        .filter_map(|t| match t.kind {
            rustemu_mips::asm::Kind::RegName(r) | rustemu_mips::asm::Kind::RegNum(r) => Some(r),
            _ => None,
        })
        .collect();
    match (mnemonic, regs.as_slice()) {
        ("jalr", [rs]) => *rs == 31,
        ("jalr", [rd, rs]) => rd == rs,
        ("bgezal" | "bltzal", [rs]) => *rs == 31,
        _ => false,
    }
}

/// Machine words of `src` assembled by gas and linked at 0x00400000.
fn gas(prefix: &str, el: bool, dir: &Path, src: &str) -> Result<Vec<u32>, String> {
    let (s, o, elf, bin) = (dir.join("g.s"), dir.join("g.o"), dir.join("g.elf"), dir.join("g.bin"));
    std::fs::write(&s, src).unwrap();
    let run = |tool: &str, args: &[&std::ffi::OsStr]| -> Result<(), String> {
        let out = Command::new(format!("{prefix}{tool}")).args(args).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).into_owned()) }
    };
    let mut as_args: Vec<&std::ffi::OsStr> = vec!["-march=mips32r2".as_ref(), "-mabi=32".as_ref(), "-O0".as_ref()];
    if el {
        as_args.push("-EL".as_ref());
    }
    as_args.extend(["-o".as_ref(), o.as_os_str(), s.as_os_str()]);
    run("as", &as_args)?;
    // Only the code, at the address MARS loads it.
    let script = dir.join("g.ld");
    std::fs::write(
        &script,
        "SECTIONS { . = 0x400000; .text : { *(.text) } /DISCARD/ : { *(.MIPS.abiflags) *(.reginfo) *(.pdr) *(.gnu.attributes) } }",
    )
    .unwrap();
    run(
        "ld",
        &[
            "-e".as_ref(),
            "0x400000".as_ref(),
            "-T".as_ref(),
            script.as_os_str(),
            "-o".as_ref(),
            elf.as_os_str(),
            o.as_os_str(),
        ],
    )?;
    run(
        "objcopy",
        &["-O".as_ref(), "binary".as_ref(), "-j".as_ref(), ".text".as_ref(), elf.as_os_str(), bin.as_os_str()],
    )?;
    let bytes = std::fs::read(&bin).map_err(|e| e.to_string())?;
    Ok(bytes.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

#[test]
fn basic_instructions_match_gnu_as() {
    let Some((prefix, el)) = gnu_tools() else {
        eprintln!("mipsel-linux-gnu-as not found: skipping the comparison with binutils");
        return;
    };
    let iterations: usize = std::env::var("GNU_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let dir = std::env::temp_dir().join(format!("rustemu_gnu_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut g = Rng(0x0BAD_5EED);
    let table = Table::get();
    let mut mnemonics: Vec<&str> = table.mnemonics().collect();
    mnemonics.sort();
    // (MARS source, GNU source, mnemonic)
    let mut lines = Vec::new();
    for m in mnemonics {
        for form in table.forms(m).unwrap() {
            let Def::Basic(b) = &form.def else { continue };
            for _ in 0..iterations {
                let _: &rustemu_mips::asm::Basic = b;
                // GNU refuses the combinations MIPS32 calls unpredictable (MARS accepts them).
                let (mars, gnu) = loop {
                    let (mars, gnu) = instance(&mut g, m, &form.operands, 8);
                    if !unpredictable(m, &mars) {
                        break (mars, gnu);
                    }
                };
                lines.push((mars, gnu, m));
            }
        }
    }
    for i in (1..lines.len()).rev() {
        lines.swap(i, g.below(i as u64 + 1) as usize);
    }
    // In programs of up to 3000 instructions with eight labels each (branches reach them).
    let mut failures = 0;
    for chunk in lines.chunks(3000) {
        failures += compare(&prefix, el, &dir, chunk);
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(failures, 0, "{failures} of {} instructions differ from gas", lines.len());
}

/// Assembles `lines` (MARS source, GNU source, mnemonic) both ways; the number of differences.
fn compare(prefix: &str, el: bool, dir: &Path, lines: &[(String, String, &str)]) -> usize {
    let step = lines.len().div_ceil(8);
    let mut mars_src = String::from(".text\n");
    let mut gnu_src = String::from("    .set noreorder\n    .set noat\n    .set nomacro\n    .text\n");
    for (i, (mars, gnu, _)) in lines.iter().enumerate() {
        if i % step == 0 {
            mars_src += &format!("L{}:\n", i / step);
            gnu_src += &format!("L{}:\n", i / step);
        }
        mars_src += &format!("    {mars}\n");
        gnu_src += &format!("    {gnu}\n");
    }
    let ours: Vec<u32> = assemble(&mars_src)
        .unwrap_or_else(|e| panic!("{}", e.iter().take(5).map(|e| e.to_string()).collect::<Vec<_>>().join("\n")))
        .statements
        .iter()
        .flat_map(|s| s.words.clone())
        .collect();
    let gnu = gas(prefix, el, dir, &gnu_src).unwrap_or_else(|e| panic!("gas: {e}"));
    assert_eq!(ours.len(), lines.len(), "one word per basic instruction");
    assert!(gnu.len() >= lines.len(), "gas produced {} words for {} instructions", gnu.len(), lines.len());
    let mut failures = 0;
    for (k, (mars, gnu_line, m)) in lines.iter().enumerate() {
        let (a, b) = (ours[k], gnu[k]);
        let same = if matches!(*m, "clz" | "clo") {
            // MARS: rt = 0; GNU: rt = rd.
            a & !(31 << 16) == b & !(31 << 16) && a & (31 << 16) == 0
        } else {
            a == b
        };
        if !same {
            failures += 1;
            if failures <= 20 {
                eprintln!("{mars:<36} ours {a:08x}   gas {b:08x}   ({gnu_line})");
            }
        }
    }
    failures
}
