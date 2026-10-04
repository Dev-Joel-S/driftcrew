//! Missionen: Liefern, Abbauen, Notrufe (Abschleppen, Kapseln einsammeln).

use bevy::math::Vec2;

use super::data::{CargoTrait, MissionType, Ore, Service, ToolKind, v};
use super::rng::hash32;
use super::ship::{CargoKind, CraneState};
use super::stats::{CrewStats, MissionReport};
use super::world::Owner;
use super::{Body, BodyKind, DT, SimEvent, SimState, ToastKind};

pub const MAX_ACTIVE: usize = 4;
/// Ab dieser Beschleunigung (m/s²) wird es den Passagieren ungemütlich.
pub const COMFORT_ACCEL: f32 = 10.0;
/// Kollisionsradius einer Schwerlastkiste.
pub const CRATE_RADIUS: f32 = 1.5;
/// Seilbelastung, unter der eine Last als „sanft geführt“ gilt (Bonus).
pub const GENTLE_STRAIN: f32 = 0.55;
/// Lastaufnahme: so ruhig und so lange muss die Kiste darin liegen.
pub const SOCKET_MAX_SPEED: f32 = 0.5;
pub const SOCKET_SETTLE: f32 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub enum MissionKind {
    /// Fracht im Frachtraum von A nach B – A kann auch ein Planeten-Außenposten sein
    /// (Material verschicken).
    Delivery {
        from: Owner,
        to: usize,
        cargo: String,
        mass: f32,
        /// Flugeigenschaften der Fracht (vor der Annahme sichtbar).
        traits: CargoTrait,
    },
    /// Schwerlast: zu schwer für den Frachtraum, wird als Kiste am Kran geschleppt.
    Haul {
        from: usize,
        to: usize,
        cargo: String,
        mass: f32,
        body: Option<u32>,
        /// Hat die Zielstation eine Lastaufnahme: wie lange die Kiste schon ruhig darin liegt.
        settle: f32,
    },
    Mining {
        ore: Ore,
        amount: f32,
        to: usize,
    },
    /// Sperrige Bergung: Objekt liegt am Fundort, muss außen am Kran in die Ablage.
    Bulky {
        site: Vec2,
        to: usize,
        name: String,
        body: Option<u32>,
        mass: f32,
        half_len: f32,
        thick: f32,
    },
    /// Passagiere: Zufriedenheit 0..1 sinkt bei harter Beschleunigung, Stößen und Kreiseln.
    Passengers {
        from: usize,
        to: usize,
        count: u32,
        comfort: f32,
        aboard: bool,
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
    /// Messflug: Messfelder der Reihe nach anfliegen und dort stillhalten.
    Survey {
        /// Indizes in `courses.ron`, survey_sites.
        sites: Vec<usize>,
        done: usize,
        /// Sekunden, die im aktuellen Feld schon gemessen wurden.
        hold: f32,
    },
    /// Geleitschutz: ein NPC-Frachter fliegt von A nach B, unterwegs lauern Piratendrohnen.
    Escort {
        from: usize,
        to: usize,
        /// Name des Frachters, sein NPC (sobald er an der Plattform steht), Hinterhalt ausgelöst?
        name: String,
        npc: Option<u32>,
        ambushed: bool,
    },
    /// Schmuggel: Ware im Frachtraum an den Zollbojen vorbei zum Ziel bringen.
    Smuggle {
        from: usize,
        to: usize,
        cargo: String,
        mass: f32,
    },
    /// Rettung (78): ein havariertes Schiff treibt am Notrufort. Längsseits gehen holt die
    /// Besatzung Person für Person an Bord – jede braucht Platz im Frachtraum.
    Rescue {
        site: Vec2,
        to: usize,
        ship: String,
        captain: String,
        crew: u32,
        aboard: u32,
        /// Das havarierte NPC-Schiff (ab Annahme).
        npc: Option<u32>,
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
    /// Höchste Seilbelastung, solange die Last des Auftrags am Kran hing (None = nie).
    pub max_strain: Option<f32>,
    /// Richtzeit in Sekunden (Zeitbonus, wenn schneller).
    pub par: f32,
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

    pub fn mission_type(&self) -> MissionType {
        match &self.kind {
            MissionKind::Delivery {
                from: Owner::Planet(_),
                ..
            } => MissionType::Shipment,
            MissionKind::Delivery { .. } => MissionType::Delivery,
            MissionKind::Haul { .. } => MissionType::Haul,
            MissionKind::Mining { .. } => MissionType::Mining,
            MissionKind::Passengers { .. } => MissionType::Passengers,
            MissionKind::Bulky { .. } => MissionType::Bulky,
            MissionKind::Tow { .. } => MissionType::Tow,
            MissionKind::Capsules { .. } => MissionType::Capsules,
            MissionKind::Survey { .. } => MissionType::Survey,
            MissionKind::Escort { .. } => MissionType::Escort,
            MissionKind::Smuggle { .. } => MissionType::Smuggle,
            MissionKind::Rescue { .. } => MissionType::Rescue,
        }
    }

    /// Für welche Crewgröße der Auftrag gedacht ist (von, bis).
    pub fn crew(&self, s: &SimState) -> (u8, u8) {
        let t = self.mission_type();
        s.data
            .missions
            .crew
            .iter()
            .find(|(m, _)| *m == t)
            .map(|(_, c)| *c)
            .unwrap_or((1, 4))
    }

    pub fn title(&self, s: &SimState) -> String {
        let st = |i: usize| s.world.stations[i].name.clone();
        match &self.kind {
            MissionKind::Delivery { to, cargo, .. } => format!("Liefern: {cargo} → {}", st(*to)),
            MissionKind::Haul { to, cargo, .. } => format!("Schwerlast: {cargo} → {}", st(*to)),
            MissionKind::Passengers { to, count, .. } => {
                format!("Passagiere: {count} Personen → {}", st(*to))
            }
            MissionKind::Bulky { name, to, .. } => format!("Bergung: {name} → {}", st(*to)),
            MissionKind::Mining { ore, amount, to } => {
                format!("Abbau: {amount:.0} t {} → {}", ore.label(), st(*to))
            }
            MissionKind::Tow { name, to, .. } => {
                format!("Notruf: {name} abschleppen → {}", st(*to))
            }
            MissionKind::Capsules { total, to, .. } => {
                format!("Notruf: {total} Rettungskapseln → {}", st(*to))
            }
            MissionKind::Survey { sites, .. } => match sites.as_slice() {
                [one] => format!("Messflug: {}", s.data.courses.survey_sites[*one].name),
                _ => format!("Messflug: {} Messfelder", sites.len()),
            },
            MissionKind::Escort { name, to, .. } => {
                format!("Geleitschutz: {name} → {}", st(*to))
            }
            MissionKind::Smuggle { cargo, to, .. } => format!("Schmuggel: {cargo} → {}", st(*to)),
            MissionKind::Rescue { ship, crew, to, .. } => {
                format!("Notruf: {ship} – {crew} Personen retten → {}", st(*to))
            }
        }
    }

    pub fn detail(&self, s: &SimState) -> String {
        match &self.kind {
            MissionKind::Delivery {
                from, mass, traits, ..
            } => {
                let aboard = s.ship.cargo.iter().find(
                    |c| matches!(c.kind, CargoKind::Container { mission, .. } if mission == self.id),
                );
                match (aboard, traits) {
                    (Some(c), CargoTrait::Fragile) => format!(
                        "{mass:.1} t empfindlich – Zustand {:.0} %, Stöße vermeiden",
                        c.cond * 100.0
                    ),
                    (Some(c), CargoTrait::Unstable) => format!(
                        "{mass:.1} t instabil – Belastung {:.0} %, sanft beschleunigen",
                        c.stress * 100.0
                    ),
                    (Some(_), CargoTrait::Tank) => {
                        format!("{mass:.1} t Tank – die Ladung schwappt nach, früh gegensteuern")
                    }
                    (Some(_), CargoTrait::None) => format!("{mass:.1} t Fracht an Bord"),
                    (None, CargoTrait::None) => format!(
                        "{mass:.1} t Fracht, Abholung an {}",
                        s.world.owner_name(*from)
                    ),
                    (None, t) => format!(
                        "{mass:.1} t {}, Abholung an {}",
                        t.label(),
                        s.world.owner_name(*from)
                    ),
                }
            }
            MissionKind::Passengers {
                from,
                comfort,
                aboard,
                ..
            } => {
                if *aboard {
                    format!(
                        "Zufriedenheit {:.0} % – sanft beschleunigen, nicht anecken",
                        comfort * 100.0
                    )
                } else {
                    format!("Abholung an {}", s.world.stations[*from].name)
                }
            }
            MissionKind::Bulky {
                mass,
                half_len,
                thick,
                to,
                ..
            } => format!(
                "{mass:.0} t, {:.0} m lang – außen am Kran, in die Ablage von {}",
                2.0 * (half_len + thick),
                s.world.stations[*to].name
            ),
            MissionKind::Haul {
                from,
                to,
                mass,
                body,
                ..
            } => {
                let socket = s.world.stations[*to].socket.is_some();
                if body.is_some() && socket {
                    format!(
                        "{mass:.0} t am Kran – in die Lastaufnahme von {} setzen, ruhig ablegen und loslassen",
                        s.world.stations[*to].name
                    )
                } else if socket {
                    format!(
                        "{mass:.0} t Kiste am Kran, Abholung an {} – Ziel hat eine Lastaufnahme",
                        s.world.stations[*from].name
                    )
                } else {
                    format!(
                        "{mass:.0} t Kiste am Kran schleppen, Abholung an {}",
                        s.world.stations[*from].name
                    )
                }
            }
            MissionKind::Survey { sites, done, hold } => {
                let sd = &s.data.missions.survey;
                match sites.get(*done) {
                    Some(&i) => format!(
                        "Messfeld {}/{}: {} – {:.1}/{:.0} s stillhalten (unter {:.1} m/s)",
                        done + 1,
                        sites.len(),
                        s.data.courses.survey_sites[i].name,
                        hold,
                        sd.seconds,
                        sd.max_speed
                    ),
                    None => "Messdaten übertragen".into(),
                }
            }
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
            MissionKind::Escort { from, npc, .. } => {
                match npc.and_then(|id| s.npcs.iter().find(|n| n.id == id && n.alive)) {
                    Some(n) if n.docked_pad().is_some() && !n.has_departed() => format!(
                        "Der Frachter wartet in {} – abdocken und in der Nähe bleiben",
                        s.world.stations[*from].name
                    ),
                    Some(n) => format!(
                        "Frachter: Hülle {:.0} % – Drohnen abfangen, dicht dranbleiben",
                        n.ship.hull / n.ship.max_hull * 100.0
                    ),
                    None => "Frachter unterwegs".into(),
                }
            }
            MissionKind::Smuggle { mass, .. } => {
                let scan = s.data.missions.smuggle.scan_time;
                match &s.customs {
                    Some(c) => format!(
                        "ZOLLSCAN {:.0} % – raus aus dem Radius der Boje!",
                        c.progress * 100.0
                    ),
                    None => format!(
                        "{mass:.1} t im Frachtraum – Zollbojen meiden (Scan dauert {scan:.0} s)"
                    ),
                }
            }
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
            MissionKind::Rescue {
                crew, aboard, to, ..
            } => {
                if aboard >= crew {
                    format!(
                        "Alle {crew} an Bord – zur Station {}",
                        s.world.stations[*to].name
                    )
                } else {
                    format!(
                        "An Bord {aboard}/{crew} – längsseits gehen (unter {:.0} m, kaum Fahrt), je Person {:.1} t Platz",
                        super::cargo::RESCUE_GAP,
                        super::cargo::PERSON_MASS
                    )
                }
            }
        }
    }

    /// Wohin soll die Navigation zeigen?
    pub fn nav_target(&self, s: &SimState) -> Option<Vec2> {
        let station = |i: usize| s.world.stations[i].pos;
        match &self.kind {
            MissionKind::Delivery { to, .. } => Some(station(*to)),
            MissionKind::Passengers {
                from, to, aboard, ..
            } => Some(station(if *aboard { *to } else { *from })),
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
            MissionKind::Bulky { site, to, body, .. } => {
                let attached = s.ship.tools.iter().any(
                    |t| matches!(t.crane, CraneState::Attached { body: b, .. } if Some(b) == *body),
                );
                if attached {
                    Some(s.drop_point(*to).0)
                } else {
                    body.and_then(|id| s.bodies.iter().find(|b| b.id == id).map(|b| b.pos))
                        .or(Some(*site))
                }
            }
            MissionKind::Survey { sites, done, .. } => sites
                .get(*done)
                .map(|&i| v(s.data.courses.survey_sites[i].pos)),
            MissionKind::Escort { from, to, npc, .. } => {
                match npc.and_then(|id| s.npcs.iter().find(|n| n.id == id && n.alive)) {
                    Some(n) if n.has_departed() => Some(n.ship.pos),
                    Some(_) => Some(station(*from)),
                    None => Some(station(*to)),
                }
            }
            MissionKind::Smuggle { to, .. } => Some(station(*to)),
            MissionKind::Rescue {
                site,
                to,
                crew,
                aboard,
                npc,
                ..
            } => {
                if aboard >= crew {
                    Some(station(*to))
                } else {
                    npc.and_then(|id| s.npcs.iter().find(|n| n.id == id && n.alive))
                        .map(|n| n.ship.pos)
                        .or(Some(*site))
                }
            }
            MissionKind::Haul { from, to, body, .. } => {
                let attached = s.ship.tools.iter().any(
                    |t| matches!(t.crane, CraneState::Attached { body: b, .. } if Some(b) == *body),
                );
                let socket = s.world.stations[*to].socket.map(|k| k.center);
                if attached {
                    Some(socket.unwrap_or(station(*to)))
                } else if let Some(k) = socket
                    && let Some(b) = body.and_then(|id| s.bodies.iter().find(|b| b.id == id))
                    && (b.pos - k).length() < 30.0
                {
                    // Kiste liegt schon an der Aufnahme: dorthin zeigen.
                    Some(k)
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
    /// Ablagezone einer Station für sperrige Bergung (Mitte, Radius). Ohne eigene Zone gilt
    /// die Nähe der Station.
    pub fn drop_point(&self, si: usize) -> (Vec2, f32) {
        let st = &self.world.stations[si];
        match self.data.world.stations[si].drop_zone {
            Some((off, r)) => (st.pos + v(off), r),
            None => (
                st.pos,
                (st.bounds.max - st.bounds.min).length() * 0.5 + 25.0,
            ),
        }
    }

    /// Schafft das aktuelle Schiff ein sperriges Objekt dieser Masse? (Kran nötig, und mit der
    /// Last am Seil muss noch genug Beschleunigung bleiben.)
    pub fn bulky_feasible(&self, mass: f32) -> Result<(), String> {
        if !self.ship.has_tool(ToolKind::Crane) {
            return Err("Braucht einen belegten Kran".into());
        }
        let thrust: f32 = self
            .ship
            .thrusters
            .iter()
            .map(|t| t.effective_thrust())
            .sum();
        let accel = thrust / (self.ship.mass + mass);
        if accel < 1.6 {
            let name = &self.data.ship(&self.crew.current_ship).name;
            return Err(format!(
                "Zu schwer für den {name} mit dieser Crew ({accel:.1} m/s² mit Last)"
            ));
        }
        Ok(())
    }

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
            MissionType::Passengers => n_st > 1,
            MissionType::Bulky => !md.bulky.is_empty(),
            MissionType::Survey => !self.data.courses.survey_sites.is_empty(),
            MissionType::Escort => !self.escort_targets(si).is_empty(),
            MissionType::Smuggle => n_st > 1 && !md.smuggle.cargo.is_empty(),
            MissionType::Shipment
            | MissionType::Tow
            | MissionType::Capsules
            | MissionType::Rescue => false,
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
            MissionType::Escort => {
                let targets = self.escort_targets(si);
                let to = targets[self.rng.index(targets.len())];
                let names = &md.escort.names;
                let name = names
                    .get(self.rng.index(names.len().max(1)))
                    .cloned()
                    .unwrap_or_else(|| "Frachter".into());
                let dist = (self.world.stations[to].pos - self.world.stations[si].pos).length();
                (
                    MissionKind::Escort {
                        from: si,
                        to,
                        name,
                        npc: None,
                        ambushed: false,
                    },
                    md.escort.reward + dist * md.reward_per_distance * 1.5,
                )
            }
            MissionType::Smuggle => {
                let to = self.smuggle_target(si);
                let sm = &md.smuggle;
                let cargo = sm.cargo[self.rng.index(sm.cargo.len())].clone();
                let mass = (self.rng.range(sm.mass.0, sm.mass.1) * 2.0).round() / 2.0;
                let dist = (self.world.stations[to].pos - self.world.stations[si].pos).length();
                (
                    MissionKind::Smuggle {
                        from: si,
                        to,
                        cargo,
                        mass,
                    },
                    sm.reward + dist * md.reward_per_distance * 2.0,
                )
            }
            MissionType::Survey => {
                let n_sites = self.data.courses.survey_sites.len();
                let want = self
                    .rng
                    .range_u32(md.survey.fields.0, md.survey.fields.1)
                    .clamp(1, n_sites as u32) as usize;
                let mut sites: Vec<usize> = Vec::new();
                while sites.len() < want {
                    let i = self.rng.index(n_sites);
                    if !sites.contains(&i) {
                        sites.push(i);
                    }
                }
                // Strecke: Station → Feld → Feld …
                let mut at = self.world.stations[si].pos;
                let mut dist = 0.0;
                for &i in &sites {
                    let p = v(self.data.courses.survey_sites[i].pos);
                    dist += (p - at).length();
                    at = p;
                }
                (
                    MissionKind::Survey {
                        sites,
                        done: 0,
                        hold: 0.0,
                    },
                    want as f32 * md.survey.reward_per_field + dist * md.reward_per_distance,
                )
            }
            MissionType::Bulky => {
                let t = md.bulky[self.rng.index(md.bulky.len())].clone();
                // Fundort: ein Notrufort oder Wrack in der Welt, leicht verstreut.
                let sites = self.data.world.distress_sites.clone();
                let site = v(sites[self.rng.index(sites.len())])
                    + Vec2::new(self.rng.range(-40.0, 40.0), self.rng.range(-40.0, 40.0));
                let dist = (site - self.world.stations[si].pos).length();
                (
                    MissionKind::Bulky {
                        site,
                        to: si,
                        name: t.name.clone(),
                        body: None,
                        mass: t.mass,
                        half_len: t.half_len,
                        thick: t.thick,
                    },
                    t.reward as f32 + dist * md.reward_per_distance,
                )
            }
            MissionType::Passengers => {
                let mut to = self.rng.index(n_st - 1);
                if to >= si {
                    to += 1;
                }
                let count = self.rng.range_u32(md.passengers.0, md.passengers.1);
                let dist = (self.world.stations[to].pos - self.world.stations[si].pos).length();
                (
                    MissionKind::Passengers {
                        from: si,
                        to,
                        count,
                        comfort: 1.0,
                        aboard: false,
                    },
                    count as f32 * md.fare_per_person + dist * md.reward_per_distance,
                )
            }
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
                        settle: 0.0,
                    }
                } else {
                    MissionKind::Delivery {
                        from: Owner::Station(si),
                        to,
                        cargo: t.name.clone(),
                        mass: t.mass,
                        traits: t.traits,
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
        let par = self.par_time(&kind);
        Mission {
            id,
            kind,
            reward: round5(reward * bonus),
            origin: Some(Owner::Station(si)),
            giver,
            start: None,
            top_speed: 0.0,
            max_strain: None,
            par,
        }
    }

    /// Richtzeit: Strecke durch ein gemütliches Tempo plus Zeit fürs Hantieren.
    pub fn par_time(&self, kind: &MissionKind) -> f32 {
        let st = |i: usize| self.world.stations[i].pos;
        let at = |o: Owner| match o {
            Owner::Station(i) => st(i),
            Owner::Planet(i) => self.world.planets[i].pos,
        };
        let t = match kind {
            MissionKind::Delivery { from, to, .. } => (at(*from) - st(*to)).length() / 10.0 + 60.0,
            MissionKind::Passengers { from, to, .. } => (st(*from) - st(*to)).length() / 8.0 + 60.0,
            MissionKind::Haul { from, to, .. } => (st(*from) - st(*to)).length() / 6.0 + 90.0,
            MissionKind::Mining { amount, to, .. } => {
                // Grob: hin zur nächsten Quelle und zurück, plus Abbau.
                let src = self
                    .world
                    .planets
                    .iter()
                    .map(|p| (p.pos - st(*to)).length())
                    .fold(f32::MAX, f32::min);
                src.min(2000.0) * 2.0 / 12.0 + amount * 12.0 + 60.0
            }
            MissionKind::Tow { site, to, .. } => (*site - st(*to)).length() / 6.0 + 150.0,
            MissionKind::Bulky { site, to, .. } => (*site - st(*to)).length() / 6.0 + 180.0,
            MissionKind::Capsules { site, to, .. } => (*site - st(*to)).length() / 10.0 + 240.0,
            MissionKind::Rescue { site, to, crew, .. } => {
                (*site - st(*to)).length() / 9.0 + 120.0 + *crew as f32 * 10.0
            }
            // Der Frachter fliegt höchstens 16 m/s, dazu Abflug, Anflug und Gefecht.
            MissionKind::Escort { from, to, .. } => (st(*from) - st(*to)).length() / 9.0 + 120.0,
            MissionKind::Smuggle { from, to, .. } => (st(*from) - st(*to)).length() / 10.0 + 90.0,
            MissionKind::Survey { sites, .. } => {
                // Ab der nächstgelegenen Station mit Aufträgen grob abgeschätzt.
                let mut at = sites
                    .first()
                    .map(|&i| v(self.data.courses.survey_sites[i].pos))
                    .unwrap_or_default();
                let start = self
                    .world
                    .stations
                    .iter()
                    .map(|st| (st.pos - at).length())
                    .fold(f32::MAX, f32::min);
                let mut d = start;
                for &i in sites.iter().skip(1) {
                    let p = v(self.data.courses.survey_sites[i].pos);
                    d += (p - at).length();
                    at = p;
                }
                d / 10.0 + sites.len() as f32 * (self.data.missions.survey.seconds + 30.0) + 30.0
            }
        };
        (t / 10.0).round() * 10.0
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
        let kind = MissionKind::Delivery {
            from: Owner::Planet(pi),
            to,
            cargo: format!("{}-Ladung", ore.label()),
            mass,
            traits: CargoTrait::None,
        };
        let par = self.par_time(&kind);
        Mission {
            id,
            kind,
            reward: round5(mass * md.shipment_per_t + dist * md.reward_per_distance),
            origin: Some(Owner::Planet(pi)),
            giver,
            start: None,
            top_speed: 0.0,
            max_strain: None,
            par,
        }
    }

    fn generate_distress(&mut self) -> Mission {
        let sites = self.data.world.distress_sites.clone();
        let site = v(sites[self.rng.index(sites.len())])
            + Vec2::new(self.rng.range(-30.0, 30.0), self.rng.range(-30.0, 30.0));
        self.generate_distress_at(site)
    }

    /// Notruf an einem bestimmten Ort (auch für spontane Notsignale).
    pub(crate) fn generate_distress_at(&mut self, site: Vec2) -> Mission {
        let id = self.next_id();
        let md = self.data.missions.clone();
        // Zielstation: die nächstgelegene Station mit Reparaturdock.
        let to = (0..self.world.stations.len())
            .min_by(|a, b| {
                (self.world.stations[*a].pos - site)
                    .length()
                    .total_cmp(&(self.world.stations[*b].pos - site).length())
            })
            .unwrap_or(0);
        let roll = self.rng.range(0.0, 1.0);
        let rd = &md.rescue;
        let (kind, reward) = if roll < 0.3 && !rd.ships.is_empty() && !rd.captains.is_empty() {
            let ship = rd.ships[self.rng.index(rd.ships.len())].clone();
            let captain = rd.captains[self.rng.index(rd.captains.len())].clone();
            let crew = self.rng.range_u32(rd.crew.0, rd.crew.1);
            (
                MissionKind::Rescue {
                    site,
                    to,
                    ship,
                    captain,
                    crew,
                    aboard: 0,
                    npc: None,
                },
                120.0 + crew as f32 * rd.reward_per_person,
            )
        } else if roll < 0.65 {
            let name = md.derelict_names[self.rng.index(md.derelict_names.len())].clone();
            (
                MissionKind::Tow {
                    site,
                    to,
                    name,
                    body: None,
                },
                self.rng.range(md.tow_reward.0, md.tow_reward.1),
            )
        } else {
            let total = self.rng.range_u32(md.capsule_count.0, md.capsule_count.1);
            (
                MissionKind::Capsules {
                    site,
                    total,
                    delivered: 0,
                    to,
                },
                self.rng.range(md.capsule_reward.0, md.capsule_reward.1),
            )
        };
        let par = self.par_time(&kind);
        Mission {
            id,
            kind,
            reward: round5(reward),
            origin: None,
            giver: None,
            start: None,
            top_speed: 0.0,
            max_strain: None,
            par,
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
                from,
                cargo,
                mass,
                traits,
                ..
            } => {
                if self.docked_owner() != Some(*from) {
                    let name = self.world.owner_name(*from).to_string();
                    self.toast(
                        format!("Fracht liegt in {name} – dort andocken"),
                        ToastKind::Warn,
                    );
                    return;
                }
                let stored = self.ship.store_with(
                    CargoKind::Container {
                        mission: m.id,
                        name: cargo.clone(),
                    },
                    *mass,
                    *traits,
                );
                if stored <= 0.0 {
                    self.toast(
                        "Kein Platz im Frachtraum für den Container",
                        ToastKind::Warn,
                    );
                    return;
                }
            }
            MissionKind::Bulky {
                site,
                name,
                body,
                mass,
                half_len,
                thick,
                ..
            } => {
                if let Err(reason) = self.bulky_feasible(*mass) {
                    self.toast(reason, ToastKind::Warn);
                    return;
                }
                let bid = self.next_id();
                let pos = *site;
                let angle = self.rng.range(0.0, 6.2);
                self.bodies.push(Body {
                    id: bid,
                    kind: BodyKind::Bulky {
                        mission: m.id,
                        name: name.clone(),
                        half_len: *half_len,
                        thick: *thick,
                    },
                    pos,
                    vel: Vec2::ZERO,
                    angle,
                    ang_vel: self.rng.range(-0.15, 0.15),
                    radius: *half_len + *thick,
                    mass: *mass,
                    prev_pos: pos,
                    prev_angle: angle,
                    alive: true,
                    seed: hash32(bid),
                    age: 0.0,
                });
                *body = Some(bid);
            }
            MissionKind::Passengers { from, aboard, .. } => {
                if self.docked_station() != Some(*from) {
                    let name = self.world.stations[*from].name.clone();
                    self.toast(
                        format!("Die Passagiere warten in {name} – dort andocken"),
                        ToastKind::Warn,
                    );
                    return;
                }
                *aboard = true;
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
            MissionKind::Escort {
                from,
                name,
                npc,
                to,
                ..
            } => {
                if self.docked_station() != Some(*from) {
                    let st = self.world.stations[*from].name.clone();
                    self.toast(
                        format!("Der Frachter startet in {st} – dort andocken"),
                        ToastKind::Warn,
                    );
                    return;
                }
                *npc = Some(self.spawn_convoy(name, m.id, *from, *to));
            }
            MissionKind::Smuggle {
                from, cargo, mass, ..
            } => {
                if self.docked_station() != Some(*from) {
                    let name = self.world.stations[*from].name.clone();
                    self.toast(
                        format!("Die Ware liegt in {name} – dort andocken"),
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
                    self.toast("Kein Platz im Frachtraum für die Ware", ToastKind::Warn);
                    return;
                }
            }
            MissionKind::Rescue {
                site, ship, npc, ..
            } => {
                *npc = Some(self.spawn_stranded(ship, m.id, *site));
            }
            MissionKind::Mining { .. } | MissionKind::Survey { .. } => {}
        }
        // Am Einsatzort einer Bergung treibt vielleicht noch etwas Wertvolles (77).
        let bonus_site = match &m.kind {
            MissionKind::Tow { site, .. }
            | MissionKind::Capsules { site, .. }
            | MissionKind::Bulky { site, .. }
            | MissionKind::Rescue { site, .. } => Some(*site),
            _ => None,
        };
        self.offers.remove(idx);
        let title = m.title(self);
        m.start = Some(Box::new(self.stats.clone()));
        m.top_speed = 0.0;
        self.active.push(m);
        self.events.push(SimEvent::MissionAccepted { id });
        self.toast(format!("Auftrag angenommen: {title}"), ToastKind::Info);
        if let Some(site) = bonus_site {
            self.maybe_spawn_bonus(site);
        }
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
            matches!(k, CargoKind::Container { mission, .. } | CargoKind::Capsule { mission } | CargoKind::Survivor { mission } if *mission == mid)
        });
        // Das havarierte Schiff wird abgeschleppt (bzw. von anderen geborgen).
        if let MissionKind::Rescue { npc: Some(id), .. } = m.kind {
            self.remove_npc(id);
        }
        for b in &mut self.bodies {
            match &b.kind {
                BodyKind::Capsule { mission }
                | BodyKind::Derelict { mission, .. }
                | BodyKind::Crate { mission, .. }
                | BodyKind::Bulky { mission, .. }
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

    pub(crate) fn complete_mission(&mut self, idx: usize) {
        let m = self.active.remove(idx);
        let start = m.start.as_deref().cloned().unwrap_or_default();
        let stats = self.stats.since(&start, m.top_speed);
        let md = &self.data.missions;
        // Passagiere zahlen nach Zufriedenheit (mindestens 30 %).
        let comfort = match m.kind {
            MissionKind::Passengers { comfort, .. } => Some(comfort),
            _ => None,
        };
        // Empfindliche Fracht zahlt nach Zustand (ebenfalls mindestens 30 %).
        let cond = match &m.kind {
            MissionKind::Delivery {
                traits: CargoTrait::Fragile,
                ..
            } => Some(self.delivery_condition(m.id)),
            _ => None,
        };
        let base = match comfort.or(cond) {
            Some(c) => round5(m.reward as f32 * (0.3 + 0.7 * c)),
            None => m.reward,
        };
        let collisions: u32 = stats.slots.iter().map(|x| x.collisions).sum();
        let bonus_time = if stats.time <= m.par {
            round5(base as f32 * md.time_bonus)
        } else {
            0
        };
        let bonus_clean = if collisions == 0 && stats.damage < 1.0 {
            round5(base as f32 * md.clean_bonus)
        } else {
            0
        };
        // Kranarbeit mit ruhiger Hand (Punkt 36): die Last nie hart ans Seil gerissen.
        let bonus_gentle = match m.max_strain {
            Some(k) if k < GENTLE_STRAIN => round5(base as f32 * md.clean_bonus),
            _ => 0,
        };
        let earned = base + bonus_time + bonus_clean + bonus_gentle;
        // Abrechnung: Versicherungsprämie und Kreditrate gehen ab, der Rest in die Kasse.
        let (premium, installment) = self.settle_mission(earned);
        let paid = earned - premium - installment;
        self.crew.credits += paid;
        self.crew.missions_done += 1;
        let title = m.title(self);
        self.events.push(SimEvent::MissionCompleted {
            id: m.id,
            reward: paid,
        });
        self.toast(
            format!("Auftrag erfüllt: {title}  +{paid} Credits"),
            ToastKind::Good,
        );
        // Die Welt reagiert (76): was sich durch den Auftrag eine Weile ändert.
        let effect = self.mission_effect(&m.kind, m.origin);
        if let Some(text) = &effect {
            self.toast(text.clone(), ToastKind::Good);
        }
        // Ruf: bei der Station, die den Auftrag vergeben hat – bei Notrufen und Lieferungen
        // von Außenposten bei der Zielstation.
        let rep_station = match (m.origin, &m.kind) {
            (Some(Owner::Station(si)), _) => Some(si),
            (_, MissionKind::Delivery { to, .. })
            | (_, MissionKind::Tow { to, .. })
            | (_, MissionKind::Capsules { to, .. })
            | (_, MissionKind::Rescue { to, .. }) => Some(*to),
            _ => None,
        };
        let reputation = rep_station.map(|si| {
            let gain = if matches!(
                m.kind,
                MissionKind::Haul { .. }
                    | MissionKind::Tow { .. }
                    | MissionKind::Bulky { .. }
                    | MissionKind::Rescue { .. }
            ) {
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
        self.report = Some(MissionReport {
            mission: m.id,
            title,
            reward: base,
            bonus_time,
            bonus_clean,
            bonus_gentle,
            max_strain: m.max_strain,
            comfort,
            cond,
            effect,
            par: m.par,
            reputation,
            stats,
            thruster_slots: self.ship.thrusters.iter().map(|t| t.slot).collect(),
            premium,
            installment,
        });
        self.cleanup_mission(&m);
    }

    /// Prüfungen, die jeden Tick laufen: Geschlepptes (Wrack, Schwerlastkiste) ist am Ziel,
    /// sobald es nahe genug an der Zielstation ist – bei Stationen mit Lastaufnahme erst, wenn
    /// die Kiste ruhig darin liegt und nicht mehr am Kran hängt. Messflüge messen.
    pub(crate) fn update_missions(&mut self) {
        self.update_survey();
        // Lastaufnahme: wie lange liegt die Kiste schon ruhig drin?
        let hooked: Vec<u32> = self
            .ship
            .tools
            .iter()
            .filter_map(|t| match t.crane {
                CraneState::Attached { body, .. } => Some(body),
                _ => None,
            })
            .collect();
        for m in &mut self.active {
            if let MissionKind::Haul {
                to,
                body: Some(bid),
                settle,
                ..
            } = &mut m.kind
                && let Some(sock) = self.world.stations[*to].socket
            {
                let resting = self
                    .bodies
                    .iter()
                    .find(|b| b.id == *bid && b.alive)
                    .is_some_and(|b| {
                        sock.holds(b.pos)
                            && b.vel.length() < SOCKET_MAX_SPEED
                            && b.ang_vel.abs() < 0.8
                    });
                *settle = if resting && !hooked.contains(bid) {
                    *settle + DT
                } else {
                    0.0
                };
            }
        }
        let mut done = None;
        for (i, m) in self.active.iter().enumerate() {
            let (to, bid, bulky) = match &m.kind {
                MissionKind::Haul {
                    to,
                    body: Some(b),
                    settle,
                    ..
                } if self.world.stations[*to].socket.is_some() => {
                    if *settle >= SOCKET_SETTLE {
                        done = Some((i, *b));
                        break;
                    }
                    continue;
                }
                MissionKind::Tow {
                    to, body: Some(b), ..
                }
                | MissionKind::Haul {
                    to, body: Some(b), ..
                } => (*to, *b, false),
                MissionKind::Bulky {
                    to, body: Some(b), ..
                } => (*to, *b, true),
                _ => continue,
            };
            // Sperriges gehört in die Ablagezone, alles andere nur in die Nähe der Station.
            let (target, reach) = if bulky {
                self.drop_point(to)
            } else {
                let st = &self.world.stations[to];
                (
                    st.pos,
                    (st.bounds.max - st.bounds.min).length() * 0.5 + 25.0,
                )
            };
            if let Some(b) = self.bodies.iter().find(|b| b.id == bid && b.alive)
                && (b.pos - target).length() < reach
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

    /// Messflug: im aktuellen Feld stillhalten; nach dem letzten Feld ist der Auftrag erfüllt.
    fn update_survey(&mut self) {
        let sd = self.data.missions.survey.clone();
        let mut finished = None;
        for i in 0..self.active.len() {
            let MissionKind::Survey { sites, done, hold } = &self.active[i].kind else {
                continue;
            };
            let Some(&site) = sites.get(*done) else {
                continue;
            };
            let (n, done_now, hold_now) = (sites.len(), *done, *hold);
            let def = &self.data.courses.survey_sites[site];
            let (pos, radius) = (v(def.pos), def.radius);
            let still = self.holding_still(pos, radius, sd.max_speed);
            let mut hold = if still {
                hold_now + DT
            } else {
                (hold_now - 2.0 * DT).max(0.0)
            };
            let mut done = done_now;
            if hold >= sd.seconds {
                done += 1;
                hold = 0.0;
                self.events.push(SimEvent::SurveyField { pos });
                if done < n {
                    self.toast(
                        format!("Messung {done}/{n} übertragen – weiter zum nächsten Feld"),
                        ToastKind::Good,
                    );
                } else {
                    finished = Some(i);
                }
            }
            if let MissionKind::Survey {
                done: d, hold: h, ..
            } = &mut self.active[i].kind
            {
                *d = done;
                *h = hold;
            }
            if finished.is_some() {
                break;
            }
        }
        if let Some(i) = finished {
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
                MissionKind::Delivery { to, .. } | MissionKind::Smuggle { to, .. } => *to == si,
                MissionKind::Passengers { to, aboard, .. } => *aboard && *to == si,
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
                MissionKind::Rescue {
                    to,
                    crew,
                    aboard,
                    ship,
                    captain,
                    ..
                } if *to == si && aboard >= crew => {
                    let mid = m.id;
                    self.ship
                        .remove_cargo(|k| *k == CargoKind::Survivor { mission: mid });
                    self.remember_rescue(ship, captain);
                    true
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
