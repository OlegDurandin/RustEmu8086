//! The MARS assembler: programs assembled and run on the machine, directives, errors.

use rustemu_mips::asm::{AsmError, Program, assemble};
use rustemu_mips::memory::{DATA_BASE, TEXT_BASE};
use rustemu_mips::{Finish, Machine};
use rustemu_target::Event;

fn ok(src: &str) -> Program {
    assemble(src).unwrap_or_else(|e| panic!("{}", e.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")))
}

fn errors(src: &str) -> Vec<AsmError> {
    assemble(src).expect_err("the program must not assemble")
}

/// Assembles and runs `src` with `input`; the console text and the machine.
fn run(src: &str, input: &str) -> (String, Machine) {
    let p = ok(src);
    let mut m = Machine::new();
    m.instant_waits = true;
    m.load(&p.image).unwrap();
    m.push_text(input);
    let ev = m.run(1_000_000);
    assert!(matches!(ev, Event::Terminated(_)), "{ev:?}\n{}", m.console.text());
    (m.console.text(), m)
}

#[test]
fn hello_world() {
    let src = r#"
        .data
msg:    .asciiz "Hello, MIPS!\n"
        .text
main:   li $v0, 4
        la $a0, msg
        syscall
        li $v0, 10
        syscall
"#;
    assert_eq!(run(src, "").0, "Hello, MIPS!\n");
    let p = ok(src);
    assert_eq!(p.statements.len(), 5);
    assert_eq!(p.statements[1].basic, ["lui $1,4097", "ori $4,$1,0"], "la expands as in MARS");
    assert_eq!(p.symbols.iter().find(|s| s.name == "msg").map(|s| s.addr), Some(DATA_BASE));
}

#[test]
fn loops_and_calls() {
    // Sum 1..n with a procedure, n read from the keyboard.
    let src = r#"
        .text
        li $v0, 5
        syscall
        move $a0, $v0
        jal sum
        move $a0, $v0
        li $v0, 1
        syscall
        li $v0, 10
        syscall
sum:    li $v0, 0
loop:   blez $a0, done
        add $v0, $v0, $a0
        addi $a0, $a0, -1
        b loop
done:   jr $ra
"#;
    assert_eq!(run(src, "100\n").0, "100\n5050");
}

#[test]
fn immediates_pick_the_form_mars_picks() {
    let p = ok(".text\nli $t0, 5\nli $t0, 40000\nli $t0, -2\nli $t0, 100000\naddi $t0, $t0, 70000\nsubi $t1, $t1, 3\n");
    let b: Vec<&[String]> = p.statements.iter().map(|s| &s.basic[..]).collect();
    assert_eq!(b[0], ["addiu $8,$0,5"]);
    assert_eq!(b[1], ["ori $8,$0,40000"]);
    assert_eq!(b[2], ["addiu $8,$0,-2"]);
    assert_eq!(b[3], ["lui $1,1", "ori $8,$1,34464"]);
    assert_eq!(b[4], ["lui $1,1", "ori $1,$1,4464", "add $8,$8,$1"]);
    assert_eq!(b[5], ["addi $1,$0,3", "sub $9,$9,$1"]);
}

#[test]
fn data_directives_and_alignment() {
    let src = r#"
        .data
b1:     .byte 1, 2, 'A'
w1:     .word 0x11223344, b1
h1:     .half -1
        .align 3
d1:     .space 3
arr:    .word 7:3
s:      .ascii "ab"
z:      .asciiz "c\n"
f:      .float 1.5
        .text
        nop
"#;
    let p = ok(src);
    let addr = |n: &str| p.symbols.iter().find(|s| s.name == n).unwrap().addr;
    assert_eq!(addr("b1"), DATA_BASE);
    assert_eq!(addr("w1"), DATA_BASE + 4, "the label moves with the aligned word");
    assert_eq!(addr("h1"), DATA_BASE + 12);
    assert_eq!(addr("d1"), DATA_BASE + 16, ".align 3: 8 bytes");
    assert_eq!(addr("arr"), DATA_BASE + 20);
    assert_eq!(addr("s"), DATA_BASE + 32);
    assert_eq!(addr("f"), DATA_BASE + 40);
    let mut m = Machine::new();
    m.load(&p.image).unwrap();
    let mem = |a: u32, n: usize| m.mem.read_bytes(a, n);
    assert_eq!(mem(DATA_BASE, 3), [1, 2, 65]);
    assert_eq!(m.mem.peek32(DATA_BASE + 4), 0x1122_3344);
    assert_eq!(m.mem.peek32(DATA_BASE + 8), DATA_BASE, "a label as data");
    assert_eq!(mem(DATA_BASE + 12, 2), [0xff, 0xff]);
    assert_eq!([m.mem.peek32(DATA_BASE + 20), m.mem.peek32(DATA_BASE + 28)], [7, 7]);
    assert_eq!(mem(DATA_BASE + 32, 5), *b"abc\n\0");
    assert_eq!(m.mem.peek32(DATA_BASE + 40), 1.5f32.to_bits());
    assert_eq!(p.data.iter().find(|d| d.line == 4).map(|d| (d.addr, d.len, d.unit)), Some((DATA_BASE + 4, 8, 4)));
}

#[test]
fn forward_references_and_offsets() {
    let src = r#"
        .data
ptr:    .word later
x:      .word 10, 20, 30
later:  .word 99
        .text
        lw $t0, x+8
        lw $t1, ptr
        lw $t1, 0($t1)
        li $v0, 10
        syscall
"#;
    let (_, m) = run(src, "");
    assert_eq!((m.cpu.regs[8], m.cpu.regs[9]), (30, 99));
}

#[test]
fn hi_lo_relocations() {
    let src = r#"
        .data
        .space 0x8004
val:    .word 1234
        .text
        lui $t0, %hi(val)
        addiu $t0, $t0, %lo(val)
        lw $t1, 0($t0)
        lui $t2, %hi(val)
        lw $t3, %lo(val)($t2)
        li $v0, 10
        syscall
"#;
    let (_, m) = run(src, "");
    assert_eq!(m.cpu.regs[8], DATA_BASE + 0x8004);
    assert_eq!((m.cpu.regs[9], m.cpu.regs[11]), (1234, 1234), "%lo of 0x...8004 is negative, %hi makes up for it");
}

#[test]
fn eqv_and_kernel_segments() {
    let src = r#"
        .eqv SERVICE 10
        .ktext 0x80000180
        mfc0 $k0, $14
        addiu $k0, $k0, 4
        mtc0 $k0, $14
        eret
        .text
        teq $zero, $zero
        li $s0, 7
        li $v0, SERVICE
        syscall
"#;
    let (_, m) = run(src, "");
    assert_eq!(m.cpu.regs[16], 7, "the handler skipped the trap");
    assert_eq!(m.finished, Some(Finish::Exit(0)));
}

#[test]
fn pseudo_instructions_compute_like_mars() {
    let src = r#"
        .text
        li $t0, 7
        li $t1, -3
        mul $t2, $t0, $t1
        div $t3, $t0, 2
        rem $t4, $t0, 4
        abs $t5, $t1
        neg $t6, $t0
        not $t7, $zero
        sgt $s0, $t0, $t1
        sle $s1, $t0, $t1
        seq $s2, $t0, 7
        rol $s3, $t0, 4
        ror $s4, $t0, 1
        bge $t0, $t1, yes
        li $s5, 1
yes:    bltu $t0, $t1, skip
        li $s6, 2
skip:   li $v0, 10
        syscall
"#;
    let (_, m) = run(src, "");
    let r = |i: usize| m.cpu.regs[i] as i32;
    assert_eq!([r(10), r(11), r(12), r(13), r(14), r(15)], [-21, 3, 3, 3, -7, -1]);
    assert_eq!([r(16), r(17), r(18), r(19), r(20)], [1, 0, 1, 7 << 4, (7u32.rotate_right(1)) as i32]);
    assert_eq!([r(21), r(22)], [0, 0], "bge taken; bltu (7 < 0xfffffffd unsigned) taken");
}

#[test]
fn errors_like_mars() {
    let e = errors(".text\nfoo $t0, $t1\n");
    assert_eq!((e[0].line, e[0].col, e[0].message.as_str()), (2, 1, "\"foo\" is not a recognized operator"));
    let e = errors(".text\nadd $t0, $t1\n");
    assert!(e[0].message.starts_with("Too few or incorrectly formatted operands. Expected: add $t1,$t2,$t3"));
    let e = errors(".text\nsll $t0, $t1, 32\n");
    assert_eq!(e[0].message, "\"32\": operand is out of range");
    assert_eq!(e[0].col, 15);
    let e = errors(".text\nj nowhere\n");
    assert_eq!(e[0].message, "Symbol \"nowhere\" not found in symbol table.");
    let e = errors("x: nop\nx: nop\n");
    assert_eq!((e[0].line, e[0].message.as_str()), (2, "label \"x\" already defined"));
    let e = errors(".text\n.word 5\n");
    assert_eq!(e[0].message, "\".word\" directive cannot appear in text segment");
    let e = errors(".data\n.word 1.5\n");
    assert_eq!(e[0].message, "\"1.5\" is not a valid integer constant or label");
    let e = errors(".text\nadd.s $f0, $f1, $f2\n");
    assert!(e[0].message.contains("coprocessor 1"));
    let w = ok(".data\n.byte 300\n").warnings;
    assert_eq!(w[0].message, "\"300\" is out-of-range for a signed value and possibly truncated");
    let e = errors(".text\nli $t0, 12x\n");
    assert!(e[0].message.contains("Invalid language element: 12x"));
}

#[test]
fn listing_of_branches_and_jumps() {
    let p = ok(".text\nstart: beq $t0, $t1, end\nj start\nend: jr $ra\n");
    assert_eq!(p.statements[0].basic, ["beq $8,$9,1"]);
    assert_eq!(p.statements[1].basic, [format!("j {}", TEXT_BASE as i32)]);
    assert_eq!(p.statements[0].words, [0x1109_0001]);
    assert_eq!(p.statements[1].words, [0x0810_0000]);
}

#[test]
fn swr_with_an_address_stores_through_at() {
    // MARS's table drops the "($1)" of this expansion (and emits a nop); here it stores.
    let p = ok(".text\nswr $t0, 40000\n");
    assert_eq!(p.statements[0].basic, ["ori $1,$0,40000", "swr $8,0($1)"]);
}

#[test]
fn every_basic_instruction_of_mars_is_known() {
    // The 87 integer basic forms of MARS's InstructionSet.java (coprocessor 1 left out).
    assert_eq!(rustemu_mips::asm::BASIC.len(), 87);
    let p = ok(".text\nclz $t0, $t1\nclo $t2, $t3\n");
    assert_eq!(p.statements[0].words, [0x7120_4020], "MARS encodes rt = 0");
    assert_eq!(p.statements[1].basic, ["clo $10,$11"]);
}
