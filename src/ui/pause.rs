//! Pausemenü (Esc / Start): weiter, Slots neu verteilen, Hilfe, speichern, beenden.

use bevy::prelude::*;

use super::title::HELP_TEXT;
use super::{
    ACCENT, Item, ItemButton, MENU_PAUSE, MUTED, MenuFocus, TEXT, mouse_pick, navigate, panel,
    spawn_items, text,
};
use crate::game::{AppState, LobbyMode, MapOpen, Paused, PendingCommands, Sim, write_save};
use crate::input::MenuInput;
use crate::sim::{Command, SimState};

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::Playing), despawn_pause)
            .add_systems(
                Update,
                pause_input
                    .before(super::logbook::logbook_input)
                    .before(super::cargo::plan_input)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
pub struct PauseRoot;

#[derive(Component)]
pub struct PauseHelp;

#[derive(Clone, Debug, PartialEq)]
enum PauseAct {
    Resume,
    AbortCourse,
    Redistribute,
    Help,
    Logbook,
    CargoPlan,
    SaveTitle,
    Quit,
}

fn items(sim: &SimState) -> Vec<Item<PauseAct>> {
    let mut v = vec![Item::new("Weiter", PauseAct::Resume)];
    if let Some(r) = &sim.course {
        let name = &sim.data.courses.courses[r.course].name;
        v.push(
            Item::new(format!("{name} abbrechen"), PauseAct::AbortCourse)
                .detail("Parcours ohne Wertung beenden"),
        );
    }
    v.extend([
        Item::new("Slots neu verteilen", PauseAct::Redistribute)
            .detail("Jemand kommt dazu oder fällt aus"),
        Item::new("Logbuch", PauseAct::Logbook).detail(format!(
            "Kapitel, Artefakte ({}/{}), Funde ({}/{})",
            sim.crew.artifacts.len(),
            sim.data.story.artifacts.len(),
            sim.story.logs.len(),
            sim.data.story.logs.len()
        )),
        Item::new("Ladeplan", PauseAct::CargoPlan).detail(format!(
            "Fracht umladen oder abwerfen · {:.1} / {:.1} t an Bord",
            sim.ship.cargo_mass().max(0.0),
            sim.ship.cargo_capacity()
        )),
        Item::new("Steuerung & Spielprinzip", PauseAct::Help),
        Item::new("Speichern & zum Titel", PauseAct::SaveTitle),
        Item::new("Spiel beenden", PauseAct::Quit).detail("Der Spielstand wird gespeichert"),
    ]);
    v
}

fn spawn_pause(commands: &mut Commands, sim: &SimState) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.6)),
            PauseRoot,
            GlobalZIndex(70),
        ))
        .with_children(|r| {
            r.spawn(panel(Val::Px(420.0))).with_children(|p| {
                p.spawn(text("PAUSE", 28.0, TEXT));
                p.spawn(text("Die Simulation ruht.", 14.0, MUTED));
                p.spawn(Node {
                    height: Val::Px(8.0),
                    ..default()
                });
                spawn_items(p, MENU_PAUSE, &items(sim));
            });
        });
}

fn despawn_pause(
    mut commands: Commands,
    q: Query<Entity, Or<(With<PauseRoot>, With<PauseHelp>)>>,
    mut paused: ResMut<Paused>,
) {
    paused.0 = false;
    for e in &q {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn pause_input(
    mut commands: Commands,
    input: Res<MenuInput>,
    map: Res<MapOpen>,
    sim: Res<Sim>,
    buttons: Query<(&Interaction, &ItemButton), Changed<Interaction>>,
    mut paused: ResMut<Paused>,
    mut focus: ResMut<MenuFocus>,
    mut next: ResMut<NextState<AppState>>,
    mut mode: ResMut<LobbyMode>,
    roots: Query<Entity, With<PauseRoot>>,
    help: Query<Entity, With<PauseHelp>>,
    mut exit: MessageWriter<AppExit>,
    mut pending: ResMut<PendingCommands>,
    mut book: ResMut<super::logbook::Logbook>,
    mut plan: ResMut<super::cargo::CargoPlan>,
) {
    if book.open || plan.open {
        return;
    }
    if !help.is_empty() {
        if input.escape || input.enter || input.start || input.back {
            for e in &help {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    let close = |commands: &mut Commands, paused: &mut Paused| {
        paused.0 = false;
        for e in &roots {
            commands.entity(e).despawn();
        }
    };
    if !paused.0 {
        // Start pausiert nur, wenn nicht angedockt (dort bestätigt Start im Stationsmenü).
        let start_pauses = input.start && sim.0.ship.docked.is_none();
        if !map.0 && (input.escape || start_pauses) {
            paused.0 = true;
            focus.0[MENU_PAUSE] = 0;
            spawn_pause(&mut commands, &sim.0);
        }
        return;
    }
    if input.escape || input.back {
        close(&mut commands, &mut paused);
        return;
    }
    let its = items(&sim.0);
    let mut f = focus.0[MENU_PAUSE];
    let mut act = if navigate(&mut f, its.len(), &input) {
        Some(f)
    } else {
        None
    };
    if let Some(i) = mouse_pick(MENU_PAUSE, &mut f, &buttons) {
        act = Some(i);
    }
    focus.0[MENU_PAUSE] = f;
    let Some(a) = act
        .and_then(|i| its.get(i))
        .and_then(|it| it.action.clone())
    else {
        return;
    };
    match a {
        PauseAct::Resume => close(&mut commands, &mut paused),
        PauseAct::AbortCourse => {
            pending.0.push(Command::AbortCourse);
            close(&mut commands, &mut paused);
        }
        PauseAct::Redistribute => {
            close(&mut commands, &mut paused);
            *mode = LobbyMode::Redistribute;
            next.set(AppState::Lobby);
        }
        PauseAct::Help => {
            commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    PauseHelp,
                    GlobalZIndex(80),
                ))
                .with_children(|r| {
                    r.spawn(panel(Val::Px(780.0))).with_children(|p| {
                        for line in HELP_TEXT {
                            let head = !line.is_empty() && line.chars().all(|c| !c.is_lowercase());
                            p.spawn(text(
                                *line,
                                if head { 17.0 } else { 15.0 },
                                if head { ACCENT } else { TEXT },
                            ));
                        }
                        p.spawn(text("Esc / Enter: schließen", 13.0, MUTED));
                    });
                });
        }
        PauseAct::Logbook => {
            book.open = true;
            book.opened = true;
        }
        PauseAct::CargoPlan => {
            // Im Flug läuft die Welt weiter, angedockt bleibt sie angehalten.
            let docked = sim.0.ship.docked.is_some();
            for e in &roots {
                commands.entity(e).despawn();
            }
            paused.0 = docked;
            plan.open = true;
            plan.opened = true;
        }
        PauseAct::SaveTitle => {
            write_save(&sim.0.to_save());
            close(&mut commands, &mut paused);
            next.set(AppState::Title);
        }
        PauseAct::Quit => {
            write_save(&sim.0.to_save());
            exit.write(AppExit::Success);
        }
    }
}
