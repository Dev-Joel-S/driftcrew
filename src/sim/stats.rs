//! Spaßstatistik: was jeder Slot während eines Auftrags getan hat, und die Auswertung
//! mit kleinen Auszeichnungen. Gezählt wird pro Slot – wer welchen Slot hatte, weiß nur
//! die Anzeige.

use super::MAX_SLOTS;
use super::finance::Expenses;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SlotStats {
    /// Sekunden, in denen das Triebwerk tatsächlich geschoben hat.
    pub thrust_time: f32,
    /// Kollisionen, bei denen dieses Teil dem Aufprall am nächsten war.
    pub collisions: u32,
    pub shots: u32,
    /// Abgebautes Erz bzw. Schrott in Tonnen.
    pub drilled: f32,
    /// Erfolgreiche Kran-Greifer.
    pub grabs: u32,
}

impl SlotStats {
    fn minus(&self, o: &SlotStats) -> SlotStats {
        SlotStats {
            thrust_time: self.thrust_time - o.thrust_time,
            collisions: self.collisions - o.collisions,
            shots: self.shots - o.shots,
            drilled: self.drilled - o.drilled,
            grabs: self.grabs - o.grabs,
        }
    }
}

/// Laufende Zähler der Crew (über die ganze Sitzung).
#[derive(Clone, Debug, PartialEq)]
pub struct CrewStats {
    pub slots: Vec<SlotStats>,
    pub time: f32,
    pub distance: f32,
    pub top_speed: f32,
    pub damage: f32,
    /// Ausgaben unterwegs (Dockgebühren, Service, Bergung).
    pub expenses: Expenses,
}

impl Default for CrewStats {
    fn default() -> Self {
        CrewStats {
            slots: vec![SlotStats::default(); MAX_SLOTS],
            time: 0.0,
            distance: 0.0,
            top_speed: 0.0,
            damage: 0.0,
            expenses: Expenses::default(),
        }
    }
}

impl CrewStats {
    pub fn slot(&mut self, slot: u8) -> &mut SlotStats {
        &mut self.slots[(slot as usize).min(MAX_SLOTS - 1)]
    }

    /// Differenz seit einem früheren Stand (für einen einzelnen Auftrag).
    /// Die Höchstgeschwindigkeit wird separat pro Auftrag geführt.
    pub fn since(&self, start: &CrewStats, top_speed: f32) -> CrewStats {
        CrewStats {
            slots: self
                .slots
                .iter()
                .zip(&start.slots)
                .map(|(a, b)| a.minus(b))
                .collect(),
            time: self.time - start.time,
            distance: self.distance - start.distance,
            top_speed,
            damage: self.damage - start.damage,
            expenses: self.expenses.minus(&start.expenses),
        }
    }
}

/// Eine Auszeichnung in der Auswertung.
#[derive(Clone, Debug, PartialEq)]
pub struct Award {
    pub title: &'static str,
    pub slot: u8,
    pub value: String,
}

/// Auswertung eines erledigten Auftrags.
#[derive(Clone, Debug, PartialEq)]
pub struct MissionReport {
    pub mission: u32,
    pub title: String,
    /// Grundbelohnung (bei Passagieren schon nach Zufriedenheit).
    pub reward: u32,
    pub bonus_time: u32,
    pub bonus_clean: u32,
    /// Bonus für sanft geführte Last am Kran und die höchste Seilbelastung dabei.
    pub bonus_gentle: u32,
    pub max_strain: Option<f32>,
    /// Zufriedenheit der Passagiere (0..1), falls welche an Bord waren.
    pub comfort: Option<f32>,
    /// Richtzeit in Sekunden.
    pub par: f32,
    /// Station, Rufstufe danach, gewonnene Punkte.
    pub reputation: Option<(String, u8, u32)>,
    pub stats: CrewStats,
    /// Welche Slots waren Triebwerke (für „Schubmeister“ und „Sparfuchs“).
    pub thruster_slots: Vec<u8>,
    /// Abrechnung: Versicherungsprämie und Kreditrate, die von den Einnahmen abgingen.
    pub premium: u32,
    pub installment: u32,
}

impl MissionReport {
    /// Einnahmen des Auftrags (Grundbelohnung und Boni).
    pub fn earned(&self) -> u32 {
        self.reward + self.bonus_time + self.bonus_clean + self.bonus_gentle
    }
    /// Was in die Kasse ging.
    pub fn paid_in(&self) -> u32 {
        self.earned()
            .saturating_sub(self.premium + self.installment)
    }
    /// Gewinn: Einnahmen minus Kosten des Auftrags (Prämie und alles unterwegs Bezahlte).
    /// Die Kreditrate zählt nicht dazu – sie zahlt das Schiff ab.
    pub fn profit(&self) -> i64 {
        self.earned() as i64 - self.premium as i64 - self.stats.expenses.total() as i64
    }
}

impl MissionReport {
    pub fn awards(&self) -> Vec<Award> {
        let s = &self.stats.slots;
        let mut v = Vec::new();
        let best = |f: &dyn Fn(&SlotStats) -> f32| -> Option<(u8, f32)> {
            s.iter()
                .enumerate()
                .map(|(i, st)| (i as u8, f(st)))
                .filter(|(_, x)| *x > 0.0)
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        };
        if let Some((slot, t)) = best(&|st| st.thrust_time) {
            v.push(Award {
                title: "Schubmeister",
                slot,
                value: format!("{t:.0} s Schub"),
            });
        }
        if self.thruster_slots.len() >= 2 {
            let lazy = self
                .thruster_slots
                .iter()
                .map(|&sl| (sl, s[sl as usize].thrust_time))
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            if let Some((slot, t)) = lazy
                && v.first().is_none_or(|a| a.slot != slot)
            {
                v.push(Award {
                    title: "Sparfuchs",
                    slot,
                    value: format!("nur {t:.0} s Schub"),
                });
            }
        }
        if let Some((slot, n)) = best(&|st| st.collisions as f32) {
            v.push(Award {
                title: "Bruchpilot",
                slot,
                value: format!("{n:.0} Kollisionen"),
            });
        }
        if let Some((slot, n)) = best(&|st| st.shots as f32) {
            v.push(Award {
                title: "Scharfschütze",
                slot,
                value: format!("{n:.0} Schüsse"),
            });
        }
        if let Some((slot, t)) = best(&|st| st.drilled) {
            v.push(Award {
                title: "Bohrkönig",
                slot,
                value: format!("{t:.1} t abgebaut"),
            });
        }
        if let Some((slot, n)) = best(&|st| st.grabs as f32) {
            v.push(Award {
                title: "Greifarm",
                slot,
                value: format!("{n:.0}× gegriffen"),
            });
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awards_pick_the_extremes() {
        let mut stats = CrewStats::default();
        stats.slots[0].thrust_time = 30.0;
        stats.slots[1].thrust_time = 4.0;
        stats.slots[1].collisions = 3;
        stats.slots[5].shots = 7;
        let r = MissionReport {
            mission: 1,
            title: "Test".into(),
            reward: 100,
            bonus_time: 0,
            bonus_clean: 0,
            bonus_gentle: 0,
            max_strain: None,
            comfort: None,
            par: 100.0,
            reputation: None,
            stats,
            thruster_slots: vec![0, 1],
            premium: 0,
            installment: 0,
        };
        let a = r.awards();
        let get = |t: &str| a.iter().find(|x| x.title == t).map(|x| x.slot);
        assert_eq!(get("Schubmeister"), Some(0));
        assert_eq!(get("Sparfuchs"), Some(1));
        assert_eq!(get("Bruchpilot"), Some(1));
        assert_eq!(get("Scharfschütze"), Some(5));
        assert_eq!(get("Bohrkönig"), None);
    }
}
