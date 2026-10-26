use std::path::PathBuf;

use rustemu_core::Event;
use rustemu_session::Session;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Runs a program to completion feeding `input` as keystrokes.
fn run_src(src: &str, input: &str) -> (Session, Event) {
    let prog = rustemu_asm::assemble_str(src).unwrap_or_else(|e| panic!("{e}"));
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    finish(s, input)
}

fn finish(mut s: Session, input: &str) -> (Session, Event) {
    let mut keys: Vec<u8> = input.bytes().map(|b| if b == b'\n' { b'\r' } else { b }).collect();
    loop {
        match s.machine.run(10_000_000) {
            Event::WaitingInput if !keys.is_empty() => {
                let k = keys.remove(0);
                s.machine.push_char(k);
            }
            ev => return (s, ev),
        }
    }
}

fn run_example(name: &str, input: &str) -> (String, Event) {
    let mut s = Session::new();
    s.load_path(&examples().join(name)).unwrap();
    let (s, ev) = finish(s, input);
    (String::from_utf8_lossy(&s.machine.output).into_owned(), ev)
}

#[test]
fn hello_com_and_exe() {
    let (out, ev) = run_example("hello.asm", "");
    assert_eq!(out, "Hello, World!\r\n");
    assert_eq!(ev, Event::Terminated(0));
    let (out, ev) = run_example("hello_exe.asm", "");
    assert_eq!(out, "Hello from EXE!\r\n");
    assert_eq!(ev, Event::Terminated(0));
}

#[test]
fn keyboard_input_examples() {
    let (out, _) = run_example("echo.asm", "Mixed Case\n");
    assert!(out.ends_with("MIXED CASE"), "{out:?}");
    let (out, ev) = run_example("calc.asm", "100\n-7\n");
    assert!(out.contains("Sum = 93"), "{out:?}");
    assert_eq!(ev, Event::Terminated(0));
}

#[test]
fn screen_and_bios_video() {
    let src = "\
org 100h
    mov ax, 3
    int 10h
    mov ah, 2
    mov bh, 0
    mov dx, 0510h       ; row 5, col 16
    int 10h
    mov ah, 9
    mov al, 'X'
    mov bl, 1Eh
    mov cx, 3
    int 10h
    mov ax, 0B800h
    mov es, ax
    mov byte ptr es:[0], 'A'
    mov byte ptr es:[1], 4Fh
    ret
";
    let (s, ev) = run_src(src, "");
    assert_eq!(ev, Event::Terminated(0));
    let screen = s.machine.screen_text();
    let lines: Vec<&str> = screen.lines().collect();
    assert_eq!(lines[0], "A");
    assert_eq!(lines[5], format!("{}XXX", " ".repeat(16)));
    assert_eq!(s.machine.sys.mem.read8(0xB8000 + (5 * 80 + 16) * 2 + 1), 0x1E);
}

#[test]
fn teletype_scrolls() {
    let src = "org 100h\nmov cx, 30\nl: mov ah, 2\nmov dl, 'a'\nint 21h\nmov dl, 13\nint 21h\nmov dl, 10\nint 21h\nloop l\nret\n";
    let (s, _) = run_src(src, "");
    let text = s.machine.screen_text();
    assert_eq!(text.lines().filter(|l| *l == "a").count(), 24);
    let (_, row) = rustemu_core::video::Video::cursor(&s.machine.sys.mem, 0);
    assert_eq!(row, 24);
}

#[test]
fn divide_error_stops() {
    let (s, ev) = run_src("org 100h\nmov bl, 0\ndiv bl\nret\n", "");
    assert!(matches!(ev, Event::Error(_)));
    // Stopped right after the faulting instruction, back in user code.
    assert_eq!(s.machine.cpu.ip, 0x104);
}

#[test]
fn int3_is_breakpoint_and_user_vectors_work() {
    let src = "\
org 100h
    ; hook INT 60h
    mov ax, 2560h
    mov dx, offset handler
    int 21h
    int 60h
    int 3
    mov ax, 4C07h
    int 21h
handler:
    mov bx, 1234h
    iret
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.run(1000), Event::Breakpoint);
    assert_eq!(s.machine.cpu.regs[3], 0x1234);
    assert_eq!(s.machine.run(1000), Event::Terminated(7));
}

#[test]
fn step_back_restores_registers_and_memory() {
    let prog = rustemu_asm::assemble_str("org 100h\nmov ax, 1\nmov [200h], ax\ninc ax\nret\n").unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    let base = s.machine.cpu.pc();
    for _ in 0..3 {
        assert_eq!(s.machine.step(), Event::Continue);
    }
    assert_eq!(s.machine.cpu.regs[0], 2);
    let a = rustemu_core::linear(s.machine.cpu.sregs[3], 0x200);
    assert_eq!(s.machine.sys.mem.read16(a), 1);
    assert!(s.machine.step_back());
    assert!(s.machine.step_back());
    assert_eq!(s.machine.cpu.regs[0], 1);
    assert_eq!(s.machine.sys.mem.read16(a), 0);
    assert!(s.machine.step_back());
    assert_eq!(s.machine.cpu.pc(), base);
    assert!(!s.machine.step_back());
}

#[test]
fn breakpoints_and_line_mapping() {
    let src = "org 100h\nstart:\n    mov cx, 3\nagain:\n    dec cx\n    jnz again\n    ret\n";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    assert_eq!(s.current_line(), Some((0, 3)));
    let bp = s.addr_of_line(0, 5).unwrap();
    s.machine.breakpoints.insert(bp);
    assert_eq!(s.machine.run(100), Event::Breakpoint);
    assert_eq!(s.current_line(), Some((0, 5)));
    assert_eq!(s.machine.cpu.regs[1], 3);
    assert_eq!(s.machine.run(100), Event::Breakpoint);
    assert_eq!(s.machine.cpu.regs[1], 2);
}

#[test]
fn dos_files_in_sandbox() {
    let dir = std::env::temp_dir().join(format!("rustemu_dos_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = "\
org 100h
    mov ah, 3Ch          ; create
    mov cx, 0
    mov dx, offset fname
    int 21h
    jc fail
    mov bx, ax
    mov ah, 40h          ; write
    mov cx, 5
    mov dx, offset data
    int 21h
    mov ah, 3Eh          ; close
    int 21h
    mov ax, 3D00h        ; open for reading
    mov dx, offset fname
    int 21h
    mov bx, ax
    mov ah, 3Fh
    mov cx, 10
    mov dx, offset buf
    int 21h
    mov cx, ax           ; bytes read
    mov ah, 3Eh
    int 21h
    mov ax, 4C00h
    int 21h
fail:
    mov ax, 4C01h
    int 21h
fname db 'c:\\test.txt', 0
data  db 'hello'
buf   db 10 dup(0)
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.machine.set_dos_root(dir.clone());
    s.load_program(prog).unwrap();
    let (s, ev) = finish(s, "");
    assert_eq!(ev, Event::Terminated(0));
    assert_eq!(s.machine.cpu.regs[1], 5);
    assert_eq!(std::fs::read(dir.join("test.txt")).unwrap(), b"hello");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn devices_ports() {
    let src = "org 100h\nmov ax, 0249h\nout 4, ax\nmov al, 'P'\nout 130, al\nmov ax, 1234\nout 199, ax\nret\n";
    let (s, _) = run_src(src, "");
    assert_eq!(s.machine.sys.dev.traffic_lights, 0x0249);
    assert_eq!(s.machine.sys.dev.printer, b"P");
    assert_eq!(s.machine.sys.dev.led, 1234);
}

#[test]
fn graphics_mode_13h() {
    let src = "org 100h\nmov ax, 13h\nint 10h\nmov ah, 0Ch\nmov al, 4\nmov cx, 10\nmov dx, 20\nint 10h\nmov ah, 0Dh\nint 10h\nret\n";
    let (s, _) = run_src(src, "");
    assert_eq!(s.machine.sys.mem.read8(0xA0000 + 20 * 320 + 10), 4);
    assert_eq!(s.machine.cpu.regs[0] & 0xFF, 4);
}

#[test]
fn waiting_for_input_does_not_flood_history() {
    let prog = rustemu_asm::assemble_str("org 100h\nmov ah, 0\nint 16h\nret\n").unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.step(), Event::WaitingInput);
    let n = s.machine.history_len();
    for _ in 0..10 {
        assert_eq!(s.machine.step(), Event::WaitingInput);
    }
    assert_eq!(s.machine.history_len(), n);
    s.machine.push_char(b'z');
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.cpu.regs[0] & 0xFF, b'z' as u16);
    // Back in user code right after INT 16h.
    assert_eq!(s.machine.cpu.ip, 0x104);
}

#[test]
fn current_line_while_waiting_for_key_is_the_int() {
    let src = "org 100h\nmov ah, 0\nint 16h\nret\n";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.run(100), Event::WaitingInput);
    assert_eq!(s.current_line(), Some((0, 3)));
}

#[test]
fn robot_example_lights_lamps() {
    use rustemu_core::robot::Cell;
    let mut s = Session::new();
    s.load_path(&examples().join("robot.asm")).unwrap();
    s.machine.sys.dev.clock = rustemu_core::devices::DeviceClock::Instructions(10);
    s.machine.history_enabled = false;
    assert_eq!(s.machine.run(20_000_000), Event::Budget);
    let r = &s.machine.sys.dev.robot;
    let lit = r.cells.iter().filter(|c| **c == Cell::Lamp { on: true }).count();
    assert!(lit >= 2, "only {lit} lamps lit");
}

#[test]
fn bios_tick_counter_advances() {
    // Waits for 5 timer ticks by polling 0040:006C.
    let src = "\
org 100h
    mov ax, 40h
    mov es, ax
    mov bx, es:[6Ch]
    add bx, 5
wait_t:
    cmp es:[6Ch], bx
    jne wait_t
    ret
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.machine.instructions_per_tick = Some(1000);
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.run(100_000), Event::Terminated(0));
    assert!(s.machine.cpu.instr_count >= 4000);
}

#[test]
fn timer_irq_calls_int_1ch() {
    let src = "\
org 100h
    mov ax, 251Ch
    mov dx, offset tick
    int 21h
    sti
wait_c:
    cmp count, 3
    jb wait_c
    mov ax, 4C00h
    int 21h
tick:
    inc cs:count
    iret
count dw 0
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.machine.instructions_per_tick = Some(500);
    s.machine.timer_irq = true;
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.run(100_000), Event::Terminated(0));
    // The ROM INT 08h handler also advanced the BIOS tick count.
    assert!(s.machine.sys.mem.read16(0x46C) >= 3);
}

#[test]
fn macro_code_maps_to_library_lines() {
    let src = "include 'emu8086.inc'\norg 100h\n    print 'Hi'\n    call scan_num\n    ret\nDEFINE_SCAN_NUM\nend\n";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let inc = prog.files.iter().position(|f| f == "emu8086.inc").unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    // First instruction of PRINT: invocation line 3, origin inside emu8086.inc.
    let loc = s.current_location().unwrap();
    assert_eq!((loc.file, loc.line), (0, 3));
    let (of, ol) = loc.origin.unwrap();
    assert_eq!(of, inc);
    let lib: Vec<&str> = rustemu_asm::EMU8086_INC.lines().collect();
    assert!(lib[ol as usize - 1].trim().to_ascii_uppercase().starts_with("PUSH"));
    // The call target is named.
    let call_target = (0..200u32).map(|i| s.image_base().unwrap() + i).find(|a| s.symbol_at(*a) == Some("SCAN_NUM"));
    assert!(call_target.is_some());
    // A breakpoint on a library line resolves to an address.
    assert!(!s.addrs_of_line(inc, ol).is_empty());
}

mod source_stepping {
    use super::*;
    use rustemu_session::{StepKind, View};

    fn session(src: &str) -> Session {
        let prog = rustemu_asm::assemble_str(src).unwrap_or_else(|e| panic!("{e}"));
        let mut s = Session::new();
        s.load_program(prog).unwrap();
        s
    }

    /// Runs a step to completion, typing `keys` when input is requested.
    fn step(s: &mut Session, kind: StepKind, keys: &str) -> Event {
        let mut goal = s.step_goal(kind, true);
        let mut keys: Vec<u8> = keys.bytes().collect();
        loop {
            match s.run_step_goal(&mut goal, 1_000_000) {
                Some(Event::WaitingInput) if !keys.is_empty() => s.machine.push_char(keys.remove(0)),
                Some(ev) => return ev,
                None => panic!("step did not finish"),
            }
        }
    }

    fn view(s: &Session) -> View {
        s.current_view(true).unwrap()
    }

    const SRC: &str = "\
include 'emu8086.inc'
org 100h
        print 'Number: '
        call scan_num
        mov ax, cx
        call twice
        ret
twice   proc
        push ax
        add ax, ax
        pop bx
        ret
twice   endp
DEFINE_SCAN_NUM
end
";

    #[test]
    fn step_over_runs_macros_and_calls() {
        let mut s = session(SRC);
        // At the first instruction of PRINT we are "on" the program line, not inside the macro.
        assert_eq!((view(&s).level, view(&s).line), (0, 3));
        assert_eq!(step(&mut s, StepKind::Over, ""), Event::Continue);
        assert_eq!((view(&s).level, view(&s).line), (0, 4));
        assert_eq!(step(&mut s, StepKind::Over, "42\r"), Event::Continue);
        assert_eq!((view(&s).level, view(&s).line), (0, 5));
        assert_eq!(s.machine.cpu.regs[1], 42);
        assert_eq!(String::from_utf8_lossy(&s.machine.output), "Number: 42\r");
        // PUSH inside the line being stepped does not confuse step over.
        assert_eq!(step(&mut s, StepKind::Over, ""), Event::Continue);
        assert_eq!(step(&mut s, StepKind::Over, ""), Event::Continue);
        assert_eq!((view(&s).level, view(&s).line), (0, 7));
    }

    #[test]
    fn single_step_enters_macro_and_step_out_leaves_it() {
        let mut s = session(SRC);
        s.machine.step(); // PUSH AX of PRINT
        let v = view(&s);
        assert_eq!(v.level, 1);
        assert_eq!(v.statement, (0, 3));
        assert_eq!(step(&mut s, StepKind::Out, ""), Event::Continue);
        assert_eq!((view(&s).level, view(&s).line), (0, 4));
    }

    #[test]
    fn step_out_of_library_procedure_and_user_procedure() {
        let mut s = session(SRC);
        step(&mut s, StepKind::Over, ""); // print
        s.machine.step(); // CALL SCAN_NUM -> inside the procedure
        assert_eq!(view(&s).level, 1);
        for _ in 0..4 {
            s.machine.step();
        }
        // Inside SCAN_NUM: waiting for keys, then out to the caller.
        assert_eq!(step(&mut s, StepKind::Out, "7\r"), Event::Continue);
        assert_eq!((view(&s).level, view(&s).line), (0, 5));
        s.machine.step(); // mov ax, cx
        s.machine.step(); // call twice
        s.machine.step(); // push ax (inside twice)
        assert_eq!(view(&s).line, 10);
        assert_eq!(step(&mut s, StepKind::Out, ""), Event::Continue);
        assert_eq!(view(&s).line, 7);
        assert_eq!(s.machine.cpu.regs[0], 14);
    }

    #[test]
    fn step_over_recursive_call() {
        let mut s = Session::new();
        s.load_path(&examples().join("factorial.asm")).unwrap();
        // Advance to the "call fact" line.
        for _ in 0..50 {
            if s.session_line_text().starts_with("call fact") {
                break;
            }
            step(&mut s, StepKind::Over, "");
        }
        assert!(s.session_line_text().starts_with("call fact"));
        let before = view(&s).line;
        assert_eq!(step(&mut s, StepKind::Over, ""), Event::Continue);
        assert_eq!(view(&s).line, before + 1);
    }

    trait LineText {
        fn session_line_text(&self) -> String;
    }

    impl LineText for Session {
        fn session_line_text(&self) -> String {
            let v = self.current_view(true).unwrap();
            let text = std::fs::read_to_string(examples().join("factorial.asm")).unwrap();
            text.lines().nth(v.line as usize - 1).unwrap().trim().to_string()
        }
    }
}

#[test]
fn command_line_parameters_in_psp() {
    let src = "org 100h\nmov cl, [80h]\nmov al, [81h]\nmov ah, [83h]\nmov bl, [84h]\nret\n";
    let prog = asm_ok(src);
    let mut s = Session::new();
    s.command_line = "ab".into();
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.run(100), Event::Terminated(0));
    let c = &s.machine.cpu;
    assert_eq!(c.regs[1] & 0xFF, 3); // " ab"
    assert_eq!(c.regs[0], ((0x62u16) << 8) | 0x20); // ' ' at 81h, 'b' at 83h
    assert_eq!(c.regs[3] & 0xFF, 0x0D);
}

fn asm_ok(src: &str) -> rustemu_asm::Program {
    rustemu_asm::assemble_str(src).unwrap()
}

#[test]
fn call_stack_and_expressions() {
    let src = "org 100h
    mov bx, 3
    call outer
    ret
outer proc
    push ax
    call inner
    pop ax
    ret
outer endp
inner proc
    int 3
    ret
inner endp
value dw 1234h
table db 1, 2, 3
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.run(1000), Event::Breakpoint);
    let frames = s.call_stack();
    let lines: Vec<u32> = frames.iter().map(|f| s.line_at_addr(f.call_addr).unwrap().1).collect();
    assert_eq!(lines, [7, 3], "{frames:?}");
    let names: Vec<&str> = frames.iter().map(|f| s.symbol_at(f.target.unwrap()).unwrap()).collect();
    assert_eq!(names, ["INNER", "OUTER"]);
    let ev = |e: &str| s.compile(e).map(|x| x.eval(&s.machine));
    assert_eq!(ev("value"), Ok(0x1234));
    assert_eq!(ev("table[bx-1]"), Ok(3));
    assert_eq!(ev("byte [offset table + 1]"), Ok(2));
    assert_eq!(ev("value == 1234h && bx = 3"), Ok(1));
    assert!(ev("nothing").is_err());

    // EXE: variables live in the data segment.
    let exe = "data segment
    n dw 77
data ends
code segment
start:
    mov ax, data
    mov ds, ax
    int 3
code ends
end start
";
    let mut s = Session::new();
    s.load_program(rustemu_asm::assemble_str(exe).unwrap()).unwrap();
    assert_eq!(s.machine.run(1000), Event::Breakpoint);
    assert_eq!(s.compile("n").unwrap().eval(&s.machine), 77);
    assert_eq!(s.compile("ds == data").unwrap().eval(&s.machine), 1);
}
