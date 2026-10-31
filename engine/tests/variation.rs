//! Each strike slightly different (`Params::variation`): the hammer meets the strings at a slightly
//! other point, the unison's strings a few µs apart and with other shares of the felt. Grand D's two
//! recorded takes per key and dynamic differ like this: their partials around the fundamental
//! within about −20 dB, above 2–4 kHz unrelated (design log step 130).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};
use std::path::Path;

const SR: f64 = 48000.0;
const GAP: usize = 3 * 48000;
const WIN: usize = (0.2 * SR) as usize;

fn load(name: &str) -> Option<Instrument> {
    Instrument::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(name)).ok()
}

/// three strikes of `key` at velocity 80, 3 s apart (damped between), the room off: the second's and
/// the third's first 200 ms (each over its predecessor's damped remainder), left channel, concatenated
/// GAP apart
fn two_strikes(inst: Instrument, key: u8, variation: f32) -> Vec<f32> {
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, variation, ..Params::default() });
    let speed = e.hammer_speed(key, 80.0);
    for i in 0..3u64 {
        e.queue(i * GAP as u64, Event::NoteOn { key, speed });
        e.queue(i * GAP as u64 + (0.5 * SR) as u64, Event::KeyUp { key });
    }
    let n = 2 * GAP + WIN + 256;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l[GAP..].to_vec()
}

/// the second strike against the first over 200 ms: the residual after the best gain, dB re the
/// first, in [lo, hi) Hz
fn residual_db(x: &[f32], lo: f64, hi: f64) -> f64 {
    let n = 1 << 15;
    let spec = |s: &[f32]| {
        let fft = realfft::RealFftPlanner::<f64>::new().plan_fft_forward(n);
        let mut buf: Vec<f64> = s.iter().map(|v| *v as f64).collect();
        buf.resize(n, 0.0);
        let mut out = fft.make_output_vec();
        fft.process(&mut buf, &mut out).unwrap();
        out
    };
    let (a, b) = (spec(&x[..WIN]), spec(&x[GAP..GAP + WIN]));
    let g = a.iter().zip(&b).map(|(p, q)| (p * q.conj()).re).sum::<f64>() / b.iter().map(|q| q.norm_sqr()).sum::<f64>();
    let band = |k: &usize| (lo..hi).contains(&(*k as f64 * SR / n as f64));
    let e: f64 = (0..a.len()).filter(band).map(|k| (a[k] - b[k] * g).norm_sqr()).sum();
    let ea: f64 = (0..a.len()).filter(band).map(|k| a[k].norm_sqr()).sum();
    10.0 * (e.max(1e-300) / ea).log10()
}

fn peak_db(x: &[f32]) -> f64 {
    20.0 * x.iter().fold(0.0f32, |m, v| m.max(v.abs())).log10() as f64
}

#[test]
fn without_variation_the_strikes_are_the_same() {
    let Some(inst) = load("grand_d") else { return };
    // each over the same remainder of the one before: the same but for rounding
    let x = two_strikes(inst, 60, 0.0);
    let d = residual_db(&x, 20.0, 20000.0);
    assert!(d < -100.0, "{d:.1} dB");
}

#[test]
fn a_strike_varies_above_and_keeps_its_fundamental_and_level() {
    let Some(inst) = load("grand_d") else { return };
    for key in [48u8, 60, 72] {
        let x = two_strikes(inst.clone(), key, 1.0);
        let f0 = 440.0 * 2f64.powf((key as f64 - 69.0) / 12.0);
        let (fund, top) = (residual_db(&x, f0 / 1.4, f0 * 1.4), residual_db(&x, 2000.0, 8000.0));
        assert!(fund < -15.0, "key {key}: the fundamental's band {fund:.1} dB");
        assert!(top > fund + 8.0, "key {key}: 2–8 kHz {top:.1} dB, the fundamental's {fund:.1} dB");
        let (p1, p2) = (peak_db(&x[..WIN]), peak_db(&x[GAP..GAP + WIN]));
        assert!((p1 - p2).abs() < 1.0, "key {key}: peaks {p1:.2} / {p2:.2} dB");
    }
}

#[test]
fn the_default_is_none() {
    // the user heard the measured variation as more realistic but wants it per piece, and strikes
    // that are not always the same bother them (2026-10-19): off by default
    assert_eq!(Params::default().variation, 0.0);
}

#[test]
fn the_variation_is_deterministic() {
    let Some(inst) = load("grand_d") else { return };
    assert_eq!(two_strikes(inst.clone(), 60, 1.0), two_strikes(inst, 60, 1.0));
}
