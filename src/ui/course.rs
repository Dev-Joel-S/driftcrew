//! Parcours-Anzeige: oben in der Mitte der laufende Lauf (Schritt, Zeit, Strafzeit, Hinweis,
//! Fortschritt beim Stillhalten), nach dem Ziel eine Tafel mit Wertung und Bestenliste.
//! Reine Anzeige – der Lauf selbst steckt in der Simulation.

use bevy::prelude::*;

use super::{ACCENT, BG, BORDER, GOOD, MUTED, Signature, TEXT, WARN, text};
use crate::game::{AppState, MapOpen, Paused, Sim};
use crate::sim::SimState;
use crate::sim::course::{MEDAL_NAMES, fmt_time};
use crate::sim::data::{CourseKind, StepKind};

/// Wie lange die Ergebnistafel stehen bleibt (Sekunden).
const SHOW_SECONDS: f32 = 16.0;

pub struct CoursePlugin;

impl Plugin for CoursePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ResultView>()
            .add_systems(OnEnter(AppState::Playing), spawn_roots)
            .add_systems(OnExit(AppState::Playing), despawn_roots)
            .add_systems(
                Update,
                (update_panel, draw_result).run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Resource, Default)]
struct ResultView {
    key: Option<(usize, u32)>,
    left: f32,
}

#[derive(Component)]
struct CourseRoot;
#[derive(Component)]
struct CoursePanel;
#[derive(Component)]
struct CourseTitle;
#[derive(Component)]
struct CourseTime;
#[derive(Component)]
struct CoursePenalty;
#[derive(Component)]
struct CourseHint;
#[derive(Component)]
struct CourseBar;
#[derive(Component)]
struct CourseFill;
#[derive(Component)]
struct ResultRoot;
#[derive(Component)]
struct CloseButton;

pub fn medal_color(m: u8) -> Color {
    match m {
        3 => Color::srgb(1.0, 0.8, 0.25),
        2 => Color::srgb(0.78, 0.84, 0.92),
        1 => Color::srgb(0.86, 0.55, 0.3),
        _ => MUTED,
    }
}

fn spawn_roots(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-215.0)),
                width: Val::Px(430.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            CourseRoot,
            Pickable::IGNORE,
        ))
        .with_children(|r| {
            r.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                    row_gap: Val::Px(4.0),
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(BG.with_alpha(0.88)),
                BorderColor::all(ACCENT.with_alpha(0.6)),
                CoursePanel,
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                p.spawn((
                    Node {
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|row| {
                    row.spawn((text("", 15.0, TEXT), CourseTitle, Pickable::IGNORE));
                    row.spawn((
                        Node {
                            column_gap: Val::Px(10.0),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|r| {
                        r.spawn((text("", 13.0, WARN), CoursePenalty, Pickable::IGNORE));
                        r.spawn((text("", 20.0, ACCENT), CourseTime, Pickable::IGNORE));
                    });
                });
                p.spawn((text("", 13.0, MUTED), CourseHint, Pickable::IGNORE));
                p.spawn((
                    Node {
                        height: Val::Px(6.0),
                        width: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
                    CourseBar,
                    Pickable::IGNORE,
                ))
                .with_children(|b| {
                    b.spawn((
                        Node {
                            height: Val::Percent(100.0),
                            width: Val::Percent(0.0),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(GOOD),
                        CourseFill,
                        Pickable::IGNORE,
                    ));
                });
            });
        });
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(118.0),
            left: Val::Px(18.0),
            width: Val::Px(330.0),
            ..default()
        },
        ResultRoot,
        Signature(0),
        GlobalZIndex(34),
    ));
}

fn despawn_roots(
    mut commands: Commands,
    q: Query<Entity, Or<(With<CourseRoot>, With<ResultRoot>)>>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Hinweis für den laufenden Schritt: aus den Daten, sonst ein Standardtext.
fn step_hint(s: &SimState, course: usize, step: usize) -> String {
    let c = &s.data.courses.courses[course];
    let Some(st) = c.steps.get(step) else {
        return String::new();
    };
    if !st.hint.is_empty() {
        return st.hint.clone();
    }
    match &st.kind {
        StepKind::Gate { .. } => "Durch das Tor, in Pfeilrichtung.".into(),
        StepKind::Pass { .. } => "Zum markierten Punkt.".into(),
        StepKind::Face { .. } => "Nase auf die Boje richten und halten.".into(),
        StepKind::Hold { max_speed, .. } => {
            format!("Im Feld stillhalten (unter {max_speed:.1} m/s).")
        }
        StepKind::Dock { .. } => "Andocken.".into(),
    }
}

#[allow(clippy::type_complexity)]
fn update_panel(
    sim: Res<Sim>,
    map: Res<MapOpen>,
    mut panel: Query<&mut Node, (With<CoursePanel>, Without<CourseBar>, Without<CourseFill>)>,
    mut bar: Query<&mut Node, (With<CourseBar>, Without<CoursePanel>, Without<CourseFill>)>,
    mut fill: Query<&mut Node, (With<CourseFill>, Without<CoursePanel>, Without<CourseBar>)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<CourseTitle>>,
        Query<&mut Text, With<CourseTime>>,
        Query<&mut Text, With<CoursePenalty>>,
        Query<&mut Text, With<CourseHint>>,
    )>,
) {
    let s = &sim.0;
    let run = s.course.as_ref().filter(|_| !map.0 && !s.ship.destroyed);
    if let Ok(mut n) = panel.single_mut() {
        let want = if run.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if n.display != want {
            n.display = want;
        }
    }
    let Some(r) = run else { return };
    let c = &s.data.courses.courses[r.course];
    // Zeitrennen zeigen die nächste erreichbare Medaille, Training nur die Art.
    let kind = match c.kind {
        CourseKind::Training => "Training · ",
        CourseKind::Race => "",
    };
    let set = |t: &mut Text, v: String| {
        if t.0 != v {
            t.0 = v;
        }
    };
    let (title, time, penalty, hint) = if r.started() {
        let n = c.steps.len() - 1;
        let medal = match c.medals {
            Some((g, sv, b)) => {
                let total = r.time + r.penalty;
                let (name, t) = if total <= g {
                    ("Gold", g)
                } else if total <= sv {
                    ("Silber", sv)
                } else {
                    ("Bronze", b)
                };
                format!("  ·  {name} {}", fmt_time(t))
            }
            None => String::new(),
        };
        (
            format!("{kind}{}  ·  Schritt {}/{n}{medal}", c.name, r.step),
            fmt_time(r.time),
            if r.penalty > 0.0 {
                format!("+{:.1} s", r.penalty)
            } else {
                String::new()
            },
            step_hint(s, r.course, r.step),
        )
    } else {
        let dist = s
            .step_target(r.course, 0)
            .map(|t| (t - s.ship.pos).length())
            .unwrap_or(0.0);
        (
            format!("{kind}{}  ·  bereit", c.name),
            String::new(),
            String::new(),
            format!(
                "{}  ({dist:.0} m bis zum Starttor)",
                step_hint(s, r.course, 0)
            ),
        )
    };
    if let Ok(mut t) = texts.p0().single_mut() {
        set(&mut t, title);
    }
    if let Ok(mut t) = texts.p1().single_mut() {
        set(&mut t, time);
    }
    if let Ok(mut t) = texts.p2().single_mut() {
        set(&mut t, penalty);
    }
    if let Ok(mut t) = texts.p3().single_mut() {
        set(&mut t, hint);
    }
    let frac = s.course_hold_fraction();
    if let Ok(mut n) = bar.single_mut() {
        let want = if frac.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if n.display != want {
            n.display = want;
        }
    }
    if let (Some(f), Ok(mut n)) = (frac, fill.single_mut()) {
        n.width = Val::Percent(f * 100.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_result(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    paused: Res<Paused>,
    map: Res<MapOpen>,
    mut view: ResMut<ResultView>,
    mut root: Query<(Entity, &mut Signature), With<ResultRoot>>,
    close: Query<&Interaction, (Changed<Interaction>, With<CloseButton>)>,
) {
    let Ok((root, mut sig)) = root.single_mut() else {
        return;
    };
    let s = &sim.0;
    if let Some(r) = &s.course_result
        && view.key != Some((r.course, r.run))
    {
        view.key = Some((r.course, r.run));
        view.left = SHOW_SECONDS;
    }
    if !paused.0 {
        view.left -= time.delta_secs();
    }
    if close.iter().any(|i| *i == Interaction::Pressed) {
        view.left = 0.0;
    }
    // Ein neuer Lauf blendet die alte Tafel aus.
    let running = s.course.as_ref().is_some_and(|r| r.started());
    let visible = view.left > 0.0 && !map.0 && !paused.0 && !running && s.course_result.is_some();
    if !visible {
        if sig.0 != 0 {
            sig.0 = 0;
            commands.entity(root).despawn_children();
        }
        return;
    }
    let r = s.course_result.as_ref().unwrap();
    let h = super::sig_of(&format!("{}|{}", r.course, r.run));
    if sig.0 == h {
        return;
    }
    sig.0 = h;
    commands.entity(root).despawn_children();
    let c = &s.data.courses.courses[r.course];
    let rec = &s.records[r.course];
    commands.entity(root).with_children(|root| {
        root.spawn((
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                row_gap: Val::Px(3.0),
                ..super::panel_node(Val::Percent(100.0))
            },
            BackgroundColor(BG),
            BorderColor::all(BORDER),
        ))
        .with_children(|p| {
            p.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn(text("PARCOURS GESCHAFFT", 13.0, GOOD));
                row.spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
                    CloseButton,
                ))
                .with_children(|b| {
                    b.spawn(text("✕", 13.0, MUTED));
                });
            });
            p.spawn(text(c.name.clone(), 19.0, TEXT));
            p.spawn(Node {
                column_gap: Val::Px(10.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn(text(fmt_time(r.total), 24.0, ACCENT));
                if r.medal > 0 {
                    let mc = medal_color(r.medal);
                    row.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                            border: UiRect::all(Val::Px(1.5)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BorderColor::all(mc),
                        children![text(
                            if r.new_medal {
                                format!("{} – neu!", MEDAL_NAMES[r.medal as usize])
                            } else {
                                MEDAL_NAMES[r.medal as usize].to_string()
                            },
                            13.0,
                            mc
                        )],
                    ));
                }
                if r.rank == Some(0) {
                    row.spawn(text("Bestzeit", 13.0, GOOD));
                }
            });
            let mut parts = vec![format!("Flugzeit {}", fmt_time(r.time))];
            if r.penalty > 0.0 {
                let mut why = Vec::new();
                if r.collisions > 0 {
                    why.push(format!("{} Kollisionen", r.collisions));
                }
                if r.dock.is_some() {
                    why.push("Andocken".to_string());
                }
                parts.push(format!(
                    "Strafzeit +{:.1} s ({})",
                    r.penalty,
                    why.join(", ")
                ));
            }
            p.spawn(text(parts.join("  ·  "), 12.0, TEXT));
            if let Some((off, speed)) = r.dock {
                p.spawn(text(
                    format!("Andocken: Versatz {off:.2} m · Aufsetzen {speed:.2} m/s"),
                    12.0,
                    MUTED,
                ));
            }
            if r.prize > 0 {
                let why = if c.kind == CourseKind::Training {
                    "Ausbildungszuschuss"
                } else {
                    "Medaillenprämie"
                };
                p.spawn(text(format!("+{} Credits ({why})", r.prize), 13.0, ACCENT));
            }
            if let Some((g, sv, b)) = c.medals
                && rec.medal < 3
            {
                let (name, t) = match rec.medal {
                    0 => ("Bronze", b),
                    1 => ("Silber", sv),
                    _ => ("Gold", g),
                };
                p.spawn(text(
                    format!("Nächste Medaille: {name} unter {}", fmt_time(t)),
                    12.0,
                    MUTED,
                ));
            }
            p.spawn((
                Node {
                    height: Val::Px(1.0),
                    margin: UiRect::vertical(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(BORDER.with_alpha(0.3)),
            ));
            p.spawn(text("BESTENLISTE", 11.0, MUTED));
            for (i, e) in rec.entries.iter().enumerate() {
                let mine = r.rank == Some(i);
                let col = if mine { ACCENT } else { TEXT };
                p.spawn(Node {
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: Val::Px(22.0),
                            ..default()
                        },
                        children![text(format!("{}.", i + 1), 13.0, col)],
                    ));
                    row.spawn((
                        Node {
                            width: Val::Px(62.0),
                            ..default()
                        },
                        children![text(fmt_time(e.total), 13.0, col)],
                    ));
                    row.spawn(text(
                        format!("{} · Crew {} · Lauf {}", e.ship, e.crew, e.run),
                        12.0,
                        if mine { ACCENT } else { MUTED },
                    ));
                });
            }
            if r.rank.is_none() {
                p.spawn(text("Diesmal nicht unter den besten fünf.", 12.0, MUTED));
            }
        });
    });
}
