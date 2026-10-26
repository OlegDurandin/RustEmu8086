use rustemu_core::bus::{Bus, Memory};
use rustemu_core::cpu::{flags::*, *};
use rustemu_core::disasm::disassemble;

struct TestBus {
    mem: Memory,
    ports: Vec<(u16, u8)>,
}

impl Bus for TestBus {
    fn read8(&mut self, a: u32) -> u8 {
        self.mem.read8(a)
    }
    fn write8(&mut self, a: u32, v: u8) {
        self.mem.write8(a, v)
    }
    fn port_in8(&mut self, p: u16) -> u8 {
        p as u8 ^ 0x5A
    }
    fn port_out8(&mut self, p: u16, v: u8) {
        self.ports.push((p, v));
    }
}

/// Loads `code` at 0100:0000 with SS:SP = 0000:FFFE and runs until HLT.
fn run(code: &[u8], setup: impl FnOnce(&mut Cpu)) -> (Cpu, TestBus) {
    let mut bus = TestBus { mem: Memory::new(), ports: vec![] };
    let mut prog = code.to_vec();
    prog.push(0xF4);
    bus.mem.load(0x1000, &prog);
    let mut cpu = Cpu::new();
    cpu.sregs = [0x0100, 0x0100, 0x0000, 0x0100];
    cpu.ip = 0;
    cpu.regs[SP] = 0xFFFE;
    setup(&mut cpu);
    for _ in 0..100_000 {
        if cpu.step(&mut bus) == StepResult::Halted {
            return (cpu, bus);
        }
    }
    panic!("did not halt");
}

fn flags_of(c: &Cpu) -> (bool, bool, bool, bool, bool, bool) {
    (c.flag(CF), c.flag(ZF), c.flag(SF), c.flag(OF), c.flag(PF), c.flag(AF))
}

#[test]
fn mov_and_add_flags() {
    // MOV AX,7FFF ; ADD AX,1
    let (c, _) = run(&[0xB8, 0xFF, 0x7F, 0x05, 0x01, 0x00], |_| {});
    assert_eq!(c.regs[AX], 0x8000);
    let (cf, zf, sf, of, pf, af) = flags_of(&c);
    assert!(!cf && !zf && sf && of && pf && af);
}

#[test]
fn sub_borrow_and_cmp() {
    // MOV AL,1 ; SUB AL,2  -> FF, CF=1 SF=1
    let (c, _) = run(&[0xB0, 0x01, 0x2C, 0x02], |_| {});
    assert_eq!(c.reg8(0), 0xFF);
    assert!(c.flag(CF) && c.flag(SF) && !c.flag(OF) && !c.flag(ZF));
    // MOV AL,80h ; CMP AL,1 -> OF=1, AL unchanged
    let (c, _) = run(&[0xB0, 0x80, 0x3C, 0x01], |_| {});
    assert_eq!(c.reg8(0), 0x80);
    assert!(c.flag(OF) && !c.flag(CF) && !c.flag(SF));
}

#[test]
fn adc_sbb() {
    // STC ; MOV AL,0FFh ; ADC AL,0 -> 0, CF=1, ZF=1
    let (c, _) = run(&[0xF9, 0xB0, 0xFF, 0x14, 0x00], |_| {});
    assert_eq!(c.reg8(0), 0);
    assert!(c.flag(CF) && c.flag(ZF));
    // STC ; MOV AL,0 ; SBB AL,0 -> FF, CF=1
    let (c, _) = run(&[0xF9, 0xB0, 0x00, 0x1C, 0x00], |_| {});
    assert_eq!(c.reg8(0), 0xFF);
    assert!(c.flag(CF));
}

#[test]
fn inc_dec_preserve_cf() {
    // STC ; MOV BX,0FFFFh ; INC BX
    let (c, _) = run(&[0xF9, 0xBB, 0xFF, 0xFF, 0x43], |_| {});
    assert_eq!(c.regs[BX], 0);
    assert!(c.flag(CF) && c.flag(ZF));
}

#[test]
fn reg8_halves() {
    // MOV AH,12h ; MOV AL,34h ; MOV BH,AL ; MOV BL,AH
    let (c, _) = run(&[0xB4, 0x12, 0xB0, 0x34, 0x88, 0xC7, 0x88, 0xE3], |_| {});
    assert_eq!(c.regs[AX], 0x1234);
    assert_eq!(c.regs[BX], 0x3412);
}

#[test]
fn memory_modrm_and_segments() {
    // MOV BX,10h ; MOV SI,2 ; MOV WORD PTR [BX+SI+4],1234h ; MOV AX,[0016h]
    let code = [0xBB, 0x10, 0x00, 0xBE, 0x02, 0x00, 0xC7, 0x40, 0x04, 0x34, 0x12, 0xA1, 0x16, 0x00];
    let (c, b) = run(&code, |_| {});
    assert_eq!(c.regs[AX], 0x1234);
    assert_eq!(b.mem.read16(0x1016), 0x1234);
    // ES: override  MOV AX,1 ; MOV ES,AX ; ES: MOV [0], AL
    let (_, b) = run(&[0xB8, 0x01, 0x00, 0x8E, 0xC0, 0x26, 0xA2, 0x00, 0x00], |_| {});
    assert_eq!(b.mem.read8(0x10), 1);
}

#[test]
fn bp_defaults_to_ss() {
    // MOV BP,10h ; MOV BYTE PTR [BP+0],77h  (SS=0)
    let (_, b) = run(&[0xBD, 0x10, 0x00, 0xC6, 0x46, 0x00, 0x77], |_| {});
    assert_eq!(b.mem.read8(0x10), 0x77);
}

#[test]
fn push_pop_call_ret() {
    // MOV AX,1234h ; PUSH AX ; POP BX ; CALL +1 ; HLT ; (sub:) INC BX ; RET
    let code = [0xB8, 0x34, 0x12, 0x50, 0x5B, 0xE8, 0x01, 0x00, 0xF4, 0x43, 0xC3];
    let (c, _) = run(&code, |_| {});
    assert_eq!(c.regs[BX], 0x1235);
    assert_eq!(c.regs[SP], 0xFFFE);
}

#[test]
fn loop_and_jcc() {
    // MOV CX,5 ; XOR AX,AX ; (l:) ADD AX,CX ; LOOP l
    let (c, _) = run(&[0xB9, 0x05, 0x00, 0x31, 0xC0, 0x01, 0xC8, 0xE2, 0xFC], |_| {});
    assert_eq!(c.regs[AX], 15);
    assert_eq!(c.regs[CX], 0);
    // MOV AL,5 ; CMP AL,3 ; JG +2 ; MOV AL,0 ; (x:) HLT
    let (c, _) = run(&[0xB0, 0x05, 0x3C, 0x03, 0x7F, 0x02, 0xB0, 0x00], |_| {});
    assert_eq!(c.reg8(0), 5);
}

#[test]
fn mul_div() {
    // MOV AX,1000 ; MOV BX,1000 ; MUL BX -> DX:AX = 1_000_000
    let (c, _) = run(&[0xB8, 0xE8, 0x03, 0xBB, 0xE8, 0x03, 0xF7, 0xE3], |_| {});
    assert_eq!(((c.regs[DX] as u32) << 16) | c.regs[AX] as u32, 1_000_000);
    assert!(c.flag(CF) && c.flag(OF));
    // DIV BX after: back to 1000 remainder 0
    let (c, _) = run(&[0xB8, 0xE8, 0x03, 0xBB, 0xE8, 0x03, 0xF7, 0xE3, 0xF7, 0xF3], |_| {});
    assert_eq!(c.regs[AX], 1000);
    assert_eq!(c.regs[DX], 0);
    // MOV AL,-3 ; MOV BL,4 ; IMUL BL -> AX = -12
    let (c, _) = run(&[0xB0, 0xFD, 0xB3, 0x04, 0xF6, 0xEB], |_| {});
    assert_eq!(c.regs[AX], (-12i16) as u16);
    assert!(!c.flag(CF));
    // MOV AX,-7 ; MOV BL,2 ; IDIV BL -> AL=-3 AH=-1
    let (c, _) = run(&[0xB8, 0xF9, 0xFF, 0xB3, 0x02, 0xF6, 0xFB], |_| {});
    assert_eq!(c.reg8(0) as i8, -3);
    assert_eq!(c.reg8(4) as i8, -1);
}

#[test]
fn divide_by_zero_raises_int0() {
    let mut bus = TestBus { mem: Memory::new(), ports: vec![] };
    // IVT[0] -> 0200:0000 where HLT lives.
    bus.mem.write16(0, 0x0000);
    bus.mem.write16(2, 0x0200);
    bus.mem.write8(0x2000, 0xF4);
    // MOV BL,0 ; DIV BL
    bus.mem.load(0x1000, &[0xB3, 0x00, 0xF6, 0xF3, 0x90]);
    let mut cpu = Cpu::new();
    cpu.sregs = [0, 0x100, 0, 0];
    cpu.ip = 0;
    cpu.regs[SP] = 0x8000;
    cpu.set_flag(IF, true);
    while cpu.step(&mut bus) != StepResult::Halted {}
    assert_eq!(cpu.sregs[CS], 0x200);
    // Return address = next instruction (8086 behaviour).
    assert_eq!(bus.mem.read16(0x7FFA), 4);
    assert_eq!(bus.mem.read16(0x7FFC), 0x100);
    assert!(!cpu.flag(IF));
}

#[test]
fn shifts_and_rotates() {
    // MOV AL,81h ; SHL AL,1 -> 02 CF=1 OF=1
    let (c, _) = run(&[0xB0, 0x81, 0xD0, 0xE0], |_| {});
    assert_eq!(c.reg8(0), 0x02);
    assert!(c.flag(CF) && c.flag(OF));
    // MOV AL,81h ; ROL AL,1 -> 03 CF=1
    let (c, _) = run(&[0xB0, 0x81, 0xD0, 0xC0], |_| {});
    assert_eq!(c.reg8(0), 0x03);
    assert!(c.flag(CF));
    // MOV AL,80h ; MOV CL,3 ; SAR AL,CL -> F0
    let (c, _) = run(&[0xB0, 0x80, 0xB1, 0x03, 0xD2, 0xF8], |_| {});
    assert_eq!(c.reg8(0), 0xF0);
    // CLC ; MOV AX,1 ; RCR AX,1 -> 0, CF=1 ; RCR AX,1 -> 8000h CF=0
    let (c, _) = run(&[0xF8, 0xB8, 0x01, 0x00, 0xD1, 0xD8, 0xD1, 0xD8], |_| {});
    assert_eq!(c.regs[AX], 0x8000);
    assert!(!c.flag(CF));
    // MOV AL,1 ; SHR AL,1 -> 0, CF=1 ZF=1
    let (c, _) = run(&[0xB0, 0x01, 0xD0, 0xE8], |_| {});
    assert!(c.flag(CF) && c.flag(ZF) && !c.flag(OF));
}

#[test]
fn strings_rep() {
    // Source at DS:0100 = "ABCD", copy to ES:0200 with REP MOVSB, then REPNE SCASB for 'C'.
    let code = [
        0xBE, 0x00, 0x01, // MOV SI,100h
        0xBF, 0x00, 0x02, // MOV DI,200h
        0xB9, 0x04, 0x00, // MOV CX,4
        0xFC, // CLD
        0xF3, 0xA4, // REP MOVSB
        0xBF, 0x00, 0x02, // MOV DI,200h
        0xB9, 0x04, 0x00, // MOV CX,4
        0xB0, b'C', // MOV AL,'C'
        0xF2, 0xAE, // REPNE SCASB
    ];
    let bus_init = |b: &mut TestBus| b.mem.load(0x1100, b"ABCD");
    let mut bus = TestBus { mem: Memory::new(), ports: vec![] };
    bus_init(&mut bus);
    let mut prog = code.to_vec();
    prog.push(0xF4);
    bus.mem.load(0x1000, &prog);
    let mut cpu = Cpu::new();
    cpu.sregs = [0x100, 0x100, 0, 0x100];
    cpu.ip = 0;
    cpu.regs[SP] = 0xFFFE;
    // Program overlaps data region at 0x1100? code is at 0x1000..0x1017, fine.
    while cpu.step(&mut bus) != StepResult::Halted {}
    assert_eq!(bus.mem.slice(0x1200, 4), b"ABCD");
    assert_eq!(cpu.regs[DI], 0x203);
    assert_eq!(cpu.regs[CX], 1);
    assert!(cpu.flag(ZF));
}

#[test]
fn bcd() {
    // MOV AL,79h ; ADD AL,35h ; DAA -> 14h CF=1
    let (c, _) = run(&[0xB0, 0x79, 0x04, 0x35, 0x27], |_| {});
    assert_eq!(c.reg8(0), 0x14);
    assert!(c.flag(CF));
    // MOV AL,35h ; SUB AL,47h ; DAS -> 88h CF=1
    let (c, _) = run(&[0xB0, 0x35, 0x2C, 0x47, 0x2F], |_| {});
    assert_eq!(c.reg8(0), 0x88);
    assert!(c.flag(CF));
    // MOV AX,0009h ; ADD AL,8 ; AAA -> AX=0107h
    let (c, _) = run(&[0xB8, 0x09, 0x00, 0x04, 0x08, 0x37], |_| {});
    assert_eq!(c.regs[AX], 0x0107);
    // MOV AL,63 ; AAM -> AX = 0603h ; AAD -> AX = 63
    let (c, _) = run(&[0xB0, 63, 0xD4, 0x0A], |_| {});
    assert_eq!(c.regs[AX], 0x0603);
    let (c, _) = run(&[0xB0, 63, 0xD4, 0x0A, 0xD5, 0x0A], |_| {});
    assert_eq!(c.regs[AX], 63);
}

#[test]
fn ports() {
    // MOV AL,7 ; OUT 4,AL ; MOV DX,3F8h ; IN AL,DX
    let (c, b) = run(&[0xB0, 0x07, 0xE6, 0x04, 0xBA, 0xF8, 0x03, 0xEC], |_| {});
    assert_eq!(b.ports, vec![(4, 7)]);
    assert_eq!(c.reg8(0), 0xF8 ^ 0x5A);
}

#[test]
fn flags_push_pop_and_lahf() {
    // STC ; PUSHF ; POP AX
    let (c, _) = run(&[0xF9, 0x9C, 0x58], |_| {});
    assert_eq!(c.regs[AX] & 0xF003, 0xF003);
    // MOV AH,0D5h ; SAHF ; LAHF
    let (c, _) = run(&[0xB4, 0xD5, 0x9E, 0xB4, 0x00, 0x9F], |_| {});
    assert_eq!(c.reg8(4), 0xD7);
}

#[test]
fn far_jump_call_and_int() {
    let mut bus = TestBus { mem: Memory::new(), ports: vec![] };
    // IVT[21h] -> 0300:0000: INC AX ; IRET
    bus.mem.write16(0x84, 0);
    bus.mem.write16(0x86, 0x300);
    bus.mem.load(0x3000, &[0x40, 0xCF]);
    // 0100:0000: CALL 0200:0000 ; INT 21h ; HLT
    bus.mem.load(0x1000, &[0x9A, 0x00, 0x00, 0x00, 0x02, 0xCD, 0x21, 0xF4]);
    // 0200:0000: MOV BX,5 ; RETF
    bus.mem.load(0x2000, &[0xBB, 0x05, 0x00, 0xCB]);
    let mut cpu = Cpu::new();
    cpu.sregs = [0, 0x100, 0, 0];
    cpu.ip = 0;
    cpu.regs[SP] = 0x8000;
    while cpu.step(&mut bus) != StepResult::Halted {}
    assert_eq!(cpu.regs[BX], 5);
    assert_eq!(cpu.regs[AX], 1);
    assert_eq!(cpu.regs[SP], 0x8000);
    assert_eq!(cpu.sregs[CS], 0x100);
}

#[test]
fn bop_is_reported() {
    let (c, _) = {
        let mut bus = TestBus { mem: Memory::new(), ports: vec![] };
        bus.mem.load(0, &[0x0F, 0xFF, 0x21]);
        let mut cpu = Cpu::new();
        cpu.sregs[CS] = 0;
        cpu.ip = 0;
        assert_eq!(cpu.step(&mut bus), StepResult::Bop(0x21));
        (cpu, bus)
    };
    assert_eq!(c.ip, 3);
}

#[test]
fn trap_flag_single_step() {
    let mut bus = TestBus { mem: Memory::new(), ports: vec![] };
    bus.mem.write16(4, 0);
    bus.mem.write16(6, 0x300);
    bus.mem.load(0x3000, &[0xF4]);
    bus.mem.load(0x1000, &[0x90, 0x90]);
    let mut cpu = Cpu::new();
    cpu.sregs = [0, 0x100, 0, 0];
    cpu.ip = 0;
    cpu.regs[SP] = 0x8000;
    cpu.set_flag(TF, true);
    cpu.step(&mut bus);
    assert_eq!(cpu.sregs[CS], 0x300);
    assert_eq!(bus.mem.read16(0x7FFA), 1);
}

#[test]
fn disasm_samples() {
    let cases: &[(&[u8], &str)] = &[
        (&[0xB8, 0x34, 0x12], "MOV AX, 1234h"),
        (&[0x8B, 0x40, 0x04], "MOV AX, [BX+SI+04h]"),
        (&[0x26, 0x88, 0x46, 0xFE], "MOV ES:[BP-02h], AL"),
        (&[0xC7, 0x06, 0x00, 0x01, 0xFF, 0x00], "MOV WORD PTR [0100h], 00FFh"),
        (&[0xCD, 0x21], "INT 21h"),
        (&[0xF3, 0xA4], "REP MOVSB"),
        (&[0x83, 0xEB, 0xFF], "SUB BX, -01h"),
        (&[0xD2, 0xE0], "SHL AL, CL"),
        (&[0xEA, 0x00, 0x00, 0xFF, 0xFF], "JMP 0FFFFh:0000h"),
    ];
    for (bytes, text) in cases {
        let i = disassemble(bytes, 0x100);
        assert_eq!(i.text, *text);
        assert_eq!(i.len, bytes.len(), "{text}");
    }
    // Relative jump target: EB FE at 100h jumps to itself.
    assert_eq!(disassemble(&[0xEB, 0xFE], 0x100).text, "JMP 0100h");
    assert_eq!(disassemble(&[0xE8, 0x00, 0x00], 0x100).text, "CALL 0103h");
}

#[test]
fn i80186_extensions() {
    let mut bus = TestBus { mem: Memory::new(), ports: vec![] };
    // MOV AX,1 ; MOV BX,2 ; PUSHA ; XOR AX,AX ; XOR BX,BX ; POPA ; PUSH 1234h ; POP CX ;
    // PUSH -2 ; POP DX ; IMUL SI, BX, 7 ; SHL BX, 3 ; HLT
    let code = [
        0xB8, 0x01, 0x00, 0xBB, 0x02, 0x00, 0x60, 0x31, 0xC0, 0x31, 0xDB, 0x61, 0x68, 0x34, 0x12, 0x59, 0x6A, 0xFE,
        0x5A, 0x6B, 0xF3, 0x07, 0xC1, 0xE3, 0x03, 0xF4,
    ];
    bus.mem.load(0x1000, &code);
    let mut cpu = Cpu::new();
    cpu.model = CpuModel::I80186;
    cpu.sregs = [0x100, 0x100, 0, 0x100];
    cpu.ip = 0;
    cpu.regs[SP] = 0xFFFE;
    while cpu.step(&mut bus) != StepResult::Halted {}
    assert_eq!(cpu.regs[AX], 1);
    assert_eq!(cpu.regs[CX], 0x1234);
    assert_eq!(cpu.regs[DX], 0xFFFE);
    assert_eq!(cpu.regs[SI], 14);
    assert_eq!(cpu.regs[BX], 16);
    assert_eq!(cpu.regs[SP], 0xFFFE);
    assert_eq!(disassemble(&[0x60], 0).text, "PUSHA");
    assert_eq!(disassemble(&[0xC1, 0xE3, 0x03], 0).text, "SHL BX, 03h");
}

#[test]
fn alu_trace_records_last_operation() {
    // mov al, 250 / add al, 10 / mov cx, 5 (no ALU) -> ADD 250 + 10 = 4 with CF.
    let (c, _) = run(&[0xB0, 0xFA, 0x04, 0x0A, 0xB9, 0x05, 0x00], |_| {});
    let t = c.alu_trace;
    assert_eq!((t.op, t.a, t.b, t.result, t.wide), (AluOp::Add, 250, 10, 4, false));
    assert_eq!(t.ip, 2);
    assert!(t.flags_out & CF != 0 && t.flags_in & CF == 0);
    // inc bx -> INC (traced as ADD internally); mov bl, 4 / mul bl with AL = 200 -> 800.
    let (c, _) = run(&[0x43], |_| {});
    assert_eq!(c.alu_trace.op, AluOp::Inc);
    let (c, _) = run(&[0xB0, 200, 0xB3, 4, 0xF6, 0xE3], |_| {});
    assert_eq!((c.alu_trace.op, c.alu_trace.a, c.alu_trace.b, c.alu_trace.result), (AluOp::Mul, 200, 4, 800));
    // mov ax, 1000 / xor dx, dx / mov bx, 7 / div bx -> quotient 142, remainder 6.
    let (c, _) = run(&[0xB8, 0xE8, 0x03, 0x31, 0xD2, 0xBB, 7, 0, 0xF7, 0xF3], |_| {});
    let t = c.alu_trace;
    assert_eq!((t.op, t.a, t.b, t.result & 0xFFFF, t.result >> 16), (AluOp::Div, 1000, 7, 142, 6));
    // mov cl, 4 / shl al, cl with AL = 3 -> 48; test al, 1 -> TEST; neg al -> NEG.
    let (c, _) = run(&[0xB1, 4, 0xB0, 3, 0xD2, 0xE0], |_| {});
    assert_eq!((c.alu_trace.op, c.alu_trace.a, c.alu_trace.b, c.alu_trace.result), (AluOp::Shl, 3, 4, 48));
    let (c, _) = run(&[0xA8, 0x01], |_| {});
    assert_eq!(c.alu_trace.op, AluOp::Test);
    let (c, _) = run(&[0xB0, 5, 0xF6, 0xD8], |_| {});
    assert_eq!((c.alu_trace.op, c.alu_trace.a, c.alu_trace.result), (AluOp::Neg, 5, 0xFB));
}
