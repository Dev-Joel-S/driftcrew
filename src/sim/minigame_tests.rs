//! Tests für Phase 10: Reparatur im Takt, Notreparatur, Andockport hacken, Ersatzteil am
//! Kran, ruhige Hand beim Bohren, sanft geführte Last.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData, ToolKind};
use super::minigame::{
    BEAT_WINDOW, HOLD_TO_LEAVE, HULL_PATCH_CAP, PATCH_HEALTH, PATCH_HITS, REST_TO_REPAIR,
    beat_offset,
};
use super::missions::{Mission, MissionKind};
use super::ship::{CargoKind, CraneState, Loadout, steady_factor};
use super::world::Owner;
use super::{DT, MAX_SLOTS, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn input(slots: u32) -> TickInput {
    TickInput {
        slots,
        aims: vec![0.0; MAX_SLOTS],
        commands: vec![],
    }
}

/// Frei im Raum, in Ruhe, weit weg von allem.
fn park(s: &mut SimState) {
    s.ship.docked = None;
    s.ship.pos = Vec2::new(-400.0, -300.0);
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.bodies.clear();
}

/// Bis kurz vor den nächsten Schlag warten (ohne Eingabe), dann `slot` einmal antippen.
fn tap_on_beat(s: &mut SimState, slot: u8, held: u32) {
    while beat_offset(s.time + DT) > BEAT_WINDOW * 0.3 {
        s.step(&input(held));
    }
    s.step(&input(held | 1 << slot));
    s.step(&input(held));
}

/// Antippen genau zwischen zwei Schlägen.
fn tap_off_beat(s: &mut SimState, slot: u8) {
    while (beat_offset(s.time + DT) - super::minigame::BEAT * 0.5).abs() > DT {
        s.step(&input(0));
    }
    s.step(&input(1 << slot));
    s.step(&input(0));
}

#[test]
fn failed_thruster_is_patched_in_rhythm_even_in_flight() {
    let mut s = sim();
    park(&mut s);
    s.ship.thrusters[0].health = 0.0;
    let slot = s.ship.thrusters[0].slot;
    // Daneben getroffen: kein Fortschritt.
    tap_off_beat(&mut s, slot);
    assert_eq!(s.repair.thrusters[0], 0.0);
    assert!(s.repair.beats.iter().any(|b| !b.hit));
    // Die tote Taste schiebt nicht – das Schiff bleibt in Ruhe.
    assert!(s.ship.vel.length() < 1e-3);
    for _ in 0..PATCH_HITS as usize {
        tap_on_beat(&mut s, slot, 0);
    }
    assert_eq!(s.ship.thrusters[0].health, PATCH_HEALTH);
    assert!(!s.ship.thrusters[0].failed());
    assert!(
        s.ship.thrusters[0].stuttering(),
        "geflickt, aber nicht rund"
    );
}

#[test]
fn emergency_repair_starts_at_rest_and_patches_the_hull() {
    let mut s = sim();
    park(&mut s);
    s.ship.hull = s.ship.max_hull * 0.3;
    // Noch nicht ruhig genug lange: nichts.
    for _ in 0..((REST_TO_REPAIR * 0.5) / DT) as usize {
        s.step(&input(0));
    }
    assert!(!s.repair.active);
    for _ in 0..((REST_TO_REPAIR * 0.6) / DT) as usize {
        s.step(&input(0));
    }
    assert!(
        s.repair.active,
        "nach einer Weile Ruhe beginnt die Notreparatur"
    );
    let hull = s.ship.hull;
    let slot = s.ship.thrusters[2].slot;
    for _ in 0..4 {
        tap_on_beat(&mut s, slot, 0);
    }
    assert!(s.ship.hull > hull + 5.0);
    // Getippt wurde ein Triebwerk – es hat nicht geschoben.
    assert!(s.ship.vel.length() < 1e-3);
    // Lange halten beendet die Notreparatur, dann schiebt die Taste wieder.
    for _ in 0..((HOLD_TO_LEAVE + 0.1) / DT) as usize {
        s.step(&input(1 << slot));
    }
    assert!(!s.repair.active);
    assert!(s.ship.vel.length() > 0.05);
    // Mehr als bis zur Obergrenze flickt die Notreparatur nicht.
    let mut t = sim();
    park(&mut t);
    t.ship.hull = t.ship.max_hull * HULL_PATCH_CAP - 1.0;
    for _ in 0..((REST_TO_REPAIR + 0.2) / DT) as usize {
        t.step(&input(0));
    }
    assert!(t.repair.active);
    let slot = t.ship.thrusters[1].slot;
    tap_on_beat(&mut t, slot, 0);
    assert!(t.ship.hull <= t.ship.max_hull * HULL_PATCH_CAP + 1e-3);
    assert!(!t.repair.active, "fertig – den Rest macht die Station");
}

fn near_smuggler_port(s: &mut SimState) -> usize {
    let si = s.data.station_index("nebelhafen").unwrap();
    let pad = s.world.stations[si].pads[0];
    let p = s.world.pads[pad].clone();
    s.ship.docked = None;
    s.ship.angle = p.ship_angle();
    s.ship.pos = p.center + p.normal * 14.0;
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    si
}

#[test]
fn smuggler_port_must_be_hacked_before_docking() {
    let mut s = sim();
    let si = near_smuggler_port(&mut s);
    assert!(s.port_locked(si));
    assert!(
        s.dock_guide().is_none(),
        "gesperrter Port bietet keine Plattform an"
    );
    s.step(&input(0));
    let h = s.hack.clone().expect("Hack beginnt in Portnähe");
    assert_eq!(h.station, si);
    assert_eq!(h.pattern.len(), 5);
    assert!(h.pattern.windows(2).all(|w| w[0] != w[1]));
    // Muster der Reihe nach: der Port geht auf. Die Tasten steuern dabei nicht.
    for &slot in &h.pattern {
        s.step(&input(1 << slot));
        s.step(&input(0));
    }
    assert!(s.hack.is_none());
    assert!(!s.port_locked(si));
    assert!(s.ship.vel.length() < 0.5);
    assert!(s.dock_guide().is_some());
}

#[test]
fn wrong_key_triggers_alarm_and_lockout() {
    let mut s = sim();
    let si = near_smuggler_port(&mut s);
    s.step(&input(0));
    let h = s.hack.clone().unwrap();
    let wrong = (0..MAX_SLOTS as u8)
        .find(|sl| *sl != h.pattern[0] && s.ship.thrusters.iter().any(|t| t.slot == *sl))
        .unwrap();
    let shield = s.ship.shield;
    assert!(shield > 0.0);
    s.step(&input(1 << wrong));
    assert!(s.hack.is_none());
    assert_eq!(s.ship.shield, 0.0, "Störimpuls");
    assert!(s.port_locked(si));
    // Gesperrt: kein neuer Versuch, bis die Sperre abgelaufen ist.
    s.step(&input(0));
    s.step(&input(0));
    assert!(s.hack.is_none());
    s.hack_lockout[si] = s.time;
    near_smuggler_port(&mut s);
    s.step(&input(0));
    assert!(s.hack.is_some());
}

#[test]
fn hack_runs_out_of_time() {
    let mut s = sim();
    near_smuggler_port(&mut s);
    s.step(&input(0));
    let total = s.hack.as_ref().unwrap().total;
    for _ in 0..((total + 0.2) / DT) as usize {
        s.step(&input(0));
    }
    assert!(s.hack.is_none());
    assert_eq!(s.ship.shield, 0.0);
}

#[test]
fn spare_part_on_the_crane_fixes_a_failed_thruster() {
    let mut s = sim();
    park(&mut s);
    s.ship.thrusters[0].health = 0.0;
    let crane = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == ToolKind::Crane)
        .unwrap();
    let cslot = s.ship.tools[crane].slot;
    let mount = s.ship.tool_world_pos(crane);
    let target = s.ship.to_world(s.ship.thrusters[0].pos);
    let aim = f32::atan2(target.y - mount.y, target.x - mount.x);
    let mut aims = vec![0.0; MAX_SLOTS];
    aims[cslot as usize] = aim;
    let hold = |s: &mut SimState, on: bool| {
        s.step(&TickInput {
            slots: if on { 1 << cslot } else { 0 },
            aims: aims.clone(),
            commands: vec![],
        })
    };
    // Ohne Ersatzteil: der Kran fährt ganz normal aus.
    hold(&mut s, false);
    hold(&mut s, true);
    assert!(matches!(
        s.ship.tools[crane].crane,
        CraneState::Extending { .. }
    ));
    for _ in 0..240 {
        hold(&mut s, false);
    }
    assert!(matches!(s.ship.tools[crane].crane, CraneState::Idle));
    // Mit Ersatzteil: halten, bis es sitzt.
    s.ship.store(
        CargoKind::Salvage {
            name: "Kühlrippe".into(),
            value: 40,
        },
        0.5,
    );
    hold(&mut s, true);
    assert!(matches!(
        s.ship.tools[crane].crane,
        CraneState::Patching { .. }
    ));
    for _ in 0..((super::minigame::CRANE_PATCH_TIME + 0.2) / DT) as usize {
        hold(&mut s, true);
    }
    assert_eq!(s.ship.thrusters[0].health, PATCH_HEALTH);
    assert!(
        !s.ship
            .cargo
            .iter()
            .any(|c| matches!(c.kind, CargoKind::Salvage { .. })),
        "Ersatzteil verbaut"
    );
}

#[test]
fn steady_hand_drills_more() {
    assert!(steady_factor(0.0) > 1.2);
    assert!(steady_factor(5.0) < 0.75);
    let mined = |shaky: bool| {
        let mut s = sim();
        // Viridia: Landezone neben dem Vorkommen, dort bohren.
        let pi = 0;
        let zone = s.world.planets[pi].zones[0];
        let p = s.world.pads[zone].clone();
        s.ship.docked = None;
        s.ship.angle = p.ship_angle();
        s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 0.02);
        s.ship.prev_pos = s.ship.pos;
        s.ship.vel = Vec2::ZERO;
        s.ship.docked = Some(zone);
        let drill = s
            .ship
            .tools
            .iter()
            .position(|t| t.kind == ToolKind::Drill)
            .unwrap();
        let dslot = s.ship.tools[drill].slot;
        // Schräg nach unten auf das Vorkommen neben der Zone (wie beim Landezonen-Test).
        let planet = s.world.planets[pi].clone();
        let dep = planet.deposits[0].angle;
        let zone_angle = f32::atan2(p.normal.y, p.normal.x);
        let side = super::world::angle_diff(dep, zone_angle).signum();
        let a = zone_angle + side * 3.5 / planet.radius;
        let at = planet.pos + Vec2::new(a.cos(), a.sin()) * planet.radius;
        let mount = s.ship.tool_world_pos(drill);
        let base = f32::atan2(at.y - mount.y, at.x - mount.x);
        for k in 0..240 {
            let mut aims = vec![0.0; MAX_SLOTS];
            let wobble = if shaky {
                0.06 * if k % 2 == 0 { 1.0 } else { -1.0 }
            } else {
                0.0
            };
            aims[dslot as usize] = base + wobble;
            s.step(&TickInput {
                slots: 1 << dslot,
                aims,
                commands: vec![],
            });
        }
        s.ship.ore_amount(planet.ore)
    };
    let calm = mined(false);
    let shaky = mined(true);
    assert!(calm > 0.0);
    assert!(calm > shaky * 1.3, "ruhig {calm} vs. zittrig {shaky}");
}

#[test]
fn gentle_load_handling_earns_a_bonus() {
    let run = |strain: Option<f32>| {
        let mut s = sim();
        let orion = s.data.station_index("werft").unwrap();
        let id = s.next_id();
        s.active.push(Mission {
            id,
            kind: MissionKind::Tow {
                site: Vec2::ZERO,
                to: orion,
                name: "Test".into(),
                body: None,
            },
            reward: 400,
            origin: Some(Owner::Station(orion)),
            giver: None,
            start: Some(Box::new(s.stats.clone())),
            top_speed: 0.0,
            max_strain: strain,
            par: -1.0,
        });
        s.stats.damage += 5.0;
        // Abschluss über das Andocken an der Zielstation geht bei Abschleppen nicht –
        // deshalb die Last direkt neben die Station setzen.
        let bid = s.next_id();
        let pos = s.world.stations[orion].pos + Vec2::new(0.0, 45.0);
        s.bodies.push(super::Body {
            id: bid,
            kind: super::BodyKind::Derelict {
                mission: id,
                name: "Test".into(),
            },
            pos,
            vel: Vec2::ZERO,
            angle: 0.0,
            ang_vel: 0.0,
            radius: 2.4,
            mass: 16.0,
            prev_pos: pos,
            prev_angle: 0.0,
            alive: true,
            seed: 1,
            age: 0.0,
        });
        if let MissionKind::Tow { body, .. } = &mut s.active[0].kind {
            *body = Some(bid);
        }
        s.step(&TickInput::default());
        s.report.clone().unwrap()
    };
    let soft = run(Some(0.3));
    assert!(soft.bonus_gentle > 0);
    assert_eq!(soft.earned(), 400 + soft.bonus_gentle);
    let hard = run(Some(0.8));
    assert_eq!(hard.bonus_gentle, 0);
    let never = run(None);
    assert_eq!(never.bonus_gentle, 0);
}
