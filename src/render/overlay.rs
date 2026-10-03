//! Flughilfen als dünne Linien: Geschwindigkeitsvektor, Flugbahn-Vorschau, Zielmarker
//! der Werkzeuge. Reine Anzeige – liest den Simulationszustand nur.

use bevy::prelude::*;

use super::{ship_pose, slot_color};
use crate::game::{AppState, MapOpen, Paused, Sim};
use crate::sim::data::ToolKind;
use crate::sim::geom::rot;

/// Höhe der Linien über der Spielebene (vor den Blockfronten).
const Z: f32 = 2.3;

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, configure_gizmos).add_systems(
            Update,
            (
                draw_flight_aids,
                draw_aim_markers,
                draw_no_return,
                draw_pings,
            )
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.line.width = 1.6;
}

fn draw_flight_aids(
    sim: Res<Sim>,
    fixed: Res<Time<Fixed>>,
    map: Res<MapOpen>,
    paused: Res<Paused>,
    mut gizmos: Gizmos,
) {
    if map.0 || paused.0 {
        return;
    }
    let s = &sim.0;
    if s.ship.docked.is_some() || s.ship.destroyed {
        return;
    }
    let alpha = fixed.overstep_fraction();
    let com = s.ship.prev_pos.lerp(s.ship.pos, alpha);
    let speed = s.ship.vel.length();
    if speed < 0.3 {
        return;
    }
    // Geschwindigkeitsvektor: wohin das Schiff in 0,6 s treibt.
    let tip = com + (s.ship.vel * 0.6).clamp_length_max(18.0);
    gizmos.arrow(
        com.extend(Z),
        tip.extend(Z),
        Color::srgba(0.85, 0.95, 1.0, 0.55),
    );

    // Flugbahn-Vorschau (ohne Eingabe), gestrichelt und zum Ende hin blasser.
    let pred = s.predict_path(4.0, 1.0 / 20.0);
    let n = pred.points.len().max(2);
    for (i, w) in pred.points.windows(2).enumerate() {
        if i % 2 == 1 {
            continue;
        }
        let a = 0.55 * (1.0 - i as f32 / n as f32);
        gizmos.line(
            w[0].extend(Z),
            w[1].extend(Z),
            Color::srgba(0.6, 0.9, 1.0, a),
        );
    }
    if let Some((p, v)) = pred.hit {
        let c = if v > crate::sim::physics::SAFE_IMPACT {
            Color::srgb(1.0, 0.25, 0.2)
        } else {
            Color::srgb(1.0, 0.8, 0.2)
        };
        let d = 0.8;
        gizmos.line(
            (p + Vec2::new(-d, -d)).extend(Z),
            (p + Vec2::new(d, d)).extend(Z),
            c,
        );
        gizmos.line(
            (p + Vec2::new(-d, d)).extend(Z),
            (p + Vec2::new(d, -d)).extend(Z),
            c,
        );
    }
}

fn draw_aim_markers(
    sim: Res<Sim>,
    fixed: Res<Time<Fixed>>,
    map: Res<MapOpen>,
    paused: Res<Paused>,
    mut gizmos: Gizmos,
) {
    if map.0 || paused.0 {
        return;
    }
    let s = &sim.0;
    if s.ship.docked.is_some() || s.ship.destroyed {
        return;
    }
    let alpha = fixed.overstep_fraction();
    let (origin, angle) = ship_pose(s, alpha);
    for t in &s.ship.tools {
        let mount = origin + rot(t.pos, angle);
        let dir = t.aim_dir();
        let reach = match t.kind {
            ToolKind::Cannon => 10.0,
            ToolKind::Crane => s.ship.crane_range * 0.6,
            ToolKind::Drill => 5.5,
        };
        let c = slot_color(t.slot).with_alpha(if t.pressed { 0.9 } else { 0.45 });
        // Gestrichelte Ziellinie in Slotfarbe – nur der Besitzer des Slots bewegt sie.
        let dashes = 6;
        for k in 0..dashes {
            let a = 1.2 + (reach - 1.2) * k as f32 / dashes as f32;
            let b = a + (reach - 1.2) / dashes as f32 * 0.55;
            gizmos.line((mount + dir * a).extend(Z), (mount + dir * b).extend(Z), c);
        }
        let end = mount + dir * reach;
        gizmos.circle(Isometry3d::from_translation(end.extend(Z)), 0.45, c);
    }
}

/// Im Sog eines Schwarzen Lochs: gestrichelter roter Kreis, ab dem auch Vollschub nicht
/// mehr reicht.
fn draw_no_return(sim: Res<Sim>, map: Res<MapOpen>, paused: Res<Paused>, mut gizmos: Gizmos) {
    if map.0 || paused.0 {
        return;
    }
    let s = &sim.0;
    if s.ship.destroyed {
        return;
    }
    let reserve = if s.ship.fuel_empty() {
        crate::sim::tools::EMERGENCY_THRUST
    } else {
        1.0
    };
    let thrust: f32 = s.ship.thrusters.iter().map(|t| t.effective_thrust()).sum();
    let max_accel = thrust * reserve / s.ship.mass;
    for an in s.world.anomalies.iter().filter(|a| a.is_black_hole()) {
        if (s.ship.pos - an.pos).length() > an.radius {
            continue;
        }
        let r = an.no_return_radius(max_accel);
        let n = 72;
        for k in (0..n).step_by(2) {
            let a0 = std::f32::consts::TAU * k as f32 / n as f32;
            let a1 = std::f32::consts::TAU * (k + 1) as f32 / n as f32;
            gizmos.line(
                (an.pos + Vec2::from_angle(a0) * r).extend(Z),
                (an.pos + Vec2::from_angle(a1) * r).extend(Z),
                Color::srgba(1.0, 0.3, 0.25, 0.6),
            );
        }
    }
}

/// Pings: Kreis in Spielerfarbe mit nach außen laufender Welle, verblasst zum Ende hin.
fn draw_pings(sim: Res<Sim>, time: Res<Time>, map: Res<MapOpen>, mut gizmos: Gizmos) {
    if map.0 {
        return;
    }
    let t = time.elapsed_secs();
    for p in &sim.0.pings {
        let c = crate::input::player_color(p.player as usize);
        let fade = (p.life / 1.5).min(1.0);
        let at = p.pos.extend(Z);
        gizmos.circle(
            Isometry3d::from_translation(at),
            1.6,
            c.with_alpha(0.9 * fade),
        );
        let wave = (t * 0.9).fract();
        gizmos.circle(
            Isometry3d::from_translation(at),
            1.6 + wave * 7.0,
            c.with_alpha(0.7 * (1.0 - wave) * fade),
        );
        // Fadenkreuz
        for d in [Vec2::X, Vec2::Y] {
            gizmos.line(
                (p.pos + d * 2.2).extend(Z),
                (p.pos + d * 3.4).extend(Z),
                c.with_alpha(fade),
            );
            gizmos.line(
                (p.pos - d * 2.2).extend(Z),
                (p.pos - d * 3.4).extend(Z),
                c.with_alpha(fade),
            );
        }
    }
}
