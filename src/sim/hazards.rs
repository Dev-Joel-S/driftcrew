//! Gefahren und bewegliche Objekte: Asteroidenfelder, Meteoriten, Geschosse.

use crate::sim::fx::{Fx, Vec2};

use super::data::Ore;
use super::geom::{ray_circle, ray_poly, rot};
use super::rng::hash32;
use super::world::Shape;
use super::{Body, BodyKind, DT, SimEvent, SimState};

/// Wracks werden etwas größer dargestellt als das Schiff, nach dem sie aussehen.
pub const WRECK_SCALE: Fx = fx!(1.3);

pub fn asteroid_hp(r: Fx) -> Fx {
    fx!(18.0) * r
}

pub fn asteroid_mass(r: Fx) -> Fx {
    r * r * fx!(2.2)
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
                .map(|p| Vec2::new(p.pos.0, p.pos.1).length() + fx!(0.5) * p.size.0.max(p.size.1))
                .fold(fx!(1.0), Fx::max)
                * WRECK_SCALE
                * fx!(0.8);
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
                ang_vel: self.rng.range(-fx!(0.04), fx!(0.04)),
                radius: r,
                mass: fx!(40.0) + w.scrap,
                prev_pos: pos,
                prev_angle: angle,
                alive: true,
                seed: hash32(id),
                age: fx!(0.0),
            });
        }
    }

    fn random_in_field(&mut self, fi: usize, avoid: Option<Vec2>) -> Vec2 {
        let f = &self.data.world.asteroid_fields[fi];
        let c = Vec2::new(f.center.0, f.center.1);
        let radius = f.radius;
        let mut p = c;
        for _ in 0..12 {
            let a = self.rng.range(fx!(0.0), Fx::TAU);
            let d = radius * self.rng.unit().sqrt();
            p = c + Vec2::new(a.cos(), a.sin()) * d;
            match avoid {
                Some(q) if (p - q).shorter_than(fx!(90.0)) => continue,
                _ => break,
            }
        }
        p
    }

    pub(crate) fn spawn_asteroid(&mut self, field: usize, pos: Vec2, radius: Option<Fx>) -> usize {
        let f = self.data.world.asteroid_fields[field].clone();
        let r = radius.unwrap_or_else(|| {
            let t = self.rng.unit();
            f.min_radius + (f.max_radius - f.min_radius) * (t * t.sqrt())
        });
        let ore = if self.rng.chance(f.ore_chance) {
            // Beimischung: manche Erz-Asteroiden tragen ein anderes Erz als das Feld.
            let roll = self.rng.unit();
            let mut acc = fx!(0.0);
            let mut o = f.ore;
            for (m, c) in &f.mixed {
                acc += c;
                if roll < acc {
                    o = *m;
                    break;
                }
            }
            Some(o)
        } else {
            None
        };
        let vein = ore.is_some() && r >= fx!(2.5) && self.rng.chance(fx!(0.2));
        let id = self.next_id();
        let vel = Vec2::new(
            self.rng.range(-fx!(1.0), fx!(1.0)),
            self.rng.range(-fx!(1.0), fx!(1.0)),
        ) * fx!(1.2);
        let ang_vel = self.rng.range(-fx!(0.6), fx!(0.6));
        let angle = self.rng.range(fx!(0.0), Fx::TAU);
        self.bodies.push(Body {
            id,
            kind: BodyKind::Asteroid {
                ore,
                ore_left: if ore.is_some() {
                    r * fx!(2.2)
                } else {
                    fx!(0.0)
                },
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
            age: fx!(0.0),
        });
        self.bodies.len() - 1
    }

    pub(crate) fn spawn_chunk(&mut self, pos: Vec2, vel: Vec2, ore: Ore, amount: Fx) {
        let id = self.next_id();
        let r = fx!(0.45) + amount.min(fx!(3.0)) * fx!(0.12);
        self.bodies.push(Body {
            id,
            kind: BodyKind::OreChunk { ore, amount },
            pos,
            vel,
            angle: fx!(0.0),
            ang_vel: self.rng.range(-fx!(2.0), fx!(2.0)),
            radius: r,
            mass: fx!(0.4) + amount * fx!(0.3),
            prev_pos: pos,
            prev_angle: fx!(0.0),
            alive: true,
            seed: hash32(id),
            age: fx!(0.0),
        });
    }

    pub(crate) fn update_hazards(&mut self) {
        self.border_warn = (self.border_warn - DT).max(-fx!(1.0));

        // Meteoritenschauer in der Nähe des Schiffs.
        let zones = self.data.world.meteor_zones.clone();
        for (zi, z) in zones.iter().enumerate() {
            let c = Vec2::new(z.center.0, z.center.1);
            if (self.ship.pos - c).longer_than(z.radius + fx!(380.0))
                || self.effect_near(super::data::EffectKind::Cleared, c)
            {
                continue;
            }
            self.meteor_timers[zi] -= DT;
            if self.meteor_timers[zi] <= fx!(0.0) {
                self.meteor_timers[zi] = z.interval * self.rng.range(fx!(0.6), fx!(1.4));
                let a = self.rng.range(fx!(0.0), Fx::TAU);
                let start = c + Vec2::new(a.cos(), a.sin()) * z.radius * fx!(1.1);
                let ta = self.rng.range(fx!(0.0), Fx::TAU);
                let target =
                    c + Vec2::new(ta.cos(), ta.sin()) * z.radius * fx!(0.6) * self.rng.unit();
                let dir = (target - start).normalize_or_zero();
                let speed = self.rng.range(z.speed.0, z.speed.1);
                let r = self.rng.range(fx!(0.7), fx!(1.7));
                let id = self.next_id();
                self.bodies.push(Body {
                    id,
                    kind: BodyKind::Meteor,
                    pos: start,
                    vel: dir * speed,
                    angle: fx!(0.0),
                    ang_vel: self.rng.range(-fx!(3.0), fx!(3.0)),
                    radius: r,
                    mass: r * r * fx!(3.0),
                    prev_pos: start,
                    prev_angle: fx!(0.0),
                    alive: true,
                    seed: hash32(id),
                    age: fx!(0.0),
                });
            }
        }
        // Meteoriten außerhalb aller Zonen verschwinden.
        for b in &mut self.bodies {
            if !matches!(b.kind, BodyKind::Meteor) || b.age < fx!(3.0) {
                continue;
            }
            let inside = zones.iter().any(|z| {
                (b.pos - Vec2::new(z.center.0, z.center.1)).shorter_than(z.radius * fx!(1.3))
            });
            if !inside || b.age > fx!(40.0) {
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
                if hp <= fx!(0.0) {
                    to_break.push(bi);
                    continue;
                }
                let f = &fields[field];
                let c = Vec2::new(f.center.0, f.center.1);
                let d = b.pos - c;
                if d.length() > f.radius * fx!(1.15) {
                    b.vel -= d.normalize_or_zero() * fx!(0.6) * DT;
                }
                // Etwas Reibung, damit Felder über lange Zeit ruhig bleiben.
                b.vel *= fx!(1.0) - fx!(0.02) * DT;
            }
            if let BodyKind::OreChunk { .. }
            | BodyKind::Capsule { .. }
            | BodyKind::Crate { .. }
            | BodyKind::Salvage { .. } = b.kind
            {
                b.vel *= fx!(1.0) - fx!(0.05) * DT;
            }
            // Abgeworfene Fracht kommt zur Ruhe, damit man sie später wiederfindet.
            if let BodyKind::Dropped { .. } = b.kind {
                b.vel *= fx!(1.0) - fx!(0.25) * DT;
                b.ang_vel *= fx!(1.0) - fx!(0.1) * DT;
            }
            // Artefakte kommen von selbst zur Ruhe.
            if let BodyKind::Artifact { .. } = b.kind {
                b.vel *= fx!(1.0) - fx!(0.3) * DT;
                b.ang_vel += (fx!(0.4) - b.ang_vel) * fx!(0.5) * DT;
            }
            // Wracks treiben träge zurück an ihren Platz, außer jemand schleppt sie weit weg.
            if let BodyKind::Wreck { home, .. } = b.kind {
                let d = home - b.pos;
                if d.length() < fx!(150.0) {
                    b.vel += d * fx!(0.004) * DT;
                }
                b.vel *= fx!(1.0) - fx!(0.15) * DT;
                b.ang_vel *= fx!(1.0) - fx!(0.05) * DT;
            }
        }
        for bi in to_break {
            self.break_asteroid(bi);
        }
        for (fi, field_def) in fields.iter().enumerate() {
            self.field_respawn[fi] -= DT;
            if self.field_respawn[fi] > fx!(0.0) {
                continue;
            }
            self.field_respawn[fi] = fx!(3.0);
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
                d.amount = (d.amount + fx!(0.04) * DT).min(d.max);
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
        let color = ore
            .map(|o| o.color())
            .unwrap_or([fx!(0.85), fx!(0.6), fx!(0.45)]);
        self.events.push(SimEvent::Explosion {
            pos: b.pos,
            size: b.radius * fx!(1.6),
            color,
        });
        if b.radius > fx!(2.4) {
            let side = rot(Vec2::X, self.rng.range(fx!(0.0), Fx::TAU));
            for s in [-fx!(1.0), fx!(1.0)] {
                let r = b.radius * fx!(0.6);
                let idx = self.spawn_asteroid(field, b.pos + side * s * r * fx!(1.05), Some(r));
                let nb = &mut self.bodies[idx];
                nb.vel = b.vel + side * s * fx!(2.5);
                if let BodyKind::Asteroid {
                    ore: o,
                    ore_left: ol,
                    ..
                } = &mut nb.kind
                {
                    *o = ore;
                    *ol = ore_left * fx!(0.4);
                }
            }
        }
        if let Some(o) = ore {
            let chunks = if b.radius > fx!(3.0) { 3 } else { 2 };
            let amount = (ore_left * fx!(0.6) / Fx::from(chunks)).max(fx!(0.6));
            for k in 0..chunks {
                let a =
                    Fx::TAU * Fx::from(k) / Fx::from(chunks) + self.rng.range(fx!(0.0), fx!(1.0));
                let dir = Vec2::new(a.cos(), a.sin());
                self.spawn_chunk(
                    b.pos + dir * b.radius * fx!(0.5),
                    b.vel + dir * fx!(2.0),
                    o,
                    amount,
                );
            }
        }
    }

    pub(crate) fn update_projectiles(&mut self) {
        let cannon_damage = self.ship.cannon_damage;
        for pi in 0..self.projectiles.len() {
            let (from, vel) = (self.projectiles[pi].pos, self.projectiles[pi].vel);
            let to = from + vel * DT;
            self.projectiles[pi].pos = to;
            self.projectiles[pi].life -= DT;
            let seg = to - from;
            let len = seg.length();
            if len < fx!(1e-6) {
                continue;
            }
            let dir = seg / len;
            let mut hit: Option<(Fx, Option<usize>)> = None;
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
                if (sp.pos - from).longer_than(sp.reach() + len + fx!(1.0)) {
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
            // NPC-Schiffe (Drohnen treffen keine Drohnen) und – bei Piratenschüssen – die Crew.
            let hostile = self.projectiles[pi].hostile;
            let mut ship_hit: Option<(Fx, Option<usize>)> = None;
            for (ni, n) in self.npcs.iter().enumerate() {
                if !n.alive
                    || (hostile && n.hostile)
                    || (n.ship.pos - from).longer_than(len + n.ship.bound_radius() + fx!(1.0))
                {
                    continue;
                }
                for q in n.ship.quads() {
                    if let Some(t) = ray_poly(from, dir, len, &q)
                        && ship_hit.is_none_or(|(ht, _)| t < ht)
                    {
                        ship_hit = Some((t, Some(ni)));
                    }
                }
            }
            if hostile
                && !self.ship.destroyed
                && self.ship.docked.is_none()
                && (self.ship.pos - from).shorter_than(len + self.ship.bound_radius() + fx!(1.0))
            {
                for q in self.ship.quads() {
                    if let Some(t) = ray_poly(from, dir, len, &q)
                        && ship_hit.is_none_or(|(ht, _)| t < ht)
                    {
                        ship_hit = Some((t, None));
                    }
                }
            }
            if let Some((st, who)) = ship_hit
                && hit.is_none_or(|(ht, _)| st < ht)
            {
                let p = from + dir * st;
                self.projectiles[pi].life = fx!(0.0);
                self.projectiles[pi].pos = p;
                self.events.push(SimEvent::ProjectileHit { pos: p });
                match who {
                    Some(ni) => {
                        let n = &mut self.npcs[ni].ship;
                        n.vel += vel * (fx!(0.6) / n.mass);
                        let dmg = if hostile {
                            super::npc::DRONE_DAMAGE
                        } else {
                            super::npc::SHOT_DAMAGE_NPC
                        };
                        self.damage_npc(ni, dmg);
                    }
                    None => {
                        self.ship.apply_impulse(vel * fx!(0.6), p);
                        self.ship_damage(super::npc::DRONE_DAMAGE, p, true);
                    }
                }
                continue;
            }
            let Some((t, target)) = hit else {
                continue;
            };
            let p = from + dir * t;
            self.projectiles[pi].life = fx!(0.0);
            self.projectiles[pi].pos = p;
            self.events.push(SimEvent::ProjectileHit { pos: p });
            if let Some(bi) = target {
                let b = &mut self.bodies[bi];
                b.vel += vel * (fx!(0.6) / b.mass);
                match &mut b.kind {
                    BodyKind::Asteroid { hp, .. } => *hp -= fx!(25.0) * cannon_damage,
                    BodyKind::Wreck { scrap, .. } => *scrap = (*scrap - fx!(0.3)).max(fx!(0.0)),
                    BodyKind::Meteor => {
                        b.alive = false;
                        let (pos, r) = (b.pos, b.radius);
                        self.events.push(SimEvent::Explosion {
                            pos,
                            size: r * fx!(2.0),
                            color: [fx!(1.0), fx!(0.55), fx!(0.2)],
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
                    && (b.pos - c).shorter_than(f.radius * fx!(1.2))
            })
            .count();
        assert!(
            inside as u32 > f.count * 3 / 4,
            "nur {inside} von {} Asteroiden im Feld",
            f.count
        );
    }
}
