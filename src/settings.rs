//! Einstellungen: Lautstärken, Vollbild, Ton bei Zurufen, Bildschirmwackeln. Liegen in
//! `settings.ron` neben dem Spielstand und gelten sofort.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};
use serde::{Deserialize, Serialize};

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Lautstärken 0..1.
    pub master: f32,
    pub sfx: f32,
    pub music: f32,
    pub fullscreen: bool,
    pub callout_sound: bool,
    pub shake: bool,
    /// Flugassistenz mit Stabilisator (aus = reibungsfreie Physik wie früher).
    pub flight_assist: bool,
    /// Wie einzelne Joy-Cons gehalten werden (in der Lobby pro Joy-Con umschaltbar).
    pub joycon_grip: crate::pads::Grip,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            master: 0.8,
            sfx: 0.9,
            music: 0.6,
            fullscreen: false,
            callout_sound: true,
            shake: true,
            flight_assist: true,
            joycon_grip: crate::pads::Grip::Sideways,
        }
    }
}

impl Settings {
    /// Wirksame Lautstärke für Effekte bzw. Musik.
    pub fn sfx_volume(&self) -> f32 {
        self.master * self.sfx
    }
    pub fn music_volume(&self) -> f32 {
        self.master * self.music
    }
}

fn path() -> std::path::PathBuf {
    crate::game::save_path().with_file_name("settings.ron")
}

pub fn load() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| ron::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn store(s: &Settings) {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = ron::ser::to_string_pretty(s, ron::ser::PrettyConfig::default()) {
        let _ = std::fs::write(p, text);
    }
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        // Im Vorführmodus immer die Grundeinstellungen (reproduzierbare Screenshots).
        // Die Vorführszenen fliegen mit dem Autopiloten, der ist auf die reibungsfreie Physik
        // ausgelegt.
        let s = if std::env::var("DRIFTCREW_DEMO").is_ok() {
            Settings {
                flight_assist: false,
                ..Settings::default()
            }
        } else {
            load()
        };
        app.insert_resource(s)
            .add_systems(Update, apply_window.run_if(resource_changed::<Settings>));
    }
}

/// Vollbild umsetzen (auch F11 schaltet, siehe `game.rs`).
fn apply_window(settings: Res<Settings>, mut windows: Query<&mut Window>) {
    for mut w in &mut windows {
        let want = if settings.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        };
        if w.mode != want {
            w.mode = want;
        }
    }
}
