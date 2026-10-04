//! Sektoren mit Effekten (Trümmer, Nebel, Sonnenwind), Zufallsereignisse
//! (Meteoritenschauer, Notsignal, Sonneneruption), Scanner-Impulse und Kartendaten.

use bevy::math::Vec2;

use super::data::{Ore, SectorEffect, v};
use super::rng::hash32;
use super::{Body, BodyKind, DT, SimEvent, SimState, ToastKind};

/// Wie stark wirken die Sektoren an einem Punkt?
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SectorState {
    pub debris: f32,
    pub nebula: f32,
    pub wind: Vec2,
}

/// Ausbreitungsgeschwindigkeit des Scanner-Impulses (m/s).
pub const SCAN_SPEED: f32 = 320.0;
/// So lange bleiben gefundene Dinge markiert (s).
pub const BLIP_SECONDS: f32 = 30.0;

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
    pub life: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScanPulse {
    pub origin: Vec2,
    pub radius: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    MeteorShower,
    SolarFlare,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldEvent {
    pub kind: EventKind,
    pub time_left: f32,
    /// Herkunftsrichtung (Meteoritenschauer).
    pub dir: Vec2,
    pub spawn_timer: f32,
}

impl WorldEvent {
    pub fn flare_active(&self) -> bool {
        self.kind == EventKind::SolarFlare && self.time_left > 0.0
    }
}

/// Himmelsrichtung für Meldungen.
pub fn compass(dir: Vec2) -> &'static str {
    let a = dir.y.atan2(dir.x).to_degrees().rem_euclid(360.0);
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
    NAMES[(((a + 22.5) / 45.0) as usize) % 8]
}

impl SimState {
    /// Effekte aller Sektoren an einem Punkt, mit weichem Rand (voll bis 70 % des Radius).
    pub fn sector_at(&self, p: Vec2) -> SectorState {
        let mut s = SectorState::default();
        for r in &self.data.world.regions {
            let Some(e) = r.effect else { continue };
            let d = (p - v(r.center)).length();
            let k = ((r.radius - d) / (r.radius * 0.3)).clamp(0.0, 1.0);
            if k <= 0.0 {
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
            if matches!(b.kind, BodyKind::Debris) && (b.pos - me).length() > 240.0 {
                b.alive = false;
            }
        }
        if self.ship.destroyed {
            return;
        }
        let density = self.sector_at(me).debris;
        let want = (density * 70.0) as usize;
        let have = self
            .bodies
            .iter()
            .filter(|b| b.alive && matches!(b.kind, BodyKind::Debris))
            .count();
        // Erst füllen (auch nah am Schiff), danach nur noch außerhalb der Sicht nachlegen.
        let near: f32 = if have < want / 2 { 25.0 } else { 90.0 };
        for _ in have..want.min(have + 10) {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            // Flächengleich verteilt zwischen `near` und 170 m.
            let u = self.rng.f32();
            let d = (near * near + (170.0f32 * 170.0 - near * near) * u).sqrt();
            let pos = me + Vec2::new(a.cos(), a.sin()) * d;
            if self.point_in_static(pos) || self.sector_at(pos).debris <= 0.05 {
                continue;
            }
            let r = self.rng.range(0.5, 1.6);
            let id = self.next_id();
            let vel = Vec2::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0)) * 2.5;
            self.bodies.push(Body {
                id,
                kind: BodyKind::Debris,
                pos,
                vel,
                angle: a,
                ang_vel: self.rng.range(-2.0, 2.0),
                radius: r,
                mass: 0.8 + r * r * 2.0,
                prev_pos: pos,
                prev_angle: a,
                alive: true,
                seed: hash32(id),
                age: 0.0,
            });
        }
    }

    pub(crate) fn update_events(&mut self) {
        let flying = self.ship.docked.is_none() && !self.ship.destroyed;
        if let Some(mut e) = self.event.take() {
            e.time_left -= DT;
            if e.kind == EventKind::MeteorShower && e.time_left < 8.0 && flying {
                e.spawn_timer -= DT;
                if e.spawn_timer <= 0.0 {
                    let n = self.data.world.events.shower_meteors.max(1) as f32;
                    e.spawn_timer = 8.0 / n;
                    self.spawn_shower_meteor(e.dir);
                }
            }
            if e.time_left > 0.0 {
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
        if self.event_timer > 0.0 {
            return;
        }
        let ev = self.data.world.events.clone();
        self.event_timer = self.rng.range(ev.interval.0, ev.interval.1);
        let roll = self.rng.f32();
        if roll < 0.4 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
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
                time_left: 11.0,
                dir,
                spawn_timer: 0.0,
            });
        } else if roll < 0.75 {
            self.spontaneous_distress(ev.distress_bonus);
        } else {
            self.ship.shield *= 0.4;
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
                spawn_timer: 0.0,
            });
        }
    }

    fn spawn_shower_meteor(&mut self, dir: Vec2) {
        let side = Vec2::new(-dir.y, dir.x);
        let start = self.ship.pos + dir * 140.0 + side * self.rng.range(-60.0, 60.0);
        let target = self.ship.pos
            + self.ship.vel * 2.0
            + Vec2::new(self.rng.range(-25.0, 25.0), self.rng.range(-25.0, 25.0));
        let speed = self.rng.range(22.0, 34.0);
        let vel = (target - start).normalize_or_zero() * speed;
        let r = self.rng.range(0.6, 1.5);
        let id = self.next_id();
        self.bodies.push(Body {
            id,
            kind: BodyKind::Meteor,
            pos: start,
            vel,
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

    /// Spontanes Notsignal in der Nähe: höher bezahlt, auf der Karte annehmbar.
    fn spontaneous_distress(&mut self, bonus: f32) {
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        let d = self.rng.range(260.0, 480.0);
        let mut site = self.ship.pos + Vec2::new(a.cos(), a.sin()) * d;
        let lim = self.world.radius * 0.95;
        if site.length() > lim {
            site = site.normalize() * lim;
        }
        let mut m = self.generate_distress_at(site);
        m.reward = ((m.reward as f32 * bonus / 5.0).round() * 5.0) as u32;
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
        let max = self.ship.scan_range * (1.0 - 0.55 * nebula) * (1.0 - 0.6 * jam);
        self.scan = Some(ScanPulse {
            origin,
            radius: 0.0,
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
        self.blips.retain(|b| b.life > 0.0);
        let Some(mut p) = self.scan.take() else {
            return;
        };
        let r0 = p.radius;
        p.radius = (p.radius + SCAN_SPEED * DT).min(p.max);
        let ring = |q: Vec2| {
            let d = (q - p.origin).length();
            d >= r0 && d < p.radius
        };
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
                if d.amount > 0.5 && ring(q) {
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
            self.blips.retain(|b| (b.pos - f.pos).length() > 2.0);
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
        let k = 1.0 + 0.1 * rep as f32;
        (self.charts_unsold as f32 * self.data.shop.chart_price as f32 * k).round() as u32
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
        assert_eq!(compass(Vec2::new(-1.0, -1.0)), "Südwesten");
    }
}
