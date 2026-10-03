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
use crate::sim::economy::{Purchase, VOTE_SECONDS};
use crate::sim::missions::MissionKind;
use crate::sim::world::Owner;
use crate::sim::{Command, SimState};

pub struct StationPlugin;

impl Plugin for StationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StationTab>()
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
    Missions,
    Market,
    Ships,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Service => "Service",
            Tab::Upgrades => "Upgrades",
            Tab::Missions => "Aufträge",
            Tab::Market => "Markt",
            Tab::Ships => "Werft",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Buy(Purchase),
    Accept(u32),
    Abandon(u32),
    Sell,
    Switch(String),
    Undock,
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
            if st.has(Service::Ammo) || st.has(Service::Shield) || st.has(Service::Repair) {
                v.push(Tab::Service);
            }
            if st.has(Service::Ships) {
                v.push(Tab::Ships);
            }
            if st.has(Service::Upgrades) {
                v.push(Tab::Upgrades);
            }
            v.push(Tab::Missions);
            if st.has(Service::Market) {
                v.push(Tab::Market);
            }
            v
        }
        Some(Owner::Planet(_)) => vec![Tab::Market, Tab::Missions],
        None => Vec::new(),
    }
}

fn items_for(sim: &SimState, tab: Tab) -> Vec<Item<Act>> {
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
                };
                v.push(it);
            }
        }
        Tab::Upgrades => {
            for u in &sim.data.shop.upgrades {
                let p = Purchase::Upgrade(u.id.clone());
                let owned = sim.crew.upgrades.contains(&u.id);
                let it = if owned {
                    Item::new(u.name.clone(), Act::Buy(p))
                        .right("✓ eingebaut")
                        .enabled(false)
                        .detail(u.description.clone())
                } else {
                    match price_or(&p) {
                        Ok(_) => Item::new(u.name.clone(), Act::Buy(p))
                            .right(format!("{} Cr", u.price))
                            .detail(u.description.clone()),
                        Err(reason) => Item::new(u.name.clone(), Act::Buy(p))
                            .right(format!("{} Cr", u.price))
                            .detail(format!("{} – {}", u.description, reason))
                            .enabled(false),
                    }
                };
                v.push(it);
            }
        }
        Tab::Ships => {
            for d in &sim.data.ships {
                let owned = sim.crew.owned_ships.contains(&d.id);
                let current = sim.crew.current_ship == d.id;
                let stats = format!(
                    "{} · {} Triebwerke · {} Werkzeuge · Hülle {:.0} · Fracht {:.0} t",
                    d.class,
                    d.max_thrusters(),
                    d.tool_parts().count(),
                    d.max_hull,
                    d.cargo_capacity()
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
                            .detail(stats),
                        Err(reason) => Item::new(format!("{} kaufen", d.name), Act::Buy(p))
                            .right(format!("{} Cr", fmt_num(d.price)))
                            .detail(format!("{stats} – {reason}"))
                            .enabled(false),
                    }
                };
                v.push(it);
            }
        }
        Tab::Missions => {
            for m in &sim.active {
                v.push(
                    Item::new(format!("✓ {}", m.title(sim)), Act::Abandon(m.id))
                        .right("abbrechen")
                        .detail(m.detail(sim)),
                );
            }
            for m in sim.offers_here() {
                let here = sim.docked_station();
                let enabled = match &m.kind {
                    MissionKind::Delivery { from, .. } => here == Some(*from),
                    _ => true,
                } && sim.active.len() < crate::sim::missions::MAX_ACTIVE;
                let detail = match &m.kind {
                    MissionKind::Delivery { mass, .. } => {
                        format!("{mass:.1} t Container – landet seitlich im Frachtraum")
                    }
                    MissionKind::Mining { .. } => m.detail(sim),
                    MissionKind::Tow { .. } => {
                        "Wrack treibt im All. Mit dem Kran zur Station schleppen.".into()
                    }
                    MissionKind::Capsules { .. } => {
                        "Kapseln einsammeln (Kran oder sanft berühren) und abliefern.".into()
                    }
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
            let mut total = 0.0;
            let mut credits = 0;
            for ore in crate::sim::data::Ore::ALL {
                let t = sim.ship.ore_amount(ore);
                if t > 0.01 {
                    total += t;
                    credits += (t * sim.data.shop.ore_price(ore) as f32).round() as u32;
                }
            }
            let it = Item::new("Erz verkaufen", Act::Sell)
                .right(format!("+{credits} Cr"))
                .detail(format!(
                    "{total:.1} t an Bord · Preise: {}",
                    sim.data
                        .shop
                        .ore_prices
                        .iter()
                        .map(|(o, p)| format!("{} {}", o.label(), p))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .enabled(total > 0.05 && sim.can_sell_here());
            v.push(it);
            for c in &sim.ship.cargo {
                let name = match &c.kind {
                    crate::sim::ship::CargoKind::Ore(o) => o.label().to_string(),
                    crate::sim::ship::CargoKind::Container { name, .. } => {
                        format!("Container: {name}")
                    }
                    crate::sim::ship::CargoKind::Capsule { .. } => "Rettungskapsel".into(),
                };
                v.push(
                    Item::new(name, Act::Sell)
                        .right(format!("{:.1} t", c.mass))
                        .enabled(false)
                        .detail(format!("Modul {}", c.pod + 1)),
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
) {
    let Ok((root, mut sig)) = root.single_mut() else {
        return;
    };
    let s = &sim.0;
    let tabs = tabs_for(s);
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
    tab.0 = tab.0.min(tabs.len() - 1);
    let current = tabs[tab.0];
    let items = items_for(s, current);

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
            Some(Act::Switch(id)) => pending.0.push(Command::SwitchShip { id }),
            Some(Act::Undock) => pending.0.push(Command::Undock),
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
                ScrollPosition::default(),
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
