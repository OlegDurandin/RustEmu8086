//! External QtSpim corpus: https://github.com/ffcabbar/MIPS-Assembly-Language-Examples
//! pinned at de9c21ea0429f9d0eb840b6806f15acc24d42b21 (18 sources and the README snippet).
//! Sources are read from testdata/mips-ffcabbar (gitignored), or FFCABBAR_DIR, never modified.
//! Setup: git clone https://github.com/ffcabbar/MIPS-Assembly-Language-Examples testdata/mips-ffcabbar
//! then check out the pinned commit above in that clone.
//! Run: MARS=/path/to/MARS cargo test -p rustemu-mips
//! --test ffcabbar_corpus -- --nocapture. MARS is optional; with it, each execution is
//! compared by output (excluding input echo), all general registers, and data memory.
//!
//! Known upstream issues are intentionally preserved: #4 prints the remainder twice, #12
//! falls through to addition for an invalid operation, #14 leaks eight stack bytes, and #18
//! sorts six words instead of five (the sixth word starts at zero). A successful test means
//! compatible execution, not that these algorithms or their input validation are correct.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rustemu_mips::asm::{assemble, assemble_settings};
use rustemu_mips::cpu::REG_NAMES;
use rustemu_mips::{Machine, Settings};
use rustemu_target::Event;

const FILES: [&str; 19] = [
    "mips-1.asm",
    "mips-2-multiplication.asm",
    "mips-3-division-and-remainder.asm",
    "mips-4-divider.asm",
    "mips-5-ifElse.asm",
    "mips-6-ifElseAdder.asm",
    "mips-7.asm",
    "mips-8-fact.asm",
    "mips-9-while.asm",
    "mips-10-functions.asm",
    "mips-11.asm",
    "mips-12-calculator.asm",
    "mips-13-Arrays.asm",
    "mips-14.asm",
    "mips-15.asm",
    "mips-16-powerFunc.asm",
    "mips-17-recursivePowerFunc.asm",
    "mips-18-bubbleSort.asm",
    "README.md",
];
const RANGES: [(u32, u32); 2] = [(0x1000_0000, 0x1000_003c), (0x1001_0000, 0x1001_00fc)];

fn root() -> Option<PathBuf> {
    let explicit = std::env::var_os("FFCABBAR_DIR");
    let root = explicit
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/mips-ffcabbar"));
    if explicit.is_none() && !root.exists() {
        eprintln!("testdata/mips-ffcabbar is missing: skipping the external QtSpim corpus (or set FFCABBAR_DIR)");
        return None;
    }
    let count = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "asm"))
        .count();
    assert_eq!(count, 18, "the upstream inventory changed; review the cases");
    Some(root)
}

fn source(root: &Path, n: usize) -> String {
    let text = std::fs::read_to_string(root.join(FILES[n - 1])).unwrap();
    if n == 19 { text.split_once("```bash\n").unwrap().1.split_once("```").unwrap().0.to_string() } else { text }
}

/// Only dialect/entry-point changes; deliberately preserves upstream algorithm errors.
fn adapted(source: &str) -> String {
    let source = source.strip_prefix("data\n").map_or_else(|| source.to_string(), |rest| format!(".data\n{rest}"));
    format!(
        ".globl main\n{}",
        source
            .replace("sub $s0,1", "sub $s0,$s0,1")
            .replace("sw $s0,$sp", "sw $s0,0($sp)")
            .replace("lw $s0,$sp", "lw $s0,0($sp)")
            .replace("addi $sp,-4", "addi $sp,$sp,-4")
            .replace("addi $sp,4", "addi $sp,$sp,4")
            .replace("addi $t0,1", "addi $t0,$t0,1")
            .replace("addi $t1,1", "addi $t1,$t1,1")
    )
}

fn run(source: &str, input: &str, main: bool) -> (Machine, Event, String) {
    let settings = Settings { start_at_main: main, ..Settings::default() };
    let program = assemble_settings("corpus.asm", source, &mut |_| None, &settings).unwrap();
    let mut machine = Machine::new();
    machine.load(&program.image).unwrap();
    machine.history_enabled = false;
    machine.push_text(input);
    let mut output = Vec::new();
    for _ in 0..100_000 {
        let input_call = machine.mem.peek32(machine.cpu.pc) == 12 && machine.cpu.regs[2] == 5;
        let before = machine.console.len();
        let event = machine.step();
        if !input_call {
            output.extend_from_slice(&machine.console.bytes()[before..]);
        }
        if event != Event::Continue {
            return (machine, event, String::from_utf8(output).unwrap());
        }
    }
    (machine, Event::Budget, String::from_utf8(output).unwrap())
}

fn mars_command(mars: &Path) -> Command {
    let mut cmd = Command::new("java");
    if mars.extension().is_some_and(|e| e == "jar") {
        cmd.arg("-jar").arg(mars);
    } else {
        cmd.arg("-cp").arg(mars).arg("Mars");
    }
    cmd.args(["nc", "ae1", "se2"]);
    cmd
}

#[test]
fn unmodified_sources() {
    let Some(root) = root() else { return };
    let mars = std::env::var_os("MARS").map(PathBuf::from);
    let temp = std::env::temp_dir().join(format!("remu_ffcabbar_raw_{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let mut counts = [0; 3]; // assembly failure, runtime failure, normal exit
    for n in 1..=19 {
        let src = source(&root, n);
        let invalid = matches!(n, 13 | 14 | 17 | 18 | 19);
        assert_eq!(assemble(&src).is_err(), invalid, "{}", FILES[n - 1]);
        if invalid {
            counts[0] += 1;
        } else {
            let (_, event, _) = run(&src, default_input(n), false);
            if matches!(n, 10 | 16) {
                assert!(matches!(event, Event::Error(ref e) if e.contains("invalid program counter")), "{event:?}");
                counts[1] += 1;
            } else {
                assert_eq!(event, Event::Terminated(0));
                counts[2] += 1;
            }
        }
        if let Some(mars) = &mars {
            let path = temp.join(format!("{n}.asm"));
            std::fs::write(&path, &src).unwrap();
            let out = mars_command(mars).arg("a").arg(&path).output().unwrap();
            assert_eq!(
                out.status.success(),
                !invalid,
                "MARS assembly {}: {}",
                FILES[n - 1],
                String::from_utf8_lossy(&out.stdout)
            );
            if matches!(n, 10 | 16) {
                let out = mars_command(mars).arg("100000").arg(&path).output().unwrap();
                let text = String::from_utf8_lossy(&out.stdout);
                assert!(text.contains("invalid program counter"), "{text}");
            }
        }
        eprintln!(
            "original {}: {}",
            FILES[n - 1],
            if invalid {
                "assembly error"
            } else if matches!(n, 10 | 16) {
                "entry-point error"
            } else {
                "exit 0"
            }
        );
    }
    assert_eq!(counts, [5, 2, 12]);
    std::fs::remove_dir_all(temp).unwrap();
}

fn default_input(n: usize) -> &'static str {
    match n {
        8 => "5\n",
        9 => "12345\n",
        12 => "7\n3\n1\n",
        13..=15 | 18 => "",
        16 | 17 => "2\n5\n",
        _ => "7\n3\n",
    }
}

fn compare_mars(mars: &Path, path: &Path, input: &str, machine: &Machine, output: &str) {
    let mut cmd = mars_command(mars);
    cmd.args(["sm", "100000"]);
    for name in REG_NAMES {
        cmd.arg(format!("${name}"));
    }
    for (start, end) in RANGES {
        cmd.arg(format!("0x{start:08x}-0x{end:08x}"));
    }
    let mut child = cmd.arg(path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let result = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success(), "MARS failed: {text} {}", String::from_utf8_lossy(&result.stderr));
    let (printed, state) = text.split_once("$zero\t").unwrap_or_else(|| panic!("MARS did not dump registers: {text}"));
    assert_eq!(printed.strip_suffix('\n').unwrap(), output, "MARS output");
    let mut regs = Vec::new();
    let mut data = Vec::new();
    for line in format!("$zero\t{state}").lines() {
        if line.starts_with('$') {
            let (name, value) = line.split_once('\t').unwrap();
            regs.push(
                u32::from_str_radix(value.trim().trim_start_matches("0x"), 16)
                    .unwrap_or_else(|_| panic!("{name}: {value}")),
            );
        } else if line.starts_with("Mem[") {
            data.extend(
                line.split('\t')
                    .skip(1)
                    .filter(|v| v.starts_with("0x"))
                    .map(|v| u32::from_str_radix(&v[2..], 16).unwrap()),
            );
        }
    }
    assert_eq!(regs, machine.cpu.regs, "MARS registers");
    let expected: Vec<u32> =
        RANGES.iter().flat_map(|&(start, end)| (start..=end).step_by(4)).map(|a| machine.mem.peek32(a)).collect();
    assert_eq!(data, expected, "MARS data memory");
}

#[test]
fn adapted_execution() {
    let Some(root) = root() else { return };
    let mars = std::env::var_os("MARS").map(PathBuf::from);
    if mars.is_none() {
        eprintln!("MARS is not set: checking expected results without the reference");
    }
    let temp = std::env::temp_dir().join(format!("remu_ffcabbar_adapted_{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let cases: &[(usize, &str, &str)] = &[
        (1, "7\n3\n", "Sum:10"),
        (1, "-7\n3\n", "Sum:-4"),
        (1, "0\n0\n", "Sum:0"),
        (2, "7\n3\n", "Result(low part):21\nResult(high part):0"),
        (2, "-7\n3\n", "Result(low part):-21\nResult(high part):-1"),
        (2, "2147483647\n2\n", "Result(low part):-2\nResult(high part):0"),
        (3, "7\n3\n", "Division:2\nRemainder:1"),
        (3, "-7\n3\n", "Division:-2\nRemainder:-1"),
        (3, "7\n-3\n", "Division:-2\nRemainder:1"),
        (3, "0\n3\n", "Division:0\nRemainder:0"),
        (4, "7\n3\n", "Lower Part : 2\n11"),
        (4, "-7\n3\n", "Lower Part : -2\n-1-1"),
        (5, "7\n3\n", "a is not equal to be  : "),
        (5, "3\n3\n", "a is equal to be : "),
        (6, "7\n3\n", "You can not sum these numbers because they are NOT same !: "),
        (6, "3\n3\n", "Summation of adding : 6"),
        (7, "7\n3\n", "Substraction result is : 4"),
        (7, "3\n3\n", "Addition result is  : 6"),
        (7, "3\n7\n9\n2\n", "Substraction result is : 7"),
        (8, "0\n", "0! = 1"),
        (8, "1\n", "1! = 1"),
        (8, "5\n", "5! = 120"),
        (8, "12\n", "12! = 479001600"),
        (8, "13\n", "13! = 1932053504"),
        (9, "12345\n", "Sum of digits = 15"),
        (9, "0\n", "Sum of digits = 0"),
        (9, "-123\n", "Sum of digits = -6"),
        (10, "7\n3\n", "10"),
        (10, "-7\n3\n", "-4"),
        (11, "7\n3\n", "Dividition : 2\nRemainder : 1\nMultiplication : 21"),
        (11, "3\n3\n", "Addition : 6"),
        (11, "-7\n3\n", "Dividition : -2\nRemainder : -1\nMultiplication : -21"),
        (12, "7\n3\n1\n", "Addition : \n10"),
        (12, "7\n3\n2\n", "subtraction : 4"),
        (12, "7\n3\n3\n", "dividition : 2"),
        (12, "7\n3\n4\n", "multplication : 21"),
        (12, "7\n3\n5\n", "Remainder is: 1"),
        (12, "-7\n3\n5\n", "Remainder is: -1"),
        (12, "7\n3\n9\n", "Addition : \n10"),
        (13, "", "c:9\na:4\nc:8\na:3\nc:7\na:2\nc:6\na:1\n"),
        (14, "", "6"),
        (15, "", "25\n15"),
        (16, "2\n5\n", "32"),
        (16, "2\n0\n", "1"),
        (16, "-2\n3\n", "-8"),
        (16, "0\n3\n", "0"),
        (17, "2\n5\n", "32"),
        (17, "2\n0\n", "1"),
        (17, "-2\n3\n", "-8"),
        (17, "0\n3\n", "0"),
        (18, "", "5\n3\n4\n6\n1\n0\n1\n3\n4\n5\n"),
        (19, "7\n3\n", "Sum:10"),
    ];
    for &(n, input, expected) in cases {
        let src = adapted(&source(&root, n));
        let (machine, event, output) = run(&src, input, true);
        assert_eq!(event, Event::Terminated(0), "{} input {input:?}", FILES[n - 1]);
        assert!(
            output.ends_with(expected),
            "{} input {input:?}: expected suffix {expected:?}, got {output:?}",
            FILES[n - 1]
        );
        if n == 14 {
            assert_eq!(machine.cpu.regs[29], 0x7fff_eff4, "upstream subtracts from sp twice");
        }
        if n == 17 {
            assert_eq!(machine.cpu.regs[29], 0x7fff_effc, "recursive calls restore sp");
        }
        if n == 18 {
            let array: Vec<u32> = (0..6).map(|i| machine.mem.peek32(0x1001_0004 + i * 4)).collect();
            assert_eq!(array, [0, 1, 3, 4, 5, 6], "upstream bubble sort accesses array[5]");
        }
        if let Some(mars) = &mars {
            let path = temp.join(format!("{n}.asm"));
            std::fs::write(&path, &src).unwrap();
            compare_mars(mars, &path, input, &machine, &output);
        }
        eprintln!(
            "adapted {} input {input:?}: exit 0{}",
            FILES[n - 1],
            if mars.is_some() { ", MARS output/registers/memory match" } else { "" }
        );
    }
    eprintln!("{} execution cases across all 19 examples", cases.len());
    std::fs::remove_dir_all(temp).unwrap();
}
