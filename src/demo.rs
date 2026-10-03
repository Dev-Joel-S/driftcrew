//! Automatischer Vorführmodus für Screenshots und Rauchtests:
//! `DRIFTCREW_DEMO=<ordner>` fliegt ein Skript ab, speichert Bildschirmfotos und
//! beendet sich danach. `DRIFTCREW_SCENE=tour|ui|systems|progress|coop|sectors|rules|rebuild` wählt das Skript.

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::game::{AppState, LobbyMode, PendingCommands, Sim};
use crate::input::{ActiveBindings, Binding, Btn, ClaimTarget, Crew, Device, MenuInput, Player};
use crate::sim::Command;
use crate::sim::economy::Purchase;
use crate::sim::geom::rot;

#[derive(Clone, Debug)]
pub struct DemoConfig {
    pub dir: String,
    pub scene: String,
}

impl DemoConfig {
    pub fn from_env() -> Option<DemoConfig> {
        let dir = std::env::var("DRIFTCREW_DEMO").ok()?;
        let scene = std::env::var("DRIFTCREW_SCENE").unwrap_or_else(|_| "tour".into());
        Some(DemoConfig { dir, scene })
    }
}

pub trait Resolution {
    fn resolution(&self) -> (u32, u32);
}

impl Resolution for Option<DemoConfig> {
    fn resolution(&self) -> (u32, u32) {
        if self.is_some() {
            (1280, 720)
        } else {
            (1600, 900)
        }
    }
}

#[derive(Resource)]
pub struct Demo {
    pub cfg: DemoConfig,
    pub t: f32,
    pub step: usize,
    pub shots: usize,
}

/// Vom Skript gedrückte Slots (wird in der Simulationsschleife dazugenommen).
#[derive(Resource, Default)]
pub struct ScriptedSlots(pub u32);

pub struct DemoPlugin(pub DemoConfig);

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Demo {
            cfg: self.0.clone(),
            t: 0.0,
            step: 0,
            shots: 0,
        })
        .init_resource::<ScriptedSlots>()
        .add_systems(
            PreUpdate,
            demo_script
                .after(bevy::input::InputSystems)
                .after(crate::input::read_menu_input),
        );
    }
}

struct Ctx<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    demo: &'a mut Demo,
    sim: &'a mut Sim,
    slots: &'a mut ScriptedSlots,
    menu: &'a mut MenuInput,
    crew: &'a mut Crew,
    active: &'a mut ActiveBindings,
    next: &'a mut NextState<AppState>,
    mode: &'a mut LobbyMode,
    pending: &'a mut PendingCommands,
    pad: Option<Entity>,
}

type Act = fn(&mut Ctx);

fn shot(c: &mut Ctx, name: &str) {
    let path = format!("{}/{:02}_{}.png", c.demo.cfg.dir, c.demo.shots, name);
    c.demo.shots += 1;
    info!("Screenshot: {path}");
    c.commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

fn keyboard_crew(c: &mut Ctx, with_pad: bool) {
    let keys = [
        KeyCode::KeyA,
        KeyCode::KeyD,
        KeyCode::KeyS,
        KeyCode::KeyQ,
        KeyCode::KeyE,
        KeyCode::Space,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
    ];
    c.crew.players = vec![Player {
        device: Device::Keyboard,
        label: "Tastatur".into(),
    }];
    c.crew.bindings.clear();
    for (i, k) in keys.iter().enumerate() {
        if with_pad && (i == 1 || i == 4 || i == 6) {
            continue;
        }
        let target = if i < 5 {
            ClaimTarget::Thruster(i)
        } else {
            ClaimTarget::Tool(i - 5)
        };
        c.crew.bindings.push(Binding {
            btn: Btn::Key(*k),
            player: 0,
            target,
        });
    }
    if with_pad && let Some(pad) = c.pad {
        c.crew.players.push(Player {
            device: Device::Pad(pad),
            label: "Joy-Con (R)".into(),
        });
        use bevy::input::gamepad::GamepadButton as G;
        for (b, t) in [
            (G::South, ClaimTarget::Thruster(1)),
            (G::East, ClaimTarget::Thruster(4)),
            (G::RightTrigger, ClaimTarget::Tool(1)),
        ] {
            c.crew.bindings.push(Binding {
                btn: Btn::Pad(pad, b),
                player: 1,
                target: t,
            });
        }
    }
    let def = c.sim.0.data.ship(&c.sim.0.crew.current_ship).clone();
    let loadout = c.crew.loadout();
    c.sim.0.rebuild_ship(loadout);
    c.sim.0.set_crew_size(c.crew.players.len() as u8);
    c.active.0 = c.crew.resolve(&def);
}

fn teleport(c: &mut Ctx, center: Vec2, dist: f32, angle: f32) {
    let s = &mut c.sim.0;
    s.ship.docked = None;
    let p = center + rot(Vec2::X, angle) * dist;
    s.ship.pos = p;
    s.ship.prev_pos = p;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.invulnerable = 5.0;
    // Nichts im Weg lassen (nur für Vorführungen).
    s.bodies.retain(|b| (b.pos - p).length() > b.radius + 6.0);
}

fn tour() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        (3.0, |c| shot(c, "angedockt")),
        (3.8, |c| c.slots.0 = 0b11111),
        (5.0, |c| shot(c, "abheben")),
        (5.8, |c| c.slots.0 = 0b00011),
        (6.8, |c| c.slots.0 = 0b01000),
        (7.3, |c| shot(c, "drehen")),
        (7.4, |c| c.slots.0 = 0),
        (8.2, |c| teleport(c, Vec2::new(640.0, 700.0), 75.0, 0.6)),
        (10.0, |c| shot(c, "planet")),
        (10.8, |c| teleport(c, Vec2::new(260.0, 1380.0), 40.0, 1.0)),
        (12.6, |c| shot(c, "asteroiden")),
        (13.4, |c| teleport(c, Vec2::new(-1170.0, 368.0), 0.0, 0.0)),
        (15.2, |c| shot(c, "werft")),
        (16.0, |c| teleport(c, Vec2::new(980.0, -1150.0), 45.0, 2.4)),
        (17.4, |c| shot(c, "anomalie")),
        (18.2, |c| teleport(c, Vec2::new(-380.0, -520.0), 0.0, 0.0)),
        (20.5, |c| shot(c, "meteore")),
        (21.3, |c| teleport(c, Vec2::new(1250.0, -230.0), 30.0, 3.0)),
        (23.0, |c| shot(c, "kepler")),
        (24.5, |_| {}),
    ]
}

/// Schiff auf eine Plattform stellen (nur für Vorführungen).
fn land_on(c: &mut Ctx, pad: usize) {
    let s = &mut c.sim.0;
    let p = s.world.pads[pad].clone();
    s.ship.angle = p.ship_angle();
    s.ship.prev_angle = s.ship.angle;
    s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 0.02);
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.docked = Some(pad);
}

fn station(c: &Ctx, id: &str) -> usize {
    c.sim.0.data.station_index(id).unwrap_or(0)
}

/// Phase-3-Systeme: Schäden, Treibstoff, Schwarzes Loch, Kapsel, Landezone, Seil, Werft.
fn systems_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, true);
            c.next.set(AppState::Playing);
        }),
        (2.5, |c| {
            let s = &mut c.sim.0;
            s.ship.thrusters[0].health = 0.0;
            s.ship.thrusters[3].health = 0.35;
            s.ship.fuel = s.ship.max_fuel * 0.12;
            c.slots.0 = 0b11111;
        }),
        (3.6, |c| c.slots.0 = 0b10110),
        (4.6, |c| shot(c, "schaden_treibstoff")),
        (6.6, |c| {
            c.slots.0 = 0;
            let bh = c
                .sim
                .0
                .world
                .anomalies
                .iter()
                .find(|a| a.is_black_hole())
                .unwrap()
                .pos;
            teleport(c, bh, 34.0, 0.4);
        }),
        (7.4, |c| shot(c, "schwarzes_loch")),
        (10.4, |c| {
            let bh = c
                .sim
                .0
                .world
                .anomalies
                .iter()
                .find(|a| a.is_black_hole())
                .unwrap()
                .pos;
            teleport(c, bh, 4.0, 0.4);
            c.sim.0.ship.invulnerable = 0.0;
        }),
        (12.2, |c| shot(c, "rettungskapsel")),
        (17.5, |c| {
            let zone = c.sim.0.world.planets[0].zones[0];
            land_on(c, zone);
        }),
        (19.5, |c| shot(c, "landezone")),
        (21.5, |c| {
            teleport(c, Vec2::new(-300.0, 300.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.angle = 0.3;
            s.ship.vel = Vec2::new(2.0, 0.5);
            let ci = s
                .ship
                .tools
                .iter()
                .position(|t| t.kind == crate::sim::data::ToolKind::Crane)
                .unwrap();
            let mount = s.ship.tool_world_pos(ci);
            let id = s.next_id();
            let pos = mount + Vec2::new(-2.5, -4.5);
            s.bodies.push(crate::sim::Body {
                id,
                kind: crate::sim::BodyKind::Crate {
                    mission: 0,
                    name: "Druckkessel".into(),
                },
                pos,
                vel: Vec2::new(2.0, 0.5),
                angle: 0.2,
                ang_vel: 0.1,
                radius: 1.5,
                mass: 9.0,
                prev_pos: pos,
                prev_angle: 0.2,
                alive: true,
                seed: 3,
                age: 0.0,
            });
            s.ship.tools[ci].crane = crate::sim::ship::CraneState::Attached {
                body: id,
                rope: 9.0,
                local: Vec2::ZERO,
            };
        }),
        (23.0, |c| shot(c, "seil_schlaff")),
        (24.6, |c| c.slots.0 = 0b01110),
        (25.8, |c| shot(c, "seil_straff")),
        (27.8, |c| {
            c.slots.0 = 0;
            let vega = station(c, "vega");
            c.sim.0.ship.docked = None;
            c.sim.0.dock_at_station(vega);
        }),
        (30.0, |c| shot(c, "vega_service")),
        (31.8, |c| c.menu.right = true),
        (33.0, |c| shot(c, "vega_werft")),
        (34.8, |c| c.menu.right = true),
        (35.2, |c| c.menu.right = true),
        (36.4, |c| shot(c, "vega_markt")),
        (38.2, |c| {
            c.menu.tab = true;
        }),
        (39.6, |c| shot(c, "karte")),
        (41.0, |_| {}),
    ]
}

/// Phase-4-Fortschritt: Auftraggeber, Lack, Auswertung, Wracks, Kartennebel, Außenposten.
fn progress_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        // Nova-Hub: Reiter Service, Upgrades, Lack, Aufträge, Markt.
        (2.5, |c| c.menu.right = true),
        (2.9, |c| c.menu.right = true),
        (3.3, |c| c.menu.right = true),
        (4.8, |c| shot(c, "auftraggeber")),
        (6.5, |c| {
            for (part, choice) in [
                (crate::sim::economy::PaintPart::Hull, Some(4)),
                (crate::sim::economy::PaintPart::Accent, Some(7)),
                (crate::sim::economy::PaintPart::Flame, Some(2)),
            ] {
                c.pending.0.push(Command::Buy {
                    purchase: Purchase::Paint { part, choice },
                    voter: 0,
                });
            }
            c.menu.left = true;
        }),
        (8.2, |c| shot(c, "lackiererei")),
        (9.8, |c| {
            let s = &mut c.sim.0;
            let nova = s.data.station_index("nova").unwrap_or(0);
            let m = s.offers.iter().find(|m| {
                matches!(m.kind, crate::sim::missions::MissionKind::Delivery { from, .. }
                    if from == crate::sim::world::Owner::Station(nova))
            });
            if let Some(m) = m {
                c.pending.0.push(Command::AcceptMission { id: m.id });
            }
            c.slots.0 = 0b10011;
        }),
        (11.0, |c| c.slots.0 = 0b00001),
        (11.6, |c| {
            c.slots.0 = 0;
            let s = &mut c.sim.0;
            let to = s.active.iter().find_map(|m| match m.kind {
                crate::sim::missions::MissionKind::Delivery { to, .. } => Some(to),
                _ => None,
            });
            if let Some(to) = to {
                s.ship.docked = None;
                s.dock_at_station(to);
                s.on_docked(crate::sim::world::Owner::Station(to));
            }
        }),
        (13.5, |c| shot(c, "auswertung")),
        (15.5, |c| {
            teleport(c, Vec2::new(-1240.0, 1560.0), 16.0, 0.3);
            c.sim.0.ship.angle = 1.9;
        }),
        (17.5, |c| shot(c, "wracks")),
        (19.5, |c| {
            let s = &mut c.sim.0;
            let ci = s
                .ship
                .tools
                .iter()
                .position(|t| t.kind == crate::sim::data::ToolKind::Crane);
            let w = s
                .bodies
                .iter()
                .filter(|b| matches!(b.kind, crate::sim::BodyKind::Wreck { .. }))
                .min_by(|a, b| {
                    (a.pos - s.ship.pos)
                        .length()
                        .total_cmp(&(b.pos - s.ship.pos).length())
                })
                .map(|b| (b.id, b.pos));
            if let (Some(ci), Some((id, wpos))) = (ci, w) {
                let mount = s.ship.tool_world_pos(ci);
                let rope = (wpos - mount).length();
                s.ship.tools[ci].crane = crate::sim::ship::CraneState::Attached {
                    body: id,
                    rope,
                    local: Vec2::ZERO,
                };
                s.ship.vel = (mount - wpos).normalize() * 5.0;
            }
        }),
        (20.6, |c| shot(c, "bauteil")),
        (22.6, |c| c.menu.tab = true),
        (24.4, |c| shot(c, "karte_nebel")),
        (25.0, |c| c.menu.tab = true),
        (26.0, |c| {
            let pad = c.sim.0.world.planets[0].pad.unwrap_or(0);
            land_on(c, pad);
        }),
        (29.0, |c| shot(c, "aussenposten")),
        (31.0, |_| {}),
    ]
}

/// Koop: Pings zweier Spieler, Crewgröße in der Lobby.
fn coop_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, true);
            c.next.set(AppState::Playing);
        }),
        (2.5, |c| {
            let p = c.sim.0.ship.pos;
            c.pending.0.push(Command::Ping {
                player: 0,
                pos: p + Vec2::new(14.0, 9.0),
            });
            let kepler = c.sim.0.world.stations[1].pos;
            c.pending.0.push(Command::Ping {
                player: 1,
                pos: kepler,
            });
        }),
        (4.2, |c| shot(c, "ping")),
        (5.5, |c| {
            *c.mode = LobbyMode::Redistribute;
            c.next.set(AppState::Lobby);
        }),
        (7.5, |c| shot(c, "lobby_crew")),
        (8.5, |_| {}),
    ]
}

fn region_center(c: &Ctx, name: &str) -> Vec2 {
    c.sim
        .0
        .data
        .world
        .regions
        .iter()
        .find(|r| r.name == name)
        .map(|r| Vec2::new(r.center.0, r.center.1))
        .unwrap_or(Vec2::ZERO)
}

/// Phase 6: Sektoreffekte, Scanner, Ereignisse, Kartendaten.
fn sectors_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        (2.0, |c| {
            let p = region_center(c, "Sonnenwind-Korridor");
            teleport(c, p, 0.0, 0.0);
        }),
        (4.5, |c| shot(c, "sonnenwind")),
        (6.0, |c| {
            let p = region_center(c, "Schleiernebel");
            teleport(c, p, 0.0, 0.0);
        }),
        (8.0, |c| shot(c, "nebel")),
        (9.5, |c| {
            let p = region_center(c, "Splitterzone");
            teleport(c, p + Vec2::new(-250.0, -380.0), 0.0, 0.0);
        }),
        (13.0, |c| shot(c, "truemmer")),
        (14.5, |c| {
            teleport(c, Vec2::new(-1240.0, 1560.0), 60.0, 0.2);
        }),
        (16.0, |c| c.slots.0 = 1 << 8),
        (16.3, |c| c.slots.0 = 0),
        (16.9, |c| shot(c, "scanner_impuls")),
        (19.0, |c| shot(c, "scanner_funde")),
        (20.5, |c| {
            c.sim.0.event = Some(crate::sim::sector::WorldEvent {
                kind: crate::sim::sector::EventKind::SolarFlare,
                time_left: 20.0,
                dir: Vec2::ZERO,
                spawn_timer: 0.0,
            });
        }),
        (22.5, |c| shot(c, "sonneneruption")),
        (23.0, |c| {
            c.sim.0.event = None;
            c.sim.0.ship.docked = None;
            c.sim.0.dock_at_station(0);
        }),
        (24.5, |c| c.menu.left = true),
        (26.5, |c| shot(c, "kartendaten")),
        (28.0, |_| {}),
    ]
}

/// Phase 8: Funk, Passagiere, Boni.
fn rules_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        (2.5, |c| {
            let s = &mut c.sim.0;
            let nova = s.data.station_index("nova").unwrap_or(0);
            let kepler = s.data.station_index("kepler").unwrap_or(1);
            let kind = crate::sim::missions::MissionKind::Passengers {
                from: nova,
                to: kepler,
                count: 5,
                comfort: 1.0,
                aboard: false,
            };
            let par = s.par_time(&kind);
            let id = s.next_id();
            s.offers.push(crate::sim::missions::Mission {
                id,
                kind,
                reward: 420,
                origin: Some(crate::sim::world::Owner::Station(nova)),
                giver: None,
                start: None,
                top_speed: 0.0,
                max_strain: None,
                par,
            });
            c.pending.0.push(Command::AcceptMission { id });
        }),
        (3.5, |c| c.slots.0 = 0b11111),
        (4.6, |c| c.slots.0 = 0b10001),
        (5.4, |c| c.slots.0 = 0),
        (6.5, |c| {
            c.slots.0 = 0;
            shot(c, "passagiere_funk");
        }),
        (8.5, |c| {
            let kepler = c.sim.0.data.station_index("kepler").unwrap_or(1);
            let pad = c.sim.0.world.stations[kepler].pads[0];
            let p = c.sim.0.world.pads[pad].clone();
            let s = &mut c.sim.0;
            s.ship.docked = None;
            s.ship.angle = p.ship_angle();
            s.ship.prev_angle = s.ship.angle;
            s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 6.0);
            s.ship.prev_pos = s.ship.pos;
            s.ship.vel = -p.normal * 1.2;
            s.ship.ang_vel = 0.0;
        }),
        (13.0, |c| shot(c, "anflug_andocken")),
        (15.0, |_| {}),
    ]
}

fn give_ore(c: &mut Ctx, ore: crate::sim::data::Ore, t: f32) {
    c.sim.0.ship.store(crate::sim::ship::CargoKind::Ore(ore), t);
}

fn give_parts(c: &mut Ctx, n: u32) {
    for _ in 0..n {
        c.sim.0.ship.store(
            crate::sim::ship::CargoKind::Salvage {
                name: "Funkmodul".into(),
                value: 60,
            },
            1.5,
        );
    }
}

/// Phase 8b: Wiederaufbau, sperrige Bergung, Präzisionsarbeit.
fn rebuild_scene() -> Vec<(f32, Act)> {
    use crate::sim::data::Ore;
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        (2.0, |c| {
            let si = station(c, "relais");
            c.sim.0.ship.docked = None;
            c.sim.0.dock_at_station(si);
        }),
        (4.0, |c| shot(c, "relais_verstummt")),
        (5.5, |c| {
            give_ore(c, Ore::Ferrit, 8.0);
            give_parts(c, 1);
            c.pending.0.push(Command::DeliverProject);
        }),
        (7.5, |c| shot(c, "relais_notstrom")),
        (9.0, |c| {
            give_ore(c, Ore::Kobalt, 6.0);
            give_ore(c, Ore::Schrott, 6.0);
            give_parts(c, 2);
            c.pending.0.push(Command::DeliverProject);
        }),
        (10.5, |c| {
            // Auf die neue Plattform der zweiten Etappe stellen.
            let si = station(c, "relais");
            let pad = c.sim.0.world.stations[si]
                .pads
                .iter()
                .copied()
                .find(|p| c.sim.0.world.pads[*p].stage == 2);
            if let Some(pad) = pad {
                land_on(c, pad);
            }
        }),
        (12.5, |c| shot(c, "relais_ausgebaut")),
        (14.0, |c| {
            teleport(c, Vec2::new(-200.0, 600.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            let ci = s
                .ship
                .tools
                .iter()
                .position(|t| t.kind == crate::sim::data::ToolKind::Crane)
                .unwrap_or(0);
            let mount = s.ship.tool_world_pos(ci);
            let id = s.next_id();
            let pos = mount + Vec2::new(5.0, -7.0);
            s.bodies.push(crate::sim::Body {
                id,
                kind: crate::sim::BodyKind::Bulky {
                    mission: 0,
                    name: "Antennenmast".into(),
                    half_len: 4.6,
                    thick: 0.45,
                },
                pos,
                vel: Vec2::ZERO,
                angle: 0.4,
                ang_vel: 0.0,
                radius: 5.05,
                mass: 7.0,
                prev_pos: pos,
                prev_angle: 0.4,
                alive: true,
                seed: 5,
                age: 0.0,
            });
            s.ship.tools[ci].crane = crate::sim::ship::CraneState::Attached {
                body: id,
                rope: 6.0,
                local: Vec2::new(-4.6, 0.0),
            };
            c.slots.0 = 0b11111;
        }),
        (15.3, |c| c.slots.0 = 0b10110),
        (15.9, |c| shot(c, "sperrig_am_kran")),
        (16.2, |c| c.slots.0 = 0),
        (18.0, |c| {
            let s = &mut c.sim.0;
            let bi = s.bodies.iter().position(|b| {
                matches!(b.kind, crate::sim::BodyKind::Asteroid { ore: Some(_), .. })
                    && b.radius > 3.0
            });
            if let Some(bi) = bi {
                if let crate::sim::BodyKind::Asteroid { vein, .. } = &mut s.bodies[bi].kind {
                    *vein = true;
                }
                s.bodies[bi].vel = Vec2::ZERO;
                s.bodies[bi].ang_vel = 0.0;
                let id = s.bodies[bi].id;
                let target = crate::sim::precision::precision_target(&s.bodies[bi])
                    .unwrap()
                    .0;
                let out = (target - s.bodies[bi].pos).normalize();
                s.ship.docked = None;
                s.ship.angle = (-out).to_angle() - std::f32::consts::FRAC_PI_2;
                s.ship.prev_angle = s.ship.angle;
                s.ship.pos = target + out * 5.0;
                s.ship.prev_pos = s.ship.pos;
                s.ship.vel = Vec2::ZERO;
                s.ship.ang_vel = 0.0;
                s.precision = Some(crate::sim::precision::Precision {
                    body: id,
                    kind: crate::sim::precision::PrecisionKind::Vein,
                    progress: 0.62,
                    offset: 0.3,
                    idle: -100.0,
                });
            }
        }),
        (20.0, |c| shot(c, "praezision")),
        (21.5, |_| {}),
    ]
}

fn course_index(c: &Ctx, id: &str) -> usize {
    c.sim
        .0
        .data
        .courses
        .courses
        .iter()
        .position(|k| k.id == id)
        .unwrap_or(0)
}

/// Schiff kurz vor ein Tor setzen, mit Schwung in Pfeilrichtung (startet bzw. zählt das Tor).
fn approach_gate(c: &mut Ctx, course: usize, step: usize, back: f32, speed: f32) {
    use crate::sim::data::{StepKind, v};
    let StepKind::Gate { pos, dir, .. } = c.sim.0.data.courses.courses[course].steps[step].kind
    else {
        return;
    };
    let d = Vec2::new(dir.to_radians().cos(), dir.to_radians().sin());
    teleport(c, v(pos) - d * back, 0.0, 0.0);
    let s = &mut c.sim.0;
    s.ship.vel = d * speed;
    s.ship.angle = f32::atan2(-d.x, d.y);
    s.ship.prev_angle = s.ship.angle;
}

/// Phase 7: Parcours im Stationsmenü, Training (Drehen, Bremsen), Zeitrennen durch den
/// Nova-Ring, Wrackring, Ergebnis mit Bestenliste, Messflug und Lastaufnahme.
fn courses_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        (2.0, |c| {
            // Ein paar frühere Läufe, damit die Bestenliste etwas zeigt.
            use crate::sim::data::RecordEntry;
            let ring = course_index(c, "nova_ring");
            let train = course_index(c, "grundkurs");
            let s = &mut c.sim.0;
            let rec = &mut s.records[ring];
            for (i, (t, ship, crew)) in [
                (58.4, "Driftkutter", 2),
                (63.9, "Driftkutter", 3),
                (71.2, "Kolibri", 1),
            ]
            .into_iter()
            .enumerate()
            {
                rec.entries.push(RecordEntry {
                    total: t,
                    penalty: if i == 1 { 4.0 } else { 0.0 },
                    ship: ship.into(),
                    crew,
                    run: i as u32 + 1,
                });
            }
            rec.finished = 3;
            rec.medal = 2;
            s.records[train].finished = 1;
        }),
        (2.4, |c| c.menu.right = true),
        (2.7, |c| c.menu.right = true),
        (3.0, |c| c.menu.right = true),
        (3.3, |c| c.menu.right = true),
        (3.6, |c| c.menu.right = true),
        (5.0, |c| shot(c, "parcours_menue")),
        (6.0, |c| {
            let ci = course_index(c, "grundkurs");
            approach_gate(c, ci, 0, 5.0, 9.0);
        }),
        (7.0, |c| {
            // Nach dem Starttor: Boje anpeilen, Nase noch etwas daneben.
            let s = &mut c.sim.0;
            s.ship.vel = Vec2::new(0.0, -1.0);
            s.ship.ang_vel = 0.0;
            s.ship.angle = 1.1;
            s.ship.prev_angle = 1.1;
            if let Some(r) = s.course.as_mut() {
                r.progress = 0.0;
            }
        }),
        (8.4, |c| shot(c, "training_drehen")),
        (9.4, |c| {
            use crate::sim::data::{StepKind, v};
            let ci = course_index(c, "grundkurs");
            let StepKind::Hold { pos, .. } = c.sim.0.data.courses.courses[ci].steps[3].kind else {
                return;
            };
            teleport(c, v(pos) + Vec2::new(-3.0, 2.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.angle = 2.0;
            s.ship.prev_angle = 2.0;
            if let Some(r) = s.course.as_mut() {
                r.step = 3;
                r.time = 38.6;
                r.progress = 0.4;
            }
        }),
        (10.4, |c| shot(c, "training_bremsen")),
        (11.2, |c| {
            c.pending.0.push(Command::AbortCourse);
            let ci = course_index(c, "nova_ring");
            approach_gate(c, ci, 0, 4.0, 12.0);
        }),
        (12.0, |c| {
            let ci = course_index(c, "nova_ring");
            approach_gate(c, ci, 3, 22.0, 11.0);
            let s = &mut c.sim.0;
            if let Some(r) = s.course.as_mut() {
                r.step = 3;
                r.time = 21.4;
            }
        }),
        (13.0, |c| shot(c, "nova_ring")),
        (14.0, |c| {
            // Wrackring: vor der Öffnung, der Lauf ist unterwegs.
            c.pending.0.push(Command::AbortCourse);
            let ci = course_index(c, "wrackring");
            approach_gate(c, ci, 0, 4.0, 9.0);
        }),
        (14.8, |c| {
            let ci = course_index(c, "wrackring");
            let center = c
                .sim
                .0
                .step_target(ci, 1)
                .unwrap_or(Vec2::new(-1050.0, 1400.0));
            teleport(c, center + Vec2::new(-34.0, 0.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.vel = Vec2::new(2.0, 0.0);
            s.ship.angle = -std::f32::consts::FRAC_PI_2;
            s.ship.prev_angle = s.ship.angle;
            if let Some(r) = s.course.as_mut() {
                r.time = 6.8;
            }
        }),
        (16.6, |c| shot(c, "wrackring")),
        (17.4, |c| {
            c.pending.0.push(Command::AbortCourse);
        }),
        (17.6, |c| {
            // Messflug am Rand der Anomalie: der Sog zieht, die Crew hält dagegen.
            use crate::sim::missions::{Mission, MissionKind};
            let s = &mut c.sim.0;
            let site = s
                .data
                .courses
                .survey_sites
                .iter()
                .position(|x| x.name == "Anomalierand")
                .unwrap_or(0);
            let id = s.next_id();
            s.active.push(Mission {
                id,
                kind: MissionKind::Survey {
                    sites: vec![site, 0],
                    done: 0,
                    hold: 3.4,
                },
                reward: 380,
                origin: Some(crate::sim::world::Owner::Station(0)),
                giver: None,
                start: Some(Box::new(s.stats.clone())),
                top_speed: 0.0,
                max_strain: None,
                par: 420.0,
            });
            let p = crate::sim::data::v(s.data.courses.survey_sites[site].pos);
            teleport(c, p + Vec2::new(1.5, -1.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.angle = -2.6;
            s.ship.prev_angle = s.ship.angle;
        }),
        (18.4, |c| shot(c, "messflug")),
        (19.4, |c| {
            // Schwerlast an Kepler: Kiste am Seil, die Lastaufnahme wartet.
            use crate::sim::missions::{Mission, MissionKind};
            let kepler = station(c, "kepler");
            let Some(sock) = c.sim.0.world.stations[kepler].socket else {
                return;
            };
            c.sim.0.active.clear();
            let ship_at = sock.center + sock.open * 16.0 + Vec2::new(0.0, 4.0);
            teleport(c, ship_at, 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.angle = std::f32::consts::FRAC_PI_2;
            s.ship.prev_angle = s.ship.angle;
            let mid = s.next_id();
            let bid = s.next_id();
            let crate_at = sock.center + sock.open * 6.5 + Vec2::new(0.0, -1.0);
            s.bodies.push(crate::sim::Body {
                id: bid,
                kind: crate::sim::BodyKind::Crate {
                    mission: mid,
                    name: "Druckkessel".into(),
                },
                pos: crate_at,
                vel: Vec2::ZERO,
                angle: 0.2,
                ang_vel: 0.0,
                radius: crate::sim::missions::CRATE_RADIUS,
                mass: 9.0,
                prev_pos: crate_at,
                prev_angle: 0.2,
                alive: true,
                seed: 3,
                age: 0.0,
            });
            s.active.push(Mission {
                id: mid,
                kind: MissionKind::Haul {
                    from: 0,
                    to: kepler,
                    cargo: "Druckkessel".into(),
                    mass: 9.0,
                    body: Some(bid),
                    settle: 0.0,
                },
                reward: 340,
                origin: Some(crate::sim::world::Owner::Station(0)),
                giver: None,
                start: Some(Box::new(s.stats.clone())),
                top_speed: 0.0,
                max_strain: None,
                par: 300.0,
            });
            let ci = s
                .ship
                .tools
                .iter()
                .position(|t| t.kind == crate::sim::data::ToolKind::Crane)
                .unwrap_or(0);
            let mount = s.ship.tool_world_pos(ci);
            s.ship.tools[ci].crane = crate::sim::ship::CraneState::Attached {
                body: bid,
                rope: (crate_at - mount).length(),
                local: Vec2::ZERO,
            };
        }),
        (21.0, |c| shot(c, "lastaufnahme")),
        (21.6, |c| {
            c.sim.0.active.clear();
            c.sim
                .0
                .bodies
                .retain(|b| !matches!(b.kind, crate::sim::BodyKind::Crate { .. }));
            for t in &mut c.sim.0.ship.tools {
                t.crane = crate::sim::ship::CraneState::Idle;
            }
        }),
        (22.0, |c| {
            // Nova-Ring: letztes Tor, dann Ziel.
            c.pending.0.push(Command::AbortCourse);
            let ci = course_index(c, "nova_ring");
            approach_gate(c, ci, 0, 4.0, 12.0);
        }),
        (22.8, |c| {
            let ci = course_index(c, "nova_ring");
            let last = c.sim.0.data.courses.courses[ci].steps.len() - 1;
            if let Some(r) = c.sim.0.course.as_mut() {
                r.step = last;
                r.time = 55.6;
            }
            approach_gate(c, ci, last, 6.0, 14.0);
        }),
        (24.4, |c| shot(c, "parcours_ziel")),
        (25.4, |_| {}),
    ]
}

/// Phase 9: Kredit in der Werft, Markt mit Preistrend und Nachfrage, Abrechnung nach dem
/// Auftrag (Versicherung, Kreditrate, Kosten unterwegs).
fn finance_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        (2.0, |c| {
            let orion = station(c, "werft");
            let s = &mut c.sim.0;
            s.crew.credits = 900;
            s.ship.docked = None;
            s.dock_at_station(orion);
        }),
        (3.0, |c| c.menu.right = true),
        (3.6, |c| c.menu.down = true),
        (3.9, |c| c.menu.down = true),
        (4.2, |c| c.menu.down = true),
        (4.5, |c| c.menu.down = true),
        (5.6, |c| shot(c, "werft_kredit")),
        (6.2, |c| {
            // Lastesel auf Kredit, dann nach Kepler: dort wurde gerade viel Kobalt verkauft,
            // und Ionit ist gefragt.
            use crate::sim::data::Ore;
            use crate::sim::world::Owner;
            let s = &mut c.sim.0;
            let price = s.data.ship("lastesel").price;
            let (down, rest, rate) = s.loan_terms(price);
            s.crew.credits -= down;
            s.crew.owned_ships.push("lastesel".into());
            s.crew.loan = Some(crate::sim::finance::Loan {
                ship: "lastesel".into(),
                left: rest,
                installment: rate,
            });
            s.crew.insured = true;
            let kepler = s.data.station_index("kepler").unwrap_or(0);
            s.note_sale(Owner::Station(kepler), Ore::Kobalt, 9.0);
            s.demand = Some(crate::sim::finance::Demand {
                owner: Owner::Station(kepler),
                ore: Ore::Ionit,
                left: 240.0,
            });
            let slot = s.market_slot(Owner::Station(kepler));
            s.market[slot][3] = 1.28;
            s.ship.docked = None;
            s.dock_at_station(kepler);
        }),
        (7.4, |c| c.menu.right = true),
        (7.8, |c| c.menu.right = true),
        (8.1, |c| c.menu.down = true),
        (8.3, |c| c.menu.down = true),
        (8.5, |c| c.menu.down = true),
        (8.7, |c| c.menu.down = true),
        (8.9, |c| c.menu.down = true),
        (9.8, |c| shot(c, "markt_trend")),
        (10.4, |c| {
            // Ein Auftrag nach Nova, unterwegs Reparatur an Kepler – dann Abrechnung.
            use crate::sim::missions::{Mission, MissionKind};
            use crate::sim::world::Owner;
            let nova = station(c, "nova");
            let kepler = station(c, "kepler");
            let s = &mut c.sim.0;
            let mut start = s.stats.clone();
            start.time -= 160.0;
            // Unterwegs: Dockgebühr in Kepler und eine Reparatur.
            s.stats.expenses.dock += 18;
            s.stats.expenses.service += 45;
            let id = s.next_id();
            s.active.push(Mission {
                id,
                kind: MissionKind::Delivery {
                    from: Owner::Station(kepler),
                    to: nova,
                    cargo: "Ersatzteile".into(),
                    mass: 4.0,
                },
                reward: 420,
                origin: Some(Owner::Station(kepler)),
                giver: None,
                start: Some(Box::new(start)),
                top_speed: 0.0,
                max_strain: None,
                par: 220.0,
            });
            let pad = s.world.stations[nova].pads[2];
            s.time += 500.0;
            s.ship.docked = None;
            s.dock(pad);
        }),
        (12.4, |c| shot(c, "abrechnung")),
        (13.4, |_| {}),
    ]
}

/// Phase 10: Triebwerk im Takt flicken, Notreparatur der Hülle, Andockport hacken,
/// Ersatzteil mit dem Kran einsetzen.
fn minigame_scene() -> Vec<(f32, Act)> {
    vec![
        (0.5, |c| {
            keyboard_crew(c, false);
            c.next.set(AppState::Playing);
        }),
        (2.0, |c| {
            teleport(c, Vec2::new(-300.0, -260.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.thrusters[0].health = 0.0;
            s.repair.thrusters = vec![0.0; s.ship.thrusters.len()];
            s.repair.thrusters[0] = 3.0 / crate::sim::minigame::PATCH_HITS;
            let slot = s.ship.thrusters[0].slot;
            s.repair.beats = vec![
                crate::sim::minigame::Beat {
                    slot,
                    hit: true,
                    age: 0.2,
                },
                crate::sim::minigame::Beat {
                    slot,
                    hit: false,
                    age: 0.5,
                },
                crate::sim::minigame::Beat {
                    slot,
                    hit: true,
                    age: 0.9,
                },
            ];
            s.ship.vel = Vec2::new(0.8, 0.2);
        }),
        (4.8, |c| {
            let s = &mut c.sim.0;
            let slot = s.ship.thrusters[0].slot;
            s.repair.beats = vec![
                crate::sim::minigame::Beat {
                    slot,
                    hit: true,
                    age: 0.6,
                },
                crate::sim::minigame::Beat {
                    slot,
                    hit: false,
                    age: 0.4,
                },
                crate::sim::minigame::Beat {
                    slot,
                    hit: true,
                    age: 0.1,
                },
            ];
        }),
        (5.0, |c| shot(c, "takt_triebwerk")),
        (5.8, |c| {
            // Notreparatur: Hülle angeschlagen, Schiff ruht, die Crew tippt im Takt.
            teleport(c, Vec2::new(-300.0, -260.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.thrusters[0].health = 1.0;
            s.ship.hull = s.ship.max_hull * 0.38;
            s.repair.active = true;
            s.repair.held = vec![0.0; crate::sim::MAX_SLOTS];
            let a = s.ship.thrusters[1].slot;
            let b = s.ship.thrusters[3].slot;
            let t = s.ship.tools[0].slot;
            s.repair.beats = vec![
                crate::sim::minigame::Beat {
                    slot: a,
                    hit: true,
                    age: 0.1,
                },
                crate::sim::minigame::Beat {
                    slot: b,
                    hit: true,
                    age: 0.3,
                },
                crate::sim::minigame::Beat {
                    slot: t,
                    hit: false,
                    age: 0.6,
                },
                crate::sim::minigame::Beat {
                    slot: a,
                    hit: true,
                    age: 0.8,
                },
            ];
        }),
        (6.8, |c| {
            let s = &mut c.sim.0;
            let a = s.ship.thrusters[1].slot;
            let b = s.ship.thrusters[3].slot;
            let t = s.ship.tools[0].slot;
            s.repair.beats = vec![
                crate::sim::minigame::Beat {
                    slot: a,
                    hit: true,
                    age: 0.7,
                },
                crate::sim::minigame::Beat {
                    slot: b,
                    hit: true,
                    age: 0.5,
                },
                crate::sim::minigame::Beat {
                    slot: t,
                    hit: false,
                    age: 0.3,
                },
                crate::sim::minigame::Beat {
                    slot: a,
                    hit: true,
                    age: 0.1,
                },
            ];
        }),
        (7.0, |c| shot(c, "notreparatur")),
        (7.8, |c| {
            // Nebelhafen: in Portnähe beginnt der Hack, zwei Tasten sind schon richtig.
            c.sim.0.repair = Default::default();
            c.sim.0.ship.hull = c.sim.0.ship.max_hull;
            let si = station(c, "nebelhafen");
            let pad = c.sim.0.world.stations[si].pads[1];
            let p = c.sim.0.world.pads[pad].clone();
            teleport(c, p.center + p.normal * 13.0, 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.angle = p.ship_angle();
            s.ship.prev_angle = s.ship.angle;
            s.explored.reveal(s.ship.pos, 300.0);
        }),
        (8.4, |c| {
            if let Some(h) = c.sim.0.hack.as_mut() {
                h.done = 2;
                h.left = h.total * 0.55;
            }
        }),
        (9.2, |c| shot(c, "port_hacken")),
        (10.0, |c| {
            // Ersatzteil mit dem Kran an das ausgefallene Triebwerk halten.
            c.sim.0.hack = None;
            teleport(c, Vec2::new(-300.0, -260.0), 0.0, 0.0);
            let s = &mut c.sim.0;
            s.ship.thrusters[4].health = 0.0;
            s.ship.store(
                crate::sim::ship::CargoKind::Salvage {
                    name: "Kühlrippe".into(),
                    value: 40,
                },
                0.5,
            );
            let crane = s
                .ship
                .tools
                .iter()
                .position(|t| t.kind == crate::sim::data::ToolKind::Crane)
                .unwrap_or(0);
            s.ship.tools[crane].crane = crate::sim::ship::CraneState::Patching {
                thruster: 4,
                progress: 0.55,
            };
        }),
        (10.05, |c| {
            // Kran-Taste gedrückt halten (sonst bricht die Notreparatur ab).
            let slot = c
                .sim
                .0
                .ship
                .tools
                .iter()
                .find(|t| t.kind == crate::sim::data::ToolKind::Crane)
                .map(|t| t.slot)
                .unwrap_or(0);
            c.slots.0 = 1 << slot;
        }),
        (10.5, |c| shot(c, "kran_ersatzteil")),
        (11.0, |c| c.slots.0 = 0),
        (11.5, |_| {}),
    ]
}

fn ui_scene() -> Vec<(f32, Act)> {
    vec![
        (2.5, |c| shot(c, "titel")),
        (2.7, |c| {
            *c.mode = LobbyMode::Initial;
            c.next.set(AppState::Lobby);
        }),
        (3.5, |c| keyboard_crew(c, true)),
        (5.0, |c| shot(c, "lobby")),
        (5.2, |c| c.menu.enter = true),
        (7.5, |c| shot(c, "station")),
        (7.7, |c| c.menu.right = true),
        (8.0, |c| c.menu.right = true),
        (8.6, |c| shot(c, "auftraege")),
        (8.8, |c| {
            c.pending.0.push(Command::Buy {
                purchase: Purchase::Upgrade("mag".into()),
                voter: 0,
            })
        }),
        (9.8, |c| shot(c, "abstimmung")),
        (10.0, |c| {
            c.pending.0.push(Command::Vote {
                voter: 1,
                yes: true,
            })
        }),
        (10.4, |c| c.menu.tab = true),
        (11.6, |c| shot(c, "karte")),
        (11.8, |c| c.menu.tab = true),
        (12.0, |c| c.slots.0 = 0b11111),
        (12.9, |c| c.slots.0 = 0b10001),
        (13.6, |c| c.slots.0 = 0),
        (14.6, |c| shot(c, "flug_hud")),
        (14.8, |c| c.menu.escape = true),
        (15.6, |c| shot(c, "pause")),
        (16.5, |_| {}),
    ]
}

#[allow(clippy::too_many_arguments)]
fn demo_script(
    mut commands: Commands,
    time: Res<Time>,
    mut demo: ResMut<Demo>,
    mut sim: ResMut<Sim>,
    mut slots: ResMut<ScriptedSlots>,
    mut menu: ResMut<MenuInput>,
    mut crew: ResMut<Crew>,
    mut active: ResMut<ActiveBindings>,
    mut next: ResMut<NextState<AppState>>,
    mut mode: ResMut<LobbyMode>,
    mut pending: ResMut<PendingCommands>,
    pads: Query<Entity, With<Gamepad>>,
    mut exit: MessageWriter<AppExit>,
) {
    demo.t += time.delta_secs();
    let t = demo.t;
    let script = match demo.cfg.scene.as_str() {
        "ui" => ui_scene(),
        "systems" => systems_scene(),
        "progress" => progress_scene(),
        "coop" => coop_scene(),
        "sectors" => sectors_scene(),
        "rules" => rules_scene(),
        "rebuild" => rebuild_scene(),
        "courses" => courses_scene(),
        "finance" => finance_scene(),
        "minigames" => minigame_scene(),
        _ => tour(),
    };
    let mut pad = pads.iter().next();
    if pad.is_none() && demo.cfg.scene != "tour" {
        // Ein Gamepad vortäuschen, damit die Lobby zwei Crewmitglieder zeigt.
        pad = Some(
            commands
                .spawn((Gamepad::default(), Name::new("Joy-Con (R)")))
                .id(),
        );
    }
    while demo.step < script.len() && t >= script[demo.step].0 {
        let f = script[demo.step].1;
        let mut ctx = Ctx {
            commands: &mut commands,
            demo: &mut demo,
            sim: &mut sim,
            slots: &mut slots,
            menu: &mut menu,
            crew: &mut crew,
            active: &mut active,
            next: &mut next,
            mode: &mut mode,
            pending: &mut pending,
            pad,
        };
        f(&mut ctx);
        demo.step += 1;
    }
    if demo.step >= script.len() && t > script.last().map(|s| s.0).unwrap_or(0.0) + 1.0 {
        exit.write(AppExit::Success);
    }
}
