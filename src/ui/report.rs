//! Auswertung nach einem erledigten Auftrag: Belohnung, Ruf, Eckdaten und kleine
//! Auszeichnungen pro Slot (wer am meisten geschoben hat, wer am häufigsten angeeckt ist …).

use bevy::prelude::*;

use super::{ACCENT, BORDER, GOOD, MUTED, Signature, TEXT, fmt_num, panel, text};
use crate::game::{AppState, MapOpen, Paused, Sim};
use crate::input::{ActiveBindings, Crew, device_badge, player_color};
use crate::render::slot_color;
use crate::sim::{SimState, thruster_label};

/// Wie lange die Auswertung stehen bleibt (Sekunden).
const SHOW_SECONDS: f32 = 14.0;

pub struct ReportPlugin;

impl Plugin for ReportPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReportView>()
            .add_systems(OnEnter(AppState::Playing), spawn_root)
            .add_systems(OnExit(AppState::Playing), despawn_root)
            .add_systems(Update, draw_report.run_if(in_state(AppState::Playing)));
    }
}

/// Welche Auswertung gerade gezeigt wird und wie lange noch (reine Anzeige).
#[derive(Resource, Default)]
struct ReportView {
    mission: Option<u32>,
    left: f32,
}

#[derive(Component)]
struct ReportRoot;

#[derive(Component)]
struct CloseButton;

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-390.0)),
            width: Val::Px(480.0),
            ..default()
        },
        ReportRoot,
        Signature(0),
        GlobalZIndex(35),
    ));
}

fn despawn_root(mut commands: Commands, q: Query<Entity, With<ReportRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Name eines Slots am aktuellen Schiff („Außen L“, „Kran“ …).
fn slot_name(s: &SimState, slot: u8) -> String {
    let n = s.ship.thrusters.len();
    if let Some(i) = s.ship.thrusters.iter().position(|t| t.slot == slot) {
        return thruster_label(i, n);
    }
    s.ship
        .tools
        .iter()
        .find(|t| t.slot == slot)
        .map(|t| t.kind.label().to_string())
        .unwrap_or_else(|| format!("Slot {}", slot + 1))
}

#[allow(clippy::too_many_arguments)]
fn draw_report(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    paused: Res<Paused>,
    map: Res<MapOpen>,
    crew: Res<Crew>,
    bindings: Res<ActiveBindings>,
    mut view: ResMut<ReportView>,
    mut root: Query<(Entity, &mut Signature), With<ReportRoot>>,
    close: Query<&Interaction, (Changed<Interaction>, With<CloseButton>)>,
) {
    let Ok((root, mut sig)) = root.single_mut() else {
        return;
    };
    let s = &sim.0;
    // Neue Auswertung? Dann anzeigen.
    if let Some(r) = &s.report
        && view.mission != Some(r.mission)
    {
        view.mission = Some(r.mission);
        view.left = SHOW_SECONDS;
    }
    if !paused.0 {
        view.left -= time.delta_secs();
    }
    if close.iter().any(|i| *i == Interaction::Pressed) {
        view.left = 0.0;
    }
    let visible = view.left > 0.0 && !map.0 && !paused.0 && s.report.is_some();
    if !visible {
        if sig.0 != 0 {
            sig.0 = 0;
            commands.entity(root).despawn_children();
        }
        return;
    }
    let r = s.report.as_ref().unwrap();
    let h = super::sig_of(&format!("{}|{:?}", r.mission, bindings.0.len()));
    if sig.0 == h {
        return;
    }
    sig.0 = h;
    commands.entity(root).despawn_children();
    let st = &r.stats;
    let mins = (st.time / 60.0).floor() as u32;
    let secs = (st.time % 60.0) as u32;
    let multi = crew.players.len() > 1;
    commands.entity(root).with_children(|root| {
        root.spawn(panel(Val::Percent(100.0))).with_children(|p| {
            p.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn(text("AUFTRAG ERFÜLLT", 14.0, GOOD));
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
            p.spawn(text(r.title.clone(), 20.0, TEXT));
            let rep = r
                .reputation
                .as_ref()
                .map(|(name, lvl, gain)| {
                    format!(
                        "  ·  Ruf bei {name} +{gain} ({})",
                        crate::sim::missions::REP_NAMES[*lvl as usize]
                    )
                })
                .unwrap_or_default();
            p.spawn(text(
                format!("+{} Credits{rep}", fmt_num(r.reward)),
                15.0,
                ACCENT,
            ));
            p.spawn(text(
                format!(
                    "Dauer {mins}:{secs:02}  ·  {} m geflogen  ·  Spitze {:.0} m/s  ·  Schaden {:.0}",
                    fmt_num(st.distance as u32),
                    st.top_speed,
                    st.damage
                ),
                13.0,
                MUTED,
            ));
            p.spawn((
                Node {
                    height: Val::Px(1.0),
                    margin: UiRect::vertical(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(BORDER.with_alpha(0.3)),
            ));
            let awards = r.awards();
            if awards.is_empty() {
                p.spawn(text("Keine besonderen Vorkommnisse.", 13.0, MUTED));
            }
            for a in awards {
                let owner = bindings.0.iter().find(|b| b.slot == a.slot);
                p.spawn(Node {
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: Val::Px(126.0),
                            ..default()
                        },
                        children![text(a.title, 15.0, TEXT)],
                    ));
                    let c = slot_color(a.slot);
                    row.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                            border: UiRect::all(Val::Px(1.5)),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BorderColor::all(c),
                        children![text(slot_name(s, a.slot), 13.0, c)],
                    ));
                    if multi && let Some(b) = owner {
                        let pl = crew.players.get(b.player);
                        let badge = pl
                            .map(|pl| device_badge(pl.device, &pl.label))
                            .unwrap_or_default();
                        row.spawn((
                            Node {
                                padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(6.0)),
                                ..default()
                            },
                            BackgroundColor(player_color(b.player)),
                            children![text(
                                format!("{badge} {}", b.player + 1),
                                11.0,
                                Color::srgb(0.04, 0.06, 0.1)
                            )],
                        ));
                    }
                    row.spawn(text(a.value, 13.0, MUTED));
                });
            }
        });
    });
}
