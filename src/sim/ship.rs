//! Das Crew-Schiff: aus Daten gebaut, Masse/Schwerpunkt/Trägheit aus den Teilen berechnet.

use bevy::math::Vec2;

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
    pub thrust_mul: f32,
    pub hull_bonus: f32,
    pub shield_bonus: f32,
    pub cargo_mul: f32,
    pub gyro: f32,
    pub ammo_bonus: u32,
    pub crane_mul: f32,
    pub drill_mul: f32,
    pub fuel_mul: f32,
    pub scan_mul: f32,
    pub hull_regen: f32,
    pub fuel_burn_mul: f32,
    pub cannon_rate: f32,
    pub cannon_damage: f32,
    pub crane_load: f32,
    /// Nachteil der Upgrades: Zusatzmasse pro betroffenem Teil.
    pub part_mass: Vec<(UpgradePart, f32)>,
}

impl Default for ShipStats {
    fn default() -> Self {
        ShipStats {
            thrust_mul: 1.0,
            hull_bonus: 0.0,
            shield_bonus: 0.0,
            cargo_mul: 1.0,
            gyro: 0.0,
            ammo_bonus: 0,
            crane_mul: 1.0,
            drill_mul: 1.0,
            fuel_mul: 1.0,
            scan_mul: 1.0,
            hull_regen: 0.0,
            fuel_burn_mul: 1.0,
            cannon_rate: 1.0,
            cannon_damage: 1.0,
            crane_load: 1.0,
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
    pub mass: f32,
    pub shape: PartShape,
}

#[derive(Clone, Debug)]
pub struct Thruster {
    pub slot: u8,
    /// Index des Teils in der Schiffsdefinition (bleibt beim Umbau gleich).
    pub part: usize,
    pub pos: Vec2,
    pub dir: Vec2,
    pub thrust: f32,
    pub firing: bool,
    /// Weiche Anzeige 0..1 für Flamme/Sound.
    pub level: f32,
    /// Zustand 0..1. Unter [`STUTTER_BELOW`] stottert das Triebwerk, bei 0 fällt es aus.
    pub health: f32,
}

/// Ab diesem Zustand setzt ein Triebwerk zufällig aus.
pub const STUTTER_BELOW: f32 = 0.6;

impl Thruster {
    pub fn failed(&self) -> bool {
        self.health <= 0.0
    }
    pub fn stuttering(&self) -> bool {
        !self.failed() && self.health < STUTTER_BELOW
    }
    /// Schub, den das Triebwerk in seinem Zustand noch liefert.
    pub fn effective_thrust(&self) -> f32 {
        if self.failed() {
            0.0
        } else {
            self.thrust * (0.7 + 0.3 * self.health)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CraneState {
    Idle,
    Extending {
        len: f32,
        dir: Vec2,
    },
    Retracting {
        len: f32,
        dir: Vec2,
    },
    /// `local`: Angriffspunkt am Körper (lokal, mitgedreht) – lange Teile pendeln am Ende.
    Attached {
        body: u32,
        rope: f32,
        local: Vec2,
    },
    /// Notreparatur: der Kran hält ein Ersatzteil an ein ausgefallenes Triebwerk.
    Patching {
        thruster: usize,
        progress: f32,
    },
}

#[derive(Clone, Debug)]
pub struct DrillHit {
    pub point: Vec2,
    pub ore: Option<Ore>,
    /// Ertragsfaktor durch die Zielhand (siehe [`steady_factor`]).
    pub steady: f32,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub slot: u8,
    pub kind: ToolKind,
    pub pos: Vec2,
    /// Zielwinkel in Weltkoordinaten (0 = +x).
    pub aim: f32,
    pub pressed: bool,
    pub prev_pressed: bool,
    pub cooldown: f32,
    pub crane: CraneState,
    pub drill: Option<DrillHit>,
    /// Seilbelastung 0..1 (1 = reißt gleich), geglättet für die Anzeige.
    pub strain: f32,
    /// Wie unruhig die Zielhand ist: geglättete Drehgeschwindigkeit des Zielwinkels (rad/s).
    pub jitter: f32,
}

/// Upgrade-Masse auf die betroffenen Teile verteilen. Triebwerke, Fracht und Werkzeuge
/// bekommen sie je Teil; Hülle, Schild und Bordsysteme sitzen im Rumpf; ein größerer Tank
/// sitzt hinten bei den Triebwerken und zieht den Schwerpunkt nach hinten.
fn add_upgrade_mass(parts: &mut [ShipPart], up: UpgradePart, m: f32) {
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
                p.mass += m / n.max(1) as f32;
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
pub fn steady_factor(jitter: f32) -> f32 {
    let k = ((jitter - 0.25) / 1.75).clamp(0.0, 1.0);
    1.25 - 0.55 * k
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
    pub capacity: f32,
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
}

/// Ein Frachtstück in einem Frachtmodul (Befestigungspunkt).
#[derive(Clone, Debug, PartialEq)]
pub struct CargoItem {
    /// Feste Kennung (für Umladen und Abwerfen).
    pub id: u32,
    pub kind: CargoKind,
    pub mass: f32,
    pub pod: usize,
    /// Flugeigenschaften: Tank, empfindlich, instabil.
    pub traits: CargoTrait,
    /// Zustand 0..1 (empfindliche Fracht) und Belastung 0..1 (instabile Fracht).
    pub cond: f32,
    pub stress: f32,
    /// Flüssigkeit: Auslenkung und Geschwindigkeit (lokal, Meter).
    pub slosh: Vec2,
    pub slosh_v: Vec2,
    /// Umladen im Flug: Zielmodul und verbleibende Sekunden (die Masse sitzt derweil mittig).
    pub moving: Option<(usize, f32)>,
}

impl CargoItem {
    pub fn new(kind: CargoKind, mass: f32, traits: CargoTrait) -> CargoItem {
        CargoItem {
            id: 0,
            kind,
            mass,
            pod: 0,
            traits,
            cond: 1.0,
            stress: 0.0,
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
    pub mass: f32,
    pub com: Vec2,
    pub inertia: f32,

    // Zustand (pos = Schwerpunkt in Weltkoordinaten).
    pub pos: Vec2,
    pub vel: Vec2,
    pub angle: f32,
    pub ang_vel: f32,
    pub prev_pos: Vec2,
    pub prev_angle: f32,

    pub hull: f32,
    pub max_hull: f32,
    pub shield: f32,
    pub max_shield: f32,
    pub ammo: u32,
    pub max_ammo: u32,
    pub ang_damp: f32,
    pub crane_range: f32,
    pub drill_rate: f32,
    pub fuel: f32,
    pub max_fuel: f32,
    pub fuel_burn: f32,
    /// Kanone: Faktor auf die Nachladezeit und den Schaden; Kran: Faktor auf die Seilgrenze.
    pub cannon_rate: f32,
    pub cannon_damage: f32,
    pub crane_load: f32,
    pub scan_range: f32,
    pub shield_regen: f32,
    pub shield_delay: f32,
    /// Sekunden seit dem letzten Treffer (für das Nachladen des Schilds).
    pub since_hit: f32,
    /// Reparaturdrohnen: Hüllenpunkte pro Sekunde.
    pub hull_regen: f32,

    pub docked: Option<usize>,
    pub dock_timer: f32,
    pub dock_cooldown: f32,
    pub destroyed: bool,
    pub respawn_timer: f32,
    pub invulnerable: f32,
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
        type Claimed = (f32, f32, f32, f32, Vec2, f32, PartShape, usize, bool);
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
                xs.get(i).copied().unwrap_or(0.0)
            };
            let pos = Vec2::new(x, y);
            parts.push(ShipPart {
                kind: PartKind::Thruster(thrust),
                pos,
                half: size * 0.5,
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
                level: 0.0,
                health: 1.0,
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
                half: v(p.size) * 0.5,
                mass: p.mass,
                shape: p.shape,
            });
            tools.push(Tool {
                slot,
                kind,
                pos: v(p.pos),
                aim: std::f32::consts::FRAC_PI_2,
                pressed: false,
                prev_pressed: false,
                cooldown: 0.0,
                crane: CraneState::Idle,
                drill: None,
                jitter: 0.0,
                strain: 0.0,
            });
            slot += 1;
        }

        for p in &def.parts {
            match p.kind {
                PartKind::Thruster(_) | PartKind::Tool(_) => {}
                PartKind::CargoPod(cap) => {
                    pods.push(CargoPod {
                        pos: v(p.pos),
                        half: v(p.size) * 0.5,
                        capacity: cap * stats.cargo_mul,
                    });
                    parts.push(ShipPart {
                        kind: p.kind.clone(),
                        pos: v(p.pos),
                        half: v(p.size) * 0.5,
                        mass: p.mass,
                        shape: p.shape,
                    });
                }
                _ => parts.push(ShipPart {
                    kind: p.kind.clone(),
                    pos: v(p.pos),
                    half: v(p.size) * 0.5,
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
            mass: 1.0,
            com: Vec2::ZERO,
            inertia: 1.0,
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            angle: 0.0,
            ang_vel: 0.0,
            prev_pos: Vec2::ZERO,
            prev_angle: 0.0,
            hull: max_hull,
            max_hull,
            shield: max_shield,
            max_shield,
            ammo: max_ammo,
            max_ammo,
            ang_damp: def.angular_damping + stats.gyro,
            crane_range: 20.0 * stats.crane_mul,
            drill_rate: 1.6 * stats.drill_mul,
            fuel: max_fuel,
            max_fuel,
            fuel_burn: def.fuel_burn * stats.fuel_burn_mul,
            cannon_rate: stats.cannon_rate,
            cannon_damage: stats.cannon_damage,
            crane_load: stats.crane_load,
            scan_range: 450.0 * stats.scan_mul,
            shield_regen: def.shield_regen,
            shield_delay: def.shield_delay,
            since_hit: 0.0,
            hull_regen: stats.hull_regen,
            docked: None,
            dock_timer: 0.0,
            dock_cooldown: 0.0,
            destroyed: false,
            respawn_timer: 0.0,
            invulnerable: 0.0,
        };
        ship.recompute_mass();
        ship
    }

    /// Masse, Schwerpunkt und Trägheitsmoment aus Teilen + Fracht neu berechnen.
    /// Die Weltposition der Teile bleibt dabei erhalten.
    pub fn recompute_mass(&mut self) {
        let old_com = self.com;
        let mut m = 0.0;
        let mut mc = Vec2::ZERO;
        for p in &self.parts {
            m += p.mass;
            mc += p.pos * p.mass;
        }
        for c in &self.cargo {
            m += c.mass;
            mc += self.cargo_pos(c) * c.mass;
        }
        let com = if m > 0.0 { mc / m } else { Vec2::ZERO };
        let mut inertia = 0.0;
        for p in &self.parts {
            let size = p.half * 2.0;
            inertia += p.mass * (size.x * size.x + size.y * size.y) / 12.0
                + p.mass * (p.pos - com).length_squared();
        }
        for c in &self.cargo {
            let pod = &self.pods[c.pod.min(self.pods.len().saturating_sub(1))];
            let size = pod.half * 2.0;
            inertia += c.mass * (size.x * size.x + size.y * size.y) / 12.0
                + c.mass * (self.cargo_pos(c) - com).length_squared();
        }
        self.mass = m.max(0.1);
        self.inertia = inertia.max(0.1);
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
    pub fn bound_radius(&self) -> f32 {
        self.parts
            .iter()
            .map(|p| (p.pos - self.com).length() + p.half.length())
            .fold(0.0, f32::max)
    }

    /// Abstand vom Schwerpunkt zur Unterkante (entgegen der Nase).
    pub fn rest_height(&self) -> f32 {
        let mut min_y = 0.0f32;
        for p in &self.parts {
            min_y = min_y.min(p.pos.y - p.half.y - self.com.y);
        }
        -min_y
    }

    pub fn cargo_mass(&self) -> f32 {
        self.cargo.iter().map(|c| c.mass).sum()
    }

    pub fn cargo_capacity(&self) -> f32 {
        self.pods.iter().map(|p| p.capacity).sum()
    }

    pub fn pod_load(&self, pod: usize) -> f32 {
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
            Some((to, _)) => (from + self.pods[to.min(last)].pos) * 0.5,
            None => from,
        }
    }

    /// Platz in einem Modul, wenn auch das schon dorthin Unterwegs-Seiende ankommt.
    pub fn pod_free(&self, pod: usize) -> f32 {
        let incoming: f32 = self
            .cargo
            .iter()
            .filter(|c| c.pod != pod && c.moving.is_some_and(|(to, _)| to == pod))
            .map(|c| c.mass)
            .sum();
        self.pods.get(pod).map_or(0.0, |p| p.capacity) - self.pod_load(pod) - incoming
    }

    /// Lesbarer Name eines Frachtmoduls nach seiner Lage („links“, „hinten rechts“ …).
    pub fn pod_label(&self, i: usize) -> String {
        let name = |p: Vec2| {
            let side = if p.x < -0.3 {
                "links"
            } else if p.x > 0.3 {
                "rechts"
            } else {
                "Mitte"
            };
            let fore = if p.y > 0.6 {
                "vorne "
            } else if p.y < -0.6 {
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
        if self.pod_free(to) + 1e-4 < c.mass {
            return Err(format!(
                "Im Modul {} ist nicht genug Platz",
                self.pod_label(to)
            ));
        }
        let secs = 0.8 + 0.6 * c.mass;
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
    pub fn update_cargo_moves(&mut self, dt: f32) -> bool {
        let mut done = false;
        for c in &mut self.cargo {
            if let Some((to, left)) = c.moving {
                let left = left - dt;
                if left <= 0.0 {
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

    pub fn ore_amount(&self, ore: Ore) -> f32 {
        self.cargo
            .iter()
            .filter(|c| c.kind == CargoKind::Ore(ore))
            .map(|c| c.mass)
            .sum()
    }

    /// Fracht einlagern: in das Modul mit dem meisten freien Platz. Erz wird zusammengefasst.
    /// Gibt die eingelagerte Masse zurück.
    pub fn store(&mut self, kind: CargoKind, mass: f32) -> f32 {
        self.store_with(kind, mass, CargoTrait::None)
    }

    /// Ein vorhandenes Frachtstück mit allen Eigenschaften einladen (Schiffswechsel,
    /// Wiederaufnahme nach dem Abwurf). Erz wird wie immer verteilt und zusammengefasst.
    pub fn store_item(&mut self, item: CargoItem) -> f32 {
        if matches!(item.kind, CargoKind::Ore(_)) {
            // Erz auf die Module verteilen, bis alles drin ist oder nichts mehr passt.
            let mut done = 0.0;
            while item.mass - done > 1e-4 {
                let s = self.store(item.kind.clone(), item.mass - done);
                if s <= 1e-5 {
                    break;
                }
                done += s;
            }
            return done;
        }
        let Some(best) = self.best_pod() else {
            return 0.0;
        };
        if self.pod_free(best) + 1e-4 < item.mass {
            return 0.0;
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
    pub fn store_with(&mut self, kind: CargoKind, mass: f32, traits: CargoTrait) -> f32 {
        if self.pods.is_empty() {
            return 0.0;
        }
        let mut best = 0;
        let mut best_free = f32::MIN;
        for i in 0..self.pods.len() {
            let free = self.pod_free(i);
            if free > best_free + 1e-4 {
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
            0.0
        };
        if stored <= 1e-5 {
            return 0.0;
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
    pub fn take_ore(&mut self, ore: Ore, amount: f32) -> f32 {
        let mut left = amount;
        for c in self.cargo.iter_mut() {
            if left <= 0.0 {
                break;
            }
            if c.kind == CargoKind::Ore(ore) {
                let t = c.mass.min(left);
                c.mass -= t;
                left -= t;
            }
        }
        self.cargo.retain(|c| c.mass > 1e-4);
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
    pub fn thruster_wear(&self) -> f32 {
        if self.thrusters.is_empty() {
            return 0.0;
        }
        self.thrusters.iter().map(|t| 1.0 - t.health).sum::<f32>() / self.thrusters.len() as f32
    }

    pub fn fuel_empty(&self) -> bool {
        self.fuel <= 0.0
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
            ship.com.x.abs() < 1e-4,
            "symmetrisches Schiff, Schwerpunkt mittig"
        );
        assert!(ship.mass > 10.0);
        // Slots: Triebwerke von links nach rechts.
        let xs: Vec<f32> = ship.thrusters.iter().map(|t| t.pos.x).collect();
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
        assert!((ship.thrusters[0].pos.x + ship.thrusters[1].pos.x).abs() < 1e-5);
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
            5.0,
        );
        assert!(ship.com.x.abs() > 0.1, "Container liegt seitlich");
        // Das Schiff springt nicht, wenn sich der Schwerpunkt verschiebt.
        assert!((ship.origin() - before).length() < 1e-4);
    }
}
