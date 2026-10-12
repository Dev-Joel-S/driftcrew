//! Präzisionsarbeit im Flug: Manche Erzadern und Wrackverbindungen geben nur nach, wenn der
//! Bohrer ein paar Sekunden genau auf einer kleinen Stelle bleibt. Eine Person bohrt, die
//! anderen halten das Schiff ruhig. Fortschritt und Abweichung sieht die ganze Crew.

use crate::sim::fx::{Fx, Vec2};

use super::rng::hash32;
use super::{Body, BodyKind, DT, SimState, ToastKind};

/// So nah (Meter) muss der Bohrpunkt an der Markierung sein.
pub const PRECISION_RADIUS: Fx = fx!(0.7);

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
    fn hold_seconds(self) -> Fx {
        match self {
            PrecisionKind::Vein => fx!(4.0),
            PrecisionKind::Joint => fx!(3.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Precision {
    pub body: u32,
    pub kind: PrecisionKind,
    /// 0..1
    pub progress: Fx,
    /// Abstand des Bohrpunkts zur Markierung (m).
    pub offset: Fx,
    /// Sekunden ohne Bohrkontakt (danach endet die Arbeit).
    pub idle: Fx,
}

/// Markierung an einem Körper (Weltposition), falls er eine hat.
pub fn precision_target(b: &Body) -> Option<(Vec2, PrecisionKind)> {
    let (kind, salt) = match b.kind {
        BodyKind::Asteroid { vein: true, .. } => (PrecisionKind::Vein, 0xa5u32),
        BodyKind::Wreck { parts, .. } if parts > 0 => (PrecisionKind::Joint, 0x51 + parts),
        _ => return None,
    };
    let a = Fx::from(hash32(b.seed ^ salt) % 6283) / fx!(1000.0) + b.angle;
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
                    progress: fx!(0.0),
                    offset: d,
                    idle: fx!(0.0),
                });
                self.precision.as_mut().unwrap()
            }
        };
        p.offset = d;
        p.idle = fx!(0.0);
        if d < PRECISION_RADIUS {
            p.progress += DT / kind.hold_seconds();
        } else {
            // Abgerutscht: der Fortschritt bröckelt.
            p.progress = (p.progress - fx!(0.35) * DT).max(fx!(0.0));
        }
        if p.progress >= fx!(1.0) {
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
                let pos = at + out * fx!(0.9);
                let vel = self.bodies[bi].vel + out * fx!(1.5);
                self.bodies.push(Body {
                    id,
                    kind: BodyKind::Salvage {
                        name: format!("Kristallkern ({})", ore.label()),
                        value,
                    },
                    pos,
                    vel,
                    angle: fx!(0.0),
                    ang_vel: fx!(1.5),
                    radius: fx!(0.7),
                    mass: fx!(1.2),
                    prev_pos: pos,
                    prev_angle: fx!(0.0),
                    alive: true,
                    seed: hash32(id),
                    age: fx!(0.0),
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
        if p.idle > DT * fx!(1.5) {
            p.progress = (p.progress - fx!(0.25) * DT).max(fx!(0.0));
        }
        let gone = !self.bodies.iter().any(|b| b.id == p.body && b.alive);
        if p.idle > fx!(2.0) || gone {
            self.precision = None;
        }
    }
}
