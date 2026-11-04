//! The MIPS32 processor as MARS simulates it: the integer instruction set of MARS (77 basic
//! instructions: arithmetic with and without overflow, HI/LO multiply and divide, shifts, logic,
//! loads and stores including `lwl`/`lwr`/`swl`/`swr` and `ll`/`sc`, branches, jumps, traps,
//! `syscall`, `break`, coprocessor 0 and `eret`). Branches take effect at once (MARS's default) or,
//! with delayed branching, after the instruction that follows them (see [`Delay`]).
//!
//! Exceptions follow MARS: Cause gets the exception code, EPC the address of the instruction,
//! BadVAddr the refused address, Status its exception level bit; when the exception handler
//! address holds code, execution continues there, otherwise the program stops with MARS's
//! message (`Runtime exception at 0x00400008: arithmetic overflow`).
//!
//! Coprocessor 1 (floating point) is in [`crate::fpu`]. External interrupts of the keyboard and
//! display ([`crate::mmio`]) are raised by the machine before an instruction.

use crate::memory::{Access, AccessError, Memory, hex};
use crate::settings::Layout;

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
    /// A runtime error MARS reports without an exception code (an odd register of a double
    /// operation): Cause, EPC and BadVAddr are not changed, but the exception handler (when there
    /// is one) is still entered.
    Runtime = 31,
    /// An external interrupt of the keyboard: exception code 0 with Cause bit 8 (MARS encodes the
    /// device in the code, which Cause gets shifted left by 2).
    KeyboardInterrupt = 0x40,
    /// An external interrupt of the display: exception code 0 with Cause bit 9.
    DisplayInterrupt = 0x80,
    /// The Digital Lab Sim's counter: Cause bit 10 (MARS's mask keeps it set afterwards).
    TimerInterrupt = 0x100,
    /// The Digital Lab Sim's hexadecimal keyboard: Cause bit 11.
    HexKeyboardInterrupt = 0x200,
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

/// A taken branch or jump waiting for its delay slot, with delayed branching (MARS's
/// `DelayedBranch`): registered by the branch, triggered at the end of that step, taken at the
/// end of the next one (the delay slot). A branch in a delay slot keeps the first target and
/// postpones it by one instruction, as in MARS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Delay {
    /// No branch pending.
    #[default]
    Cleared,
    /// A branch to this address was taken by the current instruction.
    Registered(u32),
    /// The next instruction is the delay slot; then execution goes to this address.
    Triggered(u32),
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
    /// Coprocessor 1 (floating point) registers `$f0`..`$f31`: a float, or a double in an even
    /// register and the next one (the low word first).
    pub fpr: [u32; 32],
    /// The 8 condition flags of coprocessor 1 (bit n: flag n), set by `c.eq/lt/le`.
    pub fcc: u8,
    /// Instructions executed since the reset.
    pub instr_count: u64,
    /// The last operation of the arithmetic and logic unit (for the ALU window; None until one).
    pub alu: Option<AluTrace>,
    /// Delayed branching (MARS's setting): branches and jumps take effect after their delay slot.
    pub delayed_branching: bool,
    /// The branch waiting for its delay slot.
    pub delay: Delay,
}

/// An operation of the arithmetic and logic unit: an arithmetic, logic, comparison, shift,
/// multiplication or division instruction, with its operands and result. Loads, stores, branches
/// and `lui` are not recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AluTrace {
    /// Address of the instruction.
    pub pc: u32,
    /// The instruction word (its mnemonic: [`crate::disasm`]).
    pub word: u32,
    /// The first operand: `rs` (`rt` for shifts).
    pub a: u32,
    /// The second operand: `rt`, the immediate as the instruction extends it, or the shift
    /// amount (0 for `clz`/`clo`).
    pub b: u32,
    /// What was written to the destination register (LO for the instructions that write HI and
    /// LO); for an overflow, the 32-bit result that was not written.
    pub result: u32,
    /// HI, for the instructions that write HI and LO.
    pub hi: Option<u32>,
    /// `add`, `addi`, `sub` overflowed: nothing was written, the exception was raised.
    pub overflow: bool,
    /// The instruction counter after it ([`Cpu::instr_count`]).
    pub instr: u64,
}

/// Where an ALU instruction takes its second operand and puts its result.
#[derive(Clone, Copy)]
enum AluForm {
    /// `rd = rs op rt`.
    Reg,
    /// `rt = rs op immediate` (sign-extended).
    SignedImm,
    /// `rt = rs op immediate` (zero-extended).
    UnsignedImm,
    /// `rd = rt shift shamt`.
    Shift,
    /// `rd = rt shift (rs & 31)`.
    VarShift,
    /// HI:LO from `rs` and `rt`.
    HiLo,
    /// `rd = op rs` (`clz`, `clo`).
    Unary,
}

/// The ALU form of an instruction word, None for the others and for those writing only `$zero`
/// (`nop` is `sll $zero, $zero, 0`).
fn alu_form(word: u32) -> Option<AluForm> {
    let form = match word >> 26 {
        0x00 => match word & 0x3F {
            0x00 | 0x02 | 0x03 => AluForm::Shift,
            0x04 | 0x06 | 0x07 => AluForm::VarShift,
            0x18..=0x1B => AluForm::HiLo,
            0x20..=0x27 | 0x2A | 0x2B => AluForm::Reg,
            _ => return None,
        },
        0x08..=0x0B => AluForm::SignedImm,
        0x0C..=0x0E => AluForm::UnsignedImm,
        0x1C => match word & 0x3F {
            0x02 => AluForm::Reg,
            0x00 | 0x01 | 0x04 | 0x05 => AluForm::HiLo,
            0x20 | 0x21 => AluForm::Unary,
            _ => return None,
        },
        _ => return None,
    };
    let dest = match form {
        AluForm::HiLo => return Some(form),
        AluForm::SignedImm | AluForm::UnsignedImm => word >> 16 & 31,
        _ => word >> 11 & 31,
    };
    (dest != 0).then_some(form)
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu::new()
    }
}

/// The parts of an instruction word.
#[derive(Clone, Copy)]
pub(crate) struct Fields {
    pub(crate) word: u32,
    pub(crate) rs: usize,
    pub(crate) rt: usize,
    pub(crate) rd: usize,
    pub(crate) shamt: u32,
    /// The 16-bit immediate, sign-extended.
    pub(crate) simm: u32,
    /// The 16-bit immediate, zero-extended.
    uimm: u32,
}

impl Fields {
    pub(crate) fn new(word: u32) -> Fields {
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
pub(crate) type Fault = (ExcCode, String, Option<u32>);

pub(crate) fn access(e: AccessError) -> Fault {
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

pub(crate) fn reserved(word: u32) -> Fault {
    (ExcCode::ReservedInstruction, format!("undefined instruction ({})", hex(word)), None)
}

impl Cpu {
    /// The processor after a reset in MARS's default memory configuration: registers 0, `$gp`
    /// and `$sp` at their initial values, the program counter at the text segment, Status
    /// 0x0000FF11.
    pub fn new() -> Cpu {
        Cpu::with_layout(&crate::settings::DEFAULT)
    }

    /// The processor after a reset in memory configuration `layout` (its `$gp`, `$sp` and text
    /// segment).
    pub fn with_layout(layout: &Layout) -> Cpu {
        let mut regs = [0; 32];
        regs[GP] = layout.gp;
        regs[SP] = layout.sp;
        let mut cp0 = [0; 32];
        cp0[cp0::STATUS] = cp0::STATUS_RESET;
        Cpu {
            regs,
            hi: 0,
            lo: 0,
            pc: layout.text_base,
            cp0,
            fpr: [0; 32],
            fcc: 0,
            instr_count: 0,
            alu: None,
            delayed_branching: false,
            delay: Delay::Cleared,
        }
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

    /// Executes the instruction at the program counter. After `syscall` the caller performs the
    /// service and then calls [`Cpu::end_step`] (unless the service waits for input or the
    /// program stops).
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
        let form = alu_form(word);
        let operands = form.map(|form| self.alu_operands(form, word));
        let result = self.execute(mem, word);
        if let (Some(form), Some((a, b))) = (form, operands) {
            self.record_alu(form, word, pc, a, b, matches!(&result, Err((ExcCode::Overflow, ..))));
        }
        let step = match result {
            Ok(step) => step,
            Err((code, message, bad)) => Step::Exception(self.raise(mem, code, message, pc, bad, false)),
        };
        match &step {
            Step::Ok => self.end_step(),
            Step::Exception(e) if e.handled => self.end_step(),
            _ => {}
        }
        step
    }

    /// The end of a step that went on (MARS's simulator loop after an instruction): a branch
    /// whose delay slot just executed is taken, a branch just registered is triggered.
    #[inline]
    pub fn end_step(&mut self) {
        match self.delay {
            Delay::Cleared => {}
            Delay::Registered(t) => self.delay = Delay::Triggered(t),
            Delay::Triggered(t) => {
                self.pc = t;
                self.delay = Delay::Cleared;
            }
        }
    }

    /// Goes to `target` (MARS's `processJump` / `processBranch`): at once, or after the delay
    /// slot with delayed branching.
    #[inline]
    pub(crate) fn jump(&mut self, target: u32) {
        if !self.delayed_branching {
            self.pc = target;
        } else if let Delay::Cleared = self.delay {
            self.delay = Delay::Registered(target);
        } else if let Delay::Triggered(t) = self.delay {
            self.delay = Delay::Registered(t);
        }
    }

    /// The return address a link writes (MARS's `processReturnAddress`): past the delay slot
    /// with delayed branching.
    #[inline]
    fn return_address(&self) -> u32 {
        if self.delayed_branching { self.pc.wrapping_add(4) } else { self.pc }
    }

    /// The two operands of an ALU instruction, before it executes.
    fn alu_operands(&self, form: AluForm, word: u32) -> (u32, u32) {
        let f = Fields::new(word);
        let (rs, rt) = (self.reg(f.rs), self.reg(f.rt));
        match form {
            AluForm::Reg | AluForm::HiLo => (rs, rt),
            AluForm::SignedImm => (rs, f.simm),
            AluForm::UnsignedImm => (rs, f.uimm),
            AluForm::Shift => (rt, f.shamt),
            AluForm::VarShift => (rt, rs & 31),
            AluForm::Unary => (rs, 0),
        }
    }

    /// Records an executed ALU instruction ([`Cpu::alu`]) from its operands and what it wrote.
    fn record_alu(&mut self, form: AluForm, word: u32, pc: u32, a: u32, b: u32, overflow: bool) {
        let f = Fields::new(word);
        let (result, hi) = match form {
            AluForm::HiLo => (self.lo, Some(self.hi)),
            // add / addi / sub: the destination was not written.
            _ if overflow => {
                (if word & 0x3F == 0x22 && word >> 26 == 0 { a.wrapping_sub(b) } else { a.wrapping_add(b) }, None)
            }
            AluForm::SignedImm | AluForm::UnsignedImm => (self.reg(f.rt), None),
            // `mul` also writes HI and LO.
            AluForm::Reg if word >> 26 == 0x1C => (self.reg(f.rd), Some(self.hi)),
            _ => (self.reg(f.rd), None),
        };
        self.alu = Some(AluTrace { pc, word, a, b, result, hi, overflow, instr: self.instr_count });
    }

    /// Records an exception of the instruction at `pc` in coprocessor 0 (as MARS does whether or
    /// not there is a handler) and goes to the exception handler when it holds code; otherwise the
    /// program counter stays at the instruction. `bad` is the refused address of an address
    /// exception, `fetch` tells that the instruction itself could not be fetched. The machine
    /// raises the errors of system calls with this too.
    pub fn raise(
        &mut self,
        mem: &Memory,
        code: ExcCode,
        message: String,
        pc: u32,
        bad: Option<u32>,
        fetch: bool,
    ) -> Exception {
        // MARS's runtime errors without a code leave coprocessor 0 as it is.
        if code != ExcCode::Runtime {
            if let Some(a) = bad {
                self.cp0[cp0::VADDR] = a;
            }
            self.cp0[cp0::CAUSE] = (self.cp0[cp0::CAUSE] & 0xFFFF_FC83) | ((code as u32) << 2);
            self.cp0[cp0::EPC] = pc;
            self.cp0[cp0::STATUS] |= cp0::EXL;
        }
        let handler = mem.layout.exception_handler;
        let handled = mem.has_code(handler);
        self.pc = if handled { handler } else { pc };
        Exception { code, message, pc, bad_vaddr: bad, fetch, handled }
    }

    /// Branch to `offset` instructions from the next one.
    fn branch(&mut self, f: &Fields, cond: bool) {
        if cond {
            self.jump(self.pc.wrapping_add(f.simm << 2));
        }
    }

    /// Branch and link: MARS links only when the branch is taken.
    fn branch_link(&mut self, f: &Fields, cond: bool) {
        if cond {
            self.set_reg(RA, self.return_address());
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
            0x02 => self.jump((self.pc & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2)),
            0x03 => {
                self.set_reg(RA, self.return_address());
                self.jump((self.pc & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2));
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
            0x11 => self.cop1(&f)?,
            0x31 | 0x35 | 0x39 | 0x3D => self.fpu_memory(mem, &f, addr)?,
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
            0x08 => self.jump(rs),
            0x09 => {
                // As MARS: the link first, then the jump to rs (its new value when rd = rs).
                self.set_reg(f.rd, self.return_address());
                self.jump(self.reg(f.rs));
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
            0x01 => self.movf_gpr(f)?,
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
