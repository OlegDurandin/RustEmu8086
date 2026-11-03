//! `java.util.Random`, which MARS's random number services (syscalls 40..44) use: with the same
//! seed (syscall 40) a program gets the same numbers as in MARS.

/// A linear congruential generator with Java's constants and algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JavaRandom {
    seed: u64,
}

const MULTIPLIER: u64 = 0x5_DEEC_E66D;
const MASK: u64 = (1 << 48) - 1;

impl JavaRandom {
    /// `new Random(seed)`.
    pub fn new(seed: i64) -> JavaRandom {
        let mut r = JavaRandom { seed: 0 };
        r.set_seed(seed);
        r
    }

    /// `setSeed(seed)`.
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = (seed as u64 ^ MULTIPLIER) & MASK;
    }

    /// `next(bits)`: the next `bits` (1..=32) random bits.
    fn next(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(MULTIPLIER).wrapping_add(0xB)) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// `nextFloat()`: a float in `0.0..1.0` (24 random bits).
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1u32 << 24) as f32
    }

    /// `nextDouble()`: a double in `0.0..1.0` (53 random bits).
    pub fn next_double(&mut self) -> f64 {
        let hi = (self.next(26) as i64) << 27;
        (hi + self.next(27) as i64) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// `nextInt()`: any 32-bit value.
    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// `nextInt(bound)`: a value in `0..bound`; None for a bound that is not positive (Java throws).
    pub fn next_int_below(&mut self, bound: i32) -> Option<i32> {
        if bound <= 0 {
            return None;
        }
        if (bound as u32).is_power_of_two() {
            return Some(((bound as i64 * self.next(31) as i64) >> 31) as i32);
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits.wrapping_sub(val).wrapping_add(bound - 1) >= 0 {
                return Some(val);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_numbers_as_java() {
        // Values printed by jshell (JDK 24) for the same calls.
        let mut r = JavaRandom::new(42);
        assert_eq!([r.next_int(), r.next_int(), r.next_int()], [-1170105035, 234785527, -1360544799]);
        let mut r = JavaRandom::new(42);
        assert_eq!([r.next_int_below(10), r.next_int_below(10), r.next_int_below(10)], [Some(0), Some(3), Some(8)]);
        assert_eq!(JavaRandom::new(42).next_int_below(16), Some(11));
        assert_eq!(JavaRandom::new(1).next_int_below(0), None);
        let mut r = JavaRandom::new(-7);
        assert_eq!([r.next_int_below(1_000_000_007), r.next_int_below(3)], [Some(577934662), Some(0)]);
        assert_eq!(r.next_int(), -1403676116);
    }
}
