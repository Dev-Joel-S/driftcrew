//! Funk: kurze Sprüche der Stationen beim Anflug, Andocken und Abdocken.
//! Reine Anzeige aus `radio.ron` – die Simulation kennt den Funk nicht.

use bevy::prelude::*;

use super::{BG, MUTED, TEAL, TEXT, text};
use crate::game::{AppState, Sim, SimMsg};
use crate::sim::SimEvent;
use crate::sim::world::Owner;

const SHOW_SECONDS: f32 = 9.0;

pub struct RadioPlugin;

impl Plugin for RadioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RadioLog>()
            .add_systems(OnEnter(AppState::Playing), spawn_root)
            .add_systems(OnExit(AppState::Playing), despawn_root)
            .add_systems(
                Update,
                (listen, draw).chain().run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Resource, Default)]
pub struct RadioLog {
    pub lines: Vec<(String, f32)>,
    dirty: bool,
    counter: usize,
    last_owner: Option<Owner>,
    approached: Option<Owner>,
    /// Nach dem Abdocken grüßt dieselbe Station nicht gleich wieder.
    undocked_from: Option<Owner>,
    approach_cooldown: f32,
}

impl RadioLog {
    pub fn push(&mut self, line: String) {
        self.lines.push((line, 0.0));
        if self.lines.len() > 2 {
            self.lines.remove(0);
        }
        self.dirty = true;
    }
}

#[derive(Component)]
struct RadioRoot;

fn spawn_root(mut commands: Commands, mut log: ResMut<RadioLog>) {
    log.dirty = true;
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(18.0),
            bottom: Val::Px(262.0),
            width: Val::Px(310.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            ..default()
        },
        RadioRoot,
        Pickable::IGNORE,
    ));
}

fn despawn_root(mut commands: Commands, q: Query<Entity, With<RadioRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Spruch auswählen: erst ortsspezifische, dann allgemeine Sprüche, reihum.
fn pick(sim: &crate::sim::SimState, owner: Owner, kind: &str, counter: usize) -> Option<String> {
    let radio = &sim.data.radio;
    let id = match owner {
        Owner::Station(i) => sim.world.stations[i].id.clone(),
        Owner::Planet(i) => sim.data.world.planets[i].id.clone(),
    };
    let place = radio.places.iter().find(|p| p.at == id);
    let general: &[String] = match kind {
        "approach" => &radio.approach,
        "docked" => &radio.docked,
        _ => &radio.undock,
    };
    let special: &[String] = match (kind, place) {
        ("approach", Some(p)) => &p.approach,
        ("docked", Some(p)) => &p.docked,
        _ => &[],
    };
    // Jeder zweite Spruch kommt vom Ort selbst, wenn er welche hat.
    let pool = if !special.is_empty() && counter.is_multiple_of(2) {
        special
    } else {
        general
    };
    let line = pool.get(counter % pool.len().max(1))?;
    let ship = sim.data.ship(&sim.crew.current_ship).name.clone();
    Some(
        line.replace("{station}", sim.world.owner_name(owner))
            .replace("{ship}", &ship),
    )
}

fn listen(
    time: Res<Time>,
    sim: Res<Sim>,
    mut events: MessageReader<SimMsg>,
    mut log: ResMut<RadioLog>,
) {
    let s = &sim.0;
    // Auch das Andocken zu Spielbeginn zählt (dafür gibt es kein Ereignis).
    if let Some(pad) = s.ship.docked {
        log.last_owner = Some(s.world.pads[pad].owner);
    }
    for SimMsg(e) in events.read() {
        match e {
            SimEvent::Docked { pad } => {
                let owner = s.world.pads[*pad].owner;
                log.last_owner = Some(owner);
                if s.world.pads[*pad].zone {
                    continue;
                }
                log.counter += 1;
                let c = log.counter;
                if let Some(l) = pick(s, owner, "docked", c) {
                    log.push(l);
                }
            }
            // Geschichte: Kapitel, Monumente – mit Absender.
            SimEvent::Story { speaker, text } => {
                log.push(format!("{speaker}: {text}"));
            }
            SimEvent::Undocked => {
                if let Some(owner) = log.last_owner {
                    log.counter += 1;
                    let c = log.counter;
                    if let Some(l) = pick(s, owner, "undock", c) {
                        log.push(l);
                    }
                }
                log.undocked_from = log.last_owner;
                log.approach_cooldown = 25.0;
            }
            _ => {}
        }
    }
    let dt = time.delta_secs();
    log.approach_cooldown = (log.approach_cooldown - dt).max(0.0);
    // Anflug: erste Plattform eines Ortes in Reichweite.
    if s.ship.docked.is_none() && !s.ship.destroyed {
        let near = s
            .dock_guide()
            .filter(|g| g.distance < 26.0 && !s.world.pads[g.pad].zone)
            .map(|g| s.world.pads[g.pad].owner);
        let just_left = |o: Owner| log.undocked_from == Some(o) && log.approach_cooldown > 0.0;
        if let Some(owner) = near
            && log.approached != Some(owner)
            && !just_left(owner)
        {
            log.approached = Some(owner);
            log.counter += 1;
            let c = log.counter;
            if let Some(l) = pick(s, owner, "approach", c) {
                log.push(l);
            }
        }
        if near.is_none() && s.dock_guide().is_none() {
            log.approached = None;
        }
    }
    let before = log.lines.len();
    for l in &mut log.lines {
        l.1 += dt;
    }
    // Lange Funksprüche (Geschichte) bleiben länger stehen.
    log.lines
        .retain(|l| l.1 < SHOW_SECONDS + l.0.chars().count() as f32 / 22.0);
    if log.lines.len() != before {
        log.dirty = true;
    }
}

fn draw(mut commands: Commands, mut log: ResMut<RadioLog>, root: Query<Entity, With<RadioRoot>>) {
    let Ok(root) = root.single() else { return };
    if !log.dirty {
        return;
    }
    log.dirty = false;
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for (line, _) in &log.lines {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                    border: UiRect::left(Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(BG.with_alpha(0.8)),
                BorderColor::all(TEAL.with_alpha(0.7)),
                Pickable::IGNORE,
            ))
            .with_children(|b| {
                b.spawn((text("≋ FUNK", 10.0, MUTED), Pickable::IGNORE));
                b.spawn((text(line.clone(), 14.0, TEXT), Pickable::IGNORE));
            });
        }
    });
}
