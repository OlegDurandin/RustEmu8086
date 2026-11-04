//! The MIPS machine: MARS's system services, the console, input, files and step back.

use std::collections::BTreeMap;

use rustemu_mips::cpu::V0;
use rustemu_mips::files::Storage;
use rustemu_mips::machine::HISTORY_DEPTH;
use rustemu_mips::memory::{DATA_BASE, EXCEPTION_HANDLER, HEAP_BASE, TEXT_BASE};
use rustemu_mips::{Chunk, Finish, Image, Machine};
use rustemu_target::Event;

const A0: u32 = 4;
const A1: u32 = 5;
const A2: u32 = 6;
const S0: u32 = 16;
const SYSCALL: u32 = 0x0000_000C;

fn i(op: u32, rs: u32, rt: u32, imm: i32) -> u32 {
    op << 26 | rs << 21 | rt << 16 | (imm as u32 & 0xFFFF)
}
/// `addiu rt, $zero, v`.
fn li(rt: u32, v: i32) -> u32 {
    i(0x09, 0, rt, v)
}
/// `lui rt, upper` + `ori rt, rt, lower`.
fn la(rt: u32, addr: u32) -> [u32; 2] {
    [i(0x0F, 0, rt, (addr >> 16) as i32), i(0x0D, rt, rt, (addr & 0xFFFF) as i32)]
}
/// `addu rd, rs, $zero`.
fn mv(rd: u32, rs: u32) -> u32 {
    rs << 21 | rd << 11 | 0x21
}
/// `syscall` with `$v0 = n`.
fn sys(n: i32) -> [u32; 2] {
    [li(V0 as u32, n), SYSCALL]
}

fn image(code: &[u32], data: &[u8]) -> Image {
    let mut chunks =
        vec![Chunk { addr: TEXT_BASE, bytes: code.iter().flat_map(|w| w.to_le_bytes()).collect(), code: true }];
    if !data.is_empty() {
        chunks.push(Chunk { addr: DATA_BASE, bytes: data.to_vec(), code: false });
    }
    Image { chunks, entry: TEXT_BASE }
}

fn machine(code: &[u32], data: &[u8]) -> Machine {
    let mut m = Machine::with_storage(Storage::Memory(BTreeMap::new()));
    m.instant_waits = true;
    m.load(&image(code, data)).unwrap();
    m
}

/// Runs until the program stops (input it waits for is not typed: the run ends there).
fn run(m: &mut Machine) -> Event {
    m.run(100_000)
}

fn code(parts: &[&[u32]]) -> Vec<u32> {
    parts.concat()
}

#[test]
fn hello_world() {
    let mut m = machine(&code(&[&la(A0, DATA_BASE), &sys(4), &sys(10)]), b"Hello, MIPS!\n\0");
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.console.text(), "Hello, MIPS!\n");
    assert_eq!(m.finished, Some(Finish::Exit(0)));
    assert_eq!(m.step(), Event::Terminated(0), "a finished program stays finished");
}

#[test]
fn printing_numbers_and_characters() {
    let mut m = machine(
        &code(&[
            &[li(A0, -5)],
            &sys(1),
            &[li(A0, 32)],
            &sys(11),
            &[li(A0, -5)],
            &sys(36),
            &[li(A0, 0x41)],
            &sys(11),
            &[li(A0, -5)],
            &sys(34),
            &[li(A0, 5)],
            &sys(35),
        ]),
        &[],
    );
    assert_eq!(run(&mut m), Event::Terminated(0), "dropping off the end is a normal end");
    assert_eq!(m.finished, Some(Finish::DroppedOff));
    assert_eq!(m.console.text(), "-5 4294967291A0xfffffffb00000000000000000000000000000101");
}

#[test]
fn reading_an_integer_waits_for_enter() {
    let mut m = machine(&code(&[&sys(5), &[mv(S0, V0 as u32)], &sys(5)]), &[]);
    assert_eq!(run(&mut m), Event::WaitingInput);
    assert!(m.is_waiting_input());
    m.push_text("1");
    assert_eq!(m.step(), Event::WaitingInput, "no Enter yet");
    assert_eq!(m.pending_input(), b"1");
    m.push_text("2\n");
    assert_eq!(run(&mut m), Event::WaitingInput);
    assert_eq!(m.cpu.regs[S0 as usize], 12);
    assert_eq!(m.console.text(), "12\n", "the input is echoed");
    m.push_text("12x\n");
    let Event::Error(e) = run(&mut m) else { panic!("invalid input must stop the program") };
    assert_eq!(e, "Runtime exception at 0x00400010: invalid integer input (syscall 5)");
}

#[test]
fn reading_strings_and_characters() {
    // read_string($a0 = buffer, $a1 = 5): at most 4 characters, then '\n' if there is room, then 0.
    let mut prog = la(A0, DATA_BASE).to_vec();
    prog.extend([li(A1, 5)]);
    prog.extend(sys(8));
    prog.extend(la(A0, DATA_BASE + 8));
    prog.extend(sys(8));
    prog.extend(sys(12));
    prog.extend([mv(S0, V0 as u32)]);
    let mut m = machine(&prog, &[0xEE; 16]);
    m.push_text("hello world\nhi\nx");
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(&m.mem.read_bytes(DATA_BASE, 5), b"hell\0", "4 characters end the read without Enter");
    // The rest of the line is the next input: "o world\n" -> "o wo".
    assert_eq!(&m.mem.read_bytes(DATA_BASE + 8, 5), b"o wo\0");
    assert_eq!(m.cpu.regs[S0 as usize], b'r' as u32, "read_char takes one character");
    let mut m = machine(&prog, &[0xEE; 16]);
    m.push_text("hi\n");
    m.run(100);
    assert_eq!(&m.mem.read_bytes(DATA_BASE, 5), b"hi\n\0\xEE");
}

#[test]
fn heap_with_sbrk() {
    let prog = code(&[
        &[li(A0, 5)],
        &sys(9),
        &[mv(S0, V0 as u32), li(A0, 4)],
        &sys(9),
        &[mv(S0 + 1, V0 as u32), li(A0, -1)],
        &sys(9),
    ]);
    let mut m = machine(&prog, &[]);
    let Event::Error(e) = run(&mut m) else { panic!() };
    assert_eq!((m.cpu.regs[S0 as usize], m.cpu.regs[S0 as usize + 1]), (HEAP_BASE, HEAP_BASE + 8));
    assert!(e.ends_with("request (-1) is negative heap amount (syscall 9)"), "{e}");
}

#[test]
fn exit_with_a_code() {
    let mut m = machine(&code(&[&[li(A0, 3)], &sys(17), &sys(1)]), &[]);
    assert_eq!(run(&mut m), Event::Terminated(3));
    assert_eq!(m.finished, Some(Finish::Exit(3)));
    assert!(m.console.is_empty(), "nothing runs after exit");
}

#[test]
fn files_in_the_sandbox() {
    // open "f.txt" for writing, write "data", close; open for reading, read 10 bytes, print them.
    let mut data = b"f.txt\0\0\0data".to_vec();
    data.resize(32, 0);
    let mut p = la(A0, DATA_BASE).to_vec();
    p.extend([li(A1, 1)]);
    p.extend(sys(13));
    p.extend([mv(S0, V0 as u32), mv(A0, S0)]);
    p.extend(la(A1, DATA_BASE + 8));
    p.extend([li(A2, 4)]);
    p.extend(sys(15));
    p.extend([mv(A0, S0)]);
    p.extend(sys(16));
    p.extend(la(A0, DATA_BASE));
    p.extend([li(A1, 0)]);
    p.extend(sys(13));
    p.extend([mv(A0, V0 as u32)]);
    p.extend(la(A1, DATA_BASE + 16));
    p.extend([li(A2, 10)]);
    p.extend(sys(14));
    p.extend([mv(S0 + 1, V0 as u32)]);
    p.extend(la(A0, DATA_BASE + 16));
    p.extend(sys(4));
    let mut m = machine(&p, &data);
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.cpu.regs[S0 as usize], 3, "the first file gets descriptor 3");
    assert_eq!(m.cpu.regs[S0 as usize + 1], 4, "4 bytes read");
    assert_eq!(m.console.text(), "data");
    let Storage::Memory(files) = &m.files.storage else { panic!() };
    assert_eq!(files["f.txt"], b"data");
    // File operations cannot be undone: the history starts after the last one (the read): move,
    // la (2), li, syscall 4 and the end.
    assert_eq!(m.history_len(), 6);

    // A missing file: -1 in $v0 and the program goes on, as in MARS (it checks $v0 itself); so do
    // a read and a write with a descriptor that is not open.
    let mut p = la(A0, DATA_BASE).to_vec();
    p.extend([li(A1, 0)]);
    p.extend(sys(13));
    p.extend([mv(S0, V0 as u32), li(A0, 7), li(A2, 4)]);
    p.extend(la(A1, DATA_BASE));
    p.extend(sys(14));
    p.extend([mv(S0 + 1, V0 as u32), li(A0, 7)]);
    p.extend(sys(15));
    let mut m = machine(&p, b"nope\0");
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.cpu.regs[S0 as usize], u32::MAX, "open");
    assert_eq!(m.cpu.regs[S0 as usize + 1], u32::MAX, "read");
    assert_eq!(m.cpu.regs[V0], u32::MAX, "write");
    assert!(m.file_error.as_deref().is_some_and(|e| e.contains('7')), "{:?}", m.file_error);
}

#[test]
fn console_through_descriptors_0_and_1() {
    let mut p = vec![li(A0, 0)];
    p.extend(la(A1, DATA_BASE));
    p.extend([li(A2, 3)]);
    p.extend(sys(14)); // read 3 bytes from the console
    p.extend([mv(A2, V0 as u32), li(A0, 1)]);
    p.extend(sys(15)); // write them to the console
    let mut m = machine(&p, &[0; 8]);
    m.push_text("abcdef");
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.console.text(), "abcabc", "echo, then the program's own write");
}

#[test]
fn random_numbers_as_java() {
    let p = code(&[
        &[li(A0, 0), li(A1, 42)],
        &sys(40),
        &[li(A0, 0), li(A1, 10)],
        &sys(42),
        &[mv(S0, A0), li(A0, 0), li(A1, 10)],
        &sys(42),
        &[mv(S0 + 1, A0), li(A0, 0)],
        &sys(41),
        &[mv(S0 + 2, A0)],
    ]);
    let mut m = machine(&p, &[]);
    run(&mut m);
    assert_eq!(&m.cpu.regs[16..19], &[0, 3, (-1360544799i32) as u32]);
    let mut m = machine(&code(&[&[li(A0, 0), li(A1, 0)], &sys(42)]), &[]);
    let Event::Error(e) = run(&mut m) else { panic!() };
    assert!(e.ends_with("Upper bound of range cannot be negative (syscall 42)"));
}

#[test]
fn time_and_sleep() {
    let mut m = machine(&code(&[&sys(30), &[li(A0, 30)], &sys(32), &[li(S0, 1)]]), &[]);
    m.fixed_time = Some(0x1_2345_6789);
    m.instant_waits = false;
    assert_eq!(run(&mut m), Event::Sleeping);
    assert_eq!((m.cpu.regs[A0 as usize], m.cpu.regs[A1 as usize]), (30, 1), "a0 was reloaded with the delay");
    assert_eq!(m.step(), Event::Sleeping, "still sleeping");
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.cpu.regs[S0 as usize], 1);
}

#[test]
fn unsupported_services() {
    let mut m = machine(&sys(99), &[]);
    assert_eq!(
        run(&mut m),
        Event::Error("Runtime exception at 0x00400004: invalid or unimplemented syscall service: 99 ".into())
    );
    let mut m = machine(&sys(55), &[]);
    let Event::Error(e) = run(&mut m) else { panic!() };
    assert!(e.contains("dialog window"), "{e}");
    let mut m = machine(&[i(0x0F, 0, A0, 0)].iter().chain(&sys(4)).copied().collect::<Vec<_>>(), &[]);
    let Event::Error(e) = run(&mut m) else { panic!() };
    assert_eq!(e, "Runtime exception at 0x00400008: address out of range 0x00000000", "printing from address 0");
}

#[test]
fn step_back_over_system_calls() {
    let prog = code(&[&[li(A0, 7)], &sys(1), &sys(5), &[li(A0, 4)], &sys(9)]);
    let mut m = machine(&prog, &[]);
    m.push_text("21\n");
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.console.text(), "721\n");
    assert_eq!(m.heap, HEAP_BASE + 4);
    // Back over: the end, syscall 9, li $v0, li $a0, syscall 5.
    for _ in 0..5 {
        assert!(m.step_back());
    }
    assert_eq!(m.finished, None);
    assert_eq!(m.heap, HEAP_BASE);
    assert_eq!(m.console.text(), "7", "the echo of the input is gone");
    assert_eq!(m.pending_input(), b"21\n", "the input is typed again");
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.cpu.regs[V0], HEAP_BASE);
}

#[test]
fn waiting_is_not_recorded() {
    let mut m = machine(&sys(5), &[]);
    assert_eq!(run(&mut m), Event::WaitingInput);
    let n = m.history_len();
    for _ in 0..10 {
        assert_eq!(m.step(), Event::WaitingInput);
    }
    assert_eq!(m.history_len(), n);
    assert_eq!(m.cpu.instr_count, 1, "only li ran");
}

#[test]
fn history_is_limited() {
    // A loop of 2 instructions run 2000 times: the history keeps the last HISTORY_DEPTH steps.
    let mut m = machine(&[li(S0, 2000), i(0x09, S0, S0, -1), i(0x07, S0, 0, -2)], &[]);
    assert_eq!(run(&mut m), Event::Terminated(0));
    assert_eq!(m.history_len(), HISTORY_DEPTH);
}

#[test]
fn the_exception_handler_takes_service_errors() {
    let handler = [
        0x401A_7000u32,     // mfc0 $k0, $14
        i(0x09, 26, 26, 4), // addiu $k0, $k0, 4
        0x409A_7000,        // mtc0 $k0, $14
        0x4200_0018,        // eret
    ];
    let mut img = image(&code(&[&sys(99), &[li(S0, 1)]]), &[]);
    img.chunks.push(Chunk {
        addr: EXCEPTION_HANDLER,
        bytes: handler.iter().flat_map(|w| w.to_le_bytes()).collect(),
        code: true,
    });
    let mut m = Machine::new();
    m.load(&img).unwrap();
    assert_eq!(m.run(100), Event::Terminated(0));
    assert_eq!(m.cpu.regs[S0 as usize], 1);
    assert_eq!(m.cpu.cp0_reg(13) >> 2 & 31, 8, "Cause: syscall exception");
}

#[test]
fn loading_checks_the_segments() {
    let mut m = Machine::new();
    let bad = Image { chunks: vec![Chunk { addr: DATA_BASE, bytes: vec![0; 4], code: true }], entry: DATA_BASE };
    assert!(m.load(&bad).unwrap_err().contains("outside the text segment"));
    let bad = Image { chunks: vec![Chunk { addr: 0x103f_fffe, bytes: vec![0; 4], code: false }], entry: TEXT_BASE };
    assert!(m.load(&bad).is_err());
}

#[test]
fn backspace_edits_the_line() {
    let mut m = machine(&code(&[&sys(5), &[mv(S0, V0 as u32)]]), &[]);
    m.push_text("1я");
    m.push_byte(8);
    m.push_text("3\n");
    m.run(100);
    assert_eq!(m.cpu.regs[S0 as usize], 13);
}

#[test]
fn step_back_over_the_mmio_display() {
    // A terminal (bell), characters, a form feed: every step undone gives the same display as
    // when it was first reached.
    let src = r#"
        .text
main:   li   $t1, 0xffff0000
        li   $a0, 'A'
        jal  send
        li   $a0, 0x00100107
        jal  send
        li   $a0, 'B'
        jal  send
        li   $a0, 12
        jal  send
        li   $a0, 'C'
        jal  send
wait:   lw   $t3, 8($t1)
        beqz $t3, wait
        li   $v0, 10
        syscall
send:   lw   $t3, 8($t1)
        andi $t3, $t3, 1
        beqz $t3, send
        sw   $a0, 12($t1)
        jr   $ra
"#;
    let p = rustemu_mips::asm::assemble(src).unwrap();
    let mut m = Machine::new();
    m.mmio.terminal_size = (4, 3);
    m.load(&p.image).unwrap();
    let mut seen = Vec::new();
    loop {
        seen.push((m.mmio.text().to_vec(), m.mmio.cursor_text(), m.mem.read_bytes(0xffff_0000, 16), m.cpu.pc));
        if m.step() != Event::Continue {
            break;
        }
    }
    assert_eq!(m.mmio.text(), b"C   \n    \n    ", "a form feed clears the terminal");
    while m.step_back() {
        let now = (m.mmio.text().to_vec(), m.mmio.cursor_text(), m.mem.read_bytes(0xffff_0000, 16), m.cpu.pc);
        assert_eq!(now, seen.pop().unwrap());
    }
    assert!(m.mmio.text().is_empty());
}

#[test]
fn keys_typed_on_the_mmio_keyboard() {
    let src = r#"
        .text
main:   li   $t1, 0xffff0000
loop:   lw   $t2, 0($t1)
        andi $t2, $t2, 1
        beqz $t2, loop
        lw   $a0, 4($t1)
        li   $v0, 1
        syscall
"#;
    let p = rustemu_mips::asm::assemble(src).unwrap();
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    assert_eq!(m.run(1000), Event::Budget, "polls without a key");
    m.mmio_key(b'7');
    assert_eq!(m.run(1000), Event::Terminated(0));
    assert_eq!(m.console.text(), "55");
    assert_eq!(m.mem.peek32(0xffff_0000), 0, "reading the data cleared ready");
}

#[test]
fn step_back_over_the_digital_lab() {
    let src = r#"
        .text
main:   li   $t0, 0xffff0010
        li   $t1, 0x06
        sb   $t1, 0($t0)
        li   $t1, 1
        sb   $t1, 3($t0)
loop:   addi $s0, $s0, 1
        j    loop
"#;
    let p = rustemu_mips::asm::assemble(src).unwrap();
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    m.run(5);
    m.lab_click(7);
    let mut seen = Vec::new();
    for _ in 0..100 {
        seen.push((m.lab.segments(), m.lab.is_counting(), m.cpu.pc));
        m.step();
    }
    assert_eq!(m.lab.segments(), [0, 0x06]);
    assert!(m.cpu.cp0[13] & 0x400 != 0, "a counter interrupt was taken (no handler: the program stopped)");
    while let Some(s) = seen.pop() {
        if !m.step_back() {
            break;
        }
        assert_eq!((m.lab.segments(), m.lab.is_counting(), m.cpu.pc), s);
    }
    assert_eq!(m.lab.pressed(), Some(7), "a click is not undone");
}
