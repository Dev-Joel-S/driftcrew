//! Spielzustand, feste Simulationsschleife (60 Hz), Spielstand.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;

use crate::input::{build_tick_input, ActiveBindings, Aims, Crew, InputLatch};
use crate::sim::data::{CrewSave, GameData};
use crate::sim::ship::Loadout;
use crate::sim::{Command, SimEvent, SimState, TICK_HZ};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Title,
    Lobby,
    Playing,
}

/// Wofür die Lobby gerade geöffnet ist.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LobbyMode {
    #[default]
    Initial,
    Redistribute,
    NewShip,
}

#[derive(Resource)]
pub struct Data(pub Arc<GameData>);

#[derive(Resource)]
pub struct Sim(pub SimState);

#[derive(Resource, Default)]
pub struct Paused(pub bool);

#[derive(Resource, Default)]
pub struct MapOpen(pub bool);

#[derive(Resource, Default)]
pub struct PendingCommands(pub Vec<Command>);

#[derive(Resource, Default)]
pub struct HasSave(pub bool);

/// Ereignisse der Simulation für Effekte, Sound und UI.
#[derive(Message, Clone, Debug)]
pub struct SimMsg(pub SimEvent);

#[derive(Component)]
pub struct GameCamera;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        let data = match GameData::load() {
            Ok(d) => Arc::new(d),
            Err(e) => panic!("Spieldaten fehlerhaft: {e}"),
        };
        let (save, has_save) = match load_save() {
            Some(s) => (s, true),
            None => (CrewSave::new_game(&data), false),
        };
        let def = data.ship(&save.current_ship).clone();
        let sim = SimState::new(data.clone(), &save, Loadout::full(&def), 1);

        app.insert_resource(Data(data))
            .insert_resource(Sim(sim))
            .insert_resource(HasSave(has_save))
            .insert_resource(Time::<Fixed>::from_hz(TICK_HZ))
            .init_state::<AppState>()
            .init_resource::<LobbyMode>()
            .init_resource::<Paused>()
            .init_resource::<MapOpen>()
            .init_resource::<PendingCommands>()
            .add_message::<SimMsg>()
            .add_systems(
                FixedUpdate,
                fixed_tick.run_if(in_state(AppState::Playing).and(|p: Res<Paused>| !p.0)),
            )
            .add_systems(OnEnter(AppState::Playing), |mut p: ResMut<Paused>, mut m: ResMut<MapOpen>| {
                p.0 = false;
                m.0 = false;
            })
            .add_systems(Update, react_to_events.run_if(in_state(AppState::Playing)));
    }
}

#[allow(clippy::too_many_arguments)]
fn fixed_tick(
    mut sim: ResMut<Sim>,
    bindings: Res<ActiveBindings>,
    mut latch: ResMut<InputLatch>,
    aims: Res<Aims>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    mut pending: ResMut<PendingCommands>,
    scripted: Option<Res<crate::demo::ScriptedSlots>>,
    mut out: MessageWriter<SimMsg>,
) {
    let mut input = build_tick_input(&bindings, &mut latch, &aims, &keys, &mouse, &pads);
    input.commands = std::mem::take(&mut pending.0);
    if let Some(s) = scripted {
        input.slots |= s.0;
    }
    sim.0.step(&input);
    for e in sim.0.events.iter() {
        out.write(SimMsg(e.clone()));
    }
}

fn react_to_events(
    mut events: MessageReader<SimMsg>,
    sim: Res<Sim>,
    mut next: ResMut<NextState<AppState>>,
    mut mode: ResMut<LobbyMode>,
    mut crew: ResMut<Crew>,
) {
    let mut save = false;
    for SimMsg(e) in events.read() {
        match e {
            SimEvent::ShipChanged => {
                crew.bindings.clear();
                *mode = LobbyMode::NewShip;
                next.set(AppState::Lobby);
                save = true;
            }
            SimEvent::Docked { .. }
            | SimEvent::Purchased { .. }
            | SimEvent::MissionCompleted { .. }
            | SimEvent::Sold { .. }
            | SimEvent::Respawned => save = true,
            _ => {}
        }
    }
    if save {
        write_save(&sim.0.to_save());
    }
}

// ---------------------------------------------------------------------------
// Spielstand
// ---------------------------------------------------------------------------

pub fn save_path() -> PathBuf {
    if let Ok(p) = std::env::var("DRIFTCREW_SAVE") {
        return PathBuf::from(p);
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").map(PathBuf::from).ok()
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
            .ok()
    } else {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .ok()
    };
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("driftcrew")
        .join("crew.ron")
}

pub fn load_save() -> Option<CrewSave> {
    let text = std::fs::read_to_string(save_path()).ok()?;
    ron::from_str(&text).ok()
}

pub fn write_save(save: &CrewSave) {
    let path = save_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match ron::ser::to_string_pretty(save, ron::ser::PrettyConfig::default()) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                warn!("Spielstand konnte nicht gespeichert werden: {e}");
            }
        }
        Err(e) => warn!("Spielstand konnte nicht serialisiert werden: {e}"),
    }
}
