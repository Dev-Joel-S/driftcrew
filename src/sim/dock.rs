//! Andocken (die zentrale Fähigkeit), Zerstörung und Wiedereinstieg.

use bevy::math::Vec2;

use super::ship::{CargoKind, CraneState};
use super::world::{angle_diff, Owner};
use super::{Body, BodyKind, SimEvent, SimState, TickInput, ToastKind, DT};

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
}

impl SimState {
    /// Hinweise für die Andock-Anzeige zur nächstgelegenen Plattform.
    pub fn dock_guide(&self) -> Option<DockGuide> {
        if self.ship.docked.is_some() || self.ship.destroyed {
            return None;
        }
        let mut best: Option<DockGuide> = None;
        for (pi, pad) in self.world.pads.iter().enumerate() {
            let rel = self.ship.pos - pad.center;
            let dist = rel.length();
            if dist > 28.0 || best.is_some_and(|b| b.distance < dist) {
                continue;
            }
            let along = rel.dot(pad.normal);
            let lateral = rel.dot(pad.tangent());
            let rest = self.ship.rest_height();
            best = Some(DockGuide {
                pad: pi,
                speed_ok: self.ship.vel.length() < DOCK_MAX_SPEED,
                angle_ok: angle_diff(self.ship.angle, pad.ship_angle()).abs() < DOCK_MAX_ANGLE_DEG.to_radians(),
                spin_ok: self.ship.ang_vel.abs() < 0.9,
                in_zone: lateral.abs() < pad.half_width && along > rest - 0.6 && along < rest + 1.4,
                distance: dist,
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

    fn dock(&mut self, pad: usize) {
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
        self.toast(format!("Angedockt: {name}"), ToastKind::Good);
        if let Owner::Station(si) = owner {
            self.crew.home_station = si;
        }
        self.on_docked(owner);
    }

    /// Schiff sanft auf die Plattform ziehen und dort festhalten.
    pub(crate) fn snap_to_pad(&mut self, pad: usize) {
        let p = self.world.pads[pad].clone();
        let rest = self.ship.rest_height();
        let lateral = (self.ship.pos - p.center)
            .dot(p.tangent())
            .clamp(-(p.half_width - 0.8).max(0.0), (p.half_width - 0.8).max(0.0));
        let target = p.center + p.normal * (rest + 0.02) + p.tangent() * lateral;
        let ta = p.ship_angle();
        self.ship.angle += angle_diff(ta, self.ship.angle) * 0.2;
        self.ship.pos = self.ship.pos.lerp(target, 0.2);
        self.ship.vel = Vec2::ZERO;
        self.ship.ang_vel = 0.0;
    }

    pub(crate) fn dock_at_station(&mut self, si: usize) {
        // Die Plattform, die dem Zentrum der Station am nächsten liegt (meist die sicherste).
        let Some(st) = self.world.stations.get(si) else { return };
        let Some(&pad) = st
            .pads
            .iter()
            .min_by(|a, b| {
                let da = (self.world.pads[**a].center - st.pos).length();
                let db = (self.world.pads[**b].center - st.pos).length();
                da.total_cmp(&db)
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
        self.ship.respawn_timer = 3.5;
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

        // Rettungskapseln treiben wieder frei, Erz und Container sind verloren.
        let cargo = self.ship.remove_cargo(|_| true);
        for item in cargo {
            match item.kind {
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
                CargoKind::Container { mission, .. } => self.fail_mission(mission, "Fracht zerstört"),
                CargoKind::Ore(_) => {}
            }
        }
        let fee = (self.crew.credits as f32 * self.data.shop.respawn_fee).round() as u32;
        self.crew.credits -= fee.min(self.crew.credits);
        let home = self.world.stations[self.crew.home_station].name.clone();
        self.toast(
            format!("Schiff zerstört! Bergung nach {home} (-{fee} Credits)"),
            ToastKind::Bad,
        );
    }

    pub(crate) fn update_respawn(&mut self) {
        self.ship.respawn_timer -= DT;
        if self.ship.respawn_timer > 0.0 {
            return;
        }
        self.ship.destroyed = false;
        self.ship.hull = self.ship.max_hull;
        self.ship.shield = self.ship.max_shield;
        self.ship.ammo = self.ship.ammo.max(self.ship.max_ammo / 2);
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
