//! Präzisionsarbeit im Flug: Manche Erzadern und Wrackverbindungen geben nur nach, wenn der
//! Bohrer ein paar Sekunden genau auf einer kleinen Stelle bleibt. Eine Person bohrt, die
//! anderen halten das Schiff ruhig. Fortschritt und Abweichung sieht die ganze Crew.

use bevy::math::Vec2;

use super::rng::hash32;
use super::{Body, BodyKind, DT, SimState, ToastKind};

/// So nah (Meter) muss der Bohrpunkt an der Markierung sein.
pub const PRECISION_RADIUS: f32 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrecisionKind {
    /// Reiche Erzader an einem Asteroiden.
    Vein,
    /// Verbindungsbolzen eines Wracks: löst ein Bauteil.
    Joint,
}

impl PrecisionKind {
    pub fn label(self) -> &'static str {
        match self {
            PrecisionKind::Vein => "Erzader",
            PrecisionKind::Joint => "Wrackverbindung",
        }
    }
    fn hold_seconds(self) -> f32 {
        match self {
            PrecisionKind::Vein => 4.0,
            PrecisionKind::Joint => 3.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Precision {
    pub body: u32,
    pub kind: PrecisionKind,
    /// 0..1
    pub progress: f32,
    /// Abstand des Bohrpunkts zur Markierung (m).
    pub offset: f32,
    /// Sekunden ohne Bohrkontakt (danach endet die Arbeit).
    pub idle: f32,
}

/// Markierung an einem Körper (Weltposition), falls er eine hat.
pub fn precision_target(b: &Body) -> Option<(Vec2, PrecisionKind)> {
    let (kind, salt) = match b.kind {
        BodyKind::Asteroid { vein: true, .. } => (PrecisionKind::Vein, 0xa5u32),
        BodyKind::Wreck { parts, .. } if parts > 0 => (PrecisionKind::Joint, 0x51 + parts),
        _ => return None,
    };
    let a = (hash32(b.seed ^ salt) % 6283) as f32 / 1000.0 + b.angle;
    Some((b.pos + Vec2::new(a.cos(), a.sin()) * b.radius, kind))
}

impl SimState {
    /// Vom Bohrer aufgerufen, wenn er einen Körper trifft.
    pub(crate) fn precision_drill(&mut self, bi: usize, point: Vec2) {
        let Some((target, kind)) = precision_target(&self.bodies[bi]) else {
            return;
        };
        let id = self.bodies[bi].id;
        let d = (point - target).length();
        let p = match &mut self.precision {
            Some(p) if p.body == id => p,
            _ => {
                self.precision = Some(Precision {
                    body: id,
                    kind,
                    progress: 0.0,
                    offset: d,
                    idle: 0.0,
                });
                self.precision.as_mut().unwrap()
            }
        };
        p.offset = d;
        p.idle = 0.0;
        if d < PRECISION_RADIUS {
            p.progress += DT / kind.hold_seconds();
        } else {
            // Abgerutscht: der Fortschritt bröckelt.
            p.progress = (p.progress - 0.35 * DT).max(0.0);
        }
        if p.progress >= 1.0 {
            self.precision = None;
            self.finish_precision(bi, kind, target);
        }
    }

    fn finish_precision(&mut self, bi: usize, kind: PrecisionKind, at: Vec2) {
        match kind {
            PrecisionKind::Vein => {
                let ore = match &mut self.bodies[bi].kind {
                    BodyKind::Asteroid { vein, ore, .. } => {
                        *vein = false;
                        *ore
                    }
                    _ => None,
                };
                let ore = ore.unwrap_or(super::data::Ore::Ferrit);
                let value = (self.data.shop.ore_price(ore) * 9).max(80);
                let id = self.next_id();
                let out = (at - self.bodies[bi].pos).normalize_or_zero();
                let pos = at + out * 0.9;
                let vel = self.bodies[bi].vel + out * 1.5;
                self.bodies.push(Body {
                    id,
                    kind: BodyKind::Salvage {
                        name: format!("Kristallkern ({})", ore.label()),
                        value,
                    },
                    pos,
                    vel,
                    angle: 0.0,
                    ang_vel: 1.5,
                    radius: 0.7,
                    mass: 1.2,
                    prev_pos: pos,
                    prev_angle: 0.0,
                    alive: true,
                    seed: hash32(id),
                    age: 0.0,
                });
                self.toast(
                    format!(
                        "Erzader freigelegt: Kristallkern ({}) ~{value} Cr – einsammeln!",
                        ore.label()
                    ),
                    ToastKind::Good,
                );
            }
            PrecisionKind::Joint => {
                // Das Teil löst sich an der Seite, die zum Schiff zeigt.
                let toward = self.ship.pos;
                self.tear_part_at(bi, toward);
            }
        }
    }

    /// Ohne Bohrkontakt bröckelt der Fortschritt und die Arbeit endet nach kurzer Zeit.
    pub(crate) fn update_precision(&mut self) {
        let Some(p) = &mut self.precision else {
            return;
        };
        p.idle += DT;
        if p.idle > DT * 1.5 {
            p.progress = (p.progress - 0.25 * DT).max(0.0);
        }
        let gone = !self.bodies.iter().any(|b| b.id == p.body && b.alive);
        if p.idle > 2.0 || gone {
            self.precision = None;
        }
    }
}
