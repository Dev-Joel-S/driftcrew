//! Spieldaten: Schiffe, Welt, Shop und Missionen werden als RON-Dateien beschrieben.
//!
//! Die Dateien liegen unter `assets/data/`. Sie werden zusätzlich in die Binärdatei
//! eingebettet, damit das Spiel auch ohne Asset-Ordner startet. Liegt eine Datei im
//! Ordner, hat sie Vorrang (so lassen sich Werte ohne Neukompilieren ändern).

use crate::sim::fx::{Fx, Vec2};
use serde::{Deserialize, Serialize};

pub type P = (Fx, Fx);

pub fn v(p: P) -> Vec2 {
    Vec2::new(p.0, p.1)
}

/// Farbe aus "#rrggbb" in lineare-ish sRGB-Komponenten 0..1 (die Umrechnung macht das Rendering).
pub fn hex(s: &str) -> [Fx; 3] {
    let s = s.trim_start_matches('#');
    let p = |i: usize| {
        Fx::from(u8::from_str_radix(s.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255))
            / fx!(255.0)
    };
    [p(0), p(2), p(4)]
}

// ---------------------------------------------------------------------------
// Schiffe
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Cannon,
    Crane,
    Drill,
    /// Sonar: ein Impuls zeigt Wracks, Erz und Kapseln und erfasst Kartendaten.
    Scanner,
}

impl ToolKind {
    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Cannon => "Kanone",
            ToolKind::Crane => "Kran",
            ToolKind::Drill => "Bohrer",
            ToolKind::Scanner => "Scanner",
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub enum PartKind {
    Hull,
    Cockpit,
    Armor,
    /// Frachtmodul mit Kapazität in Tonnen.
    CargoPod(Fx),
    /// Triebwerk mit Schubkraft.
    Thruster(Fx),
    Tool(ToolKind),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PartDef {
    pub kind: PartKind,
    pub pos: P,
    pub size: P,
    pub mass: Fx,
    /// Schubrichtung in Grad relativ zur Schiffsnase (0 = schiebt nach vorne).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dir: Fx,
    /// Umriss des Teils (Optik und Kollision).
    #[serde(default)]
    pub shape: PartShape,
    /// Feste Position: wird nicht mit den Triebwerken symmetrisch neu angeordnet
    /// (angebaute Module an Bauplätzen).
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixed: bool,
}

fn is_zero(x: &Fx) -> bool {
    *x == fx!(0.0)
}
fn is_false(b: &bool) -> bool {
    !*b
}

/// Umriss eines Schiffsteils. „Oben“ = Richtung Schiffsnase.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum PartShape {
    #[default]
    Box,
    /// Trapez: (Breite oben, Breite unten) als Anteil der vollen Breite.
    Taper(Fx, Fx),
    /// Abgeschrägte Ecken; Anteil der kürzeren Halbachse.
    Chamfer(Fx),
    /// Spitze nach vorne; Anteil der Höhe, den die Spitze einnimmt.
    Nose(Fx),
    /// Spitze nach hinten.
    Tail(Fx),
    /// Keil zur Seite: (Höhe links, Höhe rechts) als Anteil – für Flügel und Flossen.
    Wing(Fx, Fx),
}

impl PartShape {
    /// Umrisspunkte gegen den Uhrzeigersinn, zentriert, für die halbe Größe `half`.
    pub fn outline(&self, half: Vec2) -> Vec<Vec2> {
        let (hx, hy) = (half.x, half.y);
        let mut pts = match *self {
            PartShape::Box => vec![
                Vec2::new(-hx, -hy),
                Vec2::new(hx, -hy),
                Vec2::new(hx, hy),
                Vec2::new(-hx, hy),
            ],
            PartShape::Taper(top, bottom) => vec![
                Vec2::new(-hx * bottom, -hy),
                Vec2::new(hx * bottom, -hy),
                Vec2::new(hx * top, hy),
                Vec2::new(-hx * top, hy),
            ],
            PartShape::Chamfer(c) => {
                let k = c.clamp(fx!(0.0), fx!(0.9)) * hx.min(hy);
                vec![
                    Vec2::new(-hx + k, -hy),
                    Vec2::new(hx - k, -hy),
                    Vec2::new(hx, -hy + k),
                    Vec2::new(hx, hy - k),
                    Vec2::new(hx - k, hy),
                    Vec2::new(-hx + k, hy),
                    Vec2::new(-hx, hy - k),
                    Vec2::new(-hx, -hy + k),
                ]
            }
            PartShape::Nose(f) => {
                let y = hy - fx!(2.0) * hy * f.clamp(fx!(0.05), fx!(0.95));
                vec![
                    Vec2::new(-hx, -hy),
                    Vec2::new(hx, -hy),
                    Vec2::new(hx, y),
                    Vec2::new(fx!(0.0), hy),
                    Vec2::new(-hx, y),
                ]
            }
            PartShape::Tail(f) => {
                let y = -hy + fx!(2.0) * hy * f.clamp(fx!(0.05), fx!(0.95));
                vec![
                    Vec2::new(fx!(0.0), -hy),
                    Vec2::new(hx, y),
                    Vec2::new(hx, hy),
                    Vec2::new(-hx, hy),
                    Vec2::new(-hx, y),
                ]
            }
            PartShape::Wing(left, right) => vec![
                Vec2::new(-hx, -hy),
                Vec2::new(hx, -hy),
                Vec2::new(hx, -hy + fx!(2.0) * hy * right.clamp(fx!(0.05), fx!(1.0))),
                Vec2::new(-hx, -hy + fx!(2.0) * hy * left.clamp(fx!(0.05), fx!(1.0))),
            ],
        };
        // Doppelte Punkte (z. B. Trapez mit Spitze 0) entfernen.
        pts.dedup_by(|a, b| (*a - *b).length() < fx!(1e-4));
        if pts.len() > 3 && (pts[0] - *pts.last().unwrap()).length() < fx!(1e-4) {
            pts.pop();
        }
        pts
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ThrusterLayout {
    pub count: u8,
    pub xs: Vec<Fx>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShipDef {
    pub id: String,
    pub name: String,
    pub class: String,
    pub description: String,
    pub price: u32,
    pub hull_color: String,
    pub accent_color: String,
    pub max_hull: Fx,
    pub max_shield: Fx,
    pub max_ammo: u32,
    #[serde(default = "default_ang_damp")]
    pub angular_damping: Fx,
    #[serde(default = "default_min_thrusters")]
    pub min_thrusters: u8,
    /// Schild lädt sich nach `shield_delay` Sekunden ohne Treffer mit `shield_regen` pro Sekunde auf.
    #[serde(default = "default_shield_regen")]
    pub shield_regen: Fx,
    #[serde(default = "default_shield_delay")]
    pub shield_delay: Fx,
    /// Empfohlene Crewgröße (von, bis). Slots belegen nur Menschen, keine Bots.
    #[serde(default = "default_crew")]
    pub crew: (u8, u8),
    /// Tankgröße (Einheiten).
    #[serde(default = "default_fuel")]
    pub fuel_capacity: Fx,
    /// Verbrauch pro Schubeinheit und Sekunde.
    #[serde(default = "default_fuel_burn")]
    pub fuel_burn: Fx,
    /// Reihenfolge der Triebwerke = Reihenfolge, in der sie in der Lobby belegt werden.
    pub parts: Vec<PartDef>,
    #[serde(default)]
    pub thruster_layouts: Vec<ThrusterLayout>,
    /// Bauplätze: feste Stellen am Rumpf, an die in der Werft Module angebaut werden.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<MountDef>,
}

/// Ein Bauplatz am Rumpf.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MountDef {
    pub id: String,
    pub name: String,
    pub pos: P,
    /// Welche Modularten hier passen.
    pub accepts: Vec<ModuleSlot>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleSlot {
    Engine,
    Cargo,
    Tool,
    Armor,
}

impl ModuleSlot {
    pub fn label(self) -> &'static str {
        match self {
            ModuleSlot::Engine => "Triebwerk",
            ModuleSlot::Cargo => "Fracht",
            ModuleSlot::Tool => "Werkzeug",
            ModuleSlot::Armor => "Panzerung",
        }
    }
}

/// Ein Modul, das in der Werft aus Credits, Material und Bauteilen gebaut wird.
#[derive(Deserialize, Clone, Debug)]
pub struct ModuleDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub slot: ModuleSlot,
    pub part: PartKind,
    pub size: P,
    pub mass: Fx,
    #[serde(default)]
    pub shape: PartShape,
    /// Zusätzliche Hülle (Panzerung).
    #[serde(default)]
    pub hull: Fx,
    pub credits: u32,
    #[serde(default)]
    pub materials: Vec<(Ore, Fx)>,
    /// Bauteile aus Wracks.
    #[serde(default)]
    pub parts: u32,
}

fn default_ang_damp() -> Fx {
    fx!(0.12)
}
fn default_min_thrusters() -> u8 {
    2
}
fn default_shield_regen() -> Fx {
    fx!(4.0)
}
fn default_shield_delay() -> Fx {
    fx!(4.0)
}
fn default_crew() -> (u8, u8) {
    (1, 4)
}
fn default_fuel() -> Fx {
    fx!(100.0)
}
fn default_fuel_burn() -> Fx {
    fx!(0.0065)
}

impl ShipDef {
    pub fn thruster_parts(&self) -> impl Iterator<Item = (usize, &PartDef, Fx)> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.kind {
                PartKind::Thruster(t) => Some((i, p, t)),
                _ => None,
            })
    }
    pub fn tool_parts(&self) -> impl Iterator<Item = (usize, &PartDef, ToolKind)> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.kind {
                PartKind::Tool(k) => Some((i, p, k)),
                _ => None,
            })
    }
    pub fn max_thrusters(&self) -> u8 {
        self.thruster_parts().count() as u8
    }
    pub fn cargo_capacity(&self) -> Fx {
        self.parts
            .iter()
            .map(|p| match p.kind {
                PartKind::CargoPod(c) => c,
                _ => fx!(0.0),
            })
            .sum()
    }
    /// x-Positionen der Triebwerke, wenn `n` belegt sind (symmetrisch neu angeordnet).
    pub fn thruster_xs(&self, n: u8) -> Vec<Fx> {
        if let Some(l) = self
            .thruster_layouts
            .iter()
            .find(|l| l.count == n && l.xs.len() == n as usize)
        {
            return l.xs.clone();
        }
        let span = self
            .thruster_parts()
            .map(|(_, p, _)| p.pos.0.abs())
            .fold(fx!(0.0), Fx::max);
        if n <= 1 {
            return vec![fx!(0.0)];
        }
        (0..n)
            .map(|i| -span + fx!(2.0) * span * Fx::from(i) / Fx::from(n - 1))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Welt
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ore {
    Ferrit,
    Kobalt,
    Solarit,
    Ionit,
    /// Altmetall aus Wracks (wird wie Erz gehandelt).
    Schrott,
}

impl Ore {
    pub const ALL: [Ore; 5] = [
        Ore::Ferrit,
        Ore::Kobalt,
        Ore::Solarit,
        Ore::Ionit,
        Ore::Schrott,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Ore::Ferrit => "Ferrit",
            Ore::Kobalt => "Kobalt",
            Ore::Solarit => "Solarit",
            Ore::Ionit => "Ionit",
            Ore::Schrott => "Schrott",
        }
    }
    /// Leuchtfarbe des Erzes (sRGB).
    pub fn color(self) -> [Fx; 3] {
        match self {
            Ore::Ferrit => [fx!(1.0), fx!(0.55), fx!(0.25)],
            Ore::Kobalt => [fx!(0.25), fx!(0.55), fx!(1.0)],
            Ore::Solarit => [fx!(1.0), fx!(0.85), fx!(0.15)],
            Ore::Ionit => [fx!(0.2), fx!(1.0), fx!(0.85)],
            Ore::Schrott => [fx!(0.72), fx!(0.66), fx!(0.58)],
        }
    }
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationKind {
    Station,
    Shipyard,
    Outpost,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Ammo,
    Shield,
    Repair,
    Upgrades,
    Missions,
    Market,
    Ships,
    Fuel,
}

/// Preisfaktoren eines Ortes (1.0 = Grundpreis aus `shop.ron`).
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Prices {
    /// Ankaufspreis für Erz, pro Sorte. Fehlende Sorten zahlen den Grundpreis.
    pub ore: Vec<(Ore, Fx)>,
    pub fuel: Fx,
    pub service: Fx,
    /// Faktor auf die Dockgebühr (0 = kostenlos).
    pub dock: Fx,
}

impl Default for Prices {
    fn default() -> Self {
        Prices {
            ore: Vec::new(),
            fuel: fx!(1.0),
            service: fx!(1.0),
            dock: fx!(1.0),
        }
    }
}

impl Prices {
    pub fn ore_factor(&self, ore: Ore) -> Fx {
        self.ore
            .iter()
            .find(|(o, _)| *o == ore)
            .map(|(_, f)| *f)
            .unwrap_or(fx!(1.0))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct DecorShip {
    pub ship: String,
    /// Zelle (Spalte, Zeile) im Layout, auf der das Schiff steht.
    pub cell: (i32, i32),
}

#[derive(Deserialize, Clone, Debug)]
pub struct StationDef {
    pub id: String,
    pub name: String,
    pub kind: StationKind,
    pub pos: P,
    pub cell: Fx,
    pub main_color: String,
    pub accent_color: String,
    pub services: Vec<Service>,
    /// Raster: `#` Block, `X` Akzentblock, `W` Fensterblock, `^ v < >` Landeplattform
    /// (Pfeil = Richtung, in die die Plattform zeigt), `L` Leuchtfeuer, `.` leer.
    /// Schrägen (halbe Zelle, Zeichen zeigt die volle Ecke): `/` unten rechts,
    /// `\` unten links, `7` oben rechts, `r` oben links.
    pub layout: Vec<String>,
    #[serde(default)]
    pub decor_ships: Vec<DecorShip>,
    /// Freiliegende Ecken von `#`-Blöcken automatisch abschrägen.
    #[serde(default = "default_true")]
    pub auto_chamfer: bool,
    #[serde(default)]
    pub prices: Prices,
    /// Schiffe, die diese Werft verkauft (Kennungen aus `ships.ron`).
    #[serde(default)]
    pub ships_for_sale: Vec<String>,
    /// Von Anfang an auf der Karte (sonst erst, wenn die Gegend erkundet ist).
    #[serde(default = "default_true")]
    pub known: bool,
    /// Wiederaufbau in Etappen. Raster-Zeichen `1`–`3` sind Blöcke, `a`–`c` Plattformen,
    /// die erst ab dieser Etappe existieren.
    #[serde(default)]
    pub project: Option<ProjectDef>,
    /// Ablagezone für sperrige Bergungsobjekte (Versatz zur Stationsmitte, Radius).
    #[serde(default)]
    pub drop_zone: Option<(P, Fx)>,
    /// Lastaufnahme: U-förmige Halterung, in die Schwerlastkisten gesetzt werden
    /// (Versatz zur Stationsmitte, Richtung der Öffnung in Grad, 0 = +x).
    #[serde(default)]
    pub socket: Option<(P, Fx)>,
    /// Gesicherter Andockport: erst nach einem Hack (Tastenmuster der Slots) anfliegbar.
    #[serde(default)]
    pub hack: Option<HackDef>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct HackDef {
    /// Länge des Musters, Zeit dafür (s), Sperre nach einem Fehler (s), wie lange offen (s).
    pub length: u32,
    pub time: Fx,
    pub lockout: Fx,
    pub open: Fx,
}

/// Wiederaufbau einer Station.
#[derive(Deserialize, Clone, Debug)]
pub struct ProjectDef {
    pub name: String,
    pub stages: Vec<StageDef>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct StageDef {
    pub name: String,
    /// Benötigtes Material (Erz/Schrott in Tonnen).
    pub needs: Vec<(Ore, Fx)>,
    /// Benötigte Bauteile (aus Wracks).
    #[serde(default)]
    pub parts: u32,
    /// Dienste, die nach der Etappe wieder laufen.
    #[serde(default)]
    pub unlocks: Vec<Service>,
    /// Bezahlung der Station für die Etappe.
    #[serde(default)]
    pub reward: u32,
    /// Was sich ändert (wird beim Abschluss gezeigt).
    #[serde(default)]
    pub effect: String,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug)]
pub struct PlanetDef {
    /// Kennung für Verweise aus anderen Daten (z. B. Auftraggeber in `npcs.ron`).
    pub id: String,
    pub name: String,
    pub pos: P,
    pub radius: Fx,
    /// Farbverlauf für die prozedurale Oberfläche.
    pub colors: Vec<String>,
    pub atmosphere: String,
    pub ore: Ore,
    pub deposits: u32,
    pub deposit_amount: Fx,
    /// Winkel (Grad) einer kleinen Landestation auf der Oberfläche.
    #[serde(default)]
    pub outpost_angle: Option<Fx>,
    /// Winkel (Grad) freier Landezonen. Neben jeder liegt ein Erzvorkommen.
    #[serde(default)]
    pub landing_zones: Vec<Fx>,
    /// Preise am Außenposten.
    #[serde(default)]
    pub prices: Prices,
    #[serde(default)]
    pub rings: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FieldDef {
    pub name: String,
    pub center: P,
    pub radius: Fx,
    pub count: u32,
    pub min_radius: Fx,
    pub max_radius: Fx,
    pub ore: Ore,
    pub ore_chance: Fx,
    pub color: String,
    /// Beimischung: Anteil der Erz-Asteroiden, die stattdessen ein anderes Erz tragen.
    #[serde(default)]
    pub mixed: Vec<(Ore, Fx)>,
}

impl FieldDef {
    /// Anteil aller Asteroiden des Felds, die dieses Erz tragen.
    pub fn ore_share(&self, o: Ore) -> Fx {
        let mixed: Fx = self.mixed.iter().map(|(_, c)| c).sum();
        if o == self.ore {
            return self.ore_chance * (fx!(1.0) - mixed).max(fx!(0.0));
        }
        self.mixed
            .iter()
            .filter(|(m, _)| *m == o)
            .map(|(_, c)| self.ore_chance * c)
            .sum()
    }

    /// Erze des Felds, das Haupterz zuerst.
    pub fn ores(&self) -> Vec<Ore> {
        let mut v = vec![self.ore];
        v.extend(
            self.mixed
                .iter()
                .map(|(o, _)| *o)
                .filter(|o| *o != self.ore),
        );
        v
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct MeteorZoneDef {
    pub name: String,
    pub center: P,
    pub radius: Fx,
    pub interval: Fx,
    pub speed: P,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SpinnerDef {
    pub pos: P,
    pub arm_length: Fx,
    pub arm_width: Fx,
    pub arms: u32,
    /// Winkelgeschwindigkeit in Grad pro Sekunde.
    pub speed: Fx,
    pub color: String,
    /// Form: Arme durch die Mitte (Standard) oder ein Ring mit Öffnungen (Wrackring).
    #[serde(default)]
    pub shape: SpinnerShape,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum SpinnerShape {
    #[default]
    Arms,
    /// Ring aus `segments` Platten mit Radius `radius`; `openings` gleichmäßig verteilte
    /// Öffnungen, jede `gap` Segmente breit.
    Ring {
        radius: Fx,
        segments: u32,
        openings: u32,
        gap: u32,
    },
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnomalyKind {
    /// Gravitationsanomalie: zieht an, der Kern beschädigt.
    #[default]
    Anomaly,
    /// Schwarzes Loch: stärkerer Sog, wer den Ereignishorizont (`core_radius`) berührt, ist verloren.
    BlackHole,
}

/// Die einzigen Orte mit Anziehungskraft.
#[derive(Deserialize, Clone, Debug)]
pub struct AnomalyDef {
    pub name: String,
    #[serde(default)]
    pub kind: AnomalyKind,
    pub pos: P,
    pub radius: Fx,
    pub core_radius: Fx,
    pub strength: Fx,
}

/// Ein Wrack zum Ausschlachten: Bohrer gewinnt Schrott, der Kran reißt Bauteile ab.
#[derive(Deserialize, Clone, Debug)]
pub struct WreckDef {
    pub name: String,
    /// Ausrüstung, die in diesem Wrack steckt (der erste abgerissene Teil, solange die Crew sie
    /// noch nicht hat).
    #[serde(default)]
    pub gear: Option<Gear>,
    /// Schiffsmodell, nach dem das Wrack aussieht.
    pub ship: String,
    pub pos: P,
    #[serde(default)]
    pub angle: Fx,
    /// Schrott in Tonnen, den der Bohrer herausholen kann.
    pub scrap: Fx,
    /// Bauteile, die der Kran abreißen kann.
    pub parts: u32,
}

/// Besondere Bedingungen in einem Sektor (wirken mit weichem Rand).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum SectorEffect {
    /// Treibende Trümmer, Dichte 0..1.
    Debris(Fx),
    /// Nebel: Sicht, Radar und Scanner gestört, Stärke 0..1.
    Nebula(Fx),
    /// Sonnenwind: Richtung und Beschleunigung in m/s².
    SolarWind(P, Fx),
}

#[derive(Deserialize, Clone, Debug)]
pub struct RegionDef {
    pub name: String,
    pub center: P,
    pub radius: Fx,
    pub colors: (String, String),
    #[serde(default)]
    pub effect: Option<SectorEffect>,
}

/// Zufallsereignisse im Flug.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct EventsDef {
    /// Abstand zwischen zwei Ereignissen in Sekunden (von, bis).
    pub interval: P,
    pub flare_seconds: Fx,
    pub shower_meteors: u32,
    /// Belohnungsfaktor für spontane Notsignale.
    pub distress_bonus: Fx,
}

impl Default for EventsDef {
    fn default() -> Self {
        EventsDef {
            interval: (fx!(110.0), fx!(220.0)),
            flare_seconds: fx!(20.0),
            shower_meteors: 14,
            distress_bonus: fx!(1.3),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct BackdropDef {
    pub pos: P,
    pub depth: Fx,
    pub radius: Fx,
    pub colors: Vec<String>,
    #[serde(default)]
    pub rings: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WorldDef {
    pub seed: u64,
    pub radius: Fx,
    pub start_station: String,
    pub stations: Vec<StationDef>,
    pub planets: Vec<PlanetDef>,
    pub asteroid_fields: Vec<FieldDef>,
    pub meteor_zones: Vec<MeteorZoneDef>,
    pub spinners: Vec<SpinnerDef>,
    pub anomalies: Vec<AnomalyDef>,
    pub regions: Vec<RegionDef>,
    pub backdrop: Vec<BackdropDef>,
    pub distress_sites: Vec<P>,
    #[serde(default)]
    pub wrecks: Vec<WreckDef>,
    #[serde(default)]
    pub events: EventsDef,
    /// Versteckte Routen (79): sichere Umwege und riskante Abkürzungen.
    #[serde(default)]
    pub routes: Vec<RouteDef>,
    /// Flugassistenz und Bremsassistent des Crew-Schiffs.
    #[serde(default)]
    pub flight: FlightDef,
}

/// Flugassistenz (abschaltbar): weniger Schub pro Tastendruck, sanftes Ausrollen, kräftigere
/// und besser abgefangene Drehung. Bremsassistent (feste Taste für alle): bremst Fahrt und
/// Drehung, kostet Treibstoff.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct FlightDef {
    pub assist_thrust: Fx,
    /// Dämpfung der Fahrt pro Sekunde (Anteil).
    pub assist_linear_damping: Fx,
    /// Zusätzliche Dämpfung der Drehung pro Sekunde, und der Stabilisator obendrauf, solange
    /// kein Triebwerk feuert (fängt Drehungen in etwa einer Sekunde ab).
    pub assist_angular_damping: Fx,
    pub stabilizer: Fx,
    /// Verstärkung des Drehmoments aus versetztem Schub.
    pub assist_turn: Fx,
    /// Bremsassistent: Verzögerung (m/s²), Drehdämpfung pro Sekunde, Treibstoff pro Sekunde.
    pub brake_accel: Fx,
    pub brake_spin: Fx,
    pub brake_fuel: Fx,
    /// Rückwärtsgang (Bremstaste weiter halten, wenn das Schiff steht): Beschleunigung und
    /// Höchstgeschwindigkeit – unter der Landegrenze, damit man rückwärts aufsetzen kann.
    pub reverse_accel: Fx,
    pub reverse_max: Fx,
}

impl Default for FlightDef {
    fn default() -> Self {
        // Runde 4, zweiter Spieltest: lange, langsam ausklingende Drift (Fahrt halbiert sich
        // erst nach ~14 s), nur ein kleiner Stabilisator, schwache Grund-Bremse – mehr Bremskraft
        // gibt es als Upgrade (Bremsdüsen).
        FlightDef {
            assist_thrust: fx!(0.6),
            assist_linear_damping: fx!(0.05),
            assist_angular_damping: fx!(0.35),
            stabilizer: fx!(0.6),
            assist_turn: fx!(1.8),
            brake_accel: fx!(2.0),
            brake_spin: fx!(1.2),
            brake_fuel: fx!(1.0),
            reverse_accel: fx!(0.8),
            reverse_max: fx!(1.2),
        }
    }
}

/// Eine Route durch die Welt, die erst entdeckt werden muss (Scanner, Gerüchte an einer Station,
/// reparierter Sender).
#[derive(Deserialize, Clone, Debug)]
pub struct RouteDef {
    pub id: String,
    pub name: String,
    pub kind: RouteKind,
    pub points: Vec<P>,
    /// Hinweis fürs Logbuch.
    pub hint: String,
    /// Station, an der Piloten davon erzählen (beim Andocken).
    #[serde(default)]
    pub known_at: Option<String>,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    /// Kürzer, aber durch Gefahr (Asteroiden, Meteore, Sog).
    Shortcut,
    /// Länger, aber an der Gefahr vorbei.
    Detour,
}

impl RouteKind {
    pub fn label(self) -> &'static str {
        match self {
            RouteKind::Shortcut => "Abkürzung",
            RouteKind::Detour => "Sicherer Umweg",
        }
    }
}

/// Ausrüstung, die man erst kaufen oder finden muss. Bohrer und Scanner sind immer an Bord.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gear {
    Crane,
    Cannon,
    Shield,
}

impl Gear {
    pub const ALL: [Gear; 3] = [Gear::Crane, Gear::Cannon, Gear::Shield];
    pub fn label(self) -> &'static str {
        match self {
            Gear::Crane => "Kran",
            Gear::Cannon => "Kanone",
            Gear::Shield => "Schildgenerator",
        }
    }
    /// Welches Werkzeug diese Ausrüstung freischaltet.
    pub fn tool(self) -> Option<ToolKind> {
        match self {
            Gear::Crane => Some(ToolKind::Crane),
            Gear::Cannon => Some(ToolKind::Cannon),
            Gear::Shield => None,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct GearDef {
    pub gear: Gear,
    pub price: u32,
    pub description: String,
}

/// Auswirkung eines erledigten Auftrags auf die Umgebung einer Station (76).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    /// Versorgt: Munition und Reparatur dort gratis.
    Supplied,
    /// Geräumt: kaum Trümmer, keine Meteore im Umkreis.
    Cleared,
    /// Sender repariert: Routen in der Nähe bekannt, Scanner reicht weiter.
    Beacon,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EffectSave {
    pub kind: EffectKind,
    pub station: String,
    pub left: Fx,
}

// ---------------------------------------------------------------------------
// Shop
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum ServiceEffect {
    Ammo(u32),
    ShieldFull,
    RepairFull,
    /// Alle Triebwerke instand setzen.
    RepairThrusters,
    /// Volltanken (Preis anteilig zur fehlenden Menge).
    Refuel,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ServiceItem {
    pub id: String,
    pub name: String,
    pub price: u32,
    pub effect: ServiceEffect,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum UpgradeEffect {
    ThrustMul(Fx),
    MaxHull(Fx),
    MaxShield(Fx),
    Cargo(Fx),
    Gyro(Fx),
    MaxAmmo(u32),
    CraneRange(Fx),
    DrillRate(Fx),
    /// Tank vergrößern (Anteil).
    FuelTank(Fx),
    /// Scanner-Reichweite (Faktor).
    ScanRange(Fx),
    /// Reparaturdrohnen: Hülle flickt sich im Flug (Punkte pro Sekunde).
    RepairDrones(Fx),
    /// Treibstoffverbrauch (Faktor).
    FuelBurn(Fx),
    /// Kanone: Nachladezeit (Faktor, < 1 = schneller) und Schaden (Faktor).
    CannonRate(Fx),
    CannonDamage(Fx),
    /// Kran: Tragkraft – das Seil hält mehr aus (Faktor).
    CraneLoad(Fx),
    /// Bremsdüsen: Bremskraft und Rückwärtsgang (Faktor).
    Brake(Fx),
}

/// Welches Schiffsteil ein Upgrade betrifft (dort sitzt auch die Zusatzmasse).
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UpgradePart {
    Thrusters,
    Hull,
    Shield,
    Cargo,
    Crane,
    Drill,
    Cannon,
    Scanner,
    Tank,
    #[default]
    Systems,
}

impl UpgradePart {
    pub fn label(self) -> &'static str {
        match self {
            UpgradePart::Thrusters => "Triebwerke",
            UpgradePart::Hull => "Hülle",
            UpgradePart::Shield => "Schild",
            UpgradePart::Cargo => "Frachtraum",
            UpgradePart::Crane => "Kran",
            UpgradePart::Drill => "Bohrer",
            UpgradePart::Cannon => "Kanone",
            UpgradePart::Scanner => "Scanner",
            UpgradePart::Tank => "Tank",
            UpgradePart::Systems => "Bordsysteme",
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct UpgradeDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub price: u32,
    pub effect: UpgradeEffect,
    #[serde(default)]
    pub requires: Option<String>,
    /// Betroffenes Schiffsteil und Stufe (1–3).
    #[serde(default)]
    pub part: UpgradePart,
    #[serde(default = "default_tier")]
    pub tier: u8,
    /// Nachteil: Zusatzmasse pro betroffenem Teil (t), dazu ein kurzer Text.
    #[serde(default)]
    pub mass: Fx,
    #[serde(default)]
    pub drawback: String,
    /// Kosten außer Credits: Material aus dem Crew-Lager, Bauteile, ein Artefakt.
    #[serde(default)]
    pub materials: Vec<(Ore, Fx)>,
    #[serde(default)]
    pub parts: u32,
    #[serde(default)]
    pub artifact: bool,
    /// Schlüssel: dieses Artefakt muss in der Sammlung der Crew sein (wird nicht verbraucht).
    #[serde(default)]
    pub key: Option<String>,
    /// Nur bei dieser Person zu haben (Kennung aus npcs.ron) – an ihrem Ort.
    #[serde(default)]
    pub vendor: Option<String>,
}

fn default_tier() -> u8 {
    1
}

#[derive(Deserialize, Clone, Debug)]
pub struct PaintDef {
    pub name: String,
    pub color: String,
}

/// Flammenfarbe: `None` = Slotfarben (zeigt, wer schiebt).
#[derive(Deserialize, Clone, Debug)]
pub struct FlameDef {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ShopDef {
    pub start_credits: u32,
    /// Ausrüstung zum Kaufen in Werften (Kran, Kanone, Schildgenerator).
    #[serde(default)]
    pub gear: Vec<GearDef>,
    /// Bergungskosten nach Zerstörung: fester Betrag + Anteil der Kasse (höchstens die Kasse).
    pub respawn_fee: Fx,
    #[serde(default)]
    pub salvage_base: u32,
    pub services: Vec<ServiceItem>,
    pub upgrades: Vec<UpgradeDef>,
    pub ore_prices: Vec<(Ore, u32)>,
    /// Lackiererei: Farben für Rumpf und Akzent, Flammenfarben, Preis pro Änderung.
    #[serde(default)]
    pub paints: Vec<PaintDef>,
    #[serde(default)]
    pub flames: Vec<FlameDef>,
    #[serde(default)]
    pub paint_price: u32,
    /// Namen für Bauteile aus Wracks und ihr Wert (Spanne).
    #[serde(default)]
    pub salvage_names: Vec<String>,
    #[serde(default)]
    pub salvage_value: (u32, u32),
    /// Preis pro neu kartierter Rasterzelle (Kartendaten).
    #[serde(default = "default_chart_price")]
    pub chart_price: u32,
    /// Dockgebühr, Versicherung, Kredit, Marktschwankungen.
    #[serde(default)]
    pub finance: FinanceDef,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct FinanceDef {
    /// Dockgebühr in Credits (mal Ortsfaktor `prices.dock`, minus 20 % pro Rufstufe).
    pub dock_fee: u32,
    /// Wer innerhalb dieser Zeit wieder an derselben Station andockt, zahlt nicht noch einmal.
    pub dock_grace: Fx,
    /// Versicherung: Anteil jeder Auftragsbelohnung (mindestens `premium_min`), übernimmt
    /// `coverage` der Bergungskosten.
    pub premium_share: Fx,
    pub premium_min: u32,
    pub coverage: Fx,
    /// Schiffskredit: Anzahlung (Anteil), Zinsen auf den Rest, Zahl der Raten.
    pub down_payment: Fx,
    pub interest: Fx,
    pub installments: u32,
    /// Markt: Preisrückgang pro verkaufter Tonne, Untergrenze, Erholungszeit (s, exponentiell).
    pub drop_per_t: Fx,
    pub floor: Fx,
    pub recovery: Fx,
    /// Nachfrage: Abstand zwischen zwei Nachfragen (s, von–bis), Preisfaktor, Dauer, Anstieg (s).
    pub demand_interval: (Fx, Fx),
    pub demand_factor: Fx,
    pub demand_seconds: Fx,
    pub demand_rise: Fx,
}

impl Default for FinanceDef {
    fn default() -> Self {
        FinanceDef {
            dock_fee: 12,
            dock_grace: fx!(120.0),
            premium_share: fx!(0.08),
            premium_min: 10,
            coverage: fx!(0.7),
            down_payment: fx!(0.25),
            interest: fx!(0.12),
            installments: 8,
            drop_per_t: fx!(0.035),
            floor: fx!(0.55),
            recovery: fx!(150.0),
            demand_interval: (fx!(240.0), fx!(420.0)),
            demand_factor: fx!(1.35),
            demand_seconds: fx!(300.0),
            demand_rise: fx!(20.0),
        }
    }
}

fn default_chart_price() -> u32 {
    3
}

impl ShopDef {
    pub fn ore_price(&self, ore: Ore) -> u32 {
        self.ore_prices
            .iter()
            .find(|(o, _)| *o == ore)
            .map(|(_, p)| *p)
            .unwrap_or(5)
    }
}

// ---------------------------------------------------------------------------
// Missionen
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug)]
pub struct CargoTemplate {
    pub name: String,
    pub mass: Fx,
    pub reward: u32,
    /// Zu schwer für den Frachtraum: wird als Kiste am Kran geschleppt.
    #[serde(default)]
    pub towed: bool,
    /// Flugeigenschaften der Fracht (Tank, empfindlich, instabil).
    #[serde(default)]
    pub traits: CargoTrait,
}

/// Flugeigenschaften eines Frachtstücks (Punkt 68).
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CargoTrait {
    #[default]
    None,
    /// Flüssigkeit: schwappt bei Manövern verzögert nach.
    Tank,
    /// Empfindlich: harte Stöße kosten Zustand (und damit Lohn).
    Fragile,
    /// Instabil: harte Beschleunigung baut Belastung auf, zu viel lässt es verpuffen.
    Unstable,
}

impl CargoTrait {
    pub fn label(self) -> &'static str {
        match self {
            CargoTrait::None => "",
            CargoTrait::Tank => "Tank – schwappt bei Manövern nach",
            CargoTrait::Fragile => "Empfindlich – harte Stöße kosten Lohn",
            CargoTrait::Unstable => "Instabil – nicht hart beschleunigen",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            CargoTrait::None => "",
            CargoTrait::Tank => "Tank",
            CargoTrait::Fragile => "empfindlich",
            CargoTrait::Unstable => "instabil",
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct MiningTemplate {
    pub ore: Ore,
    pub amount: P,
    pub reward_per_t: Fx,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MissionsDef {
    pub offers_per_station: u32,
    /// Angebote an Planeten-Außenposten (Material verschicken).
    #[serde(default)]
    pub offers_per_outpost: u32,
    /// Belohnung pro Tonne für Lieferungen von Außenposten.
    #[serde(default)]
    pub shipment_per_t: Fx,
    /// Für welche Crewgröße eine Auftragsart gedacht ist (von, bis).
    #[serde(default)]
    pub crew: Vec<(MissionType, (u8, u8))>,
    /// Passagiere: Anzahl (von, bis) und Bezahlung pro Person.
    #[serde(default = "default_passengers")]
    pub passengers: (u32, u32),
    #[serde(default = "default_fare")]
    pub fare_per_person: Fx,
    /// Bonus für Abschluss innerhalb der Richtzeit und für sauberes Fliegen (Anteil der Belohnung).
    #[serde(default = "default_time_bonus")]
    pub time_bonus: Fx,
    #[serde(default = "default_clean_bonus")]
    pub clean_bonus: Fx,
    /// Sperrige Bergungsobjekte.
    #[serde(default)]
    pub bulky: Vec<BulkyTemplate>,
    /// Messflüge (Orte stehen in `courses.ron`).
    #[serde(default)]
    pub survey: SurveyDef,
    /// Geleitschutz und Schmuggel.
    #[serde(default)]
    pub escort: EscortDef,
    #[serde(default)]
    pub smuggle: SmuggleDef,
    /// Rettung havarierter Schiffe (78) und freiwillige Zusatzfunde (77).
    #[serde(default)]
    pub rescue: RescueDef,
    #[serde(default)]
    pub bonus: BonusDef,
    pub distress_offers: u32,
    pub delivery_cargo: Vec<CargoTemplate>,
    pub reward_per_distance: Fx,
    pub mining: Vec<MiningTemplate>,
    pub tow_reward: P,
    pub capsule_reward: P,
    pub capsule_count: (u32, u32),
    pub derelict_names: Vec<String>,
}

/// Messflug: wie viele Felder, wie lange stillhalten, Bezahlung.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SurveyDef {
    pub fields: (u32, u32),
    pub seconds: Fx,
    pub max_speed: Fx,
    pub reward_per_field: Fx,
}

impl Default for SurveyDef {
    fn default() -> Self {
        SurveyDef {
            fields: (1, 2),
            seconds: fx!(6.0),
            max_speed: fx!(0.6),
            reward_per_field: fx!(140.0),
        }
    }
}

/// Geleitschutz: Grundbelohnung und Namen der Frachter.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct EscortDef {
    pub reward: Fx,
    pub names: Vec<String>,
}

impl Default for EscortDef {
    fn default() -> Self {
        EscortDef {
            reward: fx!(320.0),
            names: vec!["Frachter".into()],
        }
    }
}

/// Rettung: havariertes NPC-Schiff mit Besatzung, die an Bord muss (jede Person braucht Platz).
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct RescueDef {
    pub ships: Vec<String>,
    pub captains: Vec<String>,
    pub crew: (u32, u32),
    pub reward_per_person: Fx,
    /// Dank, wenn sich die Geretteten später melden ({ship}, {captain}).
    pub thanks: Vec<String>,
    /// Sekunden bis zur Meldung, Geschenk (Credits, Bauteile).
    pub thanks_after: Fx,
    pub gift_credits: u32,
    pub gift_parts: u32,
}

impl Default for RescueDef {
    fn default() -> Self {
        RescueDef {
            ships: vec!["Kutter Ilse".into()],
            captains: vec!["Kapitänin Ilse Brandt".into()],
            crew: (2, 4),
            reward_per_person: fx!(90.0),
            thanks: vec![
                "Hier {captain} von der {ship}. Ohne euch wären wir noch da draußen.".into(),
            ],
            thanks_after: fx!(240.0),
            gift_credits: 150,
            gift_parts: 2,
        }
    }
}

/// Freiwilliger Zusatzfund am Einsatzort: wertvoll, schwer, manchmal instabil.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct BonusDef {
    pub chance: Fx,
    pub names: Vec<String>,
    pub mass: P,
    pub value: (u32, u32),
    pub unstable_chance: Fx,
}

impl Default for BonusDef {
    fn default() -> Self {
        BonusDef {
            chance: fx!(0.5),
            names: vec!["Energiezelle".into()],
            mass: (fx!(2.0), fx!(4.0)),
            value: (260, 480),
            unstable_chance: fx!(0.4),
        }
    }
}

/// Schmuggel: Ware, Masse, Grundbelohnung, Strafe beim Erwischtwerden, Dauer eines Zollscans.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SmuggleDef {
    pub cargo: Vec<String>,
    pub mass: P,
    pub reward: Fx,
    pub fine: u32,
    pub scan_time: Fx,
}

impl Default for SmuggleDef {
    fn default() -> Self {
        SmuggleDef {
            cargo: Vec::new(),
            mass: (fx!(1.0), fx!(3.0)),
            reward: fx!(380.0),
            fine: 250,
            scan_time: fx!(3.5),
        }
    }
}

fn default_passengers() -> (u32, u32) {
    (2, 6)
}
fn default_fare() -> Fx {
    fx!(45.0)
}
fn default_time_bonus() -> Fx {
    fx!(0.15)
}
fn default_clean_bonus() -> Fx {
    fx!(0.10)
}

// ---------------------------------------------------------------------------
// Funk
// ---------------------------------------------------------------------------

/// Funksprüche (Platzhalter: `{station}`, `{ship}`). Reine Anzeige.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct RadioDef {
    pub approach: Vec<String>,
    pub docked: Vec<String>,
    pub undock: Vec<String>,
    /// Zusätzliche Sprüche einzelner Orte (Station- oder Planeten-Kennung).
    pub places: Vec<RadioPlace>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct RadioPlace {
    pub at: String,
    pub speaker: String,
    pub approach: Vec<String>,
    pub docked: Vec<String>,
}

// ---------------------------------------------------------------------------
// Parcours (Training, Zeitrennen) und Messfelder
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug, Default)]
pub struct CoursesDef {
    pub courses: Vec<CourseDef>,
    /// Orte für Messflüge (Aufträge).
    #[serde(default)]
    pub survey_sites: Vec<SurveySiteDef>,
    /// Strafsekunden pro Kollision.
    #[serde(default = "default_collision_penalty")]
    pub collision_penalty: Fx,
    /// Präzisionsandocken: Strafsekunden pro Meter Versatz und pro m/s Aufsetzgeschwindigkeit.
    #[serde(default = "default_dock_penalty")]
    pub dock_penalty: (Fx, Fx),
    /// So weit darf sich das Schiff vom nächsten Ziel entfernen, sonst endet der Lauf.
    #[serde(default = "default_abort_distance")]
    pub abort_distance: Fx,
    /// Preisgeld beim ersten Erreichen von Bronze, Silber, Gold.
    #[serde(default)]
    pub medal_prizes: (u32, u32, u32),
}

fn default_collision_penalty() -> Fx {
    fx!(2.0)
}
fn default_dock_penalty() -> (Fx, Fx) {
    (fx!(1.5), fx!(1.0))
}
fn default_abort_distance() -> Fx {
    fx!(450.0)
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourseKind {
    /// Schritt für Schritt mit Hinweisen, einmaliger Ausbildungszuschuss.
    Training,
    /// Auf Zeit, mit Medaillen und Bestenliste.
    Race,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CourseDef {
    pub id: String,
    pub name: String,
    pub kind: CourseKind,
    /// Station, an der der Parcours angeboten wird.
    pub station: String,
    #[serde(default)]
    pub intro: String,
    pub steps: Vec<CourseStepDef>,
    /// Medaillenzeiten in Sekunden: Gold, Silber, Bronze.
    #[serde(default)]
    pub medals: Option<(Fx, Fx, Fx)>,
    /// Einmalige Belohnung beim ersten Abschluss.
    #[serde(default)]
    pub prize: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CourseStepDef {
    pub kind: StepKind,
    #[serde(default)]
    pub hint: String,
}

/// Ein Schritt eines Parcours. Positionen in Weltkoordinaten (Meter).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum StepKind {
    /// Durch ein Tor fliegen: Mitte, Flugrichtung in Grad (0 = +x, 90 = +y), Breite.
    Gate { pos: P, dir: Fx, width: Fx },
    /// Einen Punkt erreichen (z. B. das Innere eines Wrackrings).
    Pass { pos: P, radius: Fx },
    /// Nase auf eine Boje richten und die Ausrichtung halten.
    Face {
        pos: P,
        #[serde(default = "default_face_tolerance")]
        tolerance: Fx,
        #[serde(default = "default_face_seconds")]
        seconds: Fx,
    },
    /// Im Messfeld zur Ruhe kommen und stillhalten.
    Hold {
        pos: P,
        radius: Fx,
        seconds: Fx,
        max_speed: Fx,
    },
    /// Andocken; `near` wählt die Plattform der Station, die diesem Punkt am nächsten
    /// liegt – ohne Angabe zählt jede Plattform der Station.
    Dock {
        station: String,
        #[serde(default)]
        near: Option<P>,
    },
}

fn default_face_tolerance() -> Fx {
    fx!(10.0)
}
fn default_face_seconds() -> Fx {
    fx!(1.0)
}

#[derive(Deserialize, Clone, Debug)]
pub struct SurveySiteDef {
    pub name: String,
    pub pos: P,
    pub radius: Fx,
}

// ---------------------------------------------------------------------------
// Verkehr: NPC-Schiffe (Frachter, Drohnen, Schürfroboter, Händlerin, Rivalen), Zoll
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TrafficDef {
    /// Wo NPC-Schiffe andocken: Plattform nächst `near`, Anflug über `via` (Wegpunkte).
    pub docks: Vec<NpcDockDef>,
    pub traders: Vec<TraderDef>,
    pub nests: Vec<NestDef>,
    pub miners: Vec<MinerDef>,
    pub checkpoints: Vec<CheckpointDef>,
    pub merchant: Option<MerchantDef>,
    pub rival: Option<RivalDef>,
    /// Konvoi-Aufträge: Schiff des Frachters, Zahl der Piraten im Hinterhalt.
    pub convoy_ship: String,
    pub ambush: u32,
    /// Rettungsaufträge: Schiff, das havariert am Notrufort treibt.
    pub stranded_ship: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct NpcDockDef {
    pub station: String,
    pub near: P,
    #[serde(default)]
    pub via: Vec<P>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TraderDef {
    pub name: String,
    pub ship: String,
    pub colors: (String, String),
    pub route: Vec<String>,
}

/// Piratennest: so viele Drohnen, solange die Crew in der Nähe ist.
#[derive(Deserialize, Clone, Debug)]
pub struct NestDef {
    pub name: String,
    pub center: P,
    pub radius: Fx,
    pub count: u32,
}

/// Schürfroboter: bauen in einem Asteroidenfeld ab und liefern an einer Station ab.
#[derive(Deserialize, Clone, Debug)]
pub struct MinerDef {
    pub name: String,
    pub field: String,
    pub home: String,
}

/// Zollboje: scannt Schiffe in Reichweite (Schmuggelware).
#[derive(Deserialize, Clone, Debug)]
pub struct CheckpointDef {
    pub name: String,
    pub pos: P,
    pub radius: Fx,
}

/// Wiederkehrende Händlerin mit eigenem Schiff und Spezialsortiment.
#[derive(Deserialize, Clone, Debug)]
pub struct MerchantDef {
    pub name: String,
    pub ship: String,
    pub colors: (String, String),
    pub route: Vec<String>,
    pub goods: Vec<GoodsDef>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GoodsDef {
    pub name: String,
    pub price: u32,
    /// Material ins Crew-Lager (Sorte, t) und/oder Bauteile.
    #[serde(default)]
    pub material: Option<(Ore, Fx)>,
    #[serde(default)]
    pub parts: u32,
}

/// Rivalen-Crew mit eigenem Schiff: jagt dieselben Notrufe.
#[derive(Deserialize, Clone, Debug)]
pub struct RivalDef {
    pub name: String,
    pub ship: String,
    pub colors: (String, String),
    pub home: String,
}

// ---------------------------------------------------------------------------
// Geschichte: Artefakte, Logbuch, Kapitel, Monumente
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct StoryDef {
    pub artifacts: Vec<ArtifactDef>,
    /// Mögliche Fundorte; pro Spielstand wählt der Seed einen je Artefakt.
    pub artifact_sites: Vec<P>,
    pub logs: Vec<LogDef>,
    pub chapters: Vec<ChapterDef>,
    pub monuments: Vec<MonumentDef>,
}

/// Artefakt: einmalig pro Spielstand, mit Masse und Nebenwirkung, solange es an Bord ist.
#[derive(Deserialize, Clone, Debug)]
pub struct ArtifactDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mass: Fx,
    #[serde(default)]
    pub effect: ArtifactEffect,
    pub color: String,
    /// Für den Scanner unsichtbar (kein Signal).
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum ArtifactEffect {
    #[default]
    None,
    /// Stört Radar und Scanner (Anteil 0..1).
    Jam(Fx),
    /// Zieht Piraten an (Nester reagieren auf größere Entfernung, Drohnen greifen früher an).
    Lure,
    /// Wärmt sich auf: Treibstoffverbrauch (Faktor).
    Heat(Fx),
    /// Unruhig: kleine Drehstöße (rad/s² Spitze).
    Spin(Fx),
}

impl ArtifactEffect {
    pub fn label(&self) -> String {
        match self {
            ArtifactEffect::None => "keine spürbare Wirkung".into(),
            ArtifactEffect::Jam(_) => "stört Radar und Scanner".into(),
            ArtifactEffect::Lure => "zieht Piraten an".into(),
            ArtifactEffect::Heat(f) => {
                format!(
                    "heizt den Frachtraum auf: +{:.0} % Treibstoff",
                    (f - fx!(1.0)) * fx!(100.0)
                )
            }
            ArtifactEffect::Spin(_) => "bringt das Schiff ins Trudeln".into(),
        }
    }
}

/// Logbuch-Eintrag und wo er auftaucht.
#[derive(Deserialize, Clone, Debug)]
pub struct LogDef {
    pub id: String,
    pub title: String,
    pub text: String,
    pub source: LogSource,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum LogSource {
    /// Archiv einer Station, beim ersten Andocken.
    Station(String),
    /// Bordbuch in einem Wrack (Name aus world.ron), mit dem Scanner ausgelesen.
    Wreck(String),
    /// Funkfetzen an einem Ort, mit dem Scanner aufgefangen.
    Echo { pos: P, radius: Fx },
    /// Beim Bergen eines Artefakts.
    Artifact(String),
    /// Beim Abschluss eines Kapitels.
    Chapter(usize),
    /// Beim Wecken eines Monuments.
    Monument(String),
}

impl LogSource {
    pub fn category(&self) -> &'static str {
        match self {
            LogSource::Station(_) => "Archiv",
            LogSource::Wreck(_) => "Bordbuch",
            LogSource::Echo { .. } => "Funkfetzen",
            LogSource::Artifact(_) => "Artefakt",
            LogSource::Chapter(_) => "Kapitel",
            LogSource::Monument(_) => "Monument",
        }
    }
}

/// Kapitel des roten Fadens: startet, sobald Ruf und Sammlung reichen, und endet mit dem Ziel.
#[derive(Deserialize, Clone, Debug)]
pub struct ChapterDef {
    pub title: String,
    /// Wer sich meldet und was gesagt wird, wenn das Kapitel beginnt.
    pub speaker: String,
    pub intro: String,
    /// Kurze Zielbeschreibung fürs Logbuch.
    pub task: String,
    /// Voraussetzungen: erledigte Aufträge, Ruf (Summe über alle Stationen), Sammlung.
    #[serde(default)]
    pub missions: u32,
    #[serde(default)]
    pub reputation: u32,
    #[serde(default)]
    pub artifacts: u32,
    pub goal: ChapterGoal,
    pub reward: u32,
    pub outro: String,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum ChapterGoal {
    /// Mit dem Scanner in der Nähe eines Ortes auffangen.
    Scan { pos: P, radius: Fx },
    /// So viele Artefakte in der Sammlung.
    Artifacts(u32),
    /// Ein Monument wecken (Kennung).
    Monument(String),
}

/// Großer fester Bau der Vorgänger. Raster wie bei Stationen: `#` Block, `X` Akzent,
/// `L` Licht, Schrägen `/ \ 7 r`, `o` Tor-Block (verschwindet, wenn das Monument erwacht).
#[derive(Deserialize, Clone, Debug)]
pub struct MonumentDef {
    pub id: String,
    pub name: String,
    pub pos: P,
    pub cell: Fx,
    pub layout: Vec<String>,
    pub main_color: String,
    pub accent_color: String,
    /// So viele Artefakte braucht die Crew, damit ein Scan es weckt.
    pub artifacts: u32,
    /// Was das Wecken bewirkt (Text).
    pub effect: String,
    /// Weckt das Monument, zeigt die Karte die Fundorte der übrigen Artefakte.
    #[serde(default)]
    pub reveals_sites: bool,
}

// ---------------------------------------------------------------------------
// Auftraggeber
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionType {
    Delivery,
    Haul,
    Mining,
    /// Material von einem Planeten-Außenposten zu einer Station verschicken.
    Shipment,
    /// Notruf: Wrack abschleppen.
    Tow,
    /// Notruf: Rettungskapseln einsammeln.
    Capsules,
    /// Passagiere befördern: sanft fliegen, sonst sinkt die Bezahlung.
    Passengers,
    /// Sperriges Bergungsobjekt außen am Kran in die Ablage einer Station bringen.
    Bulky,
    /// Messflug: in Messfeldern zur Ruhe kommen und eine Weile stillhalten.
    Survey,
    /// Konvoi: einen NPC-Frachter sicher von A nach B bringen.
    Escort,
    /// Schmuggel: Ware an den Zollbojen vorbei zum Ziel bringen.
    Smuggle,
    /// Notruf: Besatzung eines havarierten Schiffs an Bord holen (jede Person braucht Platz).
    Rescue,
}

/// Sperriges Bergungsobjekt: Länge (halbe), Dicke, Masse.
#[derive(Deserialize, Clone, Debug)]
pub struct BulkyTemplate {
    pub name: String,
    pub half_len: Fx,
    pub thick: Fx,
    pub mass: Fx,
    pub reward: u32,
}

/// Aussehen des Porträts (wird im Menü aus einfachen Formen gebaut).
#[derive(Deserialize, Clone, Debug)]
pub struct Look {
    pub skin: String,
    pub hair: String,
    pub suit: String,
    #[serde(default)]
    pub helmet: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct NpcDef {
    pub id: String,
    pub name: String,
    pub role: String,
    /// Station- oder Planeten-Kennung.
    pub at: String,
    pub gives: Vec<MissionType>,
    pub look: Look,
    pub lines: Vec<String>,
}

// ---------------------------------------------------------------------------
// Spielstand der Crew
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ProjectSave {
    pub station: String,
    pub stage: u8,
    pub delivered: Vec<(Ore, Fx)>,
    pub parts: u32,
}

/// Lackierung eines Schiffs (Indizes in `shop.ron`; `None` = Werkslack).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Livery {
    pub hull: Option<usize>,
    pub accent: Option<usize>,
    pub flame: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CrewSave {
    pub version: u32,
    pub credits: u32,
    pub upgrades: Vec<String>,
    pub owned_ships: Vec<String>,
    pub current_ship: String,
    pub home_station: String,
    pub missions_done: u32,
    pub ore_sold: Fx,
    /// Ruf pro Station (Kennung, Punkte).
    #[serde(default)]
    pub reputation: Vec<(String, u32)>,
    /// Erkundete Gebiete als Hex-Bitfeld.
    #[serde(default)]
    pub explored: String,
    /// Lackierung pro Schiff.
    #[serde(default)]
    pub liveries: Vec<(String, Livery)>,
    /// Wiederaufbau: Station, erreichte Etappe, schon geliefertes Material und Bauteile.
    #[serde(default)]
    pub projects: Vec<ProjectSave>,
    /// Mit dem Scanner kartierte Gebiete (Hex-Bitfeld) und noch nicht verkaufte Zellen.
    #[serde(default)]
    pub surveyed: String,
    #[serde(default)]
    pub charts_unsold: u32,
    /// Bestenlisten der Parcours (pro Spielstand).
    #[serde(default)]
    pub records: Vec<CourseRecord>,
    /// Finanzen: Versicherung, laufender Schiffskredit, Marktpreise, laufende Nachfrage.
    #[serde(default)]
    pub insured: bool,
    #[serde(default)]
    pub loan: Option<LoanSave>,
    #[serde(default)]
    pub market: Vec<(String, Vec<Fx>)>,
    #[serde(default)]
    pub demand: Option<DemandSave>,
    /// Crew-Lager: Material pro Sorte (t), Bauteile aus Wracks, Artefakte.
    #[serde(default)]
    pub storage: Vec<(Ore, Fx)>,
    #[serde(default)]
    pub storage_parts: u32,
    #[serde(default)]
    pub artifacts: Vec<String>,
    /// Angebaute Module pro Schiff: (Schiff, [(Bauplatz, Modul)]).
    #[serde(default)]
    pub builds: Vec<(String, Vec<(String, String)>)>,
    /// Geschichte: Seed des Spielstands, wo die noch nicht geborgenen Artefakte liegen,
    /// aktuelles Kapitel, gefundene Logbuch-Einträge, geweckte Monumente, Option für 61f.
    #[serde(default)]
    pub story: StorySave,
    /// Abgeworfene Fracht, die noch in der Welt treibt (Erz, Bauteile).
    #[serde(default)]
    pub dropped: Vec<DroppedSave>,
    /// Gerettete Besatzungen, die sich noch melden werden.
    #[serde(default)]
    pub rescued: Vec<RescuedSave>,
    /// Wirkungen erledigter Aufträge, entdeckte Routen und Bestzeiten darauf.
    #[serde(default)]
    pub effects: Vec<EffectSave>,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub route_best: Vec<(String, Fx)>,
    /// Schiffsnamen, Plaketten, Tagebuch, Kartenmarkierungen (81, 82).
    #[serde(default)]
    pub journal: crate::sim::journal::Journal,
    /// Ausrüstung der Crew (None = alter Spielstand: alles an Bord). Munition und Schildladung
    /// werden mitgespeichert, weil sie gekauft werden müssen.
    #[serde(default)]
    pub gear: Option<Vec<Gear>>,
    #[serde(default)]
    pub ammo: Option<u32>,
    #[serde(default)]
    pub shield: Option<Fx>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DroppedSave {
    pub kind: crate::sim::ship::CargoKind,
    pub mass: Fx,
    #[serde(default)]
    pub traits: CargoTrait,
    #[serde(default = "one")]
    pub cond: Fx,
    pub pos: P,
}

fn one() -> Fx {
    fx!(1.0)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RescuedSave {
    pub ship: String,
    pub captain: String,
    /// Sekunden Spielzeit, bis sie sich melden.
    pub wait: Fx,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct StorySave {
    pub seed: u64,
    pub artifact_sites: Vec<(String, P)>,
    pub chapter: usize,
    pub logs: Vec<String>,
    pub monuments: Vec<String>,
    /// Artefakte an Bord bleiben bei Zerstörung im Wrack (sonst zurück an ihren Fundort).
    pub stay_in_wreck: bool,
}

impl Default for StorySave {
    fn default() -> Self {
        StorySave {
            seed: 0,
            artifact_sites: Vec::new(),
            chapter: 0,
            logs: Vec::new(),
            monuments: Vec::new(),
            stay_in_wreck: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LoanSave {
    pub ship: String,
    pub left: u32,
    pub installment: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DemandSave {
    /// Station oder Planet (Kennung), Erz, verbleibende Sekunden.
    pub at: String,
    pub ore: Ore,
    pub left: Fx,
}

/// Bestenliste eines Parcours: die besten Läufe und die beste erreichte Medaille.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CourseRecord {
    pub course: String,
    pub entries: Vec<RecordEntry>,
    /// 0 = keine, 1 = Bronze, 2 = Silber, 3 = Gold.
    #[serde(default)]
    pub medal: u8,
    /// Wie oft der Parcours schon geschafft wurde.
    #[serde(default)]
    pub finished: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RecordEntry {
    /// Wertung in Sekunden (Flugzeit plus Strafzeit).
    pub total: Fx,
    pub penalty: Fx,
    pub ship: String,
    pub crew: u8,
    /// Laufende Nummer des Laufs (zum Wiedererkennen in der Liste).
    #[serde(default)]
    pub run: u32,
}

impl CrewSave {
    pub fn new_game(data: &GameData) -> Self {
        CrewSave {
            version: 1,
            credits: data.shop.start_credits,
            upgrades: Vec::new(),
            owned_ships: vec![data.ships[0].id.clone()],
            current_ship: data.ships[0].id.clone(),
            home_station: data.world.start_station.clone(),
            missions_done: 0,
            ore_sold: fx!(0.0),
            reputation: Vec::new(),
            explored: String::new(),
            liveries: Vec::new(),
            projects: Vec::new(),
            surveyed: String::new(),
            charts_unsold: 0,
            records: Vec::new(),
            insured: false,
            loan: None,
            market: Vec::new(),
            demand: None,
            storage: Vec::new(),
            storage_parts: 0,
            artifacts: Vec::new(),
            builds: Vec::new(),
            story: StorySave::default(),
            dropped: Vec::new(),
            rescued: Vec::new(),
            effects: Vec::new(),
            routes: Vec::new(),
            route_best: Vec::new(),
            journal: Default::default(),
            gear: None,
            ammo: None,
            shield: None,
        }
    }

    /// Neues Spiel mit eigenem Seed für die Geschichte (wo die Artefakte liegen).
    pub fn new_game_seeded(data: &GameData, seed: u64) -> Self {
        let mut s = CrewSave::new_game(data);
        s.story.seed = seed;
        s
    }
}

// ---------------------------------------------------------------------------
// Laden
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct GameData {
    pub ships: Vec<ShipDef>,
    pub world: WorldDef,
    pub shop: ShopDef,
    pub missions: MissionsDef,
    pub npcs: Vec<NpcDef>,
    pub radio: RadioDef,
    pub courses: CoursesDef,
    pub modules: Vec<ModuleDef>,
    pub traffic: TrafficDef,
    pub story: StoryDef,
}

const SHIPS_RON: &str = include_str!("../../assets/data/ships.ron");
const WORLD_RON: &str = include_str!("../../assets/data/world.ron");
const SHOP_RON: &str = include_str!("../../assets/data/shop.ron");
const MISSIONS_RON: &str = include_str!("../../assets/data/missions.ron");
const NPCS_RON: &str = include_str!("../../assets/data/npcs.ron");
const RADIO_RON: &str = include_str!("../../assets/data/radio.ron");
const COURSES_RON: &str = include_str!("../../assets/data/courses.ron");
const MODULES_RON: &str = include_str!("../../assets/data/modules.ron");
const TRAFFIC_RON: &str = include_str!("../../assets/data/traffic.ron");
const STORY_RON: &str = include_str!("../../assets/data/story.ron");

fn read_or(name: &str, embedded: &str) -> String {
    let path = std::path::Path::new("assets/data").join(name);
    std::fs::read_to_string(path).unwrap_or_else(|_| embedded.to_string())
}

fn parse<T: for<'de> Deserialize<'de>>(name: &str, text: &str) -> Result<T, String> {
    ron::from_str(text).map_err(|e| format!("{name}: {e}"))
}

impl GameData {
    /// Lädt die Daten aus `assets/data` (falls vorhanden) oder aus der eingebetteten Kopie.
    pub fn load() -> Result<Self, String> {
        let data = GameData {
            ships: parse("ships.ron", &read_or("ships.ron", SHIPS_RON))?,
            world: parse("world.ron", &read_or("world.ron", WORLD_RON))?,
            shop: parse("shop.ron", &read_or("shop.ron", SHOP_RON))?,
            missions: parse("missions.ron", &read_or("missions.ron", MISSIONS_RON))?,
            npcs: parse("npcs.ron", &read_or("npcs.ron", NPCS_RON))?,
            radio: parse("radio.ron", &read_or("radio.ron", RADIO_RON))?,
            courses: parse("courses.ron", &read_or("courses.ron", COURSES_RON))?,
            modules: parse("modules.ron", &read_or("modules.ron", MODULES_RON))?,
            traffic: parse("traffic.ron", &read_or("traffic.ron", TRAFFIC_RON))?,
            story: parse("story.ron", &read_or("story.ron", STORY_RON))?,
        };
        data.validate()?;
        Ok(data)
    }

    /// Nur die eingebetteten Daten (für Tests, unabhängig vom Arbeitsverzeichnis).
    pub fn embedded() -> Result<Self, String> {
        let data = GameData {
            ships: parse("ships.ron", SHIPS_RON)?,
            world: parse("world.ron", WORLD_RON)?,
            shop: parse("shop.ron", SHOP_RON)?,
            missions: parse("missions.ron", MISSIONS_RON)?,
            npcs: parse("npcs.ron", NPCS_RON)?,
            radio: parse("radio.ron", RADIO_RON)?,
            courses: parse("courses.ron", COURSES_RON)?,
            modules: parse("modules.ron", MODULES_RON)?,
            traffic: parse("traffic.ron", TRAFFIC_RON)?,
            story: parse("story.ron", STORY_RON)?,
        };
        data.validate()?;
        Ok(data)
    }

    fn validate(&self) -> Result<(), String> {
        if self.ships.is_empty() {
            return Err("ships.ron: keine Schiffe".into());
        }
        for s in &self.ships {
            if s.max_thrusters() < s.min_thrusters {
                return Err(format!("Schiff {}: zu wenige Triebwerke", s.id));
            }
        }
        for st in &self.world.stations {
            let w = st.layout.first().map(|r| r.chars().count()).unwrap_or(0);
            if st.layout.iter().any(|r| r.chars().count() != w) {
                return Err(format!(
                    "Station {}: Layout-Zeilen unterschiedlich lang",
                    st.id
                ));
            }
        }
        for st in &self.world.stations {
            for id in &st.ships_for_sale {
                if !self.ships.iter().any(|s| &s.id == id) {
                    return Err(format!("Station {}: unbekanntes Schiff {id}", st.id));
                }
            }
        }
        for n in &self.npcs {
            let known = self.station_index(&n.at).is_some()
                || self.world.planets.iter().any(|p| p.id == n.at);
            if !known {
                return Err(format!(
                    "npcs.ron: {} steht an unbekanntem Ort {}",
                    n.id, n.at
                ));
            }
        }
        for w in &self.world.wrecks {
            if !self.ships.iter().any(|s| s.id == w.ship) {
                return Err(format!(
                    "world.ron: Wrack {} mit unbekanntem Schiff",
                    w.name
                ));
            }
        }
        if self.station_index(&self.world.start_station).is_none() {
            return Err("world.ron: start_station unbekannt".into());
        }
        for c in &self.courses.courses {
            if self.station_index(&c.station).is_none() {
                return Err(format!("courses.ron: {} an unbekannter Station", c.id));
            }
            if !matches!(
                c.steps.first().map(|s| &s.kind),
                Some(StepKind::Gate { .. })
            ) {
                return Err(format!("courses.ron: {} muss mit einem Tor beginnen", c.id));
            }
            for st in &c.steps {
                if let StepKind::Dock { station, .. } = &st.kind
                    && self.station_index(station).is_none()
                {
                    return Err(format!(
                        "courses.ron: {}: unbekannte Station {station}",
                        c.id
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn ship(&self, id: &str) -> &ShipDef {
        self.ships
            .iter()
            .find(|s| s.id == id)
            .unwrap_or(&self.ships[0])
    }

    pub fn module(&self, id: &str) -> Option<&ModuleDef> {
        self.modules.iter().find(|m| m.id == id)
    }

    /// Schiff mit angebauten Modulen: Grundschiff plus ein festes Teil pro belegtem Bauplatz.
    /// Masse, Schwerpunkt und Trägheit ergeben sich daraus wie bei jedem Schiff.
    pub fn built_ship(&self, id: &str, build: &[(String, String)]) -> ShipDef {
        let mut def = self.ship(id).clone();
        for (mount, module) in build {
            let (Some(m), Some(md)) = (
                def.mounts.iter().find(|x| &x.id == mount).cloned(),
                self.module(module),
            ) else {
                continue;
            };
            def.parts.push(PartDef {
                kind: md.part.clone(),
                pos: m.pos,
                size: md.size,
                mass: md.mass,
                dir: fx!(0.0),
                shape: md.shape,
                fixed: true,
            });
            def.max_hull += md.hull;
        }
        def
    }

    pub fn station_index(&self, id: &str) -> Option<usize> {
        self.world.stations.iter().position(|s| s.id == id)
    }

    pub fn upgrade(&self, id: &str) -> Option<&UpgradeDef> {
        self.shop.upgrades.iter().find(|u| u.id == id)
    }
}
