//! HUD: Kasse, Aufträge, Lebensbalken, Slot-Leiste, Radar, Markierungen, Andockhilfe.

use bevy::prelude::*;

use super::{ACCENT, BAD, BG, BORDER, GOOD, MUTED, Signature, TEAL, TEXT, WARN, fmt_num, text};
use crate::game::{AppState, GameCamera, MapOpen, Paused, Sim};
use crate::input::{ActiveBindings, Crew};
use crate::render::{slot_color, srgb};
use crate::sim::data::CargoTrait;
use crate::sim::data::StationKind;
use crate::sim::{SimState, thruster_label};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(OnExit(AppState::Playing), despawn_hud)
            .add_systems(
                Update,
                (
                    update_info,
                    update_bars,
                    update_tint,
                    update_precision,
                    update_slots,
                    update_radar,
                    update_markers,
                    update_center,
                )
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
    Energy,
    Fuel,
    Cargo,
}

const FUEL_COLOR: Color = Color::srgb(0.95, 0.62, 0.2);
const ENERGY_COLOR: Color = Color::srgb(0.65, 0.45, 1.0);
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
/// Anzeige der Präzisionsarbeit (für alle sichtbar).
#[derive(Component)]
struct PrecisionPanel;
#[derive(Component)]
struct PrecisionFill;
#[derive(Component)]
struct PrecisionText;

/// Bildschirmtönung bei Nebel und Sonneneruption.
#[derive(Component)]
struct SectorTint;
/// Kleinere zweite Zeile unter dem großen Mittel-Text.
#[derive(Component)]
struct CenterSub;
#[derive(Component)]
struct DockGuideText;
/// Hintergrund des Andock-Hinweises (ausgeblendet, wenn kein Text da ist).
#[derive(Component)]
struct DockGuideBox;
#[derive(Component)]
struct DockLights;
#[derive(Clone, Copy, PartialEq)]
enum LightKind {
    Speed,
    Angle,
    Spin,
}
#[derive(Component)]
struct LightDot(LightKind);
#[derive(Component)]
struct LightText(LightKind);

pub fn light_color(l: crate::sim::dock::Light) -> Color {
    match l {
        crate::sim::dock::Light::Green => GOOD,
        crate::sim::dock::Light::Yellow => WARN,
        crate::sim::dock::Light::Red => BAD,
    }
}

const RADAR: f32 = 210.0;
pub const RADAR_RANGE: f32 = 900.0;

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
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                SectorTint,
                Pickable::IGNORE,
            ));
            // Präzisionsarbeit: Mitte unten über der Andockanzeige.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(205.0),
                    left: Val::Percent(50.0),
                    margin: UiRect::left(Val::Px(-190.0)),
                    width: Val::Px(380.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(BG.with_alpha(0.85)),
                PrecisionPanel,
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                p.spawn((text("", 13.0, TEXT), PrecisionText, Pickable::IGNORE));
                p.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(8.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
                    Pickable::IGNORE,
                ))
                .with_children(|b| {
                    b.spawn((
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(GOOD),
                        PrecisionFill,
                        Pickable::IGNORE,
                    ));
                });
            });
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
                    (BarKind::Energy, "ZUSATZENERGIE", ENERGY_COLOR),
                    (BarKind::Fuel, "TREIBSTOFF", FUEL_COLOR),
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
                c.spawn((
                    text("", 36.0, BAD),
                    CenterText,
                    TextLayout::justify(Justify::Center),
                    TextShadow::default(),
                ))
                .with_children(|t| {
                    t.spawn((
                        TextSpan::new(""),
                        TextFont::from_font_size(18.0),
                        TextColor(TEXT),
                        CenterSub,
                    ));
                });
            });
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    bottom: Val::Px(150.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|c| {
                c.spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(BG.with_alpha(0.75)),
                    DockGuideBox,
                    Pickable::IGNORE,
                ))
                .with_children(|g| {
                    g.spawn((
                        text("", 16.0, TEXT),
                        DockGuideText,
                        TextLayout::justify(Justify::Center),
                    ));
                });
                // Ampel: Tempo, Winkel, Drehung
                c.spawn((
                    Node {
                        column_gap: Val::Px(10.0),
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(BG.with_alpha(0.75)),
                    DockLights,
                    Pickable::IGNORE,
                ))
                .with_children(|row| {
                    for kind in [LightKind::Speed, LightKind::Angle, LightKind::Spin] {
                        row.spawn((
                            Node {
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|it| {
                            it.spawn((
                                Node {
                                    width: Val::Px(14.0),
                                    height: Val::Px(14.0),
                                    border_radius: BorderRadius::all(Val::Px(7.0)),
                                    ..default()
                                },
                                BackgroundColor(MUTED),
                                LightDot(kind),
                                Pickable::IGNORE,
                            ));
                            it.spawn((text("", 15.0, TEXT), LightText(kind), Pickable::IGNORE));
                        });
                    }
                });
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

fn update_info(
    mut commands: Commands,
    sim: Res<Sim>,
    mut q: Query<(Entity, &mut Signature), With<InfoPanel>>,
) {
    let Ok((e, mut sig)) = q.single_mut() else {
        return;
    };
    let s = &sim.0;
    let mut lines: Vec<(String, f32, Color)> = vec![
        (
            format!("◆ {} Credits", fmt_num(s.crew.credits)),
            22.0,
            ACCENT,
        ),
        (
            format!(
                "{} · Crew {} · {}",
                match s.ship_name() {
                    Some(n) => format!("„{n}“"),
                    None => s.data.ship(&s.crew.current_ship).name.clone(),
                },
                s.crew.size,
                region_name(s)
            ),
            13.0,
            MUTED,
        ),
    ];
    // Besondere Bedingungen im Sektor und laufende Ereignisse.
    let sec = s.sector_at(s.ship.pos);
    if s.flare() {
        let left = s.event.as_ref().map_or(0.0, |e| e.time_left);
        lines.push((
            format!("☀ Sonneneruption – Radar und Scanner gestört ({left:.0} s)"),
            13.0,
            WARN,
        ));
    }
    if sec.nebula > 0.1 {
        lines.push((
            "≋ Nebel – Sicht, Radar und Scanner eingeschränkt".into(),
            13.0,
            Color::srgb(0.75, 0.65, 1.0),
        ));
    }
    if sec.wind.length() > 0.1 {
        lines.push((
            format!(
                "➜ Sonnenwind nach {} ({:.1} m/s²)",
                crate::sim::sector::compass(sec.wind),
                sec.wind.length()
            ),
            13.0,
            Color::srgb(1.0, 0.8, 0.4),
        ));
    }
    if sec.debris > 0.1 {
        lines.push((
            "✦ Trümmerzone – treibender Schrott".into(),
            13.0,
            Color::srgb(0.85, 0.75, 0.65),
        ));
    }
    if s.charts_unsold > 0 {
        lines.push((
            format!(
                "Kartendaten: {} Zellen – an einer Station verkaufen",
                s.charts_unsold
            ),
            12.0,
            TEAL,
        ));
    }
    if s.active.is_empty() {
        lines.push((
            "Keine Aufträge – an einer Station annehmen".into(),
            13.0,
            MUTED,
        ));
    } else {
        for m in &s.active {
            lines.push((
                m.title(s),
                15.0,
                if m.is_distress() {
                    Color::srgb(1.0, 0.45, 0.45)
                } else {
                    TEXT
                },
            ));
            lines.push((m.detail(s), 12.0, MUTED));
        }
    }
    let key: String = lines
        .iter()
        .map(|l| l.0.as_str())
        .collect::<Vec<_>>()
        .join("|");
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

#[allow(clippy::type_complexity)]
fn update_precision(
    sim: Res<Sim>,
    mut panel: Query<&mut Node, (With<PrecisionPanel>, Without<PrecisionFill>)>,
    mut fill: Query<
        (&mut Node, &mut BackgroundColor),
        (With<PrecisionFill>, Without<PrecisionPanel>),
    >,
    mut label: Query<&mut Text, With<PrecisionText>>,
) {
    let p = sim.0.precision.as_ref();
    if let Ok(mut n) = panel.single_mut() {
        let want = if p.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if n.display != want {
            n.display = want;
        }
    }
    let Some(p) = p else { return };
    let on = p.offset < crate::sim::precision::PRECISION_RADIUS;
    if let Ok((mut n, mut bg)) = fill.single_mut() {
        n.width = Val::Percent(p.progress.clamp(0.0, 1.0) * 100.0);
        bg.0 = if on { GOOD } else { WARN };
    }
    if let Ok(mut t) = label.single_mut() {
        let v = format!(
            "PRÄZISION · {} – {}  ·  {:.0} %  ·  Abweichung {:.1} m",
            p.kind.label(),
            if on {
                "genau drauf, ruhig halten"
            } else {
                "abgerutscht!"
            },
            p.progress * 100.0,
            p.offset
        );
        if t.0 != v {
            t.0 = v;
        }
    }
}

fn update_tint(
    sim: Res<Sim>,
    time: Res<Time>,
    mut q: Query<&mut BackgroundColor, With<SectorTint>>,
) {
    let Ok(mut bg) = q.single_mut() else { return };
    let s = &sim.0;
    let sec = s.sector_at(s.ship.pos);
    let t = time.elapsed_secs();
    let c = if s.flare() {
        let k = 0.12 + 0.06 * (t * 3.0).sin().abs();
        Color::srgba(1.0, 0.55, 0.2, k)
    } else if sec.nebula > 0.02 {
        Color::srgba(0.35, 0.25, 0.55, 0.42 * sec.nebula)
    } else {
        Color::NONE
    };
    if bg.0 != c {
        bg.0 = c;
    }
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
        BarKind::Energy => (s.energy / s.max_energy.max(1.0)).clamp(0.0, 1.0),
        BarKind::Fuel => (s.fuel / s.max_fuel.max(1.0)).clamp(0.0, 1.0),
        BarKind::Cargo => (s.cargo_mass() / s.cargo_capacity().max(0.1)).clamp(0.0, 1.0),
    };
    for (b, mut n, mut bg) in &mut bars {
        let f = frac(b.0);
        n.width = Val::Percent(f * 100.0);
        if b.0 == BarKind::Hull {
            bg.0 = if f < 0.3 {
                BAD
            } else if f < 0.6 {
                WARN
            } else {
                GOOD
            };
        }
        if b.0 == BarKind::Fuel {
            bg.0 = if f < 0.2 { BAD } else { FUEL_COLOR };
        }
    }
    for (b, mut t) in &mut texts {
        let v = match b.0 {
            BarKind::Hull => format!("{:.0} / {:.0}", s.hull.max(0.0), s.max_hull),
            BarKind::Shield => format!("{:.0} / {:.0}", s.shield, s.max_shield),
            BarKind::Energy if s.energy < 0.5 => "leer – nur Grundfunktionen".to_string(),
            BarKind::Energy => format!("{:.0} / {:.0}", s.energy, s.max_energy),
            BarKind::Fuel if s.fuel_empty() => "LEER · Notreserve 25 % Schub".to_string(),
            BarKind::Fuel => format!("{:.0} / {:.0}", s.fuel, s.max_fuel),
            BarKind::Cargo => format!(
                "{:.1} / {:.1} t",
                s.cargo_mass().max(0.0),
                s.cargo_capacity()
            ),
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
        // Heikle Fracht an Bord: Zustand bzw. Belastung im Blick behalten (68).
        let delicate: Vec<String> = s
            .cargo
            .iter()
            .filter_map(|c| match c.traits {
                CargoTrait::Fragile => {
                    Some(format!("{} {:.0} %", c.traits.short(), c.cond * 100.0))
                }
                CargoTrait::Unstable => Some(format!(
                    "{} – Belastung {:.0} %",
                    c.traits.short(),
                    c.stress * 100.0
                )),
                CargoTrait::Tank if c.slosh.length() > 0.2 => {
                    Some(format!("{} schwappt", c.traits.short()))
                }
                _ => None,
            })
            .collect();
        // Übersteuerung (69): Hitze der Triebwerke, die gerade warm sind.
        let n = s.thrusters.len();
        let heat: Vec<String> = s
            .thrusters
            .iter()
            .enumerate()
            .filter(|(_, t)| t.heat > 0.05 || t.cool_off > 0.0)
            .map(|(i, t)| {
                let name = crate::sim::thruster_label(i, n);
                if t.cool_off > 0.0 {
                    format!("{name} AUS {:.0} s", t.cool_off.ceil())
                } else {
                    format!("{name} {:.0} %", t.heat * 100.0)
                }
            })
            .collect();
        let heat = if heat.is_empty() {
            String::new()
        } else {
            format!("\nHitze: {}", heat.join(" · "))
        };
        let heat = if sim.0.braking {
            format!("{heat}\nBREMSE")
        } else {
            heat
        };
        let delicate = if delicate.is_empty() {
            String::new()
        } else {
            format!("  ·  Fracht: {}", delicate.join(", "))
        };
        let v = format!(
            "{:.1} m/s  ·  {:.1} t Masse{}{}",
            s.vel.length(),
            s.mass,
            ammo,
            delicate
        ) + &heat;
        if t.0 != v {
            t.0 = v;
        }
    }
}

/// Text in einem Slot-Kästchen (Farbe wechselt beim Drücken).
#[derive(Component)]
struct SlotText {
    slot: u8,
    accent: bool,
}

const DARK_TEXT: Color = Color::srgb(0.04, 0.06, 0.1);

fn update_slots(
    mut commands: Commands,
    sim: Res<Sim>,
    active: Res<ActiveBindings>,
    crew: Res<Crew>,
    mut strip: Query<(Entity, &mut Signature), With<SlotStrip>>,
    mut boxes: Query<(&SlotBox, &mut BackgroundColor)>,
    mut texts: Query<(&SlotText, &mut TextColor)>,
) {
    let Ok((e, mut sig)) = strip.single_mut() else {
        return;
    };
    let ship = &sim.0.ship;
    let labels: Vec<String> = crew.players.iter().map(|p| p.label.clone()).collect();
    // Zustand der Triebwerke in groben Stufen – nur bei Änderung neu aufbauen.
    let wear: Vec<u8> = ship
        .thrusters
        .iter()
        .map(|t| if t.failed() { 2 } else { t.stuttering() as u8 })
        .collect();
    // Bohren: Ruhe der Zielhand in drei Stufen (ruhig, normal, zittrig).
    let steady: Vec<i8> = ship
        .tools
        .iter()
        .map(|t| match &t.drill {
            Some(d) if d.steady >= 1.12 => 1,
            Some(d) if d.steady <= 0.9 => -1,
            Some(_) => 0,
            None => 2,
        })
        .collect();
    let key = format!(
        "{:?}|{}|{:?}|{:?}|{:?}",
        active.0, ship.slot_count, labels, wear, steady
    );
    let h = super::sig_of(&key);
    if sig.0 != h {
        sig.0 = h;
        commands.entity(e).despawn_children();
        let mut list: Vec<_> = active.0.clone();
        list.sort_by_key(|b| b.slot);
        let multi = crew.players.len() > 1;
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
                    bx.spawn((
                        text(b.btn.label(), 17.0, TEXT),
                        SlotText {
                            slot: b.slot,
                            accent: false,
                        },
                        Pickable::IGNORE,
                    ));
                    bx.spawn((
                        text(name, 11.0, c),
                        SlotText {
                            slot: b.slot,
                            accent: true,
                        },
                        Pickable::IGNORE,
                    ));
                    if let Some(t) = ship.thrusters.iter().find(|t| t.slot == b.slot) {
                        let status = if t.failed() {
                            Some(("✕ AUSFALL", BAD))
                        } else if t.stuttering() {
                            Some(("⚠ stottert", WARN))
                        } else {
                            None
                        };
                        if let Some((label, col)) = status {
                            bx.spawn((
                                Node {
                                    margin: UiRect::top(Val::Px(2.0)),
                                    padding: UiRect::axes(Val::Px(4.0), Val::Px(0.0)),
                                    border_radius: BorderRadius::all(Val::Px(4.0)),
                                    ..default()
                                },
                                BackgroundColor(BG),
                                Pickable::IGNORE,
                            ))
                            .with_children(|st| {
                                st.spawn((text(label, 10.0, col), Pickable::IGNORE));
                            });
                        }
                    }
                    if let Some(d) = ship
                        .tools
                        .iter()
                        .find(|t| t.slot == b.slot)
                        .and_then(|t| t.drill.as_ref())
                    {
                        let (label, col) = if d.steady >= 1.12 {
                            ("ruhige Hand ▲", GOOD)
                        } else if d.steady <= 0.9 {
                            ("zittrig ▼", WARN)
                        } else {
                            ("bohrt", TEXT)
                        };
                        bx.spawn((
                            Node {
                                margin: UiRect::top(Val::Px(2.0)),
                                padding: UiRect::axes(Val::Px(4.0), Val::Px(0.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(BG),
                            Pickable::IGNORE,
                        ))
                        .with_children(|st| {
                            st.spawn((text(label, 10.0, col), Pickable::IGNORE));
                        });
                    }
                    // Spieler-Abzeichen: gleiche Tasten verschiedener Geräte unterscheidbar.
                    if multi {
                        let pl = crew.players.get(b.player);
                        let badge = pl
                            .map(|pl| crate::input::device_badge(pl.device, &pl.label))
                            .unwrap_or_default();
                        bx.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                top: Val::Px(-9.0),
                                right: Val::Px(-6.0),
                                padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(6.0)),
                                ..default()
                            },
                            BackgroundColor(crate::input::player_color(b.player)),
                            Pickable::IGNORE,
                        ))
                        .with_children(|bd| {
                            bd.spawn((
                                text(format!("{badge} {}", b.player + 1), 11.0, DARK_TEXT),
                                Pickable::IGNORE,
                            ));
                        });
                    }
                });
            }
        });
    }
    let on = |slot: u8| {
        ship.thrusters.iter().any(|t| t.slot == slot && t.firing)
            || ship.tools.iter().any(|t| t.slot == slot && t.pressed)
    };
    // Ruhezustand: nur Rahmen in Slotfarbe. Gedrückt: ganz in Slotfarbe gefüllt.
    // Übersteuert: orange bis rot nach Hitze; Notabschaltung: dunkelrot.
    for (sb, mut bg) in &mut boxes {
        let th = ship.thrusters.iter().find(|t| t.slot == sb.0);
        bg.0 = match th {
            Some(t) if t.cool_off > 0.0 => Color::srgb(0.45, 0.05, 0.05),
            Some(t) if t.overdrive && t.firing => Color::srgb(1.0, 0.55 - 0.45 * t.heat, 0.1),
            _ if on(sb.0) => slot_color(sb.0),
            _ => BG,
        };
    }
    for (st, mut tc) in &mut texts {
        tc.0 = match (on(st.slot), st.accent) {
            (true, _) => DARK_TEXT,
            (false, false) => TEXT,
            (false, true) => slot_color(st.slot),
        };
    }
}

struct Poi {
    pos: Vec2,
    color: Color,
    size: f32,
    round: bool,
}

fn pois(sim: &SimState) -> Vec<Poi> {
    let mut v = Vec::new();
    for (si, st) in sim.world.stations.iter().enumerate() {
        if !sim.station_known(si) {
            continue;
        }
        let color = match st.kind {
            StationKind::Shipyard => Color::srgb(1.0, 0.55, 0.15),
            StationKind::Outpost => Color::srgb(0.3, 1.0, 0.85),
            StationKind::Station => TEAL,
        };
        v.push(Poi {
            pos: st.pos,
            color,
            size: 9.0,
            round: false,
        });
    }
    for (i, p) in sim.world.planets.iter().enumerate() {
        if !sim.discovered(p.pos) {
            continue;
        }
        let c = sim.data.world.planets[i].atmosphere.clone();
        v.push(Poi {
            pos: p.pos,
            color: srgb(crate::sim::data::hex(&c)),
            size: (p.radius / 6.0).clamp(7.0, 14.0),
            round: true,
        });
    }
    for m in sim.world.monuments.iter().filter(|m| sim.discovered(m.pos)) {
        v.push(Poi {
            pos: m.pos,
            color: srgb(m.accent_color),
            size: 9.0,
            round: false,
        });
    }
    for an in sim.world.anomalies.iter().filter(|a| sim.discovered(a.pos)) {
        v.push(Poi {
            pos: an.pos,
            color: if an.is_black_hole() {
                Color::srgb(0.65, 0.4, 1.0)
            } else {
                Color::srgb(1.0, 0.2, 0.2)
            },
            size: 9.0,
            round: true,
        });
    }
    v
}

fn update_radar(
    mut commands: Commands,
    sim: Res<Sim>,
    time: Res<Time>,
    radar: Query<Entity, With<Radar>>,
    dots: Query<Entity, With<RadarDot>>,
) {
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
    let spawn_dot = |c: &mut Commands, at: Vec2, size: f32, color: Color, round: bool| {
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
    // Nebel und Sonneneruption: Radar sieht nur noch die Nähe und rauscht.
    let sec = s.sector_at(me);
    let disturb = if s.flare() { 0.8 } else { sec.nebula };
    let reach = RADAR_RANGE * (1.0 - 0.75 * disturb);
    for p in pois(s) {
        if (p.pos - me).length() > reach && disturb > 0.05 {
            continue;
        }
        spawn_dot(&mut commands, place(p.pos), p.size, p.color, p.round);
    }
    for b in &s.blips {
        spawn_dot(
            &mut commands,
            place(b.pos),
            5.0,
            Color::srgb(0.35, 0.95, 0.9),
            false,
        );
    }
    if disturb > 0.05 {
        let n = (disturb * 26.0) as u32;
        let seed = (time.elapsed_secs() * 12.0) as u32;
        for k in 0..n {
            let h = crate::sim::rng::hash32(seed.wrapping_mul(31).wrapping_add(k));
            let x = (h % 1000) as f32 / 1000.0 * RADAR;
            let y = ((h / 1000) % 1000) as f32 / 1000.0 * RADAR;
            if (Vec2::new(x, y) - Vec2::splat(half)).length() < half - 6.0 {
                spawn_dot(
                    &mut commands,
                    Vec2::new(x, y),
                    2.0,
                    Color::srgba(0.8, 0.85, 1.0, 0.5),
                    true,
                );
            }
        }
    }
    let blink = (time.elapsed_secs() * 3.0).sin() > 0.0;
    for m in &s.active {
        if let Some(t) = m.nav_target(s)
            && blink
        {
            spawn_dot(&mut commands, place(t), 10.0, ACCENT, true);
        }
    }
    for b in &s.bodies {
        if matches!(b.kind, crate::sim::BodyKind::Meteor)
            && (b.pos - me).length() < RADAR_RANGE * 0.4
        {
            spawn_dot(
                &mut commands,
                place(b.pos),
                3.0,
                Color::srgb(1.0, 0.5, 0.2),
                true,
            );
        }
    }
    // NPC-Schiffe: Piraten rot, Händlerin gold, Rivalen orange, Konvoi gelb, sonst hellblau.
    for n in s.npcs.iter().filter(|n| n.alive) {
        if (n.ship.pos - me).length() > reach.min(RADAR_RANGE) {
            continue;
        }
        spawn_dot(&mut commands, place(n.ship.pos), 5.0, npc_color(n), false);
    }
    spawn_dot(&mut commands, Vec2::splat(half), 8.0, Color::WHITE, true);
}

/// Farbe eines NPC-Schiffs auf Radar, Karte und in Markern.
pub fn npc_color(n: &crate::sim::npc::Npc) -> Color {
    use crate::sim::npc::Role;
    match n.role {
        Role::Drone { .. } => Color::srgb(1.0, 0.25, 0.2),
        Role::Merchant { .. } => Color::srgb(1.0, 0.82, 0.3),
        Role::Rival { .. } => Color::srgb(1.0, 0.5, 0.2),
        Role::Convoy { .. } => Color::srgb(1.0, 0.95, 0.4),
        Role::Miner { .. } => Color::srgb(0.85, 0.75, 0.5),
        Role::Trader { .. } => Color::srgb(0.6, 0.8, 1.0),
        Role::Stranded { .. } => Color::srgb(1.0, 0.35, 0.55),
    }
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
    let (Ok(layer), Ok((cam, cam_t)), Ok(win)) = (layer.single(), cam.single(), windows.single())
    else {
        return;
    };
    if map.0 || paused.0 {
        return;
    }
    let s = &sim.0;
    let size = Vec2::new(win.width(), win.height());
    // Solange das Stationsmenü offen ist, keine Wegmarken dahinter.
    let menu_open = super::station::menu_open(s);
    let blocked = |q: Vec2| menu_open && q.x > size.x - 520.0 && q.y > 225.0 && q.y < size.y - 95.0;
    let course_panel = s.course.is_some();
    let mut placed: Vec<Vec2> = Vec::new();
    let mut add =
        |c: &mut Commands, world: Vec2, label: String, color: Color, arrow_always: bool| {
            let Ok(p) = cam.world_to_viewport(cam_t, world.extend(0.0)) else {
                return;
            };
            let margin = 40.0;
            let on_screen =
                p.x > margin && p.y > margin && p.x < size.x - margin && p.y < size.y - margin;
            let dist = (world - s.ship.pos).length();
            if on_screen && !arrow_always {
                if blocked(p) {
                    return;
                }
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
                        n.spawn((
                            text(label, 14.0, color),
                            TextLayout::justify(Justify::Center),
                            Pickable::IGNORE,
                        ));
                    })
                    .id();
                c.entity(layer).add_child(e);
            } else if !on_screen {
                let center = size * 0.5;
                let dir = (p - center).normalize_or_zero();
                let t = ((size.x * 0.5 - margin) / dir.x.abs().max(1e-3))
                    .min((size.y * 0.5 - margin) / dir.y.abs().max(1e-3));
                let mut at = center + dir * t;
                if at.x > size.x - 250.0 && at.y < 250.0 {
                    at.y = 250.0;
                }
                // Nicht unter die Parcours-Anzeige oben in der Mitte.
                if course_panel && at.y < 125.0 && (at.x - size.x * 0.5).abs() < 310.0 {
                    at.y = 125.0;
                }
                // Nicht über die Slot-Leiste und die Balken legen.
                at.y = at.y.min(size.y - 150.0);
                if at.x < 340.0 {
                    at.y = at.y.min(size.y - 265.0);
                }
                // Am Rand stapeln, damit sich Beschriftungen nicht überlagern.
                let along_y = at.x <= margin + 1.0 || at.x >= size.x - margin - 1.0;
                for _ in 0..8 {
                    if !placed
                        .iter()
                        .any(|q| (q.x - at.x).abs() < 150.0 && (q.y - at.y).abs() < 32.0)
                    {
                        break;
                    }
                    if along_y {
                        at.y += 34.0;
                    } else {
                        at.x += 160.0;
                    }
                }
                placed.push(at);
                if blocked(at) {
                    return;
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
                        n.spawn((
                            text(format!("{label} {:.0} m", dist), 12.0, color),
                            Pickable::IGNORE,
                        ));
                    })
                    .id();
                c.entity(layer).add_child(e);
            }
        };
    for (si, st) in s.world.stations.iter().enumerate() {
        let c = match st.kind {
            StationKind::Shipyard => Color::srgb(1.0, 0.6, 0.2),
            _ => TEAL,
        };
        if st.bounds.expand(20.0).contains(s.ship.pos) || !s.station_known(si) {
            continue;
        }
        let anchor = st.pos + Vec2::Y * ((st.bounds.max.y - st.bounds.min.y) * 0.5 + 6.0);
        add(&mut commands, anchor, st.name.clone(), c, false);
    }
    for p in &s.world.planets {
        let anchor = p.pos + Vec2::Y * (p.radius + 8.0);
        // Planeten nur beschriften, wenn sie im Bild sind (sonst wird es zu voll).
        if let Ok(v) = cam.world_to_viewport(cam_t, anchor.extend(0.0))
            && v.x > 0.0
            && v.y > 0.0
            && v.x < size.x
            && v.y < size.y
        {
            add(
                &mut commands,
                anchor,
                format!("{} · {}", p.name, p.ore.label()),
                MUTED,
                false,
            );
        }
    }
    for m in &s.active {
        if let Some(t) = m.nav_target(s) {
            let label = if m.is_distress() { "Notruf" } else { "Ziel" };
            add(&mut commands, t, label.to_string(), ACCENT, true);
        }
    }
    // Parcours: nächstes Ziel als Pfeil am Rand, sonst Starttore in der Nähe beschriften.
    let flag = Color::srgb(0.45, 1.0, 0.55);
    match &s.course {
        Some(r) => {
            if let Some(t) = s.course_target() {
                add(&mut commands, t, s.step_label(r.course, r.step), flag, true);
            }
        }
        None => {
            // Nur beschriften, wenn das Starttor im Bild ist (bei Nova liegen drei nah beisammen).
            for ci in 0..s.data.courses.courses.len() {
                if let Some(t) = s.step_target(ci, 0)
                    && cam
                        .world_to_viewport(cam_t, t.extend(0.0))
                        .is_ok_and(|v| v.x > 0.0 && v.y > 0.0 && v.x < size.x && v.y < size.y)
                {
                    add(&mut commands, t, s.step_label(ci, 0), flag, false);
                }
            }
        }
    }
    // Scanner-Funde im Bild beschriften (nur Wracks und Vorkommen, sonst wird es zu voll).
    for b in &s.blips {
        use crate::sim::sector::BlipKind;
        if !matches!(
            b.kind,
            BlipKind::Wreck | BlipKind::Deposit(_) | BlipKind::Signal
        ) {
            continue;
        }
        let on_screen = cam
            .world_to_viewport(cam_t, b.pos.extend(0.0))
            .is_ok_and(|v| v.x > 0.0 && v.y > 0.0 && v.x < size.x && v.y < size.y);
        if on_screen {
            add(
                &mut commands,
                b.pos,
                b.kind.label(),
                Color::srgb(0.35, 0.95, 0.9),
                false,
            );
        }
    }
    // Monumente beschriften, sobald entdeckt und im Bild.
    for m in &s.world.monuments {
        if !s.discovered(m.pos) {
            continue;
        }
        let anchor = m.pos + Vec2::Y * ((m.bounds.max.y - m.bounds.min.y) * 0.5 + 6.0);
        let on_screen = cam
            .world_to_viewport(cam_t, anchor.extend(0.0))
            .is_ok_and(|v| v.x > 0.0 && v.y > 0.0 && v.x < size.x && v.y < size.y);
        if on_screen {
            let state = if m.awake { "erwacht" } else { "schlafend" };
            add(
                &mut commands,
                anchor,
                format!("{} · {state}", m.name),
                srgb(m.accent_color),
                false,
            );
        }
    }
    // NPC-Schiffe in der Nähe beschriften (Konvois mit Hüllenzustand).
    for n in s.npcs.iter().filter(|n| n.alive) {
        if (n.ship.pos - s.ship.pos).length() > 220.0 {
            continue;
        }
        let anchor = n.ship.pos + Vec2::Y * (n.ship.bound_radius() + 2.0);
        let on_screen = cam
            .world_to_viewport(cam_t, anchor.extend(0.0))
            .is_ok_and(|v| v.x > 0.0 && v.y > 0.0 && v.x < size.x && v.y < size.y);
        if !on_screen {
            continue;
        }
        let label = match n.role {
            crate::sim::npc::Role::Drone { .. } => "☠ Piratendrohne".to_string(),
            crate::sim::npc::Role::Convoy { .. } => format!(
                "{} · Hülle {:.0} %",
                n.name,
                n.ship.hull / n.ship.max_hull * 100.0
            ),
            _ => format!("{} · {}", n.name, n.label()),
        };
        add(&mut commands, anchor, label, npc_color(n), false);
    }
    // Pings der Crew in Spielerfarbe.
    for p in &s.pings {
        let who = format!("◆ Spieler {}", p.player + 1);
        let label = if p.label.is_empty() {
            who
        } else {
            format!("{who}: {}", p.label)
        };
        add(
            &mut commands,
            p.pos,
            label,
            crate::input::player_color(p.player as usize),
            false,
        );
    }
}

#[derive(Component)]
struct MarkerItem;

fn update_center(
    sim: Res<Sim>,
    mut center: Query<&mut Text, (With<CenterText>, Without<DockGuideText>, Without<LightText>)>,
    mut guide: Query<
        (&mut Text, &mut TextColor),
        (With<DockGuideText>, Without<CenterText>, Without<LightText>),
    >,
    mut lights: Query<&mut Node, (With<DockLights>, Without<DockGuideBox>)>,
    mut guide_box: Query<&mut Node, (With<DockGuideBox>, Without<DockLights>)>,
    mut dots: Query<(&LightDot, &mut BackgroundColor)>,
    mut texts: Query<(&LightText, &mut Text), (Without<CenterText>, Without<DockGuideText>)>,
    mut sub: Query<&mut TextSpan, With<CenterSub>>,
) {
    let s = &sim.0;
    let (head, detail) = if s.ship.destroyed {
        let home = &s.world.stations[s.crew.home_station].name;
        (
            "SCHIFF ZERSTÖRT".to_string(),
            format!(
                "\nRettungskapsel ausgestoßen · Bergung nach {home} in {:.0} s\nBergungskosten {} Cr aus der gemeinsamen Kasse",
                s.ship.respawn_timer.max(0.0).ceil(),
                fmt_num(s.salvage_fee)
            ),
        )
    } else if let Some(c) = &s.customs {
        let name = &s.data.traffic.checkpoints[c.checkpoint].name;
        (
            format!("ZOLLSCAN {:.0} %", c.progress * 100.0),
            format!("\n{name} scannt den Frachtraum – raus aus dem Radius, sonst ist die Ware weg"),
        )
    } else {
        (String::new(), String::new())
    };
    if let Ok(mut t) = center.single_mut()
        && t.0 != head
    {
        t.0 = head;
    }
    if let Ok(mut t) = sub.single_mut()
        && t.0 != detail
    {
        t.0 = detail;
    }
    let guide_data = s.dock_guide().filter(|g| {
        let to_pad = s.world.pads[g.pad].center - s.ship.pos;
        g.distance < 22.0 && (s.ship.vel.length() < 8.0 || s.ship.vel.dot(to_pad) > 0.0)
    });
    if let Ok(mut n) = lights.single_mut() {
        let want = if guide_data.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if n.display != want {
            n.display = want;
        }
    }
    if let Some(g) = &guide_data {
        for (d, mut bg) in &mut dots {
            let l = match d.0 {
                LightKind::Speed => g.speed_light(),
                LightKind::Angle => g.angle_light(),
                LightKind::Spin => g.spin_light(),
            };
            bg.0 = light_color(l);
        }
        for (lt, mut t) in &mut texts {
            let v = match lt.0 {
                LightKind::Speed => format!("Tempo {:.1} m/s", g.speed),
                LightKind::Angle => format!("Winkel {:.0}°", g.angle_deg),
                LightKind::Spin => format!("Drehung {:.1}", g.spin),
            };
            if t.0 != v {
                t.0 = v;
            }
        }
    }
    if let Ok((mut t, mut c)) = guide.single_mut() {
        let v = if super::station::menu_open(s) {
            // Das Stationsmenü erklärt das Abdocken selbst.
            String::new()
        } else if let Some(pad) = s.ship.docked {
            let name = s.world.owner_name(s.world.pads[pad].owner).to_string();
            c.0 = GOOD;
            if s.world.pads[pad].zone {
                format!("Gelandet auf {name}  ·  Werkzeuge frei  ·  Triebwerk zünden = abheben")
            } else {
                format!("Angedockt: {name}  ·  ein Triebwerk zünden zum Abdocken")
            }
        } else if let Some(g) = &guide_data {
            c.0 = light_color(g.overall());
            let side = if !g.in_zone && g.lateral.abs() > 1.0 {
                if g.lateral > 0.0 {
                    "  ·  ← zur Mitte"
                } else {
                    "  ·  zur Mitte →"
                }
            } else {
                ""
            };
            format!(
                "ANDOCKEN  ·  {:.1} m über der Plattform{side}",
                g.height.max(0.0)
            )
        } else {
            String::new()
        };
        let want = if v.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        if let Ok(mut n) = guide_box.single_mut()
            && n.display != want
        {
            n.display = want;
        }
        if t.0 != v {
            t.0 = v;
        }
    }
}
