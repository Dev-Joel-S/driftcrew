//! Slots in Aktion: Triebwerke, Kanone, Kran und Bohrer.

use bevy::math::Vec2;

use super::data::ToolKind;
use super::geom::{cross, ray_circle, ray_quad, rot};
use super::ship::{CargoKind, CraneState, DrillHit};
use super::world::{angle_diff, Shape};
use super::{BodyKind, Projectile, SimEvent, SimState, TickInput, ToastKind, DT};

const SHOT_SPEED: f32 = 75.0;
const SHOT_RECOIL: f32 = 2.2;
const CRANE_SPEED: f32 = 55.0;
const DRILL_RANGE: f32 = 5.5;

impl SimState {
    pub(crate) fn apply_input(&mut self, input: &TickInput) {
        // Während einer Abstimmung sind die Slot-Tasten Stimmen, keine Steuerung.
        let voting = self.vote.is_some();
        let mut any_thrust = false;
        for t in &mut self.ship.thrusters {
            t.firing = !voting && input.pressed(t.slot);
            any_thrust |= t.firing;
            let target = if t.firing { 1.0 } else { 0.0 };
            t.level += (target - t.level) * (DT * 16.0).min(1.0);
        }

        if self.ship.docked.is_some() {
            if any_thrust {
                self.undock();
            } else {
                for t in &mut self.ship.tools {
                    t.prev_pressed = t.pressed;
                    t.pressed = false;
                    if let Some(a) = input.aims.get(t.slot as usize) {
                        t.aim = *a;
                    }
                }
                return;
            }
        }

        // Schub an versetzten Punkten → Kraft + Drehmoment.
        let mut force = Vec2::ZERO;
        let mut torque = 0.0;
        for t in &self.ship.thrusters {
            if !t.firing {
                continue;
            }
            let f = rot(t.dir, self.ship.angle) * t.thrust;
            let at = self.ship.to_world(t.pos);
            force += f;
            torque += cross(at - self.ship.pos, f);
        }
        self.ship.vel += force / self.ship.mass * DT;
        self.ship.ang_vel += torque / self.ship.inertia * DT;

        for i in 0..self.ship.tools.len() {
            {
                let t = &mut self.ship.tools[i];
                t.prev_pressed = t.pressed;
                t.pressed = !voting && input.pressed(t.slot);
                if let Some(a) = input.aims.get(t.slot as usize) {
                    t.aim = *a;
                }
                t.cooldown = (t.cooldown - DT).max(0.0);
            }
            match self.ship.tools[i].kind {
                ToolKind::Cannon => self.update_cannon(i),
                ToolKind::Crane => self.update_crane(i),
                ToolKind::Drill => self.update_drill(i),
            }
        }
    }

    fn update_cannon(&mut self, i: usize) {
        let t = &self.ship.tools[i];
        if !t.just_pressed() || t.cooldown > 0.0 {
            return;
        }
        if self.ship.ammo == 0 {
            self.events.push(SimEvent::EmptyGun);
            return;
        }
        let dir = t.aim_dir();
        let mount = self.ship.tool_world_pos(i);
        let vel = self.ship.point_velocity(mount) + dir * SHOT_SPEED;
        let id = self.next_id();
        self.projectiles.push(Projectile {
            id,
            pos: mount + dir * 0.6,
            prev_pos: mount + dir * 0.6,
            vel,
            life: 1.6,
        });
        self.ship.apply_impulse(-dir * SHOT_RECOIL, mount);
        self.ship.ammo -= 1;
        self.ship.tools[i].cooldown = 0.22;
        self.events.push(SimEvent::Shot { pos: mount, dir });
        if self.ship.ammo == 0 {
            self.toast("Munition leer – an einer Station nachkaufen", ToastKind::Warn);
        }
    }

    fn update_crane(&mut self, i: usize) {
        let mount = self.ship.tool_world_pos(i);
        let just = self.ship.tools[i].just_pressed();
        let aim = self.ship.tools[i].aim_dir();
        let range = self.ship.crane_range;
        let state = self.ship.tools[i].crane.clone();
        let next = match state {
            CraneState::Idle => {
                if just {
                    self.events.push(SimEvent::CraneFire);
                    CraneState::Extending { len: 0.5, dir: aim }
                } else {
                    CraneState::Idle
                }
            }
            CraneState::Extending { len, dir } => {
                if just {
                    CraneState::Retracting { len, dir }
                } else {
                    let len = len + CRANE_SPEED * DT;
                    let mut best: Option<(f32, usize)> = None;
                    for (bi, b) in self.bodies.iter().enumerate() {
                        if !b.alive || !b.grabbable() {
                            continue;
                        }
                        if let Some(t) = ray_circle(mount, dir, len, b.pos, b.radius + 0.3) {
                            if best.is_none_or(|(bt, _)| t < bt) {
                                best = Some((t, bi));
                            }
                        }
                    }
                    let tip = mount + dir * len;
                    if let Some((_, bi)) = best {
                        let b = &self.bodies[bi];
                        let rope = (b.pos - mount).length().max(b.radius + 1.5);
                        self.events.push(SimEvent::CraneAttach { pos: b.pos });
                        if let BodyKind::Derelict { name, .. } = &b.kind {
                            let msg = format!("{name} am Haken – zur Zielstation schleppen");
                            self.toast(msg, ToastKind::Info);
                        }
                        CraneState::Attached { body: self.bodies[bi].id, rope }
                    } else if len >= range || self.point_in_static(tip) {
                        CraneState::Retracting { len, dir }
                    } else {
                        CraneState::Extending { len, dir }
                    }
                }
            }
            CraneState::Retracting { len, dir } => {
                let len = len - CRANE_SPEED * 1.4 * DT;
                if len <= 0.0 {
                    CraneState::Idle
                } else {
                    CraneState::Retracting { len, dir }
                }
            }
            CraneState::Attached { body, rope } => {
                let Some(bi) = self.bodies.iter().position(|b| b.id == body && b.alive) else {
                    self.ship.tools[i].crane = CraneState::Idle;
                    return;
                };
                if just {
                    self.events.push(SimEvent::CraneRelease);
                    let d = (self.bodies[bi].pos - mount).length();
                    CraneState::Retracting { len: d, dir: (self.bodies[bi].pos - mount).normalize_or_zero() }
                } else {
                    let stowable = self.bodies[bi].stowable();
                    let mut rope = rope;
                    if stowable {
                        rope = (rope - 6.0 * DT).max(0.0);
                    }
                    let d = self.bodies[bi].pos - mount;
                    let dist = d.length();
                    if stowable && dist < self.bodies[bi].radius + 1.6 && self.try_stow(bi) {
                        self.ship.tools[i].crane = CraneState::Idle;
                        return;
                    }
                    if dist > rope * 1.8 + 6.0 {
                        self.toast("Kranseil gerissen!", ToastKind::Bad);
                        self.events.push(SimEvent::CraneRelease);
                        self.ship.tools[i].crane = CraneState::Idle;
                        return;
                    }
                    if dist > rope && dist > 1e-4 {
                        let n = d / dist;
                        let bm = self.bodies[bi].mass;
                        let mu = bm * self.ship.mass / (bm + self.ship.mass);
                        let k = 18.0 * mu;
                        let c = 5.0 * mu;
                        let rel_v = (self.bodies[bi].vel - self.ship.point_velocity(mount)).dot(n);
                        let f = (k * (dist - rope) + c * rel_v).clamp(0.0, 900.0);
                        let imp = n * f * DT;
                        self.bodies[bi].vel -= imp / bm;
                        self.ship.apply_impulse(imp, mount);
                    }
                    CraneState::Attached { body, rope }
                }
            }
        };
        self.ship.tools[i].crane = next;
    }

    fn update_drill(&mut self, i: usize) {
        self.ship.tools[i].drill = None;
        if !self.ship.tools[i].pressed {
            return;
        }
        let mount = self.ship.tool_world_pos(i);
        let dir = self.ship.tools[i].aim_dir();
        let mut best: Option<(f32, Target)> = None;
        let consider = |t: f32, tg: Target, best: &mut Option<(f32, Target)>| {
            if best.as_ref().is_none_or(|(bt, _)| t < *bt) {
                *best = Some((t, tg));
            }
        };
        for (pi, p) in self.world.planets.iter().enumerate() {
            if let Some(t) = ray_circle(mount, dir, DRILL_RANGE, p.pos, p.radius) {
                consider(t, Target::Planet(pi), &mut best);
            }
        }
        for (bi, b) in self.bodies.iter().enumerate() {
            if !b.alive {
                continue;
            }
            if let Some(t) = ray_circle(mount, dir, DRILL_RANGE, b.pos, b.radius) {
                consider(t, Target::Body(bi), &mut best);
            }
        }
        for col in &self.world.colliders {
            if let Shape::Quad(q) = col.shape {
                if !col.aabb.expand(DRILL_RANGE).contains(mount) {
                    continue;
                }
                if let Some(t) = ray_quad(mount, dir, DRILL_RANGE, &q) {
                    consider(t, Target::Rock, &mut best);
                }
            }
        }
        let Some((t, target)) = best else {
            return;
        };
        let point = mount + dir * t;
        // Rückstoß des Bohrers.
        self.ship.apply_impulse(-dir * 3.0 * DT, mount);
        let rate = self.ship.drill_rate * DT;
        let free = (self.ship.cargo_capacity() - self.ship.cargo_mass()).max(0.0);
        let mut mined_ore = None;
        let mut full = false;
        match target {
            Target::Planet(pi) => {
                let p = &self.world.planets[pi];
                let ang = f32::atan2(point.y - p.pos.y, point.x - p.pos.x);
                let half = p.deposit_half_angle();
                let ore = p.ore;
                if let Some(di) = p
                    .deposits
                    .iter()
                    .position(|d| d.amount > 0.0 && angle_diff(ang, d.angle).abs() < half)
                {
                    let amount = rate.min(self.world.planets[pi].deposits[di].amount).min(free);
                    if amount > 0.0 {
                        self.world.planets[pi].deposits[di].amount -= amount;
                        self.ship.store(CargoKind::Ore(ore), amount);
                        mined_ore = Some(ore);
                    } else if free <= 0.0 {
                        full = true;
                    }
                }
            }
            Target::Body(bi) => {
                if let BodyKind::Asteroid { ore, ore_left, hp, .. } = &mut self.bodies[bi].kind {
                    *hp -= 5.0 * DT;
                    if let Some(o) = *ore {
                        let amount = rate.min(*ore_left).min(free);
                        if amount > 0.0 {
                            *ore_left -= amount;
                            self.ship.store(CargoKind::Ore(o), amount);
                            mined_ore = Some(o);
                        } else if free <= 0.0 {
                            full = true;
                        }
                    }
                }
            }
            Target::Rock => {}
        }
        if full {
            self.cargo_full_warning();
        }
        self.ship.tools[i].drill = Some(DrillHit { point, ore: mined_ore });
    }

    fn cargo_full_warning(&mut self) {
        if self.border_warn <= 0.0 {
            self.border_warn = 3.0;
            self.toast("Frachtraum voll", ToastKind::Warn);
        }
    }

    pub(crate) fn point_in_static(&self, p: Vec2) -> bool {
        self.world.colliders.iter().any(|c| {
            c.aabb.contains(p)
                && match c.shape {
                    Shape::Quad(q) => q.contains(p),
                    Shape::Circle { c, r } => (p - c).length_squared() < r * r,
                }
        })
    }

    pub(crate) fn undock(&mut self) {
        if let Some(pad) = self.ship.docked.take() {
            let n = self.world.pads[pad].normal;
            self.ship.vel = n * 1.8;
            self.ship.ang_vel = 0.0;
            self.ship.dock_cooldown = 1.2;
            self.ship.dock_timer = 0.0;
            self.events.push(SimEvent::Undocked);
        }
    }
}

enum Target {
    Planet(usize),
    Body(usize),
    Rock,
}
