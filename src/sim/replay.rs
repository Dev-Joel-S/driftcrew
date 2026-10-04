//! Unfall-Wiederholung (Punkt 80): Ringpuffer aus Zustands-Schnappschüssen und Eingaben.
//!
//! Weil die Simulation deterministisch ist, reicht ein Schnappschuss alle zwei Sekunden plus
//! die Eingaben jedes Ticks: Schnappschuss laden und dieselben Eingaben noch einmal einspielen
//! ergibt exakt denselben Ablauf – mit denselben Slot-Eingaben, die das HUD dabei anzeigt.

use std::collections::VecDeque;

use super::{SimState, TICK_HZ, TickInput};

/// Abstand der Schnappschüsse und wie weit der Puffer zurückreicht (Ticks).
pub const SNAP_EVERY: u64 = 120;
pub const KEEP_TICKS: u64 = 14 * TICK_HZ as u64;
/// So viele Sekunden zeigt eine Wiederholung mindestens (soweit aufgezeichnet).
pub const REPLAY_SECONDS: f32 = 8.0;

#[derive(Clone, Default)]
pub struct Recorder {
    /// Zustand jeweils vor dem Schritt mit diesem Tick (sim.tick + 1).
    snaps: VecDeque<SimState>,
    /// Eingaben, die zu Tick `n` geführt haben.
    inputs: VecDeque<(u64, TickInput)>,
}

/// Ein Ausschnitt zum Abspielen: Startzustand und die Eingaben danach.
#[derive(Clone)]
pub struct Clip {
    pub start: SimState,
    pub inputs: Vec<TickInput>,
}

impl Recorder {
    pub fn clear(&mut self) {
        self.snaps.clear();
        self.inputs.clear();
    }

    /// Vor jedem Schritt aufrufen: legt bei Bedarf einen Schnappschuss ab.
    pub fn before_step(&mut self, s: &SimState) {
        if s.tick.is_multiple_of(SNAP_EVERY) || self.snaps.is_empty() {
            self.snaps.push_back(s.clone());
        }
        let oldest_wanted = s.tick.saturating_sub(KEEP_TICKS);
        while self.snaps.len() > 1 && self.snaps[1].tick <= oldest_wanted {
            self.snaps.pop_front();
        }
        let first = self.snaps.front().map_or(0, |f| f.tick);
        while self.inputs.front().is_some_and(|(t, _)| *t <= first) {
            self.inputs.pop_front();
        }
    }

    /// Nach jedem Schritt: die Eingabe dieses Ticks merken.
    pub fn after_step(&mut self, s: &SimState, input: &TickInput) {
        self.inputs.push_back((s.tick, input.clone()));
    }

    /// Die letzten (mindestens) `seconds` Sekunden bis jetzt.
    pub fn clip(&self, seconds: f32) -> Option<Clip> {
        let last = self.inputs.back()?.0;
        let want = last.saturating_sub((seconds * TICK_HZ as f32) as u64);
        // Jüngster Schnappschuss, der weit genug zurückliegt – sonst der älteste.
        let start = self
            .snaps
            .iter()
            .rev()
            .find(|s| s.tick <= want)
            .or_else(|| self.snaps.front())?;
        let inputs: Vec<TickInput> = self
            .inputs
            .iter()
            .filter(|(t, _)| *t > start.tick)
            .map(|(_, i)| i.clone())
            .collect();
        if inputs.is_empty() {
            return None;
        }
        Some(Clip {
            start: start.clone(),
            inputs,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy::math::Vec2;

    use super::*;
    use crate::sim::data::{CrewSave, GameData};
    use crate::sim::ship::Loadout;

    fn sim() -> SimState {
        let data = Arc::new(GameData::embedded().unwrap());
        let save = CrewSave::new_game(&data);
        let def = data.ship(&save.current_ship).clone();
        SimState::new(data, &save, Loadout::full(&def), 2)
    }

    #[test]
    fn replaying_a_clip_reproduces_the_flight_exactly() {
        let mut s = sim();
        let mut rec = Recorder::default();
        // Abdocken und wild herumfliegen, bis etwas kaputtgeht oder 20 s vorbei sind.
        for i in 0..1200u32 {
            let slots = match (i / 40) % 4 {
                0 => 0b11,
                1 => 0b01,
                2 => 0b10,
                _ => 0b11,
            };
            let input = TickInput {
                slots,
                ..Default::default()
            };
            rec.before_step(&s);
            s.step(&input);
            rec.after_step(&s, &input);
        }
        let clip = rec.clip(REPLAY_SECONDS).expect("Ausschnitt vorhanden");
        let secs = clip.inputs.len() as f32 / TICK_HZ as f32;
        assert!(
            (REPLAY_SECONDS..=REPLAY_SECONDS + 2.1).contains(&secs),
            "{secs} s"
        );
        // Abspielen: gleicher Endzustand bis aufs Bit.
        let mut r = clip.start.clone();
        for i in &clip.inputs {
            r.step(i);
        }
        assert_eq!(r.tick, s.tick);
        assert_eq!(r.ship.pos, s.ship.pos);
        assert_eq!(r.ship.angle, s.ship.angle);
        assert_eq!(r.ship.hull, s.ship.hull);
        assert_eq!(r.bodies.len(), s.bodies.len());
        assert_ne!(s.ship.pos, Vec2::ZERO);
    }

    #[test]
    fn buffer_stays_bounded() {
        let mut s = sim();
        let mut rec = Recorder::default();
        for _ in 0..(40 * TICK_HZ as u32) {
            let input = TickInput::default();
            rec.before_step(&s);
            s.step(&input);
            rec.after_step(&s, &input);
        }
        assert!(rec.snaps.len() <= (KEEP_TICKS / SNAP_EVERY) as usize + 2);
        assert!(rec.inputs.len() as u64 <= KEEP_TICKS + SNAP_EVERY);
    }
}
