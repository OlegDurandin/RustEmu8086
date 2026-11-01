//! 8087 floating-point coprocessor: the register stack ST(0)..ST(7) of 80-bit extended reals,
//! the status, control and tag words, and the ESC instructions D8..DF.
//!
//! Arithmetic is exact 80-bit IEEE arithmetic (`rustc_apfloat`) with the rounding and precision
//! control of the control word. The transcendental instructions are computed in double precision.
//! A few 80287/80387 instructions that tutorials use (FSTSW AX, FSIN, FCOS, FSINCOS, FUCOM, FPREM1)
//! are accepted as well. Exceptions are recorded in the status word; unmasked ones also set ES
//! (the 8087 would signal an interrupt) but the masked response is used.

use rustc_apfloat::ieee::{Double, Single, X87DoubleExtended as F80};
use rustc_apfloat::{Float, FloatConvert, Round, Status};

/// Status word bits.
pub mod sw {
    /// Invalid operation (stack overflow or underflow, 0/0, the root of a negative number, ...).
    pub const IE: u16 = 0x0001;
    /// Denormalized operand.
    pub const DE: u16 = 0x0002;
    /// Division by zero.
    pub const ZE: u16 = 0x0004;
    /// Overflow: the result is too large for the destination.
    pub const OE: u16 = 0x0008;
    /// Underflow: the result is too small for the destination.
    pub const UE: u16 = 0x0010;
    /// Precision: the result was rounded.
    pub const PE: u16 = 0x0020;
    /// Error summary: an unmasked exception is pending.
    pub const ES: u16 = 0x0080;
    /// Condition code C0 (the "carry" of comparisons: below).
    pub const C0: u16 = 0x0100;
    /// Condition code C1 (the direction of rounding; stack overflow vs underflow).
    pub const C1: u16 = 0x0200;
    /// Condition code C2 (unordered comparison; partial remainder of FPREM).
    pub const C2: u16 = 0x0400;
    /// TOP: the physical register of ST(0), bits 11..13.
    pub const TOP: u16 = 0x3800;
    /// Condition code C3 (the "zero" of comparisons: equal).
    pub const C3: u16 = 0x4000;
    /// Busy.
    pub const B: u16 = 0x8000;
}

/// Tag of a register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// A finite non-zero number.
    Valid,
    /// Zero.
    Zero,
    /// NaN, infinity or denormal.
    Special,
    /// No value (popped or never loaded).
    Empty,
}

/// The coprocessor state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fpu {
    /// Physical registers R0..R7, 80-bit little-endian (ST(i) is R[(TOP + i) & 7]).
    pub regs: [[u8; 10]; 8],
    /// Control word: exception masks, precision (bits 8..9) and rounding (bits 10..11).
    pub control: u16,
    /// Status word, TOP in bits 11..13.
    pub status: u16,
    /// Tag word: two bits per physical register ([`Tag`]).
    pub tag: u16,
    /// Linear address of the last instruction (FSTENV / FSAVE store it).
    pub last_ip: u32,
    /// Opcode of the last instruction (the low 11 bits of its ESC bytes).
    pub last_opcode: u16,
    /// Linear address of the last memory operand.
    pub last_data: u32,
}

impl Default for Fpu {
    fn default() -> Self {
        Self::new()
    }
}

/// Where an instruction came from (for FSTENV / FSAVE).
#[derive(Clone, Copy, Debug, Default)]
pub struct Ctx {
    /// Linear address of the instruction.
    pub ip: u32,
    /// Its opcode (11 bits).
    pub opcode: u16,
    /// Linear address of its memory operand.
    pub data: u32,
}

/// What the CPU does after an instruction: write `write` bytes of the buffer back to the memory
/// operand, and load AX (FSTSW AX).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Done {
    /// Bytes of the operand buffer to write back to memory (0: none).
    pub write: usize,
    /// The new AX (FSTSW AX).
    pub ax: Option<u16>,
}

/// Bytes the CPU reads from the memory operand before the instruction (ESC `op`, ModR/M `reg`).
pub fn read_len(op: u8, reg: u8) -> usize {
    match (op, reg) {
        (0xD8 | 0xDA, _) => 4,
        (0xD9, 0) => 4,
        (0xD9, 4) => 14,
        (0xD9, 5) => 2,
        (0xDB, 0) => 4,
        (0xDB, 5) => 10,
        (0xDC, _) => 8,
        (0xDD, 0) => 8,
        (0xDD, 4) => 94,
        (0xDE, _) => 2,
        (0xDF, 0) => 2,
        (0xDF, 4) => 10,
        (0xDF, 5) => 8,
        _ => 0,
    }
}

/// The "real indefinite": the quiet NaN masked invalid operations produce.
const INDEFINITE: u128 = 0xFFFF_C000_0000_0000_0000;

fn bits(b: &[u8; 10]) -> u128 {
    let mut x = [0u8; 16];
    x[..10].copy_from_slice(b);
    u128::from_le_bytes(x)
}

fn bytes(v: u128) -> [u8; 10] {
    let mut b = [0u8; 10];
    b.copy_from_slice(&v.to_le_bytes()[..10]);
    b
}

fn tag_of(v: F80) -> Tag {
    if v.is_zero() {
        Tag::Zero
    } else if v.is_nan() || v.is_infinite() || v.is_denormal() {
        Tag::Special
    } else {
        Tag::Valid
    }
}

/// Approximate value of an 80-bit real (for display and the transcendental instructions).
pub fn to_f64(v: [u8; 10]) -> f64 {
    let x = F80::from_bits(bits(&v));
    let r: Double = x.convert_r(Round::NearestTiesToEven, &mut false).value;
    f64::from_bits(r.to_bits() as u64)
}

fn from_f64(x: f64) -> F80 {
    Double::from_bits(x.to_bits() as u128).convert(&mut false).value
}

fn f80_from_bytes(b: &[u8]) -> F80 {
    let mut a = [0u8; 10];
    a.copy_from_slice(&b[..10]);
    F80::from_bits(bits(&a))
}

impl Fpu {
    /// State after FINIT.
    pub fn new() -> Self {
        Fpu { regs: [[0; 10]; 8], control: 0x03FF, status: 0, tag: 0xFFFF, last_ip: 0, last_opcode: 0, last_data: 0 }
    }

    /// The physical register of ST(0).
    pub fn top(&self) -> usize {
        ((self.status & sw::TOP) >> 11) as usize
    }

    fn set_top(&mut self, t: usize) {
        self.status = (self.status & !sw::TOP) | (((t & 7) as u16) << 11);
    }

    fn phys(&self, i: usize) -> usize {
        (self.top() + i) & 7
    }

    /// Tag of physical register `r`.
    pub fn tag_of_reg(&self, r: usize) -> Tag {
        match (self.tag >> (r * 2)) & 3 {
            0 => Tag::Valid,
            1 => Tag::Zero,
            2 => Tag::Special,
            _ => Tag::Empty,
        }
    }

    fn set_tag(&mut self, r: usize, t: Tag) {
        let v = match t {
            Tag::Valid => 0,
            Tag::Zero => 1,
            Tag::Special => 2,
            Tag::Empty => 3,
        };
        self.tag = (self.tag & !(3 << (r * 2))) | (v << (r * 2));
    }

    /// Tag of ST(i).
    pub fn st_tag(&self, i: usize) -> Tag {
        self.tag_of_reg(self.phys(i))
    }

    /// ST(i) as 80-bit bytes.
    pub fn st(&self, i: usize) -> [u8; 10] {
        self.regs[self.phys(i)]
    }

    fn round(&self) -> Round {
        match (self.control >> 10) & 3 {
            0 => Round::NearestTiesToEven,
            1 => Round::TowardNegative,
            2 => Round::TowardPositive,
            _ => Round::TowardZero,
        }
    }

    /// Records exception flags; an unmasked one also sets ES and B.
    fn raise(&mut self, flags: u16) {
        self.status |= flags;
        if flags & !self.control & 0x3F != 0 {
            self.status |= sw::ES | sw::B;
        }
    }

    fn raise_status(&mut self, s: Status) {
        let mut f = 0;
        if s.contains(Status::INVALID_OP) {
            f |= sw::IE;
        }
        if s.contains(Status::DIV_BY_ZERO) {
            f |= sw::ZE;
        }
        if s.contains(Status::OVERFLOW) {
            f |= sw::OE;
        }
        if s.contains(Status::UNDERFLOW) {
            f |= sw::UE;
        }
        if s.contains(Status::INEXACT) {
            f |= sw::PE;
        }
        self.raise(f);
    }

    /// ST(i); an empty register is a stack underflow (invalid operation, indefinite).
    fn get(&mut self, i: usize) -> F80 {
        let r = self.phys(i);
        if self.tag_of_reg(r) == Tag::Empty {
            self.raise(sw::IE);
            return F80::from_bits(INDEFINITE);
        }
        let v = F80::from_bits(bits(&self.regs[r]));
        if v.is_denormal() {
            self.raise(sw::DE);
        }
        v
    }

    fn set(&mut self, i: usize, v: F80) {
        let r = self.phys(i);
        self.regs[r] = bytes(v.to_bits());
        self.set_tag(r, tag_of(v));
    }

    fn push(&mut self, v: F80) {
        let t = (self.top() + 7) & 7;
        self.set_top(t);
        if self.tag_of_reg(t) != Tag::Empty {
            // Stack overflow.
            self.raise(sw::IE | sw::C1);
            self.set(0, F80::from_bits(INDEFINITE));
            return;
        }
        self.set(0, v);
    }

    fn pop(&mut self) {
        let t = self.top();
        self.set_tag(t, Tag::Empty);
        self.set_top(t + 1);
    }

    /// The result of an arithmetic operation, rounded to the precision of the control word.
    fn finish(&mut self, r: rustc_apfloat::StatusAnd<F80>) -> F80 {
        let mut s = r.status;
        let mut v = r.value;
        match (self.control >> 8) & 3 {
            0 => {
                let x: rustc_apfloat::StatusAnd<Single> = v.convert_r(self.round(), &mut false);
                s |= x.status;
                v = x.value.convert(&mut false).value;
            }
            2 => {
                let x: rustc_apfloat::StatusAnd<Double> = v.convert_r(self.round(), &mut false);
                s |= x.status;
                v = x.value.convert(&mut false).value;
            }
            _ => {}
        }
        self.raise_status(s);
        v
    }

    fn arith(&mut self, op: u8, a: F80, b: F80) -> F80 {
        let rc = self.round();
        let r = match op {
            0 => a.add_r(b, rc),
            1 => a.mul_r(b, rc),
            4 => a.sub_r(b, rc),
            5 => b.sub_r(a, rc),
            6 => a.div_r(b, rc),
            _ => b.div_r(a, rc),
        };
        self.finish(r)
    }

    /// FCOM / FTST: C3 C2 C0 = 000 (a > b), 001 (a < b), 100 (equal), 111 (unordered).
    fn compare(&mut self, a: F80, b: F80, quiet: bool) {
        self.status &= !(sw::C0 | sw::C2 | sw::C3);
        match a.partial_cmp(&b) {
            Some(std::cmp::Ordering::Greater) => {}
            Some(std::cmp::Ordering::Less) => self.status |= sw::C0,
            Some(std::cmp::Ordering::Equal) => self.status |= sw::C3,
            None => {
                self.status |= sw::C0 | sw::C2 | sw::C3;
                if !quiet || a.is_signaling() || b.is_signaling() {
                    self.raise(sw::IE);
                }
            }
        }
    }

    /// The operand of a memory form: real (4 / 8 / 10 bytes) or integer (2 / 4 / 8 bytes).
    fn load(&mut self, mem: &[u8], kind: Mem) -> F80 {
        let v = match kind {
            Mem::R32 => {
                let b = u32::from_le_bytes(mem[..4].try_into().unwrap());
                let x = Single::from_bits(b as u128);
                if x.is_denormal() {
                    self.raise(sw::DE);
                }
                x.convert(&mut false).value
            }
            Mem::R64 => {
                let b = u64::from_le_bytes(mem[..8].try_into().unwrap());
                let x = Double::from_bits(b as u128);
                if x.is_denormal() {
                    self.raise(sw::DE);
                }
                x.convert(&mut false).value
            }
            Mem::R80 => f80_from_bytes(mem),
            Mem::I16 => F80::from_i128(i16::from_le_bytes(mem[..2].try_into().unwrap()) as i128).value,
            Mem::I32 => F80::from_i128(i32::from_le_bytes(mem[..4].try_into().unwrap()) as i128).value,
            Mem::I64 => F80::from_i128(i64::from_le_bytes(mem[..8].try_into().unwrap()) as i128).value,
        };
        if v.is_signaling() {
            self.raise(sw::IE);
        }
        v
    }

    /// Stores `v` to memory as `kind`; returns the bytes written.
    fn store(&mut self, v: F80, mem: &mut [u8], kind: Mem) -> usize {
        let rc = self.round();
        match kind {
            Mem::R32 => {
                let x: rustc_apfloat::StatusAnd<Single> = v.convert_r(rc, &mut false);
                self.raise_status(x.status);
                mem[..4].copy_from_slice(&(x.value.to_bits() as u32).to_le_bytes());
                4
            }
            Mem::R64 => {
                let x: rustc_apfloat::StatusAnd<Double> = v.convert_r(rc, &mut false);
                self.raise_status(x.status);
                mem[..8].copy_from_slice(&(x.value.to_bits() as u64).to_le_bytes());
                8
            }
            Mem::R80 => {
                mem[..10].copy_from_slice(&bytes(v.to_bits()));
                10
            }
            Mem::I16 | Mem::I32 | Mem::I64 => {
                let width = match kind {
                    Mem::I16 => 16,
                    Mem::I32 => 32,
                    _ => 64,
                };
                let r = v.to_i128_r(width, rc, &mut true);
                // Out of range or NaN: invalid, the "integer indefinite" (the most negative value).
                let n = if r.status.contains(Status::INVALID_OP) {
                    self.raise(sw::IE);
                    -1i128 << (width - 1)
                } else {
                    self.raise_status(r.status);
                    r.value
                };
                let b = n.to_le_bytes();
                mem[..width / 8].copy_from_slice(&b[..width / 8]);
                width / 8
            }
        }
    }

    /// Executes ESC `op` with ModR/M `reg` and the register operand `rm` (None: the memory
    /// operand, whose bytes are in `mem`; results to store are written back into `mem`).
    pub fn exec(&mut self, op: u8, reg: u8, rm: Option<u8>, mem: &mut [u8], ctx: Ctx) -> Done {
        let control_op =
            matches!((op, reg, rm), (0xD9, 4..=7, None) | (0xDB, 4, Some(0..=4)) | (0xDD, 4 | 6 | 7, None))
                || (op, reg, rm) == (0xDF, 4, Some(0));
        if !control_op {
            self.last_ip = ctx.ip;
            self.last_opcode = ctx.opcode & 0x7FF;
            if rm.is_none() {
                self.last_data = ctx.data;
            }
        }
        let mut done = Done::default();
        match (op, rm) {
            // ---- arithmetic on ST(0) with a memory operand / ST(i) ----
            (0xD8 | 0xDA | 0xDC | 0xDE, None) => {
                let kind = match op {
                    0xD8 => Mem::R32,
                    0xDA => Mem::I32,
                    0xDC => Mem::R64,
                    _ => Mem::I16,
                };
                let b = self.load(mem, kind);
                let a = self.get(0);
                match reg {
                    2 => self.compare(a, b, false),
                    3 => {
                        self.compare(a, b, false);
                        self.pop();
                    }
                    _ => {
                        let r = self.arith(reg, a, b);
                        self.set(0, r);
                    }
                }
            }
            (0xD8, Some(i)) => {
                let (a, b) = (self.get(0), self.get(i as usize));
                match reg {
                    2 => self.compare(a, b, false),
                    3 => {
                        self.compare(a, b, false);
                        self.pop();
                    }
                    _ => {
                        let r = self.arith(reg, a, b);
                        self.set(0, r);
                    }
                }
            }
            // ST(i) = ST(i) op ST(0) (DC), and pop (DE); SUB/SUBR and DIV/DIVR swap places.
            (0xDC | 0xDE, Some(i)) => {
                let i = i as usize;
                match (op, reg) {
                    (_, 2) | (_, 3) if op == 0xDC => {
                        // FCOM2 / FCOMP3: aliases of FCOM / FCOMP.
                        let (a, b) = (self.get(0), self.get(i));
                        self.compare(a, b, false);
                        if reg == 3 {
                            self.pop();
                        }
                    }
                    (0xDE, 3) if i == 1 => {
                        // FCOMPP
                        let (a, b) = (self.get(0), self.get(1));
                        self.compare(a, b, false);
                        self.pop();
                        self.pop();
                    }
                    (0xDE, 2) | (0xDE, 3) => {
                        let (a, b) = (self.get(0), self.get(i));
                        self.compare(a, b, false);
                        self.pop();
                    }
                    _ => {
                        let r = if reg >= 4 { reg ^ 1 } else { reg };
                        let (a, b) = (self.get(i), self.get(0));
                        let v = self.arith(r, a, b);
                        self.set(i, v);
                        if op == 0xDE {
                            self.pop();
                        }
                    }
                }
            }
            // ---- loads, stores and the rest ----
            (0xD9, None) => match reg {
                0 => {
                    let v = self.load(mem, Mem::R32);
                    self.push(v);
                }
                2 | 3 => {
                    let v = self.get(0);
                    done.write = self.store(v, mem, Mem::R32);
                    if reg == 3 {
                        self.pop();
                    }
                }
                4 => self.load_env(mem),
                5 => self.control = u16::from_le_bytes([mem[0], mem[1]]),
                6 => done.write = self.store_env(mem),
                7 => {
                    mem[..2].copy_from_slice(&self.control.to_le_bytes());
                    done.write = 2;
                }
                _ => {}
            },
            (0xD9, Some(i)) => self.d9_reg(reg, i as usize),
            (0xDB, None) => match reg {
                0 => {
                    let v = self.load(mem, Mem::I32);
                    self.push(v);
                }
                2 | 3 => {
                    let v = self.get(0);
                    done.write = self.store(v, mem, Mem::I32);
                    if reg == 3 {
                        self.pop();
                    }
                }
                5 => {
                    let v = self.load(mem, Mem::R80);
                    self.push(v);
                }
                7 => {
                    let v = self.get(0);
                    done.write = self.store(v, mem, Mem::R80);
                    self.pop();
                }
                _ => {}
            },
            (0xDB, Some(i)) if reg == 4 => match i {
                // FENI / FDISI: interrupt enable mask (IEM, bit 7 of the control word).
                0 => self.control &= !0x80,
                1 => self.control |= 0x80,
                2 => self.status &= !(0x3F | sw::ES | sw::B),
                3 => {
                    *self = Fpu {
                        last_ip: self.last_ip,
                        last_opcode: self.last_opcode,
                        last_data: self.last_data,
                        ..Fpu::new()
                    }
                }
                _ => {}
            },
            (0xDB, Some(_)) => {}
            (0xDD, None) => match reg {
                0 => {
                    let v = self.load(mem, Mem::R64);
                    self.push(v);
                }
                2 | 3 => {
                    let v = self.get(0);
                    done.write = self.store(v, mem, Mem::R64);
                    if reg == 3 {
                        self.pop();
                    }
                }
                4 => {
                    self.load_env(mem);
                    for k in 0..8 {
                        let r = self.phys(k);
                        self.regs[r].copy_from_slice(&mem[14 + k * 10..24 + k * 10]);
                    }
                }
                6 => {
                    self.store_env(mem);
                    for k in 0..8 {
                        mem[14 + k * 10..24 + k * 10].copy_from_slice(&self.st(k));
                    }
                    done.write = 94;
                    *self = Fpu::new();
                }
                7 => {
                    mem[..2].copy_from_slice(&self.status.to_le_bytes());
                    done.write = 2;
                }
                _ => {}
            },
            (0xDD, Some(i)) => {
                let i = i as usize;
                match reg {
                    // FFREE
                    0 => {
                        let r = self.phys(i);
                        self.set_tag(r, Tag::Empty);
                    }
                    // FXCH4
                    1 => self.d9_reg(1, i),
                    2 | 3 => {
                        let v = self.get(0);
                        self.set(i, v);
                        if reg == 3 {
                            self.pop();
                        }
                    }
                    // FUCOM / FUCOMP (80387)
                    4 | 5 => {
                        let (a, b) = (self.get(0), self.get(i));
                        self.compare(a, b, true);
                        if reg == 5 {
                            self.pop();
                        }
                    }
                    _ => {}
                }
            }
            (0xDF, None) => match reg {
                0 => {
                    let v = self.load(mem, Mem::I16);
                    self.push(v);
                }
                2 | 3 => {
                    let v = self.get(0);
                    done.write = self.store(v, mem, Mem::I16);
                    if reg == 3 {
                        self.pop();
                    }
                }
                4 => {
                    let v = bcd_load(&mem[..10]);
                    self.push(v);
                }
                5 => {
                    let v = self.load(mem, Mem::I64);
                    self.push(v);
                }
                6 => {
                    let v = self.get(0);
                    done.write = self.bcd_store(v, mem);
                    self.pop();
                }
                7 => {
                    let v = self.get(0);
                    done.write = self.store(v, mem, Mem::I64);
                    self.pop();
                }
                _ => {}
            },
            (0xDF, Some(i)) => match reg {
                // FSTSW AX (80287)
                4 if i == 0 => done.ax = Some(self.status),
                // FFREEP, FXCH7, FSTP8/9 (undocumented aliases)
                0 => {
                    let r = self.phys(i as usize);
                    self.set_tag(r, Tag::Empty);
                    self.pop();
                }
                1 => self.d9_reg(1, i as usize),
                2 | 3 => {
                    let v = self.get(0);
                    self.set(i as usize, v);
                    self.pop();
                }
                _ => {}
            },
            // FUCOMPP (80387): DA E9.
            (0xDA, Some(1)) if reg == 5 => {
                let (a, b) = (self.get(0), self.get(1));
                self.compare(a, b, true);
                self.pop();
                self.pop();
            }
            _ => {}
        }
        done
    }

    /// D9 with a register operand: FLD ST(i), FXCH, and the operations without operands.
    fn d9_reg(&mut self, reg: u8, i: usize) {
        match reg {
            0 => {
                let v = self.get(i);
                self.push(v);
            }
            1 => {
                let (a, b) = (self.get(0), self.get(i));
                self.set(0, b);
                self.set(i, a);
            }
            2 => {} // FNOP (D9 D0)
            3 => {
                // FSTP1 (undocumented)
                let v = self.get(0);
                self.set(i, v);
                self.pop();
            }
            4 => match i {
                0 => {
                    let v = self.get(0);
                    self.set(0, -v);
                }
                1 => {
                    let v = self.get(0);
                    self.set(0, v.abs());
                }
                4 => {
                    let v = self.get(0);
                    self.compare(v, F80::ZERO, false);
                }
                5 => self.fxam(),
                _ => {}
            },
            5 => {
                let c = match i {
                    0 => 0x3FFF_8000_0000_0000_0000,
                    1 => 0x4000_D49A_784B_CD1B_8AFE,
                    2 => 0x3FFF_B8AA_3B29_5C17_F0BC,
                    3 => 0x4000_C90F_DAA2_2168_C235,
                    4 => 0x3FFD_9A20_9A84_FBCF_F799,
                    5 => 0x3FFE_B172_17F7_D1CF_79AC,
                    6 => 0,
                    _ => return,
                };
                self.push(F80::from_bits(c));
            }
            6 => self.d9_f0(i),
            _ => self.d9_f8(i),
        }
    }

    fn fxam(&mut self) {
        let r = self.phys(0);
        let v = F80::from_bits(bits(&self.regs[r]));
        self.status &= !(sw::C0 | sw::C1 | sw::C2 | sw::C3);
        if v.is_negative() {
            self.status |= sw::C1;
        }
        self.status |= if self.tag_of_reg(r) == Tag::Empty {
            sw::C3 | sw::C0
        } else if v.is_nan() {
            sw::C0
        } else if v.is_infinite() {
            sw::C2 | sw::C0
        } else if v.is_zero() {
            sw::C3
        } else if v.is_denormal() {
            sw::C3 | sw::C2
        } else {
            sw::C2
        };
    }

    /// D9 F0..F7: F2XM1, FYL2X, FPTAN, FPATAN, FXTRACT, FPREM1, FDECSTP, FINCSTP.
    fn d9_f0(&mut self, i: usize) {
        match i {
            0 => {
                let x = self.get(0);
                let r = from_f64(to_f64(bytes(x.to_bits())).exp2() - 1.0);
                self.set(0, r);
                self.raise(sw::PE);
            }
            1 | 3 => {
                let (x, y) = (self.get(0), self.get(1));
                let (fx, fy) = (to_f64(bytes(x.to_bits())), to_f64(bytes(y.to_bits())));
                let r = if i == 1 { fy * fx.log2() } else { fy.atan2(fx) };
                self.pop();
                self.set(0, from_f64(r));
                self.raise(sw::PE);
            }
            2 => {
                // FPTAN: ST(0) = tan, then 1.0 is pushed (ST(1) / ST(0) = tan, as the 8087's Y / X).
                let x = self.get(0);
                self.set(0, from_f64(to_f64(bytes(x.to_bits())).tan()));
                self.push(F80::from_bits(0x3FFF_8000_0000_0000_0000));
                self.status &= !sw::C2;
                self.raise(sw::PE);
            }
            4 => {
                // FXTRACT: ST(0) = significand, ST(1) = exponent.
                let x = self.get(0);
                if x.is_zero() {
                    self.raise(sw::ZE);
                    self.set(0, -F80::INFINITY);
                    self.push(x);
                } else {
                    let e = x.ilogb();
                    self.set(0, F80::from_i128(e as i128).value);
                    self.push(x.scalbn(-e));
                }
            }
            5 => self.fprem(true),
            6 => {
                let t = self.top();
                self.set_top(t + 7);
            }
            _ => {
                let t = self.top();
                self.set_top(t + 1);
            }
        }
    }

    /// D9 F8..FF: FPREM, FYL2XP1, FSQRT, FSINCOS, FRNDINT, FSCALE, FSIN, FCOS.
    fn d9_f8(&mut self, i: usize) {
        match i {
            0 => self.fprem(false),
            1 => {
                let (x, y) = (self.get(0), self.get(1));
                let r = to_f64(bytes(y.to_bits())) * to_f64(bytes(x.to_bits())).ln_1p() / std::f64::consts::LN_2;
                self.pop();
                self.set(0, from_f64(r));
                self.raise(sw::PE);
            }
            2 => {
                let x = self.get(0);
                let r = self.sqrt(x);
                self.set(0, r);
            }
            3 | 6 | 7 => {
                let x = self.get(0);
                let f = to_f64(bytes(x.to_bits()));
                self.status &= !sw::C2;
                match i {
                    3 => {
                        self.set(0, from_f64(f.sin()));
                        self.push(from_f64(f.cos()));
                    }
                    6 => self.set(0, from_f64(f.sin())),
                    _ => self.set(0, from_f64(f.cos())),
                }
                self.raise(sw::PE);
            }
            4 => {
                let x = self.get(0);
                let r = x.round_to_integral(self.round());
                self.raise_status(r.status);
                self.set(0, r.value);
            }
            _ => {
                // FSCALE: ST(0) * 2^trunc(ST(1)).
                let (x, y) = (self.get(0), self.get(1));
                let n = y.to_i128(32).value.clamp(-70000, 70000) as i32;
                self.set(0, x.scalbn(n));
            }
        }
    }

    /// Square root: a double-precision guess refined by a Newton step in 80 bits.
    fn sqrt(&mut self, x: F80) -> F80 {
        if x.is_nan() || x.is_zero() || x.is_pos_infinity() {
            return x;
        }
        if x.is_negative() {
            self.raise(sw::IE);
            return F80::from_bits(INDEFINITE);
        }
        let rc = self.round();
        let mut y = from_f64(to_f64(bytes(x.to_bits())).sqrt());
        if y.is_zero() || y.is_infinite() {
            // Out of the double range: scale by an even power of two.
            let e = x.ilogb() & !1;
            let s = x.scalbn(-e);
            y = from_f64(to_f64(bytes(s.to_bits())).sqrt()).scalbn(e / 2);
        }
        for _ in 0..2 {
            let q = x.div_r(y, rc).value;
            y = (y + q).value.scalbn(-1);
        }
        // Inexact unless the square is exact.
        if (y * y).value.partial_cmp(&x) != Some(std::cmp::Ordering::Equal) {
            self.raise(sw::PE);
        }
        y
    }

    /// FPREM (truncating) / FPREM1 (rounding): ST(0) = remainder of ST(0) / ST(1); C0 C3 C1 = the
    /// low three bits of the quotient, C2 = 0 (the reduction is complete).
    fn fprem(&mut self, ieee: bool) {
        let (a, b) = (self.get(0), self.get(1));
        self.status &= !(sw::C0 | sw::C1 | sw::C2 | sw::C3);
        if b.is_zero() || a.is_infinite() || a.is_nan() || b.is_nan() {
            self.raise(sw::IE);
            self.set(0, F80::from_bits(INDEFINITE));
            return;
        }
        let r = if ieee { a.ieee_rem(b).value } else { a.c_fmod(b).value };
        let q = (a - r).value.div_r(b, Round::NearestTiesToEven).value;
        let q = q.abs().to_u128(128).value;
        if q & 1 != 0 {
            self.status |= sw::C1;
        }
        if q & 2 != 0 {
            self.status |= sw::C3;
        }
        if q & 4 != 0 {
            self.status |= sw::C0;
        }
        self.set(0, r);
    }

    fn bcd_store(&mut self, v: F80, mem: &mut [u8]) -> usize {
        let r = v.round_to_integral(self.round()).value;
        let n = r.to_i128(70).value;
        if v.is_nan() || n.unsigned_abs() >= 1_000_000_000_000_000_000 {
            // Invalid: the BCD indefinite.
            self.raise(sw::IE);
            mem[..10].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0xC0, 0xFF, 0xFF]);
            return 10;
        }
        let mut m = n.unsigned_abs();
        for b in mem.iter_mut().take(9) {
            let lo = (m % 10) as u8;
            m /= 10;
            let hi = (m % 10) as u8;
            m /= 10;
            *b = hi << 4 | lo;
        }
        mem[9] = if v.is_negative() { 0x80 } else { 0 };
        10
    }

    fn load_env(&mut self, mem: &[u8]) {
        let w = |k: usize| u16::from_le_bytes([mem[k * 2], mem[k * 2 + 1]]);
        self.control = w(0);
        self.status = w(1);
        self.tag = w(2);
        self.last_ip = w(3) as u32 | ((w(4) as u32 >> 12) << 16);
        self.last_opcode = w(4) & 0x7FF;
        self.last_data = w(5) as u32 | ((w(6) as u32 >> 12) << 16);
    }

    /// The environment as FSTENV stores it (Emu8086's FPU window shows it as "control register").
    pub fn env_bytes(&self) -> [u8; 14] {
        let mut b = [0u8; 14];
        self.store_env(&mut b);
        b
    }

    /// The 14-byte real-mode environment.
    fn store_env(&self, mem: &mut [u8]) -> usize {
        let words = [
            self.control,
            self.status,
            self.tag,
            self.last_ip as u16,
            (((self.last_ip >> 16) as u16 & 0xF) << 12) | self.last_opcode,
            self.last_data as u16,
            ((self.last_data >> 16) as u16 & 0xF) << 12,
        ];
        for (k, w) in words.iter().enumerate() {
            mem[k * 2..k * 2 + 2].copy_from_slice(&w.to_le_bytes());
        }
        14
    }
}

#[derive(Clone, Copy)]
enum Mem {
    R32,
    R64,
    R80,
    I16,
    I32,
    I64,
}

fn bcd_load(b: &[u8]) -> F80 {
    let mut n: i128 = 0;
    for k in (0..9).rev() {
        n = n * 100 + ((b[k] >> 4) as i128) * 10 + (b[k] & 0xF) as i128;
    }
    if b[9] & 0x80 != 0 {
        n = -n;
    }
    F80::from_i128(n).value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f64) -> [u8; 10] {
        bytes(from_f64(x).to_bits())
    }

    fn run(fpu: &mut Fpu, op: u8, reg: u8, rm: Option<u8>, mem: &mut [u8]) -> Done {
        fpu.exec(op, reg, rm, mem, Ctx::default())
    }

    #[test]
    fn load_add_store() {
        let mut p = Fpu::new();
        let mut m = 2.5f32.to_le_bytes().to_vec();
        m.resize(94, 0);
        run(&mut p, 0xD9, 0, None, &mut m); // fld dword
        let mut m2 = 4i16.to_le_bytes().to_vec();
        m2.resize(94, 0);
        run(&mut p, 0xDE, 0, None, &mut m2); // fiadd word
        assert_eq!(to_f64(p.st(0)), 6.5);
        let mut out = vec![0u8; 94];
        let d = run(&mut p, 0xDD, 3, None, &mut out); // fstp qword
        assert_eq!(d.write, 8);
        assert_eq!(f64::from_le_bytes(out[..8].try_into().unwrap()), 6.5);
        assert_eq!(p.st_tag(0), Tag::Empty);
    }

    #[test]
    fn stack_and_compare() {
        let mut p = Fpu::new();
        let mut m = [0u8; 94];
        run(&mut p, 0xD9, 5, Some(0), &mut m); // fld1
        run(&mut p, 0xD9, 5, Some(3), &mut m); // fldpi
        assert_eq!(p.top(), 6);
        assert!((to_f64(p.st(0)) - std::f64::consts::PI).abs() < 1e-15);
        run(&mut p, 0xD8, 2, Some(1), &mut m); // fcom st(1): pi > 1
        assert_eq!(p.status & (sw::C0 | sw::C2 | sw::C3), 0);
        run(&mut p, 0xD9, 1, Some(1), &mut m); // fxch
        run(&mut p, 0xD8, 2, Some(1), &mut m); // 1 < pi
        assert_eq!(p.status & (sw::C0 | sw::C2 | sw::C3), sw::C0);
        // fdivp st(1), st: ST1 = ST1 / ST0 = pi / 1
        run(&mut p, 0xDE, 7, Some(1), &mut m);
        assert_eq!(p.top(), 7);
        assert!((to_f64(p.st(0)) - std::f64::consts::PI).abs() < 1e-15);
        // fsubr st, st(0) etc: ST0 = ST0 - ST0 = 0
        run(&mut p, 0xD8, 4, Some(0), &mut m);
        assert_eq!(p.st_tag(0), Tag::Zero);
        // Underflow: reading an empty register gives the indefinite and IE.
        run(&mut p, 0xD8, 0, Some(3), &mut m);
        assert!(p.status & sw::IE != 0 && F80::from_bits(bits(&p.st(0))).is_nan());
    }

    #[test]
    fn integers_rounding_and_sqrt() {
        let mut p = Fpu::new();
        p.regs[7] = f(2.5);
        p.set_top(7);
        p.set_tag(7, Tag::Valid);
        let mut m = [0u8; 94];
        run(&mut p, 0xDF, 2, None, &mut m); // fist word: nearest even -> 2
        assert_eq!(i16::from_le_bytes([m[0], m[1]]), 2);
        p.control |= 0x0C00; // chop
        run(&mut p, 0xD9, 7, None, &mut m); // fstcw
        assert_eq!(u16::from_le_bytes([m[0], m[1]]), 0x0FFF);
        p.set(0, from_f64(2.0));
        run(&mut p, 0xD9, 7, Some(2), &mut m); // fsqrt
        assert_eq!(p.st(0), bytes(0x3FFF_B504_F333_F9DE_6484)); // sqrt(2) truncated
        // Out of range: the integer indefinite.
        p.set(0, from_f64(1e10));
        run(&mut p, 0xDF, 2, None, &mut m);
        assert_eq!(i16::from_le_bytes([m[0], m[1]]), i16::MIN);
        assert!(p.status & sw::IE != 0);
    }

    #[test]
    fn bcd_and_save() {
        let mut p = Fpu::new();
        let mut m = [0u8; 94];
        m[..10].copy_from_slice(&[0x45, 0x23, 0x01, 0, 0, 0, 0, 0, 0, 0x80]);
        run(&mut p, 0xDF, 4, None, &mut m); // fbld: -12345
        assert_eq!(to_f64(p.st(0)), -12345.0);
        let mut out = [0u8; 94];
        run(&mut p, 0xDF, 6, None, &mut out); // fbstp
        assert_eq!(out[..10], m[..10]);
        run(&mut p, 0xD9, 5, Some(3), &mut m); // fldpi
        let before = p.clone();
        let d = run(&mut p, 0xDD, 6, None, &mut out); // fsave
        assert_eq!((d.write, p.top(), p.tag), (94, 0, 0xFFFF));
        run(&mut p, 0xDD, 4, None, &mut out); // frstor
        assert_eq!((p.st(0), p.top(), p.tag), (before.st(0), before.top(), before.tag));
        // fstsw ax
        assert_eq!(run(&mut p, 0xDF, 4, Some(0), &mut m).ax, Some(p.status));
    }
}
