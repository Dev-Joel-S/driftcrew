//! Das Crew-Schiff: aus Daten gebaut, Masse/Schwerpunkt/Trägheit aus den Teilen berechnet.

use bevy::math::Vec2;

use super::data::{Ore, PartKind, ShipDef, ToolKind, v};
use super::geom::{Quad, rot};

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
}

#[derive(Clone, Debug)]
pub struct Thruster {
    pub slot: u8,
    pub pos: Vec2,
    pub dir: Vec2,
    pub thrust: f32,
    pub firing: bool,
    /// Weiche Anzeige 0..1 für Flamme/Sound.
    pub level: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CraneState {
    Idle,
    Extending { len: f32, dir: Vec2 },
    Retracting { len: f32, dir: Vec2 },
    Attached { body: u32, rope: f32 },
}

#[derive(Clone, Debug)]
pub struct DrillHit {
    pub point: Vec2,
    pub ore: Option<Ore>,
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

#[derive(Clone, Debug, PartialEq)]
pub enum CargoKind {
    Ore(Ore),
    Container { mission: u32, name: String },
    Capsule { mission: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CargoItem {
    pub kind: CargoKind,
    pub mass: f32,
    pub pod: usize,
}

#[derive(Clone, Debug)]
pub struct Ship {
    pub def_id: String,
    pub parts: Vec<ShipPart>,
    pub thrusters: Vec<Thruster>,
    pub tools: Vec<Tool>,
    pub pods: Vec<CargoPod>,
    pub cargo: Vec<CargoItem>,
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
        let mut claimed: Vec<(f32, f32, f32, f32, Vec2, f32)> = def
            .thruster_parts()
            .take(n as usize)
            .map(|(_, p, t)| (p.pos.0, p.pos.1, t, p.dir, v(p.size), p.mass))
            .collect();
        claimed.sort_by(|a, b| a.0.total_cmp(&b.0));
        let xs = def.thruster_xs(n);
        let mut thrusters = Vec::new();
        for (i, (_, y, thrust, dir, size, mass)) in claimed.into_iter().enumerate() {
            let x = xs.get(i).copied().unwrap_or(0.0);
            let pos = Vec2::new(x, y);
            parts.push(ShipPart {
                kind: PartKind::Thruster(thrust),
                pos,
                half: size * 0.5,
                mass,
            });
            thrusters.push(Thruster {
                slot: i as u8,
                pos,
                dir: rot(Vec2::Y, dir.to_radians()),
                thrust: thrust * stats.thrust_mul,
                firing: false,
                level: 0.0,
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
                    });
                }
                _ => parts.push(ShipPart {
                    kind: p.kind.clone(),
                    pos: v(p.pos),
                    half: v(p.size) * 0.5,
                    mass: p.mass,
                }),
            }
        }

        let max_hull = def.max_hull + stats.hull_bonus;
        let max_shield = def.max_shield + stats.shield_bonus;
        let max_ammo = def.max_ammo + stats.ammo_bonus;
        let mut ship = Ship {
            def_id: def.id.clone(),
            parts,
            thrusters,
            tools,
            pods,
            cargo: Vec::new(),
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
            let pod = &self.pods[c.pod.min(self.pods.len().saturating_sub(1))];
            m += c.mass;
            mc += pod.pos * c.mass;
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
                + c.mass * (pod.pos - com).length_squared();
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

    pub fn quads(&self) -> Vec<Quad> {
        self.parts
            .iter()
            .map(|p| Quad::obb(self.to_world(p.pos), p.half, self.angle))
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
        if self.pods.is_empty() {
            return 0.0;
        }
        let mut best = 0;
        let mut best_free = f32::MIN;
        for i in 0..self.pods.len() {
            let free = self.pods[i].capacity - self.pod_load(i);
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
                .find(|c| c.kind == kind && c.pod == best)
        {
            item.mass += stored;
            self.recompute_mass();
            return stored;
        }
        self.cargo.push(CargoItem {
            kind,
            mass: stored,
            pod: best,
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
        assert_eq!(ship.tools.len(), 3);
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
