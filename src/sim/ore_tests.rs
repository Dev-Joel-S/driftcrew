//! Tests für die Erz-Runde: gemischte Felder, Fundorte, Bohrer-Anzeige, Richtzeit.

use std::sync::Arc;

use crate::sim::fx::{Fx, Vec2};

use super::data::{CrewSave, GameData, Ore, ToolKind};
use super::missions::MissionKind;
use super::ship::Loadout;
use super::{BodyKind, SimState};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn field(s: &SimState, name: &str) -> usize {
    s.data
        .world
        .asteroid_fields
        .iter()
        .position(|f| f.name == name)
        .unwrap()
}

#[test]
fn fields_near_nova_carry_more_than_iron() {
    let mut s = sim();
    let fi = field(&s, "Kieselfeld");
    let (mut ferrit, mut kobalt, mut other, mut barren) = (0, 0, 0, 0);
    for k in 0..600 {
        let bi = s.spawn_asteroid(
            fi,
            Vec2::new(fx!(5000.0) + Fx::from(k) * fx!(20.0), fx!(5000.0)),
            None,
        );
        match s.bodies[bi].kind {
            BodyKind::Asteroid {
                ore: Some(Ore::Ferrit),
                ..
            } => ferrit += 1,
            BodyKind::Asteroid {
                ore: Some(Ore::Kobalt),
                ..
            } => kobalt += 1,
            BodyKind::Asteroid { ore: Some(_), .. } => other += 1,
            _ => barren += 1,
        }
    }
    assert!(
        ferrit > kobalt && kobalt > other && other > 0,
        "{ferrit} {kobalt} {other}"
    );
    assert!(barren < 600 / 6, "kaum taubes Gestein: {barren}");
    // Die Karte und die Aufträge kennen die Beimischung.
    let f = &s.data.world.asteroid_fields[fi];
    assert_eq!(f.ores()[0], Ore::Ferrit);
    assert!(f.ores().contains(&Ore::Kobalt));
    let kob = s.ore_sources(Ore::Kobalt);
    assert!(kob.iter().any(|(_, n, rich)| n == "Kieselfeld" && !rich));
    assert!(kob.iter().any(|(_, n, rich)| n == "Kobaltschwarm" && *rich));
}

#[test]
fn drill_shows_what_is_in_the_rock() {
    let mut s = sim();
    s.ship.docked = None;
    s.ship.pos = Vec2::new(-fx!(900.0), -fx!(900.0));
    s.ship.angle = fx!(0.0);
    let di = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == ToolKind::Drill)
        .expect("Bohrer");
    s.ship.tools[di].aim = fx!(0.0);
    let mount = s.ship.tool_world_pos(di);
    let fi = field(&s, "Kobaltschwarm");
    let place = |s: &mut SimState, d: Fx, ore: Option<Ore>| {
        for b in &mut s.bodies {
            if matches!(b.kind, BodyKind::Asteroid { .. }) {
                b.alive = false;
            }
        }
        let bi = s.spawn_asteroid(fi, mount + Vec2::X * (d + fx!(1.5)), Some(fx!(1.5)));
        if let BodyKind::Asteroid {
            ore: o, ore_left, ..
        } = &mut s.bodies[bi].kind
        {
            *o = ore;
            *ore_left = if ore.is_some() { fx!(3.3) } else { fx!(0.0) };
        }
    };
    place(&mut s, fx!(2.0), Some(Ore::Kobalt));
    let (_, text, ore) = s.drill_preview(di).expect("Ziel in Reichweite");
    assert_eq!(ore, Some(Ore::Kobalt));
    assert!(
        text.contains("Kobalt") && text.contains("3.3") && !text.contains("näher"),
        "{text}"
    );
    place(&mut s, fx!(2.0), None);
    let (_, text, ore) = s.drill_preview(di).unwrap();
    assert!(ore.is_none() && text.contains("taub"), "{text}");
    // Weiter weg: schon sichtbar, aber mit Hinweis.
    place(&mut s, fx!(10.0), Some(Ore::Kobalt));
    let (_, text, _) = s.drill_preview(di).unwrap();
    assert!(text.contains("näher ran"), "{text}");
    place(&mut s, fx!(30.0), Some(Ore::Kobalt));
    assert!(s.drill_preview(di).is_none());
}

#[test]
fn mining_par_time_counts_the_way_to_the_ore() {
    let s = sim();
    let nova = s.data.station_index("nova").unwrap();
    let par = |ore| {
        s.par_time(&MissionKind::Mining {
            ore,
            amount: fx!(6.0),
            to: nova,
        })
    };
    // Ionit liegt weit draußen, Ferrit gleich nebenan.
    assert!(
        par(Ore::Ionit) > par(Ore::Ferrit) + fx!(60.0),
        "{} {}",
        par(Ore::Ionit),
        par(Ore::Ferrit)
    );
    // Abbau eingerechnet: 6 t brauchen deutlich mehr als nur den Weg.
    assert!(par(Ore::Ferrit) >= fx!(6.0) * fx!(12.0) + fx!(120.0));
}

#[test]
#[ignore]
fn bench_ticks() {
    let mut s = sim();
    s.ship.docked = None;
    s.ship.pos = Vec2::new(fx!(600.0), fx!(400.0));
    let t = std::time::Instant::now();
    let n = std::env::var("BENCH_TICKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    for _ in 0..n {
        s.step(&super::TickInput::default());
    }
    eprintln!(
        "{n} Ticks: {:?} ({} Körper, {} NPCs)",
        t.elapsed(),
        s.bodies.len(),
        s.npcs.len()
    );
}
