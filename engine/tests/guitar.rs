//! The guitar (`proto/export.py --profile guitar`): Iowa MIS's classical guitar in first
//! position, its top's main mode measured (`proto/fit_body_modes.py`). Needs the export
//! (skipped without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/guitar");
const SR: f64 = 48000.0;

/// The left channel of one note held for `hold` samples, `n` samples long.
fn pluck(key: u8, hold: usize, n: usize) -> Vec<f32> {
    let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    let speed = e.hammer_speed(key, 100.0);
    e.queue(0, Event::NoteOn { key, speed });
    e.queue(hold as u64, Event::KeyUp { key });
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

/// The level (dB) of frequency `f` over the window from `t0` s, 0.1 s long (Goertzel).
fn level_at(x: &[f32], f: f64, t0: f64) -> f64 {
    let (a, n) = ((t0 * SR) as usize, (0.1 * SR) as usize);
    let w = 2.0 * std::f64::consts::PI * f / SR;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x[a..a + n].iter().enumerate() {
        let h = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
        re += h * v as f64 * (w * i as f64).cos();
        im -= h * v as f64 * (w * i as f64).sin();
    }
    10.0 * (re * re + im * im).log10()
}

#[test]
fn the_top_mode_takes_the_notes_near_it() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == dussek::mic::Body::Guitar);
    let b = &inst.globals.board;
    assert!(b.measured_from < b.f.len() && b.h_share > 0.0, "a measured mode and its horizontal share");
    let mode = b.f[b.measured_from];
    assert!((200.0..250.0).contains(&mode), "{mode}");
    // A♭3's fundamental (232 Hz, on the mode) falls much faster than B3's (245 Hz) and C4's
    // (the recordings: 53 against 20 and 19 dB/s; without the mode the three fall alike, 12 dB)
    let drop = |key: u8, f: f64| {
        let x = pluck(key, 48000, 48000);
        level_at(&x, f, 0.1) - level_at(&x, f, 0.5)
    };
    let f0 = |key: u8| inst.notes[key as usize - 21].spec.string.f0;
    let (on, above, c4) = (drop(58, f0(58)), drop(59, f0(59)), drop(60, f0(60)));
    assert!(on > 1.3 * above && on > 1.3 * c4, "{on} {above} {c4}");
}

#[test]
fn a_keys_release_is_the_hand_on_the_string() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    // no damper mechanism: the release noise has no impacts; the hand stops the note
    let (held, let_go) = (pluck(52, 48000, 48000), pluck(52, 9600, 48000));
    let e = |x: &[f32]| x[28800..].iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
    assert!(e(&let_go) < 1e-2 * e(&held), "{} {}", e(&let_go), e(&held));
    assert!(held.iter().all(|v| v.is_finite()));
    let _ = inst;
}
