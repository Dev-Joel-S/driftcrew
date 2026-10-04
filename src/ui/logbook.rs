//! Logbuch (Punkte 60, 61e, 63): Kapitel des roten Fadens, Sammlung der Artefakte und alle
//! Funde (Archive, Bordbücher, Funkfetzen …) als „X von Y“ – ohne zu verraten, wo der Rest liegt.
//! Geöffnet aus dem Pausemenü; ↑↓ wählt einen Fund, Enter schaltet die Wrack-Regel (61f),
//! Esc schließt.

use bevy::prelude::*;

use super::{
    ACCENT, BG, BG_FOCUS, BORDER, GOOD, MUTED, TEAL, TEXT, WARN, panel_node, sig_of, text,
};
use crate::game::{AppState, PendingCommands, Sim};
use crate::input::MenuInput;
use crate::sim::{Command, SimState};

pub struct LogbookPlugin;

impl Plugin for LogbookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Logbook>()
            .add_systems(OnExit(AppState::Playing), close_logbook)
            .add_systems(Update, logbook_input.run_if(in_state(AppState::Playing)));
    }
}

#[derive(Resource, Default)]
pub struct Logbook {
    pub open: bool,
    /// Gerade geöffnet: die Taste, die es geöffnet hat, zählt hier nicht.
    pub opened: bool,
    pub sel: usize,
    sig: u64,
}

#[derive(Component)]
pub struct LogbookRoot;

/// Zeilen für die Fundliste: (Kategorie, Titel, Text) in Fundreihenfolge.
fn found(sim: &SimState) -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> = sim
        .story
        .logs
        .iter()
        .filter_map(|id| sim.data.story.logs.iter().find(|l| &l.id == id))
        .map(|l| {
            (
                l.source.category().to_string(),
                l.title.clone(),
                l.text.clone(),
            )
        })
        .collect();
    // Entdeckte Routen (79) mit Hinweis und Bestzeit.
    for id in &sim.routes_known {
        let Some(r) = sim.data.world.routes.iter().find(|r| &r.id == id) else {
            continue;
        };
        let best = sim
            .route_best
            .iter()
            .find(|(b, _)| b == id)
            .map(|(_, t)| format!(" Bestzeit {t:.1} s."))
            .unwrap_or_else(|| " Noch nicht am Stück geflogen.".into());
        v.push((
            format!("Route · {}", r.kind.label()),
            r.name.clone(),
            format!("{}{best}", r.hint),
        ));
    }
    v
}

fn close_logbook(
    mut commands: Commands,
    mut book: ResMut<Logbook>,
    q: Query<Entity, With<LogbookRoot>>,
) {
    book.open = false;
    for e in &q {
        commands.entity(e).despawn();
    }
}

pub fn logbook_input(
    mut commands: Commands,
    input: Res<MenuInput>,
    sim: Res<Sim>,
    mut book: ResMut<Logbook>,
    mut pending: ResMut<PendingCommands>,
    roots: Query<Entity, With<LogbookRoot>>,
) {
    if !book.open {
        if !roots.is_empty() {
            for e in &roots {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    let s = &sim.0;
    let n = found(s).len();
    if book.opened {
        // Neuester Fund zuerst ausgewählt.
        book.opened = false;
        book.sel = n.saturating_sub(1);
        book.sig = 0;
    } else if input.escape || input.back {
        book.open = false;
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    } else {
        if input.up && book.sel > 0 {
            book.sel -= 1;
        }
        if input.down && book.sel + 1 < n {
            book.sel += 1;
        }
        if input.enter || input.confirm {
            pending
                .0
                .push(Command::SetStayInWreck(!s.story.stay_in_wreck));
        }
    }
    book.sel = book.sel.min(n.saturating_sub(1));
    let key = format!(
        "{}|{}|{}|{}|{}|{:?}|{}",
        book.sel,
        n,
        s.story.chapter,
        s.story.started,
        s.story.stay_in_wreck,
        s.crew.artifacts,
        s.artifacts_aboard().len()
    ) + &format!("|{}|{}", s.routes_known.len(), s.route_best.len());
    let h = sig_of(&key);
    if h == book.sig && !roots.is_empty() {
        return;
    }
    book.sig = h;
    for e in &roots {
        commands.entity(e).despawn();
    }
    spawn_logbook(&mut commands, s, book.sel);
}

fn spawn_logbook(commands: &mut Commands, s: &SimState, sel: usize) {
    let story = &s.data.story;
    let entries = found(s);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.75)),
            LogbookRoot,
            GlobalZIndex(85),
        ))
        .with_children(|r| {
            r.spawn((
                panel_node(Val::Px(980.0)),
                BackgroundColor(BG.with_alpha(1.0)),
                BorderColor::all(BORDER),
            ))
            .with_children(|p| {
                p.spawn(text("LOGBUCH", 26.0, TEXT));
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(22.0),
                    ..default()
                })
                .with_children(|cols| {
                    // --- links: Kapitel und Artefakte ------------------------------------
                    cols.spawn(Node {
                        width: Val::Px(400.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(5.0),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn(text("KAPITEL", 15.0, ACCENT));
                        for (i, ch) in story.chapters.iter().enumerate() {
                            if i < s.story.chapter {
                                c.spawn(text(format!("✓ {}", ch.title), 14.0, GOOD));
                            } else if i == s.story.chapter {
                                c.spawn(text(format!("▶ {}", ch.title), 16.0, TEXT));
                                if s.story.started {
                                    c.spawn(text(ch.task.clone(), 13.0, TEAL));
                                    c.spawn(text(
                                        format!("„{}“ – {}", ch.intro, ch.speaker),
                                        12.0,
                                        MUTED,
                                    ));
                                } else {
                                    let miss = s.chapter_missing();
                                    c.spawn(text(
                                        format!("Beginnt, sobald: {}", miss.join(", ")),
                                        13.0,
                                        MUTED,
                                    ));
                                }
                            }
                        }
                        if s.story.chapter >= story.chapters.len() {
                            c.spawn(text(
                                "Alle Kapitel erzählt. Die Welt läuft weiter.",
                                13.0,
                                MUTED,
                            ));
                        }
                        c.spawn(Node {
                            height: Val::Px(8.0),
                            ..default()
                        });
                        c.spawn(text(
                            format!(
                                "ARTEFAKTE  {} von {}",
                                s.crew.artifacts.len(),
                                story.artifacts.len()
                            ),
                            15.0,
                            ACCENT,
                        ));
                        let aboard = s.artifacts_aboard();
                        for a in &story.artifacts {
                            if s.crew.artifacts.contains(&a.id) {
                                c.spawn(text(format!("◆ {}", a.name), 14.0, TEXT));
                                c.spawn(text(a.description.clone(), 12.0, MUTED));
                            } else if aboard.contains(&a.id) {
                                c.spawn(text(
                                    format!(
                                        "◇ {} – an Bord, {:.1} t, {}",
                                        a.name,
                                        a.mass,
                                        a.effect.label()
                                    ),
                                    14.0,
                                    WARN,
                                ));
                            } else {
                                c.spawn(text("· unbekannt", 14.0, MUTED));
                            }
                        }
                        c.spawn(Node {
                            height: Val::Px(8.0),
                            ..default()
                        });
                        let rule = if s.story.stay_in_wreck {
                            "Bei Zerstörung bleiben Artefakte an Bord im Wrack (Enter: aus)"
                        } else {
                            "Bei Zerstörung kehren Artefakte an ihren Fundort zurück (Enter: an)"
                        };
                        c.spawn(text(rule, 12.0, MUTED));
                    });
                    // --- rechts: Funde ---------------------------------------------------
                    cols.spawn(Node {
                        width: Val::Px(520.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn(text(
                            format!(
                                "FUNDE  {} von {}",
                                entries.len(),
                                story.logs.len() + s.data.world.routes.len()
                            ),
                            15.0,
                            ACCENT,
                        ));
                        if entries.is_empty() {
                            c.spawn(text(
                                "Noch nichts gefunden. Archive öffnen sich beim ersten Andocken, \
                                 Bordbücher und Funkfetzen fängt der Scanner auf.",
                                13.0,
                                MUTED,
                            ));
                            return;
                        }
                        // Fenster von höchstens 9 Einträgen um die Auswahl.
                        let first = sel.saturating_sub(4).min(entries.len().saturating_sub(9));
                        for (i, (cat, title, _)) in entries.iter().enumerate().skip(first).take(9) {
                            let chosen = i == sel;
                            c.spawn((
                                Node {
                                    padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                                    border_radius: BorderRadius::all(Val::Px(6.0)),
                                    ..default()
                                },
                                BackgroundColor(if chosen { BG_FOCUS } else { Color::NONE }),
                            ))
                            .with_children(|row| {
                                row.spawn(text(
                                    format!("{cat} · {title}"),
                                    14.0,
                                    if chosen { TEXT } else { MUTED },
                                ));
                            });
                        }
                        if let Some((cat, title, body)) = entries.get(sel) {
                            c.spawn(Node {
                                height: Val::Px(6.0),
                                ..default()
                            });
                            c.spawn(text(format!("{title} ({cat})"), 15.0, TEAL));
                            c.spawn(text(body.clone(), 14.0, TEXT));
                        }
                    });
                });
                p.spawn(text(
                    "↑↓ Fund wählen · Enter Wrack-Regel umschalten · Esc schließen",
                    12.0,
                    MUTED,
                ));
            });
        });
}
