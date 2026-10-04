//! Eigene Starrkörperphysik: Kräfte, Integration, Kontakte mit Impulsen.

use bevy::math::Vec2;

use super::geom::{Aabb, Contact, circle_circle, cross, cross_sv, poly_circle, poly_poly};
use super::ship::{CargoItem, CargoKind, Ship};
use super::world::{Shape, Surface, World};
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
        self.artifact_spin();
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
            // Gefahr im Kern der Anomalie, Ereignishorizont des Schwarzen Lochs.
            let pos = self.ship.pos;
            let mut core_damage = false;
            let mut swallowed = false;
            let mut pulled_by = None;
            for (ai, an) in self.world.anomalies.iter().enumerate() {
                let d = (pos - an.pos).length();
                if an.is_black_hole() {
                    swallowed |= d < an.core_radius + 0.5;
                } else {
                    core_damage |= d < an.core_radius + 2.0;
                }
                if d < an.radius * 0.8 {
                    pulled_by = Some(ai);
                }
            }
            if core_damage {
                self.ship_damage(30.0 * DT, pos, false);
            }
            if swallowed {
                self.ship.invulnerable = 0.0;
                self.ship.shield = 0.0;
                self.ship.hull = 0.0;
                self.toast("Vom Schwarzen Loch verschluckt!", ToastKind::Bad);
            }
            if pulled_by != self.pull_warned {
                if let Some(ai) = pulled_by {
                    let an = &self.world.anomalies[ai];
                    let msg = if an.is_black_hole() {
                        format!("{}: starker Sog – Abstand halten!", an.name)
                    } else {
                        format!("{}: Anziehung spürbar", an.name)
                    };
                    self.toast(msg, ToastKind::Warn);
                }
                self.pull_warned = pulled_by;
            }
        }
        for i in 0..self.bodies.len() {
            // Meteore fliegen gerade, Artefakte folgen keinem Sog (fremde Physik).
            if matches!(
                self.bodies[i].kind,
                BodyKind::Meteor | BodyKind::Artifact { .. }
            ) {
                continue;
            }
            let g = self.world.gravity(self.bodies[i].pos);
            let swallowed = self.world.anomalies.iter().any(|an| {
                an.is_black_hole() && (self.bodies[i].pos - an.pos).length() < an.core_radius
            });
            let b = &mut self.bodies[i];
            if swallowed {
                b.alive = false;
                continue;
            }
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
        let contacts = static_contacts(&self.ship, &self.world);
        if contacts.is_empty() {
            return;
        }
        let (max_impact, impact_at) = resolve_static(&mut self.ship, &contacts);
        if max_impact > 1.5 {
            self.events.push(SimEvent::Impact {
                pos: impact_at.0,
                normal: impact_at.1,
                strength: max_impact,
            });
            self.count_collision(impact_at.0);
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
            for (cp, cr) in b.circles().iter() {
                for q in &quads {
                    if let Some(c) = poly_circle(q, cp, cr) {
                        tmp.push((bi, c));
                    }
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
                self.count_collision(c.point);
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
            let circles = self.bodies[bi].circles();
            let mut contacts: Vec<(Contact, Vec2)> = Vec::new();
            for col in &self.world.colliders {
                if !col.enabled || !col.aabb.overlaps(&bound) {
                    continue;
                }
                for (cp, cr) in circles.iter() {
                    let c = match col.shape {
                        Shape::Poly(q) => poly_circle(&q, cp, cr),
                        Shape::Circle { c, r: pr } => circle_circle(c, pr, cp, cr),
                    };
                    if let Some(c) = c {
                        contacts.push((c, Vec2::ZERO));
                    }
                }
            }
            for (si, sp) in self.world.spinners.iter().enumerate() {
                if (sp.pos - pos).length() > sp.reach() + r + 1.0 {
                    continue;
                }
                for arm in sp.quads() {
                    for (cp, cr) in circles.iter() {
                        if let Some(c) = poly_circle(&arm, cp, cr) {
                            contacts.push((c, self.world.spinner_velocity(si, c.point)));
                        }
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
                if (a.pos - b.pos).length() > a.radius + b.radius {
                    continue;
                }
                // Tiefster Kontakt zwischen den Kreisen beider Körper.
                let (ca, cb) = (a.circles(), b.circles());
                let mut best: Option<Contact> = None;
                for (pa, ra) in ca.iter() {
                    for (pb, rb) in cb.iter() {
                        if let Some(c) = circle_circle(pa, ra, pb, rb)
                            && best.is_none_or(|x| c.depth > x.depth)
                        {
                            best = Some(c);
                        }
                    }
                }
                let Some(c) = best else {
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

    pub(crate) fn count_collision(&mut self, at: Vec2) {
        if let Some(slot) = self.nearest_slot(at) {
            self.stats.slot(slot).collisions += 1;
        }
        // Jeder Rums verschüttet den Passagieren den Kaffee.
        for m in &mut self.active {
            if let super::missions::MissionKind::Passengers { comfort, .. } = &mut m.kind {
                *comfort = (*comfort - 0.05).max(0.0);
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
        self.stats.damage += amount;
        self.ship.since_hit = 0.0;
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
            if show {
                self.damage_thrusters(left, at);
            }
        }
        if show {
            self.events.push(SimEvent::Damage { amount });
        }
    }

    /// Treffer in der Nähe eines Triebwerks beschädigen es (Stottern, dann Ausfall).
    fn damage_thrusters(&mut self, hull_damage: f32, at: Vec2) {
        const REACH: f32 = 2.2;
        let n = self.ship.thrusters.len();
        let mut msgs = Vec::new();
        for i in 0..n {
            let wp = self.ship.to_world(self.ship.thrusters[i].pos);
            let d = (wp - at).length();
            if d >= REACH {
                continue;
            }
            let t = &mut self.ship.thrusters[i];
            let (was_stutter, was_failed) = (t.stuttering(), t.failed());
            t.health = (t.health - hull_damage * 0.028 * (1.0 - d / REACH)).max(0.0);
            let name = super::thruster_label(i, n);
            if t.failed() && !was_failed {
                msgs.push((format!("Triebwerk {name} ausgefallen!"), ToastKind::Bad));
            } else if t.stuttering() && !was_stutter && !was_failed {
                msgs.push((
                    format!("Triebwerk {name} beschädigt – stottert"),
                    ToastKind::Warn,
                ));
            }
        }
        for (m, k) in msgs {
            self.toast(m, k);
        }
    }

    /// Kleines Objekt in den Frachtraum. Gibt true zurück, wenn es geklappt hat.
    pub(crate) fn try_stow(&mut self, bi: usize) -> bool {
        let b = self.bodies[bi].clone();
        let none = super::data::CargoTrait::None;
        let (item, label) = match &b.kind {
            BodyKind::OreChunk { ore, amount } => (
                CargoItem::new(CargoKind::Ore(*ore), *amount, none),
                format!("{:.1} t {}", amount, ore.label()),
            ),
            BodyKind::Capsule { mission } => (
                CargoItem::new(CargoKind::Capsule { mission: *mission }, 0.8, none),
                "Rettungskapsel".to_string(),
            ),
            BodyKind::Salvage { name, value } => (
                CargoItem::new(
                    CargoKind::Salvage {
                        name: name.clone(),
                        value: *value,
                    },
                    b.mass,
                    none,
                ),
                format!("Bauteil: {name}"),
            ),
            BodyKind::Artifact { id } => (
                CargoItem::new(CargoKind::Artifact { id: id.clone() }, b.mass, none),
                format!("Artefakt: {}", self.artifact_name(id)),
            ),
            // Abgeworfenes kommt mit Zustand und Eigenschaften zurück.
            BodyKind::Dropped { item } => (item.clone(), self.cargo_label(&item.kind)),
            _ => return false,
        };
        // Erz lässt sich auf Module verteilen, alles andere braucht ein Modul mit genug Platz.
        let fits = if matches!(item.kind, CargoKind::Ore(_)) {
            self.ship.cargo_capacity() - self.ship.cargo_mass()
        } else {
            (0..self.ship.pods.len())
                .map(|i| self.ship.pod_free(i))
                .fold(0.0, f32::max)
        };
        if fits < item.mass - 1e-3 {
            if self.border_warn <= 0.0 {
                self.border_warn = 2.0;
                self.toast("Frachtraum voll", ToastKind::Warn);
            }
            return false;
        }
        if self.ship.store_item(item) <= 0.0 {
            return false;
        }
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

/// Kontakte eines Schiffs mit der festen Welt (Stationen, Planeten, Rotoren):
/// (Kontakt, Geschwindigkeit der Oberfläche, Oberfläche). Für Crew- und NPC-Schiffe gleich.
pub fn static_contacts(ship: &Ship, world: &World) -> Vec<(Contact, Vec2, Surface)> {
    let quads = ship.quads();
    let bound = Aabb::around(ship.pos, ship.bound_radius() + 0.5);
    let mut contacts: Vec<(Contact, Vec2, Surface)> = Vec::new();
    let mut tmp = Vec::new();
    for col in &world.colliders {
        if !col.enabled || !col.aabb.overlaps(&bound) {
            continue;
        }
        for q in &quads {
            match col.shape {
                Shape::Poly(sq) => {
                    tmp.clear();
                    poly_poly(&sq, q, &mut tmp);
                    for c in &tmp {
                        contacts.push((*c, Vec2::ZERO, col.surface));
                    }
                }
                Shape::Circle { c, r } => {
                    if let Some(ct) = poly_circle(q, c, r) {
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
    for (si, sp) in world.spinners.iter().enumerate() {
        if (sp.pos - ship.pos).length() > sp.reach() + ship.bound_radius() + 1.0 {
            continue;
        }
        for arm in sp.quads() {
            for q in &quads {
                tmp.clear();
                poly_poly(&arm, q, &mut tmp);
                for c in &tmp {
                    let v = world.spinner_velocity(si, c.point);
                    contacts.push((*c, v, Surface::Block));
                }
            }
        }
    }
    contacts
}

/// Kontakte mit der festen Welt auflösen. Ergebnis: stärkster Stoß und wo er war.
pub fn resolve_static(
    ship: &mut Ship,
    contacts: &[(Contact, Vec2, Surface)],
) -> (f32, (Vec2, Vec2)) {
    let mut d = Dyn {
        pos: ship.pos,
        vel: ship.vel,
        w: ship.ang_vel,
        inv_m: 1.0 / ship.mass,
        inv_i: 1.0 / ship.inertia,
    };
    let mut max_impact = 0.0f32;
    let mut impact_at = (Vec2::ZERO, Vec2::Y);
    let share = 1.0 / contacts.len().max(1) as f32;
    for iter in 0..4 {
        for (c, sv, surface) in contacts {
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
    ship.pos = d.pos;
    ship.vel = d.vel;
    ship.ang_vel = d.w;
    (max_impact, impact_at)
}

/// Zwei Schiffe stoßen aneinander (Crew gegen NPC oder NPC gegen NPC). Ergebnis: Stoßstärke
/// und Ort, falls sie sich berühren.
pub fn collide_two_ships(a: &mut Ship, b: &mut Ship) -> Option<(f32, Vec2)> {
    if (a.pos - b.pos).length() > a.bound_radius() + b.bound_radius() + 0.2 {
        return None;
    }
    let qa = a.quads();
    let qb = b.quads();
    let mut contacts = Vec::new();
    let mut tmp = Vec::new();
    for x in &qa {
        for y in &qb {
            tmp.clear();
            poly_poly(x, y, &mut tmp);
            contacts.extend(tmp.iter().copied());
        }
    }
    if contacts.is_empty() {
        return None;
    }
    let mut da = Dyn {
        pos: a.pos,
        vel: a.vel,
        w: a.ang_vel,
        inv_m: 1.0 / a.mass,
        inv_i: 1.0 / a.inertia,
    };
    let mut db = Dyn {
        pos: b.pos,
        vel: b.vel,
        w: b.ang_vel,
        inv_m: 1.0 / b.mass,
        inv_i: 1.0 / b.inertia,
    };
    let share = 1.0 / contacts.len() as f32;
    let mut max_imp = 0.0f32;
    let mut at = contacts[0].point;
    for iter in 0..4 {
        for c in &contacts {
            let imp = solve_contact(
                &mut da,
                &mut db,
                c,
                0.3,
                0.4,
                Vec2::ZERO,
                if iter == 0 { share } else { 0.0 },
            );
            if iter == 0 && imp > max_imp {
                max_imp = imp;
                at = c.point;
            }
        }
    }
    a.pos = da.pos;
    a.vel = da.vel;
    a.ang_vel = da.w;
    b.pos = db.pos;
    b.vel = db.vel;
    b.ang_vel = db.w;
    Some((max_imp, at))
}

pub(crate) fn b_mass(r: f32) -> f32 {
    r * r * 3.0
}

/// Vorhersage der Flugbahn ohne weitere Eingabe (nur zur Anzeige, ändert nichts).
#[derive(Clone, Debug, Default)]
pub struct Prediction {
    pub points: Vec<Vec2>,
    /// Erster voraussichtlicher Aufprall (Ort, Aufprallgeschwindigkeit).
    pub hit: Option<(Vec2, f32)>,
}

impl SimState {
    /// Integriert Position und Geschwindigkeit des Schiffs `seconds` voraus (mit Schwerkraft,
    /// ohne Schub) und meldet die erste Berührung mit statischer Geometrie.
    pub fn predict_path(&self, seconds: f32, step: f32) -> Prediction {
        let mut out = Prediction::default();
        if self.ship.docked.is_some() || self.ship.destroyed {
            return out;
        }
        let r = self.ship.bound_radius() * 0.55;
        let mut p = self.ship.pos;
        let mut v = self.ship.vel;
        out.points.push(p);
        let steps = (seconds / step) as usize;
        for _ in 0..steps {
            v += self.world.gravity(p) * step;
            p += v * step;
            out.points.push(p);
            let bound = Aabb::around(p, r);
            let hit = self.world.colliders.iter().any(|c| {
                c.enabled
                    && c.aabb.overlaps(&bound)
                    && match c.shape {
                        Shape::Poly(q) => poly_circle(&q, p, r).is_some(),
                        Shape::Circle { c, r: cr } => (p - c).length() < cr + r,
                    }
            });
            if hit {
                out.hit = Some((p, v.length()));
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod prediction_tests {
    use super::*;
    use crate::sim::data::{CrewSave, GameData};
    use crate::sim::ship::Loadout;
    use std::sync::Arc;

    #[test]
    fn prediction_finds_wall_ahead() {
        let data = Arc::new(GameData::embedded().unwrap());
        let save = CrewSave::new_game(&data);
        let def = data.ship(&save.current_ship).clone();
        let mut s = SimState::new(data, &save, Loadout::full(&def), 1);
        s.ship.docked = None;
        // Mitten in Nova-Hub, mit Tempo auf die obere Ringwand zu.
        s.ship.pos = Vec2::new(0.0, 20.0);
        s.ship.vel = Vec2::new(0.0, 10.0);
        let p = s.predict_path(4.0, 0.05);
        let (hit, speed) = p.hit.expect("Aufprall erwartet");
        assert!(hit.y > 25.0 && hit.y < 46.0, "Aufprall bei {hit:?}");
        assert!((speed - 10.0).abs() < 0.1);
        // Freier Raum: kein Aufprall, Gerade.
        s.ship.pos = Vec2::new(0.0, 400.0);
        let p = s.predict_path(2.0, 0.05);
        assert!(p.hit.is_none());
        assert!((p.points.last().unwrap().y - 420.0).abs() < 0.5);
    }
}
