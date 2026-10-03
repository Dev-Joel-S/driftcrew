//! Tests für Phase 9: Dockgebühr, Versicherung, Schiffskredit, schwankende Preise und die
//! Abrechnung am Auftragsende.

use std::sync::Arc;

use super::data::{CrewSave, GameData, Ore};
use super::economy::Purchase;
use super::missions::{Mission, MissionKind};
use super::ship::{CargoKind, Loadout};
use super::world::Owner;
use super::{Command, DT, SimState, TickInput};

fn sim() -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, &save, Loadout::full(&def), 1)
}

fn st(s: &SimState, id: &str) -> usize {
    s.data.station_index(id).unwrap()
}

/// An einer Station andocken wie im Spiel (mit Gebühr und Auftragsabschluss).
fn dock_at(s: &mut SimState, si: usize) {
    let pad = s.world.stations[si]
        .pads
        .iter()
        .copied()
        .find(|p| s.world.pads[*p].enabled)
        .unwrap();
    s.ship.docked = None;
    s.dock(pad);
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

/// Lieferauftrag, der beim Andocken an `to` erfüllt ist.
fn delivery(s: &mut SimState, to: usize, reward: u32) -> u32 {
    let id = s.next_id();
    s.active.push(Mission {
        id,
        kind: MissionKind::Delivery {
            from: Owner::Station(0),
            to,
            cargo: "Test".into(),
            mass: 1.0,
        },
        reward,
        origin: Some(Owner::Station(0)),
        giver: None,
        start: Some(Box::new(s.stats.clone())),
        top_speed: 0.0,
        // Keine Boni, damit die Rechnung übersichtlich bleibt: Richtzeit schon vorbei und
        // ein Kratzer (kein Sauberkeitsbonus).
        par: -1.0,
    });
    s.stats.damage += 5.0;
    id
}

#[test]
fn dock_fee_depends_on_station_reputation_and_grace() {
    let mut s = sim();
    let kepler = st(&s, "kepler");
    let base = s.data.shop.finance.dock_fee as f32 * s.world.stations[kepler].prices.dock;
    assert_eq!(s.dock_fee_at(kepler), base.round() as u32);
    let c0 = s.crew.credits;
    dock_at(&mut s, kepler);
    assert_eq!(s.crew.credits, c0 - s.dock_fee_at(kepler));
    // Gleich wieder andocken: kostenlos.
    let c1 = s.crew.credits;
    dock_at(&mut s, kepler);
    assert_eq!(s.crew.credits, c1);
    // Später wieder: zahlen. Mit Ruf billiger.
    s.time += s.data.shop.finance.dock_grace + 1.0;
    s.crew.reputation[kepler] = 3;
    let fee = s.dock_fee_at(kepler);
    assert_eq!(fee, (base * 0.8).round() as u32);
    dock_at(&mut s, kepler);
    assert_eq!(s.crew.credits, c1 - fee);
    assert_eq!(s.stats.expenses.dock, c0 - c1 + fee);
    // Relais Ost und Planeten-Außenposten nehmen nichts.
    let relais = st(&s, "relais");
    assert_eq!(s.dock_fee_at(relais), 0);
}

#[test]
fn insurance_costs_a_premium_and_covers_salvage() {
    let mut s = sim();
    s.crew.credits = 1000;
    let gross = s.salvage_fee_gross();
    assert_eq!(s.salvage_fee_now(), gross);
    buy(&mut s, Purchase::Insurance(true));
    assert!(s.crew.insured);
    let cov = s.data.shop.finance.coverage;
    assert_eq!(
        s.salvage_fee_now(),
        (gross as f32 * (1.0 - cov)).round() as u32
    );

    // Prämie geht von der Belohnung ab.
    let kepler = st(&s, "kepler");
    delivery(&mut s, kepler, 400);
    let before = s.crew.credits;
    dock_at(&mut s, kepler);
    let r = s.report.clone().unwrap();
    let premium = s.insurance_premium(400);
    assert_eq!(r.premium, premium);
    assert_eq!(
        s.crew.credits,
        before + 400 - premium - s.dock_fee_at(kepler)
    );

    // Zerstörung: die Versicherung übernimmt den größten Teil.
    let before = s.crew.credits;
    let expect = s.salvage_fee_now();
    s.ship.docked = None;
    s.ship.hull = 0.0;
    s.step(&TickInput::default());
    assert!(s.ship.destroyed);
    assert_eq!(s.crew.credits, before - expect);

    // Kündigen geht jederzeit.
    s.ship.destroyed = false;
    dock_at(&mut s, kepler);
    buy(&mut s, Purchase::Insurance(false));
    assert!(!s.crew.insured);
}

#[test]
fn ship_on_credit_is_paid_off_in_installments() {
    let mut s = sim();
    let orion = st(&s, "werft");
    s.crew.credits = 800;
    s.ship.docked = None;
    s.dock_at_station(orion);
    // Bar zu teuer, auf Kredit machbar.
    assert!(s.purchase_info(&Purchase::Ship("lastesel".into())).is_err());
    let price = s.data.ship("lastesel").price;
    let (down, rest, rate) = s.loan_terms(price);
    assert!(rest > price - down, "Zinsen auf den Rest");
    buy(&mut s, Purchase::ShipOnCredit("lastesel".into()));
    assert_eq!(s.crew.current_ship, "lastesel");
    assert_eq!(s.crew.credits, 800 - down);
    let loan = s.crew.loan.clone().unwrap();
    assert_eq!((loan.left, loan.installment), (rest, rate));
    // Ein zweiter Kredit geht nicht.
    assert!(
        s.purchase_info(&Purchase::ShipOnCredit("kolibri".into()))
            .is_err()
    );

    // Nach jedem Auftrag eine Rate.
    let kepler = st(&s, "kepler");
    delivery(&mut s, kepler, 500);
    dock_at(&mut s, kepler);
    let r = s.report.clone().unwrap();
    assert_eq!(r.installment, rate);
    assert_eq!(s.crew.loan.as_ref().unwrap().left, rest - rate);
    assert_eq!(r.paid_in(), 500 - rate);

    // Rest auf einmal tilgen (nur in der Werft).
    let left = s.crew.loan.as_ref().unwrap().left;
    s.crew.credits = left + 50;
    assert!(s.purchase_info(&Purchase::RepayLoan).is_err());
    s.ship.docked = None;
    s.dock_at_station(orion);
    buy(&mut s, Purchase::RepayLoan);
    assert!(s.crew.loan.is_none());
    assert_eq!(s.crew.credits, 50);
}

#[test]
fn installment_never_exceeds_earnings_and_loan_ends() {
    let mut s = sim();
    s.crew.loan = Some(super::finance::Loan {
        ship: "kolibri".into(),
        left: 150,
        installment: 100,
    });
    let kepler = st(&s, "kepler");
    // Kleine Belohnung: die Rate nimmt höchstens, was verdient wurde.
    delivery(&mut s, kepler, 60);
    dock_at(&mut s, kepler);
    assert_eq!(s.report.as_ref().unwrap().installment, 60);
    assert_eq!(s.crew.loan.as_ref().unwrap().left, 90);
    s.time += 1000.0;
    delivery(&mut s, kepler, 300);
    dock_at(&mut s, kepler);
    assert_eq!(s.report.as_ref().unwrap().installment, 90);
    assert!(s.crew.loan.is_none());
}

#[test]
fn selling_lowers_prices_that_recover_and_demand_raises_them() {
    let mut s = sim();
    let kepler = st(&s, "kepler");
    let at = Owner::Station(kepler);
    let p0 = s.ore_price_at(at, Ore::Kobalt);
    s.ship.store(CargoKind::Ore(Ore::Kobalt), 10.0);
    s.ship.docked = None;
    s.dock_at_station(kepler);
    s.sell_ore();
    let f = s.market_factor(at, Ore::Kobalt);
    assert!(
        f < 0.8,
        "ein voller Laderaum drückt den Preis deutlich ({f})"
    );
    assert!(s.ore_price_at(at, Ore::Kobalt) < p0);
    // Andere Orte und Sorten bleiben unberührt.
    assert_eq!(s.market_factor(Owner::Station(0), Ore::Kobalt), 1.0);
    assert_eq!(s.market_factor(at, Ore::Ionit), 1.0);
    // Erholt sich mit der Zeit.
    s.demand_timer = 1e9;
    for _ in 0..(400.0 / DT) as usize {
        s.update_market();
    }
    assert!(s.market_factor(at, Ore::Kobalt) > 0.95);

    // Nachfrage: der Preis steigt dort.
    s.demand = Some(super::finance::Demand {
        owner: at,
        ore: Ore::Ionit,
        left: 300.0,
    });
    for _ in 0..(60.0 / DT) as usize {
        s.update_market();
    }
    assert!(s.market_factor(at, Ore::Ionit) > 1.25);

    // Alles bleibt im Spielstand.
    s.crew.insured = true;
    s.crew.loan = Some(super::finance::Loan {
        ship: "kolibri".into(),
        left: 400,
        installment: 80,
    });
    let save = s.to_save();
    let data = s.data.clone();
    let def = data.ship(&save.current_ship).clone();
    let s2 = SimState::new(data, &save, Loadout::full(&def), 1);
    assert!((s2.market_factor(at, Ore::Ionit) - s.market_factor(at, Ore::Ionit)).abs() < 1e-4);
    assert_eq!(s2.demand.map(|d| (d.owner, d.ore)), Some((at, Ore::Ionit)));
    assert!(s2.crew.insured);
    assert_eq!(s2.crew.loan, s.crew.loan);
}

#[test]
fn demand_comes_by_itself_deterministically() {
    let mut a = sim();
    let mut b = sim();
    a.demand_timer = 0.0;
    b.demand_timer = 0.0;
    a.update_market();
    b.update_market();
    assert!(a.demand.is_some());
    assert_eq!(a.demand, b.demand);
    let d = a.demand.unwrap();
    assert_ne!(d.ore, Ore::Schrott);
}

#[test]
fn report_lists_costs_along_the_way() {
    let mut s = sim();
    s.crew.credits = 1000;
    let kepler = st(&s, "kepler");
    let nova = st(&s, "nova");
    delivery(&mut s, kepler, 500);
    // Unterwegs: Andocken an Nova (Gebühr) und Reparatur.
    s.time += 1000.0;
    dock_at(&mut s, nova);
    s.ship.hull = s.ship.max_hull * 0.5;
    buy(&mut s, Purchase::Service("repair".into()));
    let repair = 1000 - s.crew.credits - s.dock_fee_at(nova);
    assert!(repair > 0);
    s.time += 1000.0;
    dock_at(&mut s, kepler);
    let r = s.report.clone().unwrap();
    assert_eq!(
        r.stats.expenses.dock,
        s.dock_fee_at(nova) + s.dock_fee_at(kepler)
    );
    assert_eq!(r.stats.expenses.service, repair);
    assert_eq!(r.earned(), 500);
    assert_eq!(
        r.profit(),
        500 - (r.stats.expenses.dock + r.stats.expenses.service) as i64
    );
}
