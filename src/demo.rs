//! Automatischer Vorführmodus für Screenshots und Rauchtests:
//! `DRIFTCREW_DEMO=<ordner>` fliegt ein Skript ab, speichert Bildschirmfotos und
//! beendet sich danach. `DRIFTCREW_SCENE=tour|ui` wählt das Skript.

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
    let script = if demo.cfg.scene == "ui" {
        ui_scene()
    } else {
        tour()
    };
    let mut pad = pads.iter().next();
    if pad.is_none() && demo.cfg.scene == "ui" {
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
