//! Tests für Phase 12: NPC-Schiffe mit voller Physik – Verkehr, Kollisionen, Piratendrohnen,
//! Geleitschutz, Schmuggel und Zoll, Händlerin, Mechaniker, Rivalen-Crew.

use std::sync::Arc;

use bevy::math::Vec2;

use super::data::{CrewSave, GameData, Ore};
use super::economy::Purchase;
use super::missions::{Mission, MissionKind};
use super::npc::{Nav, Role};
use super::ship::{CargoKind, Loadout};
use super::world::Owner;
use super::{Command, Projectile, SimEvent, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let mut save = CrewSave::new_game(&data);
    save.credits = 5_000;
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 2)
}

fn idle(s: &mut SimState, seconds: f32) {
    for _ in 0..(seconds * 60.0) as u32 {
        s.step(&TickInput::default());
    }
}

/// Crew-Schiff frei im Raum abstellen.
fn float_at(s: &mut SimState, pos: Vec2) {
    s.ship.docked = None;
    s.ship.pos = pos;
    s.ship.prev_pos = pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
}

fn npc<'a>(s: &'a SimState, name: &str) -> &'a super::npc::Npc {
    s.npcs.iter().find(|n| n.name == name).unwrap()
}

fn mission(s: &mut SimState, kind: MissionKind, origin: usize) -> u32 {
    let id = s.next_id();
    s.offers.push(Mission {
        id,
        kind,
        reward: 400,
        origin: Some(Owner::Station(origin)),
        giver: None,
        start: None,
        top_speed: 0.0,
        max_strain: None,
        par: 600.0,
    });
    s.step(&TickInput {
        commands: vec![Command::AcceptMission { id }],
        ..Default::default()
    });
    id
}

#[test]
fn traffic_spawns_docked_ships_with_their_own_physics() {
    let s = sim();
    for name in ["Möwe", "Kranich", "Juno", "Crew Kestrel"] {
        let n = npc(&s, name);
        assert!(
            n.alive && n.docked_pad().is_some(),
            "{name} steht an einer Plattform"
        );
        assert!(
            n.ship.mass > 1.0 && n.ship.thrusters.len() == 2,
            "{name}: echtes Schiff"
        );
    }
    // Keine zwei Schiffe auf derselben Plattform, die Crew-Plattform bleibt frei.
    let mut pads: Vec<usize> = s.npcs.iter().filter_map(|n| n.docked_pad()).collect();
    pads.sort();
    let n = pads.len();
    pads.dedup();
    assert_eq!(pads.len(), n);
    assert!(!pads.contains(&s.ship.docked.unwrap()));
    assert_eq!(
        s.npcs
            .iter()
            .filter(|n| matches!(n.role, Role::Miner { .. }))
            .count(),
        2
    );
}

#[test]
fn traders_fly_their_route_and_dock_without_crashing() {
    let mut s = sim();
    let leg_of = |s: &SimState, i: usize| match &s.npcs[i].role {
        Role::Trader { leg, .. } | Role::Merchant { leg, .. } => Some(*leg),
        _ => None,
    };
    let mut legs: Vec<Option<usize>> = (0..s.npcs.len()).map(|i| leg_of(&s, i)).collect();
    let mut done = vec![0u32; s.npcs.len()];
    let hull: Vec<f32> = s.npcs.iter().map(|n| n.ship.hull).collect();
    let mut docked_events = 0;
    let mut was_docked: Vec<bool> = s.npcs.iter().map(|n| n.docked_pad().is_some()).collect();
    for _ in 0..60 * 480 {
        s.step(&TickInput::default());
        for (i, n) in s.npcs.iter().enumerate().take(was_docked.len()) {
            let d = n.docked_pad().is_some();
            if d && !was_docked[i] {
                docked_events += 1;
            }
            was_docked[i] = d;
        }
        for (i, l) in legs.iter_mut().enumerate() {
            let now = leg_of(&s, i);
            if now != *l {
                done[i] += 1;
                *l = now;
            }
        }
    }
    for (i, n) in s.npcs.iter().enumerate().take(done.len()) {
        if leg_of(&s, i).is_some() {
            assert!(
                done[i] >= 1,
                "{} hat nach 8 Minuten eine Etappe geschafft",
                n.name
            );
        }
    }
    assert!(docked_events >= 4, "angedockt: {docked_events}");
    for (n, h) in s.npcs.iter().zip(hull) {
        assert!(n.alive, "{} lebt", n.name);
        assert!(
            n.ship.hull > h - 25.0,
            "{}: {} → {}",
            n.name,
            h,
            n.ship.hull
        );
    }
}

#[test]
fn npc_ships_bump_into_the_crew_ship_physically() {
    let mut s = sim();
    let pos = Vec2::new(300.0, 200.0);
    float_at(&mut s, pos);
    // Ein Frachter treibt mit 4 m/s auf das Crew-Schiff zu.
    let i = s.npcs.iter().position(|n| n.name == "Möwe").unwrap();
    {
        let n = &mut s.npcs[i];
        // Es will an der Crew vorbei – mitten durch.
        n.nav = Nav::Hold {
            target: pos + Vec2::new(30.0, 0.0),
        };
        n.ship.pos = pos + Vec2::new(-9.0, 0.0);
        n.ship.prev_pos = n.ship.pos;
        n.ship.vel = Vec2::new(4.0, 0.0);
        // Nase nach rechts, Richtung Crew.
        n.ship.angle = -std::f32::consts::FRAC_PI_2;
        n.ship.prev_angle = n.ship.angle;
    }
    let mut hit = false;
    for _ in 0..240 {
        s.step(&TickInput::default());
        hit |= s
            .events
            .iter()
            .any(|e| matches!(e, SimEvent::Impact { .. }));
        if s.ship.vel.x > 0.5 {
            break;
        }
    }
    assert!(hit, "Stoß gemeldet");
    assert!(
        s.ship.vel.x > 0.5,
        "Crew-Schiff wird angeschoben: {:?}",
        s.ship.vel
    );
    assert!(s.npcs[i].ship.vel.x < 4.0, "der Frachter wird gebremst");
}

#[test]
fn drones_spawn_near_their_nest_and_shoot_at_the_crew() {
    let mut s = sim();
    let nest = Vec2::new(
        s.data.traffic.nests[0].center.0,
        s.data.traffic.nests[0].center.1,
    );
    float_at(&mut s, nest + Vec2::new(0.0, 120.0));
    let (shield, hull) = (s.ship.shield, s.ship.hull);
    let mut shots = 0;
    for _ in 0..60 * 30 {
        // Die Crew hält still (Position festhalten, damit nur die Drohnen arbeiten).
        s.ship.pos = nest + Vec2::new(0.0, 120.0);
        s.ship.vel = Vec2::ZERO;
        s.step(&TickInput::default());
        shots += s.projectiles.iter().filter(|p| p.hostile).count().min(1);
    }
    let drones = s
        .npcs
        .iter()
        .filter(|n| n.alive && n.hostile && matches!(n.role, Role::Drone { nest: Some(0) }))
        .count();
    assert!(drones >= 2, "Drohnen aus dem Nest: {drones}");
    assert!(shots > 0, "Drohnen feuern");
    assert!(
        s.ship.shield < shield || s.ship.hull < hull,
        "Treffer: Schild {shield} → {}, Hülle {hull} → {}",
        s.ship.shield,
        s.ship.hull
    );
}

#[test]
fn crew_shots_destroy_a_drone_and_leave_salvage() {
    let mut s = sim();
    let pos = Vec2::new(-300.0, 600.0);
    float_at(&mut s, pos + Vec2::new(0.0, -200.0));
    let id = s.spawn_drone(pos, None, None);
    let mut down = false;
    for k in 0..20 {
        // Alle 0,25 s ein Schuss von unten auf die Drohne.
        let d = s.npcs.iter().find(|n| n.id == id);
        let Some(target) = d.filter(|n| n.alive).map(|n| n.ship.pos) else {
            break;
        };
        let from = target + Vec2::new(0.0, -6.0);
        let pid = s.next_id();
        s.projectiles.push(Projectile {
            id: pid,
            pos: from,
            prev_pos: from,
            vel: Vec2::new(0.0, 60.0),
            life: 1.0,
            hostile: false,
        });
        for _ in 0..15 {
            s.step(&TickInput::default());
            down |= s
                .events
                .iter()
                .any(|e| matches!(e, SimEvent::DroneDown { .. }));
        }
        assert!(k < 6, "spätestens nach ein paar Treffern zerstört");
    }
    assert!(down, "Drohne zerstört");
    assert!(
        !s.npcs.iter().any(|n| n.id == id),
        "zerstörte Drohnen verschwinden"
    );
    assert!(
        s.bodies
            .iter()
            .any(|b| matches!(b.kind, super::BodyKind::Salvage { .. })
                && (b.pos - pos).length() < 30.0),
        "Bauteil zum Bergen"
    );
}

#[test]
fn escort_convoy_waits_for_the_crew_gets_ambushed_and_arrives() {
    let mut s = sim();
    let nova = 0;
    let kepler = s.data.station_index("kepler").unwrap();
    let id = mission(
        &mut s,
        MissionKind::Escort {
            from: nova,
            to: kepler,
            name: "Frachter Lerche".into(),
            npc: None,
            ambushed: false,
        },
        nova,
    );
    let convoy = match s.active.iter().find(|m| m.id == id).map(|m| &m.kind) {
        Some(MissionKind::Escort { npc: Some(c), .. }) => *c,
        other => panic!("Konvoi nicht erzeugt: {other:?}"),
    };
    // Solange die Crew angedockt ist, wartet der Frachter.
    idle(&mut s, 25.0);
    let c = s.npcs.iter().find(|n| n.id == convoy).unwrap();
    assert!(!c.has_departed(), "wartet");
    assert!(c.ship.vel.length() < 3.0);
    // Abdocken und in der Nähe schweben: los geht's.
    let mut ambushed = false;
    let mut done = false;
    for _ in 0..60 * 400 {
        let Some(c) = s.npcs.iter().find(|n| n.id == convoy && n.alive) else {
            break;
        };
        // Die Crew begleitet den Frachter (schwebt seitlich versetzt mit).
        let at = c.ship.pos + Vec2::new(0.0, 40.0);
        float_at(&mut s, at);
        // … und schießt jeden Angreifer sofort ab.
        let drones: Vec<usize> = (0..s.npcs.len())
            .filter(|&i| s.npcs[i].alive && s.npcs[i].prey == Some(convoy))
            .collect();
        ambushed |= !drones.is_empty();
        for i in drones {
            s.damage_npc(i, 100.0);
        }
        s.step(&TickInput::default());
        if s.events.iter().any(|e| {
            *e == SimEvent::MissionCompleted { id, reward: 0 }
                || matches!(e, SimEvent::MissionCompleted { id: m, .. } if *m == id)
        }) {
            done = true;
            break;
        }
    }
    assert!(ambushed, "Hinterhalt unterwegs");
    assert!(done, "Konvoi angekommen, Auftrag erfüllt");
}

#[test]
fn destroyed_convoy_fails_the_escort() {
    let mut s = sim();
    let kepler = s.data.station_index("kepler").unwrap();
    let id = mission(
        &mut s,
        MissionKind::Escort {
            from: 0,
            to: kepler,
            name: "Frachter Reiher".into(),
            npc: None,
            ambushed: false,
        },
        0,
    );
    let ci = s
        .npcs
        .iter()
        .position(|n| matches!(n.role, Role::Convoy { mission, .. } if mission == id))
        .unwrap();
    s.damage_npc(ci, 10_000.0);
    assert!(!s.active.iter().any(|m| m.id == id), "Auftrag gescheitert");
    assert!(s.events.contains(&SimEvent::MissionFailed { id }));
}

fn smuggle(s: &mut SimState) -> u32 {
    let neb = s.data.station_index("nebelhafen").unwrap();
    let kepler = s.data.station_index("kepler").unwrap();
    let id = s.next_id();
    s.active.push(Mission {
        id,
        kind: MissionKind::Smuggle {
            from: neb,
            to: kepler,
            cargo: "Nebelperlen".into(),
            mass: 2.0,
        },
        reward: 500,
        origin: Some(Owner::Station(neb)),
        giver: None,
        start: None,
        top_speed: 0.0,
        max_strain: None,
        par: 300.0,
    });
    s.ship.store(
        CargoKind::Container {
            mission: id,
            name: "Nebelperlen".into(),
        },
        2.0,
    );
    id
}

#[test]
fn customs_buoy_catches_slow_smugglers() {
    let mut s = sim();
    let id = smuggle(&mut s);
    let cp = &s.data.traffic.checkpoints[1];
    let buoy = Vec2::new(cp.pos.0, cp.pos.1);
    let credits = s.crew.credits;
    let fine = s.data.missions.smuggle.fine;
    let scan = s.data.missions.smuggle.scan_time;
    let mut busted = false;
    for _ in 0..((scan + 1.0) * 60.0) as u32 {
        float_at(&mut s, buoy + Vec2::new(10.0, 0.0));
        s.step(&TickInput::default());
        busted |= s.events.contains(&SimEvent::Busted);
    }
    assert!(busted, "erwischt");
    assert!(!s.active.iter().any(|m| m.id == id), "Ware beschlagnahmt");
    assert_eq!(s.crew.credits, credits - fine);
    assert!(s.contraband().is_none());
}

#[test]
fn quick_pass_through_a_buoy_and_delivery_pays() {
    let mut s = sim();
    let id = smuggle(&mut s);
    let cp = &s.data.traffic.checkpoints[1];
    let buoy = Vec2::new(cp.pos.0, cp.pos.1);
    let scan = s.data.missions.smuggle.scan_time;
    for _ in 0..((scan * 0.6) * 60.0) as u32 {
        float_at(&mut s, buoy);
        s.step(&TickInput::default());
    }
    assert!(s.customs.as_ref().is_some_and(|c| c.progress > 0.4));
    // Raus aus dem Radius: der Scan bricht ab.
    float_at(&mut s, buoy + Vec2::new(0.0, 400.0));
    s.step(&TickInput::default());
    assert!(s.customs.is_none());
    assert!(s.active.iter().any(|m| m.id == id));
    // Andocken am Ziel: bezahlt.
    let credits = s.crew.credits;
    let kepler = s.data.station_index("kepler").unwrap();
    s.ship.docked = None;
    s.dock_at_station(kepler);
    s.on_docked(Owner::Station(kepler));
    assert!(!s.active.iter().any(|m| m.id == id));
    assert!(s.crew.credits > credits);
}

#[test]
fn merchant_sells_material_only_where_her_ship_is_docked() {
    let mut s = sim();
    let kepler = s.data.station_index("kepler").unwrap();
    // Juno startet in Kepler.
    assert!(npc(&s, "Juno").docked_pad().is_some());
    assert!(
        s.purchase_info(&Purchase::Goods(1)).is_err(),
        "nicht in Nova"
    );
    s.ship.docked = None;
    s.dock_at_station(kepler);
    assert!(s.merchant_here());
    let kobalt = s.stored(Ore::Kobalt);
    let credits = s.crew.credits;
    s.step(&TickInput {
        commands: vec![Command::Buy {
            purchase: Purchase::Goods(1),
            voter: 0,
        }],
        ..Default::default()
    });
    // Zwei Crewmitglieder: Abstimmung, beide stimmen zu.
    for voter in 0..2 {
        s.step(&TickInput {
            commands: vec![Command::Vote { voter, yes: true }],
            ..Default::default()
        });
    }
    idle(&mut s, 0.5);
    let price = s.data.traffic.merchant.as_ref().unwrap().goods[1].price;
    assert!((s.stored(Ore::Kobalt) - kobalt - 6.0).abs() < 1e-3);
    assert_eq!(s.crew.credits, credits - price);
}

#[test]
fn mechanic_upgrades_only_at_his_shipyard() {
    let mut s = sim();
    s.crew.storage = [20.0; 5];
    s.crew.storage_parts = 4;
    let err = s
        .purchase_info(&Purchase::Upgrade("fein_schub".into()))
        .unwrap_err();
    assert!(err.contains("Orsk"), "{err}");
    let werft = s.data.station_index("werft").unwrap();
    s.ship.docked = None;
    s.dock_at_station(werft);
    assert!(s.vendor_here("orsk"));
    s.purchase_info(&Purchase::Upgrade("fein_schub".into()))
        .expect("in der Werft Orion zu haben");
    // Ohne Zusatzmasse.
    assert_eq!(s.data.upgrade("fein_schub").unwrap().mass, 0.0);
}

#[test]
fn rival_crew_takes_an_open_distress_call() {
    let mut s = sim();
    let (site, offer) = s
        .offers
        .iter()
        .find_map(|m| match m.kind {
            MissionKind::Tow { site, .. } | MissionKind::Capsules { site, .. }
                if m.is_distress() =>
            {
                Some((site, m.id))
            }
            _ => None,
        })
        .expect("Notruf im Angebot");
    let i = s
        .npcs
        .iter()
        .position(|n| matches!(n.role, Role::Rival { .. }))
        .unwrap();
    let home = match s.npcs[i].role {
        Role::Rival { home, .. } => home,
        _ => unreachable!(),
    };
    {
        let n = &mut s.npcs[i];
        n.role = Role::Rival {
            home,
            goal: Some(site),
        };
        n.ship.pos = site + Vec2::new(3.0, 0.0);
        n.ship.prev_pos = n.ship.pos;
        n.ship.vel = Vec2::ZERO;
        n.nav = Nav::Hold { target: n.ship.pos };
    }
    let mut took = false;
    for _ in 0..30 {
        s.step(&TickInput::default());
        took |= s.events.contains(&SimEvent::RivalTook);
    }
    assert!(took);
    assert_eq!(s.rival_score, 1);
    assert!(!s.offers.iter().any(|m| m.id == offer));
}

#[test]
fn traffic_is_deterministic() {
    let mut a = sim();
    let mut b = sim();
    for _ in 0..60 * 40 {
        a.step(&TickInput::default());
        b.step(&TickInput::default());
    }
    assert_eq!(a.npcs.len(), b.npcs.len());
    for (x, y) in a.npcs.iter().zip(&b.npcs) {
        assert_eq!(x.ship.pos, y.ship.pos);
        assert_eq!(x.nav, y.nav);
    }
}
