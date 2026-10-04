//! Ladeplan (Punkte 64, 67, 68, 78): welche Fracht in welchem Modul sitzt, wo der Schwerpunkt
//! liegt, Umladen und Abwerfen. Geöffnet aus dem Pausemenü. Im Flug läuft die Simulation
//! weiter (Umladen dauert dann ein paar Sekunden), angedockt bleibt sie angehalten und es geht
//! sofort.
//!
//! ↑↓ Frachtstück wählen · ←→ Zielmodul · Enter umladen · ⌫ (Pad: Select) zweimal abwerfen ·
//! Esc schließen. Pfeiltasten sind nie Slot-Tasten, das Fliegen geht also nebenher weiter.

use bevy::prelude::*;

use super::{
    ACCENT, BAD, BG, BG_FOCUS, BORDER, GOOD, MUTED, TEAL, TEXT, WARN, panel_node, sig_of, text,
};
use crate::game::{AppState, MapOpen, Paused, PendingCommands, Sim};
use crate::input::MenuInput;
use crate::sim::data::CargoTrait;
use crate::sim::ship::{CargoItem, CargoKind, Ship};
use crate::sim::{Command, SimState};

pub struct CargoPlanPlugin;

impl Plugin for CargoPlanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CargoPlan>()
            .add_systems(OnExit(AppState::Playing), close_plan)
            .add_systems(Update, plan_input.run_if(in_state(AppState::Playing)));
    }
}

#[derive(Resource, Default)]
pub struct CargoPlan {
    pub open: bool,
    /// Gerade geöffnet: die Taste, die es geöffnet hat, zählt hier nicht.
    pub opened: bool,
    /// Gewähltes Frachtstück (Kennung) und Zielmodul.
    pub sel: Option<u32>,
    pub target: usize,
    /// Abwurf angefragt (zweiter Druck wirft ab).
    pub confirm_drop: bool,
    sig: u64,
}

#[derive(Component)]
pub struct CargoPlanRoot;

fn close_plan(
    mut commands: Commands,
    mut plan: ResMut<CargoPlan>,
    q: Query<Entity, With<CargoPlanRoot>>,
) {
    plan.open = false;
    for e in &q {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn plan_input(
    mut commands: Commands,
    input: Res<MenuInput>,
    sim: Res<Sim>,
    map: Res<MapOpen>,
    mut paused: ResMut<Paused>,
    mut plan: ResMut<CargoPlan>,
    mut pending: ResMut<PendingCommands>,
    roots: Query<Entity, With<CargoPlanRoot>>,
) {
    let s = &sim.0;
    if !plan.open || map.0 || s.ship.destroyed {
        if plan.open && s.ship.destroyed {
            plan.open = false;
        }
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    }
    let ids: Vec<u32> = s.ship.cargo.iter().map(|c| c.id).collect();
    let n_pods = s.ship.pods.len();
    let mut idx = plan
        .sel
        .and_then(|id| ids.iter().position(|&x| x == id))
        .unwrap_or(0);
    if plan.opened {
        plan.opened = false;
        plan.confirm_drop = false;
        plan.sig = 0;
        idx = 0;
        plan.target = ids
            .first()
            .and_then(|&id| s.ship.cargo.iter().find(|c| c.id == id))
            .map_or(0, |c| next_pod(&s.ship, c.pod, 1));
    } else if input.escape || input.back {
        plan.open = false;
        paused.0 = false;
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    } else if !ids.is_empty() {
        let before = idx;
        if input.up && idx > 0 {
            idx -= 1;
        }
        if input.down && idx + 1 < ids.len() {
            idx += 1;
        }
        let item = &s.ship.cargo[idx];
        if idx != before {
            plan.confirm_drop = false;
            plan.target = next_pod(&s.ship, item.pod, 1);
        }
        if n_pods > 1 {
            if input.right {
                plan.target = next_pod(&s.ship, plan.target, 1);
                if plan.target == item.pod {
                    plan.target = next_pod(&s.ship, plan.target, 1);
                }
                plan.confirm_drop = false;
            }
            if input.left {
                plan.target = next_pod(&s.ship, plan.target, n_pods - 1);
                if plan.target == item.pod {
                    plan.target = next_pod(&s.ship, plan.target, n_pods - 1);
                }
                plan.confirm_drop = false;
            }
        }
        if input.enter || input.confirm {
            plan.confirm_drop = false;
            pending.0.push(Command::MoveCargo {
                id: item.id,
                to: plan.target,
            });
        }
        if input.backspace || input.select {
            if plan.confirm_drop {
                plan.confirm_drop = false;
                pending.0.push(Command::Jettison { id: item.id });
            } else {
                plan.confirm_drop = true;
            }
        }
    }
    plan.sel = ids.get(idx).copied();
    plan.target = plan.target.min(n_pods.saturating_sub(1));
    // Neu aufbauen, wenn sich etwas Sichtbares geändert hat.
    let key = format!(
        "{:?}|{}|{}|{:?}|{}",
        plan.sel,
        plan.target,
        plan.confirm_drop,
        s.ship
            .cargo
            .iter()
            .map(|c| (
                c.id,
                c.pod,
                c.moving.map(|(to, l)| (to, (l * 4.0) as i32)),
                (c.mass * 10.0) as i32,
                (c.cond * 50.0) as i32,
                (c.stress * 50.0) as i32,
            ))
            .collect::<Vec<_>>(),
        s.ship.docked.is_some()
    );
    let h = sig_of(&key);
    if h == plan.sig && !roots.is_empty() {
        return;
    }
    plan.sig = h;
    for e in &roots {
        commands.entity(e).despawn();
    }
    spawn_plan(&mut commands, s, &plan);
}

/// Nächstes Modul in Schrittweite `step` (1 = vorwärts, n-1 = rückwärts).
fn next_pod(ship: &Ship, from: usize, step: usize) -> usize {
    let n = ship.pods.len().max(1);
    (from + step) % n
}

/// Kurzbeschreibung des Zustands eines Stücks (Eigenschaft, Zustand, Belastung, Umladen).
pub fn item_state(c: &CargoItem, ship: &Ship) -> (String, Color) {
    let mut parts: Vec<String> = Vec::new();
    let mut col = MUTED;
    match c.traits {
        CargoTrait::Tank => {
            parts.push(format!(
                "Tank, schwappt {:.0} %",
                c.slosh.length() / 0.7 * 100.0
            ));
            col = TEAL;
        }
        CargoTrait::Fragile => {
            parts.push(format!("empfindlich, Zustand {:.0} %", c.cond * 100.0));
            col = if c.cond < 0.5 {
                BAD
            } else if c.cond < 0.9 {
                WARN
            } else {
                GOOD
            };
        }
        CargoTrait::Unstable => {
            parts.push(format!("instabil, Belastung {:.0} %", c.stress * 100.0));
            col = if c.stress > 0.6 {
                BAD
            } else if c.stress > 0.25 {
                WARN
            } else {
                GOOD
            };
        }
        CargoTrait::None => {}
    }
    if let Some((to, left)) = c.moving {
        parts.push(format!(
            "→ {} in {:.1} s",
            ship.pod_label(to),
            left.max(0.0)
        ));
        col = ACCENT;
    }
    (parts.join(" · "), col)
}

fn spawn_plan(commands: &mut Commands, s: &SimState, plan: &CargoPlan) {
    let ship = &s.ship;
    let sel = plan
        .sel
        .and_then(|id| ship.cargo.iter().find(|c| c.id == id));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(96.0),
                ..default()
            },
            CargoPlanRoot,
            GlobalZIndex(60),
        ))
        .with_children(|r| {
            r.spawn((
                panel_node(Val::Px(470.0)),
                BackgroundColor(BG.with_alpha(0.92)),
                BorderColor::all(BORDER),
            ))
            .with_children(|p| {
                p.spawn(text("LADEPLAN", 22.0, TEXT));
                p.spawn(text(
                    if ship.docked.is_some() {
                        "Angedockt: Umladen geht sofort."
                    } else {
                        "Im Flug: Umladen dauert ein paar Sekunden, die Last wandert durchs Schiff."
                    },
                    12.0,
                    MUTED,
                ));
                spawn_schematic(p, ship, sel, plan.target);
                p.spawn(text(com_text(ship), 13.0, ACCENT));
                // Module mit Ladung.
                for (i, pod) in ship.pods.iter().enumerate() {
                    let load = ship.pod_load(i);
                    p.spawn(text(
                        format!(
                            "Modul {}: {:.1} / {:.1} t",
                            ship.pod_label(i),
                            load,
                            pod.capacity
                        ),
                        13.0,
                        if i == plan.target && sel.is_some() {
                            TEAL
                        } else {
                            MUTED
                        },
                    ));
                }
                p.spawn(Node {
                    height: Val::Px(4.0),
                    ..default()
                });
                if ship.cargo.is_empty() {
                    p.spawn(text("Der Frachtraum ist leer.", 14.0, MUTED));
                }
                for c in &ship.cargo {
                    let chosen = Some(c.id) == plan.sel;
                    let (state, col) = item_state(c, ship);
                    p.spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(if chosen { BG_FOCUS } else { Color::NONE }),
                    ))
                    .with_children(|row| {
                        row.spawn(text(
                            format!(
                                "{} · {:.1} t · {}",
                                s.cargo_label(&c.kind),
                                c.mass,
                                ship.pod_label(c.pod)
                            ),
                            14.0,
                            if chosen { TEXT } else { MUTED },
                        ));
                        if !state.is_empty() {
                            row.spawn(text(state, 12.0, col));
                        }
                    });
                }
                if let Some(c) = sel {
                    let free = ship.pod_free(plan.target);
                    let fits = free + 1e-4 >= c.mass && plan.target != c.pod;
                    p.spawn(text(
                        format!(
                            "Ziel: {} (frei {:.1} t){}",
                            ship.pod_label(plan.target),
                            free.max(0.0),
                            if fits { "" } else { " – passt nicht" }
                        ),
                        13.0,
                        if fits { TEAL } else { WARN },
                    ));
                    if plan.confirm_drop {
                        let msg = if matches!(c.kind, CargoKind::Survivor { .. }) {
                            "Gerettete wirft man nicht ab.".to_string()
                        } else {
                            format!(
                                "Nochmal ⌫: {} abwerfen – treibt hier und lässt sich wieder einsammeln",
                                s.cargo_label(&c.kind)
                            )
                        };
                        p.spawn(text(msg, 13.0, BAD));
                    }
                }
                p.spawn(text(
                    "↑↓ Stück · ←→ Zielmodul · Enter umladen · ⌫/Select abwerfen · Esc schließen",
                    12.0,
                    MUTED,
                ));
            });
        });
}

/// Wo der Schwerpunkt liegt – relativ zur Mitte der Triebwerke (dort schiebt das Schiff).
fn com_text(ship: &Ship) -> String {
    let center = if ship.thrusters.is_empty() {
        Vec2::ZERO
    } else {
        let sum: Vec2 = ship.thrusters.iter().map(|t| t.pos).sum();
        Vec2::new(sum.x / ship.thrusters.len() as f32, 0.0)
    };
    let off = ship.com - center;
    let side = if off.x.abs() < 0.05 {
        "mittig".to_string()
    } else if off.x < 0.0 {
        format!("{:.2} m links", -off.x)
    } else {
        format!("{:.2} m rechts", off.x)
    };
    let hint = if off.x.abs() > 0.25 {
        " – das Schiff dreht beim Geradeausschub weg"
    } else {
        ""
    };
    format!("Schwerpunkt {side}{hint} · {:.1} t gesamt", ship.mass)
}

/// Kleine Draufsicht: Teile grau, Module nach Füllstand, Schwerpunkt als Punkt.
fn spawn_schematic(
    p: &mut ChildSpawnerCommands,
    ship: &Ship,
    sel: Option<&CargoItem>,
    target: usize,
) {
    const W: f32 = 436.0;
    const H: f32 = 150.0;
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for part in &ship.parts {
        min = min.min(part.pos - part.half);
        max = max.max(part.pos + part.half);
    }
    if min.x > max.x {
        return;
    }
    let size = (max - min).max(Vec2::splat(0.5));
    let scale = ((W - 20.0) / size.x).min((H - 20.0) / size.y);
    let mid = (min + max) * 0.5;
    // Schiff zeigt nach oben (+y), UI-y wächst nach unten.
    let to_ui = |v: Vec2| {
        Vec2::new(
            W * 0.5 + (v.x - mid.x) * scale,
            H * 0.5 - (v.y - mid.y) * scale,
        )
    };
    p.spawn((
        Node {
            width: Val::Px(W),
            height: Val::Px(H),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.03)),
    ))
    .with_children(|box_| {
        let rect = |b: &mut ChildSpawnerCommands, c: Vec2, half: Vec2, col: Color| {
            let tl = to_ui(c + Vec2::new(-half.x, half.y));
            b.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(tl.x),
                    top: Val::Px(tl.y),
                    width: Val::Px((half.x * 2.0 * scale).max(2.0)),
                    height: Val::Px((half.y * 2.0 * scale).max(2.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(col),
            ));
        };
        for part in &ship.parts {
            rect(
                box_,
                part.pos,
                part.half,
                Color::srgba(0.6, 0.65, 0.75, 0.18),
            );
        }
        for (i, pod) in ship.pods.iter().enumerate() {
            let fill = (ship.pod_load(i) / pod.capacity.max(0.1)).clamp(0.0, 1.0);
            let base = if Some(i) == sel.map(|c| c.pod) {
                ACCENT
            } else if sel.is_some() && i == target {
                TEAL
            } else {
                Color::srgb(0.45, 0.55, 0.7)
            };
            rect(box_, pod.pos, pod.half, base.with_alpha(0.25 + 0.6 * fill));
        }
        // Schwerpunkt.
        let c = to_ui(ship.com);
        box_.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(c.x - 5.0),
                top: Val::Px(c.y - 5.0),
                width: Val::Px(10.0),
                height: Val::Px(10.0),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(1.0, 0.45, 0.1)),
            BorderColor::all(Color::WHITE),
        ));
    });
}
