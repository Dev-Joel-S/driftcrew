//! Einstellungsmenü (aus Titel und Pause): ↑↓ wählen, ←→ ändern, Enter umschalten/testen,
//! Esc zurück. Änderungen gelten sofort und werden gespeichert.

use bevy::prelude::*;

use super::{ACCENT, BG, BG_FOCUS, BORDER, MUTED, TEAL, TEXT, panel_node, sig_of, text};
use crate::audio::SoundTest;
use crate::input::MenuInput;
use crate::settings::{Settings, store};

pub struct SettingsUiPlugin;

impl Plugin for SettingsUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SettingsMenu>().add_systems(
            Update,
            settings_input
                .before(super::pause::pause_input)
                .before(super::title::title_input),
        );
    }
}

#[derive(Resource, Default)]
pub struct SettingsMenu {
    pub open: bool,
    /// Gerade geöffnet: die Taste, die es geöffnet hat, zählt hier nicht.
    pub opened: bool,
    sel: usize,
    sig: u64,
}

impl SettingsMenu {
    pub fn show(&mut self) {
        self.open = true;
        self.opened = true;
    }
}

#[derive(Component)]
struct SettingsRoot;

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Assist,
    Master,
    Sfx,
    Music,
    Fullscreen,
    Callout,
    Shake,
    Test,
    Back,
}

const ROWS: [Row; 9] = [
    Row::Assist,
    Row::Master,
    Row::Sfx,
    Row::Music,
    Row::Fullscreen,
    Row::Callout,
    Row::Shake,
    Row::Test,
    Row::Back,
];

fn onoff(b: bool) -> &'static str {
    if b { "an" } else { "aus" }
}

fn bar(v: f32) -> String {
    let n = (v * 10.0).round() as usize;
    format!(
        "{}{} {:>3.0} %",
        "■".repeat(n),
        "□".repeat(10 - n.min(10)),
        v * 100.0
    )
}

fn label(r: Row, s: &Settings) -> (String, String, &'static str) {
    match r {
        Row::Assist => (
            "Flugassistenz & Stabilisator".into(),
            onoff(s.flight_assist).into(),
            "an: sanfter Schub, Schiff rollt aus, Drehungen werden abgefangen · aus: reibungsfreie Physik",
        ),
        Row::Master => ("Gesamtlautstärke".into(), bar(s.master), "←→ ändern"),
        Row::Sfx => (
            "Effekte".into(),
            bar(s.sfx),
            "Triebwerke, Treffer, Meldungen",
        ),
        Row::Music => (
            "Musik".into(),
            bar(s.music),
            "Titel, Lobby und Klangfläche im Flug",
        ),
        Row::Fullscreen => (
            "Vollbild".into(),
            onoff(s.fullscreen).into(),
            "auch mit F11",
        ),
        Row::Callout => (
            "Ton bei Zurufen".into(),
            onoff(s.callout_sound).into(),
            "F5–F9 bzw. Steuerkreuz im Flug",
        ),
        Row::Shake => (
            "Bildschirm wackeln".into(),
            onoff(s.shake).into(),
            "bei Stößen und Explosionen",
        ),
        Row::Test => (
            "Ton testen".into(),
            "Enter".into(),
            "spielt einen kurzen Klang – nichts zu hören? Lautstärke und Ausgabegerät prüfen",
        ),
        Row::Back => ("Zurück".into(), String::new(), ""),
    }
}

fn settings_input(
    mut commands: Commands,
    mut input: ResMut<MenuInput>,
    mut menu: ResMut<SettingsMenu>,
    mut settings: ResMut<Settings>,
    mut test: ResMut<SoundTest>,
    roots: Query<Entity, With<SettingsRoot>>,
) {
    if !menu.open {
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    }
    let mut s = settings.clone();
    let mut close = false;
    if menu.opened {
        menu.opened = false;
        menu.sel = 0;
        menu.sig = 0;
    } else {
        if input.escape || input.back {
            close = true;
        }
        if input.up && menu.sel > 0 {
            menu.sel -= 1;
        }
        if input.down && menu.sel + 1 < ROWS.len() {
            menu.sel += 1;
        }
        let step = if input.right {
            0.1
        } else if input.left {
            -0.1
        } else {
            0.0
        };
        let activate = input.enter || input.confirm;
        let row = ROWS[menu.sel];
        let slide = |v: &mut f32| *v = ((*v + step) * 10.0).round().clamp(0.0, 10.0) / 10.0;
        match row {
            Row::Master => slide(&mut s.master),
            Row::Sfx => slide(&mut s.sfx),
            Row::Music => slide(&mut s.music),
            Row::Assist if activate || step != 0.0 => s.flight_assist = !s.flight_assist,
            Row::Fullscreen if activate || step != 0.0 => s.fullscreen = !s.fullscreen,
            Row::Callout if activate || step != 0.0 => s.callout_sound = !s.callout_sound,
            Row::Shake if activate || step != 0.0 => s.shake = !s.shake,
            Row::Test if activate => test.0 = true,
            Row::Back if activate => close = true,
            _ => {}
        }
        // Beim Verstellen der Effekt-Lautstärke gleich hören, wie laut es ist.
        if step != 0.0 && matches!(row, Row::Master | Row::Sfx) {
            test.0 = true;
        }
    }
    // Die Menüeingabe gehört diesem Fenster.
    *input = MenuInput::default();
    if s != *settings {
        store(&s);
        *settings = s;
    }
    if close {
        menu.open = false;
        for e in &roots {
            commands.entity(e).despawn();
        }
        return;
    }
    let key = format!("{}|{:?}", menu.sel, *settings);
    let h = sig_of(&key);
    if h == menu.sig && !roots.is_empty() {
        return;
    }
    menu.sig = h;
    for e in &roots {
        commands.entity(e).despawn();
    }
    spawn_menu(&mut commands, &settings, menu.sel);
}

fn spawn_menu(commands: &mut Commands, s: &Settings, sel: usize) {
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
            SettingsRoot,
            GlobalZIndex(90),
        ))
        .with_children(|r| {
            r.spawn((
                panel_node(Val::Px(560.0)),
                BackgroundColor(BG.with_alpha(1.0)),
                BorderColor::all(BORDER),
            ))
            .with_children(|p| {
                p.spawn(text("EINSTELLUNGEN", 26.0, TEXT));
                for (i, row) in ROWS.iter().enumerate() {
                    let (name, value, hint) = label(*row, s);
                    let chosen = i == sel;
                    p.spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(if chosen { BG_FOCUS } else { Color::NONE }),
                    ))
                    .with_children(|it| {
                        it.spawn(Node {
                            justify_content: JustifyContent::SpaceBetween,
                            ..default()
                        })
                        .with_children(|line| {
                            line.spawn(text(name, 16.0, if chosen { TEXT } else { MUTED }));
                            line.spawn(text(value, 15.0, if chosen { ACCENT } else { TEAL }));
                        });
                        if chosen && !hint.is_empty() {
                            it.spawn(text(hint, 12.0, MUTED));
                        }
                    });
                }
                p.spawn(text(
                    "↑↓ wählen · ←→ ändern · Enter umschalten · Esc zurück · wird gespeichert",
                    12.0,
                    MUTED,
                ));
            });
        });
}
