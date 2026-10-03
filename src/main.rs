//! DriftCrew – ein Koop-Weltraumspiel: jede Taste ist ein Triebwerk, der Rest ist Physik.

// Bevy-Systeme haben naturgemäß viele Parameter und lange Query-Typen.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod audio;
mod demo;
mod game;
mod input;
mod render;
mod sim;
mod ui;

use bevy::prelude::*;
use bevy::window::PresentMode;
use demo::Resolution;

fn main() {
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
            }),
    )
    .add_plugins((
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
