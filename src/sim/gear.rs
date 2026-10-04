//! Ausrüstung (Phase 19): Kran, Kanone und Schildgenerator muss die Crew erst kaufen (Werft)
//! oder finden (in bestimmten Wracks). Ohne sie gibt es den Slot nicht. Die Schildladung
//! verbraucht sich wie Munition und wird an Stationen nachgekauft – sie lädt nicht von selbst.

use super::data::{CrewSave, Gear, ShipDef, ToolKind};
use super::ship::Loadout;
use super::{SimEvent, SimState, ToastKind};

/// Hat die Crew diese Ausrüstung? (None = alter Spielstand, alles an Bord.)
pub fn owns(gear: &Option<Vec<Gear>>, g: Gear) -> bool {
    gear.as_ref().is_none_or(|v| v.contains(&g))
}

/// Darf dieses Werkzeug an Bord sein? Bohrer und Scanner immer.
pub fn tool_owned(gear: &Option<Vec<Gear>>, kind: ToolKind) -> bool {
    Gear::ALL
        .iter()
        .find(|g| g.tool() == Some(kind))
        .is_none_or(|g| owns(gear, *g))
}

/// Belegung ohne Werkzeuge, die die Crew noch nicht hat.
pub fn filter_loadout(def: &ShipDef, gear: &Option<Vec<Gear>>, mut l: Loadout) -> Loadout {
    let kinds: Vec<ToolKind> = def.tool_parts().map(|(_, _, k)| k).collect();
    l.tools
        .retain(|&i| kinds.get(i).is_some_and(|k| tool_owned(gear, *k)));
    l
}

impl SimState {
    pub fn has_gear(&self, g: Gear) -> bool {
        owns(&self.crew.gear, g)
    }

    /// Nach jedem Bau des Schiffs: ohne Schildgenerator kein Schild.
    pub(crate) fn apply_gear(&mut self) {
        if !self.has_gear(Gear::Shield) {
            self.ship.max_shield = 0.0;
            self.ship.shield = 0.0;
        }
    }

    /// Gespeicherte Munition und Schildladung übernehmen (gekauft ist gekauft).
    pub(crate) fn load_gear(&mut self, save: &CrewSave) {
        self.apply_gear();
        if let Some(a) = save.ammo {
            self.ship.ammo = a.min(self.ship.max_ammo);
        }
        if let Some(sh) = save.shield {
            self.ship.shield = sh.clamp(0.0, self.ship.max_shield);
        }
    }

    /// Neue Ausrüstung: gekauft oder gefunden. Werkzeuge brauchen danach eine Taste – das
    /// Ereignis sagt der Oberfläche, dass jemand sie übernehmen kann.
    pub(crate) fn unlock_gear(&mut self, g: Gear, how: &str) {
        if self.has_gear(g) {
            return;
        }
        self.crew.gear.get_or_insert_with(Vec::new).push(g);
        match g {
            Gear::Shield => {
                // Frisch eingebaut und voll geladen.
                let loadout = self.loadout.clone();
                self.rebuild_ship(loadout);
                self.ship.shield = self.ship.max_shield;
                self.toast(
                    format!(
                        "Schildgenerator {how}: {:.0} Schild – die Ladung verbraucht sich, an Stationen nachkaufen",
                        self.ship.max_shield
                    ),
                    ToastKind::Good,
                );
            }
            Gear::Crane | Gear::Cannon => {
                if g == Gear::Cannon {
                    // Mit einer ersten Ladung Munition.
                    self.ship.ammo = self.ship.max_ammo;
                }
                self.toast(
                    format!(
                        "{} {how} – wer ihn steuern will: eine freie Taste drücken (oder Pause → Tasten & Slots)",
                        g.label()
                    ),
                    ToastKind::Good,
                );
            }
        }
        self.events.push(SimEvent::GearUnlocked { gear: g });
    }

    /// Preis der Ausrüstung in der Werft.
    pub fn gear_price(&self, g: Gear) -> Option<u32> {
        self.data
            .shop
            .gear
            .iter()
            .find(|d| d.gear == g)
            .map(|d| d.price)
    }
}
