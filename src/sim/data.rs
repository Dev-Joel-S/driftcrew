//! Spieldaten: Schiffe, Welt, Shop und Missionen werden als RON-Dateien beschrieben.
//!
//! Die Dateien liegen unter `assets/data/`. Sie werden zusätzlich in die Binärdatei
//! eingebettet, damit das Spiel auch ohne Asset-Ordner startet. Liegt eine Datei im
//! Ordner, hat sie Vorrang (so lassen sich Werte ohne Neukompilieren ändern).

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

pub type P = (f32, f32);

pub fn v(p: P) -> Vec2 {
    Vec2::new(p.0, p.1)
}

/// Farbe aus "#rrggbb" in lineare-ish sRGB-Komponenten 0..1 (die Umrechnung macht das Rendering).
pub fn hex(s: &str) -> [f32; 3] {
    let s = s.trim_start_matches('#');
    let p = |i: usize| {
        u8::from_str_radix(s.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255) as f32 / 255.0
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

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum PartKind {
    Hull,
    Cockpit,
    Armor,
    /// Frachtmodul mit Kapazität in Tonnen.
    CargoPod(f32),
    /// Triebwerk mit Schubkraft.
    Thruster(f32),
    Tool(ToolKind),
}

#[derive(Deserialize, Clone, Debug)]
pub struct PartDef {
    pub kind: PartKind,
    pub pos: P,
    pub size: P,
    pub mass: f32,
    /// Schubrichtung in Grad relativ zur Schiffsnase (0 = schiebt nach vorne).
    #[serde(default)]
    pub dir: f32,
    /// Umriss des Teils (Optik und Kollision).
    #[serde(default)]
    pub shape: PartShape,
}

/// Umriss eines Schiffsteils. „Oben“ = Richtung Schiffsnase.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum PartShape {
    #[default]
    Box,
    /// Trapez: (Breite oben, Breite unten) als Anteil der vollen Breite.
    Taper(f32, f32),
    /// Abgeschrägte Ecken; Anteil der kürzeren Halbachse.
    Chamfer(f32),
    /// Spitze nach vorne; Anteil der Höhe, den die Spitze einnimmt.
    Nose(f32),
    /// Spitze nach hinten.
    Tail(f32),
    /// Keil zur Seite: (Höhe links, Höhe rechts) als Anteil – für Flügel und Flossen.
    Wing(f32, f32),
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
                let k = c.clamp(0.0, 0.9) * hx.min(hy);
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
                let y = hy - 2.0 * hy * f.clamp(0.05, 0.95);
                vec![
                    Vec2::new(-hx, -hy),
                    Vec2::new(hx, -hy),
                    Vec2::new(hx, y),
                    Vec2::new(0.0, hy),
                    Vec2::new(-hx, y),
                ]
            }
            PartShape::Tail(f) => {
                let y = -hy + 2.0 * hy * f.clamp(0.05, 0.95);
                vec![
                    Vec2::new(0.0, -hy),
                    Vec2::new(hx, y),
                    Vec2::new(hx, hy),
                    Vec2::new(-hx, hy),
                    Vec2::new(-hx, y),
                ]
            }
            PartShape::Wing(left, right) => vec![
                Vec2::new(-hx, -hy),
                Vec2::new(hx, -hy),
                Vec2::new(hx, -hy + 2.0 * hy * right.clamp(0.05, 1.0)),
                Vec2::new(-hx, -hy + 2.0 * hy * left.clamp(0.05, 1.0)),
            ],
        };
        // Doppelte Punkte (z. B. Trapez mit Spitze 0) entfernen.
        pts.dedup_by(|a, b| (*a - *b).length() < 1e-4);
        if pts.len() > 3 && (pts[0] - *pts.last().unwrap()).length() < 1e-4 {
            pts.pop();
        }
        pts
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct ThrusterLayout {
    pub count: u8,
    pub xs: Vec<f32>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ShipDef {
    pub id: String,
    pub name: String,
    pub class: String,
    pub description: String,
    pub price: u32,
    pub hull_color: String,
    pub accent_color: String,
    pub max_hull: f32,
    pub max_shield: f32,
    pub max_ammo: u32,
    #[serde(default = "default_ang_damp")]
    pub angular_damping: f32,
    #[serde(default = "default_min_thrusters")]
    pub min_thrusters: u8,
    /// Schild lädt sich nach `shield_delay` Sekunden ohne Treffer mit `shield_regen` pro Sekunde auf.
    #[serde(default = "default_shield_regen")]
    pub shield_regen: f32,
    #[serde(default = "default_shield_delay")]
    pub shield_delay: f32,
    /// Empfohlene Crewgröße (von, bis). Slots belegen nur Menschen, keine Bots.
    #[serde(default = "default_crew")]
    pub crew: (u8, u8),
    /// Tankgröße (Einheiten).
    #[serde(default = "default_fuel")]
    pub fuel_capacity: f32,
    /// Verbrauch pro Schubeinheit und Sekunde.
    #[serde(default = "default_fuel_burn")]
    pub fuel_burn: f32,
    /// Reihenfolge der Triebwerke = Reihenfolge, in der sie in der Lobby belegt werden.
    pub parts: Vec<PartDef>,
    #[serde(default)]
    pub thruster_layouts: Vec<ThrusterLayout>,
}

fn default_ang_damp() -> f32 {
    0.12
}
fn default_min_thrusters() -> u8 {
    2
}
fn default_shield_regen() -> f32 {
    4.0
}
fn default_shield_delay() -> f32 {
    4.0
}
fn default_crew() -> (u8, u8) {
    (1, 4)
}
fn default_fuel() -> f32 {
    100.0
}
fn default_fuel_burn() -> f32 {
    0.0065
}

impl ShipDef {
    pub fn thruster_parts(&self) -> impl Iterator<Item = (usize, &PartDef, f32)> {
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
    pub fn cargo_capacity(&self) -> f32 {
        self.parts
            .iter()
            .map(|p| match p.kind {
                PartKind::CargoPod(c) => c,
                _ => 0.0,
            })
            .sum()
    }
    /// x-Positionen der Triebwerke, wenn `n` belegt sind (symmetrisch neu angeordnet).
    pub fn thruster_xs(&self, n: u8) -> Vec<f32> {
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
            .fold(0.0f32, f32::max);
        if n <= 1 {
            return vec![0.0];
        }
        (0..n)
            .map(|i| -span + 2.0 * span * i as f32 / (n - 1) as f32)
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
    pub fn color(self) -> [f32; 3] {
        match self {
            Ore::Ferrit => [1.0, 0.55, 0.25],
            Ore::Kobalt => [0.25, 0.55, 1.0],
            Ore::Solarit => [1.0, 0.85, 0.15],
            Ore::Ionit => [0.2, 1.0, 0.85],
            Ore::Schrott => [0.72, 0.66, 0.58],
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
    pub ore: Vec<(Ore, f32)>,
    pub fuel: f32,
    pub service: f32,
}

impl Default for Prices {
    fn default() -> Self {
        Prices {
            ore: Vec::new(),
            fuel: 1.0,
            service: 1.0,
        }
    }
}

impl Prices {
    pub fn ore_factor(&self, ore: Ore) -> f32 {
        self.ore
            .iter()
            .find(|(o, _)| *o == ore)
            .map(|(_, f)| *f)
            .unwrap_or(1.0)
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
    pub cell: f32,
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
    pub drop_zone: Option<(P, f32)>,
    /// Lastaufnahme: U-förmige Halterung, in die Schwerlastkisten gesetzt werden
    /// (Versatz zur Stationsmitte, Richtung der Öffnung in Grad, 0 = +x).
    #[serde(default)]
    pub socket: Option<(P, f32)>,
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
    pub needs: Vec<(Ore, f32)>,
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
    pub radius: f32,
    /// Farbverlauf für die prozedurale Oberfläche.
    pub colors: Vec<String>,
    pub atmosphere: String,
    pub ore: Ore,
    pub deposits: u32,
    pub deposit_amount: f32,
    /// Winkel (Grad) einer kleinen Landestation auf der Oberfläche.
    #[serde(default)]
    pub outpost_angle: Option<f32>,
    /// Winkel (Grad) freier Landezonen. Neben jeder liegt ein Erzvorkommen.
    #[serde(default)]
    pub landing_zones: Vec<f32>,
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
    pub radius: f32,
    pub count: u32,
    pub min_radius: f32,
    pub max_radius: f32,
    pub ore: Ore,
    pub ore_chance: f32,
    pub color: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MeteorZoneDef {
    pub name: String,
    pub center: P,
    pub radius: f32,
    pub interval: f32,
    pub speed: P,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SpinnerDef {
    pub pos: P,
    pub arm_length: f32,
    pub arm_width: f32,
    pub arms: u32,
    /// Winkelgeschwindigkeit in Grad pro Sekunde.
    pub speed: f32,
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
        radius: f32,
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
    pub radius: f32,
    pub core_radius: f32,
    pub strength: f32,
}

/// Ein Wrack zum Ausschlachten: Bohrer gewinnt Schrott, der Kran reißt Bauteile ab.
#[derive(Deserialize, Clone, Debug)]
pub struct WreckDef {
    pub name: String,
    /// Schiffsmodell, nach dem das Wrack aussieht.
    pub ship: String,
    pub pos: P,
    #[serde(default)]
    pub angle: f32,
    /// Schrott in Tonnen, den der Bohrer herausholen kann.
    pub scrap: f32,
    /// Bauteile, die der Kran abreißen kann.
    pub parts: u32,
}

/// Besondere Bedingungen in einem Sektor (wirken mit weichem Rand).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum SectorEffect {
    /// Treibende Trümmer, Dichte 0..1.
    Debris(f32),
    /// Nebel: Sicht, Radar und Scanner gestört, Stärke 0..1.
    Nebula(f32),
    /// Sonnenwind: Richtung und Beschleunigung in m/s².
    SolarWind(P, f32),
}

#[derive(Deserialize, Clone, Debug)]
pub struct RegionDef {
    pub name: String,
    pub center: P,
    pub radius: f32,
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
    pub flare_seconds: f32,
    pub shower_meteors: u32,
    /// Belohnungsfaktor für spontane Notsignale.
    pub distress_bonus: f32,
}

impl Default for EventsDef {
    fn default() -> Self {
        EventsDef {
            interval: (110.0, 220.0),
            flare_seconds: 20.0,
            shower_meteors: 14,
            distress_bonus: 1.3,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct BackdropDef {
    pub pos: P,
    pub depth: f32,
    pub radius: f32,
    pub colors: Vec<String>,
    #[serde(default)]
    pub rings: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WorldDef {
    pub seed: u64,
    pub radius: f32,
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
    ThrustMul(f32),
    MaxHull(f32),
    MaxShield(f32),
    Cargo(f32),
    Gyro(f32),
    MaxAmmo(u32),
    CraneRange(f32),
    DrillRate(f32),
    /// Tank vergrößern (Anteil).
    FuelTank(f32),
    /// Scanner-Reichweite (Faktor).
    ScanRange(f32),
    /// Reparaturdrohnen: Hülle flickt sich im Flug (Punkte pro Sekunde).
    RepairDrones(f32),
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
    /// Bergungskosten nach Zerstörung: fester Betrag + Anteil der Kasse (höchstens die Kasse).
    pub respawn_fee: f32,
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
    pub mass: f32,
    pub reward: u32,
    /// Zu schwer für den Frachtraum: wird als Kiste am Kran geschleppt.
    #[serde(default)]
    pub towed: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MiningTemplate {
    pub ore: Ore,
    pub amount: P,
    pub reward_per_t: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MissionsDef {
    pub offers_per_station: u32,
    /// Angebote an Planeten-Außenposten (Material verschicken).
    #[serde(default)]
    pub offers_per_outpost: u32,
    /// Belohnung pro Tonne für Lieferungen von Außenposten.
    #[serde(default)]
    pub shipment_per_t: f32,
    /// Für welche Crewgröße eine Auftragsart gedacht ist (von, bis).
    #[serde(default)]
    pub crew: Vec<(MissionType, (u8, u8))>,
    /// Passagiere: Anzahl (von, bis) und Bezahlung pro Person.
    #[serde(default = "default_passengers")]
    pub passengers: (u32, u32),
    #[serde(default = "default_fare")]
    pub fare_per_person: f32,
    /// Bonus für Abschluss innerhalb der Richtzeit und für sauberes Fliegen (Anteil der Belohnung).
    #[serde(default = "default_time_bonus")]
    pub time_bonus: f32,
    #[serde(default = "default_clean_bonus")]
    pub clean_bonus: f32,
    /// Sperrige Bergungsobjekte.
    #[serde(default)]
    pub bulky: Vec<BulkyTemplate>,
    /// Messflüge (Orte stehen in `courses.ron`).
    #[serde(default)]
    pub survey: SurveyDef,
    pub distress_offers: u32,
    pub delivery_cargo: Vec<CargoTemplate>,
    pub reward_per_distance: f32,
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
    pub seconds: f32,
    pub max_speed: f32,
    pub reward_per_field: f32,
}

impl Default for SurveyDef {
    fn default() -> Self {
        SurveyDef {
            fields: (1, 2),
            seconds: 6.0,
            max_speed: 0.6,
            reward_per_field: 140.0,
        }
    }
}

fn default_passengers() -> (u32, u32) {
    (2, 6)
}
fn default_fare() -> f32 {
    45.0
}
fn default_time_bonus() -> f32 {
    0.15
}
fn default_clean_bonus() -> f32 {
    0.10
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
    pub collision_penalty: f32,
    /// Präzisionsandocken: Strafsekunden pro Meter Versatz und pro m/s Aufsetzgeschwindigkeit.
    #[serde(default = "default_dock_penalty")]
    pub dock_penalty: (f32, f32),
    /// So weit darf sich das Schiff vom nächsten Ziel entfernen, sonst endet der Lauf.
    #[serde(default = "default_abort_distance")]
    pub abort_distance: f32,
    /// Preisgeld beim ersten Erreichen von Bronze, Silber, Gold.
    #[serde(default)]
    pub medal_prizes: (u32, u32, u32),
}

fn default_collision_penalty() -> f32 {
    2.0
}
fn default_dock_penalty() -> (f32, f32) {
    (1.5, 1.0)
}
fn default_abort_distance() -> f32 {
    450.0
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
    pub medals: Option<(f32, f32, f32)>,
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
    Gate { pos: P, dir: f32, width: f32 },
    /// Einen Punkt erreichen (z. B. das Innere eines Wrackrings).
    Pass { pos: P, radius: f32 },
    /// Nase auf eine Boje richten und die Ausrichtung halten.
    Face {
        pos: P,
        #[serde(default = "default_face_tolerance")]
        tolerance: f32,
        #[serde(default = "default_face_seconds")]
        seconds: f32,
    },
    /// Im Messfeld zur Ruhe kommen und stillhalten.
    Hold {
        pos: P,
        radius: f32,
        seconds: f32,
        max_speed: f32,
    },
    /// Andocken; `near` wählt die Plattform der Station, die diesem Punkt am nächsten
    /// liegt – ohne Angabe zählt jede Plattform der Station.
    Dock {
        station: String,
        #[serde(default)]
        near: Option<P>,
    },
}

fn default_face_tolerance() -> f32 {
    10.0
}
fn default_face_seconds() -> f32 {
    1.0
}

#[derive(Deserialize, Clone, Debug)]
pub struct SurveySiteDef {
    pub name: String,
    pub pos: P,
    pub radius: f32,
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
}

/// Sperriges Bergungsobjekt: Länge (halbe), Dicke, Masse.
#[derive(Deserialize, Clone, Debug)]
pub struct BulkyTemplate {
    pub name: String,
    pub half_len: f32,
    pub thick: f32,
    pub mass: f32,
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
    pub delivered: Vec<(Ore, f32)>,
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
    pub ore_sold: f32,
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
    pub total: f32,
    pub penalty: f32,
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
            ore_sold: 0.0,
            reputation: Vec::new(),
            explored: String::new(),
            liveries: Vec::new(),
            projects: Vec::new(),
            surveyed: String::new(),
            charts_unsold: 0,
            records: Vec::new(),
        }
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
}

const SHIPS_RON: &str = include_str!("../../assets/data/ships.ron");
const WORLD_RON: &str = include_str!("../../assets/data/world.ron");
const SHOP_RON: &str = include_str!("../../assets/data/shop.ron");
const MISSIONS_RON: &str = include_str!("../../assets/data/missions.ron");
const NPCS_RON: &str = include_str!("../../assets/data/npcs.ron");
const RADIO_RON: &str = include_str!("../../assets/data/radio.ron");
const COURSES_RON: &str = include_str!("../../assets/data/courses.ron");

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
        };
        data.validate()?;
        Ok(data)
    }

    /// Nur die eingebetteten Daten (für Tests, unabhängig vom Arbeitsverzeichnis).
    #[cfg(test)]
    pub fn embedded() -> Result<Self, String> {
        let data = GameData {
            ships: parse("ships.ron", SHIPS_RON)?,
            world: parse("world.ron", WORLD_RON)?,
            shop: parse("shop.ron", SHOP_RON)?,
            missions: parse("missions.ron", MISSIONS_RON)?,
            npcs: parse("npcs.ron", NPCS_RON)?,
            radio: parse("radio.ron", RADIO_RON)?,
            courses: parse("courses.ron", COURSES_RON)?,
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

    pub fn station_index(&self, id: &str) -> Option<usize> {
        self.world.stations.iter().position(|s| s.id == id)
    }

    pub fn upgrade(&self, id: &str) -> Option<&UpgradeDef> {
        self.shop.upgrades.iter().find(|u| u.id == id)
    }
}
