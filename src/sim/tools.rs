//! Slots in Aktion: Triebwerke, Kanone, Kran und Bohrer.

use bevy::math::Vec2;

use super::data::ToolKind;
use super::geom::{cross, ray_circle, ray_poly, rot};
use super::ship::{CargoKind, CraneState, DrillHit};
use super::world::{Shape, angle_diff};
use super::{Body, BodyKind, DT, Projectile, SimEvent, SimState, TickInput, ToastKind};

const SHOT_SPEED: f32 = 75.0;
const SHOT_RECOIL: f32 = 2.2;
const CRANE_SPEED: f32 = 55.0;
/// Seillänge, auf die schwere Lasten eingeholt werden.
pub const TOW_ROPE: f32 = 8.0;
/// Größter Seilimpuls pro Tick, bevor das Seil reißt.
const ROPE_BREAK_IMPULSE: f32 = 70.0;
/// Ab diesem Seilimpuls reißt ein Bauteil aus einem Wrack (ein Ruck, kein ruhiges Ziehen).
pub const TEAR_IMPULSE: f32 = 12.0;
const DRILL_RANGE: f32 = 5.5;
/// Schubanteil, wenn der Tank leer ist – damit niemand im reibungsfreien All festsitzt.
pub const EMERGENCY_THRUST: f32 = 0.25;

/// Stottert ein beschädigtes Triebwerk in diesem Tick? Deterministisch aus Tick und Slot,
/// in Fenstern von 5 Ticks, damit es hörbar „hustet“ statt zu flimmern.
pub fn thruster_works(t: &super::ship::Thruster, tick: u64) -> bool {
    if t.failed() {
        return false;
    }
    if !t.stuttering() {
        return true;
    }
    let h = super::rng::hash32((tick / 5) as u32 ^ (t.part as u32).wrapping_mul(0x9e37_79b9));
    let p = 0.15 + 0.6 * (super::ship::STUTTER_BELOW - t.health) / super::ship::STUTTER_BELOW;
    (h % 1000) as f32 / 1000.0 >= p
}

impl SimState {
    pub(crate) fn apply_input(&mut self, input: &TickInput) {
        // Während einer Abstimmung sind die Slot-Tasten Stimmen, keine Steuerung.
        let voting = self.vote.is_some();
        let mut any_pressed = false;
        let tick = self.tick;
        for t in &mut self.ship.thrusters {
            let pressed = !voting && input.pressed(t.slot);
            any_pressed |= pressed;
            t.firing = pressed && thruster_works(t, tick);
            let target = if t.firing { 1.0 } else { 0.0 };
            t.level += (target - t.level) * (DT * 16.0).min(1.0);
        }

        let in_zone = self.landed_in_zone();
        if self.ship.docked.is_some() {
            if any_pressed {
                self.undock();
            } else if !in_zone {
                for t in &mut self.ship.tools {
                    t.prev_pressed = t.pressed;
                    t.pressed = false;
                    if let Some(a) = input.aims.get(t.slot as usize) {
                        t.aim = *a;
                    }
                }
                // Was am Haken hängt, bleibt am angedockten Schiff (fester Anker).
                for i in 0..self.ship.tools.len() {
                    self.hold_while_docked(i);
                }
                return;
            }
        }

        // Schub an versetzten Punkten → Kraft + Drehmoment. Ohne Treibstoff bleibt die Notreserve.
        let reserve = if self.ship.fuel_empty() {
            EMERGENCY_THRUST
        } else {
            1.0
        };
        let mut force = Vec2::ZERO;
        let mut torque = 0.0;
        let mut burned = 0.0;
        for t in &self.ship.thrusters {
            if !t.firing {
                continue;
            }
            let f = rot(t.dir, self.ship.angle) * t.effective_thrust() * reserve;
            let at = self.ship.to_world(t.pos);
            force += f;
            torque += cross(at - self.ship.pos, f);
            burned += t.thrust * self.ship.fuel_burn * DT;
        }
        if self.ship.docked.is_none() {
            self.ship.vel += force / self.ship.mass * DT;
            self.ship.ang_vel += torque / self.ship.inertia * DT;
            self.burn_fuel(burned);
        }

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

    fn burn_fuel(&mut self, amount: f32) {
        if amount <= 0.0 || self.ship.fuel_empty() {
            return;
        }
        self.ship.fuel = (self.ship.fuel - amount).max(0.0);
        let frac = self.ship.fuel / self.ship.max_fuel.max(1.0);
        if self.ship.fuel_empty() && self.fuel_warned < 2 {
            self.fuel_warned = 2;
            self.toast(
                "Tank leer – Notreserve: nur noch 25 % Schub. An einer Station tanken.",
                ToastKind::Bad,
            );
        } else if frac < 0.2 && self.fuel_warned < 1 {
            self.fuel_warned = 1;
            self.toast("Treibstoff knapp (unter 20 %)", ToastKind::Warn);
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
        let slot = self.ship.tools[i].slot;
        self.stats.slot(slot).shots += 1;
        self.ship.tools[i].cooldown = 0.22;
        self.events.push(SimEvent::Shot { pos: mount, dir });
        if self.ship.ammo == 0 {
            self.toast(
                "Munition leer – an einer Station nachkaufen",
                ToastKind::Warn,
            );
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
                        if let Some(t) = ray_circle(mount, dir, len, b.pos, b.radius + 0.3)
                            && best.is_none_or(|(bt, _)| t < bt)
                        {
                            best = Some((t, bi));
                        }
                    }
                    let tip = mount + dir * len;
                    if let Some((_, bi)) = best {
                        let slot = self.ship.tools[i].slot;
                        self.stats.slot(slot).grabs += 1;
                        let b = &self.bodies[bi];
                        let rope = (b.pos - mount).length().max(b.radius + 1.5);
                        self.events.push(SimEvent::CraneAttach { pos: b.pos });
                        let msg = match &b.kind {
                            BodyKind::Wreck { parts, .. } if *parts > 0 => Some(
                                "Wrack am Haken – mit einem Ruck reißt der Kran ein Bauteil ab"
                                    .to_string(),
                            ),
                            BodyKind::Wreck { .. } => {
                                Some("Wrack am Haken – hier ist nichts mehr zu holen".to_string())
                            }
                            BodyKind::Derelict { name, .. } => {
                                Some(format!("{name} am Haken – zur Zielstation schleppen"))
                            }
                            _ => None,
                        };
                        if let Some(msg) = msg {
                            self.toast(msg, ToastKind::Info);
                        }
                        CraneState::Attached {
                            body: self.bodies[bi].id,
                            rope,
                        }
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
                    CraneState::Retracting {
                        len: d,
                        dir: (self.bodies[bi].pos - mount).normalize_or_zero(),
                    }
                } else {
                    let stowable = self.bodies[bi].stowable();
                    let mut rope = rope;
                    if stowable {
                        rope = (rope - 6.0 * DT).max(0.0);
                    } else if rope > TOW_ROPE {
                        // Schwere Last: Seil langsam auf Schlepplänge einholen.
                        rope = (rope - 2.5 * DT).max(TOW_ROPE);
                    }
                    let d = self.bodies[bi].pos - mount;
                    let dist = d.length();
                    if stowable && dist < self.bodies[bi].radius + 1.6 && self.try_stow(bi) {
                        self.ship.tools[i].crane = CraneState::Idle;
                        return;
                    }
                    let pull = if dist > rope * 1.8 + 6.0 {
                        None
                    } else {
                        self.rope_constraint(bi, mount, rope)
                    };
                    let Some(pull) = pull else {
                        self.toast("Kranseil gerissen – zu harter Ruck!", ToastKind::Bad);
                        self.events.push(SimEvent::CraneRelease);
                        self.ship.tools[i].crane = CraneState::Idle;
                        return;
                    };
                    // Am Wrack: ein kräftiger Ruck reißt ein Bauteil ab, das dann am Haken hängt.
                    if pull > TEAR_IMPULSE
                        && let Some(piece) = self.tear_part(bi, mount)
                    {
                        let d = (self.bodies[piece].pos - mount).length();
                        self.ship.tools[i].crane = CraneState::Attached {
                            body: self.bodies[piece].id,
                            rope: d,
                        };
                        return;
                    }
                    CraneState::Attached { body, rope }
                }
            }
        };
        self.ship.tools[i].crane = next;
    }

    /// Seil als harte Längenbeschränkung: schlaff, solange die Last näher ist als die
    /// Seillänge; straff zieht es nur (nie drücken). Ein zu harter Ruck reißt das Seil
    /// (Rückgabe `false`). Aus dem Wechsel von straff und schlaff entsteht das Pendeln.
    fn rope_constraint(&mut self, bi: usize, mount: Vec2, rope: f32) -> Option<f32> {
        let d = self.bodies[bi].pos - mount;
        let dist = d.length();
        if dist <= rope || dist < 1e-4 {
            return Some(0.0);
        }
        let n = d / dist;
        let bm = self.bodies[bi].mass;
        let anchored = self.ship.docked.is_some();
        let r = mount - self.ship.pos;
        let rn = cross(r, n);
        let k = if anchored {
            1.0 / bm
        } else {
            1.0 / bm + 1.0 / self.ship.mass + rn * rn / self.ship.inertia
        };
        // Trennungsgeschwindigkeit entlang des Seils (+ etwas Lagekorrektur).
        let vrel = (self.bodies[bi].vel - self.ship.point_velocity(mount)).dot(n);
        let bias = ((dist - rope) * 0.2 / DT).min(8.0);
        let lambda = (vrel + bias) / k;
        if lambda <= 0.0 {
            return Some(0.0);
        }
        // Wracks sind verankert genug, dass vorher Bauteile nachgeben (siehe `tear_part`).
        let tearable = matches!(self.bodies[bi].kind, BodyKind::Wreck { parts, .. } if parts > 0);
        if lambda > ROPE_BREAK_IMPULSE && !tearable {
            return None;
        }
        let lambda = lambda.min(ROPE_BREAK_IMPULSE);
        self.bodies[bi].vel -= n * (lambda / bm);
        if !anchored {
            self.ship.apply_impulse(n * lambda, mount);
        }
        Some(lambda)
    }

    /// Reißt ein Bauteil aus einem Wrack. Gibt den Index des neuen Körpers zurück.
    fn tear_part(&mut self, bi: usize, mount: Vec2) -> Option<usize> {
        let BodyKind::Wreck { parts, .. } = &mut self.bodies[bi].kind else {
            return None;
        };
        if *parts == 0 {
            return None;
        }
        *parts -= 1;
        let (wpos, wr, wvel) = (
            self.bodies[bi].pos,
            self.bodies[bi].radius,
            self.bodies[bi].vel,
        );
        let toward = (mount - wpos).normalize_or_zero();
        let shop = self.data.shop.clone();
        let name = if shop.salvage_names.is_empty() {
            "Bauteil".to_string()
        } else {
            shop.salvage_names[self.rng.index(shop.salvage_names.len())].clone()
        };
        let (lo, hi) = shop.salvage_value;
        let value = self.rng.range_u32(lo.min(hi), hi.max(lo).max(1));
        let id = self.next_id();
        // Das Teil löst sich an der Oberfläche des Wracks.
        let pos = wpos + toward * (wr * 0.9 + 0.8);
        self.bodies.push(Body {
            id,
            kind: BodyKind::Salvage {
                name: name.clone(),
                value,
            },
            pos,
            vel: wvel + toward * 3.0,
            angle: 0.0,
            ang_vel: self.rng.range(-2.0, 2.0),
            radius: 0.8,
            mass: 1.5,
            prev_pos: pos,
            prev_angle: 0.0,
            alive: true,
            seed: super::rng::hash32(id),
            age: 0.0,
        });
        self.events.push(SimEvent::Explosion {
            pos,
            size: 1.6,
            color: [1.0, 0.7, 0.35],
        });
        self.toast(
            format!("Bauteil abgerissen: {name} (~{value} Cr)"),
            ToastKind::Good,
        );
        Some(self.bodies.len() - 1)
    }

    /// Angedockt: Seil hält die Last, ohne das Schiff zu bewegen; reißt nur bei Überdehnung.
    fn hold_while_docked(&mut self, i: usize) {
        let CraneState::Attached { body, rope } = self.ship.tools[i].crane else {
            return;
        };
        let Some(bi) = self.bodies.iter().position(|b| b.id == body && b.alive) else {
            self.ship.tools[i].crane = CraneState::Idle;
            return;
        };
        let mount = self.ship.tool_world_pos(i);
        let dist = (self.bodies[bi].pos - mount).length();
        if dist > rope * 1.8 + 6.0 || self.rope_constraint(bi, mount, rope).is_none() {
            self.toast("Kranseil gerissen!", ToastKind::Bad);
            self.events.push(SimEvent::CraneRelease);
            self.ship.tools[i].crane = CraneState::Idle;
        }
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
            if let Shape::Poly(q) = col.shape {
                if !col.aabb.expand(DRILL_RANGE).contains(mount) {
                    continue;
                }
                if let Some(t) = ray_poly(mount, dir, DRILL_RANGE, &q) {
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
        let mut mined_amt = 0.0;
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
                    let amount = rate
                        .min(self.world.planets[pi].deposits[di].amount)
                        .min(free);
                    if amount > 0.0 {
                        self.world.planets[pi].deposits[di].amount -= amount;
                        self.ship.store(CargoKind::Ore(ore), amount);
                        mined_ore = Some(ore);
                        mined_amt += amount;
                    } else if free <= 0.0 {
                        full = true;
                    }
                }
            }
            Target::Body(bi) => {
                if let BodyKind::Wreck { scrap, .. } = &mut self.bodies[bi].kind {
                    let amount = (rate * 0.8).min(*scrap).min(free);
                    if amount > 0.0 {
                        *scrap -= amount;
                        self.ship
                            .store(CargoKind::Ore(super::data::Ore::Schrott), amount);
                        mined_ore = Some(super::data::Ore::Schrott);
                        mined_amt += amount;
                    } else if free <= 0.0 {
                        full = true;
                    }
                } else if let BodyKind::Asteroid {
                    ore, ore_left, hp, ..
                } = &mut self.bodies[bi].kind
                {
                    *hp -= 5.0 * DT;
                    if let Some(o) = *ore {
                        let amount = rate.min(*ore_left).min(free);
                        if amount > 0.0 {
                            *ore_left -= amount;
                            self.ship.store(CargoKind::Ore(o), amount);
                            mined_ore = Some(o);
                            mined_amt += amount;
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
        if mined_amt > 0.0 {
            let slot = self.ship.tools[i].slot;
            self.stats.slot(slot).drilled += mined_amt;
        }
        self.ship.tools[i].drill = Some(DrillHit {
            point,
            ore: mined_ore,
        });
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
                    Shape::Poly(q) => q.contains(p),
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
