//! Tests zum Fixed-Point-Umbau: alte Spielstände, Spielstand-Rundreise, Bit-Genauigkeit.

use std::sync::Arc;

use super::data::{CrewSave, GameData};
use super::ship::Loadout;
use super::{SimState, TickInput};

fn sim_from(save: &CrewSave) -> SimState {
    let data = Arc::new(GameData::embedded().unwrap());
    let def = data.ship(&save.current_ship).clone();
    SimState::new(data, save, Loadout::full(&def), 7)
}

#[test]
fn a_save_written_by_the_float_version_still_loads() {
    // Echter Spielstand aus der Zeit vor Fixed-Point: Dezimalzahlen wie `ore_sold: 0.0`.
    let text = include_str!("fixtures/save_before_fixed_point.ron");
    let save: CrewSave = ron::from_str(text).expect("alter Spielstand lässt sich lesen");
    assert_eq!(save.credits, 300);
    assert_eq!(save.current_ship, "driftkutter");
    let s = sim_from(&save);
    assert!(s.ship.docked.is_some());
}

#[test]
fn saves_round_trip_without_drift() {
    let data = Arc::new(GameData::embedded().unwrap());
    let mut save = CrewSave::new_game(&data);
    save.ore_sold = fx!(12.375);
    save.credits = 4321;
    let mut s = sim_from(&save);
    s.ship.fuel = fx!(61.25);
    s.ship.shield = fx!(7.5);
    for _ in 0..30 {
        s.step(&TickInput::default());
    }
    let out = s.to_save();
    let text = ron::ser::to_string_pretty(&out, ron::ser::PrettyConfig::default()).unwrap();
    let back: CrewSave = ron::from_str(&text).unwrap();
    assert_eq!(back.credits, 4321);
    assert_eq!(back.ore_sold, fx!(12.375));
    assert_eq!(back.shield, out.shield);
    assert_eq!(
        ron::ser::to_string(&back).unwrap(),
        ron::ser::to_string(&out).unwrap()
    );
    // Beim Laden und Weiterspielen stimmt alles überein.
    let t = sim_from(&back);
    assert_eq!(t.ship.shield, s.ship.shield);
}

#[test]
fn golden_run_is_bit_exact() {
    use super::selftest::{GOLDEN, golden_run};
    let got = golden_run();
    assert_eq!(got, GOLDEN, "Soll {GOLDEN:?}, ist {got:?}");
    // Zweiter Lauf: dieselben Bits.
    assert_eq!(golden_run(), got);
}
