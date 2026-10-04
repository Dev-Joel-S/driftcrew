//! Tests für Phase 17: Schiffsname, Plaketten, Tagebuch, Notizen, Kartenmarkierungen.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData};
use super::journal::{JournalKind, MAX_MARKS, MAX_NAME};
use super::missions::{Mission, MissionKind};
use super::ship::Loadout;
use super::{Command, SimEvent, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 2)
}

fn cmd(s: &mut SimState, c: Command) {
    s.step(&TickInput {
        commands: vec![c],
        ..Default::default()
    });
}

fn reload(s: &SimState) -> SimState {
    let save = s.to_save();
    let data = s.data.clone();
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 2)
}

#[test]
fn crew_names_its_ship_and_the_name_is_saved() {
    let mut s = sim();
    assert_eq!(s.ship_name(), None);
    cmd(
        &mut s,
        Command::NameShip {
            name: "  Rostige Rakete  ".into(),
        },
    );
    assert_eq!(s.ship_name(), Some("Rostige Rakete"));
    assert!(
        s.journal
            .entries
            .iter()
            .any(|e| e.text.contains("Rostige Rakete"))
    );
    let long = "x".repeat(100);
    cmd(&mut s, Command::NameShip { name: long });
    assert_eq!(s.ship_name().unwrap().chars().count(), MAX_NAME);
    cmd(
        &mut s,
        Command::NameShip {
            name: "Zweite Chance".into(),
        },
    );
    let t = reload(&s);
    assert_eq!(t.ship_name(), Some("Zweite Chance"));
}

#[test]
fn journal_records_firsts_sectors_and_notes() {
    let mut s = sim();
    // Andocken an einer anderen Station: erstes Andocken.
    let other = (0..s.world.stations.len())
        .find(|&i| i != s.crew.home_station)
        .unwrap();
    let pad = s.world.stations[other].pads[0];
    s.dock(pad);
    assert!(
        s.journal
            .entries
            .iter()
            .any(|e| e.kind == JournalKind::First
                && e.text.contains("Erstes erfolgreiches Andocken")),
        "{:?}",
        s.journal.entries
    );
    // Gleiche Station noch einmal: kein neuer Eintrag.
    let n = s.journal.entries.len();
    s.dock(pad);
    assert_eq!(s.journal.entries.len(), n);
    // Neuer Sektor beim Herumfliegen.
    s.ship.docked = None;
    let r = s.data.world.regions.last().unwrap().clone();
    s.ship.pos = Vec2::new(r.center.0, r.center.1);
    for _ in 0..40 {
        s.ship.pos = Vec2::new(r.center.0, r.center.1);
        s.ship.vel = Vec2::ZERO;
        s.step(&TickInput::default());
    }
    assert!(
        s.journal
            .entries
            .iter()
            .any(|e| e.text.contains("Sektor erkundet")),
        "{:?}",
        s.journal.entries
    );
    // Eigene Notiz.
    cmd(
        &mut s,
        Command::AddNote {
            text: "Hier lohnt sich das Erz.".into(),
        },
    );
    let last = s.journal.entries.last().unwrap();
    assert_eq!(last.kind, JournalKind::Note);
    assert!(last.time > 0.0, "mit Flugzeit");
    let t = reload(&s);
    assert_eq!(t.journal.entries.len(), s.journal.entries.len());
    assert!(t.journal.playtime > 0.0);
}

#[test]
fn map_marks_are_capped_labelled_and_removable() {
    let mut s = sim();
    for i in 0..(MAX_MARKS + 3) {
        cmd(
            &mut s,
            Command::AddMark {
                pos: Vec2::new(i as f32 * 10.0, 500.0),
                text: format!("M{i}"),
            },
        );
    }
    assert_eq!(s.journal.marks.len(), MAX_MARKS);
    assert_eq!(s.journal.marks[0].text, "M3", "die ältesten fallen weg");
    // Ohne Text: beschriftet nach dem Ort.
    let st = s.world.stations[0].pos;
    cmd(
        &mut s,
        Command::AddMark {
            pos: st,
            text: String::new(),
        },
    );
    assert_eq!(
        s.journal.marks.last().unwrap().text,
        s.world.stations[0].name
    );
    cmd(&mut s, Command::RemoveMark { idx: 0 });
    assert_eq!(s.journal.marks[0].text, "M5");
    assert_eq!(reload(&s).journal.marks, s.journal.marks);
}

#[test]
fn special_salvage_and_rescue_give_plaques_and_records() {
    let mut s = sim();
    let to = s.crew.home_station;
    let id = s.next_id();
    s.active.push(Mission {
        id,
        kind: MissionKind::Rescue {
            site: Vec2::ZERO,
            to,
            ship: "Kutter Ilse".into(),
            captain: "Ilse".into(),
            crew: 2,
            aboard: 2,
            npc: None,
        },
        reward: 400,
        origin: None,
        giver: None,
        start: None,
        top_speed: 0.0,
        max_strain: None,
        par: 600.0,
    });
    s.ship.hull = s.ship.max_hull * 0.2;
    s.complete_mission(s.active.len() - 1);
    let plaques = s.ship_plaques();
    assert_eq!(plaques.len(), 1);
    assert!(plaques[0].title.contains("Kutter Ilse"));
    assert_eq!(s.journal.closest_rescue.map(|h| h.round()), Some(20.0));
    assert!(s.journal.biggest_salvage.is_some());
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, SimEvent::Toast { text, .. } if text.contains("Plakette")))
    );
    // Monument erwacht: Plakette über das Ereignis.
    if !s.world.monuments.is_empty() {
        s.events.push(SimEvent::MonumentWoke { idx: 0 });
        s.update_journal();
        assert_eq!(s.ship_plaques().len(), 2);
    }
    // Andere Schiffe tragen ihre eigenen Plaketten.
    let t = reload(&s);
    assert_eq!(t.ship_plaques().len(), s.ship_plaques().len());
}
