//! Andocken (die zentrale Fähigkeit), Zerstörung und Wiedereinstieg.

use bevy::math::Vec2;

use super::ship::{CargoKind, CraneState};
use super::world::{Owner, angle_diff};
use super::{Body, BodyKind, DT, EscapePod, SimEvent, SimState, TickInput, ToastKind};

/// Maximal erlaubte Geschwindigkeit beim Andocken.
pub const DOCK_MAX_SPEED: f32 = 2.6;
/// Maximal erlaubte Abweichung der Ausrichtung (Grad).
pub const DOCK_MAX_ANGLE_DEG: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DockGuide {
    pub pad: usize,
    pub speed_ok: bool,
    pub angle_ok: bool,
    pub spin_ok: bool,
    pub in_zone: bool,
    pub distance: f32,
    /// Aktuelle Werte für die Ampel-Anzeige.
    pub speed: f32,
    pub angle_deg: f32,
    pub spin: f32,
    /// Seitlicher Versatz zur Plattformmitte (positiv = in Tangentenrichtung).
    pub lateral: f32,
    /// Höhe über der Ruheposition.
    pub height: f32,
}

/// So lange fliegt die Rettungskapsel, bevor die Bergung das Schiff an der Heimatstation abstellt.
pub const RESPAWN_SECONDS: f32 = 5.0;

/// Maximal erlaubte Drehgeschwindigkeit beim Andocken (rad/s).
pub const DOCK_MAX_SPIN: f32 = 0.9;

/// Ampel für die Andockhilfe: grün = passt, gelb = bis doppelter Grenzwert, rot = darüber.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Light {
    Green,
    Yellow,
    Red,
}

pub fn light(value: f32, limit: f32) -> Light {
    if value < limit {
        Light::Green
    } else if value < limit * 2.0 {
        Light::Yellow
    } else {
        Light::Red
    }
}

impl DockGuide {
    pub fn speed_light(&self) -> Light {
        light(self.speed, DOCK_MAX_SPEED)
    }
    pub fn angle_light(&self) -> Light {
        light(self.angle_deg, DOCK_MAX_ANGLE_DEG)
    }
    pub fn spin_light(&self) -> Light {
        light(self.spin, DOCK_MAX_SPIN)
    }
    /// Schlechteste der drei Ampeln.
    pub fn overall(&self) -> Light {
        self.speed_light()
            .max(self.angle_light())
            .max(self.spin_light())
    }
}

impl SimState {
    /// Hinweise für die Andock-Anzeige zur nächstgelegenen Plattform.
    pub fn dock_guide(&self) -> Option<DockGuide> {
        if self.ship.docked.is_some() || self.ship.destroyed {
            return None;
        }
        let mut best: Option<DockGuide> = None;
        for (pi, pad) in self.world.pads.iter().enumerate() {
            if !pad.enabled {
                continue;
            }
            // Gesicherter Port: erst hacken.
            if let Owner::Station(si) = pad.owner
                && self.port_locked(si)
            {
                continue;
            }
            // Hier steht schon ein NPC-Schiff.
            if self.npc_on_pad(pi) {
                continue;
            }
            let rel = self.ship.pos - pad.center;
            let dist = rel.length();
            if dist > 28.0 || best.is_some_and(|b| b.distance < dist) {
                continue;
            }
            let along = rel.dot(pad.normal);
            let lateral = rel.dot(pad.tangent());
            let rest = self.ship.rest_height();
            let speed = self.ship.vel.length();
            let angle_deg = angle_diff(self.ship.angle, pad.ship_angle())
                .abs()
                .to_degrees();
            let spin = self.ship.ang_vel.abs();
            best = Some(DockGuide {
                pad: pi,
                speed_ok: speed < DOCK_MAX_SPEED,
                angle_ok: angle_deg < DOCK_MAX_ANGLE_DEG,
                spin_ok: spin < DOCK_MAX_SPIN,
                in_zone: lateral.abs() < pad.half_width && along > rest - 0.6 && along < rest + 1.4,
                distance: dist,
                speed,
                angle_deg,
                spin,
                lateral,
                height: along - rest,
            });
        }
        best
    }

    pub(crate) fn update_docking(&mut self, input: &TickInput) {
        if let Some(pad) = self.ship.docked {
            self.snap_to_pad(pad);
            return;
        }
        let thrusting = self.ship.thrusters.iter().any(|t| input.pressed(t.slot));
        if self.ship.dock_cooldown > 0.0 || thrusting {
            self.ship.dock_timer = 0.0;
            return;
        }
        match self.dock_guide() {
            Some(g) if g.in_zone && g.speed_ok && g.angle_ok && g.spin_ok => {
                self.ship.dock_timer += DT;
                if self.ship.dock_timer > 0.35 {
                    self.dock(g.pad);
                }
            }
            _ => self.ship.dock_timer = 0.0,
        }
    }

    pub(crate) fn dock(&mut self, pad: usize) {
        self.ship.docked = Some(pad);
        self.ship.dock_timer = 0.0;
        self.ship.vel = Vec2::ZERO;
        self.ship.ang_vel = 0.0;
        for t in &mut self.ship.thrusters {
            t.firing = false;
        }
        self.events.push(SimEvent::Docked { pad });
        let owner = self.world.pads[pad].owner;
        let name = self.world.owner_name(owner).to_string();
        if self.world.pads[pad].zone {
            self.toast(
                format!("Gelandet auf {name} – Werkzeuge frei, Triebwerk zünden = abheben"),
                ToastKind::Good,
            );
            return;
        }
        self.toast(format!("Angedockt: {name}"), ToastKind::Good);
        if let Owner::Station(si) = owner {
            self.crew.home_station = si;
            self.charge_dock_fee(si);
            self.story_docked(si);
            self.routes_docked(si);
            self.journal_docked(si);
        }
        self.on_docked(owner);
    }

    /// Schiff sanft auf die Plattform ziehen und dort festhalten.
    pub(crate) fn snap_to_pad(&mut self, pad: usize) {
        let p = self.world.pads[pad].clone();
        let rest = self.ship.rest_height();
        let lateral = (self.ship.pos - p.center).dot(p.tangent()).clamp(
            -(p.half_width - 0.8).max(0.0),
            (p.half_width - 0.8).max(0.0),
        );
        let target = p.center + p.normal * (rest + 0.02) + p.tangent() * lateral;
        let ta = p.ship_angle();
        self.ship.angle += angle_diff(ta, self.ship.angle) * 0.2;
        self.ship.pos = self.ship.pos.lerp(target, 0.2);
        self.ship.vel = Vec2::ZERO;
        self.ship.ang_vel = 0.0;
    }

    pub(crate) fn dock_at_station(&mut self, si: usize) {
        // Die Plattform, die dem Zentrum der Station am nächsten liegt (meist die sicherste).
        let Some(st) = self.world.stations.get(si) else {
            return;
        };
        // Freie Plattformen zuerst (dort steht kein NPC-Schiff).
        let Some(&pad) = st
            .pads
            .iter()
            .filter(|p| self.world.pads[**p].enabled)
            .min_by(|a, b| {
                let da = (self.world.pads[**a].center - st.pos).length();
                let db = (self.world.pads[**b].center - st.pos).length();
                (self.npc_on_pad(**a), da)
                    .partial_cmp(&(self.npc_on_pad(**b), db))
                    .unwrap()
            })
        else {
            return;
        };
        let p = self.world.pads[pad].clone();
        self.ship.angle = p.ship_angle();
        self.ship.pos = p.center + p.normal * (self.ship.rest_height() + 0.02);
        self.ship.prev_pos = self.ship.pos;
        self.ship.prev_angle = self.ship.angle;
        self.ship.vel = Vec2::ZERO;
        self.ship.ang_vel = 0.0;
        self.ship.docked = Some(pad);
        self.crew.home_station = si;
    }

    pub(crate) fn check_ship_health(&mut self) {
        if self.ship.destroyed {
            return;
        }
        if self.ship.hull <= 0.0 {
            self.destroy_ship();
            return;
        }
        let low = self.ship.hull < self.ship.max_hull * 0.3;
        if low && !self.low_hull_warned {
            self.low_hull_warned = true;
            self.toast("Hülle kritisch! Reparatur an einer Station", ToastKind::Bad);
        } else if !low {
            self.low_hull_warned = false;
        }
    }

    fn destroy_ship(&mut self) {
        let pos = self.ship.pos;
        self.ship.destroyed = true;
        self.ship.respawn_timer = RESPAWN_SECONDS;
        // Rettungskapsel: wird nach vorne ausgestoßen und treibt bis zur Bergung.
        let nose = super::geom::rot(Vec2::Y, self.ship.angle);
        self.escape = Some(EscapePod {
            pos,
            vel: self.ship.vel * 0.5 + nose * 7.0,
            angle: self.ship.angle,
            spin: self.rng.range(-1.2, 1.2),
            prev_pos: pos,
            prev_angle: self.ship.angle,
        });
        self.ship.hull = 0.0;
        for t in &mut self.ship.tools {
            t.crane = CraneState::Idle;
            t.drill = None;
        }
        for t in &mut self.ship.thrusters {
            t.firing = false;
            t.level = 0.0;
        }
        self.events.push(SimEvent::Explosion {
            pos,
            size: 9.0,
            color: [1.0, 0.7, 0.3],
        });
        self.events.push(SimEvent::ShipDestroyed);

        // Rettungskapseln treiben wieder frei, Erz und Container sind verloren, Artefakte
        // bleiben im Wrack (oder kehren an ihren Fundort zurück).
        let cargo = self.ship.remove_cargo(|_| true);
        let mut artifacts = Vec::new();
        for item in cargo {
            match item.kind {
                CargoKind::Artifact { id } => artifacts.push(id),
                CargoKind::Capsule { mission } => {
                    let id = self.next_id();
                    let a = self.rng.range(0.0, std::f32::consts::TAU);
                    let dir = Vec2::new(a.cos(), a.sin());
                    self.bodies.push(Body {
                        id,
                        kind: BodyKind::Capsule { mission },
                        pos: pos + dir * 2.0,
                        vel: self.ship.vel + dir * 3.0,
                        angle: 0.0,
                        ang_vel: 1.0,
                        radius: 0.7,
                        mass: 0.8,
                        prev_pos: pos,
                        prev_angle: 0.0,
                        alive: true,
                        seed: id,
                        age: 0.0,
                    });
                }
                CargoKind::Container { mission, .. } => {
                    self.fail_mission(mission, "Fracht zerstört")
                }
                // Die Geretteten steigen mit in die Rettungskapsel – der Auftrag ist trotzdem
                // gescheitert.
                CargoKind::Survivor { mission } => self.fail_mission(
                    mission,
                    "Schiff verloren, die Geretteten sind mit in die Kapsel",
                ),
                CargoKind::Ore(_) | CargoKind::Salvage { .. } | CargoKind::Gear(_) => {}
            }
        }
        let vel = self.ship.vel;
        self.story_ship_lost(artifacts, pos, vel);
        let gross = self.salvage_fee_gross();
        let fee = self.salvage_fee_now();
        self.crew.credits -= fee;
        self.salvage_fee = fee;
        self.stats.expenses.salvage += fee;
        let home = self.world.stations[self.crew.home_station].name.clone();
        let covered = if gross > fee {
            format!(", Versicherung übernimmt {}", gross - fee)
        } else {
            String::new()
        };
        self.toast(
            format!(
                "Schiff zerstört! Rettungskapsel ausgestoßen – Bergung nach {home} (-{fee} Credits{covered})"
            ),
            ToastKind::Bad,
        );
    }

    /// Bergungsgebühr vor Versicherung: fester Betrag + Anteil der Kasse.
    pub fn salvage_fee_gross(&self) -> u32 {
        let shop = &self.data.shop;
        let fee = shop.salvage_base as f32 + self.crew.credits as f32 * shop.respawn_fee;
        fee.round() as u32
    }

    /// Bergungsgebühr, die bei einer Zerstörung jetzt aus der gemeinsamen Kasse fällig wäre
    /// (nach Abzug der Versicherung, höchstens die Kasse).
    pub fn salvage_fee_now(&self) -> u32 {
        let gross = self.salvage_fee_gross() as f32;
        let share = if self.crew.insured {
            1.0 - self.data.shop.finance.coverage
        } else {
            1.0
        };
        ((gross * share).round() as u32).min(self.crew.credits)
    }

    pub(crate) fn update_respawn(&mut self) {
        if let Some(p) = &mut self.escape {
            p.prev_pos = p.pos;
            p.prev_angle = p.angle;
            p.pos += p.vel * DT;
            p.angle += p.spin * DT;
            // Kleine Steuerdüsen bremsen die Kapsel allmählich ab.
            p.vel *= 1.0 - 0.35 * DT;
            p.spin *= 1.0 - 0.5 * DT;
        }
        self.ship.respawn_timer -= DT;
        if self.ship.respawn_timer > 0.0 {
            return;
        }
        self.escape = None;
        self.ship.destroyed = false;
        self.ship.hull = self.ship.max_hull;
        // Verbrauchtes ist weg (Phase 19): Schildladung und Munition müssen nachgekauft werden.
        self.ship.shield = 0.0;
        self.ship.ammo = 0;
        self.ship.fuel = self.ship.fuel.max(self.ship.max_fuel * 0.5);
        self.fuel_warned = 0;
        for t in &mut self.ship.thrusters {
            t.health = 1.0;
        }
        self.ship.invulnerable = 2.0;
        self.low_hull_warned = false;
        let home = self.crew.home_station;
        self.dock_at_station(home);
        self.events.push(SimEvent::Respawned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::data::{CrewSave, GameData};
    use crate::sim::ship::Loadout;
    use std::sync::Arc;

    fn sim() -> SimState {
        let data = Arc::new(GameData::embedded().unwrap());
        let save = CrewSave::new_game(&data);
        let def = data.ship(&save.current_ship).clone();
        SimState::new(data, &save, Loadout::full(&def), 1)
    }

    #[test]
    fn gentle_landing_docks() {
        let mut s = sim();
        let pad = s.world.stations[0].pads[0];
        let p = s.world.pads[pad].clone();
        s.ship.docked = None;
        s.ship.dock_cooldown = 0.0;
        s.ship.angle = p.ship_angle();
        s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 2.0);
        s.ship.vel = -p.normal * 1.2;
        for _ in 0..240 {
            s.step(&TickInput::default());
            if s.ship.docked.is_some() {
                break;
            }
        }
        assert_eq!(s.ship.docked, Some(pad));
    }

    #[test]
    fn hard_crash_does_not_dock_and_damages() {
        let mut s = sim();
        let pad = s.world.stations[0].pads[0];
        let p = s.world.pads[pad].clone();
        s.ship.docked = None;
        s.ship.shield = 0.0;
        s.ship.angle = p.ship_angle() + 1.2;
        s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 3.0);
        s.ship.vel = -p.normal * 14.0;
        let hull = s.ship.hull;
        for _ in 0..60 {
            s.step(&TickInput::default());
        }
        assert!(s.ship.docked.is_none());
        assert!(s.ship.hull < hull, "Aufprall verursacht Schaden");
    }
}
