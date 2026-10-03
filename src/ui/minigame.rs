//! Anzeige der Minispiele: Takt-Reparatur (Notreparatur, ausgefallenes Triebwerk,
//! Ersatzteil am Kran) unten in der Mitte, Andockport-Hack mit dem Tastenmuster oben.
//! Reine Anzeige – die Spiele selbst laufen in der Simulation.

use bevy::prelude::*;

use super::{ACCENT, BAD, BG, GOOD, MUTED, Signature, TEXT, WARN, text};
use crate::game::{AppState, MapOpen, Paused, Sim};
use crate::input::ActiveBindings;
use crate::render::slot_color;
use crate::sim::minigame::{BEAT, BEAT_WINDOW, HULL_PATCH_CAP, PATCH_HITS, REST_TO_REPAIR};
use crate::sim::ship::CraneState;
use crate::sim::{SimState, thruster_label};

pub struct MinigamePlugin;

impl Plugin for MinigamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_roots)
            .add_systems(OnExit(AppState::Playing), despawn_roots)
            .add_systems(
                Update,
                (draw_rhythm, draw_hack).run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct RhythmRoot;
#[derive(Component)]
struct HackRoot;
#[derive(Component)]
struct BeatMarker;

fn spawn_roots(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(205.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-290.0)),
            width: Val::Px(580.0),
            ..default()
        },
        RhythmRoot,
        Signature(0),
        Pickable::IGNORE,
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(200.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-260.0)),
            width: Val::Px(520.0),
            ..default()
        },
        HackRoot,
        Signature(0),
        GlobalZIndex(40),
        Pickable::IGNORE,
    ));
}

fn despawn_roots(mut commands: Commands, q: Query<Entity, Or<(With<RhythmRoot>, With<HackRoot>)>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Wie heißt der Slot am Schiff und welche Taste hat er?
fn slot_names(s: &SimState, bindings: &ActiveBindings, slot: u8) -> (String, String) {
    let name = if let Some(i) = s.ship.thrusters.iter().position(|t| t.slot == slot) {
        thruster_label(i, s.ship.thrusters.len())
    } else {
        s.ship
            .tools
            .iter()
            .find(|t| t.slot == slot)
            .map(|t| t.kind.label().to_string())
            .unwrap_or_else(|| format!("Slot {}", slot + 1))
    };
    let key = bindings
        .0
        .iter()
        .find(|b| b.slot == slot)
        .map(|b| b.btn.label())
        .unwrap_or_default();
    (name, key)
}

/// Was gibt es gerade im Takt zu tun? (Titel, Zeilen) – None, wenn nichts.
fn rhythm_task(s: &SimState, bindings: &ActiveBindings) -> Option<(String, Vec<String>)> {
    if s.ship.docked.is_some() || s.ship.destroyed || s.hack.is_some() {
        return None;
    }
    let failed: Vec<usize> = (0..s.ship.thrusters.len())
        .filter(|&i| s.ship.thrusters[i].failed())
        .collect();
    let patching = s.ship.tools.iter().find_map(|t| match t.crane {
        CraneState::Patching { progress, .. } => Some(progress),
        _ => None,
    });
    let mut lines = Vec::new();
    for &i in &failed {
        let t = &s.ship.thrusters[i];
        let (name, key) = slot_names(s, bindings, t.slot);
        let p = s.repair.thrusters.get(i).copied().unwrap_or(0.0);
        let hits = (p * PATCH_HITS).round() as u32;
        lines.push(format!(
            "{name} [{key}] im Takt: {hits}/{}",
            PATCH_HITS as u32
        ));
    }
    if let Some(p) = patching {
        lines.push(format!(
            "Ersatzteil am Kran: ruhig halten {:.0} %",
            p * 100.0
        ));
    } else if !failed.is_empty()
        && s.ship.has_tool(crate::sim::data::ToolKind::Crane)
        && s.ship
            .cargo
            .iter()
            .any(|c| matches!(c.kind, crate::sim::ship::CargoKind::Salvage { .. }))
    {
        lines.push("oder: Kran aufs Triebwerk richten und halten (Ersatzteil an Bord)".into());
    }
    let hull = format!(
        "Hülle {:.0} % → Notreparatur bis {:.0} %",
        s.ship.hull / s.ship.max_hull * 100.0,
        HULL_PATCH_CAP * 100.0
    );
    if s.repair.active {
        lines.insert(0, hull);
        lines.push(
            "Alle Slot-Tasten im Takt flicken die Hülle · eine Taste lange halten = weiterfliegen"
                .into(),
        );
        return Some(("NOTREPARATUR".into(), lines));
    }
    if !failed.is_empty() || patching.is_some() {
        return Some(("TRIEBWERK AUSGEFALLEN".into(), lines));
    }
    if s.hull_needs_patch() && s.repair.rest > 0.6 {
        let left = (REST_TO_REPAIR - s.repair.rest).max(0.0);
        return Some((
            "NOTREPARATUR".into(),
            vec![
                hull,
                format!("Beginnt in {left:.1} s – Schiff ruhig, nichts drücken"),
            ],
        ));
    }
    None
}

#[allow(clippy::type_complexity)]
fn draw_rhythm(
    mut commands: Commands,
    sim: Res<Sim>,
    map: Res<MapOpen>,
    paused: Res<Paused>,
    bindings: Res<ActiveBindings>,
    mut root: Query<(Entity, &mut Signature, &mut Node), (With<RhythmRoot>, Without<BeatMarker>)>,
    mut marker: Query<&mut Node, (With<BeatMarker>, Without<RhythmRoot>)>,
) {
    let Ok((root, mut sig, mut root_node)) = root.single_mut() else {
        return;
    };
    let s = &sim.0;
    let task = rhythm_task(s, &bindings).filter(|_| !map.0 && !paused.0);
    // Liegt die Präzisionsanzeige schon unten in der Mitte, rückt das Panel darüber.
    let bottom = if s.precision.is_some() { 272.0 } else { 205.0 };
    if root_node.bottom != Val::Px(bottom) {
        root_node.bottom = Val::Px(bottom);
    }
    let Some((title, lines)) = task else {
        if sig.0 != 0 {
            sig.0 = 0;
            commands.entity(root).despawn_children();
        }
        return;
    };
    // Taktmarke: läuft von links nach rechts, Schlag an den Rändern.
    if let Ok(mut n) = marker.single_mut() {
        n.left = Val::Percent(s.beat_phase() * 100.0);
    }
    let beats: Vec<String> = s
        .repair
        .beats
        .iter()
        .map(|b| format!("{}{}", b.slot, b.hit))
        .collect();
    let h = super::sig_of(&format!("{title}|{lines:?}|{beats:?}"));
    if sig.0 == h {
        return;
    }
    sig.0 = h;
    commands.entity(root).despawn_children();
    let zone = BEAT_WINDOW / BEAT * 100.0;
    let accent = if title == "NOTREPARATUR" { GOOD } else { WARN };
    commands.entity(root).with_children(|r| {
        r.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                row_gap: Val::Px(2.0),
                border: UiRect::all(Val::Px(1.5)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(BG.with_alpha(0.85)),
            BorderColor::all(accent.with_alpha(0.7)),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((text(title, 13.0, accent), Pickable::IGNORE));
            for l in &lines {
                p.spawn((text(l.clone(), 12.0, TEXT), Pickable::IGNORE));
            }
            // Taktleiste mit den Trefferzonen an beiden Enden.
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(12.0),
                    margin: UiRect::vertical(Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
                Pickable::IGNORE,
            ))
            .with_children(|bar| {
                for left in [0.0, 100.0 - zone] {
                    bar.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(left),
                            width: Val::Percent(zone),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(GOOD.with_alpha(0.45)),
                        Pickable::IGNORE,
                    ));
                }
                bar.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(0.0),
                        width: Val::Px(6.0),
                        height: Val::Percent(100.0),
                        margin: UiRect::left(Val::Px(-3.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(TEXT),
                    BeatMarker,
                    Pickable::IGNORE,
                ));
            });
            // Letzte Tastendrücke in Slotfarbe: ✓ im Takt, ✕ daneben.
            if !s.repair.beats.is_empty() {
                p.spawn((
                    Node {
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|row| {
                    for b in s.repair.beats.iter().rev().take(8) {
                        let c = slot_color(b.slot);
                        row.spawn((
                            Node {
                                padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                border_radius: BorderRadius::all(Val::Px(5.0)),
                                ..default()
                            },
                            BorderColor::all(c),
                            Pickable::IGNORE,
                            children![(
                                text(
                                    if b.hit { "✓" } else { "✕" },
                                    12.0,
                                    if b.hit { GOOD } else { BAD }
                                ),
                                Pickable::IGNORE
                            )],
                        ));
                    }
                });
            }
        });
    });
}

fn draw_hack(
    mut commands: Commands,
    sim: Res<Sim>,
    map: Res<MapOpen>,
    paused: Res<Paused>,
    bindings: Res<ActiveBindings>,
    mut root: Query<(Entity, &mut Signature), With<HackRoot>>,
) {
    let Ok((root, mut sig)) = root.single_mut() else {
        return;
    };
    let s = &sim.0;
    let Some(h) = s.hack.as_ref().filter(|_| !map.0 && !paused.0) else {
        if sig.0 != 0 {
            sig.0 = 0;
            commands.entity(root).despawn_children();
        }
        return;
    };
    let key = format!("{}|{}|{:.1}", h.station, h.done, h.left);
    let hs = super::sig_of(&key);
    if sig.0 == hs {
        return;
    }
    sig.0 = hs;
    commands.entity(root).despawn_children();
    let name = s.world.stations[h.station].name.clone();
    commands.entity(root).with_children(|r| {
        r.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                border: UiRect::all(Val::Px(1.5)),
                border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(BG.with_alpha(0.92)),
            BorderColor::all(WARN.with_alpha(0.8)),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((
                text(format!("ANDOCKPORT {} GESICHERT", name.to_uppercase()), 14.0, WARN),
                Pickable::IGNORE,
            ));
            p.spawn((
                text(
                    "Muster der Reihe nach drücken – jede Person ihren Slot. Ein Fehler löst Alarm aus.",
                    12.0,
                    MUTED,
                ),
                Pickable::IGNORE,
            ));
            p.spawn((
                Node {
                    column_gap: Val::Px(8.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                for (i, &slot) in h.pattern.iter().enumerate() {
                    let (part, key) = slot_names(s, &bindings, slot);
                    let c = slot_color(slot);
                    let done = i < h.done;
                    let current = i == h.done;
                    row.spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            min_width: Val::Px(64.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                            border: UiRect::all(Val::Px(if current { 3.0 } else { 1.5 })),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(if current {
                            c.with_alpha(0.25)
                        } else {
                            Color::NONE
                        }),
                        BorderColor::all(if done { c.with_alpha(0.3) } else { c }),
                        Pickable::IGNORE,
                    ))
                    .with_children(|b| {
                        let top = if done { "✓".to_string() } else { key };
                        b.spawn((
                            text(top, 17.0, if done { MUTED } else { TEXT }),
                            Pickable::IGNORE,
                        ));
                        b.spawn((
                            text(part, 10.0, if done { MUTED } else { c }),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
            // Zeitleiste.
            p.spawn((
                Node {
                    width: Val::Percent(80.0),
                    height: Val::Px(6.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
                Pickable::IGNORE,
            ))
            .with_children(|bar| {
                let f = (h.left / h.total.max(0.01)).clamp(0.0, 1.0);
                bar.spawn((
                    Node {
                        width: Val::Percent(f * 100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(if f > 0.35 { ACCENT } else { BAD }),
                    Pickable::IGNORE,
                ));
            });
        });
    });
}
