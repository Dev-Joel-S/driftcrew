//! Tests für Phase 14: Ladeplan, Abwurf, Flugeigenschaften der Fracht, Zusatzfunde, Rettung
//! und zwei Kräne an einem Objekt.

use std::sync::Arc;

use bevy::math::Vec2;

use super::cargo::{PERSON_MASS, RESCUE_STEP};
use super::data::{CargoTrait, CrewSave, GameData, ToolKind};
use super::missions::{Mission, MissionKind};
use super::npc::Role;
use super::ship::{CargoItem, CargoKind, CraneState, Loadout};
use super::world::Owner;
use super::{Body, BodyKind, Command, DT, MAX_SLOTS, SimEvent, SimState, TickInput, ToastKind};

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

fn cmd(s: &mut SimState, c: Command) {
    s.step(&TickInput {
        commands: vec![c],
        ..Default::default()
    });
}

/// Crew-Schiff frei im Raum abstellen (weit weg von allem).
fn float_at(s: &mut SimState, pos: Vec2) {
    s.ship.docked = None;
    s.ship.pos = pos;
    s.ship.prev_pos = pos;
    s.ship.vel = Vec2::ZERO;
    s.ship.ang_vel = 0.0;
    s.ship.angle = 0.0;
    s.ship.prev_angle = 0.0;
}

fn toasts(s: &SimState) -> Vec<String> {
    s.events
        .iter()
        .filter_map(|e| match e {
            SimEvent::Toast { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Einen Tick „fliegen“ mit vorgegebener Beschleunigung (ohne Triebwerke, nur die Fracht).
fn push(s: &mut SimState, acc: Vec2) {
    s.ship.vel += acc * DT;
    s.update_cargo();
}

fn container(s: &mut SimState, mass: f32, traits: CargoTrait) -> u32 {
    s.ship.store_with(
        CargoKind::Container {
            mission: 0,
            name: "Testfracht".into(),
        },
        mass,
        traits,
    );
    s.ship.cargo.last().unwrap().id
}

/// Ruhige Stelle ohne Sog, Felder und Stationen.
const FAR: Vec2 = Vec2::new(-900.0, -900.0);

#[test]
fn moving_cargo_shifts_the_center_of_mass_without_pushing_the_ship() {
    let mut s = sim();
    assert!(s.ship.pods.len() >= 2, "Startschiff hat zwei Frachtmodule");
    let id = container(&mut s, 4.0, CargoTrait::None);
    let pod = s.ship.cargo[0].pod;
    let other = (pod + 1) % s.ship.pods.len();
    let side = |s: &SimState| s.ship.com.x.signum();
    let before = side(&s);
    assert_ne!(before, 0.0, "Fracht seitlich: Schwerpunkt verschoben");
    // Angedockt: sofort umgeladen.
    cmd(&mut s, Command::MoveCargo { id, to: other });
    assert_eq!(s.ship.cargo[0].pod, other);
    assert_eq!(
        side(&s),
        -before,
        "Schwerpunkt wandert auf die andere Seite"
    );

    // Im Flug: die Last wandert durchs Schiff, das Schiff selbst bekommt keinen Schubs.
    float_at(&mut s, FAR);
    let com0 = s.ship.com;
    let hull0 = s.ship.origin();
    cmd(&mut s, Command::MoveCargo { id, to: pod });
    assert!(s.ship.cargo[0].moving.is_some(), "Umladen braucht Zeit");
    assert!(
        s.ship.com.x.abs() < com0.x.abs(),
        "unterwegs sitzt die Masse dazwischen"
    );
    idle(&mut s, 0.8 + 0.6 * 4.0 + 0.3);
    assert!(s.ship.cargo[0].moving.is_none());
    assert_eq!(s.ship.cargo[0].pod, pod);
    assert_eq!(side(&s), before);
    assert!(
        s.ship.vel.length() < 1e-3,
        "keine Zugkraft: {:?}",
        s.ship.vel
    );
    assert!(
        s.ship.ang_vel.abs() < 1e-3,
        "kein Drehimpuls: {}",
        s.ship.ang_vel
    );
    assert!(
        (s.ship.origin() - hull0).length() < 1e-3,
        "der Rumpf bleibt, wo er ist"
    );
}

#[test]
fn moving_into_a_full_module_is_refused() {
    let mut s = sim();
    let a = container(&mut s, 3.0, CargoTrait::None);
    let pod_a = s.ship.cargo[0].pod;
    let other = (pod_a + 1) % s.ship.pods.len();
    // Das andere Modul randvoll machen.
    let cap = s.ship.pods[other].capacity;
    s.ship.cargo.push(CargoItem {
        id: 99,
        pod: other,
        ..CargoItem::new(
            CargoKind::Ore(super::data::Ore::Ferrit),
            cap,
            CargoTrait::None,
        )
    });
    s.ship.recompute_mass();
    cmd(&mut s, Command::MoveCargo { id: a, to: other });
    assert_eq!(s.ship.cargo[0].pod, pod_a);
    assert!(
        toasts(&s).iter().any(|t| t.contains("nicht genug Platz")),
        "{:?}",
        toasts(&s)
    );
}

#[test]
fn jettisoned_cargo_drifts_is_saved_and_can_be_picked_up_again() {
    let mut s = sim();
    float_at(&mut s, FAR);
    s.ship.store_item(CargoItem {
        cond: 0.7,
        ..CargoItem::new(
            CargoKind::Salvage {
                name: "Sensorphalanx".into(),
                value: 300,
            },
            2.0,
            CargoTrait::Fragile,
        )
    });
    let id = s.ship.cargo[0].id;
    let mass0 = s.ship.mass;
    cmd(&mut s, Command::Jettison { id });
    assert!(s.ship.cargo.is_empty());
    assert!(s.ship.mass < mass0 - 1.9);
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, SimEvent::Jettisoned { .. }))
    );
    let bi = s
        .bodies
        .iter()
        .position(|b| matches!(b.kind, BodyKind::Dropped { .. }))
        .expect("treibt in der Welt");
    idle(&mut s, 3.0);
    let b = &s.bodies[bi];
    assert!(b.vel.length() < 1.5, "kommt zur Ruhe: {:?}", b.vel);
    let dropped_at = b.pos;

    // Gespeichert und nach dem Laden wieder da – mit Zustand und Eigenschaft.
    let save = s.to_save();
    assert_eq!(save.dropped.len(), 1);
    let data = s.data.clone();
    let def = data.ship(&save.current_ship).clone();
    let mut t = SimState::new(data, &save, Loadout::full(&def), 2);
    let bi = t
        .bodies
        .iter()
        .position(|b| matches!(b.kind, BodyKind::Dropped { .. }))
        .expect("nach dem Laden noch da");
    assert!(
        (t.bodies[bi].pos - dropped_at).length() < 1.0,
        "an derselben Stelle"
    );

    // Wieder einsammeln.
    assert!(t.try_stow(bi));
    let c = t.ship.cargo.last().unwrap();
    assert_eq!(c.traits, CargoTrait::Fragile);
    assert!((c.cond - 0.7).abs() < 1e-4);
    assert!(matches!(c.kind, CargoKind::Salvage { value: 300, .. }));
}

#[test]
fn survivors_are_never_jettisoned() {
    let mut s = sim();
    float_at(&mut s, FAR);
    s.ship
        .store(CargoKind::Survivor { mission: 7 }, PERSON_MASS);
    let id = s.ship.cargo[0].id;
    cmd(&mut s, Command::Jettison { id });
    assert_eq!(s.ship.cargo.len(), 1);
}

#[test]
fn liquid_sloshes_behind_but_steady_thrust_feels_rigid() {
    let mut s = sim();
    float_at(&mut s, FAR);
    container(&mut s, 5.0, CargoTrait::Tank);
    s.update_cargo();
    // Gleichmäßig beschleunigen: die Flüssigkeit legt sich an die Wand und fährt starr mit.
    let a = Vec2::new(0.0, 6.0);
    for _ in 0..240 {
        push(&mut s, a);
    }
    let v = s.ship.vel;
    let slosh = s.ship.cargo[0].slosh;
    assert!(slosh.y < -0.3, "Flüssigkeit liegt hinten: {slosh:?}");
    push(&mut s, a);
    assert!(
        (s.ship.vel - (v + a * DT)).length() < 1e-3,
        "gleichmäßiger Schub: keine Zusatzkraft"
    );
    // Schub aus: die Flüssigkeit schwappt nach vorn und schiebt das Schiff nach.
    let v_stop = s.ship.vel;
    let mut peak = 0.0f32;
    for _ in 0..90 {
        push(&mut s, Vec2::ZERO);
        peak = peak.max((s.ship.vel - v_stop).length());
    }
    assert!(peak > 0.02, "Nachschwappen spürbar: {peak}");
}

#[test]
fn fragile_cargo_takes_damage_from_hard_knocks_and_pays_by_condition() {
    let mut s = sim();
    let home = s.docked_station().unwrap();
    let to = (0..s.world.stations.len()).find(|&i| i != home).unwrap();
    let id = s.next_id();
    s.offers.push(Mission {
        id,
        kind: MissionKind::Delivery {
            from: Owner::Station(home),
            to,
            cargo: "Messinstrumente".into(),
            mass: 2.5,
            traits: CargoTrait::Fragile,
        },
        reward: 400,
        origin: Some(Owner::Station(home)),
        giver: None,
        start: None,
        top_speed: 0.0,
        max_strain: None,
        par: 600.0,
    });
    cmd(&mut s, Command::AcceptMission { id });
    let c = s
        .ship
        .cargo
        .iter()
        .find(|c| c.traits == CargoTrait::Fragile);
    assert!(c.is_some(), "Container mit Eigenschaft an Bord");
    float_at(&mut s, FAR);
    s.update_cargo();
    // Sanft: nichts passiert.
    for _ in 0..60 {
        push(&mut s, Vec2::new(0.0, 12.0));
    }
    assert!((s.delivery_condition(id) - 1.0).abs() < 1e-4);
    // Harter Stoß: 3 m/s in einem Tick.
    s.ship.vel += Vec2::new(3.0, 0.0);
    s.update_cargo();
    let cond = s.delivery_condition(id);
    assert!(cond < 0.8 && cond > 0.0, "Zustand gesunken: {cond}");
    assert!(toasts(&s).iter().any(|t| t.contains("beschädigt")));
    // Abrechnung nach Zustand.
    s.ship.cargo.iter_mut().for_each(|c| c.cond = 0.5);
    let idx = s.active.iter().position(|m| m.id == id).unwrap();
    s.complete_mission(idx);
    let r = s.report.as_ref().unwrap();
    assert_eq!(r.reward, 260, "400 × (0,3 + 0,7 × 0,5)");
    assert_eq!(r.cond, Some(0.5));
}

#[test]
fn unstable_cargo_builds_stress_under_hard_acceleration_and_blows() {
    let mut s = sim();
    float_at(&mut s, FAR);
    container(&mut s, 2.0, CargoTrait::Unstable);
    s.update_cargo();
    // Unter der Grenze: keine Belastung.
    for _ in 0..300 {
        push(&mut s, Vec2::new(0.0, 10.0));
    }
    assert!(s.ship.cargo[0].stress < 1e-4);
    // Darüber: Belastung steigt, irgendwann verpufft es.
    let hull = s.ship.hull + s.ship.shield;
    let mut blew = false;
    for _ in 0..600 {
        push(&mut s, Vec2::new(0.0, 28.0));
        if s.ship.cargo.is_empty() {
            blew = true;
            break;
        }
    }
    assert!(blew, "instabile Fracht verpufft");
    assert!(
        s.ship.hull + s.ship.shield < hull,
        "und beschädigt das Schiff"
    );
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, SimEvent::Explosion { .. }))
    );
}

#[test]
fn cargo_traits_come_from_the_mission_data() {
    let s = sim();
    let md = &s.data.missions;
    for t in [CargoTrait::Tank, CargoTrait::Fragile, CargoTrait::Unstable] {
        assert!(
            md.delivery_cargo.iter().any(|c| c.traits == t),
            "{t:?} kommt in Lieferaufträgen vor"
        );
    }
    assert!(!md.rescue.ships.is_empty() && !md.bonus.names.is_empty());
}

fn rescue_mission(s: &mut SimState, site: Vec2, crew: u32) -> u32 {
    let id = s.next_id();
    let to = s.crew.home_station;
    s.offers.push(Mission {
        id,
        kind: MissionKind::Rescue {
            site,
            to,
            ship: "Kutter Ilse".into(),
            captain: "Kapitänin Ilse Brandt".into(),
            crew,
            aboard: 0,
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
    cmd(s, Command::AcceptMission { id });
    id
}

/// Neben das havarierte Schiff stellen (Lücke etwa 2 m).
fn alongside(s: &mut SimState) {
    let n = s
        .npcs
        .iter()
        .find(|n| matches!(n.role, Role::Stranded { .. }))
        .unwrap();
    let at = n.ship.pos + Vec2::new(n.ship.bound_radius() + s.ship.bound_radius() + 2.0, 0.0);
    let vel = n.ship.vel;
    float_at(s, at);
    s.ship.vel = vel;
}

#[test]
fn rescue_brings_crew_aboard_person_by_person_and_they_thank_later() {
    let mut s = sim();
    let site = FAR + Vec2::new(200.0, 0.0);
    let id = rescue_mission(&mut s, site, 3);
    let n = s
        .npcs
        .iter()
        .find(|n| matches!(n.role, Role::Stranded { mission } if mission == id))
        .expect("havariertes Schiff treibt am Notrufort");
    assert!((n.ship.pos - site).length() < 1.0);
    assert_eq!(n.label(), "Havariert");
    alongside(&mut s);
    let mass0 = s.ship.mass;
    idle(&mut s, RESCUE_STEP * 3.0 + 0.5);
    let MissionKind::Rescue { aboard, .. } = s.active.iter().find(|m| m.id == id).unwrap().kind
    else {
        unreachable!()
    };
    assert_eq!(aboard, 3);
    let people = s
        .ship
        .cargo
        .iter()
        .filter(|c| c.kind == CargoKind::Survivor { mission: id })
        .count();
    assert_eq!(people, 3, "jede Person belegt Platz");
    assert!(s.ship.mass > mass0 + 3.0 * PERSON_MASS - 0.01);

    // An der Zielstation abliefern.
    let to = s.crew.home_station;
    s.dock_at_station(to);
    s.on_docked(Owner::Station(to));
    assert!(s.active.iter().all(|m| m.id != id), "Auftrag erfüllt");
    assert!(
        s.ship
            .cargo
            .iter()
            .all(|c| !matches!(c.kind, CargoKind::Survivor { .. }))
    );
    assert_eq!(s.crew.rescued.len(), 1);
    idle(&mut s, 0.5);
    assert!(
        s.npcs
            .iter()
            .all(|n| !matches!(n.role, Role::Stranded { .. })),
        "das havarierte Schiff wird abgeschleppt"
    );

    // Später meldet sich die Kapitänin – an einer Station, mit Geschenk.
    s.crew.rescued[0].wait = 0.05;
    let credits = s.crew.credits;
    let parts = s.crew.storage_parts;
    let mut story = false;
    for _ in 0..10 {
        s.step(&TickInput::default());
        story |= s.events.iter().any(
            |e| matches!(e, SimEvent::Story { speaker, .. } if speaker.contains("Ilse Brandt")),
        );
    }
    assert!(story, "Funkspruch der Geretteten");
    assert!(s.crew.credits > credits && s.crew.storage_parts > parts);
    assert!(s.crew.rescued.is_empty());
}

#[test]
fn rescue_needs_room_and_the_crew_decides_what_to_leave_behind() {
    let mut s = sim();
    let site = FAR + Vec2::new(-200.0, 0.0);
    let id = rescue_mission(&mut s, site, 2);
    // Frachtraum randvoll mit Ladung.
    for p in 0..s.ship.pods.len() {
        let cap = s.ship.pods[p].capacity;
        s.ship.cargo.push(CargoItem {
            id: 100 + p as u32,
            pod: p,
            ..CargoItem::new(
                CargoKind::Salvage {
                    name: "Ballast".into(),
                    value: 10,
                },
                cap,
                CargoTrait::None,
            )
        });
    }
    s.ship.recompute_mass();
    alongside(&mut s);
    let mut warned = false;
    for _ in 0..((RESCUE_STEP + 0.3) * 60.0) as u32 {
        s.step(&TickInput::default());
        warned |= s.events.iter().any(|e| {
            matches!(e, SimEvent::Toast { text, kind: ToastKind::Warn } if text.contains("Kein Platz"))
        });
    }
    assert!(warned, "Hinweis: kein Platz");
    let aboard = |s: &SimState| match s.active.iter().find(|m| m.id == id).unwrap().kind {
        MissionKind::Rescue { aboard, .. } => aboard,
        _ => unreachable!(),
    };
    assert_eq!(aboard(&s), 0);
    // Eigene Fracht zurücklassen: jetzt passt die Besatzung.
    cmd(&mut s, Command::Jettison { id: 100 });
    alongside(&mut s);
    idle(&mut s, RESCUE_STEP * 2.0 + 0.5);
    assert_eq!(aboard(&s), 2);
}

#[test]
fn bonus_find_is_optional_extra_salvage_near_the_site() {
    let mut s = sim();
    let site = FAR;
    // Mehrmals würfeln: mindestens einmal liegt etwas da.
    for _ in 0..12 {
        s.maybe_spawn_bonus(site);
    }
    let finds: Vec<&Body> = s
        .bodies
        .iter()
        .filter(|b| matches!(b.kind, BodyKind::Dropped { .. }))
        .collect();
    assert!(!finds.is_empty(), "Zusatzfund erscheint");
    for b in &finds {
        let d = (b.pos - site).length();
        assert!((30.0..=75.0).contains(&d), "nahe am Einsatzort: {d}");
        let BodyKind::Dropped { item } = &b.kind else {
            unreachable!()
        };
        assert!(matches!(item.kind, CargoKind::Salvage { value, .. } if value >= 200));
    }
    // Der Auftrag braucht ihn nicht: ein Kapsel-Notruf bleibt ohne Zusatzfund erfüllbar.
    assert!(
        toasts(&s)
            .iter()
            .any(|t| t.contains("mitnehmen ist freiwillig"))
    );
}

#[test]
fn two_cranes_grab_one_object_at_two_points() {
    let mut s = sim();
    float_at(&mut s, FAR);
    let cranes: Vec<usize> = s
        .ship
        .tools
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind == ToolKind::Crane)
        .map(|(i, _)| i)
        .collect();
    let (a, b) = match cranes.as_slice() {
        [a, b, ..] => (*a, *b),
        // Startschiff hat nur einen Kran: einen zweiten Kran-Slot simulieren.
        [a] => {
            let mut t = s.ship.tools[*a].clone();
            t.slot = s.ship.slot_count;
            s.ship.slot_count += 1;
            s.ship.tools.push(t);
            (*a, s.ship.tools.len() - 1)
        }
        [] => panic!("kein Kran"),
    };
    // Schwere Kiste vor dem Schiff.
    let mount = s.ship.tool_world_pos(a);
    let center = mount + Vec2::new(0.0, 8.0);
    let id = s.next_id();
    s.bodies.push(Body {
        id,
        kind: BodyKind::Crate {
            mission: 0,
            name: "Test".into(),
        },
        pos: center,
        vel: Vec2::ZERO,
        angle: 0.0,
        ang_vel: 0.0,
        radius: 1.5,
        mass: 8.0,
        prev_pos: center,
        prev_angle: 0.0,
        alive: true,
        seed: 1,
        age: 0.0,
    });
    // Beide Kräne zielen auf verschiedene Seiten der Kiste.
    let fire = |s: &mut SimState, ti: usize, target: Vec2| {
        let m = s.ship.tool_world_pos(ti);
        let slot = s.ship.tools[ti].slot;
        let mut aims = vec![0.0; MAX_SLOTS];
        aims[slot as usize] = (target - m).to_angle();
        // Kurz tippen, dann fährt der Greifer aus, bis er trifft.
        s.step(&TickInput {
            slots: 1 << slot,
            aims: aims.clone(),
            commands: Vec::new(),
            brake: false,
        });
        let mut texts = Vec::new();
        for _ in 0..120 {
            s.step(&TickInput {
                slots: 0,
                aims: aims.clone(),
                commands: Vec::new(),
                brake: false,
            });
            texts.extend(toasts(s));
            if matches!(s.ship.tools[ti].crane, CraneState::Attached { .. }) {
                break;
            }
        }
        texts
    };
    fire(&mut s, a, center + Vec2::new(-1.2, 0.0));
    let second = fire(&mut s, b, center + Vec2::new(1.2, 0.0));
    let local = |s: &SimState, ti: usize| match s.ship.tools[ti].crane {
        CraneState::Attached { body, local, .. } if body == id => Some(local),
        _ => None,
    };
    let (la, lb) = (local(&s, a), local(&s, b));
    assert!(la.is_some() && lb.is_some(), "beide Kräne halten die Kiste");
    let (la, lb) = (la.unwrap(), lb.unwrap());
    assert!(
        (la - lb).length() > 0.8,
        "zwei verschiedene Griffpunkte: {la:?} {lb:?}"
    );
    assert!(
        second.iter().any(|t| t.contains("Zweiter Kran")),
        "Hinweis beim zweiten Kran: {second:?}"
    );
}
