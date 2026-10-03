//! Schiff, Körper (Asteroiden, Meteore, Kapseln, Wracks), Geschosse und Werkzeugeffekte.

use std::collections::HashMap;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::{Art, ship_pose, slot_color, srgb};
use crate::game::{Data, Sim};
use crate::sim::BodyKind;
use crate::sim::data::{PartKind, ShipDef, ToolKind, hex};
use crate::sim::geom::rot;
use crate::sim::ship::{CraneState, Loadout, Ship, ShipStats};
use crate::sim::world::angle_diff;

#[derive(Clone, Copy, Default)]
pub struct ShipModelOpts {
    pub slot_colors: bool,
    pub derelict: bool,
    /// Lackierung der Crew (sRGB); `None` = Werkslack bzw. Slotfarben bei den Flammen.
    pub hull: Option<[f32; 3]>,
    pub accent: Option<[f32; 3]>,
    pub flame: Option<[f32; 3]>,
}

/// Lackierung der Crew für das aktuelle Schiff als Farben.
pub fn livery_colors(
    sim: &crate::sim::SimState,
) -> (Option<[f32; 3]>, Option<[f32; 3]>, Option<[f32; 3]>) {
    let l = sim.crew.livery(&sim.crew.current_ship);
    let shop = &sim.data.shop;
    let paint = |i: Option<usize>| i.and_then(|i| shop.paints.get(i)).map(|p| hex(&p.color));
    let flame = l
        .flame
        .and_then(|i| shop.flames.get(i))
        .and_then(|f| f.color.as_ref())
        .map(|c| hex(c));
    (paint(l.hull), paint(l.accent), flame)
}

#[derive(Component)]
pub struct ShipVis {
    pub sig: String,
}

#[derive(Component)]
pub struct FlameVis {
    pub thruster: usize,
    pub core: bool,
    pub len: f32,
    pub radius: f32,
}

#[derive(Component)]
pub struct ToolHead(pub usize);

#[derive(Component)]
pub struct EngineLight;

#[derive(Component)]
pub struct ShieldVis;

#[derive(Component)]
pub struct BodyVis(pub u32);

#[derive(Component)]
pub struct ProjVis(pub u32);

/// Ein Segment des Kranseils (Werkzeug, Segment).
#[derive(Component)]
pub struct RopeVis(pub usize, pub usize);

const ROPE_SEGS: usize = 12;

/// Punkte entlang des Seils. Ist das Seil schlaff (Abstand < Länge), hängt es als
/// Parabel durch – optisch „nach unten“ auf dem Bildschirm, damit es wie ein Seil wirkt.
pub fn rope_points(a: Vec2, b: Vec2, length: Option<f32>) -> Vec<Vec2> {
    let d = b - a;
    let dist = d.length();
    let sag = match length {
        Some(l) if l > dist && dist > 0.05 => (3.0 * dist * (l - dist) / 8.0).sqrt().min(l * 0.5),
        _ => 0.0,
    };
    let dir = d / dist.max(1e-4);
    let mut perp = Vec2::new(-dir.y, dir.x);
    if perp.dot(Vec2::NEG_Y) < 0.0 {
        perp = -perp;
    }
    (0..=ROPE_SEGS)
        .map(|k| {
            let t = k as f32 / ROPE_SEGS as f32;
            a + d * t + perp * sag * 4.0 * t * (1.0 - t)
        })
        .collect()
}

#[derive(Component)]
pub struct ClawVis(pub usize);

#[derive(Component)]
pub struct DrillBeam(pub usize);

#[derive(Component)]
pub struct BlinkLight(pub f32);

/// Glimmende Bruchstelle eines Wracks (erlischt, wenn der Schrott abgebaut ist).
#[derive(Component)]
pub struct WreckEmber(pub u32);

#[derive(Component)]
pub struct NavLight {
    pub phase: f32,
    pub strobe: bool,
}

pub fn blink_nav_lights(time: Res<Time>, mut q: Query<(&NavLight, &mut Visibility)>) {
    let t = time.elapsed_secs();
    for (n, mut v) in &mut q {
        let on = if n.strobe {
            ((t + n.phase) * 1.1).fract() < 0.08
        } else {
            ((t + n.phase) * 0.9).fract() < 0.6
        };
        *v = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[derive(Resource, Default)]
pub struct VisMaps {
    pub bodies: HashMap<u32, Entity>,
    pub projectiles: HashMap<u32, Entity>,
    pub rock_mats: HashMap<(usize, bool), Handle<StandardMaterial>>,
}

pub fn ship_signature(ship: &Ship) -> String {
    let mut s = ship.def_id.clone();
    for t in &ship.thrusters {
        s.push_str(&format!("|t{:.2}", t.pos.x));
    }
    for t in &ship.tools {
        s.push_str(&format!("|{:?}{}", t.kind, t.slot));
    }
    s
}

fn part_depth(kind: &PartKind) -> f32 {
    match kind {
        PartKind::Hull => 1.6,
        PartKind::Cockpit => 1.15,
        PartKind::Armor => 1.25,
        PartKind::CargoPod(_) => 1.35,
        PartKind::Thruster(_) => 1.0,
        PartKind::Tool(_) => 0.9,
    }
}

/// Baut das 3D-Modell eines Schiffs aus seinen Teilen. Ursprung = Schiffsursprung.
pub fn spawn_ship_model(
    commands: &mut Commands,
    art: &mut Art,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    ship: &Ship,
    def: &ShipDef,
    opts: ShipModelOpts,
) -> Entity {
    let (hull_c, accent_c) = if opts.derelict {
        ([0.32, 0.33, 0.36], [0.45, 0.2, 0.18])
    } else {
        (
            opts.hull.unwrap_or_else(|| hex(&def.hull_color)),
            opts.accent.unwrap_or_else(|| hex(&def.accent_color)),
        )
    };
    // Lackierter Stahl statt Spielzeugplastik: etwas Metall, gedeckte Töne.
    let hull_mat = mats.add(art.panel_mat(srgb(hull_c), 0.48, 0.35));
    let accent_mat = mats.add(art.panel_mat(srgb(accent_c), 0.45, 0.3));
    let dark_mat = mats.add(art.panel_mat(srgb([0.12, 0.13, 0.15]), 0.4, 0.7));
    // Getöntes Cockpitglas mit Spiegelung, kaum Eigenleuchten.
    let glass_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.02, 0.04, 0.06),
        emissive: if opts.derelict {
            LinearRgba::BLACK
        } else {
            LinearRgba::rgb(0.01, 0.05, 0.07)
        },
        perceptual_roughness: 0.08,
        metallic: 0.85,
        reflectance: 1.0,
        ..default()
    });
    // Akzentstreifen als Lack, nicht leuchtend.
    let stripe_mat = mats.add(art.panel_mat(srgb(accent_c), 0.4, 0.3));

    let root = commands
        .spawn((
            Transform::default(),
            Visibility::default(),
            Name::new(def.name.clone()),
        ))
        .id();
    let mut children = Vec::new();

    for p in &ship.parts {
        let depth = part_depth(&p.kind);
        let size = Vec3::new(p.half.x * 2.0, p.half.y * 2.0, depth);
        let outline = p.shape.outline(p.half);
        let bevel = (p.half.min_element() * 0.3).min(0.2);
        // Rumpf und Panzer bekommen eingelassene Paneele, kleine Teile nur eine Fase.
        let panel = matches!(
            p.kind,
            PartKind::Hull | PartKind::Armor | PartKind::CargoPod(_)
        )
        .then_some((0.14, 0.07));
        let mesh = art.prism(meshes, &outline, depth, bevel, panel);
        let (mat, z) = match &p.kind {
            PartKind::Hull => (hull_mat.clone(), 0.0),
            PartKind::Cockpit => (hull_mat.clone(), 0.1),
            PartKind::Armor => (accent_mat.clone(), -0.05),
            PartKind::CargoPod(_) => (accent_mat.clone(), -0.1),
            PartKind::Thruster(_) => (dark_mat.clone(), -0.1),
            PartKind::Tool(_) => (dark_mat.clone(), 0.0),
        };
        children.push(
            commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(p.pos.x, p.pos.y, z),
                ))
                .id(),
        );
        match &p.kind {
            PartKind::Hull => {
                // Lackierter Akzentstreifen und Lüftungsschlitze für mehr Detail.
                let stripe = art.bevel_box(meshes, Vec3::new(size.x * 0.7, 0.12, 0.1));
                children.push(
                    commands
                        .spawn((
                            Mesh3d(stripe),
                            MeshMaterial3d(stripe_mat.clone()),
                            Transform::from_xyz(
                                p.pos.x,
                                p.pos.y + p.half.y * 0.55,
                                depth * 0.5 + 0.02,
                            ),
                        ))
                        .id(),
                );
                let vent = art.bevel_box(meshes, Vec3::new(0.12, p.half.y * 0.7, 0.1));
                for k in 0..4 {
                    let x = p.pos.x + (k as f32 - 1.5) * 0.24;
                    children.push(
                        commands
                            .spawn((
                                Mesh3d(vent.clone()),
                                MeshMaterial3d(dark_mat.clone()),
                                Transform::from_xyz(
                                    x,
                                    p.pos.y - p.half.y * 0.25,
                                    depth * 0.5 + 0.02,
                                ),
                            ))
                            .id(),
                    );
                }
            }
            PartKind::Cockpit => {
                // Verglasung als flaches, abgeschrägtes Visier in der Form des Cockpits.
                let glass_outline: Vec<Vec2> =
                    outline.iter().map(|v| *v * Vec2::new(0.78, 0.62)).collect();
                let visor = art.prism(meshes, &glass_outline, 0.22, 0.08, None);
                children.push(
                    commands
                        .spawn((
                            Mesh3d(visor),
                            MeshMaterial3d(glass_mat.clone()),
                            Transform::from_xyz(
                                p.pos.x,
                                p.pos.y + p.half.y * 0.12,
                                depth * 0.5 + 0.12,
                            ),
                        ))
                        .id(),
                );
            }
            PartKind::CargoPod(_) => {
                let band = art.bevel_box(meshes, Vec3::new(size.x * 1.04, 0.14, depth * 1.02));
                for k in [-1.0f32, 1.0] {
                    children.push(
                        commands
                            .spawn((
                                Mesh3d(band.clone()),
                                MeshMaterial3d(dark_mat.clone()),
                                Transform::from_xyz(p.pos.x, p.pos.y + k * p.half.y * 0.6, -0.1),
                            ))
                            .id(),
                    );
                }
            }
            _ => {}
        }
    }

    // Triebwerke: Düse, Slot-Farbring, Flamme.
    for (ti, t) in ship.thrusters.iter().enumerate() {
        let half = ship
            .parts
            .iter()
            .find(|p| matches!(p.kind, PartKind::Thruster(_)) && (p.pos - t.pos).length() < 1e-3)
            .map(|p| p.half)
            .unwrap_or(Vec2::splat(0.22));
        let col = if opts.slot_colors {
            slot_color(t.slot)
        } else {
            srgb([0.6, 0.65, 0.7])
        };
        let base = t.pos - Vec2::Y * (half.y + 0.18);
        children.push(
            commands
                .spawn((
                    Mesh3d(art.frustum.clone()),
                    MeshMaterial3d(dark_mat.clone()),
                    Transform::from_xyz(base.x, base.y, -0.1).with_scale(Vec3::new(
                        half.x * 0.85,
                        0.42,
                        half.x * 0.85,
                    )),
                ))
                .id(),
        );
        if opts.slot_colors {
            // Schmaler Kennring in Slotfarbe (das einzige Leuchten am Rumpf).
            let ring_mat = mats.add(art.emissive_mat(col, 1.4));
            let ring = art.bevel_box(meshes, Vec3::new(half.x * 2.04, 0.07, 1.04));
            children.push(
                commands
                    .spawn((
                        Mesh3d(ring),
                        MeshMaterial3d(ring_mat),
                        Transform::from_xyz(t.pos.x, t.pos.y + half.y * 0.2, -0.1),
                    ))
                    .id(),
            );
            let flame_len = 2.3;
            // Eigene Flammenfarbe außen, der Kern behält die Slotfarbe.
            let fcol = opts.flame.map(srgb).unwrap_or(col);
            let outer = art.glow_mat(mats, fcol, 7.0);
            let flame = mats.add(StandardMaterial {
                base_color: Color::LinearRgba(fcol.to_linear() * 7.0),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                ..default()
            });
            let core = mats.add(StandardMaterial {
                base_color: Color::LinearRgba(
                    LinearRgba::rgb(6.0, 6.0, 6.0) + col.to_linear() * 3.0,
                ),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                ..default()
            });
            let tip = base - Vec2::Y * 0.25;
            for (mat, core_flag, r) in [(flame, false, half.x * 0.85), (core, true, half.x * 0.42)]
            {
                children.push(
                    commands
                        .spawn((
                            Mesh3d(art.cone.clone()),
                            MeshMaterial3d(mat),
                            Transform::from_xyz(tip.x, tip.y, -0.1)
                                .with_rotation(Quat::from_rotation_z(std::f32::consts::PI))
                                .with_scale(Vec3::new(r, 0.01, r)),
                            FlameVis {
                                thruster: ti,
                                core: core_flag,
                                len: flame_len * if core_flag { 0.55 } else { 1.0 },
                                radius: r,
                            },
                            NotShadowCaster,
                        ))
                        .id(),
                );
            }
            // Leuchtfleck an der Düse
            children.push(
                commands
                    .spawn((
                        Mesh3d(art.quad.clone()),
                        MeshMaterial3d(outer),
                        Transform::from_xyz(tip.x, tip.y - 0.3, 0.7).with_scale(Vec3::splat(0.01)),
                        FlameVis {
                            thruster: ti,
                            core: false,
                            len: -1.0,
                            radius: 2.2,
                        },
                        NotShadowCaster,
                    ))
                    .id(),
            );
        }
    }

    // Werkzeuge: drehbare Köpfe.
    for (i, t) in ship.tools.iter().enumerate() {
        let col = if opts.slot_colors {
            slot_color(t.slot)
        } else {
            srgb([0.5, 0.5, 0.55])
        };
        let ring_mat = mats.add(art.emissive_mat(col, if opts.slot_colors { 1.2 } else { 0.0 }));
        let head = commands
            .spawn((
                Transform::from_xyz(t.pos.x, t.pos.y, 0.7),
                Visibility::default(),
                ToolHead(i),
            ))
            .id();
        let mut parts = vec![
            commands
                .spawn((
                    Mesh3d(art.cylinder.clone()),
                    MeshMaterial3d(ring_mat),
                    Transform::from_xyz(0.0, 0.0, -0.02)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::new(0.22, 0.05, 0.22)),
                ))
                .id(),
        ];
        match t.kind {
            ToolKind::Cannon => {
                let barrel = art.bevel_box(meshes, Vec3::new(0.18, 1.0, 0.18));
                parts.push(
                    commands
                        .spawn((
                            Mesh3d(barrel),
                            MeshMaterial3d(dark_mat.clone()),
                            Transform::from_xyz(0.0, 0.5, 0.1),
                        ))
                        .id(),
                );
                let body = art.bevel_box(meshes, Vec3::new(0.42, 0.42, 0.3));
                parts.push(
                    commands
                        .spawn((
                            Mesh3d(body),
                            MeshMaterial3d(hull_mat.clone()),
                            Transform::from_xyz(0.0, 0.0, 0.1),
                        ))
                        .id(),
                );
            }
            ToolKind::Crane => {
                let arm = art.bevel_box(meshes, Vec3::new(0.16, 0.8, 0.16));
                parts.push(
                    commands
                        .spawn((
                            Mesh3d(arm),
                            MeshMaterial3d(accent_mat.clone()),
                            Transform::from_xyz(0.0, 0.4, 0.1),
                        ))
                        .id(),
                );
                let jaw = art.bevel_box(meshes, Vec3::new(0.1, 0.3, 0.12));
                for x in [-0.14f32, 0.14] {
                    parts.push(
                        commands
                            .spawn((
                                Mesh3d(jaw.clone()),
                                MeshMaterial3d(dark_mat.clone()),
                                Transform::from_xyz(x, 0.85, 0.1),
                            ))
                            .id(),
                    );
                }
            }
            ToolKind::Drill => {
                parts.push(
                    commands
                        .spawn((
                            Mesh3d(art.cone.clone()),
                            MeshMaterial3d(dark_mat.clone()),
                            Transform::from_xyz(0.0, 0.45, 0.1)
                                .with_scale(Vec3::new(0.2, 0.7, 0.2)),
                        ))
                        .id(),
                );
                let body = art.bevel_box(meshes, Vec3::new(0.36, 0.3, 0.3));
                parts.push(
                    commands
                        .spawn((
                            Mesh3d(body),
                            MeshMaterial3d(hull_mat.clone()),
                            Transform::from_xyz(0.0, 0.0, 0.1),
                        ))
                        .id(),
                );
            }
        }
        commands.entity(head).add_children(&parts);
        children.push(head);
    }

    // Positionslichter: rot links, grün rechts, weißer Blitz oben.
    if !opts.derelict {
        let (minx, maxx, maxy) =
            ship.parts
                .iter()
                .fold((0.0f32, 0.0f32, 0.0f32), |(a, b, c), p| {
                    (
                        a.min(p.pos.x - p.half.x),
                        b.max(p.pos.x + p.half.x),
                        c.max(p.pos.y + p.half.y),
                    )
                });
        for (x, col, phase) in [
            (minx, [1.0, 0.1, 0.1], 0.0f32),
            (maxx, [0.1, 1.0, 0.3], 0.0),
            (0.0, [1.0, 1.0, 1.0], 0.5),
        ] {
            let y = if x == 0.0 { maxy + 0.1 } else { 0.4 };
            let bulb = mats.add(art.emissive_mat(srgb(col), 12.0));
            let halo = art.glow_mat(mats, srgb(col), 2.0);
            children.push(
                commands
                    .spawn((
                        Transform::from_xyz(x, y, 0.75),
                        Visibility::default(),
                        NavLight {
                            phase,
                            strobe: x == 0.0,
                        },
                    ))
                    .with_children(|c| {
                        c.spawn((
                            Mesh3d(art.sphere.clone()),
                            MeshMaterial3d(bulb),
                            Transform::from_scale(Vec3::splat(0.09)),
                        ));
                        c.spawn((
                            Mesh3d(art.quad.clone()),
                            MeshMaterial3d(halo),
                            Transform::from_xyz(0.0, 0.0, 0.1).with_scale(Vec3::splat(0.55)),
                            NotShadowCaster,
                        ));
                    })
                    .id(),
            );
        }
    }

    if opts.slot_colors {
        children.push(
            commands
                .spawn((
                    PointLight {
                        color: Color::WHITE,
                        intensity: 0.0,
                        range: 22.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, -2.2, 1.2),
                    EngineLight,
                ))
                .id(),
        );
        let shield_mat = mats.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.9, 1.0, 0.0),
            base_color_texture: Some(art.ring.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        });
        let r = ship.bound_radius() * 1.25;
        children.push(
            commands
                .spawn((
                    Mesh3d(art.quad.clone()),
                    MeshMaterial3d(shield_mat),
                    Transform::from_xyz(ship.com.x, ship.com.y, 1.2)
                        .with_scale(Vec3::splat(r * 2.0)),
                    ShieldVis,
                    NotShadowCaster,
                ))
                .id(),
        );
    }
    if opts.derelict {
        let bulb = mats.add(art.emissive_mat(srgb([1.0, 0.15, 0.1]), 10.0));
        children.push(
            commands
                .spawn((
                    Mesh3d(art.sphere.clone()),
                    MeshMaterial3d(bulb),
                    Transform::from_xyz(0.0, 0.6, 1.1).with_scale(Vec3::splat(0.3)),
                ))
                .id(),
        );
        children.push(
            commands
                .spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.2, 0.1),
                        intensity: 200_000.0,
                        range: 18.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.6, 2.0),
                    BlinkLight(200_000.0),
                ))
                .id(),
        );
    }
    commands.entity(root).add_children(&children);
    root
}

#[allow(clippy::too_many_arguments)]
pub fn sync_ship(
    mut commands: Commands,
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    data: Res<Data>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut ship_q: Query<(Entity, &ShipVis, &mut Transform, &mut Visibility)>,
    mut flames: Query<(&FlameVis, &mut Transform), Without<ShipVis>>,
    mut lights: Query<&mut PointLight, With<EngineLight>>,
) {
    let ship = &sim.0.ship;
    let (hull, accent, flame) = livery_colors(&sim.0);
    let sig = format!("{}|{hull:?}{accent:?}{flame:?}", ship_signature(ship));
    let existing = ship_q.iter().next().map(|(e, s, _, _)| (e, s.sig.clone()));
    if existing.as_ref().map(|(_, s)| s != &sig).unwrap_or(true) {
        if let Some((e, _)) = existing {
            commands.entity(e).despawn();
        }
        let def = data.0.ship(&ship.def_id).clone();
        let e = spawn_ship_model(
            &mut commands,
            &mut art,
            &mut meshes,
            &mut mats,
            ship,
            &def,
            ShipModelOpts {
                slot_colors: true,
                derelict: false,
                hull,
                accent,
                flame,
            },
        );
        commands.entity(e).insert(ShipVis { sig });
        return;
    }
    let alpha = fixed.overstep_fraction();
    let (origin, angle) = ship_pose(&sim.0, alpha);
    for (_, _, mut t, mut vis) in &mut ship_q {
        t.translation = origin.extend(0.0);
        t.rotation = Quat::from_rotation_z(angle);
        *vis = if ship.destroyed {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    let tsec = time.elapsed_secs();
    let mut total = 0.0;
    for (f, mut t) in &mut flames {
        let Some(th) = ship.thrusters.get(f.thruster) else {
            continue;
        };
        let flicker =
            0.85 + 0.15 * ((tsec * 37.0 + f.thruster as f32 * 1.7).sin() * (tsec * 23.0).cos());
        let lvl = th.level;
        if f.len < 0.0 {
            t.scale = Vec3::splat((f.radius * lvl * flicker).max(0.001));
        } else {
            let len = (f.len * lvl * flicker).max(0.001);
            // Kegel: Basis an der Düse, Spitze nach hinten.
            t.scale = Vec3::new(
                f.radius * (0.6 + 0.4 * lvl),
                len,
                f.radius * (0.6 + 0.4 * lvl),
            );
            let base_y = t.translation.y;
            let _ = base_y;
        }
        if !f.core && f.len > 0.0 {
            total += lvl;
        }
    }
    // Kegel-Mitte so verschieben, dass die Basis an der Düse bleibt.
    for (f, mut t) in &mut flames {
        if f.len < 0.0 {
            continue;
        }
        let Some(th) = ship.thrusters.get(f.thruster) else {
            continue;
        };
        let half = ship
            .parts
            .iter()
            .find(|p| matches!(p.kind, PartKind::Thruster(_)) && (p.pos - th.pos).length() < 1e-3)
            .map(|p| p.half.y)
            .unwrap_or(0.2);
        let tip_y = th.pos.y - half - 0.18 - 0.25;
        t.translation.y = tip_y - t.scale.y * 0.5;
    }
    let mut col = Vec3::ZERO;
    for th in &ship.thrusters {
        let c = slot_color(th.slot).to_linear();
        col += Vec3::new(c.red, c.green, c.blue) * th.level;
    }
    for mut l in &mut lights {
        l.intensity = total * 90_000.0;
        if total > 0.01 {
            let c = col / total;
            l.color = Color::LinearRgba(LinearRgba::rgb(c.x, c.y, c.z));
        }
    }
}

/// Der sichtbare Körper (taumelt in 3D), Kind der Wurzel mit Position.
#[derive(Component)]
pub struct BodyMesh(pub u32);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync_bodies(
    mut commands: Commands,
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    data: Res<Data>,
    mut maps: ResMut<VisMaps>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut roots: Query<(&BodyVis, &mut Transform), Without<BodyMesh>>,
    mut inner: Query<(&BodyMesh, &mut Transform), Without<BodyVis>>,
    mut blinks: Query<(&BlinkLight, &mut PointLight)>,
) {
    let alpha = fixed.overstep_fraction();
    // Entfernen, was es nicht mehr gibt.
    let alive: std::collections::HashSet<u32> = sim
        .0
        .bodies
        .iter()
        .filter(|b| b.alive)
        .map(|b| b.id)
        .collect();
    maps.bodies.retain(|id, e| {
        if alive.contains(id) {
            true
        } else {
            commands.entity(*e).despawn();
            false
        }
    });
    for b in &sim.0.bodies {
        if !b.alive || maps.bodies.contains_key(&b.id) {
            continue;
        }
        let root = commands
            .spawn((
                Transform::from_translation(b.pos.extend(0.0)),
                Visibility::default(),
                BodyVis(b.id),
            ))
            .id();
        let mut kids: Vec<Entity> = Vec::new();
        match &b.kind {
            BodyKind::Asteroid { ore, field, .. } => {
                let key = (*field, ore.is_some());
                let mat = if let Some(m) = maps.rock_mats.get(&key) {
                    m.clone()
                } else {
                    let f = &data.0.world.asteroid_fields[*field];
                    let mut m = StandardMaterial {
                        base_color: srgb(hex(&f.color)),
                        base_color_texture: Some(art.rock.clone()),
                        normal_map_texture: Some(art.rock_n.clone()),
                        perceptual_roughness: 0.92,
                        ..default()
                    };
                    if let Some(o) = ore {
                        let c = srgb(o.color()).to_linear();
                        m.emissive = c * 5.0;
                        m.emissive_texture = Some(art.veins.clone());
                    }
                    let h = mats.add(m);
                    maps.rock_mats.insert(key, h.clone());
                    h
                };
                let mesh = art.asteroids[(b.seed as usize) % art.asteroids.len()].clone();
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            Transform::from_scale(Vec3::splat(b.radius)),
                            BodyMesh(b.id),
                        ))
                        .id(),
                );
            }
            BodyKind::Meteor => {
                let mat = mats.add(StandardMaterial {
                    base_color: Color::srgb(0.25, 0.12, 0.08),
                    emissive: LinearRgba::rgb(9.0, 2.6, 0.5),
                    emissive_texture: Some(art.veins.clone()),
                    perceptual_roughness: 0.9,
                    ..default()
                });
                let glow = art.glow_mat(&mut mats, Color::srgb(1.0, 0.45, 0.12), 2.5);
                let mesh = art.asteroids[(b.seed as usize) % art.asteroids.len()].clone();
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            Transform::from_scale(Vec3::splat(b.radius)),
                            BodyMesh(b.id),
                        ))
                        .id(),
                );
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(art.quad.clone()),
                            MeshMaterial3d(glow),
                            Transform::from_xyz(0.0, 0.0, 1.0)
                                .with_scale(Vec3::splat(b.radius * 4.5)),
                            NotShadowCaster,
                        ))
                        .id(),
                );
            }
            BodyKind::OreChunk { ore, .. } => {
                let c = srgb(ore.color());
                let mat = mats.add(StandardMaterial {
                    base_color: c,
                    emissive: c.to_linear() * 7.0,
                    perceptual_roughness: 0.2,
                    ..default()
                });
                let glow = art.glow_mat(&mut mats, c, 1.5);
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(art.crystal.clone()),
                            MeshMaterial3d(mat),
                            Transform::from_scale(Vec3::splat(b.radius * 1.6)),
                            BodyMesh(b.id),
                        ))
                        .id(),
                );
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(art.quad.clone()),
                            MeshMaterial3d(glow),
                            Transform::from_xyz(0.0, 0.0, 0.8)
                                .with_scale(Vec3::splat(b.radius * 4.0)),
                            NotShadowCaster,
                        ))
                        .id(),
                );
            }
            BodyKind::Capsule { .. } => {
                let shell = mats.add(art.panel_mat(Color::srgb(0.95, 0.95, 0.98), 0.35, 0.2));
                let bulb = mats.add(art.emissive_mat(Color::srgb(1.0, 0.2, 0.15), 9.0));
                let glow = art.glow_mat(&mut mats, Color::srgb(1.0, 0.25, 0.2), 2.0);
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(art.capsule.clone()),
                            MeshMaterial3d(shell),
                            Transform::from_scale(Vec3::splat(b.radius)),
                            BodyMesh(b.id),
                        ))
                        .id(),
                );
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(art.sphere.clone()),
                            MeshMaterial3d(bulb),
                            Transform::from_xyz(0.0, 0.0, b.radius * 0.6)
                                .with_scale(Vec3::splat(0.18)),
                        ))
                        .id(),
                );
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(art.quad.clone()),
                            MeshMaterial3d(glow),
                            Transform::from_xyz(0.0, 0.0, 0.9).with_scale(Vec3::splat(3.0)),
                            NotShadowCaster,
                        ))
                        .id(),
                );
            }
            BodyKind::Crate { .. } => {
                use super::world_vis::CONTAINER_COLORS;
                let c = CONTAINER_COLORS[(b.seed as usize) % CONTAINER_COLORS.len()];
                let shell = mats.add(art.panel_mat(srgb(c), 0.6, 0.3));
                let frame = mats.add(art.panel_mat(Color::srgb(0.14, 0.15, 0.17), 0.45, 0.6));
                let band = mats.add(StandardMaterial {
                    base_color: Color::srgb(0.95, 0.72, 0.12),
                    base_color_texture: Some(art.stripes.clone()),
                    perceptual_roughness: 0.6,
                    ..default()
                });
                let bulb = mats.add(art.emissive_mat(Color::srgb(1.0, 0.65, 0.15), 8.0));
                let body = art.prism(
                    &mut meshes,
                    &crate::sim::data::PartShape::Chamfer(0.12).outline(Vec2::new(1.5, 0.95)),
                    1.9,
                    0.08,
                    Some((0.1, 0.05)),
                );
                let rail = art.bevel_box(&mut meshes, Vec3::new(3.1, 0.16, 2.0));
                let stripe = art.bevel_box(&mut meshes, Vec3::new(0.5, 1.92, 1.95));
                let eye = art.bevel_box(&mut meshes, Vec3::new(0.4, 0.3, 0.3));
                let holder = commands
                    .spawn((Transform::default(), Visibility::default(), BodyMesh(b.id)))
                    .with_children(|h| {
                        h.spawn((Mesh3d(body), MeshMaterial3d(shell)));
                        for y in [-0.9f32, 0.9] {
                            h.spawn((
                                Mesh3d(rail.clone()),
                                MeshMaterial3d(frame.clone()),
                                Transform::from_xyz(0.0, y, 0.0),
                            ));
                        }
                        for x in [-1.15f32, 1.15] {
                            h.spawn((
                                Mesh3d(stripe.clone()),
                                MeshMaterial3d(band.clone()),
                                Transform::from_xyz(x, 0.0, 0.0),
                            ));
                        }
                        h.spawn((
                            Mesh3d(eye),
                            MeshMaterial3d(frame.clone()),
                            Transform::from_xyz(0.0, 1.1, 0.0),
                        ));
                        h.spawn((
                            Mesh3d(art.sphere.clone()),
                            MeshMaterial3d(bulb),
                            Transform::from_xyz(1.2, 1.0, 0.8).with_scale(Vec3::splat(0.14)),
                        ));
                        h.spawn((
                            PointLight {
                                color: Color::srgb(1.0, 0.6, 0.15),
                                intensity: 120_000.0,
                                range: 10.0,
                                shadow_maps_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(1.2, 1.2, 1.5),
                            BlinkLight(120_000.0),
                        ));
                    })
                    .id();
                kids.push(holder);
            }
            BodyKind::Wreck { idx, .. } => {
                let w = &data.0.world.wrecks[*idx];
                let def = data.0.ship(&w.ship).clone();
                let ship = Ship::build(&def, &Loadout::full(&def), &ShipStats::default());
                let model = spawn_ship_model(
                    &mut commands,
                    &mut art,
                    &mut meshes,
                    &mut mats,
                    &ship,
                    &def,
                    ShipModelOpts {
                        derelict: true,
                        ..default()
                    },
                );
                let k = crate::sim::hazards::WRECK_SCALE;
                commands.entity(model).insert(
                    Transform::from_translation((-ship.com * k).extend(0.0))
                        .with_scale(Vec3::splat(k)),
                );
                // Glimmende Bruchstellen, solange noch Schrott drin ist.
                let ember = mats.add(art.emissive_mat(Color::srgb(1.0, 0.45, 0.15), 3.0));
                let crack = art.bevel_box(&mut meshes, Vec3::new(0.5, 0.12, 0.2));
                let holder = commands
                    .spawn((Transform::default(), Visibility::default(), BodyMesh(b.id)))
                    .with_children(|h| {
                        for k in 0..4 {
                            let a = k as f32 * 1.7 + (b.seed % 7) as f32;
                            let r = b.radius * 0.45;
                            h.spawn((
                                Mesh3d(crack.clone()),
                                MeshMaterial3d(ember.clone()),
                                Transform::from_xyz(a.cos() * r, a.sin() * r, 1.1)
                                    .with_rotation(Quat::from_rotation_z(a * 2.3)),
                                WreckEmber(b.id),
                            ));
                        }
                    })
                    .id();
                commands.entity(holder).add_child(model);
                kids.push(holder);
            }
            BodyKind::Salvage { .. } => {
                let shell = mats.add(art.panel_mat(Color::srgb(0.32, 0.34, 0.37), 0.4, 0.7));
                let band = mats.add(art.panel_mat(Color::srgb(0.85, 0.55, 0.15), 0.5, 0.3));
                let bulb = mats.add(art.emissive_mat(Color::srgb(1.0, 0.6, 0.2), 7.0));
                let body = art.bevel_box(&mut meshes, Vec3::new(1.2, 0.8, 0.8));
                let ring = art.bevel_box(&mut meshes, Vec3::new(0.3, 0.84, 0.84));
                let holder = commands
                    .spawn((Transform::default(), Visibility::default(), BodyMesh(b.id)))
                    .with_children(|h| {
                        h.spawn((Mesh3d(body), MeshMaterial3d(shell)));
                        h.spawn((Mesh3d(ring), MeshMaterial3d(band)));
                        h.spawn((
                            Mesh3d(art.sphere.clone()),
                            MeshMaterial3d(bulb),
                            Transform::from_xyz(0.5, 0.0, 0.42).with_scale(Vec3::splat(0.1)),
                        ));
                    })
                    .id();
                kids.push(holder);
            }
            BodyKind::Derelict { .. } => {
                let def = data.0.ship("kolibri").clone();
                let ship = Ship::build(&def, &Loadout::full(&def), &ShipStats::default());
                let model = spawn_ship_model(
                    &mut commands,
                    &mut art,
                    &mut meshes,
                    &mut mats,
                    &ship,
                    &def,
                    ShipModelOpts {
                        slot_colors: false,
                        derelict: true,
                        ..default()
                    },
                );
                // Modell so versetzen, dass es um den Schwerpunkt dreht.
                commands.entity(model).insert(
                    Transform::from_translation((-ship.com * 1.3).extend(0.0))
                        .with_scale(Vec3::splat(1.3)),
                );
                let holder = commands
                    .spawn((Transform::default(), Visibility::default(), BodyMesh(b.id)))
                    .id();
                commands.entity(holder).add_child(model);
                kids.push(holder);
            }
        }
        commands.entity(root).add_children(&kids);
        maps.bodies.insert(b.id, root);
    }
    // Positionen interpolieren.
    let by_id: HashMap<u32, &crate::sim::Body> = sim.0.bodies.iter().map(|b| (b.id, b)).collect();
    for (bv, mut t) in &mut roots {
        let Some(b) = by_id.get(&bv.0) else { continue };
        t.translation = b.prev_pos.lerp(b.pos, alpha).extend(0.0);
    }
    for (bm, mut t) in &mut inner {
        let Some(b) = by_id.get(&bm.0) else { continue };
        let a = b.prev_angle + angle_diff(b.angle, b.prev_angle) * alpha;
        let tilt = (b.seed % 1000) as f32 / 1000.0 * std::f32::consts::TAU;
        t.rotation = match b.kind {
            BodyKind::Asteroid { .. } | BodyKind::Meteor => {
                Quat::from_rotation_z(a)
                    * Quat::from_rotation_x(tilt + a * 0.6)
                    * Quat::from_rotation_y(tilt * 0.5)
            }
            BodyKind::OreChunk { .. } => Quat::from_rotation_z(a) * Quat::from_rotation_y(a * 1.3),
            _ => Quat::from_rotation_z(a),
        };
    }
    let tsec = time.elapsed_secs();
    for (b, mut l) in &mut blinks {
        l.intensity = if (tsec * 2.0).fract() < 0.35 {
            b.0
        } else {
            0.0
        };
    }
}

pub fn sync_projectiles(
    mut commands: Commands,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    mut maps: ResMut<VisMaps>,
    mut art: ResMut<Art>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&ProjVis, &mut Transform)>,
) {
    let alpha = fixed.overstep_fraction();
    let alive: std::collections::HashSet<u32> = sim
        .0
        .projectiles
        .iter()
        .filter(|p| p.life > 0.0)
        .map(|p| p.id)
        .collect();
    maps.projectiles.retain(|id, e| {
        if alive.contains(id) {
            true
        } else {
            commands.entity(*e).despawn();
            false
        }
    });
    for p in &sim.0.projectiles {
        if p.life <= 0.0 || maps.projectiles.contains_key(&p.id) {
            continue;
        }
        let bolt = mats.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::rgb(14.0, 10.0, 4.0)),
            unlit: true,
            ..default()
        });
        let glow = art.glow_mat(&mut mats, Color::srgb(1.0, 0.8, 0.3), 3.0);
        let e = commands
            .spawn((
                Mesh3d(art.capsule.clone()),
                MeshMaterial3d(bolt),
                Transform::default(),
                ProjVis(p.id),
                NotShadowCaster,
            ))
            .with_children(|c| {
                c.spawn((
                    Mesh3d(art.quad.clone()),
                    MeshMaterial3d(glow),
                    Transform::from_xyz(0.0, 0.0, 0.3).with_scale(Vec3::new(10.0, 2.5, 1.0)),
                    NotShadowCaster,
                ));
            })
            .id();
        maps.projectiles.insert(p.id, e);
    }
    let by_id: HashMap<u32, &crate::sim::Projectile> =
        sim.0.projectiles.iter().map(|p| (p.id, p)).collect();
    for (pv, mut t) in &mut q {
        let Some(p) = by_id.get(&pv.0) else { continue };
        let pos = p.prev_pos.lerp(p.pos, alpha);
        let dir = p.vel.normalize_or_zero();
        t.translation = pos.extend(0.3);
        t.rotation = Quat::from_rotation_arc(Vec3::Y, dir.extend(0.0).normalize_or(Vec3::Y));
        t.scale = Vec3::new(0.22, 0.7, 0.22);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync_tools(
    mut commands: Commands,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    art: ResMut<Art>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut heads: Query<
        (&ToolHead, &mut Transform),
        (Without<RopeVis>, Without<ClawVis>, Without<DrillBeam>),
    >,
    mut ropes: Query<
        (Entity, &RopeVis, &mut Transform, &mut Visibility),
        (Without<ToolHead>, Without<ClawVis>, Without<DrillBeam>),
    >,
    mut claws: Query<
        (Entity, &ClawVis, &mut Transform, &mut Visibility),
        (Without<ToolHead>, Without<RopeVis>, Without<DrillBeam>),
    >,
    mut beams: Query<
        (Entity, &DrillBeam, &mut Transform, &mut Visibility),
        (Without<ToolHead>, Without<RopeVis>, Without<ClawVis>),
    >,
) {
    let ship = &sim.0.ship;
    let alpha = fixed.overstep_fraction();
    let (origin, angle) = ship_pose(&sim.0, alpha);
    for (h, mut t) in &mut heads {
        if let Some(tool) = ship.tools.get(h.0) {
            t.rotation = Quat::from_rotation_z(tool.aim - angle - std::f32::consts::FRAC_PI_2);
        }
    }
    let n = ship.tools.len();
    // Fehlende Effekt-Objekte anlegen.
    let have_rope: Vec<usize> = ropes.iter().map(|r| r.1.0).collect();
    for i in 0..n {
        if ship.tools[i].kind == ToolKind::Crane && !have_rope.contains(&i) {
            let col = slot_color(ship.tools[i].slot);
            // Dunkles Stahlseil mit einem Hauch Slotfarbe.
            let rope_mat = mats.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.19, 0.21),
                emissive: col.to_linear() * 0.9,
                perceptual_roughness: 0.4,
                metallic: 0.7,
                ..default()
            });
            for seg in 0..ROPE_SEGS {
                commands.spawn((
                    Mesh3d(art.cylinder.clone()),
                    MeshMaterial3d(rope_mat.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    RopeVis(i, seg),
                    NotShadowCaster,
                ));
            }
            let claw = mats.add(art.panel_mat(Color::srgb(0.2, 0.2, 0.24), 0.3, 0.6));
            commands.spawn((
                Mesh3d(art.cube.clone()),
                MeshMaterial3d(claw),
                Transform::default(),
                Visibility::Hidden,
                ClawVis(i),
            ));
        }
    }
    let have_beam: Vec<usize> = beams.iter().map(|b| b.1.0).collect();
    for i in 0..n {
        if ship.tools[i].kind == ToolKind::Drill && !have_beam.contains(&i) {
            let beam_mat = mats.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::rgb(6.0, 7.0, 8.0)),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                ..default()
            });
            commands.spawn((
                Mesh3d(art.cylinder.clone()),
                MeshMaterial3d(beam_mat),
                Transform::default(),
                Visibility::Hidden,
                DrillBeam(i),
                NotShadowCaster,
            ));
        }
    }
    let mount_world = |i: usize| origin + rot(ship.tools[i].pos, angle);
    let segment = |a: Vec2, b: Vec2, thick: f32, z: f32| -> Transform {
        let d = b - a;
        let len = d.length().max(0.01);
        Transform::from_translation(((a + b) * 0.5).extend(z))
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, (d / len).extend(0.0)))
            .with_scale(Vec3::new(thick, len, thick))
    };
    for (e, r, mut t, mut vis) in &mut ropes {
        let Some(tool) = ship.tools.get(r.0).filter(|t| t.kind == ToolKind::Crane) else {
            commands.entity(e).despawn();
            continue;
        };
        let mount = mount_world(r.0);
        let tip = match &tool.crane {
            CraneState::Idle => None,
            CraneState::Extending { len, dir } | CraneState::Retracting { len, dir } => {
                Some((mount + *dir * *len, None))
            }
            CraneState::Attached { body, rope } => sim
                .0
                .bodies
                .iter()
                .find(|b| b.id == *body)
                .map(|b| (b.prev_pos.lerp(b.pos, alpha), Some(*rope))),
        };
        match tip {
            Some((tip, len)) if !ship.destroyed => {
                let pts = rope_points(mount, tip, len);
                *t = segment(pts[r.1], pts[r.1 + 1], 0.08, 0.7);
                *vis = Visibility::Visible;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    for (e, c, mut t, mut vis) in &mut claws {
        let Some(tool) = ship.tools.get(c.0).filter(|t| t.kind == ToolKind::Crane) else {
            commands.entity(e).despawn();
            continue;
        };
        let mount = mount_world(c.0);
        let tip = match &tool.crane {
            CraneState::Extending { len, dir } | CraneState::Retracting { len, dir } => {
                Some((mount + *dir * *len, *dir))
            }
            _ => None,
        };
        match tip {
            Some((p, d)) if !ship.destroyed => {
                *t = Transform::from_translation(p.extend(0.7))
                    .with_rotation(Quat::from_rotation_z(d.to_angle()))
                    .with_scale(Vec3::new(0.5, 0.5, 0.4));
                *vis = Visibility::Visible;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    for (e, b, mut t, mut vis) in &mut beams {
        let Some(tool) = ship.tools.get(b.0).filter(|t| t.kind == ToolKind::Drill) else {
            commands.entity(e).despawn();
            continue;
        };
        if !tool.pressed || ship.destroyed || ship.docked.is_some() {
            *vis = Visibility::Hidden;
            continue;
        }
        let mount = mount_world(b.0);
        let end = tool
            .drill
            .as_ref()
            .map(|d| d.point)
            .unwrap_or(mount + tool.aim_dir() * 5.5);
        *t = segment(mount, end, 0.06, 0.8);
        *vis = Visibility::Visible;
    }
}

/// Die Rettungskapsel nach einer Zerstörung.
#[derive(Component)]
pub struct PodVis;

pub fn sync_pod(
    mut commands: Commands,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    mut art: ResMut<Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut Transform), With<PodVis>>,
) {
    let Some(pod) = &sim.0.escape else {
        for (e, _) in &q {
            commands.entity(e).despawn();
        }
        return;
    };
    let alpha = fixed.overstep_fraction();
    let pos = pod.prev_pos.lerp(pod.pos, alpha);
    let angle = pod.prev_angle + angle_diff(pod.angle, pod.prev_angle) * alpha;
    let tr =
        Transform::from_translation(pos.extend(0.0)).with_rotation(Quat::from_rotation_z(angle));
    if let Some((_, mut t)) = q.iter_mut().next() {
        *t = tr;
        return;
    }
    let shell = mats.add(art.panel_mat(Color::srgb(0.88, 0.88, 0.86), 0.4, 0.25));
    let band = mats.add(art.panel_mat(Color::srgb(0.85, 0.42, 0.12), 0.5, 0.2));
    let glass = mats.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.08, 0.1),
        perceptual_roughness: 0.08,
        metallic: 0.6,
        ..default()
    });
    let bulb = mats.add(art.emissive_mat(Color::srgb(1.0, 0.2, 0.15), 9.0));
    let flame = art.glow_mat(&mut mats, Color::srgb(1.0, 0.6, 0.25), 2.0);
    let ring = art.bevel_box(&mut meshes, Vec3::new(1.25, 0.22, 1.25));
    commands
        .spawn((tr, Visibility::default(), PodVis))
        .with_children(|c| {
            c.spawn((
                Mesh3d(art.capsule.clone()),
                MeshMaterial3d(shell),
                Transform::from_scale(Vec3::new(1.1, 1.0, 1.1)),
            ));
            c.spawn((
                Mesh3d(ring),
                MeshMaterial3d(band),
                Transform::from_xyz(0.0, -0.1, 0.0),
            ));
            c.spawn((
                Mesh3d(art.sphere.clone()),
                MeshMaterial3d(glass),
                Transform::from_xyz(0.0, 0.45, 0.42).with_scale(Vec3::new(0.55, 0.35, 0.3)),
            ));
            c.spawn((
                Mesh3d(art.sphere.clone()),
                MeshMaterial3d(bulb),
                Transform::from_xyz(0.0, 0.95, 0.0).with_scale(Vec3::splat(0.14)),
            ));
            c.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.25, 0.2),
                    intensity: 200_000.0,
                    range: 14.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 1.0, 1.5),
                BlinkLight(200_000.0),
            ));
            c.spawn((
                Mesh3d(art.quad.clone()),
                MeshMaterial3d(flame),
                Transform::from_xyz(0.0, -1.2, 0.3).with_scale(Vec3::new(0.9, 1.6, 1.0)),
                NotShadowCaster,
            ));
        });
}

/// Bruchstellen glimmen, solange das Wrack noch Schrott hergibt.
pub fn update_wreck_embers(sim: Res<Sim>, mut q: Query<(&WreckEmber, &mut Visibility)>) {
    for (e, mut vis) in &mut q {
        let has_scrap = sim
            .0
            .bodies
            .iter()
            .find(|b| b.id == e.0)
            .is_some_and(|b| matches!(b.kind, BodyKind::Wreck { scrap, .. } if scrap > 0.05));
        let want = if has_scrap {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
}
