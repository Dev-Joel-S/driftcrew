//! Missionen: Liefern, Abbauen, Notrufe (Abschleppen, Kapseln einsammeln).

use bevy::math::Vec2;

use super::data::{MissionType, Ore, Service, ToolKind, v};
use super::rng::hash32;
use super::ship::{CargoKind, CraneState};
use super::stats::{CrewStats, MissionReport};
use super::world::Owner;
use super::{Body, BodyKind, SimEvent, SimState, ToastKind};

pub const MAX_ACTIVE: usize = 4;
/// Kollisionsradius einer Schwerlastkiste.
pub const CRATE_RADIUS: f32 = 1.5;

#[derive(Clone, Debug, PartialEq)]
pub enum MissionKind {
    /// Fracht im Frachtraum von A nach B – A kann auch ein Planeten-Außenposten sein
    /// (Material verschicken).
    Delivery {
        from: Owner,
        to: usize,
        cargo: String,
        mass: f32,
    },
    /// Schwerlast: zu schwer für den Frachtraum, wird als Kiste am Kran geschleppt.
    Haul {
        from: usize,
        to: usize,
        cargo: String,
        mass: f32,
        body: Option<u32>,
    },
    Mining {
        ore: Ore,
        amount: f32,
        to: usize,
    },
    Tow {
        site: Vec2,
        to: usize,
        name: String,
        body: Option<u32>,
    },
    Capsules {
        site: Vec2,
        total: u32,
        delivered: u32,
        to: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mission {
    pub id: u32,
    pub kind: MissionKind,
    pub reward: u32,
    /// Ort, der den Auftrag anbietet. `None` = Notruf, überall annehmbar.
    pub origin: Option<Owner>,
    /// Auftraggeber (Index in `npcs.ron`).
    pub giver: Option<usize>,
    /// Statistikstand bei Annahme (für die Auswertung).
    pub start: Option<Box<CrewStats>>,
    pub top_speed: f32,
}

/// Rufstufen: Punkte ab denen die Stufe gilt, und ihre Namen.
pub const REP_LEVELS: [u32; 4] = [0, 3, 7, 12];
pub const REP_NAMES: [&str; 4] = ["Neu", "Bekannt", "Geschätzt", "Partner"];

pub fn rep_level(points: u32) -> u8 {
    REP_LEVELS.iter().rposition(|&p| points >= p).unwrap_or(0) as u8
}

impl Mission {
    pub fn is_distress(&self) -> bool {
        self.origin.is_none()
    }

    pub fn title(&self, s: &SimState) -> String {
        let st = |i: usize| s.world.stations[i].name.clone();
        match &self.kind {
            MissionKind::Delivery { to, cargo, .. } => format!("Liefern: {cargo} → {}", st(*to)),
            MissionKind::Haul { to, cargo, .. } => format!("Schwerlast: {cargo} → {}", st(*to)),
            MissionKind::Mining { ore, amount, to } => {
                format!("Abbau: {amount:.0} t {} → {}", ore.label(), st(*to))
            }
            MissionKind::Tow { name, to, .. } => {
                format!("Notruf: {name} abschleppen → {}", st(*to))
            }
            MissionKind::Capsules { total, to, .. } => {
                format!("Notruf: {total} Rettungskapseln → {}", st(*to))
            }
        }
    }

    pub fn detail(&self, s: &SimState) -> String {
        match &self.kind {
            MissionKind::Delivery { from, mass, .. } => {
                format!(
                    "{mass:.1} t Fracht, Abholung an {}",
                    s.world.owner_name(*from)
                )
            }
            MissionKind::Haul { from, mass, .. } => format!(
                "{mass:.0} t Kiste am Kran schleppen, Abholung an {}",
                s.world.stations[*from].name
            ),
            MissionKind::Mining { ore, .. } => {
                let have = s.ship.ore_amount(*ore).max(0.0);
                let source = s
                    .world
                    .planets
                    .iter()
                    .find(|p| p.ore == *ore)
                    .map(|p| p.name.clone())
                    .or_else(|| {
                        s.data
                            .world
                            .asteroid_fields
                            .iter()
                            .find(|f| f.ore == *ore)
                            .map(|f| f.name.clone())
                    })
                    .unwrap_or_else(|| "?".into());
                format!("Vorkommen: {source} · an Bord {have:.1} t")
            }
            MissionKind::Tow { .. } => "Treibendes Schiff mit dem Kran zur Station ziehen".into(),
            MissionKind::Capsules {
                total, delivered, ..
            } => {
                let aboard = s
                    .ship
                    .cargo
                    .iter()
                    .filter(|c| c.kind == CargoKind::Capsule { mission: self.id })
                    .count();
                format!("Abgeliefert {delivered}/{total}, an Bord {aboard}")
            }
        }
    }

    /// Wohin soll die Navigation zeigen?
    pub fn nav_target(&self, s: &SimState) -> Option<Vec2> {
        let station = |i: usize| s.world.stations[i].pos;
        match &self.kind {
            MissionKind::Delivery { to, .. } => Some(station(*to)),
            MissionKind::Mining { ore, amount, to } => {
                if s.ship.ore_amount(*ore) + 1e-3 >= *amount {
                    Some(station(*to))
                } else {
                    let ship = s.ship.pos;
                    let planet = s
                        .world
                        .planets
                        .iter()
                        .filter(|p| p.ore == *ore)
                        .map(|p| p.pos)
                        .min_by(|a, b| (*a - ship).length().total_cmp(&(*b - ship).length()));
                    planet.or_else(|| {
                        s.data
                            .world
                            .asteroid_fields
                            .iter()
                            .find(|f| f.ore == *ore)
                            .map(|f| v(f.center))
                    })
                }
            }
            MissionKind::Haul { from, to, body, .. } => {
                let attached = s.ship.tools.iter().any(
                    |t| matches!(t.crane, CraneState::Attached { body: b, .. } if Some(b) == *body),
                );
                if attached {
                    Some(station(*to))
                } else {
                    body.and_then(|id| s.bodies.iter().find(|b| b.id == id).map(|b| b.pos))
                        .or(Some(station(*from)))
                }
            }
            MissionKind::Tow { site, to, body, .. } => {
                let attached = s.ship.tools.iter().any(
                    |t| matches!(t.crane, CraneState::Attached { body: b, .. } if Some(b) == *body),
                );
                if attached {
                    Some(station(*to))
                } else {
                    body.and_then(|id| s.bodies.iter().find(|b| b.id == id).map(|b| b.pos))
                        .or(Some(*site))
                }
            }
            MissionKind::Capsules { site, to, .. } => {
                let left = s
                    .bodies
                    .iter()
                    .filter(|b| b.alive && b.kind == BodyKind::Capsule { mission: self.id })
                    .min_by(|a, b| {
                        (a.pos - s.ship.pos)
                            .length()
                            .total_cmp(&(b.pos - s.ship.pos).length())
                    });
                match left {
                    Some(b) => Some(b.pos),
                    None if s
                        .ship
                        .cargo
                        .iter()
                        .any(|c| c.kind == CargoKind::Capsule { mission: self.id }) =>
                    {
                        Some(station(*to))
                    }
                    None => Some(*site),
                }
            }
        }
    }
}

fn round5(x: f32) -> u32 {
    ((x / 5.0).round() * 5.0).max(5.0) as u32
}

impl SimState {
    /// Rufstufe bei einer Station.
    pub fn rep_level_at(&self, si: usize) -> u8 {
        rep_level(self.crew.reputation.get(si).copied().unwrap_or(0))
    }

    /// Auftraggeber an einem Ort.
    pub fn npcs_at(&self, owner: Owner) -> Vec<usize> {
        let id = match owner {
            Owner::Station(i) => &self.world.stations[i].id,
            Owner::Planet(i) => &self.data.world.planets[i].id,
        };
        (0..self.data.npcs.len())
            .filter(|&n| &self.data.npcs[n].at == id)
            .collect()
    }

    /// Angebote an Stationen, Außenposten und Notrufe auffüllen.
    pub(crate) fn refresh_offers(&mut self) {
        let per = self.data.missions.offers_per_station as usize;
        for si in 0..self.world.stations.len() {
            if !self.world.stations[si].has(Service::Missions) {
                continue;
            }
            // Mehr Ruf = mehr Angebote.
            let want = per + self.rep_level_at(si) as usize;
            let here = Some(Owner::Station(si));
            while self.offers.iter().filter(|m| m.origin == here).count() < want {
                let m = self.generate_station_offer(si);
                self.offers.push(m);
            }
        }
        let per_outpost = self.data.missions.offers_per_outpost as usize;
        for pi in 0..self.world.planets.len() {
            let owner = Owner::Planet(pi);
            if self.world.planets[pi].pad.is_none() || self.npcs_at(owner).is_empty() {
                continue;
            }
            while self
                .offers
                .iter()
                .filter(|m| m.origin == Some(owner))
                .count()
                < per_outpost
            {
                let m = self.generate_outpost_offer(pi);
                self.offers.push(m);
            }
        }
        while self.offers.iter().filter(|m| m.origin.is_none()).count()
            < self.data.missions.distress_offers as usize
        {
            let m = self.generate_distress();
            self.offers.push(m);
        }
    }

    fn generate_station_offer(&mut self, si: usize) -> Mission {
        let id = self.next_id();
        let md = self.data.missions.clone();
        let n_st = self.world.stations.len();
        let level = self.rep_level_at(si);
        let ore_exists = |s: &SimState, ore: Ore| {
            s.world.planets.iter().any(|p| p.ore == ore)
                || s.data.world.asteroid_fields.iter().any(|f| f.ore == ore)
        };
        let ores_exist = md.mining.iter().any(|m| ore_exists(self, m.ore));
        // Wer vergibt den Auftrag, und welche Art? Schwerlast gibt es erst ab Stufe „Bekannt“.
        let npcs = self.npcs_at(Owner::Station(si));
        let giver = (!npcs.is_empty()).then(|| npcs[self.rng.index(npcs.len())]);
        let mut types: Vec<MissionType> = match giver {
            Some(g) => self.data.npcs[g].gives.clone(),
            None => vec![MissionType::Delivery, MissionType::Mining],
        };
        types.retain(|t| match t {
            MissionType::Delivery => n_st > 1,
            MissionType::Haul => n_st > 1 && level >= 1,
            MissionType::Mining => ores_exist,
            MissionType::Shipment => false,
        });
        if types.is_empty() {
            types.push(if n_st > 1 {
                MissionType::Delivery
            } else {
                MissionType::Mining
            });
        }
        let ty = types[self.rng.index(types.len())];
        let bonus = 1.0 + 0.1 * level as f32;
        let (kind, reward) = match ty {
            MissionType::Delivery | MissionType::Haul => {
                let mut to = self.rng.index(n_st - 1);
                if to >= si {
                    to += 1;
                }
                let want_towed = ty == MissionType::Haul;
                let pool: Vec<_> = md
                    .delivery_cargo
                    .iter()
                    .filter(|c| c.towed == want_towed)
                    .cloned()
                    .collect();
                let pool = if pool.is_empty() {
                    md.delivery_cargo.clone()
                } else {
                    pool
                };
                let t = &pool[self.rng.index(pool.len())];
                let dist = (self.world.stations[to].pos - self.world.stations[si].pos).length();
                let kind = if t.towed {
                    MissionKind::Haul {
                        from: si,
                        to,
                        cargo: t.name.clone(),
                        mass: t.mass,
                        body: None,
                    }
                } else {
                    MissionKind::Delivery {
                        from: Owner::Station(si),
                        to,
                        cargo: t.name.clone(),
                        mass: t.mass,
                    }
                };
                (kind, t.reward as f32 + dist * md.reward_per_distance)
            }
            _ => {
                // Nur Erze, die es in der Welt auch gibt.
                let available: Vec<_> = md
                    .mining
                    .iter()
                    .filter(|m| ore_exists(self, m.ore))
                    .cloned()
                    .collect();
                let t = &available[self.rng.index(available.len())];
                // Größere Aufträge für Crews mit gutem Ruf.
                let scale = 1.0 + 0.15 * level as f32;
                let amount = (self.rng.range(t.amount.0, t.amount.1) * scale * 2.0).round() / 2.0;
                (
                    MissionKind::Mining {
                        ore: t.ore,
                        amount,
                        to: si,
                    },
                    amount * t.reward_per_t,
                )
            }
        };
        Mission {
            id,
            kind,
            reward: round5(reward * bonus),
            origin: Some(Owner::Station(si)),
            giver,
            start: None,
            top_speed: 0.0,
        }
    }

    /// Außenposten verschicken ihr Erz als Ladung zu einer Station.
    fn generate_outpost_offer(&mut self, pi: usize) -> Mission {
        let id = self.next_id();
        let md = self.data.missions.clone();
        let npcs = self.npcs_at(Owner::Planet(pi));
        let giver = (!npcs.is_empty()).then(|| npcs[self.rng.index(npcs.len())]);
        let targets: Vec<usize> = (0..self.world.stations.len())
            .filter(|&i| self.world.stations[i].has(Service::Market))
            .collect();
        let to = targets[self.rng.index(targets.len())];
        let ore = self.world.planets[pi].ore;
        let mass = (self.rng.range(3.0, 7.0) * 2.0).round() / 2.0;
        let dist = (self.world.stations[to].pos - self.world.planets[pi].pos).length();
        Mission {
            id,
            kind: MissionKind::Delivery {
                from: Owner::Planet(pi),
                to,
                cargo: format!("{}-Ladung", ore.label()),
                mass,
            },
            reward: round5(mass * md.shipment_per_t + dist * md.reward_per_distance),
            origin: Some(Owner::Planet(pi)),
            giver,
            start: None,
            top_speed: 0.0,
        }
    }

    fn generate_distress(&mut self) -> Mission {
        let id = self.next_id();
        let md = self.data.missions.clone();
        let sites = self.data.world.distress_sites.clone();
        let site = v(sites[self.rng.index(sites.len())])
            + Vec2::new(self.rng.range(-30.0, 30.0), self.rng.range(-30.0, 30.0));
        // Zielstation: die nächstgelegene Station mit Reparaturdock.
        let to = (0..self.world.stations.len())
            .min_by(|a, b| {
                (self.world.stations[*a].pos - site)
                    .length()
                    .total_cmp(&(self.world.stations[*b].pos - site).length())
            })
            .unwrap_or(0);
        if self.rng.chance(0.5) {
            let name = md.derelict_names[self.rng.index(md.derelict_names.len())].clone();
            Mission {
                id,
                kind: MissionKind::Tow {
                    site,
                    to,
                    name,
                    body: None,
                },
                reward: round5(self.rng.range(md.tow_reward.0, md.tow_reward.1)),
                origin: None,
                giver: None,
                start: None,
                top_speed: 0.0,
            }
        } else {
            let total = self.rng.range_u32(md.capsule_count.0, md.capsule_count.1);
            Mission {
                id,
                kind: MissionKind::Capsules {
                    site,
                    total,
                    delivered: 0,
                    to,
                },
                reward: round5(self.rng.range(md.capsule_reward.0, md.capsule_reward.1)),
                origin: None,
                giver: None,
                start: None,
                top_speed: 0.0,
            }
        }
    }

    pub(crate) fn accept_mission(&mut self, id: u32) {
        let Some(idx) = self.offers.iter().position(|m| m.id == id) else {
            return;
        };
        if self.active.len() >= MAX_ACTIVE {
            self.toast(
                format!("Höchstens {MAX_ACTIVE} Aufträge gleichzeitig"),
                ToastKind::Warn,
            );
            return;
        }
        let mut m = self.offers[idx].clone();
        match &mut m.kind {
            MissionKind::Delivery {
                from, cargo, mass, ..
            } => {
                if self.docked_owner() != Some(*from) {
                    let name = self.world.owner_name(*from).to_string();
                    self.toast(
                        format!("Fracht liegt in {name} – dort andocken"),
                        ToastKind::Warn,
                    );
                    return;
                }
                let stored = self.ship.store(
                    CargoKind::Container {
                        mission: m.id,
                        name: cargo.clone(),
                    },
                    *mass,
                );
                if stored <= 0.0 {
                    self.toast(
                        "Kein Platz im Frachtraum für den Container",
                        ToastKind::Warn,
                    );
                    return;
                }
            }
            MissionKind::Haul {
                from,
                cargo,
                mass,
                body,
                ..
            } => {
                let here = self.docked_station();
                if here != Some(*from) {
                    let name = self.world.stations[*from].name.clone();
                    self.toast(
                        format!("Die Kiste steht in {name} – dort andocken"),
                        ToastKind::Warn,
                    );
                    return;
                }
                if !self.ship.has_tool(ToolKind::Crane) {
                    self.toast("Schwerlast braucht einen belegten Kran", ToastKind::Warn);
                    return;
                }
                // Kiste schwebt über der Plattform, auf der das Schiff steht.
                let pad = self.world.pads[self.ship.docked.unwrap_or(0)].clone();
                let pos = pad.center + pad.normal * (self.ship.rest_height() * 2.0 + 5.5);
                let bid = self.next_id();
                self.bodies.push(Body {
                    id: bid,
                    kind: BodyKind::Crate {
                        mission: m.id,
                        name: cargo.clone(),
                    },
                    pos,
                    vel: Vec2::ZERO,
                    angle: pad.ship_angle(),
                    ang_vel: 0.0,
                    radius: CRATE_RADIUS,
                    mass: *mass,
                    prev_pos: pos,
                    prev_angle: pad.ship_angle(),
                    alive: true,
                    seed: hash32(bid),
                    age: 0.0,
                });
                *body = Some(bid);
            }
            MissionKind::Tow {
                site, body, name, ..
            } => {
                let bid = self.next_id();
                let pos = *site;
                self.bodies.push(Body {
                    id: bid,
                    kind: BodyKind::Derelict {
                        mission: m.id,
                        name: name.clone(),
                    },
                    pos,
                    vel: Vec2::new(self.rng.range(-0.5, 0.5), self.rng.range(-0.5, 0.5)),
                    angle: self.rng.range(0.0, 6.2),
                    ang_vel: self.rng.range(-0.25, 0.25),
                    radius: 2.4,
                    mass: 16.0,
                    prev_pos: pos,
                    prev_angle: 0.0,
                    alive: true,
                    seed: hash32(bid),
                    age: 0.0,
                });
                *body = Some(bid);
            }
            MissionKind::Capsules { site, total, .. } => {
                for k in 0..*total {
                    let bid = self.next_id();
                    let a =
                        std::f32::consts::TAU * k as f32 / *total as f32 + self.rng.range(0.0, 0.8);
                    let pos = *site + Vec2::new(a.cos(), a.sin()) * self.rng.range(6.0, 22.0);
                    self.bodies.push(Body {
                        id: bid,
                        kind: BodyKind::Capsule { mission: m.id },
                        pos,
                        vel: Vec2::new(self.rng.range(-0.8, 0.8), self.rng.range(-0.8, 0.8)),
                        angle: 0.0,
                        ang_vel: self.rng.range(-1.0, 1.0),
                        radius: 0.7,
                        mass: 0.8,
                        prev_pos: pos,
                        prev_angle: 0.0,
                        alive: true,
                        seed: hash32(bid),
                        age: 0.0,
                    });
                }
            }
            MissionKind::Mining { .. } => {}
        }
        self.offers.remove(idx);
        let title = m.title(self);
        m.start = Some(Box::new(self.stats.clone()));
        m.top_speed = 0.0;
        self.active.push(m);
        self.events.push(SimEvent::MissionAccepted { id });
        self.toast(format!("Auftrag angenommen: {title}"), ToastKind::Info);
        self.refresh_offers();
    }

    pub(crate) fn abandon_mission(&mut self, id: u32) {
        let Some(idx) = self.active.iter().position(|m| m.id == id) else {
            return;
        };
        let m = self.active.remove(idx);
        self.cleanup_mission(&m);
        self.toast("Auftrag abgebrochen", ToastKind::Warn);
    }

    fn cleanup_mission(&mut self, m: &Mission) {
        let mid = m.id;
        self.ship.remove_cargo(|k| {
            matches!(k, CargoKind::Container { mission, .. } | CargoKind::Capsule { mission } if *mission == mid)
        });
        for b in &mut self.bodies {
            match &b.kind {
                BodyKind::Capsule { mission }
                | BodyKind::Derelict { mission, .. }
                | BodyKind::Crate { mission, .. }
                    if *mission == mid =>
                {
                    b.alive = false;
                }
                _ => {}
            }
        }
    }

    pub(crate) fn fail_mission(&mut self, id: u32, reason: &str) {
        if let Some(idx) = self.active.iter().position(|m| m.id == id) {
            let m = self.active.remove(idx);
            self.cleanup_mission(&m);
            let title = m.title(self);
            self.events.push(SimEvent::MissionFailed { id });
            self.toast(
                format!("Auftrag gescheitert ({reason}): {title}"),
                ToastKind::Bad,
            );
        }
    }

    fn complete_mission(&mut self, idx: usize) {
        let m = self.active.remove(idx);
        self.crew.credits += m.reward;
        self.crew.missions_done += 1;
        let title = m.title(self);
        self.events.push(SimEvent::MissionCompleted {
            id: m.id,
            reward: m.reward,
        });
        self.toast(
            format!("Auftrag erfüllt: {title}  +{} Credits", m.reward),
            ToastKind::Good,
        );
        // Ruf: bei der Station, die den Auftrag vergeben hat – bei Notrufen und Lieferungen
        // von Außenposten bei der Zielstation.
        let rep_station = match (m.origin, &m.kind) {
            (Some(Owner::Station(si)), _) => Some(si),
            (_, MissionKind::Delivery { to, .. })
            | (_, MissionKind::Tow { to, .. })
            | (_, MissionKind::Capsules { to, .. }) => Some(*to),
            _ => None,
        };
        let reputation = rep_station.map(|si| {
            let gain = if matches!(m.kind, MissionKind::Haul { .. } | MissionKind::Tow { .. }) {
                2
            } else {
                1
            };
            let before = self.rep_level_at(si);
            if let Some(p) = self.crew.reputation.get_mut(si) {
                *p += gain;
            }
            let after = self.rep_level_at(si);
            let name = self.world.stations[si].name.clone();
            if after > before {
                self.toast(
                    format!(
                        "Ruf bei {name}: {} – mehr und besser bezahlte Aufträge",
                        REP_NAMES[after as usize]
                    ),
                    ToastKind::Good,
                );
            }
            (name, after, gain)
        });
        let start = m.start.as_deref().cloned().unwrap_or_default();
        self.report = Some(MissionReport {
            mission: m.id,
            title,
            reward: m.reward,
            reputation,
            stats: self.stats.since(&start, m.top_speed),
            thruster_slots: self.ship.thrusters.iter().map(|t| t.slot).collect(),
        });
        self.cleanup_mission(&m);
    }

    /// Prüfungen, die jeden Tick laufen: Geschlepptes (Wrack, Schwerlastkiste) ist am Ziel,
    /// sobald es nahe genug an der Zielstation ist.
    pub(crate) fn update_missions(&mut self) {
        let mut done = None;
        for (i, m) in self.active.iter().enumerate() {
            let (to, bid) = match &m.kind {
                MissionKind::Tow {
                    to, body: Some(b), ..
                }
                | MissionKind::Haul {
                    to, body: Some(b), ..
                } => (*to, *b),
                _ => continue,
            };
            let st = &self.world.stations[to];
            let reach = (st.bounds.max - st.bounds.min).length() * 0.5 + 25.0;
            if let Some(b) = self.bodies.iter().find(|b| b.id == bid && b.alive)
                && (b.pos - st.pos).length() < reach
            {
                done = Some((i, bid));
                break;
            }
        }
        if let Some((i, bid)) = done {
            for t in &mut self.ship.tools {
                if matches!(t.crane, CraneState::Attached { body, .. } if body == bid) {
                    t.crane = CraneState::Idle;
                    self.events.push(SimEvent::CraneRelease);
                }
            }
            self.complete_mission(i);
        }
    }

    /// Beim Andocken: Lieferungen abschließen.
    pub(crate) fn on_docked(&mut self, owner: Owner) {
        let Owner::Station(si) = owner else {
            self.refresh_offers();
            return;
        };
        let mut i = 0;
        while i < self.active.len() {
            let m = self.active[i].clone();
            let finished = match &m.kind {
                MissionKind::Delivery { to, .. } => *to == si,
                MissionKind::Mining { ore, amount, to } => {
                    if *to == si && self.ship.ore_amount(*ore) + 1e-3 >= *amount {
                        self.ship.take_ore(*ore, *amount);
                        true
                    } else {
                        false
                    }
                }
                MissionKind::Capsules {
                    total,
                    delivered,
                    to,
                    ..
                } if *to == si => {
                    let mid = m.id;
                    let n = self
                        .ship
                        .remove_cargo(|k| *k == CargoKind::Capsule { mission: mid })
                        .len() as u32;
                    let now = delivered + n;
                    if let MissionKind::Capsules { delivered: d, .. } = &mut self.active[i].kind {
                        *d = now;
                    }
                    if n > 0 && now < *total {
                        self.toast(
                            format!("Kapseln abgeliefert: {now}/{total}"),
                            ToastKind::Info,
                        );
                    }
                    now >= *total
                }
                _ => false,
            };
            if finished {
                self.complete_mission(i);
            } else {
                i += 1;
            }
        }
        self.refresh_offers();
    }

    pub fn offers_here(&self) -> Vec<&Mission> {
        let here = self.docked_owner();
        self.offers
            .iter()
            .filter(|m| m.origin.is_none() || m.origin == here)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::data::{CrewSave, GameData};
    use crate::sim::ship::Loadout;
    use crate::sim::{Command, TickInput};
    use std::sync::Arc;

    fn sim() -> SimState {
        let data = Arc::new(GameData::embedded().unwrap());
        let save = CrewSave::new_game(&data);
        let def = data.ship(&save.current_ship).clone();
        SimState::new(data, &save, Loadout::full(&def), 1)
    }

    #[test]
    fn offers_exist_and_delivery_completes_on_dock() {
        let mut s = sim();
        assert!(!s.offers.is_empty());
        let here = s.docked_station().unwrap();
        let m = s
            .offers
            .iter()
            .find(|m| {
                matches!(m.kind, MissionKind::Delivery { from, .. } if from == Owner::Station(here))
            })
            .cloned();
        let Some(m) = m else { return };
        s.step(&TickInput {
            commands: vec![Command::AcceptMission { id: m.id }],
            ..Default::default()
        });
        assert_eq!(s.active.len(), 1);
        assert!(s.ship.cargo_mass() > 0.0);
        let MissionKind::Delivery { to, .. } = m.kind else {
            unreachable!()
        };
        let credits = s.crew.credits;
        s.ship.docked = None;
        s.dock_at_station(to);
        s.on_docked(Owner::Station(to));
        assert!(s.active.is_empty());
        assert_eq!(s.crew.credits, credits + m.reward);
        assert!(s.ship.cargo_mass() < 1e-4);
    }
}
