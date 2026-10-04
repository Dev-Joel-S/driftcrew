//! Die Welt reagiert (Punkte 76, 79).
//!
//! * Erledigte Aufträge wirken eine Weile nach: eine belieferte Station hat Munition und
//!   Reparatur gratis, nach Bergungen ist der Umkreis der Zielstation geräumt (kaum Trümmer, keine
//!   Meteore), nach Messflug oder Geleitschutz läuft der Sender der Station wieder (Routen in der
//!   Nähe bekannt, Scanner reicht weiter). Bei Abschluss sagt eine Meldung, was sich ändert.
//! * Versteckte Routen durch Trümmerfelder und an Anomalien vorbei: entdeckt per Scanner,
//!   Gerücht beim Andocken oder reparierten Sender; danach auf der Karte und im Logbuch. Wer eine
//!   Route am Stück abfliegt, bekommt eine Zeit (Bestzeit pro Route).

use bevy::math::Vec2;

use super::data::{CrewSave, EffectKind, EffectSave, v};
use super::missions::MissionKind;
use super::world::Owner;
use super::{DT, SimEvent, SimState, ToastKind};

/// Wie lange eine Wirkung anhält (Sekunden Spielzeit).
pub const EFFECT_SECONDS: f32 = 600.0;
/// Reichweite um die Station (geräumt, Sender).
pub const EFFECT_RADIUS: f32 = 1400.0;
/// Routenflug: so nah muss man an einen Wegpunkt, so lange darf man zwischen zweien brauchen.
pub const ROUTE_REACH: f32 = 45.0;
const ROUTE_TIMEOUT: f32 = 150.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub kind: EffectKind,
    pub station: usize,
    pub left: f32,
}

/// Laufender Flug entlang einer entdeckten Route.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteRun {
    pub route: usize,
    /// Nächster Wegpunkt.
    pub next: usize,
    /// Rückwärts geflogen (vom letzten zum ersten Punkt)?
    pub reverse: bool,
    pub time: f32,
    pub since_point: f32,
}

pub fn effect_text(kind: EffectKind, station: &str) -> String {
    match kind {
        EffectKind::Supplied => {
            format!("{station} ist versorgt: Munition und Reparatur dort 10 Minuten gratis")
        }
        EffectKind::Cleared => format!(
            "Umgebung von {station} geräumt: 10 Minuten kaum Trümmer und keine Meteore im Umkreis"
        ),
        EffectKind::Beacon => format!(
            "Sender von {station} läuft wieder: Routen in der Nähe auf der Karte, Scanner reicht 10 Minuten weiter"
        ),
    }
}

pub fn effect_short(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Supplied => "versorgt (Munition, Reparatur gratis)",
        EffectKind::Cleared => "geräumt (kaum Trümmer, keine Meteore)",
        EffectKind::Beacon => "Sender aktiv (Routen, Scanner +50 %)",
    }
}

impl SimState {
    /// Was ein erledigter Auftrag in der Welt bewirkt (oder nichts). Gibt den Meldetext zurück.
    pub(crate) fn mission_effect(
        &mut self,
        kind: &MissionKind,
        origin: Option<Owner>,
    ) -> Option<String> {
        let (effect, station) = match kind {
            MissionKind::Delivery { to, .. }
            | MissionKind::Haul { to, .. }
            | MissionKind::Mining { to, .. }
            | MissionKind::Passengers { to, .. } => (EffectKind::Supplied, *to),
            MissionKind::Tow { to, .. }
            | MissionKind::Bulky { to, .. }
            | MissionKind::Capsules { to, .. }
            | MissionKind::Rescue { to, .. } => (EffectKind::Cleared, *to),
            MissionKind::Survey { .. } | MissionKind::Escort { .. } => {
                let Some(Owner::Station(si)) = origin else {
                    return None;
                };
                (EffectKind::Beacon, si)
            }
            MissionKind::Smuggle { .. } => return None,
        };
        Some(self.add_effect(effect, station))
    }

    pub(crate) fn add_effect(&mut self, kind: EffectKind, station: usize) -> String {
        match self
            .effects
            .iter_mut()
            .find(|e| e.kind == kind && e.station == station)
        {
            Some(e) => e.left = EFFECT_SECONDS,
            None => self.effects.push(Effect {
                kind,
                station,
                left: EFFECT_SECONDS,
            }),
        }
        let name = self.world.stations[station].name.clone();
        match kind {
            EffectKind::Cleared => self.clear_debris_near(station),
            EffectKind::Beacon => {
                let at = self.world.stations[station].pos;
                self.explored.reveal(at, 900.0);
                self.reveal_routes_near(at, EFFECT_RADIUS, "Navigationsdaten des Senders");
            }
            EffectKind::Supplied => {}
        }
        effect_text(kind, &name)
    }

    /// Wirkt gerade etwas an dieser Station?
    pub fn effect_at(&self, kind: EffectKind, station: usize) -> bool {
        self.effects
            .iter()
            .any(|e| e.kind == kind && e.station == station)
    }

    /// Wirkt etwas in der Nähe dieses Ortes (geräumt, Sender)?
    pub fn effect_near(&self, kind: EffectKind, p: Vec2) -> bool {
        self.effects.iter().any(|e| {
            e.kind == kind && (self.world.stations[e.station].pos - p).length() < EFFECT_RADIUS
        })
    }

    /// Versorgte Station: Munition und Reparatur kosten nichts.
    pub fn service_free_here(&self) -> bool {
        self.docked_station()
            .is_some_and(|si| self.effect_at(EffectKind::Supplied, si))
    }

    fn clear_debris_near(&mut self, station: usize) {
        let at = self.world.stations[station].pos;
        for b in &mut self.bodies {
            if matches!(b.kind, super::BodyKind::Debris | super::BodyKind::Meteor)
                && (b.pos - at).length() < EFFECT_RADIUS
            {
                b.alive = false;
            }
        }
    }

    /// Jeder Tick: Wirkungen laufen ab, Routenflug, Gerüchte beim Andocken.
    pub(crate) fn update_effects(&mut self) {
        let mut ended = Vec::new();
        for e in &mut self.effects {
            e.left -= DT;
            if e.left <= 0.0 {
                ended.push((e.kind, e.station));
            }
        }
        self.effects.retain(|e| e.left > 0.0);
        for (k, si) in ended {
            let name = self.world.stations[si].name.clone();
            let what = match k {
                EffectKind::Supplied => "Die Vorräte in {} sind wieder knapp",
                EffectKind::Cleared => "Um {} treibt wieder mehr Schrott",
                EffectKind::Beacon => "Der Sender von {} ist wieder still",
            };
            self.toast(what.replace("{}", &name), ToastKind::Info);
        }
        self.update_route_run();
    }

    // -----------------------------------------------------------------------------------------
    // Routen
    // -----------------------------------------------------------------------------------------

    fn route_points(&self, ri: usize) -> Vec<Vec2> {
        self.data.world.routes[ri]
            .points
            .iter()
            .map(|p| v(*p))
            .collect()
    }

    pub fn route_known(&self, ri: usize) -> bool {
        let id = &self.data.world.routes[ri].id;
        self.routes_known.iter().any(|r| r == id)
    }

    /// Route entdecken (einmalig): Meldung, Logbuch, Karte.
    pub(crate) fn discover_route(&mut self, ri: usize, how: &str) {
        if self.route_known(ri) {
            return;
        }
        let r = self.data.world.routes[ri].clone();
        self.routes_known.push(r.id.clone());
        self.events.push(SimEvent::LogFound { id: r.id.clone() });
        self.toast(
            format!("Route entdeckt ({how}): {} – {}", r.name, r.kind.label()),
            ToastKind::Good,
        );
    }

    pub(crate) fn reveal_routes_near(&mut self, at: Vec2, radius: f32, how: &str) {
        for ri in 0..self.data.world.routes.len() {
            if self
                .route_points(ri)
                .iter()
                .any(|p| (*p - at).length() < radius)
            {
                self.discover_route(ri, how);
            }
        }
    }

    /// Scanner: der Ring erfasst einen Wegpunkt.
    pub(crate) fn scan_routes(&mut self, origin: Vec2, r0: f32, r1: f32) {
        for ri in 0..self.data.world.routes.len() {
            let hit = self.route_points(ri).iter().any(|p| {
                let d = (*p - origin).length();
                d >= r0 && d < r1
            });
            if hit {
                self.discover_route(ri, "Scanner");
            }
        }
    }

    /// Gerüchte: an manchen Stationen erzählen Piloten von einer Route.
    pub(crate) fn routes_docked(&mut self, si: usize) {
        let sid = self.world.stations[si].id.clone();
        for ri in 0..self.data.world.routes.len() {
            if self.data.world.routes[ri].known_at.as_deref() == Some(sid.as_str()) {
                self.discover_route(ri, "Gerücht unter Piloten");
            }
        }
    }

    /// Routenflug: an einem Ende einer bekannten Route beginnen, alle Punkte der Reihe nach.
    fn update_route_run(&mut self) {
        if self.ship.destroyed || self.ship.docked.is_some() {
            self.route_run = None;
            return;
        }
        let pos = self.ship.pos;
        if let Some(mut run) = self.route_run.take() {
            run.time += DT;
            run.since_point += DT;
            let pts = self.route_points(run.route);
            let idx = if run.reverse {
                pts.len() - 1 - run.next
            } else {
                run.next
            };
            if (pts[idx] - pos).length() < ROUTE_REACH {
                run.next += 1;
                run.since_point = 0.0;
                if run.next >= pts.len() {
                    self.finish_route(run.route, run.time);
                    self.route_rest = Some(run.route);
                    return;
                }
            }
            if run.since_point < ROUTE_TIMEOUT {
                self.route_run = Some(run);
            }
            return;
        }
        // Start: an einem Ende einer bekannten Route.
        for ri in 0..self.data.world.routes.len() {
            if !self.route_known(ri) {
                continue;
            }
            let pts = self.route_points(ri);
            let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
                continue;
            };
            if self.route_rest == Some(ri) {
                let away = (*first - pos).length() > ROUTE_REACH * 2.0
                    && (*last - pos).length() > ROUTE_REACH * 2.0;
                if away {
                    self.route_rest = None;
                }
                continue;
            }
            let reverse = if (*first - pos).length() < ROUTE_REACH {
                false
            } else if (*last - pos).length() < ROUTE_REACH {
                true
            } else {
                continue;
            };
            let name = self.data.world.routes[ri].name.clone();
            self.route_run = Some(RouteRun {
                route: ri,
                next: 1,
                reverse,
                time: 0.0,
                since_point: 0.0,
            });
            self.toast(
                format!("Route {name}: Zeit läuft – alle Wegpunkte der Reihe nach"),
                ToastKind::Info,
            );
            return;
        }
    }

    fn finish_route(&mut self, ri: usize, time: f32) {
        let r = self.data.world.routes[ri].clone();
        let best = self.route_best.iter_mut().find(|(id, _)| *id == r.id);
        let msg = match best {
            Some((_, b)) if time < *b => {
                let old = *b;
                *b = time;
                format!(
                    "{}: {time:.1} s – neue Bestzeit (vorher {old:.1} s)",
                    r.name
                )
            }
            Some((_, b)) => format!("{}: {time:.1} s (Bestzeit {:.1} s)", r.name, *b),
            None => {
                self.route_best.push((r.id.clone(), time));
                format!("{}: {time:.1} s – erste Zeit im Logbuch", r.name)
            }
        };
        self.toast(format!("Route geflogen · {msg}"), ToastKind::Good);
    }

    pub(crate) fn load_world_state(&mut self, save: &CrewSave) {
        self.effects = save
            .effects
            .iter()
            .filter_map(|e| {
                Some(Effect {
                    kind: e.kind,
                    station: self.data.station_index(&e.station)?,
                    left: e.left,
                })
            })
            .collect();
        self.routes_known = save.routes.clone();
        self.route_best = save.route_best.clone();
    }

    pub(crate) fn save_world_state(&self, save: &mut CrewSave) {
        save.effects = self
            .effects
            .iter()
            .map(|e| EffectSave {
                kind: e.kind,
                station: self.world.stations[e.station].id.clone(),
                left: e.left,
            })
            .collect();
        save.routes = self.routes_known.clone();
        save.route_best = self.route_best.clone();
    }
}
