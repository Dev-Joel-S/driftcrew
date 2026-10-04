//! Tests für Phase 16: Wirkungen erledigter Aufträge und versteckte Routen.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, EffectKind, GameData, v};
use super::economy::Purchase;
use super::effects::{EFFECT_SECONDS, ROUTE_REACH};
use super::missions::{Mission, MissionKind};
use super::ship::Loadout;
use super::world::Owner;
use super::{BodyKind, DT, SimEvent, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let mut save = CrewSave::new_game(&data);
    save.credits = 5_000;
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn toasts(s: &SimState) -> Vec<String> {
    s.events
        .iter()
        .filter_map(|e| match e {
            SimEvent::Toast { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn complete(s: &mut SimState, kind: MissionKind, origin: Option<Owner>) {
    let id = s.next_id();
    s.active.push(Mission {
        id,
        kind,
        reward: 300,
        origin,
        giver: None,
        start: None,
        top_speed: 0.0,
        max_strain: None,
        par: 600.0,
    });
    s.events.clear();
    let idx = s.active.len() - 1;
    s.complete_mission(idx);
}

#[test]
fn delivery_supplies_the_station_and_service_is_free_there() {
    let mut s = sim();
    let si = s.docked_station().unwrap();
    s.ship.hull = s.ship.max_hull * 0.5;
    let price_before = s
        .purchase_info(&Purchase::Service("repair".into()))
        .map(|(_, p)| p);
    complete(
        &mut s,
        MissionKind::Delivery {
            from: Owner::Station(si),
            to: si,
            cargo: "Test".into(),
            mass: 1.0,
            traits: Default::default(),
        },
        Some(Owner::Station(si)),
    );
    assert!(s.effect_at(EffectKind::Supplied, si));
    let report = s.report.as_ref().unwrap();
    assert!(
        report.effect.as_deref().unwrap_or("").contains("versorgt"),
        "Auswertung nennt die Wirkung: {:?}",
        report.effect
    );
    assert!(toasts(&s).iter().any(|t| t.contains("gratis")));
    if let Ok(before) = price_before {
        assert!(before > 0);
        let (_, after) = s
            .purchase_info(&Purchase::Service("repair".into()))
            .unwrap();
        assert_eq!(after, 0, "Reparatur gratis");
    }
    // Läuft nach einer Weile ab.
    for e in &mut s.effects {
        e.left = DT * 2.0;
    }
    for _ in 0..3 {
        s.step(&TickInput::default());
    }
    assert!(!s.effect_at(EffectKind::Supplied, si));
}

#[test]
fn salvage_clears_debris_and_meteors_around_the_target() {
    let mut s = sim();
    // Eine Station in der Nähe eines Meteorfelds suchen.
    let zone = s.data.world.meteor_zones[0].clone();
    let c = v(zone.center);
    let si = (0..s.world.stations.len())
        .min_by(|a, b| {
            (s.world.stations[*a].pos - c)
                .length()
                .total_cmp(&(s.world.stations[*b].pos - c).length())
        })
        .unwrap();
    // Schiff mitten ins Meteorfeld: ohne Räumung kommen Meteore.
    let fly_in = |s: &mut SimState| {
        s.ship.docked = None;
        s.ship.pos = c + Vec2::new(0.0, zone.radius + 100.0);
        s.ship.vel = Vec2::ZERO;
        let meteors = |s: &SimState| {
            s.bodies
                .iter()
                .filter(|b| b.alive && matches!(b.kind, BodyKind::Meteor))
                .count()
        };
        let mut seen = 0;
        for _ in 0..600 {
            s.step(&TickInput::default());
            s.ship.pos = c + Vec2::new(0.0, zone.radius + 100.0);
            s.ship.vel = Vec2::ZERO;
            seen = seen.max(meteors(s));
        }
        seen
    };
    let mut a = s.clone();
    assert!(fly_in(&mut a) > 0, "Meteore ohne Räumung");
    complete(
        &mut s,
        MissionKind::Tow {
            site: c,
            to: si,
            name: "Test".into(),
            body: None,
        },
        None,
    );
    assert!(s.effect_near(EffectKind::Cleared, c));
    assert_eq!(fly_in(&mut s), 0, "geräumt: keine Meteore");
}

#[test]
fn routes_are_found_by_scanner_rumour_and_beacon() {
    let mut s = sim();
    let routes = s.data.world.routes.clone();
    assert!(routes.len() >= 4);
    assert!(s.routes_known.is_empty());
    // Gerücht beim Andocken.
    let (ri, at) = routes
        .iter()
        .enumerate()
        .find_map(|(i, r)| Some((i, r.known_at.clone()?)))
        .unwrap();
    let si = s.data.station_index(&at).unwrap();
    s.dock_at_station(si);
    s.routes_docked(si);
    assert!(s.route_known(ri));
    // Scanner: Ring erfasst einen Wegpunkt.
    let rj = (0..routes.len())
        .find(|&i| routes[i].known_at.is_none())
        .unwrap();
    let p = v(routes[rj].points[1]);
    s.scan_routes(p + Vec2::new(30.0, 0.0), 0.0, 50.0);
    assert!(s.route_known(rj));
    assert!(toasts(&s).iter().any(|t| t.contains("Route entdeckt")));
    // Sender (Messflug abgeschlossen) deckt Routen in der Nähe auf.
    let mut t = sim();
    let nova = t.data.station_index("nova").unwrap();
    complete(
        &mut t,
        MissionKind::Survey {
            sites: vec![0],
            done: 1,
            hold: 0.0,
        },
        Some(Owner::Station(nova)),
    );
    assert!(t.effect_at(EffectKind::Beacon, nova));
    assert!(!t.routes_known.is_empty(), "Navigationsdaten des Senders");
    // Gespeichert.
    let save = t.to_save();
    assert_eq!(save.routes, t.routes_known);
    assert_eq!(save.effects.len(), 1);
    assert!(save.effects[0].left > EFFECT_SECONDS - 5.0);
}

#[test]
fn flying_a_route_end_to_end_records_a_time() {
    let mut s = sim();
    let ri = 0;
    let id = s.data.world.routes[ri].id.clone();
    s.routes_known.push(id.clone());
    let pts: Vec<Vec2> = s.data.world.routes[ri]
        .points
        .iter()
        .map(|p| v(*p))
        .collect();
    s.ship.docked = None;
    // Von Punkt zu Punkt „teleportieren“ und je ein paar Ticks verweilen.
    for p in &pts {
        for _ in 0..20 {
            s.ship.pos = *p + Vec2::new(ROUTE_REACH * 0.3, 0.0);
            s.ship.vel = Vec2::ZERO;
            s.step(&TickInput::default());
        }
    }
    let best = s.route_best.iter().find(|(r, _)| *r == id);
    assert!(best.is_some(), "Zeit eingetragen");
    assert!(best.unwrap().1 > 0.5);
    assert!(s.route_run.is_none());
}
