//! HUD: Kasse, Aufträge, Lebensbalken, Slot-Leiste, Radar, Markierungen, Andockhilfe.

use bevy::prelude::*;

use super::{chip, fmt_num, text, Signature, ACCENT, BAD, BG, BORDER, GOOD, MUTED, TEAL, TEXT, WARN};
use crate::game::{AppState, GameCamera, MapOpen, Paused, Sim};
use crate::input::{ActiveBindings, Crew};
use crate::render::{slot_color, srgb};
use crate::sim::data::StationKind;
use crate::sim::{thruster_label, SimState};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(OnExit(AppState::Playing), despawn_hud)
            .add_systems(
                Update,
                (update_info, update_bars, update_slots, update_radar, update_markers, update_center)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct InfoPanel;
#[derive(Component)]
struct SlotStrip;
#[derive(Component)]
struct SlotBox(u8);
#[derive(Component)]
struct Bar(BarKind);
#[derive(Component)]
struct BarText(BarKind);
#[derive(Clone, Copy, PartialEq)]
enum BarKind {
    Hull,
    Shield,
    Cargo,
}
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct Radar;
#[derive(Component)]
struct RadarDot;
#[derive(Component)]
struct MarkerLayer;
#[derive(Component)]
struct CenterText;
#[derive(Component)]
struct DockGuideText;

const RADAR: f32 = 210.0;
const RADAR_RANGE: f32 = 900.0;

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            HudRoot,
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            // Oben links: Kasse und Aufträge
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(18.0),
                    top: Val::Px(16.0),
                    width: Val::Px(400.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(12.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(BG.with_alpha(0.7)),
                BorderColor::all(BORDER.with_alpha(0.3)),
                InfoPanel,
                Signature(0),
                Pickable::IGNORE,
            ));
            // Unten links: Balken
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(18.0),
                    bottom: Val::Px(18.0),
                    width: Val::Px(300.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::all(Val::Px(12.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(BG.with_alpha(0.7)),
                BorderColor::all(BORDER.with_alpha(0.3)),
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                for (kind, label, color) in [
                    (BarKind::Hull, "HÜLLE", GOOD),
                    (BarKind::Shield, "SCHILD", TEAL),
                    (BarKind::Cargo, "FRACHT", ACCENT),
                ] {
                    p.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn(text(label, 13.0, MUTED));
                        row.spawn((text("", 13.0, TEXT), BarText(kind)));
                    });
                    p.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(9.0),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(color),
                            Bar(kind),
                        ));
                    });
                }
                p.spawn((text("", 14.0, TEXT), StatusText));
            });
            // Unten Mitte: Slot-Leiste
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(18.0),
                    left: Val::Px(340.0),
                    right: Val::Px(250.0),
                    justify_content: JustifyContent::Center,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                SlotStrip,
                Signature(0),
                Pickable::IGNORE,
            ));
            // Oben rechts: Radar
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(18.0),
                    top: Val::Px(16.0),
                    width: Val::Px(RADAR),
                    height: Val::Px(RADAR),
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Percent(50.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(BG.with_alpha(0.75)),
                BorderColor::all(BORDER),
                Radar,
                Pickable::IGNORE,
            ));
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                MarkerLayer,
                Pickable::IGNORE,
            ));
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    top: Val::Percent(30.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|c| {
                c.spawn((text("", 40.0, BAD), CenterText, TextLayout::justify(Justify::Center)));
            });
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    bottom: Val::Px(150.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|c| {
                c.spawn((text("", 16.0, TEXT), DockGuideText, TextLayout::justify(Justify::Center)));
            });
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(18.0),
                    bottom: Val::Px(10.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|c| {
                c.spawn(text("Tab Karte · Esc Pause · Mausrad Zoom", 12.0, MUTED));
            });
        });
}

fn despawn_hud(mut commands: Commands, q: Query<Entity, With<HudRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn region_name(sim: &SimState) -> String {
    let p = sim.ship.pos;
    sim.data
        .world
        .regions
        .iter()
        .min_by(|a, b| {
            let da = (Vec2::new(a.center.0, a.center.1) - p).length() / a.radius;
            let db = (Vec2::new(b.center.0, b.center.1) - p).length() / b.radius;
            da.total_cmp(&db)
        })
        .map(|r| r.name.clone())
        .unwrap_or_default()
}

fn update_info(mut commands: Commands, sim: Res<Sim>, mut q: Query<(Entity, &mut Signature), With<InfoPanel>>) {
    let Ok((e, mut sig)) = q.single_mut() else { return };
    let s = &sim.0;
    let mut lines: Vec<(String, f32, Color)> = vec![
        (format!("◆ {} Credits", fmt_num(s.crew.credits)), 22.0, ACCENT),
        (
            format!("{} · Crew {} · {}", s.data.ship(&s.crew.current_ship).name, s.crew.size, region_name(s)),
            13.0,
            MUTED,
        ),
    ];
    if s.active.is_empty() {
        lines.push(("Keine Aufträge – an einer Station annehmen".into(), 13.0, MUTED));
    } else {
        for m in &s.active {
            lines.push((m.title(s), 15.0, if m.is_distress() { Color::srgb(1.0, 0.45, 0.45) } else { TEXT }));
            lines.push((m.detail(s), 12.0, MUTED));
        }
    }
    let key: String = lines.iter().map(|l| l.0.as_str()).collect::<Vec<_>>().join("|");
    let h = super::sig_of(&key);
    if sig.0 == h {
        return;
    }
    sig.0 = h;
    commands.entity(e).despawn_children();
    commands.entity(e).with_children(|p| {
        for (t, size, c) in lines {
            p.spawn((text(t, size, c), Pickable::IGNORE));
        }
    });
}

fn update_bars(
    sim: Res<Sim>,
    mut bars: Query<(&Bar, &mut Node, &mut BackgroundColor)>,
    mut texts: Query<(&BarText, &mut Text), Without<StatusText>>,
    mut status: Query<&mut Text, (With<StatusText>, Without<BarText>)>,
) {
    let s = &sim.0.ship;
    let frac = |k: BarKind| match k {
        BarKind::Hull => (s.hull / s.max_hull).clamp(0.0, 1.0),
        BarKind::Shield => (s.shield / s.max_shield.max(1.0)).clamp(0.0, 1.0),
        BarKind::Cargo => (s.cargo_mass() / s.cargo_capacity().max(0.1)).clamp(0.0, 1.0),
    };
    for (b, mut n, mut bg) in &mut bars {
        let f = frac(b.0);
        n.width = Val::Percent(f * 100.0);
        if b.0 == BarKind::Hull {
            bg.0 = if f < 0.3 { BAD } else if f < 0.6 { WARN } else { GOOD };
        }
    }
    for (b, mut t) in &mut texts {
        let v = match b.0 {
            BarKind::Hull => format!("{:.0} / {:.0}", s.hull.max(0.0), s.max_hull),
            BarKind::Shield => format!("{:.0} / {:.0}", s.shield, s.max_shield),
            BarKind::Cargo => format!("{:.1} / {:.1} t", s.cargo_mass().max(0.0), s.cargo_capacity()),
        };
        if t.0 != v {
            t.0 = v;
        }
    }
    if let Ok(mut t) = status.single_mut() {
        let ammo = if s.has_tool(crate::sim::data::ToolKind::Cannon) {
            format!("  ·  Munition {}/{}", s.ammo, s.max_ammo)
        } else {
            String::new()
        };
        let v = format!("{:.1} m/s  ·  {:.1} t Masse{}", s.vel.length(), s.mass, ammo);
        if t.0 != v {
            t.0 = v;
        }
    }
}

fn update_slots(
    mut commands: Commands,
    sim: Res<Sim>,
    active: Res<ActiveBindings>,
    crew: Res<Crew>,
    mut strip: Query<(Entity, &mut Signature), With<SlotStrip>>,
    mut boxes: Query<(&SlotBox, &mut BackgroundColor)>,
) {
    let Ok((e, mut sig)) = strip.single_mut() else { return };
    let ship = &sim.0.ship;
    let key = format!("{:?}|{}", active.0, ship.slot_count);
    let h = super::sig_of(&key);
    if sig.0 != h {
        sig.0 = h;
        commands.entity(e).despawn_children();
        let mut list: Vec<_> = active.0.clone();
        list.sort_by_key(|b| b.slot);
        commands.entity(e).with_children(|p| {
            for b in list {
                let name = if let Some(i) = ship.thrusters.iter().position(|t| t.slot == b.slot) {
                    thruster_label(i, ship.thrusters.len())
                } else if let Some(t) = ship.tools.iter().find(|t| t.slot == b.slot) {
                    t.kind.label().to_string()
                } else {
                    "?".into()
                };
                let c = slot_color(b.slot);
                let who = if crew.players.len() > 1 { format!("S{}", b.player + 1) } else { String::new() };
                p.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        min_width: Val::Px(74.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(BG),
                    BorderColor::all(c),
                    SlotBox(b.slot),
                    Pickable::IGNORE,
                ))
                .with_children(|bx| {
                    bx.spawn((text(b.btn.label(), 17.0, TEXT), Pickable::IGNORE));
                    bx.spawn((text(format!("{name} {who}"), 11.0, c), Pickable::IGNORE));
                });
            }
        });
    }
    for (sb, mut bg) in &mut boxes {
        let on = ship.thrusters.iter().any(|t| t.slot == sb.0 && t.firing) || ship.tools.iter().any(|t| t.slot == sb.0 && t.pressed);
        bg.0 = if on { slot_color(sb.0).with_alpha(0.55) } else { BG };
    }
}

struct Poi {
    pos: Vec2,
    color: Color,
    size: f32,
    label: String,
    round: bool,
}

fn pois(sim: &SimState) -> Vec<Poi> {
    let mut v = Vec::new();
    for st in &sim.world.stations {
        let color = match st.kind {
            StationKind::Shipyard => Color::srgb(1.0, 0.55, 0.15),
            StationKind::Outpost => Color::srgb(0.3, 1.0, 0.85),
            StationKind::Station => TEAL,
        };
        v.push(Poi {
            pos: st.pos,
            color,
            size: 9.0,
            label: st.name.clone(),
            round: false,
        });
    }
    for (i, p) in sim.world.planets.iter().enumerate() {
        let c = sim.data.world.planets[i].atmosphere.clone();
        v.push(Poi {
            pos: p.pos,
            color: srgb(crate::sim::data::hex(&c)),
            size: (p.radius / 6.0).clamp(7.0, 14.0),
            label: format!("{} ({})", p.name, p.ore.label()),
            round: true,
        });
    }
    for an in &sim.world.anomalies {
        v.push(Poi {
            pos: an.pos,
            color: Color::srgb(1.0, 0.2, 0.2),
            size: 9.0,
            label: an.name.clone(),
            round: true,
        });
    }
    v
}

fn update_radar(mut commands: Commands, sim: Res<Sim>, time: Res<Time>, radar: Query<Entity, With<Radar>>, dots: Query<Entity, With<RadarDot>>) {
    let Ok(radar) = radar.single() else { return };
    for d in &dots {
        commands.entity(d).despawn();
    }
    let s = &sim.0;
    let me = s.ship.pos;
    let half = RADAR * 0.5;
    let scale = (half - 8.0) / RADAR_RANGE;
    let place = |p: Vec2| {
        let mut d = (p - me) * scale;
        if d.length() > half - 8.0 {
            d = d.normalize() * (half - 8.0);
        }
        Vec2::new(half + d.x, half - d.y)
    };
    let mut spawn_dot = |c: &mut Commands, at: Vec2, size: f32, color: Color, round: bool| {
        let e = c
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(at.x - size * 0.5),
                    top: Val::Px(at.y - size * 0.5),
                    width: Val::Px(size),
                    height: Val::Px(size),
                    border_radius: BorderRadius::all(Val::Px(if round { size } else { 2.0 })),
                    ..default()
                },
                BackgroundColor(color),
                RadarDot,
                Pickable::IGNORE,
            ))
            .id();
        c.entity(radar).add_child(e);
    };
    for p in pois(s) {
        spawn_dot(&mut commands, place(p.pos), p.size, p.color, p.round);
    }
    let blink = (time.elapsed_secs() * 3.0).sin() > 0.0;
    for m in &s.active {
        if let Some(t) = m.nav_target(s) {
            if blink {
                spawn_dot(&mut commands, place(t), 10.0, ACCENT, true);
            }
        }
    }
    for b in &s.bodies {
        if matches!(b.kind, crate::sim::BodyKind::Meteor) && (b.pos - me).length() < RADAR_RANGE * 0.4 {
            spawn_dot(&mut commands, place(b.pos), 3.0, Color::srgb(1.0, 0.5, 0.2), true);
        }
    }
    spawn_dot(&mut commands, Vec2::splat(half), 8.0, Color::WHITE, true);
}

#[allow(clippy::too_many_arguments)]
fn update_markers(
    mut commands: Commands,
    sim: Res<Sim>,
    map: Res<MapOpen>,
    paused: Res<Paused>,
    windows: Query<&Window>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    layer: Query<Entity, With<MarkerLayer>>,
    old: Query<Entity, With<MarkerItem>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let (Ok(layer), Ok((cam, cam_t)), Ok(win)) = (layer.single(), cam.single(), windows.single()) else {
        return;
    };
    if map.0 || paused.0 {
        return;
    }
    let s = &sim.0;
    let size = Vec2::new(win.width(), win.height());
    let mut add = |c: &mut Commands, world: Vec2, label: String, color: Color, arrow_always: bool| {
        let Ok(p) = cam.world_to_viewport(cam_t, world.extend(0.0)) else { return };
        let margin = 40.0;
        let on_screen = p.x > margin && p.y > margin && p.x < size.x - margin && p.y < size.y - margin;
        let dist = (world - s.ship.pos).length();
        if on_screen && !arrow_always {
            let e = c
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(p.x - 100.0),
                        top: Val::Px(p.y - 40.0),
                        width: Val::Px(200.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    MarkerItem,
                    Pickable::IGNORE,
                ))
                .with_children(|n| {
                    n.spawn((text(label, 14.0, color), TextLayout::justify(Justify::Center), Pickable::IGNORE));
                })
                .id();
            c.entity(layer).add_child(e);
        } else if !on_screen {
            let center = size * 0.5;
            let dir = (p - center).normalize_or_zero();
            let t = ((size.x * 0.5 - margin) / dir.x.abs().max(1e-3)).min((size.y * 0.5 - margin) / dir.y.abs().max(1e-3));
            let mut at = center + dir * t;
            if at.x > size.x - 250.0 && at.y < 250.0 {
                at.y = 250.0;
            }
            let arrow = match (dir.x.abs() > dir.y.abs(), dir.x > 0.0, dir.y > 0.0) {
                (true, true, _) => "▶",
                (true, false, _) => "◀",
                (false, _, true) => "▼",
                (false, _, false) => "▲",
            };
            let e = c
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(at.x - 90.0),
                        top: Val::Px(at.y - 12.0),
                        width: Val::Px(180.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    MarkerItem,
                    Pickable::IGNORE,
                ))
                .with_children(|n| {
                    n.spawn((text(arrow, 16.0, color), Pickable::IGNORE));
                    n.spawn((text(format!("{label} {:.0} m", dist), 12.0, color), Pickable::IGNORE));
                })
                .id();
            c.entity(layer).add_child(e);
        }
    };
    for st in &s.world.stations {
        let c = match st.kind {
            StationKind::Shipyard => Color::srgb(1.0, 0.6, 0.2),
            _ => TEAL,
        };
        if st.bounds.expand(20.0).contains(s.ship.pos) {
            continue;
        }
        let anchor = st.pos + Vec2::Y * ((st.bounds.max.y - st.bounds.min.y) * 0.5 + 6.0);
        add(&mut commands, anchor, st.name.clone(), c, false);
    }
    for p in &s.world.planets {
        let anchor = p.pos + Vec2::Y * (p.radius + 8.0);
        // Planeten nur beschriften, wenn sie im Bild sind (sonst wird es zu voll).
        if let Ok(v) = cam.world_to_viewport(cam_t, anchor.extend(0.0)) {
            if v.x > 0.0 && v.y > 0.0 && v.x < size.x && v.y < size.y {
                add(&mut commands, anchor, format!("{} · {}", p.name, p.ore.label()), MUTED, false);
            }
        }
    }
    for m in &s.active {
        if let Some(t) = m.nav_target(s) {
            let label = if m.is_distress() { "Notruf" } else { "Ziel" };
            add(&mut commands, t, label.to_string(), ACCENT, true);
        }
    }
}

#[derive(Component)]
struct MarkerItem;

fn update_center(
    sim: Res<Sim>,
    mut center: Query<&mut Text, (With<CenterText>, Without<DockGuideText>)>,
    mut guide: Query<(&mut Text, &mut TextColor), (With<DockGuideText>, Without<CenterText>)>,
) {
    let s = &sim.0;
    if let Ok(mut t) = center.single_mut() {
        let v = if s.ship.destroyed {
            format!("SCHIFF ZERSTÖRT\nBergung in {:.0} …", s.ship.respawn_timer.max(0.0).ceil())
        } else {
            String::new()
        };
        if t.0 != v {
            t.0 = v;
        }
    }
    if let Ok((mut t, mut c)) = guide.single_mut() {
        let v = if let Some(pad) = s.ship.docked {
            let name = s.world.owner_name(s.world.pads[pad].owner).to_string();
            c.0 = GOOD;
            format!("Angedockt: {name}  ·  ein Triebwerk zünden zum Abdocken")
        } else if let Some(g) = s.dock_guide() {
            let mark = |ok: bool| if ok { "✓" } else { "✗" };
            c.0 = if g.speed_ok && g.angle_ok && g.spin_ok { GOOD } else { WARN };
            format!(
                "ANDOCKEN  Tempo {} {:.1}  ·  Ausrichtung {}  ·  Drehung {}",
                mark(g.speed_ok),
                s.ship.vel.length(),
                mark(g.angle_ok),
                mark(g.spin_ok)
            )
        } else {
            String::new()
        };
        if t.0 != v {
            t.0 = v;
        }
    }
    let _ = chip(Color::WHITE, 1.0);
}
