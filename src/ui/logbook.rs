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
    /// 0 = Geschichte und Funde, 1 = Tagebuch der Crew (Schiff, Plaketten, Erlebnisse, Notizen).
    pub page: u8,
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
    mut entry: ResMut<super::text_entry::TextEntry>,
    mut paused: ResMut<crate::game::Paused>,
) {
    if entry.open {
        return;
    }
    if !book.open {
        if !roots.is_empty() {
            for e in &roots {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    let s = &sim.0;
    let n = if book.page == 0 {
        found(s).len()
    } else {
        s.journal.entries.len()
    };
    if book.opened {
        // Neuester Fund zuerst ausgewählt.
        book.opened = false;
        book.sel = if book.page == 1 {
            0
        } else {
            n.saturating_sub(1)
        };
        book.sig = 0;
    } else if input.escape || input.back {
        book.open = false;
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    } else {
        if input.left || input.right || input.tab {
            book.page = 1 - book.page;
            book.sel = 0;
            book.opened = true;
        }
        if input.up && book.sel > 0 {
            book.sel -= 1;
        }
        if input.down && book.sel + 1 < n {
            book.sel += 1;
        }
        if input.enter || input.confirm {
            if book.page == 0 {
                pending
                    .0
                    .push(Command::SetStayInWreck(!s.story.stay_in_wreck));
            } else {
                entry.start(super::text_entry::Purpose::Note, String::new(), &mut paused);
            }
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
    ) + &format!(
        "|{}|{}|{}|{}|{}",
        s.routes_known.len(),
        s.route_best.len(),
        book.page,
        s.journal.entries.len(),
        s.journal.plaques.len()
    );
    let h = sig_of(&key);
    if h == book.sig && !roots.is_empty() {
        return;
    }
    book.sig = h;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if book.page == 1 {
        spawn_journal(&mut commands, s, book.sel);
    } else {
        spawn_logbook(&mut commands, s, book.sel);
    }
}

/// Seite 2: das Tagebuch der Crew (81, 82).
fn spawn_journal(commands: &mut Commands, s: &SimState, sel: usize) {
    use crate::sim::journal::fmt_time;
    let j = &s.journal;
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
                p.spawn(text("LOGBUCH · TAGEBUCH DER CREW", 26.0, TEXT));
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(22.0),
                    ..default()
                })
                .with_children(|cols| {
                    cols.spawn(Node {
                        width: Val::Px(400.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(5.0),
                        ..default()
                    })
                    .with_children(|c| {
                        let def = s.data.ship(&s.crew.current_ship);
                        c.spawn(text("UNSER SCHIFF", 15.0, ACCENT));
                        c.spawn(text(
                            match s.ship_name() {
                                Some(n) => format!("„{n}“ ({})", def.name),
                                None => format!("{} – noch ohne Namen (Pause → Schiff taufen)", def.name),
                            },
                            16.0,
                            TEXT,
                        ));
                        c.spawn(text(
                            format!(
                                "Flugzeit {} · {} Aufträge erledigt",
                                fmt_time(j.playtime),
                                s.crew.missions_done
                            ),
                            13.0,
                            MUTED,
                        ));
                        c.spawn(Node {
                            height: Val::Px(6.0),
                            ..default()
                        });
                        c.spawn(text(
                            format!("PLAKETTEN  {}", j.plaques.len()),
                            15.0,
                            ACCENT,
                        ));
                        if j.plaques.is_empty() {
                            c.spawn(text(
                                "Noch keine. Besondere Bergungen, Rettungen, erwachte Monumente und Wiederaufbau bringen Andenken für den Rumpf.",
                                13.0,
                                MUTED,
                            ));
                        }
                        for pl in &j.plaques {
                            let here = pl.ship == s.crew.current_ship;
                            c.spawn(text(
                                format!(
                                    "◆ {}{}",
                                    pl.title,
                                    if here {
                                        String::new()
                                    } else {
                                        format!(" (an der {})", s.data.ship(&pl.ship).name)
                                    }
                                ),
                                14.0,
                                Color::srgb(pl.color[0], pl.color[1], pl.color[2]),
                            ));
                        }
                        c.spawn(Node {
                            height: Val::Px(6.0),
                            ..default()
                        });
                        c.spawn(text("REKORDE", 15.0, ACCENT));
                        c.spawn(text(
                            match j.closest_rescue {
                                Some(h) => format!("Knappste Rettung: mit {h:.0} % Hülle"),
                                None => "Knappste Rettung: –".to_string(),
                            },
                            13.0,
                            MUTED,
                        ));
                        c.spawn(text(
                            match j.biggest_salvage {
                                Some(c) => format!("Größte Bergung: {c} Cr"),
                                None => "Größte Bergung: –".to_string(),
                            },
                            13.0,
                            MUTED,
                        ));
                        let sectors = j.firsts.iter().filter(|f| f.starts_with("sector:")).count();
                        c.spawn(text(
                            format!(
                                "Sektoren besucht: {sectors} von {} · Kartenmarkierungen: {}",
                                s.data.world.regions.len(),
                                j.marks.len()
                            ),
                            13.0,
                            MUTED,
                        ));
                    });
                    cols.spawn(Node {
                        width: Val::Px(520.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn(text(
                            format!("ERLEBNISSE  {}", j.entries.len()),
                            15.0,
                            ACCENT,
                        ));
                        if j.entries.is_empty() {
                            c.spawn(text(
                                "Noch leer. Das erste Andocken, neue Sektoren, Rekorde und Plaketten tragen sich von selbst ein – eigene Notizen mit Enter.",
                                13.0,
                                MUTED,
                            ));
                        }
                        // Neueste zuerst, Fenster von 12 Einträgen.
                        let list: Vec<_> = j.entries.iter().rev().collect();
                        let first = sel.saturating_sub(5).min(list.len().saturating_sub(12));
                        for (i, e) in list.iter().enumerate().skip(first).take(12) {
                            let chosen = i == sel;
                            let col = if e.kind == crate::sim::journal::JournalKind::Note {
                                TEAL
                            } else if chosen {
                                TEXT
                            } else {
                                MUTED
                            };
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
                                    format!("{} {} · {}", e.kind.icon(), fmt_time(e.time), e.text),
                                    13.0,
                                    col,
                                ));
                            });
                        }
                    });
                });
                p.spawn(text(
                    "←→ Seite wechseln · ↑↓ blättern · Enter eigene Notiz · Esc schließen",
                    12.0,
                    MUTED,
                ));
            });
        });
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
                    "←→ Tagebuch · ↑↓ Fund wählen · Enter Wrack-Regel umschalten · Esc schließen",
                    12.0,
                    MUTED,
                ));
            });
        });
}
