//! MARS's Keyboard and Display MMIO Simulator against the core: programs that poll the ready bits,
//! take keyboard and display interrupts, position the terminal's caret, with keys typed after
//! given instructions and different transmitter delays.
//!
//! Runs when the environment variable `MARS` names MARS (see `mars_compare.rs`): the tool's own
//! class runs the program, connected without showing its window, through `mmio/MmioHarness.java`
//! (compiled with `javac`). The display text, the registers, PC, Status / Cause / EPC, the four
//! device registers and the error must be the same.

use std::path::Path;
use std::process::Command;

use rustemu_mips::Machine;
use rustemu_mips::cpu::cp0;
use rustemu_mips::mmio::DELAYS;

/// A program and how to run it.
struct Case {
    source: &'static str,
    /// Index in MARS's delay table (its slider).
    delay: usize,
    /// Display after delay.
    dad: bool,
    /// (instructions executed, character typed).
    keys: &'static [(u64, u8)],
    steps: u64,
}

const HELLO: &str = r#"
        .data
msg:    .asciiz "Hello, MMIO!\n"
        .text
main:   la   $t0, msg
        li   $t1, 0xffff0000
loop:   lb   $t2, 0($t0)
        beqz $t2, done
wait:   addi $s0, $s0, 1
        lw   $t3, 8($t1)
        andi $t3, $t3, 1
        beqz $t3, wait
        sw   $t2, 12($t1)
        addi $t0, $t0, 1
        j    loop
done:   li   $v0, 10
        syscall
"#;

const TERMINAL: &str = r#"
        .text
main:   li   $t1, 0xffff0000
        li   $a0, 'A'
        jal  send
        li   $a0, 'B'
        jal  send
        li   $a0, 0x00302007    # bell: x 3, y 2
        jal  send
        li   $a0, 'C'
        jal  send
        li   $a0, 'D'
        jal  send
        li   $a0, 0xfff00007    # x beyond the last column, y 0
        jal  send
        li   $a0, 'E'
        jal  send
        li   $a0, 'F'
        jal  send
        li   $a0, 'G'
        jal  send
        li   $a0, 0xffffff07    # the last column of the last row
        jal  send
        li   $a0, 'X'
        jal  send
        li   $a0, 'Y'
        jal  send
        li   $a0, 'Z'
        jal  send
        li   $a0, 0x00001107    # x 0, y 17
        jal  send
        li   $a0, 'Q'
        jal  send
        li   $a0, 10            # a new line replaces a character
        jal  send
        li   $a0, 'R'
        jal  send
        li   $v0, 10
        syscall
send:   lw   $t3, 8($t1)
        andi $t3, $t3, 1
        beqz $t3, send
        sw   $a0, 12($t1)
        jr   $ra
"#;

const CLEAR: &str = r#"
        .text
main:   li   $t1, 0xffff0000
        li   $a0, 'A'
        jal  send
        li   $a0, 12            # form feed
        jal  send
        li   $a0, 'B'
        jal  send
        li   $a0, 7             # bell (sb: x and y 0)
        jal  sendb
        li   $a0, 'C'
        jal  send
        li   $a0, 12
        jal  send
        li   $a0, 'D'
        jal  send
        li   $v0, 10
        syscall
send:   lw   $t3, 8($t1)
        andi $t3, $t3, 1
        beqz $t3, send
        sw   $a0, 12($t1)
        jr   $ra
sendb:  lw   $t3, 8($t1)
        andi $t3, $t3, 1
        beqz $t3, sendb
        sb   $a0, 12($t1)
        jr   $ra
"#;

/// Stores without waiting: characters stored while the display is busy are lost.
const IMPATIENT: &str = r#"
        .text
main:   li   $t1, 0xffff0000
        li   $t2, 'a'
        li   $t3, 'z'
next:   sw   $t2, 12($t1)
        addi $t2, $t2, 1
        ble  $t2, $t3, next
        li   $t2, 0x0748        # sh: the low half, 'H'
        sh   $t2, 12($t1)
        li   $v0, 10
        syscall
"#;

const DISPLAY_INTERRUPTS: &str = r#"
        .ktext 0x80000180
        mfc0 $s1, $13
        mfc0 $s2, $14
        addi $s3, $s3, 1
        lb   $k1, 0($t0)
        beqz $k1, stop
        sw   $k1, 12($t1)
        addi $t0, $t0, 1
        eret
stop:   sw   $zero, 8($t1)
        eret
        .data
msg:    .asciiz "interrupts!"
        .text
main:   la   $t0, msg
        li   $t1, 0xffff0000
        li   $k1, 3
        sw   $k1, 8($t1)        # ready, interrupt enable
        lb   $k1, 0($t0)
        sw   $k1, 12($t1)
        addi $t0, $t0, 1
work:   addi $s0, $s0, 1
        lw   $t4, 8($t1)
        bnez $t4, work
        li   $v0, 10
        syscall
"#;

const UNHANDLED: &str = r#"
        .text
main:   li   $t1, 0xffff0000
        li   $t2, 3
        sw   $t2, 8($t1)
        li   $t2, '!'
        sw   $t2, 12($t1)
loop:   addi $s0, $s0, 1
        j    loop
"#;

const ECHO: &str = r#"
        .text
main:   li   $t1, 0xffff0000
loop:   addi $s0, $s0, 1
        lw   $t2, 0($t1)
        andi $t2, $t2, 1
        beqz $t2, loop
        lbu  $s1, 5($t1)        # not the data register: ready stays
        lw   $s2, 0($t1)
        lw   $a0, 4($t1)
        beq  $a0, 'q', done
send:   lw   $t3, 8($t1)
        andi $t3, $t3, 1
        beqz $t3, send
        sw   $a0, 12($t1)
        j    loop
done:   li   $v0, 10
        syscall
"#;

const KEY_INTERRUPTS: &str = r#"
        .ktext 0x80000180
        lw   $k0, 4($t1)
        add  $s1, $s1, $k0
        mfc0 $s2, $13
        mfc0 $s3, $14
        mfc0 $s5, $12
        addi $s4, $s4, 1
        eret
        .text
main:   li   $t1, 0xffff0000
        li   $t2, 2
        sw   $t2, 0($t1)        # interrupt enable
loop:   addi $s0, $s0, 1
        blt  $s4, 3, loop
        li   $v0, 10
        syscall
"#;

const KEY_UNHANDLED: &str = r#"
        .text
main:   li   $t1, 0xffff0000
        li   $t2, 2
        sw   $t2, 0($t1)
loop:   addi $s0, $s0, 1
        j    loop
"#;

/// Status interrupt enable off: no interrupt, the program polls.
const MASKED: &str = r#"
        .text
main:   li   $t1, 0xffff0000
        mfc0 $t2, $12
        andi $t2, $t2, 0xfffe
        mtc0 $t2, $12
        li   $t2, 2
        sw   $t2, 0($t1)
loop:   addi $s0, $s0, 1
        lw   $t2, 0($t1)
        andi $t2, $t2, 1
        beqz $t2, loop
        lw   $s1, 4($t1)
        li   $v0, 10
        syscall
"#;

const CASES: &[Case] = &[
    Case { source: HELLO, delay: 4, dad: true, keys: &[], steps: 2000 },
    Case { source: HELLO, delay: 0, dad: true, keys: &[], steps: 2000 },
    Case { source: HELLO, delay: 10, dad: true, keys: &[], steps: 5000 },
    Case { source: HELLO, delay: 6, dad: false, keys: &[], steps: 2000 },
    Case { source: HELLO, delay: 12, dad: true, keys: &[], steps: 700 },
    Case { source: TERMINAL, delay: 4, dad: true, keys: &[], steps: 3000 },
    Case { source: CLEAR, delay: 4, dad: true, keys: &[], steps: 3000 },
    Case { source: CLEAR, delay: 2, dad: false, keys: &[], steps: 3000 },
    Case { source: IMPATIENT, delay: 5, dad: true, keys: &[], steps: 3000 },
    Case { source: IMPATIENT, delay: 1, dad: false, keys: &[], steps: 3000 },
    Case { source: DISPLAY_INTERRUPTS, delay: 4, dad: true, keys: &[], steps: 5000 },
    Case { source: DISPLAY_INTERRUPTS, delay: 9, dad: false, keys: &[], steps: 5000 },
    Case { source: UNHANDLED, delay: 4, dad: true, keys: &[], steps: 200 },
    Case { source: ECHO, delay: 4, dad: true, keys: &[(30, b'h'), (80, b'i'), (80, b'j'), (200, b'q')], steps: 1000 },
    Case { source: ECHO, delay: 10, dad: true, keys: &[(5, b'x'), (6, b'y'), (300, b'q')], steps: 1000 },
    Case { source: KEY_INTERRUPTS, delay: 4, dad: true, keys: &[(20, b'A'), (60, b'B'), (100, b'C')], steps: 1000 },
    // The third key comes while the handler runs (exception level set): no interrupt.
    Case { source: KEY_INTERRUPTS, delay: 4, dad: true, keys: &[(20, b'A'), (60, b'B'), (61, b'C')], steps: 500 },
    Case { source: KEY_UNHANDLED, delay: 4, dad: true, keys: &[(10, b'k')], steps: 100 },
    Case { source: MASKED, delay: 4, dad: true, keys: &[(25, b'm')], steps: 200 },
    Case {
        source: include_str!("../../../examples/mips/keyboard_display.s"),
        delay: 4,
        dad: true,
        keys: &[(600, b'h'), (700, b'i'), (800, 27), (900, b'x')],
        steps: 2000,
    },
];

/// What a run leaves.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    display: String,
    regs: Vec<u32>,
    pc: u32,
    cp0: [u32; 3],
    mmio: [u32; 4],
    error: Option<String>,
}

/// The display text escaped as the harness prints it.
fn escape(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&c| if (32..127).contains(&c) && c != b'\\' { (c as char).to_string() } else { format!("\\x{c:02x}") })
        .collect()
}

fn run_core(case: &Case, size: (u32, u32)) -> Outcome {
    let p = rustemu_mips::asm::assemble(case.source).expect("the program assembles");
    let mut m = Machine::new();
    m.mmio.delay = DELAYS[case.delay];
    m.mmio.display_after_delay = case.dad;
    m.mmio.terminal_size = size;
    m.load(&p.image).unwrap();
    let mut done = 0;
    let mut error = None;
    let mut over = false;
    let mut run = |m: &mut Machine, n: u64, over: &mut bool| {
        if *over || n == 0 {
            return;
        }
        let (ev, _) = m.run_counted(n);
        match ev {
            rustemu_target::Event::Budget => {}
            rustemu_target::Event::Error(e) => {
                error = Some(e);
                *over = true;
            }
            _ => *over = true,
        }
    };
    for &(at, key) in case.keys {
        let at = at.min(case.steps);
        run(&mut m, at - done, &mut over);
        done = at;
        m.mmio_key(key);
    }
    run(&mut m, case.steps - done, &mut over);
    Outcome {
        display: escape(m.mmio.text()),
        regs: m.cpu.regs.to_vec(),
        pc: m.cpu.pc,
        cp0: [m.cpu.cp0[cp0::STATUS], m.cpu.cp0[cp0::CAUSE], m.cpu.cp0[cp0::EPC]],
        mmio: std::array::from_fn(|i| m.mem.peek32(0xffff_0000 + 4 * i as u32)),
        error,
    }
}

/// Compiles the harness against MARS.
fn harness(mars: &Path, dir: &Path) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mmio/MmioHarness.java");
    let out = Command::new("javac")
        .args(["-nowarn", "-cp"])
        .arg(mars)
        .arg("-d")
        .arg(dir)
        .arg(&src)
        .output()
        .expect("javac runs");
    assert!(out.status.success(), "the harness compiles: {}", String::from_utf8_lossy(&out.stderr));
}

/// Runs the case on MARS: the terminal size of its display and the outcome.
fn run_mars(mars: &Path, dir: &Path, case: &Case) -> ((u32, u32), Outcome) {
    let file = dir.join("prog.asm");
    std::fs::write(&file, case.source).unwrap();
    let keys: Vec<String> = case.keys.iter().map(|(at, k)| format!("{at}:{k}")).collect();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let out = Command::new("java")
        .arg("-cp")
        .arg(format!("{}{sep}{}", mars.display(), dir.display()))
        .arg("MmioHarness")
        .arg(&file)
        .arg(case.delay.to_string())
        .arg(if case.dad { "1" } else { "0" })
        .arg(keys.join(","))
        .arg(case.steps.to_string())
        .output()
        .expect("java runs");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "the harness runs:\n{text}{}", String::from_utf8_lossy(&out.stderr));
    let mut size = (0, 0);
    let mut o = Outcome { display: String::new(), regs: vec![0; 32], pc: 0, cp0: [0; 3], mmio: [0; 4], error: None };
    let hex = |s: &str| u32::from_str_radix(s.trim(), 16).unwrap();
    for line in text.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "SIZE" => {
                let (c, r) = value.split_once(' ').unwrap();
                size = (c.parse().unwrap(), r.parse().unwrap());
            }
            "DISPLAY" => o.display = value.to_string(),
            "PC" => o.pc = hex(value),
            "C12" => o.cp0[0] = hex(value),
            "C13" => o.cp0[1] = hex(value),
            "C14" => o.cp0[2] = hex(value),
            "ERROR" => {
                o.error = value.find("Runtime exception at ").map(|i| value[i..].trim().to_string());
            }
            _ if key.starts_with('R') => o.regs[key[1..].parse::<usize>().unwrap()] = hex(value),
            _ if key.starts_with('M') => o.mmio[key[1..].parse::<usize>().unwrap()] = hex(value),
            _ => {}
        }
    }
    (size, o)
}

#[test]
fn keyboard_and_display_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let dir = std::env::temp_dir().join(format!("rustemu_mars_mmio_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    harness(&mars, &dir);
    let mut failures = 0;
    for (k, case) in CASES.iter().enumerate() {
        let (size, theirs) = run_mars(&mars, &dir, case);
        let mut ours = run_core(case, size);
        // After an error the core leaves PC on the instruction (as for every exception), MARS past it.
        if ours.error.is_some() && ours.pc + 4 == theirs.pc {
            ours.pc = theirs.pc;
        }
        if ours != theirs {
            failures += 1;
            eprintln!("case {k} differs\n--- core\n{ours:?}\n--- MARS\n{theirs:?}\n");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(failures, 0, "{failures} of {} cases differ from MARS", CASES.len());
}

#[test]
fn cases_run_on_the_core() {
    // Without MARS: the programs assemble and the polling ones print.
    let o = run_core(&CASES[0], (80, 25));
    assert_eq!(o.display, "Hello, MMIO!\\x0a");
    assert_eq!(o.error, None);
    let o = run_core(&CASES[12], (80, 25));
    assert!(o.error.unwrap().ends_with("External Interrupt"));
}

// ---- Digital Lab Sim ----

/// Scans the rows, shows the key's digit on the right display and its code on the left one.
const LAB_SCAN: &str = r#"
        .data
digits: .byte 0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f, 0x77, 0x7c, 0x39, 0x5e, 0x79, 0x71
        .text
main:   li   $t0, 0xffff0010
scan:   li   $t1, 1
row:    sb   $t1, 2($t0)          # scan a row, interrupts off
        lbu  $t2, 4($t0)
        bnez $t2, found
        sll  $t1, $t1, 1
        ble  $t1, 8, row
        addi $s0, $s0, 1
        j    scan
found:  sb   $t2, 1($t0)          # the code on the left display
        andi $t3, $t2, 0xf        # row bit -> 0..3
        srl  $t4, $t2, 4          # column bit -> 0..3
        li   $t5, 0
r1:     srl  $t3, $t3, 1
        beqz $t3, c0
        addi $t5, $t5, 4
        j    r1
c0:     srl  $t4, $t4, 1
        beqz $t4, show
        addi $t5, $t5, 1
        j    c0
show:   lbu  $t6, digits($t5)
        sb   $t6, 0($t0)
        move $s1, $t5
        j    scan
"#;

const LAB_KEY_INTERRUPT: &str = r#"
        .ktext 0x80000180
        mfc0 $s5, $13
        mfc0 $s6, $14
        addi $s2, $s2, 1
        li   $k1, 0x81            # scan row 1 (interrupts stay on)
        sb   $k1, 0xffff0012
        lbu  $s3, 0xffff0014
        add  $s4, $s4, $s3
        eret
        .text
main:   li   $t0, 0xffff0010
        li   $t1, 0x80            # keyboard interrupt on
        sb   $t1, 2($t0)
loop:   addi $s0, $s0, 1
        blt  $s2, 2, loop
        li   $v0, 10
        syscall
"#;

const LAB_TIMER: &str = r#"
        .ktext 0x80000180
        mfc0 $s5, $13
        mfc0 $s6, $14
        addi $s2, $s2, 1
        eret
        .text
main:   li   $t0, 0xffff0010
        li   $t1, 1
        sb   $t1, 3($t0)          # counter on
loop:   addi $s0, $s0, 1
        blt  $s2, 5, loop
        sb   $zero, 3($t0)        # counter off
        li   $v0, 10
        syscall
"#;

/// Accesses to the tool's bytes count as well, in the program and in the handler.
const LAB_TIMER_ACCESSES: &str = r#"
        .ktext 0x80000180
        addi $s2, $s2, 1
        sb   $s2, 0xffff0010      # counts, but no interrupt while in the handler
        sb   $s2, 0xffff0011
        mfc0 $s6, $14
        eret
        .text
main:   li   $t0, 0xffff0010
        li   $t1, 0x107
        sh   $t1, 2($t0)          # row 7 (several bits: no key matches); MARS sees only 0xffff0012
        li   $t1, 1
        sb   $t1, 3($t0)          # counter on
loop:   addi $s0, $s0, 1
        lbu  $t2, 0($t0)          # a load counts too
        blt  $s2, 4, loop
        li   $v0, 10
        syscall
"#;

const LAB_TIMER_UNHANDLED: &str = r#"
        .text
main:   li   $t0, 0xffff0010
        li   $t1, 1
        sb   $t1, 3($t0)
loop:   addi $s0, $s0, 1
        j    loop
"#;

/// Loads light the displays with the value loaded; a word stores only its address's byte.
const LAB_LOADS: &str = r#"
        .text
main:   li   $t0, 0xffff0010
        li   $t1, 0x4f5b
        sw   $t1, 0($t0)          # the right display gets 0x5b, the left one nothing
        mfc0 $s0, $12
        lbu  $s1, 1($t0)          # the left display gets 0x4f
        li   $t1, 0x0002
        sh   $t1, 2($t0)          # row 2
        lbu  $s2, 4($t0)
        lw   $s3, 0($t0)          # a word load: the right display gets the word
        li   $v0, 10
        syscall
"#;

/// A Digital Lab Sim program and its clicks: (instructions executed, key).
struct LabCase {
    source: &'static str,
    clicks: &'static [(u64, u8)],
    steps: u64,
}

const LAB_CASES: &[LabCase] = &[
    LabCase { source: LAB_SCAN, clicks: &[(100, 5), (300, 5), (400, 0xe), (700, 0), (800, 9)], steps: 1200 },
    LabCase { source: LAB_SCAN, clicks: &[(50, 0), (51, 3)], steps: 600 },
    LabCase { source: LAB_KEY_INTERRUPT, clicks: &[(30, 2), (60, 2), (90, 3)], steps: 400 },
    // A click before the keyboard interrupt is on: none.
    LabCase { source: LAB_KEY_INTERRUPT, clicks: &[(1, 1), (40, 1), (41, 0)], steps: 300 },
    LabCase { source: LAB_TIMER, clicks: &[], steps: 2000 },
    LabCase { source: LAB_TIMER_ACCESSES, clicks: &[], steps: 3000 },
    LabCase { source: LAB_TIMER_UNHANDLED, clicks: &[], steps: 300 },
    LabCase { source: LAB_LOADS, clicks: &[(4, 4)], steps: 100 },
    LabCase {
        source: include_str!("../../../examples/mips/digital_lab.s"),
        clicks: &[(500, 5), (800, 5), (900, 12)],
        steps: 20000,
    },
];

/// What a Digital Lab Sim run leaves.
#[derive(Debug, PartialEq, Eq)]
struct LabOutcome {
    segments: [u8; 2],
    key: Option<u8>,
    regs: Vec<u32>,
    pc: u32,
    cp0: [u32; 3],
    bytes: [u32; 2],
    error: Option<String>,
}

fn run_core_lab(case: &LabCase) -> LabOutcome {
    let p = rustemu_mips::asm::assemble(case.source).expect("the program assembles");
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    let mut done = 0;
    let mut error = None;
    let mut over = false;
    let mut run = |m: &mut Machine, n: u64, over: &mut bool| {
        if *over || n == 0 {
            return;
        }
        match m.run_counted(n).0 {
            rustemu_target::Event::Budget => {}
            rustemu_target::Event::Error(e) => {
                error = Some(e);
                *over = true;
            }
            _ => *over = true,
        }
    };
    for &(at, key) in case.clicks {
        let at = at.min(case.steps);
        run(&mut m, at - done, &mut over);
        done = at;
        m.lab_click(key);
    }
    run(&mut m, case.steps - done, &mut over);
    LabOutcome {
        segments: m.lab.segments(),
        key: m.lab.pressed(),
        regs: m.cpu.regs.to_vec(),
        pc: m.cpu.pc,
        cp0: [m.cpu.cp0[cp0::STATUS], m.cpu.cp0[cp0::CAUSE], m.cpu.cp0[cp0::EPC]],
        bytes: std::array::from_fn(|i| m.mem.peek32(0xffff_0010 + 4 * i as u32)),
        error,
    }
}

fn run_mars_lab(mars: &Path, dir: &Path, case: &LabCase) -> LabOutcome {
    let file = dir.join("lab.asm");
    std::fs::write(&file, case.source).unwrap();
    let clicks: Vec<String> = case.clicks.iter().map(|(at, k)| format!("{at}:{k}")).collect();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let out = Command::new("java")
        .arg("-cp")
        .arg(format!("{}{sep}{}", mars.display(), dir.display()))
        .arg("LabHarness")
        .arg(&file)
        .arg(clicks.join(","))
        .arg(case.steps.to_string())
        .output()
        .expect("java runs");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "the harness runs:\n{text}{}", String::from_utf8_lossy(&out.stderr));
    let mut o =
        LabOutcome { segments: [0; 2], key: None, regs: vec![0; 32], pc: 0, cp0: [0; 3], bytes: [0; 2], error: None };
    let hex = |s: &str| u32::from_str_radix(s.trim(), 16).unwrap();
    for line in text.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "SEG" => {
                let (l, r) = value.split_once(' ').unwrap();
                o.segments = [hex(l) as u8, hex(r) as u8];
            }
            "KEY" => o.key = value.parse::<i32>().ok().filter(|&k| k >= 0).map(|k| k as u8),
            "PC" => o.pc = hex(value),
            "C12" => o.cp0[0] = hex(value),
            "C13" => o.cp0[1] = hex(value),
            "C14" => o.cp0[2] = hex(value),
            "ERROR" => o.error = value.find("Runtime exception at ").map(|i| value[i..].trim().to_string()),
            _ if key.starts_with('R') => o.regs[key[1..].parse::<usize>().unwrap()] = hex(value),
            _ if key.starts_with('M') => o.bytes[key[1..].parse::<usize>().unwrap()] = hex(value),
            _ => {}
        }
    }
    o
}

#[test]
fn digital_lab_matches_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let dir = std::env::temp_dir().join(format!("rustemu_mars_lab_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mmio/LabHarness.java");
    let out = Command::new("javac")
        .args(["-nowarn", "-cp"])
        .arg(&mars)
        .arg("-d")
        .arg(&dir)
        .arg(&src)
        .output()
        .expect("javac runs");
    assert!(out.status.success(), "the harness compiles: {}", String::from_utf8_lossy(&out.stderr));
    let mut failures = 0;
    for (k, case) in LAB_CASES.iter().enumerate() {
        let theirs = run_mars_lab(&mars, &dir, case);
        let mut ours = run_core_lab(case);
        if ours.error.is_some() && ours.pc + 4 == theirs.pc {
            ours.pc = theirs.pc;
        }
        if ours != theirs {
            failures += 1;
            eprintln!("lab case {k} differs\n--- core\n{ours:?}\n--- MARS\n{theirs:?}\n");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(failures, 0, "{failures} of {} cases differ from MARS", LAB_CASES.len());
}

#[test]
fn lab_cases_run_on_the_core() {
    let o = run_core_lab(&LAB_CASES[0]);
    assert_eq!(o.regs[17], 9, "the last key shown");
    assert!(run_core_lab(&LAB_CASES[6]).error.unwrap().ends_with("External Interrupt"));
    assert_eq!(run_core_lab(&LAB_CASES[4]).regs[18], 5, "five counter interrupts");
    assert_eq!(run_core_lab(&LAB_CASES[5]).regs[18], 4, "accesses count as well");
}
