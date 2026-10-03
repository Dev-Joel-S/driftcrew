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
            (draw_flight_aids, draw_aim_markers).run_if(in_state(AppState::Playing)),
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
    // Geschwindigkeitsvektor: wohin das Schiff in einer Sekunde treibt.
    let tip = com + s.ship.vel.clamp_length_max(30.0);
    gizmos.arrow(
        com.extend(Z),
        tip.extend(Z),
        Color::srgba(0.85, 0.95, 1.0, 0.75),
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
