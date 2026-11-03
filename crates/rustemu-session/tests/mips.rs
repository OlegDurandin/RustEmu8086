//! Source-level debugging of MIPS programs through the session: lines, symbols, Step Over / Out,
//! watches, breakpoints, Reload.

use rustemu_session::{Arch, Event, Session, StepKind};

const SRC: &str = r#".data
count:  .word 3
        .text
main:   lw $a0, count
        jal twice
        move $s0, $v0
        li $v0, 10
        syscall
twice:  add $v0, $a0, $a0
        jr $ra
"#;

fn session() -> Session {
    let mut s = Session::new_mips();
    s.load_mips_source("twice.s", SRC).unwrap();
    s
}

#[test]
fn lines_and_symbols() {
    let s = session();
    assert_eq!(s.target.arch(), Arch::Mips32);
    assert_eq!(s.current_line(), Some((0, 4)), "lw $a0, count");
    assert_eq!(s.symbol_at(0x0040_0000), Some("main"));
    assert_eq!(s.symbol_at(0x1001_0000), Some("count"));
    let twice = s.addr_of_line(0, 9).unwrap();
    assert_eq!(s.symbol_at(twice), Some("twice"));
    // lw $a0, label expands to two instructions of one line.
    assert_eq!(s.line_at_addr(0x0040_0004), Some((0, 4)));
    assert_eq!(s.line_at_addr(0x0040_0008), Some((0, 5)));
}

#[test]
fn expressions() {
    let mut s = session();
    s.target.run(3);
    let ev = |s: &Session, e: &str| s.compile(e).map(|x| x.eval(&*s.target));
    assert_eq!(ev(&s, "$a0"), Ok(3));
    assert_eq!(ev(&s, "$4 + 1"), Ok(4));
    assert_eq!(ev(&s, "count"), Ok(0x1001_0000), "a label is its address");
    assert_eq!(ev(&s, "dword [count]"), Ok(3));
    assert_eq!(ev(&s, "$ra == main + 8 + 4"), Ok(1), "jal links past itself (no delay slot)");
}

#[test]
fn step_over_and_out() {
    let mut s = session();
    s.target.step();
    s.target.step(); // the two instructions of lw
    assert_eq!(s.current_line(), Some((0, 5)));
    let mut goal = s.step_goal(StepKind::Over, true);
    assert_eq!(s.run_step_goal(&mut goal, 100), Some(Event::Continue));
    assert_eq!(s.current_line(), Some((0, 6)), "over the call");
    assert_eq!(s.target.register(s.target.register_index("$v0").unwrap()), 6);
    let mut s = session();
    while s.current_line() != Some((0, 9)) {
        s.target.step();
    }
    let mut goal = s.step_goal(StepKind::Out, true);
    assert_eq!(s.run_step_goal(&mut goal, 100), Some(Event::Continue));
    assert_eq!(s.current_line(), Some((0, 6)), "out of twice");
}

#[test]
fn breakpoints_reload_and_the_end() {
    let mut s = session();
    let twice = s.addr_of_line(0, 9).unwrap();
    s.target.breakpoints_mut().insert(twice);
    assert_eq!(s.target.run(100), Event::Breakpoint);
    assert_eq!(s.target.pc(), twice);
    assert_eq!(s.call_stack().first().map(|f| f.return_addr), Some(0x0040_000c));
    s.target.breakpoints_mut().clear();
    assert_eq!(s.target.run(100), Event::Terminated(0));
    s.reload().unwrap();
    assert_eq!((s.target.pc(), s.target.terminated()), (0x0040_0000, None));
    assert!(s.load_bytes(rustemu_session::Loadable::Com, vec![0xC3]).is_err(), "no 8086 executables on MIPS");
}

/// The MIPS examples run to the end with the expected output.
#[test]
fn examples_run() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mips");
    let cases: &[(&str, &str, &str)] = &[
        ("hello.s", "", "Hello, MIPS!\n"),
        ("sum.s", "100\n", "n = 100\nsum = 5050"),
        ("fibonacci.s", "", "0 1 1 2 3 5 8 13 21 34 "),
        ("strings.s", "Hello, World\n", "Type a line: Hello, World\nUpper case: HELLO, WORLD\nCharacters: 12"),
        ("sort.s", "", "1 5 7 14 23 32 34 62 "),
        ("exception.s", "", "overflow caught, going on\n42"),
        ("macros.s", "", "Squares: 1 4 9 16 25 \n"),
    ];
    for (file, input, expected) in cases {
        let text = std::fs::read_to_string(dir.join(file)).unwrap();
        let mut s = Session::new_mips();
        // By its path: macros.s includes a file next to it.
        s.load_mips_source(&dir.join(file).to_string_lossy(), &text).unwrap_or_else(|e| panic!("{e}"));
        let m = s.mips_mut().unwrap();
        m.instant_waits = true;
        m.push_text(input);
        assert_eq!(s.target.run(10_000_000), Event::Terminated(0), "{file}");
        assert_eq!(s.mips().unwrap().console.text(), *expected, "{file}");
    }
    // random.s: five numbers below 100, the same on every run.
    let text = std::fs::read_to_string(dir.join("random.s")).unwrap();
    let out = |_: ()| {
        let mut s = Session::new_mips();
        s.load_mips_source("random.s", &text).unwrap();
        s.target.run(1_000_000);
        s.mips().unwrap().console.text()
    };
    let a = out(());
    assert_eq!(a.lines().count(), 5);
    assert!(a.lines().all(|l| l.parse::<u32>().is_ok_and(|v| v < 100)), "{a}");
    assert_eq!(a, out(()));
}

#[test]
fn floating_point_registers_in_expressions() {
    let mut s = Session::new_mips();
    s.load_mips_source("fp.s", ".data\nx: .float 1.5\n.text\nlwc1 $f2, x\nc.le.s 3, $f2, $f2\n").unwrap();
    s.target.run(3);
    let ev = |s: &Session, e: &str| s.compile(e).map(|x| x.eval(&*s.target));
    assert_eq!(ev(&s, "$f2"), Ok(0x3fc0_0000), "the bits of 1.5");
    assert_eq!(ev(&s, "fcc"), Ok(8), "condition flag 3");
}

/// Stepping through a macro of an included file shows its body lines in that file.
#[test]
fn macro_bodies_in_included_files() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mips");
    let path = dir.join("macros.s");
    let mut s = Session::new_mips();
    s.load_mips_source(&path.to_string_lossy(), &std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut seen = Vec::new();
    for _ in 0..6 {
        let v = s.view_at(s.target.pc(), true).unwrap();
        seen.push((v.file, v.line));
        s.target.step();
    }
    assert_eq!(seen[0], (0, 21), "the call of print_str first");
    assert!(seen.contains(&(1, 13)), "then la $a0, text in print_macros.s: {seen:?}");
    assert_eq!(seen[5], (0, 9), "for (line 22) runs square: the body line of for");
}
