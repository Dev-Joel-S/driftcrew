//! NPC-Schiffe (Punkte 17, 21, 40, 44): dieselben Schiffe und dieselbe Physik wie die Crew,
//! gesteuert vom Autopiloten über echte Triebwerke (zwei Triebwerke, wie eine
//! Zwei-Personen-Crew). Frachter pendeln und docken an, Piratendrohnen greifen an,
//! Schürfroboter bauen ab, eine Händlerin reist mit Spezialsortiment, eine Rivalen-Crew jagt
//! Notrufe, Konvoi-Frachter brauchen Geleitschutz. Alles deterministisch in der Simulation.

use bevy::math::Vec2;

use super::autopilot::Autopilot;
use super::data::{hex, v};
use super::geom::{cross, poly_circle, rot};
use super::physics::{
    Dyn, SAFE_IMPACT, collide_two_ships, resolve_static, solve_contact, static_contacts,
};
use super::rng::hash32;
use super::ship::{Loadout, Ship, ShipStats};
use super::tools::thruster_works;
use super::world::angle_diff;
use super::{Body, BodyKind, DT, Projectile, SimEvent, SimState, ToastKind};

/// Abstand des Anflugpunkts über einer Plattform.
const APPROACH: f32 = 18.0;
/// Drohnen: Angriffsabstand, Reichweite, Feuerpause, Geschoss.
const DRONE_RANGE: f32 = 22.0;
const DRONE_AGGRO: f32 = 240.0;
const DRONE_FIRE: f32 = 1.8;
const DRONE_SHOT_SPEED: f32 = 38.0;
pub const DRONE_DAMAGE: f32 = 6.0;
/// Schaden eines Crew-Geschosses an einem NPC-Schiff.
pub const SHOT_DAMAGE_NPC: f32 = 10.0;
/// Schürfroboter: Ladung, bis er heimfliegt, und Abbau pro Sekunde.
const MINER_LOAD: f32 = 6.0;
const MINER_RATE: f32 = 0.15;
/// Tempo: frei, nahe Stationen, in Asteroidenfeldern.
const SPEED_FREE: f32 = 16.0;
const SPEED_STATION: f32 = 6.0;
const SPEED_FIELD: f32 = 5.0;
/// So lange darf ein NPC ohne Fortschritt feststecken, bevor der Bergungsdienst ihn holt.
const STUCK_SECONDS: f32 = 30.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    /// Pendelt zwischen Stationen (Stationsindizes), `leg` = nächstes Ziel.
    Trader { route: Vec<usize>, leg: usize },
    /// Händlerin mit Spezialsortiment – fliegt wie ein Frachter.
    Merchant { route: Vec<usize>, leg: usize },
    /// Piratendrohne aus einem Nest (None = Hinterhalt eines Auftrags).
    Drone { nest: Option<usize> },
    /// Schürfroboter: Feld, Heimatstation, Ladung, Ziel-Asteroid.
    Miner {
        def: usize,
        field: usize,
        home: usize,
        cargo: f32,
        target: Option<u32>,
    },
    /// Konvoi-Frachter eines Auftrags: fährt los, sobald die Crew dabei ist.
    Convoy {
        mission: u32,
        to: usize,
        departed: bool,
    },
    /// Rivalen-Crew: jagt Notrufe, die noch niemand angenommen hat.
    Rival { home: usize, goal: Option<Vec2> },
    /// Havariertes Schiff eines Rettungsauftrags: treibt antriebslos, bis die Crew längsseits
    /// geht und die Besatzung übernimmt.
    Stranded { mission: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Nav {
    Docked {
        pad: usize,
        wait: f32,
    },
    /// Im Leitstrahl einer Station: beim Start hoch über die Plattform und den Anflugweg
    /// hinaus (danach `rest` im freien Raum), bei der Ankunft den Anflugweg hinein bis über die
    /// Plattform (danach die Landung). Die Station führt, die Triebwerke ruhen.
    Guided {
        pad: usize,
        guide: Vec<Vec2>,
        idx: usize,
        rest: Vec<Vec2>,
        land: Option<usize>,
        arriving: bool,
    },
    /// Wegpunkte abfliegen, danach landen (`land`) oder am letzten Punkt bleiben.
    Path {
        points: Vec<Vec2>,
        idx: usize,
        land: Option<usize>,
    },
    Landing {
        pad: usize,
        timer: f32,
    },
    /// Zu einem Punkt fliegen und dort bleiben.
    Hold {
        target: Vec2,
    },
}

#[derive(Clone, Debug)]
pub struct Npc {
    pub id: u32,
    pub name: String,
    pub def: String,
    pub role: Role,
    pub ship: Ship,
    pub nav: Nav,
    /// Rumpf- und Akzentfarbe (sRGB).
    pub colors: ([f32; 3], [f32; 3]),
    pub hostile: bool,
    pub fire_cd: f32,
    pub alive: bool,
    /// Sekunden bis zum Wiedererscheinen nach einer Zerstörung (< 0 = nie).
    pub respawn: f32,
    /// Wen eine Drohne angreift: None = Crew, Some(id) = ein NPC (Konvoi).
    pub prey: Option<u32>,
    /// Bohrt gerade (Schürfroboter) – nur für die Anzeige.
    pub drilling: Option<Vec2>,
    /// Sekunden ohne Fortschritt (für den Bergungsdienst).
    pub stuck: f32,
}

/// Laufender Zollscan: welche Boje, wie weit (0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct CustomsScan {
    pub checkpoint: usize,
    pub progress: f32,
}

impl Npc {
    /// Konvoi schon unterwegs? (Andere Rollen gelten immer als unterwegs.)
    pub fn has_departed(&self) -> bool {
        !matches!(
            self.role,
            Role::Convoy {
                departed: false,
                ..
            }
        )
    }

    /// Kurzbeschreibung für Karte und Marker.
    pub fn label(&self) -> &'static str {
        match self.role {
            Role::Trader { .. } => "Frachter",
            Role::Merchant { .. } => "Händlerin",
            Role::Drone { .. } => "Pirat",
            Role::Miner { .. } => "Schürfroboter",
            Role::Convoy { .. } => "Konvoi",
            Role::Rival { .. } => "Rivalen",
            Role::Stranded { .. } => "Havariert",
        }
    }

    pub fn docked_pad(&self) -> Option<usize> {
        match self.nav {
            Nav::Docked { pad, .. } => Some(pad),
            _ => None,
        }
    }
    /// Plattform, die dieses Schiff belegt oder gleich anfliegt.
    fn claimed_pad(&self) -> Option<usize> {
        match &self.nav {
            Nav::Docked { pad, .. } | Nav::Landing { pad, .. } => Some(*pad),
            Nav::Path { land, .. } => *land,
            // Beim Start ist die eigene Plattform noch belegt (das Ziel ist reserviert).
            Nav::Guided {
                pad,
                arriving,
                land,
                ..
            } => {
                if *arriving {
                    Some(*pad)
                } else {
                    *land
                }
            }
            Nav::Hold { .. } => None,
        }
    }

    /// Hängt das Schiff gerade im Leitstrahl einer Station (Start oder Endanflug)?
    pub fn in_tractor(&self) -> Option<usize> {
        match self.nav {
            Nav::Guided { pad, .. } | Nav::Landing { pad, .. } => Some(pad),
            _ => None,
        }
    }
}

/// Wo NPCs an einer Station andocken: Plattform und Anflugweg.
#[derive(Clone, Debug)]
pub struct NpcDock {
    pub station: usize,
    pub pad: usize,
    pub via: Vec<Vec2>,
}

fn build_ship(data: &super::data::GameData, def: &str) -> Ship {
    let d = data.ship(def);
    Ship::build(
        d,
        &Loadout {
            thrusters: 2,
            tools: (0..d.tool_parts().count()).collect(),
        },
        &ShipStats::default(),
    )
}

impl SimState {
    /// Andockplätze und Verkehr beim Start anlegen.
    pub(crate) fn spawn_traffic(&mut self) {
        let t = self.data.traffic.clone();
        self.npc_docks = t
            .docks
            .iter()
            .filter_map(|d| {
                let si = self.data.station_index(&d.station)?;
                let near = v(d.near);
                let pad = *self.world.stations[si].pads.iter().min_by(|a, b| {
                    (self.world.pads[**a].center - near)
                        .length()
                        .total_cmp(&(self.world.pads[**b].center - near).length())
                })?;
                Some(NpcDock {
                    station: si,
                    pad,
                    via: d.via.iter().map(|p| v(*p)).collect(),
                })
            })
            .collect();
        for td in &t.traders {
            let route: Vec<usize> = td
                .route
                .iter()
                .filter_map(|id| self.data.station_index(id))
                .collect();
            if route.len() < 2 {
                continue;
            }
            let colors = (hex(&td.colors.0), hex(&td.colors.1));
            self.spawn_docked(
                &td.name,
                &td.ship,
                colors,
                Role::Trader {
                    route: route.clone(),
                    leg: 1,
                },
                route[0],
            );
        }
        if let Some(m) = &t.merchant {
            let route: Vec<usize> = m
                .route
                .iter()
                .filter_map(|id| self.data.station_index(id))
                .collect();
            if route.len() >= 2 {
                let colors = (hex(&m.colors.0), hex(&m.colors.1));
                self.spawn_docked(
                    &m.name,
                    &m.ship,
                    colors,
                    Role::Merchant {
                        route: route.clone(),
                        leg: 1,
                    },
                    route[0],
                );
            }
        }
        if let Some(r) = &t.rival
            && let Some(home) = self.data.station_index(&r.home)
        {
            let colors = (hex(&r.colors.0), hex(&r.colors.1));
            self.spawn_docked(
                &r.name,
                &r.ship,
                colors,
                Role::Rival { home, goal: None },
                home,
            );
        }
        for (mi, md) in t.miners.iter().enumerate() {
            let Some(field) = self
                .data
                .world
                .asteroid_fields
                .iter()
                .position(|f| f.name == md.field)
            else {
                continue;
            };
            let Some(home) = self.data.station_index(&md.home) else {
                continue;
            };
            let c = v(self.data.world.asteroid_fields[field].center);
            let off = Vec2::new(self.rng.range(-40.0, 40.0), self.rng.range(-40.0, 40.0));
            let id = self.next_id();
            let mut ship = build_ship(&self.data, "schuerfer");
            place(&mut ship, c + off, 0.0);
            self.npcs.push(Npc {
                id,
                name: md.name.clone(),
                def: "schuerfer".into(),
                role: Role::Miner {
                    def: mi,
                    field,
                    home,
                    cargo: 0.0,
                    target: None,
                },
                ship,
                nav: Nav::Hold { target: c + off },
                colors: ([0.54, 0.48, 0.32], [1.0, 0.7, 0.12]),
                hostile: false,
                fire_cd: 0.0,
                alive: true,
                respawn: 0.0,
                prey: None,
                drilling: None,
                stuck: 0.0,
            });
        }
    }

    /// Ein NPC-Schiff angedockt an einer Station erzeugen (falls dort Platz ist).
    pub(crate) fn spawn_docked(
        &mut self,
        name: &str,
        def: &str,
        colors: ([f32; 3], [f32; 3]),
        role: Role,
        station: usize,
    ) -> Option<u32> {
        let dock = self.free_dock(station)?;
        let id = self.next_id();
        let mut ship = build_ship(&self.data, def);
        let p = self.world.pads[dock.pad].clone();
        let rest = ship.rest_height();
        place(
            &mut ship,
            p.center + p.normal * (rest + 0.02),
            p.ship_angle(),
        );
        let wait = self.rng.range(4.0, 18.0);
        self.npcs.push(Npc {
            id,
            name: name.to_string(),
            def: def.to_string(),
            role,
            ship,
            nav: Nav::Docked {
                pad: dock.pad,
                wait,
            },
            colors,
            hostile: false,
            fire_cd: 0.0,
            alive: true,
            respawn: 0.0,
            prey: None,
            drilling: None,
            stuck: 0.0,
        });
        Some(id)
    }

    /// Konvoi-Frachter eines Geleitschutz-Auftrags: an einer freien Plattform, sonst wartet er
    /// vor der Station im freien Raum.
    pub(crate) fn spawn_convoy(&mut self, name: &str, mission: u32, from: usize, to: usize) -> u32 {
        let ship = self.data.traffic.convoy_ship.clone();
        let colors = ([0.62, 0.64, 0.6], [0.95, 0.75, 0.2]);
        let role = Role::Convoy {
            mission,
            to,
            departed: false,
        };
        if let Some(id) = self.spawn_docked(name, &ship, colors, role.clone(), from) {
            return id;
        }
        let id = self.next_id();
        let toward = self.world.stations[to].pos;
        let spot = self.waiting_spot(from, id, toward);
        let mut s = build_ship(&self.data, &ship);
        place(&mut s, spot, 0.0);
        self.npcs.push(Npc {
            id,
            name: name.to_string(),
            def: ship,
            role,
            ship: s,
            nav: Nav::Hold { target: spot },
            colors,
            hostile: false,
            fire_cd: 0.0,
            alive: true,
            respawn: 0.0,
            prey: None,
            drilling: None,
            stuck: 0.0,
        });
        id
    }

    /// Havariertes Schiff eines Rettungsauftrags am Notrufort.
    pub(crate) fn spawn_stranded(&mut self, name: &str, mission: u32, site: Vec2) -> u32 {
        let def = self.data.traffic.stranded_ship.clone();
        let id = self.next_id();
        let mut ship = build_ship(&self.data, &def);
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        place(&mut ship, site, a);
        ship.ang_vel = self.rng.range(-0.12, 0.12);
        ship.hull = ship.max_hull * 0.35;
        self.npcs.push(Npc {
            id,
            name: name.to_string(),
            def,
            role: Role::Stranded { mission },
            ship,
            nav: Nav::Hold { target: site },
            colors: ([0.5, 0.48, 0.44], [0.9, 0.3, 0.2]),
            hostile: false,
            fire_cd: 0.0,
            alive: true,
            respawn: -1.0,
            prey: None,
            drilling: None,
            stuck: 0.0,
        });
        id
    }

    /// NPC verschwindet (Auftrag erledigt oder abgebrochen). Nur markiert – entfernt wird am
    /// Ende von `update_npcs`, damit laufende Schleifen über die Liste gültig bleiben.
    pub(crate) fn remove_npc(&mut self, id: u32) {
        if let Some(n) = self.npcs.iter_mut().find(|n| n.id == id) {
            n.alive = false;
            n.respawn = -1.0;
        }
    }

    /// Ist die Crew bereit für den Geleitschutz (abgedockt und in der Nähe)?
    fn crew_escorting(&self, at: Vec2) -> bool {
        self.ship.docked.is_none() && !self.ship.destroyed && (self.ship.pos - at).length() < 200.0
    }

    /// Drohne an einem Ort erzeugen.
    pub(crate) fn spawn_drone(&mut self, at: Vec2, nest: Option<usize>, prey: Option<u32>) -> u32 {
        let id = self.next_id();
        let mut ship = build_ship(&self.data, "drohne");
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        place(&mut ship, at, a);
        self.npcs.push(Npc {
            id,
            name: "Piratendrohne".into(),
            def: "drohne".into(),
            role: Role::Drone { nest },
            ship,
            nav: Nav::Hold { target: at },
            colors: ([0.23, 0.17, 0.17], [1.0, 0.23, 0.19]),
            hostile: true,
            fire_cd: self.rng.range(0.5, DRONE_FIRE),
            alive: true,
            respawn: -1.0,
            prey,
            drilling: None,
            stuck: 0.0,
        });
        id
    }

    /// Ist diese Plattform frei (keine Crew, kein NPC darauf oder im Anflug)?
    fn pad_free(&self, pad: usize, me: Option<u32>) -> bool {
        self.ship.docked != Some(pad)
            && !self
                .npcs
                .iter()
                .any(|n| n.alive && Some(n.id) != me && n.claimed_pad() == Some(pad))
    }

    fn free_dock(&self, station: usize) -> Option<NpcDock> {
        self.npc_docks
            .iter()
            .find(|d| d.station == station && self.pad_free(d.pad, None))
            .cloned()
    }

    fn free_dock_for(&self, station: usize, me: u32) -> Option<NpcDock> {
        self.npc_docks
            .iter()
            .find(|d| d.station == station && self.pad_free(d.pad, Some(me)))
            .cloned()
    }

    /// Weg zur Station: im freien Raum bis zum Anfang ihres Anflugwegs (den Rest führt der
    /// Leitstrahl). Ohne Startplattform geht es ab `from` los.
    fn route_to(&self, dock: &NpcDock, from_pad: Option<usize>, from: Vec2) -> Nav {
        let entry = self.npc_guide(dock.pad)[0];
        match from_pad {
            Some(fp) => self.launch(fp, vec![entry], Some(dock.pad)),
            None => Nav::Path {
                points: self.open_path(from, vec![entry]),
                idx: 0,
                land: Some(dock.pad),
            },
        }
    }

    /// Wartepunkt vor einer Station (alle Plätze belegt): auf der Seite, von der das Schiff
    /// kommt, mit Abstand zu Rotoren, je Schiff etwas versetzt.
    fn waiting_spot(&self, station: usize, id: u32, from: Vec2) -> Vec2 {
        let st = &self.world.stations[station];
        let r = (st.bounds.max - st.bounds.min).length() * 0.5 + 70.0;
        let base = (from - st.pos).normalize_or(Vec2::Y);
        let mut spot = st.pos + base * r;
        for k in 1..8 {
            let clear = self
                .world
                .spinners
                .iter()
                .all(|sp| (sp.pos - spot).length() > sp.reach() + 45.0);
            if clear {
                break;
            }
            let a = 0.6 * k as f32 * if k % 2 == 0 { 1.0 } else { -1.0 };
            spot = st.pos + rot(base, a) * r;
        }
        let side = Vec2::new(-base.y, base.x);
        spot + side * ((id % 5) as f32 - 2.0) * 14.0
    }

    /// Hindernisse für Strecken im freien Raum: Stationen und Rotoren.
    fn obstacles(&self) -> Vec<super::geom::Aabb> {
        let mut v: Vec<_> = self
            .world
            .stations
            .iter()
            .map(|st| st.bounds.expand(14.0))
            .collect();
        v.extend(
            self.world
                .spinners
                .iter()
                .map(|sp| super::geom::Aabb::around(sp.pos, sp.reach() + 30.0)),
        );
        v.extend(self.world.monuments.iter().map(|m| m.bounds.expand(14.0)));
        v
    }

    /// Strecke im freien Raum: Umwege um Stationen und Rotoren, die im Weg liegen.
    fn open_path(&self, from: Vec2, points: Vec<Vec2>) -> Vec<Vec2> {
        let obstacles = self.obstacles();
        let mut out = Vec::new();
        let mut at = from;
        for p in points {
            for _ in 0..6 {
                // Das erste Hindernis entlang der Strecke.
                let blocked = |a: Vec2, b: Vec2, o: &super::geom::Aabb| {
                    !o.contains(a) && !o.contains(b) && o.hits_segment(a, b)
                };
                let Some(b) = obstacles
                    .iter()
                    .filter(|o| blocked(at, p, o))
                    .min_by(|x, y| {
                        let tx = x.segment_entry(at, p).unwrap_or(1.0);
                        let ty = y.segment_entry(at, p).unwrap_or(1.0);
                        tx.total_cmp(&ty)
                    })
                else {
                    break;
                };
                // Die Ecke, über die der Umweg am kürzesten ist – bevorzugt eine, die frei
                // anfliegbar ist.
                let free = |c: Vec2| !obstacles.iter().any(|o| blocked(at, c, o));
                let corners: Vec<Vec2> = b
                    .expand(25.0)
                    .corners()
                    .into_iter()
                    .filter(|c| (*c - at).length() > 1.0 && !blocked(at, *c, b))
                    .collect();
                let cost = |c: &Vec2| (*c - at).length() + (p - *c).length();
                let best = corners
                    .iter()
                    .filter(|c| free(**c))
                    .min_by(|x, y| cost(x).total_cmp(&cost(y)))
                    .or_else(|| corners.iter().min_by(|x, y| cost(x).total_cmp(&cost(y))));
                let Some(&c) = best else {
                    break;
                };
                out.push(c);
                at = c;
            }
            out.push(p);
            at = p;
        }
        out
    }

    /// Anflugweg einer Plattform von außen nach innen, bis über die Plattform.
    fn npc_guide(&self, pad: usize) -> Vec<Vec2> {
        let mut g: Vec<Vec2> = self
            .npc_docks
            .iter()
            .find(|d| d.pad == pad)
            .map(|d| d.via.clone())
            .unwrap_or_default();
        let p = &self.world.pads[pad];
        g.push(p.center + p.normal * APPROACH);
        g
    }

    /// Start von einer Plattform: der Leitstrahl hebt das Schiff hoch und führt es den
    /// Anflugweg rückwärts hinaus, dann geht es im freien Raum über `to` weiter.
    fn launch(&self, pad: usize, to: Vec<Vec2>, land: Option<usize>) -> Nav {
        let mut guide = self.npc_guide(pad);
        guide.reverse();
        Nav::Guided {
            pad,
            guide,
            idx: 0,
            rest: to,
            land,
            arriving: false,
        }
    }

    /// Erlaubtes Tempo an einem Ort: langsam nahe Stationen und in Asteroidenfeldern.
    fn speed_limit(&self, pos: Vec2, vel: Vec2) -> f32 {
        // Vorausschauend: rechtzeitig vor der Station bzw. dem Feld bremsen.
        let ahead = pos + vel.normalize_or_zero() * 70.0;
        let near_station = self.world.stations.iter().any(|st| {
            st.bounds.expand(60.0).contains(pos) || st.bounds.expand(30.0).contains(ahead)
        });
        let in_field = self.data.world.asteroid_fields.iter().any(|f| {
            (v(f.center) - pos).length() < f.radius + 25.0
                || (v(f.center) - ahead).length() < f.radius
        });
        if near_station {
            SPEED_STATION
        } else if in_field {
            SPEED_FIELD
        } else {
            SPEED_FREE
        }
    }

    /// Ein Simulationsschritt für alle NPC-Schiffe (nach dem Crew-Schiff).
    pub(crate) fn update_npcs(&mut self) {
        self.spawn_nest_drones();
        self.update_escorts();
        self.update_customs();
        for i in 0..self.npcs.len() {
            if !self.npcs[i].alive {
                self.tick_respawn(i);
                continue;
            }
            if self.npcs[i].hostile && self.drone_gives_up(i) {
                self.npcs[i].alive = false;
                continue;
            }
            let mut npc = self.npcs[i].clone();
            npc.ship.prev_pos = npc.ship.pos;
            npc.ship.prev_angle = npc.ship.angle;
            let ap = Autopilot {
                max_speed: if npc.hostile {
                    SPEED_FREE * 1.4
                } else {
                    self.speed_limit(npc.ship.pos, npc.ship.vel)
                },
            };
            let slots = self.think(&mut npc, &ap);
            self.fly(&mut npc, slots);
            self.check_stuck(&mut npc);
            self.npcs[i] = npc;
        }
        // Zerstörte Hinterhalt-Drohnen und Konvois verschwinden ganz.
        self.npcs.retain(|n| n.alive || n.respawn >= 0.0);
    }

    /// Entscheiden, was das NPC tut. Ergebnis: Slot-Maske für die Triebwerke.
    fn think(&mut self, npc: &mut Npc, ap: &Autopilot) -> u32 {
        npc.drilling = None;
        if npc.hostile {
            return self.think_drone(npc, ap);
        }
        if let Role::Stranded { .. } = npc.role {
            // Kein Antrieb mehr – es treibt und dreht sich langsam.
            return 0;
        }
        if let Role::Miner { .. } = npc.role
            && matches!(npc.nav, Nav::Hold { .. })
        {
            return self.think_miner(npc, ap);
        }
        if let Role::Rival { .. } = npc.role
            && let Nav::Hold { target } = npc.nav
        {
            self.rival_at_goal(npc, target);
        }
        match npc.nav.clone() {
            Nav::Docked { pad, wait } => {
                // Wie bei der Crew: sanft auf die Plattform ziehen und festhalten.
                let p = self.world.pads[pad].clone();
                let rest = npc.ship.rest_height();
                let target = p.center + p.normal * (rest + 0.02);
                let s = &mut npc.ship;
                s.angle += angle_diff(p.ship_angle(), s.angle) * 0.2;
                s.pos = s.pos.lerp(target, 0.2);
                s.vel = Vec2::ZERO;
                s.ang_vel = 0.0;
                let wait = wait - DT;
                npc.nav = Nav::Docked { pad, wait };
                if wait <= 0.0 {
                    self.depart(npc, pad);
                }
                0
            }
            Nav::Guided {
                pad,
                guide,
                idx,
                rest,
                land,
                arriving,
            } => {
                let last = idx + 1 >= guide.len();
                let target = guide.get(idx).copied().unwrap_or(npc.ship.pos);
                let d = (npc.ship.pos - target).length();
                let reached = if last && arriving {
                    d < 2.5 && npc.ship.vel.length() < 2.0
                } else {
                    d < 4.0
                };
                if !reached {
                    return 0;
                }
                npc.nav = if !last {
                    Nav::Guided {
                        pad,
                        guide,
                        idx: idx + 1,
                        rest,
                        land,
                        arriving,
                    }
                } else if arriving {
                    Nav::Landing { pad, timer: 0.0 }
                } else {
                    Nav::Path {
                        points: self.open_path(npc.ship.pos, rest),
                        idx: 0,
                        land,
                    }
                };
                0
            }
            Nav::Path { points, idx, land } => {
                let Some(&target) = points.get(idx) else {
                    npc.nav = match land {
                        Some(pad) => Nav::Guided {
                            pad,
                            guide: self.npc_guide(pad),
                            idx: 0,
                            rest: Vec::new(),
                            land,
                            arriving: true,
                        },
                        None => Nav::Hold {
                            target: npc.ship.pos,
                        },
                    };
                    return 0;
                };
                let last = idx + 1 == points.len();
                let (tol, vlim) = if last && land.is_none() {
                    (12.0, 6.0)
                } else {
                    (10.0, 8.0)
                };
                let d = (npc.ship.pos - target).length();
                if d < tol && npc.ship.vel.length() < vlim {
                    npc.nav = Nav::Path {
                        points,
                        idx: idx + 1,
                        land,
                    };
                }
                ap.steer_ship(&npc.ship, target, Vec2::Y, false)
            }
            Nav::Landing { pad, timer } => {
                // Plattform inzwischen belegt? Dann eine andere suchen.
                if !self.pad_free(pad, Some(npc.id)) {
                    npc.nav = Nav::Hold {
                        target: npc.ship.pos,
                    };
                    return 0;
                }
                // Den Endanflug übernimmt der Leitstrahl (siehe `fly`), die Triebwerke ruhen.
                let p = self.world.pads[pad].clone();
                let rest = npc.ship.rest_height();
                let slots = 0;
                let rel = npc.ship.pos - p.center;
                let along = rel.dot(p.normal);
                let lateral = rel.dot(p.tangent());
                let ok = lateral.abs() < p.half_width
                    && along > rest - 0.6
                    && along < rest + 0.4
                    && npc.ship.vel.length() < 1.2
                    && angle_diff(npc.ship.angle, p.ship_angle())
                        .abs()
                        .to_degrees()
                        < super::dock::DOCK_MAX_ANGLE_DEG
                    && npc.ship.ang_vel.abs() < super::dock::DOCK_MAX_SPIN;
                let timer = if ok && slots == 0 { timer + DT } else { 0.0 };
                if timer > 0.35 {
                    npc.ship.vel = Vec2::ZERO;
                    npc.ship.ang_vel = 0.0;
                    let wait = self.rng.range(8.0, 22.0);
                    npc.nav = Nav::Docked { pad, wait };
                    self.arrived(npc, pad);
                } else {
                    npc.nav = Nav::Landing { pad, timer };
                }
                slots
            }
            Nav::Hold { target }
                if matches!(
                    npc.role,
                    Role::Convoy {
                        departed: false,
                        ..
                    }
                ) =>
            {
                // Konvoi wartet vor der Station auf die Crew.
                if self.crew_escorting(npc.ship.pos)
                    && let Role::Convoy { mission, to, .. } = npc.role
                {
                    if !self.active.iter().any(|m| m.id == mission) {
                        npc.alive = false;
                        npc.respawn = -1.0;
                        return 0;
                    }
                    npc.role = Role::Convoy {
                        mission,
                        to,
                        departed: true,
                    };
                    npc.nav = match self.free_dock_for(to, npc.id) {
                        Some(dock) => self.route_to(&dock, None, npc.ship.pos),
                        None => Nav::Path {
                            points: self.open_path(
                                npc.ship.pos,
                                vec![self.waiting_spot(to, npc.id, npc.ship.pos)],
                            ),
                            idx: 0,
                            land: None,
                        },
                    };
                    self.toast(
                        format!("{} legt ab – bleibt in der Nähe", npc.name),
                        ToastKind::Info,
                    );
                }
                ap.steer_ship(&npc.ship, target, Vec2::Y, false)
            }
            Nav::Hold { target } => {
                // Ohne Ziel (Platz besetzt, Notruf weg): wieder nach Hause bzw. weiter.
                if let Some(next) = self.next_station(npc)
                    && (npc.ship.pos - target).length() < 12.0
                    && let Some(dock) = self.free_dock_for(next, npc.id)
                {
                    npc.nav = self.route_to(&dock, None, npc.ship.pos);
                }
                ap.steer_ship(&npc.ship, target, Vec2::Y, false)
            }
        }
    }

    /// Wohin will ein Pendler als Nächstes?
    fn next_station(&self, npc: &Npc) -> Option<usize> {
        match &npc.role {
            Role::Trader { route, leg } | Role::Merchant { route, leg } => {
                route.get(*leg % route.len()).copied()
            }
            Role::Convoy { to, departed, .. } if *departed => Some(*to),
            Role::Rival { home, goal: None } => Some(*home),
            Role::Miner { home, cargo, .. } if *cargo >= MINER_LOAD => Some(*home),
            _ => None,
        }
    }

    /// Abflug von einer Plattform.
    fn depart(&mut self, npc: &mut Npc, pad: usize) {
        let p = self.world.pads[pad].clone();
        match npc.role.clone() {
            // Auftrag vorbei (erfüllt, abgebrochen) oder am Ziel: der Frachter fliegt in den Hangar.
            Role::Convoy {
                mission, departed, ..
            } if departed || !self.active.iter().any(|m| m.id == mission) => {
                npc.alive = false;
                npc.respawn = -1.0;
                return;
            }
            Role::Convoy { .. } => {
                // Konvoi wartet, bis die Crew abgedockt und in der Nähe ist.
                if !self.crew_escorting(npc.ship.pos) {
                    npc.nav = Nav::Docked { pad, wait: 0.5 };
                    return;
                }
                let Role::Convoy { to, .. } = npc.role else {
                    return;
                };
                let Some(dock) = self.free_dock_for(to, npc.id) else {
                    npc.nav = Nav::Docked { pad, wait: 2.0 };
                    return;
                };
                if let Role::Convoy { departed, .. } = &mut npc.role {
                    *departed = true;
                }
                npc.nav = self.route_to(&dock, Some(pad), npc.ship.pos);
                self.toast(
                    format!("{} legt ab – bleibt in der Nähe", npc.name),
                    ToastKind::Info,
                );
                return;
            }
            Role::Rival { home, goal: None } => {
                // Notruf in Reichweite, den noch niemand angenommen hat?
                let site = self
                    .offers
                    .iter()
                    .filter_map(|m| match &m.kind {
                        super::missions::MissionKind::Tow { site, .. }
                        | super::missions::MissionKind::Capsules { site, .. }
                            if m.is_distress() =>
                        {
                            Some(*site)
                        }
                        _ => None,
                    })
                    .min_by(|a, b| {
                        (*a - p.center)
                            .length()
                            .total_cmp(&(*b - p.center).length())
                    });
                let Some(site) = site else {
                    npc.nav = Nav::Docked { pad, wait: 10.0 };
                    return;
                };
                npc.role = Role::Rival {
                    home,
                    goal: Some(site),
                };
                npc.nav = self.launch(pad, vec![site], None);
                return;
            }
            Role::Miner {
                def,
                field,
                home,
                target,
                ..
            } => {
                // Zurück ins Feld.
                let c = v(self.data.world.asteroid_fields[field].center);
                npc.role = Role::Miner {
                    def,
                    field,
                    home,
                    cargo: 0.0,
                    target,
                };
                npc.nav = self.launch(pad, vec![c], None);
                return;
            }
            _ => {}
        }
        let Some(next) = self.next_station(npc) else {
            npc.nav = Nav::Docked { pad, wait: 5.0 };
            return;
        };
        // Kein Platz frei: trotzdem los und vor der Station warten (sonst blockieren sich
        // zwei Schiffe, die jeweils auf den Platz des anderen warten).
        npc.nav = match self.free_dock_for(next, npc.id) {
            Some(dock) => self.route_to(&dock, Some(pad), npc.ship.pos),
            None => {
                let spot = self.waiting_spot(next, npc.id, npc.ship.pos);
                self.launch(pad, vec![spot], None)
            }
        };
    }

    /// Angekommen und angedockt.
    fn arrived(&mut self, npc: &mut Npc, pad: usize) {
        let owner = self.world.pads[pad].owner;
        match &mut npc.role {
            Role::Trader { route, leg } | Role::Merchant { route, leg } => {
                *leg = (*leg + 1) % route.len();
            }
            Role::Miner { cargo, .. } => {
                *cargo = 0.0;
            }
            Role::Convoy { mission, .. } => {
                let mid = *mission;
                self.convoy_arrived(mid);
            }
            Role::Rival { goal, .. } => {
                *goal = None;
            }
            Role::Drone { .. } | Role::Stranded { .. } => {}
        }
        if matches!(npc.role, Role::Merchant { .. })
            && self.ship.docked.map(|p| self.world.pads[p].owner) == Some(owner)
        {
            let name = self.world.owner_name(owner).to_string();
            self.toast(
                format!(
                    "{} hat in {name} angedockt – Sonderangebote im Markt",
                    npc.name
                ),
                ToastKind::Info,
            );
        }
    }

    /// Rivalen am Notruf: übernehmen, was noch niemand angenommen hat.
    fn rival_at_goal(&mut self, npc: &mut Npc, at: Vec2) {
        let Role::Rival {
            home,
            goal: Some(site),
        } = npc.role
        else {
            return;
        };
        if (npc.ship.pos - site).length() > 30.0 || (at - npc.ship.pos).length() > 30.0 {
            return;
        }
        let taken = self.offers.iter().position(|m| {
            m.is_distress()
                && matches!(&m.kind,
                    super::missions::MissionKind::Tow { site: s, .. }
                    | super::missions::MissionKind::Capsules { site: s, .. } if (*s - site).length() < 1.0)
        });
        if let Some(idx) = taken {
            self.offers.remove(idx);
            self.rival_score += 1;
            self.toast(
                format!("{} war schneller und hat den Notruf übernommen", npc.name),
                ToastKind::Warn,
            );
            self.events.push(SimEvent::RivalTook);
        } else {
            self.toast(
                format!("{} dreht ab – der Notruf ist schon vergeben", npc.name),
                ToastKind::Info,
            );
        }
        npc.role = Role::Rival { home, goal: None };
        if let Some(dock) = self.free_dock_for(home, npc.id) {
            npc.nav = self.route_to(&dock, None, npc.ship.pos);
        }
    }

    fn think_miner(&mut self, npc: &mut Npc, ap: &Autopilot) -> u32 {
        let Role::Miner {
            def,
            field,
            home,
            cargo,
            target,
        } = npc.role
        else {
            return 0;
        };
        // Voll: heimfliegen.
        if cargo >= MINER_LOAD {
            if let Some(dock) = self.free_dock_for(home, npc.id) {
                npc.nav = self.route_to(&dock, None, npc.ship.pos);
            }
            return 0;
        }
        let rock = target
            .and_then(|id| self.bodies.iter().position(|b| b.id == id && b.alive))
            .filter(|&bi| {
                matches!(self.bodies[bi].kind, BodyKind::Asteroid { ore: Some(_), ore_left, .. } if ore_left > 0.3)
            });
        let bi = match rock {
            Some(bi) => bi,
            None => {
                // Nächster erzhaltiger Asteroid im eigenen Feld.
                let best = self
                    .bodies
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| {
                        b.alive
                            && matches!(b.kind, BodyKind::Asteroid { ore: Some(_), ore_left, field: f, .. } if ore_left > 0.5 && f == field)
                    })
                    .min_by(|a, b| {
                        (a.1.pos - npc.ship.pos)
                            .length()
                            .total_cmp(&(b.1.pos - npc.ship.pos).length())
                    })
                    .map(|(i, _)| i);
                let Some(i) = best else {
                    let c = v(self.data.world.asteroid_fields[field].center);
                    npc.nav = Nav::Hold { target: c };
                    return ap.steer_ship(&npc.ship, c, Vec2::Y, false);
                };
                npc.role = Role::Miner {
                    def,
                    field,
                    home,
                    cargo,
                    target: Some(self.bodies[i].id),
                };
                i
            }
        };
        let b = &self.bodies[bi];
        let away = (npc.ship.pos - b.pos).normalize_or(Vec2::Y);
        let spot = b.pos + away * (b.radius + 3.5);
        npc.nav = Nav::Hold { target: spot };
        let close = (npc.ship.pos - spot).length() < 5.0 && npc.ship.vel.length() < 2.0;
        if close {
            let amount = (MINER_RATE * DT).min(match b.kind {
                BodyKind::Asteroid { ore_left, .. } => ore_left,
                _ => 0.0,
            });
            let bpos = b.pos;
            if let BodyKind::Asteroid { ore_left, .. } = &mut self.bodies[bi].kind {
                *ore_left -= amount;
            }
            npc.role = Role::Miner {
                def,
                field,
                home,
                cargo: cargo + amount,
                target: Some(self.bodies[bi].id),
            };
            npc.drilling = Some(bpos);
        }
        ap.steer_ship(&npc.ship, spot, Vec2::Y, false)
    }

    fn think_drone(&mut self, npc: &mut Npc, ap: &Autopilot) -> u32 {
        // Beute: ein angegriffener Konvoi, sonst die Crew in Reichweite.
        let prey_pos = match npc.prey {
            Some(id) => self
                .npcs
                .iter()
                .find(|n| n.id == id && n.alive)
                .map(|n| (n.ship.pos, n.ship.vel)),
            None => {
                let d = (self.ship.pos - npc.ship.pos).length();
                // Ein Resonanzkern an Bord lockt die Drohnen von weiter her an.
                let aggro = if self.artifact_lure() {
                    DRONE_AGGRO * 1.6
                } else {
                    DRONE_AGGRO
                };
                (d < aggro && self.ship.docked.is_none() && !self.ship.destroyed)
                    .then_some((self.ship.pos, self.ship.vel))
            }
        };
        if npc.prey.is_some() && prey_pos.is_none() {
            npc.prey = None;
        }
        let Some((pp, pv)) = prey_pos else {
            // Patrouille am Nest.
            let home = match npc.role {
                Role::Drone { nest: Some(n) } => v(self.data.traffic.nests[n].center),
                _ => npc.ship.pos,
            };
            let a = (self.time * 0.15 + (npc.id % 7) as f32).rem_euclid(std::f32::consts::TAU);
            let spot = home + Vec2::new(a.cos(), a.sin()) * 60.0;
            npc.nav = Nav::Hold { target: spot };
            return ap.steer_ship(&npc.ship, spot, Vec2::Y, false);
        };
        // Im Abstand bleiben, leicht seitlich versetzt.
        let side = if npc.id.is_multiple_of(2) { 1.0 } else { -1.0 };
        let away = (npc.ship.pos - pp).normalize_or(Vec2::X);
        let spot = pp + rot(away, 0.35 * side) * DRONE_RANGE;
        npc.nav = Nav::Hold { target: spot };
        npc.fire_cd -= DT;
        let dist = (pp - npc.ship.pos).length();
        if npc.fire_cd <= 0.0 && dist < DRONE_RANGE * 1.8 {
            npc.fire_cd = DRONE_FIRE;
            let lead = pp + pv * (dist / DRONE_SHOT_SPEED);
            let dir = (lead - npc.ship.pos).normalize_or(Vec2::X);
            let spread = self.rng.range(-0.06, 0.06);
            let dir = rot(dir, spread);
            let id = self.next_id();
            let pos = npc.ship.pos + dir * (npc.ship.bound_radius() + 0.3);
            self.projectiles.push(Projectile {
                id,
                pos,
                prev_pos: pos,
                vel: npc.ship.vel + dir * DRONE_SHOT_SPEED,
                life: 1.6,
                hostile: true,
            });
            self.events.push(SimEvent::Shot { pos, dir });
        }
        ap.steer_ship(&npc.ship, spot, Vec2::Y, false)
    }

    /// Triebwerke, Kräfte, Bewegung und Kollisionen eines NPC-Schiffs – wie beim Crew-Schiff.
    fn fly(&mut self, npc: &mut Npc, slots: u32) {
        let docked = matches!(npc.nav, Nav::Docked { .. });
        let tick = self.tick;
        let beam = match &npc.nav {
            Nav::Guided {
                pad, guide, idx, ..
            } => guide
                .get(*idx)
                .map(|t| (*t, self.world.pads[*pad].ship_angle(), 6.0)),
            Nav::Landing { pad, .. } => {
                let p = &self.world.pads[*pad];
                Some((
                    p.center + p.normal * (npc.ship.rest_height() - 0.3),
                    p.ship_angle(),
                    2.5,
                ))
            }
            _ => None,
        };
        let ship = &mut npc.ship;
        let mut force = Vec2::ZERO;
        let mut torque = 0.0;
        for t in &mut ship.thrusters {
            t.firing = !docked && slots >> t.slot & 1 == 1 && thruster_works(t, tick);
            let target = if t.firing { 1.0 } else { 0.0 };
            t.level += (target - t.level) * (DT * 16.0).min(1.0);
        }
        if docked {
            // Angedockt steht das Schiff fest – die Crew prallt trotzdem daran ab.
            if self.ship.docked.is_none()
                && !self.ship.destroyed
                && let Some((imp, at)) = collide_two_ships(&mut self.ship, &mut npc.ship)
                && imp > 1.5
            {
                self.events.push(SimEvent::Impact {
                    pos: at,
                    normal: Vec2::Y,
                    strength: imp,
                });
                self.count_collision(at);
            }
            return;
        }
        if let Some((target, angle, max_speed)) = beam {
            tractor(ship, target, angle, max_speed);
        }
        for t in &ship.thrusters {
            if !t.firing {
                continue;
            }
            let f = rot(t.dir, ship.angle) * t.effective_thrust();
            let at = ship.to_world(t.pos);
            force += f;
            torque += cross(at - ship.pos, f);
        }
        ship.vel += (force / ship.mass + self.world.gravity(ship.pos)) * DT;
        ship.ang_vel += torque / ship.inertia * DT;
        ship.ang_vel *= 1.0 - (ship.ang_damp * DT).min(0.5);
        let d = ship.pos.length();
        if d > self.world.radius {
            ship.vel -= ship.pos / d * 8.0 * DT;
        }
        ship.pos += ship.vel * DT;
        ship.angle += ship.ang_vel * DT;
        // Feste Welt.
        let contacts = static_contacts(ship, &self.world);
        if !contacts.is_empty() {
            let (imp, at) = resolve_static(ship, &contacts);
            if imp > 1.5 {
                self.events.push(SimEvent::Impact {
                    pos: at.0,
                    normal: at.1,
                    strength: imp,
                });
            }
            if imp > SAFE_IMPACT {
                npc.ship.hull -= (imp - SAFE_IMPACT) * 3.6;
            }
        }
        // Asteroiden und andere Körper.
        let quads = npc.ship.quads();
        let reach = npc.ship.bound_radius();
        for bi in 0..self.bodies.len() {
            let b = &self.bodies[bi];
            if !b.alive || (b.pos - npc.ship.pos).length() > reach + b.radius + 0.5 {
                continue;
            }
            for (cp, cr) in b.circles().iter() {
                for q in &quads {
                    let Some(c) = poly_circle(q, cp, cr) else {
                        continue;
                    };
                    let mut sd = dyn_of(&npc.ship);
                    let b = &self.bodies[bi];
                    let mut bd = Dyn {
                        pos: b.pos,
                        vel: b.vel,
                        w: b.ang_vel,
                        inv_m: 1.0 / b.mass,
                        inv_i: 1.0 / b.inertia(),
                    };
                    let imp = solve_contact(&mut sd, &mut bd, &c, 0.3, 0.4, Vec2::ZERO, 1.0);
                    npc.ship.pos = sd.pos;
                    npc.ship.vel = sd.vel;
                    npc.ship.ang_vel = sd.w;
                    let b = &mut self.bodies[bi];
                    b.pos = bd.pos;
                    b.vel = bd.vel;
                    b.ang_vel = bd.w;
                    if imp > SAFE_IMPACT {
                        let factor = (b.mass / npc.ship.mass * 1.5).clamp(0.15, 1.0);
                        npc.ship.hull -= (imp - SAFE_IMPACT) * 3.6 * factor;
                    }
                    if matches!(self.bodies[bi].kind, BodyKind::Meteor) {
                        self.bodies[bi].alive = false;
                        npc.ship.hull -= 12.0;
                    }
                }
            }
        }
        // Crew-Schiff: Stöße zählen für beide.
        if self.ship.docked.is_none()
            && !self.ship.destroyed
            && let Some((imp, at)) = collide_two_ships(&mut self.ship, &mut npc.ship)
        {
            if imp > 1.5 {
                self.events.push(SimEvent::Impact {
                    pos: at,
                    normal: Vec2::Y,
                    strength: imp,
                });
                self.count_collision(at);
            }
            if imp > SAFE_IMPACT {
                let dmg = (imp - SAFE_IMPACT) * 3.6;
                self.ship_damage(
                    dmg * (npc.ship.mass / self.ship.mass).clamp(0.2, 1.0),
                    at,
                    true,
                );
                npc.ship.hull -= dmg;
            }
        }
        if npc.ship.hull <= 0.0 {
            self.destroy_npc(npc);
        }
    }

    /// Festgefahren (verkeilt, abgedrängt)? Nach einer Weile holt der Bergungsdienst das Schiff
    /// zur nächsten Station – aber nie vor den Augen der Crew.
    fn check_stuck(&mut self, npc: &mut Npc) {
        let moving = npc.ship.vel.length() > 1.0;
        let travelling = matches!(
            npc.nav,
            Nav::Path { .. } | Nav::Guided { .. } | Nav::Landing { .. }
        );
        if !npc.alive || npc.hostile || !travelling {
            npc.stuck = 0.0;
            return;
        }
        if moving {
            npc.stuck = (npc.stuck - DT * 0.5).max(0.0);
            return;
        }
        npc.stuck += DT;
        if npc.stuck < STUCK_SECONDS || (self.ship.pos - npc.ship.pos).length() < 300.0 {
            return;
        }
        npc.stuck = 0.0;
        let station = match &npc.role {
            Role::Trader { route, leg } | Role::Merchant { route, leg } => {
                route[*leg % route.len()]
            }
            Role::Convoy { to, .. } => *to,
            Role::Rival { home, .. } | Role::Miner { home, .. } => *home,
            Role::Drone { .. } | Role::Stranded { .. } => return,
        };
        match self.free_dock_for(station, npc.id) {
            Some(dock) => {
                let p = self.world.pads[dock.pad].clone();
                let rest = npc.ship.rest_height();
                place(
                    &mut npc.ship,
                    p.center + p.normal * (rest + 0.02),
                    p.ship_angle(),
                );
                npc.nav = Nav::Docked {
                    pad: dock.pad,
                    wait: 10.0,
                };
                self.arrived(npc, dock.pad);
            }
            None => {
                // Alles belegt: vor der Station im freien Raum absetzen.
                let spot = self.waiting_spot(station, npc.id, npc.ship.pos);
                place(&mut npc.ship, spot, 0.0);
                npc.nav = Nav::Hold { target: spot };
            }
        }
    }

    /// Ein NPC-Schiff ist zerstört: Explosion, Trümmer, später neu.
    pub(crate) fn destroy_npc(&mut self, npc: &mut Npc) {
        npc.alive = false;
        let pos = npc.ship.pos;
        self.events.push(SimEvent::Explosion {
            pos,
            size: npc.ship.bound_radius() * 2.5,
            color: [1.0, 0.5, 0.2],
        });
        // Bauteil zum Bergen, Schürfer verlieren ihre Ladung als Erzbrocken.
        let names = self.data.shop.salvage_names.clone();
        let name = names
            .get(hash32(npc.id) as usize % names.len().max(1))
            .cloned()
            .unwrap_or_else(|| "Bauteil".into());
        let (lo, hi) = self.data.shop.salvage_value;
        let value = lo + hash32(npc.id ^ 0x55) % (hi - lo + 1).max(1);
        self.spawn_loose(
            pos,
            npc.ship.vel,
            BodyKind::Salvage { name, value },
            0.6,
            0.7,
        );
        if let Role::Miner { field, cargo, .. } = npc.role {
            let ore = self.data.world.asteroid_fields[field].ore;
            let mut left = cargo;
            while left > 0.2 {
                let amount = left.min(1.5);
                left -= amount;
                self.spawn_loose(
                    pos,
                    npc.ship.vel,
                    BodyKind::OreChunk { ore, amount },
                    0.5,
                    amount.max(0.3),
                );
            }
        }
        npc.respawn = match npc.role {
            Role::Drone { .. } | Role::Convoy { .. } | Role::Stranded { .. } => -1.0,
            _ => 90.0,
        };
        if let Role::Convoy { mission, .. } = npc.role {
            self.fail_mission(mission, "Konvoi zerstört");
        }
        if let Role::Stranded { mission } = npc.role {
            self.fail_mission(mission, "das havarierte Schiff ist zerstört");
        }
        if npc.hostile {
            self.events.push(SimEvent::DroneDown { pos });
        }
    }

    fn spawn_loose(&mut self, pos: Vec2, vel: Vec2, kind: BodyKind, radius: f32, mass: f32) {
        let id = self.next_id();
        let jitter = Vec2::new(self.rng.range(-2.0, 2.0), self.rng.range(-2.0, 2.0));
        self.bodies.push(Body {
            id,
            kind,
            pos: pos + jitter * 0.3,
            vel: vel * 0.5 + jitter,
            angle: 0.0,
            ang_vel: self.rng.range(-1.0, 1.0),
            radius,
            mass,
            prev_pos: pos,
            prev_angle: 0.0,
            alive: true,
            seed: hash32(id),
            age: 0.0,
        });
    }

    /// Zerstörte Pendler kommen nach einer Weile an ihrer Station zurück.
    fn tick_respawn(&mut self, i: usize) {
        if self.npcs[i].respawn < 0.0 {
            return;
        }
        self.npcs[i].respawn -= DT;
        if self.npcs[i].respawn > 0.0 {
            return;
        }
        let npc = self.npcs[i].clone();
        let station = match &npc.role {
            Role::Trader { route, .. } | Role::Merchant { route, .. } => route[0],
            Role::Rival { home, .. } | Role::Miner { home, .. } => *home,
            _ => return,
        };
        let Some(dock) = self.free_dock(station) else {
            self.npcs[i].respawn = 10.0;
            return;
        };
        let mut ship = build_ship(&self.data, &npc.def);
        let p = self.world.pads[dock.pad].clone();
        let rest = ship.rest_height();
        place(
            &mut ship,
            p.center + p.normal * (rest + 0.02),
            p.ship_angle(),
        );
        let n = &mut self.npcs[i];
        n.ship = ship;
        n.alive = true;
        n.nav = Nav::Docked {
            pad: dock.pad,
            wait: 10.0,
        };
        if let Role::Miner { cargo, target, .. } = &mut n.role {
            *cargo = 0.0;
            *target = None;
        }
        if let Role::Rival { goal, .. } = &mut n.role {
            *goal = None;
        }
    }

    /// Piratennester: Drohnen auffüllen, solange die Crew in der Nähe ist.
    fn spawn_nest_drones(&mut self) {
        if !self.tick.is_multiple_of(30) || self.ship.destroyed {
            return;
        }
        let nests = self.data.traffic.nests.clone();
        let lure = self.artifact_lure();
        for (ni, n) in nests.iter().enumerate() {
            let c = v(n.center);
            let reach = n.radius + if lure { 1200.0 } else { 500.0 };
            if (self.ship.pos - c).length() > reach {
                continue;
            }
            let alive = self
                .npcs
                .iter()
                .filter(|x| x.alive && x.role == Role::Drone { nest: Some(ni) })
                .count() as u32;
            let count = n.count + u32::from(lure);
            if alive >= count || self.nest_cooldown.get(ni).copied().unwrap_or(0.0) > self.time {
                continue;
            }
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let at = c + Vec2::new(a.cos(), a.sin()) * self.rng.range(0.0, n.radius * 0.6);
            self.spawn_drone(at, Some(ni), None);
            if let Some(cd) = self.nest_cooldown.get_mut(ni) {
                *cd = self.time + 25.0;
            }
        }
    }

    /// Schaden an einem NPC-Schiff (Geschoss).
    pub(crate) fn damage_npc(&mut self, i: usize, amount: f32) {
        let mut npc = self.npcs[i].clone();
        if !npc.alive {
            return;
        }
        npc.ship.hull -= amount;
        // Wer auf Frachter schießt, macht sich Feinde nicht – aber Drohnen merken es.
        if npc.ship.hull <= 0.0 {
            self.destroy_npc(&mut npc);
        }
        self.npcs[i] = npc;
    }
}

impl SimState {
    /// Ziele für Geleitschutz ab einer Station: andere Stationen mit NPC-Andockplatz.
    pub(crate) fn escort_targets(&self, from: usize) -> Vec<usize> {
        if !self.npc_docks.iter().any(|d| d.station == from) {
            return Vec::new();
        }
        let mut v: Vec<usize> = self
            .npc_docks
            .iter()
            .map(|d| d.station)
            .filter(|&s| s != from)
            .collect();
        v.dedup();
        v
    }

    /// Ziel für Schmuggelware: eine andere Station mit Markt, bevorzugt eine mit Zollboje.
    pub(crate) fn smuggle_target(&mut self, from: usize) -> usize {
        let guarded: Vec<usize> = (0..self.world.stations.len())
            .filter(|&i| {
                i != from
                    && self
                        .data
                        .traffic
                        .checkpoints
                        .iter()
                        .any(|c| (v(c.pos) - self.world.stations[i].pos).length() < 400.0)
            })
            .collect();
        if !guarded.is_empty() {
            return guarded[self.rng.index(guarded.len())];
        }
        let mut to = self.rng.index(self.world.stations.len() - 1);
        if to >= from {
            to += 1;
        }
        to
    }

    /// Steht ein NPC-Schiff auf dieser Plattform?
    pub fn npc_on_pad(&self, pad: usize) -> bool {
        self.npcs
            .iter()
            .any(|n| n.alive && n.docked_pad() == Some(pad))
    }

    /// Hat die Händlerin an der Station angedockt, an der die Crew gerade steht?
    pub fn merchant_here(&self) -> bool {
        let Some(si) = self.docked_station() else {
            return false;
        };
        self.npcs.iter().any(|n| {
            n.alive
                && matches!(n.role, Role::Merchant { .. })
                && n.docked_pad()
                    .is_some_and(|p| self.world.pads[p].owner == super::world::Owner::Station(si))
        })
    }

    /// Ist diese Person (Kennung aus npcs.ron) an der Station, an der die Crew angedockt hat?
    pub fn vendor_here(&self, id: &str) -> bool {
        let Some(si) = self.docked_station() else {
            return false;
        };
        let here = &self.world.stations[si].id;
        self.data.npcs.iter().any(|n| n.id == id && &n.at == here)
    }

    /// Der Konvoi-Frachter hat am Ziel angedockt: Auftrag erfüllt.
    pub(crate) fn convoy_arrived(&mut self, mission: u32) {
        if let Some(i) = self.active.iter().position(|m| m.id == mission) {
            self.complete_mission(i);
        }
    }

    /// Geleitschutz: auf halber Strecke schlagen die Piraten zu.
    fn update_escorts(&mut self) {
        let ambush = self.data.traffic.ambush;
        for mi in 0..self.active.len() {
            let super::missions::MissionKind::Escort {
                from,
                to,
                npc: Some(id),
                ambushed: false,
                ..
            } = self.active[mi].kind
            else {
                continue;
            };
            let Some(n) = self.npcs.iter().find(|n| n.id == id && n.alive) else {
                continue;
            };
            if !n.has_departed() {
                continue;
            }
            let (a, b) = (self.world.stations[from].pos, self.world.stations[to].pos);
            let total = (b - a).length().max(1.0);
            let left = (b - n.ship.pos).length();
            if left > total * 0.6 {
                continue;
            }
            let (pos, vel) = (n.ship.pos, n.ship.vel);
            let ahead = vel.normalize_or((b - pos).normalize_or(Vec2::Y));
            let side = Vec2::new(-ahead.y, ahead.x);
            for k in 0..ambush {
                let s = if k % 2 == 0 { 1.0 } else { -1.0 };
                let at = pos + ahead * 110.0 + side * s * (70.0 + 15.0 * k as f32);
                self.spawn_drone(at, None, Some(id));
            }
            if let super::missions::MissionKind::Escort { ambushed, .. } = &mut self.active[mi].kind
            {
                *ambushed = true;
            }
            self.toast(
                "Hinterhalt! Piratendrohnen greifen den Frachter an",
                ToastKind::Bad,
            );
        }
    }

    /// Drohnen ohne Nest (Hinterhalt) ziehen ab, wenn es nichts mehr zu holen gibt.
    fn drone_gives_up(&self, i: usize) -> bool {
        let n = &self.npcs[i];
        if !matches!(n.role, Role::Drone { nest: None }) {
            return false;
        }
        let prey_alive = n
            .prey
            .is_some_and(|id| self.npcs.iter().any(|x| x.id == id && x.alive));
        !prey_alive && (self.ship.pos - n.ship.pos).length() > DRONE_AGGRO * 1.5
    }

    /// Schmuggelware an Bord? (Mission, für die sie gebraucht wird)
    pub fn contraband(&self) -> Option<u32> {
        self.active
            .iter()
            .filter(|m| matches!(m.kind, super::missions::MissionKind::Smuggle { .. }))
            .map(|m| m.id)
            .find(|&id| {
                self.ship.cargo.iter().any(|c| {
                    matches!(&c.kind, super::ship::CargoKind::Container { mission, .. } if *mission == id)
                })
            })
    }

    /// Zollbojen: wer mit Schmuggelware zu lange in ihrem Radius bleibt, wird erwischt.
    fn update_customs(&mut self) {
        let Some(mission) = self.contraband() else {
            self.customs = None;
            return;
        };
        if self.ship.destroyed || self.ship.docked.is_some() {
            self.customs = None;
            return;
        }
        let pos = self.ship.pos;
        let inside = self
            .data
            .traffic
            .checkpoints
            .iter()
            .position(|c| (v(c.pos) - pos).length() < c.radius);
        let Some(ci) = inside else {
            self.customs = None;
            return;
        };
        let scan = self.data.missions.smuggle.scan_time.max(0.5);
        let progress = match &self.customs {
            Some(c) if c.checkpoint == ci => c.progress + DT / scan,
            _ => {
                let cp = self.data.traffic.checkpoints[ci].clone();
                self.events.push(SimEvent::CustomsScan { pos: v(cp.pos) });
                self.toast(
                    format!("{} scannt euch – raus aus dem Radius!", cp.name),
                    ToastKind::Warn,
                );
                0.0
            }
        };
        if progress < 1.0 {
            self.customs = Some(CustomsScan {
                checkpoint: ci,
                progress,
            });
            return;
        }
        // Erwischt: Ware weg, Strafe, Ruf bei der nächsten Station leidet.
        self.customs = None;
        let fine = self.data.missions.smuggle.fine.min(self.crew.credits);
        self.crew.credits -= fine;
        let cp = v(self.data.traffic.checkpoints[ci].pos);
        let near = (0..self.world.stations.len()).min_by(|a, b| {
            (self.world.stations[*a].pos - cp)
                .length()
                .total_cmp(&(self.world.stations[*b].pos - cp).length())
        });
        if let Some(si) = near
            && let Some(r) = self.crew.reputation.get_mut(si)
        {
            *r = r.saturating_sub(2);
        }
        self.events.push(SimEvent::Busted);
        self.fail_mission(
            mission,
            &format!("Zoll: Ware beschlagnahmt, {fine} Cr Strafe"),
        );
    }
}

fn dyn_of(s: &Ship) -> Dyn {
    Dyn {
        pos: s.pos,
        vel: s.vel,
        w: s.ang_vel,
        inv_m: 1.0 / s.mass,
        inv_i: 1.0 / s.inertia,
    }
}

/// Leitstrahl der Station: zieht wie eine gedämpfte Feder zum Ziel und richtet das Schiff aus.
fn tractor(ship: &mut Ship, target: Vec2, angle: f32, max_speed: f32) {
    let v_des = ((target - ship.pos) * 0.7).clamp_length_max(max_speed);
    let acc = ((v_des - ship.vel) * 1.6).clamp_length_max(5.0);
    ship.vel += acc * DT;
    let alpha = (angle_diff(angle, ship.angle) * 2.0 - ship.ang_vel * 2.8).clamp(-3.0, 3.0);
    ship.ang_vel += alpha * DT;
}

/// Schiff an einen Ort setzen (Schwerpunkt), in Ruhe.
fn place(ship: &mut Ship, pos: Vec2, angle: f32) {
    ship.angle = angle;
    ship.prev_angle = angle;
    ship.pos = pos;
    ship.prev_pos = pos;
    ship.vel = Vec2::ZERO;
    ship.ang_vel = 0.0;
}
