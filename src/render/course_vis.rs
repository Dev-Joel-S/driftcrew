//! Parcours sichtbar machen: Torpfosten mit Leuchtkappe, Bojen, Lastaufnahmen an Stationen.
//! Linien, Pfeile und Fortschrittsringe zeichnet das Overlay. Reine Anzeige.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::{Art, srgb};
use crate::game::Sim;
use crate::sim::data::{StepKind, v};
use crate::sim::world::{SOCKET_HALF_DEPTH, SOCKET_HALF_WIDTH, SOCKET_WALL};

/// Leuchtkappe eines Torpfostens oder einer Boje.
#[derive(Component)]
pub struct GateCap {
    course: usize,
    step: usize,
}

/// Materialien für die Zustände eines Tors.
#[derive(Resource, Clone)]
pub struct GateMats {
    /// Starttor, kein Lauf: ruhiges Grün.
    start: Handle<StandardMaterial>,
    /// Nächstes Ziel: hell.
    next: Handle<StandardMaterial>,
    /// Übernächstes: gedämpft.
    later: Handle<StandardMaterial>,
    /// Alles andere: fast aus.
    off: Handle<StandardMaterial>,
}

pub fn spawn_course_vis(
    mut commands: Commands,
    sim: Res<Sim>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let s = &sim.0;
    let gm = GateMats {
        start: mats.add(art.emissive_mat(Color::srgb(0.35, 1.0, 0.5), 3.0)),
        next: mats.add(art.emissive_mat(Color::srgb(1.0, 0.78, 0.2), 9.0)),
        later: mats.add(art.emissive_mat(Color::srgb(1.0, 0.85, 0.5), 1.2)),
        off: mats.add(art.emissive_mat(Color::srgb(0.25, 0.28, 0.32), 0.15)),
    };
    commands.insert_resource(gm.clone());
    let post_mat = mats.add(art.panel_mat(Color::srgb(0.24, 0.26, 0.3), 0.5, 0.3));
    let stripe_mat = mats.add(art.panel_mat(Color::srgb(0.85, 0.62, 0.15), 0.45, 0.2));
    let post = art.bevel_box(&mut meshes, Vec3::new(1.0, 1.0, 2.2));
    let band = art.bevel_box(&mut meshes, Vec3::new(1.12, 1.12, 0.35));
    let post_at = |commands: &mut Commands, p: Vec2, course: usize, step: usize, cap: f32| {
        commands
            .spawn((
                Mesh3d(post.clone()),
                MeshMaterial3d(post_mat.clone()),
                Transform::from_xyz(p.x, p.y, 0.0),
            ))
            .with_children(|c| {
                c.spawn((
                    Mesh3d(band.clone()),
                    MeshMaterial3d(stripe_mat.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.45),
                ));
                c.spawn((
                    Mesh3d(art.sphere.clone()),
                    MeshMaterial3d(gm.off.clone()),
                    Transform::from_xyz(0.0, 0.0, 1.25).with_scale(Vec3::splat(cap)),
                    GateCap { course, step },
                    NotShadowCaster,
                ));
            });
    };
    for (ci, c) in s.data.courses.courses.iter().enumerate() {
        for (si, st) in c.steps.iter().enumerate() {
            match st.kind {
                StepKind::Gate { pos, dir, width } => {
                    let d = Vec2::new(dir.to_radians().cos(), dir.to_radians().sin());
                    let side = Vec2::new(-d.y, d.x) * (width * 0.5 + 0.6);
                    post_at(&mut commands, v(pos) + side, ci, si, 0.75);
                    post_at(&mut commands, v(pos) - side, ci, si, 0.75);
                }
                StepKind::Face { pos, .. } => {
                    // Boje: ein Pfosten mit großer Kappe. Steht sie mitten in einem Tor,
                    // bekommt sie trotzdem ihren eigenen Pfosten (Tore haben zwei außen).
                    post_at(&mut commands, v(pos), ci, si, 1.3);
                }
                _ => {}
            }
        }
    }

    // Lastaufnahmen: U-förmige Halterung in der Akzentfarbe der Station, zwei Lichter an
    // der Öffnung.
    for st in &s.world.stations {
        let Some(sock) = st.socket else { continue };
        let wall_mat = mats.add(art.panel_mat(srgb(st.accent_color), 0.45, 0.2));
        let frame_mat = mats.add(art.panel_mat(srgb(st.main_color), 0.5, 0.15));
        let lamp = mats.add(art.emissive_mat(Color::srgb(1.0, 0.7, 0.2), 6.0));
        let angle = f32::atan2(sock.open.y, sock.open.x);
        let rotz = Quat::from_rotation_z(angle);
        let back = sock.center - sock.open * (SOCKET_HALF_DEPTH + SOCKET_WALL * 0.5);
        let side = Vec2::new(-sock.open.y, sock.open.x);
        let back_mesh = art.bevel_box(
            &mut meshes,
            Vec3::new(SOCKET_WALL, 2.0 * (SOCKET_HALF_WIDTH + SOCKET_WALL), 1.6),
        );
        commands.spawn((
            Mesh3d(back_mesh),
            MeshMaterial3d(frame_mat.clone()),
            Transform::from_xyz(back.x, back.y, 0.0).with_rotation(rotz),
        ));
        let side_mesh = art.bevel_box(
            &mut meshes,
            Vec3::new(2.0 * (SOCKET_HALF_DEPTH + SOCKET_WALL), SOCKET_WALL, 1.6),
        );
        for k in [-1.0f32, 1.0] {
            let c = sock.center + side * k * (SOCKET_HALF_WIDTH + SOCKET_WALL * 0.5)
                - sock.open * (SOCKET_WALL * 0.5);
            commands.spawn((
                Mesh3d(side_mesh.clone()),
                MeshMaterial3d(wall_mat.clone()),
                Transform::from_xyz(c.x, c.y, 0.0).with_rotation(rotz),
            ));
            let tip = sock.center
                + sock.open * (SOCKET_HALF_DEPTH + SOCKET_WALL * 0.4)
                + side * k * (SOCKET_HALF_WIDTH + SOCKET_WALL * 0.5);
            commands.spawn((
                Mesh3d(art.sphere.clone()),
                MeshMaterial3d(lamp.clone()),
                Transform::from_xyz(tip.x, tip.y, 0.95).with_scale(Vec3::splat(0.38)),
                NotShadowCaster,
            ));
        }
    }
}

/// Kappen je nach Zustand umfärben: Starttore grün, nächstes Ziel hell, Rest gedämpft.
pub fn update_gate_caps(
    sim: Res<Sim>,
    mats: Option<Res<GateMats>>,
    mut caps: Query<(&GateCap, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let Some(m) = mats else { return };
    let s = &sim.0;
    let run = s.course.as_ref();
    for (cap, mut mat) in &mut caps {
        let want = match run {
            Some(r) if r.course == cap.course => {
                if cap.step == r.step {
                    &m.next
                } else if cap.step == r.step + 1 {
                    &m.later
                } else {
                    &m.off
                }
            }
            Some(_) => &m.off,
            None if cap.step == 0 => &m.start,
            None => &m.off,
        };
        if mat.0 != *want {
            mat.0 = want.clone();
        }
    }
}
