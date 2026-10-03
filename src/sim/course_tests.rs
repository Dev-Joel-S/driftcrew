//! Tests für Phase 7: Parcours (Training, Zeitrennen, Bestenliste), Messflug und
//! Lastaufnahme.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData, SpinnerShape, StepKind, v};
use super::missions::{Mission, MissionKind};
use super::ship::{CraneState, Loadout};
use super::world::angle_diff;
use super::{BodyKind, Command, DT, MAX_SLOTS, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn idle() -> TickInput {
    TickInput {
        slots: 0,
        aims: vec![0.0; MAX_SLOTS],
        commands: vec![],
    }
}

fn course(s: &SimState, id: &str) -> usize {
    s.data
        .courses
        .courses
        .iter()
        .position(|c| c.id == id)
        .unwrap()
}

/// Schiff kurz vor einen Punkt setzen und mit fester Geschwindigkeit in Richtung `dir`
/// (Grad) hindurchfliegen lassen.
fn fly_through(s: &mut SimState, pos: Vec2, dir_deg: f32, speed: f32, ticks: usize) {
    let dir = Vec2::new(dir_deg.to_radians().cos(), dir_deg.to_radians().sin());
    s.ship.docked = None;
    s.ship.pos = pos - dir * 4.0;
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = dir * speed;
    s.ship.ang_vel = 0.0;
    // Mindestens so lange, dass das Schiff sicher durch ist.
    let need = (6.0 / speed / DT) as usize;
    for _ in 0..ticks.max(need) {
        s.step(&idle());
    }
}

/// Schiff ruhig an einen Ort stellen.
fn park(s: &mut SimState, pos: Vec2, angle: f32) {
    s.ship.docked = None;
    s.ship.pos = pos;
    s.ship.prev_pos = pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.angle = angle;
}

fn land_on(s: &mut SimState, pad: usize, lateral: f32) {
    let p = s.world.pads[pad].clone();
    s.ship.angle = p.ship_angle();
    s.ship.pos = p.center + p.normal * (s.ship.rest_height() + 0.02) + p.tangent() * lateral;
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.docked = Some(pad);
}

fn gate(s: &SimState, ci: usize, step: usize) -> (Vec2, f32) {
    match s.data.courses.courses[ci].steps[step].kind {
        StepKind::Gate { pos, dir, .. } => (v(pos), dir),
        ref k => panic!("kein Tor: {k:?}"),
    }
}

#[test]
fn start_gate_only_starts_in_arrow_direction() {
    let mut s = sim();
    let ci = course(&s, "nova_ring");
    let (pos, dir) = gate(&s, ci, 0);
    // Gegen die Pfeilrichtung: nichts passiert.
    fly_through(&mut s, pos, dir + 180.0, 10.0, 60);
    assert!(s.course.is_none());
    // Schräg (60° daneben) auch nicht – sonst startet jeder Vorbeiflug einen Lauf.
    fly_through(&mut s, pos, dir + 60.0, 10.0, 60);
    assert!(s.course.is_none());
    fly_through(&mut s, pos, dir, 10.0, 30);
    let r = s.course.as_ref().expect("Lauf läuft");
    assert_eq!((r.course, r.step), (ci, 1));
    assert!(r.time > 0.0);
}

#[test]
fn race_through_all_gates_enters_leaderboard_and_pays_medals_once() {
    let mut s = sim();
    let ci = course(&s, "nova_ring");
    let n = s.data.courses.courses[ci].steps.len();
    let credits = s.crew.credits;
    for step in 0..n {
        let (pos, dir) = gate(&s, ci, step);
        fly_through(&mut s, pos, dir, 14.0, 40);
        if step + 1 < n {
            assert_eq!(
                s.course.as_ref().map(|r| r.step),
                Some(step + 1),
                "Tor {step}"
            );
        }
    }
    assert!(s.course.is_none());
    let res = s.course_result.clone().expect("Ergebnis");
    assert_eq!(res.rank, Some(0));
    assert_eq!(res.medal, 3, "teleportiert ist das klar Gold");
    let (b, sv, g) = s.data.courses.medal_prizes;
    assert_eq!(res.prize, b + sv + g);
    assert_eq!(s.crew.credits, credits + b + sv + g);
    assert_eq!(s.records[ci].entries.len(), 1);
    assert_eq!(s.records[ci].medal, 3);

    // Zweiter Lauf: landet in der Liste, aber keine Medaillenprämie mehr.
    let credits = s.crew.credits;
    for step in 0..n {
        let (pos, dir) = gate(&s, ci, step);
        fly_through(&mut s, pos, dir, 14.0, 40);
    }
    assert_eq!(s.records[ci].entries.len(), 2);
    assert_eq!(s.records[ci].finished, 2);
    assert_eq!(s.crew.credits, credits);

    // Bestenliste überlebt Speichern und Laden.
    let save = s.to_save();
    let data = s.data.clone();
    let def = data.ship(&save.current_ship).clone();
    let s2 = SimState::new(data, &save, Loadout::full(&def), 2);
    assert_eq!(s2.records[ci].entries, s.records[ci].entries);
    assert_eq!(s2.records[ci].medal, 3);
}

#[test]
fn collisions_cost_penalty_seconds() {
    let mut s = sim();
    let ci = course(&s, "nova_ring");
    let (pos, dir) = gate(&s, ci, 0);
    fly_through(&mut s, pos, dir, 10.0, 20);
    assert!(s.course.is_some());
    s.stats.slots[0].collisions += 2;
    s.step(&idle());
    let r = s.course.as_ref().unwrap();
    assert_eq!(r.collisions, 2);
    assert!((r.penalty - 2.0 * s.data.courses.collision_penalty).abs() < 1e-4);
}

#[test]
fn training_teaches_turn_thrust_brake_dock() {
    let mut s = sim();
    let ci = course(&s, "grundkurs");
    let credits = s.crew.credits;
    // Im Stationsmenü gewählt: die Zeit läuft erst am Starttor.
    s.step(&TickInput {
        commands: vec![Command::StartCourse { course: ci }],
        ..idle()
    });
    let r = s.course.as_ref().unwrap();
    assert_eq!(r.step, 0);
    let (pos, dir) = gate(&s, ci, 0);
    fly_through(&mut s, pos, dir, 8.0, 30);
    assert_eq!(s.course.as_ref().unwrap().step, 1);

    // Drehen: Nase auf die Boje, ruhig halten.
    let StepKind::Face { pos: buoy, .. } = s.data.courses.courses[ci].steps[1].kind else {
        panic!()
    };
    let here = v(buoy) + Vec2::new(-40.0, 20.0);
    let to = v(buoy) - here;
    park(&mut s, here, f32::atan2(-to.x, to.y) + 0.6);
    for _ in 0..90 {
        s.step(&idle());
    }
    assert_eq!(s.course.as_ref().unwrap().step, 1, "schief zählt nicht");
    park(&mut s, here, f32::atan2(-to.x, to.y));
    for _ in 0..70 {
        s.step(&idle());
    }
    assert_eq!(s.course.as_ref().unwrap().step, 2);
    assert!(angle_diff(s.ship.angle, f32::atan2(-to.x, to.y)).abs() < 0.2);

    let (pos, dir) = gate(&s, ci, 2);
    fly_through(&mut s, pos, dir, 8.0, 30);
    assert_eq!(s.course.as_ref().unwrap().step, 3);

    // Bremsen: im Feld zu schnell zählt nicht, stillstehen schon.
    let StepKind::Hold {
        pos: field,
        seconds,
        max_speed,
        ..
    } = s.data.courses.courses[ci].steps[3].kind
    else {
        panic!()
    };
    park(&mut s, v(field), 0.0);
    s.ship.vel = Vec2::new(max_speed * 1.5, 0.0);
    for _ in 0..30 {
        s.step(&idle());
    }
    assert_eq!(s.course.as_ref().unwrap().progress, 0.0);
    park(&mut s, v(field), 0.0);
    let ticks = (seconds / DT) as usize + 10;
    for _ in 0..ticks {
        s.step(&idle());
    }
    assert_eq!(s.course.as_ref().unwrap().step, 4);

    // Andocken an Nova beendet den Grundkurs, der Zuschuss kommt einmal.
    let nova = s.data.station_index("nova").unwrap();
    let pad = s.world.stations[nova].pads[0];
    land_on(&mut s, pad, 0.0);
    s.step(&idle());
    assert!(s.course.is_none());
    let res = s.course_result.clone().unwrap();
    assert_eq!(res.prize, s.data.courses.courses[ci].prize);
    assert!(s.crew.credits >= credits + res.prize);
    assert_eq!(s.records[ci].finished, 1);
}

#[test]
fn precision_dock_measures_offset_and_touchdown_speed() {
    let mut s = sim();
    let ci = course(&s, "dockpruefung");
    let (pos, dir) = gate(&s, ci, 0);
    fly_through(&mut s, pos, dir, 6.0, 20);
    assert_eq!(s.course.as_ref().unwrap().step, 1);
    let StepKind::Dock { station, near } = s.data.courses.courses[ci].steps[1].kind.clone() else {
        panic!()
    };
    let pads = s.dock_pads(&station, near);
    assert_eq!(pads.len(), 1, "genau eine Zielplattform");
    // Falsche Plattform: Lauf endet.
    let nova = s.data.station_index("nova").unwrap();
    let other = *s.world.stations[nova]
        .pads
        .iter()
        .find(|p| **p != pads[0])
        .unwrap();
    land_on(&mut s, other, 0.0);
    s.step(&idle());
    assert!(s.course.is_none());

    fly_through(&mut s, pos, dir, 6.0, 20);
    land_on(&mut s, pads[0], 1.0);
    s.prev_vel = Vec2::new(0.0, -1.2);
    s.step(&idle());
    let res = s.course_result.clone().expect("Ziel erreicht");
    let (off, speed) = res.dock.unwrap();
    assert!((off - 1.0).abs() < 0.05, "Versatz {off}");
    assert!((speed - 1.2).abs() < 0.05, "Aufsetzen {speed}");
    let (per_m, per_s) = s.data.courses.dock_penalty;
    assert!((res.penalty - (off * per_m + speed * per_s)).abs() < 1e-3);
    assert!((res.total - res.time - res.penalty).abs() < 1e-4);
}

#[test]
fn run_aborts_when_far_from_course_or_docked_elsewhere() {
    let mut s = sim();
    let ci = course(&s, "nova_ring");
    let (pos, dir) = gate(&s, ci, 0);
    fly_through(&mut s, pos, dir, 10.0, 20);
    assert!(s.course.is_some());
    park(&mut s, Vec2::new(900.0, 900.0), 0.0);
    s.step(&idle());
    assert!(s.course.is_none());

    fly_through(&mut s, pos, dir, 10.0, 20);
    assert!(s.course.is_some());
    let nova = s.data.station_index("nova").unwrap();
    let pad = s.world.stations[nova].pads[0];
    land_on(&mut s, pad, 0.0);
    s.step(&idle());
    assert!(s.course.is_none());

    // Abbruch auf Wunsch.
    fly_through(&mut s, pos, dir, 10.0, 20);
    s.step(&TickInput {
        commands: vec![Command::AbortCourse],
        ..idle()
    });
    assert!(s.course.is_none());
}

#[test]
fn wreck_ring_has_rotating_openings() {
    let mut s = sim();
    let ring = s
        .world
        .spinners
        .iter()
        .position(|sp| matches!(sp.shape, SpinnerShape::Ring { .. }))
        .expect("Wrackring");
    let SpinnerShape::Ring {
        radius,
        segments,
        openings,
        gap,
    } = s.world.spinners[ring].shape
    else {
        panic!()
    };
    assert_eq!(
        s.world.spinners[ring].quads().len() as u32,
        segments - openings * gap
    );
    // In der Mitte ist Platz, und der Ring dreht sich.
    let center = s.world.spinners[ring].pos;
    park(&mut s, center, 0.0);
    let a0 = s.world.spinners[ring].angle;
    for _ in 0..60 {
        s.step(&idle());
    }
    assert!(
        (s.ship.pos - center).length() < 0.5,
        "nichts schiebt in der Mitte"
    );
    assert!((s.world.spinners[ring].angle - a0).abs() > 0.1);
    // Die Ringwand stößt ab.
    let wall = s.world.spinners[ring].quads()[0].center();
    assert!((wall - center).length() > radius - 1.0);
}

fn survey_mission(s: &mut SimState, sites: Vec<usize>) -> u32 {
    let id = s.next_id();
    s.active.push(Mission {
        id,
        kind: MissionKind::Survey {
            sites,
            done: 0,
            hold: 0.0,
        },
        reward: 200,
        origin: Some(super::world::Owner::Station(0)),
        giver: None,
        start: Some(Box::new(s.stats.clone())),
        top_speed: 0.0,
        max_strain: None,
        par: 300.0,
    });
    id
}

#[test]
fn survey_needs_stillness_in_each_field() {
    let mut s = sim();
    let sd = s.data.missions.survey.clone();
    let id = survey_mission(&mut s, vec![0, 1]);
    let site0 = v(s.data.courses.survey_sites[0].pos);
    let site1 = v(s.data.courses.survey_sites[1].pos);
    // Zu schnell: keine Messung.
    park(&mut s, site0, 0.0);
    s.ship.vel = Vec2::new(sd.max_speed * 2.0, 0.0);
    for _ in 0..60 {
        s.step(&idle());
    }
    let hold = |s: &SimState| match &s.active.iter().find(|m| m.id == id).unwrap().kind {
        MissionKind::Survey { done, hold, .. } => (*done, *hold),
        _ => unreachable!(),
    };
    assert_eq!(hold(&s), (0, 0.0));
    park(&mut s, site0, 0.0);
    for _ in 0..((sd.seconds / DT) as usize + 5) {
        s.step(&idle());
    }
    assert_eq!(hold(&s).0, 1);
    assert_eq!(s.active[0].nav_target(&s), Some(site1));
    let credits = s.crew.credits;
    park(&mut s, site1, 0.0);
    for _ in 0..((sd.seconds / DT) as usize + 5) {
        s.step(&idle());
    }
    assert!(s.active.iter().all(|m| m.id != id), "Auftrag erfüllt");
    assert!(s.crew.credits > credits);
}

#[test]
fn survey_offers_come_from_survey_givers() {
    let mut s = sim();
    // Viele Angebote erzeugen: Messflüge tauchen auf und haben gültige Felder.
    for _ in 0..40 {
        s.offers.clear();
        s.refresh_offers();
        if s.offers
            .iter()
            .any(|m| matches!(m.kind, MissionKind::Survey { .. }))
        {
            break;
        }
    }
    let m = s
        .offers
        .iter()
        .find(|m| matches!(m.kind, MissionKind::Survey { .. }))
        .expect("Messflug im Angebot");
    let MissionKind::Survey { sites, .. } = &m.kind else {
        unreachable!()
    };
    assert!(!sites.is_empty());
    assert!(sites.iter().all(|&i| i < s.data.courses.survey_sites.len()));
    assert!(m.par > s.data.missions.survey.seconds);
}

fn spawn_crate(s: &mut SimState, mission: u32, pos: Vec2) -> u32 {
    let id = s.next_id();
    s.bodies.push(super::Body {
        id,
        kind: BodyKind::Crate {
            mission,
            name: "Druckkessel".into(),
        },
        pos,
        vel: Vec2::ZERO,
        angle: 0.0,
        ang_vel: 0.0,
        radius: super::missions::CRATE_RADIUS,
        mass: 9.0,
        prev_pos: pos,
        prev_angle: 0.0,
        alive: true,
        seed: 1,
        age: 0.0,
    });
    id
}

#[test]
fn haul_to_socket_station_completes_only_when_set_in_and_released() {
    let mut s = sim();
    let kepler = s.data.station_index("kepler").unwrap();
    let sock = s.world.stations[kepler]
        .socket
        .expect("Kepler hat eine Lastaufnahme");
    let mid = s.next_id();
    s.active.push(Mission {
        id: mid,
        kind: MissionKind::Haul {
            from: 0,
            to: kepler,
            cargo: "Druckkessel".into(),
            mass: 9.0,
            body: None,
            settle: 0.0,
        },
        reward: 300,
        origin: Some(super::world::Owner::Station(0)),
        giver: None,
        start: Some(Box::new(s.stats.clone())),
        top_speed: 0.0,
        max_strain: None,
        par: 300.0,
    });
    // Neben der Station, aber nicht in der Aufnahme: früher hätte das gereicht, jetzt nicht.
    let kpos = s.world.stations[kepler].pos;
    let bid = spawn_crate(&mut s, mid, kpos + Vec2::new(0.0, -50.0));
    if let MissionKind::Haul { body, .. } = &mut s.active[0].kind {
        *body = Some(bid);
    }
    park(&mut s, kpos + Vec2::new(-120.0, 60.0), 0.0);
    for _ in 0..90 {
        s.step(&idle());
    }
    assert_eq!(s.active.len(), 1);

    // In der Aufnahme, aber noch am Kran: zählt nicht.
    let b = s.bodies.iter_mut().find(|b| b.id == bid).unwrap();
    b.pos = sock.center - sock.open * 0.5;
    b.prev_pos = b.pos;
    b.vel = Vec2::ZERO;
    let crane = s
        .ship
        .tools
        .iter()
        .position(|t| t.kind == super::data::ToolKind::Crane)
        .unwrap();
    s.ship.tools[crane].crane = CraneState::Attached {
        body: bid,
        rope: 200.0,
        local: Vec2::ZERO,
    };
    for _ in 0..90 {
        s.step(&idle());
        // Seil schlaff halten: Schiff bleibt in Seillänge, Kiste ruht.
        let b = s.bodies.iter_mut().find(|b| b.id == bid).unwrap();
        b.vel = Vec2::ZERO;
    }
    assert_eq!(s.active.len(), 1, "am Kran gilt nicht als abgesetzt");

    s.ship.tools[crane].crane = CraneState::Idle;
    for _ in 0..80 {
        s.step(&idle());
    }
    assert!(s.active.is_empty(), "abgesetzt und losgelassen = erledigt");
}

#[test]
fn socket_walls_block_and_hold() {
    let s = sim();
    let kepler = s.data.station_index("kepler").unwrap();
    let sock = s.world.stations[kepler].socket.unwrap();
    assert!(sock.holds(sock.center - sock.open * 0.8));
    assert!(
        !sock.holds(sock.center + sock.open * 3.0),
        "vor der Öffnung"
    );
    // Drei Wände als statische Kollider der Station.
    let walls = s
        .world
        .colliders
        .iter()
        .filter(|c| {
            c.station == Some(kepler) && c.aabb.expand(0.1).contains(sock.center - sock.open * 2.3)
        })
        .count();
    assert!(walls >= 1, "Rückwand vorhanden");
}

#[test]
fn course_points_and_survey_fields_lie_in_free_space() {
    let s = sim();
    for c in &s.data.courses.courses {
        for (i, st) in c.steps.iter().enumerate() {
            let pts: Vec<Vec2> = match st.kind {
                StepKind::Gate { pos, dir, width } => {
                    let d = Vec2::new(dir.to_radians().cos(), dir.to_radians().sin());
                    let side = Vec2::new(-d.y, d.x);
                    (0..=8)
                        .map(|k| v(pos) + side * width * (k as f32 / 8.0 - 0.5))
                        .collect()
                }
                StepKind::Pass { pos, .. }
                | StepKind::Face { pos, .. }
                | StepKind::Hold { pos, .. } => vec![v(pos)],
                StepKind::Dock { .. } => vec![],
            };
            for p in pts {
                assert!(
                    !s.point_in_static(p),
                    "{} Schritt {i}: {p:?} steckt in einer Wand",
                    c.id
                );
            }
        }
    }
    for site in &s.data.courses.survey_sites {
        assert!(
            !s.point_in_static(v(site.pos)),
            "Messfeld {} in einer Wand",
            site.name
        );
        for p in &s.world.planets {
            assert!(
                (v(site.pos) - p.pos).length() > p.radius + site.radius,
                "Messfeld {} im Planeten",
                site.name
            );
        }
    }
}
