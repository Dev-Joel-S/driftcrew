//! Wiederaufbau von Stationen in Etappen: Material und Bauteile abgeben, danach ändern sich
//! Aussehen und Dienste der Station. Der Fortschritt steht im Spielstand.

use super::data::{Ore, ProjectSave, Service};
use super::ship::CargoKind;
use super::{SimEvent, SimState, ToastKind};

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectState {
    pub station: usize,
    pub stage: u8,
    /// Für die laufende Etappe schon geliefert.
    pub delivered: Vec<(Ore, f32)>,
    pub parts: u32,
}

impl ProjectState {
    pub fn delivered_of(&self, ore: Ore) -> f32 {
        self.delivered
            .iter()
            .find(|(o, _)| *o == ore)
            .map_or(0.0, |(_, t)| *t)
    }
}

impl SimState {
    pub(crate) fn load_projects(&mut self, save: &super::data::CrewSave) {
        self.projects.clear();
        for (si, sd) in self.data.world.stations.iter().enumerate() {
            if sd.project.is_none() {
                continue;
            }
            let saved = save.projects.iter().find(|p| p.station == sd.id);
            self.projects.push(ProjectState {
                station: si,
                stage: saved.map_or(0, |p| p.stage),
                delivered: saved.map(|p| p.delivered.clone()).unwrap_or_default(),
                parts: saved.map_or(0, |p| p.parts),
            });
        }
        for i in 0..self.projects.len() {
            let (si, stage) = (self.projects[i].station, self.projects[i].stage);
            self.apply_stage(si, stage);
        }
    }

    pub(crate) fn save_projects(&self) -> Vec<ProjectSave> {
        self.projects
            .iter()
            .map(|p| ProjectSave {
                station: self.world.stations[p.station].id.clone(),
                stage: p.stage,
                delivered: p.delivered.clone(),
                parts: p.parts,
            })
            .collect()
    }

    fn apply_stage(&mut self, si: usize, stage: u8) {
        let Some(proj) = self.data.world.stations[si].project.clone() else {
            return;
        };
        let unlocked: Vec<Service> = proj
            .stages
            .iter()
            .take(stage as usize)
            .flat_map(|s| s.unlocks.iter().copied())
            .collect();
        self.world.set_stage(si, stage, &unlocked);
    }

    /// Projekt der angedockten Station (falls es eins gibt).
    pub fn project_here(&self) -> Option<&ProjectState> {
        let si = self.docked_station()?;
        self.projects.iter().find(|p| p.station == si)
    }

    /// Material und Bauteile an Bord abgeben, soweit die laufende Etappe sie braucht.
    pub(crate) fn deliver_project(&mut self) {
        let Some(si) = self.docked_station() else {
            return;
        };
        let Some(pi) = self.projects.iter().position(|p| p.station == si) else {
            return;
        };
        let Some(proj) = self.data.world.stations[si].project.clone() else {
            return;
        };
        let stage = self.projects[pi].stage as usize;
        let Some(def) = proj.stages.get(stage).cloned() else {
            self.toast("Wiederaufbau abgeschlossen", ToastKind::Info);
            return;
        };
        let mut gave = Vec::new();
        for (ore, need) in &def.needs {
            let missing = need - self.projects[pi].delivered_of(*ore);
            if missing <= 1e-3 {
                continue;
            }
            let taken = self.ship.take_ore(*ore, missing);
            if taken > 1e-3 {
                let p = &mut self.projects[pi];
                match p.delivered.iter_mut().find(|(o, _)| o == ore) {
                    Some(d) => d.1 += taken,
                    None => p.delivered.push((*ore, taken)),
                }
                gave.push(format!("{taken:.1} t {}", ore.label()));
            }
        }
        let parts_missing = def.parts.saturating_sub(self.projects[pi].parts);
        if parts_missing > 0 {
            let mut taken = 0;
            let mut keep = Vec::new();
            for item in std::mem::take(&mut self.ship.cargo) {
                if taken < parts_missing && matches!(item.kind, CargoKind::Salvage { .. }) {
                    taken += 1;
                } else {
                    keep.push(item);
                }
            }
            self.ship.cargo = keep;
            self.ship.recompute_mass();
            if taken > 0 {
                self.projects[pi].parts += taken;
                gave.push(format!("{taken} Bauteile"));
            }
        }
        if gave.is_empty() {
            self.toast(
                "Nichts Passendes an Bord – Material oder Bauteile fehlen",
                ToastKind::Warn,
            );
            return;
        }
        self.toast(format!("Abgegeben: {}", gave.join(", ")), ToastKind::Info);
        let p = &self.projects[pi];
        let done = def
            .needs
            .iter()
            .all(|(o, n)| p.delivered_of(*o) + 1e-3 >= *n)
            && p.parts >= def.parts;
        if !done {
            return;
        }
        // Etappe fertig: Station sieht anders aus und kann wieder mehr.
        let new_stage = stage as u8 + 1;
        self.projects[pi].stage = new_stage;
        self.projects[pi].delivered.clear();
        self.projects[pi].parts = 0;
        self.apply_stage(si, new_stage);
        self.crew.credits += def.reward;
        if let Some(r) = self.crew.reputation.get_mut(si) {
            *r += 3;
        }
        let name = self.world.stations[si].name.clone();
        self.events.push(SimEvent::StageCompleted {
            station: si,
            stage: new_stage,
        });
        self.toast(
            format!(
                "{name}: „{}“ fertig! {} (+{} Credits)",
                def.name, def.effect, def.reward
            ),
            ToastKind::Good,
        );
    }
}
