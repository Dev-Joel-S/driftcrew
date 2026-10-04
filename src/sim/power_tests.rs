//! Tests für Phase 15: Übersteuerung, Zusatzenergie, Zurufe.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData};
use super::power::{COOL_OFF, OVERDRIVE_MUL};
use super::ship::Loadout;
use super::{CALLOUT_SECONDS, Call, Command, SimEvent, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    let mut s = SimState::new(data, &save, Loadout::full(&def), 2);
    // Frei im Raum an einer ruhigen Stelle.
    s.ship.docked = None;
    s.ship.pos = Vec2::new(-900.0, -900.0);
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.angle = 0.0;
    s
}

fn press(s: &mut SimState, slots: u32, ticks: u32) {
    for _ in 0..ticks {
        s.step(&TickInput {
            slots,
            ..Default::default()
        });
    }
}

/// Alle Triebwerke: einmal kurz tippen, kurz los, dann halten (= übersteuern).
fn double_tap_hold(s: &mut SimState, all: u32, hold: u32) {
    press(s, all, 4);
    press(s, 0, 6);
    press(s, all, hold);
}

#[test]
fn long_press_never_overdrives_but_double_tap_does() {
    let mut s = sim();
    let all = (1u32 << s.ship.thrusters.len()) - 1;
    press(&mut s, all, 120);
    assert!(
        s.ship.thrusters.iter().all(|t| !t.overdrive),
        "langes Drücken übersteuert nie"
    );
    let v_normal = s.ship.vel.length();

    let mut t = sim();
    double_tap_hold(&mut t, all, 116);
    assert!(t.ship.thrusters.iter().all(|t| t.overdrive));
    let v_od = t.ship.vel.length();
    assert!(
        v_od > v_normal * 1.35,
        "übersteuert schneller: {v_od} gegen {v_normal}"
    );
    assert!(
        t.ship.thrusters.iter().all(|th| th.heat > 0.2),
        "und heißer"
    );
    assert!(t.ship.energy < t.ship.max_energy, "zieht Zusatzenergie");
    // Loslassen beendet die Übersteuerung, die Hitze sinkt.
    let h = t.ship.thrusters[0].heat;
    press(&mut t, 0, 60);
    assert!(!t.ship.thrusters[0].overdrive);
    assert!(t.ship.thrusters[0].heat < h);
    let _ = OVERDRIVE_MUL;
}

#[test]
fn each_thruster_heats_up_warns_and_shuts_down() {
    let mut s = sim();
    // Nur ein Triebwerk übersteuern, das andere normal.
    double_tap_hold(&mut s, 0b01, 1);
    let mut warned = 0;
    let mut alarm = false;
    for _ in 0..(8 * 60) {
        s.step(&TickInput {
            slots: 0b11,
            ..Default::default()
        });
        for e in &s.events {
            match e {
                SimEvent::Toast { text, .. }
                    if text.contains("heiß") || text.contains("kurz vor") =>
                {
                    warned += 1
                }
                SimEvent::Alarm => alarm = true,
                _ => {}
            }
        }
        if s.ship.thrusters[0].cool_off > 0.0 {
            break;
        }
    }
    assert!(warned >= 2, "zwei Warnstufen vor der Abschaltung");
    assert!(alarm);
    let t0 = &s.ship.thrusters[0];
    assert!(t0.cool_off > COOL_OFF - 0.5, "Notabschaltung");
    assert!(!t0.firing && !t0.overdrive);
    assert!(s.ship.thrusters[1].heat < 0.01, "das andere bleibt kalt");
    // Während der Pause zündet es nicht, danach wieder.
    press(&mut s, 0b01, 60);
    assert!(!s.ship.thrusters[0].firing);
    press(&mut s, 0, (COOL_OFF * 60.0) as u32);
    press(&mut s, 0b01, 2);
    assert!(s.ship.thrusters[0].firing);
}

#[test]
fn empty_energy_only_removes_the_bonus() {
    let mut s = sim();
    let all = (1u32 << s.ship.thrusters.len()) - 1;
    s.ship.energy = 0.0;
    double_tap_hold(&mut s, all, 60);
    assert!(
        s.ship.thrusters.iter().all(|t| t.firing),
        "Triebwerke schieben normal"
    );
    assert!(
        s.ship.thrusters.iter().all(|t| t.heat < 1e-4),
        "ohne Energie kein Übersteuern, also keine Hitze"
    );
    let v = s.ship.vel.length();
    let mut n = sim();
    press(&mut n, all, 70);
    assert!(
        (v - n.ship.vel.length()).abs() < 0.6,
        "{v} vs {}",
        n.ship.vel.length()
    );
    // Schild lädt auch ohne Energie, nur langsamer.
    let mut a = sim();
    let mut b = sim();
    for x in [&mut a, &mut b] {
        x.ship.shield = 0.0;
        x.ship.since_hit = 100.0;
    }
    b.ship.energy = 0.0;
    b.ship.max_energy = 0.0;
    press(&mut a, 0, 120);
    press(&mut b, 0, 120);
    assert!(b.ship.shield > 0.0, "Grundfunktion");
    assert!(
        a.ship.shield > b.ship.shield * 1.5,
        "Schnellladen mit Energie"
    );
    // Energie lädt, wenn nichts zieht.
    let mut c = sim();
    c.ship.energy = 10.0;
    press(&mut c, 0, 120);
    assert!(c.ship.energy > 20.0);
}

#[test]
fn callouts_show_for_everyone_and_fade() {
    let mut s = sim();
    s.step(&TickInput {
        commands: vec![
            Command::Callout {
                player: 0,
                call: Call::Brake,
            },
            Command::Callout {
                player: 1,
                call: Call::ToolReady,
            },
        ],
        ..Default::default()
    });
    assert_eq!(s.callouts.len(), 2);
    assert!(s.events.iter().any(|e| matches!(
        e,
        SimEvent::Callout {
            player: 0,
            call: Call::Brake
        }
    )));
    // Ein neuer Zuruf derselben Person ersetzt den alten.
    s.step(&TickInput {
        commands: vec![Command::Callout {
            player: 0,
            call: Call::ThrustOff,
        }],
        ..Default::default()
    });
    assert_eq!(s.callouts.len(), 2);
    assert!(s.callouts.iter().any(|c| c.call == Call::ThrustOff));
    press(&mut s, 0, (CALLOUT_SECONDS * 60.0) as u32 + 2);
    assert!(s.callouts.is_empty());
    for c in [
        Call::Brake,
        Call::ThrustOff,
        Call::TurnLeft,
        Call::TurnRight,
        Call::ToolReady,
    ] {
        assert!(!c.label().is_empty());
    }
}
