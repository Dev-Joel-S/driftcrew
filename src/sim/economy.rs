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
    /// Schiff mit Anzahlung kaufen, der Rest wird nach jedem Auftrag in Raten abgezahlt.
    ShipOnCredit(String),
    /// Restschuld auf einmal tilgen.
    RepayLoan,
    /// Versicherung abschließen (true) oder kündigen (false).
    Insurance(bool),
    /// Modul an einem Bauplatz des aktuellen Schiffs anbauen bzw. abbauen.
    Module {
        mount: String,
        module: String,
    },
    RemoveModule {
        mount: String,
    },
    /// Sonderangebot der Händlerin (Index in `traffic.ron`, merchant.goods) – nur, wenn ihr
    /// Schiff an derselben Station angedockt hat.
    Goods(usize),
    /// Ausrüstung in der Werft: Kran, Kanone, Schildgenerator.
    Gear(super::data::Gear),
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

/// Kosten eines Moduls.
pub fn module_cost(m: &super::data::ModuleDef) -> super::workshop::Cost {
    super::workshop::Cost {
        credits: m.credits,
        materials: m.materials.clone(),
        parts: m.parts,
        key: None,
    }
}

/// Kosten eines Upgrades.
pub fn upgrade_cost(u: &super::data::UpgradeDef) -> super::workshop::Cost {
    super::workshop::Cost {
        credits: u.price,
        materials: u.materials.clone(),
        parts: u.parts,
        // Altes Feld `artifact`: irgendein Artefakt als Schlüssel.
        key: u
            .key
            .clone()
            .or_else(|| u.artifact.then(|| "*".to_string())),
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
            UpgradeEffect::RepairDrones(r) => s.hull_regen += r,
            UpgradeEffect::FuelBurn(m) => s.fuel_burn_mul *= m,
            UpgradeEffect::CannonRate(m) => s.cannon_rate *= m,
            UpgradeEffect::CannonDamage(m) => s.cannon_damage *= m,
            UpgradeEffect::CraneLoad(m) => s.crane_load *= m,
            UpgradeEffect::Brake(m) => s.brake_mul *= m,
        }
        if u.mass > 0.0 {
            s.part_mass.push((u.part, u.mass));
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

    /// Ankaufspreis für eine Tonne Erz an einem Ort (mit Angebot und Nachfrage).
    pub fn ore_price_at(&self, owner: Owner, ore: Ore) -> u32 {
        let base = self.data.shop.ore_price(ore) as f32;
        (base * self.prices_of(owner).ore_factor(ore) * self.market_factor(owner, ore))
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
        // Versorgte Station (76): Munition und Reparatur gratis.
        if self.service_free_here()
            && matches!(
                effect,
                ServiceEffect::Ammo(_) | ServiceEffect::RepairFull | ServiceEffect::RepairThrusters
            )
        {
            return 0;
        }
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
                // Ohne Gerät nichts nachzuladen (Phase 19).
                match item.effect {
                    ServiceEffect::ShieldFull if !self.has_gear(super::data::Gear::Shield) => {
                        return Err("Kein Schildgenerator an Bord – gibt es in der Werft".into());
                    }
                    ServiceEffect::Ammo(_) if !self.has_gear(super::data::Gear::Cannon) => {
                        return Err("Keine Kanone an Bord – gibt es in der Werft".into());
                    }
                    _ => {}
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
                if let Some(vendor) = &u.vendor
                    && !self.vendor_here(vendor)
                {
                    let who = self
                        .data
                        .npcs
                        .iter()
                        .find(|n| &n.id == vendor)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    return Err(format!("Gibt es nur bei {who}"));
                }
                if self.crew.upgrades.contains(id) {
                    return Err("Bereits eingebaut".into());
                }
                // Verbesserungen für ein Gerät, das die Crew noch nicht hat, ergeben keinen Sinn.
                let needs = match u.part {
                    super::data::UpgradePart::Crane => Some(super::data::Gear::Crane),
                    super::data::UpgradePart::Cannon => Some(super::data::Gear::Cannon),
                    super::data::UpgradePart::Shield => Some(super::data::Gear::Shield),
                    _ => None,
                };
                if let Some(g) = needs
                    && !self.has_gear(g)
                {
                    return Err(format!("Erst die Ausrüstung kaufen: {}", g.label()));
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
                self.can_afford(&upgrade_cost(u))?;
                (u.name.clone(), u.price)
            }
            Purchase::Gear(g) => {
                if !st.has(Service::Upgrades) && !st.has(Service::Ships) {
                    return Err(format!("{} baut keine Ausrüstung ein", st.name));
                }
                if self.has_gear(*g) {
                    return Err("Bereits an Bord".into());
                }
                let price = self.gear_price(*g).ok_or("Nicht im Angebot")?;
                if self.crew.credits < price {
                    return Err("Zu wenig Credits".into());
                }
                (g.label().to_string(), price)
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
            Purchase::ShipOnCredit(id) => {
                if !st.has(Service::Ships) {
                    return Err("Schiffe gibt es nur in Werften".into());
                }
                if self.crew.owned_ships.contains(id) {
                    return Err("Gehört der Crew bereits".into());
                }
                if !st.ships_for_sale.contains(id) {
                    return Err(format!("{} führt dieses Schiff nicht", st.name));
                }
                if self.crew.loan.is_some() {
                    return Err("Es läuft schon ein Kredit".into());
                }
                let def = self
                    .data
                    .ships
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or("Unbekannt")?;
                let (down, _, _) = self.loan_terms(def.price);
                (format!("Schiff auf Kredit: {}", def.name), down)
            }
            Purchase::RepayLoan => {
                if !st.has(Service::Ships) {
                    return Err("Tilgen geht nur in einer Werft".into());
                }
                let l = self.crew.loan.as_ref().ok_or("Kein Kredit offen")?;
                ("Kredit tilgen".to_string(), l.left)
            }
            Purchase::Module { mount, module } => {
                if !st.has(Service::Ships) {
                    return Err("Anbauen geht nur in einer Werft".into());
                }
                let base = self.data.ship(&self.crew.current_ship);
                let m = base
                    .mounts
                    .iter()
                    .find(|x| &x.id == mount)
                    .ok_or("Unbekannter Bauplatz")?;
                let md = self.data.module(module).ok_or("Unbekanntes Modul")?;
                if !m.accepts.contains(&md.slot) {
                    return Err(format!("{} passt nicht an {}", md.name, m.name));
                }
                if self
                    .build_of(&self.crew.current_ship)
                    .iter()
                    .any(|(x, y)| x == mount && y == module)
                {
                    return Err("Ist schon angebaut".into());
                }
                self.can_afford(&module_cost(md))?;
                (format!("{} an {}", md.name, m.name), md.credits)
            }
            Purchase::RemoveModule { mount } => {
                if !st.has(Service::Ships) {
                    return Err("Abbauen geht nur in einer Werft".into());
                }
                let (_, module) = self
                    .build_of(&self.crew.current_ship)
                    .iter()
                    .find(|(x, _)| x == mount)
                    .ok_or("Der Bauplatz ist leer")?;
                let name = self
                    .data
                    .module(module)
                    .map(|m| m.name.clone())
                    .unwrap_or_default();
                (format!("{name} abbauen"), 0)
            }
            Purchase::Goods(i) => {
                let m = self.data.traffic.merchant.as_ref().ok_or("Unbekannt")?;
                let g = m.goods.get(*i).ok_or("Unbekannt")?;
                if !self.merchant_here() {
                    return Err(format!("{} ist gerade nicht hier", m.name));
                }
                (format!("{}: {}", m.name, g.name), g.price)
            }
            Purchase::Insurance(on) => {
                if *on == self.crew.insured {
                    return Err(if *on {
                        "Schon versichert".into()
                    } else {
                        "Keine Versicherung abgeschlossen".into()
                    });
                }
                let label = if *on {
                    "Versicherung abschließen"
                } else {
                    "Versicherung kündigen"
                };
                (label.to_string(), 0)
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
                Command::SellUpgrade { id } => self.sell_upgrade(id),
                Command::SwitchShip { id } => self.switch_ship(id),
                Command::Ping { player, pos } => self.add_ping(*player, *pos),
                Command::SellCharts => self.sell_charts(),
                Command::DeliverProject => self.deliver_project(),
                Command::StoreCargo => self.store_cargo(),
                Command::SetStayInWreck(on) => self.story.stay_in_wreck = *on,
                Command::MoveCargo { id, to } => self.move_cargo_cmd(*id, *to),
                Command::Jettison { id } => self.jettison(*id),
                Command::NameShip { name } => self.name_ship(name),
                Command::AddNote { text } => self.add_note(text),
                Command::AddMark { pos, text } => self.add_mark(*pos, text),
                Command::RemoveMark { idx } => self.remove_mark(*idx),
                Command::SetFlightAssist(on) => self.flight_assist = *on,
                Command::Callout { player, call } => {
                    // Ein Zuruf pro Crewmitglied; ein neuer ersetzt den alten.
                    self.callouts.retain(|c| c.player != *player);
                    self.callouts.push(super::Callout {
                        player: *player,
                        call: *call,
                        life: super::CALLOUT_SECONDS,
                    });
                    self.events.push(SimEvent::Callout {
                        player: *player,
                        call: *call,
                    });
                }
                Command::StartCourse { course } => self.arm_course(*course),
                Command::AbortCourse => self.abort_course("auf Wunsch der Crew"),
                Command::SetLoadout {
                    thrusters,
                    tools,
                    crew_size,
                } => {
                    let def = self.current_def();
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
            Purchase::Module { mount, module } => {
                if let Some(md) = self.data.module(module).cloned() {
                    // Ersetzt ein anderes Modul? Dessen halbes Material kommt ins Lager zurück.
                    self.refund_module(mount);
                    self.pay_materials(&module_cost(&md));
                    self.set_module(mount, Some(module));
                    self.after_rebuild(md.slot);
                }
            }
            Purchase::RemoveModule { mount } => {
                let slot = self
                    .build_of(&self.crew.current_ship)
                    .iter()
                    .find(|(x, _)| x == mount)
                    .and_then(|(_, m)| self.data.module(m))
                    .map(|m| m.slot);
                self.refund_module(mount);
                self.set_module(mount, None);
                if let Some(slot) = slot {
                    self.after_rebuild(slot);
                }
            }
            Purchase::ShipOnCredit(id) => {
                let full = self.data.ship(id).price;
                let (_, rest, rate) = self.loan_terms(full);
                self.crew.loan = Some(super::finance::Loan {
                    ship: id.clone(),
                    left: rest,
                    installment: rate,
                });
                self.crew.owned_ships.push(id.clone());
                self.switch_ship(id);
            }
            Purchase::RepayLoan => {
                self.crew.loan = None;
            }
            Purchase::Goods(i) => {
                if let Some(g) = self
                    .data
                    .traffic
                    .merchant
                    .as_ref()
                    .and_then(|m| m.goods.get(*i))
                    .cloned()
                {
                    if let Some((ore, t)) = g.material {
                        self.store_material(ore, t);
                    }
                    self.crew.storage_parts += g.parts;
                }
            }
            Purchase::Insurance(on) => {
                self.crew.insured = *on;
            }
            Purchase::Service(id) => {
                self.stats.expenses.service += price;
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
                if let Some(u) = self.data.upgrade(id).cloned() {
                    self.pay_materials(&upgrade_cost(&u));
                }
                self.crew.upgrades.push(id.clone());
                let (old_max_hull, old_max_shield, old_max_fuel) =
                    (self.ship.max_hull, self.ship.max_shield, self.ship.max_fuel);
                let loadout = self.loadout.clone();
                self.rebuild_ship(loadout);
                self.ship.hull += (self.ship.max_hull - old_max_hull).max(0.0);
                self.ship.shield += (self.ship.max_shield - old_max_shield).max(0.0);
                self.ship.fuel += (self.ship.max_fuel - old_max_fuel).max(0.0);
            }
            Purchase::Gear(g) => self.unlock_gear(*g, "eingebaut"),
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
        let def = self.current_def();
        // Fracht bleibt in der Werft nicht liegen – sie wird umgeladen, soweit Platz ist.
        let pad = self.ship.docked;
        let stats = stats_for(&self.data, &self.crew.upgrades);
        let cargo = std::mem::take(&mut self.ship.cargo);
        self.ship = super::ship::Ship::build(&def, &Loadout::full(&def), &stats);
        self.loadout = Loadout::full(&def);
        for item in cargo {
            self.ship.store_item(item);
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

    /// Abgebautes oder ersetztes Modul: die Hälfte des Materials kommt ins Lager zurück.
    fn refund_module(&mut self, mount: &str) {
        let Some(md) = self
            .build_of(&self.crew.current_ship)
            .iter()
            .find(|(x, _)| x == mount)
            .and_then(|(_, m)| self.data.module(m))
            .cloned()
        else {
            return;
        };
        for (o, t) in &md.materials {
            let i = super::data::Ore::ALL
                .iter()
                .position(|x| x == o)
                .unwrap_or(0);
            self.crew.storage[i] += t * 0.5;
        }
    }

    /// Nach dem Umbau: Schiff neu zusammensetzen. Neue oder entfernte Slots (Triebwerk,
    /// Werkzeug) heißen: Slots neu verteilen.
    fn after_rebuild(&mut self, slot: super::data::ModuleSlot) {
        use super::data::ModuleSlot;
        let def = self.current_def();
        match slot {
            ModuleSlot::Engine | ModuleSlot::Tool => {
                self.rebuild_ship(Loadout::full(&def));
                self.events.push(SimEvent::ShipChanged);
            }
            ModuleSlot::Cargo | ModuleSlot::Armor => {
                let loadout = self.loadout.clone();
                self.rebuild_ship(loadout);
            }
        }
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
            if let Some(o) = self.docked_owner() {
                self.note_sale(o, ore, taken);
            }
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

    /// Lässt sich das Upgrade hier ausbauen und verkaufen? Ok = Erlös in Credits.
    pub fn upgrade_sale(&self, id: &str) -> Result<u32, String> {
        let st = self
            .docked_station()
            .map(|i| &self.world.stations[i])
            .ok_or("Nicht angedockt")?;
        if !st.has(Service::Upgrades) {
            return Err(format!("{} hat keine Upgrade-Werkstatt", st.name));
        }
        let u = self.data.upgrade(id).ok_or("Unbekannt")?;
        if !self.crew.upgrades.iter().any(|x| x == id) {
            return Err("Nicht eingebaut".into());
        }
        // Stufen bauen aufeinander auf: erst die höhere ausbauen.
        if let Some(dep) = self
            .crew
            .upgrades
            .iter()
            .filter_map(|x| self.data.upgrade(x))
            .find(|d| d.requires.as_deref() == Some(id))
        {
            return Err(format!("Erst {} ausbauen", dep.name));
        }
        // Die Fracht muss danach noch in den Frachtraum passen.
        let rest: Vec<String> = self
            .crew
            .upgrades
            .iter()
            .filter(|x| *x != id)
            .cloned()
            .collect();
        let def = self.current_def();
        let loadout = super::gear::filter_loadout(&def, &self.crew.gear, self.loadout.clone());
        let cap = super::ship::Ship::build(&def, &loadout, &stats_for(&self.data, &rest))
            .cargo_capacity();
        if self.ship.cargo_mass() > cap + 1e-3 {
            return Err("Erst Fracht verkaufen – sonst passt sie nicht mehr".into());
        }
        Ok((u.price as f32 * UPGRADE_RESALE).round() as u32)
    }

    pub(crate) fn sell_upgrade(&mut self, id: &str) {
        match self.upgrade_sale(id) {
            Ok(credits) => {
                let name = self
                    .data
                    .upgrade(id)
                    .map(|u| u.name.clone())
                    .unwrap_or_default();
                self.crew.upgrades.retain(|x| x != id);
                self.crew.credits += credits;
                let loadout = self.loadout.clone();
                self.rebuild_ship(loadout);
                self.events.push(SimEvent::Sold { credits });
                self.toast(
                    format!("{name} ausgebaut und verkauft  +{credits} Credits"),
                    ToastKind::Good,
                );
            }
            Err(e) => self.toast(e, ToastKind::Warn),
        }
    }
}

/// Ausgebaute Upgrades bringen die Hälfte des Kaufpreises (Material und Bauteile bleiben weg).
pub const UPGRADE_RESALE: f32 = 0.5;

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
        // Volles Crew-Lager: Upgrades kosten auch Material und Bauteile.
        save.storage = crate::sim::data::Ore::ALL
            .iter()
            .map(|o| (*o, 50.0))
            .collect();
        save.storage_parts = 10;
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
    fn upgrades_can_be_sold_top_tier_first() {
        let mut s = sim(1);
        s.crew.upgrades = vec!["thrust1".into(), "thrust2".into(), "cargo1".into()];
        let loadout = s.loadout.clone();
        s.rebuild_ship(loadout);
        let thrust = s.ship.thrusters[0].thrust;
        let sell = |s: &mut SimState, id: &str| {
            s.step(&TickInput {
                commands: vec![Command::SellUpgrade { id: id.into() }],
                ..Default::default()
            });
        };
        // Stufe I trägt Stufe II – erst die obere ausbauen.
        assert!(s.upgrade_sale("thrust1").is_err());
        let credits = s.crew.credits;
        let price = s.data.upgrade("thrust2").unwrap().price;
        sell(&mut s, "thrust2");
        assert!(!s.crew.upgrades.contains(&"thrust2".to_string()));
        assert_eq!(s.crew.credits, credits + price / 2);
        assert!(s.ship.thrusters[0].thrust < thrust, "Schub wieder kleiner");
        assert!(s.upgrade_sale("thrust1").is_ok());
        // Volle Fracht: der größere Frachtraum lässt sich nicht verkaufen.
        let cap = s.ship.cargo_capacity();
        s.ship.store_item(crate::sim::ship::CargoItem::new(
            crate::sim::ship::CargoKind::Ore(Ore::Ferrit),
            cap * 0.9,
            crate::sim::data::CargoTrait::None,
        ));
        assert!(s.upgrade_sale("cargo1").is_err());
        // Nicht eingebaut oder unterwegs: geht nicht.
        assert!(s.upgrade_sale("drill1").is_err());
        s.ship.docked = None;
        assert!(s.upgrade_sale("thrust1").is_err());
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
        let price = s.ore_price_at(kepler, Ore::Solarit);
        s.sell_ore();
        let expected = (4.0 * price as f32).round() as u32;
        assert_eq!(s.crew.credits - before, expected);
        // Der Verkauf drückt den Preis dort (Angebot und Nachfrage).
        assert!(s.ore_price_at(kepler, Ore::Solarit) < price);
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
