//! Gefahren und bewegliche Objekte: Asteroidenfelder, Meteoriten, Geschosse.

use bevy::math::Vec2;

use super::data::Ore;
use super::geom::{ray_circle, ray_poly, rot};
use super::rng::hash32;
use super::world::Shape;
use super::{Body, BodyKind, DT, SimEvent, SimState};

/// Wracks werden etwas größer dargestellt als das Schiff, nach dem sie aussehen.
pub const WRECK_SCALE: f32 = 1.3;

pub fn asteroid_hp(r: f32) -> f32 {
    18.0 * r
}

pub fn asteroid_mass(r: f32) -> f32 {
    r * r * 2.2
}

impl SimState {
    pub(crate) fn populate_fields(&mut self) {
        let fields = self.data.world.asteroid_fields.clone();
        for (fi, f) in fields.iter().enumerate() {
            for _ in 0..f.count {
                let pos = self.random_in_field(fi, None);
                self.spawn_asteroid(fi, pos, None);
            }
        }
    }

    /// Wracks aus den Daten an ihren Platz legen.
    pub(crate) fn populate_wrecks(&mut self) {
        let wrecks = self.data.world.wrecks.clone();
        for (idx, w) in wrecks.iter().enumerate() {
            let id = self.next_id();
            let pos = Vec2::new(w.pos.0, w.pos.1);
            let def = self.data.ship(&w.ship);
            // Grobe Größe aus dem Schiff: Abstand der äußersten Teile.
            let r = def
                .parts
                .iter()
                .map(|p| Vec2::new(p.pos.0, p.pos.1).length() + 0.5 * p.size.0.max(p.size.1))
                .fold(1.0f32, f32::max)
                * WRECK_SCALE
                * 0.8;
            let angle = w.angle.to_radians();
            self.bodies.push(Body {
                id,
                kind: BodyKind::Wreck {
                    idx,
                    scrap: w.scrap,
                    parts: w.parts,
                    home: pos,
                },
                pos,
                vel: Vec2::ZERO,
                angle,
                ang_vel: self.rng.range(-0.04, 0.04),
                radius: r,
                mass: 40.0 + w.scrap,
                prev_pos: pos,
                prev_angle: angle,
                alive: true,
                seed: hash32(id),
                age: 0.0,
            });
        }
    }

    fn random_in_field(&mut self, fi: usize, avoid: Option<Vec2>) -> Vec2 {
        let f = &self.data.world.asteroid_fields[fi];
        let c = Vec2::new(f.center.0, f.center.1);
        let radius = f.radius;
        let mut p = c;
        for _ in 0..12 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = radius * self.rng.f32().sqrt();
            p = c + Vec2::new(a.cos(), a.sin()) * d;
            match avoid {
                Some(q) if (p - q).length() < 90.0 => continue,
                _ => break,
            }
        }
        p
    }

    pub(crate) fn spawn_asteroid(&mut self, field: usize, pos: Vec2, radius: Option<f32>) -> usize {
        let f = self.data.world.asteroid_fields[field].clone();
        let r = radius.unwrap_or_else(|| {
            let t = self.rng.f32();
            f.min_radius + (f.max_radius - f.min_radius) * t.powf(1.4)
        });
        let ore = if self.rng.chance(f.ore_chance) {
            Some(f.ore)
        } else {
            None
        };
        let vein = ore.is_some() && r >= 2.5 && self.rng.chance(0.2);
        let id = self.next_id();
        let vel = Vec2::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0)) * 1.2;
        let ang_vel = self.rng.range(-0.6, 0.6);
        let angle = self.rng.range(0.0, std::f32::consts::TAU);
        self.bodies.push(Body {
            id,
            kind: BodyKind::Asteroid {
                ore,
                ore_left: if ore.is_some() { r * 1.6 } else { 0.0 },
                hp: asteroid_hp(r),
                field,
                vein,
            },
            pos,
            vel,
            angle,
            ang_vel,
            radius: r,
            mass: asteroid_mass(r),
            prev_pos: pos,
            prev_angle: angle,
            alive: true,
            seed: hash32(id),
            age: 0.0,
        });
        self.bodies.len() - 1
    }

    pub(crate) fn spawn_chunk(&mut self, pos: Vec2, vel: Vec2, ore: Ore, amount: f32) {
        let id = self.next_id();
        let r = 0.45 + amount.min(3.0) * 0.12;
        self.bodies.push(Body {
            id,
            kind: BodyKind::OreChunk { ore, amount },
            pos,
            vel,
            angle: 0.0,
            ang_vel: self.rng.range(-2.0, 2.0),
            radius: r,
            mass: 0.4 + amount * 0.3,
            prev_pos: pos,
            prev_angle: 0.0,
            alive: true,
            seed: hash32(id),
            age: 0.0,
        });
    }

    pub(crate) fn update_hazards(&mut self) {
        self.border_warn = (self.border_warn - DT).max(-1.0);

        // Meteoritenschauer in der Nähe des Schiffs.
        let zones = self.data.world.meteor_zones.clone();
        for (zi, z) in zones.iter().enumerate() {
            let c = Vec2::new(z.center.0, z.center.1);
            if (self.ship.pos - c).length() > z.radius + 380.0 {
                continue;
            }
            self.meteor_timers[zi] -= DT;
            if self.meteor_timers[zi] <= 0.0 {
                self.meteor_timers[zi] = z.interval * self.rng.range(0.6, 1.4);
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let start = c + Vec2::new(a.cos(), a.sin()) * z.radius * 1.1;
                let ta = self.rng.range(0.0, std::f32::consts::TAU);
                let target = c + Vec2::new(ta.cos(), ta.sin()) * z.radius * 0.6 * self.rng.f32();
                let dir = (target - start).normalize_or_zero();
                let speed = self.rng.range(z.speed.0, z.speed.1);
                let r = self.rng.range(0.7, 1.7);
                let id = self.next_id();
                self.bodies.push(Body {
                    id,
                    kind: BodyKind::Meteor,
                    pos: start,
                    vel: dir * speed,
                    angle: 0.0,
                    ang_vel: self.rng.range(-3.0, 3.0),
                    radius: r,
                    mass: r * r * 3.0,
                    prev_pos: start,
                    prev_angle: 0.0,
                    alive: true,
                    seed: hash32(id),
                    age: 0.0,
                });
            }
        }
        // Meteoriten außerhalb aller Zonen verschwinden.
        for b in &mut self.bodies {
            if !matches!(b.kind, BodyKind::Meteor) || b.age < 3.0 {
                continue;
            }
            let inside = zones
                .iter()
                .any(|z| (b.pos - Vec2::new(z.center.0, z.center.1)).length() < z.radius * 1.3);
            if !inside || b.age > 40.0 {
                b.alive = false;
            }
        }

        // Asteroiden: zerbrechen, im Feld halten, nachwachsen.
        let fields = self.data.world.asteroid_fields.clone();
        let mut to_break = Vec::new();
        for (bi, b) in self.bodies.iter_mut().enumerate() {
            if let BodyKind::Asteroid { hp, field, .. } = b.kind {
                if !b.alive {
                    continue;
                }
                if hp <= 0.0 {
                    to_break.push(bi);
                    continue;
                }
                let f = &fields[field];
                let c = Vec2::new(f.center.0, f.center.1);
                let d = b.pos - c;
                if d.length() > f.radius * 1.15 {
                    b.vel -= d.normalize_or_zero() * 0.6 * DT;
                }
                // Etwas Reibung, damit Felder über lange Zeit ruhig bleiben.
                b.vel *= 1.0 - 0.02 * DT;
            }
            if let BodyKind::OreChunk { .. }
            | BodyKind::Capsule { .. }
            | BodyKind::Crate { .. }
            | BodyKind::Salvage { .. } = b.kind
            {
                b.vel *= 1.0 - 0.05 * DT;
            }
            // Wracks treiben träge zurück an ihren Platz, außer jemand schleppt sie weit weg.
            if let BodyKind::Wreck { home, .. } = b.kind {
                let d = home - b.pos;
                if d.length() < 150.0 {
                    b.vel += d * 0.004 * DT;
                }
                b.vel *= 1.0 - 0.15 * DT;
                b.ang_vel *= 1.0 - 0.05 * DT;
            }
        }
        for bi in to_break {
            self.break_asteroid(bi);
        }
        for (fi, field_def) in fields.iter().enumerate() {
            self.field_respawn[fi] -= DT;
            if self.field_respawn[fi] > 0.0 {
                continue;
            }
            self.field_respawn[fi] = 3.0;
            let count = self
                .bodies
                .iter()
                .filter(|b| {
                    b.alive && matches!(b.kind, BodyKind::Asteroid { field, .. } if field == fi)
                })
                .count() as u32;
            if count < field_def.count {
                let pos = self.random_in_field(fi, Some(self.ship.pos));
                self.spawn_asteroid(fi, pos, None);
            }
        }

        // Erzvorkommen auf Planeten wachsen langsam nach.
        for p in &mut self.world.planets {
            for d in &mut p.deposits {
                d.amount = (d.amount + 0.04 * DT).min(d.max);
            }
        }
    }

    fn break_asteroid(&mut self, bi: usize) {
        let b = self.bodies[bi].clone();
        self.bodies[bi].alive = false;
        let BodyKind::Asteroid {
            ore,
            ore_left,
            field,
            ..
        } = b.kind
        else {
            return;
        };
        let color = ore.map(|o| o.color()).unwrap_or([0.85, 0.6, 0.45]);
        self.events.push(SimEvent::Explosion {
            pos: b.pos,
            size: b.radius * 1.6,
            color,
        });
        if b.radius > 2.4 {
            let side = rot(Vec2::X, self.rng.range(0.0, std::f32::consts::TAU));
            for s in [-1.0f32, 1.0] {
                let r = b.radius * 0.6;
                let idx = self.spawn_asteroid(field, b.pos + side * s * r * 1.05, Some(r));
                let nb = &mut self.bodies[idx];
                nb.vel = b.vel + side * s * 2.5;
                if let BodyKind::Asteroid {
                    ore: o,
                    ore_left: ol,
                    ..
                } = &mut nb.kind
                {
                    *o = ore;
                    *ol = ore_left * 0.4;
                }
            }
        }
        if let Some(o) = ore {
            let chunks = if b.radius > 3.0 { 3 } else { 2 };
            let amount = (ore_left * 0.6 / chunks as f32).max(0.6);
            for k in 0..chunks {
                let a = std::f32::consts::TAU * k as f32 / chunks as f32 + self.rng.range(0.0, 1.0);
                let dir = Vec2::new(a.cos(), a.sin());
                self.spawn_chunk(b.pos + dir * b.radius * 0.5, b.vel + dir * 2.0, o, amount);
            }
        }
    }

    pub(crate) fn update_projectiles(&mut self) {
        for pi in 0..self.projectiles.len() {
            let (from, vel) = (self.projectiles[pi].pos, self.projectiles[pi].vel);
            let to = from + vel * DT;
            self.projectiles[pi].pos = to;
            self.projectiles[pi].life -= DT;
            let seg = to - from;
            let len = seg.length();
            if len < 1e-6 {
                continue;
            }
            let dir = seg / len;
            let mut hit: Option<(f32, Option<usize>)> = None;
            for (bi, b) in self.bodies.iter().enumerate() {
                if !b.alive {
                    continue;
                }
                for (cp, cr) in b.circles().iter() {
                    if let Some(t) = ray_circle(from, dir, len, cp, cr)
                        && hit.is_none_or(|(ht, _)| t < ht)
                    {
                        hit = Some((t, Some(bi)));
                    }
                }
            }
            for col in &self.world.colliders {
                if !col.enabled || !col.aabb.expand(len).contains(from) {
                    continue;
                }
                let t = match col.shape {
                    Shape::Poly(q) => ray_poly(from, dir, len, &q),
                    Shape::Circle { c, r } => ray_circle(from, dir, len, c, r),
                };
                if let Some(t) = t
                    && hit.is_none_or(|(ht, _)| t < ht)
                {
                    hit = Some((t, None));
                }
            }
            for sp in &self.world.spinners {
                if (sp.pos - from).length() > sp.arm_length + len + 1.0 {
                    continue;
                }
                for q in sp.quads() {
                    if let Some(t) = ray_poly(from, dir, len, &q)
                        && hit.is_none_or(|(ht, _)| t < ht)
                    {
                        hit = Some((t, None));
                    }
                }
            }
            let Some((t, target)) = hit else {
                continue;
            };
            let p = from + dir * t;
            self.projectiles[pi].life = 0.0;
            self.projectiles[pi].pos = p;
            self.events.push(SimEvent::ProjectileHit { pos: p });
            if let Some(bi) = target {
                let b = &mut self.bodies[bi];
                b.vel += vel * (0.6 / b.mass);
                match &mut b.kind {
                    BodyKind::Asteroid { hp, .. } => *hp -= 25.0,
                    BodyKind::Wreck { scrap, .. } => *scrap = (*scrap - 0.3).max(0.0),
                    BodyKind::Meteor => {
                        b.alive = false;
                        let (pos, r) = (b.pos, b.radius);
                        self.events.push(SimEvent::Explosion {
                            pos,
                            size: r * 2.0,
                            color: [1.0, 0.55, 0.2],
                        });
                    }
                    _ => {}
                }
            }
        }
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
    fn asteroid_fields_stay_populated() {
        let data = Arc::new(GameData::load().unwrap());
        let save = CrewSave::new_game(&data);
        let def = data.ship(&save.current_ship).clone();
        let mut s = SimState::new(data.clone(), &save, Loadout::full(&def), 1);
        for _ in 0..600 {
            s.step(&TickInput::default());
        }
        let f = &data.world.asteroid_fields[0];
        let c = Vec2::new(f.center.0, f.center.1);
        let inside = s
            .bodies
            .iter()
            .filter(|b| {
                matches!(b.kind, BodyKind::Asteroid { field: 0, .. })
                    && (b.pos - c).length() < f.radius * 1.2
            })
            .count();
        assert!(
            inside as u32 > f.count * 3 / 4,
            "nur {inside} von {} Asteroiden im Feld",
            f.count
        );
    }
}
