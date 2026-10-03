//! Gemeinsame Kasse, Käufe und das Abstimmungssystem der Crew.

use super::data::{GameData, Ore, Prices, Service, ServiceEffect, UpgradeEffect};
use super::ship::{Loadout, ShipStats};
use super::world::Owner;
use super::{Command, DT, SimEvent, SimState, TickInput, ToastKind};

pub const VOTE_SECONDS: f32 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Purchase {
    Service(String),
    Upgrade(String),
    Ship(String),
    /// Lackierung des aktuellen Schiffs ändern (`None` = zurück zum Werkslack).
    Paint {
        part: PaintPart,
        choice: Option<usize>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintPart {
    Hull,
    Accent,
    Flame,
}

impl PaintPart {
    pub fn label(self) -> &'static str {
        match self {
            PaintPart::Hull => "Rumpf",
            PaintPart::Accent => "Akzent",
            PaintPart::Flame => "Flammen",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Vote {
    pub purchase: Purchase,
    pub label: String,
    pub price: u32,
    /// 0 = enthält sich, 1 = ja, -1 = nein (pro Crewmitglied).
    pub votes: Vec<i8>,
    pub timer: f32,
    pub initiator: u8,
}

impl Vote {
    pub fn tally(&self) -> (usize, usize) {
        let yes = self.votes.iter().filter(|v| **v > 0).count();
        let no = self.votes.iter().filter(|v| **v < 0).count();
        (yes, no)
    }
}

pub fn stats_for(data: &GameData, upgrades: &[String]) -> ShipStats {
    let mut s = ShipStats::default();
    for id in upgrades {
        let Some(u) = data.upgrade(id) else { continue };
        match u.effect {
            UpgradeEffect::ThrustMul(m) => s.thrust_mul *= m,
            UpgradeEffect::MaxHull(h) => s.hull_bonus += h,
            UpgradeEffect::MaxShield(h) => s.shield_bonus += h,
            UpgradeEffect::Cargo(c) => s.cargo_mul += c,
            UpgradeEffect::Gyro(g) => s.gyro += g,
            UpgradeEffect::MaxAmmo(a) => s.ammo_bonus += a,
            UpgradeEffect::CraneRange(m) => s.crane_mul *= m,
            UpgradeEffect::DrillRate(m) => s.drill_mul *= m,
            UpgradeEffect::FuelTank(f) => s.fuel_mul += f,
            UpgradeEffect::ScanRange(m) => s.scan_mul *= m,
        }
    }
    s
}

impl SimState {
    pub fn docked_owner(&self) -> Option<Owner> {
        self.ship.docked.map(|p| self.world.pads[p].owner)
    }

    pub fn docked_station(&self) -> Option<usize> {
        self.docked_owner().and_then(Owner::station)
    }

    /// Gelandet auf einer freien Landezone (kein Menü, Werkzeuge aktiv)?
    pub fn landed_in_zone(&self) -> bool {
        self.ship.docked.is_some_and(|p| self.world.pads[p].zone)
    }

    pub fn can_sell_here(&self) -> bool {
        if self.landed_in_zone() {
            return false;
        }
        match self.docked_owner() {
            Some(Owner::Station(i)) => self.world.stations[i].has(Service::Market),
            Some(Owner::Planet(_)) => true,
            None => false,
        }
    }

    /// Preisfaktoren eines Ortes.
    pub fn prices_of(&self, owner: Owner) -> &Prices {
        match owner {
            Owner::Station(i) => &self.world.stations[i].prices,
            Owner::Planet(i) => &self.world.planets[i].prices,
        }
    }

    /// Ankaufspreis für eine Tonne Erz an einem Ort.
    pub fn ore_price_at(&self, owner: Owner, ore: Ore) -> u32 {
        let base = self.data.shop.ore_price(ore) as f32;
        (base * self.prices_of(owner).ore_factor(ore))
            .round()
            .max(1.0) as u32
    }

    /// Bester Ankaufspreis für eine Erzsorte irgendwo in der Welt (Ort, Preis).
    pub fn best_ore_price(&self, ore: Ore) -> Option<(Owner, u32)> {
        let stations = (0..self.world.stations.len())
            .filter(|&i| self.world.stations[i].has(Service::Market))
            .map(Owner::Station);
        let planets = (0..self.world.planets.len())
            .filter(|&i| self.world.planets[i].pad.is_some())
            .map(Owner::Planet);
        stations
            .chain(planets)
            .map(|o| (o, self.ore_price_at(o, ore)))
            .max_by_key(|(_, p)| *p)
    }

    /// Preis für einen Service (Reparatur, Schild und Tanken anteilig), mit Ortsfaktor.
    fn service_price(&self, effect: &ServiceEffect, base: u32) -> u32 {
        let frac = match effect {
            ServiceEffect::RepairFull => 1.0 - self.ship.hull / self.ship.max_hull,
            ServiceEffect::ShieldFull => 1.0 - self.ship.shield / self.ship.max_shield.max(1.0),
            ServiceEffect::Ammo(_) => 1.0,
            ServiceEffect::RepairThrusters => self.ship.thruster_wear(),
            ServiceEffect::Refuel => 1.0 - self.ship.fuel / self.ship.max_fuel.max(1.0),
        };
        let factor = self.docked_owner().map_or(1.0, |o| {
            let p = self.prices_of(o);
            let base = if *effect == ServiceEffect::Refuel {
                p.fuel
            } else {
                p.service
            };
            // Stammkunden zahlen weniger: 5 % pro Rufstufe.
            let rep = o.station().map_or(0, |si| self.rep_level_at(si));
            base * (1.0 - 0.05 * rep as f32)
        });
        ((base as f32 * frac * factor).ceil() as u32).max(5)
    }

    /// Prüft einen Kauf. Ok((Bezeichnung, Preis)) oder Err(Grund).
    pub fn purchase_info(&self, p: &Purchase) -> Result<(String, u32), String> {
        let Some(si) = self.docked_station() else {
            return Err("Nur angedockt an einer Station".into());
        };
        let st = &self.world.stations[si];
        let (label, price) = match p {
            Purchase::Service(id) => {
                let item = self
                    .data
                    .shop
                    .services
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or("Unbekannt")?;
                let (needed, ok) = match item.effect {
                    ServiceEffect::Ammo(_) => (Service::Ammo, self.ship.ammo < self.ship.max_ammo),
                    ServiceEffect::ShieldFull => (
                        Service::Shield,
                        self.ship.shield < self.ship.max_shield - 0.5,
                    ),
                    ServiceEffect::RepairFull => {
                        (Service::Repair, self.ship.hull < self.ship.max_hull - 0.5)
                    }
                    ServiceEffect::RepairThrusters => {
                        (Service::Repair, self.ship.thruster_wear() > 0.005)
                    }
                    ServiceEffect::Refuel => {
                        (Service::Fuel, self.ship.fuel < self.ship.max_fuel - 0.5)
                    }
                };
                if !st.has(needed) {
                    return Err(format!("{} bietet das nicht an", st.name));
                }
                if !ok {
                    return Err("Bereits voll".into());
                }
                if matches!(item.effect, ServiceEffect::Ammo(_))
                    && !self.ship.has_tool(super::data::ToolKind::Cannon)
                {
                    return Err("Keine Kanone belegt".into());
                }
                (
                    item.name.clone(),
                    self.service_price(&item.effect, item.price),
                )
            }
            Purchase::Upgrade(id) => {
                if !st.has(Service::Upgrades) {
                    return Err(format!("{} hat keine Upgrade-Werkstatt", st.name));
                }
                let u = self.data.upgrade(id).ok_or("Unbekannt")?;
                if self.crew.upgrades.contains(id) {
                    return Err("Bereits eingebaut".into());
                }
                if let Some(req) = &u.requires
                    && !self.crew.upgrades.contains(req)
                {
                    let name = self
                        .data
                        .upgrade(req)
                        .map(|r| r.name.clone())
                        .unwrap_or_default();
                    return Err(format!("Benötigt {name}"));
                }
                (u.name.clone(), u.price)
            }
            Purchase::Ship(id) => {
                if !st.has(Service::Ships) {
                    return Err("Schiffe gibt es nur in Werften".into());
                }
                if self.crew.owned_ships.contains(id) {
                    return Err("Gehört der Crew bereits".into());
                }
                if !st.ships_for_sale.contains(id) {
                    return Err(format!("{} führt dieses Schiff nicht", st.name));
                }
                let def = self
                    .data
                    .ships
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or("Unbekannt")?;
                (format!("Schiff: {}", def.name), def.price)
            }
            Purchase::Paint { part, choice } => {
                if !st.has(Service::Upgrades) {
                    return Err(format!("{} hat keine Lackiererei", st.name));
                }
                let current = self.crew.livery(&self.crew.current_ship);
                let now = match part {
                    PaintPart::Hull => current.hull,
                    PaintPart::Accent => current.accent,
                    PaintPart::Flame => current.flame,
                };
                if now == *choice {
                    return Err("Ist schon so lackiert".into());
                }
                let name = match (part, choice) {
                    (_, None) => "Werkslack".to_string(),
                    (PaintPart::Flame, Some(i)) => self
                        .data
                        .shop
                        .flames
                        .get(*i)
                        .ok_or("Unbekannt")?
                        .name
                        .clone(),
                    (_, Some(i)) => self
                        .data
                        .shop
                        .paints
                        .get(*i)
                        .ok_or("Unbekannt")?
                        .name
                        .clone(),
                };
                (
                    format!("{}: {name}", part.label()),
                    self.data.shop.paint_price,
                )
            }
        };
        if price > self.crew.credits {
            return Err(format!("Zu wenig Credits ({price})"));
        }
        Ok((label, price))
    }

    pub(crate) fn handle_commands(&mut self, cmds: &[Command]) {
        for c in cmds {
            match c {
                Command::Undock => self.undock(),
                Command::Buy { purchase, voter } => self.request_purchase(purchase.clone(), *voter),
                Command::Vote { voter, yes } => {
                    if let Some(v) = &mut self.vote
                        && let Some(slot) = v.votes.get_mut(*voter as usize)
                    {
                        *slot = if *yes { 1 } else { -1 };
                    }
                }
                Command::AcceptMission { id } => self.accept_mission(*id),
                Command::AbandonMission { id } => self.abandon_mission(*id),
                Command::SellOre => self.sell_ore(),
                Command::SwitchShip { id } => self.switch_ship(id),
                Command::Ping { player, pos } => self.add_ping(*player, *pos),
                Command::SellCharts => self.sell_charts(),
                Command::SetLoadout {
                    thrusters,
                    tools,
                    crew_size,
                } => {
                    let def = self.data.ship(&self.crew.current_ship);
                    let tools: Vec<usize> = tools
                        .iter()
                        .copied()
                        .filter(|t| *t < def.tool_parts().count())
                        .collect();
                    let thrusters = (*thrusters).min(def.max_thrusters());
                    self.rebuild_ship(super::ship::Loadout { thrusters, tools });
                    self.set_crew_size(*crew_size);
                }
            }
        }
    }

    fn request_purchase(&mut self, p: Purchase, voter: u8) {
        if self.vote.is_some() {
            self.toast("Es läuft bereits eine Abstimmung", ToastKind::Warn);
            return;
        }
        match self.purchase_info(&p) {
            Err(reason) => self.toast(reason, ToastKind::Warn),
            Ok((label, price)) => {
                if self.crew.size <= 1 {
                    self.execute_purchase(&p);
                } else {
                    let mut votes = vec![0i8; self.crew.size as usize];
                    if let Some(v) = votes.get_mut(voter as usize) {
                        *v = 1;
                    }
                    self.vote = Some(Vote {
                        purchase: p,
                        label,
                        price,
                        votes,
                        timer: VOTE_SECONDS,
                        initiator: voter,
                    });
                    self.events.push(SimEvent::VoteStarted);
                }
            }
        }
    }

    pub(crate) fn update_vote(&mut self, _input: &TickInput) {
        let Some(v) = &mut self.vote else { return };
        v.timer -= DT;
        let all_voted = v.votes.iter().all(|x| *x != 0);
        if v.timer > 0.0 && !all_voted {
            return;
        }
        let v = self.vote.take().unwrap();
        let (yes, no) = v.tally();
        // Mehrheit entscheidet, Gleichstand bedeutet nein.
        let passed = yes > no;
        self.events.push(SimEvent::VoteEnded {
            passed,
            label: v.label.clone(),
        });
        if passed {
            self.execute_purchase(&v.purchase);
        } else {
            self.toast(
                format!("Abgelehnt: {} ({yes}:{no})", v.label),
                ToastKind::Warn,
            );
        }
    }

    fn execute_purchase(&mut self, p: &Purchase) {
        let (label, price) = match self.purchase_info(p) {
            Ok(x) => x,
            Err(reason) => {
                self.toast(reason, ToastKind::Warn);
                return;
            }
        };
        self.crew.credits -= price;
        match p {
            Purchase::Service(id) => {
                let effect = self
                    .data
                    .shop
                    .services
                    .iter()
                    .find(|s| &s.id == id)
                    .map(|s| s.effect.clone());
                match effect {
                    Some(ServiceEffect::Ammo(n)) => {
                        self.ship.ammo = (self.ship.ammo + n).min(self.ship.max_ammo)
                    }
                    Some(ServiceEffect::ShieldFull) => self.ship.shield = self.ship.max_shield,
                    Some(ServiceEffect::RepairFull) => self.ship.hull = self.ship.max_hull,
                    Some(ServiceEffect::RepairThrusters) => {
                        for t in &mut self.ship.thrusters {
                            t.health = 1.0;
                        }
                    }
                    Some(ServiceEffect::Refuel) => {
                        self.ship.fuel = self.ship.max_fuel;
                        self.fuel_warned = 0;
                    }
                    None => {}
                }
            }
            Purchase::Upgrade(id) => {
                self.crew.upgrades.push(id.clone());
                let (old_max_hull, old_max_shield, old_max_fuel) =
                    (self.ship.max_hull, self.ship.max_shield, self.ship.max_fuel);
                let loadout = self.loadout.clone();
                self.rebuild_ship(loadout);
                self.ship.hull += (self.ship.max_hull - old_max_hull).max(0.0);
                self.ship.shield += (self.ship.max_shield - old_max_shield).max(0.0);
                self.ship.fuel += (self.ship.max_fuel - old_max_fuel).max(0.0);
            }
            Purchase::Ship(id) => {
                self.crew.owned_ships.push(id.clone());
                self.switch_ship(id);
            }
            Purchase::Paint { part, choice } => {
                let ship = self.crew.current_ship.clone();
                let mut l = self.crew.livery(&ship);
                match part {
                    PaintPart::Hull => l.hull = *choice,
                    PaintPart::Accent => l.accent = *choice,
                    PaintPart::Flame => l.flame = *choice,
                }
                self.crew.liveries.retain(|(s, _)| *s != ship);
                self.crew.liveries.push((ship, l));
            }
        }
        self.events.push(SimEvent::Purchased {
            name: label.clone(),
        });
        self.toast(format!("Gekauft: {label}  (-{price})"), ToastKind::Good);
    }

    fn switch_ship(&mut self, id: &str) {
        if !self.crew.owned_ships.iter().any(|s| s == id) || self.crew.current_ship == id {
            return;
        }
        let Some(si) = self.docked_station() else {
            return;
        };
        if !self.world.stations[si].has(Service::Ships) {
            self.toast("Schiffswechsel nur in Werften", ToastKind::Warn);
            return;
        }
        self.crew.current_ship = id.to_string();
        let def = self.data.ship(id).clone();
        // Fracht bleibt in der Werft nicht liegen – sie wird umgeladen, soweit Platz ist.
        let pad = self.ship.docked;
        let stats = stats_for(&self.data, &self.crew.upgrades);
        let cargo = std::mem::take(&mut self.ship.cargo);
        self.ship = super::ship::Ship::build(&def, &Loadout::full(&def), &stats);
        self.loadout = Loadout::full(&def);
        for item in cargo {
            self.ship.store(item.kind, item.mass);
        }
        if let Some(pad) = pad {
            let p = self.world.pads[pad].clone();
            self.ship.angle = p.ship_angle();
            self.ship.pos = p.center + p.normal * (self.ship.rest_height() + 0.02);
            self.ship.prev_pos = self.ship.pos;
            self.ship.prev_angle = self.ship.angle;
            self.ship.docked = Some(pad);
        }
        self.events.push(SimEvent::ShipChanged);
        self.toast(
            format!("Neues Schiff: {} – Slots neu verteilen", def.name),
            ToastKind::Good,
        );
    }

    /// Erz an Station oder Planeten-Außenposten abgeben. Was aktive Abbau-Aufträge
    /// brauchen, bleibt an Bord.
    pub(crate) fn sell_ore(&mut self) {
        if !self.can_sell_here() {
            self.toast("Hier gibt es keinen Markt", ToastKind::Warn);
            return;
        }
        let mut earned = 0u32;
        let mut sold_t = 0.0;
        for ore in super::data::Ore::ALL {
            let reserved: f32 = self
                .active
                .iter()
                .filter_map(|m| match m.kind {
                    super::missions::MissionKind::Mining { ore: o, amount, .. } if o == ore => {
                        Some(amount)
                    }
                    _ => None,
                })
                .sum();
            let sellable = (self.ship.ore_amount(ore) - reserved).max(0.0);
            if sellable < 0.05 {
                continue;
            }
            let price = self
                .docked_owner()
                .map_or(self.data.shop.ore_price(ore), |o| self.ore_price_at(o, ore));
            let taken = self.ship.take_ore(ore, sellable);
            sold_t += taken;
            earned += (taken * price as f32).round() as u32;
        }
        // Bauteile aus Wracks gehen zum festen Wert weg.
        let parts = self
            .ship
            .remove_cargo(|k| matches!(k, super::ship::CargoKind::Salvage { .. }));
        for p in &parts {
            if let super::ship::CargoKind::Salvage { value, .. } = p.kind {
                earned += value;
            }
        }
        if earned == 0 {
            self.toast("Nichts Verkaufbares an Bord", ToastKind::Info);
            return;
        }
        self.crew.credits += earned;
        self.crew.ore_sold += sold_t;
        self.events.push(SimEvent::Sold { credits: earned });
        let what = match (sold_t > 0.05, parts.len()) {
            (true, 0) => format!("{sold_t:.1} t Erz"),
            (false, n) => format!("{n} Bauteile"),
            (true, n) => format!("{sold_t:.1} t Erz und {n} Bauteile"),
        };
        self.toast(
            format!("{what} verkauft  +{earned} Credits"),
            ToastKind::Good,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::data::CrewSave;
    use crate::sim::ship::Loadout;
    use std::sync::Arc;

    fn sim(crew: u8) -> SimState {
        let data = Arc::new(GameData::embedded().unwrap());
        let mut save = CrewSave::new_game(&data);
        save.credits = 5000;
        let def = data.ship(&save.current_ship).clone();
        SimState::new(data, &save, Loadout::full(&def), crew)
    }

    fn buy(s: &mut SimState, p: Purchase, voter: u8) {
        s.step(&TickInput {
            commands: vec![Command::Buy { purchase: p, voter }],
            ..Default::default()
        });
    }

    #[test]
    fn solo_buys_directly() {
        let mut s = sim(1);
        let before = s.crew.credits;
        buy(&mut s, Purchase::Upgrade("thrust1".into()), 0);
        assert!(s.crew.upgrades.contains(&"thrust1".to_string()));
        assert!(s.crew.credits < before);
    }

    #[test]
    fn tie_means_no() {
        let mut s = sim(2);
        buy(&mut s, Purchase::Upgrade("armor1".into()), 0);
        assert!(s.vote.is_some());
        s.step(&TickInput {
            commands: vec![Command::Vote {
                voter: 1,
                yes: false,
            }],
            ..Default::default()
        });
        // Alle haben abgestimmt → 1:1 → abgelehnt.
        assert!(s.vote.is_none());
        assert!(!s.crew.upgrades.contains(&"armor1".to_string()));
    }

    #[test]
    fn majority_buys_and_silence_abstains() {
        let mut s = sim(3);
        buy(&mut s, Purchase::Upgrade("armor1".into()), 0);
        s.step(&TickInput {
            commands: vec![Command::Vote {
                voter: 2,
                yes: true,
            }],
            ..Default::default()
        });
        for _ in 0..(VOTE_SECONDS * 60.0) as usize + 5 {
            s.step(&TickInput::default());
        }
        assert!(s.vote.is_none());
        assert!(s.crew.upgrades.contains(&"armor1".to_string()));
    }

    #[test]
    fn ship_purchase_only_at_shipyard() {
        let mut s = sim(1);
        assert!(s.purchase_info(&Purchase::Ship("kolibri".into())).is_err());
        let yard = s.data.station_index("werft").unwrap();
        s.ship.docked = None;
        s.dock_at_station(yard);
        assert!(s.purchase_info(&Purchase::Ship("kolibri".into())).is_ok());
        // Jede Werft hat ihr eigenes Angebot.
        assert!(s.purchase_info(&Purchase::Ship("hornisse".into())).is_err());
        buy(&mut s, Purchase::Ship("kolibri".into()), 0);
        assert_eq!(s.crew.current_ship, "kolibri");
        assert!(s.events.contains(&SimEvent::ShipChanged));
        let vega = s.data.station_index("vega").unwrap();
        s.ship.docked = None;
        s.dock_at_station(vega);
        assert!(s.purchase_info(&Purchase::Ship("hornisse".into())).is_ok());
    }

    #[test]
    fn ore_prices_differ_between_stations() {
        let mut s = sim(1);
        let nova = Owner::Station(s.data.station_index("nova").unwrap());
        let kepler = Owner::Station(s.data.station_index("kepler").unwrap());
        assert_ne!(
            s.ore_price_at(nova, Ore::Solarit),
            s.ore_price_at(kepler, Ore::Solarit)
        );
        // Verkauf nutzt den Ortspreis.
        s.ship
            .store(crate::sim::ship::CargoKind::Ore(Ore::Solarit), 4.0);
        s.ship.docked = None;
        s.dock_at_station(kepler.station().unwrap());
        let before = s.crew.credits;
        s.sell_ore();
        let expected = (4.0 * s.ore_price_at(kepler, Ore::Solarit) as f32).round() as u32;
        assert_eq!(s.crew.credits - before, expected);
    }

    #[test]
    fn refuel_and_thruster_repair_cost_money() {
        let mut s = sim(1);
        s.ship.fuel = 10.0;
        s.ship.thrusters[0].health = 0.0;
        let before = s.crew.credits;
        buy(&mut s, Purchase::Service("fuel".into()), 0);
        buy(&mut s, Purchase::Service("engines".into()), 0);
        assert_eq!(s.ship.fuel, s.ship.max_fuel);
        assert!(s.ship.thrusters.iter().all(|t| t.health == 1.0));
        assert!(s.crew.credits < before);
    }
}
