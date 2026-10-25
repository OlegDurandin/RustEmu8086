//! Intel 8086 CPU interpreter.

use crate::bus::{Bus, linear};

// General-purpose register indices (encoding order).
pub const AX: usize = 0;
pub const CX: usize = 1;
pub const DX: usize = 2;
pub const BX: usize = 3;
pub const SP: usize = 4;
pub const BP: usize = 5;
pub const SI: usize = 6;
pub const DI: usize = 7;

// Segment register indices (encoding order).
pub const ES: usize = 0;
pub const CS: usize = 1;
pub const SS: usize = 2;
pub const DS: usize = 3;

pub mod flags {
    pub const CF: u16 = 1 << 0;
    pub const PF: u16 = 1 << 2;
    pub const AF: u16 = 1 << 4;
    pub const ZF: u16 = 1 << 6;
    pub const SF: u16 = 1 << 7;
    pub const TF: u16 = 1 << 8;
    pub const IF: u16 = 1 << 9;
    pub const DF: u16 = 1 << 10;
    pub const OF: u16 = 1 << 11;
    /// Bits that can actually be changed on an 8086.
    pub const WRITABLE: u16 = 0x0FD5;
    /// Bits that always read as 1 on an 8086.
    pub const FIXED: u16 = 0xF002;
    /// Arithmetic/status flags.
    pub const STATUS: u16 = CF | PF | AF | ZF | SF | OF;
}
use flags::*;

pub const REG16_NAMES: [&str; 8] = ["AX", "CX", "DX", "BX", "SP", "BP", "SI", "DI"];
pub const REG8_NAMES: [&str; 8] = ["AL", "CL", "DL", "BL", "AH", "CH", "DH", "BH"];
pub const SREG_NAMES: [&str; 4] = ["ES", "CS", "SS", "DS"];

/// Opcode prefix used by ROM stubs to call host ("HLE") interrupt handlers: `0F FF nn`.
pub const BOP_PREFIX: [u8; 2] = [0x0F, 0xFF];

/// Outcome of a single [`Cpu::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    /// An instruction (or a hardware interrupt entry) was executed.
    Ok,
    /// The CPU is halted (HLT) and waits for an interrupt.
    Halted,
    /// A BIOS-operation stub `0F FF nn` was executed; the host must service function `nn`.
    /// IP already points past the 3-byte BOP instruction.
    Bop(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rep {
    None,
    /// REPE/REPZ (F3)
    Repe,
    /// REPNE/REPNZ (F2)
    Repne,
}

/// Decoded r/m operand.
#[derive(Clone, Copy, Debug)]
enum Operand {
    Reg(u8),
    Mem { seg: u16, off: u16 },
}

#[derive(Clone, Copy, Debug)]
struct ModRm {
    reg: u8,
    rm: Operand,
    /// Offset part of the effective address (valid for memory operands).
    ea: u16,
}

/// Instruction set to emulate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CpuModel {
    /// Strict 8086: 60-6F are aliases of the conditional jumps, C0/C1/C8/C9 of RET/RETF.
    #[default]
    I8086,
    /// 8086 plus the 80186 additions used by Emu8086 programs (PUSHA, POPA, PUSH imm, ...).
    I80186,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cpu {
    pub model: CpuModel,
    pub regs: [u16; 8],
    pub sregs: [u16; 4],
    pub ip: u16,
    pub flags: u16,
    pub halted: bool,
    /// Number of instructions executed since reset.
    pub instr_count: u64,
    seg_override: Option<usize>,
    rep: Rep,
    /// Interrupts are inhibited for one instruction after MOV/POP to SS and after STI-like cases.
    inhibit_irq: bool,
    /// Last computed effective address (8086 uses it for LEA/LDS with a register operand).
    last_ea: u16,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn parity(b: u8) -> bool {
    b.count_ones() & 1 == 0
}

impl Cpu {
    pub fn new() -> Self {
        let mut c = Cpu {
            model: CpuModel::I8086,
            regs: [0; 8],
            sregs: [0; 4],
            ip: 0,
            flags: FIXED,
            halted: false,
            instr_count: 0,
            seg_override: None,
            rep: Rep::None,
            inhibit_irq: false,
            last_ea: 0,
        };
        c.reset();
        c
    }

    /// Hardware reset: CS:IP = FFFF:0000, everything else zero.
    pub fn reset(&mut self) {
        self.regs = [0; 8];
        self.sregs = [0, 0xFFFF, 0, 0];
        self.ip = 0;
        self.flags = FIXED;
        self.halted = false;
        self.instr_count = 0;
        self.seg_override = None;
        self.rep = Rep::None;
        self.inhibit_irq = false;
    }

    // ---- register helpers -------------------------------------------------

    #[inline]
    pub fn reg8(&self, r: u8) -> u8 {
        let r = r as usize;
        if r < 4 { self.regs[r] as u8 } else { (self.regs[r - 4] >> 8) as u8 }
    }

    #[inline]
    pub fn set_reg8(&mut self, r: u8, v: u8) {
        let r = r as usize;
        if r < 4 {
            self.regs[r] = (self.regs[r] & 0xFF00) | v as u16;
        } else {
            self.regs[r - 4] = (self.regs[r - 4] & 0x00FF) | ((v as u16) << 8);
        }
    }

    #[inline]
    pub fn flag(&self, f: u16) -> bool {
        self.flags & f != 0
    }

    #[inline]
    pub fn set_flag(&mut self, f: u16, on: bool) {
        if on {
            self.flags |= f;
        } else {
            self.flags &= !f;
        }
    }

    /// Sets FLAGS from a 16-bit image honouring 8086 fixed bits.
    #[inline]
    pub fn set_flags_word(&mut self, v: u16) {
        self.flags = (v & WRITABLE) | FIXED;
    }

    #[inline]
    pub fn cs(&self) -> u16 {
        self.sregs[CS]
    }

    /// Compares the architectural state (registers, flags, halt), ignoring counters.
    pub fn same_state(&self, o: &Cpu) -> bool {
        self.regs == o.regs
            && self.sregs == o.sregs
            && self.ip == o.ip
            && self.flags == o.flags
            && self.halted == o.halted
    }

    /// Physical address of CS:IP.
    pub fn pc(&self) -> u32 {
        linear(self.sregs[CS], self.ip)
    }

    // ---- memory helpers ---------------------------------------------------

    #[inline]
    fn rd8<B: Bus>(&self, bus: &mut B, seg: u16, off: u16) -> u8 {
        bus.read8(linear(seg, off))
    }

    #[inline]
    fn rd16<B: Bus>(&self, bus: &mut B, seg: u16, off: u16) -> u16 {
        // Word accesses wrap inside the segment.
        let lo = bus.read8(linear(seg, off)) as u16;
        let hi = bus.read8(linear(seg, off.wrapping_add(1))) as u16;
        lo | (hi << 8)
    }

    #[inline]
    fn wr8<B: Bus>(&self, bus: &mut B, seg: u16, off: u16, v: u8) {
        bus.write8(linear(seg, off), v);
    }

    #[inline]
    fn wr16<B: Bus>(&self, bus: &mut B, seg: u16, off: u16, v: u16) {
        bus.write8(linear(seg, off), v as u8);
        bus.write8(linear(seg, off.wrapping_add(1)), (v >> 8) as u8);
    }

    #[inline]
    fn fetch8<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let v = bus.read8(linear(self.sregs[CS], self.ip));
        self.ip = self.ip.wrapping_add(1);
        v
    }

    #[inline]
    fn fetch16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch8(bus) as u16;
        let hi = self.fetch8(bus) as u16;
        lo | (hi << 8)
    }

    pub fn push<B: Bus>(&mut self, bus: &mut B, v: u16) {
        self.regs[SP] = self.regs[SP].wrapping_sub(2);
        let (ss, sp) = (self.sregs[SS], self.regs[SP]);
        self.wr16(bus, ss, sp, v);
    }

    pub fn pop<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let v = self.rd16(bus, self.sregs[SS], self.regs[SP]);
        self.regs[SP] = self.regs[SP].wrapping_add(2);
        v
    }

    // ---- ModR/M -------------------------------------------------------------

    fn seg_or(&self, default: usize) -> u16 {
        self.sregs[self.seg_override.unwrap_or(default)]
    }

    fn modrm<B: Bus>(&mut self, bus: &mut B) -> ModRm {
        let b = self.fetch8(bus);
        let md = b >> 6;
        let reg = (b >> 3) & 7;
        let rm = b & 7;
        if md == 3 {
            return ModRm { reg, rm: Operand::Reg(rm), ea: self.last_ea };
        }
        let disp: u16 = match md {
            0 if rm == 6 => self.fetch16(bus),
            0 => 0,
            1 => self.fetch8(bus) as i8 as u16,
            _ => self.fetch16(bus),
        };
        let r = &self.regs;
        let (base, def_seg) = match rm {
            0 => (r[BX].wrapping_add(r[SI]), DS),
            1 => (r[BX].wrapping_add(r[DI]), DS),
            2 => (r[BP].wrapping_add(r[SI]), SS),
            3 => (r[BP].wrapping_add(r[DI]), SS),
            4 => (r[SI], DS),
            5 => (r[DI], DS),
            6 if md == 0 => (0, DS),
            6 => (r[BP], SS),
            _ => (r[BX], DS),
        };
        let off = base.wrapping_add(disp);
        self.last_ea = off;
        ModRm { reg, rm: Operand::Mem { seg: self.seg_or(def_seg), off }, ea: off }
    }

    fn read_op<B: Bus>(&self, bus: &mut B, op: Operand, w: bool) -> u16 {
        match (op, w) {
            (Operand::Reg(r), false) => self.reg8(r) as u16,
            (Operand::Reg(r), true) => self.regs[r as usize],
            (Operand::Mem { seg, off }, false) => self.rd8(bus, seg, off) as u16,
            (Operand::Mem { seg, off }, true) => self.rd16(bus, seg, off),
        }
    }

    fn write_op<B: Bus>(&mut self, bus: &mut B, op: Operand, w: bool, v: u16) {
        match (op, w) {
            (Operand::Reg(r), false) => self.set_reg8(r, v as u8),
            (Operand::Reg(r), true) => self.regs[r as usize] = v,
            (Operand::Mem { seg, off }, false) => self.wr8(bus, seg, off, v as u8),
            (Operand::Mem { seg, off }, true) => self.wr16(bus, seg, off, v),
        }
    }

    fn read_reg(&self, r: u8, w: bool) -> u16 {
        if w { self.regs[r as usize] } else { self.reg8(r) as u16 }
    }

    fn write_reg(&mut self, r: u8, w: bool, v: u16) {
        if w {
            self.regs[r as usize] = v;
        } else {
            self.set_reg8(r, v as u8);
        }
    }

    /// Reads a far pointer (offset, segment) from a memory operand.
    fn read_far<B: Bus>(&self, bus: &mut B, op: Operand) -> (u16, u16) {
        let (seg, off) = match op {
            Operand::Mem { seg, off } => (seg, off),
            // Undefined on 8086 with a register operand; use DS:last_ea.
            Operand::Reg(_) => (self.sregs[DS], self.last_ea),
        };
        (self.rd16(bus, seg, off), self.rd16(bus, seg, off.wrapping_add(2)))
    }

    // ---- flags ------------------------------------------------------------

    fn set_szp(&mut self, w: bool, res: u16) {
        let (zero, sign) = if w { (res == 0, res & 0x8000 != 0) } else { (res & 0xFF == 0, res & 0x80 != 0) };
        self.set_flag(ZF, zero);
        self.set_flag(SF, sign);
        self.set_flag(PF, parity(res as u8));
    }

    /// ALU operation `op` (0=ADD 1=OR 2=ADC 3=SBB 4=AND 5=SUB 6=XOR 7=CMP). Returns the result.
    pub fn alu(&mut self, op: u8, a: u16, b: u16, w: bool) -> u16 {
        let (mask, sign) = if w { (0xFFFFu32, 0x8000u32) } else { (0xFF, 0x80) };
        let a = a as u32 & mask;
        let b = b as u32 & mask;
        let res = match op {
            0 | 2 => {
                let c = (op == 2 && self.flag(CF)) as u32;
                let r = a + b + c;
                self.set_flag(CF, r > mask);
                self.set_flag(OF, (a ^ r) & (b ^ r) & sign != 0);
                self.set_flag(AF, (a ^ b ^ r) & 0x10 != 0);
                r & mask
            }
            3 | 5 | 7 => {
                let c = (op == 3 && self.flag(CF)) as u32;
                let r = a.wrapping_sub(b).wrapping_sub(c);
                self.set_flag(CF, b + c > a);
                self.set_flag(OF, (a ^ b) & (a ^ r) & sign != 0);
                self.set_flag(AF, (a ^ b ^ r) & 0x10 != 0);
                r & mask
            }
            _ => {
                let r = match op {
                    1 => a | b,
                    4 => a & b,
                    _ => a ^ b,
                };
                self.flags &= !(CF | OF | AF);
                r
            }
        } as u16;
        self.set_szp(w, res);
        res
    }

    fn inc_dec(&mut self, v: u16, dec: bool, w: bool) -> u16 {
        let cf = self.flag(CF);
        let r = self.alu(if dec { 5 } else { 0 }, v, 1, w);
        self.set_flag(CF, cf);
        r
    }

    /// Shift/rotate group (0=ROL 1=ROR 2=RCL 3=RCR 4=SHL 5=SHR 6=SETMO 7=SAR).
    pub fn shift(&mut self, op: u8, val: u16, count: u8, w: bool) -> u16 {
        if count == 0 {
            return val;
        }
        let bits = if w { 16 } else { 8 };
        let mask: u32 = if w { 0xFFFF } else { 0xFF };
        let msb: u32 = 1 << (bits - 1);
        let mut v = val as u32 & mask;
        let mut cf = self.flag(CF) as u32;
        for _ in 0..count {
            match op {
                0 => {
                    cf = (v & msb != 0) as u32;
                    v = ((v << 1) | cf) & mask;
                }
                1 => {
                    cf = v & 1;
                    v = (v >> 1) | (cf << (bits - 1));
                }
                2 => {
                    let n = (v & msb != 0) as u32;
                    v = ((v << 1) | cf) & mask;
                    cf = n;
                }
                3 => {
                    let n = v & 1;
                    v = (v >> 1) | (cf << (bits - 1));
                    cf = n;
                }
                4 => {
                    cf = (v & msb != 0) as u32;
                    v = (v << 1) & mask;
                }
                6 => {
                    // SETMO/SETMOC (undocumented): result is all ones.
                    v = mask;
                    cf = 0;
                }
                5 => {
                    cf = v & 1;
                    v >>= 1;
                }
                _ => {
                    cf = v & 1;
                    v = (v >> 1) | (v & msb);
                }
            }
        }
        self.set_flag(CF, cf != 0);
        let top = v & msb != 0;
        let of = match op {
            0 | 2 | 4 => top ^ (cf != 0),
            6 => false,
            _ => top ^ (v & (msb >> 1) != 0),
        };
        self.set_flag(OF, of);
        if op >= 4 {
            self.set_szp(w, v as u16);
        }
        v as u16
    }

    fn cond(&self, cc: u8) -> bool {
        let f = |x| self.flag(x);
        let r = match cc >> 1 {
            0 => f(OF),
            1 => f(CF),
            2 => f(ZF),
            3 => f(CF) || f(ZF),
            4 => f(SF),
            5 => f(PF),
            6 => f(SF) != f(OF),
            _ => f(ZF) || (f(SF) != f(OF)),
        };
        r ^ (cc & 1 != 0)
    }

    // ---- interrupts -------------------------------------------------------

    /// Enters interrupt `n`: pushes FLAGS, CS, IP, clears IF/TF and jumps through the IVT.
    pub fn interrupt<B: Bus>(&mut self, bus: &mut B, n: u8) {
        let f = self.flags;
        self.push(bus, f);
        self.flags &= !(IF | TF);
        let cs = self.sregs[CS];
        self.push(bus, cs);
        let ip = self.ip;
        self.push(bus, ip);
        let vec = (n as u16) * 4;
        self.ip = self.rd16(bus, 0, vec);
        self.sregs[CS] = self.rd16(bus, 0, vec + 2);
        self.halted = false;
    }

    /// Executes one instruction (including all its prefixes), or services a pending interrupt.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> StepResult {
        if self.flag(IF)
            && !self.inhibit_irq
            && let Some(v) = bus.take_irq()
        {
            self.interrupt(bus, v);
            return StepResult::Ok;
        }
        if self.halted {
            return StepResult::Halted;
        }
        self.inhibit_irq = false;
        let trap = self.flag(TF);
        let start_ip = self.ip;
        self.seg_override = None;
        self.rep = Rep::None;
        let res = self.exec(bus, start_ip);
        self.instr_count += 1;
        if trap && !self.inhibit_irq && matches!(res, StepResult::Ok) {
            self.interrupt(bus, 1);
        }
        res
    }

    fn exec<B: Bus>(&mut self, bus: &mut B, start_ip: u16) -> StepResult {
        let mut op;
        loop {
            op = self.fetch8(bus);
            match op {
                0x26 => self.seg_override = Some(ES),
                0x2E => self.seg_override = Some(CS),
                0x36 => self.seg_override = Some(SS),
                0x3E => self.seg_override = Some(DS),
                0xF0 | 0xF1 => {} // LOCK
                0xF2 => self.rep = Rep::Repne,
                0xF3 => self.rep = Rep::Repe,
                _ => break,
            }
        }

        if self.model == CpuModel::I80186
            && let Some(r) = self.exec186(bus, op)
        {
            return r;
        }

        match op {
            // ALU ops: ADD OR ADC SBB AND SUB XOR CMP
            0x00..=0x3F if op & 7 < 6 => {
                let aop = op >> 3;
                let w = op & 1 != 0;
                match op & 7 {
                    0 | 1 => {
                        let m = self.modrm(bus);
                        let a = self.read_op(bus, m.rm, w);
                        let b = self.read_reg(m.reg, w);
                        let r = self.alu(aop, a, b, w);
                        if aop != 7 {
                            self.write_op(bus, m.rm, w, r);
                        }
                    }
                    2 | 3 => {
                        let m = self.modrm(bus);
                        let a = self.read_reg(m.reg, w);
                        let b = self.read_op(bus, m.rm, w);
                        let r = self.alu(aop, a, b, w);
                        if aop != 7 {
                            self.write_reg(m.reg, w, r);
                        }
                    }
                    _ => {
                        let b = if w { self.fetch16(bus) } else { self.fetch8(bus) as u16 };
                        let a = self.read_reg(0, w);
                        let r = self.alu(aop, a, b, w);
                        if aop != 7 {
                            self.write_reg(0, w, r);
                        }
                    }
                }
            }
            0x06 | 0x0E | 0x16 | 0x1E => {
                let v = self.sregs[(op >> 3) as usize];
                self.push(bus, v);
            }
            0x0F => {
                // 0F FF nn = host BIOS operation; otherwise 8086 "POP CS".
                let save = self.ip;
                if self.fetch8(bus) == 0xFF {
                    let n = self.fetch8(bus);
                    return StepResult::Bop(n);
                }
                self.ip = save;
                self.sregs[CS] = self.pop(bus);
            }
            0x07 | 0x17 | 0x1F => {
                let v = self.pop(bus);
                self.sregs[(op >> 3) as usize] = v;
                if op == 0x17 {
                    self.inhibit_irq = true;
                }
            }
            0x27 => self.daa(),
            0x2F => self.das(),
            0x37 => self.aaa(),
            0x3F => self.aas(),
            0x40..=0x4F => {
                let r = (op & 7) as usize;
                self.regs[r] = self.inc_dec(self.regs[r], op >= 0x48, true);
            }
            0x50..=0x57 => {
                let r = (op & 7) as usize;
                if r == SP {
                    // 8086 pushes the already decremented SP.
                    self.regs[SP] = self.regs[SP].wrapping_sub(2);
                    let (ss, sp) = (self.sregs[SS], self.regs[SP]);
                    self.wr16(bus, ss, sp, sp);
                } else {
                    let v = self.regs[r];
                    self.push(bus, v);
                }
            }
            0x58..=0x5F => {
                let v = self.pop(bus);
                self.regs[(op & 7) as usize] = v;
            }
            // 60-6F are aliases of 70-7F on the 8086.
            0x60..=0x7F => {
                let d = self.fetch8(bus) as i8 as u16;
                if self.cond(op & 0xF) {
                    self.ip = self.ip.wrapping_add(d);
                }
            }
            0x80..=0x83 => {
                let w = op & 1 != 0;
                let m = self.modrm(bus);
                let b = match op {
                    0x81 => self.fetch16(bus),
                    0x83 => self.fetch8(bus) as i8 as u16,
                    _ => self.fetch8(bus) as u16,
                };
                let a = self.read_op(bus, m.rm, w);
                let r = self.alu(m.reg, a, b, w);
                if m.reg != 7 {
                    self.write_op(bus, m.rm, w, r);
                }
            }
            0x84 | 0x85 => {
                let w = op & 1 != 0;
                let m = self.modrm(bus);
                let a = self.read_op(bus, m.rm, w);
                let b = self.read_reg(m.reg, w);
                self.alu(4, a, b, w);
            }
            0x86 | 0x87 => {
                let w = op & 1 != 0;
                let m = self.modrm(bus);
                let a = self.read_op(bus, m.rm, w);
                let b = self.read_reg(m.reg, w);
                self.write_op(bus, m.rm, w, b);
                self.write_reg(m.reg, w, a);
            }
            0x88..=0x8B => {
                let w = op & 1 != 0;
                let m = self.modrm(bus);
                if op & 2 == 0 {
                    let v = self.read_reg(m.reg, w);
                    self.write_op(bus, m.rm, w, v);
                } else {
                    let v = self.read_op(bus, m.rm, w);
                    self.write_reg(m.reg, w, v);
                }
            }
            0x8C => {
                let m = self.modrm(bus);
                let v = self.sregs[(m.reg & 3) as usize];
                self.write_op(bus, m.rm, true, v);
            }
            0x8D => {
                let m = self.modrm(bus);
                self.regs[m.reg as usize] = m.ea;
            }
            0x8E => {
                let m = self.modrm(bus);
                let v = self.read_op(bus, m.rm, true);
                let s = (m.reg & 3) as usize;
                self.sregs[s] = v;
                if s == SS {
                    self.inhibit_irq = true;
                }
            }
            0x8F => {
                // POP r/m: SP is incremented before the EA is computed.
                let v = self.pop(bus);
                let m = self.modrm(bus);
                self.write_op(bus, m.rm, true, v);
            }
            0x90 => {}
            0x91..=0x97 => self.regs.swap(AX, (op & 7) as usize),
            0x98 => self.regs[AX] = self.regs[AX] as u8 as i8 as i16 as u16,
            0x99 => {
                self.regs[DX] = if self.regs[AX] & 0x8000 != 0 { 0xFFFF } else { 0 };
            }
            0x9A => {
                let off = self.fetch16(bus);
                let seg = self.fetch16(bus);
                let cs = self.sregs[CS];
                self.push(bus, cs);
                let ip = self.ip;
                self.push(bus, ip);
                self.sregs[CS] = seg;
                self.ip = off;
            }
            0x9B => {} // WAIT
            0x9C => {
                let f = self.flags;
                self.push(bus, f);
            }
            0x9D => {
                let v = self.pop(bus);
                self.set_flags_word(v);
            }
            0x9E => {
                let ah = self.reg8(4) as u16;
                self.flags = (self.flags & 0xFF00) | (ah & (SF | ZF | AF | PF | CF)) | 0x02;
            }
            0x9F => {
                let f = (self.flags & 0xFF) as u8;
                self.set_reg8(4, f);
            }
            0xA0..=0xA3 => {
                let w = op & 1 != 0;
                let off = self.fetch16(bus);
                let seg = self.seg_or(DS);
                let mem = Operand::Mem { seg, off };
                if op & 2 == 0 {
                    let v = self.read_op(bus, mem, w);
                    self.write_reg(0, w, v);
                } else {
                    let v = self.read_reg(0, w);
                    self.write_op(bus, mem, w, v);
                }
            }
            0xA4..=0xA7 | 0xAA..=0xAF => self.string_op(bus, op),
            0xA8 => {
                let b = self.fetch8(bus) as u16;
                let a = self.reg8(0) as u16;
                self.alu(4, a, b, false);
            }
            0xA9 => {
                let b = self.fetch16(bus);
                self.alu(4, self.regs[AX], b, true);
            }
            0xB0..=0xB7 => {
                let v = self.fetch8(bus);
                self.set_reg8(op & 7, v);
            }
            0xB8..=0xBF => {
                let v = self.fetch16(bus);
                self.regs[(op & 7) as usize] = v;
            }
            0xC0 | 0xC2 => {
                let n = self.fetch16(bus);
                self.ip = self.pop(bus);
                self.regs[SP] = self.regs[SP].wrapping_add(n);
            }
            0xC1 | 0xC3 => self.ip = self.pop(bus),
            0xC4 | 0xC5 => {
                let m = self.modrm(bus);
                let (off, seg) = self.read_far(bus, m.rm);
                self.regs[m.reg as usize] = off;
                self.sregs[if op == 0xC4 { ES } else { DS }] = seg;
            }
            0xC6 | 0xC7 => {
                let w = op & 1 != 0;
                let m = self.modrm(bus);
                let v = if w { self.fetch16(bus) } else { self.fetch8(bus) as u16 };
                self.write_op(bus, m.rm, w, v);
            }
            0xC8 | 0xCA => {
                let n = self.fetch16(bus);
                self.ip = self.pop(bus);
                self.sregs[CS] = self.pop(bus);
                self.regs[SP] = self.regs[SP].wrapping_add(n);
            }
            0xC9 | 0xCB => {
                self.ip = self.pop(bus);
                self.sregs[CS] = self.pop(bus);
            }
            0xCC => self.interrupt(bus, 3),
            0xCD => {
                let n = self.fetch8(bus);
                self.interrupt(bus, n);
            }
            0xCE => {
                if self.flag(OF) {
                    self.interrupt(bus, 4);
                }
            }
            0xCF => {
                self.ip = self.pop(bus);
                self.sregs[CS] = self.pop(bus);
                let f = self.pop(bus);
                self.set_flags_word(f);
            }
            0xD0..=0xD3 => {
                let w = op & 1 != 0;
                let m = self.modrm(bus);
                let count = if op & 2 != 0 { self.reg8(1) } else { 1 };
                let v = self.read_op(bus, m.rm, w);
                let r = self.shift(m.reg, v, count, w);
                self.write_op(bus, m.rm, w, r);
            }
            0xD4 => {
                let base = self.fetch8(bus);
                let al = self.reg8(0);
                match (al.checked_div(base), al.checked_rem(base)) {
                    (Some(q), Some(r)) => {
                        self.set_reg8(4, q);
                        self.set_reg8(0, r);
                        self.set_szp(false, r as u16);
                    }
                    _ => {
                        // The 8086 updates SZP as for a zero result before raising the exception.
                        self.set_szp(false, 0);
                        self.interrupt(bus, 0);
                    }
                }
            }
            0xD5 => {
                let base = self.fetch8(bus);
                let al = self.reg8(0).wrapping_add(self.reg8(4).wrapping_mul(base));
                self.regs[AX] = al as u16;
                self.set_szp(false, al as u16);
            }
            0xD6 => {
                // SALC (undocumented)
                let v = if self.flag(CF) { 0xFF } else { 0 };
                self.set_reg8(0, v);
            }
            0xD7 => {
                let seg = self.seg_or(DS);
                let off = self.regs[BX].wrapping_add(self.reg8(0) as u16);
                let v = self.rd8(bus, seg, off);
                self.set_reg8(0, v);
            }
            0xD8..=0xDF => {
                // ESC (coprocessor) - decode operand and ignore.
                self.modrm(bus);
            }
            0xE0..=0xE2 => {
                let d = self.fetch8(bus) as i8 as u16;
                self.regs[CX] = self.regs[CX].wrapping_sub(1);
                let take = self.regs[CX] != 0
                    && match op {
                        0xE0 => !self.flag(ZF),
                        0xE1 => self.flag(ZF),
                        _ => true,
                    };
                if take {
                    self.ip = self.ip.wrapping_add(d);
                }
            }
            0xE3 => {
                let d = self.fetch8(bus) as i8 as u16;
                if self.regs[CX] == 0 {
                    self.ip = self.ip.wrapping_add(d);
                }
            }
            0xE4 | 0xE5 | 0xEC | 0xED => {
                let port = if op & 8 != 0 { self.regs[DX] } else { self.fetch8(bus) as u16 };
                if op & 1 != 0 {
                    self.regs[AX] = bus.port_in16(port);
                } else {
                    let v = bus.port_in8(port);
                    self.set_reg8(0, v);
                }
            }
            0xE6 | 0xE7 | 0xEE | 0xEF => {
                let port = if op & 8 != 0 { self.regs[DX] } else { self.fetch8(bus) as u16 };
                if op & 1 != 0 {
                    bus.port_out16(port, self.regs[AX]);
                } else {
                    bus.port_out8(port, self.reg8(0));
                }
            }
            0xE8 => {
                let d = self.fetch16(bus);
                let ip = self.ip;
                self.push(bus, ip);
                self.ip = self.ip.wrapping_add(d);
            }
            0xE9 => {
                let d = self.fetch16(bus);
                self.ip = self.ip.wrapping_add(d);
            }
            0xEA => {
                let off = self.fetch16(bus);
                let seg = self.fetch16(bus);
                self.ip = off;
                self.sregs[CS] = seg;
            }
            0xEB => {
                let d = self.fetch8(bus) as i8 as u16;
                self.ip = self.ip.wrapping_add(d);
            }
            0xF4 => {
                self.halted = true;
                return StepResult::Halted;
            }
            0xF5 => self.flags ^= CF,
            0xF6 | 0xF7 => self.group3(bus, op & 1 != 0, start_ip),
            0xF8 => self.set_flag(CF, false),
            0xF9 => self.set_flag(CF, true),
            0xFA => self.set_flag(IF, false),
            0xFB => {
                self.set_flag(IF, true);
                self.inhibit_irq = true;
            }
            0xFC => self.set_flag(DF, false),
            0xFD => self.set_flag(DF, true),
            0xFE | 0xFF => self.group45(bus, op == 0xFF),
            // Prefixes are consumed above; every other byte value is covered.
            _ => unreachable!("opcode {op:02X}"),
        }
        StepResult::Ok
    }

    /// 80186 instructions. Returns `None` for opcodes that behave like on the 8086.
    fn exec186<B: Bus>(&mut self, bus: &mut B, op: u8) -> Option<StepResult> {
        match op {
            0x60 => {
                // PUSHA
                let sp = self.regs[SP];
                for r in [AX, CX, DX, BX] {
                    let v = self.regs[r];
                    self.push(bus, v);
                }
                self.push(bus, sp);
                for r in [BP, SI, DI] {
                    let v = self.regs[r];
                    self.push(bus, v);
                }
            }
            0x61 => {
                // POPA (the saved SP is discarded)
                for r in [DI, SI, BP] {
                    self.regs[r] = self.pop(bus);
                }
                self.pop(bus);
                for r in [BX, DX, CX, AX] {
                    self.regs[r] = self.pop(bus);
                }
            }
            0x62 => {
                // BOUND r16, m16&16
                let m = self.modrm(bus);
                let (lo, hi) = self.read_far(bus, m.rm);
                let v = self.regs[m.reg as usize] as i16;
                if v < lo as i16 || v > hi as i16 {
                    self.interrupt(bus, 5);
                }
            }
            0x68 => {
                let v = self.fetch16(bus);
                self.push(bus, v);
            }
            0x6A => {
                let v = self.fetch8(bus) as i8 as u16;
                self.push(bus, v);
            }
            0x69 | 0x6B => {
                // IMUL r16, r/m16, imm
                let m = self.modrm(bus);
                let a = self.read_op(bus, m.rm, true) as i16 as i32;
                let b = if op == 0x69 { self.fetch16(bus) as i16 as i32 } else { self.fetch8(bus) as i8 as i32 };
                let r = a * b;
                self.regs[m.reg as usize] = r as u16;
                let ovf = r != (r as i16) as i32;
                self.set_flag(CF, ovf);
                self.set_flag(OF, ovf);
            }
            0x6C..=0x6F => self.string_io(bus, op),
            0xC0 | 0xC1 => {
                let w = op & 1 != 0;
                let m = self.modrm(bus);
                let count = self.fetch8(bus) & 0x1F;
                let v = self.read_op(bus, m.rm, w);
                let r = self.shift(m.reg, v, count, w);
                self.write_op(bus, m.rm, w, r);
            }
            0xC8 => {
                // ENTER size, level
                let size = self.fetch16(bus);
                let level = self.fetch8(bus) & 0x1F;
                let bp = self.regs[BP];
                self.push(bus, bp);
                let frame = self.regs[SP];
                for _ in 1..level {
                    self.regs[BP] = self.regs[BP].wrapping_sub(2);
                    let v = self.rd16(bus, self.sregs[SS], self.regs[BP]);
                    self.push(bus, v);
                }
                if level > 0 {
                    self.push(bus, frame);
                }
                self.regs[BP] = frame;
                self.regs[SP] = self.regs[SP].wrapping_sub(size);
            }
            0xC9 => {
                // LEAVE
                self.regs[SP] = self.regs[BP];
                self.regs[BP] = self.pop(bus);
            }
            _ => return None,
        }
        Some(StepResult::Ok)
    }

    /// INSB/INSW/OUTSB/OUTSW with optional REP.
    fn string_io<B: Bus>(&mut self, bus: &mut B, op: u8) {
        let w = op & 1 != 0;
        let step: u16 = if w { 2 } else { 1 };
        let delta = if self.flag(DF) { step.wrapping_neg() } else { step };
        let src_seg = self.seg_or(DS);
        let rep = self.rep != Rep::None;
        loop {
            if rep && self.regs[CX] == 0 {
                break;
            }
            let port = self.regs[DX];
            if op < 0x6E {
                let v = if w { bus.port_in16(port) } else { bus.port_in8(port) as u16 };
                let es = self.sregs[ES];
                self.write_op(bus, Operand::Mem { seg: es, off: self.regs[DI] }, w, v);
                self.regs[DI] = self.regs[DI].wrapping_add(delta);
            } else {
                let v = self.read_op(bus, Operand::Mem { seg: src_seg, off: self.regs[SI] }, w);
                if w {
                    bus.port_out16(port, v);
                } else {
                    bus.port_out8(port, v as u8);
                }
                self.regs[SI] = self.regs[SI].wrapping_add(delta);
            }
            if !rep {
                break;
            }
            self.regs[CX] = self.regs[CX].wrapping_sub(1);
        }
    }

    fn group3<B: Bus>(&mut self, bus: &mut B, w: bool, _start_ip: u16) {
        let m = self.modrm(bus);
        let v = self.read_op(bus, m.rm, w);
        match m.reg {
            0 | 1 => {
                let imm = if w { self.fetch16(bus) } else { self.fetch8(bus) as u16 };
                self.alu(4, v, imm, w);
            }
            2 => self.write_op(bus, m.rm, w, !v),
            3 => {
                let r = self.alu(5, 0, v, w);
                self.set_flag(CF, v & if w { 0xFFFF } else { 0xFF } != 0);
                self.write_op(bus, m.rm, w, r);
            }
            4 => {
                if w {
                    let r = self.regs[AX] as u32 * v as u32;
                    self.regs[AX] = r as u16;
                    self.regs[DX] = (r >> 16) as u16;
                    self.set_flag(CF, r >> 16 != 0);
                    self.set_flag(OF, r >> 16 != 0);
                    self.set_flag(ZF, r == 0);
                } else {
                    let r = self.reg8(0) as u16 * v;
                    self.regs[AX] = r;
                    self.set_flag(CF, r >> 8 != 0);
                    self.set_flag(OF, r >> 8 != 0);
                    self.set_flag(ZF, r == 0);
                }
            }
            5 => {
                if w {
                    let r = (self.regs[AX] as i16 as i32) * (v as i16 as i32);
                    self.regs[AX] = r as u16;
                    self.regs[DX] = (r >> 16) as u16;
                    let ovf = r != (r as i16) as i32;
                    self.set_flag(CF, ovf);
                    self.set_flag(OF, ovf);
                    self.set_flag(ZF, r == 0);
                } else {
                    let r = (self.reg8(0) as i8 as i16) * (v as u8 as i8 as i16);
                    self.regs[AX] = r as u16;
                    let ovf = r != (r as i8) as i16;
                    self.set_flag(CF, ovf);
                    self.set_flag(OF, ovf);
                    self.set_flag(ZF, r == 0);
                }
            }
            6 => {
                if v == 0 {
                    return self.interrupt(bus, 0);
                }
                if w {
                    let n = ((self.regs[DX] as u32) << 16) | self.regs[AX] as u32;
                    let q = n / v as u32;
                    if q > 0xFFFF {
                        return self.interrupt(bus, 0);
                    }
                    self.regs[AX] = q as u16;
                    self.regs[DX] = (n % v as u32) as u16;
                } else {
                    let n = self.regs[AX];
                    let q = n / v;
                    if q > 0xFF {
                        return self.interrupt(bus, 0);
                    }
                    self.set_reg8(0, q as u8);
                    self.set_reg8(4, (n % v) as u8);
                }
            }
            _ => {
                if (w && v == 0) || (!w && v & 0xFF == 0) {
                    return self.interrupt(bus, 0);
                }
                // 8086 quirk: a REP prefix negates the quotient.
                let neg = self.rep != Rep::None;
                if w {
                    let n = (((self.regs[DX] as u32) << 16) | self.regs[AX] as u32) as i32 as i64;
                    let d = v as i16 as i64;
                    let q = if neg { -(n / d) } else { n / d };
                    if !(-32767..=32767).contains(&q) {
                        return self.interrupt(bus, 0);
                    }
                    self.regs[AX] = q as u16;
                    self.regs[DX] = (n % d) as u16;
                } else {
                    let n = self.regs[AX] as i16 as i32;
                    let d = v as u8 as i8 as i32;
                    let q = if neg { -(n / d) } else { n / d };
                    if !(-127..=127).contains(&q) {
                        return self.interrupt(bus, 0);
                    }
                    self.set_reg8(0, q as u8);
                    self.set_reg8(4, (n % d) as u8);
                }
            }
        }
    }

    fn group45<B: Bus>(&mut self, bus: &mut B, w: bool) {
        let m = self.modrm(bus);
        match m.reg {
            0 | 1 => {
                let v = self.read_op(bus, m.rm, w);
                let r = self.inc_dec(v, m.reg == 1, w);
                self.write_op(bus, m.rm, w, r);
            }
            // The rest is only defined for FF.
            2 => {
                let t = self.read_op(bus, m.rm, true);
                let ip = self.ip;
                self.push(bus, ip);
                self.ip = t;
            }
            3 => {
                let (off, seg) = self.read_far(bus, m.rm);
                let cs = self.sregs[CS];
                self.push(bus, cs);
                let ip = self.ip;
                self.push(bus, ip);
                self.sregs[CS] = seg;
                self.ip = off;
            }
            4 => self.ip = self.read_op(bus, m.rm, true),
            5 => {
                let (off, seg) = self.read_far(bus, m.rm);
                self.sregs[CS] = seg;
                self.ip = off;
            }
            _ => {
                let v = self.read_op(bus, m.rm, true);
                if let Operand::Reg(r) = m.rm
                    && r as usize == SP
                {
                    self.regs[SP] = self.regs[SP].wrapping_sub(2);
                    let (ss, sp) = (self.sregs[SS], self.regs[SP]);
                    self.wr16(bus, ss, sp, sp);
                } else {
                    self.push(bus, v);
                }
            }
        }
    }

    fn string_op<B: Bus>(&mut self, bus: &mut B, op: u8) {
        let w = op & 1 != 0;
        let delta: u16 = match (w, self.flag(DF)) {
            (false, false) => 1,
            (false, true) => 0xFFFF,
            (true, false) => 2,
            (true, true) => 0xFFFE,
        };
        let src_seg = self.seg_or(DS);
        let compares = matches!(op, 0xA6 | 0xA7 | 0xAE | 0xAF);
        let rep = self.rep;
        loop {
            if rep != Rep::None && self.regs[CX] == 0 {
                break;
            }
            match op {
                0xA4 | 0xA5 => {
                    let v = self.read_op(bus, Operand::Mem { seg: src_seg, off: self.regs[SI] }, w);
                    let es = self.sregs[ES];
                    self.write_op(bus, Operand::Mem { seg: es, off: self.regs[DI] }, w, v);
                    self.regs[SI] = self.regs[SI].wrapping_add(delta);
                    self.regs[DI] = self.regs[DI].wrapping_add(delta);
                }
                0xA6 | 0xA7 => {
                    let a = self.read_op(bus, Operand::Mem { seg: src_seg, off: self.regs[SI] }, w);
                    let es = self.sregs[ES];
                    let b = self.read_op(bus, Operand::Mem { seg: es, off: self.regs[DI] }, w);
                    self.alu(7, a, b, w);
                    self.regs[SI] = self.regs[SI].wrapping_add(delta);
                    self.regs[DI] = self.regs[DI].wrapping_add(delta);
                }
                0xAA | 0xAB => {
                    let v = self.read_reg(0, w);
                    let es = self.sregs[ES];
                    self.write_op(bus, Operand::Mem { seg: es, off: self.regs[DI] }, w, v);
                    self.regs[DI] = self.regs[DI].wrapping_add(delta);
                }
                0xAC | 0xAD => {
                    let v = self.read_op(bus, Operand::Mem { seg: src_seg, off: self.regs[SI] }, w);
                    self.write_reg(0, w, v);
                    self.regs[SI] = self.regs[SI].wrapping_add(delta);
                }
                _ => {
                    let a = self.read_reg(0, w);
                    let es = self.sregs[ES];
                    let b = self.read_op(bus, Operand::Mem { seg: es, off: self.regs[DI] }, w);
                    self.alu(7, a, b, w);
                    self.regs[DI] = self.regs[DI].wrapping_add(delta);
                }
            }
            if rep == Rep::None {
                break;
            }
            self.regs[CX] = self.regs[CX].wrapping_sub(1);
            if compares {
                let z = self.flag(ZF);
                if (rep == Rep::Repe && !z) || (rep == Rep::Repne && z) {
                    break;
                }
            }
        }
    }

    // ---- BCD --------------------------------------------------------------

    fn daa(&mut self) {
        let old_al = self.reg8(0);
        let old_cf = self.flag(CF);
        // 8086 quirk: with AF set the high-digit adjust threshold is 9Fh, not 99h.
        let limit = if self.flag(AF) { 0x9F } else { 0x99 };
        let mut al = old_al;
        if al & 0x0F > 9 || self.flag(AF) {
            let (r, c) = al.overflowing_add(6);
            al = r;
            self.set_flag(CF, old_cf || c);
            self.set_flag(AF, true);
        } else {
            self.set_flag(AF, false);
        }
        if old_al > limit || old_cf {
            al = al.wrapping_add(0x60);
            self.set_flag(CF, true);
        } else {
            self.set_flag(CF, false);
        }
        self.set_reg8(0, al);
        self.set_szp(false, al as u16);
    }

    fn das(&mut self) {
        let old_al = self.reg8(0);
        let old_cf = self.flag(CF);
        // 8086 quirk: with AF set the high-digit adjust threshold is 9Fh, not 99h.
        let limit = if self.flag(AF) { 0x9F } else { 0x99 };
        let mut al = old_al;
        if al & 0x0F > 9 || self.flag(AF) {
            let (r, b) = al.overflowing_sub(6);
            al = r;
            self.set_flag(CF, old_cf || b);
            self.set_flag(AF, true);
        } else {
            self.set_flag(AF, false);
        }
        if old_al > limit || old_cf {
            al = al.wrapping_sub(0x60);
            self.set_flag(CF, true);
        } else {
            self.set_flag(CF, false);
        }
        self.set_reg8(0, al);
        self.set_szp(false, al as u16);
    }

    fn aaa(&mut self) {
        let al = self.reg8(0);
        if al & 0x0F > 9 || self.flag(AF) {
            self.set_reg8(0, al.wrapping_add(6));
            self.set_reg8(4, self.reg8(4).wrapping_add(1));
            self.set_flag(AF, true);
            self.set_flag(CF, true);
        } else {
            self.set_flag(AF, false);
            self.set_flag(CF, false);
        }
        self.set_reg8(0, self.reg8(0) & 0x0F);
    }

    fn aas(&mut self) {
        let al = self.reg8(0);
        if al & 0x0F > 9 || self.flag(AF) {
            self.set_reg8(0, al.wrapping_sub(6));
            self.set_reg8(4, self.reg8(4).wrapping_sub(1));
            self.set_flag(AF, true);
            self.set_flag(CF, true);
        } else {
            self.set_flag(AF, false);
            self.set_flag(CF, false);
        }
        self.set_reg8(0, self.reg8(0) & 0x0F);
    }
}
