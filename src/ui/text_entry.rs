//! Texteingabe (Punkte 81, 82): Schiff taufen, Notiz ins Logbuch, Kartenmarkierung beschriften.
//! Solange das Feld offen ist, ruht die Simulation und Slot-Tasten schreiben nur Text – danach
//! wird der Tastenspeicher geleert, damit nichts nachträglich zündet. Ohne Tastatur (Pad):
//! ←→ blättert durch Vorschläge.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use super::{ACCENT, BG, BORDER, MUTED, TEXT, text};
use crate::game::{AppState, Paused, PendingCommands, Sim};
use crate::input::{InputLatch, MenuInput};
use crate::sim::Command;
use crate::sim::journal::{MAX_NAME, MAX_NOTE};

pub struct TextEntryPlugin;

impl Plugin for TextEntryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TextEntry>()
            .add_systems(OnExit(AppState::Playing), close_entry)
            .add_systems(
                Update,
                entry_input
                    .before(super::pause::pause_input)
                    .before(super::logbook::logbook_input)
                    .before(super::cargo::plan_input)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Purpose {
    #[default]
    ShipName,
    Note,
    Mark(Vec2),
}

impl Purpose {
    fn title(&self) -> &'static str {
        match self {
            Purpose::ShipName => "SCHIFF TAUFEN",
            Purpose::Note => "NOTIZ INS LOGBUCH",
            Purpose::Mark(_) => "KARTE MARKIEREN",
        }
    }
    fn max(&self) -> usize {
        match self {
            Purpose::Note => MAX_NOTE,
            _ => MAX_NAME,
        }
    }
    fn suggestions(&self) -> &'static [&'static str] {
        match self {
            Purpose::ShipName => &[
                "Rostige Rakete",
                "Kleine Möwe",
                "Treibgut",
                "Zweite Chance",
                "Sternschnuppe",
                "Alte Liebe",
                "Schubkraft",
                "Nordlicht",
            ],
            Purpose::Note => &[
                "Hier lohnt sich das Erz.",
                "Gefährlich – nicht nochmal ohne Schild.",
                "Später zurückkommen.",
                "Schönste Aussicht im Sektor.",
                "Hier haben wir uns verflogen.",
            ],
            Purpose::Mark(_) => &[
                "Erz",
                "Gefahr",
                "Wrack",
                "Später",
                "Treffpunkt",
                "Schöne Aussicht",
            ],
        }
    }
}

#[derive(Resource, Default)]
pub struct TextEntry {
    pub open: bool,
    pub purpose: Purpose,
    pub text: String,
    /// Gerade geöffnet: die Taste, die es geöffnet hat, zählt hier nicht.
    pub opened: bool,
    suggestion: usize,
    /// War die Simulation schon pausiert (Pausemenü)? Dann bleibt sie es danach.
    was_paused: bool,
}

impl TextEntry {
    pub fn start(&mut self, purpose: Purpose, text: String, paused: &mut Paused) {
        self.open = true;
        self.opened = true;
        self.purpose = purpose;
        self.text = text;
        self.suggestion = 0;
        self.was_paused = paused.0;
        paused.0 = true;
    }
}

#[derive(Component)]
struct EntryRoot;

#[derive(Component)]
struct EntryText;

fn close_entry(
    mut commands: Commands,
    mut entry: ResMut<TextEntry>,
    q: Query<Entity, With<EntryRoot>>,
) {
    entry.open = false;
    for e in &q {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn entry_input(
    mut commands: Commands,
    mut keys: MessageReader<KeyboardInput>,
    mut menu: ResMut<MenuInput>,
    mut entry: ResMut<TextEntry>,
    mut paused: ResMut<Paused>,
    mut latch: ResMut<InputLatch>,
    mut pending: ResMut<PendingCommands>,
    sim: Res<Sim>,
    roots: Query<Entity, With<EntryRoot>>,
    mut label: Query<&mut Text, With<EntryText>>,
) {
    if !entry.open {
        keys.clear();
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    }
    let max = entry.purpose.max();
    let mut done: Option<bool> = None;
    if entry.opened {
        entry.opened = false;
        keys.clear();
    } else {
        for k in keys.read() {
            if k.state != ButtonState::Pressed {
                continue;
            }
            match &k.logical_key {
                Key::Enter => done = Some(true),
                Key::Escape => done = Some(false),
                Key::Backspace => {
                    entry.text.pop();
                }
                _ => {
                    if let Some(t) = &k.text {
                        for c in t.chars().filter(|c| !c.is_control()) {
                            if entry.text.chars().count() < max {
                                entry.text.push(c);
                            }
                        }
                    }
                }
            }
        }
        // Pad: Vorschläge blättern, bestätigen, abbrechen.
        let sug = entry.purpose.suggestions();
        if menu.right || menu.left {
            let n = sug.len();
            entry.suggestion = if menu.right {
                (entry.suggestion + 1) % n
            } else {
                (entry.suggestion + n - 1) % n
            };
            entry.text = sug[entry.suggestion].to_string();
        }
        if menu.confirm && !menu.enter {
            done = Some(true);
        }
        if menu.back {
            done = Some(false);
        }
    }
    // Die Menüeingabe gehört diesem Feld.
    *menu = MenuInput::default();
    if let Some(ok) = done {
        if ok {
            let text = entry.text.clone();
            pending.0.push(match entry.purpose.clone() {
                Purpose::ShipName => Command::NameShip { name: text },
                Purpose::Note => Command::AddNote { text },
                Purpose::Mark(pos) => Command::AddMark { pos, text },
            });
        }
        entry.open = false;
        paused.0 = entry.was_paused;
        latch.0 = 0;
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    }
    let shown = format!("{}▏", entry.text);
    if roots.is_empty() {
        spawn_entry(&mut commands, &entry, &shown, &sim.0);
    } else if let Ok(mut t) = label.single_mut()
        && t.0 != shown
    {
        t.0 = shown;
    }
}

fn spawn_entry(commands: &mut Commands, entry: &TextEntry, shown: &str, s: &crate::sim::SimState) {
    let hint = match &entry.purpose {
        Purpose::ShipName => format!(
            "Name für {} – steht auf dem Rumpf, im HUD und im Logbuch",
            s.data.ship(&s.crew.current_ship).name
        ),
        Purpose::Note => "Landet mit Flugzeit im Tagebuch des Logbuchs".to_string(),
        Purpose::Mark(_) => "Beschriftung für die Markierung hier auf der Karte".to_string(),
    };
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.6)),
            EntryRoot,
            GlobalZIndex(95),
        ))
        .with_children(|r| {
            r.spawn((
                Node {
                    width: Val::Px(560.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(18.0)),
                    row_gap: Val::Px(8.0),
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(BG.with_alpha(1.0)),
                BorderColor::all(BORDER),
            ))
            .with_children(|p| {
                p.spawn(text(entry.purpose.title(), 22.0, ACCENT));
                p.spawn(text(hint, 13.0, MUTED));
                p.spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
                ))
                .with_children(|f| {
                    f.spawn((text(shown.to_string(), 18.0, TEXT), EntryText));
                });
                p.spawn(text(
                    "Tippen · ⌫ löschen · Enter übernehmen · Esc abbrechen · Pad: ←→ Vorschläge, A übernehmen, B abbrechen",
                    12.0,
                    MUTED,
                ));
            });
        });
}
