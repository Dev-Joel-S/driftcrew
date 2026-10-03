//! Tests für die Systeme aus Runde 2, Phase 3: Schwerkraft nur an Anomalien, Landezonen,
//! Rettungskapsel, Triebwerksschäden, Treibstoff, Kranseil und Schwerlast.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData, Ore};
use super::dock::RESPAWN_SECONDS;
use super::missions::{Mission, MissionKind};
use super::ship::{CraneState, Loadout};
use super::tools::{EMERGENCY_THRUST, TOW_ROPE, thruster_works};
use super::world::Owner;
use super::{BodyKind, Command, DT, MAX_SLOTS, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

/// Schiff frei im Raum, weit weg von allem.
fn free(s: &mut SimState, pos: Vec2) {
    s.ship.docked = None;
    s.ship.pos = pos;
    s.ship.prev_pos = pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.angle = 0.0;
}

fn press(slots: u32) -> TickInput {
    TickInput {
        slots,
        aims: vec![0.0; MAX_SLOTS],
        commands: vec![],
    }
}

/// Schiff auf eine Plattform stellen (ohne Anflug).
fn land_on(s: &mut SimState, pad: usize) {
    let p = s.world.pads[pad].clone();
    s.ship.angle = p.ship_angle();
    s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 0.02);
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.docked = Some(pad);
}

#[test]
fn planets_do_not_pull() {
    let mut s = sim();
    let viridia = s.world.planets[0].clone();
    // 20 m über der Oberfläche, in Ruhe: bleibt in Ruhe.
    free(&mut s, viridia.pos + Vec2::new(0.0, viridia.radius + 20.0));
    assert_eq!(s.world.gravity(s.ship.pos), Vec2::ZERO);
    for _ in 0..120 {
        s.step(&TickInput::default());
    }
    assert!(s.ship.vel.length() < 1e-4, "vel = {:?}", s.ship.vel);
}

#[test]
fn black_hole_pulls_and_swallows_into_escape_pod() {
    let mut s = sim();
    s.crew.credits = 1000;
    let bh = s
        .world
        .anomalies
        .iter()
        .find(|a| a.is_black_hole())
        .cloned()
        .expect("Schwarzes Loch in world.ron");
    // Im Sog: Anziehung zeigt zum Zentrum.
    let probe = bh.pos + Vec2::new(bh.radius * 0.5, 0.0);
    assert!(s.world.gravity(probe).x < -1.0);
    // Am Horizont: Schiff ist verloren, Kapsel fliegt, Kasse zahlt die Bergung.
    let fee = s.salvage_fee_now();
    assert!(fee > 0 && fee < 1000);
    free(&mut s, bh.pos + Vec2::new(bh.core_radius * 0.5, 0.0));
    s.step(&TickInput::default());
    assert!(s.ship.destroyed);
    let pod0 = s.escape.as_ref().expect("Rettungskapsel").pos;
    assert_eq!(s.crew.credits, 1000 - fee);
    assert_eq!(s.salvage_fee, fee);
    for _ in 0..30 {
        s.step(&TickInput::default());
    }
    assert!(
        (s.escape.as_ref().unwrap().pos - pod0).length() > 1.0,
        "Kapsel fliegt"
    );
    for _ in 0..((RESPAWN_SECONDS / DT) as usize) {
        s.step(&TickInput::default());
    }
    assert!(!s.ship.destroyed);
    assert!(s.escape.is_none());
    assert!(s.ship.docked.is_some(), "an der Heimatstation abgestellt");
}

#[test]
fn failed_thruster_gives_no_thrust_and_damage_hits_nearby_thruster() {
    let mut s = sim();
    free(&mut s, Vec2::new(0.0, 400.0));
    s.ship.thrusters[0].health = 0.0;
    let slot = s.ship.thrusters[0].slot;
    for _ in 0..30 {
        s.step(&press(1 << slot));
    }
    assert!(s.ship.vel.length() < 1e-4 && s.ship.ang_vel.abs() < 1e-4);

    // Treffer direkt am rechten äußeren Triebwerk beschädigt nur dieses (und Nachbarn).
    let mut s = sim();
    free(&mut s, Vec2::new(0.0, 400.0));
    s.ship.shield = 0.0;
    let last = s.ship.thrusters.len() - 1;
    let at = s.ship.to_world(s.ship.thrusters[last].pos);
    s.ship_damage(30.0, at, true);
    assert!(s.ship.thrusters[last].health < 0.4);
    assert_eq!(s.ship.thrusters[0].health, 1.0, "linkes Triebwerk weit weg");
}

#[test]
fn damaged_thruster_stutters_deterministically() {
    let mut s = sim();
    let mut t = s.ship.thrusters[1].clone();
    t.health = 0.3;
    let on = (0..600u64).filter(|k| thruster_works(&t, *k)).count();
    assert!(
        on > 60 && on < 540,
        "stottert, fällt aber nicht ganz aus: {on}/600"
    );
    let again = (0..600u64).filter(|k| thruster_works(&t, *k)).count();
    assert_eq!(on, again);
    t.health = 0.9;
    assert!((0..600u64).all(|k| thruster_works(&t, k)));
    s.ship.thrusters[1].health = 1.0;
}

#[test]
fn fuel_burns_and_emergency_reserve_keeps_you_moving() {
    let mut s = sim();
    free(&mut s, Vec2::new(0.0, 400.0));
    let all = (1u32 << s.ship.thrusters.len()) - 1;
    let f0 = s.ship.fuel;
    for _ in 0..60 {
        s.step(&press(all));
    }
    assert!(s.ship.fuel < f0, "Verbrauch");
    let v_full = s.ship.vel.length();

    let mut s = sim();
    free(&mut s, Vec2::new(0.0, 400.0));
    s.ship.fuel = 0.0;
    for _ in 0..60 {
        s.step(&press(all));
    }
    let ratio = s.ship.vel.length() / v_full;
    assert!(
        (ratio - EMERGENCY_THRUST).abs() < 0.03,
        "Notreserve: {ratio}"
    );
}

#[test]
fn gentle_landing_on_zone_and_drilling_while_landed() {
    let mut s = sim();
    let (pi, zone) = s
        .world
        .planets
        .iter()
        .enumerate()
        .find_map(|(i, p)| p.zones.first().map(|z| (i, *z)))
        .expect("Landezone");
    let p = s.world.pads[zone].clone();
    assert!(p.zone);
    s.ship.docked = None;
    s.ship.dock_cooldown = 0.0;
    s.ship.angle = p.ship_angle();
    s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 2.0);
    s.ship.vel = -p.normal * 1.2;
    for _ in 0..240 {
        s.step(&TickInput::default());
        if s.ship.docked.is_some() {
            break;
        }
    }
    assert_eq!(s.ship.docked, Some(zone));
    assert!(s.landed_in_zone());
    assert!(!s.can_sell_here());

    // Bohren: schräg nach unten auf das Vorkommen neben der Zone.
    land_on(&mut s, zone);
    let planet = s.world.planets[pi].clone();
    let dep = planet.deposits[0].clone();
    let zone_angle = f32::atan2(p.normal.y, p.normal.x);
    let side = super::world::angle_diff(dep.angle, zone_angle).signum();
    let a = zone_angle + side * 3.5 / planet.radius;
    let target = planet.pos + Vec2::new(a.cos(), a.sin()) * planet.radius;
    let di = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == super::data::ToolKind::Drill)
        .unwrap();
    let mount = s.ship.tool_world_pos(di);
    let aim = (target - mount).to_angle();
    let slot = s.ship.tools[di].slot;
    let mut inp = press(1 << slot);
    inp.aims = vec![aim; MAX_SLOTS];
    let before = s.ship.ore_amount(planet.ore);
    for _ in 0..90 {
        s.step(&inp);
    }
    assert_eq!(s.ship.docked, Some(zone), "bleibt beim Bohren gelandet");
    assert!(
        s.ship.ore_amount(planet.ore) > before + 0.5,
        "Erz abgebaut: {}",
        s.ship.ore_amount(planet.ore)
    );
}

fn spawn_crate(s: &mut SimState, pos: Vec2, mass: f32) -> u32 {
    let id = s.next_id();
    s.bodies.push(super::Body {
        id,
        kind: BodyKind::Crate {
            mission: 0,
            name: "Test".into(),
        },
        pos,
        vel: Vec2::ZERO,
        angle: 0.0,
        ang_vel: 0.0,
        radius: 1.5,
        mass,
        prev_pos: pos,
        prev_angle: 0.0,
        alive: true,
        seed: 1,
        age: 0.0,
    });
    id
}

#[test]
fn crane_rope_is_a_hard_length_limit() {
    let mut s = sim();
    free(&mut s, Vec2::new(0.0, 400.0));
    let ci = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == super::data::ToolKind::Crane)
        .unwrap();
    let mount = s.ship.tool_world_pos(ci);
    let id = spawn_crate(&mut s, mount + Vec2::new(0.0, -TOW_ROPE), 9.0);
    s.ship.tools[ci].crane = CraneState::Attached {
        body: id,
        rope: TOW_ROPE,
    };
    // Vollgas nach oben: die Kiste muss mitkommen, das Seil bleibt höchstens etwas gedehnt.
    let all = (1u32 << s.ship.thrusters.len()) - 1;
    let mut max_stretch = 0.0f32;
    for _ in 0..180 {
        s.step(&press(all));
        let b = s.bodies.iter().find(|b| b.id == id).unwrap();
        let d = (b.pos - s.ship.tool_world_pos(ci)).length();
        max_stretch = max_stretch.max(d - TOW_ROPE);
    }
    assert!(matches!(
        s.ship.tools[ci].crane,
        CraneState::Attached { .. }
    ));
    let b = s.bodies.iter().find(|b| b.id == id).unwrap();
    assert!(b.vel.y > 2.0, "Kiste wird gezogen: {:?}", b.vel);
    assert!(max_stretch < 0.8, "Seil dehnt sich kaum: {max_stretch}");
}

#[test]
fn heavy_haul_is_towed_and_completes_at_target() {
    let mut s = sim();
    let nova = s.data.station_index("nova").unwrap();
    let kepler = s.data.station_index("kepler").unwrap();
    let id = s.next_id();
    s.offers.push(Mission {
        id,
        kind: MissionKind::Haul {
            from: nova,
            to: kepler,
            cargo: "Druckkessel".into(),
            mass: 9.0,
            body: None,
        },
        reward: 300,
        origin: Some(Owner::Station(nova)),
        giver: None,
        start: None,
        top_speed: 0.0,
    });
    s.step(&TickInput {
        commands: vec![Command::AcceptMission { id }],
        ..Default::default()
    });
    assert_eq!(s.active.len(), 1);
    assert!(
        s.ship.cargo_mass() < 1e-4,
        "Schwerlast liegt nicht im Frachtraum"
    );
    let crate_id = match s.active[0].kind {
        MissionKind::Haul { body: Some(b), .. } => b,
        _ => panic!("Kiste fehlt"),
    };
    // Kiste in die Nähe von Kepler bringen → Auftrag erfüllt.
    let target = s.world.stations[kepler].pos + Vec2::new(0.0, 30.0);
    let credits = s.crew.credits;
    let b = s.bodies.iter_mut().find(|b| b.id == crate_id).unwrap();
    b.pos = target;
    b.prev_pos = target;
    s.step(&TickInput::default());
    assert!(s.active.is_empty());
    assert_eq!(s.crew.credits, credits + 300);
    assert!(!s.bodies.iter().any(|b| b.id == crate_id && b.alive));
}

#[test]
fn outpost_prices_favour_scarce_ore() {
    let s = sim();
    let viridia = Owner::Planet(0);
    assert_eq!(s.world.planets[0].ore, Ore::Kobalt);
    // Am Kobalt-Planeten gibt es wenig für Kobalt.
    assert!(
        s.ore_price_at(viridia, Ore::Kobalt) < s.data.shop.ore_price(Ore::Kobalt),
        "Kobalt am Kobalt-Planeten billiger"
    );
    let best = s.best_ore_price(Ore::Kobalt).unwrap();
    assert!(best.1 > s.data.shop.ore_price(Ore::Kobalt));
}

#[test]
fn black_hole_has_a_point_of_no_return() {
    let s = sim();
    let bh = s
        .world
        .anomalies
        .iter()
        .find(|a| a.is_black_hole())
        .unwrap();
    let accel = s.ship.thrusters.iter().map(|t| t.thrust).sum::<f32>() / s.ship.mass;
    let r = bh.no_return_radius(accel);
    assert!(
        r > bh.core_radius * 3.0 && r < bh.radius * 0.5,
        "Kein Zurück bei {r} m"
    );
    // Mit Notreserve liegt die Grenze deutlich weiter draußen.
    assert!(bh.no_return_radius(accel * EMERGENCY_THRUST) > r * 1.5);
}
