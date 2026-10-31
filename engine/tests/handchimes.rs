//! The hand chimes (`proto/bars.py` "handchimes"): a tube's fundamental with its harmonics (the
//! fit's 2, 3, 5 …), the tube's own modes and the strike's short-lived ones (`strike`), the low
//! tubes' fundamental swelling after the strike (`rise`: two partners at its frequency), an assumed
//! clapper (one dynamic recorded), a damper (the ringer's shoulder) lifted by a held key or the
//! pedal. Needs the exported instrument (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/handchimes");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

fn rms(s: &[f32]) -> f64 {
    (s.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt()
}

#[test]
fn a_chime_rings_at_its_pitch_with_its_harmonics() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == pianoengine::mic::Body::HandChimes && !inst.motor);
    for key in [60u8, 72, 84, 96] {
        let st = &inst.notes[key as usize - 21].spec.string;
        let pitch = 440.0 * 2f64.powf((key as f64 - 69.0) / 12.0);
        let cents = 1200.0 * (st.f0 / pitch).log2();
        assert!(cents.abs() < 20.0, "key {key}: the fundamental {:.1} Hz, {cents:+.1} cents off", st.f0);
        let m = st.modes.as_ref().expect("a chime's modes");
        // the tracks first (the fundamental, the modes near 1.5 and 6), then the extras
        assert!(m.ratio[0] == 1.0 && m.ratio[1..3].iter().all(|&r| r > 1.3));
        assert!(m.ratio[3..].iter().all(|&r| r == 1.0 || r > 1.05), "key {key}: {:?}", m.ratio);
        // the fundamental outweighs the other tracks by far (the recordings: 13–50 dB)
        assert!(m.out[1..3].iter().all(|&o| o < 0.3 * m.out[0]), "key {key}: {:?}", m.out);
        // the swell's partners: −2 and +1 × the fundamental's weight
        let partners: Vec<f64> = m.ratio.iter().zip(&m.out).skip(1).filter(|(r, _)| **r == 1.0).map(|(_, o)| o / m.out[0]).collect();
        let near = |a: &[f64], b: &[f64]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(partners.is_empty() || near(&partners, &[-2.0, 1.0]), "key {key}: {partners:?}");
    }
}

#[test]
fn a_held_key_rings_and_the_damper_takes_it_at_the_release() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let level = |hold: u64, pedal: bool| -> f64 {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        if pedal {
            e.queue(0, Event::PedalDown);
        }
        e.queue(0, Event::NoteOn { key: 67, speed: 1.0 });
        e.queue(hold, Event::KeyUp { key: 67 });
        let x = render(&mut e, (3.0 * SR) as usize);
        20.0 * (rms(&x[(2.5 * SR) as usize..]) / rms(&x[(0.1 * SR) as usize..(0.3 * SR) as usize])).log10()
    };
    let (held, released, pedalled) = (level((3.0 * SR) as u64, false), level((0.4 * SR) as u64, false), level((0.4 * SR) as u64, true));
    assert!(held > -25.0 && pedalled > -25.0, "held {held:.1} dB, pedalled {pedalled:.1} dB after 2.5 s");
    assert!(released < held - 40.0, "released {released:.1} dB, held {held:.1} dB");
}

#[test]
fn a_harder_strike_is_louder() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let level = |speed: f64| -> f64 {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 72, speed });
        let x = render(&mut e, (0.5 * SR) as usize);
        20.0 * rms(&x).log10()
    };
    let (soft, loud) = (level(0.25), level(3.0));
    // the assumed clapper: 6 dB per doubling of the speed
    assert!((loud - soft - 21.6).abs() < 4.0, "pp → ff {:.1} dB", loud - soft);
}

/// The level (dB) at f of an 8 ms Hann window starting at sample i.
fn level_at(x: &[f32], i: usize, f: f64) -> f64 {
    let w = (0.008 * SR) as usize;
    let (mut re, mut im) = (0.0, 0.0);
    for k in 0..w {
        let h = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * k as f64 / w as f64).cos();
        let ph = 2.0 * std::f64::consts::PI * f * k as f64 / SR;
        re += h * x[i + k] as f64 * ph.cos();
        im -= h * x[i + k] as f64 * ph.sin();
    }
    10.0 * (re * re + im * im + 1e-30).log10()
}

fn strike(key: u8) -> Vec<f32> {
    let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
    e.queue(0, Event::NoteOn { key, speed: 1.0 });
    render(&mut e, SR as usize)
}

#[test]
fn a_low_tube_swells_after_the_strike_a_high_one_does_not() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let swell = |key: u8| -> f64 {
        let f = inst.notes[key as usize - 21].spec.string.f0;
        let x = strike(key);
        let on = x.iter().position(|v| v.abs() > 1e-4).unwrap();
        let top = (15..40).map(|j| level_at(&x, on + j * 96, f)).fold(f64::MIN, f64::max);
        top - level_at(&x, on, f)
    };
    // the recordings: C4 32 dB under the swell's top in its first 8 ms, D4 23, C5 38; C7 none
    for key in [60u8, 62, 64] {
        let s = swell(key);
        assert!(s > 15.0, "key {key}: the first 8 ms {s:.1} dB under the top");
    }
    let s = swell(96);
    assert!(s < 6.0, "key 96: {s:.1} dB");
}

#[test]
fn the_strike_rings_the_high_modes_briefly() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    // energy above 2 kHz: the first 30 ms over 300–600 ms (C4's recording: 33 dB; the tracks alone 10)
    let x = strike(60);
    let on = x.iter().position(|v| v.abs() > 1e-4).unwrap();
    let high = |a: f64, b: f64| -> f64 {
        let s = &x[on + (a * SR) as usize..on + (b * SR) as usize];
        // a 2 kHz highpass of order 4 (each stage the input less its one-pole lowpass): the
        // fundamental at 264 Hz 70 dB down
        let c = (-2.0 * std::f64::consts::PI * 2000.0 / SR).exp();
        let (mut lp, mut e) = ([0.0f64; 4], 0.0);
        for &v in s {
            let mut y = v as f64;
            for l in lp.iter_mut() {
                *l = (1.0 - c) * y + c * *l;
                y -= *l;
            }
            e += y * y;
        }
        10.0 * (e / s.len() as f64).log10()
    };
    let d = high(0.0, 0.03) - high(0.3, 0.6);
    assert!(d > 20.0, "the strike's high modes {d:.1} dB over their level at 0.3 s");
}
