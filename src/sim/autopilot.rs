//! Einfacher Autopilot für Tests und Vorführungen: steuert nur mit den beiden äußersten
//! Triebwerken (links/rechts), wie eine Zwei-Personen-Crew.

use bevy::math::Vec2;

use super::SimState;
use super::geom::forward;
use super::world::angle_diff;

pub struct Autopilot {
    pub max_speed: f32,
}

impl Default for Autopilot {
    fn default() -> Self {
        Autopilot { max_speed: 22.0 }
    }
}

impl Autopilot {
    /// Slot-Bitmaske für einen Tick, um `target` zu erreichen. Mit `landing` wird
    /// vorsichtig angeflogen und mit der Nase Richtung `up` aufgesetzt.
    pub fn steer(&self, sim: &SimState, target: Vec2, up: Vec2, landing: bool) -> u32 {
        let ship = &sim.ship;
        if ship.thrusters.len() < 2 {
            return 0;
        }
        let left = ship.thrusters.first().unwrap().slot;
        let right = ship.thrusters.last().unwrap().slot;
        let thrust: f32 = ship.thrusters.iter().map(|t| t.thrust).sum::<f32>() * 2.0
            / ship.thrusters.len() as f32;
        let accel = thrust / ship.mass;

        let to_t = target - ship.pos;
        let dist = to_t.length();
        // Bremsweg inklusive Zeit zum Umdrehen (≈ 2 s).
        let brake = ((2.0 * accel * 0.5 * dist).sqrt() - accel * 0.6).max(1.0);
        // Landeanflug: oberhalb nie ganz anhalten (sonst müsste man zum Sinken umdrehen),
        // unterhalb von 10 m nur noch mit der Nase nach oben bremsen.
        let fine = landing && dist < 10.0;
        let limit = if fine {
            (dist * 0.25).clamp(0.5, 2.0)
        } else if landing {
            (dist * 0.25).clamp(1.6, 3.0)
        } else {
            self.max_speed.min(brake)
        };
        let v_des = if dist > 0.05 {
            to_t / dist * limit
        } else {
            Vec2::ZERO
        };
        let dv = v_des - ship.vel;
        let coast = dv.length()
            < if fine {
                0.45
            } else if landing {
                0.8
            } else {
                2.0
            };

        let mut want = if coast { up } else { dv.normalize() };
        if fine {
            // Kurz vor dem Aufsetzen nie umdrehen: höchstens ±35° neigen.
            let a = angle_diff(want.to_angle(), up.to_angle()).clamp(-0.6, 0.6);
            want = Vec2::from_angle(up.to_angle() + a);
        }
        let desired = f32::atan2(-want.x, want.y);
        let err = angle_diff(desired, ship.angle);
        let hold = (coast && !landing) || (fine && err.abs() < 0.3);
        let w_des = if hold {
            0.0
        } else {
            (err * 1.6).clamp(-1.2, 1.2)
        };
        let dw = w_des - ship.ang_vel;
        // Im Endanflug kleine Drehungen ignorieren: jede Korrektur schiebt auch nach oben.
        let dw = if fine && dw.abs() < 0.3 { 0.0 } else { dw };

        let mut slots = 0u32;
        let helps = dv.dot(forward(ship.angle)) > if fine { 0.45 } else { 0.0 };
        let _ = up;
        if !coast && helps && err.abs() < if fine { 0.2 } else { 0.3 } {
            slots |= (1 << left) | (1 << right);
            if dw > 0.4 {
                slots &= !(1 << left);
            } else if dw < -0.4 {
                slots &= !(1 << right);
            }
        } else if dw > 0.12 {
            slots |= 1 << right;
        } else if dw < -0.12 {
            slots |= 1 << left;
        }
        slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::TickInput;
    use crate::sim::data::{CrewSave, GameData};
    use crate::sim::ship::Loadout;
    use std::sync::Arc;

    #[test]
    fn two_person_crew_flies_from_nova_to_kepler_and_docks() {
        let data = Arc::new(GameData::embedded().unwrap());
        let save = CrewSave::new_game(&data);
        let mut s = SimState::new(
            data.clone(),
            &save,
            Loadout {
                thrusters: 2,
                tools: vec![],
            },
            2,
        );
        let kepler = data.station_index("kepler").unwrap();
        // Plattform auf dem linken Arm (frei anfliegbar von oben).
        let pad = *s.world.stations[kepler]
            .pads
            .iter()
            .max_by(|a, b| {
                s.world.pads[**a]
                    .center
                    .y
                    .total_cmp(&s.world.pads[**b].center.y)
            })
            .unwrap();
        let p = s.world.pads[pad].clone();
        let rest = s.ship.rest_height();
        let waypoints = [
            (Vec2::new(0.0, 0.0), 6.0),
            (Vec2::new(74.0, 0.0), 8.0),
            (p.center + p.normal * 24.0, 6.0),
            (p.center + p.normal * (rest - 0.3), 0.3),
        ];
        let ap = Autopilot::default();
        let mut wp = 0;
        let mut max_damage = 0.0f32;
        for tick in 0..60 * 240 {
            let (target, tol) = waypoints[wp.min(waypoints.len() - 1)];
            if wp + 1 < waypoints.len()
                && (s.ship.pos - target).length() < tol
                && s.ship.vel.length() < 6.0
            {
                wp += 1;
            }
            let mut slots = ap.steer(&s, target, p.normal, wp + 1 == waypoints.len());
            if wp == 0 && tick < 20 {
                slots = 0b11; // abheben
            }
            s.step(&TickInput {
                slots,
                aims: vec![0.0; crate::sim::MAX_SLOTS],
                commands: vec![],
            });
            max_damage = max_damage.max(s.ship.max_hull - s.ship.hull);
            if std::env::var("AP_TRACE").is_ok() && tick % 30 == 0 {
                let rel = s.ship.pos - p.center;
                eprintln!(
                    "t{tick:5} wp{wp} rel({:7.1},{:7.1}) v({:5.1},{:5.1}) a{:5.2} w{:5.2} hull{:5.1} slots{slots:02b}",
                    rel.x,
                    rel.y,
                    s.ship.vel.x,
                    s.ship.vel.y,
                    angle_diff(s.ship.angle, 0.0),
                    s.ship.ang_vel,
                    s.ship.hull
                );
            }
            if s.ship.docked == Some(pad) {
                break;
            }
            assert!(
                !s.ship.destroyed,
                "Schiff zerstört bei Wegpunkt {wp}, Tick {tick}"
            );
        }
        assert_eq!(
            s.ship.docked,
            Some(pad),
            "nicht angedockt; Wegpunkt {wp}, pos {:?}, vel {:?}, winkel {:.2}/{:.2}, guide {:?}, pad {:?}",
            s.ship.pos,
            s.ship.vel,
            s.ship.angle,
            p.ship_angle(),
            s.dock_guide(),
            p
        );
        assert!(max_damage < 60.0, "zu viel Schaden: {max_damage}");
    }
}
