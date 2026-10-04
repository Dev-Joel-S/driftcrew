//! Tests für Phase 19: Ausrüstung kaufen und finden, Schildladung, Verlust bei Zerstörung.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData, Gear, ToolKind};
use super::economy::Purchase;
use super::ship::{CargoKind, Loadout};
use super::{BodyKind, Command, SimEvent, SimState, TickInput};

/// Neue Crew wie im Titelmenü: ohne Kran, Kanone, Schild.
fn fresh() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let mut save = CrewSave::new_game(&data);
    save.gear = Some(Vec::new());
    save.ammo = Some(0);
    save.shield = Some(0.0);
    save.credits = 3_000;
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn cmd(s: &mut SimState, c: Command) {
    s.step(&TickInput {
        commands: vec![c],
        ..Default::default()
    });
}

#[test]
fn new_crew_starts_without_crane_cannon_and_shield() {
    let s = fresh();
    assert!(!s.ship.has_tool(ToolKind::Crane));
    assert!(!s.ship.has_tool(ToolKind::Cannon));
    assert!(s.ship.has_tool(ToolKind::Drill), "Bohrer ist immer da");
    assert!(s.ship.has_tool(ToolKind::Scanner), "Scanner ist immer da");
    assert_eq!(s.ship.max_shield, 0.0);
    assert_eq!(s.ship.shield, 0.0);
    // Alte Spielstände (ohne Feld) haben alles an Bord.
    let data = s.data.clone();
    let save = CrewSave::new_game(&data);
    assert!(save.gear.is_none());
    let def = data.ship(&save.current_ship).clone();
    let old = SimState::new(data, &save, Loadout::full(&def), 1);
    assert!(old.ship.has_tool(ToolKind::Crane) && old.ship.max_shield > 0.0);
}

#[test]
fn gear_is_bought_at_a_shipyard_and_then_needs_a_key() {
    let mut s = fresh();
    // Eine Station mit Werkstatt suchen und dort andocken.
    let si = (0..s.world.stations.len())
        .find(|&i| {
            s.world.stations[i].has(super::data::Service::Upgrades)
                || s.world.stations[i].has(super::data::Service::Ships)
        })
        .expect("eine Werft");
    s.dock_at_station(si);
    let price = s.gear_price(Gear::Crane).unwrap();
    let credits = s.crew.credits;
    cmd(
        &mut s,
        Command::Buy {
            purchase: Purchase::Gear(Gear::Crane),
            voter: 0,
        },
    );
    assert!(s.has_gear(Gear::Crane));
    assert_eq!(s.crew.credits, credits - price);
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, SimEvent::GearUnlocked { gear: Gear::Crane }))
    );
    // Ohne Taste noch kein Slot – erst die Belegung baut ihn ein.
    assert!(!s.ship.has_tool(ToolKind::Crane));
    let def = s.current_def();
    let crane = def
        .tool_parts()
        .position(|(_, _, k)| k == ToolKind::Crane)
        .unwrap();
    let mut tools = s.loadout.tools.clone();
    tools.push(crane);
    let thrusters = s.loadout.thrusters;
    cmd(
        &mut s,
        Command::SetLoadout {
            thrusters,
            tools,
            crew_size: 1,
        },
    );
    assert!(s.ship.has_tool(ToolKind::Crane));
    // Zweimal kaufen geht nicht.
    assert!(
        s.purchase_info(&Purchase::Gear(Gear::Crane)).is_err(),
        "schon an Bord"
    );
    // Gespeichert.
    let save = s.to_save();
    assert_eq!(save.gear, Some(vec![Gear::Crane]));
}

#[test]
fn shield_does_not_recharge_by_itself_and_is_bought_at_stations() {
    let mut s = fresh();
    s.unlock_gear(Gear::Shield, "eingebaut");
    assert!(s.ship.max_shield > 0.0);
    assert_eq!(s.ship.shield, s.ship.max_shield, "frisch geladen");
    // Treffer, dann lange warten: kein Nachladen.
    s.ship.shield = 5.0;
    s.ship.docked = None;
    s.ship.pos = Vec2::new(-900.0, -900.0);
    s.ship.since_hit = 100.0;
    for _ in 0..600 {
        s.step(&TickInput::default());
    }
    assert!((s.ship.shield - 5.0).abs() < 1e-4, "{}", s.ship.shield);
    // An der Station kostet das Aufladen.
    let home = s.crew.home_station;
    s.dock_at_station(home);
    let (_, price) = s
        .purchase_info(&Purchase::Service("shield".into()))
        .expect("Schild aufladen");
    assert!(price > 0);
    cmd(
        &mut s,
        Command::Buy {
            purchase: Purchase::Service("shield".into()),
            voter: 0,
        },
    );
    assert_eq!(s.ship.shield, s.ship.max_shield);
    // Munition und Schildladung überleben Speichern und Laden.
    s.ship.shield = 7.0;
    let save = s.to_save();
    let data = s.data.clone();
    let def = data.ship(&save.current_ship).clone();
    let t = SimState::new(data, &save, Loadout::full(&def), 1);
    assert!((t.ship.shield - 7.0).abs() < 1e-4);
}

#[test]
fn destruction_loses_ammo_and_shield_charge_but_keeps_gear() {
    let mut s = fresh();
    s.unlock_gear(Gear::Shield, "eingebaut");
    s.unlock_gear(Gear::Cannon, "eingebaut");
    assert!(s.ship.ammo > 0, "Kanone kommt mit Munition");
    s.ship.docked = None;
    s.ship.pos = Vec2::new(-900.0, -900.0);
    s.ship.hull = 0.0;
    s.ship.shield = 0.0;
    for _ in 0..(60 * 30) {
        s.step(&TickInput::default());
        if s.ship.docked.is_some() && !s.ship.destroyed {
            break;
        }
    }
    assert!(!s.ship.destroyed, "geborgen");
    assert_eq!(s.ship.shield, 0.0, "Schildladung weg");
    assert_eq!(s.ship.ammo, 0, "Munition weg");
    assert!(
        s.has_gear(Gear::Shield) && s.has_gear(Gear::Cannon),
        "Geräte bleiben"
    );
}

#[test]
fn gear_is_found_in_a_wreck_and_unlocked_when_collected() {
    let mut s = fresh();
    // Das Wrack mit dem Kran.
    let widx = s
        .data
        .world
        .wrecks
        .iter()
        .position(|w| w.gear == Some(Gear::Crane))
        .expect("ein Wrack mit Kran");
    let bi = s
        .bodies
        .iter()
        .position(|b| matches!(b.kind, BodyKind::Wreck { idx, .. } if idx == widx))
        .expect("Wrack in der Welt");
    let at = s.bodies[bi].pos + Vec2::new(10.0, 0.0);
    let piece = s.tear_part_at(bi, at).expect("ein Teil löst sich");
    let BodyKind::Dropped { item } = &s.bodies[piece].kind else {
        panic!("Ausrüstung kommt als Fundstück heraus")
    };
    assert_eq!(item.kind, CargoKind::Gear(Gear::Crane));
    // Solange es draußen treibt, kommt kein zweites.
    let next = s.tear_part_at(bi, at).unwrap();
    assert!(!matches!(
        &s.bodies[next].kind,
        BodyKind::Dropped { item } if item.kind == CargoKind::Gear(Gear::Crane)
    ));
    // Einsammeln: eingebaut, kein Frachtraum belegt.
    let cargo = s.ship.cargo_mass();
    assert!(s.try_stow(piece));
    assert!(s.has_gear(Gear::Crane));
    assert_eq!(s.ship.cargo_mass(), cargo);
}
