//! Deterministischer Zufall (PCG32). Kein Zufall ohne Seed – wichtig für späteres Rollback.

use super::fx::Fx;

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng {
            state: 0,
            inc: (seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) << 1) | 1,
        };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Gleichverteilt in [0, 1): die 32 Zufallsbits sind genau die Nachkommastellen.
    pub fn unit(&mut self) -> Fx {
        Fx::from_raw(self.next_u32() as i64)
    }

    pub fn range(&mut self, lo: Fx, hi: Fx) -> Fx {
        lo + (hi - lo) * self.unit()
    }

    pub fn range_u32(&mut self, lo: u32, hi_inclusive: u32) -> u32 {
        if hi_inclusive <= lo {
            return lo;
        }
        lo + self.next_u32() % (hi_inclusive - lo + 1)
    }

    pub fn index(&mut self, len: usize) -> usize {
        if len == 0 {
            0
        } else {
            (self.next_u32() as usize) % len
        }
    }

    pub fn chance(&mut self, p: Fx) -> bool {
        self.unit() < p
    }
}

/// Stateless Hash für Varianten (z. B. Optik pro Objekt).
pub fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn f32_in_unit_range() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let x = r.unit();
            assert!((fx!(0.0)..fx!(1.0)).contains(&x));
        }
    }
}
