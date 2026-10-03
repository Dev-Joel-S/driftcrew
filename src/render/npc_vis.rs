//! NPC-Schiffe (Frachter, Drohnen, Schürfroboter, Händlerin, Rivalen, Konvois) sowie Zollbojen
//! und Piratennester. Die Modelle entstehen aus denselben Schiffsdaten wie das Crew-Schiff.

use std::collections::HashMap;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::ship_vis::{FlameVis, ShipModelOpts, spawn_ship_model};
use super::{Art, srgb};
use crate::game::{Data, Sim};
use crate::sim::data::PartKind;
use crate::sim::geom::rot;
use crate::sim::npc::{Npc, Role};
use crate::sim::world::angle_diff;

#[derive(Component)]
pub struct NpcVis(pub u32);

#[derive(Resource, Default)]
pub struct NpcMap(pub HashMap<u32, Entity>);

/// Flammenfarbe nach Rolle: Piraten rot, Roboter gelblich, sonst warmweiß.
fn flame_color(n: &Npc) -> [f32; 3] {
    match n.role {
        Role::Drone { .. } => [1.0, 0.25, 0.15],
        Role::Miner { .. } => [1.0, 0.75, 0.25],
        Role::Rival { .. } => [1.0, 0.45, 0.2],
        _ => [0.75, 0.85, 1.0],
    }
}

/// Lage eines NPC-Schiffs zwischen zwei Simulationsschritten (Ursprung, Winkel).
pub fn npc_pose(n: &Npc, alpha: f32) -> (Vec2, f32) {
    let s = &n.ship;
    let angle = s.prev_angle + angle_diff(s.angle, s.prev_angle) * alpha;
    let com = s.prev_pos.lerp(s.pos, alpha);
    (com - rot(s.com, angle), angle)
}

#[allow(clippy::too_many_arguments)]
pub fn sync_npcs(
    mut commands: Commands,
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    data: Res<Data>,
    mut map: ResMut<NpcMap>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut roots: Query<(&NpcVis, &mut Transform)>,
    mut flames: Query<(&FlameVis, &mut Transform), Without<NpcVis>>,
) {
    let s = &sim.0;
    // Verschwundene und zerstörte Schiffe entfernen.
    map.0.retain(|id, e| {
        let alive = s.npcs.iter().any(|n| n.id == *id && n.alive);
        if !alive {
            commands.entity(*e).despawn();
        }
        alive
    });
    for n in s.npcs.iter().filter(|n| n.alive) {
        if map.0.contains_key(&n.id) {
            continue;
        }
        let def = data.0.ship(&n.def).clone();
        let e = spawn_ship_model(
            &mut commands,
            &mut art,
            &mut meshes,
            &mut mats,
            &n.ship,
            &def,
            ShipModelOpts {
                hull: Some(n.colors.0),
                accent: Some(n.colors.1),
                npc: Some((n.id, flame_color(n))),
                ..default()
            },
        );
        commands.entity(e).insert(NpcVis(n.id));
        map.0.insert(n.id, e);
    }
    let alpha = fixed.overstep_fraction();
    let by_id: HashMap<u32, &Npc> = s.npcs.iter().map(|n| (n.id, n)).collect();
    for (v, mut t) in &mut roots {
        let Some(n) = by_id.get(&v.0) else { continue };
        let (origin, angle) = npc_pose(n, alpha);
        t.translation = origin.extend(0.0);
        t.rotation = Quat::from_rotation_z(angle);
    }
    let tsec = time.elapsed_secs();
    for (f, mut t) in &mut flames {
        let Some(n) = f.owner.and_then(|id| by_id.get(&id)) else {
            continue;
        };
        let Some(th) = n.ship.thrusters.get(f.thruster) else {
            continue;
        };
        let flicker = 0.85
            + 0.15
                * ((tsec * 37.0 + f.thruster as f32 * 1.7 + n.id as f32).sin()
                    * (tsec * 23.0).cos());
        let lvl = th.level;
        if f.len < 0.0 {
            t.scale = Vec3::splat((f.radius * lvl * flicker).max(0.001));
            continue;
        }
        let len = (f.len * lvl * flicker).max(0.001);
        t.scale = Vec3::new(
            f.radius * (0.6 + 0.4 * lvl),
            len,
            f.radius * (0.6 + 0.4 * lvl),
        );
        let half = n
            .ship
            .parts
            .iter()
            .find(|p| matches!(p.kind, PartKind::Thruster(_)) && (p.pos - th.pos).length() < 1e-3)
            .map(|p| p.half.y)
            .unwrap_or(0.2);
        let tip_y = th.pos.y - half - 0.18 - 0.25;
        t.translation.y = tip_y - t.scale.y * 0.5;
    }
}

/// Zollbojen und Piratennester als feste Bauten in der Welt.
pub fn spawn_traffic_props(
    mut commands: Commands,
    data: Res<Data>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let t = &data.0.traffic;
    let body = mats.add(art.panel_mat(srgb([0.82, 0.68, 0.18]), 0.45, 0.4));
    let dark = mats.add(art.panel_mat(srgb([0.12, 0.12, 0.14]), 0.5, 0.6));
    let lamp = mats.add(art.emissive_mat(srgb([1.0, 0.7, 0.15]), 14.0));
    let halo = art.glow_mat(&mut mats, srgb([1.0, 0.7, 0.2]), 2.5);
    for cp in &t.checkpoints {
        let p = Vec2::new(cp.pos.0, cp.pos.1);
        commands
            .spawn((
                Transform::from_translation(p.extend(0.0)),
                Visibility::default(),
                Name::new(cp.name.clone()),
            ))
            .with_children(|c| {
                // Schwimmkörper, Mast und Leuchte.
                c.spawn((
                    Mesh3d(art.cylinder.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::new(1.4, 1.6, 1.4)),
                ));
                for k in [-1.0f32, 1.0] {
                    c.spawn((
                        Mesh3d(art.bevel_box(&mut meshes, Vec3::new(0.25, 2.4, 0.25))),
                        MeshMaterial3d(dark.clone()),
                        Transform::from_xyz(k * 0.9, 1.6, 0.0)
                            .with_rotation(Quat::from_rotation_z(-k * 0.35)),
                    ));
                }
                c.spawn((
                    Mesh3d(art.sphere.clone()),
                    MeshMaterial3d(lamp.clone()),
                    Transform::from_xyz(0.0, 2.9, 0.0).with_scale(Vec3::splat(0.55)),
                ));
                c.spawn((
                    Mesh3d(art.quad.clone()),
                    MeshMaterial3d(halo.clone()),
                    Transform::from_xyz(0.0, 2.9, 0.6).with_scale(Vec3::splat(4.0)),
                    NotShadowCaster,
                ));
                c.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.7, 0.25),
                        intensity: 400_000.0,
                        range: 30.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 2.9, 3.0),
                ));
            });
    }
    // Piratennest: verbogene Containerstapel mit roten Lichtern.
    let hull = mats.add(art.panel_mat(srgb([0.22, 0.18, 0.18]), 0.6, 0.5));
    let red = mats.add(art.emissive_mat(srgb([1.0, 0.15, 0.1]), 16.0));
    let red_halo = art.glow_mat(&mut mats, srgb([1.0, 0.2, 0.15]), 2.5);
    for n in &t.nests {
        let c = Vec2::new(n.center.0, n.center.1);
        commands
            .spawn((
                Transform::from_translation(c.extend(-1.0)),
                Visibility::default(),
                Name::new(n.name.clone()),
            ))
            .with_children(|p| {
                for k in 0..5 {
                    let a = k as f32 * 1.3;
                    let off = Vec2::new(a.cos(), a.sin()) * (3.0 + k as f32 * 1.6);
                    let size = Vec3::new(5.0 + k as f32 * 0.6, 2.4, 2.6);
                    p.spawn((
                        Mesh3d(art.bevel_box(&mut meshes, size)),
                        MeshMaterial3d(hull.clone()),
                        Transform::from_translation(off.extend(-(k as f32) * 0.4))
                            .with_rotation(Quat::from_rotation_z(a * 0.7)),
                    ));
                }
                for k in 0..3 {
                    let a = k as f32 * 2.1 + 0.4;
                    let at = Vec2::new(a.cos(), a.sin()) * 6.5;
                    p.spawn((
                        Mesh3d(art.sphere.clone()),
                        MeshMaterial3d(red.clone()),
                        Transform::from_translation(at.extend(1.6)).with_scale(Vec3::splat(0.4)),
                    ));
                    p.spawn((
                        Mesh3d(art.quad.clone()),
                        MeshMaterial3d(red_halo.clone()),
                        Transform::from_translation(at.extend(2.0)).with_scale(Vec3::splat(3.0)),
                        NotShadowCaster,
                    ));
                }
                p.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.2, 0.15),
                        intensity: 900_000.0,
                        range: 40.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 6.0),
                ));
            });
    }
}
