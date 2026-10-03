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
                draw_scan,
                draw_drop_zones,
                draw_precision_marks,
                draw_courses,
                draw_survey_and_sockets,
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
            // Der Scanner strahlt rundum, keine Ziellinie.
            ToolKind::Scanner => continue,
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

/// Scanner: Welle des laufenden Impulses und Rauten an allem, was gefunden wurde.
fn draw_scan(sim: Res<Sim>, time: Res<Time>, map: Res<MapOpen>, mut gizmos: Gizmos) {
    if map.0 {
        return;
    }
    let s = &sim.0;
    if let Some(p) = &s.scan {
        let a = 0.7 * (1.0 - p.radius / p.max.max(1.0)) + 0.15;
        gizmos.circle(
            Isometry3d::from_translation(p.origin.extend(Z)),
            p.radius,
            Color::srgba(0.35, 0.95, 0.9, a),
        );
    }
    let t = time.elapsed_secs();
    for b in &s.blips {
        let fade = (b.life / 4.0).min(1.0);
        let c = match &b.kind {
            crate::sim::sector::BlipKind::Ore(o) | crate::sim::sector::BlipKind::Deposit(o) => {
                super::srgb(o.color())
            }
            crate::sim::sector::BlipKind::Wreck => Color::srgb(0.85, 0.8, 0.7),
            _ => Color::srgb(0.35, 0.95, 0.9),
        }
        .with_alpha(0.8 * fade);
        let r = 1.4 + 0.3 * (t * 3.0).sin();
        let pts = [Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y].map(|d| b.pos + d * r);
        for i in 0..4 {
            gizmos.line(pts[i].extend(Z), pts[(i + 1) % 4].extend(Z), c);
        }
    }
}

/// Ablagezonen für laufende Bergungsaufträge: gestrichelter Kreis in der Station.
fn draw_drop_zones(sim: Res<Sim>, time: Res<Time>, map: Res<MapOpen>, mut gizmos: Gizmos) {
    if map.0 {
        return;
    }
    let s = &sim.0;
    let t = time.elapsed_secs();
    for m in &s.active {
        let crate::sim::missions::MissionKind::Bulky { to, .. } = m.kind else {
            continue;
        };
        let (c, r) = s.drop_point(to);
        let col = Color::srgba(1.0, 0.75, 0.2, 0.55 + 0.25 * (t * 2.0).sin());
        let n = 48;
        for k in (0..n).step_by(2) {
            let a0 = std::f32::consts::TAU * k as f32 / n as f32 + t * 0.2;
            let a1 = std::f32::consts::TAU * (k + 1) as f32 / n as f32 + t * 0.2;
            gizmos.line(
                (c + Vec2::from_angle(a0) * r).extend(Z),
                (c + Vec2::from_angle(a1) * r).extend(Z),
                col,
            );
        }
    }
}

/// Markierungen für Präzisionsarbeit (Erzadern gold, Wrackverbindungen türkis) in der Nähe.
fn draw_precision_marks(
    sim: Res<Sim>,
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    map: Res<MapOpen>,
    mut gizmos: Gizmos,
) {
    use crate::sim::precision::{PRECISION_RADIUS, PrecisionKind, precision_target};
    if map.0 {
        return;
    }
    let s = &sim.0;
    let t = time.elapsed_secs();
    let alpha = fixed.overstep_fraction();
    for b in &s.bodies {
        if !b.alive || (b.pos - s.ship.pos).length() > 90.0 {
            continue;
        }
        // Interpoliert, damit die Markierung nicht ruckelt.
        let mut ib = b.clone();
        ib.pos = b.prev_pos.lerp(b.pos, alpha);
        ib.angle = b.prev_angle + crate::sim::world::angle_diff(b.angle, b.prev_angle) * alpha;
        let Some((p, kind)) = precision_target(&ib) else {
            continue;
        };
        let c = match kind {
            PrecisionKind::Vein => Color::srgb(1.0, 0.8, 0.25),
            PrecisionKind::Joint => Color::srgb(0.35, 0.95, 0.9),
        };
        let pulse = 0.6 + 0.4 * (t * 4.0).sin();
        let at = p.extend(Z);
        gizmos.circle(
            Isometry3d::from_translation(at),
            PRECISION_RADIUS,
            c.with_alpha(0.9),
        );
        gizmos.circle(
            Isometry3d::from_translation(at),
            PRECISION_RADIUS + 0.5 * pulse,
            c.with_alpha(0.4 * pulse),
        );
    }
}

/// Kreis aus Strichen (gestrichelt) bzw. Bogen von 0 bis `frac` (durchgezogen).
fn dashed_circle(gizmos: &mut Gizmos, c: Vec2, r: f32, col: Color, spin: f32) {
    let n = 40;
    for k in (0..n).step_by(2) {
        let a0 = std::f32::consts::TAU * k as f32 / n as f32 + spin;
        let a1 = std::f32::consts::TAU * (k + 1) as f32 / n as f32 + spin;
        gizmos.line(
            (c + Vec2::from_angle(a0) * r).extend(Z),
            (c + Vec2::from_angle(a1) * r).extend(Z),
            col,
        );
    }
}

fn arc(gizmos: &mut Gizmos, c: Vec2, r: f32, frac: f32, col: Color) {
    let n = (48.0 * frac).ceil() as usize;
    let start = std::f32::consts::FRAC_PI_2;
    for k in 0..n {
        let a0 = start - std::f32::consts::TAU * frac * k as f32 / n as f32;
        let a1 = start - std::f32::consts::TAU * frac * (k + 1) as f32 / n as f32;
        gizmos.line(
            (c + Vec2::from_angle(a0) * r).extend(Z),
            (c + Vec2::from_angle(a1) * r).extend(Z),
            col,
        );
    }
}

/// Ein Parcours-Schritt: Torlinie mit Pfeilen, Punkt, Boje, Feld oder Zielplattform.
fn draw_step(
    gizmos: &mut Gizmos,
    s: &crate::sim::SimState,
    course: usize,
    step: usize,
    col: Color,
    progress: Option<f32>,
    t: f32,
) {
    use crate::sim::data::{StepKind, v};
    let Some(st) = s.data.courses.courses[course].steps.get(step) else {
        return;
    };
    match &st.kind {
        StepKind::Gate { pos, dir, width } => {
            let c = v(*pos);
            let d = Vec2::new(dir.to_radians().cos(), dir.to_radians().sin());
            let side = Vec2::new(-d.y, d.x);
            let half = width * 0.5;
            // Torlinie gestrichelt, damit sie nicht wie eine Wand wirkt.
            let n = 12;
            for k in (0..n).step_by(2) {
                let a = c + side * (-half + *width * k as f32 / n as f32);
                let b = c + side * (-half + *width * (k + 1) as f32 / n as f32);
                gizmos.line(a.extend(Z), b.extend(Z), col.with_alpha(0.55));
            }
            // Drei Pfeile in Flugrichtung, die langsam durchs Tor wandern.
            let phase = (t * 1.5).fract() * 3.0;
            for k in 0..3 {
                let m = c + d * (k as f32 * 3.0 - 4.5 + phase);
                for sgn in [-1.0f32, 1.0] {
                    gizmos.line(
                        (m + d * 1.2).extend(Z),
                        (m - d * 0.6 + side * sgn * 1.6).extend(Z),
                        col,
                    );
                }
            }
        }
        StepKind::Pass { pos, radius } => {
            let c = v(*pos);
            dashed_circle(gizmos, c, *radius, col, t * 0.4);
            gizmos.line((c - Vec2::X).extend(Z), (c + Vec2::X).extend(Z), col);
            gizmos.line((c - Vec2::Y).extend(Z), (c + Vec2::Y).extend(Z), col);
        }
        StepKind::Face { pos, tolerance, .. } => {
            let c = v(*pos);
            gizmos.circle(Isometry3d::from_translation(c.extend(Z)), 2.2, col);
            // Peillinie von der Schiffsnase: grün, sobald sie auf die Boje zeigt.
            let ship = s.ship.pos;
            let nose = s.ship_nose();
            let to = (c - ship).normalize_or_zero();
            let ok = nose.angle_to(to).abs().to_degrees() <= *tolerance;
            let lc = if ok {
                Color::srgb(0.35, 1.0, 0.55)
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.5)
            };
            gizmos.line(ship.extend(Z), (ship + nose * 18.0).extend(Z), lc);
            let len = (c - ship).length();
            let n = (len / 3.0) as usize;
            for k in (0..n).step_by(2) {
                let a = ship + to * (len * k as f32 / n as f32);
                let b = ship + to * (len * (k + 1) as f32 / n as f32);
                gizmos.line(a.extend(Z), b.extend(Z), col.with_alpha(0.35));
            }
            if let Some(f) = progress {
                arc(gizmos, c, 3.2, f, Color::srgb(0.35, 1.0, 0.55));
            }
        }
        StepKind::Hold { pos, radius, .. } => {
            let c = v(*pos);
            dashed_circle(gizmos, c, *radius, col, t * 0.3);
            if let Some(f) = progress {
                arc(gizmos, c, *radius + 0.8, f, Color::srgb(0.35, 1.0, 0.55));
            }
        }
        StepKind::Dock { station, near } => {
            for pad in s.dock_pads(station, *near) {
                if near.is_none() {
                    continue;
                }
                let p = &s.world.pads[pad];
                let tg = p.tangent() * p.half_width;
                let up = p.normal * 5.0;
                let a = p.center - tg;
                let b = p.center + tg;
                for (x, y) in [(a, b), (a, a + up), (b, b + up), (a + up, b + up)] {
                    gizmos.line(x.extend(Z), y.extend(Z), col);
                }
            }
        }
    }
}

/// Parcours: aktuelles Ziel hell, das nächste gedämpft; ohne Lauf die Starttore in der Nähe.
fn draw_courses(sim: Res<Sim>, time: Res<Time>, map: Res<MapOpen>, mut gizmos: Gizmos) {
    if map.0 {
        return;
    }
    let s = &sim.0;
    let t = time.elapsed_secs();
    let bright = Color::srgba(1.0, 0.8, 0.25, 0.75 + 0.25 * (t * 4.0).sin());
    let start = Color::srgba(0.4, 1.0, 0.55, 0.75);
    match &s.course {
        Some(r) => {
            let col = if r.started() { bright } else { start };
            draw_step(
                &mut gizmos,
                s,
                r.course,
                r.step,
                col,
                s.course_hold_fraction(),
                t,
            );
            if r.started() {
                draw_step(
                    &mut gizmos,
                    s,
                    r.course,
                    r.step + 1,
                    Color::srgba(1.0, 0.85, 0.5, 0.3),
                    None,
                    t,
                );
            }
        }
        None => {
            for ci in 0..s.data.courses.courses.len() {
                if s.step_target(ci, 0)
                    .is_some_and(|p| (p - s.ship.pos).length() < 400.0)
                {
                    draw_step(&mut gizmos, s, ci, 0, start, None, t);
                }
            }
        }
    }
}

/// Messfelder laufender Messflüge und Lastaufnahmen, in die eine Kiste soll.
fn draw_survey_and_sockets(sim: Res<Sim>, time: Res<Time>, map: Res<MapOpen>, mut gizmos: Gizmos) {
    use crate::sim::missions::{MissionKind, SOCKET_SETTLE};
    use crate::sim::world::{SOCKET_HALF_DEPTH, SOCKET_HALF_WIDTH};
    if map.0 {
        return;
    }
    let s = &sim.0;
    let t = time.elapsed_secs();
    let teal = Color::srgba(0.35, 0.95, 0.9, 0.7 + 0.2 * (t * 3.0).sin());
    for m in &s.active {
        match &m.kind {
            MissionKind::Survey { sites, done, hold } => {
                let Some(&i) = sites.get(*done) else { continue };
                let site = &s.data.courses.survey_sites[i];
                let c = crate::sim::data::v(site.pos);
                dashed_circle(&mut gizmos, c, site.radius, teal, -t * 0.3);
                for k in 0..4 {
                    let d = Vec2::from_angle(std::f32::consts::FRAC_PI_2 * k as f32);
                    gizmos.line(
                        (c + d * (site.radius - 1.2)).extend(Z),
                        (c + d * (site.radius + 1.2)).extend(Z),
                        teal,
                    );
                }
                let f = hold / s.data.missions.survey.seconds.max(0.01);
                if f > 0.0 {
                    arc(
                        &mut gizmos,
                        c,
                        site.radius + 0.9,
                        f.min(1.0),
                        Color::srgb(0.35, 1.0, 0.55),
                    );
                }
            }
            MissionKind::Haul { to, settle, .. } => {
                let Some(sock) = s.world.stations[*to].socket else {
                    continue;
                };
                let side = Vec2::new(-sock.open.y, sock.open.x);
                let col = Color::srgba(1.0, 0.75, 0.2, 0.55 + 0.35 * (t * 2.5).sin());
                let a = sock.center - sock.open * SOCKET_HALF_DEPTH;
                let b = sock.center + sock.open * SOCKET_HALF_DEPTH;
                let corners = [
                    a - side * SOCKET_HALF_WIDTH,
                    a + side * SOCKET_HALF_WIDTH,
                    b + side * SOCKET_HALF_WIDTH,
                    b - side * SOCKET_HALF_WIDTH,
                ];
                for k in 0..4 {
                    gizmos.line(corners[k].extend(Z), corners[(k + 1) % 4].extend(Z), col);
                }
                // Einflugpfeile vor der Öffnung, nach innen gerichtet.
                for k in 0..3 {
                    let m = sock.center + sock.open * (SOCKET_HALF_DEPTH + 3.0 + k as f32 * 2.5);
                    for sgn in [-1.0f32, 1.0] {
                        gizmos.line(
                            (m - sock.open * 1.0).extend(Z),
                            (m + sock.open * 0.6 + side * sgn * 1.3).extend(Z),
                            col,
                        );
                    }
                }
                if *settle > 0.0 {
                    arc(
                        &mut gizmos,
                        sock.center,
                        SOCKET_HALF_WIDTH + 1.4,
                        (settle / SOCKET_SETTLE).min(1.0),
                        Color::srgb(0.35, 1.0, 0.55),
                    );
                }
            }
            _ => {}
        }
    }
}
