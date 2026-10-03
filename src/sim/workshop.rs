//! Werkstatt (Punkte 33, 34, 55–57): Crew-Lager für Material und Bauteile, Module an festen
//! Bauplätzen der Rümpfe, Kosten aus Credits, Material, Bauteilen und Artefakten.

use super::data::{CrewSave, Ore, ShipDef};
use super::ship::CargoKind;
use super::{SimState, ToastKind};

fn ore_index(ore: Ore) -> usize {
    Ore::ALL.iter().position(|o| *o == ore).unwrap_or(0)
}

/// Was etwas kostet (außer Credits auch Material aus dem Lager, Bauteile, ein Artefakt).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cost {
    pub credits: u32,
    pub materials: Vec<(Ore, f32)>,
    pub parts: u32,
    pub artifact: bool,
}

impl Cost {
    /// Kurzform für Menüs: „120 Cr · 5 t Ferrit · 1 Bauteil“.
    pub fn label(&self) -> String {
        let mut v = vec![format!("{} Cr", self.credits)];
        for (o, t) in &self.materials {
            v.push(format!("{t:.0} t {}", o.label()));
        }
        if self.parts > 0 {
            v.push(format!(
                "{} {}",
                self.parts,
                if self.parts == 1 {
                    "Bauteil"
                } else {
                    "Bauteile"
                }
            ));
        }
        if self.artifact {
            v.push("1 Artefakt".into());
        }
        v.join(" · ")
    }
}

impl SimState {
    /// Material einer Sorte im Crew-Lager.
    pub fn stored(&self, ore: Ore) -> f32 {
        self.crew.storage[ore_index(ore)]
    }

    /// Reicht es? Ok oder der erste fehlende Posten.
    pub fn can_afford(&self, cost: &Cost) -> Result<(), String> {
        if self.crew.credits < cost.credits {
            return Err(format!("Zu wenig Credits ({})", cost.credits));
        }
        for (o, t) in &cost.materials {
            if self.stored(*o) + 1e-3 < *t {
                return Err(format!(
                    "Im Crew-Lager fehlt {} ({:.1}/{t:.0} t)",
                    o.label(),
                    self.stored(*o)
                ));
            }
        }
        if self.crew.storage_parts < cost.parts {
            return Err(format!(
                "Zu wenig Bauteile im Lager ({}/{})",
                self.crew.storage_parts, cost.parts
            ));
        }
        if cost.artifact && self.crew.artifacts.is_empty() {
            return Err("Braucht ein Artefakt".into());
        }
        Ok(())
    }

    /// Kosten abziehen (vorher mit `can_afford` geprüft). Credits zieht der Kauf selbst ab.
    pub(crate) fn pay_materials(&mut self, cost: &Cost) {
        for (o, t) in &cost.materials {
            let s = &mut self.crew.storage[ore_index(*o)];
            *s = (*s - t).max(0.0);
        }
        self.crew.storage_parts = self.crew.storage_parts.saturating_sub(cost.parts);
        if cost.artifact && !self.crew.artifacts.is_empty() {
            self.crew.artifacts.remove(0);
        }
    }

    /// Module, die an einem Schiff angebaut sind: (Bauplatz, Modul).
    pub fn build_of(&self, ship: &str) -> &[(String, String)] {
        self.crew
            .builds
            .iter()
            .find(|(s, _)| s == ship)
            .map(|(_, b)| b.as_slice())
            .unwrap_or(&[])
    }

    /// Das aktuelle Schiff mit allen angebauten Modulen.
    pub fn current_def(&self) -> ShipDef {
        let id = &self.crew.current_ship;
        self.data.built_ship(id, self.build_of(id))
    }

    /// Modul an einem Bauplatz anbauen oder (None) abbauen.
    pub(crate) fn set_module(&mut self, mount: &str, module: Option<&str>) {
        let ship = self.crew.current_ship.clone();
        let idx = match self.crew.builds.iter().position(|(s, _)| *s == ship) {
            Some(i) => i,
            None => {
                self.crew.builds.push((ship, Vec::new()));
                self.crew.builds.len() - 1
            }
        };
        let list = &mut self.crew.builds[idx].1;
        list.retain(|(m, _)| m != mount);
        if let Some(md) = module {
            list.push((mount.to_string(), md.to_string()));
        }
    }

    /// Erz, Schrott und Bauteile aus dem Frachtraum ins Crew-Lager (was Abbau-Aufträge brauchen,
    /// bleibt an Bord).
    pub(crate) fn store_cargo(&mut self) {
        if self.docked_station().is_none() {
            self.toast("Einlagern geht nur an Stationen", ToastKind::Warn);
            return;
        }
        let mut stored_t = 0.0;
        for ore in Ore::ALL {
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
            let free = (self.ship.ore_amount(ore) - reserved).max(0.0);
            if free < 0.05 {
                continue;
            }
            let taken = self.ship.take_ore(ore, free);
            self.crew.storage[ore_index(ore)] += taken;
            stored_t += taken;
        }
        let parts = self
            .ship
            .remove_cargo(|k| matches!(k, CargoKind::Salvage { .. }))
            .len() as u32;
        self.crew.storage_parts += parts;
        if stored_t < 0.05 && parts == 0 {
            self.toast("Nichts zum Einlagern an Bord", ToastKind::Info);
            return;
        }
        let what = match (stored_t > 0.05, parts) {
            (true, 0) => format!("{stored_t:.1} t Material"),
            (false, n) => format!("{n} Bauteile"),
            (true, n) => format!("{stored_t:.1} t Material und {n} Bauteile"),
        };
        self.toast(
            format!("{what} ins Crew-Lager gebracht – in jeder Werft verbaubar"),
            ToastKind::Good,
        );
    }

    pub(crate) fn load_workshop(&mut self, save: &CrewSave) {
        let mut storage = [0.0f32; 5];
        for (o, t) in &save.storage {
            storage[ore_index(*o)] += *t;
        }
        self.crew.storage = storage;
        self.crew.storage_parts = save.storage_parts;
        self.crew.artifacts = save.artifacts.clone();
        self.crew.builds = save.builds.clone();
    }

    pub(crate) fn save_workshop(&self, save: &mut CrewSave) {
        save.storage = Ore::ALL
            .iter()
            .map(|o| (*o, self.stored(*o)))
            .filter(|(_, t)| *t > 0.001)
            .collect();
        save.storage_parts = self.crew.storage_parts;
        save.artifacts = self.crew.artifacts.clone();
        save.builds = self
            .crew
            .builds
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .cloned()
            .collect();
    }
}
