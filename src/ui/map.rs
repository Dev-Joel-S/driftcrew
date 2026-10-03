//! Sektorkarte (Tab / Select): alle Orte, Regionen, Aufträge und Notrufe.

use bevy::prelude::*;

use super::{
    ACCENT, BG, BORDER, Item, ItemButton, MENU_MAP, MUTED, MenuFocus, Signature, TEAL, TEXT,
    fmt_num, mouse_pick, navigate, panel, spawn_items, text,
};
use crate::game::{AppState, MapOpen, Paused, PendingCommands, Sim};
use crate::input::MenuInput;
use crate::render::srgb;
use crate::sim::Command;
use crate::sim::data::{StationKind, hex};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::Playing), close_map)
            .add_systems(
                Update,
                (toggle_map, draw_map, map_input)
                    .chain()
                    .after(super::pause::pause_input)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct MapRoot;

#[derive(Clone, Debug, PartialEq)]
enum MapAct {
    Accept(u32),
    Abandon(u32),
}

const MAP: f32 = 620.0;

fn toggle_map(
    mut commands: Commands,
    input: Res<MenuInput>,
    paused: Res<Paused>,
    mut open: ResMut<MapOpen>,
    q: Query<Entity, With<MapRoot>>,
    mut focus: ResMut<MenuFocus>,
) {
    if paused.0 {
        return;
    }
    let toggle = input.tab || input.select || (open.0 && (input.escape || input.back));
    if !toggle {
        return;
    }
    open.0 = !open.0;
    info!("Karte {}", if open.0 { "auf" } else { "zu" });
    for e in &q {
        commands.entity(e).despawn();
    }
    if open.0 {
        focus.0[MENU_MAP] = 0;
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.72)),
            MapRoot,
            Signature(0),
            GlobalZIndex(60),
        ));
    }
}

fn close_map(mut commands: Commands, q: Query<Entity, With<MapRoot>>, mut open: ResMut<MapOpen>) {
    open.0 = false;
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn items(sim: &crate::sim::SimState) -> Vec<Item<MapAct>> {
    let mut v = Vec::new();
    for m in &sim.active {
        v.push(
            Item::new(format!("✓ {}", m.title(sim)), MapAct::Abandon(m.id))
                .right("abbrechen")
                .detail(m.detail(sim)),
        );
    }
    for m in sim.offers.iter().filter(|m| m.is_distress()) {
        v.push(
            Item::new(m.title(sim), MapAct::Accept(m.id))
                .right(format!("+{} Cr", m.reward))
                .detail("Notruf – überall annehmbar")
                .enabled(sim.active.len() < crate::sim::missions::MAX_ACTIVE),
        );
    }
    v
}

fn map_input(
    input: Res<MenuInput>,
    open: Res<MapOpen>,
    sim: Res<Sim>,
    buttons: Query<(&Interaction, &ItemButton), Changed<Interaction>>,
    mut focus: ResMut<MenuFocus>,
    mut pending: ResMut<PendingCommands>,
) {
    if !open.0 {
        return;
    }
    let its = items(&sim.0);
    let mut f = focus.0[MENU_MAP];
    let mut act = if navigate(
        &mut f,
        its.len(),
        &MenuInput {
            confirm: input.enter,
            ..input.clone()
        },
    ) {
        Some(f)
    } else {
        None
    };
    if let Some(i) = mouse_pick(MENU_MAP, &mut f, &buttons) {
        act = Some(i);
    }
    focus.0[MENU_MAP] = f;
    if let Some(it) = act.and_then(|i| its.get(i)).filter(|it| it.enabled) {
        match it.action.clone() {
            Some(MapAct::Accept(id)) => pending.0.push(Command::AcceptMission { id }),
            Some(MapAct::Abandon(id)) => pending.0.push(Command::AbandonMission { id }),
            None => {}
        }
    }
}

fn draw_map(
    mut commands: Commands,
    sim: Res<Sim>,
    open: Res<MapOpen>,
    time: Res<Time>,
    mut root: Query<(Entity, &mut Signature), With<MapRoot>>,
) {
    if !open.0 {
        return;
    }
    let Ok((root, mut sig)) = root.single_mut() else {
        warn!("Kartenwurzel fehlt");
        return;
    };
    let s = &sim.0;
    let its = items(s);
    let blink = (time.elapsed_secs() * 2.0) as u32 % 2;
    let key = format!(
        "{:.0}|{:.0}|{}|{:?}|{}",
        s.ship.pos.x / 8.0,
        s.ship.pos.y / 8.0,
        blink,
        its.iter().map(|i| i.label.clone()).collect::<Vec<_>>(),
        s.crew.credits
    );
    let h = super::sig_of(&key);
    if sig.0 == h {
        return;
    }
    sig.0 = h;
    commands.entity(root).despawn_children();
    let r = s.world.radius;
    let scale = (MAP * 0.5 - 10.0) / r;
    let to_map = |p: Vec2| Vec2::new(MAP * 0.5 + p.x * scale, MAP * 0.5 - p.y * scale);
    commands.entity(root).with_children(|root| {
        // Karte
        root.spawn((
            Node {
                width: Val::Px(MAP),
                height: Val::Px(MAP),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Percent(50.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(BORDER),
        ))
        .with_children(|m| {
            let dot = |m: &mut ChildSpawnerCommands,
                       at: Vec2,
                       size: f32,
                       color: Color,
                       round: bool| {
                m.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(at.x - size * 0.5),
                        top: Val::Px(at.y - size * 0.5),
                        width: Val::Px(size),
                        height: Val::Px(size),
                        border_radius: BorderRadius::all(Val::Px(if round { size } else { 3.0 })),
                        ..default()
                    },
                    BackgroundColor(color),
                ));
            };
            let label = |m: &mut ChildSpawnerCommands, at: Vec2, t: String, c: Color| {
                m.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(at.x - 80.0),
                        top: Val::Px(at.y + 7.0),
                        width: Val::Px(160.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    children![(text(t, 12.0, c), TextLayout::justify(Justify::Center))],
                ));
            };
            for reg in &s.data.world.regions {
                let c = srgb(hex(&reg.colors.0)).with_alpha(0.10);
                let at = to_map(Vec2::new(reg.center.0, reg.center.1));
                dot(m, at, reg.radius * scale * 2.0, c, true);
            }
            for f in &s.data.world.asteroid_fields {
                let at = to_map(Vec2::new(f.center.0, f.center.1));
                dot(
                    m,
                    at,
                    f.radius * scale * 2.0,
                    srgb(hex(&f.color)).with_alpha(0.35),
                    true,
                );
                label(m, at, f.name.clone(), MUTED);
            }
            for z in &s.data.world.meteor_zones {
                let at = to_map(Vec2::new(z.center.0, z.center.1));
                dot(
                    m,
                    at,
                    z.radius * scale * 2.0,
                    Color::srgba(1.0, 0.4, 0.1, 0.18),
                    true,
                );
                label(m, at, format!("⚠ {}", z.name), Color::srgb(1.0, 0.55, 0.3));
            }
            for (i, p) in s.world.planets.iter().enumerate() {
                let at = to_map(p.pos);
                let c = srgb(hex(&s.data.world.planets[i].atmosphere));
                dot(m, at, (p.radius * scale * 2.0).max(12.0), c, true);
                label(m, at, format!("{} · {}", p.name, p.ore.label()), c);
            }
            for an in &s.world.anomalies {
                let at = to_map(an.pos);
                dot(
                    m,
                    at,
                    an.radius * scale * 2.0,
                    Color::srgba(0.6, 0.6, 0.65, 0.35),
                    true,
                );
                dot(m, at, 6.0, Color::srgb(1.0, 0.2, 0.15), true);
                label(m, at, an.name.clone(), Color::srgb(1.0, 0.4, 0.4));
            }
            for st in &s.world.stations {
                let at = to_map(st.pos);
                let c = match st.kind {
                    StationKind::Shipyard => Color::srgb(1.0, 0.55, 0.15),
                    _ => TEAL,
                };
                dot(m, at, 12.0, c, false);
                label(m, at, st.name.clone(), c);
            }
            for mi in &s.active {
                if let Some(t) = mi.nav_target(s)
                    && blink == 0
                {
                    dot(m, to_map(t), 12.0, ACCENT, true);
                }
            }
            for mi in s.offers.iter().filter(|m| m.is_distress()) {
                let site = match &mi.kind {
                    crate::sim::missions::MissionKind::Tow { site, .. }
                    | crate::sim::missions::MissionKind::Capsules { site, .. } => Some(*site),
                    _ => None,
                };
                if let Some(site) = site
                    && blink == 1
                {
                    dot(m, to_map(site), 10.0, Color::srgb(1.0, 0.25, 0.25), true);
                }
            }
            let me = to_map(s.ship.pos);
            dot(m, me, 10.0, Color::WHITE, true);
            label(m, me, "Ihr".into(), TEXT);
        });
        // Liste
        root.spawn(panel(Val::Px(460.0))).with_children(|p| {
            p.spawn(text("SEKTORKARTE", 22.0, TEXT));
            p.spawn(text(
                format!(
                    "Kasse {} Credits · {} Aufträge erledigt",
                    fmt_num(s.crew.credits),
                    s.crew.missions_done
                ),
                14.0,
                ACCENT,
            ));
            p.spawn(Node {
                height: Val::Px(6.0),
                ..default()
            });
            if its.is_empty() {
                p.spawn(text("Keine Aufträge oder Notrufe.", 15.0, MUTED));
            }
            spawn_items(p, MENU_MAP, &its);
            p.spawn(text(
                "Notrufe (rot blinkend) lassen sich überall annehmen.",
                13.0,
                MUTED,
            ));
            p.spawn(text(
                "↑↓ Auswahl · Enter/Start annehmen · Tab/Esc schließen",
                12.0,
                MUTED,
            ));
        });
    });
}
