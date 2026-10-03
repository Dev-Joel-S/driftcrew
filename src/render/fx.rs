//! Effekte: Partikel (Abgase, Funken, Explosionen, Bohrstaub), Lichtblitze, Kamerawackeln.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::ship_vis::ShieldVis;
use super::{Art, CameraRig, slot_color, srgb};
use crate::game::{AppState, Sim, SimMsg};
use crate::sim::geom::rot;
use crate::sim::rng::Rng;
use crate::sim::{BodyKind, SimEvent};

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FxRng(Rng::new(4242)))
            .init_resource::<ShieldFlash>()
            .add_systems(
                Update,
                (
                    (react_to_sim_events, emit_continuous).run_if(in_state(AppState::Playing)),
                    update_particles,
                    update_flashes,
                    update_shield,
                ),
            );
    }
}

#[derive(Resource)]
pub struct FxRng(pub Rng);

#[derive(Resource, Default)]
pub struct ShieldFlash(pub f32);

#[derive(Component)]
pub struct Particle {
    pub vel: Vec3,
    pub life: f32,
    pub max_life: f32,
    pub size0: f32,
    pub size1: f32,
    pub drag: f32,
}

#[derive(Component)]
pub struct Flash {
    pub life: f32,
    pub max_life: f32,
    pub intensity: f32,
}

#[derive(Component)]
pub struct Shockwave {
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
}

const MAX_PARTICLES: usize = 2500;

#[allow(clippy::too_many_arguments)]
pub fn spawn_particle(
    commands: &mut Commands,
    art: &mut Art,
    mats: &mut Assets<StandardMaterial>,
    pos: Vec3,
    vel: Vec3,
    color: Color,
    intensity: f32,
    life: f32,
    size: (f32, f32),
    drag: f32,
) {
    let mat = art.glow_mat(mats, color, intensity);
    commands.spawn((
        Mesh3d(art.quad.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(pos).with_scale(Vec3::splat(size.0)),
        Particle {
            vel,
            life,
            max_life: life,
            size0: size.0,
            size1: size.1,
            drag,
        },
        NotShadowCaster,
    ));
}

fn burst(
    commands: &mut Commands,
    art: &mut Art,
    mats: &mut Assets<StandardMaterial>,
    rng: &mut Rng,
    pos: Vec2,
    count: usize,
    speed: (f32, f32),
    color: Color,
    intensity: f32,
    life: (f32, f32),
    size: (f32, f32),
) {
    for _ in 0..count {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let s = rng.range(speed.0, speed.1);
        let v = Vec3::new(a.cos() * s, a.sin() * s, rng.range(-0.3, 0.3) * s);
        let l = rng.range(life.0, life.1);
        spawn_particle(
            commands,
            art,
            mats,
            pos.extend(rng.range(0.5, 1.5)),
            v,
            color,
            intensity,
            l,
            size,
            1.8,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn react_to_sim_events(
    mut commands: Commands,
    mut events: MessageReader<SimMsg>,
    mut art: ResMut<Art>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut rng: ResMut<FxRng>,
    mut rig: ResMut<CameraRig>,
    mut shield: ResMut<ShieldFlash>,
    sim: Res<Sim>,
    particles: Query<(), With<Particle>>,
) {
    let crowded = particles.iter().count() > MAX_PARTICLES;
    let ship_pos = sim.0.ship.pos;
    for SimMsg(e) in events.read() {
        match e {
            SimEvent::Impact {
                pos,
                normal,
                strength,
            } => {
                let n = (*strength * 2.0).min(30.0) as usize;
                if !crowded {
                    for _ in 0..n {
                        let a = normal.to_angle() + rng.0.range(-1.2, 1.2);
                        let s = rng.0.range(3.0, 10.0) * (strength / 6.0).min(2.0);
                        let v = Vec3::new(a.cos() * s, a.sin() * s, rng.0.range(-2.0, 2.0));
                        spawn_particle(
                            &mut commands,
                            &mut art,
                            &mut mats,
                            pos.extend(1.0),
                            v,
                            Color::srgb(1.0, 0.75, 0.35),
                            5.0,
                            rng.0.range(0.2, 0.6),
                            (0.35, 0.05),
                            3.0,
                        );
                    }
                }
                if (*pos - ship_pos).length() < 20.0 {
                    rig.shake = (rig.shake + strength * 0.05).min(1.2);
                }
            }
            SimEvent::Explosion { pos, size, color } => {
                let c = srgb(*color);
                if !crowded {
                    burst(
                        &mut commands,
                        &mut art,
                        &mut mats,
                        &mut rng.0,
                        *pos,
                        (14.0 * size) as usize + 10,
                        (2.0, 8.0 * size.sqrt()),
                        c,
                        4.0,
                        (0.4, 1.1),
                        (size * 0.9, 0.1),
                    );
                    burst(
                        &mut commands,
                        &mut art,
                        &mut mats,
                        &mut rng.0,
                        *pos,
                        (6.0 * size) as usize + 4,
                        (0.5, 2.0),
                        Color::srgb(1.0, 0.95, 0.8),
                        6.0,
                        (0.15, 0.4),
                        (size * 1.6, 0.2),
                    );
                    burst(
                        &mut commands,
                        &mut art,
                        &mut mats,
                        &mut rng.0,
                        *pos,
                        (10.0 * size) as usize,
                        (1.0, 4.0),
                        Color::srgb(0.35, 0.3, 0.3),
                        0.6,
                        (1.0, 2.2),
                        (size * 0.7, size * 1.4),
                    );
                }
                commands.spawn((
                    PointLight {
                        color: c,
                        intensity: 600_000.0 * size,
                        range: 12.0 * size + 10.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(pos.extend(3.0)),
                    Flash {
                        life: 0.45,
                        max_life: 0.45,
                        intensity: 600_000.0 * size,
                    },
                ));
                let ring = mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(c.to_linear() * 3.0),
                    base_color_texture: Some(art.ring.clone()),
                    unlit: true,
                    alpha_mode: AlphaMode::Add,
                    ..default()
                });
                commands.spawn((
                    Mesh3d(art.quad.clone()),
                    MeshMaterial3d(ring),
                    Transform::from_translation(pos.extend(1.5)).with_scale(Vec3::splat(0.1)),
                    Shockwave {
                        life: 0.5,
                        max_life: 0.5,
                        size: size * 5.0,
                    },
                    NotShadowCaster,
                ));
                let d = (*pos - ship_pos).length();
                rig.shake = (rig.shake + size * 0.25 * (1.0 - d / 80.0).max(0.0)).min(1.5);
            }
            SimEvent::Shot { pos, dir } => {
                for _ in 0..8 {
                    let a = dir.to_angle() + rng.0.range(-0.5, 0.5);
                    let s = rng.0.range(4.0, 14.0);
                    let v = Vec3::new(a.cos() * s, a.sin() * s, 0.0);
                    spawn_particle(
                        &mut commands,
                        &mut art,
                        &mut mats,
                        pos.extend(1.0),
                        v,
                        Color::srgb(1.0, 0.85, 0.4),
                        6.0,
                        rng.0.range(0.08, 0.2),
                        (0.5, 0.05),
                        6.0,
                    );
                }
                rig.shake = (rig.shake + 0.12).min(1.0);
            }
            SimEvent::ProjectileHit { pos } => {
                burst(
                    &mut commands,
                    &mut art,
                    &mut mats,
                    &mut rng.0,
                    *pos,
                    10,
                    (2.0, 9.0),
                    Color::srgb(1.0, 0.7, 0.3),
                    5.0,
                    (0.1, 0.35),
                    (0.4, 0.05),
                );
            }
            SimEvent::ShieldHit { .. } => shield.0 = 1.0,
            SimEvent::Docked { .. } => {
                let p = sim.0.ship.pos;
                let ring = mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(LinearRgba::rgb(0.4, 3.0, 1.4)),
                    base_color_texture: Some(art.ring.clone()),
                    unlit: true,
                    alpha_mode: AlphaMode::Add,
                    ..default()
                });
                commands.spawn((
                    Mesh3d(art.quad.clone()),
                    MeshMaterial3d(ring),
                    Transform::from_translation(p.extend(1.5)).with_scale(Vec3::splat(0.1)),
                    Shockwave {
                        life: 0.8,
                        max_life: 0.8,
                        size: 14.0,
                    },
                    NotShadowCaster,
                ));
            }
            SimEvent::Stowed { .. } | SimEvent::CraneAttach { .. } => {
                let p = match e {
                    SimEvent::CraneAttach { pos } => *pos,
                    _ => sim.0.ship.pos,
                };
                burst(
                    &mut commands,
                    &mut art,
                    &mut mats,
                    &mut rng.0,
                    p,
                    12,
                    (1.0, 4.0),
                    Color::srgb(0.4, 1.0, 0.9),
                    4.0,
                    (0.2, 0.5),
                    (0.4, 0.05),
                );
            }
            SimEvent::ShipDestroyed => rig.shake = 1.6,
            _ => {}
        }
    }
}

/// Dauerhafte Emitter: Triebwerksabgas, Bohrstaub, Meteoritenschweife.
#[allow(clippy::too_many_arguments)]
fn emit_continuous(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    paused: Res<crate::game::Paused>,
    mut art: ResMut<Art>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut rng: ResMut<FxRng>,
    particles: Query<(), With<Particle>>,
    mut acc: Local<f32>,
) {
    if paused.0 {
        return;
    }
    if particles.iter().count() > MAX_PARTICLES {
        return;
    }
    // Feste Emissionsrate (60/s), unabhängig von der Bildrate.
    *acc += time.delta_secs();
    let steps = (*acc * 60.0) as u32;
    *acc -= steps as f32 / 60.0;
    let steps = steps.min(3);
    let ship = &sim.0.ship;
    for _ in 0..steps {
        if !ship.destroyed {
            for t in &ship.thrusters {
                if t.level < 0.2 {
                    continue;
                }
                let world_dir = rot(t.dir, ship.angle);
                let nozzle = ship.to_world(t.pos) - world_dir * 1.0;
                let spread = rng.0.range(-0.25, 0.25);
                let v = ship.vel - rot(world_dir, spread) * rng.0.range(14.0, 22.0);
                let c = slot_color(t.slot);
                spawn_particle(
                    &mut commands,
                    &mut art,
                    &mut mats,
                    nozzle.extend(rng.0.range(-0.2, 0.4)),
                    v.extend(0.0),
                    c,
                    3.5 * t.level,
                    rng.0.range(0.25, 0.5),
                    (0.55, 0.05),
                    2.5,
                );
                if rng.0.chance(0.35) {
                    let v2 = ship.vel - world_dir * rng.0.range(3.0, 6.0);
                    spawn_particle(
                        &mut commands,
                        &mut art,
                        &mut mats,
                        nozzle.extend(-0.3),
                        v2.extend(0.0),
                        Color::srgb(0.5, 0.5, 0.6),
                        0.35,
                        rng.0.range(0.8, 1.6),
                        (0.4, 1.6),
                        1.0,
                    );
                }
            }
            for tool in &ship.tools {
                if let Some(d) = &tool.drill {
                    let c = d
                        .ore
                        .map(|o| srgb(o.color()))
                        .unwrap_or(Color::srgb(1.0, 0.7, 0.4));
                    for _ in 0..2 {
                        let a = rng.0.range(0.0, std::f32::consts::TAU);
                        let s = rng.0.range(2.0, 7.0);
                        let v = Vec3::new(a.cos() * s, a.sin() * s, rng.0.range(0.0, 3.0));
                        spawn_particle(
                            &mut commands,
                            &mut art,
                            &mut mats,
                            d.point.extend(0.5),
                            v,
                            c,
                            4.0,
                            rng.0.range(0.2, 0.5),
                            (0.35, 0.05),
                            3.0,
                        );
                    }
                }
            }
        }
        // Sonnenwind: feine Schlieren in Windrichtung rund um das Schiff.
        let wind = sim.0.sector_at(ship.pos).wind;
        for _ in 0..4 {
            if wind.length() <= 0.2 || !rng.0.chance((wind.length() * 0.35).min(0.9)) {
                continue;
            }
            let dir = wind.normalize();
            let side = Vec2::new(-dir.y, dir.x);
            let p = ship.pos - dir * 45.0
                + side * rng.0.range(-40.0, 40.0)
                + dir * rng.0.range(0.0, 50.0);
            spawn_particle(
                &mut commands,
                &mut art,
                &mut mats,
                p.extend(rng.0.range(-2.0, 2.0)),
                (dir * rng.0.range(30.0, 45.0)).extend(0.0),
                Color::srgb(1.0, 0.85, 0.55),
                2.6,
                rng.0.range(0.8, 1.4),
                (0.75, 0.2),
                0.0,
            );
        }
        for b in &sim.0.bodies {
            if !matches!(b.kind, BodyKind::Meteor) || (b.pos - ship.pos).length() > 140.0 {
                continue;
            }
            let back = -b.vel.normalize_or_zero();
            let p = b.pos
                + back * b.radius * 0.8
                + Vec2::new(rng.0.range(-0.4, 0.4), rng.0.range(-0.4, 0.4));
            spawn_particle(
                &mut commands,
                &mut art,
                &mut mats,
                p.extend(0.3),
                (b.vel * 0.3).extend(0.0),
                Color::srgb(1.0, 0.45, 0.15),
                3.0,
                rng.0.range(0.4, 0.8),
                (b.radius * 1.4, 0.1),
                1.0,
            );
        }
    }
}

fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    paused: Res<crate::game::Paused>,
    mut q: Query<(Entity, &mut Particle, &mut Transform)>,
    mut waves: Query<(Entity, &mut Shockwave, &mut Transform), Without<Particle>>,
) {
    if paused.0 {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    for (e, mut p, mut t) in &mut q {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let k = 1.0 - p.life / p.max_life;
        let drag = (1.0 - p.drag * dt).max(0.0);
        p.vel *= drag;
        t.translation += p.vel * dt;
        t.scale = Vec3::splat((p.size0 + (p.size1 - p.size0) * k).max(0.001));
    }
    for (e, mut w, mut t) in &mut waves {
        w.life -= dt;
        if w.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let k = 1.0 - w.life / w.max_life;
        t.scale = Vec3::splat(w.size * (0.2 + k * 1.2) * (1.0 - k * 0.3));
    }
}

fn update_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Flash, &mut PointLight)>,
) {
    let dt = time.delta_secs();
    for (e, mut f, mut l) in &mut q {
        f.life -= dt;
        if f.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let k = f.life / f.max_life;
        l.intensity = f.intensity * k * k;
    }
}

fn update_shield(
    time: Res<Time>,
    sim: Res<Sim>,
    mut flash: ResMut<ShieldFlash>,
    q: Query<&MeshMaterial3d<StandardMaterial>, With<ShieldVis>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    flash.0 = (flash.0 - time.delta_secs() * 2.5).max(0.0);
    let ship = &sim.0.ship;
    let base = if ship.shield > 0.5 && ship.docked.is_none() {
        0.012
    } else {
        0.0
    };
    let a = base + flash.0 * 0.9;
    for m in &q {
        if let Some(mut mat) = mats.get_mut(&m.0) {
            mat.base_color =
                Color::LinearRgba(LinearRgba::rgb(0.3 * a * 3.0, 0.9 * a * 3.0, 1.0 * a * 3.0));
        }
    }
}
