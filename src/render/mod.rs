//! Darstellung: 2.5D – 3D-Grafik, Spiel in der XY-Ebene, Kamera schaut entlang -Z.

pub mod course_vis;
pub mod fx;
pub mod meshes;
pub mod npc_vis;
pub mod overlay;
pub mod ship_vis;
pub mod story_vis;
pub mod textures;
pub mod world_vis;

pub use ship_vis::ShipModelOpts;

use std::collections::HashMap;

use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::light::{GlobalAmbientLight, NotShadowCaster};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::game::{AppState, GameCamera, Sim};
use crate::sim::geom::rot;
use crate::sim::world::angle_diff;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.004, 0.004, 0.012)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.65, 0.72, 1.0),
                brightness: 420.0,
                affects_lightmapped_meshes: true,
            })
            .init_resource::<CameraRig>()
            .init_resource::<ship_vis::VisMaps>()
            .init_resource::<npc_vis::NpcMap>()
            .add_systems(PreStartup, setup_art)
            .add_systems(
                Startup,
                (
                    setup_camera,
                    world_vis::spawn_world,
                    course_vis::spawn_course_vis,
                    world_vis::spawn_background,
                    world_vis::spawn_dust,
                    npc_vis::spawn_traffic_props,
                    story_vis::spawn_monuments,
                ),
            )
            .add_systems(
                Update,
                (
                    camera_follow,
                    world_vis::update_stars,
                    world_vis::update_dust,
                    world_vis::update_dynamic_world,
                    ship_vis::sync_ship,
                    npc_vis::sync_npcs,
                    ship_vis::sync_bodies,
                    ship_vis::sync_projectiles,
                    ship_vis::sync_tools,
                    ship_vis::sync_pod,
                    ship_vis::update_wreck_embers,
                    ship_vis::blink_nav_lights,
                    world_vis::update_pad_lights,
                    world_vis::update_stage_vis,
                    story_vis::update_monuments,
                    course_vis::update_gate_caps,
                )
                    .chain(),
            )
            .add_plugins((fx::FxPlugin, overlay::OverlayPlugin));
    }
}

/// Gemeinsame Texturen, Meshes und Materialien.
#[derive(Resource)]
pub struct Art {
    pub panel: Handle<Image>,
    pub panel_n: Handle<Image>,
    pub window_alb: Handle<Image>,
    pub window_emi: Handle<Image>,
    pub rock: Handle<Image>,
    pub rock_n: Handle<Image>,
    pub veins: Handle<Image>,
    pub glow: Handle<Image>,
    pub ring: Handle<Image>,
    pub stripes: Handle<Image>,
    pub nebulae: Vec<Handle<Image>>,
    pub block: Handle<Mesh>,
    pub quad: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    pub sphere_hi: Handle<Mesh>,
    pub cube: Handle<Mesh>,
    pub crystal: Handle<Mesh>,
    pub asteroids: Vec<Handle<Mesh>>,
    pub cone: Handle<Mesh>,
    pub frustum: Handle<Mesh>,
    pub cylinder: Handle<Mesh>,
    pub capsule: Handle<Mesh>,
    pub box_cache: HashMap<[u32; 3], Handle<Mesh>>,
    pub prism_cache: HashMap<String, Handle<Mesh>>,
    pub glow_cache: HashMap<[u16; 4], Handle<StandardMaterial>>,
}

impl Art {
    /// Abgeschrägtes Prisma aus einem Umriss (zwischengespeichert über einen Schlüssel).
    pub fn prism(
        &mut self,
        meshes: &mut Assets<Mesh>,
        outline: &[Vec2],
        depth: f32,
        bevel: f32,
        panel: Option<(f32, f32)>,
    ) -> Handle<Mesh> {
        let mut key = format!("{depth:.2}|{bevel:.2}|{panel:?}");
        for p in outline {
            key.push_str(&format!("|{:.2},{:.2}", p.x, p.y));
        }
        self.prism_cache
            .entry(key)
            .or_insert_with(|| meshes.add(meshes::beveled_prism(outline, depth, bevel, panel)))
            .clone()
    }

    /// Abgeschrägte Box in passender Größe (zwischengespeichert).
    pub fn bevel_box(&mut self, meshes: &mut Assets<Mesh>, size: Vec3) -> Handle<Mesh> {
        let key = [
            (size.x * 100.0) as u32,
            (size.y * 100.0) as u32,
            (size.z * 100.0) as u32,
        ];
        self.box_cache
            .entry(key)
            .or_insert_with(|| {
                let bevel = (size.min_element() * 0.16).min(0.22);
                meshes.add(meshes::beveled_block(size, bevel, 0.0, 0.0))
            })
            .clone()
    }

    /// Additives, unbeleuchtetes Leuchtmaterial (HDR-Farbe), zwischengespeichert.
    pub fn glow_mat(
        &mut self,
        mats: &mut Assets<StandardMaterial>,
        color: Color,
        intensity: f32,
    ) -> Handle<StandardMaterial> {
        let c = color.to_linear();
        let q = |x: f32| (x * 12.0).round().clamp(0.0, 4000.0) as u16;
        let key = [q(c.red), q(c.green), q(c.blue), q(intensity)];
        let tex = self.glow.clone();
        self.glow_cache
            .entry(key)
            .or_insert_with(|| {
                mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(LinearRgba::rgb(
                        c.red * intensity,
                        c.green * intensity,
                        c.blue * intensity,
                    )),
                    base_color_texture: Some(tex),
                    unlit: true,
                    alpha_mode: AlphaMode::Add,
                    fog_enabled: false,
                    ..default()
                })
            })
            .clone()
    }

    pub fn panel_mat(&self, color: Color, rough: f32, metal: f32) -> StandardMaterial {
        StandardMaterial {
            base_color: color,
            base_color_texture: Some(self.panel.clone()),
            normal_map_texture: Some(self.panel_n.clone()),
            perceptual_roughness: rough,
            metallic: metal,
            reflectance: 0.45,
            ..default()
        }
    }

    pub fn emissive_mat(&self, color: Color, strength: f32) -> StandardMaterial {
        let c = color.to_linear();
        StandardMaterial {
            base_color: color,
            emissive: LinearRgba::rgb(c.red * strength, c.green * strength, c.blue * strength),
            perceptual_roughness: 0.4,
            ..default()
        }
    }
}

pub fn srgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

/// Kräftige Slot-Farben (eine pro Slot), damit man sieht, wer gerade schiebt.
pub const SLOT_COLORS: [[f32; 3]; 12] = [
    [1.0, 0.24, 0.32],
    [0.18, 0.86, 1.0],
    [1.0, 0.82, 0.12],
    [0.46, 1.0, 0.3],
    [0.85, 0.36, 1.0],
    [1.0, 0.55, 0.12],
    [0.2, 1.0, 0.75],
    [1.0, 0.42, 0.75],
    [0.4, 0.55, 1.0],
    [0.95, 1.0, 0.4],
    [1.0, 1.0, 1.0],
    [0.6, 1.0, 1.0],
];

pub fn slot_color(slot: u8) -> Color {
    srgb(SLOT_COLORS[slot as usize % SLOT_COLORS.len()])
}

fn setup_art(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Texturen parallel erzeugen, damit der Start flott bleibt.
    let (panel, window, rock, veins, nebulae) = std::thread::scope(|s| {
        let p = s.spawn(|| textures::panels(512, 2, 11));
        let w = s.spawn(|| textures::windows(256, 5));
        let r = s.spawn(|| textures::rock(256, 3));
        let v = s.spawn(|| textures::veins(256, 9));
        let n: Vec<_> = (0..3u32)
            .map(|i| s.spawn(move || textures::nebula(384, 40 + i * 7)))
            .collect();
        (
            p.join().unwrap(),
            w.join().unwrap(),
            r.join().unwrap(),
            v.join().unwrap(),
            n.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>(),
        )
    });
    let asteroids = (0..8)
        .map(|i| meshes.add(meshes::asteroid(1000 + i * 17)))
        .collect();
    let art = Art {
        panel: images.add(panel.albedo),
        panel_n: images.add(panel.normal),
        window_alb: images.add(window.0),
        window_emi: images.add(window.1),
        rock: images.add(rock.albedo),
        rock_n: images.add(rock.normal),
        veins: images.add(veins),
        glow: images.add(textures::glow(64)),
        ring: images.add(textures::ring(256, 0.12)),
        stripes: images.add(textures::stripes(64)),
        nebulae: nebulae.into_iter().map(|n| images.add(n)).collect(),
        block: meshes.add(meshes::beveled_block(Vec3::splat(4.0), 0.42, 0.42, 0.28)),
        quad: meshes.add(meshes::quad(Vec2::ONE)),
        sphere: meshes.add(Sphere::new(1.0).mesh().uv(24, 16)),
        sphere_hi: meshes.add(Sphere::new(1.0).mesh().uv(96, 48)),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        crystal: meshes.add(meshes::crystal(0.35, 1.6)),
        asteroids,
        cone: meshes.add(Cone {
            radius: 1.0,
            height: 1.0,
        }),
        frustum: meshes.add(ConicalFrustum {
            radius_top: 0.6,
            radius_bottom: 1.0,
            height: 1.0,
        }),
        cylinder: meshes.add(Cylinder::new(1.0, 1.0)),
        capsule: meshes.add(Capsule3d::new(0.5, 1.0)),
        box_cache: HashMap::new(),
        prism_cache: HashMap::new(),
        glow_cache: HashMap::new(),
    };
    commands.insert_resource(art);
}

/// Kamera-Steuerung: folgt dem Schiff, zoomt mit der Geschwindigkeit.
#[derive(Resource)]
pub struct CameraRig {
    pub zoom: f32,
    pub shake: f32,
    pub pos: Vec3,
    pub t: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        CameraRig {
            zoom: 1.0,
            shake: 0.0,
            pos: Vec3::new(0.0, 0.0, 80.0),
            t: 0.0,
        }
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Hdr,
        Tonemapping::AcesFitted,
        Bloom {
            intensity: 0.22,
            ..Bloom::NATURAL
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 40f32.to_radians(),
            near: 0.5,
            far: 8000.0,
            ..default()
        }),
        Transform::from_xyz(0.0, 0.0, 80.0).looking_to(Vec3::NEG_Z, Vec3::Y),
        GameCamera,
    ));
    // Sonne von oben links vorne.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.95, 0.88),
            illuminance: 9000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-0.55, 0.75, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Schwaches Gegenlicht (farbig) für mehr Tiefe.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.45, 0.35, 1.0),
            illuminance: 1800.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.8, -0.6, 0.4).looking_at(Vec3::ZERO, Vec3::Y),
        NotShadowCaster,
    ));
}

/// Interpolierte Pose des Schiffs (zwischen zwei Simulationsschritten).
pub fn ship_pose(sim: &crate::sim::SimState, alpha: f32) -> (Vec2, f32) {
    let s = &sim.ship;
    let angle = s.prev_angle + angle_diff(s.angle, s.prev_angle) * alpha;
    let com = s.prev_pos.lerp(s.pos, alpha);
    (com - rot(s.com, angle), angle)
}

fn camera_follow(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    sim: Res<Sim>,
    state: Res<State<AppState>>,
    scroll: Res<AccumulatedMouseScroll>,
    map: Res<crate::game::MapOpen>,
    settings: Res<crate::settings::Settings>,
    mut rig: ResMut<CameraRig>,
    mut cam: Query<&mut Transform, With<GameCamera>>,
) {
    let Ok(mut t) = cam.single_mut() else { return };
    let dt = time.delta_secs().min(0.1);
    rig.t += dt;
    if *state.get() == AppState::Playing && !map.0 && scroll.delta.y.abs() > 0.0 {
        rig.zoom = (rig.zoom * (1.0 - scroll.delta.y.signum() * 0.1)).clamp(0.45, 2.6);
    }
    let alpha = fixed.overstep_fraction();
    let (origin, _) = ship_pose(&sim.0, alpha);
    let s = &sim.0.ship;
    let com = s.prev_pos.lerp(s.pos, alpha);
    let target = match state.get() {
        AppState::Title => {
            let a = rig.t * 0.03;
            Vec3::new(
                origin.x + 40.0 * a.cos() - 30.0,
                origin.y + 25.0 * a.sin() + 12.0,
                150.0,
            )
        }
        AppState::Lobby => Vec3::new(com.x + 5.0, com.y + 0.3, 21.0),
        AppState::Playing if s.destroyed && sim.0.escape.is_some() => {
            // Die Kamera folgt der Rettungskapsel.
            let p = sim.0.escape.as_ref().unwrap();
            let pos = p.prev_pos.lerp(p.pos, alpha);
            Vec3::new(pos.x, pos.y, 34.0 * rig.zoom)
        }
        AppState::Playing => {
            let speed = s.vel.length();
            let look = s.vel * 0.55;
            let dist = (48.0 + speed * 1.1).min(140.0) * rig.zoom;
            let dist = if s.docked.is_some() {
                38.0 * rig.zoom
            } else {
                dist
            };
            Vec3::new(com.x + look.x, com.y + look.y, dist)
        }
    };
    let k = 1.0 - (-dt * 4.5).exp();
    let kz = 1.0
        - (-dt
            * if *state.get() == AppState::Playing {
                1.6
            } else {
                4.0
            })
        .exp();
    let mut p = rig.pos;
    p.x += (target.x - p.x) * k;
    p.y += (target.y - p.y) * k;
    p.z += (target.z - p.z) * kz;
    // Bei Sprüngen (Wiedereinstieg, Zustandswechsel) nicht durch die halbe Welt schwenken.
    if (p.truncate() - target.truncate()).length() > 300.0 {
        p = target;
    }
    rig.pos = p;
    rig.shake = (rig.shake - dt * 2.5).max(0.0);
    let sh = if settings.shake {
        rig.shake * rig.shake
    } else {
        0.0
    };
    let jitter = Vec3::new((rig.t * 53.0).sin(), (rig.t * 61.0).cos(), 0.0) * sh * 0.9;
    *t = Transform::from_translation(p + jitter).looking_to(Vec3::NEG_Z, Vec3::Y);
}
