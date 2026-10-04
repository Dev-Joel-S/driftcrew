//! Deterministische Simulation ohne Rendering-Abhängigkeit.
//!
//! * feste Schrittweite (60 Hz), keine Abhängigkeit von der Framerate
//! * Eingabe pro Tick = Bitmaske der Slots + Zielwinkel der Werkzeuge + Befehle
//! * Zufall nur über den geseedeten [`rng::Rng`]
//! * nur `Vec`s mit fester Reihenfolge, keine Hash-Iteration
//!
//! Die Simulation weiß nicht, wer gedrückt hat – nur welcher Slot.

pub mod autopilot;
pub mod cargo;
#[cfg(test)]
mod cargo_tests;
pub mod course;
#[cfg(test)]
mod course_tests;
pub mod data;
pub mod dock;
pub mod economy;
pub mod effects;
#[cfg(test)]
mod effects_tests;
pub mod explore;
pub mod finance;
#[cfg(test)]
mod finance_tests;
pub mod geom;
pub mod hazards;
pub mod journal;
#[cfg(test)]
mod journal_tests;
pub mod minigame;
#[cfg(test)]
mod minigame_tests;
pub mod missions;
pub mod npc;
#[cfg(test)]
mod npc_tests;
pub mod physics;
pub mod power;
#[cfg(test)]
mod power_tests;
pub mod precision;
#[cfg(test)]
mod progress_tests;
pub mod project;
pub mod replay;
pub mod rng;
pub mod sector;
pub mod ship;
pub mod stats;
pub mod story;
#[cfg(test)]
mod story_tests;
#[cfg(test)]
mod systems_tests;
pub mod tools;
pub mod workshop;
#[cfg(test)]
mod workshop_tests;
pub mod world;

use std::sync::Arc;

use bevy::math::Vec2;

use data::{CrewSave, GameData, Livery, Ore};
use economy::{Purchase, Vote};
use missions::Mission;
use rng::Rng;
use ship::{Loadout, Ship};
use world::World;

pub const TICK_HZ: f64 = 60.0;
pub const DT: f32 = 1.0 / 60.0;
pub const MAX_SLOTS: usize = 32;

/// Eingabe für genau einen Tick.
#[derive(Clone, Debug, Default)]
pub struct TickInput {
    /// Bit i = Slot i gedrückt.
    pub slots: u32,
    /// Zielwinkel (Welt, Bogenmaß) pro Slot, nur für Werkzeuge relevant.
    pub aims: Vec<f32>,
    pub commands: Vec<Command>,
}

impl TickInput {
    pub fn pressed(&self, slot: u8) -> bool {
        (self.slots >> slot) & 1 == 1
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Undock,
    /// Kauf anstoßen (bei mehreren Crewmitgliedern startet eine Abstimmung).
    Buy {
        purchase: Purchase,
        voter: u8,
    },
    /// Stimme setzen: Some(true) = ja, Some(false) = nein.
    Vote {
        voter: u8,
        yes: bool,
    },
    AcceptMission {
        id: u32,
    },
    AbandonMission {
        id: u32,
    },
    SellOre,
    SwitchShip {
        id: String,
    },
    /// Ein Crewmitglied markiert einen Ort (für alle sichtbar).
    Ping {
        player: u8,
        pos: Vec2,
    },
    /// Kartendaten an der Station verkaufen.
    SellCharts,
    /// Material und Bauteile für den Wiederaufbau der angedockten Station abgeben.
    DeliverProject,
    /// Erz, Schrott und Bauteile aus dem Frachtraum ins Crew-Lager bringen.
    StoreCargo,
    /// Parcours wählen (der Lauf beginnt am Starttor) bzw. den laufenden abbrechen.
    StartCourse {
        course: usize,
    },
    AbortCourse,
    /// 61f: Artefakte an Bord bleiben bei Zerstörung im Wrack (true) oder kehren zurück.
    SetStayInWreck(bool),
    /// Ladeplan (64, 67): Frachtstück in ein anderes Modul umladen bzw. abwerfen.
    MoveCargo {
        id: u32,
        to: usize,
    },
    Jettison {
        id: u32,
    },
    /// Persönliches (81, 82): Schiff taufen, Notiz ins Logbuch, Kartenmarkierung setzen/entfernen.
    NameShip {
        name: String,
    },
    AddNote {
        text: String,
    },
    AddMark {
        pos: Vec2,
        text: String,
    },
    RemoveMark {
        idx: usize,
    },
    /// Zuruf (73): kurzes Signal eines Crewmitglieds an alle.
    Callout {
        player: u8,
        call: Call,
    },
    /// Slots neu verteilt (z. B. Hot-Join mitten im Flug): Schiff umbauen.
    SetLoadout {
        thrusters: u8,
        tools: Vec<usize>,
        crew_size: u8,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToastKind {
    Info,
    Good,
    Warn,
    Bad,
}

/// Ereignisse eines Ticks – für Effekte, Sound und UI. Werden jeden Tick geleert.
#[derive(Clone, Debug, PartialEq)]
pub enum SimEvent {
    Impact {
        pos: Vec2,
        normal: Vec2,
        strength: f32,
    },
    ShieldHit {
        pos: Vec2,
    },
    Damage {
        amount: f32,
    },
    Explosion {
        pos: Vec2,
        size: f32,
        color: [f32; 3],
    },
    Shot {
        pos: Vec2,
        dir: Vec2,
    },
    EmptyGun,
    ProjectileHit {
        pos: Vec2,
    },
    CraneFire,
    CraneAttach {
        pos: Vec2,
    },
    CraneRelease,
    Stowed {
        what: String,
    },
    Docked {
        pad: usize,
    },
    Undocked,
    Purchased {
        name: String,
    },
    VoteStarted,
    VoteEnded {
        passed: bool,
        label: String,
    },
    MissionAccepted {
        id: u32,
    },
    MissionCompleted {
        id: u32,
        reward: u32,
    },
    MissionFailed {
        id: u32,
    },
    Toast {
        text: String,
        kind: ToastKind,
    },
    ShipDestroyed,
    Respawned,
    /// Ein neues Schiff wurde gekauft/gewählt: Slots müssen neu verteilt werden.
    ShipChanged,
    Sold {
        credits: u32,
    },
    Ping {
        player: u8,
        pos: Vec2,
    },
    ScanPulse {
        pos: Vec2,
        range: f32,
    },
    /// Wiederaufbau: eine Etappe ist fertig.
    StageCompleted {
        station: usize,
        stage: u8,
    },
    /// Parcours: Start, Tor/Schritt geschafft, Ziel, Abbruch.
    CourseStarted {
        course: usize,
    },
    CourseCheckpoint {
        course: usize,
        step: usize,
    },
    CourseFinished {
        course: usize,
    },
    CourseAborted {
        course: usize,
    },
    /// Messflug: ein Messfeld ist fertig gemessen.
    SurveyField {
        pos: Vec2,
    },
    /// Minispiele: Tastendruck im Takt (getroffen?), Triebwerk geflickt, Hack, Alarm.
    Beat {
        hit: bool,
    },
    Patched {
        slot: u8,
    },
    HackStarted,
    HackDone,
    Alarm,
    /// Die Rivalen-Crew hat einen Notruf übernommen; eine Piratendrohne ist zerstört.
    RivalTook,
    DroneDown {
        pos: Vec2,
    },
    /// Zoll: Scan gestartet, Schmuggelware entdeckt.
    CustomsScan {
        pos: Vec2,
    },
    Busted,
    /// Geschichte: Funkspruch (Kapitel, Monument), Logbuch-Fund, Artefakt in der Sammlung,
    /// Monument erwacht, Kapitel abgeschlossen.
    Story {
        speaker: String,
        text: String,
    },
    LogFound {
        id: String,
    },
    ArtifactCollected {
        id: String,
    },
    MonumentWoke {
        idx: usize,
    },
    ChapterDone {
        chapter: usize,
    },
    /// Zuruf eines Crewmitglieds (für den optionalen Ton).
    Callout {
        player: u8,
        call: Call,
    },
    /// Fracht abgeworfen (treibt jetzt in der Welt).
    Jettisoned {
        pos: Vec2,
    },
}

/// Zurufe (73): kurze Signale ohne Pflichtrollen – jeder darf alles rufen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Brake,
    ThrustOff,
    TurnLeft,
    TurnRight,
    ToolReady,
}

impl Call {
    pub fn label(self) -> &'static str {
        match self {
            Call::Brake => "Bremsen!",
            Call::ThrustOff => "Schub aus!",
            Call::TurnLeft => "Links drehen!",
            Call::TurnRight => "Rechts drehen!",
            Call::ToolReady => "Werkzeug bereit!",
        }
    }
}

/// Ein Zuruf über dem Schiff, verblasst nach [`CALLOUT_SECONDS`].
#[derive(Clone, Debug, PartialEq)]
pub struct Callout {
    pub player: u8,
    pub call: Call,
    pub life: f32,
}

pub const CALLOUT_SECONDS: f32 = 2.5;

/// Markierung eines Crewmitglieds (Ping), verblasst nach [`PING_SECONDS`].
#[derive(Clone, Debug, PartialEq)]
pub struct Ping {
    pub player: u8,
    pub pos: Vec2,
    /// Was dort ist („Wrack“, „Kepler-Außenposten“ …), leer = freier Raum.
    pub label: String,
    pub life: f32,
}

pub const PING_SECONDS: f32 = 6.0;

#[derive(Clone, Debug, PartialEq)]
pub enum BodyKind {
    Asteroid {
        ore: Option<Ore>,
        ore_left: f32,
        hp: f32,
        field: usize,
        /// Reiche Erzader an einer Stelle der Oberfläche (Präzisionsarbeit).
        vein: bool,
    },
    Meteor,
    OreChunk {
        ore: Ore,
        amount: f32,
    },
    Capsule {
        mission: u32,
    },
    Derelict {
        mission: u32,
        name: String,
    },
    /// Schwerlastkiste eines Lieferauftrags – passt in keinen Frachtraum, wird geschleppt.
    Crate {
        mission: u32,
        name: String,
    },
    /// Wrack zum Ausschlachten (Index in `world.wrecks`).
    Wreck {
        idx: usize,
        scrap: f32,
        parts: u32,
        home: Vec2,
    },
    /// Abgerissenes Bauteil – klein genug zum Verstauen, wird am Markt verkauft.
    Salvage {
        name: String,
        value: u32,
    },
    /// Treibender Schrott in Trümmerzonen.
    Debris,
    /// Sperriges Bergungsobjekt (Antenne, Ringsegment, Rumpfplatte): passt in keinen
    /// Frachtraum. Form = Kette aus Kreisen entlang der lokalen x-Achse.
    Bulky {
        mission: u32,
        name: String,
        half_len: f32,
        thick: f32,
    },
    /// Artefakt der Vorgänger (Geschichte): einmalig, schwer für seine Größe.
    Artifact {
        id: String,
    },
    /// Abgeworfenes oder gefundenes Frachtstück (mit Eigenschaften und Zustand).
    Dropped {
        item: ship::CargoItem,
    },
}

/// Kollisionskreise eines Körpers (Weltposition, Radius). Runde Körper haben einen,
/// sperrige eine Kette.
#[derive(Clone, Copy, Debug)]
pub struct Circles {
    pub list: [(Vec2, f32); 9],
    pub n: usize,
}

impl Circles {
    pub fn iter(&self) -> impl Iterator<Item = (Vec2, f32)> + '_ {
        self.list[..self.n].iter().copied()
    }
}

#[derive(Clone, Debug)]
pub struct Body {
    pub id: u32,
    pub kind: BodyKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub angle: f32,
    pub ang_vel: f32,
    pub radius: f32,
    pub mass: f32,
    pub prev_pos: Vec2,
    pub prev_angle: f32,
    pub alive: bool,
    pub seed: u32,
    pub age: f32,
}

impl Body {
    pub fn inertia(&self) -> f32 {
        match self.kind {
            BodyKind::Bulky {
                half_len, thick, ..
            } => self.mass * ((2.0 * half_len).powi(2) / 12.0 + 0.5 * thick * thick),
            _ => 0.5 * self.mass * self.radius * self.radius,
        }
    }

    pub fn circles(&self) -> Circles {
        let mut c = Circles {
            list: [(self.pos, self.radius); 9],
            n: 1,
        };
        if let BodyKind::Bulky {
            half_len, thick, ..
        } = self.kind
        {
            let axis = Vec2::new(self.angle.cos(), self.angle.sin());
            let n = ((2.0 * half_len / thick).ceil() as usize + 1).clamp(2, 9);
            for k in 0..n {
                let t = -half_len + 2.0 * half_len * k as f32 / (n - 1) as f32;
                c.list[k] = (self.pos + axis * t, thick);
            }
            c.n = n;
        }
        c
    }

    /// Punkt im Körper (lokal, mitgedreht) in Weltkoordinaten.
    pub fn anchor(&self, local: Vec2) -> Vec2 {
        self.pos + geom::rot(local, self.angle)
    }

    pub fn point_velocity(&self, world: Vec2) -> Vec2 {
        self.vel + geom::cross_sv(self.ang_vel, world - self.pos)
    }
    pub fn grabbable(&self) -> bool {
        !matches!(self.kind, BodyKind::Meteor)
    }
    /// Klein genug, um eingeholt und eingelagert zu werden.
    pub fn stowable(&self) -> bool {
        matches!(
            self.kind,
            BodyKind::OreChunk { .. }
                | BodyKind::Capsule { .. }
                | BodyKind::Salvage { .. }
                | BodyKind::Artifact { .. }
                | BodyKind::Dropped { .. }
        )
    }
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub id: u32,
    pub pos: Vec2,
    pub prev_pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    /// Von einer Piratendrohne abgefeuert (trifft die Crew und Konvois, nicht Drohnen).
    pub hostile: bool,
}

/// Rettungskapsel der Crew, nachdem das Schiff zerstört wurde (fliegt bis zur Bergung).
#[derive(Clone, Debug)]
pub struct EscapePod {
    pub pos: Vec2,
    pub vel: Vec2,
    pub angle: f32,
    pub spin: f32,
    pub prev_pos: Vec2,
    pub prev_angle: f32,
}

/// Gemeinsame Kasse und Fortschritt der Crew.
#[derive(Clone, Debug)]
pub struct Crew {
    pub size: u8,
    pub credits: u32,
    pub upgrades: Vec<String>,
    pub owned_ships: Vec<String>,
    pub current_ship: String,
    pub home_station: usize,
    pub missions_done: u32,
    pub ore_sold: f32,
    /// Rufpunkte pro Station (Index wie `world.stations`).
    pub reputation: Vec<u32>,
    /// Lackierung pro Schiff.
    pub liveries: Vec<(String, Livery)>,
    /// Versicherung abgeschlossen (Prämie bei jedem Auftrag, übernimmt Bergungskosten).
    pub insured: bool,
    /// Laufender Schiffskredit.
    pub loan: Option<finance::Loan>,
    /// Crew-Lager: Material pro Erzsorte (Reihenfolge wie `Ore::ALL`), Bauteile, Artefakte.
    pub storage: [f32; 5],
    pub storage_parts: u32,
    pub artifacts: Vec<String>,
    /// Angebaute Module pro Schiff: (Schiff, [(Bauplatz, Modul)]).
    pub builds: Vec<(String, Vec<(String, String)>)>,
    /// Gerettete Besatzungen, die sich noch melden werden.
    pub rescued: Vec<data::RescuedSave>,
}

impl Crew {
    pub fn livery(&self, ship: &str) -> Livery {
        self.liveries
            .iter()
            .find(|(s, _)| s == ship)
            .map(|(_, l)| l.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct SimState {
    pub data: Arc<GameData>,
    pub tick: u64,
    pub time: f32,
    pub rng: Rng,
    pub world: World,
    pub ship: Ship,
    pub loadout: Loadout,
    pub bodies: Vec<Body>,
    pub projectiles: Vec<Projectile>,
    pub crew: Crew,
    pub offers: Vec<Mission>,
    pub active: Vec<Mission>,
    pub vote: Option<Vote>,
    pub events: Vec<SimEvent>,
    pub next_id: u32,
    pub meteor_timers: Vec<f32>,
    pub field_respawn: Vec<f32>,
    pub border_warn: f32,
    pub low_hull_warned: bool,
    /// 0 = Tank ok, 1 = Warnung „knapp“ gezeigt, 2 = Warnung „leer“ gezeigt.
    pub fuel_warned: u8,
    /// Nach der Zerstörung: die Kapsel und die fällige Bergungsgebühr.
    pub escape: Option<EscapePod>,
    pub salvage_fee: u32,
    /// Index der Anomalie, in deren Sog das Schiff zuletzt gewarnt wurde.
    pub pull_warned: Option<usize>,
    /// Fog of War.
    pub explored: explore::Exploration,
    /// Spaßstatistik (laufend) und die Auswertung des zuletzt erledigten Auftrags.
    pub stats: stats::CrewStats,
    pub report: Option<stats::MissionReport>,
    pub pings: Vec<Ping>,
    /// Laufendes Zufallsereignis und Zeit bis zum nächsten.
    pub event: Option<sector::WorldEvent>,
    pub event_timer: f32,
    /// Scanner: laufender Impuls und gefundene Dinge.
    pub scan: Option<sector::ScanPulse>,
    pub blips: Vec<sector::ScanBlip>,
    /// Kartografie: mit dem Scanner erfasste Zellen und noch nicht verkaufte Daten.
    pub surveyed: explore::Exploration,
    pub charts_unsold: u32,
    /// Geschwindigkeit im letzten Tick (für die Beschleunigung, die Passagiere spüren).
    pub prev_vel: Vec2,
    /// Wiederaufbau pro Station (nur Stationen mit Projekt haben einen Eintrag).
    pub projects: Vec<project::ProjectState>,
    /// Laufende Präzisionsarbeit mit dem Bohrer (Erzader, Wrackverbindung).
    pub precision: Option<precision::Precision>,
    /// Parcours: laufender Lauf, letztes Ergebnis, Bestenlisten (eine pro Parcours).
    pub course: Option<course::CourseRun>,
    pub course_result: Option<course::CourseResult>,
    pub records: Vec<data::CourseRecord>,
    /// Markt: Preisfaktor pro Ort (Stationen, dann Planeten) und Erzsorte; laufende Nachfrage.
    pub market: Vec<[f32; 5]>,
    pub demand: Option<finance::Demand>,
    pub demand_timer: f32,
    /// Wo und wann zuletzt Dockgebühr gezahlt wurde.
    pub last_dock_fee: Option<(usize, f32)>,
    /// Minispiele: Slot-Tasten im letzten Tick, Notreparatur, laufender Hack, bis wann ein
    /// gesicherter Port offen ist und bis wann er nach einem Fehler gesperrt bleibt.
    pub prev_slots: u32,
    pub repair: minigame::Repair,
    pub hack: Option<minigame::Hack>,
    pub unlocked: Vec<f32>,
    pub hack_lockout: Vec<f32>,
    /// NPC-Schiffe mit eigener Physik, ihre Andockplätze, Punkte der Rivalen-Crew,
    /// Abklingzeit der Piratennester, laufender Zollscan.
    pub npcs: Vec<npc::Npc>,
    pub npc_docks: Vec<npc::NpcDock>,
    pub rival_score: u32,
    pub nest_cooldown: Vec<f32>,
    pub customs: Option<npc::CustomsScan>,
    /// Geschichte: Kapitel, Logbuch, Fundorte der Artefakte.
    pub story: story::Story,
    /// Fracht: Geschwindigkeit im letzten Flug-Tick (für Stöße und Schwappen), Fortschritt
    /// beim Hinüberholen Geretteter, Pause zwischen Frachtwarnungen.
    pub cargo_vel: Option<Vec2>,
    pub rescue_timer: f32,
    pub cargo_warn: f32,
    /// Die Welt reagiert (76, 79): Wirkungen erledigter Aufträge, entdeckte Routen, Bestzeiten,
    /// laufender Routenflug.
    pub effects: Vec<effects::Effect>,
    pub routes_known: Vec<String>,
    pub route_best: Vec<(String, f32)>,
    pub route_run: Option<effects::RouteRun>,
    /// Gerade beendete Route: startet erst wieder, wenn das Schiff die Enden verlassen hat.
    pub route_rest: Option<usize>,
    /// Schiffsname, Plaketten, Tagebuch, Kartenmarkierungen (81, 82).
    pub journal: journal::Journal,
    /// Zusammenarbeit (69, 70, 73): Energiebedarf dieses Ticks, Warnung „leer“ gezeigt, Zurufe.
    pub power_draw: f32,
    pub energy_warned: bool,
    pub callouts: Vec<Callout>,
}

impl SimState {
    pub fn new(data: Arc<GameData>, save: &CrewSave, loadout: Loadout, crew_size: u8) -> SimState {
        let mut rng = Rng::new(data.world.seed);
        let world = World::build(&data, &mut rng);
        let home = data.station_index(&save.home_station).unwrap_or(0);
        let reputation = world
            .stations
            .iter()
            .map(|st| {
                save.reputation
                    .iter()
                    .find(|(id, _)| *id == st.id)
                    .map_or(0, |(_, p)| *p)
            })
            .collect();
        let mut explored = explore::Exploration::new(world.radius);
        explored.load_hex(&save.explored);
        let mut surveyed = explore::Exploration::new(world.radius);
        surveyed.load_hex(&save.surveyed);
        let first_event = data.world.events.interval.0;
        // Bekannte Stationen sind von Anfang an aufgedeckt.
        for (st, sd) in world.stations.iter().zip(&data.world.stations) {
            if sd.known {
                explored.reveal(st.pos, 260.0);
            }
        }
        let crew = Crew {
            size: crew_size.max(1),
            credits: save.credits,
            upgrades: save.upgrades.clone(),
            owned_ships: save.owned_ships.clone(),
            current_ship: save.current_ship.clone(),
            home_station: home,
            missions_done: save.missions_done,
            ore_sold: save.ore_sold,
            reputation,
            liveries: save.liveries.clone(),
            insured: false,
            loan: None,
            storage: [0.0; 5],
            storage_parts: 0,
            artifacts: Vec::new(),
            builds: save.builds.clone(),
            rescued: Vec::new(),
        };
        let stats = economy::stats_for(&data, &crew.upgrades);
        let build: Vec<(String, String)> = crew
            .builds
            .iter()
            .find(|(sh, _)| *sh == crew.current_ship)
            .map(|(_, b)| b.clone())
            .unwrap_or_default();
        let def = data.built_ship(&crew.current_ship, &build);
        let ship = Ship::build(&def, &loadout, &stats);
        let n_zones = data.world.meteor_zones.len();
        let n_stations = data.world.stations.len();
        let n_fields = data.world.asteroid_fields.len();
        let mut s = SimState {
            data,
            tick: 0,
            time: 0.0,
            rng,
            world,
            ship,
            loadout,
            bodies: Vec::new(),
            projectiles: Vec::new(),
            crew,
            offers: Vec::new(),
            active: Vec::new(),
            vote: None,
            events: Vec::new(),
            next_id: 1,
            meteor_timers: vec![0.0; n_zones],
            field_respawn: vec![0.0; n_fields],
            border_warn: 0.0,
            low_hull_warned: false,
            fuel_warned: 0,
            escape: None,
            salvage_fee: 0,
            pull_warned: None,
            explored,
            stats: stats::CrewStats::default(),
            report: None,
            pings: Vec::new(),
            event: None,
            event_timer: first_event,
            scan: None,
            blips: Vec::new(),
            surveyed,
            charts_unsold: save.charts_unsold,
            prev_vel: Vec2::ZERO,
            projects: Vec::new(),
            precision: None,
            course: None,
            course_result: None,
            records: Vec::new(),
            market: Vec::new(),
            demand: None,
            demand_timer: 0.0,
            last_dock_fee: None,
            prev_slots: 0,
            repair: minigame::Repair::default(),
            hack: None,
            unlocked: vec![0.0; n_stations],
            hack_lockout: vec![0.0; n_stations],
            npcs: Vec::new(),
            npc_docks: Vec::new(),
            rival_score: 0,
            nest_cooldown: Vec::new(),
            customs: None,
            story: story::Story::default(),
            cargo_vel: None,
            rescue_timer: 0.0,
            cargo_warn: 0.0,
            effects: Vec::new(),
            routes_known: Vec::new(),
            route_best: Vec::new(),
            route_run: None,
            route_rest: None,
            journal: journal::Journal::default(),
            power_draw: 0.0,
            energy_warned: false,
            callouts: Vec::new(),
        };
        s.load_projects(save);
        s.load_records(save);
        s.load_finance(save);
        s.load_workshop(save);
        s.load_story(save);
        s.load_cargo_state(save);
        s.load_world_state(save);
        s.load_journal(save);
        s.populate_fields();
        s.populate_wrecks();
        s.refresh_offers();
        s.dock_at_station(home);
        s.nest_cooldown = vec![0.0; s.data.traffic.nests.len()];
        s.spawn_traffic();
        s.events.clear();
        s
    }

    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.events.push(SimEvent::Toast {
            text: text.into(),
            kind,
        });
    }

    /// Schiff mit neuer Slot-Belegung neu bauen (nach der Lobby). Zustand bleibt erhalten.
    pub fn rebuild_ship(&mut self, loadout: Loadout) {
        let stats = economy::stats_for(&self.data, &self.crew.upgrades);
        let def = self.current_def();
        let old = self.ship.clone();
        let mut ship = Ship::build(&def, &loadout, &stats);
        let same_ship = old.def_id == ship.def_id;
        ship.angle = old.angle;
        ship.prev_angle = old.prev_angle;
        let origin = old.origin();
        ship.pos = origin + geom::rot(ship.com, ship.angle);
        ship.prev_pos = ship.pos;
        ship.vel = old.vel;
        ship.ang_vel = old.ang_vel;
        ship.docked = old.docked;
        if same_ship {
            ship.hull = old.hull.min(ship.max_hull);
            ship.shield = old.shield.min(ship.max_shield);
            ship.ammo = old.ammo.min(ship.max_ammo);
            ship.fuel = old.fuel.min(ship.max_fuel);
            // Schäden bleiben am jeweiligen Triebwerk, auch wenn es neu angeordnet wird.
            for t in &mut ship.thrusters {
                if let Some(o) = old.thrusters.iter().find(|o| o.part == t.part) {
                    t.health = o.health;
                }
            }
        }
        // Fracht übernehmen, soweit Platz ist.
        for item in old.cargo {
            ship.store_item(item);
        }
        self.ship = ship;
        self.loadout = loadout;
        if let Some(pad) = self.ship.docked {
            self.snap_to_pad(pad);
        }
    }

    pub fn set_crew_size(&mut self, n: u8) {
        self.crew.size = n.max(1);
    }

    pub fn to_save(&self) -> CrewSave {
        let mut save = CrewSave {
            version: 1,
            credits: self.crew.credits,
            upgrades: self.crew.upgrades.clone(),
            owned_ships: self.crew.owned_ships.clone(),
            current_ship: self.crew.current_ship.clone(),
            home_station: self.world.stations[self.crew.home_station].id.clone(),
            missions_done: self.crew.missions_done,
            ore_sold: self.crew.ore_sold,
            reputation: self
                .world
                .stations
                .iter()
                .zip(&self.crew.reputation)
                .filter(|(_, p)| **p > 0)
                .map(|(st, p)| (st.id.clone(), *p))
                .collect(),
            explored: self.explored.to_hex(),
            liveries: self.crew.liveries.clone(),
            surveyed: self.surveyed.to_hex(),
            charts_unsold: self.charts_unsold,
            projects: self.save_projects(),
            records: self.save_records(),
            insured: false,
            loan: None,
            market: Vec::new(),
            demand: None,
            storage: Vec::new(),
            storage_parts: 0,
            artifacts: Vec::new(),
            builds: Vec::new(),
            story: Default::default(),
            dropped: Vec::new(),
            rescued: Vec::new(),
            effects: Vec::new(),
            routes: Vec::new(),
            route_best: Vec::new(),
            journal: Default::default(),
        };
        self.save_finance(&mut save);
        self.save_workshop(&mut save);
        self.save_story(&mut save);
        self.save_cargo_state(&mut save);
        self.save_world_state(&mut save);
        self.save_journal(&mut save);
        save
    }

    /// Ein Simulationsschritt.
    pub fn step(&mut self, input: &TickInput) {
        self.events.clear();
        self.tick += 1;
        self.time += DT;

        self.ship.prev_pos = self.ship.pos;
        self.ship.prev_angle = self.ship.angle;
        for b in &mut self.bodies {
            b.prev_pos = b.pos;
            b.prev_angle = b.angle;
        }
        for p in &mut self.projectiles {
            p.prev_pos = p.pos;
        }
        for s in &mut self.world.spinners {
            s.prev_angle = s.angle;
            s.angle += s.speed * DT;
        }

        self.handle_commands(&input.commands);
        self.update_vote(input);

        if self.ship.destroyed {
            self.update_respawn();
        } else if self.update_minigames(input) {
            // Slot-Tasten gehören gerade einem Minispiel: das Schiff treibt.
            let idle = TickInput {
                slots: 0,
                aims: input.aims.clone(),
                commands: Vec::new(),
            };
            self.apply_input(&idle);
        } else {
            self.apply_input(input);
        }
        self.apply_forces();
        self.integrate();
        self.collide();
        self.update_npcs();
        self.update_story();
        self.update_projectiles();
        if !self.ship.destroyed {
            self.update_docking(input);
        }
        self.update_hazards();
        self.update_sectors();
        self.update_market();
        self.update_missions();
        // Vor dem Tracking: dort wird die Geschwindigkeit des letzten Ticks überschrieben,
        // die das Präzisionsandocken als Aufsetzgeschwindigkeit braucht.
        self.update_course();
        self.update_cargo();
        self.update_effects();
        self.update_tracking();
        self.update_precision();
        self.update_regen();
        self.update_power();
        self.check_ship_health();
        self.update_journal();
        for p in &mut self.pings {
            p.life -= DT;
        }
        for c in &mut self.callouts {
            c.life -= DT;
        }
        self.callouts.retain(|c| c.life > 0.0);
        self.pings.retain(|p| p.life > 0.0);

        self.bodies.retain(|b| b.alive);
        self.projectiles.retain(|p| p.life > 0.0);
    }
}

impl SimState {
    /// Ping setzen: höchstens einer pro Crewmitglied, der neue ersetzt den alten.
    pub(crate) fn add_ping(&mut self, player: u8, pos: Vec2) {
        let label = self.describe_spot(pos);
        self.pings.retain(|p| p.player != player);
        self.pings.push(Ping {
            player,
            pos,
            label,
            life: PING_SECONDS,
        });
        self.events.push(SimEvent::Ping { player, pos });
    }

    /// Was liegt an diesem Ort? (für die Beschriftung von Pings)
    pub fn describe_spot(&self, p: Vec2) -> String {
        let body = self
            .bodies
            .iter()
            .filter(|b| b.alive && (b.pos - p).length() < b.radius + 6.0)
            .min_by(|a, b| (a.pos - p).length().total_cmp(&(b.pos - p).length()));
        if let Some(b) = body {
            return match &b.kind {
                BodyKind::Asteroid { ore: Some(o), .. } => format!("Asteroid ({})", o.label()),
                BodyKind::Asteroid { .. } => "Asteroid".into(),
                BodyKind::Meteor => "Meteor".into(),
                BodyKind::OreChunk { ore, .. } => format!("Erzbrocken ({})", ore.label()),
                BodyKind::Capsule { .. } => "Rettungskapsel".into(),
                BodyKind::Derelict { name, .. } => name.clone(),
                BodyKind::Crate { name, .. } => name.clone(),
                BodyKind::Wreck { idx, .. } => self.data.world.wrecks[*idx].name.clone(),
                BodyKind::Salvage { name, .. } => name.clone(),
                BodyKind::Debris => "Trümmer".into(),
                BodyKind::Bulky { name, .. } => name.clone(),
                BodyKind::Artifact { .. } => "Fremdes Objekt".into(),
                BodyKind::Dropped { item } => self.cargo_label(&item.kind),
            };
        }
        if let Some(st) = self
            .world
            .stations
            .iter()
            .find(|st| st.bounds.expand(10.0).contains(p))
        {
            return st.name.clone();
        }
        if let Some(pl) = self
            .world
            .planets
            .iter()
            .find(|pl| (pl.pos - p).length() < pl.radius + 15.0)
        {
            return pl.name.clone();
        }
        String::new()
    }

    /// Schild lädt nach einer Pause ohne Treffer nach (nicht während einer Sonneneruption),
    /// Reparaturdrohnen flicken die Hülle langsam. Die Hülle heilt sonst nie von selbst.
    fn update_regen(&mut self) {
        if self.ship.destroyed {
            return;
        }
        self.ship.since_hit += DT;
        if self.ship.since_hit > self.ship.shield_delay && !self.flare() {
            // Schnellladen aus der Zusatzenergie (70), solange der Schild nicht voll ist.
            let fast = self.ship.shield < self.ship.max_shield - 0.01
                && self.draw_energy(power::DRAW_SHIELD);
            let regen = self.ship.shield_regen * if fast { 2.5 } else { 1.0 };
            self.ship.shield = (self.ship.shield + regen * DT).min(self.ship.max_shield);
        }
        if self.ship.hull_regen > 0.0 && self.ship.hull > 0.0 {
            self.ship.hull = (self.ship.hull + self.ship.hull_regen * DT).min(self.ship.max_hull);
        }
    }

    /// Erkundung und Statistik nachführen (läuft jeden Tick, deckt alle 15 Ticks auf).
    fn update_tracking(&mut self) {
        if self.ship.destroyed {
            return;
        }
        if self.tick.is_multiple_of(15) {
            // Im Nebel sieht die Crew weniger weit.
            let fog = self.sector_at(self.ship.pos).nebula;
            self.explored
                .reveal(self.ship.pos, explore::SIGHT * (1.0 - 0.6 * fog));
        }
        let speed = self.ship.vel.length();
        self.stats.time += DT;
        if self.ship.docked.is_none() {
            self.stats.distance += speed * DT;
            self.stats.top_speed = self.stats.top_speed.max(speed);
            for t in &self.ship.thrusters {
                if t.firing {
                    self.stats.slots[t.slot as usize].thrust_time += DT;
                }
            }
        }
        // Passagiere spüren Beschleunigung, Stöße und schnelles Drehen.
        let acc = (self.ship.vel - self.prev_vel).length() / DT;
        self.prev_vel = self.ship.vel;
        let spin = self.ship.ang_vel.abs();
        let flying = self.ship.docked.is_none();
        // Seilbelastung pro Auftrag, solange dessen Last am Kran hängt.
        let hooked: Vec<(u32, f32)> = self
            .ship
            .tools
            .iter()
            .filter_map(|t| match t.crane {
                ship::CraneState::Attached { body, .. } => Some((body, t.strain)),
                _ => None,
            })
            .collect();
        for m in &mut self.active {
            m.top_speed = m.top_speed.max(speed);
            let body = match m.kind {
                missions::MissionKind::Haul { body, .. }
                | missions::MissionKind::Bulky { body, .. }
                | missions::MissionKind::Tow { body, .. } => body,
                _ => None,
            };
            if let Some(&(_, k)) = body.and_then(|b| hooked.iter().find(|(h, _)| *h == b)) {
                m.max_strain = Some(m.max_strain.unwrap_or(0.0).max(k));
            }
            if let missions::MissionKind::Passengers { comfort, .. } = &mut m.kind
                && flying
            {
                let loss = (acc - missions::COMFORT_ACCEL).clamp(0.0, 400.0) * 0.004
                    + (spin - 1.8).max(0.0) * 0.03;
                *comfort = (*comfort - loss * DT).max(0.0);
            }
        }
    }

    /// Ist dieser Ort schon entdeckt (Fog of War)?
    pub fn discovered(&self, p: Vec2) -> bool {
        self.explored.is_explored(p)
    }

    /// Station auf Karte und Radar zeigen? Bekannte Stationen von Anfang an, andere erst,
    /// wenn die Gegend erkundet ist.
    pub fn station_known(&self, si: usize) -> bool {
        self.data.world.stations[si].known || self.discovered(self.world.stations[si].pos)
    }

    /// Slot, dessen Teil einem Punkt am nächsten liegt (für die Kollisionsstatistik).
    pub(crate) fn nearest_slot(&self, p: Vec2) -> Option<u8> {
        let th = self
            .ship
            .thrusters
            .iter()
            .map(|t| (t.slot, self.ship.to_world(t.pos)));
        let tl = self
            .ship
            .tools
            .iter()
            .map(|t| (t.slot, self.ship.to_world(t.pos)));
        th.chain(tl)
            .min_by(|a, b| (a.1 - p).length().total_cmp(&(b.1 - p).length()))
            .map(|(s, _)| s)
    }
}

/// Bezeichnung eines Triebwerks nach Position (von links nach rechts).
pub fn thruster_label(i: usize, n: usize) -> String {
    let names: &[&str] = match n {
        1 => &["Mitte"],
        2 => &["Links", "Rechts"],
        3 => &["Links", "Mitte", "Rechts"],
        4 => &["Außen L", "Links", "Rechts", "Außen R"],
        5 => &["Außen L", "Links", "Mitte", "Rechts", "Außen R"],
        _ => &[],
    };
    names
        .get(i)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Triebwerk {}", i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_sim() -> SimState {
        let data = Arc::new(GameData::embedded().unwrap());
        let save = CrewSave::new_game(&data);
        let def = data.ship(&save.current_ship).clone();
        SimState::new(data, &save, Loadout::full(&def), 1)
    }

    fn fingerprint(s: &SimState) -> (u32, u32, u32, usize) {
        (
            s.ship.pos.x.to_bits() ^ s.ship.pos.y.to_bits(),
            s.ship.angle.to_bits(),
            s.bodies.iter().fold(0u32, |a, b| {
                a.wrapping_add(b.pos.x.to_bits()).rotate_left(3)
            }),
            s.bodies.len(),
        )
    }

    fn scripted_input(t: u64) -> TickInput {
        // Abheben, drehen, schießen, Kran – ein bisschen von allem.
        let mut slots = 0u32;
        if t > 10 {
            slots |= 0b11;
        }
        if (200..260).contains(&t) {
            slots &= !0b1;
        }
        if t.is_multiple_of(90) {
            slots |= 1 << 5;
        }
        if t.is_multiple_of(120) {
            slots |= 1 << 6;
        }
        TickInput {
            slots,
            aims: vec![1.0; MAX_SLOTS],
            commands: vec![],
        }
    }

    #[test]
    fn simulation_is_deterministic() {
        let mut a = new_sim();
        let mut b = new_sim();
        for t in 0..1200 {
            let inp = scripted_input(t);
            a.step(&inp);
            b.step(&inp);
        }
        assert_eq!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn starts_docked_and_undocks_with_thrust() {
        let mut s = new_sim();
        assert!(s.ship.docked.is_some());
        let inp = TickInput {
            slots: 0b11,
            aims: vec![0.0; MAX_SLOTS],
            commands: vec![],
        };
        for _ in 0..90 {
            s.step(&inp);
        }
        assert!(s.ship.docked.is_none());
        assert!(
            s.ship.vel.length() > 0.5,
            "Schiff bewegt sich: {:?}",
            s.ship.vel
        );
    }

    #[test]
    fn single_side_thrust_rotates() {
        let mut s = new_sim();
        s.ship.docked = None;
        s.ship.pos = Vec2::new(0.0, 300.0); // freier Raum
        let left_only = TickInput {
            slots: 0b1,
            aims: vec![0.0; MAX_SLOTS],
            commands: vec![],
        };
        for _ in 0..30 {
            s.step(&left_only);
        }
        // Linkes Triebwerk dreht nach rechts (im Uhrzeigersinn → negative Winkelgeschwindigkeit).
        assert!(s.ship.ang_vel < -0.1, "ang_vel = {}", s.ship.ang_vel);
    }

    #[test]
    fn idle_ship_keeps_drifting() {
        let mut s = new_sim();
        s.ship.docked = None;
        s.ship.pos = Vec2::new(0.0, 300.0);
        s.ship.vel = Vec2::new(3.0, 0.0);
        s.step(&TickInput::default());
        let v0 = s.ship.vel;
        for _ in 0..120 {
            s.step(&TickInput::default());
        }
        assert!(
            (s.ship.vel - v0).length() < 0.05,
            "Trägheit: {:?} → {:?}",
            v0,
            s.ship.vel
        );
    }
}
