//! Zufall für Darstellung und Ton (Partikel, Texturen, Rauschen) – mit Float-Ergebnissen.
//! Die Simulation nutzt ihn nie: dort gilt `sim::rng::Rng` mit Fixed-Point.

use crate::sim::rng::Rng;

#[derive(Clone, Debug)]
pub struct Prng(Rng);

#[allow(dead_code)]
impl Prng {
    pub fn new(seed: u64) -> Self {
        Prng(Rng::new(seed))
    }

    /// Gleichverteilt in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.0.next_u32() >> 8) as f32 / 16_777_216.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    pub fn index(&mut self, len: usize) -> usize {
        self.0.index(len)
    }

    pub fn range_u32(&mut self, lo: u32, hi_inclusive: u32) -> u32 {
        self.0.range_u32(lo, hi_inclusive)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
}
