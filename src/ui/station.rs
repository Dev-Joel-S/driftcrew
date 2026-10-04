//! Stationsmenü (angedockt) und Abstimmungs-Popup der Crew.

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;

use super::{
    ACCENT, BAD, GOOD, Item, ItemButton, MENU_STATION, MUTED, MenuFocus, Signature, TEAL, TEXT,
    WARN, fmt_num, mouse_pick, navigate, panel, spawn_items, text,
};
use crate::game::{AppState, MapOpen, Paused, PendingCommands, Sim};
use crate::input::{ActiveBindings, Crew, Device, MenuInput, btn_just_pressed};
use crate::sim::data::{Service, ServiceEffect, StationKind};
use crate::sim::economy::{PaintPart, Purchase, VOTE_SECONDS};
use crate::sim::missions::MissionKind;
use crate::sim::world::Owner;
use crate::sim::{Command, SimState};

pub struct StationPlugin;

impl Plugin for StationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StationTab>()
            .init_resource::<BuildView>()
            .add_systems(OnEnter(AppState::Playing), spawn_roots)
            .add_systems(OnExit(AppState::Playing), despawn_roots)
            .add_systems(
                Update,
                (station_menu, vote_input, draw_vote).run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Resource, Default)]
pub struct StationTab(pub usize);

#[derive(Component)]
struct StationRoot;

#[derive(Component)]
struct VoteRoot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Service,
    Upgrades,
    Paint,
    Missions,
    Market,
    Ships,
    Project,
    Courses,
    Build,
    Merchant,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Service => "Service",
            Tab::Upgrades => "Upgrades",
            Tab::Paint => "Lack",
            Tab::Missions => "Aufträge",
            Tab::Market => "Markt",
            Tab::Ships => "Werft",
            Tab::Project => "Aufbau",
            Tab::Courses => "Parcours",
            Tab::Build => "Bau",
            Tab::Merchant => "Händlerin",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Buy(Purchase),
    Accept(u32),
    Abandon(u32),
    Sell,
    SellCharts,
    Deliver,
    Switch(String),
    Undock,
    Course(usize),
    AbortCourse,
    /// Werft-Editor: Bauplatz wählen (None = zurück zur Übersicht), Bauplan exportieren.
    SelectMount(Option<String>),
    Export,
    Store,
}

/// Welcher Bauplatz im Werft-Editor gerade gewählt ist (reine Anzeige) und ob der Editor offen
/// ist – das Overlay markiert dann die Bauplätze am Schiff.
#[derive(Resource, Default, Clone, Debug)]
pub struct BuildView {
    pub open: bool,
    pub selected: Option<String>,
}

/// Inhalt des Crew-Lagers in einer Zeile.
pub fn storage_line(sim: &SimState) -> String {
    let mut v: Vec<String> = crate::sim::data::Ore::ALL
        .iter()
        .filter(|o| sim.stored(**o) > 0.05)
        .map(|o| format!("{} {:.0}", o.label(), sim.stored(*o)))
        .collect();
    if v.is_empty() {
        v.push("kein Material".into());
    }
    v.push(format!("{} Teile", sim.crew.storage_parts));
    if !sim.crew.artifacts.is_empty() {
        v.push(format!("{} Artefakte", sim.crew.artifacts.len()));
    }
    format!("Lager (t): {}", v.join(" · "))
}

/// Einlagern: was ginge gerade ins Lager?
fn store_item(sim: &SimState) -> Option<Item<Act>> {
    let ore: f32 = crate::sim::data::Ore::ALL
        .iter()
        .map(|o| sim.ship.ore_amount(*o))
        .sum();
    let parts = sim
        .ship
        .cargo
        .iter()
        .filter(|c| matches!(c.kind, crate::sim::ship::CargoKind::Salvage { .. }))
        .count();
    if ore < 0.05 && parts == 0 {
        return None;
    }
    Some(
        Item::new("Ins Crew-Lager einlagern", Act::Store)
            .right(format!("{ore:.1} t · {parts} Teile"))
            .detail("Material und Bauteile für Module und Upgrades – in jeder Werft verbaubar"),
    )
}

/// Bauplan des aktuellen Schiffs (mit Modulen) als RON-Datei neben den Spielstand schreiben.
fn export_layout(sim: &SimState) -> String {
    let mut def = sim.current_def();
    let n = sim.build_of(&sim.crew.current_ship).len();
    if n > 0 {
        def.id = format!("{}_umbau", def.id);
        def.name = format!("{} (Umbau)", def.name);
    }
    let text = match ron::ser::to_string_pretty(&def, ron::ser::PrettyConfig::default()) {
        Ok(t) => t,
        Err(e) => return format!("Export fehlgeschlagen: {e}"),
    };
    let dir = crate::game::save_path()
        .parent()
        .map(|p| p.join("bauplaene"))
        .unwrap_or_else(|| std::path::PathBuf::from("bauplaene"));
    let path = dir.join(format!("{}.ron", def.id));
    let header = format!(
        "// Bauplan aus DriftCrew: {} mit {n} Modulen. In ships.ron als neuen Eintrag einfügen.\n",
        def.name
    );
    match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, header + &text)) {
        Ok(()) => format!("Bauplan gespeichert: {}", path.display()),
        Err(e) => format!("Export fehlgeschlagen: {e}"),
    }
}

/// Reiter „Bau“ (Werft): Bauplätze des Schiffs, Module, Werte, Export.
fn build_items(sim: &SimState, sel: &Option<String>, v: &mut Vec<Item<Act>>) {
    let base = sim.data.ship(&sim.crew.current_ship);
    let build = sim.build_of(&sim.crew.current_ship);
    let module_at = |mount: &str| {
        build
            .iter()
            .find(|(m, _)| m == mount)
            .and_then(|(_, md)| sim.data.module(md))
    };
    if let Some(it) = store_item(sim) {
        v.push(it);
    }
    if base.mounts.is_empty() {
        v.push(Item::new("Dieser Rumpf hat keine Bauplätze", Act::Undock).enabled(false));
        return;
    }
    match sel
        .as_ref()
        .and_then(|id| base.mounts.iter().find(|m| &m.id == id))
    {
        None => {
            for m in &base.mounts {
                let now = module_at(&m.id).map_or("leer".to_string(), |md| md.name.clone());
                let fits: Vec<&str> = m.accepts.iter().map(|a| a.label()).collect();
                v.push(
                    Item::new(
                        format!("{}: {now}", m.name),
                        Act::SelectMount(Some(m.id.clone())),
                    )
                    .right("wählen")
                    .detail(format!("passt: {}", fits.join(", "))),
                );
            }
            v.push(
                Item::new("Bauplan exportieren", Act::Export)
                    .detail("Schiff mit allen Modulen als RON-Datei – lässt sich in ships.ron als neuer Rumpf übernehmen"),
            );
        }
        Some(m) => {
            v.push(
                Item::new(format!("← {}", m.name), Act::SelectMount(None))
                    .detail("zurück zur Übersicht"),
            );
            if let Some(md) = module_at(&m.id) {
                let p = Purchase::RemoveModule {
                    mount: m.id.clone(),
                };
                v.push(
                    Item::new(format!("{} abbauen", md.name), Act::Buy(p))
                        .right("0 Cr")
                        .detail("Die Hälfte des Materials kommt ins Crew-Lager zurück"),
                );
            }
            for md in sim
                .data
                .modules
                .iter()
                .filter(|md| m.accepts.contains(&md.slot))
            {
                let p = Purchase::Module {
                    mount: m.id.clone(),
                    module: md.id.clone(),
                };
                let cost = crate::sim::economy::module_cost(md);
                let detail = format!("{} · +{:.1} t · {}", cost.label(), md.mass, md.description);
                let it = Item::new(format!("{} anbauen", md.name), Act::Buy(p.clone()))
                    .right(format!("{} Cr", md.credits));
                v.push(match sim.purchase_info(&p) {
                    Ok(_) => it.detail(detail),
                    Err(reason) => it.detail(format!("{detail} – {reason}")).enabled(false),
                });
            }
        }
    }
}

/// Parcours, die an dieser Station angeboten werden.
fn courses_here(sim: &SimState) -> Vec<usize> {
    let Some(si) = sim.docked_station() else {
        return Vec::new();
    };
    let id = &sim.world.stations[si].id;
    (0..sim.data.courses.courses.len())
        .filter(|&c| &sim.data.courses.courses[c].station == id)
        .collect()
}

/// Reiter „Parcours“: Training und Zeitrennen mit Medaillenzeiten und Bestenliste.
fn course_items(sim: &SimState, v: &mut Vec<Item<Act>>) {
    use crate::sim::course::{MEDAL_NAMES, fmt_time};
    use crate::sim::data::CourseKind;
    if let Some(r) = &sim.course {
        let name = &sim.data.courses.courses[r.course].name;
        v.push(
            Item::new(format!("{name} abbrechen"), Act::AbortCourse).detail(if r.started() {
                "Der Lauf ist unterwegs – ohne Wertung beenden".to_string()
            } else {
                "Gewählt – startet am Starttor".to_string()
            }),
        );
    }
    for ci in courses_here(sim) {
        let c = &sim.data.courses.courses[ci];
        let rec = &sim.records[ci];
        let kind = match c.kind {
            CourseKind::Training => "Training",
            CourseKind::Race => "Zeitrennen",
        };
        let right = match (c.kind, rec.entries.first()) {
            (CourseKind::Training, _) if rec.finished > 0 => "✓ geschafft".to_string(),
            (CourseKind::Training, _) if c.prize > 0 => format!("+{} Cr", c.prize),
            (_, Some(best)) => {
                let medal = if rec.medal > 0 {
                    format!(" · {}", MEDAL_NAMES[rec.medal as usize])
                } else {
                    String::new()
                };
                format!("Bestzeit {}{medal}", fmt_time(best.total))
            }
            _ => "noch keine Zeit".to_string(),
        };
        let mut detail = format!("{kind} · {} Schritte · {}", c.steps.len(), c.intro);
        if let Some((g, sv, b)) = c.medals {
            let (pb, ps, pg) = sim.data.courses.medal_prizes;
            detail.push_str(&format!(
                "\nGold {} (+{pg}) · Silber {} (+{ps}) · Bronze {} (+{pb}) – Prämie je Medaille einmal",
                fmt_time(g),
                fmt_time(sv),
                fmt_time(b)
            ));
        }
        if !rec.entries.is_empty() {
            let board: Vec<String> = rec
                .entries
                .iter()
                .take(3)
                .enumerate()
                .map(|(i, e)| {
                    format!(
                        "{}. {} ({}, Crew {})",
                        i + 1,
                        fmt_time(e.total),
                        e.ship,
                        e.crew
                    )
                })
                .collect();
            detail.push_str(&format!("\n{}", board.join("  ·  ")));
        }
        let armed = sim.course.as_ref().is_some_and(|r| r.course == ci);
        let label = if armed {
            format!("▶ {}", c.name)
        } else {
            c.name.clone()
        };
        v.push(
            Item::new(label, Act::Course(ci))
                .right(right)
                .detail(detail)
                .enabled(!armed),
        );
    }
}

/// Reiter „Aufbau“: was die laufende Etappe braucht, was schon da ist, was an Bord ist.
fn project_items(sim: &SimState, v: &mut Vec<Item<Act>>) {
    let Some(p) = sim.project_here() else { return };
    let Some(def) = sim.data.world.stations[p.station].project.as_ref() else {
        return;
    };
    let total = def.stages.len();
    let Some(stage) = def.stages.get(p.stage as usize) else {
        v.push(
            Item::new(format!("{} – abgeschlossen", def.name), Act::Deliver)
                .detail("Alle Etappen fertig. Danke, Crew.")
                .enabled(false),
        );
        return;
    };
    let aboard_parts = sim
        .ship
        .cargo
        .iter()
        .filter(|c| matches!(c.kind, crate::sim::ship::CargoKind::Salvage { .. }))
        .count() as u32;
    let mut useful = false;
    let mut needs = Vec::new();
    for (ore, n) in &stage.needs {
        let have = p.delivered_of(*ore);
        // `+ 0.0` macht aus einer negativen Null eine positive (sonst „-0.0“).
        let aboard = sim.ship.ore_amount(*ore).max(0.0) + 0.0;
        if have + 1e-3 < *n && aboard > 0.01 {
            useful = true;
        }
        needs.push(format!(
            "{} {:.1}/{:.0} t (an Bord {:.1})",
            ore.label(),
            have,
            n,
            aboard
        ));
    }
    if stage.parts > 0 {
        if p.parts < stage.parts && aboard_parts > 0 {
            useful = true;
        }
        needs.push(format!(
            "Bauteile {}/{} (an Bord {aboard_parts})",
            p.parts, stage.parts
        ));
    }
    v.push(
        Item::new(
            format!("Etappe {}/{total}: {}", p.stage + 1, stage.name),
            Act::Deliver,
        )
        .right(format!("+{} Cr", stage.reward))
        .detail(stage.effect.clone())
        .enabled(false),
    );
    v.push(
        Item::new("Material abgeben", Act::Deliver)
            .detail(needs.join(" · "))
            .enabled(useful),
    );
    for (i, s) in def.stages.iter().enumerate() {
        let state = if (i as u8) < p.stage {
            "✓ fertig"
        } else if i as u8 == p.stage {
            "läuft"
        } else {
            "später"
        };
        v.push(
            Item::new(format!("{}. {}", i + 1, s.name), Act::Deliver)
                .right(state)
                .enabled(false),
        );
    }
}

/// Zustand der Triebwerke in einer Zeile, z. B. „Links 40 % stottert · Mitte ausgefallen“.
fn engine_status(sim: &SimState) -> String {
    let n = sim.ship.thrusters.len();
    let bad: Vec<String> = sim
        .ship
        .thrusters
        .iter()
        .enumerate()
        .filter(|(_, t)| t.health < 0.995)
        .map(|(i, t)| {
            let name = crate::sim::thruster_label(i, n);
            if t.failed() {
                format!("{name} ausgefallen")
            } else if t.stuttering() {
                format!("{name} {:.0} % stottert", t.health * 100.0)
            } else {
                format!("{name} {:.0} %", t.health * 100.0)
            }
        })
        .collect();
    if bad.is_empty() {
        "Alle Triebwerke in Ordnung".into()
    } else {
        bad.join(" · ")
    }
}

/// Ist das Stationsmenü gerade sichtbar (angedockt an einem Ort mit Menü)?
pub fn menu_open(sim: &SimState) -> bool {
    sim.ship.docked.is_some() && !sim.ship.destroyed && !tabs_for(sim).is_empty()
}

fn tabs_for(sim: &SimState) -> Vec<Tab> {
    match sim.docked_owner() {
        Some(Owner::Station(si)) => {
            let st = &sim.world.stations[si];
            let mut v = Vec::new();
            if sim.project_here().is_some() {
                v.push(Tab::Project);
            }
            if st.has(Service::Ammo)
                || st.has(Service::Shield)
                || st.has(Service::Repair)
                || st.has(Service::Fuel)
            {
                v.push(Tab::Service);
            }
            if st.has(Service::Ships) {
                v.push(Tab::Ships);
                v.push(Tab::Build);
            }
            if st.has(Service::Upgrades) {
                v.push(Tab::Upgrades);
                v.push(Tab::Paint);
            }
            v.push(Tab::Missions);
            if st.has(Service::Market) {
                v.push(Tab::Market);
            }
            if !courses_here(sim).is_empty() {
                v.push(Tab::Courses);
            }
            // Die Händlerin hat hier angedockt: ihr Sonderangebot.
            if sim.merchant_here() {
                v.push(Tab::Merchant);
            }
            v
        }
        Some(Owner::Planet(_)) if sim.landed_in_zone() => Vec::new(),
        Some(Owner::Planet(_)) => vec![Tab::Market, Tab::Missions],
        None => Vec::new(),
    }
}

fn items_for(sim: &SimState, tab: Tab, sel: &Option<String>) -> Vec<Item<Act>> {
    let mut v: Vec<Item<Act>> = Vec::new();
    let price_or = |p: &Purchase| sim.purchase_info(p);
    match tab {
        Tab::Service => {
            for s in &sim.data.shop.services {
                let p = Purchase::Service(s.id.clone());
                let it = match price_or(&p) {
                    Ok((_, price)) => {
                        Item::new(s.name.clone(), Act::Buy(p)).right(format!("{price} Cr"))
                    }
                    Err(reason) => Item::new(s.name.clone(), Act::Buy(p))
                        .right("—")
                        .detail(reason)
                        .enabled(false),
                };
                let it = match s.effect {
                    ServiceEffect::Ammo(_) => {
                        it.detail(format!("Magazin {}/{}", sim.ship.ammo, sim.ship.max_ammo))
                    }
                    ServiceEffect::ShieldFull => it.detail(format!(
                        "Schild {:.0}/{:.0}",
                        sim.ship.shield, sim.ship.max_shield
                    )),
                    ServiceEffect::RepairFull => it.detail(format!(
                        "Hülle {:.0}/{:.0}",
                        sim.ship.hull, sim.ship.max_hull
                    )),
                    ServiceEffect::RepairThrusters => it.detail(engine_status(sim)),
                    ServiceEffect::Refuel => it.detail(format!(
                        "Tank {:.0}/{:.0}",
                        sim.ship.fuel, sim.ship.max_fuel
                    )),
                };
                v.push(it);
            }
            // Versicherung: Abo, Prämie bei jedem Auftrag, übernimmt einen Teil der Bergung.
            let fd = &sim.data.shop.finance;
            let terms = format!(
                "{:.0} % jeder Auftragsbelohnung (mind. {} Cr), übernimmt {:.0} % der Bergungskosten",
                fd.premium_share * 100.0,
                fd.premium_min,
                fd.coverage * 100.0
            );
            let (label, on) = if sim.crew.insured {
                ("Versicherung kündigen", false)
            } else {
                ("Versicherung abschließen", true)
            };
            v.push(
                Item::new(label, Act::Buy(Purchase::Insurance(on)))
                    .right(if sim.crew.insured { "läuft" } else { "0 Cr" })
                    .detail(if sim.crew.insured {
                        format!("Versichert: {terms}")
                    } else {
                        terms
                    }),
            );
        }
        Tab::Upgrades => {
            // Ausrüstung zuerst: ohne Kran, Kanone, Schild fehlen ganze Spielarten.
            for gd in &sim.data.shop.gear {
                let p = Purchase::Gear(gd.gear);
                let name = format!("Ausrüstung · {}", gd.gear.label());
                let it = if sim.has_gear(gd.gear) {
                    Item::new(name, Act::Buy(p))
                        .right("✓ an Bord")
                        .enabled(false)
                        .detail(gd.description.clone())
                } else {
                    let it = Item::new(name, Act::Buy(p.clone())).right(format!("{} Cr", gd.price));
                    match price_or(&p) {
                        Ok(_) => it.detail(format!(
                            "{} · danach eine freie Taste drücken, um ihn zu übernehmen",
                            gd.description
                        )),
                        Err(reason) => it
                            .detail(format!("{} – {reason}", gd.description))
                            .enabled(false),
                    }
                };
                v.push(it);
            }
            for u in &sim.data.shop.upgrades {
                // Feintuning gibt es nur dort, wo die Person arbeitet.
                let vendor = u
                    .vendor
                    .as_ref()
                    .and_then(|id| sim.data.npcs.iter().find(|n| &n.id == id));
                if let Some(vn) = vendor
                    && !sim.vendor_here(&vn.id)
                {
                    continue;
                }
                let p = Purchase::Upgrade(u.id.clone());
                let owned = sim.crew.upgrades.contains(&u.id);
                let tier = ["", "I", "II", "III"][u.tier.min(3) as usize];
                let name = match vendor {
                    Some(vn) => format!("{} · {} (nur bei {})", u.part.label(), u.name, vn.name),
                    None => format!("{} · {}", u.part.label(), u.name),
                };
                let cost = crate::sim::economy::upgrade_cost(u);
                let info = if u.drawback.is_empty() {
                    format!("Stufe {tier} · {}", u.description)
                } else {
                    format!(
                        "Stufe {tier} · {} · Nachteil: {}",
                        u.description, u.drawback
                    )
                };
                let it = if owned {
                    Item::new(name, Act::Buy(p))
                        .right("✓ eingebaut")
                        .enabled(false)
                        .detail(info)
                } else {
                    let it = Item::new(name, Act::Buy(p.clone())).right(format!("{} Cr", u.price));
                    match price_or(&p) {
                        Ok(_) => it.detail(format!("{info}\nKosten: {}", cost.label())),
                        Err(reason) => it
                            .detail(format!("{info}\nKosten: {} – {reason}", cost.label()))
                            .enabled(false),
                    }
                };
                v.push(it);
            }
        }
        Tab::Ships => {
            let stock: &[String] = match sim.docked_station() {
                Some(si) => &sim.world.stations[si].ships_for_sale,
                None => &[],
            };
            for d in &sim.data.ships {
                let owned = sim.crew.owned_ships.contains(&d.id);
                let current = sim.crew.current_ship == d.id;
                if !owned && !stock.contains(&d.id) {
                    continue;
                }
                let stats = format!(
                    "{} · Crew {}–{} · {} Triebwerke · {} Werkzeuge · Hülle {:.0} · Fracht {:.0} t · Tank {:.0}",
                    d.class,
                    d.crew.0,
                    d.crew.1,
                    d.max_thrusters(),
                    d.tool_parts().count(),
                    d.max_hull,
                    d.cargo_capacity(),
                    d.fuel_capacity
                );
                let it = if current {
                    Item::new(d.name.clone(), Act::Switch(d.id.clone()))
                        .right("im Einsatz")
                        .detail(stats)
                        .enabled(false)
                } else if owned {
                    Item::new(format!("{} wechseln", d.name), Act::Switch(d.id.clone()))
                        .right("gehört euch")
                        .detail(stats)
                } else {
                    let p = Purchase::Ship(d.id.clone());
                    match price_or(&p) {
                        Ok(_) => Item::new(format!("{} kaufen", d.name), Act::Buy(p))
                            .right(format!("{} Cr", fmt_num(d.price)))
                            .detail(format!("{stats} – {}", d.description)),
                        Err(reason) => Item::new(format!("{} kaufen", d.name), Act::Buy(p))
                            .right(format!("{} Cr", fmt_num(d.price)))
                            .detail(format!("{stats} – {reason}"))
                            .enabled(false),
                    }
                };
                v.push(it);
                // Zu teuer? Dann auf Kredit: Anzahlung jetzt, Raten nach jedem Auftrag.
                if !owned && d.price > sim.crew.credits {
                    let p = Purchase::ShipOnCredit(d.id.clone());
                    let (down, rest, rate) = sim.loan_terms(d.price);
                    let fd = &sim.data.shop.finance;
                    let terms = format!(
                        "Anzahlung {} Cr, Rest {} Cr ({:.0} % Zinsen) in {} Raten à {} Cr – je eine nach jedem Auftrag",
                        fmt_num(down),
                        fmt_num(rest),
                        fd.interest * 100.0,
                        fd.installments,
                        fmt_num(rate)
                    );
                    let it = Item::new(format!("{} auf Kredit", d.name), Act::Buy(p.clone()))
                        .right(format!("{} Cr", fmt_num(down)));
                    v.push(match price_or(&p) {
                        Ok(_) => it.detail(terms),
                        Err(reason) => it.detail(format!("{terms} – {reason}")).enabled(false),
                    });
                }
            }
            if let Some(l) = &sim.crew.loan {
                let p = Purchase::RepayLoan;
                let it = Item::new("Kredit tilgen", Act::Buy(p.clone()))
                    .right(format!("{} Cr", fmt_num(l.left)));
                let detail = format!(
                    "{}: noch {} Cr, Rate {} Cr nach jedem Auftrag",
                    sim.data.ship(&l.ship).name,
                    fmt_num(l.left),
                    l.installment
                );
                v.push(match price_or(&p) {
                    Ok(_) => it.detail(detail),
                    Err(reason) => it.detail(format!("{detail} – {reason}")).enabled(false),
                });
            }
            // Was andere Werften führen – damit sich der Weg lohnt.
            for (si, st) in sim.world.stations.iter().enumerate() {
                if Some(si) == sim.docked_station() || st.ships_for_sale.is_empty() {
                    continue;
                }
                let names: Vec<String> = st
                    .ships_for_sale
                    .iter()
                    .filter(|id| !stock.contains(id))
                    .map(|id| sim.data.ship(id).name.clone())
                    .collect();
                if !names.is_empty() {
                    v.push(
                        Item::new(format!("Nur in {}", st.name), Act::Undock)
                            .detail(names.join(", "))
                            .enabled(false),
                    );
                }
            }
        }
        Tab::Project => project_items(sim, &mut v),
        Tab::Courses => course_items(sim, &mut v),
        Tab::Build => build_items(sim, sel, &mut v),
        Tab::Merchant => {
            if let Some(m) = &sim.data.traffic.merchant {
                v.push(
                    Item::new(format!("{} ist mit ihrem Schiff hier", m.name), Act::Undock)
                        .detail(format!(
                            "Sonderposten direkt ins Crew-Lager – nur solange sie angedockt hat. {}",
                            storage_line(sim)
                        ))
                        .enabled(false),
                );
                for (i, g) in m.goods.iter().enumerate() {
                    let p = Purchase::Goods(i);
                    let what = match (g.material, g.parts) {
                        (Some((o, t)), 0) => format!("{t:.0} t {} ins Lager", o.label()),
                        (Some((o, t)), n) => format!("{t:.0} t {} und {n} Bauteile", o.label()),
                        (None, n) => format!("{n} Bauteile ins Lager"),
                    };
                    let it = Item::new(g.name.clone(), Act::Buy(p.clone()))
                        .right(format!("{} Cr", g.price));
                    v.push(match price_or(&p) {
                        Ok(_) => it.detail(what),
                        Err(reason) => it.detail(format!("{what} – {reason}")).enabled(false),
                    });
                }
            }
        }
        Tab::Paint => {
            use crate::render::srgb;
            use crate::sim::data::hex;
            let price = sim.data.shop.paint_price;
            let current = sim.crew.livery(&sim.crew.current_ship);
            let def = sim.data.ship(&sim.crew.current_ship);
            let mut push = |part: PaintPart,
                            choice: Option<usize>,
                            name: String,
                            color: Option<&str>,
                            detail: &str| {
                let now = match part {
                    PaintPart::Hull => current.hull,
                    PaintPart::Accent => current.accent,
                    PaintPart::Flame => current.flame,
                };
                let p = Purchase::Paint { part, choice };
                let label = format!("{} · {name}", part.label());
                let mut it = if now == choice {
                    Item::new(label, Act::Buy(p))
                        .right("✓ aktuell")
                        .enabled(false)
                } else {
                    let ok = sim.purchase_info(&p).is_ok();
                    Item::new(label, Act::Buy(p))
                        .right(format!("{price} Cr"))
                        .enabled(ok)
                };
                it = it.detail(detail);
                if let Some(c) = color {
                    it = it.swatch(srgb(hex(c)));
                }
                v.push(it);
            };
            push(
                PaintPart::Hull,
                None,
                "Werkslack".into(),
                Some(&def.hull_color),
                "Rumpf und Cockpit",
            );
            for (i, c) in sim.data.shop.paints.iter().enumerate() {
                push(
                    PaintPart::Hull,
                    Some(i),
                    c.name.clone(),
                    Some(&c.color),
                    "Rumpf und Cockpit",
                );
            }
            push(
                PaintPart::Accent,
                None,
                "Werkslack".into(),
                Some(&def.accent_color),
                "Streifen, Panzer, Frachtmodule",
            );
            for (i, c) in sim.data.shop.paints.iter().enumerate() {
                push(
                    PaintPart::Accent,
                    Some(i),
                    c.name.clone(),
                    Some(&c.color),
                    "Streifen, Panzer, Frachtmodule",
                );
            }
            for (i, f) in sim.data.shop.flames.iter().enumerate() {
                let choice = f.color.as_ref().map(|_| i);
                let detail = match &f.color {
                    Some(_) => "Außenflamme – der Kern und die Ringe bleiben in Slotfarbe",
                    None => "Jede Flamme in ihrer Slotfarbe – man sieht, wer schiebt",
                };
                push(
                    PaintPart::Flame,
                    choice,
                    f.name.clone(),
                    f.color.as_deref(),
                    detail,
                );
            }
        }
        Tab::Missions => {
            // Was erledigte Aufträge hier gerade bewirken (76).
            if let Some(si) = sim.docked_station() {
                for e in sim.effects.iter().filter(|e| e.station == si) {
                    v.push(
                        Item::new(
                            format!("◆ {}", crate::sim::effects::effect_short(e.kind)),
                            Act::Undock,
                        )
                        .right(format!(
                            "noch {}:{:02}",
                            e.left as u32 / 60,
                            e.left as u32 % 60
                        ))
                        .detail("Wirkung eurer erledigten Aufträge")
                        .enabled(false),
                    );
                }
            }
            for m in &sim.active {
                v.push(
                    Item::new(format!("✓ {}", m.title(sim)), Act::Abandon(m.id))
                        .right("abbrechen")
                        .detail(m.detail(sim)),
                );
            }
            for m in sim.offers_here() {
                let here = sim.docked_owner();
                let has_crane = sim.ship.has_tool(crate::sim::data::ToolKind::Crane);
                let enabled = match &m.kind {
                    MissionKind::Delivery { from, .. } => here == Some(*from),
                    MissionKind::Haul { from, .. } => {
                        here == Some(Owner::Station(*from)) && has_crane
                    }
                    MissionKind::Bulky { mass, .. } => sim.bulky_feasible(*mass).is_ok(),
                    MissionKind::Passengers { from, .. }
                    | MissionKind::Escort { from, .. }
                    | MissionKind::Smuggle { from, .. } => here == Some(Owner::Station(*from)),
                    _ => true,
                } && sim.active.len() < crate::sim::missions::MAX_ACTIVE;
                let detail = match &m.kind {
                    MissionKind::Bulky { mass, .. } => match sim.bulky_feasible(*mass) {
                        Ok(()) => format!(
                            "{} – Form, Masse und Engstellen machen es schwer",
                            m.detail(sim)
                        ),
                        Err(reason) => format!("{} – {reason}", m.detail(sim)),
                    },
                    MissionKind::Passengers { .. } => {
                        "Sanft beschleunigen, nicht anecken – sonst sinkt die Bezahlung".into()
                    }
                    MissionKind::Delivery {
                        mass,
                        from: Owner::Planet(_),
                        ..
                    } => format!("{mass:.1} t Erzladung – verschiebt Masse und Schwerpunkt"),
                    MissionKind::Delivery {
                        mass,
                        traits: crate::sim::data::CargoTrait::None,
                        ..
                    } => {
                        format!("{mass:.1} t Container – landet seitlich im Frachtraum")
                    }
                    MissionKind::Delivery { mass, traits, .. } => {
                        format!("{mass:.1} t {}", traits.label())
                    }
                    MissionKind::Haul { mass, to, .. }
                        if has_crane && sim.world.stations[*to].socket.is_some() =>
                    {
                        format!(
                            "{mass:.0} t Kiste am Kran – am Ziel präzise in die Lastaufnahme setzen und loslassen"
                        )
                    }
                    MissionKind::Haul { mass, .. } if has_crane => format!(
                        "{mass:.0} t Kiste – passt in keinen Frachtraum, am Kran schleppen. Pendelt!"
                    ),
                    MissionKind::Haul { mass, .. } => {
                        format!("{mass:.0} t Kiste – braucht einen belegten Kran")
                    }
                    MissionKind::Mining { .. } => m.detail(sim),
                    MissionKind::Tow { .. } => {
                        "Wrack treibt im All. Mit dem Kran zur Station schleppen.".into()
                    }
                    MissionKind::Capsules { .. } => {
                        "Kapseln einsammeln (Kran oder sanft berühren) und abliefern.".into()
                    }
                    MissionKind::Rescue { crew, .. } => {
                        let need = *crew as f32 * crate::sim::cargo::PERSON_MASS;
                        let free: f32 = (0..sim.ship.pods.len())
                            .map(|i| sim.ship.pod_free(i).max(0.0))
                            .sum();
                        let room = if free + 1e-3 >= need {
                            "Platz reicht".to_string()
                        } else {
                            format!("frei nur {free:.1} t – Fracht umladen oder abwerfen")
                        };
                        format!(
                            "Havariertes Schiff treibt antriebslos. Längsseits gehen, Besatzung übernehmen: {crew} Personen brauchen {need:.1} t Platz ({room})."
                        )
                    }
                    MissionKind::Escort { .. } => {
                        "Frachter legt mit euch ab – unterwegs lauern Piratendrohnen. Abfangen, bevor sie ihn zerlegen."
                            .into()
                    }
                    MissionKind::Smuggle { mass, .. } => format!(
                        "{mass:.1} t heiße Ware im Frachtraum – Zollbojen ({:.0} s Scan) im Bogen umfliegen oder schnell durch. Erwischt: Ware weg, {} Cr Strafe",
                        sim.data.missions.smuggle.scan_time,
                        sim.data.missions.smuggle.fine
                    ),
                    MissionKind::Survey { sites, .. } => {
                        let sd = &sim.data.missions.survey;
                        let names: Vec<&str> = sites
                            .iter()
                            .map(|&i| sim.data.courses.survey_sites[i].name.as_str())
                            .collect();
                        format!(
                            "{} – im Feld {:.0} s stillhalten (unter {:.1} m/s), Daten gehen per Funk raus",
                            names.join(", "),
                            sd.seconds,
                            sd.max_speed
                        )
                    }
                };
                let (lo, hi) = m.crew(sim);
                let par = format!("Richtzeit {}:{:02}", m.par as u32 / 60, m.par as u32 % 60);
                let detail = match m.giver.and_then(|g| sim.data.npcs.get(g)) {
                    Some(n) => format!(
                        "{} ({}) · Crew {lo}–{hi} · {par} · {detail}",
                        n.name, n.role
                    ),
                    None => format!("Crew {lo}–{hi} · {par} · {detail}"),
                };
                v.push(
                    Item::new(m.title(sim), Act::Accept(m.id))
                        .right(format!("+{} Cr", m.reward))
                        .detail(detail)
                        .enabled(enabled),
                );
            }
            if v.is_empty() {
                v.push(Item::new("Keine Aufträge verfügbar", Act::Undock).enabled(false));
            }
        }
        Tab::Market => {
            let here = sim.docked_owner();
            let price =
                |ore| here.map_or(sim.data.shop.ore_price(ore), |o| sim.ore_price_at(o, ore));
            let mut total = 0.0;
            let mut credits = 0;
            for ore in crate::sim::data::Ore::ALL {
                let t = sim.ship.ore_amount(ore);
                if t > 0.01 {
                    total += t;
                    credits += (t * price(ore) as f32).round() as u32;
                }
            }
            let mut parts = 0;
            for c in &sim.ship.cargo {
                if let crate::sim::ship::CargoKind::Salvage { value, .. } = c.kind {
                    credits += value;
                    parts += 1;
                }
            }
            let what = if parts > 0 {
                format!("{total:.1} t Erz/Schrott und {parts} Bauteile an Bord")
            } else {
                format!("{total:.1} t an Bord")
            };
            if let Some(it) = store_item(sim) {
                v.push(it);
            }
            let it = Item::new("Fracht verkaufen", Act::Sell)
                .right(format!("+{credits} Cr"))
                .detail(what)
                .enabled((total > 0.05 || parts > 0) && sim.can_sell_here());
            v.push(it);
            if sim.docked_station().is_some() && sim.charts_unsold > 0 {
                v.push(
                    Item::new("Kartendaten verkaufen", Act::SellCharts)
                        .right(format!("+{} Cr", sim.charts_value()))
                        .detail(format!(
                            "{} neu kartierte Sektorzellen (Scanner)",
                            sim.charts_unsold
                        )),
                );
            }
            // Laufende Nachfrage irgendwo in der Welt.
            if let Some(d) = sim.demand {
                v.push(
                    Item::new(
                        format!(
                            "Nachfrage: {} sucht {}",
                            sim.world.owner_name(d.owner),
                            d.ore.label()
                        ),
                        Act::Sell,
                    )
                    .right(format!("noch {:.0} min", (d.left / 60.0).ceil()))
                    .detail("Der Preis dort steigt, solange die Nachfrage anhält")
                    .enabled(false),
                );
            }
            // Preistafel: hier (mit Trend), und wo es am meisten gibt.
            for ore in crate::sim::data::Ore::ALL {
                let p = price(ore);
                let best = sim.best_ore_price(ore);
                let hint = match best {
                    Some((o, bp)) if bp > p && Some(o) != here => {
                        format!("{} zahlt {bp} Cr/t", sim.world.owner_name(o))
                    }
                    _ => "bester Preis weit und breit".to_string(),
                };
                // In 5-%-Schritten, damit die Liste nicht bei jeder Kleinigkeit neu entsteht.
                let f = here.map_or(1.0, |o| sim.market_factor(o, ore));
                let pct = ((f - 1.0) * 20.0).round() * 5.0;
                let trend = if pct <= -5.0 {
                    format!(" · ▼ {:.0} % (viel verkauft, erholt sich)", -pct)
                } else if pct >= 5.0 {
                    format!(" · ▲ +{pct:.0} % (Nachfrage)")
                } else {
                    String::new()
                };
                v.push(
                    Item::new(format!("Ankauf {}", ore.label()), Act::Sell)
                        .right(format!("{p} Cr/t"))
                        .detail(format!("{hint}{trend}"))
                        .enabled(false),
                );
            }
            for c in &sim.ship.cargo {
                let name = match &c.kind {
                    crate::sim::ship::CargoKind::Ore(o) => o.label().to_string(),
                    crate::sim::ship::CargoKind::Container { name, .. } => {
                        format!("Container: {name}")
                    }
                    crate::sim::ship::CargoKind::Capsule { .. } => "Rettungskapsel".into(),
                    crate::sim::ship::CargoKind::Salvage { name, value } => {
                        format!("Bauteil: {name} ({value} Cr)")
                    }
                    crate::sim::ship::CargoKind::Artifact { id } => {
                        format!("Artefakt: {}", sim.artifact_name(id))
                    }
                    crate::sim::ship::CargoKind::Survivor { .. } => "Gerettete Person".into(),
                    crate::sim::ship::CargoKind::Gear(g) => g.label().to_string(),
                };
                v.push(
                    Item::new(name, Act::Sell)
                        .right(format!("{:.1} t", c.mass))
                        .enabled(false)
                        .detail(format!("Modul {}", sim.ship.pod_label(c.pod))),
                );
            }
        }
    }
    v.push(Item::new("Abdocken", Act::Undock).detail("oder einfach ein Triebwerk zünden"));
    v
}

fn spawn_roots(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(18.0),
            top: Val::Px(240.0),
            bottom: Val::Px(110.0),
            width: Val::Px(470.0),
            ..default()
        },
        StationRoot,
        Signature(0),
        GlobalZIndex(10),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(150.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-430.0)),
            width: Val::Px(500.0),
            ..default()
        },
        VoteRoot,
        Signature(0),
        GlobalZIndex(30),
    ));
}

fn despawn_roots(
    mut commands: Commands,
    q: Query<Entity, Or<(With<StationRoot>, With<VoteRoot>)>>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn voter_for(device: Option<Device>, crew: &Crew) -> u8 {
    let d = device.unwrap_or(Device::Keyboard);
    crew.players.iter().position(|p| p.device == d).unwrap_or(0) as u8
}

#[allow(clippy::too_many_arguments)]
fn station_menu(
    mut commands: Commands,
    sim: Res<Sim>,
    input: Res<MenuInput>,
    paused: Res<Paused>,
    map: Res<MapOpen>,
    crew: Res<Crew>,
    buttons: Query<(&Interaction, &ItemButton), Changed<Interaction>>,
    mut tab: ResMut<StationTab>,
    mut focus: ResMut<MenuFocus>,
    mut pending: ResMut<PendingCommands>,
    mut root: Query<(Entity, &mut Signature), With<StationRoot>>,
    tab_buttons: Query<(&Interaction, &TabButton), Changed<Interaction>>,
    mut last_owner: Local<Option<Owner>>,
    mut scroll: ResMut<super::MenuScroll>,
    mut view: ResMut<BuildView>,
    mut toasts: ResMut<super::Toasts>,
) {
    let Ok((root, mut sig)) = root.single_mut() else {
        return;
    };
    let s = &sim.0;
    let tabs = tabs_for(s);
    // Andere Station, andere Reiter: dort beim ersten anfangen.
    let owner = s.docked_owner();
    if owner.is_some() && owner != *last_owner {
        *last_owner = owner;
        tab.0 = 0;
        focus.0[MENU_STATION] = 0;
    }
    let visible = s.ship.docked.is_some() && !paused.0 && !map.0 && !s.ship.destroyed;
    if !visible || tabs.is_empty() {
        if sig.0 != 0 {
            sig.0 = 0;
            commands.entity(root).despawn_children();
        }
        return;
    }
    if input.left {
        tab.0 = (tab.0 + tabs.len() - 1) % tabs.len();
        focus.0[MENU_STATION] = 0;
    }
    if input.right {
        tab.0 = (tab.0 + 1) % tabs.len();
        focus.0[MENU_STATION] = 0;
    }
    for (i, tb) in &tab_buttons {
        if *i == Interaction::Pressed {
            tab.0 = tb.0;
            focus.0[MENU_STATION] = 0;
        }
    }
    // Neuer Reiter oder neue Station: Liste beginnt oben.
    if focus.0[MENU_STATION] == 0 {
        scroll.0[MENU_STATION] = 0.0;
    }
    tab.0 = tab.0.min(tabs.len() - 1);
    let current = tabs[tab.0];
    view.open = current == Tab::Build;
    if !view.open {
        view.selected = None;
    }
    let items = items_for(s, current, &view.selected);

    let mut f = focus.0[MENU_STATION];
    // Bestätigen nur per Enter/Start (Gesichtstasten könnten Triebwerke sein).
    let mut activate = if navigate(
        &mut f,
        items.len(),
        &MenuInput {
            confirm: input.enter,
            ..input.clone()
        },
    ) {
        Some(f)
    } else {
        None
    };
    if let Some(i) = mouse_pick(MENU_STATION, &mut f, &buttons) {
        activate = Some(i);
    }
    focus.0[MENU_STATION] = f.min(items.len().saturating_sub(1));
    if let Some(i) = activate
        && let Some(it) = items.get(i).filter(|it| it.enabled)
    {
        let voter = voter_for(input.device, &crew);
        match it.action.clone() {
            Some(Act::Buy(p)) => pending.0.push(Command::Buy { purchase: p, voter }),
            Some(Act::Accept(id)) => pending.0.push(Command::AcceptMission { id }),
            Some(Act::Abandon(id)) => pending.0.push(Command::AbandonMission { id }),
            Some(Act::Sell) => pending.0.push(Command::SellOre),
            Some(Act::SellCharts) => pending.0.push(Command::SellCharts),
            Some(Act::Deliver) => pending.0.push(Command::DeliverProject),
            Some(Act::Switch(id)) => pending.0.push(Command::SwitchShip { id }),
            Some(Act::Undock) => pending.0.push(Command::Undock),
            Some(Act::Course(course)) => pending.0.push(Command::StartCourse { course }),
            Some(Act::AbortCourse) => pending.0.push(Command::AbortCourse),
            Some(Act::SelectMount(m)) => {
                view.selected = m;
                focus.0[MENU_STATION] = 0;
            }
            Some(Act::Store) => pending.0.push(Command::StoreCargo),
            Some(Act::Export) => {
                let msg = export_layout(s);
                toasts.push(msg, crate::sim::ToastKind::Info);
            }
            None => {}
        }
    }

    // Neu zeichnen, wenn sich etwas geändert hat.
    let title = match s.docked_owner() {
        Some(Owner::Station(si)) => {
            let st = &s.world.stations[si];
            let kind = match st.kind {
                StationKind::Station => "Raumstation",
                StationKind::Shipyard => "Raumwerft",
                StationKind::Outpost => "Außenposten",
            };
            (st.name.clone(), kind.to_string())
        }
        Some(Owner::Planet(pi)) => (
            format!("{} – Außenposten", s.world.planets[pi].name),
            "Erzannahme".to_string(),
        ),
        None => (String::new(), String::new()),
    };
    let key = format!(
        "{:?}|{}|{:?}|{}",
        title,
        tab.0,
        items
            .iter()
            .map(|i| format!("{}{}{}{}", i.label, i.right, i.detail, i.enabled))
            .collect::<Vec<_>>(),
        s.crew.credits
    );
    let h = super::sig_of(&key);
    if sig.0 == h {
        return;
    }
    sig.0 = h;
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|r| {
        r.spawn(panel(Val::Percent(100.0))).with_children(|p| {
            p.spawn(text(title.0.to_uppercase(), 22.0, TEXT));
            p.spawn(text(
                format!("{} · Kasse {} Credits", title.1, fmt_num(s.crew.credits)),
                14.0,
                ACCENT,
            ));
            if let Some(si) = s.docked_station() {
                p.spawn(text(reputation_line(s, si), 13.0, MUTED));
            }
            // Werkstatt: was im Crew-Lager liegt und wie das Schiff gerade dasteht.
            if matches!(current, Tab::Build | Tab::Upgrades) {
                p.spawn(text(storage_line(s), 12.0, TEAL));
            }
            if current == Tab::Build {
                let sh = &s.ship;
                p.spawn(text(
                    format!(
                        "{:.1} t · Schwerpunkt {:+.2}/{:+.2} · Trägheit {:.0} · Hülle {:.0} · Fracht {:.0} t",
                        sh.mass,
                        sh.com.x + 0.0,
                        sh.com.y + 0.0,
                        sh.inertia,
                        sh.max_hull,
                        sh.cargo_capacity()
                    ),
                    12.0,
                    MUTED,
                ));
            }
            if current == Tab::Missions
                && let Some(owner) = s.docked_owner()
            {
                let npcs = s.npcs_at(owner);
                if !npcs.is_empty() {
                    p.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        margin: UiRect::vertical(Val::Px(4.0)),
                        ..default()
                    })
                    .with_children(|col| {
                        for &n in &npcs {
                            spawn_npc_row(col, s, n);
                        }
                    });
                }
            }
            p.spawn(Node {
                column_gap: Val::Px(6.0),
                margin: UiRect::vertical(Val::Px(6.0)),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            })
            .with_children(|row| {
                for (i, t) in tabs.iter().enumerate() {
                    let on = i == tab.0;
                    row.spawn((
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(if on {
                            TEAL.with_alpha(0.35)
                        } else {
                            Color::srgba(1.0, 1.0, 1.0, 0.05)
                        }),
                        TabButton(i),
                    ))
                    .with_children(|b| {
                        b.spawn(text(t.label(), 15.0, if on { TEXT } else { MUTED }));
                    });
                }
            });
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(5.0),
                    overflow: Overflow::scroll_y(),
                    flex_grow: 1.0,
                    ..default()
                },
                ScrollPosition(Vec2::new(0.0, scroll.0[MENU_STATION])),
                super::ScrollList(MENU_STATION),
            ))
            .with_children(|list| spawn_items(list, MENU_STATION, &items));
            p.spawn(text(
                "←→ Reiter · ↑↓ Auswahl · Enter/Start kaufen · Maus geht auch",
                12.0,
                MUTED,
            ));
        });
    });
}

#[derive(Component)]
struct TabButton(usize);

/// „Ruf: Bekannt ★★☆ · 4/7“ für eine Station.
fn reputation_line(s: &SimState, si: usize) -> String {
    use crate::sim::missions::{REP_LEVELS, REP_NAMES};
    let pts = s.crew.reputation.get(si).copied().unwrap_or(0);
    let lvl = s.rep_level_at(si) as usize;
    let stars: String = (1..REP_LEVELS.len())
        .map(|i| if i <= lvl { '★' } else { '☆' })
        .collect();
    let next = REP_LEVELS
        .get(lvl + 1)
        .map(|n| format!(" · {pts}/{n} bis {}", REP_NAMES[lvl + 1]))
        .unwrap_or_else(|| " · höchste Stufe".into());
    let perk = if lvl > 0 {
        format!(" · {} % Rabatt, +{} Aufträge", lvl * 5, lvl)
    } else {
        String::new()
    };
    let fee = s.dock_fee_at(si);
    let fee = if fee > 0 {
        format!(" · Dockgebühr {fee} Cr")
    } else {
        " · keine Dockgebühr".to_string()
    };
    format!("Ruf: {} {stars}{next}{perk}{fee}", REP_NAMES[lvl])
}

/// Porträt aus einfachen Formen (Schultern, Kopf, Haare oder Helm).
pub fn spawn_portrait(p: &mut ChildSpawnerCommands, look: &crate::sim::data::Look, size: f32) {
    use crate::render::srgb;
    use crate::sim::data::hex;
    let c = |h: &str| srgb(hex(h));
    let abs = |left: f32, top: f32, w: f32, h: f32| Node {
        position_type: PositionType::Absolute,
        left: Val::Px(left * size),
        top: Val::Px(top * size),
        width: Val::Px(w * size),
        height: Val::Px(h * size),
        ..default()
    };
    p.spawn((
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(8.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundGradient::from(LinearGradient::to_bottom(vec![
            ColorStop::auto(Color::srgb(0.16, 0.2, 0.28)),
            ColorStop::auto(Color::srgb(0.06, 0.07, 0.1)),
        ])),
    ))
    .with_children(|f| {
        // Schultern
        f.spawn((
            Node {
                border_radius: BorderRadius::top(Val::Px(size * 0.3)),
                ..abs(0.08, 0.7, 0.84, 0.4)
            },
            BackgroundColor(c(&look.suit)),
        ));
        // Hals
        f.spawn((abs(0.42, 0.56, 0.16, 0.16), BackgroundColor(c(&look.skin))));
        if look.helmet {
            f.spawn((
                Node {
                    border: UiRect::all(Val::Px(size * 0.05)),
                    border_radius: BorderRadius::all(Val::Percent(50.0)),
                    ..abs(0.2, 0.1, 0.6, 0.6)
                },
                BorderColor::all(Color::srgb(0.75, 0.78, 0.8)),
                BackgroundColor(Color::srgba(0.6, 0.75, 0.85, 0.12)),
            ));
        }
        // Kopf
        f.spawn((
            Node {
                border_radius: BorderRadius::all(Val::Percent(50.0)),
                ..abs(0.29, 0.18, 0.42, 0.46)
            },
            BackgroundColor(c(&look.skin)),
        ));
        // Haare
        f.spawn((
            Node {
                border_radius: BorderRadius::top(Val::Px(size * 0.22)),
                ..abs(0.28, 0.15, 0.44, 0.16)
            },
            BackgroundColor(c(&look.hair)),
        ));
        // Augen
        for x in [0.39, 0.55] {
            f.spawn((
                Node {
                    border_radius: BorderRadius::all(Val::Percent(50.0)),
                    ..abs(x, 0.38, 0.06, 0.06)
                },
                BackgroundColor(Color::srgb(0.08, 0.08, 0.1)),
            ));
        }
    });
}

fn spawn_npc_row(p: &mut ChildSpawnerCommands, s: &SimState, n: usize) {
    let npc = &s.data.npcs[n];
    // Ein Spruch pro Besuch (wechselt mit jedem erledigten Auftrag).
    let line = &npc.lines[(s.crew.missions_done as usize + n) % npc.lines.len().max(1)];
    p.spawn(Node {
        column_gap: Val::Px(10.0),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|row| {
        spawn_portrait(row, &npc.look, 52.0);
        row.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 1.0,
            ..default()
        })
        .with_children(|t| {
            t.spawn(text(format!("{} · {}", npc.name, npc.role), 14.0, TEXT));
            t.spawn(text(format!("„{line}“"), 12.0, MUTED));
        });
    });
}

/// Während einer Abstimmung: eigene Slot-Taste schaltet Ja ↔ Nein.
#[allow(clippy::too_many_arguments)]
fn vote_input(
    sim: Res<Sim>,
    active: Res<ActiveBindings>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    mut pending: ResMut<PendingCommands>,
    buttons: Query<(&Interaction, &VoteButton), Changed<Interaction>>,
) {
    let Some(v) = &sim.0.vote else { return };
    let mut toggled: Vec<u8> = Vec::new();
    for b in &active.0 {
        if btn_just_pressed(&b.btn, &keys, &mouse, &pads) && !toggled.contains(&(b.player as u8)) {
            toggled.push(b.player as u8);
        }
    }
    for p in toggled {
        let cur = v.votes.get(p as usize).copied().unwrap_or(0);
        pending.0.push(Command::Vote {
            voter: p,
            yes: cur != 1,
        });
    }
    for (i, vb) in &buttons {
        if *i == Interaction::Pressed {
            pending.0.push(Command::Vote {
                voter: vb.voter,
                yes: vb.yes,
            });
        }
    }
}

#[derive(Component)]
struct VoteButton {
    voter: u8,
    yes: bool,
}

fn draw_vote(
    mut commands: Commands,
    sim: Res<Sim>,
    crew: Res<Crew>,
    mut root: Query<(Entity, &mut Signature), With<VoteRoot>>,
) {
    let Ok((root, mut sig)) = root.single_mut() else {
        return;
    };
    let s = &sim.0;
    let Some(v) = &s.vote else {
        if sig.0 != 0 {
            sig.0 = 0;
            commands.entity(root).despawn_children();
        }
        return;
    };
    let key = format!("{:?}|{}|{}", v.votes, v.timer.ceil(), v.label);
    let h = super::sig_of(&key);
    if sig.0 == h {
        return;
    }
    sig.0 = h;
    commands.entity(root).despawn_children();
    let (yes, no) = v.tally();
    let after = s.crew.credits.saturating_sub(v.price);
    commands.entity(root).with_children(|r| {
        r.spawn(panel(Val::Percent(100.0))).with_children(|p| {
            p.spawn(text(format!("ABSTIMMUNG · angestoßen von Spieler {}", v.initiator + 1), 14.0, ACCENT));
            p.spawn(text(v.label.clone(), 22.0, TEXT));
            p.spawn(text(
                format!("Preis {} Cr · Kasse jetzt {} → danach {}", fmt_num(v.price), fmt_num(s.crew.credits), fmt_num(after)),
                15.0,
                MUTED,
            ));
            p.spawn(Node {
                column_gap: Val::Px(8.0),
                flex_wrap: FlexWrap::Wrap,
                margin: UiRect::vertical(Val::Px(6.0)),
                ..default()
            })
            .with_children(|row| {
                for (i, vote) in v.votes.iter().enumerate() {
                    let (mark, c) = match vote {
                        1 => ("JA", GOOD),
                        -1 => ("NEIN", BAD),
                        _ => ("…", MUTED),
                    };
                    let who = crew.players.get(i).map(|p| p.label.clone()).unwrap_or_default();
                    row.spawn((
                        Button,
                        Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BorderColor::all(c),
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04)),
                        VoteButton {
                            voter: i as u8,
                            yes: *vote != 1,
                        },
                    ))
                    .with_children(|b| {
                        b.spawn(text(format!("Spieler {} {}", i + 1, mark), 16.0, c));
                        b.spawn(text(who, 11.0, MUTED));
                    });
                }
            });
            let frac = (v.timer / VOTE_SECONDS).clamp(0.0, 1.0);
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(6.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
            ))
            .with_children(|b| {
                b.spawn((
                    Node {
                        width: Val::Percent(frac * 100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(WARN),
                ));
            });
            p.spawn(text(
                format!("Ja {yes} : {no} Nein · eigene Slot-Taste drücken = Ja/Nein umschalten · Gleichstand = Nein"),
                13.0,
                MUTED,
            ));
        });
    });
}
