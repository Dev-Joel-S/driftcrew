//! Fracht und Bergung (Punkte 64, 66–68, 77, 78): Ladeplan (umladen, abwerfen), Fracht mit
//! Flugeigenschaften (Tank, empfindlich, instabil), freiwillige Zusatzfunde und Rettung
//! havarierter Schiffe mit Platzentscheidung.
//!
//! Umladen ändert nur, wo die Masse sitzt – Schwerpunkt und Trägheit rechnen sich wie immer
//! aus den Positionen, es gibt keine künstlichen Zugkräfte. Nur Flüssigkeit schwappt: sie wird
//! als gedämpfte Feder im Modul gerechnet, und nur der Unterschied zur starren Last wirkt aufs
//! Schiff (in gleichmäßiger Beschleunigung also nichts).

use bevy::math::Vec2;

use super::data::{CargoTrait, CrewSave, DroppedSave, RescuedSave, v};
use super::geom::rot;
use super::missions::MissionKind;
use super::rng::hash32;
use super::ship::{CargoItem, CargoKind};
use super::{Body, BodyKind, DT, SimEvent, SimState, ToastKind};

/// Platz, den eine gerettete Person im Frachtraum braucht (mit Notversorgung), t.
pub const PERSON_MASS: f32 = 0.4;
/// Flüssigkeit: Federkonstante, Dämpfung, größte Auslenkung im Modul.
const SLOSH_K: f32 = 4.0;
const SLOSH_C: f32 = 0.8;
const SLOSH_MAX: f32 = 0.7;
/// Ab dieser Beschleunigung (m/s²) leidet empfindliche Fracht – das ist ein Stoß, kein Schub.
pub const FRAGILE_ACC: f32 = 40.0;
/// Ab dieser Beschleunigung baut instabile Fracht Belastung auf (volle Schubkraft liegt knapp
/// darüber, also vorsichtig Gas geben).
pub const UNSTABLE_ACC: f32 = 13.0;
/// Längsseits: höchstens so viel Lücke zwischen den Rümpfen und so schnell relativ zueinander.
pub const RESCUE_GAP: f32 = 5.0;
const RESCUE_SPEED: f32 = 1.5;
/// So lange dauert es, eine Person hinüberzuholen.
pub const RESCUE_STEP: f32 = 1.2;

impl SimState {
    /// Ladeplan: ein Stück umladen (angedockt sofort, im Flug mit Laufzeit).
    pub(crate) fn move_cargo_cmd(&mut self, id: u32, to: usize) {
        let instant = self.ship.docked.is_some();
        match self.ship.move_cargo(id, to, instant) {
            Ok(()) => {
                let where_ = self.ship.pod_label(to);
                let msg = if instant {
                    format!("Umgeladen nach {where_}")
                } else {
                    format!("Wird nach {where_} umgeladen …")
                };
                self.toast(msg, ToastKind::Info);
            }
            Err(e) => self.toast(e, ToastKind::Warn),
        }
    }

    /// Notabwurf: das Stück treibt als Körper in der Welt und lässt sich wieder einsammeln.
    pub(crate) fn jettison(&mut self, id: u32) {
        let Some(i) = self.ship.cargo.iter().position(|c| c.id == id) else {
            return;
        };
        if matches!(self.ship.cargo[i].kind, CargoKind::Survivor { .. }) {
            self.toast("Gerettete wirft man nicht ab", ToastKind::Warn);
            return;
        }
        let item = self.ship.cargo.remove(i);
        let at = self.ship.to_world(self.ship.cargo_pos(&item));
        self.ship.recompute_mass();
        // Seitlich weg vom Schiff, damit es nicht gleich wieder anstößt.
        let out = (at - self.ship.pos).normalize_or(rot(-glam_y(), self.ship.angle));
        let pos = at + out * 2.0;
        let vel = self.ship.vel + out * 1.5;
        let label = self.cargo_label(&item.kind);
        match &item.kind {
            CargoKind::Artifact { id } => {
                let id = id.clone();
                self.spawn_artifact(&id, pos, vel);
            }
            CargoKind::Capsule { mission } => {
                let mission = *mission;
                self.spawn_body(BodyKind::Capsule { mission }, pos, vel, 0.7, 0.8);
            }
            _ => {
                let r = dropped_radius(item.mass);
                let m = item.mass.max(0.3);
                self.spawn_body(BodyKind::Dropped { item }, pos, vel, r, m);
            }
        }
        self.events.push(SimEvent::Jettisoned { pos });
        self.toast(
            format!("Abgeworfen: {label} – treibt hier und lässt sich wieder einsammeln"),
            ToastKind::Warn,
        );
    }

    pub fn cargo_label(&self, k: &CargoKind) -> String {
        match k {
            CargoKind::Ore(o) => o.label().to_string(),
            CargoKind::Container { name, .. } => format!("Container: {name}"),
            CargoKind::Capsule { .. } => "Rettungskapsel".into(),
            CargoKind::Salvage { name, value } => format!("Bauteil: {name} ({value} Cr)"),
            CargoKind::Artifact { id } => format!("Artefakt: {}", self.artifact_name(id)),
            CargoKind::Survivor { .. } => "Gerettete Person".into(),
            CargoKind::Gear(g) => format!("Ausrüstung: {}", g.label()),
        }
    }

    fn spawn_body(&mut self, kind: BodyKind, pos: Vec2, vel: Vec2, radius: f32, mass: f32) {
        let id = self.next_id();
        self.bodies.push(Body {
            id,
            kind,
            pos,
            vel,
            angle: 0.0,
            ang_vel: 0.6,
            radius,
            mass,
            prev_pos: pos,
            prev_angle: 0.0,
            alive: true,
            seed: hash32(id),
            age: 0.0,
        });
    }

    /// Jeder Tick (nach Bewegung und Kollisionen): Umladen, Flugeigenschaften, Rettung,
    /// Meldungen Geretteter.
    pub(crate) fn update_cargo(&mut self) {
        if self.ship.update_cargo_moves(DT) {
            self.toast("Fracht umgeladen und arretiert", ToastKind::Info);
        }
        self.cargo_dynamics();
        self.update_rescue();
        self.update_rescued();
        self.cargo_warn = (self.cargo_warn - DT).max(0.0);
    }

    /// Schwappen, Stöße und Überlastung. Gemessen wird die Beschleunigung des Schiffs
    /// (nur im freien Flug – Andocken setzt die Geschwindigkeit hart auf null).
    fn cargo_dynamics(&mut self) {
        let flying = self.ship.docked.is_none() && !self.ship.destroyed;
        let prev = if flying { self.cargo_vel } else { None };
        self.cargo_vel = flying.then_some(self.ship.vel);
        let Some(prev) = prev else {
            return;
        };
        let acc = (self.ship.vel - prev) / DT;
        let a = acc.length();
        let local = rot(acc, -self.ship.angle);
        let mut lost: Vec<(u32, Vec2, CargoTrait)> = Vec::new();
        let mut damaged = false;
        let mut stressed = false;
        let mut impulses: Vec<(Vec2, Vec2)> = Vec::new();
        for i in 0..self.ship.cargo.len() {
            let pos_local = self.ship.cargo_pos(&self.ship.cargo[i]);
            let c = &mut self.ship.cargo[i];
            match c.traits {
                CargoTrait::Tank => {
                    // x'' = -k x - c x' - a (Trägheit im Schiffssystem).
                    let v0 = c.slosh_v;
                    let xa = -c.slosh * SLOSH_K - c.slosh_v * SLOSH_C - local;
                    c.slosh_v += xa * DT;
                    c.slosh += c.slosh_v * DT;
                    // An der Tankwand: die Flüssigkeit liegt an und fährt starr mit.
                    if c.slosh.length() > SLOSH_MAX {
                        let n = c.slosh.normalize();
                        c.slosh = n * SLOSH_MAX;
                        let vn = c.slosh_v.dot(n);
                        if vn > 0.0 {
                            c.slosh_v -= n * vn;
                        }
                    }
                    // Rückwirkung aufs Schiff: nur der Unterschied zur starren Last (−m·x''),
                    // bei gleichmäßigem Schub also nichts.
                    let f = (-(c.slosh_v - v0) / DT * c.mass).clamp_length_max(c.mass * 20.0);
                    impulses.push((rot(f, self.ship.angle) * DT, pos_local + c.slosh));
                }
                CargoTrait::Fragile if a > FRAGILE_ACC => {
                    c.cond = (c.cond - ((a - FRAGILE_ACC) / 250.0).min(0.5)).max(0.0);
                    damaged = true;
                    if c.cond <= 0.0 {
                        lost.push((c.id, pos_local, c.traits));
                    }
                }
                CargoTrait::Unstable => {
                    if a > UNSTABLE_ACC {
                        c.stress += (a - UNSTABLE_ACC) / UNSTABLE_ACC * 0.6 * DT;
                        stressed = c.stress > 0.6;
                    } else {
                        c.stress = (c.stress - 0.08 * DT).max(0.0);
                    }
                    if c.stress >= 1.0 {
                        lost.push((c.id, pos_local, c.traits));
                    }
                }
                _ => {}
            }
        }
        for (j, at) in impulses {
            let w = self.ship.to_world(at);
            self.ship.apply_impulse(j, w);
        }
        if damaged && self.cargo_warn <= 0.0 {
            self.cargo_warn = 3.0;
            let worst = self
                .ship
                .cargo
                .iter()
                .filter(|c| c.traits == CargoTrait::Fragile)
                .map(|c| c.cond)
                .fold(1.0, f32::min);
            self.toast(
                format!(
                    "Empfindliche Fracht beschädigt – Zustand {:.0} %",
                    worst * 100.0
                ),
                ToastKind::Warn,
            );
        }
        if stressed && self.cargo_warn <= 0.0 {
            self.cargo_warn = 3.0;
            self.toast(
                "Instabile Fracht überlastet – Schub zurücknehmen!",
                ToastKind::Bad,
            );
        }
        for (id, at, t) in lost {
            self.lose_cargo(id, at, t);
        }
    }

    /// Ein Stück ist hinüber (zerbrochen oder verpufft).
    fn lose_cargo(&mut self, id: u32, local: Vec2, t: CargoTrait) {
        let Some(i) = self.ship.cargo.iter().position(|c| c.id == id) else {
            return;
        };
        let item = self.ship.cargo.remove(i);
        self.ship.recompute_mass();
        let at = self.ship.to_world(local);
        let label = self.cargo_label(&item.kind);
        if t == CargoTrait::Unstable {
            self.events.push(SimEvent::Explosion {
                pos: at,
                size: 4.0,
                color: [0.6, 0.9, 1.0],
            });
            self.ship_damage(18.0, at, true);
            self.toast(format!("{label} ist verpufft!"), ToastKind::Bad);
        } else {
            self.toast(format!("{label} ist zu Bruch gegangen"), ToastKind::Bad);
        }
        if let CargoKind::Container { mission, .. } = item.kind {
            self.fail_mission(mission, "Fracht zerstört");
        }
    }

    /// Zustand der Lieferfracht eines Auftrags (empfindlich: 0..1, sonst 1).
    pub fn delivery_condition(&self, mission: u32) -> f32 {
        self.ship
            .cargo
            .iter()
            .find(|c| matches!(c.kind, CargoKind::Container { mission: m, .. } if m == mission))
            .map_or(1.0, |c| c.cond)
    }

    /// Zusatzfund (77) in der Nähe eines Einsatzorts – vielleicht.
    pub(crate) fn maybe_spawn_bonus(&mut self, site: Vec2) {
        let bd = self.data.missions.bonus.clone();
        if bd.names.is_empty() || !self.rng.chance(bd.chance) {
            return;
        }
        let name = bd.names[self.rng.index(bd.names.len())].clone();
        let mass = (self.rng.range(bd.mass.0, bd.mass.1) * 2.0).round() / 2.0;
        let value = self.rng.range_u32(bd.value.0, bd.value.1);
        let traits = if self.rng.chance(bd.unstable_chance) {
            CargoTrait::Unstable
        } else {
            CargoTrait::None
        };
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        let pos = site + Vec2::new(a.cos(), a.sin()) * self.rng.range(35.0, 70.0);
        let item = CargoItem::new(
            CargoKind::Salvage {
                name: name.clone(),
                value,
            },
            mass,
            traits,
        );
        self.spawn_body(
            BodyKind::Dropped { item },
            pos,
            Vec2::ZERO,
            dropped_radius(mass),
            mass,
        );
        let extra = if traits == CargoTrait::Unstable {
            ", instabil"
        } else {
            ""
        };
        self.toast(
            format!(
                "Am Einsatzort treibt ein Zusatzfund: {name} ({mass:.1} t{extra}, {value} Cr) – mitnehmen ist freiwillig"
            ),
            ToastKind::Info,
        );
    }

    /// Rettung (78): längsseits gehen holt die Besatzung Person für Person an Bord – wenn
    /// Platz ist.
    fn update_rescue(&mut self) {
        if self.ship.destroyed || self.ship.docked.is_some() {
            self.rescue_timer = 0.0;
            return;
        }
        let mut target = None;
        for (mi, m) in self.active.iter().enumerate() {
            if let MissionKind::Rescue {
                npc: Some(id),
                crew,
                aboard,
                ..
            } = m.kind
                && aboard < crew
                && let Some(n) = self.npcs.iter().find(|n| n.id == id && n.alive)
            {
                let gap = (n.ship.pos - self.ship.pos).length()
                    - n.ship.bound_radius()
                    - self.ship.bound_radius();
                let rel = (n.ship.vel - self.ship.vel).length();
                if gap < RESCUE_GAP && rel < RESCUE_SPEED {
                    target = Some(mi);
                    break;
                }
            }
        }
        let Some(mi) = target else {
            self.rescue_timer = 0.0;
            return;
        };
        self.rescue_timer += DT;
        if self.rescue_timer < RESCUE_STEP {
            return;
        }
        self.rescue_timer = 0.0;
        let mid = self.active[mi].id;
        let stored = self
            .ship
            .store(CargoKind::Survivor { mission: mid }, PERSON_MASS);
        if stored <= 0.0 {
            if self.cargo_warn <= 0.0 {
                self.cargo_warn = 4.0;
                self.toast(
                    "Kein Platz für weitere Gerettete – Fracht umladen oder abwerfen (Pause → Ladeplan)",
                    ToastKind::Warn,
                );
            }
            return;
        }
        let MissionKind::Rescue {
            crew, aboard, to, ..
        } = &mut self.active[mi].kind
        else {
            return;
        };
        *aboard += 1;
        let (now, all, to) = (*aboard, *crew, *to);
        if now >= all {
            let st = self.world.stations[to].name.clone();
            self.toast(
                format!("Alle {all} an Bord – zur Station {st}"),
                ToastKind::Good,
            );
        } else {
            self.toast(format!("Gerettet: {now}/{all} an Bord"), ToastKind::Info);
        }
        self.events.push(SimEvent::Stowed {
            what: "Gerettete Person".into(),
        });
    }

    /// Gerettete melden sich später – an einer Station, mit Dank und einem Geschenk.
    fn update_rescued(&mut self) {
        if self.crew.rescued.is_empty() {
            return;
        }
        for r in &mut self.crew.rescued {
            r.wait -= DT;
        }
        if self.docked_station().is_none() {
            return;
        }
        let Some(i) = self.crew.rescued.iter().position(|r| r.wait <= 0.0) else {
            return;
        };
        let r = self.crew.rescued.remove(i);
        let rd = self.data.missions.rescue.clone();
        let line = rd
            .thanks
            .get(hash32(r.ship.len() as u32) as usize % rd.thanks.len().max(1))
            .cloned()
            .unwrap_or_default()
            .replace("{ship}", &r.ship)
            .replace("{captain}", &r.captain);
        self.events.push(SimEvent::Story {
            speaker: r.captain.clone(),
            text: line,
        });
        self.crew.credits += rd.gift_credits;
        self.crew.storage_parts += rd.gift_parts;
        self.toast(
            format!(
                "{} schickt {} Cr und {} Bauteile ins Crew-Lager – danke für die Rettung",
                r.captain, rd.gift_credits, rd.gift_parts
            ),
            ToastKind::Good,
        );
    }

    pub(crate) fn load_cargo_state(&mut self, save: &CrewSave) {
        for d in &save.dropped {
            if !matches!(d.kind, CargoKind::Ore(_) | CargoKind::Salvage { .. }) {
                continue;
            }
            let mut item = CargoItem::new(d.kind.clone(), d.mass, d.traits);
            item.cond = d.cond;
            let (r, m) = (dropped_radius(d.mass), d.mass.max(0.3));
            self.spawn_body(BodyKind::Dropped { item }, v(d.pos), Vec2::ZERO, r, m);
        }
        self.crew.rescued = save.rescued.clone();
    }

    pub(crate) fn save_cargo_state(&self, save: &mut CrewSave) {
        save.dropped = self
            .bodies
            .iter()
            .filter_map(|b| match &b.kind {
                BodyKind::Dropped { item }
                    if b.alive
                        && matches!(item.kind, CargoKind::Ore(_) | CargoKind::Salvage { .. }) =>
                {
                    Some(DroppedSave {
                        kind: item.kind.clone(),
                        mass: item.mass,
                        traits: item.traits,
                        cond: item.cond,
                        pos: (b.pos.x, b.pos.y),
                    })
                }
                _ => None,
            })
            .collect();
        save.rescued = self.crew.rescued.clone();
    }

    /// Rettung abgeschlossen: die Besatzung meldet sich später noch einmal.
    pub(crate) fn remember_rescue(&mut self, ship: &str, captain: &str) {
        self.crew.rescued.push(RescuedSave {
            ship: ship.to_string(),
            captain: captain.to_string(),
            wait: self.data.missions.rescue.thanks_after,
        });
    }
}

fn glam_y() -> Vec2 {
    Vec2::Y
}

/// Radius eines abgeworfenen Stücks nach Masse.
pub fn dropped_radius(mass: f32) -> f32 {
    (0.45 + mass.sqrt() * 0.3).min(1.6)
}
