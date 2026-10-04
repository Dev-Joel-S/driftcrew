//! Persönlichkeit (Punkte 81, 82): Schiffsname, Plaketten als Andenken am Rumpf, und das
//! Crew-Tagebuch – automatische Erlebnisse (erstes Andocken, neue Sektoren, knappste Rettung,
//! größte Bergung, Plaketten), eigene Notizen und Kartenmarkierungen.

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

use super::data::{CrewSave, P, v};
use super::missions::MissionKind;
use super::{SimEvent, SimState, ToastKind};

/// Höchstens so viele Markierungen auf der Karte (die älteste fällt weg).
pub const MAX_MARKS: usize = 16;
/// Höchstlänge für Namen und Notizen (Zeichen).
pub const MAX_NAME: usize = 28;
pub const MAX_NOTE: usize = 160;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalKind {
    /// Etwas zum ersten Mal (Andocken, Sektor).
    First,
    /// Ein Rekord (knappste Rettung, größte Bergung).
    Record,
    /// Plakette bekommen.
    Plaque,
    /// Kapitel der Geschichte.
    Story,
    /// Von der Crew selbst geschrieben.
    Note,
}

impl JournalKind {
    pub fn icon(self) -> &'static str {
        match self {
            JournalKind::First => "★",
            JournalKind::Record => "▲",
            JournalKind::Plaque => "◆",
            JournalKind::Story => "§",
            JournalKind::Note => "✎",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct JournalEntry {
    /// Flugzeit der Crew in Sekunden (über alle Sitzungen).
    pub time: f32,
    pub kind: JournalKind,
    pub text: String,
}

/// Andenken am Rumpf: wofür, Farbe des Abzeichens, an welchem Schiff.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Plaque {
    pub id: String,
    pub title: String,
    pub color: [f32; 3],
    pub ship: String,
}

/// Markierung der Crew auf der Karte.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mark {
    pub pos: P,
    pub text: String,
}

/// Alles Persönliche der Crew (wird gespeichert).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Journal {
    pub playtime: f32,
    /// Name pro Schiffstyp: (Schiff, Name).
    pub names: Vec<(String, String)>,
    pub plaques: Vec<Plaque>,
    pub entries: Vec<JournalEntry>,
    pub marks: Vec<Mark>,
    /// Schon erlebte Erstmaligkeiten („dock:nova“, „sector:Kernsektor“).
    pub firsts: Vec<String>,
    /// Rekorde: knappste Rettung (Hülle in %), größte Bergung (Credits).
    pub closest_rescue: Option<f32>,
    pub biggest_salvage: Option<u32>,
}

/// Flugzeit lesbar: „1 h 05 min“ bzw. „12 min“.
pub fn fmt_time(t: f32) -> String {
    let m = (t / 60.0) as u32;
    if m >= 60 {
        format!("{} h {:02} min", m / 60, m % 60)
    } else {
        format!("{m} min")
    }
}

impl SimState {
    /// Name des aktuellen Schiffs (leer = noch keiner).
    pub fn ship_name(&self) -> Option<&str> {
        self.journal
            .names
            .iter()
            .find(|(s, _)| *s == self.crew.current_ship)
            .map(|(_, n)| n.as_str())
    }

    pub(crate) fn name_ship(&mut self, name: &str) {
        let name: String = name.trim().chars().take(MAX_NAME).collect();
        let ship = self.crew.current_ship.clone();
        self.journal.names.retain(|(s, _)| *s != ship);
        if name.is_empty() {
            self.toast("Schiffsname entfernt", ToastKind::Info);
            return;
        }
        self.journal.names.push((ship, name.clone()));
        self.add_entry(
            JournalKind::First,
            format!("Die Crew tauft ihr Schiff auf den Namen „{name}“"),
        );
        self.toast(format!("Schiff getauft: {name}"), ToastKind::Good);
    }

    pub(crate) fn add_entry(&mut self, kind: JournalKind, text: String) {
        self.journal.entries.push(JournalEntry {
            time: self.journal.playtime,
            kind,
            text,
        });
    }

    pub(crate) fn add_note(&mut self, text: &str) {
        let text: String = text.trim().chars().take(MAX_NOTE).collect();
        if text.is_empty() {
            return;
        }
        self.add_entry(JournalKind::Note, text);
        self.toast("Notiz im Logbuch", ToastKind::Info);
    }

    pub(crate) fn add_mark(&mut self, pos: Vec2, text: &str) {
        let mut text: String = text.trim().chars().take(MAX_NAME).collect();
        if text.is_empty() {
            text = self.describe_spot(pos);
            if text.is_empty() {
                text = "Markierung".into();
            }
        }
        if self.journal.marks.len() >= MAX_MARKS {
            self.journal.marks.remove(0);
        }
        self.journal.marks.push(super::journal::Mark {
            pos: (pos.x, pos.y),
            text: text.clone(),
        });
        self.toast(format!("Karte markiert: {text}"), ToastKind::Info);
    }

    pub(crate) fn remove_mark(&mut self, idx: usize) {
        if idx < self.journal.marks.len() {
            self.journal.marks.remove(idx);
        }
    }

    /// Einmalige Erlebnisse: true, wenn es neu war.
    fn first(&mut self, key: String, text: String) -> bool {
        if self.journal.firsts.contains(&key) {
            return false;
        }
        self.journal.firsts.push(key);
        self.add_entry(JournalKind::First, text);
        true
    }

    /// Plakette vergeben (einmal pro Anlass).
    pub(crate) fn award_plaque(&mut self, id: String, title: String, color: [f32; 3]) {
        if self.journal.plaques.iter().any(|p| p.id == id) {
            return;
        }
        let ship = self.crew.current_ship.clone();
        self.journal.plaques.push(Plaque {
            id,
            title: title.clone(),
            color,
            ship,
        });
        self.add_entry(JournalKind::Plaque, format!("Plakette am Rumpf: {title}"));
        self.toast(format!("Neue Plakette am Rumpf: {title}"), ToastKind::Good);
    }

    /// Plaketten am aktuellen Schiff.
    pub fn ship_plaques(&self) -> Vec<&Plaque> {
        self.journal
            .plaques
            .iter()
            .filter(|p| p.ship == self.crew.current_ship)
            .collect()
    }

    /// Beim Andocken an einer Station.
    pub(crate) fn journal_docked(&mut self, si: usize) {
        let st = &self.world.stations[si];
        let (id, name) = (st.id.clone(), st.name.clone());
        let any = self.journal.firsts.iter().any(|f| f.starts_with("dock:"));
        let text = if any {
            format!("Zum ersten Mal in {name} angedockt")
        } else {
            format!("Erstes erfolgreiches Andocken – in {name}")
        };
        self.first(format!("dock:{id}"), text);
    }

    /// Bei Abschluss eines Auftrags: Rekorde und Plaketten.
    pub(crate) fn journal_mission(&mut self, kind: &MissionKind, title: &str, earned: u32) {
        let hull = (self.ship.hull / self.ship.max_hull * 100.0).max(0.0);
        let rescue = matches!(
            kind,
            MissionKind::Rescue { .. } | MissionKind::Capsules { .. } | MissionKind::Tow { .. }
        );
        if rescue && self.journal.closest_rescue.is_none_or(|h| hull < h) {
            let before = self.journal.closest_rescue.replace(hull);
            if before.is_some() || hull < 60.0 {
                self.add_entry(
                    JournalKind::Record,
                    format!("Knappste Rettung bisher: {title} – mit nur {hull:.0} % Hülle"),
                );
            }
        }
        let salvage = matches!(
            kind,
            MissionKind::Tow { .. }
                | MissionKind::Bulky { .. }
                | MissionKind::Haul { .. }
                | MissionKind::Rescue { .. }
                | MissionKind::Capsules { .. }
        );
        if salvage && self.journal.biggest_salvage.is_none_or(|b| earned > b) {
            self.journal.biggest_salvage = Some(earned);
            self.add_entry(
                JournalKind::Record,
                format!("Größte Bergung bisher: {title} (+{earned} Cr)"),
            );
        }
        match kind {
            MissionKind::Rescue { ship, .. } => self.award_plaque(
                format!("rescue:{ship}"),
                format!("Retter der {ship}"),
                [0.95, 0.35, 0.3],
            ),
            MissionKind::Bulky { name, .. } => self.award_plaque(
                format!("bulky:{name}"),
                format!("Bergung: {name}"),
                [1.0, 0.7, 0.15],
            ),
            _ => {}
        }
    }

    /// Jeder Tick: Flugzeit, neue Sektoren, Meilensteine aus den Ereignissen.
    pub(crate) fn update_journal(&mut self) {
        self.journal.playtime += super::DT;
        if self.tick.is_multiple_of(30)
            && !self.ship.destroyed
            && let Some(name) = self.region_at(self.ship.pos)
        {
            self.first(format!("sector:{name}"), format!("Sektor erkundet: {name}"));
        }
        let milestones: Vec<SimEvent> = self
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    SimEvent::MonumentWoke { .. }
                        | SimEvent::StageCompleted { .. }
                        | SimEvent::ChapterDone { .. }
                )
            })
            .cloned()
            .collect();
        for e in milestones {
            match e {
                SimEvent::MonumentWoke { idx } => {
                    let Some(m) = self.world.monuments.get(idx) else {
                        continue;
                    };
                    let (name, c) = (m.name.clone(), m.accent_color);
                    self.award_plaque(format!("monument:{name}"), format!("Monument {name}"), c);
                }
                SimEvent::StageCompleted { station, stage } => {
                    let name = self.world.stations[station].name.clone();
                    self.award_plaque(
                        format!("project:{station}:{stage}"),
                        format!("Wiederaufbau {name}, Etappe {}", stage + 1),
                        [0.35, 0.9, 0.6],
                    );
                }
                SimEvent::ChapterDone { chapter } => {
                    let title = self
                        .data
                        .story
                        .chapters
                        .get(chapter)
                        .map(|c| c.title.clone())
                        .unwrap_or_default();
                    self.add_entry(
                        JournalKind::Story,
                        format!("Kapitel abgeschlossen: {title}"),
                    );
                }
                _ => {}
            }
        }
    }

    /// Name des Sektors an einem Ort (der kleinste, der ihn enthält).
    pub fn region_at(&self, p: Vec2) -> Option<String> {
        self.data
            .world
            .regions
            .iter()
            .filter(|r| (v(r.center) - p).length() < r.radius)
            .min_by(|a, b| a.radius.total_cmp(&b.radius))
            .map(|r| r.name.clone())
    }

    pub(crate) fn load_journal(&mut self, save: &CrewSave) {
        self.journal = save.journal.clone();
    }

    pub(crate) fn save_journal(&self, save: &mut CrewSave) {
        save.journal = self.journal.clone();
    }
}
