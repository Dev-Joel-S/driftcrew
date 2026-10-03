//! Minispiele für einzelne Slots (Punkte 35, 38, 72): Reparatur im Takt, Andockport hacken,
//! Notreparatur mit dem Kran. Alles läuft in der Simulation und wird nur mit Slot-Tasten
//! gespielt – deterministisch, damit es später auch online geht.

use bevy::math::Vec2;

use super::ship::CargoKind;
use super::world::angle_diff;
use super::{DT, SimEvent, SimState, TickInput, ToastKind};

/// Takt in Sekunden und wie weit daneben ein Druck noch zählt.
pub const BEAT: f32 = 0.7;
pub const BEAT_WINDOW: f32 = 0.14;
/// Treffer, bis ein ausgefallenes Triebwerk wieder läuft.
pub const PATCH_HITS: f32 = 8.0;
/// Zustand eines notdürftig geflickten Triebwerks (läuft, stottert aber).
pub const PATCH_HEALTH: f32 = 0.45;
/// Notreparatur der Hülle: Punkte pro Treffer, höchstens bis zu diesem Anteil.
pub const HULL_PER_HIT: f32 = 2.0;
pub const HULL_PATCH_CAP: f32 = 0.75;
/// So lange ruhig und ohne Eingabe, dann beginnt die Notreparatur.
pub const REST_TO_REPAIR: f32 = 3.0;
/// Eine Taste so lange halten beendet die Notreparatur (die Taste wirkt dann normal).
pub const HOLD_TO_LEAVE: f32 = 0.45;
/// Kran-Notreparatur: so lange muss das Ersatzteil ruhig an der Stelle sitzen.
pub const CRANE_PATCH_TIME: f32 = 2.5;
/// Kran-Notreparatur: so genau muss der Kran zielen (Grad).
pub const CRANE_PATCH_AIM: f32 = 15.0;

/// Rückmeldung zu einem Tastendruck im Takt (für die Anzeige).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub slot: u8,
    pub hit: bool,
    pub age: f32,
}

/// Notreparatur und Takt-Reparatur.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repair {
    /// Notreparatur läuft (Schiff ruht, Slot-Tasten flicken statt zu steuern).
    pub active: bool,
    /// Wie lange das Schiff schon ruhig und ohne Eingabe ist.
    pub rest: f32,
    /// Wie lange jede Taste schon gehalten wird.
    pub held: Vec<f32>,
    /// Fortschritt pro Triebwerk (Index wie `ship.thrusters`), 0..1.
    pub thrusters: Vec<f32>,
    pub beats: Vec<Beat>,
}

/// Laufender Hack eines Andockports.
#[derive(Clone, Debug, PartialEq)]
pub struct Hack {
    pub station: usize,
    /// Slots in der Reihenfolge, in der sie gedrückt werden müssen.
    pub pattern: Vec<u8>,
    pub done: usize,
    pub left: f32,
    pub total: f32,
}

/// Abstand zum nächsten Taktschlag (Sekunden).
pub fn beat_offset(time: f32) -> f32 {
    let phase = time.rem_euclid(BEAT);
    phase.min(BEAT - phase)
}

impl SimState {
    /// Liegt dieser Zeitpunkt im Takt?
    pub fn on_beat(&self) -> bool {
        beat_offset(self.time) <= BEAT_WINDOW
    }

    /// Phase im Takt (0..1, 0 = Schlag) – für die Anzeige.
    pub fn beat_phase(&self) -> f32 {
        self.time.rem_euclid(BEAT) / BEAT
    }

    /// Braucht die Hülle eine Notreparatur?
    pub fn hull_needs_patch(&self) -> bool {
        self.ship.hull < self.ship.max_hull * HULL_PATCH_CAP - 0.5
    }

    /// Minispiele vor der Steuerung auswerten. Ergebnis: Slot-Tasten sind in diesem Tick
    /// Eingabe für ein Minispiel und steuern das Schiff nicht.
    pub(crate) fn update_minigames(&mut self, input: &TickInput) -> bool {
        let edges = input.slots & !self.prev_slots;
        self.prev_slots = input.slots;
        let n = self.ship.thrusters.len();
        if self.repair.thrusters.len() != n {
            self.repair.thrusters = vec![0.0; n];
        }
        if self.repair.held.len() != super::MAX_SLOTS {
            self.repair.held = vec![0.0; super::MAX_SLOTS];
        }
        for b in &mut self.repair.beats {
            b.age += DT;
        }
        self.repair.beats.retain(|b| b.age < 1.2);
        if self.ship.docked.is_some() || self.ship.destroyed || self.vote.is_some() {
            self.repair.active = false;
            self.repair.rest = 0.0;
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
            age: 0.0,
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
            *p + 1.0 / PATCH_HITS
        } else {
            (*p - 0.5 / PATCH_HITS).max(0.0)
        };
        if *p >= 1.0 - 1e-4 {
            *p = 0.0;
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
        let calm = speed < 0.6 && self.ship.ang_vel.abs() < 0.4;
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
                    self.repair.held.iter_mut().for_each(|h| *h = 0.0);
                    self.toast(
                        "Notreparatur: Slot-Tasten im Takt drücken – lange halten zum Weiterfliegen",
                        ToastKind::Info,
                    );
                }
            } else {
                self.repair.rest = 0.0;
            }
            return false;
        }
        // Notreparatur läuft.
        for slot in 0..super::MAX_SLOTS {
            let down = input.slots >> slot & 1 == 1;
            self.repair.held[slot] = if down {
                self.repair.held[slot] + DT
            } else {
                0.0
            };
        }
        let leave = self.repair.held.iter().any(|h| *h >= HOLD_TO_LEAVE);
        if leave || speed > 2.0 {
            self.repair.active = false;
            self.repair.rest = 0.0;
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
            self.repair.rest = 0.0;
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
            && self.time >= self.unlocked.get(si).copied().unwrap_or(0.0)
    }

    fn try_start_hack(&mut self) {
        if self.ship.vel.length() > 5.0 {
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
                .any(|&p| (self.world.pads[p].center - self.ship.pos).length() < 30.0);
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
            .all(|&p| (self.world.pads[p].center - self.ship.pos).length() > 60.0);
        if far {
            self.hack = None;
            return;
        }
        h.left -= DT;
        let mut failed = h.left <= 0.0;
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
                .map_or(30.0, |d| d.lockout);
            self.hack_lockout[si] = self.time + lock;
            // Störimpuls: der Schild bricht zusammen.
            self.ship.shield = 0.0;
            self.ship.since_hit = 0.0;
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
                .map_or(600.0, |d| d.open);
            self.unlocked[si] = self.time + open;
            self.events.push(SimEvent::HackDone);
            self.toast(
                format!("Port geknackt – {:.0} Minuten offen", open / 60.0),
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
                let off = angle_diff(f32::atan2(to.y, to.x), aim).abs().to_degrees();
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
