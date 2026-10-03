//! Tests für Runde 2, Phase 4: Auftraggeber, Lieferungen von Außenposten, Ruf,
//! Auswertung mit Statistik, Wracks, Lackierung, Fog of War im Spielstand.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData, Ore, ToolKind};
use super::economy::{PaintPart, Purchase};
use super::missions::MissionKind;
use super::ship::{CargoKind, CraneState, Loadout};
use super::world::Owner;
use super::{BodyKind, Command, MAX_SLOTS, SimState, TickInput};

fn sim_from(save: &CrewSave) -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, save, Loadout::full(&def), 1)
}

fn sim() -> SimState {
    let data = GameData::embedded().unwrap();
    let mut save = CrewSave::new_game(&data);
    save.credits = 5000;
    sim_from(&save)
}

fn cmd(s: &mut SimState, c: Command) {
    s.step(&TickInput {
        commands: vec![c],
        ..Default::default()
    });
}

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
fn station_offers_come_from_local_npcs() {
    let s = sim();
    let nova = s.data.station_index("nova").unwrap();
    let here: Vec<_> = s
        .offers
        .iter()
        .filter(|m| m.origin == Some(Owner::Station(nova)))
        .collect();
    assert!(!here.is_empty());
    for m in here {
        let g = m.giver.expect("Auftraggeber");
        assert_eq!(s.data.npcs[g].at, "nova");
    }
}

#[test]
fn outpost_ships_ore_to_a_station() {
    let mut s = sim();
    let pi = s
        .world
        .planets
        .iter()
        .position(|p| p.pad.is_some())
        .unwrap();
    let offer = s
        .offers
        .iter()
        .find(|m| m.origin == Some(Owner::Planet(pi)))
        .cloned()
        .expect("Angebot am Außenposten");
    let MissionKind::Delivery { to, mass, .. } = offer.kind else {
        panic!("Lieferung erwartet");
    };
    s.ship.docked = None;
    let pad = s.world.planets[pi].pad.unwrap();
    land_on(&mut s, pad);
    let com_before = s.ship.com;
    cmd(&mut s, Command::AcceptMission { id: offer.id });
    assert_eq!(s.active.len(), 1);
    assert!((s.ship.cargo_mass() - mass).abs() < 1e-3);
    assert!(
        (s.ship.com - com_before).length() > 0.01,
        "Ladung verschiebt den Schwerpunkt"
    );
    let rep_before = s.crew.reputation[to];
    s.ship.docked = None;
    s.dock_at_station(to);
    s.on_docked(Owner::Station(to));
    assert!(s.active.is_empty());
    assert_eq!(s.crew.reputation[to], rep_before + 1);
}

#[test]
fn reputation_brings_more_offers_and_discounts() {
    let mut s = sim();
    let nova = s.data.station_index("nova").unwrap();
    let count = |s: &SimState| {
        s.offers
            .iter()
            .filter(|m| m.origin == Some(Owner::Station(nova)))
            .count()
    };
    let base = count(&s);
    s.ship.hull = s.ship.max_hull * 0.5;
    let price_before = s
        .purchase_info(&Purchase::Service("repair".into()))
        .unwrap()
        .1;
    s.crew.reputation[nova] = 7; // „Geschätzt“
    assert_eq!(s.rep_level_at(nova), 2);
    s.refresh_offers();
    assert_eq!(count(&s), base + 2);
    let price_after = s
        .purchase_info(&Purchase::Service("repair".into()))
        .unwrap()
        .1;
    assert!(price_after < price_before, "{price_after} < {price_before}");
}

#[test]
fn completed_mission_has_a_report_with_awards() {
    let mut s = sim();
    let nova = s.data.station_index("nova").unwrap();
    let m = s
        .offers
        .iter()
        .find(|m| {
            matches!(m.kind, MissionKind::Delivery { from, .. } if from == Owner::Station(nova))
        })
        .cloned();
    let Some(m) = m else { return };
    cmd(&mut s, Command::AcceptMission { id: m.id });
    // Etwas fliegen: zwei Triebwerke schieben unterschiedlich lange.
    let (a, b) = (s.ship.thrusters[0].slot, s.ship.thrusters[1].slot);
    for k in 0..120 {
        let slots = if k < 40 { (1 << a) | (1 << b) } else { 1 << a };
        s.step(&TickInput {
            slots,
            aims: vec![0.0; MAX_SLOTS],
            commands: vec![],
        });
    }
    let MissionKind::Delivery { to, .. } = m.kind else {
        unreachable!()
    };
    s.ship.docked = None;
    s.dock_at_station(to);
    s.on_docked(Owner::Station(to));
    let r = s.report.as_ref().expect("Auswertung");
    assert_eq!(r.mission, m.id);
    assert!(r.stats.slots[a as usize].thrust_time > r.stats.slots[b as usize].thrust_time);
    let awards = r.awards();
    assert_eq!(
        awards
            .iter()
            .find(|x| x.title == "Schubmeister")
            .map(|x| x.slot),
        Some(a)
    );
    assert!(r.reputation.is_some());
}

#[test]
fn collisions_are_counted_per_slot() {
    let mut s = sim();
    let pad = s.world.stations[0].pads[0];
    let p = s.world.pads[pad].clone();
    s.ship.docked = None;
    s.ship.shield = 1000.0;
    s.ship.angle = p.ship_angle();
    s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 2.0);
    s.ship.vel = -p.normal * 9.0;
    for _ in 0..30 {
        s.step(&TickInput::default());
    }
    let total: u32 = s.stats.slots.iter().map(|x| x.collisions).sum();
    assert!(total >= 1, "Aufprall gezählt");
}

#[test]
fn wreck_gives_scrap_and_tears_parts() {
    let mut s = sim();
    let wi = s
        .bodies
        .iter()
        .position(|b| matches!(b.kind, BodyKind::Wreck { .. }))
        .expect("Wrack");
    let (wpos, wr, wid) = (s.bodies[wi].pos, s.bodies[wi].radius, s.bodies[wi].id);
    // Bohren: Schiff direkt daneben, Bohrer zeigt aufs Wrack.
    s.ship.docked = None;
    s.ship.angle = 0.0;
    let di = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == ToolKind::Drill)
        .unwrap();
    let drill_local = s.ship.tools[di].pos - s.ship.com;
    s.ship.pos = wpos + Vec2::new(wr + 2.5, 0.0) - drill_local;
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::ZERO;
    let slot = s.ship.tools[di].slot;
    let aim = (wpos - s.ship.tool_world_pos(di)).to_angle();
    for _ in 0..60 {
        s.step(&TickInput {
            slots: 1 << slot,
            aims: vec![aim; MAX_SLOTS],
            commands: vec![],
        });
    }
    assert!(s.ship.ore_amount(Ore::Schrott) > 0.3, "Schrott gebohrt");

    // Kran am Wrack, dann ein Ruck: ein Bauteil reißt ab und hängt am Haken.
    let ci = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == ToolKind::Crane)
        .unwrap();
    let mount = s.ship.tool_world_pos(ci);
    let rope = (s.bodies.iter().find(|b| b.id == wid).unwrap().pos - mount).length();
    s.ship.tools[ci].crane = CraneState::Attached {
        body: wid,
        rope,
        local: Vec2::ZERO,
    };
    s.ship.vel = (mount - wpos).normalize() * 6.0;
    let parts = |s: &SimState| match s.bodies.iter().find(|b| b.id == wid).unwrap().kind {
        BodyKind::Wreck { parts, .. } => parts,
        _ => 0,
    };
    let before = parts(&s);
    for _ in 0..10 {
        s.step(&TickInput::default());
    }
    assert_eq!(parts(&s), before - 1, "ein Bauteil abgerissen");
    // Das Teil hängt am Haken (oder ist schon eingeholt).
    let hooked = s.bodies.iter().any(|b| {
        matches!(b.kind, BodyKind::Salvage { .. })
            && matches!(s.ship.tools[ci].crane, CraneState::Attached { body, .. } if body == b.id)
    });
    let aboard = s
        .ship
        .cargo
        .iter()
        .any(|c| matches!(c.kind, CargoKind::Salvage { .. }));
    assert!(hooked || aboard);
    // Einholen, verstauen, verkaufen.
    for _ in 0..600 {
        s.step(&TickInput::default());
        if s.ship
            .cargo
            .iter()
            .any(|c| matches!(c.kind, CargoKind::Salvage { .. }))
        {
            break;
        }
    }
    assert!(
        s.ship
            .cargo
            .iter()
            .any(|c| matches!(c.kind, CargoKind::Salvage { .. }))
    );
    s.ship.docked = None;
    s.dock_at_station(0);
    let before = s.crew.credits;
    s.sell_ore();
    assert!(s.crew.credits > before);
    assert!(
        !s.ship
            .cargo
            .iter()
            .any(|c| matches!(c.kind, CargoKind::Salvage { .. }))
    );
}

#[test]
fn paint_and_exploration_survive_saving() {
    let mut s = sim();
    cmd(
        &mut s,
        Command::Buy {
            purchase: Purchase::Paint {
                part: PaintPart::Hull,
                choice: Some(2),
            },
            voter: 0,
        },
    );
    assert_eq!(s.crew.livery(&s.crew.current_ship).hull, Some(2));
    // Irgendwo weit draußen herumfliegen → dort ist es danach entdeckt.
    let far = Vec2::new(1800.0, -1800.0);
    assert!(!s.discovered(far));
    s.ship.docked = None;
    s.ship.pos = far;
    s.ship.vel = Vec2::ZERO;
    for _ in 0..20 {
        s.step(&TickInput::default());
    }
    assert!(s.discovered(far));
    s.crew.reputation[1] = 4;

    let save = s.to_save();
    let t = sim_from(&save);
    assert_eq!(t.crew.livery(&t.crew.current_ship).hull, Some(2));
    assert!(t.discovered(far));
    assert_eq!(t.crew.reputation[1], 4);
    // Werft Vega ist nicht von Anfang an bekannt.
    let vega = t.data.station_index("vega").unwrap();
    let fresh = sim();
    assert!(!fresh.station_known(vega));
    assert!(fresh.station_known(0));
}

#[test]
fn ping_names_the_spot_and_fades() {
    let mut s = sim();
    let wreck = s
        .bodies
        .iter()
        .find(|b| matches!(b.kind, BodyKind::Wreck { .. }))
        .map(|b| b.pos)
        .unwrap();
    cmd(
        &mut s,
        Command::Ping {
            player: 1,
            pos: wreck,
        },
    );
    assert_eq!(s.pings.len(), 1);
    assert!(s.pings[0].label.contains("Frachter") || !s.pings[0].label.is_empty());
    // Ein neuer Ping desselben Spielers ersetzt den alten.
    cmd(
        &mut s,
        Command::Ping {
            player: 1,
            pos: Vec2::new(5000.0, 0.0),
        },
    );
    assert_eq!(s.pings.len(), 1);
    assert!(s.pings[0].label.is_empty());
    for _ in 0..(super::PING_SECONDS * 60.0) as usize + 2 {
        s.step(&TickInput::default());
    }
    assert!(s.pings.is_empty());
}

#[test]
fn hot_join_rebuilds_the_ship_in_flight() {
    let data = GameData::embedded().unwrap();
    let save = CrewSave::new_game(&data);
    let data = Arc::new(data);
    let def = data.ship(&save.current_ship).clone();
    let mut s = SimState::new(
        data,
        &save,
        Loadout {
            thrusters: 2,
            tools: vec![],
        },
        1,
    );
    s.ship.docked = None;
    s.ship.pos = Vec2::new(0.0, 400.0);
    s.ship.vel = Vec2::new(4.0, 1.0);
    let origin = s.ship.origin();
    cmd(
        &mut s,
        Command::SetLoadout {
            thrusters: 3,
            tools: vec![0],
            crew_size: 2,
        },
    );
    assert_eq!(s.ship.thrusters.len(), 3);
    assert_eq!(s.ship.tools.len(), 1);
    assert_eq!(s.crew.size, 2);
    assert!(
        (s.ship.vel - Vec2::new(4.0, 1.0)).length() < 0.05,
        "Schwung bleibt"
    );
    assert!((s.ship.origin() - origin).length() < 0.2, "kein Sprung");
    assert!(def.crew.0 <= def.crew.1);
}

#[test]
fn every_ship_and_mission_type_has_a_crew_size() {
    let s = sim();
    for d in &s.data.ships {
        assert!(d.crew.0 >= 1 && d.crew.0 <= d.crew.1, "{}", d.id);
    }
    for m in &s.offers {
        let (lo, hi) = m.crew(&s);
        assert!(lo >= 1 && lo <= hi);
    }
    let lastesel = s.data.ship("lastesel");
    assert!(lastesel.crew.0 >= 4, "Frachter für große Crews");
}

fn free_at(s: &mut SimState, pos: Vec2) {
    s.ship.docked = None;
    s.ship.pos = pos;
    s.ship.prev_pos = pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.angle = 0.0;
}

fn region(s: &SimState, name: &str) -> Vec2 {
    let r = s
        .data
        .world
        .regions
        .iter()
        .find(|r| r.name == name)
        .unwrap();
    Vec2::new(r.center.0, r.center.1)
}

#[test]
fn solar_wind_pushes_and_nebula_hides() {
    let mut s = sim();
    let corridor = region(&s, "Sonnenwind-Korridor");
    free_at(&mut s, corridor);
    for _ in 0..60 {
        s.step(&TickInput::default());
    }
    assert!(s.ship.vel.x > 1.0, "Wind schiebt: {:?}", s.ship.vel);
    let neb = region(&s, "Schleiernebel");
    assert!(s.sector_at(neb).nebula > 0.5);
    assert!(s.sector_at(Vec2::ZERO).nebula < 1e-3);
}

#[test]
fn debris_drifts_in_the_debris_zone() {
    let mut s = sim();
    let zone = region(&s, "Splitterzone");
    free_at(&mut s, zone + Vec2::new(0.0, -300.0));
    for _ in 0..240 {
        s.step(&TickInput::default());
    }
    let n = s
        .bodies
        .iter()
        .filter(|b| b.alive && matches!(b.kind, BodyKind::Debris))
        .count();
    assert!(n >= 4, "Trümmer: {n}");
}

#[test]
fn scanner_finds_wrecks_and_charts_sell() {
    let mut s = sim();
    let wreck = s
        .bodies
        .iter()
        .find(|b| matches!(b.kind, BodyKind::Wreck { .. }))
        .map(|b| b.pos)
        .unwrap();
    free_at(&mut s, wreck + Vec2::new(200.0, 0.0));
    let si = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == ToolKind::Scanner)
        .expect("Scanner am Driftkutter");
    let slot = s.ship.tools[si].slot;
    s.step(&TickInput {
        slots: 1 << slot,
        aims: vec![0.0; MAX_SLOTS],
        commands: vec![],
    });
    for _ in 0..120 {
        s.step(&TickInput::default());
    }
    assert!(
        s.blips
            .iter()
            .any(|b| b.kind == super::sector::BlipKind::Wreck),
        "Wrack gefunden"
    );
    assert!(s.charts_unsold > 10);
    let n = s.charts_unsold;
    // Zweiter Scan an derselben Stelle bringt keine neuen Daten.
    s.step(&TickInput {
        slots: 0,
        ..Default::default()
    });
    for _ in 0..200 {
        s.step(&TickInput::default());
    }
    s.step(&TickInput {
        slots: 1 << slot,
        aims: vec![0.0; MAX_SLOTS],
        commands: vec![],
    });
    for _ in 0..120 {
        s.step(&TickInput::default());
    }
    assert_eq!(s.charts_unsold, n);
    s.dock_at_station(0);
    let before = s.crew.credits;
    cmd(&mut s, Command::SellCharts);
    assert!(s.crew.credits > before);
    assert_eq!(s.charts_unsold, 0);
    // Gespeichert: dieselben Zellen gelten als kartiert.
    let t = sim_from(&s.to_save());
    assert_eq!(t.surveyed.to_hex(), s.surveyed.to_hex());
}

#[test]
fn random_events_cover_all_kinds() {
    use super::sector::EventKind;
    let mut s = sim();
    free_at(&mut s, Vec2::new(0.0, 500.0));
    let offers_before = s.offers.iter().filter(|m| m.is_distress()).count();
    let (mut shower, mut flare) = (false, false);
    for _ in 0..40 {
        s.event = None;
        s.event_timer = 0.0;
        s.ship.shield = s.ship.max_shield;
        s.update_events();
        match s.event.as_ref().map(|e| e.kind) {
            Some(EventKind::MeteorShower) => shower = true,
            Some(EventKind::SolarFlare) => {
                flare = true;
                assert!(s.ship.shield < s.ship.max_shield * 0.5);
                assert!(s.flare());
            }
            None => {}
        }
    }
    let offers_after = s.offers.iter().filter(|m| m.is_distress()).count();
    assert!(shower && flare);
    assert!(offers_after > offers_before, "spontane Notsignale");
}

#[test]
fn shield_recharges_but_hull_needs_drones() {
    let mut s = sim();
    free_at(&mut s, Vec2::new(0.0, 500.0));
    s.ship.shield = 0.0;
    s.ship.hull = 50.0;
    s.ship.since_hit = 0.0;
    for _ in 0..60 {
        s.step(&TickInput::default());
    }
    assert_eq!(s.ship.shield, 0.0, "erst nach der Pause");
    for _ in 0..(6 * 60) {
        s.step(&TickInput::default());
    }
    assert!(s.ship.shield > 5.0, "lädt nach: {}", s.ship.shield);
    assert_eq!(s.ship.hull, 50.0, "Hülle heilt nicht von selbst");
    // Reparaturdrohnen kaufen → Hülle flickt sich langsam.
    s.dock_at_station(0);
    cmd(
        &mut s,
        Command::Buy {
            purchase: Purchase::Upgrade("drones".into()),
            voter: 0,
        },
    );
    assert!(s.ship.hull_regen > 0.0);
    free_at(&mut s, Vec2::new(0.0, 500.0));
    let h = s.ship.hull;
    for _ in 0..(4 * 60) {
        s.step(&TickInput::default());
    }
    assert!(s.ship.hull > h + 1.0);
}

#[test]
fn passengers_pay_by_comfort_and_bonuses_apply() {
    let mut s = sim();
    let nova = s.data.station_index("nova").unwrap();
    let kepler = s.data.station_index("kepler").unwrap();
    let id = s.next_id();
    let kind = MissionKind::Passengers {
        from: nova,
        to: kepler,
        count: 4,
        comfort: 1.0,
        aboard: false,
    };
    let par = s.par_time(&kind);
    assert!(par > 60.0);
    s.offers.push(super::missions::Mission {
        id,
        kind,
        reward: 400,
        origin: Some(Owner::Station(nova)),
        giver: None,
        start: None,
        top_speed: 0.0,
        par,
    });
    cmd(&mut s, Command::AcceptMission { id });
    assert_eq!(s.active.len(), 1);
    // Vollgas: mehr als 10 m/s² → die Zufriedenheit sinkt.
    free_at(&mut s, Vec2::new(0.0, 500.0));
    let all = (1u32 << s.ship.thrusters.len()) - 1;
    for _ in 0..180 {
        s.step(&TickInput {
            slots: all,
            aims: vec![0.0; MAX_SLOTS],
            commands: vec![],
        });
    }
    let comfort = match s.active[0].kind {
        MissionKind::Passengers { comfort, .. } => comfort,
        _ => unreachable!(),
    };
    assert!(comfort < 0.99 && comfort > 0.5, "Zufriedenheit {comfort}");
    let before = s.crew.credits;
    s.ship.docked = None;
    s.dock_at_station(kepler);
    s.on_docked(Owner::Station(kepler));
    let r = s.report.as_ref().unwrap();
    assert_eq!(r.comfort, Some(comfort));
    assert!(r.reward < 400, "weniger als voll bezahlt");
    assert!(r.bonus_time > 0, "schnell erledigt");
    assert!(r.bonus_clean > 0, "ohne Kollision");
    assert_eq!(
        s.crew.credits - before,
        r.reward + r.bonus_time + r.bonus_clean
    );
}

#[test]
fn every_offer_has_a_par_time() {
    let s = sim();
    for m in &s.offers {
        assert!(m.par >= 60.0, "{:?}", m.kind);
    }
}

#[test]
fn relay_station_is_rebuilt_in_stages() {
    let mut s = sim();
    let si = s.data.station_index("relais").expect("Relais Ost");
    assert_eq!(s.world.stations[si].stage, 0);
    assert!(s.world.stations[si].services.is_empty());
    let staged_pads: Vec<usize> = s.world.stations[si]
        .pads
        .iter()
        .copied()
        .filter(|p| s.world.pads[*p].stage > 0)
        .collect();
    assert!(!staged_pads.is_empty());
    assert!(staged_pads.iter().all(|p| !s.world.pads[*p].enabled));
    let disabled = s
        .world
        .colliders
        .iter()
        .filter(|c| c.station == Some(si) && !c.enabled)
        .count();
    assert!(
        disabled > 0,
        "Teile späterer Etappen kollidieren noch nicht"
    );

    // Etappe 1: 8 t Ferrit + 1 Bauteil, in zwei Lieferungen.
    s.ship.docked = None;
    s.dock_at_station(si);
    s.ship.store(CargoKind::Ore(Ore::Ferrit), 5.0);
    cmd(&mut s, Command::DeliverProject);
    assert_eq!(s.world.stations[si].stage, 0);
    assert!((s.project_here().unwrap().delivered_of(Ore::Ferrit) - 5.0).abs() < 1e-3);
    s.ship.store(CargoKind::Ore(Ore::Ferrit), 5.0);
    s.ship.store(
        CargoKind::Salvage {
            name: "Funkmodul".into(),
            value: 60,
        },
        1.5,
    );
    let credits = s.crew.credits;
    cmd(&mut s, Command::DeliverProject);
    assert_eq!(s.world.stations[si].stage, 1);
    assert!(s.world.stations[si].has(crate::sim::data::Service::Market));
    assert!(s.crew.credits > credits, "Station zahlt für die Etappe");
    assert!(
        (s.ship.ore_amount(Ore::Ferrit) - 2.0).abs() < 1e-3,
        "Rest bleibt an Bord"
    );

    // Gespeichert und wieder geladen: Etappe bleibt.
    let t = sim_from(&s.to_save());
    assert_eq!(t.world.stations[si].stage, 1);
    assert!(t.world.stations[si].has(crate::sim::data::Service::Fuel));
}

fn spawn_rod(s: &mut SimState, pos: Vec2, half_len: f32, mass: f32, mission: u32) -> u32 {
    let id = s.next_id();
    s.bodies.push(super::Body {
        id,
        kind: BodyKind::Bulky {
            mission,
            name: "Antennenmast".into(),
            half_len,
            thick: 0.45,
        },
        pos,
        vel: Vec2::ZERO,
        angle: 0.0,
        ang_vel: 0.0,
        radius: half_len + 0.45,
        mass,
        prev_pos: pos,
        prev_angle: 0.0,
        alive: true,
        seed: 7,
        age: 0.0,
    });
    id
}

#[test]
fn rod_collides_along_its_length() {
    let mut s = sim();
    // Stab liegt waagrecht, nur sein rechtes Ende ragt in eine Wand – die Mitte ist frei.
    let probe = Vec2::new(0.0, 900.0);
    let id = spawn_rod(&mut s, probe, 4.5, 7.0, 0);
    let bi = s.bodies.iter().position(|b| b.id == id).unwrap();
    assert_eq!(
        s.bodies[bi].circles().n,
        9.min(((9.0f32 / 0.45).ceil() as usize + 1).max(2))
    );
    // Ein Körper genau am rechten Ende stößt den Stab an (und dreht ihn).
    let end = s.bodies[bi].anchor(Vec2::new(4.5, 0.0));
    let other = s.next_id();
    s.bodies.push(super::Body {
        id: other,
        kind: BodyKind::Debris,
        pos: end + Vec2::new(0.0, 1.0),
        vel: Vec2::new(0.0, -6.0),
        angle: 0.0,
        ang_vel: 0.0,
        radius: 0.6,
        mass: 3.0,
        prev_pos: end + Vec2::new(0.0, 1.0),
        prev_angle: 0.0,
        alive: true,
        seed: 1,
        age: 0.0,
    });
    for _ in 0..30 {
        s.step(&TickInput::default());
    }
    let b = s.bodies.iter().find(|b| b.id == id).unwrap();
    assert!(
        b.ang_vel.abs() > 0.05,
        "Stoß am Ende dreht den Stab: {}",
        b.ang_vel
    );
}

#[test]
fn rod_grabbed_at_the_end_swings() {
    let mut s = sim();
    free_at(&mut s, Vec2::new(0.0, 900.0));
    let ci = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == ToolKind::Crane)
        .unwrap();
    let mount = s.ship.tool_world_pos(ci);
    let id = spawn_rod(&mut s, mount + Vec2::new(6.0, -6.0), 4.5, 7.0, 0);
    s.ship.tools[ci].crane = CraneState::Attached {
        body: id,
        rope: 5.0,
        local: Vec2::new(-4.5, 0.0),
    };
    let all = (1u32 << s.ship.thrusters.len()) - 1;
    for _ in 0..120 {
        s.step(&TickInput {
            slots: all,
            aims: vec![0.0; MAX_SLOTS],
            commands: vec![],
        });
    }
    let b = s.bodies.iter().find(|b| b.id == id).unwrap();
    assert!(b.angle.abs() > 0.2, "Stab schwingt am Seil: {}", b.angle);
    assert!(
        s.ship.tools[ci].strain > 0.0,
        "Seilbelastung wird angezeigt"
    );
}

#[test]
fn bulky_salvage_needs_the_drop_zone_and_a_capable_ship() {
    let mut s = sim();
    let nova = s.data.station_index("nova").unwrap();
    assert!(s.bulky_feasible(14.0).is_ok());
    assert!(s.bulky_feasible(500.0).is_err(), "viel zu schwer");
    let id = s.next_id();
    let kind = MissionKind::Bulky {
        site: Vec2::new(300.0, 300.0),
        to: nova,
        name: "Antennenmast".into(),
        body: None,
        mass: 7.0,
        half_len: 4.6,
        thick: 0.45,
    };
    let par = s.par_time(&kind);
    s.offers.push(super::missions::Mission {
        id,
        kind,
        reward: 380,
        origin: Some(Owner::Station(nova)),
        giver: None,
        start: None,
        top_speed: 0.0,
        par,
    });
    cmd(&mut s, Command::AcceptMission { id });
    let bid = match s.active[0].kind {
        MissionKind::Bulky { body: Some(b), .. } => b,
        _ => panic!("Objekt fehlt"),
    };
    // Neben der Station, aber nicht in der Ablage: nicht erledigt.
    let (zone, r) = s.drop_point(nova);
    let outside = s.world.stations[nova].pos + Vec2::new(0.0, 70.0);
    assert!((outside - zone).length() > r);
    let b = s.bodies.iter_mut().find(|b| b.id == bid).unwrap();
    b.pos = outside;
    b.prev_pos = outside;
    s.step(&TickInput::default());
    assert_eq!(s.active.len(), 1, "außerhalb der Ablage zählt nicht");
    let b = s.bodies.iter_mut().find(|b| b.id == bid).unwrap();
    b.pos = zone;
    b.prev_pos = zone;
    b.vel = Vec2::ZERO;
    s.step(&TickInput::default());
    assert!(s.active.is_empty(), "in der Ablage erledigt");
}

#[test]
fn precision_drilling_frees_a_vein_only_when_held_on_target() {
    use super::precision::precision_target;
    let mut s = sim();
    // Einen erzhaltigen Asteroiden zur Ader machen und ruhigstellen.
    let bi = s
        .bodies
        .iter()
        .position(|b| matches!(b.kind, BodyKind::Asteroid { ore: Some(_), .. }) && b.radius > 2.0)
        .unwrap();
    if let BodyKind::Asteroid { vein, .. } = &mut s.bodies[bi].kind {
        *vein = true;
    }
    let (apos, ar) = (Vec2::new(0.0, 1200.0), s.bodies[bi].radius);
    let aid = s.bodies[bi].id;
    let b = &mut s.bodies[bi];
    b.pos = apos;
    b.prev_pos = apos;
    b.vel = Vec2::ZERO;
    b.ang_vel = 0.0;
    let (target, _) = precision_target(&s.bodies[bi]).unwrap();
    let out = (target - apos).normalize();
    let di = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == ToolKind::Drill)
        .unwrap();
    s.ship.docked = None;
    // Nase (mit dem Bohrer) zeigt zum Asteroiden.
    s.ship.angle = (-out).to_angle() - std::f32::consts::FRAC_PI_2;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    let drill_local = crate::sim::geom::rot(s.ship.tools[di].pos - s.ship.com, s.ship.angle);
    s.ship.pos = target + out * 2.4 - drill_local;
    s.ship.prev_pos = s.ship.pos;
    let slot = s.ship.tools[di].slot;
    let mut cores = 0;
    for _ in 0..(5 * 60) {
        let body = s.bodies.iter().find(|b| b.id == aid).unwrap();
        let t = precision_target(body).map(|x| x.0).unwrap_or(target);
        let aim = (t - s.ship.tool_world_pos(di)).to_angle();
        // Die Crew hält Schiff und Ziel ruhig (hier vereinfacht: beide stehen still).
        s.ship.vel = Vec2::ZERO;
        s.ship.ang_vel = 0.0;
        if let Some(b) = s.bodies.iter_mut().find(|b| b.id == aid) {
            b.vel = Vec2::ZERO;
            b.ang_vel = 0.0;
        }
        s.step(&TickInput {
            slots: 1 << slot,
            aims: vec![aim; MAX_SLOTS],
            commands: vec![],
        });
        cores = s
            .bodies
            .iter()
            .filter(|b| matches!(&b.kind, BodyKind::Salvage { name, .. } if name.starts_with("Kristallkern")))
            .count();
        if cores > 0 {
            break;
        }
    }
    assert_eq!(cores, 1, "Kristallkern nach ruhigem Bohren (r = {ar})");
    let body = s.bodies.iter().find(|b| b.id == aid).unwrap();
    assert!(precision_target(body).is_none(), "Ader ist erschöpft");
}
