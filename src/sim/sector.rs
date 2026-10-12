//! Sektoren mit Effekten (Trümmer, Nebel, Sonnenwind), Zufallsereignisse
//! (Meteoritenschauer, Notsignal, Sonneneruption), Scanner-Impulse und Kartendaten.

use crate::sim::fx::{Fx, Vec2};

use super::data::{Ore, SectorEffect, v};
use super::rng::hash32;
use super::{Body, BodyKind, DT, SimEvent, SimState, ToastKind};

/// Wie stark wirken die Sektoren an einem Punkt?
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SectorState {
    pub debris: Fx,
    pub nebula: Fx,
    pub wind: Vec2,
}

/// Ausbreitungsgeschwindigkeit des Scanner-Impulses (m/s).
pub const SCAN_SPEED: Fx = fx!(320.0);
/// So lange bleiben gefundene Dinge markiert (s).
pub const BLIP_SECONDS: Fx = fx!(30.0);

#[derive(Clone, Debug, PartialEq)]
pub enum BlipKind {
    Wreck,
    Ore(Ore),
    Deposit(Ore),
    Capsule,
    Cargo,
    /// Fremdes Signal (Artefakt der Vorgänger).
    Signal,
}

impl BlipKind {
    pub fn label(&self) -> String {
        match self {
            BlipKind::Wreck => "Wrack".into(),
            BlipKind::Ore(o) => format!("Erz ({})", o.label()),
            BlipKind::Deposit(o) => format!("Vorkommen ({})", o.label()),
            BlipKind::Capsule => "Kapsel".into(),
            BlipKind::Cargo => "Fracht".into(),
            BlipKind::Signal => "Fremdes Signal".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScanBlip {
    pub pos: Vec2,
    pub kind: BlipKind,
    pub life: Fx,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScanPulse {
    pub origin: Vec2,
    pub radius: Fx,
    pub max: Fx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    MeteorShower,
    SolarFlare,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldEvent {
    pub kind: EventKind,
    pub time_left: Fx,
    /// Herkunftsrichtung (Meteoritenschauer).
    pub dir: Vec2,
    pub spawn_timer: Fx,
}

impl WorldEvent {
    pub fn flare_active(&self) -> bool {
        self.kind == EventKind::SolarFlare && self.time_left > fx!(0.0)
    }
}

/// Himmelsrichtung für Meldungen.
pub fn compass(dir: Vec2) -> &'static str {
    let a = dir.y.atan2(dir.x).to_degrees().rem_euclid(fx!(360.0));
    const NAMES: [&str; 8] = [
        "Osten",
        "Nordosten",
        "Norden",
        "Nordwesten",
        "Westen",
        "Südwesten",
        "Süden",
        "Südosten",
    ];
    NAMES[((a + fx!(22.5)) / fx!(45.0)).to_usize() % 8]
}

impl SimState {
    /// Effekte aller Sektoren an einem Punkt, mit weichem Rand (voll bis 70 % des Radius).
    pub fn sector_at(&self, p: Vec2) -> SectorState {
        let mut s = SectorState::default();
        for r in &self.data.world.regions {
            let Some(e) = r.effect else { continue };
            let d = (p - v(r.center)).length();
            let k = ((r.radius - d) / (r.radius * fx!(0.3))).clamp(fx!(0.0), fx!(1.0));
            if k <= fx!(0.0) {
                continue;
            }
            match e {
                SectorEffect::Debris(x) => s.debris = s.debris.max(x * k),
                SectorEffect::Nebula(x) => s.nebula = s.nebula.max(x * k),
                SectorEffect::SolarWind(dir, a) => s.wind += v(dir).normalize_or_zero() * a * k,
            }
        }
        s
    }

    /// Sonneneruption gerade aktiv?
    pub fn flare(&self) -> bool {
        self.event.as_ref().is_some_and(|e| e.flare_active())
    }

    pub(crate) fn update_sectors(&mut self) {
        // Sonnenwind schiebt alles Leichte, was nicht festgemacht ist.
        if self.ship.docked.is_none() && !self.ship.destroyed {
            let w = self.sector_at(self.ship.pos).wind;
            self.ship.vel += w * DT;
        }
        for i in 0..self.bodies.len() {
            if matches!(self.bodies[i].kind, BodyKind::Wreck { .. }) {
                continue;
            }
            let w = self.sector_at(self.bodies[i].pos).wind;
            if w != Vec2::ZERO {
                self.bodies[i].vel += w * DT;
            }
        }
        if self.tick.is_multiple_of(30) {
            self.maintain_debris();
        }
        self.update_events();
        self.update_scan();
    }

    /// Trümmer um das Schiff herum auffüllen und entfernte wieder wegnehmen.
    fn maintain_debris(&mut self) {
        let me = self.ship.pos;
        for b in &mut self.bodies {
            if matches!(b.kind, BodyKind::Debris) && (b.pos - me).longer_than(fx!(240.0)) {
                b.alive = false;
            }
        }
        if self.ship.destroyed {
            return;
        }
        let mut density = self.sector_at(me).debris;
        // Geräumte Umgebung (76): kaum noch Schrott.
        if self.effect_near(super::data::EffectKind::Cleared, me) {
            density *= fx!(0.15);
        }
        // Etwa ein Stück auf 4000 m² bei voller Dichte – genug Gefahr, aber zum Ausweichen.
        let want = (density * fx!(22.0)).to_usize();
        let have = self
            .bodies
            .iter()
            .filter(|b| b.alive && matches!(b.kind, BodyKind::Debris))
            .count();
        // Erst füllen (auch nah am Schiff), danach nur noch außerhalb der Sicht nachlegen.
        let near: Fx = if have < want / 2 {
            fx!(25.0)
        } else {
            fx!(90.0)
        };
        for _ in have..want.min(have + 10) {
            let a = self.rng.range(fx!(0.0), Fx::TAU);
            // Flächengleich verteilt zwischen `near` und 170 m.
            let u = self.rng.unit();
            let d = (near * near + (fx!(170.0) * fx!(170.0) - near * near) * u).sqrt();
            let pos = me + Vec2::new(a.cos(), a.sin()) * d;
            if self.point_in_static(pos) || self.sector_at(pos).debris <= fx!(0.05) {
                continue;
            }
            let r = self.rng.range(fx!(0.5), fx!(1.6));
            let id = self.next_id();
            let vel = Vec2::new(
                self.rng.range(-fx!(1.0), fx!(1.0)),
                self.rng.range(-fx!(1.0), fx!(1.0)),
            ) * fx!(2.5);
            self.bodies.push(Body {
                id,
                kind: BodyKind::Debris,
                pos,
                vel,
                angle: a,
                ang_vel: self.rng.range(-fx!(2.0), fx!(2.0)),
                radius: r,
                mass: fx!(0.8) + r * r * fx!(2.0),
                prev_pos: pos,
                prev_angle: a,
                alive: true,
                seed: hash32(id),
                age: fx!(0.0),
            });
        }
    }

    pub(crate) fn update_events(&mut self) {
        let flying = self.ship.docked.is_none() && !self.ship.destroyed;
        if let Some(mut e) = self.event.take() {
            e.time_left -= DT;
            if e.kind == EventKind::MeteorShower && e.time_left < fx!(8.0) && flying {
                e.spawn_timer -= DT;
                if e.spawn_timer <= fx!(0.0) {
                    let n = Fx::from(self.data.world.events.shower_meteors.max(1));
                    e.spawn_timer = fx!(8.0) / n;
                    self.spawn_shower_meteor(e.dir);
                }
            }
            if e.time_left > fx!(0.0) {
                self.event = Some(e);
            } else if e.kind == EventKind::SolarFlare {
                self.toast(
                    "Sonneneruption vorbei – Instrumente wieder klar",
                    ToastKind::Info,
                );
            }
            return;
        }
        if !flying {
            return;
        }
        self.event_timer -= DT;
        if self.event_timer > fx!(0.0) {
            return;
        }
        let ev = self.data.world.events.clone();
        self.event_timer = self.rng.range(ev.interval.0, ev.interval.1);
        let roll = self.rng.unit();
        if roll < fx!(0.4) {
            let a = self.rng.range(fx!(0.0), Fx::TAU);
            let dir = Vec2::new(a.cos(), a.sin());
            self.toast(
                format!(
                    "Meteoritenschauer aus {} im Anflug – ausweichen!",
                    compass(dir)
                ),
                ToastKind::Bad,
            );
            self.event = Some(WorldEvent {
                kind: EventKind::MeteorShower,
                time_left: fx!(11.0),
                dir,
                spawn_timer: fx!(0.0),
            });
        } else if roll < fx!(0.75) {
            self.spontaneous_distress(ev.distress_bonus);
        } else {
            self.ship.shield *= fx!(0.4);
            self.toast(
                format!(
                    "Sonneneruption! Schild geschwächt, Radar und Scanner gestört ({:.0} s)",
                    ev.flare_seconds
                ),
                ToastKind::Warn,
            );
            self.event = Some(WorldEvent {
                kind: EventKind::SolarFlare,
                time_left: ev.flare_seconds,
                dir: Vec2::ZERO,
                spawn_timer: fx!(0.0),
            });
        }
    }

    fn spawn_shower_meteor(&mut self, dir: Vec2) {
        let side = Vec2::new(-dir.y, dir.x);
        let start = self.ship.pos + dir * fx!(140.0) + side * self.rng.range(-fx!(60.0), fx!(60.0));
        let target = self.ship.pos
            + self.ship.vel * fx!(2.0)
            + Vec2::new(
                self.rng.range(-fx!(25.0), fx!(25.0)),
                self.rng.range(-fx!(25.0), fx!(25.0)),
            );
        let speed = self.rng.range(fx!(22.0), fx!(34.0));
        let vel = (target - start).normalize_or_zero() * speed;
        let r = self.rng.range(fx!(0.6), fx!(1.5));
        let id = self.next_id();
        self.bodies.push(Body {
            id,
            kind: BodyKind::Meteor,
            pos: start,
            vel,
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

    /// Spontanes Notsignal in der Nähe: höher bezahlt, auf der Karte annehmbar.
    fn spontaneous_distress(&mut self, bonus: Fx) {
        let a = self.rng.range(fx!(0.0), Fx::TAU);
        let d = self.rng.range(fx!(260.0), fx!(480.0));
        let mut site = self.ship.pos + Vec2::new(a.cos(), a.sin()) * d;
        let lim = self.world.radius * fx!(0.95);
        if site.length() > lim {
            site = site.normalize() * lim;
        }
        let mut m = self.generate_distress_at(site);
        m.reward = ((Fx::from(m.reward) * bonus / fx!(5.0)).round() * fx!(5.0)).to_u32();
        let title = m.title(self);
        self.offers.push(m);
        self.toast(
            format!("Notsignal aufgefangen: {title} – Karte (Tab) zum Annehmen"),
            ToastKind::Warn,
        );
    }

    /// Scanner-Impuls starten (vom Werkzeug aus).
    pub(crate) fn start_scan(&mut self, origin: Vec2) -> bool {
        if self.scan.is_some() {
            return false;
        }
        if self.flare() {
            self.toast("Scanner gestört – Sonneneruption", ToastKind::Warn);
            return false;
        }
        let nebula = self.sector_at(origin).nebula;
        // Artefakte an Bord stören den Scanner ebenfalls.
        let jam = self.artifact_jam();
        let mut max =
            self.ship.scan_range * (fx!(1.0) - fx!(0.55) * nebula) * (fx!(1.0) - fx!(0.6) * jam);
        // Reparierter Sender in der Nähe (76): Scanner reicht weiter.
        if self.effect_near(super::data::EffectKind::Beacon, origin) {
            max *= fx!(1.5);
        }
        self.scan = Some(ScanPulse {
            origin,
            radius: fx!(0.0),
            max,
        });
        self.events.push(SimEvent::ScanPulse {
            pos: origin,
            range: max,
        });
        self.story_scan_start(origin);
        true
    }

    fn update_scan(&mut self) {
        for b in &mut self.blips {
            b.life -= DT;
        }
        self.blips.retain(|b| b.life > fx!(0.0));
        let Some(mut p) = self.scan.take() else {
            return;
        };
        let r0 = p.radius;
        p.radius = (p.radius + SCAN_SPEED * DT).min(p.max);
        let ring = |q: Vec2| {
            let d = (q - p.origin).length();
            d >= r0 && d < p.radius
        };
        self.scan_routes(p.origin, r0, p.radius);
        let mut found = Vec::new();
        let mut wrecks = Vec::new();
        for b in &self.bodies {
            if !b.alive || !ring(b.pos) {
                continue;
            }
            if let BodyKind::Wreck { idx, .. } = b.kind {
                wrecks.push(idx);
            }
            let kind = match &b.kind {
                BodyKind::Artifact { id } if !self.artifact_def(id).is_some_and(|a| a.hidden) => {
                    BlipKind::Signal
                }
                BodyKind::Wreck { .. } | BodyKind::Derelict { .. } => BlipKind::Wreck,
                BodyKind::Asteroid { ore: Some(o), .. } => BlipKind::Ore(*o),
                BodyKind::OreChunk { ore, .. } => BlipKind::Ore(*ore),
                BodyKind::Capsule { .. } => BlipKind::Capsule,
                BodyKind::Crate { .. } | BodyKind::Salvage { .. } | BodyKind::Dropped { .. } => {
                    BlipKind::Cargo
                }
                _ => continue,
            };
            found.push(ScanBlip {
                pos: b.pos,
                kind,
                life: BLIP_SECONDS,
            });
        }
        for pl in &self.world.planets {
            for d in &pl.deposits {
                let q = pl.pos + Vec2::new(d.angle.cos(), d.angle.sin()) * pl.radius;
                if d.amount > fx!(0.5) && ring(q) {
                    found.push(ScanBlip {
                        pos: q,
                        kind: BlipKind::Deposit(pl.ore),
                        life: BLIP_SECONDS,
                    });
                }
            }
        }
        // Bordbücher der Wracks im Ring auslesen.
        for idx in wrecks {
            self.story_scan_wreck(idx);
        }
        // Gefundenes ersetzt alte Markierungen am selben Ort.
        for f in found {
            self.blips.retain(|b| (b.pos - f.pos).longer_than(fx!(2.0)));
            self.blips.push(f);
        }
        if p.radius >= p.max {
            self.explored.reveal(p.origin, p.max);
            let new = self.surveyed.reveal(p.origin, p.max);
            if new > 0 {
                self.charts_unsold += new;
                self.toast(
                    format!("Kartiert: {new} neue Sektorzellen – an Stationen verkaufen"),
                    ToastKind::Info,
                );
            }
        } else {
            self.scan = Some(p);
        }
    }

    /// Wert der unverkauften Kartendaten hier.
    pub fn charts_value(&self) -> u32 {
        let rep = self.docked_station().map_or(0, |si| self.rep_level_at(si));
        let k = fx!(1.0) + fx!(0.1) * Fx::from(rep);
        (Fx::from(self.charts_unsold) * Fx::from(self.data.shop.chart_price) * k).round_int() as u32
    }

    pub(crate) fn sell_charts(&mut self) {
        if self.docked_station().is_none() || self.charts_unsold == 0 {
            return;
        }
        let value = self.charts_value();
        let n = self.charts_unsold;
        self.charts_unsold = 0;
        self.crew.credits += value;
        self.events.push(SimEvent::Sold { credits: value });
        self.toast(
            format!("Kartendaten verkauft: {n} Zellen  +{value} Credits"),
            ToastKind::Good,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_names() {
        assert_eq!(compass(Vec2::X), "Osten");
        assert_eq!(compass(Vec2::Y), "Norden");
        assert_eq!(compass(Vec2::new(-fx!(1.0), -fx!(1.0))), "Südwesten");
    }
}
