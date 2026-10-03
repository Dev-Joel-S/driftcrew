//! Statische Welt: Stationen aus Rastern, Planeten, Landeplattformen, Rotoren, Anomalien.

use bevy::math::Vec2;

use super::data::{v, GameData, Ore, Service, StationKind};
use super::geom::{rot, Aabb, Quad};
use super::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Station(usize),
    Planet(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellKind {
    Block,
    Accent,
    Window,
    Light,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub kind: CellKind,
    pub center: Vec2,
    pub col: i32,
    pub row: i32,
}

#[derive(Clone, Debug)]
pub struct Pad {
    pub owner: Owner,
    /// Mittelpunkt der Plattform-Oberfläche.
    pub center: Vec2,
    /// Richtung, in die die Plattform zeigt (Schiffsnase beim Andocken).
    pub normal: Vec2,
    pub half_width: f32,
}

impl Pad {
    pub fn tangent(&self) -> Vec2 {
        Vec2::new(self.normal.y, -self.normal.x)
    }
    /// Winkel, den ein Schiff angedockt hat (Nase entlang der Normale).
    pub fn ship_angle(&self) -> f32 {
        f32::atan2(-self.normal.x, self.normal.y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Quad(Quad),
    Circle { c: Vec2, r: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Block,
    Pad(usize),
    Planet(usize),
}

#[derive(Clone, Debug)]
pub struct StaticCollider {
    pub shape: Shape,
    pub aabb: Aabb,
    pub surface: Surface,
}

#[derive(Clone, Debug)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub kind: StationKind,
    pub pos: Vec2,
    pub cell: f32,
    pub cells: Vec<Cell>,
    pub pads: Vec<usize>,
    pub services: Vec<Service>,
    pub bounds: Aabb,
    pub main_color: [f32; 3],
    pub accent_color: [f32; 3],
}

impl Station {
    pub fn has(&self, s: Service) -> bool {
        self.services.contains(&s)
    }
}

#[derive(Clone, Debug)]
pub struct Deposit {
    pub angle: f32,
    pub amount: f32,
    pub max: f32,
}

#[derive(Clone, Debug)]
pub struct Planet {
    pub id: String,
    pub name: String,
    pub pos: Vec2,
    pub radius: f32,
    pub gravity: f32,
    pub influence: f32,
    pub ore: Ore,
    pub deposits: Vec<Deposit>,
    pub pad: Option<usize>,
}

impl Planet {
    pub fn accel(&self, p: Vec2) -> Vec2 {
        let d = self.pos - p;
        let dist = d.length().max(self.radius * 0.5);
        if dist > self.influence {
            return Vec2::ZERO;
        }
        let fade = smoothstep(self.influence, self.influence * 0.7, dist);
        let r = (self.radius / dist.max(self.radius)).powi(2);
        d / dist * self.gravity * r * fade
    }
    /// Halbe Winkelbreite eines Erzvorkommens.
    pub fn deposit_half_angle(&self) -> f32 {
        (4.5 / self.radius).min(0.35)
    }
}

#[derive(Clone, Debug)]
pub struct Spinner {
    pub pos: Vec2,
    pub arm_length: f32,
    pub arm_width: f32,
    pub arms: u32,
    pub speed: f32,
    pub angle: f32,
    pub prev_angle: f32,
    pub color: [f32; 3],
}

impl Spinner {
    pub fn quads(&self) -> Vec<Quad> {
        (0..self.arms)
            .map(|i| {
                let a = self.angle + std::f32::consts::PI * i as f32 / self.arms as f32;
                Quad::obb(self.pos, Vec2::new(self.arm_length, self.arm_width * 0.5), a)
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct Anomaly {
    pub name: String,
    pub pos: Vec2,
    pub radius: f32,
    pub core_radius: f32,
    pub strength: f32,
}

impl Anomaly {
    pub fn accel(&self, p: Vec2) -> Vec2 {
        let d = self.pos - p;
        let dist = d.length();
        if dist >= self.radius || dist < 1e-3 {
            return Vec2::ZERO;
        }
        let k = 1.0 - dist / self.radius;
        d / dist * self.strength * k * k
    }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Debug)]
pub struct World {
    pub radius: f32,
    pub stations: Vec<Station>,
    pub planets: Vec<Planet>,
    pub pads: Vec<Pad>,
    pub colliders: Vec<StaticCollider>,
    pub spinners: Vec<Spinner>,
    pub anomalies: Vec<Anomaly>,
}

const PAD_THICKNESS: f32 = 0.35;

impl World {
    pub fn build(data: &GameData, rng: &mut Rng) -> World {
        let wd = &data.world;
        let mut w = World {
            radius: wd.radius,
            stations: Vec::new(),
            planets: Vec::new(),
            pads: Vec::new(),
            colliders: Vec::new(),
            spinners: Vec::new(),
            anomalies: Vec::new(),
        };

        for (si, sd) in wd.stations.iter().enumerate() {
            let rows: Vec<Vec<char>> = sd.layout.iter().map(|r| r.chars().collect()).collect();
            let h = rows.len() as i32;
            let wcols = rows.first().map(|r| r.len()).unwrap_or(0) as i32;
            let pos = v(sd.pos);
            let cell = sd.cell;
            let center_of = |col: i32, row: i32| {
                pos + Vec2::new(
                    (col as f32 - (wcols - 1) as f32 * 0.5) * cell,
                    ((h - 1) as f32 * 0.5 - row as f32) * cell,
                )
            };
            let at = |col: i32, row: i32| -> char {
                if col < 0 || row < 0 || row >= h || col >= wcols {
                    '.'
                } else {
                    rows[row as usize][col as usize]
                }
            };
            let solid = |ch: char| matches!(ch, '#' | 'X' | 'W');

            let mut station = Station {
                id: sd.id.clone(),
                name: sd.name.clone(),
                kind: sd.kind,
                pos,
                cell,
                cells: Vec::new(),
                pads: Vec::new(),
                services: sd.services.clone(),
                bounds: Aabb {
                    min: center_of(0, h - 1) - Vec2::splat(cell * 0.5),
                    max: center_of(wcols - 1, 0) + Vec2::splat(cell * 0.5),
                },
                main_color: super::data::hex(&sd.main_color),
                accent_color: super::data::hex(&sd.accent_color),
            };

            for row in 0..h {
                // Blöcke: horizontale Läufe zu einem Kollider zusammenfassen.
                let mut col = 0;
                while col < wcols {
                    let ch = at(col, row);
                    let kind = match ch {
                        '#' => Some(CellKind::Block),
                        'X' => Some(CellKind::Accent),
                        'W' => Some(CellKind::Window),
                        'L' => Some(CellKind::Light),
                        _ => None,
                    };
                    if let Some(k) = kind {
                        station.cells.push(Cell {
                            kind: k,
                            center: center_of(col, row),
                            col,
                            row,
                        });
                    }
                    if solid(ch) {
                        let start = col;
                        while col < wcols && solid(at(col, row)) {
                            col += 1;
                        }
                        let a = center_of(start, row);
                        let b = center_of(col - 1, row);
                        let c = (a + b) * 0.5;
                        let half = Vec2::new((b.x - a.x) * 0.5 + cell * 0.5, cell * 0.5);
                        w.push_quad(Quad::obb(c, half, 0.0), Surface::Block);
                        continue;
                    }
                    col += 1;
                }
                // Landeplattformen.
                let mut col = 0;
                while col < wcols {
                    let ch = at(col, row);
                    if matches!(ch, '^' | 'v' | '<' | '>') {
                        let start = col;
                        while col < wcols && at(col, row) == ch && matches!(ch, '^' | 'v') {
                            col += 1;
                        }
                        if col == start {
                            col += 1;
                        }
                        // Vertikale Plattformen ('<', '>') sind eine Zelle groß.
                        let a = center_of(start, row);
                        let b = center_of(col - 1, row);
                        let mid = (a + b) * 0.5;
                        let span = (b - a).length() * 0.5 + cell * 0.5;
                        let normal = match ch {
                            '^' => Vec2::Y,
                            'v' => -Vec2::Y,
                            '<' => -Vec2::X,
                            _ => Vec2::X,
                        };
                        let surface_center = mid - normal * (cell * 0.5 - PAD_THICKNESS);
                        let pad_idx = w.pads.len();
                        w.pads.push(Pad {
                            owner: Owner::Station(si),
                            center: surface_center,
                            normal,
                            half_width: span * 0.92,
                        });
                        station.pads.push(pad_idx);
                        let plate_c = surface_center - normal * (PAD_THICKNESS * 0.5);
                        let angle = f32::atan2(-normal.x, normal.y);
                        w.push_quad(
                            Quad::obb(plate_c, Vec2::new(span, PAD_THICKNESS * 0.5), angle),
                            Surface::Pad(pad_idx),
                        );
                        continue;
                    }
                    col += 1;
                }
            }
            w.stations.push(station);
        }

        for (pi, pd) in wd.planets.iter().enumerate() {
            let pos = v(pd.pos);
            let mut planet = Planet {
                id: pd.id.clone(),
                name: pd.name.clone(),
                pos,
                radius: pd.radius,
                gravity: pd.surface_gravity,
                influence: pd.influence,
                ore: pd.ore,
                deposits: Vec::new(),
                pad: None,
            };
            w.colliders.push(StaticCollider {
                shape: Shape::Circle { c: pos, r: pd.radius },
                aabb: Aabb::around(pos, pd.radius),
                surface: Surface::Planet(pi),
            });
            let outpost = pd.outpost_angle.map(|a| a.to_radians());
            if let Some(a) = outpost {
                let n = Vec2::new(a.cos(), a.sin());
                let pad_idx = w.pads.len();
                let surface_center = pos + n * (pd.radius + PAD_THICKNESS);
                w.pads.push(Pad {
                    owner: Owner::Planet(pi),
                    center: surface_center,
                    normal: n,
                    half_width: 2.6,
                });
                planet.pad = Some(pad_idx);
                let angle = f32::atan2(-n.x, n.y);
                // Plattform + zwei Stützblöcke links/rechts.
                w.push_quad(
                    Quad::obb(pos + n * (pd.radius + PAD_THICKNESS * 0.5 - 0.3), Vec2::new(3.0, PAD_THICKNESS * 0.5 + 0.3), angle),
                    Surface::Pad(pad_idx),
                );
                for side in [-1.0f32, 1.0] {
                    let t = Vec2::new(n.y, -n.x);
                    let c = pos + n * (pd.radius + 0.8) + t * side * 4.2;
                    w.push_quad(Quad::obb(c, Vec2::new(1.1, 1.6), angle), Surface::Block);
                }
            }
            // Erzvorkommen gleichmäßig verteilt, mit etwas Zufall, nicht auf der Landestation.
            let count = pd.deposits.max(1);
            let base = rng.range(0.0, std::f32::consts::TAU);
            for i in 0..count {
                let mut a = base + std::f32::consts::TAU * i as f32 / count as f32 + rng.range(-0.2, 0.2);
                if let Some(o) = outpost {
                    let diff = angle_diff(a, o);
                    if diff.abs() < 0.35 {
                        a = o + if diff >= 0.0 { 0.5 } else { -0.5 };
                    }
                }
                planet.deposits.push(Deposit {
                    angle: a.rem_euclid(std::f32::consts::TAU),
                    amount: pd.deposit_amount,
                    max: pd.deposit_amount,
                });
            }
            w.planets.push(planet);
        }

        for sp in &wd.spinners {
            w.spinners.push(Spinner {
                pos: v(sp.pos),
                arm_length: sp.arm_length,
                arm_width: sp.arm_width,
                arms: sp.arms.max(1),
                speed: sp.speed.to_radians(),
                angle: 0.0,
                prev_angle: 0.0,
                color: super::data::hex(&sp.color),
            });
        }
        for an in &wd.anomalies {
            w.anomalies.push(Anomaly {
                name: an.name.clone(),
                pos: v(an.pos),
                radius: an.radius,
                core_radius: an.core_radius,
                strength: an.strength,
            });
        }
        w
    }

    fn push_quad(&mut self, q: Quad, surface: Surface) {
        self.colliders.push(StaticCollider {
            shape: Shape::Quad(q),
            aabb: q.aabb(),
            surface,
        });
    }

    pub fn gravity(&self, p: Vec2) -> Vec2 {
        let mut a = Vec2::ZERO;
        for pl in &self.planets {
            a += pl.accel(p);
        }
        for an in &self.anomalies {
            a += an.accel(p);
        }
        a
    }

    pub fn owner_name(&self, o: Owner) -> &str {
        match o {
            Owner::Station(i) => &self.stations[i].name,
            Owner::Planet(i) => &self.planets[i].name,
        }
    }

    /// Ort eines Punktes relativ zu einem Spinner-Arm (für die Geschwindigkeit am Kontakt).
    pub fn spinner_velocity(&self, idx: usize, p: Vec2) -> Vec2 {
        let s = &self.spinners[idx];
        super::geom::cross_sv(s.speed, p - s.pos)
    }
}

/// Kleinster vorzeichenbehafteter Winkelabstand a - b in (-PI, PI].
pub fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = (a - b).rem_euclid(std::f32::consts::TAU);
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    d
}

/// Punkt auf einem Planetenrand.
pub fn rim_point(p: &Planet, angle: f32, extra: f32) -> Vec2 {
    p.pos + rot(Vec2::X, angle) * (p.radius + extra)
}
