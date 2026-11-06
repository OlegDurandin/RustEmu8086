//! MARS's dialog services (syscall 50..59): the program waits for the answer, which sets the
//! registers and the buffer as MARS's services do (worked out from their sources: MARS shows
//! Swing dialogs, which cannot be driven from a test).

use rustemu_mips::asm::assemble;
use rustemu_mips::{DialogAnswer, DialogKind, Machine, MessageType};
use rustemu_target::Event;

const A0: usize = 4;
const A1: usize = 5;

/// A machine with `src` loaded.
fn machine(src: &str) -> Machine {
    let p = assemble(src).unwrap_or_else(|e| panic!("{e:?}"));
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    m
}

/// Runs to the dialog of a `syscall` with `$v0 = service` (message "Number?", `$a1`, `$a2` as
/// given), answers it and runs to the end; the machine after.
fn answered(service: u32, a1: &str, answer: DialogAnswer) -> Machine {
    let src = format!(
        ".data\nmsg: .asciiz \"Number?\"\nbuf: .space 16\n.text\nli $v0, {service}\nla $a0, msg\n{a1}\nsyscall\nli $v0, 10\nsyscall\n"
    );
    let mut m = machine(&src);
    assert_eq!(m.run(100), Event::WaitingInput);
    assert!(m.dialog().is_some(), "the program waits on the dialog");
    // Waiting again changes nothing and records nothing.
    let history = m.history_len();
    assert_eq!(m.run(100), Event::WaitingInput);
    assert_eq!(m.history_len(), history);
    m.answer_dialog(answer);
    assert_eq!(m.run(100), Event::Terminated(0));
    m
}

fn int(answer: DialogAnswer) -> (i32, i32) {
    let m = answered(51, "", answer);
    (m.cpu.regs[A0] as i32, m.cpu.regs[A1] as i32)
}

fn ok(s: &str) -> DialogAnswer {
    DialogAnswer::Ok(s.to_string())
}

#[test]
fn confirmation() {
    let choice = |a| answered(50, "", a).cpu.regs[A0] as i32;
    assert_eq!(
        [
            choice(DialogAnswer::Yes),
            choice(DialogAnswer::No),
            choice(DialogAnswer::Cancel),
            choice(DialogAnswer::Closed)
        ],
        [0, 1, 2, -1]
    );
}

#[test]
fn integer_input_as_mars() {
    assert_eq!(int(ok("42")), (42, 0));
    assert_eq!(int(ok("-2147483648")), (i32::MIN, 0));
    // Integer.parseInt without trim: a space is not a number.
    assert_eq!(int(ok(" 42")), (0, -1));
    assert_eq!(int(ok("12a")), (0, -1));
    assert_eq!(int(ok("")), (0, -3));
    assert_eq!(int(DialogAnswer::Cancel), (0, -2));
}

#[test]
fn float_and_double_input() {
    let m = answered(52, "", ok(" 2.5f "));
    assert_eq!((f32::from_bits(m.cpu.fpr[0]), m.cpu.regs[A1] as i32), (2.5, 0));
    let m = answered(52, "", ok("x"));
    assert_eq!((m.cpu.fpr[0], m.cpu.regs[A1] as i32), (0, -1));
    let m = answered(53, "", ok("-1e300"));
    assert_eq!((m.cpu.f64(0), m.cpu.regs[A1] as i32), (-1e300, 0));
    let m = answered(53, "", DialogAnswer::Cancel);
    assert_eq!((m.cpu.f64(0), m.cpu.regs[A1] as i32), (0.0, -2));
}

#[test]
fn hexadecimal_float_dialogs() {
    let m = answered(52, "", ok(" -0X1.8P1f "));
    assert_eq!((m.cpu.fpr[0], m.cpu.regs[A1]), ((-3.0_f32).to_bits(), 0));
    // Just above a float midpoint: parsing via double would incorrectly round to 1.0.
    let m = answered(52, "", ok("0x1.00000100000000000001p0"));
    assert_eq!((m.cpu.fpr[0], m.cpu.regs[A1]), (0x3f80_0001, 0));
    let m = answered(53, "", ok("0x1p-1074F"));
    assert_eq!((m.cpu.f64(0).to_bits(), m.cpu.regs[A1]), (1, 0));
    for service in [52, 53] {
        let m = answered(service, "", ok("0x1.8"));
        assert_eq!((m.cpu.fpr[0], m.cpu.regs[A1] as i32), (0, -1));
        let m = answered(service, "", ok(""));
        assert_eq!(m.cpu.regs[A1] as i32, -3);
        let m = answered(service, "", DialogAnswer::Cancel);
        assert_eq!(m.cpu.regs[A1] as i32, -2);
    }
}

#[test]
fn string_input_fills_the_buffer_as_mars() {
    let buffer = |size: u32, answer| {
        let m = answered(54, &format!("la $a1, buf\nli $a2, {size}"), answer);
        let buf = 0x1001_0008;
        (m.mem.read_bytes(buf, 8), m.cpu.regs[A1] as i32)
    };
    assert_eq!(buffer(16, ok("hello")), (b"hello\n\0\0".to_vec(), 0));
    // Longer than the buffer: as many as fit, the zero, no new line, status −4.
    assert_eq!(buffer(5, ok("abcdefgh")), (b"abcd\0\0\0\0".to_vec(), -4));
    // Exactly size − 1: no room for the new line.
    assert_eq!(buffer(5, ok("abcd")), (b"abcd\0\0\0\0".to_vec(), 0));
    assert_eq!(buffer(16, ok("")), (vec![0; 8], -3));
    assert_eq!(buffer(16, DialogAnswer::Closed), (vec![0; 8], -2));
}

#[test]
fn messages_show_values_as_mars() {
    let shown = |service: u32, setup: &str| {
        let src = format!(
            ".data\nmsg: .asciiz \"x = \"\nname: .asciiz \"MIPS\"\nhalf: .float 0.5\nthird: .double 0.1\n.text\n{setup}\nli $v0, {service}\nla $a0, msg\nsyscall\n"
        );
        let mut m = machine(&src);
        assert_eq!(m.run(100), Event::WaitingInput);
        let d = m.dialog().unwrap().clone();
        m.answer_dialog(ok(""));
        assert_eq!(m.run(100), Event::Terminated(0));
        (d.kind, d.message)
    };
    let info = DialogKind::Message(MessageType::Information);
    assert_eq!(shown(55, "li $a1, 2"), (DialogKind::Message(MessageType::Warning), "x = ".into()));
    assert_eq!(shown(55, "li $a1, 9"), (DialogKind::Message(MessageType::Plain), "x = ".into()));
    assert_eq!(shown(56, "li $a1, -7"), (info, "x = -7".into()));
    assert_eq!(shown(57, "l.s $f12, half"), (info, "x = 0.5".into()));
    assert_eq!(shown(58, "l.d $f12, third"), (info, "x = 0.1".into()));
    assert_eq!(shown(59, "la $a1, name"), (info, "x = MIPS".into()));
}

#[test]
fn a_bad_message_address_is_an_exception() {
    let mut m = machine(".text\nli $v0, 55\nli $a0, 0\nsyscall\n");
    match m.run(100) {
        Event::Error(e) => assert!(e.contains("address out of range 0x00000000"), "{e}"),
        ev => panic!("{ev:?}"),
    }
}

#[test]
fn step_back_asks_again() {
    let mut m = machine(".data\nmsg: .asciiz \"n?\"\n.text\nli $v0, 51\nla $a0, msg\nsyscall\nli $v0, 10\nsyscall\n");
    assert_eq!(m.run(100), Event::WaitingInput);
    m.answer_dialog(ok("5"));
    assert_eq!(m.step(), Event::Continue);
    assert_eq!(m.cpu.regs[A0], 5);
    assert!(m.step_back());
    assert_eq!(m.step(), Event::WaitingInput, "the dialog again");
    assert!(m.dialog().is_some());
}

#[test]
fn dialogs_in_the_console() {
    let mut m = machine(
        ".data\nmsg: .asciiz \"How many?\"\nbye: .asciiz \"Bye, \"\n.text\nli $v0, 51\nla $a0, msg\nsyscall\nmove $a1, $a0\nli $v0, 56\nla $a0, bye\nsyscall\n",
    );
    m.dialogs_in_console = true;
    assert_eq!(m.run(100), Event::WaitingInput);
    assert!(m.dialog().is_none(), "no window");
    m.push_text("12\n");
    assert_eq!(m.run(100), Event::Terminated(0));
    assert_eq!(m.console.text(), "How many? 12\nBye, 12\n");
}

#[test]
fn popup_input_as_mars() {
    let src = ".data\nbuf: .space 8\n.text\nli $v0, 5\nsyscall\nmove $s0, $v0\nla $a0, buf\nli $a1, 8\nli $v0, 8\nsyscall\nli $v0, 12\nsyscall\nmove $s1, $v0\n";
    let settings = rustemu_mips::Settings { popup_input: true, ..Default::default() };
    let p = rustemu_mips::asm::assemble_settings("", src, &mut |_| None, &settings).unwrap();
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    let answer = |m: &mut Machine, text: &str, prompt: &str| {
        assert_eq!(m.run(100), Event::WaitingInput);
        let d = m.dialog().unwrap();
        assert_eq!((d.kind, d.message.as_str()), (DialogKind::KeyboardInput, prompt));
        m.answer_dialog(ok(text));
    };
    answer(&mut m, " 12 ", "Enter an integer value (syscall 5)");
    answer(&mut m, "abcdefghij", "Enter a string of maximum length 7 (syscall 8)");
    answer(&mut m, "xyz", "Enter a character value (syscall 12)");
    assert_eq!(m.run(100), Event::Terminated(0));
    assert_eq!((m.cpu.regs[16], m.cpu.regs[17]), (12, b'x' as u32));
    assert_eq!(m.mem.read_bytes(0x1001_0000, 8), b"abcdefg\0");
    assert_eq!(m.console.text(), "**** user input :  12 \n**** user input : abcdefghij\n**** user input : xyz\n");
    // Closing the window: Swing's "uninitializedValue", not an integer.
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    assert_eq!(m.run(100), Event::WaitingInput);
    m.answer_dialog(DialogAnswer::Closed);
    let Event::Error(e) = m.run(100) else { panic!() };
    assert!(e.ends_with("invalid integer input (syscall 5)"), "{e}");
}
