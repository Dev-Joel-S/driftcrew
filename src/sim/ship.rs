//! Das Crew-Schiff: aus Daten gebaut, Masse/Schwerpunkt/Trägheit aus den Teilen berechnet.

use crate::sim::fx::{Fx, Vec2};

use super::data::{CargoTrait, Ore, PartKind, PartShape, ShipDef, ToolKind, UpgradePart, v};
use super::geom::{Poly, rot};

/// Was die Lobby belegt hat: Anzahl Triebwerke und welche Werkzeuge (Index in `tool_parts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loadout {
    pub thrusters: u8,
    pub tools: Vec<usize>,
}

impl Loadout {
    pub fn full(def: &ShipDef) -> Loadout {
        Loadout {
            thrusters: def.max_thrusters(),
            tools: (0..def.tool_parts().count()).collect(),
        }
    }
}

/// Werte aus gekauften Upgrades.
#[derive(Clone, Debug)]
pub struct ShipStats {
    pub thrust_mul: Fx,
    pub hull_bonus: Fx,
    pub shield_bonus: Fx,
    pub cargo_mul: Fx,
    pub gyro: Fx,
    pub ammo_bonus: u32,
    pub crane_mul: Fx,
    pub drill_mul: Fx,
    pub fuel_mul: Fx,
    pub scan_mul: Fx,
    pub hull_regen: Fx,
    pub fuel_burn_mul: Fx,
    pub cannon_rate: Fx,
    pub cannon_damage: Fx,
    pub crane_load: Fx,
    /// Bremsdüsen: Faktor auf Bremse und Rückwärtsgang.
    pub brake_mul: Fx,
    /// Nachteil der Upgrades: Zusatzmasse pro betroffenem Teil.
    pub part_mass: Vec<(UpgradePart, Fx)>,
}

impl Default for ShipStats {
    fn default() -> Self {
        ShipStats {
            thrust_mul: fx!(1.0),
            hull_bonus: fx!(0.0),
            shield_bonus: fx!(0.0),
            cargo_mul: fx!(1.0),
            gyro: fx!(0.0),
            ammo_bonus: 0,
            crane_mul: fx!(1.0),
            drill_mul: fx!(1.0),
            fuel_mul: fx!(1.0),
            scan_mul: fx!(1.0),
            hull_regen: fx!(0.0),
            fuel_burn_mul: fx!(1.0),
            cannon_rate: fx!(1.0),
            cannon_damage: fx!(1.0),
            crane_load: fx!(1.0),
            brake_mul: fx!(1.0),
            part_mass: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShipPart {
    pub kind: PartKind,
    /// Position relativ zum Schiffsursprung (nicht zum Schwerpunkt).
    pub pos: Vec2,
    pub half: Vec2,
    pub mass: Fx,
    pub shape: PartShape,
}

#[derive(Clone, Debug)]
pub struct Thruster {
    pub slot: u8,
    /// Index des Teils in der Schiffsdefinition (bleibt beim Umbau gleich).
    pub part: usize,
    pub pos: Vec2,
    pub dir: Vec2,
    pub thrust: Fx,
    pub firing: bool,
    /// Weiche Anzeige 0..1 für Flamme/Sound.
    pub level: Fx,
    /// Zustand 0..1. Unter [`STUTTER_BELOW`] stottert das Triebwerk, bei 0 fällt es aus.
    pub health: Fx,
    /// Übersteuerung (69): aktiv, Hitze 0..1, Abkühlpause nach Notabschaltung, Warnstufe,
    /// Tastenverlauf für den Doppeltipp (Ticks gehalten, seit dem Loslassen, letzter war Tipp).
    pub overdrive: bool,
    pub heat: Fx,
    pub cool_off: Fx,
    pub heat_warn: u8,
    pub held: u32,
    pub since_release: u32,
    pub tap: bool,
}

/// Ab diesem Zustand setzt ein Triebwerk zufällig aus.
pub const STUTTER_BELOW: Fx = fx!(0.6);

impl Thruster {
    pub fn failed(&self) -> bool {
        self.health <= fx!(0.0)
    }
    pub fn stuttering(&self) -> bool {
        !self.failed() && self.health < STUTTER_BELOW
    }
    /// Schub, den das Triebwerk in seinem Zustand noch liefert.
    pub fn effective_thrust(&self) -> Fx {
        if self.failed() {
            fx!(0.0)
        } else {
            self.thrust * (fx!(0.7) + fx!(0.3) * self.health)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CraneState {
    Idle,
    Extending {
        len: Fx,
        dir: Vec2,
    },
    Retracting {
        len: Fx,
        dir: Vec2,
    },
    /// `local`: Angriffspunkt am Körper (lokal, mitgedreht) – lange Teile pendeln am Ende.
    Attached {
        body: u32,
        rope: Fx,
        local: Vec2,
    },
    /// Notreparatur: der Kran hält ein Ersatzteil an ein ausgefallenes Triebwerk.
    Patching {
        thruster: usize,
        progress: Fx,
    },
}

#[derive(Clone, Debug)]
pub struct DrillHit {
    pub point: Vec2,
    pub ore: Option<Ore>,
    /// Ertragsfaktor durch die Zielhand (siehe [`steady_factor`]).
    pub steady: Fx,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub slot: u8,
    pub kind: ToolKind,
    pub pos: Vec2,
    /// Zielwinkel in Weltkoordinaten (0 = +x).
    pub aim: Fx,
    pub pressed: bool,
    pub prev_pressed: bool,
    pub cooldown: Fx,
    pub crane: CraneState,
    pub drill: Option<DrillHit>,
    /// Seilbelastung 0..1 (1 = reißt gleich), geglättet für die Anzeige.
    pub strain: Fx,
    /// Wie unruhig die Zielhand ist: geglättete Drehgeschwindigkeit des Zielwinkels (rad/s).
    pub jitter: Fx,
}

/// Upgrade-Masse auf die betroffenen Teile verteilen. Triebwerke, Fracht und Werkzeuge
/// bekommen sie je Teil; Hülle, Schild und Bordsysteme sitzen im Rumpf; ein größerer Tank
/// sitzt hinten bei den Triebwerken und zieht den Schwerpunkt nach hinten.
fn add_upgrade_mass(parts: &mut [ShipPart], up: UpgradePart, m: Fx) {
    let each = |k: &PartKind| match up {
        UpgradePart::Thrusters => matches!(k, PartKind::Thruster(_)),
        UpgradePart::Cargo => matches!(k, PartKind::CargoPod(_)),
        UpgradePart::Crane => *k == PartKind::Tool(ToolKind::Crane),
        UpgradePart::Drill => *k == PartKind::Tool(ToolKind::Drill),
        UpgradePart::Cannon => *k == PartKind::Tool(ToolKind::Cannon),
        UpgradePart::Scanner => *k == PartKind::Tool(ToolKind::Scanner),
        _ => false,
    };
    if up == UpgradePart::Tank {
        let n = parts
            .iter()
            .filter(|p| matches!(p.kind, PartKind::Thruster(_)))
            .count();
        for p in parts.iter_mut() {
            if matches!(p.kind, PartKind::Thruster(_)) {
                p.mass += m / Fx::from(n.max(1));
            }
        }
        return;
    }
    let mut hit = false;
    for p in parts.iter_mut() {
        if each(&p.kind) {
            p.mass += m;
            hit = true;
        }
    }
    if !hit && let Some(p) = parts.iter_mut().find(|p| p.kind == PartKind::Hull) {
        p.mass += m;
    }
}

/// Ertragsfaktor beim Bohren je nach Ruhe der Zielhand: ruhig mehr, zittrig weniger.
pub fn steady_factor(jitter: Fx) -> Fx {
    let k = ((jitter - fx!(0.25)) / fx!(1.75)).clamp(fx!(0.0), fx!(1.0));
    fx!(1.25) - fx!(0.55) * k
}

impl Tool {
    pub fn just_pressed(&self) -> bool {
        self.pressed && !self.prev_pressed
    }
    pub fn aim_dir(&self) -> Vec2 {
        Vec2::new(self.aim.cos(), self.aim.sin())
    }
}

#[derive(Clone, Debug)]
pub struct CargoPod {
    pub pos: Vec2,
    pub half: Vec2,
    pub capacity: Fx,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CargoKind {
    Ore(Ore),
    Container {
        mission: u32,
        name: String,
    },
    Capsule {
        mission: u32,
    },
    /// Bauteil aus einem Wrack, wird am Markt zum festen Wert verkauft.
    Salvage {
        name: String,
        value: u32,
    },
    /// Artefakt der Vorgänger – kommt beim Andocken an einer Station in die Sammlung.
    Artifact {
        id: String,
    },
    /// Gerettete Besatzung eines havarierten Schiffs (Rettungsauftrag).
    Survivor {
        mission: u32,
    },
    /// Ausrüstung aus einem Wrack (wird beim Einsammeln sofort eingebaut).
    Gear(super::data::Gear),
}

/// Ein Frachtstück in einem Frachtmodul (Befestigungspunkt).
#[derive(Clone, Debug, PartialEq)]
pub struct CargoItem {
    /// Feste Kennung (für Umladen und Abwerfen).
    pub id: u32,
    pub kind: CargoKind,
    pub mass: Fx,
    pub pod: usize,
    /// Flugeigenschaften: Tank, empfindlich, instabil.
    pub traits: CargoTrait,
    /// Zustand 0..1 (empfindliche Fracht) und Belastung 0..1 (instabile Fracht).
    pub cond: Fx,
    pub stress: Fx,
    /// Flüssigkeit: Auslenkung und Geschwindigkeit (lokal, Meter).
    pub slosh: Vec2,
    pub slosh_v: Vec2,
    /// Umladen im Flug: Zielmodul und verbleibende Sekunden (die Masse sitzt derweil mittig).
    pub moving: Option<(usize, Fx)>,
}

impl CargoItem {
    pub fn new(kind: CargoKind, mass: Fx, traits: CargoTrait) -> CargoItem {
        CargoItem {
            id: 0,
            kind,
            mass,
            pod: 0,
            traits,
            cond: fx!(1.0),
            stress: fx!(0.0),
            slosh: Vec2::ZERO,
            slosh_v: Vec2::ZERO,
            moving: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Ship {
    pub def_id: String,
    pub parts: Vec<ShipPart>,
    pub thrusters: Vec<Thruster>,
    pub tools: Vec<Tool>,
    pub pods: Vec<CargoPod>,
    pub cargo: Vec<CargoItem>,
    /// Nächste freie Kennung für ein Frachtstück.
    pub next_cargo: u32,
    pub slot_count: u8,

    // Massen-Eigenschaften (lokal relativ zum Schiffsursprung).
    pub mass: Fx,
    pub com: Vec2,
    pub inertia: Fx,

    // Zustand (pos = Schwerpunkt in Weltkoordinaten).
    pub pos: Vec2,
    pub vel: Vec2,
    pub angle: Fx,
    pub ang_vel: Fx,
    pub prev_pos: Vec2,
    pub prev_angle: Fx,

    pub hull: Fx,
    pub max_hull: Fx,
    pub shield: Fx,
    pub max_shield: Fx,
    pub ammo: u32,
    pub max_ammo: u32,
    pub ang_damp: Fx,
    /// Bremsdüsen-Upgrades: Faktor auf Bremse und Rückwärtsgang.
    pub brake_mul: Fx,
    pub crane_range: Fx,
    pub drill_rate: Fx,
    pub fuel: Fx,
    pub max_fuel: Fx,
    pub fuel_burn: Fx,
    /// Kanone: Faktor auf die Nachladezeit und den Schaden; Kran: Faktor auf die Seilgrenze.
    pub cannon_rate: Fx,
    pub cannon_damage: Fx,
    pub crane_load: Fx,
    pub scan_range: Fx,
    /// Selbstladen des Schilds – seit Phase 19 abgeschaltet (Ladung gibt es an Stationen), die
    /// Werte aus der Schiffsdefinition bleiben für ein mögliches Upgrade erhalten.
    #[allow(dead_code)]
    pub shield_regen: Fx,
    #[allow(dead_code)]
    pub shield_delay: Fx,
    /// Sekunden seit dem letzten Treffer.
    pub since_hit: Fx,
    /// Gemeinsame Zusatzenergie (70).
    pub energy: Fx,
    pub max_energy: Fx,
    /// Reparaturdrohnen: Hüllenpunkte pro Sekunde.
    pub hull_regen: Fx,

    pub docked: Option<usize>,
    pub dock_timer: Fx,
    pub dock_cooldown: Fx,
    pub destroyed: bool,
    pub respawn_timer: Fx,
    pub invulnerable: Fx,
}

impl Ship {
    pub fn build(def: &ShipDef, loadout: &Loadout, stats: &ShipStats) -> Ship {
        let mut parts = Vec::new();
        let mut pods = Vec::new();

        // Triebwerke: die ersten n in Belegungsreihenfolge, symmetrisch neu angeordnet.
        let n = loadout.thrusters.clamp(
            def.min_thrusters.min(def.max_thrusters()),
            def.max_thrusters(),
        );
        // Angebaute Module (feste Position) werden nicht umsortiert, sie hängen hinten an.
        type Claimed = (Fx, Fx, Fx, Fx, Vec2, Fx, PartShape, usize, bool);
        let mut claimed: Vec<Claimed> = def
            .thruster_parts()
            .take(n as usize)
            .map(|(pi, p, t)| {
                (
                    p.pos.0,
                    p.pos.1,
                    t,
                    p.dir,
                    v(p.size),
                    p.mass,
                    p.shape,
                    pi,
                    p.fixed,
                )
            })
            .collect();
        claimed.sort_by(|a, b| a.8.cmp(&b.8).then(a.0.total_cmp(&b.0)));
        let movable = claimed.iter().filter(|c| !c.8).count() as u8;
        let xs = def.thruster_xs(movable);
        let mut thrusters = Vec::new();
        for (i, (x0, y, thrust, dir, size, mass, shape, part, fixed)) in
            claimed.into_iter().enumerate()
        {
            let x = if fixed {
                x0
            } else {
                xs.get(i).copied().unwrap_or(fx!(0.0))
            };
            let pos = Vec2::new(x, y);
            parts.push(ShipPart {
                kind: PartKind::Thruster(thrust),
                pos,
                half: size * fx!(0.5),
                mass,
                shape,
            });
            thrusters.push(Thruster {
                slot: i as u8,
                part,
                pos,
                dir: rot(Vec2::Y, dir.to_radians()),
                thrust: thrust * stats.thrust_mul,
                firing: false,
                level: fx!(0.0),
                health: fx!(1.0),
                overdrive: false,
                heat: fx!(0.0),
                cool_off: fx!(0.0),
                heat_warn: 0,
                held: 0,
                since_release: u32::MAX,
                tap: false,
            });
        }

        let mut tools = Vec::new();
        let mut slot = thrusters.len() as u8;
        for (ti, (_, p, kind)) in def.tool_parts().enumerate() {
            if !loadout.tools.contains(&ti) {
                continue;
            }
            parts.push(ShipPart {
                kind: PartKind::Tool(kind),
                pos: v(p.pos),
                half: v(p.size) * fx!(0.5),
                mass: p.mass,
                shape: p.shape,
            });
            tools.push(Tool {
                slot,
                kind,
                pos: v(p.pos),
                aim: Fx::HALF_PI,
                pressed: false,
                prev_pressed: false,
                cooldown: fx!(0.0),
                crane: CraneState::Idle,
                drill: None,
                jitter: fx!(0.0),
                strain: fx!(0.0),
            });
            slot += 1;
        }

        for p in &def.parts {
            match p.kind {
                PartKind::Thruster(_) | PartKind::Tool(_) => {}
                PartKind::CargoPod(cap) => {
                    pods.push(CargoPod {
                        pos: v(p.pos),
                        half: v(p.size) * fx!(0.5),
                        capacity: cap * stats.cargo_mul,
                    });
                    parts.push(ShipPart {
                        kind: p.kind.clone(),
                        pos: v(p.pos),
                        half: v(p.size) * fx!(0.5),
                        mass: p.mass,
                        shape: p.shape,
                    });
                }
                _ => parts.push(ShipPart {
                    kind: p.kind.clone(),
                    pos: v(p.pos),
                    half: v(p.size) * fx!(0.5),
                    mass: p.mass,
                    shape: p.shape,
                }),
            }
        }

        // Nachteil der Upgrades: Zusatzmasse an den betroffenen Teilen.
        for (up, m) in &stats.part_mass {
            add_upgrade_mass(&mut parts, *up, *m);
        }
        let max_hull = def.max_hull + stats.hull_bonus;
        let max_shield = def.max_shield + stats.shield_bonus;
        let max_ammo = def.max_ammo + stats.ammo_bonus;
        let max_fuel = def.fuel_capacity * stats.fuel_mul;
        let mut ship = Ship {
            def_id: def.id.clone(),
            parts,
            thrusters,
            tools,
            pods,
            cargo: Vec::new(),
            next_cargo: 1,
            slot_count: slot,
            mass: fx!(1.0),
            com: Vec2::ZERO,
            inertia: fx!(1.0),
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            angle: fx!(0.0),
            ang_vel: fx!(0.0),
            prev_pos: Vec2::ZERO,
            prev_angle: fx!(0.0),
            hull: max_hull,
            max_hull,
            shield: max_shield,
            max_shield,
            ammo: max_ammo,
            max_ammo,
            ang_damp: def.angular_damping + stats.gyro,
            brake_mul: stats.brake_mul,
            crane_range: fx!(20.0) * stats.crane_mul,
            // Runde 4: schneller abbauen (vorher 1,6 t/s) – Anfliegen und Halten dauern genug.
            drill_rate: fx!(2.4) * stats.drill_mul,
            fuel: max_fuel,
            max_fuel,
            fuel_burn: def.fuel_burn * stats.fuel_burn_mul,
            cannon_rate: stats.cannon_rate,
            cannon_damage: stats.cannon_damage,
            crane_load: stats.crane_load,
            scan_range: fx!(450.0) * stats.scan_mul,
            shield_regen: def.shield_regen,
            shield_delay: def.shield_delay,
            since_hit: fx!(0.0),
            energy: super::power::ENERGY_MAX,
            max_energy: super::power::ENERGY_MAX,
            hull_regen: stats.hull_regen,
            docked: None,
            dock_timer: fx!(0.0),
            dock_cooldown: fx!(0.0),
            destroyed: false,
            respawn_timer: fx!(0.0),
            invulnerable: fx!(0.0),
        };
        ship.recompute_mass();
        ship
    }

    /// Masse, Schwerpunkt und Trägheitsmoment aus Teilen + Fracht neu berechnen.
    /// Die Weltposition der Teile bleibt dabei erhalten.
    pub fn recompute_mass(&mut self) {
        let old_com = self.com;
        let mut m = fx!(0.0);
        let mut mc = Vec2::ZERO;
        for p in &self.parts {
            m += p.mass;
            mc += p.pos * p.mass;
        }
        for c in &self.cargo {
            m += c.mass;
            mc += self.cargo_pos(c) * c.mass;
        }
        let com = if m > fx!(0.0) { mc / m } else { Vec2::ZERO };
        let mut inertia = fx!(0.0);
        for p in &self.parts {
            let size = p.half * fx!(2.0);
            inertia += p.mass * (size.x * size.x + size.y * size.y) / fx!(12.0)
                + p.mass * (p.pos - com).length_squared();
        }
        for c in &self.cargo {
            let pod = &self.pods[c.pod.min(self.pods.len().saturating_sub(1))];
            let size = pod.half * fx!(2.0);
            inertia += c.mass * (size.x * size.x + size.y * size.y) / fx!(12.0)
                + c.mass * (self.cargo_pos(c) - com).length_squared();
        }
        self.mass = m.max(fx!(0.1));
        self.inertia = inertia.max(fx!(0.1));
        // Schwerpunkt verschiebt sich: Welt-Position so anpassen, dass das Schiff nicht springt.
        self.pos += rot(com - old_com, self.angle);
        self.prev_pos += rot(com - old_com, self.prev_angle);
        self.com = com;
    }

    pub fn to_world(&self, local: Vec2) -> Vec2 {
        self.pos + rot(local - self.com, self.angle)
    }

    pub fn origin(&self) -> Vec2 {
        self.to_world(Vec2::ZERO)
    }

    pub fn point_velocity(&self, world: Vec2) -> Vec2 {
        self.vel + super::geom::cross_sv(self.ang_vel, world - self.pos)
    }

    pub fn apply_impulse(&mut self, impulse: Vec2, at: Vec2) {
        self.vel += impulse / self.mass;
        self.ang_vel += super::geom::cross(at - self.pos, impulse) / self.inertia;
    }

    pub fn quads(&self) -> Vec<Poly> {
        self.parts
            .iter()
            .map(|p| Poly::transformed(&p.shape.outline(p.half), self.to_world(p.pos), self.angle))
            .collect()
    }

    /// Ungefährer Radius um den Schwerpunkt (für Broadphase).
    pub fn bound_radius(&self) -> Fx {
        self.parts
            .iter()
            .map(|p| (p.pos - self.com).length() + p.half.length())
            .fold(fx!(0.0), Fx::max)
    }

    /// Abstand vom Schwerpunkt zur Unterkante (entgegen der Nase).
    pub fn rest_height(&self) -> Fx {
        let mut min_y = fx!(0.0);
        for p in &self.parts {
            min_y = min_y.min(p.pos.y - p.half.y - self.com.y);
        }
        -min_y
    }

    pub fn cargo_mass(&self) -> Fx {
        self.cargo.iter().map(|c| c.mass).sum()
    }

    pub fn cargo_capacity(&self) -> Fx {
        self.pods.iter().map(|p| p.capacity).sum()
    }

    pub fn pod_load(&self, pod: usize) -> Fx {
        self.cargo
            .iter()
            .filter(|c| c.pod == pod)
            .map(|c| c.mass)
            .sum()
    }

    /// Wo ein Frachtstück sitzt (lokal): in seinem Modul oder – beim Umladen – dazwischen.
    pub fn cargo_pos(&self, c: &CargoItem) -> Vec2 {
        if self.pods.is_empty() {
            return Vec2::ZERO;
        }
        let last = self.pods.len() - 1;
        let from = self.pods[c.pod.min(last)].pos;
        match c.moving {
            Some((to, _)) => (from + self.pods[to.min(last)].pos) * fx!(0.5),
            None => from,
        }
    }

    /// Platz in einem Modul, wenn auch das schon dorthin Unterwegs-Seiende ankommt.
    pub fn pod_free(&self, pod: usize) -> Fx {
        let incoming: Fx = self
            .cargo
            .iter()
            .filter(|c| c.pod != pod && c.moving.is_some_and(|(to, _)| to == pod))
            .map(|c| c.mass)
            .sum();
        self.pods.get(pod).map_or(fx!(0.0), |p| p.capacity) - self.pod_load(pod) - incoming
    }

    /// Lesbarer Name eines Frachtmoduls nach seiner Lage („links“, „hinten rechts“ …).
    pub fn pod_label(&self, i: usize) -> String {
        let name = |p: Vec2| {
            let side = if p.x < -fx!(0.3) {
                "links"
            } else if p.x > fx!(0.3) {
                "rechts"
            } else {
                "Mitte"
            };
            let fore = if p.y > fx!(0.6) {
                "vorne "
            } else if p.y < -fx!(0.6) {
                "hinten "
            } else {
                ""
            };
            format!("{fore}{side}")
        };
        let Some(pod) = self.pods.get(i) else {
            return "?".into();
        };
        let n = name(pod.pos);
        let same = self.pods[..i].iter().filter(|p| name(p.pos) == n).count();
        if same > 0 {
            format!("{n} {}", same + 1)
        } else {
            n
        }
    }

    /// Frachtstück in ein anderes Modul umladen: sofort (angedockt) oder mit Laufzeit.
    pub fn move_cargo(&mut self, id: u32, to: usize, instant: bool) -> Result<(), String> {
        let Some(i) = self.cargo.iter().position(|c| c.id == id) else {
            return Err("Dieses Frachtstück gibt es nicht mehr".into());
        };
        if to >= self.pods.len() {
            return Err("Unbekanntes Frachtmodul".into());
        }
        let c = &self.cargo[i];
        if c.pod == to && c.moving.is_none() {
            return Err("Liegt schon dort".into());
        }
        if self.pod_free(to) + fx!(1e-4) < c.mass {
            return Err(format!(
                "Im Modul {} ist nicht genug Platz",
                self.pod_label(to)
            ));
        }
        let secs = fx!(0.8) + fx!(0.6) * c.mass;
        let c = &mut self.cargo[i];
        if instant {
            c.pod = to;
            c.moving = None;
        } else {
            c.moving = Some((to, secs));
        }
        self.recompute_mass();
        Ok(())
    }

    /// Laufendes Umladen fortschreiben; true, wenn ein Stück eingerastet ist.
    pub fn update_cargo_moves(&mut self, dt: Fx) -> bool {
        let mut done = false;
        for c in &mut self.cargo {
            if let Some((to, left)) = c.moving {
                let left = left - dt;
                if left <= fx!(0.0) {
                    c.pod = to;
                    c.moving = None;
                    done = true;
                } else {
                    c.moving = Some((to, left));
                }
            }
        }
        if done {
            self.recompute_mass();
        }
        done
    }

    pub fn ore_amount(&self, ore: Ore) -> Fx {
        self.cargo
            .iter()
            .filter(|c| c.kind == CargoKind::Ore(ore))
            .map(|c| c.mass)
            .sum()
    }

    /// Fracht einlagern: in das Modul mit dem meisten freien Platz. Erz wird zusammengefasst.
    /// Gibt die eingelagerte Masse zurück.
    pub fn store(&mut self, kind: CargoKind, mass: Fx) -> Fx {
        self.store_with(kind, mass, CargoTrait::None)
    }

    /// Ein vorhandenes Frachtstück mit allen Eigenschaften einladen (Schiffswechsel,
    /// Wiederaufnahme nach dem Abwurf). Erz wird wie immer verteilt und zusammengefasst.
    pub fn store_item(&mut self, item: CargoItem) -> Fx {
        if matches!(item.kind, CargoKind::Ore(_)) {
            // Erz auf die Module verteilen, bis alles drin ist oder nichts mehr passt.
            let mut done = fx!(0.0);
            while item.mass - done > fx!(1e-4) {
                let s = self.store(item.kind.clone(), item.mass - done);
                if s <= fx!(1e-5) {
                    break;
                }
                done += s;
            }
            return done;
        }
        let Some(best) = self.best_pod() else {
            return fx!(0.0);
        };
        if self.pod_free(best) + fx!(1e-4) < item.mass {
            return fx!(0.0);
        }
        let id = self.next_cargo;
        self.next_cargo += 1;
        let mass = item.mass;
        self.cargo.push(CargoItem {
            id,
            pod: best,
            moving: None,
            slosh: Vec2::ZERO,
            slosh_v: Vec2::ZERO,
            ..item
        });
        self.recompute_mass();
        mass
    }

    /// Modul mit dem meisten freien Platz.
    fn best_pod(&self) -> Option<usize> {
        (0..self.pods.len()).max_by(|a, b| {
            self.pod_free(*a)
                .total_cmp(&self.pod_free(*b))
                .then(b.cmp(a))
        })
    }

    /// Fracht mit Flugeigenschaften einlagern (Lieferaufträge).
    pub fn store_with(&mut self, kind: CargoKind, mass: Fx, traits: CargoTrait) -> Fx {
        if self.pods.is_empty() {
            return fx!(0.0);
        }
        let mut best = 0;
        let mut best_free = Fx::MIN;
        for i in 0..self.pods.len() {
            let free = self.pod_free(i);
            if free > best_free + fx!(1e-4) {
                best_free = free;
                best = i;
            }
        }
        let divisible = matches!(kind, CargoKind::Ore(_));
        let stored = if divisible {
            mass.min(best_free)
        } else if best_free >= mass {
            mass
        } else {
            fx!(0.0)
        };
        if stored <= fx!(1e-5) {
            return fx!(0.0);
        }
        if divisible
            && let Some(item) = self
                .cargo
                .iter_mut()
                .find(|c| c.kind == kind && c.pod == best && c.moving.is_none())
        {
            item.mass += stored;
            self.recompute_mass();
            return stored;
        }
        let id = self.next_cargo;
        self.next_cargo += 1;
        self.cargo.push(CargoItem {
            id,
            pod: best,
            ..CargoItem::new(kind, stored, traits)
        });
        self.recompute_mass();
        stored
    }

    /// Erz entnehmen (für Verkauf oder Missionen). Gibt die entnommene Menge zurück.
    pub fn take_ore(&mut self, ore: Ore, amount: Fx) -> Fx {
        let mut left = amount;
        for c in self.cargo.iter_mut() {
            if left <= fx!(0.0) {
                break;
            }
            if c.kind == CargoKind::Ore(ore) {
                let t = c.mass.min(left);
                c.mass -= t;
                left -= t;
            }
        }
        self.cargo.retain(|c| c.mass > fx!(1e-4));
        self.recompute_mass();
        amount - left
    }

    pub fn remove_cargo(&mut self, pred: impl Fn(&CargoKind) -> bool) -> Vec<CargoItem> {
        let (removed, kept): (Vec<_>, Vec<_>) = self.cargo.drain(..).partition(|c| pred(&c.kind));
        self.cargo = kept;
        self.recompute_mass();
        removed
    }

    pub fn tool_world_pos(&self, i: usize) -> Vec2 {
        self.to_world(self.tools[i].pos)
    }

    pub fn has_tool(&self, kind: ToolKind) -> bool {
        self.tools.iter().any(|t| t.kind == kind)
    }

    /// Durchschnittlicher Schaden der Triebwerke (0 = alles heil, 1 = alles ausgefallen).
    pub fn thruster_wear(&self) -> Fx {
        if self.thrusters.is_empty() {
            return fx!(0.0);
        }
        self.thrusters
            .iter()
            .map(|t| fx!(1.0) - t.health)
            .sum::<Fx>()
            / Fx::from(self.thrusters.len())
    }

    pub fn fuel_empty(&self) -> bool {
        self.fuel <= fx!(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::data::GameData;

    #[test]
    fn mass_and_symmetry() {
        let data = GameData::embedded().unwrap();
        let def = data.ship("driftkutter");
        let ship = Ship::build(def, &Loadout::full(def), &ShipStats::default());
        assert_eq!(ship.thrusters.len(), 5);
        assert_eq!(ship.tools.len(), 4);
        assert!(
            ship.com.x.abs() < fx!(1e-4),
            "symmetrisches Schiff, Schwerpunkt mittig"
        );
        assert!(ship.mass > fx!(10.0));
        // Slots: Triebwerke von links nach rechts.
        let xs: Vec<Fx> = ship.thrusters.iter().map(|t| t.pos.x).collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn two_thrusters_are_symmetric() {
        let data = GameData::embedded().unwrap();
        let def = data.ship("driftkutter");
        let ship = Ship::build(
            def,
            &Loadout {
                thrusters: 2,
                tools: vec![],
            },
            &ShipStats::default(),
        );
        assert_eq!(ship.thrusters.len(), 2);
        assert!((ship.thrusters[0].pos.x + ship.thrusters[1].pos.x).abs() < fx!(1e-5));
        assert_eq!(ship.slot_count, 2);
    }

    #[test]
    fn cargo_shifts_center_of_mass() {
        let data = GameData::embedded().unwrap();
        let def = data.ship("driftkutter");
        let mut ship = Ship::build(def, &Loadout::full(def), &ShipStats::default());
        let before = ship.origin();
        ship.store(
            CargoKind::Container {
                mission: 1,
                name: "Test".into(),
            },
            fx!(5.0),
        );
        assert!(ship.com.x.abs() > fx!(0.1), "Container liegt seitlich");
        // Das Schiff springt nicht, wenn sich der Schwerpunkt verschiebt.
        assert!((ship.origin() - before).length() < fx!(1e-4));
    }
}
