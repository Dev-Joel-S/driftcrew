//! Deterministische Simulation ohne Rendering-Abhängigkeit.
//!
//! * feste Schrittweite (60 Hz), keine Abhängigkeit von der Framerate
//! * Eingabe pro Tick = Bitmaske der Slots + Zielwinkel der Werkzeuge + Befehle
//! * Zufall nur über den geseedeten [`rng::Rng`]
//! * nur `Vec`s mit fester Reihenfolge, keine Hash-Iteration
//!
//! Die Simulation weiß nicht, wer gedrückt hat – nur welcher Slot.

pub mod data;
pub mod dock;
pub mod economy;
pub mod geom;
pub mod hazards;
pub mod missions;
pub mod physics;
pub mod rng;
pub mod ship;
pub mod tools;
pub mod world;

use std::sync::Arc;

use bevy::math::Vec2;

use data::{CrewSave, GameData, Ore};
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
    Buy { purchase: Purchase, voter: u8 },
    /// Stimme setzen: Some(true) = ja, Some(false) = nein.
    Vote { voter: u8, yes: bool },
    AcceptMission { id: u32 },
    AbandonMission { id: u32 },
    SellOre,
    SwitchShip { id: String },
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
    Impact { pos: Vec2, normal: Vec2, strength: f32 },
    ShieldHit { pos: Vec2 },
    Damage { amount: f32 },
    Explosion { pos: Vec2, size: f32, color: [f32; 3] },
    Shot { pos: Vec2, dir: Vec2 },
    EmptyGun,
    ProjectileHit { pos: Vec2 },
    CraneFire,
    CraneAttach { pos: Vec2 },
    CraneRelease,
    Stowed { what: String },
    Docked { pad: usize },
    Undocked,
    Purchased { name: String },
    VoteStarted,
    VoteEnded { passed: bool, label: String },
    MissionAccepted { id: u32 },
    MissionCompleted { id: u32, reward: u32 },
    MissionFailed { id: u32 },
    Toast { text: String, kind: ToastKind },
    ShipDestroyed,
    Respawned,
    /// Ein neues Schiff wurde gekauft/gewählt: Slots müssen neu verteilt werden.
    ShipChanged,
    Sold { credits: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum BodyKind {
    Asteroid { ore: Option<Ore>, ore_left: f32, hp: f32, field: usize },
    Meteor,
    OreChunk { ore: Ore, amount: f32 },
    Capsule { mission: u32 },
    Derelict { mission: u32, name: String },
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
        0.5 * self.mass * self.radius * self.radius
    }
    pub fn grabbable(&self) -> bool {
        !matches!(self.kind, BodyKind::Meteor)
    }
    /// Klein genug, um eingeholt und eingelagert zu werden.
    pub fn stowable(&self) -> bool {
        matches!(self.kind, BodyKind::OreChunk { .. } | BodyKind::Capsule { .. })
    }
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub id: u32,
    pub pos: Vec2,
    pub prev_pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
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
}

impl SimState {
    pub fn new(data: Arc<GameData>, save: &CrewSave, loadout: Loadout, crew_size: u8) -> SimState {
        let mut rng = Rng::new(data.world.seed);
        let world = World::build(&data, &mut rng);
        let home = data.station_index(&save.home_station).unwrap_or(0);
        let crew = Crew {
            size: crew_size.max(1),
            credits: save.credits,
            upgrades: save.upgrades.clone(),
            owned_ships: save.owned_ships.clone(),
            current_ship: save.current_ship.clone(),
            home_station: home,
            missions_done: save.missions_done,
            ore_sold: save.ore_sold,
        };
        let stats = economy::stats_for(&data, &crew.upgrades);
        let ship = Ship::build(data.ship(&crew.current_ship), &loadout, &stats);
        let n_zones = data.world.meteor_zones.len();
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
        };
        s.populate_fields();
        s.refresh_offers();
        s.dock_at_station(home);
        s.events.clear();
        s
    }

    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.events.push(SimEvent::Toast { text: text.into(), kind });
    }

    /// Schiff mit neuer Slot-Belegung neu bauen (nach der Lobby). Zustand bleibt erhalten.
    pub fn rebuild_ship(&mut self, loadout: Loadout) {
        let stats = economy::stats_for(&self.data, &self.crew.upgrades);
        let def = self.data.ship(&self.crew.current_ship).clone();
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
        }
        // Fracht übernehmen, soweit Platz ist.
        for item in old.cargo {
            ship.store(item.kind, item.mass);
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
        CrewSave {
            version: 1,
            credits: self.crew.credits,
            upgrades: self.crew.upgrades.clone(),
            owned_ships: self.crew.owned_ships.clone(),
            current_ship: self.crew.current_ship.clone(),
            home_station: self.world.stations[self.crew.home_station].id.clone(),
            missions_done: self.crew.missions_done,
            ore_sold: self.crew.ore_sold,
        }
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
        } else {
            self.apply_input(input);
        }
        self.apply_forces();
        self.integrate();
        self.collide();
        self.update_projectiles();
        if !self.ship.destroyed {
            self.update_docking(input);
        }
        self.update_hazards();
        self.update_missions();
        self.check_ship_health();

        self.bodies.retain(|b| b.alive);
        self.projectiles.retain(|p| p.life > 0.0);
    }

    /// Momentane Belegung eines Slots (für Anzeige).
    pub fn slot_label(&self, slot: u8) -> String {
        if let Some(t) = self.ship.thrusters.iter().find(|t| t.slot == slot) {
            let n = self.ship.thrusters.len();
            return thruster_label(t.slot as usize, n);
        }
        if let Some(t) = self.ship.tools.iter().find(|t| t.slot == slot) {
            return t.kind.label().to_string();
        }
        "?".into()
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
            s.bodies.iter().fold(0u32, |a, b| a.wrapping_add(b.pos.x.to_bits()).rotate_left(3)),
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
        if t % 90 == 0 {
            slots |= 1 << 5;
        }
        if t % 120 == 0 {
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
        assert!(s.ship.vel.length() > 0.5, "Schiff bewegt sich: {:?}", s.ship.vel);
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
        assert!((s.ship.vel - v0).length() < 0.05, "Trägheit: {:?} → {:?}", v0, s.ship.vel);
    }
}
