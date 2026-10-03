//! Klang: alle Geräusche werden beim Start synthetisiert (keine fremden Dateien).

use std::f32::consts::TAU;
use std::sync::Arc;

use bevy::audio::{AudioSinkPlayback, PlaybackMode, Volume};
use bevy::prelude::*;

use crate::game::{AppState, Paused, Sim, SimMsg};
use crate::sim::SimEvent;
use crate::sim::rng::Rng;

const RATE: u32 = 44_100;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_sounds)
            .add_systems(
                Update,
                (play_event_sounds, update_loops).run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), mute_loops);
    }
}

#[derive(Resource)]
pub struct Sounds {
    shot: Handle<AudioSource>,
    explosion: Handle<AudioSource>,
    impact: Handle<AudioSource>,
    dock: Handle<AudioSource>,
    coin: Handle<AudioSource>,
    crane: Handle<AudioSource>,
    clank: Handle<AudioSource>,
    alarm: Handle<AudioSource>,
    blip: Handle<AudioSource>,
    click: Handle<AudioSource>,
}

#[derive(Component)]
struct ThrustLoop;

#[derive(Component)]
struct DrillLoop;

fn wav(samples: &[f32]) -> AudioSource {
    let mut b: Vec<u8> = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = (samples.len() * 2) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32000.0) as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    AudioSource {
        bytes: Arc::from(b),
    }
}

fn len(sec: f32) -> usize {
    (sec * RATE as f32) as usize
}

fn env(t: f32, attack: f32, decay: f32) -> f32 {
    if t < attack {
        t / attack
    } else {
        (-(t - attack) / decay).exp()
    }
}

/// Einfacher Tiefpass (eine Polstelle).
fn lowpass(x: &mut [f32], cutoff: f32) {
    let a = 1.0 - (-TAU * cutoff / RATE as f32).exp();
    let mut y = 0.0;
    for _ in 0..2 {
        for s in x.iter_mut() {
            y += a * (*s - y);
            *s = y;
        }
    }
}

fn synth_thrust(rng: &mut Rng) -> Vec<f32> {
    let n = len(2.0);
    let mut low: Vec<f32> = (0..n).map(|_| rng.range(-1.0, 1.0)).collect();
    let mut hiss = low.clone();
    lowpass(&mut low, 140.0);
    lowpass(&mut hiss, 2200.0);
    // Nahtlos schleifen: Anfang und Ende überblenden.
    let fade = len(0.15);
    let mut out: Vec<f32> = (0..n).map(|i| low[i] * 3.2 + hiss[i] * 0.35).collect();
    for i in 0..fade {
        let t = i as f32 / fade as f32;
        out[i] = out[i] * t + out[n - fade + i] * (1.0 - t);
    }
    out.truncate(n - fade);
    out
}

fn synth_drill(rng: &mut Rng) -> Vec<f32> {
    let n = len(1.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let saw = ((t * 110.0).fract() * 2.0 - 1.0) * 0.35;
            let buzz = (TAU * 55.0 * t).sin() * 0.3 * (1.0 + (TAU * 22.0 * t).sin()) * 0.5;
            saw + buzz + rng.range(-0.12, 0.12)
        })
        .collect()
}

fn synth_shot(rng: &mut Rng) -> Vec<f32> {
    let n = len(0.22);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 1300.0 * (1.0 - t / 0.22).powi(2) + 150.0;
            phase += f / RATE as f32;
            let sq = if phase.fract() < 0.5 { 1.0 } else { -1.0 };
            (sq * 0.35 + rng.range(-0.3, 0.3) * (1.0 - t * 8.0).max(0.0)) * env(t, 0.003, 0.07)
        })
        .collect()
}

fn synth_explosion(rng: &mut Rng) -> Vec<f32> {
    let n = len(1.3);
    let mut noise: Vec<f32> = (0..n).map(|_| rng.range(-1.0, 1.0)).collect();
    lowpass(&mut noise, 900.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let thump = (TAU * (70.0 - t * 30.0) * t).sin() * env(t, 0.005, 0.25);
            (noise[i] * 2.4 * env(t, 0.004, 0.35) + thump * 0.8).clamp(-1.0, 1.0)
        })
        .collect()
}

fn synth_impact(rng: &mut Rng) -> Vec<f32> {
    let n = len(0.3);
    let mut noise: Vec<f32> = (0..n).map(|_| rng.range(-1.0, 1.0)).collect();
    lowpass(&mut noise, 600.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 95.0 - t * 120.0;
            ((TAU * f * t).sin() * 0.9 + noise[i] * 1.5) * env(t, 0.002, 0.07)
        })
        .collect()
}

fn tones(notes: &[(f32, f32, f32)], total: f32) -> Vec<f32> {
    let n = len(total);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            notes
                .iter()
                .map(|(start, f, dec)| {
                    if t < *start {
                        0.0
                    } else {
                        let tt = t - start;
                        ((TAU * f * tt).sin() + 0.3 * (TAU * f * 2.0 * tt).sin())
                            * env(tt, 0.004, *dec)
                            * 0.35
                    }
                })
                .sum()
        })
        .collect()
}

fn synth_crane() -> Vec<f32> {
    let n = len(0.2);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            phase += (200.0 + t * 3500.0) / RATE as f32;
            (phase.fract() * 2.0 - 1.0) * 0.25 * (1.0 - t / 0.2)
        })
        .collect()
}

fn synth_clank() -> Vec<f32> {
    tones(
        &[(0.0, 523.0, 0.08), (0.0, 1307.0, 0.05), (0.0, 2091.0, 0.04)],
        0.3,
    )
}

fn synth_alarm() -> Vec<f32> {
    let n = len(0.4);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let on = (t < 0.12) || (t > 0.2 && t < 0.32);
            if on {
                (if (t * 440.0).fract() < 0.5 { 0.3 } else { -0.3 }) * 0.8
            } else {
                0.0
            }
        })
        .collect()
}

fn setup_sounds(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut rng = Rng::new(31337);
    let thrust = sources.add(wav(&synth_thrust(&mut rng)));
    let drill = sources.add(wav(&synth_drill(&mut rng)));
    let s = Sounds {
        shot: sources.add(wav(&synth_shot(&mut rng))),
        explosion: sources.add(wav(&synth_explosion(&mut rng))),
        impact: sources.add(wav(&synth_impact(&mut rng))),
        dock: sources.add(wav(&tones(&[(0.0, 660.0, 0.25), (0.14, 990.0, 0.35)], 0.8))),
        coin: sources.add(wav(&tones(
            &[
                (0.0, 880.0, 0.08),
                (0.07, 1320.0, 0.08),
                (0.14, 1760.0, 0.18),
            ],
            0.5,
        ))),
        crane: sources.add(wav(&synth_crane())),
        clank: sources.add(wav(&synth_clank())),
        alarm: sources.add(wav(&synth_alarm())),
        blip: sources.add(wav(&tones(
            &[(0.0, 1200.0, 0.05), (0.08, 1600.0, 0.06)],
            0.25,
        ))),
        click: sources.add(wav(&tones(&[(0.0, 300.0, 0.02)], 0.08))),
    };
    commands.insert_resource(s);
    commands.spawn((
        AudioPlayer::new(thrust),
        PlaybackSettings {
            mode: PlaybackMode::Loop,
            volume: Volume::Linear(0.0),
            ..PlaybackSettings::LOOP
        },
        ThrustLoop,
    ));
    commands.spawn((
        AudioPlayer::new(drill),
        PlaybackSettings {
            mode: PlaybackMode::Loop,
            volume: Volume::Linear(0.0),
            ..PlaybackSettings::LOOP
        },
        DrillLoop,
    ));
}

fn one_shot(commands: &mut Commands, h: &Handle<AudioSource>, vol: f32) {
    if vol <= 0.01 {
        return;
    }
    commands.spawn((
        AudioPlayer::new(h.clone()),
        PlaybackSettings {
            volume: Volume::Linear(vol),
            ..PlaybackSettings::DESPAWN
        },
    ));
}

fn play_event_sounds(
    mut commands: Commands,
    mut events: MessageReader<SimMsg>,
    sounds: Option<Res<Sounds>>,
    sim: Res<Sim>,
) {
    let Some(s) = sounds else { return };
    let me = sim.0.ship.pos;
    let by_dist = |p: Vec2| (1.0 - (p - me).length() / 160.0).clamp(0.0, 1.0);
    let mut impacts = 0;
    for SimMsg(e) in events.read() {
        match e {
            SimEvent::Shot { .. } => one_shot(&mut commands, &s.shot, 0.5),
            SimEvent::EmptyGun => one_shot(&mut commands, &s.click, 0.6),
            SimEvent::Explosion { pos, size, .. } => one_shot(
                &mut commands,
                &s.explosion,
                by_dist(*pos) * (0.3 + size * 0.08).min(0.9),
            ),
            SimEvent::Impact { pos, strength, .. } if impacts < 2 => {
                impacts += 1;
                one_shot(
                    &mut commands,
                    &s.impact,
                    by_dist(*pos) * (strength / 10.0).min(0.9),
                );
            }
            SimEvent::ProjectileHit { pos } => {
                one_shot(&mut commands, &s.impact, by_dist(*pos) * 0.3)
            }
            SimEvent::Docked { .. } => one_shot(&mut commands, &s.dock, 0.5),
            SimEvent::Ping { .. } => one_shot(&mut commands, &s.blip, 0.6),
            SimEvent::Purchased { .. }
            | SimEvent::MissionCompleted { .. }
            | SimEvent::Sold { .. } => one_shot(&mut commands, &s.coin, 0.5),
            SimEvent::CraneFire => one_shot(&mut commands, &s.crane, 0.35),
            SimEvent::CraneAttach { .. } | SimEvent::Stowed { .. } => {
                one_shot(&mut commands, &s.clank, 0.5)
            }
            SimEvent::VoteStarted | SimEvent::MissionAccepted { .. } => {
                one_shot(&mut commands, &s.blip, 0.4)
            }
            // Minispiele: Takt getroffen klickt hell, daneben dumpf; geflickt und Port offen klingen.
            SimEvent::Beat { hit: true } => one_shot(&mut commands, &s.blip, 0.35),
            SimEvent::Beat { hit: false } => one_shot(&mut commands, &s.click, 0.4),
            SimEvent::Patched { .. } | SimEvent::HackDone => one_shot(&mut commands, &s.clank, 0.6),
            SimEvent::HackStarted => one_shot(&mut commands, &s.blip, 0.5),
            SimEvent::Toast {
                kind: crate::sim::ToastKind::Bad,
                ..
            } => one_shot(&mut commands, &s.alarm, 0.4),
            _ => {}
        }
    }
}

fn update_loops(
    sim: Res<Sim>,
    paused: Res<Paused>,
    mut thrust: Query<&mut AudioSink, (With<ThrustLoop>, Without<DrillLoop>)>,
    mut drill: Query<&mut AudioSink, (With<DrillLoop>, Without<ThrustLoop>)>,
) {
    let ship = &sim.0.ship;
    let level: f32 = ship.thrusters.iter().map(|t| t.level).sum();
    let tv = if paused.0 || ship.destroyed {
        0.0
    } else {
        (level * 0.22).min(0.8)
    };
    for mut s in &mut thrust {
        s.set_volume(Volume::Linear(tv));
    }
    let drilling = ship.tools.iter().any(|t| t.drill.is_some());
    for mut s in &mut drill {
        s.set_volume(Volume::Linear(if drilling && !paused.0 {
            0.3
        } else {
            0.0
        }));
    }
}

fn mute_loops(mut q: Query<&mut AudioSink>) {
    for mut s in &mut q {
        s.set_volume(Volume::Linear(0.0));
    }
}
