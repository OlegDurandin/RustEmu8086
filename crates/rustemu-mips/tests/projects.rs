//! Programs of several files, as MARS assembles them (an exception handler file, all the files of
//! a folder): labels local to their file unless `.globl`, `.extern` in the global table, `.text`
//! and `.data` going on from file to file, macros and `.eqv` per file, MARS's errors.
//!
//! When the environment variable `MARS` names MARS (see `mars_compare.rs`), each case is also
//! assembled by MARS with the same files in the same order (`Mars a main.s lib.s …`): the
//! machine code, the data and the errors must be the same.

use std::path::Path;
use std::process::Command;

use rustemu_mips::asm::{SourceFile, assemble_files};
use rustemu_mips::memory::DATA_BASE;
use rustemu_mips::{Machine, Settings};
use rustemu_target::Event;

struct Case {
    name: &'static str,
    /// The files, the program first, in the order of assembly.
    files: &'static [(&'static str, &'static str)],
    /// What the program prints (None: an assembly error is expected).
    output: Option<&'static str>,
}

const CASES: &[Case] = &[
    Case {
        name: "a global procedure and local labels of the same name",
        files: &[
            (
                "main.s",
                ".data\nmsg: .asciiz \"sum = \"\n.text\n.globl main\nmain:\n    li $a0, 4\n    jal sum\n    move $t0, $v0\n    la $a0, msg\n    li $v0, 4\n    syscall\n    move $a0, $t0\n    li $v0, 1\n    syscall\n    j done\nloop: nop\ndone:\n    li $v0, 10\n    syscall\n",
            ),
            (
                "lib.s",
                ".data\nsteps: .word 0\n.text\n.globl sum\nsum:\n    li $v0, 0\nloop:\n    add $v0, $v0, $a0\n    addi $a0, $a0, -1\n    bgtz $a0, loop\n    jr $ra\n",
            ),
        ],
        output: Some("sum = 10"),
    },
    Case {
        name: "data refers to a global label of a later file",
        files: &[
            (
                "main.s",
                ".data\nptr: .word table\n.text\n    lw $t0, ptr\n    lw $a0, 4($t0)\n    li $v0, 1\n    syscall\n",
            ),
            ("lib.s", ".data\n.globl table\ntable: .word 10, 20, 30\n"),
        ],
        output: Some("20"),
    },
    Case {
        name: "extern data in two files",
        files: &[
            (
                "main.s",
                ".extern counter 4\n.text\n    lw $t0, counter\n    addi $t0, $t0, 5\n    sw $t0, counter\n    jal bump\n    lw $a0, counter\n    li $v0, 1\n    syscall\n    li $v0, 10\n    syscall\n",
            ),
            (
                "lib.s",
                ".extern counter 4\n.text\n.globl bump\nbump:\n    lw $t0, counter\n    addi $t0, $t0, 1\n    sw $t0, counter\n    jr $ra\n",
            ),
        ],
        output: Some("6"),
    },
    Case {
        name: "a label of another file that is not global",
        files: &[("main.s", ".text\n    jal helper\n"), ("lib.s", ".text\nhelper:\n    jr $ra\n")],
        output: None,
    },
    Case {
        name: "global but not defined",
        files: &[("main.s", ".text\n.globl main, start\nmain: nop\n"), ("lib.s", ".text\nnop\n")],
        output: None,
    },
    Case {
        name: "the same global in two files",
        files: &[("main.s", ".text\n.globl f\nf: nop\n"), ("lib.s", ".text\n.globl f\nf: jr $ra\n")],
        output: None,
    },
    Case {
        name: "macros and .eqv are per file",
        files: &[
            (
                "main.s",
                ".eqv LIMIT 3\n.macro twice (%r)\n    add %r, %r, %r\n.end_macro\n.text\n    li $t0, LIMIT\n    twice ($t0)\n",
            ),
            ("lib.s", ".text\n    li $t1, LIMIT\n    twice ($t1)\n"),
        ],
        output: None,
    },
];

/// The files of a case as source files.
fn sources(case: &Case) -> Vec<SourceFile> {
    case.files.iter().map(|(n, t)| SourceFile { name: n.to_string(), text: t.to_string() }).collect()
}

/// Our assembly: the code words in address order, the first 16 data words, the errors as
/// `file line L column C: message`.
fn ours(case: &Case) -> (Vec<u32>, Vec<u32>, Vec<String>) {
    let mut files = sources(case);
    let program = files.remove(0);
    match assemble_files(program, None, files, &mut |_| None, &Settings::default()) {
        Ok(p) => {
            let mut m = Machine::new();
            m.load(&p.image).unwrap();
            let data = (0..16).map(|i| m.mem.peek32(DATA_BASE + 4 * i)).collect();
            (p.statements.iter().flat_map(|s| s.words.clone()).collect(), data, Vec::new())
        }
        Err(e) => {
            let mut errors: Vec<String> = e.errors.iter().map(|x| format!("{} {x}", e.files[x.file])).collect();
            errors.sort();
            (Vec::new(), Vec::new(), errors)
        }
    }
}

#[test]
fn programs_of_several_files() {
    for case in CASES {
        let mut files = sources(case);
        let program = files.remove(0);
        let result = assemble_files(program, None, files, &mut |_| None, &Settings::default());
        match (case.output, result) {
            (Some(out), Ok(p)) => {
                let mut m = Machine::new();
                m.load(&p.image).unwrap();
                assert_eq!(m.run(10_000), Event::Terminated(0), "{}", case.name);
                assert_eq!(m.console.text(), out, "{}", case.name);
            }
            (None, Err(e)) => assert!(!e.errors.is_empty(), "{}", case.name),
            (expected, r) => {
                panic!("{}: expected {expected:?}, got {:?}", case.name, r.map(|_| ()).map_err(|e| e.to_string()))
            }
        }
    }
    let (_, _, errors) = ours(&CASES[3]);
    assert_eq!(errors, ["main.s line 2 column 9: Symbol \"helper\" not found in symbol table."]);
    let (_, _, errors) = ours(&CASES[4]);
    assert_eq!(errors, ["main.s line 2 column 14: \"start\" declared global label but not defined."]);
    let (_, _, errors) = ours(&CASES[5]);
    assert_eq!(errors, ["lib.s line 2 column 8: \"f\" already defined as global in a different file."]);
}

#[test]
fn the_exception_handler_file_comes_first() {
    // MARS assembles the handler before the program: its data comes first.
    let handler = SourceFile {
        name: "handler.s".into(),
        text: ".kdata\nsaved: .word 0\n.data\nnote: .asciiz \"h\"\n.ktext 0x80000180\n    mfc0 $k0, $14\n    addi $k0, $k0, 4\n    mtc0 $k0, $14\n    eret\n".into(),
    };
    let program = SourceFile {
        name: "main.s".into(),
        text: ".data\nx: .word 7\n.text\n    li $t0, 0x7fffffff\n    addi $t0, $t0, 1\n    lw $a0, x\n    li $v0, 1\n    syscall\n".into(),
    };
    let p = assemble_files(program, Some(handler), Vec::new(), &mut |_| None, &Settings::default()).unwrap();
    assert_eq!(p.files[0].name, "main.s", "the program is file 0");
    let x = p.symbols.iter().find(|s| s.name == "x").unwrap().addr;
    assert_eq!(x, DATA_BASE + 4, "after the handler's string, aligned");
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    assert_eq!(m.run(1000), Event::Terminated(0));
    assert_eq!(m.console.text(), "7");
}

#[test]
fn projects_match_mars() {
    let Some(mars) = std::env::var_os("MARS") else {
        eprintln!("MARS is not set: skipping the comparison with MARS");
        return;
    };
    let mars = Path::new(&mars).canonicalize().expect("MARS exists");
    let root = std::env::temp_dir().join(format!("rustemu_mars_projects_{}", std::process::id()));
    let mut failures = 0;
    for (k, case) in CASES.iter().enumerate() {
        let dir = root.join(k.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        for (n, t) in case.files {
            std::fs::write(dir.join(n), t).unwrap();
        }
        let (text_dump, data_dump) = (dir.join("text.hex"), dir.join("data.hex"));
        let mut cmd = Command::new("java");
        if mars.extension().is_some_and(|e| e == "jar") {
            cmd.arg("-jar").arg(&mars);
        } else {
            cmd.arg("-cp").arg(&mars).arg("Mars");
        }
        cmd.args(["nc", "a", "dump", ".text", "HexText"])
            .arg(&text_dump)
            .args(["dump", ".data", "HexText"])
            .arg(&data_dump);
        for (n, _) in case.files {
            cmd.arg(dir.join(n));
        }
        let out = cmd.output().expect("java runs");
        let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
        // "Error in /path/lib.s line 2 column 1: …" -> "lib.s line 2 column 1: …"
        let mut their_errors: Vec<String> = text
            .lines()
            .filter_map(|l| l.strip_prefix("Error in "))
            .map(|l| {
                let (path, rest) = l.split_once(" line ").unwrap_or((l, ""));
                let file = Path::new(path).file_name().map_or(path.to_string(), |f| f.to_string_lossy().into_owned());
                format!("{file} line {}", rest.trim_end())
            })
            .collect();
        their_errors.sort();
        let hex = |p: &Path| -> Vec<u32> {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .lines()
                .map(|l| u32::from_str_radix(l.trim(), 16).unwrap())
                .collect()
        };
        let (our_code, our_data, our_errors) = ours(case);
        let (their_code, their_data) = if their_errors.is_empty() {
            (hex(&text_dump), hex(&data_dump).into_iter().take(16).collect::<Vec<_>>())
        } else {
            (Vec::new(), Vec::new())
        };
        let n = their_data.len().min(our_data.len());
        if our_errors != their_errors || our_code != their_code || our_data[..n] != their_data[..n] {
            failures += 1;
            eprintln!(
                "{} differs:\n--- ours\n{our_errors:#?}\n{our_code:08x?}\n{our_data:08x?}\n--- MARS\n{their_errors:#?}\n{their_code:08x?}\n{their_data:08x?}\n{text}",
                case.name
            );
        }
    }
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(failures, 0, "{failures} of {} projects differ from MARS", CASES.len());
}
