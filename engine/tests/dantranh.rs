//! The đàn tranh (`proto/scale.py` "dantranh"): plucked strings at equal temperament (the
//! recordings' bridges lie −43 … +49 cents off), bent by the pitch wheel (the left hand pressing
//! behind the bridge: `Event::PitchBend` × `Params::bend_range`). Needs the exported instrument
//! (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/dantranh");
const HARP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/harp");
const SR: f64 = 48000.0;

fn engine(dir: &str, bend_range: f32) -> Option<Engine> {
    let inst = Instrument::load(dir).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, knock: 0.0, bend_range, ..Params::default() });
    Some(e)
}

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

/// The strongest component within ±300 cents of `f` (Hann-windowed DFT, 1-cent grid, then
/// 0.1), in cents re `f`.
fn peak_cents(x: &[f32], f: f64) -> f64 {
    let n = x.len();
    let w: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()).collect();
    let mag = |c: f64| {
        let om = 2.0 * std::f64::consts::PI * f * 2f64.powf(c / 1200.0) / SR;
        let (mut re, mut im) = (0.0, 0.0);
        for i in 0..n {
            let v = x[i] as f64 * w[i];
            re += v * (om * i as f64).cos();
            im -= v * (om * i as f64).sin();
        }
        re * re + im * im
    };
    let coarse = (-300..=300).map(|c| c as f64).max_by(|a, b| mag(*a).total_cmp(&mag(*b))).unwrap();
    (-10..=10).map(|d| coarse + d as f64 * 0.1).max_by(|a, b| mag(*a).total_cmp(&mag(*b))).unwrap()
}

fn pitch(key: u8) -> f64 {
    440.0 * 2f64.powf((key as f64 - 69.0) / 12.0)
}

#[test]
fn the_strings_play_at_equal_temperament_and_bend() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.bend && !inst.tangent);
    for key in [50u8, 59, 62, 75] {
        let f0 = inst.notes[key as usize - 21].spec.string.f0;
        assert!((1200.0 * (f0 / pitch(key)).log2()).abs() < 0.01, "key {key}: {f0:.2} Hz");
    }
}

#[test]
fn the_pitch_wheel_bends_a_sounding_note_by_the_range() {
    let Some(_) = engine(DIR, 2.0) else { return };
    let f = pitch(69);
    let run = |wheel: Option<f32>, range: f32, before: bool| -> f64 {
        let mut e = engine(DIR, range).unwrap();
        if let (Some(v), true) = (wheel, before) {
            e.queue(0, Event::PitchBend { value: v });
        }
        e.queue(0, Event::NoteOn { key: 69, speed: 1.0 });
        if let (Some(v), false) = (wheel, before) {
            e.queue(9600, Event::PitchBend { value: v });
        }
        let x = render(&mut e, 33600);
        assert!(x.iter().all(|v| v.is_finite()));
        peak_cents(&x[14400..33600], f)
    };
    let base = run(None, 2.0, false);
    let up = run(Some(1.0), 2.0, false) - base;
    let down = run(Some(-0.5), 2.0, false) - base;
    let wide = run(Some(1.0), 3.0, true) - base;
    assert!((up - 200.0).abs() < 3.0, "wheel up on a sounding note: {up:+.1} cents");
    assert!((down + 100.0).abs() < 3.0, "half down: {down:+.1} cents");
    // a note struck with the wheel already bent sounds bent (the board's reactive coupling
    // pulls a mode a few cents, differently at the bent pitch)
    assert!((wide - 300.0).abs() < 5.0, "range 3, bent before the strike: {wide:+.1} cents");
    let none = run(Some(1.0), 0.0, false) - base;
    assert!(none.abs() < 0.5, "range 0: {none:+.1} cents");
}

#[test]
fn other_instruments_ignore_the_pitch_wheel() {
    let Some(_) = engine(HARP, 2.0) else { return };
    let run = |wheel: bool| {
        let mut e = engine(HARP, 2.0).unwrap();
        if wheel {
            e.queue(0, Event::PitchBend { value: 1.0 });
        }
        e.queue(0, Event::NoteOn { key: 69, speed: 1.0 });
        render(&mut e, 12000)
    };
    assert_eq!(run(false), run(true));
}
