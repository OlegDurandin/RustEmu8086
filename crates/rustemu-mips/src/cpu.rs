//! The MIPS32 processor as MARS simulates it: the integer instruction set of MARS (77 basic
//! instructions: arithmetic with and without overflow, HI/LO multiply and divide, shifts, logic,
//! loads and stores including `lwl`/`lwr`/`swl`/`swr` and `ll`/`sc`, branches, jumps, traps,
//! `syscall`, `break`, coprocessor 0 and `eret`), without branch delay slots (MARS's default).
//!
//! Exceptions follow MARS: Cause gets the exception code, EPC the address of the instruction,
//! BadVAddr the refused address, Status its exception level bit; when the exception handler
//! address holds code, execution continues there, otherwise the program stops with MARS's
//! message (`Runtime exception at 0x00400008: arithmetic overflow`).
//!
//! Coprocessor 1 (floating point) is not emulated yet: its instructions raise a reserved
//! instruction exception.

use crate::memory::{self, Access, AccessError, Memory, hex};

/// Register names in number order (`$0` is `$zero`).
pub const REG_NAMES: [&str; 32] = [
    "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "s0", "s1", "s2",
    "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp", "fp", "ra",
];

/// Number of the return address register `$ra`.
pub const RA: usize = 31;
/// Number of the system call number / result register `$v0`.
pub const V0: usize = 2;
/// Number of the global pointer `$gp`.
pub const GP: usize = 28;
/// Number of the stack pointer `$sp`.
pub const SP: usize = 29;

/// Coprocessor 0 register numbers MARS has.
pub mod cp0 {
    /// BadVAddr: the address of the last address exception.
    pub const VADDR: usize = 8;
    /// Status: interrupt mask, exception level (bit 1), interrupt enable (bit 0).
    pub const STATUS: usize = 12;
    /// Cause: the exception code in bits 2..6.
    pub const CAUSE: usize = 13;
    /// EPC: the address of the instruction that caused the exception.
    pub const EPC: usize = 14;
    /// Status after reset in MARS.
    pub const STATUS_RESET: u32 = 0x0000_ff11;
    /// The exception level bit of Status.
    pub const EXL: u32 = 1 << 1;
}

/// Exception codes (the Cause register's ExcCode field), as MARS numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcCode {
    /// Address error on a load or an instruction fetch.
    AddressLoad = 4,
    /// Address error on a store.
    AddressStore = 5,
    /// `syscall` (MARS performs the service instead of raising it).
    Syscall = 8,
    /// `break`.
    Breakpoint = 9,
    /// An instruction MARS does not know.
    ReservedInstruction = 10,
    /// Signed overflow in `add`, `addi`, `sub`.
    Overflow = 12,
    /// A trap instruction whose condition holds.
    Trap = 13,
}

/// An exception raised by an instruction (see [`Cpu::step`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    /// What happened.
    pub code: ExcCode,
    /// MARS's description: `arithmetic overflow`, `address out of range 0x00000000`.
    pub message: String,
    /// Address of the instruction (for an instruction fetch: the program counter).
    pub pc: u32,
    /// The refused address of an address exception.
    pub bad_vaddr: Option<u32>,
    /// The instruction could not be fetched (an invalid program counter).
    pub fetch: bool,
    /// The exception handler took it: execution goes on there.
    pub handled: bool,
}

impl Exception {
    /// The error as MARS reports a program it stops: `Runtime exception at 0x00400008:
    /// arithmetic overflow`, or `invalid program counter value: 0x00000003` for a fetch.
    pub fn report(&self) -> String {
        if self.fetch {
            format!("invalid program counter value: {}", hex(self.pc))
        } else {
            format!("Runtime exception at {}: {}", hex(self.pc), self.message)
        }
    }
}

/// Result of [`Cpu::step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The instruction executed.
    Ok,
    /// `syscall` executed: the service number is in `$v0`, the program counter is past the
    /// instruction. The machine performs the service.
    Syscall,
    /// No instruction at the program counter: the program ran past its last instruction, which
    /// MARS treats as the normal end of the program.
    DroppedOff,
    /// The instruction raised an exception (see [`Exception::handled`]).
    Exception(Exception),
}

/// The processor state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    /// General-purpose registers `$0`..`$31` (`$0` is always 0).
    pub regs: [u32; 32],
    /// HI: high word of a product, remainder of a division.
    pub hi: u32,
    /// LO: low word of a product, quotient of a division.
    pub lo: u32,
    /// Address of the next instruction.
    pub pc: u32,
    /// Coprocessor 0 registers by number (MARS has 8, 12, 13 and 14; the others read as 0).
    pub cp0: [u32; 32],
    /// Instructions executed since the reset.
    pub instr_count: u64,
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu::new()
    }
}

/// The parts of an instruction word.
#[derive(Clone, Copy)]
struct Fields {
    word: u32,
    rs: usize,
    rt: usize,
    rd: usize,
    shamt: u32,
    /// The 16-bit immediate, sign-extended.
    simm: u32,
    /// The 16-bit immediate, zero-extended.
    uimm: u32,
}

impl Fields {
    fn new(word: u32) -> Fields {
        Fields {
            word,
            rs: (word >> 21 & 31) as usize,
            rt: (word >> 16 & 31) as usize,
            rd: (word >> 11 & 31) as usize,
            shamt: word >> 6 & 31,
            simm: word as i16 as i32 as u32,
            uimm: word & 0xFFFF,
        }
    }
}

/// Why an instruction stopped: an exception before the CPU state is touched.
type Fault = (ExcCode, String, Option<u32>);

fn access(e: AccessError) -> Fault {
    let code = match e.access {
        Access::Load => ExcCode::AddressLoad,
        Access::Store => ExcCode::AddressStore,
    };
    (code, e.message, Some(e.addr))
}

fn overflow() -> Fault {
    (ExcCode::Overflow, "arithmetic overflow".into(), None)
}

fn trap(cond: bool) -> Result<(), Fault> {
    if cond { Err((ExcCode::Trap, "trap".into(), None)) } else { Ok(()) }
}

fn reserved(word: u32) -> Fault {
    (ExcCode::ReservedInstruction, format!("undefined instruction ({})", hex(word)), None)
}

impl Cpu {
    /// The processor after a reset in MARS: registers 0, `$gp` and `$sp` at their initial
    /// values, the program counter at the text segment, Status 0x0000FF11.
    pub fn new() -> Cpu {
        let mut regs = [0; 32];
        regs[GP] = memory::GP_INIT;
        regs[SP] = memory::SP_INIT;
        let mut cp0 = [0; 32];
        cp0[cp0::STATUS] = cp0::STATUS_RESET;
        Cpu { regs, hi: 0, lo: 0, pc: memory::TEXT_BASE, cp0, instr_count: 0 }
    }

    /// Register `r` (`$0` reads as 0).
    #[inline]
    pub fn reg(&self, r: usize) -> u32 {
        self.regs[r]
    }

    /// Sets register `r` (writes to `$0` are ignored).
    #[inline]
    pub fn set_reg(&mut self, r: usize, v: u32) {
        if r != 0 {
            self.regs[r] = v;
        }
    }

    /// Executes the instruction at the program counter.
    pub fn step(&mut self, mem: &mut Memory) -> Step {
        let pc = self.pc;
        let word = match mem.fetch(pc) {
            Ok(Some(w)) => w,
            Ok(None) => return Step::DroppedOff,
            Err(e) => {
                let (code, message, bad) = access(e);
                return Step::Exception(self.raise(mem, code, message, pc, bad, true));
            }
        };
        self.instr_count += 1;
        self.pc = pc.wrapping_add(4);
        match self.execute(mem, word) {
            Ok(step) => step,
            Err((code, message, bad)) => Step::Exception(self.raise(mem, code, message, pc, bad, false)),
        }
    }

    /// Records an exception in coprocessor 0 (as MARS does whether or not there is a handler)
    /// and goes to the exception handler when it holds code; otherwise the program counter stays
    /// at the instruction.
    fn raise(
        &mut self,
        mem: &Memory,
        code: ExcCode,
        message: String,
        pc: u32,
        bad: Option<u32>,
        fetch: bool,
    ) -> Exception {
        if let Some(a) = bad {
            self.cp0[cp0::VADDR] = a;
        }
        self.cp0[cp0::CAUSE] = (self.cp0[cp0::CAUSE] & 0xFFFF_FC83) | ((code as u32) << 2);
        self.cp0[cp0::EPC] = pc;
        self.cp0[cp0::STATUS] |= cp0::EXL;
        let handled = mem.has_code(memory::EXCEPTION_HANDLER);
        self.pc = if handled { memory::EXCEPTION_HANDLER } else { pc };
        Exception { code, message, pc, bad_vaddr: bad, fetch, handled }
    }

    /// Branch to `offset` instructions from the next one.
    fn branch(&mut self, f: &Fields, cond: bool) {
        if cond {
            self.pc = self.pc.wrapping_add(f.simm << 2);
        }
    }

    /// Branch and link: MARS links only when the branch is taken.
    fn branch_link(&mut self, f: &Fields, cond: bool) {
        if cond {
            self.set_reg(RA, self.pc);
            self.branch(f, true);
        }
    }

    fn execute(&mut self, mem: &mut Memory, word: u32) -> Result<Step, Fault> {
        let f = Fields::new(word);
        let (rs, rt) = (self.reg(f.rs), self.reg(f.rt));
        let addr = rs.wrapping_add(f.simm);
        match word >> 26 {
            0x00 => return self.special(mem, &f),
            0x01 => match f.rt {
                0x00 => self.branch(&f, (rs as i32) < 0),
                0x01 => self.branch(&f, (rs as i32) >= 0),
                0x10 => self.branch_link(&f, (rs as i32) < 0),
                0x11 => self.branch_link(&f, (rs as i32) >= 0),
                0x08 => trap(rs as i32 >= f.simm as i32)?,
                0x09 => trap(rs >= f.simm)?,
                0x0A => trap((rs as i32) < f.simm as i32)?,
                0x0B => trap(rs < f.simm)?,
                0x0C => trap(rs == f.simm)?,
                0x0E => trap(rs != f.simm)?,
                _ => return Err(reserved(word)),
            },
            0x02 => self.pc = (self.pc & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2),
            0x03 => {
                self.set_reg(RA, self.pc);
                self.pc = (self.pc & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2);
            }
            0x04 => self.branch(&f, rs == rt),
            0x05 => self.branch(&f, rs != rt),
            0x06 => self.branch(&f, rs as i32 <= 0),
            0x07 => self.branch(&f, rs as i32 > 0),
            0x08 => {
                let v = (rs as i32).checked_add(f.simm as i32).ok_or_else(overflow)?;
                self.set_reg(f.rt, v as u32);
            }
            0x09 => self.set_reg(f.rt, rs.wrapping_add(f.simm)),
            0x0A => self.set_reg(f.rt, ((rs as i32) < f.simm as i32) as u32),
            0x0B => self.set_reg(f.rt, (rs < f.simm) as u32),
            0x0C => self.set_reg(f.rt, rs & f.uimm),
            0x0D => self.set_reg(f.rt, rs | f.uimm),
            0x0E => self.set_reg(f.rt, rs ^ f.uimm),
            0x0F => self.set_reg(f.rt, f.uimm << 16),
            0x10 => return self.cop0(&f),
            0x1C => return self.special2(&f),
            0x20 => self.set_reg(f.rt, mem.load8(addr).map_err(access)? as i8 as i32 as u32),
            0x21 => self.set_reg(f.rt, mem.load16(addr).map_err(access)? as i16 as i32 as u32),
            0x22 => {
                // lwl: from the addressed byte down to the start of its word, into the high
                // bytes of rt (little-endian).
                let mut v = rt;
                for i in 0..=addr % 4 {
                    let b = mem.load8(addr.wrapping_sub(i)).map_err(access)? as u32;
                    let shift = 8 * (3 - i);
                    v = (v & !(0xFF << shift)) | (b << shift);
                }
                self.set_reg(f.rt, v);
            }
            0x23 | 0x30 => self.set_reg(f.rt, mem.load32(addr).map_err(access)?),
            0x24 => self.set_reg(f.rt, mem.load8(addr).map_err(access)? as u32),
            0x25 => self.set_reg(f.rt, mem.load16(addr).map_err(access)? as u32),
            0x26 => {
                // lwr: from the addressed byte up to the end of its word, into the low bytes.
                let mut v = rt;
                for i in 0..=3 - addr % 4 {
                    let b = mem.load8(addr.wrapping_add(i)).map_err(access)? as u32;
                    v = (v & !(0xFF << (8 * i))) | (b << (8 * i));
                }
                self.set_reg(f.rt, v);
            }
            0x28 => mem.store8(addr, rt as u8).map_err(access)?,
            0x29 => mem.store16(addr, rt as u16).map_err(access)?,
            0x2A => {
                // swl: the high bytes of rt, from the addressed byte down to the start of its word.
                check_bytes(mem, Access::Store, (0..=addr % 4).map(|i| addr.wrapping_sub(i)))?;
                for i in 0..=addr % 4 {
                    mem.store8(addr.wrapping_sub(i), (rt >> (8 * (3 - i))) as u8).map_err(access)?;
                }
            }
            0x2B => mem.store32(addr, rt).map_err(access)?,
            0x2E => {
                // swr: the low bytes of rt, from the addressed byte up to the end of its word.
                check_bytes(mem, Access::Store, (0..=3 - addr % 4).map(|i| addr.wrapping_add(i)))?;
                for i in 0..=3 - addr % 4 {
                    mem.store8(addr.wrapping_add(i), (rt >> (8 * i)) as u8).map_err(access)?;
                }
            }
            0x38 => {
                // sc: MARS simulates one processor, so the store always succeeds.
                mem.store32(addr, rt).map_err(access)?;
                self.set_reg(f.rt, 1);
            }
            0x11 | 0x31 | 0x35 | 0x39 | 0x3D => {
                return Err((
                    ExcCode::ReservedInstruction,
                    format!("floating-point instructions are not supported yet ({})", hex(word)),
                    None,
                ));
            }
            _ => return Err(reserved(word)),
        }
        Ok(Step::Ok)
    }

    /// Opcode 0: register-register operations, shifts, jumps through registers, HI/LO, traps,
    /// `syscall` and `break`.
    fn special(&mut self, _mem: &mut Memory, f: &Fields) -> Result<Step, Fault> {
        let (rs, rt) = (self.reg(f.rs), self.reg(f.rt));
        match f.word & 0x3F {
            0x00 => self.set_reg(f.rd, rt << f.shamt),
            0x02 => self.set_reg(f.rd, rt >> f.shamt),
            0x03 => self.set_reg(f.rd, ((rt as i32) >> f.shamt) as u32),
            0x04 => self.set_reg(f.rd, rt << (rs & 31)),
            0x06 => self.set_reg(f.rd, rt >> (rs & 31)),
            0x07 => self.set_reg(f.rd, ((rt as i32) >> (rs & 31)) as u32),
            0x08 => self.pc = rs,
            0x09 => {
                self.set_reg(f.rd, self.pc);
                self.pc = rs;
            }
            0x0A => {
                if rt == 0 {
                    self.set_reg(f.rd, rs);
                }
            }
            0x0B => {
                if rt != 0 {
                    self.set_reg(f.rd, rs);
                }
            }
            0x0C => return Ok(Step::Syscall),
            0x0D => {
                let code = f.word >> 6 & 0xF_FFFF;
                let message = if code == 0 {
                    "break instruction executed; no code given.".to_string()
                } else {
                    format!("break instruction executed; code = {code}.")
                };
                return Err((ExcCode::Breakpoint, message, None));
            }
            0x10 => self.set_reg(f.rd, self.hi),
            0x11 => self.hi = rs,
            0x12 => self.set_reg(f.rd, self.lo),
            0x13 => self.lo = rs,
            0x18 => self.set_hilo((rs as i32 as i64).wrapping_mul(rt as i32 as i64) as u64),
            0x19 => self.set_hilo(rs as u64 * rt as u64),
            0x1A => {
                // MARS (as MIPS32): no exception and HI, LO unchanged on a zero divisor.
                if rt != 0 {
                    self.lo = (rs as i32).wrapping_div(rt as i32) as u32;
                    self.hi = (rs as i32).wrapping_rem(rt as i32) as u32;
                }
            }
            0x1B => {
                if let (Some(q), Some(r)) = (rs.checked_div(rt), rs.checked_rem(rt)) {
                    (self.lo, self.hi) = (q, r);
                }
            }
            0x20 => {
                let v = (rs as i32).checked_add(rt as i32).ok_or_else(overflow)?;
                self.set_reg(f.rd, v as u32);
            }
            0x21 => self.set_reg(f.rd, rs.wrapping_add(rt)),
            0x22 => {
                let v = (rs as i32).checked_sub(rt as i32).ok_or_else(overflow)?;
                self.set_reg(f.rd, v as u32);
            }
            0x23 => self.set_reg(f.rd, rs.wrapping_sub(rt)),
            0x24 => self.set_reg(f.rd, rs & rt),
            0x25 => self.set_reg(f.rd, rs | rt),
            0x26 => self.set_reg(f.rd, rs ^ rt),
            0x27 => self.set_reg(f.rd, !(rs | rt)),
            0x2A => self.set_reg(f.rd, ((rs as i32) < rt as i32) as u32),
            0x2B => self.set_reg(f.rd, (rs < rt) as u32),
            0x30 => trap(rs as i32 >= rt as i32)?,
            0x31 => trap(rs >= rt)?,
            0x32 => trap((rs as i32) < rt as i32)?,
            0x33 => trap(rs < rt)?,
            0x34 => trap(rs == rt)?,
            0x36 => trap(rs != rt)?,
            0x01 => {
                return Err((
                    ExcCode::ReservedInstruction,
                    format!("floating-point instructions are not supported yet ({})", hex(f.word)),
                    None,
                ));
            }
            _ => return Err(reserved(f.word)),
        }
        Ok(Step::Ok)
    }

    /// Opcode 0x1C: `mul`, `madd(u)`, `msub(u)`, `clz`, `clo`.
    fn special2(&mut self, f: &Fields) -> Result<Step, Fault> {
        let (rs, rt) = (self.reg(f.rs), self.reg(f.rt));
        let signed = (rs as i32 as i64).wrapping_mul(rt as i32 as i64) as u64;
        let unsigned = rs as u64 * rt as u64;
        match f.word & 0x3F {
            0x02 => {
                // MARS (like SPIM) also puts the product into HI and LO.
                self.set_hilo(signed);
                self.set_reg(f.rd, signed as u32);
            }
            0x00 => self.set_hilo(self.hilo().wrapping_add(signed)),
            0x01 => self.set_hilo(self.hilo().wrapping_add(unsigned)),
            0x04 => self.set_hilo(self.hilo().wrapping_sub(signed)),
            0x05 => self.set_hilo(self.hilo().wrapping_sub(unsigned)),
            // The rt field is not checked: MARS encodes clz / clo with rt = 0, MIPS32 with rt = rd.
            0x20 => self.set_reg(f.rd, rs.leading_zeros()),
            0x21 => self.set_reg(f.rd, rs.leading_ones()),
            _ => return Err(reserved(f.word)),
        }
        Ok(Step::Ok)
    }

    /// Opcode 0x10: `mfc0`, `mtc0`, `eret`.
    fn cop0(&mut self, f: &Fields) -> Result<Step, Fault> {
        match (f.rs, f.word & 0x3F) {
            (0x00, 0) => self.set_reg(f.rt, self.cp0_reg(f.rd)),
            (0x04, 0) => {
                if is_cp0_reg(f.rd) {
                    self.cp0[f.rd] = self.reg(f.rt);
                }
            }
            (0x10, 0x18) => {
                self.cp0[cp0::STATUS] &= !cp0::EXL;
                self.pc = self.cp0[cp0::EPC];
            }
            _ => return Err(reserved(f.word)),
        }
        Ok(Step::Ok)
    }

    /// A coprocessor 0 register (the ones MARS does not have read as 0).
    pub fn cp0_reg(&self, r: usize) -> u32 {
        if is_cp0_reg(r) { self.cp0[r] } else { 0 }
    }

    /// HI:LO as one 64-bit value.
    pub fn hilo(&self) -> u64 {
        (self.hi as u64) << 32 | self.lo as u64
    }

    fn set_hilo(&mut self, v: u64) {
        self.hi = (v >> 32) as u32;
        self.lo = v as u32;
    }
}

fn is_cp0_reg(r: usize) -> bool {
    matches!(r, cp0::VADDR | cp0::STATUS | cp0::CAUSE | cp0::EPC)
}

/// Checks every byte of a partial-word store before writing any of them, so that a refused store
/// leaves memory as it was (the bytes are in one word, but a word can straddle a segment end).
fn check_bytes(mem: &Memory, access_kind: Access, mut addrs: impl Iterator<Item = u32>) -> Result<(), Fault> {
    addrs.try_for_each(|a| mem.check_access(access_kind, a)).map_err(access)
}
