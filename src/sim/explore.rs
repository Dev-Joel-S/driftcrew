//! Fog of War: ein grobes Raster über die Welt, das beim Fliegen aufgedeckt wird.
//! Wird im Spielstand als Hex-Bitfeld gespeichert.

use bevy::math::Vec2;

/// Kantenlänge einer Rasterzelle in Metern.
pub const CELL: f32 = 100.0;
/// So weit reicht die Sicht der Crew beim Erkunden.
pub const SIGHT: f32 = 380.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Exploration {
    /// Zellen pro Kante; das Raster deckt [-half, half]² ab.
    pub n: usize,
    pub half: f32,
    bits: Vec<u64>,
    /// Zählt Änderungen (für die Anzeige: Karte nur neu zeichnen, wenn sich etwas tat).
    pub version: u32,
}

impl Exploration {
    pub fn new(world_radius: f32) -> Exploration {
        let half = world_radius + CELL;
        let n = ((2.0 * half) / CELL).ceil() as usize;
        Exploration {
            n,
            half,
            bits: vec![0; (n * n).div_ceil(64)],
            version: 0,
        }
    }

    fn cell_of(&self, p: Vec2) -> Option<(usize, usize)> {
        let x = ((p.x + self.half) / CELL).floor();
        let y = ((p.y + self.half) / CELL).floor();
        if x < 0.0 || y < 0.0 || x >= self.n as f32 || y >= self.n as f32 {
            return None;
        }
        Some((x as usize, y as usize))
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        let i = y * self.n + x;
        (self.bits[i / 64] >> (i % 64)) & 1 == 1
    }

    fn set(&mut self, x: usize, y: usize) -> bool {
        let i = y * self.n + x;
        let mask = 1u64 << (i % 64);
        let was = self.bits[i / 64] & mask != 0;
        self.bits[i / 64] |= mask;
        !was
    }

    pub fn is_explored(&self, p: Vec2) -> bool {
        self.cell_of(p).is_some_and(|(x, y)| self.get(x, y))
    }

    /// Mittelpunkt einer Zelle in Weltkoordinaten.
    pub fn cell_center(&self, x: usize, y: usize) -> Vec2 {
        Vec2::new(
            -self.half + (x as f32 + 0.5) * CELL,
            -self.half + (y as f32 + 0.5) * CELL,
        )
    }

    /// Alle Zellen aufdecken, deren Mitte im Kreis liegt. Gibt die Zahl neuer Zellen zurück.
    pub fn reveal(&mut self, p: Vec2, radius: f32) -> u32 {
        let r = (radius / CELL).ceil() as i32 + 1;
        let Some((cx, cy)) =
            self.cell_of(p.clamp(Vec2::splat(-self.half + 1.0), Vec2::splat(self.half - 1.0)))
        else {
            return 0;
        };
        let mut new = 0;
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx as i32 + dx, cy as i32 + dy);
                if x < 0 || y < 0 || x >= self.n as i32 || y >= self.n as i32 {
                    continue;
                }
                let (x, y) = (x as usize, y as usize);
                if (self.cell_center(x, y) - p).length() <= radius && self.set(x, y) {
                    new += 1;
                }
            }
        }
        if new > 0 {
            self.version = self.version.wrapping_add(1);
        }
        new
    }

    /// Anteil der aufgedeckten Fläche (0..1).
    pub fn fraction(&self) -> f32 {
        let set: u32 = self.bits.iter().map(|b| b.count_ones()).sum();
        set as f32 / (self.n * self.n) as f32
    }

    pub fn to_hex(&self) -> String {
        self.bits.iter().map(|b| format!("{b:016x}")).collect()
    }

    pub fn load_hex(&mut self, s: &str) {
        for (i, chunk) in s.as_bytes().chunks(16).enumerate() {
            if i >= self.bits.len() {
                break;
            }
            if let Ok(t) = std::str::from_utf8(chunk)
                && let Ok(v) = u64::from_str_radix(t, 16)
            {
                self.bits[i] = v;
            }
        }
        self.version = self.version.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_and_roundtrip() {
        let mut e = Exploration::new(2600.0);
        assert!(!e.is_explored(Vec2::ZERO));
        e.reveal(Vec2::new(100.0, -50.0), SIGHT);
        assert!(e.is_explored(Vec2::new(100.0, -50.0)));
        assert!(e.is_explored(Vec2::new(300.0, -50.0)));
        assert!(!e.is_explored(Vec2::new(900.0, 0.0)));
        let mut f = Exploration::new(2600.0);
        f.load_hex(&e.to_hex());
        assert_eq!(e.bits, f.bits);
        assert!(f.fraction() > 0.0 && f.fraction() < 0.05);
    }
}
