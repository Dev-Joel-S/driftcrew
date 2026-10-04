//! Zurufe (Punkt 73): kurze Signale an die ganze Crew, ohne Pflichtrollen.
//!
//! Tastatur: F5 Bremsen · F6 Schub aus · F7 Links drehen · F8 Rechts drehen · F9 Werkzeug bereit.
//! Gamepad (Steuerkreuz, nie als Slot belegbar): ← Links drehen, → Rechts drehen,
//! ↑ Werkzeug bereit. ↓ ist der Bremsassistent (siehe `input.rs`).
//! Der Zuruf erscheint in der Farbe des Crewmitglieds über dem Schiff; ein kurzer Ton ist
//! optional (Einstellungen).

use bevy::prelude::*;

use super::cargo::CargoPlan;
use super::logbook::Logbook;
use super::text_entry::TextEntry;
use crate::game::{AppState, GameCamera, MapOpen, Paused, PendingCommands, Sim};
use crate::input::{Crew, Device, player_color};
use crate::sim::{CALLOUT_SECONDS, Call, Command};

pub struct CalloutPlugin;

impl Plugin for CalloutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (callout_input, draw_callouts).run_if(in_state(AppState::Playing)),
        );
    }
}

pub const KEY_CALLS: [(KeyCode, Call); 5] = [
    (KeyCode::F5, Call::Brake),
    (KeyCode::F6, Call::ThrustOff),
    (KeyCode::F7, Call::TurnLeft),
    (KeyCode::F8, Call::TurnRight),
    (KeyCode::F9, Call::ToolReady),
];

#[allow(clippy::too_many_arguments)]
fn callout_input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad)>,
    crew: Res<Crew>,
    sim: Res<Sim>,
    paused: Res<Paused>,
    map: Res<MapOpen>,
    plan: Res<CargoPlan>,
    book: Res<Logbook>,
    entry: Res<TextEntry>,
    mut pending: ResMut<PendingCommands>,
) {
    // Nur im Flug ohne offenes Menü (dort gehört das Steuerkreuz dem Menü).
    if paused.0 || map.0 || plan.open || book.open || entry.open || sim.0.ship.docked.is_some() {
        return;
    }
    let player_of = |d: Device| crew.players.iter().position(|p| p.device == d);
    if let Some(p) = player_of(Device::Keyboard) {
        for (k, call) in KEY_CALLS {
            if keys.just_pressed(k) {
                pending.0.push(Command::Callout {
                    player: p as u8,
                    call,
                });
            }
        }
    }
    for (e, g) in &pads {
        let Some(p) = player_of(Device::Pad(e)) else {
            continue;
        };
        let mut call = None;
        if g.just_pressed(GamepadButton::DPadLeft) {
            call = Some(Call::TurnLeft);
        } else if g.just_pressed(GamepadButton::DPadRight) {
            call = Some(Call::TurnRight);
        } else if g.just_pressed(GamepadButton::DPadUp) {
            call = Some(Call::ToolReady);
        }
        if let Some(call) = call {
            pending.0.push(Command::Callout {
                player: p as u8,
                call,
            });
        }
    }
}

#[derive(Component)]
struct CalloutLabel;

/// Zurufe als Sprechblasen über dem Schiff, in der Farbe des Crewmitglieds.
fn draw_callouts(
    mut commands: Commands,
    sim: Res<Sim>,
    crew: Res<Crew>,
    map: Res<MapOpen>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    old: Query<Entity, With<CalloutLabel>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let s = &sim.0;
    if map.0 || s.callouts.is_empty() || s.ship.destroyed {
        return;
    }
    let Ok((cam, cam_t)) = cam.single() else {
        return;
    };
    let Ok(p) = cam.world_to_viewport(cam_t, s.ship.pos.extend(0.0)) else {
        return;
    };
    for (k, c) in s.callouts.iter().enumerate() {
        let alpha = (c.life / CALLOUT_SECONDS * 3.0).clamp(0.0, 1.0);
        let color = player_color(c.player as usize);
        let who = crew
            .players
            .get(c.player as usize)
            .map(|pl| pl.label.clone())
            .unwrap_or_else(|| format!("Spieler {}", c.player + 1));
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(p.x - 110.0),
                    top: Val::Px(p.y - 110.0 - k as f32 * 34.0),
                    width: Val::Px(220.0),
                    justify_content: JustifyContent::Center,
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.03, 0.07, 0.8 * alpha)),
                BorderColor::all(color.with_alpha(alpha)),
                CalloutLabel,
                GlobalZIndex(30),
            ))
            .with_children(|b| {
                b.spawn(super::text(
                    format!("{who}: {}", c.call.label()),
                    16.0,
                    color.with_alpha(alpha),
                ));
            });
    }
}
