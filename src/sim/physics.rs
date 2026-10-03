//! Eigene Starrkörperphysik: Kräfte, Integration, Kontakte mit Impulsen.

use bevy::math::Vec2;

use super::geom::{Aabb, Contact, circle_circle, cross, cross_sv, quad_circle, quad_quad};
use super::ship::CargoKind;
use super::world::{Shape, Surface};
use super::{BodyKind, DT, SimEvent, SimState, ToastKind};

const SLOP: f32 = 0.01;
/// Aufprallgeschwindigkeit, ab der die Hülle Schaden nimmt.
pub const SAFE_IMPACT: f32 = 6.0;

#[derive(Clone, Copy, Debug)]
pub struct Dyn {
    pub pos: Vec2,
    pub vel: Vec2,
    pub w: f32,
    pub inv_m: f32,
    pub inv_i: f32,
}

impl Dyn {
    pub const STATIC: Dyn = Dyn {
        pos: Vec2::ZERO,
        vel: Vec2::ZERO,
        w: 0.0,
        inv_m: 0.0,
        inv_i: 0.0,
    };
    fn point_vel(&self, p: Vec2) -> Vec2 {
        self.vel + cross_sv(self.w, p - self.pos)
    }
    fn apply(&mut self, imp: Vec2, r: Vec2) {
        self.vel += imp * self.inv_m;
        self.w += cross(r, imp) * self.inv_i;
    }
}

/// Löst einen Kontakt (Normale von A nach B). Liefert die Aufprallgeschwindigkeit.
pub fn solve_contact(
    a: &mut Dyn,
    b: &mut Dyn,
    c: &Contact,
    e: f32,
    mu: f32,
    a_surface_vel: Vec2,
    pos_share: f32,
) -> f32 {
    let n = c.normal;
    let ra = c.point - a.pos;
    let rb = c.point - b.pos;
    let inv_lin = a.inv_m + b.inv_m;
    if inv_lin <= 0.0 {
        return 0.0;
    }
    if pos_share > 0.0 {
        let corr = (c.depth - SLOP).max(0.0) * 0.85 * pos_share;
        a.pos -= n * corr * a.inv_m / inv_lin;
        b.pos += n * corr * b.inv_m / inv_lin;
    }
    let rel = b.point_vel(c.point) - (a.point_vel(c.point) + a_surface_vel);
    let vn = rel.dot(n);
    if vn >= 0.0 {
        return 0.0;
    }
    let ran = cross(ra, n);
    let rbn = cross(rb, n);
    let k = inv_lin + ran * ran * a.inv_i + rbn * rbn * b.inv_i;
    let e = if -vn < 1.2 { 0.0 } else { e };
    let j = -(1.0 + e) * vn / k;
    a.apply(-n * j, ra);
    b.apply(n * j, rb);

    let rel2 = b.point_vel(c.point) - (a.point_vel(c.point) + a_surface_vel);
    let tv = rel2 - n * rel2.dot(n);
    let tl = tv.length();
    if tl > 1e-6 {
        let t = tv / tl;
        let rat = cross(ra, t);
        let rbt = cross(rb, t);
        let kt = inv_lin + rat * rat * a.inv_i + rbt * rbt * b.inv_i;
        let jt = (-rel2.dot(t) / kt).clamp(-mu * j, mu * j);
        a.apply(-t * jt, ra);
        b.apply(t * jt, rb);
    }
    -vn
}

impl SimState {
    pub(crate) fn apply_forces(&mut self) {
        let world_r = self.world.radius;
        if self.ship.docked.is_none() && !self.ship.destroyed {
            let g = self.world.gravity(self.ship.pos);
            self.ship.vel += g * DT;
            let d = self.ship.pos.length();
            if d > world_r {
                self.ship.vel -= self.ship.pos / d * 8.0 * DT;
                self.border_warn -= DT;
                if self.border_warn <= 0.0 {
                    self.border_warn = 4.0;
                    self.toast("Rand des Sektors – Strömung drückt zurück", ToastKind::Warn);
                }
            }
            // Gefahr im Kern der Anomalie.
            let pos = self.ship.pos;
            let in_core = self
                .world
                .anomalies
                .iter()
                .any(|an| (pos - an.pos).length() < an.core_radius + 2.0);
            if in_core {
                self.ship_damage(30.0 * DT, pos, false);
            }
        }
        for i in 0..self.bodies.len() {
            if matches!(self.bodies[i].kind, BodyKind::Meteor) {
                continue;
            }
            let g = self.world.gravity(self.bodies[i].pos);
            let b = &mut self.bodies[i];
            b.vel += g * DT;
            let d = b.pos.length();
            if d > world_r {
                b.vel -= b.pos / d * 8.0 * DT;
            }
        }
    }

    pub(crate) fn integrate(&mut self) {
        let s = &mut self.ship;
        if s.docked.is_none() && !s.destroyed {
            s.ang_vel *= 1.0 - (s.ang_damp * DT).min(0.5);
            s.pos += s.vel * DT;
            s.angle += s.ang_vel * DT;
        }
        s.dock_cooldown = (s.dock_cooldown - DT).max(0.0);
        s.invulnerable = (s.invulnerable - DT).max(0.0);
        for b in &mut self.bodies {
            b.pos += b.vel * DT;
            b.angle += b.ang_vel * DT;
            b.age += DT;
        }
    }

    pub(crate) fn collide(&mut self) {
        if self.ship.docked.is_none() && !self.ship.destroyed {
            self.collide_ship_static();
            self.collide_ship_bodies();
        }
        self.collide_bodies_static();
        self.collide_bodies_bodies();
    }

    fn ship_dyn(&self) -> Dyn {
        Dyn {
            pos: self.ship.pos,
            vel: self.ship.vel,
            w: self.ship.ang_vel,
            inv_m: 1.0 / self.ship.mass,
            inv_i: 1.0 / self.ship.inertia,
        }
    }

    fn set_ship_dyn(&mut self, d: Dyn) {
        self.ship.pos = d.pos;
        self.ship.vel = d.vel;
        self.ship.ang_vel = d.w;
    }

    fn collide_ship_static(&mut self) {
        let quads = self.ship.quads();
        let bound = Aabb::around(self.ship.pos, self.ship.bound_radius() + 0.5);
        // (Kontakt, Oberflächengeschwindigkeit, Oberfläche)
        let mut contacts: Vec<(Contact, Vec2, Surface)> = Vec::new();
        let mut tmp = Vec::new();
        for col in &self.world.colliders {
            if !col.aabb.overlaps(&bound) {
                continue;
            }
            for q in &quads {
                match col.shape {
                    Shape::Quad(sq) => {
                        tmp.clear();
                        quad_quad(&sq, q, &mut tmp);
                        for c in &tmp {
                            contacts.push((*c, Vec2::ZERO, col.surface));
                        }
                    }
                    Shape::Circle { c, r } => {
                        if let Some(ct) = quad_circle(q, c, r) {
                            contacts.push((
                                Contact {
                                    point: ct.point,
                                    normal: -ct.normal,
                                    depth: ct.depth,
                                },
                                Vec2::ZERO,
                                col.surface,
                            ));
                        }
                    }
                }
            }
        }
        for (si, sp) in self.world.spinners.iter().enumerate() {
            if (sp.pos - self.ship.pos).length() > sp.arm_length + self.ship.bound_radius() + 1.0 {
                continue;
            }
            for arm in sp.quads() {
                for q in &quads {
                    tmp.clear();
                    quad_quad(&arm, q, &mut tmp);
                    for c in &tmp {
                        let v = self.world.spinner_velocity(si, c.point);
                        contacts.push((*c, v, Surface::Block));
                    }
                }
            }
        }
        if contacts.is_empty() {
            return;
        }
        let mut d = self.ship_dyn();
        let mut max_impact = 0.0f32;
        let mut impact_at = (Vec2::ZERO, Vec2::Y);
        let share = 1.0 / contacts.len() as f32;
        for iter in 0..4 {
            for (c, sv, surface) in &contacts {
                let mut stat = Dyn::STATIC;
                let (e, mu) = match surface {
                    Surface::Pad(_) => (0.1, 0.9),
                    Surface::Planet(_) => (0.25, 0.7),
                    Surface::Block | Surface::Structure => (0.35, 0.5),
                };
                let imp = solve_contact(
                    &mut stat,
                    &mut d,
                    c,
                    e,
                    mu,
                    *sv,
                    if iter == 0 { share } else { 0.0 },
                );
                if iter == 0 && imp > max_impact {
                    max_impact = imp;
                    impact_at = (c.point, c.normal);
                }
            }
        }
        self.set_ship_dyn(d);
        if max_impact > 1.5 {
            self.events.push(SimEvent::Impact {
                pos: impact_at.0,
                normal: impact_at.1,
                strength: max_impact,
            });
        }
        if max_impact > SAFE_IMPACT {
            let dmg = (max_impact - SAFE_IMPACT) * 3.6;
            self.ship_damage(dmg, impact_at.0, true);
        }
    }

    fn collide_ship_bodies(&mut self) {
        let quads = self.ship.quads();
        let reach = self.ship.bound_radius();
        let mut tmp: Vec<(usize, Contact)> = Vec::new();
        for (bi, b) in self.bodies.iter().enumerate() {
            if !b.alive || (b.pos - self.ship.pos).length() > reach + b.radius + 0.5 {
                continue;
            }
            for q in &quads {
                if let Some(c) = quad_circle(q, b.pos, b.radius) {
                    tmp.push((bi, c));
                }
            }
        }
        for (bi, c) in tmp {
            if !self.bodies[bi].alive {
                continue;
            }
            // Kleinteile werden bei sanfter Berührung direkt eingesammelt.
            let rel = (self.bodies[bi].vel - self.ship.point_velocity(c.point)).length();
            if self.bodies[bi].stowable() && rel < 7.0 && self.try_stow(bi) {
                continue;
            }
            if matches!(self.bodies[bi].kind, BodyKind::Meteor) {
                let b = &mut self.bodies[bi];
                b.alive = false;
                let dmg = 10.0 + rel * 0.7 * b.radius;
                let pos = b.pos;
                let r = b.radius;
                self.events.push(SimEvent::Explosion {
                    pos,
                    size: r * 2.0,
                    color: [1.0, 0.55, 0.2],
                });
                let push = (self.ship.pos - pos).normalize_or_zero() * b_mass(r) * rel * 0.5;
                self.ship.apply_impulse(push, c.point);
                self.ship_damage(dmg, pos, true);
                continue;
            }
            let mut sd = self.ship_dyn();
            let b = &self.bodies[bi];
            let mut bd = Dyn {
                pos: b.pos,
                vel: b.vel,
                w: b.ang_vel,
                inv_m: 1.0 / b.mass,
                inv_i: 1.0 / b.inertia(),
            };
            let impact = solve_contact(&mut sd, &mut bd, &c, 0.3, 0.4, Vec2::ZERO, 1.0);
            let bmass = b.mass;
            self.set_ship_dyn(sd);
            let b = &mut self.bodies[bi];
            b.pos = bd.pos;
            b.vel = bd.vel;
            b.ang_vel = bd.w;
            if impact > 1.5 {
                self.events.push(SimEvent::Impact {
                    pos: c.point,
                    normal: c.normal,
                    strength: impact,
                });
            }
            if impact > SAFE_IMPACT {
                let factor = (bmass / self.ship.mass * 1.5).clamp(0.15, 1.0);
                self.ship_damage((impact - SAFE_IMPACT) * 3.6 * factor, c.point, true);
            }
        }
    }

    fn collide_bodies_static(&mut self) {
        for bi in 0..self.bodies.len() {
            if !self.bodies[bi].alive {
                continue;
            }
            let (pos, r) = (self.bodies[bi].pos, self.bodies[bi].radius);
            let bound = Aabb::around(pos, r);
            let mut contacts: Vec<(Contact, Vec2)> = Vec::new();
            for col in &self.world.colliders {
                if !col.aabb.overlaps(&bound) {
                    continue;
                }
                let c = match col.shape {
                    Shape::Quad(q) => quad_circle(&q, pos, r),
                    Shape::Circle { c, r: pr } => circle_circle(c, pr, pos, r),
                };
                if let Some(c) = c {
                    contacts.push((c, Vec2::ZERO));
                }
            }
            for (si, sp) in self.world.spinners.iter().enumerate() {
                if (sp.pos - pos).length() > sp.arm_length + r + 1.0 {
                    continue;
                }
                for arm in sp.quads() {
                    if let Some(c) = quad_circle(&arm, pos, r) {
                        contacts.push((c, self.world.spinner_velocity(si, c.point)));
                    }
                }
            }
            if contacts.is_empty() {
                continue;
            }
            if matches!(self.bodies[bi].kind, BodyKind::Meteor) {
                self.bodies[bi].alive = false;
                self.events.push(SimEvent::Explosion {
                    pos,
                    size: r * 2.0,
                    color: [1.0, 0.55, 0.2],
                });
                continue;
            }
            let b = &self.bodies[bi];
            let mut bd = Dyn {
                pos: b.pos,
                vel: b.vel,
                w: b.ang_vel,
                inv_m: 1.0 / b.mass,
                inv_i: 1.0 / b.inertia(),
            };
            let share = 1.0 / contacts.len() as f32;
            for (c, sv) in &contacts {
                let mut st = Dyn::STATIC;
                solve_contact(&mut st, &mut bd, c, 0.4, 0.4, *sv, share);
            }
            let b = &mut self.bodies[bi];
            b.pos = bd.pos;
            b.vel = bd.vel;
            b.ang_vel = bd.w;
        }
    }

    fn collide_bodies_bodies(&mut self) {
        let n = self.bodies.len();
        for i in 0..n {
            if !self.bodies[i].alive {
                continue;
            }
            for j in (i + 1)..n {
                if !self.bodies[j].alive {
                    continue;
                }
                let (a, b) = (&self.bodies[i], &self.bodies[j]);
                let Some(c) = circle_circle(a.pos, a.radius, b.pos, b.radius) else {
                    continue;
                };
                let meteor_a = matches!(a.kind, BodyKind::Meteor);
                let meteor_b = matches!(b.kind, BodyKind::Meteor);
                if meteor_a || meteor_b {
                    // Meteor zerplatzt und schubst den anderen Körper.
                    let (mi, oi) = if meteor_a { (i, j) } else { (j, i) };
                    let m = self.bodies[mi].clone();
                    self.bodies[mi].alive = false;
                    let o = &mut self.bodies[oi];
                    o.vel += m.vel * (m.mass / (m.mass + o.mass));
                    if let BodyKind::Asteroid { hp, .. } = &mut o.kind {
                        *hp -= 15.0;
                    }
                    self.events.push(SimEvent::Explosion {
                        pos: m.pos,
                        size: m.radius * 2.0,
                        color: [1.0, 0.55, 0.2],
                    });
                    continue;
                }
                let mut ad = Dyn {
                    pos: a.pos,
                    vel: a.vel,
                    w: a.ang_vel,
                    inv_m: 1.0 / a.mass,
                    inv_i: 1.0 / a.inertia(),
                };
                let mut bd = Dyn {
                    pos: b.pos,
                    vel: b.vel,
                    w: b.ang_vel,
                    inv_m: 1.0 / b.mass,
                    inv_i: 1.0 / b.inertia(),
                };
                solve_contact(&mut ad, &mut bd, &c, 0.5, 0.3, Vec2::ZERO, 1.0);
                let a = &mut self.bodies[i];
                a.pos = ad.pos;
                a.vel = ad.vel;
                a.ang_vel = ad.w;
                let b = &mut self.bodies[j];
                b.pos = bd.pos;
                b.vel = bd.vel;
                b.ang_vel = bd.w;
            }
        }
    }

    /// Schaden an Schild/Hülle.
    pub(crate) fn ship_damage(&mut self, amount: f32, at: Vec2, show: bool) {
        if amount <= 0.0
            || self.ship.invulnerable > 0.0
            || self.ship.destroyed
            || self.ship.docked.is_some()
        {
            return;
        }
        let mut left = amount;
        if self.ship.shield > 0.0 {
            let absorbed = self.ship.shield.min(left);
            self.ship.shield -= absorbed;
            left -= absorbed;
            if show {
                self.events.push(SimEvent::ShieldHit { pos: at });
            }
        }
        if left > 0.0 {
            self.ship.hull -= left;
        }
        if show {
            self.events.push(SimEvent::Damage { amount });
        }
    }

    /// Kleines Objekt in den Frachtraum. Gibt true zurück, wenn es geklappt hat.
    pub(crate) fn try_stow(&mut self, bi: usize) -> bool {
        let b = self.bodies[bi].clone();
        let (kind, mass, label) = match &b.kind {
            BodyKind::OreChunk { ore, amount } => (
                CargoKind::Ore(*ore),
                *amount,
                format!("{:.1} t {}", amount, ore.label()),
            ),
            BodyKind::Capsule { mission } => (
                CargoKind::Capsule { mission: *mission },
                0.8,
                "Rettungskapsel".to_string(),
            ),
            _ => return false,
        };
        let free = self.ship.cargo_capacity() - self.ship.cargo_mass();
        if free < mass - 1e-3 {
            if self.border_warn <= 0.0 {
                self.border_warn = 2.0;
                self.toast("Frachtraum voll", ToastKind::Warn);
            }
            return false;
        }
        self.ship.store(kind, mass);
        self.bodies[bi].alive = false;
        // Kran lösen, falls er daran hing.
        for t in &mut self.ship.tools {
            if let super::ship::CraneState::Attached { body, .. } = t.crane
                && body == b.id
            {
                t.crane = super::ship::CraneState::Idle;
            }
        }
        self.events.push(SimEvent::Stowed { what: label });
        true
    }
}

fn b_mass(r: f32) -> f32 {
    r * r * 3.0
}
