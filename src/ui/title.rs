//! Titelbildschirm: Weiterspielen, Neues Spiel, Steuerung, Beenden.

use std::sync::Arc;

use bevy::prelude::*;

use super::{
    ACCENT, Item, ItemButton, MENU_TITLE, MUTED, MenuFocus, TEAL, TEXT, mouse_pick, navigate,
    panel, spawn_items, text,
};
use crate::game::{AppState, Data, HasSave, LobbyMode, Sim, write_save};
use crate::input::{ActiveBindings, Crew, MenuInput};
use crate::sim::SimState;
use crate::sim::data::CrewSave;
use crate::sim::ship::Loadout;

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HelpOpen>()
            .add_systems(OnEnter(AppState::Title), spawn_title)
            .add_systems(OnExit(AppState::Title), despawn_title)
            .add_systems(Update, title_input.run_if(in_state(AppState::Title)));
    }
}

#[derive(Component)]
struct TitleRoot;

#[derive(Component)]
struct HelpPanel;

#[derive(Resource, Default)]
pub struct HelpOpen(pub bool);

#[derive(Clone, Debug, PartialEq)]
enum TitleAction {
    Continue,
    NewGame,
    Help,
    Quit,
}

fn items(has_save: bool, sim: &SimState) -> Vec<Item<TitleAction>> {
    let mut v = Vec::new();
    if has_save {
        let ship = sim.data.ship(&sim.crew.current_ship).name.clone();
        v.push(
            Item::new("Weiterspielen", TitleAction::Continue).detail(format!(
                "{} Credits · {} · {} Aufträge erledigt",
                super::fmt_num(sim.crew.credits),
                ship,
                sim.crew.missions_done
            )),
        );
    }
    v.push(
        Item::new("Neues Spiel", TitleAction::NewGame).detail(if has_save {
            "Überschreibt den Spielstand der Crew"
        } else {
            "Eine neue Crew, ein Driftkutter, 300 Credits"
        }),
    );
    v.push(Item::new("Steuerung & Spielprinzip", TitleAction::Help));
    v.push(Item::new("Beenden", TitleAction::Quit));
    v
}

pub const HELP_TEXT: &[&str] = &[
    "JEDE TASTE IST EIN TRIEBWERK",
    "Alle Triebwerke sitzen am Heck und schieben nach vorne. Wer links schiebt, dreht",
    "nach rechts – und umgekehrt. Außen = langer Hebel = starke Drehung, Mitte = geradeaus.",
    "Es gibt keine Reibung: alles driftet weiter, bis jemand gegensteuert.",
    "",
    "CREW & SLOTS",
    "In der Lobby drückt jede Person eine beliebige Taste (oder Gamepad-Taste) und",
    "übernimmt damit den markierten Slot: Triebwerk, Kanone, Kran oder Bohrer.",
    "Tastatur+Maus und jedes Gamepad (auch einzelne Joy-Cons) sind eigene Crewmitglieder.",
    "Werkzeuge zielen mit der Maus bzw. dem Stick des Geräts, das sie belegt hat.",
    "",
    "ANDOCKEN",
    "Langsam (< 2,6 m/s), gerade und ohne Drehung auf eine Plattform setzen.",
    "Die Plattform leuchtet grün, wenn alles passt. Triebwerk zünden = abdocken.",
    "",
    "TREIBSTOFF, SCHÄDEN, BERGUNG",
    "Jedes feuernde Triebwerk verbraucht Treibstoff. Leer bleibt eine Notreserve mit 25 % Schub.",
    "Treffer können einzelne Triebwerke beschädigen (stottern, ausfallen) – Reparatur an Stationen.",
    "Hülle 0: Rettungskapsel, Bergung zur Heimatstation, Kosten zahlt die gemeinsame Kasse.",
    "Schwerkraft gibt es nur an Anomalien und am Schwarzen Loch – Planeten ziehen nicht an.",
    "",
    "ÜBERSTEUERN & ZUSATZENERGIE",
    "Slot-Taste zweimal tippen und halten: +60 % Schub, aber das Triebwerk heizt auf – bei voller",
    "Hitze 4 s Notabschaltung. Übersteuern, Schild-Schnellladen und Werkzeug-Boost teilen sich",
    "die Zusatzenergie. Leer fällt nur der Bonus weg, alles andere geht weiter.",
    "",
    "ZURUFE (keine Pflichtrollen)",
    "F5 Bremsen · F6 Schub aus · F7 Links drehen · F8 Rechts drehen · F9 Werkzeug bereit",
    "Gamepad-Steuerkreuz: ↓ Bremsen (2× = Schub aus) · ← / → drehen · ↑ Werkzeug bereit",
    "",
    "KASSE & ABSTIMMUNG",
    "Credits gehören der Crew. Käufe werden abgestimmt: eigene Slot-Taste = Ja/Nein.",
    "Wer nicht reagiert, enthält sich. Gleichstand bedeutet Nein. Solo kauft direkt.",
    "",
    "TASTEN (fest)",
    "Esc / Start: Pause   ·   Tab / Select: Karte   ·   Pfeile / Steuerkreuz: Menüs",
    "Enter / Start: Bestätigen   ·   Mausrad: Zoom   ·   ^ / Stick drücken: Ping",
    "Später dazukommen: einfach mitten im Flug eine Taste drücken – nächster freier Slot.",
];

fn spawn_title(
    mut commands: Commands,
    has_save: Res<HasSave>,
    sim: Res<Sim>,
    mut focus: ResMut<MenuFocus>,
) {
    focus.0[MENU_TITLE] = 0;
    let its = items(has_save.0, &sim.0);
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::left(Val::Px(90.0)),
                row_gap: Val::Px(10.0),
                ..default()
            },
            TitleRoot,
        ))
        .with_children(|root| {
            root.spawn((
                text("DRIFTCREW", 92.0, TEXT),
                TextShadow {
                    offset: Vec2::new(0.0, 0.0),
                    color: TEAL.with_alpha(0.0),
                },
            ));
            root.spawn((
                Node {
                    width: Val::Px(520.0),
                    height: Val::Px(4.0),
                    margin: UiRect::bottom(Val::Px(6.0)),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundGradient::from(LinearGradient::to_right(vec![
                    ColorStop::auto(Color::srgb(1.0, 0.24, 0.32)),
                    ColorStop::auto(Color::srgb(1.0, 0.82, 0.12)),
                    ColorStop::auto(Color::srgb(0.18, 0.86, 1.0)),
                    ColorStop::auto(Color::srgb(0.85, 0.36, 1.0)),
                ])),
            ));
            root.spawn(text("Jede Taste ein Triebwerk. Der Rest ist Physik.", 22.0, ACCENT));
            root.spawn(text("Koop für 1 bis viele · offene Welt · Stationen, Werften, Missionen", 16.0, MUTED));
            root.spawn(Node {
                height: Val::Px(28.0),
                ..default()
            });
            root.spawn(panel(Val::Px(460.0))).with_children(|p| spawn_items(p, MENU_TITLE, &its));
            root.spawn(Node {
                height: Val::Px(20.0),
                ..default()
            });
            root.spawn(text(
                "Inspiriert vom Prinzip von „Rakete“ (Mario von Rickenbach) – eigenes Spiel, eigene Grafik.",
                13.0,
                MUTED,
            ));
        });
}

fn spawn_help(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(60.0),
                top: Val::Px(60.0),
                bottom: Val::Px(60.0),
                width: Val::Px(760.0),
                ..default()
            },
            HelpPanel,
            GlobalZIndex(40),
        ))
        .with_children(|p| {
            p.spawn(panel(Val::Percent(100.0))).with_children(|p| {
                for line in HELP_TEXT {
                    let is_head = !line.is_empty() && line.chars().all(|c| !c.is_lowercase());
                    p.spawn(text(
                        *line,
                        if is_head { 17.0 } else { 15.0 },
                        if is_head { ACCENT } else { TEXT },
                    ));
                }
                p.spawn(text("Esc / Enter: schließen", 13.0, MUTED));
            });
        });
}

fn despawn_title(
    mut commands: Commands,
    q: Query<Entity, With<TitleRoot>>,
    help: Query<Entity, With<HelpPanel>>,
    mut open: ResMut<HelpOpen>,
) {
    for e in q.iter().chain(help.iter()) {
        commands.entity(e).despawn();
    }
    open.0 = false;
}

#[allow(clippy::too_many_arguments)]
fn title_input(
    mut commands: Commands,
    input: Res<MenuInput>,
    buttons: Query<(&Interaction, &ItemButton), Changed<Interaction>>,
    mut focus: ResMut<MenuFocus>,
    mut has_save: ResMut<HasSave>,
    mut sim: ResMut<Sim>,
    data: Res<Data>,
    mut next: ResMut<NextState<AppState>>,
    mut mode: ResMut<LobbyMode>,
    mut crew: ResMut<Crew>,
    mut bindings: ResMut<ActiveBindings>,
    mut help_open: ResMut<HelpOpen>,
    help: Query<Entity, With<HelpPanel>>,
    mut exit: MessageWriter<AppExit>,
) {
    if help_open.0 {
        if input.escape || input.confirm || input.start || input.back {
            help_open.0 = false;
            for e in &help {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    let its = items(has_save.0, &sim.0);
    let mut f = focus.0[MENU_TITLE];
    let mut activate = if navigate(&mut f, its.len(), &input) {
        Some(f)
    } else {
        None
    };
    if let Some(i) = mouse_pick(MENU_TITLE, &mut f, &buttons) {
        activate = Some(i);
    }
    focus.0[MENU_TITLE] = f;
    let Some(i) = activate else { return };
    let Some(action) = its.get(i).and_then(|it| it.action.clone()) else {
        return;
    };
    match action {
        TitleAction::Continue => {
            *mode = LobbyMode::Initial;
            next.set(AppState::Lobby);
        }
        TitleAction::NewGame => {
            let save = CrewSave::new_game_seeded(&data.0, crate::game::story_seed());
            write_save(&save);
            has_save.0 = true;
            let def = data.0.ship(&save.current_ship).clone();
            sim.0 = SimState::new(Arc::clone(&data.0), &save, Loadout::full(&def), 1);
            crew.bindings.clear();
            crew.players.clear();
            bindings.0.clear();
            *mode = LobbyMode::Initial;
            next.set(AppState::Lobby);
        }
        TitleAction::Help => {
            help_open.0 = true;
            spawn_help(&mut commands);
        }
        TitleAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}
