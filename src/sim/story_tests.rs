//! Tests für Phase 13: Artefakte (einmalig per Seed, Nebenwirkungen, Sammlung, Wrack-Regel),
//! Logbuch (Archiv, Funkfetzen, Bordbuch), Kapitel und Monumente.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData};
use super::ship::{CargoKind, Loadout};
use super::{BodyKind, Command, SimEvent, SimState, TickInput};

fn sim_seeded(seed: u64) -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game_seeded(&data, seed);
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn sim() -> SimState {
    sim_seeded(7)
}

fn reload(s: &SimState) -> SimState {
    let save = s.to_save();
    let data = s.data.clone();
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn idle(s: &mut SimState, seconds: f32) {
    for _ in 0..(seconds * 60.0) as u32 {
        s.step(&TickInput::default());
    }
}

fn float_at(s: &mut SimState, pos: Vec2) {
    s.ship.docked = None;
    s.ship.pos = pos;
    s.ship.prev_pos = pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
}

fn artifact_body(s: &SimState, id: &str) -> Option<usize> {
    s.bodies
        .iter()
        .position(|b| b.alive && matches!(&b.kind, BodyKind::Artifact { id: a } if a == id))
}

/// Artefakt einladen (wie vom Kran eingeholt).
fn pick_up(s: &mut SimState, id: &str) {
    let bi = artifact_body(s, id).expect("Artefakt liegt in der Welt");
    let p = s.bodies[bi].pos;
    float_at(s, p + Vec2::new(0.0, 6.0));
    assert!(s.try_stow(bi), "passt in den Frachtraum");
}

fn dock_at(s: &mut SimState, station: &str) {
    let si = s.data.station_index(station).unwrap();
    s.ship.docked = None;
    let pad = s.world.stations[si].pads[0];
    s.dock(pad);
}

#[test]
fn artifacts_lie_once_per_save_at_seeded_sites() {
    let a = sim_seeded(1);
    let b = sim_seeded(1);
    let n = a.data.story.artifacts.len();
    assert_eq!(
        a.artifact_positions().len(),
        n,
        "jedes Artefakt genau einmal"
    );
    assert_eq!(
        a.artifact_positions(),
        b.artifact_positions(),
        "gleicher Seed, gleiche Orte"
    );
    let sites: Vec<Vec2> = a
        .data
        .story
        .artifact_sites
        .iter()
        .map(|p| Vec2::new(p.0, p.1))
        .collect();
    for p in a.artifact_positions() {
        assert!(sites.iter().any(|q| (*q - p).length() < 1e-3));
    }
    let differs =
        (2..8).any(|seed| sim_seeded(seed).artifact_positions() != a.artifact_positions());
    assert!(differs, "andere Spielstände, andere Orte");
    // Die Orte stehen im Spielstand und bleiben auch bei geänderten Daten gleich.
    let save = a.to_save();
    assert_eq!(save.story.artifact_sites.len(), n);
    assert_eq!(reload(&a).artifact_positions(), a.artifact_positions());
}

#[test]
fn artifacts_aboard_weigh_and_have_side_effects() {
    let mut s = sim();
    let mass = s.ship.mass;
    pick_up(&mut s, "sternkarte");
    assert!(s.ship.mass > mass + 0.5, "Masse an Bord");
    assert!(s.artifact_jam() > 0.3, "stört die Instrumente");
    pick_up(&mut s, "antriebskristall");
    assert!(s.artifact_heat() > 1.2, "mehr Treibstoffverbrauch");
    pick_up(&mut s, "resonanzkern");
    assert!(s.artifact_lure());
    // Die Stimmgabel bringt das Schiff ins Trudeln.
    pick_up(&mut s, "stimmgabel");
    let p = s.ship.pos;
    float_at(&mut s, p);
    s.ship.angle = 0.0;
    idle(&mut s, 3.0);
    assert!(s.ship.ang_vel.abs() > 0.01 || s.ship.angle.abs() > 0.01);
}

#[test]
fn docking_brings_artifacts_into_the_collection_for_good() {
    let mut s = sim();
    pick_up(&mut s, "leerstein");
    dock_at(&mut s, "nova");
    assert!(s.crew.artifacts.contains(&"leerstein".to_string()));
    assert!(s.artifacts_aboard().is_empty());
    assert!(s.events.contains(&SimEvent::ArtifactCollected {
        id: "leerstein".into()
    }));
    assert!(s.story.logs.contains(&"fund_leerstein".to_string()));
    // Kein Respawn – auch nicht nach dem Laden.
    let s2 = reload(&s);
    assert!(artifact_body(&s2, "leerstein").is_none());
    assert_eq!(
        s2.artifact_positions().len(),
        s.data.story.artifacts.len() - 1
    );
}

#[test]
fn artifacts_stay_in_the_wreck_or_return_home() {
    let mut s = sim();
    let home = s.bodies[artifact_body(&s, "stimmgabel").unwrap()].pos;
    pick_up(&mut s, "stimmgabel");
    let at = Vec2::new(300.0, 300.0);
    float_at(&mut s, at);
    s.ship.hull = 0.0;
    s.ship.shield = 0.0;
    s.step(&TickInput::default());
    assert!(s.ship.destroyed);
    let p = s.bodies[artifact_body(&s, "stimmgabel").expect("liegt im Wrack")].pos;
    assert!((p - at).length() < 10.0, "am Wrack: {p:?}");
    // Mit abgeschalteter Regel kehrt es an den Fundort zurück.
    let mut s = sim();
    s.step(&TickInput {
        commands: vec![Command::SetStayInWreck(false)],
        ..Default::default()
    });
    pick_up(&mut s, "stimmgabel");
    float_at(&mut s, at);
    s.ship.hull = 0.0;
    s.ship.shield = 0.0;
    s.step(&TickInput::default());
    let p = s.bodies[artifact_body(&s, "stimmgabel").unwrap()].pos;
    assert!((p - home).length() < 1.0);
    assert!(!s.to_save().story.stay_in_wreck);
}

#[test]
fn an_artifact_aboard_is_saved_where_the_ship_is() {
    let mut s = sim();
    pick_up(&mut s, "sternkarte");
    let at = Vec2::new(-200.0, 400.0);
    float_at(&mut s, at);
    let s2 = reload(&s);
    let p = s2.bodies[artifact_body(&s2, "sternkarte").unwrap()].pos;
    assert!((p - at).length() < 1e-3);
}

#[test]
fn logbook_entries_come_from_archives_echoes_and_wrecks() {
    let mut s = sim();
    // Archiv: beim (ersten) Andocken.
    dock_at(&mut s, "kepler");
    assert!(s.story.logs.contains(&"archiv_kepler".to_string()));
    // Funkfetzen: in der Nähe von Ember scannen.
    float_at(&mut s, Vec2::new(-700.0, -650.0));
    assert!(s.start_scan(s.ship.pos));
    assert!(s.story.logs.contains(&"echo_ember".to_string()));
    // Bordbuch: ein Wrack im Scannerring.
    let wreck = Vec2::new(-1240.0, 1560.0);
    float_at(&mut s, wreck + Vec2::new(0.0, 60.0));
    s.scan = None;
    assert!(s.start_scan(s.ship.pos));
    idle(&mut s, 4.0);
    assert!(
        s.story.logs.contains(&"wrack_kassiopeia".to_string()),
        "{:?}",
        s.story.logs
    );
    // Einmalig: ein zweiter Scan findet nichts Neues.
    let n = s.story.logs.len();
    s.scan = None;
    s.start_scan(s.ship.pos);
    idle(&mut s, 4.0);
    assert_eq!(s.story.logs.len(), n);
}

#[test]
fn chapters_start_with_their_requirements_and_pay_out() {
    let mut s = sim();
    idle(&mut s, 1.0);
    assert!(!s.story.started, "erst nach dem ersten Auftrag");
    s.crew.missions_done = 1;
    idle(&mut s, 1.0);
    assert!(s.story.started);
    // Kapitel 1: bei der Anomalie scannen.
    let credits = s.crew.credits;
    float_at(&mut s, Vec2::new(980.0, -900.0));
    s.ship.invulnerable = 10.0;
    assert!(s.start_scan(s.ship.pos));
    assert_eq!(s.story.chapter, 1);
    assert_eq!(s.crew.credits, credits + s.data.story.chapters[0].reward);
    assert!(s.story.logs.contains(&"kapitel_0".to_string()));
    // Kapitel 2 wartet auf Ruf, dann reicht ein abgeliefertes Artefakt.
    idle(&mut s, 1.0);
    assert!(!s.story.started);
    assert!(!s.chapter_missing().is_empty());
    s.crew.reputation[0] = 3;
    idle(&mut s, 1.0);
    assert!(s.story.started);
    pick_up(&mut s, "leerstein");
    dock_at(&mut s, "nova");
    assert_eq!(s.story.chapter, 2);
    // Fortschritt bleibt im Spielstand.
    assert_eq!(reload(&s).story.chapter, 2);
}

#[test]
fn monuments_wake_with_enough_artifacts_and_open_their_doors() {
    let mut s = sim();
    let turm = s
        .world
        .monuments
        .iter()
        .position(|m| m.id == "signalturm")
        .unwrap();
    let pos = s.world.monuments[turm].pos;
    float_at(&mut s, pos + Vec2::new(50.0, 0.0));
    assert!(s.start_scan(s.ship.pos));
    assert!(!s.world.monuments[turm].awake, "zu wenig Artefakte");
    assert!(s.events.iter().any(|e| matches!(e, SimEvent::Story { .. })));
    s.crew.artifacts = vec!["leerstein".into(), "sternkarte".into()];
    s.scan = None;
    assert!(s.start_scan(s.ship.pos));
    assert!(s.world.monuments[turm].awake);
    assert!(s.story.sites_revealed, "Fundorte auf der Karte");
    assert!(s.story.logs.contains(&"monument_turm".to_string()));
    // Das Tor: Tor-Blöcke kollidieren nicht mehr, wenn es erwacht ist.
    let tor = s
        .world
        .monuments
        .iter()
        .position(|m| m.id == "tor")
        .unwrap();
    let doors = |s: &SimState| {
        s.world
            .colliders
            .iter()
            .filter(|c| c.door == Some(tor) && c.enabled)
            .count()
    };
    assert!(doors(&s) > 0);
    s.crew
        .artifacts
        .extend(["stimmgabel".to_string(), "resonanzkern".to_string()]);
    let p = s.world.monuments[tor].pos;
    float_at(&mut s, p + Vec2::new(-70.0, 0.0));
    s.scan = None;
    assert!(s.start_scan(s.ship.pos));
    assert!(s.world.monuments[tor].awake);
    assert_eq!(doors(&s), 0);
    // Bleibt nach dem Laden offen.
    let s2 = reload(&s);
    assert!(s2.world.monuments[tor].awake);
    assert_eq!(doors(&s2), 0);
}

#[test]
fn lure_artifact_draws_pirates_from_further_away() {
    let mut s = sim();
    pick_up(&mut s, "resonanzkern");
    let n = s.data.traffic.nests[0].clone();
    let nest = Vec2::new(n.center.0, n.center.1);
    // Außerhalb der normalen Reichweite, aber innerhalb der Lockreichweite.
    float_at(&mut s, nest + Vec2::new(0.0, n.radius + 900.0));
    for _ in 0..60 {
        let p = s.ship.pos;
        float_at(&mut s, p);
        s.step(&TickInput::default());
    }
    assert!(s.npcs.iter().any(|x| x.alive
        && x.hostile
        && matches!(x.role, super::npc::Role::Drone { nest: Some(0) })));
}

#[test]
fn hidden_artifact_gives_no_scanner_signal() {
    let mut s = sim();
    for id in ["leerstein", "sternkarte"] {
        let bi = artifact_body(&s, id).unwrap();
        let p = s.bodies[bi].pos;
        float_at(&mut s, p + Vec2::new(0.0, 40.0));
        s.scan = None;
        s.blips.clear();
        assert!(s.start_scan(s.ship.pos));
        idle(&mut s, 3.0);
        let signal = s
            .blips
            .iter()
            .any(|b| b.kind == super::sector::BlipKind::Signal && (b.pos - p).length() < 5.0);
        assert_eq!(signal, id != "leerstein", "{id}");
    }
    let _ = CargoKind::Artifact { id: String::new() };
}
