//! Monumente der Vorgänger: Blöcke aus fremdem Stein mit Glyphen, die mit Artefakten an Bord
//! anschwellen (Resonanz) und hell leuchten, sobald das Monument erwacht ist. Tor-Blöcke
//! verschwinden beim Erwachen.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::{Art, srgb};
use crate::game::Sim;
use crate::sim::world::CellKind;

/// Tor-Block eines Monuments.
#[derive(Component)]
pub struct MonumentDoor(pub usize);

/// Leuchtende Teile eines Monuments: gemeinsames Material und Licht.
#[derive(Component)]
pub struct MonumentLight(pub usize);

#[derive(Resource, Default)]
pub struct MonumentMats(pub Vec<Handle<StandardMaterial>>);

pub fn spawn_monuments(
    mut commands: Commands,
    sim: Res<Sim>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let mut glyph_mats = Vec::new();
    for (mi, m) in sim.0.world.monuments.iter().enumerate() {
        let stone = mats.add(art.panel_mat(srgb(m.main_color), 0.8, 0.15));
        let stone_dark = mats.add(art.panel_mat(srgb(m.main_color).darker(0.15), 0.85, 0.1));
        // Glyphen: Akzentfarbe, deren Leuchten das Update-System steuert.
        let glyph = mats.add(StandardMaterial {
            base_color: srgb(m.accent_color).darker(0.4),
            emissive: LinearRgba::BLACK,
            perceptual_roughness: 0.3,
            ..default()
        });
        glyph_mats.push(glyph.clone());
        let door = mats.add(StandardMaterial {
            base_color: srgb(m.main_color).lighter(0.05),
            emissive: srgb(m.accent_color).to_linear() * 0.6,
            perceptual_roughness: 0.4,
            metallic: 0.6,
            ..default()
        });
        let root = commands
            .spawn((
                Transform::default(),
                Visibility::default(),
                Name::new(m.name.clone()),
            ))
            .id();
        for (k, cell) in m.cells.iter().enumerate() {
            let pos = cell.center.extend(0.0);
            let e = match cell.kind {
                CellKind::Block => Some(
                    commands
                        .spawn((
                            Mesh3d(art.block.clone()),
                            MeshMaterial3d(if k % 3 == 0 {
                                stone_dark.clone()
                            } else {
                                stone.clone()
                            }),
                            Transform::from_translation(pos),
                        ))
                        .id(),
                ),
                CellKind::Accent | CellKind::Window => Some(
                    commands
                        .spawn((
                            Mesh3d(art.block.clone()),
                            MeshMaterial3d(glyph.clone()),
                            Transform::from_translation(pos),
                        ))
                        .id(),
                ),
                CellKind::Door => Some(
                    commands
                        .spawn((
                            Mesh3d(art.block.clone()),
                            MeshMaterial3d(door.clone()),
                            Transform::from_translation(pos).with_scale(Vec3::new(1.0, 1.0, 0.8)),
                            MonumentDoor(mi),
                        ))
                        .id(),
                ),
                CellKind::Slope(ch) => {
                    let outline = crate::sim::world::slope_outline(ch, m.cell * 0.5);
                    let mesh = art.prism(&mut meshes, &outline, m.cell, 0.3, Some((0.3, 0.22)));
                    Some(
                        commands
                            .spawn((
                                Mesh3d(mesh),
                                MeshMaterial3d(stone.clone()),
                                Transform::from_translation(pos),
                            ))
                            .id(),
                    )
                }
                CellKind::Light => {
                    let halo = art.glow_mat(&mut mats, srgb(m.accent_color), 2.0);
                    let e = commands
                        .spawn((
                            Transform::from_translation(pos),
                            Visibility::default(),
                            MonumentLight(mi),
                        ))
                        .with_children(|c| {
                            c.spawn((
                                Mesh3d(art.sphere.clone()),
                                MeshMaterial3d(glyph.clone()),
                                Transform::from_scale(Vec3::splat(1.2)),
                            ));
                            c.spawn((
                                Mesh3d(art.quad.clone()),
                                MeshMaterial3d(halo),
                                Transform::from_xyz(0.0, 0.0, 1.5).with_scale(Vec3::splat(9.0)),
                                NotShadowCaster,
                            ));
                            c.spawn((
                                PointLight {
                                    color: srgb(m.accent_color),
                                    intensity: 0.0,
                                    range: 70.0,
                                    shadow_maps_enabled: false,
                                    ..default()
                                },
                                Transform::from_xyz(0.0, 0.0, 6.0),
                                MonumentLight(mi),
                            ));
                        })
                        .id();
                    Some(e)
                }
            };
            if let Some(e) = e {
                commands.entity(root).add_child(e);
            }
        }
    }
    commands.insert_resource(MonumentMats(glyph_mats));
}

/// Glyphen und Lichter folgen der Resonanz; Tor-Blöcke verschwinden beim Erwachen.
pub fn update_monuments(
    time: Res<Time>,
    sim: Res<Sim>,
    glyphs: Option<Res<MonumentMats>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut doors: Query<(&MonumentDoor, &mut Visibility)>,
    mut lights: Query<(&MonumentLight, &mut PointLight)>,
) {
    let s = &sim.0;
    let t = time.elapsed_secs();
    for (d, mut vis) in &mut doors {
        let want = if s.world.monuments.get(d.0).is_some_and(|m| m.awake) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != want {
            *vis = want;
        }
    }
    let level = |mi: usize| {
        let r = s.monument_resonance(mi);
        let awake = s.world.monuments[mi].awake;
        if awake {
            0.85 + 0.15 * (t * 1.3).sin()
        } else {
            // Dösen: ein schwaches Glimmen im Takt des Signals (41 s), mit Resonanz stärker.
            0.04 + 0.03 * (t * std::f32::consts::TAU / 41.0).sin().max(0.0)
                + r * (0.45 + 0.2 * (t * 4.0).sin())
        }
    };
    if let Some(g) = glyphs {
        for (mi, h) in g.0.iter().enumerate() {
            if mi >= s.world.monuments.len() {
                continue;
            }
            let l = level(mi);
            if let Some(mut m) = mats.get_mut(h) {
                let c = srgb(s.world.monuments[mi].accent_color).to_linear();
                m.emissive = c * (0.2 + l * 1.8);
            }
        }
    }
    for (ml, mut pl) in &mut lights {
        if ml.0 < s.world.monuments.len() {
            pl.intensity = level(ml.0) * 4_000_000.0;
        }
    }
}
