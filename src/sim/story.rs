//! Geschichte (Punkte 45, 58–63, 61a/d/e/f): Artefakte der Vorgänger, Logbuch, Kapitel als
//! roter Faden und Monumente. Alles aus `story.ron`, deterministisch, im Spielstand.
//!
//! - Artefakte gibt es pro Spielstand genau einmal. Wo sie liegen, wählt der Seed des
//!   Spielstands beim ersten Start aus den möglichen Fundorten; danach steht der Ort im
//!   Spielstand (kein Respawn). An Bord haben sie Masse und eine Nebenwirkung; an einer Station
//!   abgeliefert, gehören sie zur Sammlung der Crew (Schlüssel für Kapitel und Upgrades).
//! - Logbuch-Einträge kommen aus Archiven (erstes Andocken), Bordbüchern in Wracks und
//!   Funkfetzen (beides mit dem Scanner), Funden, Kapiteln und Monumenten.
//! - Kapitel beginnen, wenn Aufträge, Ruf und Sammlung reichen, und enden mit einem Ziel.
//! - Monumente erwachen, wenn die Crew sie mit genug Artefakten in der Sammlung scannt.

use bevy::math::Vec2;

use super::data::{ArtifactEffect, ChapterGoal, CrewSave, LogSource, v};
use super::rng::{Rng, hash32};
use super::ship::CargoKind;
use super::{Body, BodyKind, DT, SimEvent, SimState, ToastKind};

/// Radius und Dämpfung eines Artefakts in der Welt (es treibt kaum und folgt keinem Sog).
pub const ARTIFACT_RADIUS: f32 = 0.9;
/// So nah muss ein Scan an einem Monument beginnen, damit es reagiert (über seinen Rand hinaus).
const MONUMENT_SCAN: f32 = 70.0;
/// Bis zu dieser Entfernung spüren Monumente Artefakte an Bord (Resonanz).
pub const RESONANCE_RANGE: f32 = 260.0;

#[derive(Clone, Debug, Default)]
pub struct Story {
    pub seed: u64,
    /// Aktuelles Kapitel (gleich der Kapitelzahl = alle erledigt) und ob es schon begonnen hat.
    pub chapter: usize,
    pub started: bool,
    /// Gefundene Logbuch-Einträge in der Reihenfolge des Findens.
    pub logs: Vec<String>,
    /// Fundorte der noch nicht abgelieferten Artefakte (ursprünglicher Ort).
    pub homes: Vec<(String, Vec2)>,
    /// Zeigt die Karte die Fundorte (nach dem Signalturm)?
    pub sites_revealed: bool,
    /// 61f: Artefakte an Bord bleiben bei Zerstörung im Wrack (sonst zurück an den Fundort).
    pub stay_in_wreck: bool,
}

impl SimState {
    pub(crate) fn load_story(&mut self, save: &CrewSave) {
        let st = &save.story;
        self.story = Story {
            seed: st.seed,
            chapter: st.chapter.min(self.data.story.chapters.len()),
            started: false,
            logs: st.logs.clone(),
            homes: Vec::new(),
            sites_revealed: false,
            stay_in_wreck: st.stay_in_wreck,
        };
        for id in &st.monuments {
            if let Some(mi) = self.world.monuments.iter().position(|m| &m.id == id) {
                self.world.wake_monument(mi);
                if self.data.story.monuments[mi].reveals_sites {
                    self.story.sites_revealed = true;
                }
            }
        }
        // Fundorte: aus dem Spielstand oder einmalig per Seed gewählt.
        let mut sites: Vec<(String, Vec2)> = st
            .artifact_sites
            .iter()
            .map(|(id, p)| (id.clone(), v(*p)))
            .collect();
        if sites.is_empty() {
            let mut rng = Rng::new(st.seed ^ 0x51A7_0F00_D5EE_D000);
            let mut pool: Vec<Vec2> = self
                .data
                .story
                .artifact_sites
                .iter()
                .map(|p| v(*p))
                .collect();
            for a in &self.data.story.artifacts {
                if pool.is_empty() {
                    break;
                }
                let p = pool.remove(rng.index(pool.len()));
                sites.push((a.id.clone(), p));
            }
        }
        for (id, pos) in sites {
            if self.crew.artifacts.contains(&id) {
                continue;
            }
            self.story.homes.push((id.clone(), pos));
            self.spawn_artifact(&id, pos, Vec2::ZERO);
        }
    }

    pub(crate) fn save_story(&self, save: &mut CrewSave) {
        let st = &mut save.story;
        st.seed = self.story.seed;
        st.chapter = self.story.chapter;
        st.logs = self.story.logs.clone();
        st.stay_in_wreck = self.story.stay_in_wreck;
        st.monuments = self
            .world
            .monuments
            .iter()
            .filter(|m| m.awake)
            .map(|m| m.id.clone())
            .collect();
        // Wo liegen die nicht abgelieferten Artefakte jetzt? In der Welt, an Bord (dann dort,
        // wo das Schiff ist) oder – falls verschwunden – am ursprünglichen Fundort.
        st.artifact_sites = self
            .story
            .homes
            .iter()
            .map(|(id, home)| {
                let in_world = self.bodies.iter().find(|b| {
                    b.alive && matches!(&b.kind, BodyKind::Artifact { id: a } if a == id)
                });
                let aboard = self
                    .ship
                    .cargo
                    .iter()
                    .any(|c| matches!(&c.kind, CargoKind::Artifact { id: a } if a == id));
                let p = match (in_world, aboard) {
                    (Some(b), _) => b.pos,
                    (None, true) => self.ship.pos,
                    _ => *home,
                };
                (id.clone(), (p.x, p.y))
            })
            .collect();
    }

    pub fn artifact_def(&self, id: &str) -> Option<&super::data::ArtifactDef> {
        self.data.story.artifacts.iter().find(|a| a.id == id)
    }

    pub fn artifact_name(&self, id: &str) -> String {
        self.artifact_def(id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    fn spawn_artifact(&mut self, id: &str, pos: Vec2, vel: Vec2) {
        let mass = self.artifact_def(id).map_or(1.0, |a| a.mass);
        let bid = self.next_id();
        self.bodies.push(Body {
            id: bid,
            kind: BodyKind::Artifact { id: id.to_string() },
            pos,
            vel,
            angle: 0.0,
            ang_vel: 0.4,
            radius: ARTIFACT_RADIUS,
            mass,
            prev_pos: pos,
            prev_angle: 0.0,
            alive: true,
            seed: hash32(bid),
            age: 0.0,
        });
    }

    /// Artefakte, die gerade an Bord sind.
    pub fn artifacts_aboard(&self) -> Vec<String> {
        self.ship
            .cargo
            .iter()
            .filter_map(|c| match &c.kind {
                CargoKind::Artifact { id } => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    fn aboard_effects(&self) -> Vec<ArtifactEffect> {
        self.artifacts_aboard()
            .iter()
            .filter_map(|id| self.artifact_def(id).map(|a| a.effect))
            .collect()
    }

    /// Störung von Radar und Scanner durch Artefakte an Bord (0..1).
    pub fn artifact_jam(&self) -> f32 {
        self.aboard_effects()
            .iter()
            .map(|e| match e {
                ArtifactEffect::Jam(j) => *j,
                _ => 0.0,
            })
            .fold(0.0, f32::max)
    }

    /// Zieht ein Artefakt an Bord Piraten an?
    pub fn artifact_lure(&self) -> bool {
        self.aboard_effects().contains(&ArtifactEffect::Lure)
    }

    /// Mehrverbrauch an Treibstoff durch Artefakte an Bord (Faktor).
    pub fn artifact_heat(&self) -> f32 {
        self.aboard_effects()
            .iter()
            .map(|e| match e {
                ArtifactEffect::Heat(f) => *f,
                _ => 1.0,
            })
            .product()
    }

    /// Drehstöße durch Artefakte an Bord (in `apply_forces`).
    pub(crate) fn artifact_spin(&mut self) {
        let k: f32 = self
            .aboard_effects()
            .iter()
            .map(|e| match e {
                ArtifactEffect::Spin(s) => *s,
                _ => 0.0,
            })
            .sum();
        if k <= 0.0 || self.ship.docked.is_some() || self.ship.destroyed {
            return;
        }
        let t = self.time;
        let wobble = (t * 1.7).sin() * 0.6 + (t * 0.63 + 1.3).sin() * 0.4;
        self.ship.ang_vel += wobble * k * DT;
    }

    /// Resonanz eines Monuments mit den Artefakten an Bord (0..1, für Licht und Ton).
    pub fn monument_resonance(&self, mi: usize) -> f32 {
        let Some(m) = self.world.monuments.get(mi) else {
            return 0.0;
        };
        if m.awake {
            return 1.0;
        }
        let d = (self.ship.pos - m.pos).length();
        if d > RESONANCE_RANGE || self.ship.destroyed {
            return 0.0;
        }
        let need = self.data.story.monuments[mi].artifacts.max(1) as f32;
        let have = (self.artifacts_aboard().len() + self.crew.artifacts.len()) as f32;
        (have / need).min(1.0) * (1.0 - d / RESONANCE_RANGE).clamp(0.2, 1.0)
    }

    /// Logbuch-Eintrag gefunden (einmalig).
    pub(crate) fn find_log(&mut self, id: &str) {
        if self.story.logs.iter().any(|l| l == id) {
            return;
        }
        let Some(log) = self.data.story.logs.iter().find(|l| l.id == id).cloned() else {
            return;
        };
        self.story.logs.push(id.to_string());
        self.events.push(SimEvent::LogFound { id: id.to_string() });
        self.toast(
            format!("Logbuch ({}): {}", log.source.category(), log.title),
            ToastKind::Info,
        );
    }

    fn find_log_by(&mut self, pred: impl Fn(&LogSource) -> bool) {
        let ids: Vec<String> = self
            .data
            .story
            .logs
            .iter()
            .filter(|l| pred(&l.source))
            .map(|l| l.id.clone())
            .collect();
        for id in ids {
            self.find_log(&id);
        }
    }

    /// Beim Andocken an einer Station: Archiv lesen, Artefakte an Bord in die Sammlung.
    pub(crate) fn story_docked(&mut self, si: usize) {
        let sid = self.world.stations[si].id.clone();
        self.find_log_by(|s| *s == LogSource::Station(sid.clone()));
        let found = self
            .ship
            .remove_cargo(|k| matches!(k, CargoKind::Artifact { .. }));
        for item in found {
            let CargoKind::Artifact { id } = item.kind else {
                continue;
            };
            if self.crew.artifacts.contains(&id) {
                continue;
            }
            self.crew.artifacts.push(id.clone());
            self.story.homes.retain(|(h, _)| *h != id);
            let name = self.artifact_name(&id);
            self.events
                .push(SimEvent::ArtifactCollected { id: id.clone() });
            self.toast(
                format!("Artefakt geborgen: {name} – jetzt in der Sammlung der Crew"),
                ToastKind::Good,
            );
            self.find_log_by(|s| *s == LogSource::Artifact(id.clone()));
        }
        self.update_chapter();
    }

    /// Schiff zerstört: Artefakte an Bord bleiben im Wrack (61f) oder kehren an ihren Fundort
    /// zurück.
    pub(crate) fn story_ship_lost(&mut self, ids: Vec<String>, at: Vec2, vel: Vec2) {
        for (k, id) in ids.into_iter().enumerate() {
            if self.story.stay_in_wreck {
                let a = k as f32 * 2.1;
                let off = Vec2::new(a.cos(), a.sin()) * 2.5;
                self.spawn_artifact(&id, at + off, vel * 0.5 + off * 0.4);
                let name = self.artifact_name(&id);
                self.toast(
                    format!("{name} liegt jetzt im Wrack – holt es zurück"),
                    ToastKind::Warn,
                );
            } else if let Some(&(_, home)) = self.story.homes.iter().find(|(h, _)| *h == id) {
                self.spawn_artifact(&id, home, Vec2::ZERO);
            }
        }
    }

    /// Scanner-Impuls beginnt: Funkfetzen, Kapitelziele und Monumente in der Nähe.
    pub(crate) fn story_scan_start(&mut self, origin: Vec2) {
        let echoes: Vec<String> = self
            .data
            .story
            .logs
            .iter()
            .filter(|l| {
                matches!(l.source, LogSource::Echo { pos, radius } if (v(pos) - origin).length() < radius)
            })
            .map(|l| l.id.clone())
            .collect();
        for id in echoes {
            self.find_log(&id);
        }
        // Kapitelziel „hier scannen“.
        if self.story.started
            && let Some(ch) = self.data.story.chapters.get(self.story.chapter)
            && let ChapterGoal::Scan { pos, radius } = ch.goal
            && (v(pos) - origin).length() < radius
        {
            self.complete_chapter();
        }
        // Monumente in Reichweite.
        for mi in 0..self.world.monuments.len() {
            let m = &self.world.monuments[mi];
            let reach = (m.bounds.max - m.bounds.min).length() * 0.5 + MONUMENT_SCAN;
            if (m.pos - origin).length() > reach {
                continue;
            }
            self.scan_monument(mi);
        }
    }

    /// Ein Wrack im Scannerring: sein Bordbuch auslesen.
    pub(crate) fn story_scan_wreck(&mut self, idx: usize) {
        let Some(name) = self.data.world.wrecks.get(idx).map(|w| w.name.clone()) else {
            return;
        };
        self.find_log_by(|s| *s == LogSource::Wreck(name.clone()));
    }

    fn scan_monument(&mut self, mi: usize) {
        if self.world.monuments[mi].awake {
            return;
        }
        let md = self.data.story.monuments[mi].clone();
        let have = self.crew.artifacts.len() as u32;
        if have < md.artifacts {
            let aboard = self.artifacts_aboard().len();
            let hint = if aboard > 0 {
                " – Artefakte an Bord zählen erst, wenn sie an einer Station abgeliefert sind"
            } else {
                ""
            };
            self.events.push(SimEvent::Story {
                speaker: md.name.clone(),
                text: format!(
                    "Der Stein summt kurz und verstummt. ({have}/{} Artefakte in der Sammlung{hint})",
                    md.artifacts
                ),
            });
            return;
        }
        self.world.wake_monument(mi);
        self.events.push(SimEvent::MonumentWoke { idx: mi });
        self.events.push(SimEvent::Story {
            speaker: md.name.clone(),
            text: md.effect.clone(),
        });
        self.toast(format!("{} ist erwacht", md.name), ToastKind::Good);
        if md.reveals_sites {
            self.story.sites_revealed = true;
            let homes: Vec<Vec2> = self.artifact_positions();
            for p in homes {
                self.explored.reveal(p, 180.0);
            }
            for m in self.world.monuments.clone() {
                self.explored.reveal(m.pos, 220.0);
            }
        }
        self.find_log_by(|s| *s == LogSource::Monument(md.id.clone()));
        if self.story.started
            && let Some(ch) = self.data.story.chapters.get(self.story.chapter)
            && ch.goal == ChapterGoal::Monument(md.id.clone())
        {
            self.complete_chapter();
        }
    }

    /// Wo die nicht abgelieferten Artefakte gerade sind (für Karte und Tests).
    pub fn artifact_positions(&self) -> Vec<Vec2> {
        self.bodies
            .iter()
            .filter(|b| b.alive && matches!(b.kind, BodyKind::Artifact { .. }))
            .map(|b| b.pos)
            .collect()
    }

    /// Ruf über alle Stationen.
    pub fn total_reputation(&self) -> u32 {
        self.crew.reputation.iter().sum()
    }

    /// Was fehlt noch, damit das aktuelle Kapitel beginnt? (leer = nichts)
    pub fn chapter_missing(&self) -> Vec<String> {
        let Some(ch) = self.data.story.chapters.get(self.story.chapter) else {
            return Vec::new();
        };
        let mut v = Vec::new();
        if self.crew.missions_done < ch.missions {
            v.push(format!(
                "{} erledigte Aufträge (jetzt {})",
                ch.missions, self.crew.missions_done
            ));
        }
        if self.total_reputation() < ch.reputation {
            v.push(format!(
                "Ruf {} insgesamt (jetzt {})",
                ch.reputation,
                self.total_reputation()
            ));
        }
        if (self.crew.artifacts.len() as u32) < ch.artifacts {
            v.push(format!(
                "{} Artefakte in der Sammlung (jetzt {})",
                ch.artifacts,
                self.crew.artifacts.len()
            ));
        }
        v
    }

    /// Kapitel beginnen bzw. Ziele prüfen, die keinen eigenen Auslöser haben.
    pub(crate) fn update_chapter(&mut self) {
        let Some(ch) = self.data.story.chapters.get(self.story.chapter).cloned() else {
            return;
        };
        if !self.story.started {
            if !self.chapter_missing().is_empty() {
                return;
            }
            self.story.started = true;
            self.events.push(SimEvent::Story {
                speaker: ch.speaker.clone(),
                text: ch.intro.clone(),
            });
            self.toast(
                format!("Neues Kapitel: {} – {}", ch.title, ch.task),
                ToastKind::Info,
            );
        }
        let done = match &ch.goal {
            ChapterGoal::Artifacts(n) => self.crew.artifacts.len() as u32 >= *n,
            ChapterGoal::Monument(id) => {
                self.world.monuments.iter().any(|m| &m.id == id && m.awake)
            }
            ChapterGoal::Scan { .. } => false,
        };
        if done {
            self.complete_chapter();
        }
    }

    fn complete_chapter(&mut self) {
        let i = self.story.chapter;
        let Some(ch) = self.data.story.chapters.get(i).cloned() else {
            return;
        };
        self.crew.credits += ch.reward;
        self.events.push(SimEvent::Story {
            speaker: ch.speaker.clone(),
            text: ch.outro.clone(),
        });
        self.toast(
            format!(
                "Kapitel abgeschlossen: {}  +{} Credits",
                ch.title, ch.reward
            ),
            ToastKind::Good,
        );
        self.find_log_by(|s| *s == LogSource::Chapter(i));
        self.story.chapter += 1;
        self.story.started = false;
        self.events.push(SimEvent::ChapterDone { chapter: i });
    }

    /// Jede halbe Sekunde: Kapitel beginnen lassen (Ruf und Aufträge ändern sich laufend).
    pub(crate) fn update_story(&mut self) {
        if self.tick.is_multiple_of(30) {
            self.update_chapter();
        }
    }
}
