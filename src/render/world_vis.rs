//! Statische Welt sichtbar machen: Stationen, Planeten, Rotoren, Anomalien, Hintergrund.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::textures::{self, Rgb};
use super::{Art, ShipModelOpts, meshes, srgb};
use crate::game::{Data, GameCamera, Sim};
use crate::sim::data::{Ore, hex, v};
use crate::sim::rng::Rng;
use crate::sim::ship::{Loadout, Ship, ShipStats};
use crate::sim::world::{CellKind, Shape, Surface};

#[derive(Component)]
pub struct SpinnerVis(pub usize);

#[derive(Component)]
pub struct Beacon {
    pub phase: f32,
    pub speed: f32,
    pub base: f32,
}

#[derive(Component)]
pub struct DepositVis {
    pub planet: usize,
    pub deposit: usize,
    pub base: Vec3,
}

#[derive(Component)]
pub struct PadGlow(pub usize);

#[derive(Component)]
pub struct StarTile {
    pub tile: f32,
    pub offset: IVec2,
}

/// Langsame Drehung um eine lokale Achse.
#[derive(Component)]
pub struct Swirl(pub Vec3, pub f32);

/// Planetenoberfläche scrollt langsam (wirkt wie Rotation, ohne die Abflachung zu stören).
#[derive(Component)]
pub struct PlanetSpin(pub f32);

fn darker(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_world(
    mut commands: Commands,
    sim: Res<Sim>,
    data: Res<Data>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let world = &sim.0.world;
    let pad_lights = PadLightMats {
        off: mats.add(art.emissive_mat(Color::srgb(0.05, 0.05, 0.05), 0.0)),
        idle: mats.add(art.emissive_mat(Color::srgb(0.9, 0.55, 0.1), 0.6)),
        green: mats.add(art.emissive_mat(Color::srgb(0.2, 1.0, 0.45), 5.0)),
        yellow: mats.add(art.emissive_mat(Color::srgb(1.0, 0.75, 0.1), 5.0)),
        red: mats.add(art.emissive_mat(Color::srgb(1.0, 0.15, 0.1), 5.0)),
    };
    commands.insert_resource(pad_lights.clone());

    // --- Stationen -------------------------------------------------------
    for (si, st) in world.stations.iter().enumerate() {
        let main = st.main_color;
        let accent = st.accent_color;
        // Drei leicht unterschiedliche Töne pro Station für mehr Leben.
        let variants: Vec<Handle<StandardMaterial>> = [1.0, 0.92, 1.06]
            .iter()
            .map(|k| mats.add(art.panel_mat(srgb(darker(main, *k)), 0.52, 0.08)))
            .collect();
        let accent_mat = mats.add(art.panel_mat(srgb(accent), 0.45, 0.12));
        let window_mat = mats.add(StandardMaterial {
            base_color: srgb(darker(main, 0.55)),
            base_color_texture: Some(art.window_alb.clone()),
            emissive_texture: Some(art.window_emi.clone()),
            emissive: LinearRgba::rgb(3.2, 3.2, 3.2),
            perceptual_roughness: 0.25,
            metallic: 0.3,
            ..default()
        });
        let block = art.block.clone();
        let root = commands
            .spawn((
                Transform::default(),
                Visibility::default(),
                Name::new(st.name.clone()),
            ))
            .id();
        let mut rng = Rng::new(si as u64 * 977 + 13);
        for cell in &st.cells {
            let pos = Vec3::new(cell.center.x, cell.center.y, 0.0);
            match cell.kind {
                CellKind::Block | CellKind::Accent | CellKind::Window => {
                    let mat = match cell.kind {
                        CellKind::Accent => accent_mat.clone(),
                        CellKind::Window => window_mat.clone(),
                        _ => variants[rng.index(variants.len())].clone(),
                    };
                    let e = commands
                        .spawn((
                            Mesh3d(block.clone()),
                            MeshMaterial3d(mat),
                            Transform::from_translation(pos),
                        ))
                        .id();
                    commands.entity(root).add_child(e);
                }
                CellKind::Slope(ch) => {
                    let outline = crate::sim::world::slope_outline(ch, st.cell * 0.5);
                    let mesh = art.prism(&mut meshes, &outline, st.cell, 0.42, Some((0.3, 0.22)));
                    let mat = variants[rng.index(variants.len())].clone();
                    let e = commands
                        .spawn((
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            Transform::from_translation(pos),
                        ))
                        .id();
                    commands.entity(root).add_child(e);
                }
                CellKind::Light => {
                    spawn_beacon(
                        &mut commands,
                        &mut art,
                        &mut mats,
                        pos,
                        srgb(accent),
                        rng.range(0.0, 6.0),
                        Some(root),
                    );
                }
            }
        }
        // Kleine Aufbauten (Antennen, Lüfter, Schüsseln, Rohre) auf freien Oberseiten.
        {
            let sd = &data.0.world.stations[si];
            let grid: Vec<Vec<char>> = sd.layout.iter().map(|r| r.chars().collect()).collect();
            let at = |c: i32, r: i32| -> char {
                if r < 0 || c < 0 || r as usize >= grid.len() || c as usize >= grid[0].len() {
                    '.'
                } else {
                    grid[r as usize][c as usize]
                }
            };
            let metal = mats.add(art.panel_mat(srgb([0.32, 0.34, 0.4]), 0.35, 0.7));
            let light = mats.add(art.panel_mat(srgb([0.85, 0.87, 0.9]), 0.4, 0.3));
            let tip = mats.add(art.emissive_mat(srgb([1.0, 0.25, 0.2]), 10.0));
            let vent = art.bevel_box(&mut meshes, Vec3::new(1.5, 0.4, 1.8));
            for cell in &st.cells {
                if !matches!(cell.kind, CellKind::Block | CellKind::Accent) {
                    continue;
                }
                for (dir, dr) in [(1.0f32, -1), (-1.0f32, 1)] {
                    if at(cell.col, cell.row + dr) != '.' || !rng.chance(0.22) {
                        continue;
                    }
                    let top = cell.center + Vec2::Y * dir * st.cell * 0.5;
                    let x = rng.range(-1.0, 1.0);
                    let rot =
                        Quat::from_rotation_z(if dir > 0.0 { 0.0 } else { std::f32::consts::PI });
                    let mut parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> =
                        Vec::new();
                    if rng.chance(0.18) {
                        let up = Vec2::Y * dir;
                        spawn_containers(
                            &mut commands,
                            &mut art,
                            &mut meshes,
                            &mut mats,
                            top,
                            up,
                            &mut rng,
                        );
                        continue;
                    }
                    match rng.index(4) {
                        0 => {
                            let h = rng.range(1.0, 1.8);
                            parts.push((
                                art.cylinder.clone(),
                                metal.clone(),
                                Transform::from_xyz(x, h * 0.5, -1.2)
                                    .with_scale(Vec3::new(0.07, h, 0.07)),
                            ));
                            parts.push((
                                art.sphere.clone(),
                                tip.clone(),
                                Transform::from_xyz(x, h, -1.2).with_scale(Vec3::splat(0.16)),
                            ));
                        }
                        1 => parts.push((
                            vent.clone(),
                            metal.clone(),
                            Transform::from_xyz(x * 0.6, 0.2, -1.0),
                        )),
                        2 => {
                            parts.push((
                                art.cylinder.clone(),
                                metal.clone(),
                                Transform::from_xyz(x, 0.3, -1.2)
                                    .with_scale(Vec3::new(0.09, 0.6, 0.09)),
                            ));
                            parts.push((
                                art.sphere.clone(),
                                light.clone(),
                                Transform::from_xyz(x, 0.75, -1.2)
                                    .with_rotation(Quat::from_rotation_z(0.5))
                                    .with_scale(Vec3::new(0.75, 0.22, 0.75)),
                            ));
                        }
                        _ => parts.push((
                            art.cylinder.clone(),
                            light.clone(),
                            Transform::from_xyz(0.0, 0.22, -1.1)
                                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                                .with_scale(Vec3::new(0.2, st.cell * 0.95, 0.2)),
                        )),
                    }
                    let holder = commands
                        .spawn((
                            Transform::from_translation(top.extend(0.0)).with_rotation(rot),
                            Visibility::default(),
                        ))
                        .id();
                    for (m, mat, t) in parts {
                        let e = commands.spawn((Mesh3d(m), MeshMaterial3d(mat), t)).id();
                        commands.entity(holder).add_child(e);
                    }
                    commands.entity(root).add_child(holder);
                }
            }
        }
        // Landeplattformen dieser Station
        for &pad in &st.pads {
            spawn_pad(
                &mut commands,
                &mut art,
                &mut meshes,
                &mut mats,
                &pad_lights,
                &sim.0,
                pad,
                srgb(accent),
            );
            // Kran oder Containerstapel an einem Ende der Plattform (Deko, hinter der Ebene).
            let pd = sim.0.world.pads[pad].clone();
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let end = pd.center + pd.tangent() * side * (pd.half_width + 0.9);
            if rng.chance(0.6) {
                spawn_crane(
                    &mut commands,
                    &mut art,
                    &mut meshes,
                    &mut mats,
                    end,
                    pd.normal,
                    -side * (pd.half_width * 0.9 + 0.9),
                    4.6,
                    srgb(darker(accent, 0.8)),
                );
            } else {
                spawn_containers(
                    &mut commands,
                    &mut art,
                    &mut meshes,
                    &mut mats,
                    end,
                    pd.normal,
                    &mut rng,
                );
            }
        }
        // Ausgestellte Schiffe in der Werft (hinter der Spielebene, ohne Kollision).
        let sd = &data.0.world.stations[si];
        let rows = sd.layout.len() as f32;
        let cols = sd.layout.first().map(|r| r.chars().count()).unwrap_or(0) as f32;
        for d in &sd.decor_ships {
            let def = data.0.ship(&d.ship);
            let mut ship = Ship::build(def, &Loadout::full(def), &ShipStats::default());
            let cell_c = st.pos
                + Vec2::new(
                    (d.cell.0 as f32 - (cols - 1.0) * 0.5) * st.cell,
                    ((rows - 1.0) * 0.5 - d.cell.1 as f32) * st.cell,
                );
            let floor = cell_c.y - st.cell * 0.5;
            ship.angle = 0.0;
            ship.pos = Vec2::new(cell_c.x, floor + ship.rest_height() + 0.1);
            let origin = ship.origin();
            let e = super::ship_vis::spawn_ship_model(
                &mut commands,
                &mut art,
                &mut meshes,
                &mut mats,
                &ship,
                def,
                ShipModelOpts::default(),
            );
            commands
                .entity(e)
                .insert(Transform::from_xyz(origin.x, origin.y, -3.2));
        }
    }

    // --- Planeten ---------------------------------------------------------
    let planet_defs = data.0.world.planets.clone();
    let textures: Vec<Image> = std::thread::scope(|s| {
        let handles: Vec<_> = planet_defs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let colors: Vec<Rgb> = p.colors.iter().map(|c| hex(c)).collect();
                s.spawn(move || textures::planet_disc(&colors, 700 + i as u32 * 31, 1024))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let planet_mesh = meshes.add(meshes::planet_sphere());
    for ((pi, p), tex) in world.planets.iter().enumerate().zip(textures) {
        let pd = &planet_defs[pi];
        let tex = images.add(tex);
        let mat = mats.add(StandardMaterial {
            base_color_texture: Some(tex),
            perceptual_roughness: 0.85,
            metallic: 0.0,
            ..default()
        });
        let r = p.radius;
        commands.spawn((
            Mesh3d(planet_mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_xyz(p.pos.x, p.pos.y, 0.0).with_scale(Vec3::new(r, r, r * 0.3)),
            Swirl(Vec3::Z, 0.004 + pi as f32 * 0.002),
            Name::new(p.name.clone()),
        ));
        // Atmosphäre als Lichthof hinter dem Planeten.
        let atmo = srgb(hex(&pd.atmosphere));
        let halo = art.glow_mat(&mut mats, atmo, 1.6);
        commands.spawn((
            Mesh3d(art.quad.clone()),
            MeshMaterial3d(halo),
            Transform::from_xyz(p.pos.x, p.pos.y, -r * 0.5).with_scale(Vec3::splat(r * 2.9)),
            NotShadowCaster,
        ));
        if pd.rings {
            let ring_mat = mats.add(StandardMaterial {
                base_color: atmo.with_alpha(0.55),
                base_color_texture: Some(art.ring.clone()),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            commands.spawn((
                Mesh3d(art.quad.clone()),
                MeshMaterial3d(ring_mat),
                Transform::from_xyz(p.pos.x, p.pos.y, 0.0)
                    .with_rotation(Quat::from_rotation_z(0.3) * Quat::from_rotation_x(1.25))
                    .with_scale(Vec3::splat(r * 4.2)),
                NotShadowCaster,
            ));
        }
        // Erzvorkommen als leuchtende Kristallgruppen am Rand.
        let ore_col = srgb(p.ore.color());
        let crystal_mat = mats.add(StandardMaterial {
            base_color: ore_col,
            emissive: ore_col.to_linear() * 6.0,
            perceptual_roughness: 0.15,
            metallic: 0.2,
            ..default()
        });
        for (di, d) in p.deposits.iter().enumerate() {
            let dir = Vec2::new(d.angle.cos(), d.angle.sin());
            let base = p.pos + dir * (p.radius - 0.4);
            let root = commands
                .spawn((
                    Transform::from_xyz(base.x, base.y, 0.0).with_rotation(Quat::from_rotation_z(
                        d.angle - std::f32::consts::FRAC_PI_2,
                    )),
                    Visibility::default(),
                    DepositVis {
                        planet: pi,
                        deposit: di,
                        base: Vec3::ONE,
                    },
                ))
                .id();
            for k in 0..5 {
                let x = (k as f32 - 2.0) * 0.9;
                let h = 1.4 + ((k * 7 + di * 3) % 5) as f32 * 0.45;
                let tilt = (k as f32 - 2.0) * 0.18;
                let c = commands
                    .spawn((
                        Mesh3d(art.crystal.clone()),
                        MeshMaterial3d(crystal_mat.clone()),
                        Transform::from_xyz(x, h * 0.4, ((k % 3) as f32 - 1.0) * 0.6)
                            .with_rotation(Quat::from_rotation_z(-tilt))
                            .with_scale(Vec3::new(1.2, h, 1.2)),
                        NotShadowCaster,
                    ))
                    .id();
                commands.entity(root).add_child(c);
            }
            let light = commands
                .spawn((
                    PointLight {
                        color: ore_col,
                        intensity: 60_000.0,
                        range: 14.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 2.5, 2.0),
                ))
                .id();
            commands.entity(root).add_child(light);
        }
        if let Some(pad) = p.pad {
            spawn_pad(
                &mut commands,
                &mut art,
                &mut meshes,
                &mut mats,
                &pad_lights,
                &sim.0,
                pad,
                srgb([1.0, 0.75, 0.15]),
            );
        }
        // Freie Landezonen: gleiche Druckplatte, kühlere Markierung.
        for &pad in &p.zones {
            spawn_pad(
                &mut commands,
                &mut art,
                &mut meshes,
                &mut mats,
                &pad_lights,
                &sim.0,
                pad,
                srgb([0.55, 0.78, 0.82]),
            );
        }
    }
    // Stützblöcke der Planeten-Außenposten
    let outpost_mat = mats.add(art.panel_mat(srgb([0.85, 0.87, 0.92]), 0.5, 0.2));
    for col in &world.colliders {
        if let (Shape::Poly(q), Surface::Structure) = (col.shape, col.surface) {
            let e = q.v[1] - q.v[0];
            let ang = e.y.atan2(e.x);
            let c = q.center();
            let size = Vec3::new(e.length(), (q.v[2] - q.v[1]).length(), 2.4);
            let m = art.bevel_box(&mut meshes, size);
            commands.spawn((
                Mesh3d(m),
                MeshMaterial3d(outpost_mat.clone()),
                Transform::from_xyz(c.x, c.y, 0.0).with_rotation(Quat::from_rotation_z(ang)),
            ));
            let top = c + crate::sim::geom::rot(Vec2::new(0.0, size.y * 0.5 + 0.3), ang);
            spawn_beacon(
                &mut commands,
                &mut art,
                &mut mats,
                top.extend(0.6),
                srgb([1.0, 0.3, 0.2]),
                c.x,
                None,
            );
        }
    }

    // --- Rotoren ----------------------------------------------------------
    for (i, sp) in world.spinners.iter().enumerate() {
        let col = srgb(sp.color);
        let arm_mat = mats.add(art.panel_mat(col, 0.4, 0.15));
        let hub_mat = mats.add(art.emissive_mat(srgb([1.0, 0.12, 0.1]), 4.0));
        let root = commands
            .spawn((
                Transform::from_xyz(sp.pos.x, sp.pos.y, 0.0),
                Visibility::default(),
                SpinnerVis(i),
            ))
            .id();
        for a in 0..sp.arms {
            let ang = std::f32::consts::PI * a as f32 / sp.arms as f32;
            let m = art.bevel_box(
                &mut meshes,
                Vec3::new(sp.arm_length * 2.0, sp.arm_width, 0.9),
            );
            let arm = commands
                .spawn((
                    Mesh3d(m),
                    MeshMaterial3d(arm_mat.clone()),
                    Transform::from_rotation(Quat::from_rotation_z(ang))
                        .with_translation(Vec3::Z * (a as f32 * 0.05)),
                ))
                .id();
            commands.entity(root).add_child(arm);
        }
        let hub = commands
            .spawn((
                Mesh3d(art.sphere.clone()),
                MeshMaterial3d(hub_mat),
                Transform::from_xyz(0.0, 0.0, 0.6).with_scale(Vec3::splat(sp.arm_width * 0.8)),
            ))
            .id();
        commands.entity(root).add_child(hub);
    }

    // --- Anomalien ----------------------------------------------------------
    for an in &world.anomalies {
        if an.is_black_hole() {
            spawn_black_hole(
                &mut commands,
                &mut art,
                &mut meshes,
                &mut mats,
                &mut images,
                an,
            );
            continue;
        }
        let tex = images.add(textures::anomaly(512, 77));
        let disk = mats.add(StandardMaterial {
            base_color: Color::srgba(0.85, 0.8, 1.0, 0.85),
            base_color_texture: Some(tex),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.spawn((
            Mesh3d(art.quad.clone()),
            MeshMaterial3d(disk),
            Transform::from_xyz(an.pos.x, an.pos.y, -1.0).with_scale(Vec3::splat(an.radius * 1.4)),
            Swirl(Vec3::Z, 0.25),
            NotShadowCaster,
        ));
        // Leuchtende Akkretionsringe, gegenläufig rotierend.
        for (k, (col, size, speed)) in [
            ([1.0, 0.25, 0.75], 7.0f32, 0.6f32),
            ([1.0, 0.55, 0.15], 12.0, -0.35),
            ([0.55, 0.3, 1.0], 20.0, 0.18),
        ]
        .into_iter()
        .enumerate()
        {
            let c = srgb(col).to_linear();
            let ring = mats.add(StandardMaterial {
                base_color: Color::LinearRgba(c * (0.9 - k as f32 * 0.22)),
                base_color_texture: Some(art.ring.clone()),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                ..default()
            });
            commands.spawn((
                Mesh3d(art.quad.clone()),
                MeshMaterial3d(ring),
                Transform::from_xyz(an.pos.x, an.pos.y, -0.5 + k as f32 * 0.05)
                    .with_rotation(Quat::from_rotation_x(0.35 * (k as f32 - 1.0)))
                    .with_scale(Vec3::new(
                        an.core_radius * size,
                        an.core_radius * size * 0.92,
                        1.0,
                    )),
                Swirl(Vec3::Z, speed),
                NotShadowCaster,
            ));
        }
        let lens = art.glow_mat(&mut mats, srgb([1.0, 0.35, 0.6]), 0.45);
        commands.spawn((
            Mesh3d(art.quad.clone()),
            MeshMaterial3d(lens),
            Transform::from_xyz(an.pos.x, an.pos.y, -0.8)
                .with_scale(Vec3::splat(an.core_radius * 22.0)),
            NotShadowCaster,
        ));
        let core = mats.add(art.emissive_mat(srgb([1.0, 0.08, 0.06]), 4.0));
        commands.spawn((
            Mesh3d(art.sphere.clone()),
            MeshMaterial3d(core),
            Transform::from_xyz(an.pos.x, an.pos.y, 0.5)
                .with_scale(Vec3::splat(an.core_radius * 0.45)),
        ));
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.25, 0.2),
                intensity: 4_000_000.0,
                range: 120.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(an.pos.x, an.pos.y, 8.0),
        ));
    }
}

/// Schwarzes Loch: schwarze Kugel (Ereignishorizont), heller Photonenring, schräg
/// gestellte Akkretionsscheibe, dunkler Hof. Bewusst ohne Rot – sonst wie die Anomalie.
fn spawn_black_hole(
    commands: &mut Commands,
    art: &mut Art,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    an: &crate::sim::world::Anomaly,
) {
    let (p, r) = (an.pos, an.core_radius);
    // Dunkler Hof: schluckt das Hintergrundlicht.
    let tex = images.add(textures::anomaly(512, 91));
    let shade = mats.add(StandardMaterial {
        base_color: Color::srgba(0.05, 0.03, 0.09, 0.9),
        base_color_texture: Some(tex),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(art.quad.clone()),
        MeshMaterial3d(shade),
        Transform::from_xyz(p.x, p.y, -2.0).with_scale(Vec3::splat(r * 14.0)),
        Swirl(Vec3::Z, 0.05),
        NotShadowCaster,
    ));
    let horizon = mats.add(StandardMaterial {
        base_color: Color::BLACK,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Mesh3d(art.sphere_hi.clone()),
        MeshMaterial3d(horizon),
        Transform::from_xyz(p.x, p.y, 0.0).with_scale(Vec3::splat(r)),
        NotShadowCaster,
    ));
    // Photonenring knapp außerhalb des Horizonts: dünner, scharfer Lichtring.
    let photon = mats.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(5.0, 4.0, 3.0)),
        unlit: true,
        ..default()
    });
    let torus = meshes.add(Torus::new(r * 1.08, r * 1.14));
    commands.spawn((
        Mesh3d(torus),
        MeshMaterial3d(photon),
        Transform::from_xyz(p.x, p.y, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        NotShadowCaster,
    ));
    // Akkretionsscheibe: zwei schräge, gegenläufige Ringe in Glut- und Weißtönen.
    for (k, (col, size, speed, tilt)) in [
        ([1.0, 0.62, 0.25], 4.2f32, 0.9f32, 1.18f32),
        ([1.0, 0.86, 0.6], 3.1, 1.4, 1.22),
        ([0.75, 0.35, 0.15], 6.0, 0.5, 1.12),
    ]
    .into_iter()
    .enumerate()
    {
        let c = srgb(col).to_linear() * (1.6 - k as f32 * 0.35);
        let ring = mats.add(StandardMaterial {
            base_color: Color::LinearRgba(c),
            base_color_texture: Some(art.ring.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Mesh3d(art.quad.clone()),
            MeshMaterial3d(ring),
            Transform::from_xyz(p.x, p.y, 0.1 + k as f32 * 0.03)
                .with_rotation(Quat::from_rotation_z(0.35) * Quat::from_rotation_x(tilt))
                .with_scale(Vec3::splat(r * size)),
            Swirl(Vec3::Z, speed),
            NotShadowCaster,
        ));
    }
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.7, 0.4),
            intensity: 2_500_000.0,
            range: 90.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(p.x, p.y, 10.0),
    ));
}

fn spawn_beacon(
    commands: &mut Commands,
    art: &mut Art,
    mats: &mut Assets<StandardMaterial>,
    pos: Vec3,
    color: Color,
    phase: f32,
    parent: Option<Entity>,
) {
    let bulb = mats.add(art.emissive_mat(color, 8.0));
    let halo = art.glow_mat(mats, color, 3.0);
    let e = commands
        .spawn((
            Transform::from_translation(pos),
            Visibility::default(),
            Beacon {
                phase,
                speed: 2.2,
                base: 140_000.0,
            },
            PointLight {
                color,
                intensity: 140_000.0,
                range: 26.0,
                shadow_maps_enabled: false,
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn((
                Mesh3d(art.sphere.clone()),
                MeshMaterial3d(bulb),
                Transform::from_scale(Vec3::splat(0.55)),
            ));
            c.spawn((
                Mesh3d(art.quad.clone()),
                MeshMaterial3d(halo),
                Transform::from_xyz(0.0, 0.0, 0.6).with_scale(Vec3::splat(4.5)),
                NotShadowCaster,
            ));
        })
        .id();
    if let Some(p) = parent {
        commands.entity(p).add_child(e);
    }
}

/// Ein Lauflicht an der Vorderkante einer Landeplattform.
#[derive(Component)]
pub struct PadLight {
    pub pad: usize,
    pub index: usize,
    pub count: usize,
}

/// Gemeinsame Materialien für die Plattform-Lauflichter.
#[derive(Resource, Clone)]
pub struct PadLightMats {
    pub off: Handle<StandardMaterial>,
    pub idle: Handle<StandardMaterial>,
    pub green: Handle<StandardMaterial>,
    pub yellow: Handle<StandardMaterial>,
    pub red: Handle<StandardMaterial>,
}

pub const CONTAINER_COLORS: [[f32; 3]; 5] = [
    [0.54, 0.29, 0.18],
    [0.25, 0.37, 0.48],
    [0.36, 0.42, 0.23],
    [0.43, 0.44, 0.46],
    [0.6, 0.48, 0.18],
];

/// Kran: Mast, Ausleger, Seil und Haken (Deko, hinter der Spielebene).
fn spawn_crane(
    commands: &mut Commands,
    art: &mut Art,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    base: Vec2,
    up: Vec2,
    reach: f32,
    height: f32,
    color: Color,
) {
    let angle = f32::atan2(-up.x, up.y);
    let frame = mats.add(art.panel_mat(color, 0.5, 0.4));
    let dark = mats.add(art.panel_mat(Color::srgb(0.12, 0.13, 0.15), 0.4, 0.7));
    let mast = art.bevel_box(meshes, Vec3::new(0.35, height, 0.35));
    let boom = art.bevel_box(meshes, Vec3::new(reach, 0.28, 0.3));
    let hook = art.bevel_box(meshes, Vec3::new(0.45, 0.35, 0.35));
    let root = commands
        .spawn((
            Transform::from_xyz(base.x, base.y, -1.5).with_rotation(Quat::from_rotation_z(angle)),
            Visibility::default(),
        ))
        .id();
    let s = reach.signum();
    let parts = [
        commands
            .spawn((
                Mesh3d(mast),
                MeshMaterial3d(frame.clone()),
                Transform::from_xyz(0.0, height * 0.5, 0.0),
            ))
            .id(),
        commands
            .spawn((
                Mesh3d(boom),
                MeshMaterial3d(frame.clone()),
                Transform::from_xyz(reach * 0.45, height, 0.0),
            ))
            .id(),
        // Gegengewicht
        commands
            .spawn((
                Mesh3d(art.bevel_box(meshes, Vec3::new(0.7, 0.55, 0.45))),
                MeshMaterial3d(dark.clone()),
                Transform::from_xyz(-s * 0.55, height, 0.0),
            ))
            .id(),
        commands
            .spawn((
                Mesh3d(art.cylinder.clone()),
                MeshMaterial3d(dark.clone()),
                Transform::from_xyz(reach * 0.85, height - 0.9, 0.0)
                    .with_scale(Vec3::new(0.03, 1.8, 0.03)),
            ))
            .id(),
        commands
            .spawn((
                Mesh3d(hook),
                MeshMaterial3d(dark),
                Transform::from_xyz(reach * 0.85, height - 1.9, 0.0),
            ))
            .id(),
    ];
    commands.entity(root).add_children(&parts);
}

/// Stapel aus Frachtcontainern (Deko, hinter der Spielebene).
fn spawn_containers(
    commands: &mut Commands,
    art: &mut Art,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    base: Vec2,
    up: Vec2,
    rng: &mut Rng,
) {
    let angle = f32::atan2(-up.x, up.y);
    let root = commands
        .spawn((
            Transform::from_xyz(base.x, base.y, -1.2).with_rotation(Quat::from_rotation_z(angle)),
            Visibility::default(),
        ))
        .id();
    let box_mesh = art.prism(
        meshes,
        &crate::sim::data::PartShape::Box.outline(Vec2::new(0.8, 0.42)),
        1.0,
        0.06,
        Some((0.08, 0.04)),
    );
    let layers = 1 + rng.index(3);
    let mut kids = Vec::new();
    for l in 0..layers {
        let n = 2 - (l / 2).min(1);
        for k in 0..n {
            let c = CONTAINER_COLORS[rng.index(CONTAINER_COLORS.len())];
            let m = mats.add(art.panel_mat(srgb(c), 0.6, 0.3));
            let x = (k as f32 - (n - 1) as f32 * 0.5) * 1.68 + if l % 2 == 1 { 0.4 } else { 0.0 };
            kids.push(
                commands
                    .spawn((
                        Mesh3d(box_mesh.clone()),
                        MeshMaterial3d(m),
                        Transform::from_xyz(x, 0.42 + l as f32 * 0.86, 0.0),
                    ))
                    .id(),
            );
        }
    }
    commands.entity(root).add_children(&kids);
}

fn spawn_pad(
    commands: &mut Commands,
    art: &mut Art,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    lights: &PadLightMats,
    sim: &crate::sim::SimState,
    pad: usize,
    color: Color,
) {
    let p = &sim.world.pads[pad];
    let angle = p.ship_angle();
    let width = p.half_width * 2.0 / 0.92;
    let rot = Quat::from_rotation_z(angle);
    let to_world = |local: Vec2, z: f32| -> Transform {
        let w = p.center + crate::sim::geom::rot(local, angle);
        Transform::from_xyz(w.x, w.y, z).with_rotation(rot)
    };
    // Gehäuse (dunkles Metall) und darauf die eigentliche Druckplatte in Warnfarbe.
    let housing = art.prism(
        meshes,
        &crate::sim::data::PartShape::Chamfer(0.6).outline(Vec2::new(width * 0.5, 0.13)),
        3.6,
        0.08,
        None,
    );
    let housing_mat = mats.add(art.panel_mat(Color::srgb(0.16, 0.17, 0.19), 0.45, 0.6));
    commands.spawn((
        Mesh3d(housing),
        MeshMaterial3d(housing_mat),
        to_world(Vec2::new(0.0, -0.22), 0.0),
    ));
    let plate = art.prism(
        meshes,
        &crate::sim::data::PartShape::Taper(0.96, 1.0).outline(Vec2::new(width * 0.5 - 0.3, 0.06)),
        3.0,
        0.04,
        None,
    );
    let plate_mat = mats.add(StandardMaterial {
        base_color: color,
        base_color_texture: Some(art.stripes.clone()),
        perceptual_roughness: 0.55,
        metallic: 0.2,
        ..default()
    });
    commands.spawn((
        Mesh3d(plate),
        MeshMaterial3d(plate_mat),
        to_world(Vec2::new(0.0, -0.06), 0.0),
    ));

    // Lauflichter an der Vorderkante.
    let count = ((width / 1.1) as usize).clamp(3, 14);
    let bulb = art.bevel_box(meshes, Vec3::new(0.32, 0.1, 0.08));
    for i in 0..count {
        let x = (i as f32 + 0.5) / count as f32 * (width - 0.8) - (width - 0.8) * 0.5;
        commands.spawn((
            Mesh3d(bulb.clone()),
            MeshMaterial3d(lights.idle.clone()),
            to_world(Vec2::new(x, -0.24), 1.84),
            PadLight {
                pad,
                index: i,
                count,
            },
        ));
    }
    // Andockmarkierungen: Winkel an beiden Enden, die nach oben zeigen.
    let chevron = art.prism(
        meshes,
        &[
            Vec2::new(-0.35, -0.18),
            Vec2::new(0.35, -0.18),
            Vec2::new(0.0, 0.2),
        ],
        0.06,
        0.02,
        None,
    );
    let white = mats.add(art.panel_mat(Color::srgb(0.85, 0.86, 0.84), 0.5, 0.1));
    for side in [-1.0f32, 1.0] {
        commands.spawn((
            Mesh3d(chevron.clone()),
            MeshMaterial3d(white.clone()),
            to_world(Vec2::new(side * (width * 0.5 - 0.45), -0.2), 1.83),
        ));
        // Lichtmast hinter der Spielebene
        let mast_h = 2.4;
        let pole = to_world(
            Vec2::new(side * (width * 0.5 + 0.35), mast_h * 0.5 - 0.3),
            -1.6,
        );
        commands.spawn((
            Mesh3d(art.cylinder.clone()),
            MeshMaterial3d(white.clone()),
            pole.with_scale(Vec3::new(0.07, mast_h, 0.07)),
        ));
        let lamp = mats.add(art.emissive_mat(color, 5.0));
        commands.spawn((
            Mesh3d(art.sphere.clone()),
            MeshMaterial3d(lamp),
            to_world(Vec2::new(side * (width * 0.5 + 0.35), mast_h - 0.3), -1.6)
                .with_scale(Vec3::splat(0.16)),
        ));
    }
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.92, 0.8),
            intensity: 90_000.0,
            range: 14.0,
            shadow_maps_enabled: false,
            ..default()
        },
        to_world(Vec2::new(0.0, 3.0), 3.5),
    ));
    // Leuchtende Andockzone (Ampelfarbe).
    let glow = mats.add(StandardMaterial {
        base_color: Color::srgba(0.2, 1.0, 0.5, 0.0),
        base_color_texture: Some(art.glow.clone()),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let gc = p.center + p.normal * 1.6;
    commands.spawn((
        Mesh3d(art.quad.clone()),
        MeshMaterial3d(glow),
        Transform::from_xyz(gc.x, gc.y, 1.9)
            .with_rotation(rot)
            .with_scale(Vec3::new(width * 1.1, 4.0, 1.0)),
        PadGlow(pad),
        NotShadowCaster,
    ));
}

/// Lauflichter: im Ruhezustand gedimmt, beim Anflug ein Lauflicht in Ampelfarbe,
/// angedockt durchgehend grün.
pub fn update_pad_lights(
    time: Res<Time>,
    sim: Res<Sim>,
    mats: Option<Res<PadLightMats>>,
    mut q: Query<(&PadLight, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let Some(mats) = mats else { return };
    let t = time.elapsed_secs();
    let guide = sim.0.dock_guide();
    for (l, mut m) in &mut q {
        let want = if sim.0.ship.docked == Some(l.pad) {
            &mats.green
        } else if let Some(g) = guide.filter(|g| g.pad == l.pad && g.distance < 22.0) {
            let head = ((t * 9.0) as usize) % l.count;
            let lit = (l.index + l.count - head) % l.count < 2;
            if !lit {
                &mats.off
            } else {
                match g.overall() {
                    crate::sim::dock::Light::Green => &mats.green,
                    crate::sim::dock::Light::Yellow => &mats.yellow,
                    crate::sim::dock::Light::Red => &mats.red,
                }
            }
        } else {
            &mats.idle
        };
        if m.0 != *want {
            m.0 = want.clone();
        }
    }
}

pub fn spawn_background(
    mut commands: Commands,
    data: Res<Data>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Sternenschichten in verschiedenen Tiefen → echte Parallaxe durch die Perspektive.
    let star_mat = mats.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(2.4, 2.4, 2.4)),
        base_color_texture: Some(art.glow.clone()),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        fog_enabled: false,
        ..default()
    });
    let mut rng = Rng::new(99);
    for (depth, tile, count, size) in [
        (180.0f32, 700.0f32, 700, (0.18, 0.5)),
        (520.0, 1300.0, 1300, (0.5, 1.1)),
        (1100.0, 2300.0, 1900, (1.0, 2.4)),
    ] {
        let stars: Vec<(Vec3, f32, [f32; 4])> = (0..count)
            .map(|_| {
                let p = Vec3::new(
                    rng.range(-tile * 0.5, tile * 0.5),
                    rng.range(-tile * 0.5, tile * 0.5),
                    -depth,
                );
                let s = rng.range(size.0, size.1) * if rng.chance(0.04) { 2.2 } else { 1.0 };
                let t = rng.f32();
                let c = if t < 0.6 {
                    [1.0, 1.0, 1.0, 1.0]
                } else if t < 0.75 {
                    [0.6, 0.8, 1.0, 1.0]
                } else if t < 0.88 {
                    [1.0, 0.85, 0.6, 1.0]
                } else {
                    [1.0, 0.6, 0.9, 1.0]
                };
                let b = rng.range(0.35, 1.0);
                (p, s, [c[0] * b, c[1] * b, c[2] * b, 1.0])
            })
            .collect();
        let mesh = meshes.add(meshes::star_layer(&stars));
        for ox in -1..=1 {
            for oy in -1..=1 {
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(star_mat.clone()),
                    Transform::default(),
                    StarTile {
                        tile,
                        offset: IVec2::new(ox, oy),
                    },
                    NotShadowCaster,
                ));
            }
        }
    }

    // Nebel pro Region, zwei Farbschichten.
    for (ri, r) in data.0.world.regions.iter().enumerate() {
        for (k, col) in [&r.colors.0, &r.colors.1].iter().enumerate() {
            let c = srgb(hex(col)).to_linear();
            let intensity = if k == 0 { 1.7 } else { 1.15 };
            let mat = mats.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::rgb(
                    c.red * intensity,
                    c.green * intensity,
                    c.blue * intensity,
                )),
                base_color_texture: Some(art.nebulae[(ri + k) % art.nebulae.len()].clone()),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                fog_enabled: false,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let depth = 1500.0 + k as f32 * 260.0 + ri as f32 * 15.0;
            let size = r.radius * (3.6 + k as f32 * 0.8);
            let off = Vec2::new(
                ((ri * 37 + k * 11) % 9) as f32 - 4.0,
                ((ri * 13 + k * 29) % 9) as f32 - 4.0,
            ) * r.radius
                * 0.06;
            commands.spawn((
                Mesh3d(art.quad.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz(r.center.0 + off.x, r.center.1 + off.y, -depth)
                    .with_rotation(Quat::from_rotation_z((ri * 3 + k) as f32 * 0.9))
                    .with_scale(Vec3::splat(size)),
                NotShadowCaster,
            ));
        }
    }

    // Felsbrocken hinter der Spielebene: lassen Asteroidenfelder dichter wirken (nur Optik).
    let mut rng2 = Rng::new(555);
    for f in &data.0.world.asteroid_fields {
        let mat = mats.add(StandardMaterial {
            base_color: srgb(hex(&f.color)),
            base_color_texture: Some(art.rock.clone()),
            perceptual_roughness: 0.95,
            ..default()
        });
        let c = v(f.center);
        for _ in 0..(f.count as usize * 2) {
            let a = rng2.range(0.0, std::f32::consts::TAU);
            let d = f.radius * 1.25 * rng2.f32().sqrt();
            let z = -rng2.range(14.0, 90.0);
            let p = c + Vec2::new(a.cos(), a.sin()) * d * (1.0 + -z / 140.0);
            let mesh = art.asteroids[rng2.index(art.asteroids.len())].clone();
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(p.x, p.y, z)
                    .with_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        rng2.range(0.0, 6.0),
                        rng2.range(0.0, 6.0),
                        rng2.range(0.0, 6.0),
                    ))
                    .with_scale(Vec3::splat(rng2.range(0.8, 4.5))),
                Swirl(
                    Vec3::new(rng2.range(-1.0, 1.0), rng2.range(-1.0, 1.0), 1.0).normalize(),
                    rng2.range(-0.3, 0.3),
                ),
                NotShadowCaster,
            ));
        }
    }

    // Farbiger Dunst ganz hinten: der Himmel ist nie ganz schwarz.
    for (ri, r) in data.0.world.regions.iter().enumerate() {
        let c = srgb(hex(&r.colors.0));
        let mat = art.glow_mat(&mut mats, c, 0.16);
        commands.spawn((
            Mesh3d(art.quad.clone()),
            MeshMaterial3d(mat),
            Transform::from_xyz(r.center.0, r.center.1, -2600.0 - ri as f32 * 10.0)
                .with_scale(Vec3::splat(r.radius * 5.0)),
            NotShadowCaster,
        ));
    }

    // Ferne Riesenplaneten.
    let backdrop = data.0.world.backdrop.clone();
    let texs: Vec<Image> = std::thread::scope(|s| {
        let hs: Vec<_> = backdrop
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let colors: Vec<Rgb> = b.colors.iter().map(|c| hex(c)).collect();
                s.spawn(move || textures::planet(&colors, 4100 + i as u32 * 7, 512, 256, 14.0))
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (b, tex) in backdrop.iter().zip(texs) {
        let tex = images.add(tex);
        let mat = mats.add(StandardMaterial {
            base_color_texture: Some(tex),
            perceptual_roughness: 0.9,
            fog_enabled: false,
            ..default()
        });
        let pos = v(b.pos);
        commands.spawn((
            Mesh3d(art.sphere_hi.clone()),
            MeshMaterial3d(mat),
            Transform::from_xyz(pos.x, pos.y, -b.depth)
                .with_rotation(Quat::from_rotation_x(0.4) * Quat::from_rotation_z(0.3))
                .with_scale(Vec3::splat(b.radius)),
            Swirl(Vec3::Y, 0.012),
            NotShadowCaster,
        ));
        let halo_col = srgb(hex(b
            .colors
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("#ffffff")));
        let halo = art.glow_mat(&mut mats, halo_col, 0.9);
        commands.spawn((
            Mesh3d(art.quad.clone()),
            MeshMaterial3d(halo),
            Transform::from_xyz(pos.x, pos.y, -b.depth - b.radius)
                .with_scale(Vec3::splat(b.radius * 2.9)),
            NotShadowCaster,
        ));
        if b.rings {
            let ring_mat = mats.add(StandardMaterial {
                base_color: halo_col.with_alpha(0.7),
                base_color_texture: Some(art.ring.clone()),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                double_sided: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            });
            commands.spawn((
                Mesh3d(art.quad.clone()),
                MeshMaterial3d(ring_mat),
                Transform::from_xyz(pos.x, pos.y, -b.depth)
                    .with_rotation(Quat::from_rotation_z(-0.35) * Quat::from_rotation_x(1.2))
                    .with_scale(Vec3::splat(b.radius * 4.4)),
                NotShadowCaster,
            ));
        }
    }
    let _ = Ore::ALL;
}

pub fn update_stars(
    cam: Query<&Transform, (With<GameCamera>, Without<StarTile>)>,
    mut tiles: Query<(&StarTile, &mut Transform)>,
) {
    let Ok(cam) = cam.single() else { return };
    for (t, mut tr) in &mut tiles {
        let base = (cam.translation.truncate() / t.tile).round() * t.tile;
        tr.translation = (base + t.offset.as_vec2() * t.tile).extend(0.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_dynamic_world(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    mut spinners: Query<(&SpinnerVis, &mut Transform), (Without<DepositVis>, Without<Swirl>)>,
    mut beacons: Query<(&Beacon, &mut PointLight)>,
    mut deposits: Query<(&DepositVis, &mut Transform), (Without<SpinnerVis>, Without<Swirl>)>,
    mut swirls: Query<(&Swirl, &mut Transform), (Without<SpinnerVis>, Without<DepositVis>)>,
    pads: Query<(&PadGlow, &MeshMaterial3d<StandardMaterial>)>,
    planets: Query<(&PlanetSpin, &MeshMaterial3d<StandardMaterial>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let alpha = fixed.overstep_fraction();
    let t = time.elapsed_secs();
    for (sv, mut tr) in &mut spinners {
        let s = &sim.0.world.spinners[sv.0];
        let a = s.prev_angle + (s.angle - s.prev_angle) * alpha;
        tr.rotation = Quat::from_rotation_z(a);
    }
    for (b, mut l) in &mut beacons {
        let k = ((t * b.speed + b.phase).sin() * 0.5 + 0.5).powf(3.0);
        l.intensity = b.base * (0.15 + 0.85 * k);
    }
    for (d, mut tr) in &mut deposits {
        let dep = &sim.0.world.planets[d.planet].deposits[d.deposit];
        let k = (dep.amount / dep.max.max(0.01)).clamp(0.05, 1.0);
        tr.scale = d.base * (0.35 + 0.65 * k);
    }
    let dt = time.delta_secs();
    for (s, mut tr) in &mut swirls {
        tr.rotate_local_axis(Dir3::new(s.0).unwrap_or(Dir3::Z), s.1 * dt);
    }
    for (spin, mat) in &planets {
        if let Some(mut m) = mats.get_mut(&mat.0) {
            m.uv_transform.translation.x = (m.uv_transform.translation.x + spin.0 * dt).fract();
        }
    }
    let guide = sim.0.dock_guide();
    for (pg, mat) in &pads {
        let target = match guide {
            Some(g) if g.pad == pg.0 && g.distance < 22.0 => {
                use crate::sim::dock::Light;
                // Die Plattform zeigt dieselbe Ampel wie das HUD.
                match g.overall() {
                    Light::Green if g.in_zone => LinearRgba::rgb(0.2, 3.0, 0.8),
                    Light::Green | Light::Yellow => {
                        LinearRgba::rgb(2.2, 1.6, 0.1) * (0.7 + 0.3 * (t * 4.0).sin().abs())
                    }
                    Light::Red => {
                        LinearRgba::rgb(2.6, 0.25, 0.1) * (0.5 + 0.5 * (t * 8.0).sin().abs())
                    }
                }
            }
            _ => LinearRgba::rgb(0.0, 0.0, 0.0),
        };
        if let Some(mut m) = mats.get_mut(&mat.0) {
            m.base_color = Color::LinearRgba(target);
        }
    }
}
