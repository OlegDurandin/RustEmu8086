//! Regression tests exercise the public executable, including parsing and exit codes.
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rustemu_cli_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(name), bytes).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rustemu"))
            .current_dir(&self.0)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap()
}
fn registers(out: &Output, mips: bool) -> String {
    let text = stderr(out);
    let start = text.find(if mips { "$zero =" } else { "AX=" }).unwrap();
    // x86 trace lines also contain AX; final register dump begins on a separate line.
    if mips {
        text[start..].to_string()
    } else {
        text.lines().filter(|l| l.starts_with("AX=") || l.starts_with("CS=")).collect::<Vec<_>>().join("\n")
    }
}

#[test]
fn trace_preserves_multiple_mmio_keys_console_input_exit_and_registers() {
    let f = Fixture::new();
    f.write(
        "keys.s",
        r#".text
li $t0, 0xffff0000
li $t2, 3
poll:
lw $t1, 0($t0)
andi $t1, $t1, 1
beqz $t1, poll
lw $a0, 4($t0)
li $v0, 11
syscall
addiu $t2, $t2, -1
bnez $t2, poll
li $v0, 12
syscall
move $a0, $v0
li $v0, 11
syscall
li $a0, 7
li $v0, 17
syscall
"#,
    );
    let args = ["run", "keys.s", "--keys", "AB\\n", "--input", "Z", "--max", "200", "--regs"];
    let normal = f.run(&args);
    let traced = f.run(&[args.as_slice(), &["--trace"]].concat());
    assert_eq!(normal.status.code(), Some(7), "{}", stderr(&normal));
    assert_eq!(traced.status.code(), Some(7), "{}", stderr(&traced));
    assert_eq!(normal.stdout, b"AB\nZZ");
    assert_eq!(normal.stdout, traced.stdout);
    assert_eq!(registers(&normal, true), registers(&traced, true));
}

#[test]
fn trace_preserves_dos_input_and_exit() {
    let f = Fixture::new();
    f.write("echo.asm", "org 100h\nmov ah, 1\nint 21h\nmov dl, al\nmov ah, 2\nint 21h\nmov ax, 4c07h\nint 21h\n");
    let args = ["run", "echo.asm", "--input", "A", "--regs", "--max=100"];
    let normal = f.run(&args);
    let traced = f.run(&[args.as_slice(), &["--trace"]].concat());
    assert_eq!(normal.status.code(), Some(7), "{}", stderr(&normal));
    assert_eq!(traced.status.code(), Some(7), "{}", stderr(&traced));
    assert_eq!(normal.stdout, b"AA");
    assert_eq!(normal.stdout, traced.stdout);
    assert_eq!(registers(&normal, false), registers(&traced, false));
}

#[test]
fn trace_and_normal_stop_at_the_same_instruction_for_both_processors() {
    let f = Fixture::new();
    f.write("loop.s", "loop: addiu $t0, $t0, 1\nb loop\n");
    f.write("loop.asm", "org 100h\nagain: inc ax\njmp again\n");
    for (file, mips) in [("loop.s", true), ("loop.asm", false)] {
        for max in ["0", "1", "2", "9"] {
            let args = ["run", file, "--max", max, "--regs"];
            let normal = f.run(&args);
            let traced = f.run(&[args.as_slice(), &["--trace"]].concat());
            assert_eq!(normal.status.code(), Some(4), "{}", stderr(&normal));
            assert_eq!(traced.status.code(), Some(4));
            assert_eq!(registers(&normal, mips), registers(&traced, mips), "{file} max={max}");
            let lines = stderr(&traced)
                .lines()
                .filter(|l| if mips { l.starts_with("0040") } else { l.starts_with("0700:") })
                .count();
            assert_eq!(lines, max.parse::<usize>().unwrap(), "{file}");
        }
    }
}

#[test]
fn invalid_options_fail_before_execution_or_output_file_creation() {
    let f = Fixture::new();
    f.write("exit.s", "li $a0, 88\nli $v0, 11\nsyscall\nli $v0, 10\nsyscall\n");
    for options in [
        vec!["--max"],
        vec!["--max", "--regs"],
        vec!["--max=nonsense"],
        vec!["--max=-1"],
        vec!["--max=18446744073709551616"],
        vec!["--unknown"],
        vec!["--trace=true"],
        vec!["--max=1", "--max=2"],
        vec!["--arch=bad"],
        vec!["--memory=bad"],
        vec!["--screen"],
        vec!["--syntax=nasm"],
    ] {
        let out = f.run(&[&["run", "exit.s"][..], &options].concat());
        assert_eq!(out.status.code(), Some(1), "{options:?}: {}", stderr(&out));
        assert!(out.stdout.is_empty(), "{options:?}");
        assert!(stderr(&out).contains("error:"));
    }
    let out = f.run(&["asm", "exit.s", "-o", "output.bin", "--unknown"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!f.0.join("output.bin").exists());
    for options in
        [vec!["--org=xyz"], vec!["--org=10000"], vec!["--org=100000000", "--arch=mips"], vec!["--org"], vec!["--mars"]]
    {
        let out = f.run(&[&["disasm", "missing.bin"][..], &options].concat());
        assert_eq!(out.status.code(), Some(1));
        assert!(stderr(&out).contains("--"), "{}", stderr(&out));
        assert!(!stderr(&out).contains("No such file"), "options must be validated before opening the file");
    }
}

#[test]
fn valid_assembly_origins_help_and_literal_option_values_work() {
    let f = Fixture::new();
    f.write("-test.asm", "bits 16\norg 100h\nmov ax, 42\nret\n");
    let out = f.run(&["asm", "--syntax=nasm", "-o=out.bin", "-l", "-s", "--", "-test.asm"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(std::fs::read(f.0.join("out.bin")).unwrap(), [0xb8, 42, 0, 0xc3]);
    assert!(f.0.join("-test.lst").exists());
    assert!(f.0.join("-test.symbol").exists());
    for org in ["0x100", "100h", "0100"] {
        let out = f.run(&["disasm", "out.bin", "--org", org]);
        assert!(out.status.success());
        assert!(out.stdout.starts_with(b"0100"));
    }
    let out = f.run(&["disasm", "out.bin", "--arch=mips", "--org=F0000000"]);
    assert!(out.status.success());
    assert!(out.stdout.starts_with(b"f0000000"));
    assert!(f.run(&["--help"]).status.success());
    assert!(f.run(&["run", "--help"]).status.success());
    f.write("input.s", "li $v0, 12\nsyscall\nmove $a0, $v0\nli $v0, 11\nsyscall\nli $v0, 10\nsyscall\n");
    let out = f.run(&["run", "input.s", "--input=--trace"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(out.stdout, b"--");
    assert!(out.stderr.is_empty());
}
