//! Minispiele für einzelne Slots (Punkte 35, 38, 72): Reparatur im Takt, Andockport hacken,
//! Notreparatur mit dem Kran. Alles läuft in der Simulation und wird nur mit Slot-Tasten
//! gespielt – deterministisch, damit es später auch online geht.

use crate::sim::fx::{Fx, Vec2};

use super::ship::CargoKind;
use super::world::angle_diff;
use super::{DT, SimEvent, SimState, TickInput, ToastKind};

/// Takt in Sekunden und wie weit daneben ein Druck noch zählt.
pub const BEAT: Fx = fx!(0.7);
pub const BEAT_WINDOW: Fx = fx!(0.14);
/// Treffer, bis ein ausgefallenes Triebwerk wieder läuft.
pub const PATCH_HITS: Fx = fx!(8.0);
/// Zustand eines notdürftig geflickten Triebwerks (läuft, stottert aber).
pub const PATCH_HEALTH: Fx = fx!(0.45);
/// Notreparatur der Hülle: Punkte pro Treffer, höchstens bis zu diesem Anteil.
pub const HULL_PER_HIT: Fx = fx!(2.0);
pub const HULL_PATCH_CAP: Fx = fx!(0.75);
/// So lange ruhig und ohne Eingabe, dann beginnt die Notreparatur.
pub const REST_TO_REPAIR: Fx = fx!(3.0);
/// Eine Taste so lange halten beendet die Notreparatur (die Taste wirkt dann normal).
pub const HOLD_TO_LEAVE: Fx = fx!(0.45);
/// Kran-Notreparatur: so lange muss das Ersatzteil ruhig an der Stelle sitzen.
pub const CRANE_PATCH_TIME: Fx = fx!(2.5);
/// Kran-Notreparatur: so genau muss der Kran zielen (Grad).
pub const CRANE_PATCH_AIM: Fx = fx!(15.0);

/// Rückmeldung zu einem Tastendruck im Takt (für die Anzeige).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub slot: u8,
    pub hit: bool,
    pub age: Fx,
}

/// Notreparatur und Takt-Reparatur.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repair {
    /// Notreparatur läuft (Schiff ruht, Slot-Tasten flicken statt zu steuern).
    pub active: bool,
    /// Wie lange das Schiff schon ruhig und ohne Eingabe ist.
    pub rest: Fx,
    /// Wie lange jede Taste schon gehalten wird.
    pub held: Vec<Fx>,
    /// Fortschritt pro Triebwerk (Index wie `ship.thrusters`), 0..1.
    pub thrusters: Vec<Fx>,
    pub beats: Vec<Beat>,
}

/// Laufender Hack eines Andockports.
#[derive(Clone, Debug, PartialEq)]
pub struct Hack {
    pub station: usize,
    /// Slots in der Reihenfolge, in der sie gedrückt werden müssen.
    pub pattern: Vec<u8>,
    pub done: usize,
    pub left: Fx,
    pub total: Fx,
}

/// Abstand zum nächsten Taktschlag (Sekunden).
pub fn beat_offset(time: Fx) -> Fx {
    let phase = time.rem_euclid(BEAT);
    phase.min(BEAT - phase)
}

impl SimState {
    /// Liegt dieser Zeitpunkt im Takt?
    pub fn on_beat(&self) -> bool {
        beat_offset(self.time) <= BEAT_WINDOW
    }

    /// Phase im Takt (0..1, 0 = Schlag) – für die Anzeige.
    pub fn beat_phase(&self) -> Fx {
        self.time.rem_euclid(BEAT) / BEAT
    }

    /// Braucht die Hülle eine Notreparatur?
    pub fn hull_needs_patch(&self) -> bool {
        self.ship.hull < self.ship.max_hull * HULL_PATCH_CAP - fx!(0.5)
    }

    /// Minispiele vor der Steuerung auswerten. Ergebnis: Slot-Tasten sind in diesem Tick
    /// Eingabe für ein Minispiel und steuern das Schiff nicht.
    pub(crate) fn update_minigames(&mut self, input: &TickInput) -> bool {
        let edges = input.slots & !self.prev_slots;
        self.prev_slots = input.slots;
        let n = self.ship.thrusters.len();
        if self.repair.thrusters.len() != n {
            self.repair.thrusters = vec![fx!(0.0); n];
        }
        if self.repair.held.len() != super::MAX_SLOTS {
            self.repair.held = vec![fx!(0.0); super::MAX_SLOTS];
        }
        for b in &mut self.repair.beats {
            b.age += DT;
        }
        self.repair.beats.retain(|b| b.age < fx!(1.2));
        if self.ship.docked.is_some() || self.ship.destroyed || self.vote.is_some() {
            self.repair.active = false;
            self.repair.rest = fx!(0.0);
            self.hack = None;
            return false;
        }
        if self.hack.is_some() {
            self.update_hack(edges);
            return true;
        }
        self.try_start_hack();
        if self.hack.is_some() {
            return true;
        }
        self.update_repair(input, edges)
    }

    fn rhythm(&mut self, slot: u8) -> bool {
        let hit = self.on_beat();
        self.repair.beats.push(Beat {
            slot,
            hit,
            age: fx!(0.0),
        });
        self.events.push(SimEvent::Beat { hit });
        hit
    }

    /// Takt-Reparatur eines ausgefallenen Triebwerks (geht auch im Flug: die Taste ist tot).
    fn patch_thruster(&mut self, ti: usize) {
        let slot = self.ship.thrusters[ti].slot;
        let hit = self.rhythm(slot);
        let p = &mut self.repair.thrusters[ti];
        *p = if hit {
            *p + fx!(1.0) / PATCH_HITS
        } else {
            (*p - fx!(0.5) / PATCH_HITS).max(fx!(0.0))
        };
        if *p >= fx!(1.0) - fx!(1e-4) {
            *p = fx!(0.0);
            self.finish_thruster_patch(ti, "im Takt geflickt");
        }
    }

    pub(crate) fn finish_thruster_patch(&mut self, ti: usize, how: &str) {
        let n = self.ship.thrusters.len();
        self.ship.thrusters[ti].health = PATCH_HEALTH;
        let label = super::thruster_label(ti, n);
        self.events.push(SimEvent::Patched {
            slot: self.ship.thrusters[ti].slot,
        });
        self.toast(
            format!("Triebwerk {label} {how} – läuft wieder, aber nicht rund"),
            ToastKind::Good,
        );
    }

    fn update_repair(&mut self, input: &TickInput, edges: u32) -> bool {
        let speed = self.ship.vel.length();
        let calm = speed < fx!(0.6) && self.ship.ang_vel.abs() < fx!(0.4);
        if !self.repair.active {
            // Ausgefallene Triebwerke lassen sich jederzeit im Takt flicken.
            for ti in 0..self.ship.thrusters.len() {
                let t = &self.ship.thrusters[ti];
                if t.failed() && edges >> t.slot & 1 == 1 {
                    self.patch_thruster(ti);
                }
            }
            // Ruhig und niemand drückt etwas: die Notreparatur beginnt.
            if self.hull_needs_patch() && calm && input.slots == 0 {
                self.repair.rest += DT;
                if self.repair.rest >= REST_TO_REPAIR {
                    self.repair.active = true;
                    self.repair.held.iter_mut().for_each(|h| *h = fx!(0.0));
                    self.toast(
                        "Notreparatur: Slot-Tasten im Takt drücken – lange halten zum Weiterfliegen",
                        ToastKind::Info,
                    );
                }
            } else {
                self.repair.rest = fx!(0.0);
            }
            return false;
        }
        // Notreparatur läuft.
        for slot in 0..super::MAX_SLOTS {
            let down = input.slots >> slot & 1 == 1;
            self.repair.held[slot] = if down {
                self.repair.held[slot] + DT
            } else {
                fx!(0.0)
            };
        }
        let leave = self.repair.held.iter().any(|h| *h >= HOLD_TO_LEAVE);
        if leave || speed > fx!(2.0) {
            self.repair.active = false;
            self.repair.rest = fx!(0.0);
            return false;
        }
        for slot in 0..super::MAX_SLOTS as u8 {
            if edges >> slot & 1 == 0 {
                continue;
            }
            if let Some(ti) = self
                .ship
                .thrusters
                .iter()
                .position(|t| t.slot == slot && t.failed())
            {
                self.patch_thruster(ti);
            } else if self.rhythm(slot) {
                let cap = self.ship.max_hull * HULL_PATCH_CAP;
                self.ship.hull = (self.ship.hull + HULL_PER_HIT).min(cap.max(self.ship.hull));
            }
        }
        let thrusters_ok = self.ship.thrusters.iter().all(|t| !t.failed());
        if !self.hull_needs_patch() && thrusters_ok {
            self.repair.active = false;
            self.repair.rest = fx!(0.0);
            self.toast(
                "Notreparatur fertig – den Rest macht die Station",
                ToastKind::Good,
            );
        }
        true
    }

    /// Station mit gesichertem Port, die gerade nicht offen ist?
    pub fn port_locked(&self, si: usize) -> bool {
        self.data.world.stations[si].hack.is_some()
            && self.time >= self.unlocked.get(si).copied().unwrap_or(fx!(0.0))
    }

    fn try_start_hack(&mut self) {
        if self.ship.vel.length() > fx!(5.0) {
            return;
        }
        for si in 0..self.world.stations.len() {
            let Some(def) = self.data.world.stations[si].hack.clone() else {
                continue;
            };
            if !self.port_locked(si) || self.time < self.hack_lockout[si] {
                continue;
            }
            let near = self.world.stations[si]
                .pads
                .iter()
                .any(|&p| (self.world.pads[p].center - self.ship.pos).shorter_than(fx!(30.0)));
            if !near {
                continue;
            }
            // Muster aus den belegten Slots, nie zweimal derselbe hintereinander.
            let mut slots: Vec<u8> = self
                .ship
                .thrusters
                .iter()
                .map(|t| t.slot)
                .chain(self.ship.tools.iter().map(|t| t.slot))
                .collect();
            slots.sort_unstable();
            slots.dedup();
            if slots.is_empty() {
                return;
            }
            let mut pattern: Vec<u8> = Vec::new();
            while pattern.len() < def.length as usize {
                let s = slots[self.rng.index(slots.len())];
                if slots.len() > 1 && pattern.last() == Some(&s) {
                    continue;
                }
                pattern.push(s);
            }
            self.hack = Some(Hack {
                station: si,
                pattern,
                done: 0,
                left: def.time,
                total: def.time,
            });
            let name = self.world.stations[si].name.clone();
            self.events.push(SimEvent::HackStarted);
            self.toast(
                format!("{name}: Andockport gesichert – Muster eingeben, jede Taste ihr Slot"),
                ToastKind::Warn,
            );
            return;
        }
    }

    fn update_hack(&mut self, edges: u32) {
        let Some(mut h) = self.hack.clone() else {
            return;
        };
        // Weggeflogen: Abbruch ohne Alarm.
        let far = self.world.stations[h.station]
            .pads
            .iter()
            .all(|&p| (self.world.pads[p].center - self.ship.pos).longer_than(fx!(60.0)));
        if far {
            self.hack = None;
            return;
        }
        h.left -= DT;
        let mut failed = h.left <= fx!(0.0);
        for slot in 0..super::MAX_SLOTS as u8 {
            if failed || h.done >= h.pattern.len() || edges >> slot & 1 == 0 {
                continue;
            }
            if h.pattern[h.done] == slot {
                h.done += 1;
                self.events.push(SimEvent::Beat { hit: true });
            } else {
                failed = true;
            }
        }
        let si = h.station;
        if failed {
            self.hack = None;
            let lock = self.data.world.stations[si]
                .hack
                .as_ref()
                .map_or(fx!(30.0), |d| d.lockout);
            self.hack_lockout[si] = self.time + lock;
            // Störimpuls: der Schild bricht zusammen.
            self.ship.shield = fx!(0.0);
            self.ship.since_hit = fx!(0.0);
            self.events.push(SimEvent::Alarm);
            self.toast(
                format!("Alarm! Störimpuls – Schild weg, Port {lock:.0} s gesperrt"),
                ToastKind::Bad,
            );
        } else if h.done >= h.pattern.len() {
            self.hack = None;
            let open = self.data.world.stations[si]
                .hack
                .as_ref()
                .map_or(fx!(600.0), |d| d.open);
            self.unlocked[si] = self.time + open;
            self.events.push(SimEvent::HackDone);
            self.toast(
                format!("Port geknackt – {:.0} Minuten offen", open / fx!(60.0)),
                ToastKind::Good,
            );
        } else {
            self.hack = Some(h);
        }
    }

    /// Kran-Notreparatur: zielt der Kran auf ein ausgefallenes Triebwerk, und ist ein
    /// Ersatzteil an Bord? Ergebnis: Index des Triebwerks.
    pub fn crane_patch_target(&self, tool: usize) -> Option<usize> {
        if !self
            .ship
            .cargo
            .iter()
            .any(|c| matches!(c.kind, CargoKind::Salvage { .. }))
        {
            return None;
        }
        let mount = self.ship.tool_world_pos(tool);
        let aim = self.ship.tools[tool].aim;
        self.ship
            .thrusters
            .iter()
            .enumerate()
            .filter(|(_, t)| t.failed())
            .map(|(i, t)| {
                let to: Vec2 = self.ship.to_world(t.pos) - mount;
                let off = angle_diff(Fx::atan2(to.y, to.x), aim).abs().to_degrees();
                (i, off, to.length())
            })
            .filter(|(_, off, d)| *off <= CRANE_PATCH_AIM && *d <= self.ship.crane_range)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _, _)| i)
    }

    /// Ein Ersatzteil aus dem Frachtraum verbauen.
    pub(crate) fn use_spare_part(&mut self) -> bool {
        let Some(idx) = self
            .ship
            .cargo
            .iter()
            .position(|c| matches!(c.kind, CargoKind::Salvage { .. }))
        else {
            return false;
        };
        self.ship.cargo.remove(idx);
        self.ship.recompute_mass();
        true
    }
}
