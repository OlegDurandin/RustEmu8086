//! Instruction semantics of the MIPS core, checked against what MARS's simulation code does
//! (`mars/mips/instructions/InstructionSet.java`).

use rustemu_mips::cpu::{RA, cp0};
use rustemu_mips::memory::{self, DATA_BASE, EXCEPTION_HANDLER, TEXT_BASE};
use rustemu_mips::{Cpu, ExcCode, Memory, Step};

// Registers used in the tests.
const T0: u32 = 8;
const T1: u32 = 9;
const T2: u32 = 10;
const T3: u32 = 11;
const A0: u32 = 4;
const K0: u32 = 26;

/// R format: SPECIAL (or `op`), rs, rt, rd, shamt, funct.
fn r(op: u32, rs: u32, rt: u32, rd: u32, shamt: u32, funct: u32) -> u32 {
    op << 26 | rs << 21 | rt << 16 | rd << 11 | shamt << 6 | funct
}
fn sp(rs: u32, rt: u32, rd: u32, shamt: u32, funct: u32) -> u32 {
    r(0, rs, rt, rd, shamt, funct)
}
/// I format: op, rs, rt, 16-bit immediate.
fn i(op: u32, rs: u32, rt: u32, imm: i32) -> u32 {
    op << 26 | rs << 21 | rt << 16 | (imm as u32 & 0xFFFF)
}
/// J format: op, word address.
fn j(op: u32, target: u32) -> u32 {
    op << 26 | (target >> 2 & 0x03FF_FFFF)
}
/// `li` of a 32-bit value: lui + ori.
fn li(rt: u32, v: u32) -> [u32; 2] {
    [i(0x0F, 0, rt, (v >> 16) as i32), i(0x0D, rt, rt, (v & 0xFFFF) as i32)]
}

fn load(m: &mut Memory, at: u32, code: &[u32]) {
    let bytes: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    m.load(at, &bytes);
    m.add_code(at, at + bytes.len() as u32);
}

/// Runs `code` from the start of the text segment until it drops off its end, makes a system
/// call or an unhandled exception stops it.
fn run_with(mut mem: Memory, code: &[u32]) -> (Cpu, Memory, Step) {
    load(&mut mem, TEXT_BASE, code);
    let mut cpu = Cpu::new();
    for _ in 0..10_000 {
        match cpu.step(&mut mem) {
            Step::Ok => {}
            Step::Exception(e) if e.handled => {}
            s => return (cpu, mem, s),
        }
    }
    panic!("the program does not end");
}

fn run(code: &[u32]) -> (Cpu, Memory, Step) {
    run_with(Memory::new(), code)
}

fn regs(code: &[u32]) -> Cpu {
    let (cpu, _, s) = run(code);
    assert_eq!(s, Step::DroppedOff, "the program stopped early");
    cpu
}

fn exception(code: &[u32]) -> (Cpu, rustemu_mips::Exception) {
    match run(code) {
        (cpu, _, Step::Exception(e)) => (cpu, e),
        (_, _, s) => panic!("no exception: {s:?}"),
    }
}

#[test]
fn reset_state_is_mars() {
    let c = Cpu::new();
    assert_eq!(c.pc, 0x0040_0000);
    assert_eq!(c.regs[28], 0x1000_8000);
    assert_eq!(c.regs[29], 0x7fff_effc);
    assert_eq!(c.cp0_reg(cp0::STATUS), 0x0000_ff11);
}

#[test]
fn add_overflows_like_mars() {
    let mut code = li(T1, 0x7fff_ffff).to_vec();
    code.push(i(0x09, 0, T2, 1)); // addiu $t2, $zero, 1
    code.push(sp(T1, T2, T0, 0, 0x20)); // add $t0, $t1, $t2
    let (cpu, e) = exception(&code);
    assert_eq!(e.code, ExcCode::Overflow);
    assert_eq!(e.report(), "Runtime exception at 0x0040000c: arithmetic overflow");
    assert_eq!(cpu.regs[T0 as usize], 0, "the destination is not written");
    assert_eq!(cpu.cp0_reg(cp0::EPC), 0x0040_000c);
    assert_eq!(cpu.cp0_reg(cp0::CAUSE) >> 2 & 31, 12);
    assert_ne!(cpu.cp0_reg(cp0::STATUS) & cp0::EXL, 0);
    assert_eq!(cpu.pc, 0x0040_000c, "an unhandled exception stops at the instruction");
}

#[test]
fn arithmetic() {
    let mut code = li(T1, 0x7fff_ffff).to_vec();
    code.extend([
        i(0x09, T1, T2, 1),      // addiu $t2, $t1, 1    -> 0x80000000, no overflow
        sp(T1, T1, T3, 0, 0x21), // addu $t3, $t1, $t1   -> 0xfffffffe
        i(0x08, 0, T0, -5),      // addi $t0, $zero, -5
        sp(T0, T1, 16, 0, 0x23), // subu $s0, $t0, $t1
        sp(T1, T0, 17, 0, 0x22), // sub $s1, $t1, $t0: 0x7fffffff - (-5) overflows
    ]);
    let (cpu, e) = exception(&code);
    assert_eq!(cpu.regs[T2 as usize], 0x8000_0000);
    assert_eq!(cpu.regs[T3 as usize], 0xffff_fffe);
    assert_eq!(cpu.regs[T0 as usize], (-5i32) as u32);
    assert_eq!(cpu.regs[16], (-5i32 as u32).wrapping_sub(0x7fff_ffff));
    assert_eq!(e.code, ExcCode::Overflow, "0x7fffffff - (-5) overflows");
    // addi overflows too.
    let mut code = li(T1, 0x7fff_fff0).to_vec();
    code.push(i(0x08, T1, T0, 0x10));
    assert_eq!(exception(&code).1.code, ExcCode::Overflow);
}

#[test]
fn multiply_and_divide() {
    let mut code = li(T0, (-6i32) as u32).to_vec();
    code.extend(li(T1, 7));
    code.push(sp(T0, T1, 0, 0, 0x18)); // mult -> -42
    code.push(sp(0, 0, 16, 0, 0x10)); // mfhi $s0
    code.push(sp(0, 0, 17, 0, 0x12)); // mflo $s1
    code.push(sp(T0, T1, 0, 0, 0x19)); // multu
    code.push(sp(0, 0, 18, 0, 0x10)); // mfhi $s2
    code.push(sp(0, 0, 19, 0, 0x12)); // mflo $s3
    let c = regs(&code);
    assert_eq!((c.regs[16], c.regs[17]), (0xffff_ffff, (-42i32) as u32));
    let p = 0xffff_fffau64 * 7;
    assert_eq!((c.regs[18], c.regs[19]), ((p >> 32) as u32, p as u32));

    let mut code = li(T0, (-7i32) as u32).to_vec();
    code.extend(li(T1, 2));
    code.push(sp(T0, T1, 0, 0, 0x1A)); // div: -3 rem -1 (truncating, as Java)
    code.push(sp(0, 0, 16, 0, 0x12));
    code.push(sp(0, 0, 17, 0, 0x10));
    code.push(sp(T0, T1, 0, 0, 0x1B)); // divu
    code.push(sp(0, 0, 18, 0, 0x12));
    code.push(sp(0, 0, 19, 0, 0x10));
    code.push(sp(T0, 0, 0, 0, 0x1A)); // div by $zero: no exception, HI and LO unchanged
    code.push(sp(0, 0, 20, 0, 0x12));
    let c = regs(&code);
    assert_eq!((c.regs[16], c.regs[17]), ((-3i32) as u32, (-1i32) as u32));
    assert_eq!((c.regs[18], c.regs[19]), (0xffff_fff9 / 2, 0xffff_fff9 % 2));
    assert_eq!(c.regs[20], 0xffff_fff9 / 2);

    // INT_MIN / -1 wraps as in Java: quotient INT_MIN, remainder 0.
    let mut code = li(T0, 0x8000_0000).to_vec();
    code.extend(li(T1, 0xffff_ffff));
    code.push(sp(T0, T1, 0, 0, 0x1A));
    let c = regs(&code);
    assert_eq!((c.lo, c.hi), (0x8000_0000, 0));
}

#[test]
fn special2_mul_madd_msub_clz_clo() {
    let mut code = li(T0, 100_000).to_vec();
    code.extend(li(T1, 300_000));
    code.push(r(0x1C, T0, T1, T2, 0, 0x02)); // mul $t2, $t0, $t1 (also HI/LO)
    let c = regs(&code);
    let p = 100_000u64 * 300_000;
    assert_eq!(c.regs[T2 as usize], p as u32);
    assert_eq!(c.hilo(), p);

    let mut code = li(T0, (-2i32) as u32).to_vec();
    code.extend(li(T1, 3));
    code.push(sp(T0, T1, 0, 0, 0x19)); // multu: HI:LO = 0xfffffffe * 3
    code.push(r(0x1C, T0, T1, 0, 0, 0x00)); // madd: + (-6)
    let c = regs(&code);
    assert_eq!(c.hilo(), (0xffff_fffeu64 * 3).wrapping_sub(6));
    let mut code = code[..code.len() - 1].to_vec();
    code.push(r(0x1C, T0, T1, 0, 0, 0x01)); // maddu
    assert_eq!(regs(&code).hilo(), 0xffff_fffeu64 * 3 * 2);
    code.pop();
    code.push(r(0x1C, T0, T1, 0, 0, 0x04)); // msub
    assert_eq!(regs(&code).hilo(), (0xffff_fffeu64 * 3).wrapping_add(6));
    code.pop();
    code.push(r(0x1C, T0, T1, 0, 0, 0x05)); // msubu
    assert_eq!(regs(&code).hilo(), 0);

    let mut code = li(T0, 0x0000_ff00).to_vec();
    code.extend(li(T1, 0xfff0_0000));
    code.push(r(0x1C, T0, 0, T2, 0, 0x20)); // clz $t2, $t0 (MARS: rt = 0)
    code.push(r(0x1C, T1, T3, T3, 0, 0x21)); // clo $t3, $t1 (MIPS32: rt = rd)
    code.push(r(0x1C, 0, 0, 16, 0, 0x20)); // clz of 0
    let c = regs(&code);
    assert_eq!((c.regs[T2 as usize], c.regs[T3 as usize], c.regs[16]), (16, 12, 32));
}

#[test]
fn logic_shifts_and_comparisons() {
    let mut code = li(T0, 0xf0f0_1234).to_vec();
    code.extend(li(T1, 0x0ff0_00ff));
    code.extend([
        sp(T0, T1, 16, 0, 0x24), // and
        sp(T0, T1, 17, 0, 0x25), // or
        sp(T0, T1, 18, 0, 0x26), // xor
        sp(T0, T1, 19, 0, 0x27), // nor
        i(0x0C, T0, 20, 0xff00), // andi (zero-extended)
        i(0x0D, 0, 21, 0x8000),  // ori  (zero-extended)
        i(0x0E, T0, 22, 0xffff), // xori
        sp(0, T0, 23, 4, 0x00),  // sll 4
        sp(0, T0, 24, 4, 0x02),  // srl 4
        sp(0, T0, 25, 4, 0x03),  // sra 4
        i(0x09, 0, T2, 36),      // shift amount 36 -> 4
        sp(T2, T0, 2, 0, 0x04),  // sllv $v0
        sp(T2, T0, 3, 0, 0x07),  // srav $v1
    ]);
    let c = regs(&code);
    let (a, b) = (0xf0f0_1234u32, 0x0ff0_00ffu32);
    assert_eq!(&c.regs[16..20], &[a & b, a | b, a ^ b, !(a | b)]);
    assert_eq!(&c.regs[20..23], &[a & 0xff00, 0x8000, a ^ 0xffff]);
    assert_eq!(&c.regs[23..26], &[a << 4, a >> 4, ((a as i32) >> 4) as u32]);
    assert_eq!((c.regs[2], c.regs[3]), (a << 4, ((a as i32) >> 4) as u32));

    let mut code = li(T0, (-1i32) as u32).to_vec();
    code.extend([
        sp(T0, 0, 16, 0, 0x2A), // slt $s0, $t0, $zero   -> 1
        sp(T0, 0, 17, 0, 0x2B), // sltu $s1, $t0, $zero  -> 0
        i(0x0A, T0, 18, 0),     // slti $s2, $t0, 0      -> 1
        i(0x0B, 0, 19, -1),     // sltiu $s3, $zero, -1  -> 1 (0 < 0xffffffff)
        i(0x0B, T0, 20, -1),    // sltiu $s4, $t0, -1    -> 0
        i(0x0F, 0, 21, 0x1234), // lui
    ]);
    let c = regs(&code);
    assert_eq!(&c.regs[16..22], &[1, 0, 1, 1, 0, 0x1234_0000]);
}

#[test]
fn zero_register_stays_zero() {
    let c = regs(&[i(0x09, 0, 0, 5), sp(0, 0, 0, 0, 0x27)]);
    assert_eq!(c.regs[0], 0);
}

#[test]
fn loads_and_stores() {
    let mut code = li(A0, DATA_BASE).to_vec();
    code.extend(li(T0, 0x8899_aabb));
    code.extend([
        i(0x2B, A0, T0, 0), // sw $t0, 0($a0)
        i(0x20, A0, 16, 0), // lb  -> 0xffffffbb
        i(0x24, A0, 17, 0), // lbu -> 0xbb
        i(0x21, A0, 18, 2), // lh  -> 0xffff8899
        i(0x25, A0, 19, 2), // lhu -> 0x8899
        i(0x23, A0, 20, 0), // lw
        i(0x28, A0, T0, 5), // sb at +5
        i(0x29, A0, T0, 6), // sh at +6
        i(0x23, A0, 21, 4), // lw +4 -> 0xaabbbb00
        i(0x30, A0, 22, 0), // ll
        i(0x09, 0, 23, 77),
        i(0x38, A0, 23, 8), // sc: stores 77, $s7 = 1
        i(0x23, A0, 24, 8),
    ]);
    let (c, mem, s) = run(&code);
    assert_eq!(s, Step::DroppedOff);
    assert_eq!(&c.regs[16..20], &[0xffff_ffbb, 0xbb, 0xffff_8899, 0x8899]);
    assert_eq!(c.regs[20], 0x8899_aabb);
    assert_eq!(c.regs[21], 0xaabb_bb00);
    assert_eq!((c.regs[22], c.regs[23], c.regs[24]), (0x8899_aabb, 1, 77));
    assert_eq!(mem.peek32(DATA_BASE + 8), 77);
    // The stack is memory too.
    let c = regs(&[i(0x09, 0, T0, 9), i(0x2B, 29, T0, -4), i(0x23, 29, T1, -4)]);
    assert_eq!(c.regs[T1 as usize], 9);
}

#[test]
fn unaligned_words_with_lwl_lwr_swl_swr() {
    // The bytes 00 11 22 .. 77 at DATA_BASE; the word at +1 is 0x44332211.
    let mut mem = Memory::new();
    mem.load(DATA_BASE, &[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]);
    let mut code = li(A0, DATA_BASE).to_vec();
    code.extend([
        i(0x22, A0, T0, 4), // lwl $t0, 4($a0)  (address + 3)
        i(0x26, A0, T0, 1), // lwr $t0, 1($a0)
        i(0x22, A0, T1, 2), // lwl alone: bytes 2..0 into the high three bytes, low byte kept (0)
        i(0x26, A0, T2, 6), // lwr alone: bytes 6..7 into the low two bytes
    ]);
    code.extend(li(T3, 0xaabb_ccdd));
    code.extend([
        i(0x2A, A0, T3, 0x0D), // swl $t3, 13($a0): address 0x..0d, bytes 0x0d, 0x0c
        i(0x2E, A0, T3, 0x12), // swr $t3, 18($a0): address 0x..12, bytes 0x12, 0x13
    ]);
    let (c, mem, s) = run_with(mem, &code);
    assert_eq!(s, Step::DroppedOff);
    assert_eq!(c.regs[T0 as usize], 0x4433_2211);
    assert_eq!(c.regs[T1 as usize], 0x2211_0000);
    assert_eq!(c.regs[T2 as usize], 0x0000_7766);
    assert_eq!(mem.read_bytes(DATA_BASE + 0x0C, 2), [0xbb, 0xaa]);
    assert_eq!(mem.read_bytes(DATA_BASE + 0x12, 2), [0xdd, 0xcc]);
}

#[test]
fn address_exceptions() {
    let mut code = li(A0, DATA_BASE + 2).to_vec();
    code.push(i(0x23, A0, T0, 0)); // lw unaligned
    let (cpu, e) = exception(&code);
    assert_eq!(e.code, ExcCode::AddressLoad);
    assert_eq!(e.bad_vaddr, Some(DATA_BASE + 2));
    assert_eq!(cpu.cp0_reg(cp0::VADDR), DATA_BASE + 2);
    assert_eq!(e.report(), "Runtime exception at 0x00400008: fetch address not aligned on word boundary 0x10010002");

    let (_, e) = exception(&[i(0x29, 0, T0, 1)]); // sh at 0x00000001
    assert_eq!((e.code, e.bad_vaddr), (ExcCode::AddressStore, Some(1)));
    assert!(e.message.starts_with("store address not aligned on halfword boundary"));

    let (_, e) = exception(&[i(0x20, 0, T0, 0x10)]); // lb at 0x00000010
    assert_eq!(e.message, "address out of range 0x00000010");

    let mut code = li(A0, TEXT_BASE).to_vec();
    code.push(i(0x2B, A0, T0, 0)); // sw into the text segment
    let (_, e) = exception(&code);
    assert_eq!(
        (e.code, e.message.as_str()),
        (ExcCode::AddressStore, "Cannot write directly to text segment!0x00400000")
    );

    // A refused swl leaves memory alone: 0x7fbffffd is the stack, 0x7fbffffc is not.
    let mut code = li(A0, 0x7fbf_fffc).to_vec();
    code.push(i(0x2A, A0, T0, 1));
    let (_, mem, s) = run(&code);
    assert!(matches!(s, Step::Exception(ref e) if e.code == ExcCode::AddressStore));
    assert_eq!(mem.pages_in_use(), 1, "only the program's page");
}

#[test]
fn branches_without_delay_slots() {
    // $t0 = 5 + 4 + 3 + 2 + 1 with a loop: bgtz back.
    let c = regs(&[
        i(0x09, 0, T1, 5),       // addiu $t1, $zero, 5
        sp(T0, T1, T0, 0, 0x21), // loop: addu $t0, $t0, $t1
        i(0x09, T1, T1, -1),     // addiu $t1, $t1, -1
        i(0x07, T1, 0, -3),      // bgtz $t1, loop
    ]);
    assert_eq!(c.regs[T0 as usize], 15);
    let c = regs(&[
        i(0x09, 0, T0, -1),
        i(0x04, T0, T0, 1), // beq taken: skips the next one
        i(0x09, 0, 16, 1),
        i(0x05, T0, T0, 1), // bne not taken
        i(0x09, 0, 17, 1),
        i(0x01, T0, 0x00, 1), // bltz taken
        i(0x09, 0, 18, 1),
        i(0x01, T0, 0x01, 1), // bgez not taken
        i(0x09, 0, 19, 1),
        i(0x06, 0, 0, 1), // blez $zero taken
        i(0x09, 0, 20, 1),
        i(0x01, T0, 0x11, 1), // bgezal not taken: MARS does not link
        i(0x01, T0, 0x10, 1), // bltzal taken: links
        i(0x09, 0, 21, 1),
    ]);
    assert_eq!(&c.regs[16..22], &[0, 1, 0, 1, 0, 0]);
    assert_eq!(c.regs[RA], TEXT_BASE + 13 * 4);
}

#[test]
fn jumps_and_links() {
    let sub = TEXT_BASE + 0x20;
    let mut code = vec![
        j(0x03, sub),              // jal sub
        i(0x09, 0, 16, 1),         // back here
        j(0x02, TEXT_BASE + 0x10), // j over the next
        i(0x09, 0, 17, 1),
        i(0x09, 0, 18, 1),         // 0x10
        j(0x02, TEXT_BASE + 0x40), // to the end
        0,
        0,
        // sub (0x20): $s3 = return address, jalr through $t0, jr $ra
        sp(RA as u32, 0, 19, 0, 0x21), // addu $s3, $ra, $zero
    ];
    code.extend(li(T0, TEXT_BASE + 0x38));
    code.push(sp(T0, 0, 20, 0, 0x09)); // jalr $s4, $t0  (0x2c)
    code.push(sp(19, 0, 0, 0, 0x08)); // jr $s3 back to 0x04 (0x30)
    code.push(0); // 0x34
    code.push(sp(20, 0, 0, 0, 0x08)); // 0x38: jr $s4 -> 0x30
    code.push(0); // 0x3c
    code.push(0); // 0x40: end
    let c = regs(&code);
    assert_eq!(c.regs[19], TEXT_BASE + 4);
    assert_eq!(c.regs[20], TEXT_BASE + 0x30);
    assert_eq!(&c.regs[16..19], &[1, 0, 1]);
}

#[test]
fn moves_and_traps() {
    let c = regs(&[i(0x09, 0, T0, 3), sp(T0, 0, 16, 0, 0x0A), sp(T0, 0, 17, 0, 0x0B), sp(T0, T0, 18, 0, 0x0B)]);
    assert_eq!(&c.regs[16..19], &[3, 0, 3], "movz moves when rt = 0, movn when rt != 0");

    for (trap, fires) in [
        (sp(T0, T0, 0, 0, 0x34), true),  // teq
        (sp(T0, T0, 0, 0, 0x36), false), // tne
        (sp(T0, 0, 0, 0, 0x30), true),   // tge 3 >= 0
        (sp(T0, 0, 0, 0, 0x32), false),  // tlt
        (i(0x01, T0, 0x0C, 3), true),    // teqi
        (i(0x01, T0, 0x0E, 3), false),   // tnei
        (i(0x01, T0, 0x0B, -1), true),   // tltiu 3 < 0xffffffff
        (i(0x01, T0, 0x0A, -1), false),  // tlti 3 < -1
        (i(0x01, T0, 0x09, -1), false),  // tgeiu
        (i(0x01, T0, 0x08, -1), true),   // tgei
        (sp(T0, 0, 0, 0, 0x31), true),   // tgeu
        (sp(T0, 0, 0, 0, 0x33), false),  // tltu
    ] {
        let (_, _, s) = run(&[i(0x09, 0, T0, 3), trap]);
        match s {
            Step::Exception(e) => {
                assert!(fires, "{trap:08x} should not trap");
                assert_eq!((e.code, e.message.as_str()), (ExcCode::Trap, "trap"));
            }
            s => assert!(!fires && s == Step::DroppedOff, "{trap:08x}: {s:?}"),
        }
    }
}

#[test]
fn syscall_break_and_reserved() {
    let (c, _, s) = run(&[i(0x09, 0, 2, 10), sp(0, 0, 0, 0, 0x0C)]);
    assert_eq!(s, Step::Syscall);
    assert_eq!((c.regs[2], c.pc), (10, TEXT_BASE + 8));

    let (_, e) = exception(&[sp(0, 0, 0, 0, 0x0D)]);
    assert_eq!((e.code, e.message.as_str()), (ExcCode::Breakpoint, "break instruction executed; no code given."));
    let (_, e) = exception(&[100 << 6 | 0x0D]);
    assert_eq!(e.message, "break instruction executed; code = 100.");

    let (_, e) = exception(&[0xFC00_0000]);
    assert_eq!((e.code, e.message.as_str()), (ExcCode::ReservedInstruction, "undefined instruction (0xfc000000)"));
    let (_, e) = exception(&[0x4680_0000]); // coprocessor 1 with format 20 (L): not in MARS
    assert_eq!(e.code, ExcCode::ReservedInstruction);
}

#[test]
fn invalid_program_counter() {
    let mut code = li(T0, TEXT_BASE + 2).to_vec();
    code.push(sp(T0, 0, 0, 0, 0x08)); // jr to an unaligned address
    let (cpu, e) = exception(&code);
    assert!(e.fetch);
    assert_eq!(e.report(), "invalid program counter value: 0x00400002");
    assert_eq!(cpu.cp0_reg(cp0::EPC), TEXT_BASE + 2);
    let mut code = li(T0, DATA_BASE).to_vec();
    code.push(sp(T0, 0, 0, 0, 0x08)); // jr into the data segment
    assert_eq!(exception(&code).1.code, ExcCode::AddressLoad);
}

#[test]
fn exception_handler_takes_the_exception() {
    // A handler like MARS's examples: skip the faulting instruction and return.
    let mut mem = Memory::new();
    load(
        &mut mem,
        EXCEPTION_HANDLER,
        &[
            r(0x10, 0, K0, 14, 0, 0), // mfc0 $k0, $14 (EPC)
            i(0x09, K0, K0, 4),       // addiu $k0, $k0, 4
            r(0x10, 4, K0, 14, 0, 0), // mtc0 $k0, $14
            r(0x10, 0, 27, 13, 0, 0), // mfc0 $k1, $13 (Cause)
            0x4200_0018,              // eret
        ],
    );
    let mut code = li(T1, 0x7fff_ffff).to_vec();
    code.push(i(0x08, T1, T0, 1)); // addi overflows
    code.push(i(0x09, 0, 16, 1)); // the handler returns here
    let (c, _, s) = run_with(mem, &code);
    assert_eq!(s, Step::DroppedOff);
    assert_eq!(c.regs[16], 1);
    assert_eq!(c.regs[27] >> 2 & 31, ExcCode::Overflow as u32);
    assert_eq!(c.cp0_reg(cp0::STATUS) & cp0::EXL, 0, "eret clears the exception level");
    assert_eq!(c.cp0_reg(cp0::STATUS), 0x0000_ff11);
}

#[test]
fn step_back_with_the_journal() {
    let mut mem = Memory::new();
    let mut code = li(A0, DATA_BASE).to_vec();
    code.extend([i(0x09, 0, T0, 0x55), i(0x28, A0, T0, 3)]); // sb
    load(&mut mem, TEXT_BASE, &code);
    let mut cpu = Cpu::new();
    for _ in 0..3 {
        cpu.step(&mut mem);
    }
    mem.set_journaling(true);
    let before = cpu.clone();
    assert_eq!(cpu.step(&mut mem), Step::Ok);
    assert_eq!(mem.peek8(DATA_BASE + 3), 0x55);
    let undo = mem.take_journal();
    mem.undo(&undo);
    cpu = before;
    assert_eq!(mem.peek8(DATA_BASE + 3), 0);
    assert_eq!(cpu.pc, TEXT_BASE + 12, "back at the store");
    assert_eq!(memory::Segment::of(DATA_BASE + 3), Some(memory::Segment::Data));
}

#[test]
fn the_last_alu_operation_is_recorded() {
    // li $t0, 7; li $t1, -2; nop; sub $t2, $t0, $t1; lw (not ALU); sll $t3, $t0, 4; mult $t0, $t1
    let mut code = vec![];
    code.extend(li(T0, 7));
    code.extend(li(T1, -2i32 as u32));
    code.push(0);
    code.push(sp(T0, T1, T2, 0, 0x22));
    let (cpu, ..) = run(&code);
    let t = cpu.alu.expect("sub was recorded, nop was not");
    assert_eq!((t.pc, t.a, t.b, t.result, t.hi, t.overflow), (TEXT_BASE + 20, 7, -2i32 as u32, 9, None, false));
    code.push(sp(0, T0, T3, 4, 0x00));
    let (cpu, ..) = run(&code);
    let t = cpu.alu.unwrap();
    assert_eq!((t.a, t.b, t.result), (7, 4, 112));
    code.push(sp(T0, T1, 0, 0, 0x18));
    let (cpu, ..) = run(&code);
    let t = cpu.alu.unwrap();
    assert_eq!((t.result, t.hi), (-14i32 as u32, Some(u32::MAX)));
    // addi overflow: the result is recorded, not written.
    let mut code = li(T0, 0x7FFF_FFFF).to_vec();
    code.push(i(0x08, T0, T1, 1));
    let (cpu, ..) = run(&code);
    let t = cpu.alu.unwrap();
    assert!(t.overflow && t.result == 0x8000_0000 && cpu.regs[T1 as usize] == 0);
}

/// Coprocessor 1 as MARS: Java's arithmetic, canonical NaNs, MARS's rounding to words and its
/// checks of even registers.
#[test]
fn floating_point_as_mars() {
    use rustemu_mips::asm::assemble;
    let run_src = |src: &str| {
        let mut m = rustemu_mips::Machine::new();
        m.load(&assemble(src).unwrap().image).unwrap();
        let ev = m.run(10_000);
        (m, ev)
    };
    let (m, _) = run_src(
        ".data
a: .float 1.5
b: .float -2.25
d: .double 0.1
.text
l.s $f0, a
l.s $f1, b
add.s $f2, $f0, $f1
mul.s $f3, $f0, $f1
sub.s $f4, $f0, $f0
div.s $f5, $f4, $f4
l.d $f6, d
add.d $f8, $f6, $f6
cvt.w.s $f10, $f1
round.w.s $f11, $f0
c.lt.s 3, $f1, $f0
c.eq.d $f6, $f8
neg.s $f12, $f0
sqrt.s $f13, $f1
li $t0, 7
mtc1 $t0, $f14
cvt.s.w $f15, $f14
floor.w.d $f16, $f8
",
    );
    let f = |r: usize| f32::from_bits(m.cpu.fpr[r]);
    assert_eq!((f(2), f(3)), (-0.75, -3.375));
    assert_eq!(m.cpu.fpr[5], 0x7fc0_0000, "0/0: the canonical NaN");
    assert_eq!(m.cpu.f64(8), 0.2);
    assert_eq!(m.cpu.fpr[10] as i32, -2, "cvt.w truncates");
    assert_eq!(m.cpu.fpr[11] as i32, 2, "1.5 rounds to the even 2");
    assert_eq!((m.cpu.fcc(3), m.cpu.fcc(0)), (true, false));
    assert_eq!(f(12), -1.5);
    assert_eq!(m.cpu.fpr[13], 0x7fc0_0000, "sqrt of a negative number");
    assert_eq!(f(15), 7.0);
    assert_eq!(m.cpu.fpr[16], 0, "floor of 0.2");

    // An odd register in a double operation: MARS's message, no exception code (CP0 unchanged).
    let (m, ev) = run_src(".text\nadd.d $f2, $f3, $f4\n");
    assert_eq!(
        ev,
        rustemu_target::Event::Error("Runtime exception at 0x00400000: all registers must be even-numbered".into())
    );
    assert_eq!(m.cpu.cp0[13], 0, "Cause is not set");
    // ldc1 at an address that is not a multiple of 8.
    let (_, ev) = run_src(".data\n.word 0\nx: .word 0, 0\n.text\nla $t0, x\nldc1 $f2, 0($t0)\n");
    assert_eq!(
        ev,
        rustemu_target::Event::Error(
            "Runtime exception at 0x00400008: address not aligned on doubleword boundary 0x10010004".into()
        )
    );
}
