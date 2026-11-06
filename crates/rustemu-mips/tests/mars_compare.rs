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
    run_core_with(code, false)
}

/// [`run_core`], with delayed branching or not.
fn run_core_with(code: &[u32], delayed: bool) -> Outcome {
    let mut mem = Memory::new();
    let bytes: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    mem.load(TEXT_BASE, &bytes);
    mem.add_code(TEXT_BASE, TEXT_BASE + bytes.len() as u32);
    let mut cpu = Cpu::new();
    cpu.delayed_branching = delayed;
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
    run_mars_with(mars, dir, src, &[])
}

/// [`run_mars`] with MARS's `options` (`db`: delayed branching).
fn run_mars_with(mars: &Path, dir: &Path, src: &str, options: &[&str]) -> Outcome {
    let file = dir.join("prog.asm");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new("java");
    if mars.extension().is_some_and(|e| e == "jar") {
        cmd.arg("-jar").arg(mars);
    } else {
        cmd.arg("-cp").arg(mars).arg("Mars");
    }
    cmd.arg("nc").args(options).arg("10000");
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

/// The same random programs with delayed branching (MARS's `db`): the instruction after each
/// branch and jump executes, a branch in a delay slot postpones the first one, `jal` and `jalr`
/// link past the slot, and a trap in a slot ends the program there.
#[test]
fn delayed_branching_matches_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let count: u64 = std::env::var("MARS_PROGRAMS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let first: u64 = std::env::var("MARS_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let dir = std::env::temp_dir().join(format!("rustemu_mars_db_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut errors = 0;
    for seed in first..first + count {
        let (src, code) = program(seed);
        let (ours, theirs) = (run_core_with(&code, true), run_mars_with(&mars, &dir, &src, &["db"]));
        if ours != theirs {
            errors += 1;
            eprintln!("seed {seed}: differs\n--- program\n{src}--- core\n{ours:?}\n--- MARS\n{theirs:?}\n");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(errors, 0, "{errors} of {count} programs differ from MARS with delayed branching");
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
        settings: Default::default(),
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
        // ($f31 has no next register for the pairs of mfc1.d / mtc1.d.)
        Operand::FReg => format!("$f{}", g.below(31)),
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

/// Words for the floating-point registers: the values where floats differ (zeros, ±1.5, halves
/// for rounding, NaN, infinities, denormals, the edges of the 32-bit range), the high words of
/// such doubles, and random bits.
const FLOAT_WORDS: [u32; 30] = [
    0x0000_0000,
    0x8000_0000,
    0x3f80_0000,
    0x3fc0_0000,
    0x4020_0000,
    0xc020_0000,
    0x3f00_0000,
    0xbfc0_0000,
    0x7fc0_0000,
    0x7f80_0001,
    0x7f80_0000,
    0xff80_0000,
    0x4f32_d05e,
    0xcf32_d05e,
    0x0000_0001,
    0x3eaa_aaab,
    0x4b00_0000,
    0x4eff_ffff,
    0x4f00_0000,
    0xcf00_0000,
    0x3ff0_0000,
    0x4004_0000,
    0xc004_0000,
    0x7ff8_0000,
    0x7ff0_0000,
    0x41e0_0000,
    0xc1e0_0000,
    0x3fe0_0000,
    0x41df_ffff,
    0x400c_0000,
];

/// A random program of coprocessor 1: registers from `FLOAT_WORDS` and random bits, then
/// arithmetic, square roots, conversions and roundings to words, comparisons into the 8
/// condition flags, conditional moves and branches, moves to and from the general registers,
/// stores. Double operations take even registers, sometimes an odd one (MARS's runtime error).
/// At the end the condition flags go to $s0..$s7.
fn float_program(seed: u64) -> String {
    let mut g = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut src = String::from(".data\nwords:\n");
    for _ in 0..32 {
        let w = if g.below(3) == 0 { g.next() as u32 } else { FLOAT_WORDS[g.below(30) as usize] };
        src += &format!("    .word 0x{w:08x}\n");
    }
    src += "out: .space 128\n.text\n    la $a0, words\n    la $a1, out\n";
    for r in 0..32 {
        src += &format!("    lwc1 $f{r}, {}($a0)\n", 4 * r);
    }
    for t in ["$t0", "$t1", "$t2"] {
        src += &format!("    li {t}, {}\n", g.below(4) as i32 - 1);
    }
    let any = |g: &mut Rng| format!("$f{}", g.below(32));
    let even = |g: &mut Rng| {
        // One time in 600 an odd register (a program in ten or so): MARS stops with
        // "must be even-numbered".
        if g.below(600) == 0 { format!("$f{}", 2 * g.below(16) + 1) } else { format!("$f{}", 2 * g.below(16)) }
    };
    let mut label = 0;
    for _ in 0..40 {
        let line = match g.below(16) {
            0 => {
                let op = ["add", "sub", "mul", "div"][g.below(4) as usize];
                format!("{op}.s {}, {}, {}", any(&mut g), any(&mut g), any(&mut g))
            }
            1 => {
                let op = ["add", "sub", "mul", "div"][g.below(4) as usize];
                format!("{op}.d {}, {}, {}", even(&mut g), even(&mut g), even(&mut g))
            }
            2 => {
                let op = ["sqrt", "abs", "neg", "mov", "round.w", "trunc.w", "ceil.w", "floor.w", "cvt.w"]
                    [g.below(9) as usize];
                format!("{op}.s {}, {}", any(&mut g), any(&mut g))
            }
            3 => {
                let op = ["sqrt", "abs", "neg", "mov"][g.below(4) as usize];
                format!("{op}.d {}, {}", even(&mut g), even(&mut g))
            }
            4 => {
                let op = ["round.w", "trunc.w", "ceil.w", "floor.w", "cvt.w", "cvt.s"][g.below(6) as usize];
                format!("{op}.d {}, {}", any(&mut g), even(&mut g))
            }
            5 => match g.below(3) {
                0 => format!("cvt.d.s {}, {}", even(&mut g), any(&mut g)),
                1 => format!("cvt.d.w {}, {}", even(&mut g), any(&mut g)),
                _ => format!("cvt.s.w {}, {}", any(&mut g), any(&mut g)),
            },
            6 | 7 => {
                let c = ["eq", "lt", "le"][g.below(3) as usize];
                if g.below(2) == 0 {
                    format!("c.{c}.s {}, {}, {}", g.below(8), any(&mut g), any(&mut g))
                } else {
                    format!("c.{c}.d {}, {}, {}", g.below(8), even(&mut g), even(&mut g))
                }
            }
            8 => {
                let op = ["movf", "movt"][g.below(2) as usize];
                if g.below(2) == 0 {
                    format!("{op}.s {}, {}, {}", any(&mut g), any(&mut g), g.below(8))
                } else {
                    format!("{op}.d {}, {}, {}", even(&mut g), even(&mut g), g.below(8))
                }
            }
            9 => {
                let op = ["movn", "movz"][g.below(2) as usize];
                let t = ["$t0", "$t1", "$t2"][g.below(3) as usize];
                if g.below(2) == 0 {
                    format!("{op}.s {}, {}, {t}", any(&mut g), any(&mut g))
                } else {
                    format!("{op}.d {}, {}, {t}", even(&mut g), even(&mut g))
                }
            }
            10 => {
                let t = ["$t3", "$t4", "$t5"][g.below(3) as usize];
                format!("mfc1 {t}, {}", any(&mut g))
            }
            11 => format!("mtc1 {}, {}", ["$t0", "$t1", "$t2"][g.below(3) as usize], any(&mut g)),
            12 => {
                label += 1;
                let op = ["bc1t", "bc1f"][g.below(2) as usize];
                format!(
                    "{op} {}, skip{label}\n    abs.s $f{}, $f{}\nskip{label}:",
                    g.below(8),
                    g.below(32),
                    g.below(32)
                )
            }
            13 => {
                let op = ["movf", "movt"][g.below(2) as usize];
                format!("{op} $t6, $t{}, {}", g.below(3), g.below(8))
            }
            14 => format!("swc1 {}, {}($a1)", any(&mut g), 4 * g.below(32)),
            _ => format!("sdc1 {}, {}($a1)", even(&mut g), 8 * g.below(16)),
        };
        src += &format!("    {line}\n");
    }
    src += "    li $k0, 1\n";
    for cc in 0..8 {
        src += &format!("    li $s{cc}, 0\n    movt $s{cc}, $k0, {cc}\n");
    }
    src
}

/// Registers compared by the floating-point programs.
fn float_regs() -> Vec<String> {
    let gpr = ["t0", "t1", "t2", "t3", "t4", "t5", "t6", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7"];
    gpr.iter().map(|r| format!("${r}")).chain((0..32).map(|r| format!("$f{r}"))).collect()
}

/// The program on the core: the registers of `float_regs`, 128 bytes at `out`, the error.
fn run_core_float(src: &str) -> Outcome {
    let p = rustemu_mips::asm::assemble(src).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
    let mut m = rustemu_mips::Machine::new();
    m.load(&p.image).unwrap();
    let error = match m.run(100_000) {
        rustemu_target::Event::Error(e) => Some(e),
        _ => None,
    };
    let regs = float_regs()
        .into_iter()
        .map(|r| {
            let v = match r.strip_prefix("$f") {
                Some(n) => m.cpu.fpr[n.parse::<usize>().unwrap()],
                None => m.cpu.regs[NAMES.iter().position(|n| *n == &r[1..]).unwrap()],
            };
            (r[1..].to_string(), v)
        })
        .collect();
    let out = DATA_BASE + 128;
    Outcome { regs, data: (0..32).map(|k| m.mem.peek32(out + 4 * k)).collect(), error }
}

fn run_mars_float(mars: &Path, dir: &Path, src: &str) -> Outcome {
    let file = dir.join("float.asm");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new("java");
    if mars.extension().is_some_and(|e| e == "jar") {
        cmd.arg("-jar").arg(mars);
    } else {
        cmd.arg("-cp").arg(mars).arg("Mars");
    }
    cmd.arg("nc").arg("100000").args(float_regs());
    let out = DATA_BASE + 128;
    cmd.arg(format!("0x{out:08x}-0x{:08x}", out + 124)).arg(&file);
    let output = cmd.output().expect("java runs");
    let text = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
    let mut regs = Vec::new();
    let mut data = Vec::new();
    let mut error = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('$') {
            let mut cols = rest.split('\t');
            let name = cols.next().unwrap().to_string();
            let value = cols.next().unwrap().trim().trim_start_matches("0x");
            regs.push((name, u32::from_str_radix(value, 16).unwrap()));
        } else if line.starts_with("Mem[") {
            data.extend(
                line.split('\t')
                    .skip(1)
                    .filter(|w| w.starts_with("0x"))
                    .map(|w| u32::from_str_radix(&w[2..], 16).unwrap()),
            );
        } else if let Some(i) = line.find("Runtime exception at ") {
            error = Some(line[i..].trim().to_string());
        } else if line.contains("Assemble: operation completed with errors") {
            panic!("MARS did not assemble the program:\n{text}\n{src}");
        }
    }
    Outcome { regs, data, error }
}

/// Random programs of coprocessor 1 on the core and on MARS: the general and floating-point
/// registers, the condition flags, the stored words and the error must be the same.
#[test]
fn float_programs_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let count: u64 = std::env::var("MARS_PROGRAMS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let dir = std::env::temp_dir().join(format!("rustemu_mars_float_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut errors = 0;
    let mut stopped = 0;
    for seed in 1..=count {
        let src = float_program(seed);
        let (ours, theirs) = (run_core_float(&src), run_mars_float(&mars, &dir, &src));
        stopped += ours.error.is_some() as u32;
        if ours != theirs {
            errors += 1;
            if errors <= 5 {
                eprintln!("seed {seed}: differs\n--- program\n{src}--- core\n{ours:?}\n--- MARS\n{theirs:?}\n");
            }
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    eprintln!("{count} programs, {stopped} stopped by an odd register");
    assert_eq!(errors, 0, "{errors} of {count} floating-point programs differ from MARS");
}

/// Programs with macros and `.include`s: (main program, included files by path).
const MACRO_CASES: &[(&str, &[(&str, &str)])] = &[
    // MARS's help: arguments with and without parentheses, a label in a data segment of a macro,
    // a macro passed by name to another one.
    (
        r#".macro print_int (%x)
    li $v0, 1
    add $a0, $zero, %x
    syscall
.end_macro
.macro print_str (%str)
    .data
myLabel: .asciiz %str
    .text
    li $v0, 4
    la $a0, myLabel
    syscall
.end_macro
.macro for (%regIterator, %from, %to, %bodyMacroName)
    add %regIterator, $zero, %from
    Loop:
    %bodyMacroName ()
    add %regIterator, %regIterator, 1
    ble %regIterator, %to, Loop
.end_macro
.macro body()
    print_int $t0
    print_str "\n"
.end_macro
.text
    print_int (10)
    print_str ("test1")
    for ($t0, 1, 3, body)
    li $v0, 10
    syscall
"#,
        &[],
    ),
    // Overloading by the number of arguments; a redefinition (the last one wins); SPIM's `$x`
    // parameters; arguments separated by spaces; a label on the line of a call.
    (
        r#".macro inc(%r)
    addi %r, %r, 1
.end_macro
.macro inc(%r, %n)
    addi %r, %r, %n
.end_macro
.macro inc(%r)
    addiu %r, %r, 1
.end_macro
.macro swap($a, $b)
    move $at, $a
    move $a, $b
    move $b, $at
.end_macro
.text
start: inc $t0
    inc($t1, 5)
here: swap $t0 $t1
    j here
    beq $t0, $t1, start
"#,
        &[],
    ),
    // MARS's substitution: a parameter that starts another one's name replaces its first
    // occurrence (here inside the other parameter).
    (".macro m(%r, %rr)\n    add %r, %rr, %r\n    add %rr, %r, %r\n.end_macro\n.text\n    m($t1, $t2)\n", &[]),
    // A macro in the data segment, a continuation of a data directive, .eqv in a body, labels
    // of two expansions, a label used in data.
    (
        r#".macro table(%v)
    .eqv TWICE 2
    .word %v, %v
here: .word here, TWICE
.end_macro
.data
    table(7)
    table(-1)
    .half 3
    5, 6
"#,
        &[],
    ),
    // .include: a macro library from a subfolder, which includes a file next to itself.
    (
        ".include \"lib/macros.asm\"\n.text\nmain: exit_with(3)\n    print(42)\n",
        &[
            (
                "lib/macros.asm",
                ".include \"print.asm\"\n.macro exit_with(%code)\n    li $a0, %code\n    li $v0, 17\n    syscall\n.end_macro\n",
            ),
            ("lib/print.asm", ".macro print(%n)\n    li $a0, %n\n    li $v0, 1\n    syscall\n.end_macro\n"),
        ],
    ),
    // Errors: an unknown parameter, a loop, a nested definition, .end_macro alone, an unknown
    // name, a forward reference, invalid names and parameters, an error in a nested expansion.
    (".macro m(%a)\nli $t0, %b\n.end_macro\nm(1)\n", &[]),
    (".macro m\nm\n.end_macro\nm\n", &[]),
    (".macro m\n.macro n\n.end_macro\n", &[]),
    ("nop\n.end_macro\n", &[]),
    ("nop\n.macro m\nnop\n", &[]),
    ("m\n.macro m\nnop\n.end_macro\nm 1\n", &[]),
    (".macro li\n.end_macro\n.macro 5\n.end_macro\n", &[]),
    (".macro m(%a, b)\n.end_macro\n", &[]),
    (".macro m(%a)\n  nop\n  addi $t0, %a, 1\n.end_macro\n.macro n(%r)\n  m(%r)\n.end_macro\n.text\n  n(foo)\n", &[]),
    (".macro m\n  .eqv K 1\n.end_macro\n.eqv K 2\nm\n", &[]),
    // Labels of a loop in two expansions; errors of the second pass in an expansion.
    (
        ".macro count(%r, %n)\n    li %r, 0\nloop:\n    addi %r, %r, 1\n    blt %r, %n, loop\n.end_macro\n.text\n    count($t0, 3)\n    count($t1, 5)\n    j loop_M1\n",
        &[],
    ),
    (".macro m\n  nop\n  j nowhere\n.end_macro\n.text\n  nop\n  m\n", &[]),
    // .include errors: a second include of a file, a missing one, without quotes; a label on
    // an .include line goes with the line.
    (".include \"inc.asm\"\n.include \"inc.asm\"\n", &[("inc.asm", "nop\n")]),
    (".include \"nofile.asm\"\n", &[]),
    (".include inc.asm\nnop\n", &[("inc.asm", "nop\n")]),
    ("x: .include \"inc.asm\"\nj x\n", &[("inc.asm", "nop\n")]),
    (".include \"inc.asm\"\nnop\n", &[("inc.asm", "nop\nli $t0, %x\nbogus\n")]),
];

/// Macros and `.include` against MARS: the machine code and data of each case, or its errors
/// (file, line with the macro calls, column, message).
#[test]
fn macros_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let root = std::env::temp_dir().join(format!("rustemu_mars_macros_{}", std::process::id()));
    let mut failures = 0;
    for (k, (main, files)) in MACRO_CASES.iter().enumerate() {
        let dir = root.join(k.to_string());
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let file = dir.join("main.asm");
        std::fs::write(&file, main).unwrap();
        for (name, text) in *files {
            std::fs::write(dir.join(name), text).unwrap();
        }
        // Ours.
        let mut read = |p: &str| std::fs::read_to_string(p).ok();
        let ours = rustemu_mips::asm::assemble_with(&file.to_string_lossy(), main, &mut read);
        let short = |s: &str| s.replace(&format!("{}/", dir.display()), "");
        let (our_code, our_data, our_errors) = match &ours {
            Ok(p) => {
                let mut m = rustemu_mips::Machine::new();
                m.load(&p.image).unwrap();
                let data: Vec<u32> = (0..64).map(|i| m.mem.peek32(DATA_BASE + 4 * i)).collect();
                (p.statements.iter().flat_map(|s| s.words.clone()).collect::<Vec<u32>>(), data, Vec::new())
            }
            Err(e) => {
                let errors = e
                    .errors
                    .iter()
                    .map(|x| short(format!("{} {x}", e.files[x.file]).trim_end()))
                    .collect::<Vec<String>>();
                (Vec::new(), Vec::new(), errors)
            }
        };
        // MARS.
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
        let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
        let their_errors: Vec<String> =
            text.lines().filter_map(|l| l.strip_prefix("Error in ")).map(|l| short(l.trim_end())).collect();
        let hex = |p: &Path| -> Vec<u32> {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .lines()
                .map(|l| u32::from_str_radix(l.trim(), 16).unwrap())
                .collect()
        };
        let their_code = if their_errors.is_empty() { hex(&text_dump) } else { Vec::new() };
        let their_data: Vec<u32> =
            if their_errors.is_empty() { hex(&data_dump).into_iter().take(64).collect() } else { Vec::new() };
        let ok = our_errors == their_errors
            && our_code == their_code
            && our_data[..their_data.len().min(our_data.len())] == their_data[..their_data.len().min(our_data.len())];
        if !ok {
            failures += 1;
            eprintln!(
                "case {k} differs:\n{main}--- ours\n{our_errors:#?}\n{our_code:08x?}\n--- MARS\n{their_errors:#?}\n{their_code:08x?}\n{text}"
            );
        }
    }
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(failures, 0, "{failures} of {} macro / .include cases differ from MARS", MACRO_CASES.len());
}

/// Reads an integer, a float, a double, a string (buffer of 16) and an integer, then prints them
/// back separated by `|`.
const INPUT_PROGRAM: &str = ".data
buf: .space 16
.text
    li $v0, 5
    syscall
    move $s0, $v0
    li $v0, 6
    syscall
    mov.s $f20, $f0
    li $v0, 7
    syscall
    mov.d $f22, $f0
    la $a0, buf
    li $a1, 16
    li $v0, 8
    syscall
    li $v0, 5
    syscall
    move $s1, $v0
    move $a0, $s0
    li $v0, 1
    syscall
    li $a0, '|'
    li $v0, 11
    syscall
    mov.s $f12, $f20
    li $v0, 2
    syscall
    li $a0, '|'
    li $v0, 11
    syscall
    mov.d $f12, $f22
    li $v0, 3
    syscall
    li $a0, '|'
    li $v0, 11
    syscall
    la $a0, buf
    li $v0, 4
    syscall
    li $a0, '|'
    li $v0, 11
    syscall
    move $a0, $s1
    li $v0, 1
    syscall
";

/// MARS's input services read from the console (`nc`: a line each) as from its Run I/O pane
/// when each line fits: integers, floats and doubles as Java parses them (spaces, signs, suffixes,
/// hexadecimal literals, `NaN`, out of range), strings shorter than the buffer, and the errors of bad numbers. (A
/// character, and a string longer than its buffer, are read differently by MARS's two modes;
/// RustEmu8086 reads as the Run I/O pane does.)
#[test]
fn input_services_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let dir = std::env::temp_dir().join(format!("rustemu_mars_input_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("input.asm");
    std::fs::write(&file, INPUT_PROGRAM).unwrap();
    let prog = rustemu_mips::asm::assemble(INPUT_PROGRAM).unwrap();
    let inputs = [
        "42\n2.5\n1e300\nhello\n-7\n",
        " 42 \n+3.5f\nNaN\n\n0\n",
        "-2147483648\n-Infinity\n2.5e-310\nfourteen chars\n2147483647\n",
        "1\n0.1\n0.1d\nwith  spaces\n+0\n",
        "1\n0x1.8p1f\n-0X1.8P1D\nhex input\n0\n",
        "1\n0x1.00000100000000000001p0\n0x1p-1074F\nrounding\n0\n",
        "1\n0x1p+\n",
        "1\n1\n0x1.8\n",
        "abc\n",
        "2147483648\n",
        "1\nx\n",
        "1\n1\n.\n",
        "1\n1\n1\nok\n12a\n",
    ];
    let mut failures = 0;
    for input in inputs {
        let mut m = rustemu_mips::Machine::new();
        m.load(&prog.image).unwrap();
        m.push_text(input);
        let ev = m.run(100_000);
        // The console echoes what was typed (as the Run I/O pane does), then the program's output.
        let console = m.console.text();
        let typed: String = input.chars().take(console.len()).collect();
        let ours_out = console.strip_prefix(typed.as_str()).unwrap_or(&console).to_string();
        let ours_err = match ev {
            rustemu_target::Event::Error(e) => Some(e),
            _ => None,
        };
        let mut cmd = Command::new("java");
        if mars.extension().is_some_and(|e| e == "jar") {
            cmd.arg("-jar").arg(&mars);
        } else {
            cmd.arg("-cp").arg(&mars).arg("Mars");
        }
        let mut child = cmd
            .arg("nc")
            .arg(&file)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("java runs");
        use std::io::Write;
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
        let their_err = text.lines().find_map(|l| l.find("Runtime exception at ").map(|i| l[i..].trim().to_string()));
        let their_out = if their_err.is_some() {
            String::new()
        } else {
            String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string()
        };
        let ours_out = if ours_err.is_some() { String::new() } else { ours_out };
        if (ours_out.as_str(), &ours_err) != (their_out.as_str(), &their_err) {
            failures += 1;
            eprintln!(
                "input {input:?} differs:\n--- ours\n{ours_out:?}\n{ours_err:?}\n--- MARS\n{their_out:?}\n{their_err:?}\n"
            );
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(failures, 0, "{failures} of {} inputs differ from MARS", inputs.len());
}
