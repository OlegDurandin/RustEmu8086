//! MARS's settings: delayed branching, self-modifying code, start at `main`, no
//! pseudo-instructions, warnings as errors, the compact memory configurations.
//!
//! Each case is a program, the settings and what it must end with (worked out from MARS's
//! sources). When the environment variable `MARS` names MARS (see `mars_compare.rs`), the same
//! programs also run in MARS with its options for the settings (`db`, `smc`, `sm`, `np`, `we`,
//! `mc …`), and the registers, the output, the error or the assembly messages must be the same.

use std::path::Path;
use std::process::Command;

use rustemu_mips::asm::assemble_settings;
use rustemu_mips::cpu::REG_NAMES;
use rustemu_mips::{Machine, MemoryConfig, Settings};
use rustemu_target::Event;

/// How a program ended.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Outcome {
    /// The registers `$v0..$ra` (by name).
    regs: Vec<(String, u32)>,
    /// What the program printed.
    output: String,
    /// The runtime error, MARS's message.
    error: Option<String>,
    /// The assembly's errors (or, when it succeeded, its warnings): `line L column C: message`.
    messages: Vec<String>,
}

impl Outcome {
    fn reg(&self, name: &str) -> u32 {
        self.regs.iter().find(|r| r.0 == name).map_or_else(|| panic!("no register {name}"), |r| r.1)
    }
}

struct Case {
    name: &'static str,
    settings: Settings,
    src: &'static str,
    check: fn(&Outcome),
}

fn shown() -> impl Iterator<Item = usize> {
    2..32
}

fn run_ours(case: &Case) -> Outcome {
    let prog = match assemble_settings("prog.s", case.src, &mut |_| None, &case.settings) {
        Ok(p) => p,
        Err(e) => return Outcome { messages: e.errors.iter().map(|e| e.to_string()).collect(), ..Outcome::default() },
    };
    let mut m = Machine::new();
    m.instant_waits = true;
    m.load(&prog.image).unwrap();
    let error = match m.run(100_000) {
        Event::Error(e) => Some(e),
        Event::Terminated(_) => None,
        ev => panic!("{}: the program did not end: {ev:?}", case.name),
    };
    Outcome {
        regs: shown().map(|r| (REG_NAMES[r].to_string(), m.cpu.regs[r])).collect(),
        output: m.console.text(),
        error,
        messages: prog.warnings.iter().map(|w| w.to_string()).collect(),
    }
}

fn run_mars(mars: &Path, dir: &Path, case: &Case) -> Outcome {
    let file = dir.join("prog.s");
    std::fs::write(&file, case.src).unwrap();
    let mut cmd = Command::new("java");
    if mars.extension().is_some_and(|e| e == "jar") {
        cmd.arg("-jar").arg(mars);
    } else {
        cmd.arg("-cp").arg(mars).arg("Mars");
    }
    cmd.arg("nc").args(case.settings.mars_options()).arg("100000");
    for r in shown() {
        cmd.arg(format!("${}", REG_NAMES[r]));
    }
    let out = cmd.arg(&file).output().expect("java runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let text = stdout.clone() + &String::from_utf8_lossy(&out.stderr);
    let mut o = Outcome::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('$')
            && let Some((name, value)) = rest.split_once('\t')
        {
            o.regs.push((name.to_string(), u32::from_str_radix(value.trim().trim_start_matches("0x"), 16).unwrap()));
        } else if let Some(i) = line.find("Runtime exception at ").or_else(|| line.find("invalid program counter")) {
            o.error = Some(line[i..].trim().to_string());
        } else if (line.starts_with("Error in ") || line.starts_with("Warning in "))
            && let Some(i) = line.find(" line ")
        {
            o.messages.push(line[i + 1..].trim().to_string());
        }
    }
    // The output is what comes before the registers, and MARS ends it with a new line of its own.
    let end = stdout.find("\n$").unwrap_or(stdout.len());
    let output = &stdout[..end];
    o.output = output
        .lines()
        .filter(|l| !l.contains("Runtime exception") && !l.starts_with("Error"))
        .collect::<Vec<_>>()
        .join("\n");
    if !o.messages.is_empty() && o.regs.is_empty() {
        o.output.clear();
    }
    o
}

fn delayed() -> Settings {
    Settings { delayed_branching: true, ..Settings::default() }
}

fn compact(memory: MemoryConfig) -> Settings {
    Settings { memory, ..Settings::default() }
}

const DELAYED: &str = "
.text
    li $t0, 1
    beq $zero, $zero, L
    addi $t0, $t0, 10      # the delay slot
    addi $t0, $t0, 100
L:  jal f
    addi $t1, $zero, 7     # the delay slot
    li $v0, 1
    move $a0, $t0
    syscall
    li $v0, 10
    syscall
f:  move $t2, $ra
    jr $ra
    nop
";

const BRANCH_IN_SLOT: &str = "
.text
    beq $zero, $zero, A
    beq $zero, $zero, B    # a branch in the delay slot keeps the first target, one later
    addi $t0, $t0, 1
    addi $t0, $t0, 2
A:  addi $t1, $zero, 1
    li $v0, 10
    syscall
B:  addi $t2, $zero, 1
    li $v0, 10
    syscall
";

const EXCEPTION_IN_SLOT: &str = "
.text
    li $t0, 0x7fffffff
    b T
    addi $t0, $t0, 1       # overflow in the delay slot
    addi $t3, $zero, 3
T:  addi $t1, $zero, 1
    li $v0, 10
    syscall
.ktext 0x80000180
    addi $t2, $zero, 2
    mfc0 $k0, $14
    addi $k0, $k0, 4
    mtc0 $k0, $14
    eret
";

const SYSCALL_IN_SLOT: &str = "
.text
    li $a0, 5
    li $v0, 1
    j L
    syscall                # the delay slot prints 5
    li $a0, 9
L:  li $v0, 10
    syscall
";

const DELAY_NOPS: &str = "
.text
    li $t0, 3
    mulou $t1, $t0, $t0    # its overflow check skips the delay slot nop and the break
    li $t0, 0x10000
    li $v0, 1
    move $a0, $t1
    syscall
    mulo $t2, $t0, $t0     # overflows: break
";

const SELF_MODIFYING: &str = "
.data
model: .word 0x200a002a    # addi $t2, $zero, 42
.text
    la $t0, patch
    lw $t1, model
    sw $t1, 0($t0)
patch:
    nop
    lw $t3, 0($t0)         # the code reads as the stored word
    la $t4, spare
    li $t5, 0x1234
    sh $t5, 0($t4)         # a halfword store replaces the whole word
    lw $t6, 0($t4)
    lb $t7, 0($t4)
    la $t8, after
    li $t2, 0
    sw $t1, 0($t8)         # a new instruction past the end of the program
    j after
spare:
    nop
after:
";

const UNALIGNED_TEXT_STORE: &str = "
.text
    la $t0, here
here:
    sb $zero, 1($t0)
";

const CODE_IN_DATA: &str = "
.data
code: .word 0x2409002a, 0x03e00008   # addiu $t1, $zero, 42; jr $ra
.text
    la $t0, code
    jalr $t0
    li $v0, 10
    syscall
";

const TEXT_STORE: &str = "
.text
    la $t0, here
here:
    sw $zero, 0($t0)
";

const MAIN: &str = "
.text
    li $t0, 1
    li $v0, 10
    syscall
.globl main
main:
    li $t0, 2
    li $v0, 10
    syscall
";

const MAIN_NOT_GLOBAL: &str = "
.text
    li $t0, 1
    li $v0, 10
    syscall
main:
    li $t0, 2
    li $v0, 10
    syscall
";

const PSEUDO: &str = "
.text
    addi $t0, $zero, 5
    li $t1, 5
    addi $t2, $t0, 100000
";

const WARNING: &str = "
.data
x: .byte 300
.text
    lb $t0, x
";

const COMPACT: &str = "
.data
x: .word 7
y: .word 9
.text
    lw $t0, y              # one instruction: lw $t0, 4($zero) in a compact configuration
    la $t1, y
    move $t2, $gp
    move $t3, $sp
    li $a0, 16
    li $v0, 9
    syscall
    move $t4, $v0
    li $t5, 0x7fffffff
    addi $t5, $t5, 1       # to the exception handler
    li $v0, 10
    syscall
.ktext 0x4180
    mfc0 $t6, $14
    addi $t7, $t6, 4
    mtc0 $t7, $14
    eret
";

const CASES: &[Case] = &[
    Case {
        name: "delayed branches and jal",
        settings: Settings { delayed_branching: true, ..DEFAULTS },
        src: DELAYED,
        check: |o| {
            assert_eq!((o.reg("t0"), o.reg("t1"), o.reg("t2")), (11, 7, 0x0040_0018));
            assert_eq!(o.output, "11");
        },
    },
    Case {
        name: "the same without delayed branching",
        settings: DEFAULTS,
        src: DELAYED,
        check: |o| {
            assert_eq!((o.reg("t0"), o.reg("t1"), o.reg("t2")), (1, 7, 0x0040_0014));
            assert_eq!(o.output, "1");
        },
    },
    Case {
        name: "a branch in a delay slot",
        settings: Settings { delayed_branching: true, ..DEFAULTS },
        src: BRANCH_IN_SLOT,
        check: |o| assert_eq!((o.reg("t0"), o.reg("t1"), o.reg("t2")), (1, 1, 0)),
    },
    Case {
        name: "an exception in a delay slot: the branch is taken over the handler",
        settings: Settings { delayed_branching: true, ..DEFAULTS },
        src: EXCEPTION_IN_SLOT,
        check: |o| assert_eq!((o.reg("t1"), o.reg("t2"), o.reg("t3")), (1, 0, 0)),
    },
    Case {
        name: "a syscall in a delay slot",
        settings: Settings { delayed_branching: true, ..DEFAULTS },
        src: SYSCALL_IN_SLOT,
        check: |o| assert_eq!(o.output, "5"),
    },
    Case {
        name: "delay slot nops of pseudo-instructions",
        settings: Settings { delayed_branching: true, ..DEFAULTS },
        src: DELAY_NOPS,
        check: |o| {
            assert_eq!(o.output, "9");
            assert!(o.error.as_deref().is_some_and(|e| e.contains("break instruction executed")), "{o:?}");
        },
    },
    Case {
        name: "self-modifying code",
        settings: Settings { self_modifying: true, ..DEFAULTS },
        src: SELF_MODIFYING,
        check: |o| {
            assert_eq!(o.reg("t3"), 0x200a_002a);
            assert_eq!((o.reg("t6"), o.reg("t7")), (0x1234, 0x34));
            assert_eq!(o.reg("t2"), 42, "the stored instruction ran");
        },
    },
    Case {
        name: "a byte store not at a word's address in the text segment",
        settings: Settings { self_modifying: true, ..DEFAULTS },
        src: UNALIGNED_TEXT_STORE,
        check: |o| {
            assert_eq!(
                o.error.as_deref(),
                Some(
                    "Runtime exception at 0x00400008: fetch address for text segment not aligned to word boundary 0x00400009"
                )
            )
        },
    },
    Case {
        name: "code in the data segment",
        settings: Settings { self_modifying: true, ..DEFAULTS },
        src: CODE_IN_DATA,
        check: |o| assert_eq!(o.reg("t1"), 42),
    },
    Case {
        name: "no self-modifying code",
        settings: DEFAULTS,
        src: TEXT_STORE,
        check: |o| {
            assert_eq!(
                o.error.as_deref(),
                Some("Runtime exception at 0x00400008: Cannot write directly to text segment!0x00400008")
            )
        },
    },
    Case {
        name: "start at the global main",
        settings: Settings { start_at_main: true, ..DEFAULTS },
        src: MAIN,
        check: |o| assert_eq!(o.reg("t0"), 2),
    },
    Case {
        name: "main that is not global",
        settings: Settings { start_at_main: true, ..DEFAULTS },
        src: MAIN_NOT_GLOBAL,
        check: |o| assert_eq!(o.reg("t0"), 1),
    },
    Case {
        name: "no pseudo-instructions",
        settings: Settings { pseudo_instructions: false, ..DEFAULTS },
        src: PSEUDO,
        check: |o| {
            let m = "Extended (pseudo) instruction or format not permitted.  See Settings.";
            assert_eq!(o.messages, [format!("line 4 column 5: {m}"), format!("line 5 column 5: {m}")]);
        },
    },
    Case {
        name: "a warning",
        settings: DEFAULTS,
        src: WARNING,
        check: |o| {
            assert_eq!(o.messages.len(), 1);
            assert_eq!(o.reg("t0"), 44);
        },
    },
    Case {
        name: "warnings are errors",
        settings: Settings { warnings_are_errors: true, ..DEFAULTS },
        src: WARNING,
        check: |o| {
            assert_eq!(
                o.messages,
                ["line 3 column 10: \"300\" is out-of-range for a signed value and possibly truncated"]
            );
            assert!(o.regs.is_empty(), "not run");
        },
    },
    Case {
        name: "compact, data at address 0",
        settings: Settings { memory: MemoryConfig::CompactDataAtZero, ..DEFAULTS },
        src: COMPACT,
        check: |o| {
            assert_eq!((o.reg("t0"), o.reg("t1")), (9, 4));
            assert_eq!((o.reg("t2"), o.reg("t3"), o.reg("t4")), (0x1800, 0x2ffc, 0x2000));
            assert_eq!(o.reg("t6"), 0x3000 + 4 * 10, "EPC: the addi");
        },
    },
    Case {
        name: "compact, text at address 0",
        settings: Settings { memory: MemoryConfig::CompactTextAtZero, ..DEFAULTS },
        src: COMPACT,
        check: |o| {
            assert_eq!((o.reg("t0"), o.reg("t1")), (9, 0x2004));
            assert_eq!((o.reg("t2"), o.reg("t3"), o.reg("t4")), (0x1800, 0x3ffc, 0x3000));
            assert_eq!(o.reg("t6"), 4 * 10, "EPC: the addi");
        },
    },
];

/// MARS's default settings (a constant for the cases).
const DEFAULTS: Settings = Settings {
    delayed_branching: false,
    self_modifying: false,
    start_at_main: false,
    pseudo_instructions: true,
    warnings_are_errors: false,
    memory: MemoryConfig::Default,
};

#[test]
fn defaults_are_mars_defaults() {
    assert_eq!(DEFAULTS, Settings::default());
    assert_eq!(delayed().mars_options(), ["db"]);
    assert_eq!(compact(MemoryConfig::CompactDataAtZero).mars_options(), ["mc", "CompactDataAtZero"]);
}

#[test]
fn settings_change_assembly_and_execution() {
    for case in CASES {
        let o = run_ours(case);
        let r = std::panic::catch_unwind(|| (case.check)(&o));
        assert!(r.is_ok(), "{}: {o:#?}", case.name);
    }
}

#[test]
fn pseudo_instructions_take_their_mars_forms() {
    let words = |src: &str, s: &Settings| -> Vec<usize> {
        assemble_settings("", src, &mut |_| None, s).unwrap().statements.iter().map(|s| s.words.len()).collect()
    };
    // The delay slot nop and the longer branch of mulou.
    assert_eq!(words(".text\nmulou $t1, $t0, $t0\n", &DEFAULTS), [5]);
    assert_eq!(words(".text\nmulou $t1, $t0, $t0\n", &delayed()), [6]);
    // 16-bit addresses: lw and la of a label in one instruction.
    let src = ".data\nx: .word 1\n.text\nlw $t0, x\nla $t1, x\n";
    assert_eq!(words(src, &DEFAULTS), [2, 2]);
    assert_eq!(words(src, &compact(MemoryConfig::CompactDataAtZero)), [1, 1]);
    // The program's settings go with its image; the machine takes them on load.
    let p = assemble_settings("", src, &mut |_| None, &compact(MemoryConfig::CompactTextAtZero)).unwrap();
    assert_eq!(p.image.entry, 0);
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    assert_eq!((m.cpu.pc, m.cpu.regs[29], m.mem.layout.mmio_base), (0, 0x3ffc, 0x7f00));
}

#[test]
fn step_back_over_delay_slots_and_written_code() {
    let src = SELF_MODIFYING;
    let p = assemble_settings("", src, &mut |_| None, &Settings { self_modifying: true, ..DEFAULTS }).unwrap();
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    let after = p.symbols.iter().find(|s| s.name == "after").unwrap().addr;
    assert_eq!(m.mem.fetch(after), Ok(None));
    while m.mem.fetch(after) == Ok(None) {
        assert_eq!(m.step(), Event::Continue);
    }
    // Back over the store: no instruction there again.
    assert!(m.step_back());
    assert_eq!(m.mem.fetch(after), Ok(None));
    // A branch waiting for its delay slot is undone too.
    let p = assemble_settings("", DELAYED, &mut |_| None, &delayed()).unwrap();
    m.load(&p.image).unwrap();
    m.step();
    m.step();
    assert_ne!(m.cpu.delay, rustemu_mips::Delay::Cleared);
    let pending = m.cpu.clone();
    m.step();
    assert!(m.step_back());
    assert_eq!(m.cpu, pending);
}

#[test]
fn devices_follow_the_memory_configuration() {
    // In a compact configuration MMIO is at 0x7f00: the display's data register is 0x7f0c.
    let src = ".text\nli $t0, 'A'\nsw $t0, 0x7f0c($zero)\nnop\nnop\nnop\nnop\nnop\nnop\n";
    let p = assemble_settings("", src, &mut |_| None, &compact(MemoryConfig::CompactDataAtZero)).unwrap();
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    m.run(100);
    assert_eq!(m.mmio.text(), b"A");
}

#[test]
fn settings_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let dir = std::env::temp_dir().join(format!("rustemu_mars_settings_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = 0;
    for case in CASES {
        let (ours, theirs) = (run_ours(case), run_mars(&mars, &dir, case));
        if ours != theirs {
            failures += 1;
            eprintln!(
                "{} differs ({:?}):\n{}--- ours\n{ours:#?}\n--- MARS\n{theirs:#?}\n",
                case.name,
                case.settings.mars_options(),
                case.src
            );
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(failures, 0, "{failures} of {} cases differ from MARS", CASES.len());
}
