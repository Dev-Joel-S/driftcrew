//! Selbsttest der Rechnung: ein fester Lauf der Simulation mit festen Eingaben. Das Ergebnis
//! muss auf jedem Rechner (Windows, Linux, …) dieselben Bits haben – das ist der Sinn von
//! Fixed-Point. `driftcrew --selbsttest` führt ihn aus, die Tests prüfen ihn ebenfalls.

use std::sync::Arc;

use super::data::{CrewSave, GameData};
use super::fx::Vec2;
use super::ship::Loadout;
use super::{SimState, TickInput};

pub type Fingerprint = (i64, i64, i64, i64, i64, i64, i64);

/// Sollwert. Ändert sich Physik oder Rechenweise, ändert er sich mit – dann bewusst neu
/// eintragen (und prüfen, dass das Spielverhalten so gewollt ist).
pub const GOLDEN: Fingerprint = (
    1_366_123_725_371,
    2_831_209_837_792,
    2_806_226_402,
    27_451_265_636,
    -20_700_433_503,
    -85_045,
    417_563_592_640,
);

/// 900 Ticks mit festen Eingaben; liefert Ort, Tempo, Winkel, Drehung und Treibstoff als Rohbits.
pub fn golden_run() -> Fingerprint {
    let data = Arc::new(GameData::embedded().expect("Daten"));
    let save = CrewSave::new_game(&data);
    let def = data.ship(&save.current_ship).clone();
    let mut s = SimState::new(data, &save, Loadout::full(&def), 7);
    s.flight_assist = true;
    s.ship.docked = None;
    s.ship.pos = Vec2::new(fx!(300.0), fx!(500.0));
    s.ship.prev_pos = s.ship.pos;
    s.ship.vel = Vec2::new(fx!(1.5), fx!(-0.5));
    let all = (1u32 << s.ship.thrusters.len()) - 1;
    for tick in 0..900u32 {
        let slots = match tick {
            0..=119 => all,
            120..=179 => 0b01,
            300..=359 => 0b10,
            _ => 0,
        };
        s.step(&TickInput {
            slots,
            brake: (600..660).contains(&tick),
            ..Default::default()
        });
    }
    (
        s.ship.pos.x.raw(),
        s.ship.pos.y.raw(),
        s.ship.vel.x.raw(),
        s.ship.vel.y.raw(),
        s.ship.angle.raw(),
        s.ship.ang_vel.raw(),
        s.ship.fuel.raw(),
    )
}

/// Text für `--selbsttest`: gleich oder abweichend, dazu die Rohwerte zum Vergleichen.
pub fn report() -> (bool, String) {
    let got = golden_run();
    let ok = got == GOLDEN;
    let text = format!(
        "DriftCrew Selbsttest (Fixed-Point)\nErgebnis: {}\nIst:  {:?}\nSoll: {:?}\n",
        if ok {
            "OK – Bits stimmen mit dem Sollwert überein"
        } else {
            "ABWEICHUNG"
        },
        got,
        GOLDEN
    );
    (ok, text)
}
