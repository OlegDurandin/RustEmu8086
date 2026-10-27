use std::path::PathBuf;

use rustemu_core::Event;
use rustemu_session::{Loadable, Session};

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
fn failed_load_keeps_the_previous_session_intact() {
    let original = rustemu_asm::assemble_str("org 100h\nmov ax, 1234h\nhlt\n").unwrap();
    let mut s = Session::new();
    s.load_program(original).unwrap();
    assert_eq!(s.machine.step(), Event::Continue);
    let old_cpu = s.machine.cpu.clone();
    let old_info = s.machine.loaded.clone();
    let old_image = s.program.as_ref().unwrap().image.clone();

    let mut oversized = rustemu_asm::assemble_str("org 100h\nnop\n").unwrap();
    oversized.image.resize(0xFF00, 0);
    assert!(s.load_program(oversized).is_err());

    assert_eq!(s.machine.cpu, old_cpu);
    assert_eq!(s.machine.loaded, old_info);
    assert_eq!(s.program.as_ref().unwrap().image, old_image);

    // This MZ gets past the basic header checks but has a truncated relocation
    // table. Validation must finish before the old machine is reset.
    let mut corrupt_exe = vec![0; 0x20];
    corrupt_exe[..2].copy_from_slice(b"MZ");
    let mut word = |at: usize, value: u16| corrupt_exe[at..at + 2].copy_from_slice(&value.to_le_bytes());
    word(2, 0x20); // bytes in the last page
    word(4, 1); // pages
    word(6, 1); // one relocation
    word(8, 2); // 32-byte header
    word(0x18, 0x1F); // relocation starts one byte before EOF
    assert!(s.load_bytes(Loadable::Exe, corrupt_exe).is_err());
    assert_eq!(s.machine.cpu, old_cpu);
    assert_eq!(s.machine.loaded, old_info);
    assert_eq!(s.program.as_ref().unwrap().image, old_image);

    // Reload still refers to the original program, not either failed attempt.
    s.reload().unwrap();
    assert_eq!(s.program.as_ref().unwrap().image, old_image);
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.cpu.regs[0], 0x1234);

    s.load_bytes(Loadable::Com, vec![0xF4]).unwrap();
    assert!(s.program.is_none());
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
fn step_back_restores_input_devices_and_floppy() {
    let input =
        rustemu_asm::assemble_str("org 100h\nmov dx, offset buf\nmov ah, 0Ah\nint 21h\nhlt\nbuf db 10, 0, 10 dup(0)\n")
            .unwrap();
    let mut s = Session::new();
    s.load_program(input).unwrap();
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.step(), Event::Continue);
    s.machine.push_char(b'a');
    assert_eq!(s.machine.step(), Event::WaitingInput);
    assert!(s.machine.step_back());
    assert_eq!(s.machine.step(), Event::WaitingInput);
    s.machine.push_char(b'\r');
    assert_eq!(s.machine.step(), Event::Continue);
    let ds = s.machine.cpu.sregs[3];
    let buf = s.program.as_ref().unwrap().symbols.iter().find(|v| v.name == "BUF").unwrap().value as u16;
    assert_eq!(s.machine.sys.mem.read8(rustemu_core::linear(ds, buf + 1)), 1);
    assert_eq!(s.machine.sys.mem.read8(rustemu_core::linear(ds, buf + 2)), b'a');

    let io = rustemu_asm::assemble_str(
        "org 100h\nmov ax, 0249h\nout 4, ax\nmov ax, 0301h\nmov bx, offset data\nmov cx, 1\nmov dx, 0\nint 13h\nhlt\ndata db 512 dup(0AAh)\n",
    )
    .unwrap();
    s.load_program(io).unwrap();
    assert!(s.machine.floppies[0].is_empty());
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.sys.dev.traffic_lights, 0x0249);
    assert!(s.machine.step_back());
    assert_eq!(s.machine.sys.dev.traffic_lights, 0);
    assert_eq!(s.machine.step(), Event::Continue);
    for _ in 0..4 {
        assert_eq!(s.machine.step(), Event::Continue);
    }
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.floppies[0].len(), 1_474_560);
    assert_eq!(s.machine.floppies[0][0], 0xAA);
    assert!(s.machine.step_back());
    assert!(s.machine.floppies[0].is_empty());
}

#[test]
fn int13_rejects_invalid_floppy_geometry() {
    let src = "\
org 100h
    mov ax, 0200h       ; zero sectors
    mov cx, 0001h
    xor dx, dx
    int 13h
    jnc fail
    mov ax, 0201h       ; sector 19
    mov cx, 0013h
    int 13h
    jnc fail
    mov cx, 0001h       ; head 2
    mov dx, 0200h
    int 13h
    jnc fail
    mov cx, 5001h       ; cylinder 80
    xor dx, dx
    int 13h
    jnc fail
    mov ax, 4C00h
    int 21h
fail:
    mov ax, 4C01h
    int 21h
";
    let (_, ev) = run_src(src, "");
    assert_eq!(ev, Event::Terminated(0));
}

#[test]
fn host_file_io_is_a_step_back_boundary() {
    let root = std::env::temp_dir().join(format!("rustemu_history_boundary_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prog = rustemu_asm::assemble_str(
        "org 100h\nmov ah, 3Ch\nmov dx, offset name\nint 21h\nhlt\nname db 'created.txt', 0\n",
    )
    .unwrap();
    let mut s = Session::new();
    s.machine.set_dos_root(root.clone());
    s.load_program(prog).unwrap();
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.step(), Event::Continue);
    assert!(s.machine.history_len() >= 2);
    assert_eq!(s.machine.step(), Event::Continue);
    assert_eq!(s.machine.history_len(), 0);
    assert!(!s.machine.step_back());
    assert!(root.join("created.txt").exists());
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
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

#[test]
fn step_back_through_mixed_light_and_device_steps() {
    // Plain instructions (light snapshots) mixed with port I/O and a BIOS call (full snapshots).
    let src = "org 100h
    mov ax, 1234h
    out 199, ax
    mov cx, 3
next:
    inc bx
    mov [500h], bx
    loop next
    in al, 125
    mov ah, 1
    int 16h
    mov ax, 5678h
    out 199, ax
    hlt
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    let cpu0 = s.machine.cpu.clone();
    let led0 = s.machine.sys.dev.led;
    let mem0 = s.machine.sys.mem.read16(rustemu_core::linear(s.machine.cpu.sregs[3], 0x500));
    let mut steps = 0;
    while s.machine.step() == Event::Continue {
        steps += 1;
    }
    assert_eq!(s.machine.sys.dev.led, 0x5678);
    assert!(s.machine.history_len() > steps - 1);
    while s.machine.step_back() {}
    assert!(s.machine.cpu.same_state(&cpu0));
    assert_eq!(s.machine.sys.dev.led, led0);
    assert_eq!(s.machine.sys.mem.read16(rustemu_core::linear(s.machine.cpu.sregs[3], 0x500)), mem0);
    // And forward again gives the same result.
    while s.machine.step() == Event::Continue {}
    assert_eq!(s.machine.sys.dev.led, 0x5678);
    assert_eq!(s.machine.cpu.regs[3], 3);
}

#[test]
fn saved_state_restores_the_whole_machine() {
    let src = "org 100h\n    mov cx, 5\nnext:\n    inc word ptr [200h]\n    mov ah, 2\n    mov dl, 'x'\n    int 21h\n    loop next\n    hlt\n";
    let mut s = Session::new();
    s.load_program(rustemu_asm::assemble_str(src).unwrap()).unwrap();
    for _ in 0..6 {
        s.machine.step();
    }
    let saved = s.save_state();
    let (cpu, out) = (s.machine.cpu.clone(), s.machine.output.clone());
    assert_eq!(s.machine.run(1000), Event::Halted);
    let (end_cpu, end_out) = (s.machine.cpu.clone(), s.machine.output.clone());
    s.restore_state(saved);
    assert!(s.machine.cpu.same_state(&cpu));
    assert_eq!(s.machine.output, out);
    assert_eq!(s.current_line(), s.line_at_addr(s.machine.cpu.pc()));
    // Running again gives the same result.
    assert_eq!(s.machine.run(1000), Event::Halted);
    assert!(s.machine.cpu.same_state(&end_cpu));
    assert_eq!(s.machine.output, end_out);
    // Reset emulator and RAM: nothing is loaded any more.
    s.unload();
    assert!(s.machine.loaded.is_none() && s.program.is_none());
    assert_eq!(s.machine.sys.mem.read8(rustemu_core::linear(0x0700, 0x100)), 0);
}

#[test]
fn bin_file_written_to_floppy_sectors() {
    let mut s = Session::new();
    let data: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
    // Cylinder 0, head 0, sector 2 = right after the boot sector.
    assert_eq!(s.write_floppy(0, 0, 0, 2, &data), Ok(2));
    assert_eq!(s.machine.floppies[0].len(), 1_474_560);
    assert_eq!(&s.machine.floppies[0][512..1512], &data[..]);
    // Cylinder 1, head 1, sector 18 is the last sector of the second cylinder: LBA 71.
    assert_eq!(s.write_floppy(0, 1, 1, 18, &[7; 512]), Ok(1));
    assert_eq!(s.machine.floppies[0][71 * 512], 7);
    assert!(s.write_floppy(0, 0, 0, 0, &data).is_err());
    assert!(s.write_floppy(0, 79, 1, 18, &data).is_err(), "does not fit after the last sector");
}

#[test]
fn several_floppy_drives() {
    // INT 11h -> SI, then write one sector to drive DL = 1.
    let src = "org 100h
    int 11h
    mov si, ax
    mov ax, 0301h
    mov cx, 1
    mov dx, 1
    mov bx, offset data
    int 13h
    hlt
data db 512 dup(55h)
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    let mut s = Session::new();
    s.load_program(prog.clone()).unwrap();
    s.machine.run(100);
    assert!(s.machine.cpu.flags & rustemu_core::cpu::flags::CF != 0, "no FLOPPY_1 yet");
    assert_eq!(s.machine.cpu.regs[rustemu_core::cpu::AX] >> 8, 0x80);
    assert_eq!(s.machine.cpu.regs[rustemu_core::cpu::SI] & 0xC1, 0x01, "one drive");

    assert_eq!(s.machine.add_floppy(), Some(1));
    s.load_program(prog).unwrap();
    while s.machine.cpu.regs[rustemu_core::cpu::SI] == 0 || s.machine.floppies[1][0] == 0 {
        assert_eq!(s.machine.step(), Event::Continue);
    }
    assert_eq!(s.machine.cpu.regs[rustemu_core::cpu::SI] & 0xC1, 0x41, "two drives in INT 11h");
    assert_eq!(s.machine.floppies[1][..512], [0x55; 512]);
    assert!(s.machine.floppies[0].iter().all(|&b| b == 0), "FLOPPY_0 untouched");
    assert!(s.machine.step_back());
    assert_eq!(s.machine.floppies[1][0], 0, "step back undoes the write on FLOPPY_1");
    // Booting from FLOPPY_1 passes its number in DL; at most four drives.
    s.write_floppy(1, 0, 0, 1, &[0xF4]).unwrap();
    s.boot_from_floppy(1).unwrap();
    assert_eq!(s.machine.cpu.regs[rustemu_core::cpu::DX], 1);
    assert_eq!((s.machine.add_floppy(), s.machine.add_floppy(), s.machine.add_floppy()), (Some(2), Some(3), None));
    assert!(s.boot_from_floppy(4).is_err());
}

#[test]
fn nasm_program_runs_and_debugs() {
    let src = "bits 16
org 100h
section .text
start:
    mov word [count], 3
.again:
    mov dx, msg
    mov ah, 9
    int 21h
    dec word [count]
    jnz .again
    int 3
    ret
section .data
msg   db 'ok', 13, 10, '$'
count dw 0
";
    let prog = rustemu_asm::assemble_str(src).unwrap();
    assert_eq!(prog.dialect, rustemu_asm::Dialect::Nasm);
    let mut s = Session::new();
    s.load_program(prog).unwrap();
    assert_eq!(s.current_line(), Some((0, 5)));
    assert_eq!(s.machine.run(10_000), Event::Breakpoint);
    assert_eq!(String::from_utf8_lossy(&s.machine.output), "ok\r\nok\r\nok\r\n");
    assert_eq!(s.current_line(), Some((0, 13)));
    // NASM meaning of names in debugger expressions: `count` is the address, `[count]` the word.
    let ev = |e: &str| s.compile(e).map(|x| x.eval(&s.machine));
    let addr = s.program.as_ref().unwrap().symbols.iter().find(|x| x.name == "COUNT").unwrap().value;
    assert_eq!(ev("count"), Ok(addr));
    assert_eq!(ev("[count]"), Ok(0));
    assert_eq!(ev("byte [msg]"), Ok(b'o' as i64));
    assert_eq!(ev("[ds:msg]"), Ok(b'o' as i64));
    // The local label has its full name.
    assert!(s.program.as_ref().unwrap().symbols.iter().any(|x| x.name == "START.AGAIN"));
}

#[test]
fn nasm_examples() {
    let (out, ev) = run_example("hello_nasm.asm", "");
    assert_eq!(out, "Hello from NASM!\r\n".repeat(3));
    assert_eq!(ev, Event::Terminated(0));
    let mut s = Session::new();
    s.load_path(&examples().join("boot_nasm.asm")).unwrap();
    assert_eq!(s.program.as_ref().unwrap().format, rustemu_asm::Format::Boot);
    assert_eq!(s.machine.run(100_000), Event::Halted);
    assert!(s.machine.screen_text().contains("Booted from a NASM boot sector!"));
}
