//! Velocity to level on a hammer piano: the peak level follows one law over the velocity
//! (`velocity::dynamics_db`), the Dynamics control sets its range, and the recording's ff stays
//! at its own velocity and speed, on every key. Grand D and Grand F rose +7 dB from velocity 32 to
//! 64 and +15 dB from 64 to 112 with the recordings' pp/mf/ff placed at fixed velocities; Grand F's
//! C6 played 4–10 dB under its middle's steps below velocity 64.
mod common;

use dussek::instrument::LEVEL_KEYS;
use dussek::velocity::{DYNAMICS_DB, FF_VELOCITY, dynamics_db};
use dussek::{Engine, Event, Instrument, Params, RadiationCache, Tuning};

const SR: f64 = 48000.0;

fn dir(name: &str) -> String {
    format!("{}/../instruments/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// The peak (dB) over 1 s of `key` played at `velocity` with the Dynamics control at `range`.
fn peak_db(name: &str, key: u8, velocity: f64, range: f32) -> Option<f64> {
    Some(played_db(common::engine(&dir(name), SR, Tuning::default())?, key, velocity, range))
}

fn played_db(mut e: Engine, key: u8, velocity: f64, range: f32) -> f64 {
    // the strings alone, as the level table is measured (the thump has its own growth with the speed:
    // it put Grand F's C4 at ff 2.1 dB over its strings once its board's low end was trimmed)
    e.set_params(Params { dynamics: range, knock: 0.0, ..Params::default() });
    let speed = e.hammer_speed(key, velocity);
    e.queue(0, Event::NoteOn { key, speed });
    let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
    let mut peak = 0.0f32;
    for _ in 0..(SR / 256.0) as usize {
        e.process(&mut l, &mut r);
        peak = l.iter().chain(&r).fold(peak, |m, x| m.max(x.abs()));
    }
    20.0 * (peak as f64).log10()
}

/// The level at each of `velocities` re FF_VELOCITY, the mean over `keys` (dB).
fn steps(name: &str, keys: &[u8], velocities: &[f64], range: f32) -> Option<Vec<f64>> {
    let cases: Vec<(u8, f64)> = velocities.iter().chain(&[FF_VELOCITY]).flat_map(|&v| keys.iter().map(move |&k| (k, v))).collect();
    let all: Vec<f64> = common::par(cases.len(), |i| peak_db(name, cases[i].0, cases[i].1, range)).into_iter().collect::<Option<_>>()?;
    Some(re_ff(&all, keys.len(), velocities.len()))
}

/// Per velocity (then FF_VELOCITY), `n` keys' levels: the mean re the ff one's.
fn re_ff(all: &[f64], n: usize, velocities: usize) -> Vec<f64> {
    let mean = |j: usize| all[j * n..(j + 1) * n].iter().sum::<f64>() / n as f64;
    let ff = mean(velocities);
    (0..velocities).map(|j| mean(j) - ff).collect()
}

const VS: [f64; 4] = [32.0, 64.0, 96.0, 112.0];

fn check(name: &str, range: f32) {
    check_keys(name, &LEVEL_KEYS, range);
}

fn check_keys(name: &str, keys: &[u8], range: f32) {
    for &k in keys {
        let Some(got) = steps(name, &[k], &VS, range) else { return };
        compare(&format!("{name} key {k}"), range, &got);
    }
}

fn compare(name: &str, range: f32, got: &[f64]) {
    let vs = VS;
    for (v, g) in vs.iter().zip(got) {
        let want = dynamics_db(*v, range as f64) - dynamics_db(FF_VELOCITY, range as f64);
        eprintln!("{name} at {range} dB: velocity {v}: {g:+.1} dB, the law {want:+.1}");
        assert!((g - want).abs() < 1.5, "{name} at {range} dB: velocity {v} plays {g:+.1} dB re ff, the law {want:+.1}");
    }
}

#[test]
fn grand_e_s_steps_follow_the_law() {
    check("grand_d", DYNAMICS_DB as f32);
}

#[test]
fn grand_c_s_steps_follow_the_law() {
    check("grand_f", DYNAMICS_DB as f32);
}

#[test]
fn the_bass_and_the_treble_follow_it_too() {
    // the register's own curve (Grand F's upper middle is the steepest) and a key's own scatter
    check_keys("grand_f", &[24, 36, 84, 100], DYNAMICS_DB as f32);
    check_keys("grand_a", &[36, 84], DYNAMICS_DB as f32);
}

#[test]
fn the_dynamics_control_sets_the_range() {
    check("grand_d", 30.0);
    check("grand_d", 60.0);
}

#[test]
fn the_ff_stays_at_the_recording_s_speed() {
    for range in [30.0, 60.0] {
        let Some(mut e) = common::engine(&dir("grand_d"), SR, Tuning::default()) else { return };
        e.set_params(Params { dynamics: range, ..Params::default() });
        let ff = common::load(&dir("grand_d")).unwrap().velocity_speed[2];
        let s = e.hammer_speed(60, FF_VELOCITY);
        assert!((s / ff - 1.0).abs() < 0.01, "at {range} dB: {s:.3} m/s, the recording's ff {ff:.3}");
    }
}

#[test]
fn an_instrument_without_a_level_table_keeps_its_anchors() {
    // the harpsichord: plucked, no table in its stored level
    let Some(e) = common::engine(&dir("harpsichord"), SR, Tuning::default()) else { return };
    let inst = common::load(&dir("harpsichord")).unwrap();
    assert!(inst.dynamics.is_empty());
    let s = e.hammer_speed(60, inst.velocity_midi[1]);
    assert!((s / inst.velocity_speed[1] - 1.0).abs() < 1e-9, "{s} m/s at the mf anchor, not {}", inst.velocity_speed[1]);
}

#[test]
fn a_morph_s_steps_follow_the_law() {
    // its table blends its sides' (no render of its own)
    let (a, b) = (dir("grand_d"), dir("grand_a"));
    let Ok(inst) = Instrument::morph(&[(&a, 0.5), (&b, 0.5)]) else { return };
    assert!(!inst.dynamics.is_empty(), "the morph has no level table");
    let range = DYNAMICS_DB as f32;
    let cases: Vec<(u8, f64)> = VS.iter().chain(&[FF_VELOCITY]).flat_map(|&v| LEVEL_KEYS.map(|k| (k, v))).collect();
    let all = common::par(cases.len(), |i| {
        let e = Engine::with_cache(inst.clone(), SR, Tuning::default(), &mut RadiationCache::default()).unwrap();
        played_db(e, cases[i].0, cases[i].1, range)
    });
    compare("grand_d + grand_a", range, &re_ff(&all, LEVEL_KEYS.len(), VS.len()));
}

#[test]
fn the_speed_rises_with_the_velocity_everywhere() {
    // past the table's ends too (Osiris spans 37 dB in its table, 70 dB asks for 69 dB), on every
    // piano, key and range: finite, positive, rising
    let pianos = ["grand_a", "grand_d", "grand_f", "grand_g", "upright_c"];
    let bad: Vec<String> = common::par(pianos.len(), |i| {
        let Some(mut e) = common::engine(&dir(pianos[i]), SR, Tuning::default()) else { return Vec::new() };
        let mut bad = Vec::new();
        for range in [10.0, 45.0, 70.0] {
            e.set_params(Params { dynamics: range, ..Params::default() });
            for key in 21..=108u8 {
                let s: Vec<f64> = (1..=127).map(|v| e.hammer_speed(key, v as f64)).collect();
                if s.iter().any(|x| !(x.is_finite() && *x > 0.0)) || s.windows(2).any(|w| w[1] <= w[0]) {
                    bad.push(format!("{} key {key} at {range} dB", pianos[i]));
                }
            }
        }
        bad
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
