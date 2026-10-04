//! Tests für Phase 11: Crew-Lager, Module an Bauplätzen, Upgrades pro Teil mit Nachteil,
//! Material- und Artefaktkosten, Export des Bauplans.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData, Ore, ShipDef};
use super::economy::Purchase;
use super::missions::{Mission, MissionKind};
use super::ship::{CargoKind, Loadout};
use super::world::Owner;
use super::{Command, MAX_SLOTS, SimEvent, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let mut save = CrewSave::new_game(&data);
    save.credits = 10_000;
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn at_orion(s: &mut SimState) {
    let orion = s.data.station_index("werft").unwrap();
    s.ship.docked = None;
    s.dock_at_station(orion);
}

fn stock(s: &mut SimState) {
    s.crew.storage = [40.0; 5];
    s.crew.storage_parts = 6;
}

fn buy(s: &mut SimState, p: Purchase) {
    s.step(&TickInput {
        commands: vec![Command::Buy {
            purchase: p,
            voter: 0,
        }],
        ..Default::default()
    });
}

fn module(mount: &str, module: &str) -> Purchase {
    Purchase::Module {
        mount: mount.into(),
        module: module.into(),
    }
}

#[test]
fn cargo_goes_into_the_crew_storage_except_mission_ore() {
    let mut s = sim();
    s.ship.store(CargoKind::Ore(Ore::Ferrit), 5.0);
    s.ship.store(CargoKind::Ore(Ore::Kobalt), 3.0);
    s.ship.store(
        CargoKind::Salvage {
            name: "Ventil".into(),
            value: 30,
        },
        0.5,
    );
    // Ein Abbau-Auftrag braucht 2 t Kobalt – die bleiben an Bord.
    let id = s.next_id();
    s.active.push(Mission {
        id,
        kind: MissionKind::Mining {
            ore: Ore::Kobalt,
            amount: 2.0,
            to: 0,
        },
        reward: 100,
        origin: Some(Owner::Station(0)),
        giver: None,
        start: None,
        top_speed: 0.0,
        max_strain: None,
        par: 100.0,
    });
    s.step(&TickInput {
        commands: vec![Command::StoreCargo],
        ..Default::default()
    });
    assert!((s.stored(Ore::Ferrit) - 5.0).abs() < 1e-3);
    assert!((s.stored(Ore::Kobalt) - 1.0).abs() < 1e-3);
    assert_eq!(s.crew.storage_parts, 1);
    assert!((s.ship.ore_amount(Ore::Kobalt) - 2.0).abs() < 1e-3);
    // Bleibt im Spielstand.
    let save = s.to_save();
    let data = s.data.clone();
    let def = data.ship(&save.current_ship).clone();
    let s2 = SimState::new(data, &save, Loadout::full(&def), 1);
    assert!((s2.stored(Ore::Ferrit) - 5.0).abs() < 1e-3);
    assert_eq!(s2.crew.storage_parts, 1);
}

#[test]
fn cargo_module_shifts_the_centre_of_mass_and_survives_saving() {
    let mut s = sim();
    at_orion(&mut s);
    // Ohne Material geht nichts.
    let p = module("seite_l", "frachtmodul");
    let err = s.purchase_info(&p).unwrap_err();
    assert!(err.contains("Crew-Lager"), "{err}");
    stock(&mut s);
    let (mass, com, cap) = (s.ship.mass, s.ship.com, s.ship.cargo_capacity());
    let credits = s.crew.credits;
    buy(&mut s, p);
    assert!(s.ship.mass > mass + 0.5);
    assert!(
        s.ship.com.x < com.x - 0.05,
        "Schwerpunkt wandert nach links"
    );
    assert!((s.ship.cargo_capacity() - cap - 5.0).abs() < 1e-3);
    let md = s.data.module("frachtmodul").unwrap().clone();
    assert_eq!(s.crew.credits, credits - md.credits);
    assert!((s.stored(Ore::Ferrit) - (40.0 - 5.0)).abs() < 1e-3);
    assert!(
        !s.events.contains(&SimEvent::ShipChanged),
        "kein neuer Slot, keine Lobby"
    );
    // Spielstand: der Umbau bleibt.
    let save = s.to_save();
    let data = s.data.clone();
    let def = data.built_ship(&save.current_ship, &save.builds[0].1);
    let s2 = SimState::new(data, &save, Loadout::full(&def), 1);
    assert!((s2.ship.cargo_capacity() - s.ship.cargo_capacity()).abs() < 1e-3);
}

#[test]
fn engine_module_adds_a_fixed_thruster_with_its_own_slot() {
    let mut s = sim();
    at_orion(&mut s);
    stock(&mut s);
    let before = s.current_def().max_thrusters();
    buy(&mut s, module("heck_l", "triebwerk"));
    assert!(
        s.events.contains(&SimEvent::ShipChanged),
        "neuer Slot → Lobby"
    );
    let def = s.current_def();
    assert_eq!(def.max_thrusters(), before + 1);
    // Wie nach der Lobby: alle Triebwerke belegt.
    s.rebuild_ship(Loadout::full(&def));
    let n = s.ship.thrusters.len();
    assert_eq!(n as u8, before + 1);
    let extra = s.ship.thrusters.last().unwrap();
    assert!(
        (extra.pos - Vec2::new(-2.15, -1.25)).length() < 1e-3,
        "feste Position"
    );
    // Die Grundtriebwerke stehen weiter nach ihrem Layout (das äußerste rechte bei 1,45).
    let max_x = s.ship.thrusters[..n - 1]
        .iter()
        .map(|t| t.pos.x)
        .fold(f32::MIN, f32::max);
    assert!((max_x - 1.45).abs() < 1e-3);
    // Allein gezündet dreht das seitliche Triebwerk das Schiff.
    let slot = extra.slot;
    s.ship.docked = None;
    s.ship.pos = Vec2::new(-400.0, -300.0);
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    for _ in 0..30 {
        s.step(&TickInput {
            slots: 1 << slot,
            aims: vec![0.0; MAX_SLOTS],
            commands: vec![],
            brake: false,
        });
    }
    assert!(s.ship.ang_vel.abs() > 0.05);
}

#[test]
fn modules_must_fit_the_mount_and_removal_refunds_half() {
    let mut s = sim();
    at_orion(&mut s);
    stock(&mut s);
    // Fracht passt nicht ans Heck, Panzerung schon.
    assert!(s.purchase_info(&module("heck_l", "frachtmodul")).is_err());
    let hull = s.ship.max_hull;
    buy(&mut s, module("heck_r", "panzerplatte"));
    assert!((s.ship.max_hull - hull - 30.0).abs() < 1e-3);
    let ferrit = s.stored(Ore::Ferrit);
    buy(
        &mut s,
        Purchase::RemoveModule {
            mount: "heck_r".into(),
        },
    );
    assert!((s.ship.max_hull - hull).abs() < 1e-3);
    assert!(
        (s.stored(Ore::Ferrit) - ferrit - 3.0).abs() < 1e-3,
        "halbes Material zurück"
    );
    // Nur in Werften.
    s.ship.docked = None;
    s.dock_at_station(0);
    assert!(s.purchase_info(&module("heck_r", "panzerplatte")).is_err());
}

#[test]
fn upgrades_cost_material_and_add_mass_where_they_sit() {
    let mut s = sim();
    s.ship.docked = None;
    s.dock_at_station(0);
    assert!(
        s.purchase_info(&Purchase::Upgrade("armor1".into()))
            .unwrap_err()
            .contains("Crew-Lager")
    );
    stock(&mut s);
    let mass = s.ship.mass;
    let inertia = s.ship.inertia;
    buy(&mut s, Purchase::Upgrade("armor1".into()));
    assert!((s.ship.mass - mass - 1.5).abs() < 1e-3, "Panzerung wiegt");
    assert!(s.ship.inertia > inertia);
    assert!((s.stored(Ore::Ferrit) - 34.0).abs() < 1e-3);
    // Ein größerer Tank zieht den Schwerpunkt nach hinten.
    let com = s.ship.com;
    buy(&mut s, Purchase::Upgrade("tank1".into()));
    assert!(s.ship.com.y < com.y - 0.02, "{} → {}", com.y, s.ship.com.y);
    // Kanone und Kran: Feuerrate und Tragkraft.
    buy(&mut s, Purchase::Upgrade("rapid1".into()));
    buy(&mut s, Purchase::Upgrade("crane1".into()));
    buy(&mut s, Purchase::Upgrade("crane2".into()));
    assert!(s.ship.cannon_rate < 1.0);
    assert!(s.ship.crane_load > 1.0);
}

#[test]
fn strongest_tier_needs_its_artifact_as_a_key() {
    let mut s = sim();
    s.ship.docked = None;
    s.dock_at_station(0);
    stock(&mut s);
    s.crew.upgrades = vec!["thrust1".into(), "thrust2".into()];
    let err = s
        .purchase_info(&Purchase::Upgrade("thrust3".into()))
        .unwrap_err();
    assert!(err.contains("Antriebskristall"), "{err}");
    // Ein anderes Artefakt passt nicht.
    s.crew.artifacts.push("leerstein".into());
    assert!(
        s.purchase_info(&Purchase::Upgrade("thrust3".into()))
            .is_err()
    );
    s.crew.artifacts.push("antriebskristall".into());
    buy(&mut s, Purchase::Upgrade("thrust3".into()));
    assert!(s.crew.upgrades.contains(&"thrust3".to_string()));
    assert_eq!(s.crew.artifacts.len(), 2, "Schlüssel, nicht verbraucht");
}

#[test]
fn exported_layout_is_a_valid_ship_definition() {
    let mut s = sim();
    at_orion(&mut s);
    stock(&mut s);
    buy(&mut s, module("seite_r", "kran"));
    let def = s.current_def();
    let text = ron::ser::to_string_pretty(&def, ron::ser::PrettyConfig::default()).unwrap();
    let back: ShipDef = ron::from_str(&text).expect("RON lässt sich wieder lesen");
    assert_eq!(back.parts.len(), def.parts.len());
    assert!(back.parts.iter().any(|p| p.fixed));
    assert_eq!(back.tool_parts().count(), def.tool_parts().count());
}
