//! The balafon (`proto/bars.py` "balafon"): wooden bars with fast modes, the gourds' buzz as the
//! fundamental's harmonics up to the 7th and the membranes' noise (`Instrument::membrane`), three
//! mallets (Params::mallet), no dampers. Needs the exported instrument (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/balafon");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

#[test]
fn a_bar_buzzes_with_its_harmonics_and_dies_within_a_second() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == pianoengine::mic::Body::Balafon && !inst.motor);
    let names: Vec<&str> = inst.mallets.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["traditional", "hard", "soft"]);
    for key in [61u8, 72, 89] {
        let n = &inst.notes[key as usize - 21];
        assert!(!n.spec.has_damper);
        let m = n.spec.string.modes.as_ref().expect("a bar's modes");
        // the buzz: harmonics up to the 7th
        assert!(m.harmonics.h.iter().any(|&h| h >= 5), "key {key}: harmonics {:?}", m.harmonics.h);
        // the wood's fast fundamental: under 1.5 s to −60 dB
        assert!(60.0 / (8.686 * m.sigma[0]) < 1.5, "key {key}: σ {:.1}", m.sigma[0]);
    }
}

#[test]
fn the_mallets_differ() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let run = |mallet: u32| {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, mallet, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 72, speed: 1.0 });
        render(&mut e, (0.3 * SR) as usize)
    };
    let (a, b, c) = (run(0), run(1), run(2));
    assert!(a != b && b != c && a != c);
    // the hard mallet is the brightest: the high band's share (a first difference) is larger
    let hf = |x: &[f32]| x.windows(2).map(|w| ((w[1] - w[0]) as f64).powi(2)).sum::<f64>() / x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
    assert!(hf(&b) > hf(&a) && hf(&b) > hf(&c), "brightness: traditional {:.4}, hard {:.4}, soft {:.4}", hf(&a), hf(&b), hf(&c));
}

#[test]
fn the_membranes_buzz_between_the_lines() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.membrane.is_some_and(|m| m.level > 0.0));
    let run = |membrane: bool| {
        let mut inst = Instrument::load(DIR).unwrap();
        if !membrane {
            inst.membrane = None;
        }
        let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 72, speed: 1.0 });
        render(&mut e, (0.3 * SR) as usize)
    };
    let (with, without) = (run(true), run(false));
    // between C5's second and third harmonics (1047, 1570 Hz; the second mode at 1635): the
    // membrane's noise fills it, without it holds the lines' skirts alone (Hann-windowed)
    let band = |x: &[f32]| {
        let x = &x[x.len() / 6..];
        let n = x.len();
        let w: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()).collect();
        let mut p = 0.0;
        for f in (1150..1450).step_by(10) {
            let om = 2.0 * std::f64::consts::PI * f as f64 / SR;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &v) in x.iter().enumerate() {
                re += v as f64 * w[i] * (om * i as f64).cos();
                im -= v as f64 * w[i] * (om * i as f64).sin();
            }
            p += re * re + im * im;
        }
        p
    };
    let gain = 10.0 * (band(&with) / band(&without)).log10();
    assert!(gain > 10.0, "the membrane adds {gain:.1} dB between the lines");
}

/// Power (Hann-windowed) at the frequencies `fs` of x[a..b] (seconds), summed.
fn power(x: &[f32], a: f64, b: f64, fs: impl Iterator<Item = f64>) -> f64 {
    let x = &x[(a * SR) as usize..(b * SR) as usize];
    let n = x.len();
    let w: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()).collect();
    fs.map(|f| {
        let om = 2.0 * std::f64::consts::PI * f / SR;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &v) in x.iter().enumerate() {
            re += v as f64 * w[i] * (om * i as f64).cos();
            im -= v as f64 * w[i] * (om * i as f64).sin();
        }
        re * re + im * im
    })
    .sum()
}

fn strike(key: u8, speed: f64) -> Vec<f32> {
    let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    e.queue(0, Event::NoteOn { key, speed });
    render(&mut e, (0.4 * SR) as usize)
}

#[test]
fn a_harder_strike_buzzes_more() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    // C5: the noise between its 4th and 5th harmonics (2093, 2616 Hz; the third mode at 3033) re
    // the fundamental; the recordings' 2–4 kHz noise re it grows 12 dB from pp to ff
    let f1 = inst.notes[72 - 21].spec.string.f0;
    let share = |speed: f64| {
        let x = strike(72, speed);
        10.0 * (power(&x, 0.02, 0.2, (2200..2500).step_by(20).map(|f| f as f64)) / power(&x, 0.02, 0.2, std::iter::once(f1))).log10()
    };
    let (soft, loud) = (share(inst.velocity_speed[0]), share(inst.velocity_speed[2]));
    assert!(loud - soft > 5.0, "the buzz re the fundamental: pp {soft:.1} dB, ff {loud:.1} dB");
}

#[test]
fn the_upper_modes_ring_under_the_fundamental() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    // C5 at mf: its third mode (5.8 f1) 10–50 ms after the strike lies 13–20 dB under the
    // fundamental in the recordings (the decay fit's level, a spike's, put it at the fundamental's)
    let st = &inst.notes[72 - 21].spec.string;
    let m = st.modes.as_ref().unwrap();
    let x = strike(72, 1.0);
    let at = |r: f64| power(&x, 0.01, 0.05, std::iter::once(r * st.f0));
    let d = 10.0 * (at(m.ratio[2]) / at(1.0)).log10();
    assert!(d < -8.0, "the third mode {d:.1} dB re the fundamental");
}

#[test]
fn a_strike_starts_from_zero() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    // the mallet's contact: no step at the strike (the modes released all at once stepped to a
    // quarter of the strike's peak in one sample, a click), its first 0.2 ms under a fifth of it
    for key in [61u8, 72, 84] {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, knock: 0.0, ..Params::default() });
        e.queue(0, Event::NoteOn { key, speed: 1.0 });
        let x = render(&mut e, (0.06 * SR) as usize);
        let peak = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        let on = x.iter().position(|v| v.abs() > 1e-6 * peak).unwrap();
        let first = x[on..on + (0.0002 * SR) as usize].iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(first < 0.2 * peak, "key {key}: {:.2} of the peak in the first 0.2 ms", first / peak);
    }
}

#[test]
fn each_mallet_clicks_on_the_wood_its_own_way() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let k: Vec<f64> = inst.mallets.iter().map(|m| m.knock).collect();
    // the contact's noise re the note: traditional −18 dB, hard −24, soft −31 in the recordings
    assert!(inst.knock.is_some() && k[0] == 1.0 && k[1] < k[0] && k[2] < k[1], "{k:?}");
}

#[test]
fn the_buzz_follows_the_sympathetic_control() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let f1 = inst.notes[72 - 21].spec.string.f0;
    let run = |sympathetic: f32| {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, knock: 0.0, sympathetic, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 72, speed: 1.0 });
        render(&mut e, (0.4 * SR) as usize)
    };
    let (on, half) = (run(1.0), run(0.5));
    // C5: its 2nd harmonic and the noise between the 4th and 5th, both 6 dB down at half; the fundamental alike
    let db = |x: &[f32], fs: &mut dyn Iterator<Item = f64>| 10.0 * power(x, 0.02, 0.2, fs).log10();
    let d1 = db(&on, &mut std::iter::once(f1)) - db(&half, &mut std::iter::once(f1));
    let dh = db(&on, &mut std::iter::once(2.0 * f1)) - db(&half, &mut std::iter::once(2.0 * f1));
    let dn = db(&on, &mut (2200..2500).step_by(20).map(|f| f as f64)) - db(&half, &mut (2200..2500).step_by(20).map(|f| f as f64));
    assert!(d1.abs() < 0.5 && (dh - 6.0).abs() < 1.5 && (dn - 6.0).abs() < 1.5, "fundamental {d1:.1}, harmonic {dh:.1}, noise {dn:.1} dB");
}

/// The buzz re the fundamental as the analysis measures it (`rattle_db`): the median power over
/// 1–4 kHz, 50–300 ms, re the fundamental's over the first 0.1 s (dB).
fn buzz(inst: &Instrument, key: u8, speed: f64, mallet: u32) -> f64 {
    let f1 = inst.notes[key as usize - 21].spec.string.f0;
    let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
    e.set_params(Params { room: 0.0, knock: 0.0, mallet, ..Params::default() });
    e.queue(0, Event::NoteOn { key, speed });
    let x = render(&mut e, (0.35 * SR) as usize);
    let mut p: Vec<f64> = (1000..4000).step_by(25).map(|f| power(&x, 0.05, 0.3, std::iter::once(f as f64))).collect();
    p.sort_by(|a, b| a.partial_cmp(b).unwrap());
    10.0 * (p[p.len() / 2] / power(&x, 0.0, 0.1, std::iter::once(f1))).log10()
}

#[test]
fn the_soft_mallet_buzzes_least() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    // for the same swing of the bar: the recordings' soft mallet buzzes 8 dB under the
    // traditional one (re the engine's, which buzzed alike for every mallet), the hard one 2.5
    for key in [65u8, 77] {
        let (t, h, s) = (buzz(&inst, key, 1.0, 0), buzz(&inst, key, 1.0, 1), buzz(&inst, key, 1.0, 2));
        assert!(s < t - 5.0 && s < h, "key {key}: traditional {t:.1}, hard {h:.1}, soft {s:.1} dB");
    }
}

#[test]
fn every_key_buzzes_more_the_harder_it_is_struck() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    // the buzz (`buzz`): the strike's level re the key's own at speed 1 sets the membrane's gain
    // and lowpass: summed with signs it cancelled (each mode's onset partner in antiphase), and the
    // buzz came out at random per key, mallet and dynamic (hard C♯4 −82 dB at mf, F6 +24 over)
    for mallet in 0..3u32 {
        for key in [61u8, 65, 72, 77, 84, 89] {
            let s: Vec<f64> = inst.velocity_speed.iter().map(|&v| buzz(&inst, key, v, mallet)).collect();
            assert!(s[1] > s[0] + 1.0 && s[2] > s[1] + 1.0, "mallet {mallet}, key {key}: the buzz at pp, mf, ff {s:.1?} dB");
        }
    }
}
