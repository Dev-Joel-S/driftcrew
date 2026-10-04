//! Lobby: Wer eine Taste drückt, übernimmt den markierten Slot.
//! Nicht belegte Triebwerke verschwinden, die belegten werden symmetrisch neu angeordnet –
//! das sieht man live am Schiff.

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;

use super::{ACCENT, BAD, BG_FOCUS, BORDER, GOOD, MUTED, Signature, TEAL, TEXT, chip, panel, text};
use crate::game::{AppState, GameCamera, LobbyMode, Sim};
use crate::input::{
    ActiveBindings, Binding, Btn, ClaimTarget, Crew, Device, MenuInput, btn_pressed,
    fresh_claimable,
};
use crate::render::slot_color;
use crate::sim::data::ShipDef;
use crate::sim::thruster_label;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LobbyState>()
            .add_systems(OnEnter(AppState::Lobby), enter_lobby)
            .add_systems(OnExit(AppState::Lobby), exit_lobby)
            .add_systems(
                Update,
                (lobby_input, lobby_preview, draw_lobby, draw_slot_labels)
                    .chain()
                    .run_if(in_state(AppState::Lobby)),
            )
            .add_systems(Update, hot_join.run_if(in_state(AppState::Playing)));
    }
}

/// Hot-Join: Ein Gerät, das noch nicht zur Crew gehört, drückt mitten im Flug eine Taste und
/// übernimmt damit den nächsten freien Slot. Der Umbau läuft als Befehl durch die Simulation.
#[allow(clippy::too_many_arguments)]
fn hot_join(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    paused: Res<crate::game::Paused>,
    sim: Res<Sim>,
    mut crew: ResMut<Crew>,
    mut active: ResMut<ActiveBindings>,
    mut pending: ResMut<crate::game::PendingCommands>,
    mut toasts: ResMut<super::Toasts>,
    mut full_warned: Local<Vec<Device>>,
) {
    if paused.0 {
        return;
    }
    let def = sim.0.current_def();
    let ts = targets(&def);
    for btn in fresh_claimable(&keys, &mouse, &pads) {
        let device = btn.device();
        if crew.players.iter().any(|p| p.device == device) {
            continue;
        }
        let Some(target) = ts
            .iter()
            .copied()
            .find(|t| !crew.bindings.iter().any(|b| b.target == *t))
        else {
            if !full_warned.contains(&device) {
                full_warned.push(device);
                toasts.push(
                    "Alle Slots belegt – im Pausemenü lassen sich Slots neu verteilen",
                    crate::sim::ToastKind::Warn,
                );
            }
            continue;
        };
        let player = crew.player_for(device, &pads);
        crew.bindings.push(Binding {
            btn,
            player,
            target,
        });
        let loadout = crew.loadout();
        pending.0.push(crate::sim::Command::SetLoadout {
            thrusters: loadout.thrusters,
            tools: loadout.tools,
            crew_size: crew.players.len() as u8,
        });
        active.0 = crew.resolve(&def);
        let who = crew.players[player].label.clone();
        toasts.push(
            format!(
                "Spieler {} ({who}) steigt ein: {} auf [{}]",
                player + 1,
                target_name(&def, target),
                btn.label()
            ),
            crate::sim::ToastKind::Good,
        );
    }
}

#[derive(Resource, Default)]
pub struct LobbyState {
    pub cursor: usize,
    pub backup: Option<Crew>,
    pub message: Option<(String, f32)>,
    /// Gehaltene Taste eines belegten Slots und wie lange schon (Halten = Slot abgeben).
    pub hold: Option<(Btn, f32)>,
}

/// So lange die eigene Taste halten, um den Slot abzugeben.
pub const HOLD_RELEASE: f32 = 1.2;

#[derive(Component)]
struct LobbyRoot;

#[derive(Component)]
struct TargetRow(usize);

#[derive(Component)]
struct SlotLabel(usize);

#[derive(Component)]
struct SlotLabelLayer;

/// Alle belegbaren Ziele des Schiffs in Belegungsreihenfolge.
pub fn targets(def: &ShipDef) -> Vec<ClaimTarget> {
    let mut v: Vec<ClaimTarget> = (0..def.max_thrusters() as usize)
        .map(ClaimTarget::Thruster)
        .collect();
    v.extend((0..def.tool_parts().count()).map(ClaimTarget::Tool));
    v
}

pub fn target_name(def: &ShipDef, t: ClaimTarget) -> String {
    match t {
        ClaimTarget::Thruster(i) => {
            let order = [
                "links",
                "rechts",
                "Mitte",
                "außen links",
                "außen rechts",
                "ganz außen links",
                "ganz außen rechts",
            ];
            format!("Triebwerk {}", order.get(i).copied().unwrap_or("extra"))
        }
        ClaimTarget::Tool(i) => def
            .tool_parts()
            .nth(i)
            .map(|(_, _, k)| k.label().to_string())
            .unwrap_or_else(|| "Werkzeug".into()),
    }
}

fn first_free(crew: &Crew, ts: &[ClaimTarget]) -> usize {
    ts.iter()
        .position(|t| !crew.bindings.iter().any(|b| b.target == *t))
        .unwrap_or(ts.len().saturating_sub(1))
}

fn enter_lobby(
    mut commands: Commands,
    mode: Res<LobbyMode>,
    crew: Res<Crew>,
    sim: Res<Sim>,
    mut state: ResMut<LobbyState>,
) {
    let def = sim.0.current_def();
    let ts = targets(&def);
    state.cursor = first_free(&crew, &ts);
    state.backup = (*mode == LobbyMode::Redistribute).then(|| crew.clone());
    state.message = None;
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            padding: UiRect::right(Val::Px(48.0)),
            ..default()
        },
        LobbyRoot,
        Signature(0),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        SlotLabelLayer,
        Pickable::IGNORE,
    ));
}

fn exit_lobby(
    mut commands: Commands,
    q: Query<Entity, Or<(With<LobbyRoot>, With<SlotLabelLayer>)>>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn lobby_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    input: Res<MenuInput>,
    rows: Query<(&Interaction, &TargetRow), Changed<Interaction>>,
    mode: Res<LobbyMode>,
    time: Res<Time>,
    mut crew: ResMut<Crew>,
    mut state: ResMut<LobbyState>,
    mut sim: ResMut<Sim>,
    mut active: ResMut<ActiveBindings>,
    mut next: ResMut<NextState<AppState>>,
) {
    let def = sim.0.current_def();
    let ts = targets(&def);
    if ts.is_empty() {
        return;
    }
    if let Some((_, t)) = &mut state.message {
        *t -= time.delta_secs();
    }
    let mut changed = false;

    for (i, r) in &rows {
        if *i == Interaction::Pressed {
            state.cursor = r.0;
        }
    }
    if input.tab || input.down {
        state.cursor = (state.cursor + 1) % ts.len();
    }
    if input.up {
        state.cursor = (state.cursor + ts.len() - 1) % ts.len();
    }
    if input.backspace {
        // Letzte Tastatur-Belegung lösen.
        if let Some(pos) = crew
            .bindings
            .iter()
            .rposition(|b| b.btn.device() == Device::Keyboard)
        {
            let b = crew.bindings.remove(pos);
            state.cursor = ts.iter().position(|t| *t == b.target).unwrap_or(0);
            changed = true;
        }
    }
    // Eigene Taste gedrückt halten = Slot abgeben (geht mit jedem Gerät, auch einem Joy-Con).
    let held = crew
        .bindings
        .iter()
        .map(|b| b.btn)
        .find(|btn| btn_pressed(btn, &keys, &mouse, &pads));
    state.hold = match (held, state.hold) {
        (Some(btn), Some((h, t))) if h == btn => Some((btn, t + time.delta_secs())),
        (Some(btn), _) => Some((btn, 0.0)),
        (None, _) => None,
    };
    if let Some((btn, t)) = state.hold
        && t >= HOLD_RELEASE
        && let Some(pos) = crew.bindings.iter().position(|b| b.btn == btn)
    {
        let b = crew.bindings.remove(pos);
        state.cursor = ts.iter().position(|t| *t == b.target).unwrap_or(0);
        state.message = Some((
            format!("Slot abgegeben: {}", target_name(&def, b.target)),
            2.5,
        ));
        state.hold = None;
        changed = true;
    }

    for btn in fresh_claimable(&keys, &mouse, &pads) {
        if crew.bindings.iter().any(|b| b.btn == btn) {
            continue; // schon belegt – feuert nur in der Vorschau
        }
        let target = ts[state.cursor.min(ts.len() - 1)];
        let player = crew.player_for(btn.device(), &pads);
        crew.bindings.retain(|b| b.target != target);
        crew.bindings.push(Binding {
            btn,
            player,
            target,
        });
        state.cursor = first_free(&crew, &ts);
        changed = true;
    }

    if changed {
        let loadout = crew.loadout();
        sim.0.rebuild_ship(loadout);
        active.0 = crew.resolve(&def);
    }

    // Losfliegen: Enter, Start (+) oder Select (−) – auch ein einzelner Joy-Con hat eins davon.
    if input.enter || input.start || input.select {
        let thrusters = crew.thruster_claims().len() as u8;
        if thrusters < def.min_thrusters {
            state.message = Some((
                format!("Mindestens {} Triebwerke belegen", def.min_thrusters),
                3.0,
            ));
        } else {
            crew.prune_players();
            let loadout = crew.loadout();
            sim.0.rebuild_ship(loadout);
            sim.0.set_crew_size(crew.players.len() as u8);
            active.0 = crew.resolve(&def);
            next.set(AppState::Playing);
        }
    }
    if input.escape {
        match *mode {
            LobbyMode::Initial => next.set(AppState::Title),
            LobbyMode::Redistribute => {
                if let Some(b) = state.backup.take() {
                    *crew = b;
                }
                let loadout = crew.loadout();
                sim.0.rebuild_ship(loadout);
                active.0 = crew.resolve(&def);
                next.set(AppState::Playing);
            }
            LobbyMode::NewShip => {
                state.message = Some(("Neues Schiff: bitte Slots verteilen".into(), 3.0));
            }
        }
    }
}

/// Vorschau: belegte Tasten zünden die Triebwerke am Schiff.
fn lobby_preview(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    active: Res<ActiveBindings>,
    time: Res<Time>,
    mut sim: ResMut<Sim>,
) {
    let dt = time.delta_secs();
    for t in &mut sim.0.ship.thrusters {
        let pressed = active
            .0
            .iter()
            .any(|b| b.slot == t.slot && crate::input::btn_pressed(&b.btn, &keys, &mouse, &pads));
        let target = if pressed { 1.0 } else { 0.0 };
        t.level += (target - t.level) * (dt * 14.0).min(1.0);
    }
}

fn binding_text(crew: &Crew, b: &Binding) -> String {
    let who = crew
        .players
        .get(b.player)
        .map(|p| p.label.clone())
        .unwrap_or_default();
    format!("[{}]  Spieler {} · {}", b.btn.label(), b.player + 1, who)
}

#[allow(clippy::too_many_arguments)]
fn draw_lobby(
    mut commands: Commands,
    crew: Res<Crew>,
    state: Res<LobbyState>,
    sim: Res<Sim>,
    mode: Res<LobbyMode>,
    active: Res<ActiveBindings>,
    mut root: Query<(Entity, &mut Signature), With<LobbyRoot>>,
) {
    let Ok((root, mut sig)) = root.single_mut() else {
        return;
    };
    let def = sim.0.current_def();
    let ts = targets(&def);
    let msg = state
        .message
        .as_ref()
        .filter(|(_, t)| *t > 0.0)
        .map(|(m, _)| m.clone());
    let hold = state.hold.map(|(b, t)| (b, (t / HOLD_RELEASE * 5.0) as u8));
    let key = format!(
        "{:?}|{}|{:?}|{:?}|{:?}|{:?}",
        crew.bindings, state.cursor, msg, active.0, *mode, hold
    );
    let s = super::sig_of(&key);
    if sig.0 == s {
        return;
    }
    sig.0 = s;
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|r| {
        r.spawn(panel(Val::Px(560.0))).with_children(|p| {
            p.spawn(text(
                match *mode {
                    LobbyMode::NewShip => "NEUES SCHIFF – SLOTS VERTEILEN",
                    LobbyMode::Redistribute => "SLOTS NEU VERTEILEN",
                    LobbyMode::Initial => "CREW ZUSAMMENSTELLEN",
                },
                24.0,
                TEXT,
            ));
            p.spawn(text(format!("{} · {}", def.name, def.class), 17.0, ACCENT));
            p.spawn(text(def.description.clone(), 14.0, MUTED));
            let n = crew.players.len();
            let (lo, hi) = def.crew;
            let fit = if n == 0 {
                String::new()
            } else if (n as u8) < lo {
                " – fürs Erste etwas viel Arbeit pro Person".into()
            } else if (n as u8) > hi {
                " – eng, aber machbar".into()
            } else {
                " – passt".into()
            };
            p.spawn(text(
                format!("Empfohlene Crew: {lo}–{hi} Personen · ihr seid {n}{fit}"),
                14.0,
                TEAL,
            ));
            p.spawn(Node {
                height: Val::Px(8.0),
                ..default()
            });
            for (i, t) in ts.iter().enumerate() {
                let bound = crew.bindings.iter().find(|b| b.target == *t);
                let slot = bound
                    .and_then(|b| active.0.iter().find(|a| a.btn == b.btn))
                    .map(|a| a.slot);
                let is_cursor = i == state.cursor;
                let color = slot
                    .map(slot_color)
                    .unwrap_or(Color::srgb(0.25, 0.27, 0.33));
                p.spawn((
                    Button,
                    Node {
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(12.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(7.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(if is_cursor {
                        BG_FOCUS
                    } else {
                        Color::srgba(1.0, 1.0, 1.0, 0.03)
                    }),
                    BorderColor::all(if is_cursor { BORDER } else { Color::NONE }),
                    TargetRow(i),
                ))
                .with_children(|row| {
                    row.spawn(chip(color, 16.0));
                    row.spawn((
                        Node {
                            width: Val::Px(190.0),
                            ..default()
                        },
                        children![text(target_name(&def, *t), 16.0, TEXT)],
                    ));
                    match bound {
                        Some(b) if hold.is_some_and(|(h, n)| h == b.btn && n > 0) => {
                            let n = hold.map_or(0, |(_, n)| n.min(5)) as usize;
                            row.spawn(text(
                                format!(
                                    "weiter halten = abgeben {}{}",
                                    "■".repeat(n),
                                    "□".repeat(5 - n)
                                ),
                                15.0,
                                ACCENT,
                            ));
                        }
                        Some(b) => {
                            row.spawn(text(binding_text(&crew, b), 15.0, GOOD));
                        }
                        None if is_cursor => {
                            row.spawn(text("← Taste drücken!", 15.0, ACCENT));
                        }
                        None => {
                            row.spawn(text("frei", 15.0, MUTED));
                        }
                    }
                });
            }
            p.spawn(Node {
                height: Val::Px(8.0),
                ..default()
            });
            let n_thr = crew.thruster_claims().len();
            let status = if n_thr < def.min_thrusters as usize {
                (
                    format!("Triebwerke: {n_thr} (mindestens {})", def.min_thrusters),
                    BAD,
                )
            } else {
                (
                    format!("Triebwerke: {n_thr} · symmetrisch angeordnet"),
                    GOOD,
                )
            };
            p.spawn(text(status.0, 15.0, status.1));
            let players: Vec<String> = crew
                .players
                .iter()
                .enumerate()
                .map(|(i, pl)| {
                    let n = crew.bindings.iter().filter(|b| b.player == i).count();
                    format!("Spieler {} – {} ({} Slots)", i + 1, pl.label, n)
                })
                .collect();
            if players.is_empty() {
                p.spawn(text("Noch niemand an Bord.", 15.0, MUTED));
            }
            for pl in players {
                p.spawn(text(pl, 15.0, TEAL));
            }
            if let Some(m) = &msg {
                p.spawn(text(m.clone(), 16.0, BAD));
            }
            p.spawn(Node {
                height: Val::Px(8.0),
                ..default()
            });
            for (head, line) in [
                (
                    "Nehmen",
                    "beliebige Taste drücken – sie steuert dann den markierten Slot",
                ),
                ("Abgeben", "die eigene Taste gut 1 Sekunde gedrückt halten"),
                ("Wählen", "↑↓ · Tab · Steuerkreuz · Stick · Klick"),
                (
                    "Los",
                    "Enter · Start (+) · Select (−) – geht auch mit nur einem Joy-Con",
                ),
                ("Zurück", "Esc"),
            ] {
                p.spawn(Node {
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|l| {
                    l.spawn((
                        Node {
                            width: Val::Px(70.0),
                            ..default()
                        },
                        children![text(head, 13.0, ACCENT)],
                    ));
                    l.spawn(text(line, 13.0, MUTED));
                });
            }
            p.spawn(text(
                "Werkzeuge zielen mit der Maus (Tastatur) bzw. dem Stick des Geräts",
                12.0,
                MUTED,
            ));
        });
    });
}

/// Tastenbeschriftungen direkt am 3D-Schiff.
fn draw_slot_labels(
    mut commands: Commands,
    sim: Res<Sim>,
    crew: Res<Crew>,
    active: Res<ActiveBindings>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    layer: Query<Entity, With<SlotLabelLayer>>,
    mut labels: Query<(Entity, &SlotLabel, &mut Node, &mut Text)>,
) {
    let (Ok((cam, cam_t)), Ok(layer)) = (cam.single(), layer.single()) else {
        return;
    };
    let ship = &sim.0.ship;
    let mut wanted: Vec<(usize, Vec2, String)> = Vec::new();
    for (ti, t) in ship.thrusters.iter().enumerate() {
        let world = ship.to_world(t.pos - Vec2::Y * 1.6);
        let label = active
            .0
            .iter()
            .find(|b| b.slot == t.slot)
            .map(|b| b.btn.label())
            .unwrap_or_else(|| "?".into());
        let _ = thruster_label(ti, ship.thrusters.len());
        wanted.push((t.slot as usize, world, label));
    }
    for t in &ship.tools {
        let out = (t.pos - ship.com).normalize_or(Vec2::Y);
        let world = ship.to_world(t.pos + out * 1.7);
        let label = active
            .0
            .iter()
            .find(|b| b.slot == t.slot)
            .map(|b| b.btn.label());
        if let Some(l) = label {
            wanted.push((
                t.slot as usize,
                world,
                format!("{}\n[{}]", t.kind.label(), l),
            ));
        }
    }
    let _ = &crew;
    let mut seen = Vec::new();
    for (e, sl, mut node, mut txt) in &mut labels {
        if let Some((_, w, l)) = wanted.iter().find(|(s, _, _)| *s == sl.0) {
            if let Ok(p) = cam.world_to_viewport(cam_t, w.extend(1.0)) {
                node.left = Val::Px(p.x - 50.0);
                node.top = Val::Px(p.y - 12.0);
            }
            if txt.0 != *l {
                txt.0 = l.clone();
            }
            seen.push(sl.0);
        } else {
            commands.entity(e).despawn();
        }
    }
    for (slot, _, l) in wanted {
        if seen.contains(&slot) {
            continue;
        }
        let c = slot_color(slot as u8);
        let e = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(100.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Text::new(l),
                TextFont::from_font_size(15.0),
                TextColor(c),
                TextLayout::justify(Justify::Center),
                SlotLabel(slot),
                Pickable::IGNORE,
            ))
            .id();
        commands.entity(layer).add_child(e);
    }
}

#[allow(dead_code)]
fn _unused(_: Btn) {}
