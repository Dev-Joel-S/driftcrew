//! Finanzen (Punkte 47–49): Dockgebühren, Versicherung, Schiffskredit, schwankende Marktpreise
//! und was davon in die Abrechnung am Auftragsende gehört. Alles deterministisch in der
//! Simulation und im Spielstand.

use super::data::{CrewSave, DemandSave, LoanSave, Ore, Service};
use super::world::Owner;
use super::{DT, SimState, ToastKind};

/// Laufender Schiffskredit: Restschuld (mit Zinsen) und Rate nach jedem Auftrag.
#[derive(Clone, Debug, PartialEq)]
pub struct Loan {
    pub ship: String,
    pub left: u32,
    pub installment: u32,
}

/// Nachfrage: ein Ort zahlt eine Weile deutlich mehr für ein Erz.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Demand {
    pub owner: Owner,
    pub ore: Ore,
    pub left: f32,
}

/// Was die Crew unterwegs ausgegeben hat (laufende Summen; für einen Auftrag die Differenz).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Expenses {
    pub dock: u32,
    pub service: u32,
    pub salvage: u32,
}

impl Expenses {
    pub fn minus(&self, o: &Expenses) -> Expenses {
        Expenses {
            dock: self.dock.saturating_sub(o.dock),
            service: self.service.saturating_sub(o.service),
            salvage: self.salvage.saturating_sub(o.salvage),
        }
    }
    pub fn total(&self) -> u32 {
        self.dock + self.service + self.salvage
    }
}

fn ore_index(ore: Ore) -> usize {
    Ore::ALL.iter().position(|o| *o == ore).unwrap_or(0)
}

impl SimState {
    /// Platz eines Ortes in der Markttabelle: erst Stationen, dann Planeten.
    pub fn market_slot(&self, owner: Owner) -> usize {
        match owner {
            Owner::Station(i) => i,
            Owner::Planet(i) => self.world.stations.len() + i,
        }
    }

    fn owner_of_slot(&self, slot: usize) -> Owner {
        let n = self.world.stations.len();
        if slot < n {
            Owner::Station(slot)
        } else {
            Owner::Planet(slot - n)
        }
    }

    fn owner_id(&self, owner: Owner) -> String {
        match owner {
            Owner::Station(i) => self.world.stations[i].id.clone(),
            Owner::Planet(i) => self.data.world.planets[i].id.clone(),
        }
    }

    fn owner_by_id(&self, id: &str) -> Option<Owner> {
        self.data.station_index(id).map(Owner::Station).or_else(|| {
            self.data
                .world
                .planets
                .iter()
                .position(|p| p.id == id)
                .map(Owner::Planet)
        })
    }

    /// Orte mit Erzannahme (Stationen mit Markt, Planeten mit Außenposten).
    pub fn market_owners(&self) -> Vec<Owner> {
        let stations = (0..self.world.stations.len())
            .filter(|&i| self.world.stations[i].has(Service::Market))
            .map(Owner::Station);
        let planets = (0..self.world.planets.len())
            .filter(|&i| self.world.planets[i].pad.is_some())
            .map(Owner::Planet);
        stations.chain(planets).collect()
    }

    /// Angebot und Nachfrage: aktueller Faktor auf den Ankaufspreis (1.0 = normal).
    pub fn market_factor(&self, owner: Owner, ore: Ore) -> f32 {
        self.market
            .get(self.market_slot(owner))
            .map_or(1.0, |m| m[ore_index(ore)])
    }

    /// Verkauf drückt den Preis an diesem Ort.
    pub(crate) fn note_sale(&mut self, owner: Owner, ore: Ore, tons: f32) {
        let fd = &self.data.shop.finance;
        let (drop, floor) = (fd.drop_per_t, fd.floor);
        let slot = self.market_slot(owner);
        if let Some(m) = self.market.get_mut(slot) {
            let f = &mut m[ore_index(ore)];
            *f = (*f - drop * tons).max(floor);
        }
    }

    /// Jeden Tick: Preise erholen sich, Nachfragen kommen und gehen.
    pub(crate) fn update_market(&mut self) {
        let fd = self.data.shop.finance.clone();
        if let Some(d) = &mut self.demand {
            d.left -= DT;
            if d.left <= 0.0 {
                self.demand = None;
            }
        }
        self.demand_timer -= DT;
        if self.demand_timer <= 0.0 {
            self.demand_timer = self.rng.range(fd.demand_interval.0, fd.demand_interval.1);
            if self.demand.is_none() {
                self.start_demand(fd.demand_seconds, fd.demand_factor);
            }
        }
        let demand = self.demand;
        for slot in 0..self.market.len() {
            let owner = self.owner_of_slot(slot);
            for (k, ore) in Ore::ALL.iter().enumerate() {
                let hot = demand.is_some_and(|d| d.owner == owner && d.ore == *ore);
                let target = if hot { fd.demand_factor } else { 1.0 };
                let f = self.market[slot][k];
                let tau = if hot && f < target {
                    fd.demand_rise
                } else {
                    fd.recovery
                };
                self.market[slot][k] = f + (target - f) * (DT / tau.max(DT));
            }
        }
    }

    fn start_demand(&mut self, seconds: f32, factor: f32) {
        let owners = self.market_owners();
        if owners.is_empty() {
            return;
        }
        let owner = owners[self.rng.index(owners.len())];
        // Schrott ist nie gefragt, und nur Erze, die es in der Welt auch gibt.
        let ores: Vec<Ore> = Ore::ALL
            .iter()
            .copied()
            .filter(|o| {
                *o != Ore::Schrott
                    && (self.world.planets.iter().any(|p| p.ore == *o)
                        || self.data.world.asteroid_fields.iter().any(|f| f.ore == *o))
            })
            .collect();
        if ores.is_empty() {
            return;
        }
        let ore = ores[self.rng.index(ores.len())];
        self.demand = Some(Demand {
            owner,
            ore,
            left: seconds,
        });
        let name = self.world.owner_name(owner).to_string();
        self.toast(
            format!(
                "Nachfrage: {name} zahlt bald +{:.0} % für {}",
                (factor - 1.0) * 100.0,
                ore.label()
            ),
            ToastKind::Info,
        );
    }

    /// Dockgebühr an einer Station (Ruf senkt sie um 20 % pro Stufe).
    pub fn dock_fee_at(&self, si: usize) -> u32 {
        let fd = &self.data.shop.finance;
        let rep = self.rep_level_at(si) as f32;
        let f = fd.dock_fee as f32 * self.world.stations[si].prices.dock * (1.0 - 0.2 * rep);
        f.round().max(0.0) as u32
    }

    /// Beim Andocken: Gebühr abbuchen (nicht noch einmal, wer gerade erst hier war).
    pub(crate) fn charge_dock_fee(&mut self, si: usize) {
        let grace = self.data.shop.finance.dock_grace;
        if let Some((last, t)) = self.last_dock_fee
            && last == si
            && self.time - t < grace
        {
            return;
        }
        let fee = self.dock_fee_at(si).min(self.crew.credits);
        if fee == 0 {
            return;
        }
        self.crew.credits -= fee;
        self.stats.expenses.dock += fee;
        self.last_dock_fee = Some((si, self.time));
        self.toast(format!("Dockgebühr -{fee} Credits"), ToastKind::Info);
    }

    /// Versicherungsprämie für eine Auftragsbelohnung.
    pub fn insurance_premium(&self, base: u32) -> u32 {
        if !self.crew.insured {
            return 0;
        }
        let fd = &self.data.shop.finance;
        ((base as f32 * fd.premium_share).round() as u32).max(fd.premium_min)
    }

    /// Kreditbedingungen für einen Schiffspreis: (Anzahlung, Restschuld mit Zinsen, Rate).
    pub fn loan_terms(&self, price: u32) -> (u32, u32, u32) {
        let fd = &self.data.shop.finance;
        let down = (price as f32 * fd.down_payment).ceil() as u32;
        let rest = ((price - down.min(price)) as f32 * (1.0 + fd.interest)).ceil() as u32;
        let rate = rest.div_ceil(fd.installments.max(1));
        (down, rest, rate)
    }

    /// Am Auftragsende: Prämie und Kreditrate von den Einnahmen abziehen.
    /// Ergebnis: (Prämie, Rate) – beides höchstens so viel, wie verdient wurde.
    pub(crate) fn settle_mission(&mut self, earned: u32) -> (u32, u32) {
        let premium = self.insurance_premium(earned).min(earned);
        let mut rate = 0;
        let mut paid_off = None;
        if let Some(l) = &mut self.crew.loan {
            rate = l.installment.min(l.left).min(earned - premium);
            l.left -= rate;
            if l.left == 0 {
                paid_off = Some(l.ship.clone());
            }
        }
        if let Some(ship) = paid_off {
            self.crew.loan = None;
            let name = self.data.ship(&ship).name.clone();
            self.toast(format!("Kredit für den {name} abbezahlt"), ToastKind::Good);
        }
        (premium, rate)
    }

    pub(crate) fn load_finance(&mut self, save: &CrewSave) {
        self.crew.insured = save.insured;
        self.crew.loan = save.loan.as_ref().map(|l| Loan {
            ship: l.ship.clone(),
            left: l.left,
            installment: l.installment,
        });
        let n = self.world.stations.len() + self.world.planets.len();
        self.market = vec![[1.0; 5]; n];
        for (id, factors) in &save.market {
            if let Some(owner) = self.owner_by_id(id) {
                let slot = self.market_slot(owner);
                for (k, f) in factors.iter().enumerate().take(5) {
                    self.market[slot][k] = *f;
                }
            }
        }
        self.demand = save.demand.as_ref().and_then(|d| {
            self.owner_by_id(&d.at).map(|owner| Demand {
                owner,
                ore: d.ore,
                left: d.left,
            })
        });
        let fd = &self.data.shop.finance;
        self.demand_timer = fd.demand_interval.0;
    }

    pub(crate) fn save_finance(&self, save: &mut CrewSave) {
        save.insured = self.crew.insured;
        save.loan = self.crew.loan.as_ref().map(|l| LoanSave {
            ship: l.ship.clone(),
            left: l.left,
            installment: l.installment,
        });
        save.market = self
            .market
            .iter()
            .enumerate()
            .filter(|(_, m)| m.iter().any(|f| (f - 1.0).abs() > 0.005))
            .map(|(slot, m)| (self.owner_id(self.owner_of_slot(slot)), m.to_vec()))
            .collect();
        save.demand = self.demand.map(|d| DemandSave {
            at: self.owner_id(d.owner),
            ore: d.ore,
            left: d.left,
        });
    }
}
