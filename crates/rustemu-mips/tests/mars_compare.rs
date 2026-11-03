//! Random programs run on the core and on the real MARS, compared register by register.
//!
//! Runs when the environment variable `MARS` names MARS: `Mars.jar`, or a directory with the
//! compiled `Mars.class` (a build of github.com/dpetersanderson/MARS). `MARS_PROGRAMS` sets the
//! number of programs (default 200), `MARS_SEED` the first seed. Without `MARS` the test passes
//! without doing anything.
//!
//! Each program sets registers to random values and runs random integer instructions: arithmetic
//! (overflow included), HI/LO, shifts, logic, comparisons, loads and stores (aligned and
//! `lwl`/`lwr`/`swl`/`swr`) in the data segment, forward branches and traps. The registers,
//! HI/LO, the first 256 bytes of `.data` and the exception message (if any) must be the same.

use std::path::Path;
use std::process::Command;

use rustemu_mips::memory::{DATA_BASE, TEXT_BASE};
use rustemu_mips::{Cpu, Memory, Step};

/// Registers the programs compute with ($a0 holds the data address, $k0/$k1 get HI/LO).
const POOL: [u32; 12] = [8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19];
const NAMES: [&str; 32] = [
    "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "s0", "s1", "s2",
    "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp", "fp", "ra",
];
const A0: u32 = 4;

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
    fn reg(&mut self) -> u32 {
        POOL[self.below(POOL.len() as u64) as usize]
    }
    fn imm(&mut self) -> i32 {
        match self.below(4) {
            0 => self.below(16) as i32 - 8,
            1 => i16::MIN as i32 + self.below(4) as i32,
            2 => i16::MAX as i32 - self.below(4) as i32,
            _ => self.next() as i16 as i32,
        }
    }
}

fn r(rs: u32, rt: u32, rd: u32, shamt: u32, funct: u32) -> u32 {
    rs << 21 | rt << 16 | rd << 11 | shamt << 6 | funct
}
fn i(op: u32, rs: u32, rt: u32, imm: i32) -> u32 {
    op << 26 | rs << 21 | rt << 16 | (imm as u32 & 0xFFFF)
}

/// One line of the program: MARS source and the machine word, or a label.
enum Line {
    Ins(String, u32),
    /// A forward branch: source with a label name, the word without the offset, the label.
    Branch(String, u32, usize),
    /// A forward jump (`j` / `jal`): source, the word without the target, the label.
    Jump(String, u32, usize),
    /// `la reg, label` (MARS: `lui $1, hi` + `ori reg, $1, lo`).
    La(u32, usize),
    Label(usize),
}

fn n(r: u32) -> String {
    format!("${}", NAMES[r as usize])
}

/// A random instruction.
fn instruction(g: &mut Rng, labels: &mut usize, lines: &mut Vec<Line>) {
    let (d, s, t) = (g.reg(), g.reg(), g.reg());
    let three: [(&str, u32, u32); 13] = [
        ("add", 0, 0x20),
        ("addu", 0, 0x21),
        ("sub", 0, 0x22),
        ("subu", 0, 0x23),
        ("and", 0, 0x24),
        ("or", 0, 0x25),
        ("xor", 0, 0x26),
        ("nor", 0, 0x27),
        ("slt", 0, 0x2A),
        ("sltu", 0, 0x2B),
        ("movz", 0, 0x0A),
        ("movn", 0, 0x0B),
        ("mul", 0x1C, 0x02),
    ];
    let line = match g.below(12) {
        0 | 1 => {
            let (m, op, f) = three[g.below(three.len() as u64) as usize];
            Line::Ins(format!("{m} {}, {}, {}", n(d), n(s), n(t)), op << 26 | r(s, t, d, 0, f))
        }
        2 => {
            let (m, f) = [("sllv", 0x04), ("srlv", 0x06), ("srav", 0x07)][g.below(3) as usize];
            Line::Ins(format!("{m} {}, {}, {}", n(d), n(t), n(s)), r(s, t, d, 0, f))
        }
        3 => {
            let (m, f) = [("sll", 0x00), ("srl", 0x02), ("sra", 0x03)][g.below(3) as usize];
            let sh = g.below(32) as u32;
            Line::Ins(format!("{m} {}, {}, {sh}", n(d), n(t)), r(0, t, d, sh, f))
        }
        4 | 5 => {
            let (m, op, signed) = [
                ("addi", 0x08, true),
                ("addiu", 0x09, true),
                ("slti", 0x0A, true),
                ("sltiu", 0x0B, true),
                ("andi", 0x0C, false),
                ("ori", 0x0D, false),
                ("xori", 0x0E, false),
                ("lui", 0x0F, false),
            ][g.below(8) as usize];
            let imm = if signed { g.imm() } else { g.below(0x10000) as i32 };
            if m == "lui" {
                Line::Ins(format!("lui {}, {imm}", n(t)), i(op, 0, t, imm))
            } else {
                Line::Ins(format!("{m} {}, {}, {imm}", n(t), n(s),), i(op, s, t, imm))
            }
        }
        6 => {
            let (m, op, f) = [
                ("mult", 0, 0x18),
                ("multu", 0, 0x19),
                ("div", 0, 0x1A),
                ("divu", 0, 0x1B),
                ("madd", 0x1C, 0x00),
                ("maddu", 0x1C, 0x01),
                ("msub", 0x1C, 0x04),
                ("msubu", 0x1C, 0x05),
            ][g.below(8) as usize];
            Line::Ins(format!("{m} {}, {}", n(s), n(t)), op << 26 | r(s, t, 0, 0, f))
        }
        7 => match g.below(6) {
            0 => Line::Ins(format!("mfhi {}", n(d)), r(0, 0, d, 0, 0x10)),
            1 => Line::Ins(format!("mflo {}", n(d)), r(0, 0, d, 0, 0x12)),
            2 => Line::Ins(format!("mthi {}", n(s)), r(s, 0, 0, 0, 0x11)),
            3 => Line::Ins(format!("mtlo {}", n(s)), r(s, 0, 0, 0, 0x13)),
            4 => Line::Ins(format!("clz {}, {}", n(d), n(s)), 0x1C << 26 | r(s, 0, d, 0, 0x20)),
            _ => Line::Ins(format!("clo {}, {}", n(d), n(s)), 0x1C << 26 | r(s, 0, d, 0, 0x21)),
        },
        8 | 9 => {
            let (m, op, align) = [
                ("lb", 0x20, 1),
                ("lbu", 0x24, 1),
                ("lh", 0x21, 2),
                ("lhu", 0x25, 2),
                ("lw", 0x23, 4),
                ("sb", 0x28, 1),
                ("sh", 0x29, 2),
                ("sw", 0x2B, 4),
                ("lwl", 0x22, 1),
                ("lwr", 0x26, 1),
                ("swl", 0x2A, 1),
                ("swr", 0x2E, 1),
                ("ll", 0x30, 4),
                ("sc", 0x38, 4),
            ][g.below(14) as usize];
            // Mostly aligned; sometimes not, for the address exceptions.
            let mut off = g.below(256) as i32;
            if g.below(16) != 0 {
                off -= off % align;
            }
            Line::Ins(format!("{m} {}, {off}({})", n(t), n(A0)), i(op, A0, t, off))
        }
        10 => {
            let label = *labels;
            *labels += 1;
            let (text, word) = match g.below(11) {
                0 => (format!("beq {}, {}", n(s), n(t)), i(0x04, s, t, 0)),
                1 => (format!("bne {}, {}", n(s), n(t)), i(0x05, s, t, 0)),
                2 => (format!("blez {}", n(s)), i(0x06, s, 0, 0)),
                3 => (format!("bgtz {}", n(s)), i(0x07, s, 0, 0)),
                4 => (format!("bltz {}", n(s)), i(0x01, s, 0, 0)),
                5 => (format!("bgez {}", n(s)), i(0x01, s, 1, 0)),
                // MARS links only when the branch is taken.
                6 => (format!("bltzal {}", n(s)), i(0x01, s, 0x10, 0)),
                7 => (format!("bgezal {}", n(s)), i(0x01, s, 0x11, 0)),
                8 => (String::new(), 0x02 << 26),
                9 => (String::new(), 0x03 << 26),
                _ => (String::new(), u32::MAX),
            };
            match word {
                u32::MAX => {
                    // jalr through a register loaded with la.
                    lines.push(Line::La(25, label));
                    lines.push(Line::Ins(format!("jalr {}, $t9", n(d)), r(25, 0, d, 0, 0x09)));
                }
                w if text.is_empty() => {
                    let m = if w >> 26 == 2 { "j" } else { "jal" };
                    lines.push(Line::Jump(format!("{m} L{label}"), w, label));
                }
                w => lines.push(Line::Branch(format!("{text}, L{label}"), w, label)),
            }
            for _ in 0..=g.below(2) {
                instruction(g, labels, lines);
            }
            Line::Label(label)
        }
        // Traps end the program, so they are rare (one program in four or so has one that fires).
        _ if g.below(10) != 0 => {
            let (m, op, f) = three[g.below(three.len() as u64) as usize];
            Line::Ins(format!("{m} {}, {}, {}", n(d), n(s), n(t)), op << 26 | r(s, t, d, 0, f))
        }
        _ => {
            let (m, f) = [("teq", 0x34), ("tne", 0x36), ("tge", 0x30), ("tgeu", 0x31), ("tlt", 0x32), ("tltu", 0x33)]
                [g.below(6) as usize];
            Line::Ins(format!("{m} {}, {}", n(s), n(t)), r(s, t, 0, 0, f))
        }
    };
    lines.push(line);
}

/// A random program: MARS source, machine code.
fn program(seed: u64) -> (String, Vec<u32>) {
    let mut g = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut lines = vec![Line::Ins("lui $a0, 0x1001".into(), i(0x0F, 0, A0, 0x1001))];
    for &reg in &POOL {
        let v = g.next() as u32;
        let v = if g.below(4) == 0 { [0, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff][g.below(5) as usize] } else { v };
        lines.push(Line::Ins(format!("lui {}, {}", n(reg), v >> 16), i(0x0F, 0, reg, (v >> 16) as i32)));
        lines.push(Line::Ins(
            format!("ori {}, {}, {}", n(reg), n(reg), v & 0xFFFF),
            i(0x0D, reg, reg, (v & 0xFFFF) as i32),
        ));
    }
    let mut labels = 0;
    for _ in 0..40 {
        instruction(&mut g, &mut labels, &mut lines);
    }
    lines.push(Line::Ins("mfhi $k0".into(), r(0, 0, 26, 0, 0x10)));
    lines.push(Line::Ins("mflo $k1".into(), r(0, 0, 27, 0, 0x12)));
    // Addresses: instructions are 4 bytes, labels take none.
    let mut at = Vec::new();
    let mut label_at = vec![0u32; labels];
    let mut pc = TEXT_BASE;
    for l in &lines {
        at.push(pc);
        match l {
            Line::Label(k) => label_at[*k] = pc,
            Line::La(..) => pc += 8,
            _ => pc += 4,
        }
    }
    let mut src = String::from(".data\n.space 256\n.text\n");
    let mut code = Vec::new();
    for (l, &pc) in lines.iter().zip(&at) {
        match l {
            Line::Ins(t, w) => {
                src += &format!("    {t}\n");
                code.push(*w);
            }
            Line::Branch(t, w, k) => {
                src += &format!("    {t}\n");
                let off = (label_at[*k] as i64 - (pc as i64 + 4)) / 4;
                code.push(w | (off as u32 & 0xFFFF));
            }
            Line::Jump(t, w, k) => {
                src += &format!("    {t}\n");
                code.push(w | (label_at[*k] >> 2 & 0x03FF_FFFF));
            }
            Line::La(reg, k) => {
                src += &format!("    la {}, L{k}\n", n(*reg));
                let a = label_at[*k];
                code.push(i(0x0F, 0, 1, (a >> 16) as i32));
                code.push(i(0x0D, 1, *reg, (a & 0xFFFF) as i32));
            }
            Line::Label(k) => src += &format!("L{k}:\n"),
        }
    }
    (src, code)
}

/// What a run ends with: the registers, HI/LO, 64 words of `.data`, the exception message.
#[derive(Debug, PartialEq)]
struct Outcome {
    regs: Vec<(String, u32)>,
    data: Vec<u32>,
    error: Option<String>,
}

fn shown_regs() -> Vec<usize> {
    (2..32).filter(|&r| r != 1).collect()
}

fn run_core(code: &[u32]) -> Outcome {
    let mut mem = Memory::new();
    let bytes: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    mem.load(TEXT_BASE, &bytes);
    mem.add_code(TEXT_BASE, TEXT_BASE + bytes.len() as u32);
    let mut cpu = Cpu::new();
    let mut error = None;
    for _ in 0..10_000 {
        match cpu.step(&mut mem) {
            Step::Ok => {}
            Step::Exception(e) => {
                error = Some(e.report());
                break;
            }
            _ => break,
        }
    }
    Outcome {
        regs: shown_regs().into_iter().map(|r| (NAMES[r].to_string(), cpu.regs[r])).collect(),
        data: (0..64).map(|k| mem.peek32(DATA_BASE + 4 * k)).collect(),
        error,
    }
}

fn run_mars(mars: &Path, dir: &Path, src: &str) -> Outcome {
    let file = dir.join("prog.asm");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new("java");
    if mars.extension().is_some_and(|e| e == "jar") {
        cmd.arg("-jar").arg(mars);
    } else {
        cmd.arg("-cp").arg(mars).arg("Mars");
    }
    cmd.arg("nc").arg("10000");
    for r in shown_regs() {
        cmd.arg(format!("${}", NAMES[r]));
    }
    cmd.arg(format!("0x{:08x}-0x{:08x}", DATA_BASE, DATA_BASE + 252)).arg(&file);
    let out = cmd.output().expect("java runs");
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let mut regs = Vec::new();
    let mut data = Vec::new();
    let mut error = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('$')
            && let Some((name, value)) = rest.split_once('\t')
        {
            regs.push((name.to_string(), u32::from_str_radix(value.trim().trim_start_matches("0x"), 16).unwrap()));
        } else if line.starts_with("Mem[") {
            data.extend(
                line.split('\t')
                    .skip(1)
                    .filter(|w| w.starts_with("0x"))
                    .map(|w| u32::from_str_radix(&w[2..], 16).unwrap()),
            );
        } else if let Some(i) = line.find("Runtime exception at ").or_else(|| line.find("invalid program counter")) {
            error = Some(line[i..].trim().to_string());
        } else if line.contains("Assemble: operation completed with errors") {
            panic!("MARS did not assemble the program:\n{text}\n{src}");
        }
    }
    Outcome { regs, data, error }
}

#[test]
fn random_programs_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let count: u64 = std::env::var("MARS_PROGRAMS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let first: u64 = std::env::var("MARS_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let dir = std::env::temp_dir().join(format!("rustemu_mars_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut errors = 0;
    for seed in first..first + count {
        let (src, code) = program(seed);
        let (ours, theirs) = (run_core(&code), run_mars(&mars, &dir, &src));
        if ours != theirs {
            errors += 1;
            eprintln!("seed {seed}: differs\n--- program\n{src}--- core\n{ours:?}\n--- MARS\n{theirs:?}\n");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(errors, 0, "{errors} of {count} programs differ from MARS");
}

#[test]
fn generated_programs_run_on_the_core() {
    // The generator itself: programs run and use most kinds of instructions.
    let mut ends = std::collections::BTreeMap::new();
    for seed in 1..201 {
        let (src, code) = program(seed);
        // The core's assembler encodes the program as the generator does.
        let assembled: Vec<u32> =
            rustemu_mips::asm::assemble(&src).unwrap().statements.iter().flat_map(|s| s.words.clone()).collect();
        assert_eq!(assembled, code, "seed {seed}:\n{src}");
        let end = match run_core(&code).error {
            None => "completed".to_string(),
            Some(e) => e.split(": ").nth(1).unwrap_or(&e).split(" 0x").next().unwrap().to_string(),
        };
        *ends.entry(end).or_insert(0) += 1;
    }
    eprintln!("{ends:#?}");
    // The comparison covers programs that complete and the main exceptions.
    for kind in ["completed", "arithmetic overflow", "trap"] {
        assert!(ends.get(kind).is_some_and(|&n| n >= 10), "too few programs end with {kind}: {ends:?}");
    }
    assert!(ends.keys().any(|k| k.contains("not aligned")), "no address exceptions: {ends:?}");
}

/// MARS's services without input: the printing formats, `sbrk`, seeded random numbers, `exit2`.
#[test]
fn services_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    // (MARS source, machine code) of each instruction.
    let li = |r: u32, v: i32| (format!("addiu {}, $zero, {v}", n(r)), i(0x09, 0, r, v));
    let sys = |v: i32| [li(2, v), ("syscall".to_string(), 0x0000_000C)];
    let mv = |d: u32, s: u32| (format!("addu {}, {}, $zero", n(d), n(s)), r(s, 0, d, 0, 0x21));
    let mut p: Vec<(String, u32)> = Vec::new();
    for v in [0, -1, 12345, -32768, 32767] {
        p.push(li(4, v));
        for s in [1, 34, 35, 36] {
            p.extend(sys(s));
            p.push(li(4, b' ' as i32));
            p.extend(sys(11));
            p.push(li(4, v));
        }
    }
    p.push(("lui $a0, 0x1001".into(), i(0x0F, 0, 4, 0x1001)));
    p.extend(sys(4));
    for size in [5, 0, 3, 100] {
        p.push(li(4, size));
        p.extend(sys(9));
        p.push(mv(4, 2));
        p.extend(sys(34));
    }
    p.push(li(4, 1));
    p.push(li(5, 2024));
    p.extend(sys(40));
    for bound in [10, 16, 1000, 7, 0x7fff] {
        p.push(li(4, 1));
        p.push(li(5, bound));
        p.extend(sys(42));
        p.extend(sys(1));
    }
    p.push(li(4, 1));
    p.extend(sys(41));
    p.extend(sys(1));
    // File errors: -1 in $v0, the program goes on (a missing file, descriptors not open).
    p.push(("lui $a0, 0x1001".into(), i(0x0F, 0, 4, 0x1001)));
    p.push(("ori $a0, $a0, 7".into(), i(0x0D, 4, 4, 7)));
    p.push(li(5, 0));
    p.extend(sys(13));
    p.push(mv(4, 2));
    p.extend(sys(1));
    for service in [14, 15] {
        p.push(li(4, 9));
        p.push(("lui $a1, 0x1001".into(), i(0x0F, 0, 5, 0x1001)));
        p.push(li(6, 4));
        p.extend(sys(service));
        p.push(mv(4, 2));
        p.extend(sys(1));
    }
    p.push(li(4, 7));
    p.extend(sys(17));
    let src = format!(
        ".data\n.asciiz \" text\\n\"\n.asciiz \"no_such_file\"\n.text\n{}",
        p.iter().map(|(t, _)| format!("    {t}\n")).collect::<String>()
    );
    let words: Vec<u32> = p.iter().map(|&(_, w)| w).collect();

    let mut m = rustemu_mips::Machine::new();
    let data = b" text\n\0no_such_file\0".to_vec();
    m.load(&rustemu_mips::Image {
        chunks: vec![
            rustemu_mips::Chunk {
                addr: TEXT_BASE,
                bytes: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
                code: true,
            },
            rustemu_mips::Chunk { addr: DATA_BASE, bytes: data, code: false },
        ],
        entry: TEXT_BASE,
    })
    .unwrap();
    let ev = m.run(100_000);
    let ours = m.console.text();

    let dir = std::env::temp_dir().join(format!("rustemu_mars_sys_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("services.asm");
    std::fs::write(&file, &src).unwrap();
    let mut cmd = Command::new("java");
    if mars.extension().is_some_and(|e| e == "jar") {
        cmd.arg("-jar").arg(&mars);
    } else {
        cmd.arg("-cp").arg(&mars).arg("Mars");
    }
    let out = cmd.arg("nc").arg(&file).output().expect("java runs");
    std::fs::remove_dir_all(&dir).ok();
    let theirs = String::from_utf8_lossy(&out.stdout).to_string();
    // MARS ends its output with a new line of its own.
    assert_eq!(ours, theirs.strip_suffix('\n').unwrap_or(&theirs), "\n{src}");
    assert_eq!(ev, rustemu_target::Event::Terminated(7));
    assert_eq!(out.status.code(), Some(7), "MARS exits with the code of exit2");
}

/// A source operand of the kind `op` requires (for the assembler comparison).
fn source_operand(g: &mut Rng, op: rustemu_mips::asm::Operand, branch: bool) -> String {
    use rustemu_mips::asm::{IntClass, Operand};
    match op {
        Operand::Reg => n(g.reg()),
        Operand::Int(IntClass::I5) => g.below(32).to_string(),
        Operand::Int(IntClass::I16U) => (32 + g.below(65536 - 32)).to_string(),
        Operand::Int(IntClass::I16) => (-1 - g.below(32768) as i64).to_string(),
        Operand::Int(IntClass::I32) => {
            let v = g.next() as i32;
            if (-32768..=65535).contains(&v) { "100000".into() } else { v.to_string() }
        }
        Operand::Label if branch => format!("t{}", g.below(4)),
        Operand::Label => format!("d{}", g.below(4)),
        Operand::LParen => "(".into(),
        Operand::RParen => ")".into(),
        Operand::Plus => "+".into(),
    }
}

/// Every form of every instruction, with random operands, assembled by the core's assembler and
/// by MARS: the machine code and the data must be the same.
#[test]
fn assembler_matches_mars() {
    use rustemu_mips::asm::{Table, assemble};
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let table = Table::get();
    let mut mnemonics: Vec<&str> = table.mnemonics().collect();
    mnemonics.sort();
    let dir = std::env::temp_dir().join(format!("rustemu_mars_asm_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rounds: u64 = std::env::var("MARS_PROGRAMS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    let mut failures = 0;
    for round in 0..rounds {
        let mut g = Rng(0x5EED + round * 7919);
        // Data labels around a 0x8000 boundary (the upper half of their address is rounded up
        // for signed offsets), text labels between the instructions.
        let mut src = String::from(
            ".data\nd0: .word 1, 2, 3\n.space 0x7ff0\nd1: .byte 7\nd2: .half 9\n.align 2\nd3: .asciiz \"x\\n\"\n.word d1, d3\n.text\n",
        );
        let mut lines = Vec::new();
        for m in &mnemonics {
            if matches!(*m, "syscall" | "break" | "eret") {
                continue;
            }
            for form in table.forms(m).unwrap() {
                // MARS assembles a nop for this one (see the instruction table).
                if form.example == "swr $t1,100" {
                    continue;
                }
                let branch = m.starts_with('b') || m.starts_with('j');
                let ops: Vec<String> = form.operands.iter().map(|&o| source_operand(&mut g, o, branch)).collect();
                let mut line = format!("{m} ");
                for (i, o) in ops.iter().enumerate() {
                    let sep = if i == 0 || o == "(" || o == ")" || ops[i - 1] == "(" || o == "+" || ops[i - 1] == "+" {
                        ""
                    } else {
                        ", "
                    };
                    line += sep;
                    line += o;
                }
                lines.push(line);
            }
        }
        // Shuffle, with the text labels spread among the lines.
        for i in (1..lines.len()).rev() {
            lines.swap(i, g.below(i as u64 + 1) as usize);
        }
        for (i, l) in lines.iter().enumerate() {
            if i % (lines.len() / 4) == 0 && i / (lines.len() / 4) < 4 {
                src += &format!("t{}:\n", i / (lines.len() / 4));
            }
            src += &format!("    {l}\n");
        }
        let ours = assemble(&src).unwrap_or_else(|e| panic!("{:?}\n{src}", &e[..e.len().min(5)]));
        let words: Vec<u32> = ours.statements.iter().flat_map(|s| s.words.clone()).collect();
        let mut data = Vec::new();
        let mut m = rustemu_mips::Machine::new();
        m.load(&ours.image).unwrap();
        for k in 0..((0x8010 + 24) / 4) {
            data.push(m.mem.peek32(DATA_BASE + 4 * k));
        }

        let file = dir.join("asm.asm");
        std::fs::write(&file, &src).unwrap();
        let (text_dump, data_dump) = (dir.join("text.hex"), dir.join("data.hex"));
        let mut cmd = Command::new("java");
        if mars.extension().is_some_and(|e| e == "jar") {
            cmd.arg("-jar").arg(&mars);
        } else {
            cmd.arg("-cp").arg(&mars).arg("Mars");
        }
        let out = cmd
            .args(["nc", "a", "dump", ".text", "HexText"])
            .arg(&text_dump)
            .args(["dump", ".data", "HexText"])
            .arg(&data_dump)
            .arg(&file)
            .output()
            .expect("java runs");
        let hex = |p: &Path| -> Vec<u32> {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .lines()
                .map(|l| u32::from_str_radix(l.trim(), 16).unwrap())
                .collect()
        };
        let (theirs, their_data) = (hex(&text_dump), hex(&data_dump));
        if theirs.is_empty() {
            panic!("MARS did not assemble:\n{}\n{src}", String::from_utf8_lossy(&out.stdout));
        }
        // The listing pairs source lines with both encodings.
        let mut k = 0;
        for s in &ours.statements {
            for (w, b) in s.words.iter().zip(&s.basic) {
                if theirs.get(k) != Some(w) {
                    failures += 1;
                    if failures <= 20 {
                        let line = src.lines().nth(s.line - 1).unwrap_or("");
                        eprintln!(
                            "round {round}, line {}: {line:<40} ours {w:08x} {b:<24} MARS {:08x?}",
                            s.line,
                            theirs.get(k)
                        );
                    }
                }
                k += 1;
            }
        }
        assert_eq!(words.len(), theirs.len(), "the same number of instructions (round {round})");
        let n = data.len().min(their_data.len());
        assert_eq!(data[..n], their_data[..n], "the data segment (round {round})");
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(failures, 0, "{failures} instructions differ from MARS");
}
