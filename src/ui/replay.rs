//! Unfall-Wiederholung (Punkt 80): Angebot nach einem schweren Unfall, Banner während der
//! Wiedergabe. Ein Druck startet sie für alle am Bildschirm, ein weiterer überspringt sie.
//! Welche Slots gedrückt waren, zeigt die Slot-Leiste unten wie im echten Flug – sie liest den
//! Zustand der wiedergegebenen Simulation.

use bevy::prelude::*;

use super::{ACCENT, BG, BORDER, MUTED, TEXT, WARN, text};
use crate::game::{AppState, MapOpen, Paused, Replay, Sim};
use crate::input::MenuInput;

pub struct ReplayUiPlugin;

impl Plugin for ReplayUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_banner)
            .add_systems(OnExit(AppState::Playing), despawn_banner)
            .add_systems(
                Update,
                (replay_input, draw_banner)
                    .chain()
                    .before(super::pause::pause_input)
                    .before(super::cargo::plan_input)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct ReplayBanner;

#[derive(Component)]
struct ReplayText;

#[derive(Component)]
struct ReplayBar;

fn spawn_banner(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(70.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-230.0)),
                width: Val::Px(460.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.5)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(BORDER),
            ReplayBanner,
            GlobalZIndex(50),
        ))
        .with_children(|b| {
            b.spawn((text("", 16.0, TEXT), ReplayText));
            b.spawn((
                Node {
                    width: Val::Px(420.0),
                    height: Val::Px(5.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
            ))
            .with_children(|track| {
                track.spawn((
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(ACCENT),
                    ReplayBar,
                ));
            });
        });
}

fn despawn_banner(mut commands: Commands, q: Query<Entity, With<ReplayBanner>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Starten (Enter / Pad-Bestätigen / Select) und Überspringen (Esc, Enter, Start, Zurück).
/// Was hier verarbeitet wird, verschwindet aus der Menüeingabe, damit Pause & Co. es nicht
/// auch noch sehen.
fn replay_input(
    mut input: ResMut<MenuInput>,
    mut sim: ResMut<Sim>,
    mut replay: ResMut<Replay>,
    paused: Res<Paused>,
    map: Res<MapOpen>,
) {
    if replay.play.is_some() {
        if input.escape || input.enter || input.confirm || input.start || input.back {
            replay.stop(&mut sim.0);
        }
        // Während der Wiedergabe gehört die Menüeingabe ganz ihr.
        *input = MenuInput::default();
        return;
    }
    if replay.offer.is_none() || paused.0 || map.0 || sim.0.ship.docked.is_some() {
        return;
    }
    if input.enter || input.confirm || input.select {
        replay.start(&mut sim.0);
        *input = MenuInput::default();
    }
}

fn draw_banner(
    replay: Res<Replay>,
    paused: Res<Paused>,
    sim: Res<Sim>,
    mut banner: Query<&mut Node, (With<ReplayBanner>, Without<ReplayBar>)>,
    mut label: Query<(&mut Text, &mut TextColor), With<ReplayText>>,
    mut bar: Query<&mut Node, (With<ReplayBar>, Without<ReplayBanner>)>,
) {
    let (show, msg, color, progress) = if let Some(p) = &replay.play {
        let slow = if p.slow() { " · Zeitlupe" } else { "" };
        (
            true,
            format!(
                "WIEDERHOLUNG  −{:.1} s{slow}\nunten: wer welchen Slot gedrückt hat · Enter / Esc überspringen",
                p.left()
            ),
            WARN,
            p.progress(),
        )
    } else if replay.offer.is_some() && !paused.0 && sim.0.ship.docked.is_none() {
        (
            true,
            format!(
                "Unfall ansehen? Enter / Select – die letzten Sekunden für alle ({:.0} s)",
                replay.offer_left.max(0.0).ceil()
            ),
            MUTED,
            replay.offer_left / 10.0,
        )
    } else {
        (false, String::new(), TEXT, 0.0)
    };
    if let Ok(mut n) = banner.single_mut() {
        let want = if show { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
    if let Ok((mut t, mut c)) = label.single_mut() {
        if t.0 != msg {
            t.0 = msg;
        }
        c.0 = color;
    }
    if let Ok(mut n) = bar.single_mut() {
        n.width = Val::Percent(progress.clamp(0.0, 1.0) * 100.0);
    }
}
