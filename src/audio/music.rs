//! Musik, beim Start synthetisiert: ein ruhiges Stück für Titel und Lobby und eine leise
//! Klangfläche für den Flug. Beide laufen nahtlos in Schleife (alle Töne werden zyklisch in
//! den Puffer geschrieben, das Echo läuft über das Ende hinaus wieder vorne weiter).

use std::f32::consts::TAU;

use crate::sim::rng::Rng;

pub const RATE: u32 = 22_050;

fn midi(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// Ton zyklisch in den Puffer addieren: weiche Fläche (mehrere verstimmte Sinus) oder
/// gezupfter Ton (Sinus mit Oberton und Abklingen).
fn add_note(buf: &mut [f32], start: f32, dur: f32, freq: f32, amp: f32, pad: bool) {
    let n = buf.len();
    let rate = RATE as f32;
    let (attack, release) = if pad { (1.2, 1.6) } else { (0.006, 0.0) };
    let total = if pad { dur + release } else { dur };
    let s0 = (start * rate) as usize;
    let len = (total * rate) as usize;
    for k in 0..len {
        let t = k as f32 / rate;
        let env = if pad {
            let a = (t / attack).min(1.0);
            let r = if t > dur {
                1.0 - (t - dur) / release
            } else {
                1.0
            };
            a * r.max(0.0)
        } else {
            (t / attack).min(1.0) * (-t / (dur * 0.35)).exp()
        };
        let v = if pad {
            (TAU * freq * t).sin()
                + 0.6 * (TAU * freq * 1.003 * t).sin()
                + 0.6 * (TAU * freq * 0.997 * t).sin()
                + 0.15 * (TAU * freq * 2.0 * t).sin()
        } else {
            (TAU * freq * t).sin() + 0.25 * (TAU * freq * 3.0 * t).sin() * (-t * 8.0).exp()
        };
        buf[(s0 + k) % n] += v * env * amp;
    }
}

/// Echo mit Rückkopplung, zyklisch (zweimal herum, damit der Anfang das Ende hört).
fn echo(buf: &mut [f32], delay: f32, feedback: f32) {
    let n = buf.len();
    let d = (delay * RATE as f32) as usize;
    let dry = buf.to_vec();
    let mut wet = vec![0.0f32; n];
    for _ in 0..2 {
        for i in 0..n {
            let j = (i + n - d % n) % n;
            wet[i] = (dry[j] + wet[j]) * feedback;
        }
    }
    for i in 0..n {
        buf[i] += wet[i];
    }
}

/// Weicher Tiefpass, zyklisch eingeschwungen.
fn lowpass(buf: &mut [f32], cutoff: f32) {
    let a = 1.0 - (-TAU * cutoff / RATE as f32).exp();
    let mut y = 0.0;
    for _ in 0..2 {
        for s in buf.iter_mut() {
            y += a * (*s - y);
        }
    }
    for s in buf.iter_mut() {
        y += a * (*s - y);
        *s = y;
    }
}

fn normalize(buf: &mut [f32], peak: f32) {
    let m = buf.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    for s in buf.iter_mut() {
        *s *= peak / m;
    }
}

/// Titel und Lobby: 76 BPM, Am – F – C – Em, je zwei Takte; Flächen, Bass und ein
/// gezupftes Arpeggio mit Echo. 25 s Schleife.
pub fn menu_track() -> Vec<f32> {
    let beat = 60.0 / 76.0;
    let bar = beat * 4.0;
    let chords: [[f32; 3]; 4] = [
        [57.0, 60.0, 64.0], // Am
        [53.0, 57.0, 60.0], // F
        [55.0, 60.0, 64.0], // C (Lage über G)
        [52.0, 55.0, 59.0], // Em
    ];
    let total = bar * 8.0;
    let mut pad = vec![0.0f32; (total * RATE as f32) as usize];
    let mut pluck = vec![0.0f32; pad.len()];
    for (ci, ch) in chords.iter().enumerate() {
        let start = ci as f32 * bar * 2.0;
        for &n in ch {
            add_note(&mut pad, start, bar * 2.0, midi(n), 0.09, true);
        }
        // Bass auf der Eins jedes Takts.
        for b in 0..2 {
            add_note(
                &mut pluck,
                start + b as f32 * bar,
                2.2,
                midi(ch[0] - 24.0),
                0.22,
                false,
            );
        }
        // Arpeggio in Achteln: Grundton, Terz, Quinte, Oktave …
        let pattern = [0, 1, 2, 1, 0, 2, 1, 2];
        for k in 0..16 {
            let n = ch[pattern[k % 8]]
                + if k % 8 == 6 {
                    12.0
                } else {
                    12.0 * (k / 8) as f32
                };
            let vel = if k % 4 == 0 { 0.12 } else { 0.07 };
            add_note(
                &mut pluck,
                start + k as f32 * beat * 0.5,
                0.9,
                midi(n),
                vel,
                false,
            );
        }
    }
    echo(&mut pluck, beat * 0.75, 0.35);
    let mut out: Vec<f32> = pad.iter().zip(&pluck).map(|(a, b)| a + b).collect();
    lowpass(&mut out, 3200.0);
    normalize(&mut out, 0.8);
    out
}

/// Flug: sehr leise Klangfläche (Dm9 – Bbmaj7 – Gm9 – A sus), dazwischen vereinzelte
/// Glockentöne aus der Pentatonik mit langem Echo. 48 s Schleife.
pub fn flight_track() -> Vec<f32> {
    let seg = 12.0;
    let chords: [[f32; 4]; 4] = [
        [50.0, 57.0, 60.0, 64.0], // Dm9 (ohne Terz unten)
        [46.0, 53.0, 57.0, 62.0], // Bbmaj7
        [43.0, 50.0, 58.0, 62.0], // Gm9
        [45.0, 52.0, 57.0, 59.0], // Asus2
    ];
    let total = seg * 4.0;
    let mut pad = vec![0.0f32; (total * RATE as f32) as usize];
    let mut bells = vec![0.0f32; pad.len()];
    for (ci, ch) in chords.iter().enumerate() {
        for &n in ch {
            add_note(&mut pad, ci as f32 * seg, seg, midi(n), 0.07, true);
        }
    }
    let scale = [62.0, 64.0, 67.0, 69.0, 72.0, 74.0, 76.0, 79.0];
    let mut rng = Rng::new(4242);
    let mut t = 1.5;
    while t < total - 0.5 {
        let n = scale[rng.index(scale.len())];
        add_note(&mut bells, t, 2.6, midi(n), 0.05, false);
        t += rng.range(2.2, 5.5);
    }
    echo(&mut bells, 0.9, 0.45);
    let mut out: Vec<f32> = pad.iter().zip(&bells).map(|(a, b)| a + b).collect();
    lowpass(&mut out, 1800.0);
    normalize(&mut out, 0.7);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_loop_without_a_click() {
        for t in [menu_track(), flight_track()] {
            assert!(t.len() > RATE as usize * 20);
            assert!(t.iter().all(|v| v.is_finite() && v.abs() <= 0.81));
            // Übergang Ende → Anfang ist nicht größer als ein normaler Schritt.
            let jump = (t[0] - t[t.len() - 1]).abs();
            let step = t
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0f32, f32::max);
            assert!(
                jump <= step * 1.5 + 1e-3,
                "Sprung {jump}, größter Schritt {step}"
            );
            // Nicht stumm.
            let rms = (t.iter().map(|v| v * v).sum::<f32>() / t.len() as f32).sqrt();
            assert!(rms > 0.03, "rms {rms}");
        }
    }
}
