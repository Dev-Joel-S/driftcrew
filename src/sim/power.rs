//! Zusammenarbeit am Schiff (Punkte 69, 70): Triebwerks-Übersteuerung mit Hitze und eine
//! gemeinsame Zusatzenergie.
//!
//! * Übersteuern: Slot-Taste **zweimal tippen und gedrückt halten** – das ist die eigene
//!   Eingabe, normales langes Drücken übersteuert nie. +60 % Schub, aber das Triebwerk heizt auf.
//!   Bei 65 % und 88 % Hitze kommen Warnungen, bei 100 % schaltet es 4 s lang ab. Jeder
//!   Triebwerks-Slot hat seine eigene Hitze.
//! * Zusatzenergie: Übersteuern, Schnellladen des Schilds und Werkzeug-Boost (Bohrer schneller,
//!   Kran holt schneller ein) ziehen aus demselben Speicher. Ist er leer, fällt nur der Bonus
//!   weg – Schub, Schild und Werkzeuge funktionieren normal weiter. Er lädt, wenn nichts zieht.

use super::ship::Thruster;
use super::thruster_label as sim_label;
use super::{DT, SimEvent, SimState, ToastKind};

pub const OVERDRIVE_MUL: f32 = 1.6;
/// Ein Tipp ist höchstens so lang, die Pause bis zum zweiten Druck höchstens so lang (Ticks).
pub const TAP_TICKS: u32 = 15;
pub const TAP_GAP: u32 = 18;
pub const HEAT_RATE: f32 = 0.22;
pub const COOL_RATE: f32 = 0.16;
pub const COOL_OFF: f32 = 4.0;
pub const ENERGY_MAX: f32 = 100.0;
pub const ENERGY_REGEN: f32 = 7.0;
/// Verbrauch pro Sekunde: je übersteuertem Triebwerk, Schild-Schnellladen, Werkzeug-Boost.
pub const DRAW_OVERDRIVE: f32 = 9.0;
pub const DRAW_SHIELD: f32 = 8.0;
pub const DRAW_TOOL: f32 = 6.0;
/// Werkzeug-Boost: Bohrer und Kran so viel schneller.
pub const TOOL_BOOST: f32 = 1.5;

/// Doppeltipp-und-Halten erkennen. Muss jeden Tick mit dem Tastenzustand laufen.
pub fn track_taps(t: &mut Thruster, pressed: bool) {
    if pressed {
        if t.held == 0 {
            t.overdrive = t.tap && t.since_release <= TAP_GAP && t.cool_off <= 0.0;
        }
        t.held = t.held.saturating_add(1);
    } else {
        if t.held > 0 {
            t.tap = t.held <= TAP_TICKS;
            t.since_release = 0;
        } else {
            t.since_release = t.since_release.saturating_add(1);
        }
        t.held = 0;
        t.overdrive = false;
    }
}

impl SimState {
    /// Ist gerade Zusatzenergie da?
    pub fn energy_ok(&self) -> bool {
        self.ship.energy > 0.5
    }

    /// Ein Verbraucher meldet seinen Bedarf für diesen Tick (true = Bonus gewährt).
    pub(crate) fn draw_energy(&mut self, per_second: f32) -> bool {
        if !self.energy_ok() {
            return false;
        }
        self.power_draw += per_second;
        true
    }

    /// Schubfaktor eines Triebwerks (Übersteuerung nur mit Energie).
    pub fn thrust_factor(&self, t: &Thruster) -> f32 {
        if t.overdrive && t.cool_off <= 0.0 && self.energy_ok() {
            OVERDRIVE_MUL
        } else {
            1.0
        }
    }

    /// Jeder Tick, nach allen Verbrauchern: Hitze, Warnungen, Abschaltung, Energie.
    pub(crate) fn update_power(&mut self) {
        let energy_ok = self.energy_ok();
        let n = self.ship.thrusters.len();
        let mut msgs: Vec<(String, ToastKind, bool)> = Vec::new();
        let mut od = 0;
        for (i, t) in self.ship.thrusters.iter_mut().enumerate() {
            t.cool_off = (t.cool_off - DT).max(0.0);
            let hot = t.overdrive && t.firing && energy_ok && t.cool_off <= 0.0;
            if hot {
                od += 1;
                t.heat += HEAT_RATE * DT;
            } else {
                t.heat = (t.heat - COOL_RATE * DT).max(0.0);
            }
            let name = sim_label(i, n);
            if t.heat >= 1.0 {
                t.heat = 1.0;
                t.cool_off = COOL_OFF;
                t.overdrive = false;
                t.firing = false;
                msgs.push((
                    format!("{name}: Notabschaltung – {COOL_OFF:.0} s Pause zum Abkühlen"),
                    ToastKind::Bad,
                    true,
                ));
            } else if t.heat > 0.88 && t.heat_warn < 2 {
                t.heat_warn = 2;
                msgs.push((
                    format!("{name}: kurz vor der Notabschaltung – loslassen!"),
                    ToastKind::Bad,
                    true,
                ));
            } else if t.heat > 0.65 && t.heat_warn < 1 {
                t.heat_warn = 1;
                msgs.push((
                    format!("{name} wird heiß – Übersteuerung bald lösen"),
                    ToastKind::Warn,
                    false,
                ));
            } else if t.heat < 0.4 {
                t.heat_warn = 0;
            }
        }
        if od > 0 {
            self.power_draw += DRAW_OVERDRIVE * od as f32;
        }
        for (text, kind, alarm) in msgs {
            if alarm {
                self.events.push(SimEvent::Alarm);
            }
            self.toast(text, kind);
        }
        let draw = std::mem::take(&mut self.power_draw);
        let e = &mut self.ship.energy;
        if draw > 0.0 {
            *e = (*e - draw * DT).max(0.0);
        } else {
            *e = (*e + ENERGY_REGEN * DT).min(self.ship.max_energy);
        }
        if *e <= 0.0 && draw > 0.0 && !self.energy_warned {
            self.energy_warned = true;
            self.toast(
                "Zusatzenergie leer – Übersteuern, Schild-Schnellladen und Werkzeug-Boost ruhen, alles andere geht",
                ToastKind::Warn,
            );
        } else if *e > self.ship.max_energy * 0.5 {
            self.energy_warned = false;
        }
    }
}
