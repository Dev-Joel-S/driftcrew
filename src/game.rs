//! Spielzustand, feste Simulationsschleife (60 Hz), Spielstand.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;

use crate::input::{ActiveBindings, Aims, Crew, InputLatch, build_tick_input};
use crate::sim::data::{CrewSave, GameData};
use crate::sim::replay::{Clip, REPLAY_SECONDS, Recorder};
use crate::sim::ship::Loadout;
use crate::sim::{Command, DT, SimEvent, SimState, TICK_HZ, TickInput};

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

/// Unfall-Wiederholung (80): Aufzeichnung, Angebot nach einem Unfall, laufende Wiedergabe.
#[derive(Resource, Default)]
pub struct Replay {
    pub rec: Recorder,
    /// Ticks bis der Ausschnitt nach einem Unfall genommen wird (der Knall soll mit drauf).
    pub capture_in: u32,
    /// Angebotener Ausschnitt und wie lange das Angebot noch steht (Sekunden).
    pub offer: Option<Clip>,
    pub offer_left: f32,
    pub play: Option<Playback>,
}

/// Laufende Wiedergabe: der echte Zustand wartet, die Kopie spielt die Eingaben noch einmal ab.
pub struct Playback {
    pub real: SimState,
    pub clip: Clip,
    pub idx: usize,
    pub frame: u32,
    /// Nach dem letzten aufgezeichneten Tick noch kurz weiterlaufen lassen.
    pub hold: f32,
}

impl Playback {
    /// Sekunden bis zum Ende der Aufzeichnung (für die Anzeige „−3,2 s“).
    pub fn left(&self) -> f32 {
        (self.clip.inputs.len() - self.idx.min(self.clip.inputs.len())) as f32 / TICK_HZ as f32
    }
    pub fn progress(&self) -> f32 {
        self.idx as f32 / self.clip.inputs.len().max(1) as f32
    }
    /// Die letzten 1,5 s laufen in halber Geschwindigkeit.
    pub fn slow(&self) -> bool {
        self.left() < 1.5
    }
}

impl Replay {
    /// Wiedergabe starten: echten Zustand beiseitelegen, Ausschnitt laden.
    pub fn start(&mut self, sim: &mut SimState) {
        let Some(clip) = self.offer.take() else {
            return;
        };
        self.offer_left = 0.0;
        let real = std::mem::replace(sim, clip.start.clone());
        self.play = Some(Playback {
            real,
            clip,
            idx: 0,
            frame: 0,
            hold: 1.2,
        });
    }

    /// Wiedergabe beenden (oder überspringen): zurück zum echten Zustand.
    pub fn stop(&mut self, sim: &mut SimState) {
        if let Some(p) = self.play.take() {
            *sim = p.real;
        }
    }

    /// Der echte Spielzustand (auch während einer Wiedergabe).
    pub fn real<'a>(&'a self, sim: &'a SimState) -> &'a SimState {
        self.play.as_ref().map_or(sim, |p| &p.real)
    }
}

/// Was eine Wiedergabe an Ereignissen weitergibt: nur Bild und Ton, keine Meldungen, kein
/// Speichern, keine Zustandswechsel.
fn replay_visible(e: &SimEvent) -> bool {
    matches!(
        e,
        SimEvent::Impact { .. }
            | SimEvent::ShieldHit { .. }
            | SimEvent::Damage { .. }
            | SimEvent::Explosion { .. }
            | SimEvent::Shot { .. }
            | SimEvent::ProjectileHit { .. }
            | SimEvent::CraneFire
            | SimEvent::CraneAttach { .. }
            | SimEvent::CraneRelease
            | SimEvent::ShipDestroyed
            | SimEvent::DroneDown { .. }
            | SimEvent::Jettisoned { .. }
    )
}

/// Ereignisse der Simulation für Effekte, Sound und UI.
#[derive(Message, Clone, Debug)]
pub struct SimMsg(pub SimEvent);

#[derive(Component)]
pub struct GameCamera;

pub struct GamePlugin;

/// Seed für die Geschichte eines neuen Spielstands (wo die Artefakte liegen). Im Vorführmodus
/// fest, damit Screenshots reproduzierbar sind.
pub fn story_seed() -> u64 {
    if std::env::var("DRIFTCREW_DEMO").is_ok() {
        return 0;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        let data = match GameData::load() {
            Ok(d) => Arc::new(d),
            Err(e) => panic!("Spieldaten fehlerhaft: {e}"),
        };
        let (save, has_save) = match load_save() {
            Some(s) => (s, true),
            None => (CrewSave::new_game_seeded(&data, story_seed()), false),
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
            .init_resource::<Replay>()
            .add_message::<SimMsg>()
            .add_systems(
                FixedUpdate,
                fixed_tick.run_if(in_state(AppState::Playing).and_then(|p: Res<Paused>| !p.0)),
            )
            .add_systems(
                OnEnter(AppState::Playing),
                |mut p: ResMut<Paused>, mut m: ResMut<MapOpen>, mut r: ResMut<Replay>| {
                    p.0 = false;
                    m.0 = false;
                    r.rec.clear();
                    r.offer = None;
                },
            )
            .add_systems(
                OnExit(AppState::Playing),
                |mut sim: ResMut<Sim>, mut r: ResMut<Replay>| r.stop(&mut sim.0),
            )
            .add_systems(Update, react_to_events.run_if(in_state(AppState::Playing)))
            .add_systems(Update, toggle_fullscreen)
            .add_systems(
                Update,
                sync_flight_assist.run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnEnter(AppState::Playing), first_hints)
            .add_systems(Last, save_on_exit);
    }
}

/// Flugassistenz aus den Einstellungen als Befehl in die Simulation (deterministisch).
fn sync_flight_assist(
    sim: Res<Sim>,
    settings: Res<crate::settings::Settings>,
    mut pending: ResMut<PendingCommands>,
    replay: Res<Replay>,
) {
    let want = settings.flight_assist;
    if replay.play.is_none()
        && sim.0.flight_assist != want
        && !pending.0.contains(&Command::SetFlightAssist(want))
    {
        pending.0.push(Command::SetFlightAssist(want));
    }
}

/// F11: Vollbild umschalten (wird wie im Einstellungsmenü gespeichert).
fn toggle_fullscreen(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<crate::settings::Settings>,
) {
    if keys.just_pressed(KeyCode::F11) {
        settings.fullscreen = !settings.fullscreen;
        crate::settings::store(&settings);
    }
}

fn save_on_exit(
    mut exit: MessageReader<AppExit>,
    state: Res<State<AppState>>,
    sim: Res<Sim>,
    has_save: Res<HasSave>,
    replay: Res<Replay>,
) {
    if exit.read().next().is_some() && (*state.get() == AppState::Playing || has_save.0) {
        write_save(&replay.real(&sim.0).to_save());
    }
}

/// Ein paar Tipps beim ersten Start der Sitzung.
fn first_hints(mut shown: Local<bool>, mut toasts: ResMut<crate::ui::Toasts>) {
    if *shown {
        return;
    }
    *shown = true;
    toasts.push(
        "Alle Triebwerke = geradeaus · nur links = Drehung nach rechts",
        crate::sim::ToastKind::Info,
    );
    toasts.push(
        "Aufträge im Stationsmenü · Tab öffnet die Karte · F11 Vollbild",
        crate::sim::ToastKind::Info,
    );
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
    mut replay: ResMut<Replay>,
) {
    if replay.play.is_some() {
        play_tick(&mut sim.0, &mut replay, &mut out);
        // Eingaben während der Wiedergabe zählen nicht.
        pending.0.clear();
        latch.0 = 0;
        return;
    }
    let mut input = build_tick_input(&bindings, &mut latch, &aims, &keys, &mouse, &pads);
    input.commands = std::mem::take(&mut pending.0);
    if let Some(s) = scripted {
        input.slots |= s.0;
        input.brake |= s.1;
    }
    replay.rec.before_step(&sim.0);
    sim.0.step(&input);
    replay.rec.after_step(&sim.0, &input);
    // Schwerer Unfall: Zerstörung oder ein Treffer, der mehr als ein Drittel der Hülle kostet.
    let max_hull = sim.0.ship.max_hull;
    let crash = sim.0.events.iter().any(|e| match e {
        SimEvent::ShipDestroyed => true,
        SimEvent::Damage { amount } => *amount >= max_hull * 0.34,
        _ => false,
    });
    if crash && replay.capture_in == 0 {
        replay.capture_in = (TICK_HZ * 0.9) as u32;
    }
    if replay.capture_in > 0 {
        replay.capture_in -= 1;
        if replay.capture_in == 0 {
            replay.offer = replay.rec.clip(REPLAY_SECONDS + 0.9);
            replay.offer_left = 10.0;
        }
    }
    if replay.offer.is_some() {
        replay.offer_left -= DT;
        if replay.offer_left <= 0.0 {
            replay.offer = None;
        }
    }
    for e in sim.0.events.iter() {
        out.write(SimMsg(e.clone()));
    }
}

/// Ein Tick der Wiedergabe (die letzten Sekunden in Zeitlupe).
fn play_tick(sim: &mut SimState, replay: &mut Replay, out: &mut MessageWriter<SimMsg>) {
    let Some(p) = replay.play.as_mut() else {
        return;
    };
    p.frame += 1;
    if p.slow() && p.frame % 2 == 1 {
        return;
    }
    if p.idx < p.clip.inputs.len() {
        let input = p.clip.inputs[p.idx].clone();
        p.idx += 1;
        sim.step(&input);
    } else {
        p.hold -= DT;
        sim.step(&TickInput::default());
        if p.hold <= 0.0 {
            replay.stop(sim);
            return;
        }
    }
    for e in sim.events.iter().filter(|e| replay_visible(e)) {
        out.write(SimMsg(e.clone()));
    }
}

fn react_to_events(
    mut events: MessageReader<SimMsg>,
    sim: Res<Sim>,
    mut next: ResMut<NextState<AppState>>,
    mut mode: ResMut<LobbyMode>,
    mut crew: ResMut<Crew>,
    mut active: ResMut<ActiveBindings>,
) {
    let mut save = false;
    for SimMsg(e) in events.read() {
        match e {
            SimEvent::ShipChanged => {
                crew.bindings.clear();
                active.0.clear();
                *mode = LobbyMode::NewShip;
                next.set(AppState::Lobby);
                save = true;
            }
            SimEvent::Docked { .. }
            | SimEvent::Purchased { .. }
            | SimEvent::MissionCompleted { .. }
            | SimEvent::Sold { .. }
            | SimEvent::StageCompleted { .. }
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
