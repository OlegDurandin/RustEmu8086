//! Small semantic checks in ordinary CI, separate from corpus assembly statistics.
use rustemu_core::cpu::{AX, CX, DI, ES, flags::ZF};
use rustemu_session::{Event, Session};

#[test]
fn scasw_example_matches_word_and_returns_to_its_start() {
    let source = include_str!("../../../examples/scasw.asm");
    let p = rustemu_asm::assemble_str(source).unwrap();
    let base = p.symbols.iter().find(|s| s.name == "DAT1").unwrap().value as u16;
    let mut s = Session::new();
    s.load_program(p).unwrap();
    let mut saw_search = false;
    for _ in 0..100 {
        let ev = s.target.step();
        let m = s.x86().unwrap();
        if !saw_search && m.cpu.regs[DI] == base + 6 && m.cpu.regs[CX] == 1 && m.cpu.flags & ZF != 0 {
            assert_eq!(m.cpu.regs[AX], 0x9075);
            assert_eq!(m.cpu.sregs[ES], m.cpu.cs());
            saw_search = true;
        }
        if ev == Event::WaitingInput {
            assert!(saw_search, "observe the state immediately after REPNE SCASW");
            assert_eq!(m.cpu.regs[DI], base + 4, "the example returns to the matched word");
            let addr = rustemu_core::linear(m.cpu.sregs[ES], m.cpu.regs[DI]);
            assert_eq!(m.sys.mem.read16(addr), 0x9075);
            assert_eq!(s.target.output(), b"\"yes\" - found!\r\n");
            s.target.push_char(b'x');
            assert_eq!(s.target.run(100), Event::Terminated(0));
            return;
        }
    }
    panic!("example did not reach its keyboard wait");
}

#[test]
fn a86_constants_include_and_macro_have_the_expected_semantics() {
    struct Includes;
    impl rustemu_asm::FileLoader for Includes {
        fn load(&mut self, name: &str, _: &str) -> Option<(String, String)> {
            (name == "set.inc").then(|| (name.into(), "setax macro\nmov ax,0100\n#em\n".into()))
        }
    }
    let p = rustemu_asm::assemble_dialect(
        "main.a86",
        "include 'set.inc'\nsetax\nmov bx,ax\nmov dl,'A'\nmov ah,02\nint 021\nint 020\nvalue dw 0100\n",
        &mut Includes,
        Some(rustemu_asm::Dialect::A86),
    )
    .unwrap();
    assert!(p.image.windows(2).any(|w| w == [0xcd, 0x21]));
    assert!(p.image.ends_with(&[0, 1]));
    let mut s = Session::new();
    s.load_program(p).unwrap();
    assert_eq!(s.target.run(100), Event::Terminated(0));
    assert_eq!(s.compile("bx").unwrap().eval(&*s.target), 256);
    assert_eq!(s.target.output(), b"A");
}

#[test]
fn mips_pseudo_io_memory_and_delay_slots_are_distinct() {
    let src = ".data\nvalue: .word 0\n.text\nmain: li $v0,5\nsyscall\nmove $t0,$v0\nli $t1,0x12345678\nsw $t0,value\nli $t2,0\nb target\naddiu $t2,$t2,1\ntarget: move $a0,$t0\nli $v0,1\nsyscall\nli $v0,10\nsyscall\n";
    for delayed in [false, true] {
        let mut s = Session::new_mips();
        let settings = rustemu_mips::Settings { delayed_branching: delayed, ..Default::default() };
        s.load_mips_source_with("test.s", src, &settings).unwrap();
        assert_eq!(s.target.run(100), Event::WaitingInput);
        for b in b"42\n" {
            s.target.push_char(*b);
        }
        assert_eq!(s.target.run(100), Event::Terminated(0));
        for (expr, want) in [("$t0", 42), ("$t1", 0x12345678), ("$t2", i64::from(delayed)), ("dword [value]", 42)] {
            assert_eq!(s.compile(expr).unwrap().eval(&*s.target), want, "{expr}, delay={delayed}");
        }
        assert!(String::from_utf8_lossy(s.target.output()).ends_with("42"));
    }
}
