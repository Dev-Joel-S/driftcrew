//! DriftCrew – ein Koop-Weltraumspiel: jede Taste ist ein Triebwerk, der Rest ist Physik.

// Bevy-Systeme haben naturgemäß viele Parameter und lange Query-Typen.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
// Unter Windows im Release kein Konsolenfenster neben dem Spiel (Abstürze landen in crash.txt).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod demo;
mod game;
mod input;
mod pads;
mod render;
mod settings;
mod sim;
mod ui;

use bevy::prelude::*;
use bevy::window::PresentMode;
use demo::Resolution;

/// Abstürze zusätzlich in eine Datei neben dem Spielstand schreiben – ohne Konsole (Windows)
/// sieht man sie sonst nicht, und so lassen sie sich beim Testen weitergeben.
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = game::save_path().with_file_name("crash.txt");
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let bt = std::backtrace::Backtrace::force_capture();
        let _ = std::fs::write(
            &path,
            format!(
                "DriftCrew {} ist abgestürzt:\n{info}\n\n{bt}\n",
                env!("CARGO_PKG_VERSION")
            ),
        );
        default(info);
    }));
}

fn main() {
    install_crash_log();
    let demo = demo::DemoConfig::from_env();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "DriftCrew".into(),
                    resolution: demo.resolution().into(),
                    present_mode: PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            })
            .set(bevy::log::LogPlugin {
                filter: "wgpu=error,naga=warn,bevy_render=warn,bevy_app=warn,gilrs=warn".into(),
                ..default()
            })
            // Gamepads binden wir selbst an (einzelne Joy-Cons, siehe pads.rs).
            .disable::<bevy::gilrs::GilrsPlugin>(),
    )
    .add_plugins((
        settings::SettingsPlugin,
        pads::PadsPlugin,
        game::GamePlugin,
        input::InputPlugin,
        render::RenderPlugin,
        ui::UiPlugin,
        audio::SoundPlugin,
    ));
    if let Some(cfg) = demo {
        app.add_plugins(demo::DemoPlugin(cfg));
    }
    app.run();
}
