//! Parcours in der offenen Welt (Punkte 28, 29, 74): Training mit Hinweisen und Zeitrennen
//! mit Medaillen und Bestenliste pro Spielstand. Ein Lauf beginnt, sobald das Schiff in
//! Pfeilrichtung durch das Starttor fliegt. Tore, Bojen und Messfelder sind nur Prüfungen auf
//! dem Schiffszustand – geflogen wird mit derselben Physik wie überall.

use bevy::math::Vec2;

use super::data::{CourseKind, CourseRecord, CrewSave, RecordEntry, StepKind, v};
use super::geom::{forward, segments_cross};
use super::world::angle_diff;
use super::{DT, SimEvent, SimState, ToastKind};

/// So viele Läufe stehen in der Bestenliste.
pub const BOARD_SIZE: usize = 5;
/// Ein Starttor startet nur, wenn die Flugrichtung dazu passt (Kosinus der Abweichung) –
/// sonst würde jeder Vorbeiflug einen Lauf beginnen.
const START_ALIGN: f32 = 0.85;
/// Höchstdauer eines Laufs.
const MAX_RUN: f32 = 900.0;
/// Ein gewählter, noch nicht begonnener Parcours verfällt so weit vom Starttor entfernt.
const ARMED_RANGE: f32 = 1800.0;
/// Stillhalten: höchstens so schnell drehen (rad/s).
pub const HOLD_MAX_SPIN: f32 = 0.5;

#[derive(Clone, Debug, PartialEq)]
pub struct CourseRun {
    pub course: usize,
    /// Nächster Schritt. 0 = Starttor: der Parcours ist gewählt, die Zeit läuft noch nicht.
    pub step: usize,
    pub time: f32,
    /// Fortschritt bei Halte-Schritten (Sekunden).
    pub progress: f32,
    /// Strafzeit bisher (Kollisionen, Präzisionsandocken).
    pub penalty: f32,
    pub collisions: u32,
    collisions_start: u32,
    /// Zwischenzeiten an jedem geschafften Schritt.
    pub splits: Vec<f32>,
    /// Präzisionsandocken: Versatz (m) und Aufsetzgeschwindigkeit (m/s).
    pub dock: Option<(f32, f32)>,
}

impl CourseRun {
    pub fn started(&self) -> bool {
        self.step > 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CourseResult {
    pub course: usize,
    /// Wievielter Abschluss dieses Parcours (zum Wiedererkennen in der Bestenliste).
    pub run: u32,
    pub time: f32,
    pub penalty: f32,
    pub total: f32,
    pub collisions: u32,
    /// Platz in der Bestenliste (0 = bester), falls eingetragen.
    pub rank: Option<usize>,
    /// Medaille dieses Laufs (0 keine, 1 Bronze, 2 Silber, 3 Gold) und ob sie neu ist.
    pub medal: u8,
    pub new_medal: bool,
    pub prize: u32,
    pub dock: Option<(f32, f32)>,
}

pub const MEDAL_NAMES: [&str; 4] = ["–", "Bronze", "Silber", "Gold"];

/// Zeit als „m:ss.s“.
pub fn fmt_time(t: f32) -> String {
    let t = t.max(0.0);
    let m = (t / 60.0).floor() as u32;
    format!("{m}:{:04.1}", t - m as f32 * 60.0)
}

impl SimState {
    /// Bestenlisten aus dem Spielstand übernehmen (eine pro Parcours, gleiche Reihenfolge).
    pub(crate) fn load_records(&mut self, save: &CrewSave) {
        self.records = self
            .data
            .courses
            .courses
            .iter()
            .map(|c| {
                save.records
                    .iter()
                    .find(|r| r.course == c.id)
                    .cloned()
                    .unwrap_or_else(|| CourseRecord {
                        course: c.id.clone(),
                        ..Default::default()
                    })
            })
            .collect();
    }

    pub(crate) fn save_records(&self) -> Vec<CourseRecord> {
        self.records
            .iter()
            .filter(|r| r.finished > 0)
            .cloned()
            .collect()
    }

    /// Plattformen, die für einen Andock-Schritt zählen.
    pub fn dock_pads(&self, station: &str, near: Option<(f32, f32)>) -> Vec<usize> {
        let Some(si) = self.data.station_index(station) else {
            return Vec::new();
        };
        let pads: Vec<usize> = self.world.stations[si]
            .pads
            .iter()
            .copied()
            .filter(|&p| self.world.pads[p].enabled)
            .collect();
        match near {
            None => pads,
            Some(at) => {
                let at = v(at);
                pads.into_iter()
                    .min_by(|a, b| {
                        (self.world.pads[*a].center - at)
                            .length()
                            .total_cmp(&(self.world.pads[*b].center - at).length())
                    })
                    .into_iter()
                    .collect()
            }
        }
    }

    /// Wo liegt ein Schritt? (Mitte von Tor, Boje, Feld oder Plattform)
    pub fn step_target(&self, course: usize, step: usize) -> Option<Vec2> {
        let st = self.data.courses.courses.get(course)?.steps.get(step)?;
        Some(match &st.kind {
            StepKind::Gate { pos, .. }
            | StepKind::Pass { pos, .. }
            | StepKind::Face { pos, .. }
            | StepKind::Hold { pos, .. } => v(*pos),
            StepKind::Dock { station, near } => {
                let pads = self.dock_pads(station, *near);
                match pads.as_slice() {
                    [one] => self.world.pads[*one].center,
                    _ => self.world.stations[self.data.station_index(station)?].pos,
                }
            }
        })
    }

    /// Kurzname eines Schritts für Wegmarken.
    pub fn step_label(&self, course: usize, step: usize) -> String {
        let c = &self.data.courses.courses[course];
        if step == 0 {
            return format!("Start: {}", c.name);
        }
        match c.steps.get(step).map(|s| &s.kind) {
            Some(StepKind::Gate { .. }) if step + 1 == c.steps.len() => "Ziel".into(),
            Some(StepKind::Gate { .. }) => format!("Tor {step}"),
            Some(StepKind::Pass { .. }) => "Durchflug".into(),
            Some(StepKind::Face { .. }) => "Boje".into(),
            Some(StepKind::Hold { .. }) => "Feld".into(),
            Some(StepKind::Dock { .. }) => "Plattform".into(),
            None => String::new(),
        }
    }

    /// Ziel des laufenden Parcours (für Navigation und Wegmarke).
    pub fn course_target(&self) -> Option<Vec2> {
        let r = self.course.as_ref()?;
        self.step_target(r.course, r.step)
    }

    /// Steht das Schiff ruhig in diesem Feld? (Messfelder und Halte-Schritte)
    pub fn holding_still(&self, pos: Vec2, radius: f32, max_speed: f32) -> bool {
        !self.ship.destroyed
            && self.ship.docked.is_none()
            && (self.ship.pos - pos).length() <= radius
            && self.ship.vel.length() <= max_speed
            && self.ship.ang_vel.abs() <= HOLD_MAX_SPIN
    }

    /// Ist das Schiff in diesem Tick in Pfeilrichtung durch das Tor geflogen?
    fn crossed_gate(&self, pos: Vec2, dir_deg: f32, width: f32, align: f32) -> bool {
        let dir = Vec2::new(dir_deg.to_radians().cos(), dir_deg.to_radians().sin());
        let side = Vec2::new(-dir.y, dir.x) * (width * 0.5);
        let (a, b) = (self.ship.prev_pos, self.ship.pos);
        if (b - a).dot(dir) <= 0.0 {
            return false;
        }
        let moving = (b - a).normalize_or_zero();
        moving.dot(dir) >= align && segments_cross(a, b, pos - side, pos + side)
    }

    fn collisions_total(&self) -> u32 {
        self.stats.slots.iter().map(|s| s.collisions).sum()
    }

    /// Parcours im Stationsmenü wählen: der Lauf beginnt am Starttor.
    pub(crate) fn arm_course(&mut self, course: usize) {
        if course >= self.data.courses.courses.len() {
            return;
        }
        self.course = Some(CourseRun {
            course,
            step: 0,
            time: 0.0,
            progress: 0.0,
            penalty: 0.0,
            collisions: 0,
            collisions_start: 0,
            splits: Vec::new(),
            dock: None,
        });
        let name = self.data.courses.courses[course].name.clone();
        self.toast(
            format!("{name}: Fliegt durch das Starttor – dann läuft die Zeit"),
            ToastKind::Info,
        );
    }

    fn begin_course(&mut self, course: usize) {
        let restart = self
            .course
            .as_ref()
            .is_some_and(|r| r.course == course && r.started());
        self.course = Some(CourseRun {
            course,
            step: 1,
            time: 0.0,
            progress: 0.0,
            penalty: 0.0,
            collisions: 0,
            collisions_start: self.collisions_total(),
            splits: Vec::new(),
            dock: None,
        });
        self.events.push(SimEvent::CourseStarted { course });
        let name = self.data.courses.courses[course].name.clone();
        let msg = if restart {
            format!("{name}: Neustart")
        } else {
            format!("{name}: Los!")
        };
        self.toast(msg, ToastKind::Info);
    }

    pub(crate) fn abort_course(&mut self, reason: &str) {
        if let Some(r) = self.course.take() {
            let name = self.data.courses.courses[r.course].name.clone();
            self.events
                .push(SimEvent::CourseAborted { course: r.course });
            self.toast(format!("{name} abgebrochen ({reason})"), ToastKind::Warn);
        }
    }

    /// Jeden Tick: Starttore prüfen, laufenden Schritt auswerten.
    pub(crate) fn update_course(&mut self) {
        if self.ship.destroyed {
            if self.course.as_ref().is_some_and(|r| r.started()) {
                self.abort_course("Schiff verloren");
            }
            return;
        }
        // Starttore: beginnt einen Lauf (auch einen Neustart des laufenden Parcours).
        if self.ship.docked.is_none() {
            let starts: Vec<usize> = (0..self.data.courses.courses.len())
                .filter(|&ci| {
                    let first = &self.data.courses.courses[ci].steps[0].kind;
                    matches!(first, StepKind::Gate { pos, dir, width }
                        if self.crossed_gate(v(*pos), *dir, *width, START_ALIGN))
                })
                .collect();
            let running = self
                .course
                .as_ref()
                .filter(|r| r.started())
                .map(|r| r.course);
            if let Some(&ci) = starts
                .iter()
                .find(|&&ci| running.is_none() || running == Some(ci))
            {
                self.begin_course(ci);
                return;
            }
        }
        let Some(mut run) = self.course.clone() else {
            return;
        };
        let course = self.data.courses.courses[run.course].clone();
        if !run.started() {
            if let Some(t) = self.step_target(run.course, 0)
                && (t - self.ship.pos).length() > ARMED_RANGE
            {
                self.course = None;
            }
            return;
        }
        run.time += DT;
        run.collisions = self.collisions_total().saturating_sub(run.collisions_start);
        let (per_m, per_speed) = self.data.courses.dock_penalty;
        run.penalty = run.collisions as f32 * self.data.courses.collision_penalty
            + run
                .dock
                .map_or(0.0, |(off, sp)| off * per_m + sp * per_speed);
        let Some(target) = self.step_target(run.course, run.step) else {
            self.course = None;
            return;
        };
        if run.time > MAX_RUN {
            self.abort_course("Zeit abgelaufen");
            return;
        }
        if (target - self.ship.pos).length() > self.data.courses.abort_distance {
            self.abort_course("zu weit vom Kurs");
            return;
        }
        let step = &course.steps[run.step].kind;
        if self.ship.docked.is_some() && !matches!(step, StepKind::Dock { .. }) {
            self.abort_course("angedockt");
            return;
        }
        let done = match step {
            StepKind::Gate { pos, dir, width } => self.crossed_gate(v(*pos), *dir, *width, 0.0),
            StepKind::Pass { pos, radius } => {
                // Auch bei hoher Geschwindigkeit: Abstand der Flugstrecke dieses Ticks.
                let (a, b) = (self.ship.prev_pos, self.ship.pos);
                let p = v(*pos);
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                (a + ab * t - p).length() <= *radius
            }
            StepKind::Face {
                pos,
                tolerance,
                seconds,
            } => {
                let to = v(*pos) - self.ship.pos;
                let want = f32::atan2(-to.x, to.y);
                let off = angle_diff(want, self.ship.angle).abs().to_degrees();
                let ok = off <= *tolerance && self.ship.ang_vel.abs() <= HOLD_MAX_SPIN;
                run.progress = if ok {
                    run.progress + DT
                } else {
                    (run.progress - 2.0 * DT).max(0.0)
                };
                run.progress >= *seconds
            }
            StepKind::Hold {
                pos,
                radius,
                seconds,
                max_speed,
            } => {
                run.progress = if self.holding_still(v(*pos), *radius, *max_speed) {
                    run.progress + DT
                } else {
                    (run.progress - 2.0 * DT).max(0.0)
                };
                run.progress >= *seconds
            }
            StepKind::Dock { station, near } => match self.ship.docked {
                Some(pad) if self.dock_pads(station, *near).contains(&pad) => {
                    // Gemessen im Tick des Aufsetzens, bevor das Schiff auf die Plattform
                    // gezogen wird; die Geschwindigkeit stammt aus dem Tick davor.
                    let p = &self.world.pads[pad];
                    let off = (self.ship.pos - p.center).dot(p.tangent()).abs();
                    let speed = self.prev_vel.length();
                    if course.kind == CourseKind::Race && near.is_some() {
                        run.dock = Some((off, speed));
                        run.penalty += off * per_m + speed * per_speed;
                    }
                    true
                }
                Some(_) => {
                    self.abort_course("falsche Plattform");
                    return;
                }
                None => false,
            },
        };
        if done {
            run.splits.push(run.time);
            run.step += 1;
            run.progress = 0.0;
            self.events.push(SimEvent::CourseCheckpoint {
                course: run.course,
                step: run.step,
            });
            if run.step >= course.steps.len() {
                self.finish_course(run);
                return;
            }
        }
        self.course = Some(run);
    }

    fn finish_course(&mut self, run: CourseRun) {
        self.course = None;
        let ci = run.course;
        let course = self.data.courses.courses[ci].clone();
        let total = run.time + run.penalty;
        let ship = self.data.ship(&self.crew.current_ship).name.clone();
        let rec = &mut self.records[ci];
        let first = rec.finished == 0;
        rec.finished += 1;
        let entry = RecordEntry {
            total,
            penalty: run.penalty,
            ship,
            crew: self.crew.size,
            run: rec.finished,
        };
        let pos = rec.entries.iter().position(|e| total < e.total);
        let rank = match pos {
            Some(i) => {
                rec.entries.insert(i, entry);
                Some(i)
            }
            None if rec.entries.len() < BOARD_SIZE => {
                rec.entries.push(entry);
                Some(rec.entries.len() - 1)
            }
            None => None,
        };
        rec.entries.truncate(BOARD_SIZE);
        let rank = rank.filter(|&r| r < BOARD_SIZE);
        let medal = match course.medals {
            Some((g, _, _)) if total <= g => 3,
            Some((_, s, _)) if total <= s => 2,
            Some((_, _, b)) if total <= b => 1,
            _ => 0,
        };
        let before = rec.medal;
        let new_medal = medal > before;
        rec.medal = rec.medal.max(medal);
        let (pb, ps, pg) = self.data.courses.medal_prizes;
        let mut prize = if first { course.prize } else { 0 };
        for level in (before + 1)..=medal {
            prize += match level {
                1 => pb,
                2 => ps,
                _ => pg,
            };
        }
        self.crew.credits += prize;
        self.events.push(SimEvent::CourseFinished { course: ci });
        let mut msg = format!("{} geschafft: {}", course.name, fmt_time(total));
        if rank == Some(0) {
            msg.push_str(" – Bestzeit!");
        }
        if prize > 0 {
            msg.push_str(&format!("  +{prize} Credits"));
        }
        self.toast(msg, ToastKind::Good);
        self.course_result = Some(CourseResult {
            course: ci,
            run: self.records[ci].finished,
            time: run.time,
            penalty: run.penalty,
            total,
            collisions: run.collisions,
            rank,
            medal,
            new_medal,
            prize,
            dock: run.dock,
        });
    }

    /// Fortschritt des laufenden Halte-Schritts (0..1), falls einer läuft.
    pub fn course_hold_fraction(&self) -> Option<f32> {
        let r = self.course.as_ref().filter(|r| r.started())?;
        let st = self.data.courses.courses[r.course].steps.get(r.step)?;
        match st.kind {
            StepKind::Face { seconds, .. } | StepKind::Hold { seconds, .. } => {
                Some((r.progress / seconds.max(0.01)).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }

    /// Nase des Schiffs (für Tests und Anzeige).
    pub fn ship_nose(&self) -> Vec2 {
        forward(self.ship.angle)
    }
}
