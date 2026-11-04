//! Coprocessor 1 (floating point) as MARS simulates it: 32 registers of 32 bits, a double in an
//! even register and the next one (low word first), 8 condition flags, and the instructions of
//! its `InstructionSet.java` with Java's float and double arithmetic.
//!
//! What MARS does that a real FPU would not, kept here: results are written with
//! `Float.floatToIntBits` (any NaN becomes the canonical `0x7fc00000`, `0x7ff80000_00000000` for a
//! double); `floor`, `ceil`, `trunc` and `round` to a word give `0x7fffffff` for NaN, an infinity
//! or a value out of range, `round` rounds a half to the even neighbour; `cvt.w` saturates (Java's
//! cast); the square root of a negative number is NaN; there are no FPU exceptions and no FCSR.
//! An odd register in a double operation stops the program with MARS's message ("all registers
//! must be even-numbered") without an exception code.

use crate::cpu::{Cpu, ExcCode, Fault, Fields, access, reserved};
use crate::memory::{Memory, hex};

/// `Float.floatToIntBits`: the bits of `x`, a NaN as the canonical one.
pub fn f32_bits(x: f32) -> u32 {
    if x.is_nan() { 0x7fc0_0000 } else { x.to_bits() }
}

/// `Double.doubleToLongBits`: the bits of `x`, a NaN as the canonical one.
pub fn f64_bits(x: f64) -> u64 {
    if x.is_nan() { 0x7ff8_0000_0000_0000 } else { x.to_bits() }
}

/// A runtime error MARS reports without an exception code (see [`ExcCode::Runtime`]).
fn runtime(message: &str) -> Fault {
    (ExcCode::Runtime, message.into(), None)
}

/// MARS's conversion of a float or a double to a word by `rounded` (floor, ceil, truncation),
/// with `0x7fffffff` for NaN, an infinity or a value outside the 32-bit range.
fn to_word(x: f64, rounded: f64) -> u32 {
    if x.is_nan() || x.is_infinite() || x < i32::MIN as f64 || x > i32::MAX as f64 {
        i32::MAX as u32
    } else {
        rounded as i32 as u32
    }
}

/// `round.w`: to the nearest, a half to the even neighbour (MARS since 3.5). `half` tells whether
/// `x` is exactly between two integers, computed as MARS does in the operand's precision.
fn round_word(x: f64, half: impl Fn(i32, i32) -> bool) -> u32 {
    if x.is_nan() || x.is_infinite() || x < i32::MIN as f64 || x > i32::MAX as f64 {
        return i32::MAX as u32;
    }
    // The nearest integer; a half is decided below, as MARS does.
    let mut round = x.round() as i32;
    let t = x.trunc() as i32;
    let (below, above) = if x < 0.0 { (t.wrapping_sub(1), t) } else { (t, t.wrapping_add(1)) };
    if half(below, above) {
        round = if above % 2 == 0 { above } else { below };
    }
    round as u32
}

impl Cpu {
    fn f32(&self, r: usize) -> f32 {
        f32::from_bits(self.fpr[r])
    }

    fn set_f32(&mut self, r: usize, x: f32) {
        self.fpr[r] = f32_bits(x);
    }

    /// The double in `r` and `r + 1` (the high word in `r + 1`).
    pub fn f64(&self, r: usize) -> f64 {
        f64::from_bits((self.fpr[r + 1] as u64) << 32 | self.fpr[r] as u64)
    }

    fn set_f64(&mut self, r: usize, x: f64) {
        let b = f64_bits(x);
        self.fpr[r] = b as u32;
        self.fpr[r + 1] = (b >> 32) as u32;
    }

    /// Condition flag `cc` (0..7).
    pub fn fcc(&self, cc: usize) -> bool {
        self.fcc >> cc & 1 != 0
    }

    fn set_fcc(&mut self, cc: usize, v: bool) {
        self.fcc = (self.fcc & !(1 << cc)) | ((v as u8) << cc);
    }

    /// `movf` / `movt` between general registers (opcode 0, function 1): `rd = rs` when
    /// condition flag `cc` is `tf`.
    pub(crate) fn movf_gpr(&mut self, f: &Fields) -> Result<(), Fault> {
        if f.rt & 2 != 0 || f.shamt != 0 {
            return Err(reserved(f.word));
        }
        if self.fcc(f.rt >> 2) == (f.rt & 1 == 1) {
            self.set_reg(f.rd, self.reg(f.rs));
        }
        Ok(())
    }

    /// `lwc1`, `ldc1`, `swc1`, `sdc1` (opcodes 0x31, 0x35, 0x39, 0x3D) at `addr`.
    pub(crate) fn fpu_memory(&mut self, mem: &mut Memory, f: &Fields, addr: u32) -> Result<(), Fault> {
        let ft = f.rt;
        let op = f.word >> 26;
        let double = op == 0x35 || op == 0x3D;
        if double {
            if ft % 2 == 1 {
                return Err(runtime("first register must be even-numbered"));
            }
            if !addr.is_multiple_of(8) {
                let code = if op == 0x35 { ExcCode::AddressLoad } else { ExcCode::AddressStore };
                return Err((code, format!("address not aligned on doubleword boundary {}", hex(addr)), Some(addr)));
            }
        }
        match op {
            0x31 => self.fpr[ft] = mem.load32(addr).map_err(access)?,
            0x35 => {
                self.fpr[ft] = mem.load32(addr).map_err(access)?;
                self.fpr[ft + 1] = mem.load32(addr.wrapping_add(4)).map_err(access)?;
            }
            0x39 => mem.store32(addr, self.fpr[ft]).map_err(access)?,
            _ => {
                mem.store32(addr, self.fpr[ft]).map_err(access)?;
                mem.store32(addr.wrapping_add(4), self.fpr[ft + 1]).map_err(access)?;
            }
        }
        Ok(())
    }

    /// Opcode 0x11: moves between the register files, branches on a condition flag and the
    /// operations of formats S (single), D (double) and W (word).
    pub(crate) fn cop1(&mut self, f: &Fields) -> Result<(), Fault> {
        let w = f.word;
        let (fmt, ft, fs, fd, funct) = (f.rs, f.rt, f.rd, f.shamt as usize, w & 0x3F);
        let low = w & 0x7FF;
        match fmt {
            // mfc1 / mtc1
            0 if low == 0 => self.set_reg(ft, self.fpr[fs]),
            4 if low == 0 => self.fpr[fs] = self.reg(ft),
            // bc1f / bc1t
            8 if ft & 2 == 0 => {
                if self.fcc(ft >> 2) == (ft & 1 == 1) {
                    self.jump(self.pc.wrapping_add(f.simm << 2));
                }
            }
            16 => self.single(f, ft, fs, fd, funct)?,
            17 => self.double(f, ft, fs, fd, funct)?,
            20 if ft == 0 && funct == 0x20 => self.set_f32(fd, self.fpr[fs] as i32 as f32),
            20 if ft == 0 && funct == 0x21 => {
                even(&[fd], "first register must be even-numbered")?;
                self.set_f64(fd, self.fpr[fs] as i32 as f64);
            }
            _ => return Err(reserved(w)),
        }
        Ok(())
    }

    fn single(&mut self, f: &Fields, ft: usize, fs: usize, fd: usize, funct: u32) -> Result<(), Fault> {
        let (a, b) = (self.f32(fs), self.f32(ft));
        // Instructions with one operand have 0 in the ft field.
        let unary = matches!(funct, 4..=7 | 0x0C..=0x0F | 0x21 | 0x24);
        if unary && ft != 0 {
            return Err(reserved(f.word));
        }
        match funct {
            0 => self.set_f32(fd, a + b),
            1 => self.set_f32(fd, a - b),
            2 => self.set_f32(fd, a * b),
            3 => self.set_f32(fd, a / b),
            4 => self.set_f32(fd, if a < 0.0 { f32::NAN } else { (a as f64).sqrt() as f32 }),
            5 => self.fpr[fd] = self.fpr[fs] & 0x7FFF_FFFF,
            6 => self.fpr[fd] = self.fpr[fs],
            7 => self.fpr[fd] = self.fpr[fs] ^ 0x8000_0000,
            0x0C => self.fpr[fd] = round_word(a as f64, |lo, hi| a - lo as f32 == hi as f32 - a),
            0x0D => self.fpr[fd] = to_word(a as f64, (a as f64).trunc()),
            0x0E => self.fpr[fd] = to_word(a as f64, (a as f64).ceil()),
            0x0F => self.fpr[fd] = to_word(a as f64, (a as f64).floor()),
            0x11 if ft & 2 == 0 => {
                if self.fcc(ft >> 2) == (ft & 1 == 1) {
                    self.fpr[fd] = self.fpr[fs];
                }
            }
            0x12 if self.reg(ft) == 0 => self.fpr[fd] = self.fpr[fs],
            0x13 if self.reg(ft) != 0 => self.fpr[fd] = self.fpr[fs],
            0x12 | 0x13 => {}
            0x21 => {
                even(&[fd], "first register must be even-numbered")?;
                self.set_f64(fd, a as f64);
            }
            0x24 => self.fpr[fd] = a as i32 as u32,
            0x32 | 0x3C | 0x3E if fd & 3 == 0 => {
                let v = match funct {
                    0x32 => a == b,
                    0x3C => a < b,
                    _ => a <= b,
                };
                self.set_fcc(fd >> 2, v);
            }
            _ => return Err(reserved(f.word)),
        }
        Ok(())
    }

    fn double(&mut self, f: &Fields, ft: usize, fs: usize, fd: usize, funct: u32) -> Result<(), Fault> {
        let unary = matches!(funct, 4..=7 | 0x0C..=0x0F | 0x20 | 0x24);
        if unary && ft != 0 {
            return Err(reserved(f.word));
        }
        match funct {
            0..=3 => {
                even(&[fd, fs, ft], "all registers must be even-numbered")?;
                let (a, b) = (self.f64(fs), self.f64(ft));
                let r = match funct {
                    0 => a + b,
                    1 => a - b,
                    2 => a * b,
                    _ => a / b,
                };
                self.set_f64(fd, r);
            }
            4 => {
                even(&[fd, fs], "both registers must be even-numbered")?;
                let a = self.f64(fs);
                self.set_f64(fd, if a < 0.0 { f64::NAN } else { a.sqrt() });
            }
            5..=7 => {
                even(&[fd, fs], "both registers must be even-numbered")?;
                let hi = self.fpr[fs + 1];
                self.fpr[fd] = self.fpr[fs];
                self.fpr[fd + 1] = match funct {
                    5 => hi & 0x7FFF_FFFF,
                    6 => hi,
                    _ => hi ^ 0x8000_0000,
                };
            }
            0x0C..=0x0F | 0x20 | 0x24 => {
                even(&[fs], "second register must be even-numbered")?;
                let a = self.f64(fs);
                self.fpr[fd] = match funct {
                    0x0C => round_word(a, |lo, hi| a - lo as f64 == hi as f64 - a),
                    0x0D => to_word(a, a.trunc()),
                    0x0E => to_word(a, a.ceil()),
                    0x0F => to_word(a, a.floor()),
                    0x20 => f32_bits(a as f32),
                    _ => a as i32 as u32,
                };
            }
            0x11..=0x13 => {
                if funct == 0x11 && ft & 2 != 0 {
                    return Err(reserved(f.word));
                }
                even(&[fd, fs], "both registers must be even-numbered")?;
                let take = match funct {
                    0x11 => self.fcc(ft >> 2) == (ft & 1 == 1),
                    0x12 => self.reg(ft) == 0,
                    _ => self.reg(ft) != 0,
                };
                if take {
                    self.fpr[fd] = self.fpr[fs];
                    self.fpr[fd + 1] = self.fpr[fs + 1];
                }
            }
            0x32 | 0x3C | 0x3E if fd & 3 == 0 => {
                even(&[fs, ft], "both registers must be even-numbered")?;
                let (a, b) = (self.f64(fs), self.f64(ft));
                let v = match funct {
                    0x32 => a == b,
                    0x3C => a < b,
                    _ => a <= b,
                };
                self.set_fcc(fd >> 2, v);
            }
            _ => return Err(reserved(f.word)),
        }
        Ok(())
    }
}

/// MARS's check that the registers of a double operation are even.
fn even(regs: &[usize], message: &str) -> Result<(), Fault> {
    if regs.iter().any(|r| r % 2 == 1) { Err(runtime(message)) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_as_mars() {
        let r = |x: f32| round_word(x as f64, |lo, hi| x - lo as f32 == hi as f32 - x) as i32;
        assert_eq!([r(4.5), r(5.5), r(-4.5), r(-5.5), r(-5.1), r(-5.6), r(2.4)], [4, 6, -4, -6, -5, -6, 2]);
        assert_eq!(round_word(f64::NAN, |_, _| false), 0x7FFF_FFFF);
        assert_eq!(to_word(3e9, 3e9), 0x7FFF_FFFF);
        assert_eq!(to_word(-2.5, (-2.5f64).floor()) as i32, -3);
        assert_eq!(f32_bits(-f32::NAN), 0x7fc0_0000);
    }
}
