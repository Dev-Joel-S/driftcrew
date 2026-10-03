//! Statische Welt: Stationen aus Rastern, Planeten, Landeplattformen, Rotoren, Anomalien.

use bevy::math::Vec2;

use super::data::{AnomalyKind, GameData, Ore, Prices, Service, SpinnerShape, StationKind, v};
use super::geom::{Aabb, Poly, rot};
use super::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Station(usize),
    Planet(usize),
}

impl Owner {
    pub fn station(self) -> Option<usize> {
        match self {
            Owner::Station(i) => Some(i),
            Owner::Planet(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellKind {
    Block,
    Accent,
    Window,
    Light,
    /// Schräge (halbe Zelle); das Zeichen sagt, welche Ecke voll ist: `/ \\ 7 r`.
    Slope(char),
    /// Tor-Block eines Monuments: verschwindet, wenn das Monument erwacht.
    Door,
}

pub fn is_slope(ch: char) -> bool {
    matches!(ch, '/' | '\\' | '7' | 'r')
}

/// Umriss (gegen den Uhrzeigersinn) einer Schrägen-Zelle, zentriert.
pub fn slope_outline(ch: char, h: f32) -> Vec<Vec2> {
    match ch {
        '/' => vec![Vec2::new(-h, -h), Vec2::new(h, -h), Vec2::new(h, h)],
        '\\' => vec![Vec2::new(-h, -h), Vec2::new(h, -h), Vec2::new(-h, h)],
        '7' => vec![Vec2::new(h, -h), Vec2::new(h, h), Vec2::new(-h, h)],
        _ => vec![Vec2::new(-h, -h), Vec2::new(h, h), Vec2::new(-h, h)],
    }
}

/// Freiliegende Ecken abschrägen: Ein `#` mit zwei leeren Nachbarn über Eck (und festen
/// Nachbarn gegenüber) wird zur Schräge. Blöcke unter Plattformen bleiben unverändert.
pub fn auto_chamfer(rows: &[Vec<char>]) -> Vec<Vec<char>> {
    let h = rows.len() as i32;
    let w = rows.first().map(|r| r.len()).unwrap_or(0) as i32;
    let at = |c: i32, r: i32| -> char {
        if c < 0 || r < 0 || r >= h || c >= w {
            '.'
        } else {
            rows[r as usize][c as usize]
        }
    };
    let empty = |ch: char| matches!(ch, '.' | 'L');
    let solid = |ch: char| matches!(ch, '#' | 'X' | 'W') || is_slope(ch);
    let mut out = rows.to_vec();
    for r in 0..h {
        for c in 0..w {
            if at(c, r) != '#' {
                continue;
            }
            let (up, down, left, right) = (at(c, r - 1), at(c, r + 1), at(c - 1, r), at(c + 1, r));
            let slope = if empty(up) && empty(left) && solid(down) && solid(right) {
                Some('/')
            } else if empty(up) && empty(right) && solid(down) && solid(left) {
                Some('\\')
            } else if empty(down) && empty(left) && solid(up) && solid(right) {
                Some('7')
            } else if empty(down) && empty(right) && solid(up) && solid(left) {
                Some('r')
            } else {
                None
            };
            if let Some(s) = slope {
                out[r as usize][c as usize] = s;
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub kind: CellKind,
    pub center: Vec2,
    pub col: i32,
    pub row: i32,
    /// Ab welcher Wiederaufbau-Etappe es diese Zelle gibt (0 = immer).
    pub stage: u8,
}

/// Etappe eines Rasterzeichens: `1`–`3` Blöcke, `a`–`c` Plattformen.
pub fn stage_of(ch: char) -> u8 {
    match ch {
        '1' | 'a' => 1,
        '2' | 'b' => 2,
        '3' | 'c' => 3,
        _ => 0,
    }
}

#[derive(Clone, Debug)]
pub struct Pad {
    pub owner: Owner,
    /// Mittelpunkt der Plattform-Oberfläche.
    pub center: Vec2,
    /// Richtung, in die die Plattform zeigt (Schiffsnase beim Andocken).
    pub normal: Vec2,
    pub half_width: f32,
    /// Freie Landezone auf einem Planeten (kein Menü, Werkzeuge bleiben aktiv).
    pub zone: bool,
    /// Wiederaufbau: Plattform gibt es erst ab dieser Etappe.
    pub stage: u8,
    pub enabled: bool,
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
    Poly(Poly),
    Circle { c: Vec2, r: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Block,
    /// Kleine Bauten auf Planeten (Außenposten).
    Structure,
    Pad(usize),
    Planet(usize),
}

#[derive(Clone, Debug)]
pub struct StaticCollider {
    pub shape: Shape,
    pub aabb: Aabb,
    pub surface: Surface,
    /// Wiederaufbau: Teile, die es noch nicht gibt, kollidieren nicht.
    pub enabled: bool,
    pub stage: u8,
    pub station: Option<usize>,
    /// Tor-Block eines Monuments (Index): fällt weg, wenn das Monument erwacht.
    pub door: Option<usize>,
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
    pub prices: Prices,
    pub ships_for_sale: Vec<String>,
    /// Erreichte Wiederaufbau-Etappe und die Dienste, die die Station von Anfang an hat.
    pub stage: u8,
    pub base_services: Vec<Service>,
    /// Lastaufnahme für Schwerlastkisten (falls vorhanden).
    pub socket: Option<Socket>,
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
/// Planeten ziehen nicht an (Schwerkraft gibt es nur an Anomalien und Schwarzen Löchern).
/// Sie sind Hindernisse, Landeorte und Erzquellen.
pub struct Planet {
    pub name: String,
    pub pos: Vec2,
    pub radius: f32,
    pub ore: Ore,
    pub deposits: Vec<Deposit>,
    /// Außenposten mit Erzannahme.
    pub pad: Option<usize>,
    /// Freie Landezonen.
    pub zones: Vec<usize>,
    pub prices: Prices,
}

impl Planet {
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
    pub shape: SpinnerShape,
}

impl Spinner {
    /// Größter Abstand eines Teils von der Drehachse (für die Grobprüfung).
    pub fn reach(&self) -> f32 {
        match self.shape {
            SpinnerShape::Arms => self.arm_length,
            SpinnerShape::Ring { radius, .. } => radius + self.arm_width,
        }
    }

    /// Ringsegmente in lokalen Koordinaten (Mitte, Winkel), ohne die Öffnungen.
    /// Bei Armen leer.
    pub fn ring_segments(&self) -> Vec<(Vec2, f32)> {
        let SpinnerShape::Ring {
            radius,
            segments,
            openings,
            gap,
        } = self.shape
        else {
            return Vec::new();
        };
        let n = segments.max(3);
        let period = (n / openings.max(1)).max(1);
        (0..n)
            .filter(|i| openings == 0 || i % period >= gap)
            .map(|i| {
                let a = std::f32::consts::TAU * (i as f32 + 0.5) / n as f32;
                (Vec2::new(a.cos(), a.sin()) * radius, a)
            })
            .collect()
    }

    /// Länge eines Ringsegments (Sehne plus etwas Überlappung).
    pub fn segment_length(&self) -> f32 {
        match self.shape {
            SpinnerShape::Ring {
                radius, segments, ..
            } => 2.0 * radius * (std::f32::consts::PI / segments.max(3) as f32).sin() + 0.3,
            SpinnerShape::Arms => 0.0,
        }
    }

    pub fn quads(&self) -> Vec<Poly> {
        match self.shape {
            SpinnerShape::Arms => (0..self.arms)
                .map(|i| {
                    let a = self.angle + std::f32::consts::PI * i as f32 / self.arms as f32;
                    Poly::obb(
                        self.pos,
                        Vec2::new(self.arm_length, self.arm_width * 0.5),
                        a,
                    )
                })
                .collect(),
            SpinnerShape::Ring { .. } => {
                let half = Vec2::new(self.segment_length() * 0.5, self.arm_width * 0.5);
                self.ring_segments()
                    .into_iter()
                    .map(|(c, a)| {
                        Poly::obb(
                            self.pos + rot(c, self.angle),
                            half,
                            a + self.angle + std::f32::consts::FRAC_PI_2,
                        )
                    })
                    .collect()
            }
        }
    }
}

/// Lastaufnahme: U-förmige Halterung an einer Station. Eine Schwerlastkiste gilt als
/// abgesetzt, wenn sie ruhig im Inneren liegt und nicht mehr am Kran hängt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Socket {
    /// Mitte des Innenraums.
    pub center: Vec2,
    /// Richtung der Öffnung (Einheitsvektor).
    pub open: Vec2,
}

/// Innenmaße der Lastaufnahme (halbe Breite quer zur Öffnung, halbe Tiefe) und Wandstärke.
pub const SOCKET_HALF_WIDTH: f32 = 2.3;
pub const SOCKET_HALF_DEPTH: f32 = 1.9;
pub const SOCKET_WALL: f32 = 0.6;

impl Socket {
    /// Steckt ein Körper mit dieser Mitte weit genug in der Halterung?
    pub fn holds(&self, p: Vec2) -> bool {
        let d = p - self.center;
        let side = Vec2::new(-self.open.y, self.open.x);
        let depth = d.dot(self.open);
        (-SOCKET_HALF_DEPTH..=SOCKET_HALF_DEPTH - 0.9).contains(&depth)
            && d.dot(side).abs() <= SOCKET_HALF_WIDTH
    }

    /// Die drei Wände als Quader: Boden (gegenüber der Öffnung) und zwei Seiten.
    pub fn walls(&self) -> [Poly; 3] {
        let side = Vec2::new(-self.open.y, self.open.x);
        let angle = f32::atan2(self.open.y, self.open.x);
        let back = self.center - self.open * (SOCKET_HALF_DEPTH + SOCKET_WALL * 0.5);
        let half_side = Vec2::new(SOCKET_HALF_DEPTH + SOCKET_WALL, SOCKET_WALL * 0.5);
        let side_off = SOCKET_HALF_WIDTH + SOCKET_WALL * 0.5;
        [
            Poly::obb(
                back,
                Vec2::new(SOCKET_WALL * 0.5, SOCKET_HALF_WIDTH + SOCKET_WALL),
                angle,
            ),
            Poly::obb(
                self.center + side * side_off - self.open * (SOCKET_WALL * 0.5),
                half_side,
                angle,
            ),
            Poly::obb(
                self.center - side * side_off - self.open * (SOCKET_WALL * 0.5),
                half_side,
                angle,
            ),
        ]
    }
}

#[derive(Clone, Debug)]
pub struct Anomaly {
    pub name: String,
    pub kind: AnomalyKind,
    pub pos: Vec2,
    pub radius: f32,
    /// Kern der Anomalie bzw. Ereignishorizont des Schwarzen Lochs.
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
        let a = match self.kind {
            AnomalyKind::Anomaly => self.strength * k * k,
            // Zum Horizont hin steiler: ab einem gewissen Abstand reicht kein Schub mehr.
            AnomalyKind::BlackHole => {
                self.strength * k * k * (1.0 + 2.0 * self.core_radius / dist.max(self.core_radius))
            }
        };
        d / dist * a
    }
    pub fn is_black_hole(&self) -> bool {
        self.kind == AnomalyKind::BlackHole
    }

    /// Abstand, innerhalb dessen der Sog stärker ist als `max_accel` – von dort kommt man
    /// auch mit Vollschub nicht mehr weg. Nur für die Anzeige.
    pub fn no_return_radius(&self, max_accel: f32) -> f32 {
        let pull = |d: f32| self.accel(self.pos + Vec2::new(d, 0.0)).length();
        if pull(self.core_radius) <= max_accel {
            return self.core_radius;
        }
        let (mut lo, mut hi) = (self.core_radius, self.radius);
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            if pull(mid) > max_accel {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi
    }
}

/// Bauwerk der Vorgänger (Geschichte): Raster wie eine Station, ohne Plattformen und Dienste.
#[derive(Clone, Debug)]
pub struct Monument {
    pub id: String,
    pub name: String,
    pub pos: Vec2,
    pub cell: f32,
    pub cells: Vec<Cell>,
    pub bounds: Aabb,
    pub main_color: [f32; 3],
    pub accent_color: [f32; 3],
    pub awake: bool,
}

#[derive(Clone, Debug)]
pub struct World {
    pub radius: f32,
    pub stations: Vec<Station>,
    pub monuments: Vec<Monument>,
    pub planets: Vec<Planet>,
    pub pads: Vec<Pad>,
    pub colliders: Vec<StaticCollider>,
    pub spinners: Vec<Spinner>,
    pub anomalies: Vec<Anomaly>,
}

const PAD_THICKNESS: f32 = 0.35;
/// Halbe Breite einer freien Landezone auf Planeten.
pub const ZONE_HALF: f32 = 2.5;
/// Abstand (Meter entlang der Oberfläche) zwischen Landezone und Erzvorkommen.
const ZONE_DEPOSIT_OFFSET: f32 = 6.0;

impl World {
    pub fn build(data: &GameData, rng: &mut Rng) -> World {
        let wd = &data.world;
        let mut w = World {
            radius: wd.radius,
            stations: Vec::new(),
            monuments: Vec::new(),
            planets: Vec::new(),
            pads: Vec::new(),
            colliders: Vec::new(),
            spinners: Vec::new(),
            anomalies: Vec::new(),
        };

        for (si, sd) in wd.stations.iter().enumerate() {
            let mut rows: Vec<Vec<char>> = sd.layout.iter().map(|r| r.chars().collect()).collect();
            if sd.auto_chamfer {
                rows = auto_chamfer(&rows);
            }
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
            let solid = |ch: char| matches!(ch, '#' | 'X' | 'W' | '1' | '2' | '3');

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
                prices: sd.prices.clone(),
                ships_for_sale: sd.ships_for_sale.clone(),
                stage: 0,
                base_services: sd.services.clone(),
                socket: sd.socket.map(|(off, deg)| Socket {
                    center: pos + v(off),
                    open: Vec2::new(deg.to_radians().cos(), deg.to_radians().sin()),
                }),
            };

            for row in 0..h {
                // Blöcke: horizontale Läufe zu einem Kollider zusammenfassen.
                let mut col = 0;
                while col < wcols {
                    let ch = at(col, row);
                    let kind = match ch {
                        '#' | '1' | '2' | '3' => Some(CellKind::Block),
                        'X' => Some(CellKind::Accent),
                        'W' => Some(CellKind::Window),
                        'L' => Some(CellKind::Light),
                        c if is_slope(c) => Some(CellKind::Slope(c)),
                        _ => None,
                    };
                    if is_slope(ch) {
                        let c = center_of(col, row);
                        let pts = slope_outline(ch, cell * 0.5);
                        w.push_quad(Poly::transformed(&pts, c, 0.0), Surface::Block);
                    }
                    if let Some(k) = kind {
                        station.cells.push(Cell {
                            kind: k,
                            center: center_of(col, row),
                            col,
                            row,
                            stage: stage_of(ch),
                        });
                    }
                    if solid(ch) {
                        // Läufe nur innerhalb derselben Etappe zusammenfassen.
                        let stage = stage_of(ch);
                        let start = col;
                        col += 1;
                        while col < wcols && solid(at(col, row)) && stage_of(at(col, row)) == stage
                        {
                            let k = match at(col, row) {
                                'X' => CellKind::Accent,
                                'W' => CellKind::Window,
                                _ => CellKind::Block,
                            };
                            station.cells.push(Cell {
                                kind: k,
                                center: center_of(col, row),
                                col,
                                row,
                                stage,
                            });
                            col += 1;
                        }
                        let a = center_of(start, row);
                        let b = center_of(col - 1, row);
                        let c = (a + b) * 0.5;
                        let half = Vec2::new((b.x - a.x) * 0.5 + cell * 0.5, cell * 0.5);
                        w.push_staged(Poly::obb(c, half, 0.0), Surface::Block, si, stage);
                        continue;
                    }
                    col += 1;
                }
                // Landeplattformen.
                let mut col = 0;
                while col < wcols {
                    let ch = at(col, row);
                    if matches!(ch, '^' | 'v' | '<' | '>' | 'a' | 'b' | 'c') {
                        let stage = stage_of(ch);
                        let start = col;
                        while col < wcols
                            && at(col, row) == ch
                            && matches!(ch, '^' | 'v' | 'a' | 'b' | 'c')
                        {
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
                            '^' | 'a' | 'b' | 'c' => Vec2::Y,
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
                            zone: false,
                            stage,
                            enabled: stage == 0,
                        });
                        station.pads.push(pad_idx);
                        let plate_c = surface_center - normal * (PAD_THICKNESS * 0.5);
                        let angle = f32::atan2(-normal.x, normal.y);
                        w.push_staged(
                            Poly::obb(plate_c, Vec2::new(span, PAD_THICKNESS * 0.5), angle),
                            Surface::Pad(pad_idx),
                            si,
                            stage,
                        );
                        continue;
                    }
                    col += 1;
                }
            }
            if let Some(sock) = station.socket {
                for q in sock.walls() {
                    w.push_staged(q, Surface::Block, si, 0);
                }
            }
            w.stations.push(station);
        }

        for (pi, pd) in wd.planets.iter().enumerate() {
            let pos = v(pd.pos);
            let mut planet = Planet {
                name: pd.name.clone(),
                pos,
                radius: pd.radius,
                ore: pd.ore,
                deposits: Vec::new(),
                pad: None,
                zones: Vec::new(),
                prices: pd.prices.clone(),
            };
            w.colliders.push(StaticCollider {
                shape: Shape::Circle {
                    c: pos,
                    r: pd.radius,
                },
                aabb: Aabb::around(pos, pd.radius),
                surface: Surface::Planet(pi),
                enabled: true,
                stage: 0,
                station: None,
                door: None,
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
                    zone: false,
                    stage: 0,
                    enabled: true,
                });
                planet.pad = Some(pad_idx);
                let angle = f32::atan2(-n.x, n.y);
                // Plattform + zwei Stützblöcke links/rechts.
                w.push_quad(
                    Poly::obb(
                        pos + n * (pd.radius + PAD_THICKNESS * 0.5 - 0.3),
                        Vec2::new(3.0, PAD_THICKNESS * 0.5 + 0.3),
                        angle,
                    ),
                    Surface::Pad(pad_idx),
                );
                for side in [-1.0f32, 1.0] {
                    let t = Vec2::new(n.y, -n.x);
                    let c = pos + n * (pd.radius + 0.8) + t * side * 4.2;
                    w.push_quad(Poly::obb(c, Vec2::new(1.1, 1.6), angle), Surface::Structure);
                }
            }
            // Freie Landezonen: schmale Plattform, daneben ein Erzvorkommen in Bohrreichweite.
            let zone_angles: Vec<f32> = pd.landing_zones.iter().map(|a| a.to_radians()).collect();
            for (zi, &a) in zone_angles.iter().enumerate() {
                let n = Vec2::new(a.cos(), a.sin());
                let pad_idx = w.pads.len();
                w.pads.push(Pad {
                    owner: Owner::Planet(pi),
                    center: pos + n * (pd.radius + PAD_THICKNESS),
                    normal: n,
                    half_width: ZONE_HALF * 0.92,
                    zone: true,
                    stage: 0,
                    enabled: true,
                });
                planet.zones.push(pad_idx);
                let angle = f32::atan2(-n.x, n.y);
                w.push_quad(
                    Poly::obb(
                        pos + n * (pd.radius + PAD_THICKNESS * 0.5 - 0.3),
                        Vec2::new(ZONE_HALF, PAD_THICKNESS * 0.5 + 0.3),
                        angle,
                    ),
                    Surface::Pad(pad_idx),
                );
                let side = if zi % 2 == 0 { 1.0 } else { -1.0 };
                planet.deposits.push(Deposit {
                    angle: (a + side * ZONE_DEPOSIT_OFFSET / pd.radius)
                        .rem_euclid(std::f32::consts::TAU),
                    amount: pd.deposit_amount,
                    max: pd.deposit_amount,
                });
            }
            // Übrige Erzvorkommen gleichmäßig verteilt, mit etwas Zufall, abseits der Plattformen.
            let count = pd.deposits.max(1).saturating_sub(zone_angles.len() as u32);
            let base = rng.range(0.0, std::f32::consts::TAU);
            let keep_clear: Vec<f32> = outpost.iter().copied().chain(zone_angles).collect();
            for i in 0..count {
                let mut a =
                    base + std::f32::consts::TAU * i as f32 / count as f32 + rng.range(-0.2, 0.2);
                for &o in &keep_clear {
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
                shape: sp.shape,
            });
        }
        w.build_monuments(data);
        for an in &wd.anomalies {
            w.anomalies.push(Anomaly {
                name: an.name.clone(),
                kind: an.kind,
                pos: v(an.pos),
                radius: an.radius,
                core_radius: an.core_radius,
                strength: an.strength,
            });
        }
        w
    }

    fn push_quad(&mut self, q: Poly, surface: Surface) {
        self.colliders.push(StaticCollider {
            shape: Shape::Poly(q),
            aabb: q.aabb(),
            surface,
            enabled: true,
            stage: 0,
            station: None,
            door: None,
        });
    }

    /// Kollider, der zu einer Wiederaufbau-Etappe einer Station gehört.
    fn push_staged(&mut self, q: Poly, surface: Surface, station: usize, stage: u8) {
        self.colliders.push(StaticCollider {
            shape: Shape::Poly(q),
            aabb: q.aabb(),
            surface,
            enabled: stage == 0,
            stage,
            station: Some(station),
            door: None,
        });
    }

    /// Monumente der Geschichte aus ihrem Raster bauen (Blöcke, Schrägen, Lichter, Tor-Blöcke).
    fn build_monuments(&mut self, data: &GameData) {
        for (mi, md) in data.story.monuments.iter().enumerate() {
            let rows: Vec<Vec<char>> = md.layout.iter().map(|r| r.chars().collect()).collect();
            let h = rows.len() as i32;
            let wcols = rows.first().map(|r| r.len()).unwrap_or(0) as i32;
            let pos = v(md.pos);
            let cell = md.cell;
            let center_of = |col: i32, row: i32| {
                pos + Vec2::new(
                    (col as f32 - (wcols - 1) as f32 * 0.5) * cell,
                    ((h - 1) as f32 * 0.5 - row as f32) * cell,
                )
            };
            let mut m = Monument {
                id: md.id.clone(),
                name: md.name.clone(),
                pos,
                cell,
                cells: Vec::new(),
                bounds: Aabb {
                    min: center_of(0, h - 1) - Vec2::splat(cell * 0.5),
                    max: center_of(wcols - 1, 0) + Vec2::splat(cell * 0.5),
                },
                main_color: super::data::hex(&md.main_color),
                accent_color: super::data::hex(&md.accent_color),
                awake: false,
            };
            for (row, line) in rows.iter().enumerate() {
                for (col, &ch) in line.iter().enumerate() {
                    let (col, row) = (col as i32, row as i32);
                    let c = center_of(col, row);
                    let kind = match ch {
                        '#' => CellKind::Block,
                        'X' => CellKind::Accent,
                        'L' => CellKind::Light,
                        'o' => CellKind::Door,
                        c if is_slope(c) => CellKind::Slope(c),
                        _ => continue,
                    };
                    m.cells.push(Cell {
                        kind,
                        center: c,
                        col,
                        row,
                        stage: 0,
                    });
                    let q = match kind {
                        CellKind::Light => continue,
                        CellKind::Slope(ch) => {
                            Poly::transformed(&slope_outline(ch, cell * 0.5), c, 0.0)
                        }
                        _ => Poly::obb(c, Vec2::splat(cell * 0.5), 0.0),
                    };
                    self.colliders.push(StaticCollider {
                        shape: Shape::Poly(q),
                        aabb: q.aabb(),
                        surface: Surface::Block,
                        enabled: true,
                        stage: 0,
                        station: None,
                        door: (kind == CellKind::Door).then_some(mi),
                    });
                }
            }
            self.monuments.push(m);
        }
    }

    /// Ein Monument erwacht: die Tor-Blöcke fallen weg.
    pub fn wake_monument(&mut self, mi: usize) {
        for c in &mut self.colliders {
            if c.door == Some(mi) {
                c.enabled = false;
            }
        }
        if let Some(m) = self.monuments.get_mut(mi) {
            m.awake = true;
        }
    }

    /// Etappe einer Station setzen: Teile und Plattformen bis zu dieser Etappe existieren,
    /// die Dienste sind Grunddienste plus alles, was die Etappen freischalten.
    pub fn set_stage(&mut self, si: usize, stage: u8, unlocked: &[Service]) {
        for c in &mut self.colliders {
            if c.station == Some(si) {
                c.enabled = c.stage <= stage;
            }
        }
        for &p in &self.stations[si].pads {
            self.pads[p].enabled = self.pads[p].stage <= stage;
        }
        let st = &mut self.stations[si];
        st.stage = stage;
        st.services = st.base_services.clone();
        for s in unlocked {
            if !st.services.contains(s) {
                st.services.push(*s);
            }
        }
    }

    /// Anziehung an einem Punkt – nur Anomalien und Schwarze Löcher, Planeten nicht.
    pub fn gravity(&self, p: Vec2) -> Vec2 {
        let mut a = Vec2::ZERO;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_solid_cell_is_visible() {
        let data = GameData::embedded().unwrap();
        let w = World::build(&data, &mut Rng::new(1));
        for (st, sd) in w.stations.iter().zip(&data.world.stations) {
            let solid = sd
                .layout
                .iter()
                .flat_map(|r| r.chars())
                .filter(|c| matches!(c, '#' | 'X' | 'W' | '1' | '2' | '3'))
                .count();
            let cells = st
                .cells
                .iter()
                .filter(|c| c.kind != CellKind::Light)
                .count();
            assert_eq!(solid, cells, "Station {}", st.id);
        }
    }
}
