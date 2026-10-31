//! The marimba (`proto/export_bars.py --profile marimba`): bars with four modes, three
//! mallets to choose from, no dampers. Needs the exported instrument (skipped without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/marimba");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

/// Level (dB) at f over x (Hann-windowed DFT bin).
fn level(x: &[f32], f: f64) -> f64 {
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n).cos();
        let ph = 2.0 * std::f64::consts::PI * f * i as f64 / SR;
        re += w * v as f64 * ph.cos();
        im -= w * v as f64 * ph.sin();
    }
    20.0 * (re.hypot(im) + 1e-30).log10()
}

#[test]
fn bars_with_mallets_and_no_dampers() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == dussek::mic::Body::Marimba && !inst.motor);
    assert_eq!(inst.mallets.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["yarn", "cord", "rubber"]);
    // four modes, each with its onset partner (the same mode in antiphase: from zero over the contact)
    assert!(inst.notes.iter().all(|n| !n.has_damper
        && n.spec.string.modes.as_ref().is_some_and(|m| m.ratio.len() == 8 && m.ratio[4..] == m.ratio[..4])));
    // the first mallet is the notes' own
    let p = inst.notes[60 - 21].pluck.expect("a pluck");
    assert!((p.fq_oct_per_step - inst.mallets[0].fq_oct_per_step).abs() < 1e-12 && p.bar);
}

#[test]
fn a_harder_mallet_is_brighter_and_a_key_up_damps_nothing() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let key = 60u8;
    let f = inst.notes[key as usize - 21].f_mean.clone();
    let strike = |mallet: u32, up: Option<u64>| -> Vec<f32> {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, mallet, ..Params::default() });
        let speed = e.hammer_speed(key, 100.0);
        e.queue(0, Event::NoteOn { key, speed });
        if let Some(t) = up {
            e.queue(t, Event::KeyUp { key });
        }
        render(&mut e, 24000)
    };
    // the second mode re the fundamental over the first 0.1 s: yarn < cord < rubber
    let share = |x: &[f32]| level(&x[240..5040], f[1]) - level(&x[240..5040], f[0]);
    let (yarn, cord, rubber) = (strike(0, None), strike(1, None), strike(2, None));
    assert!(share(&yarn) < share(&cord) && share(&cord) < share(&rubber), "{} {} {}", share(&yarn), share(&cord), share(&rubber));
    assert!(yarn.iter().chain(&rubber).all(|v| v.is_finite()));
    // no dampers: the key's release changes nothing; an unknown mallet is the first
    assert_eq!(strike(0, Some(2400)), yarn);
    assert_eq!(strike(7, None), yarn);
}

#[test]
fn a_strike_rises_from_zero() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    // each mode from zero over 0.5 ms (export_bars `onset_s`): released all at once the modes
    // stepped to 0.19 of the strike's peak in its first 0.2 ms, the recordings 0.07
    for mallet in 0..3u32 {
        for key in [36u8, 60, 84, 96] {
            let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
            e.set_params(Params { room: 0.0, knock: 0.0, mallet, ..Params::default() });
            e.queue(0, Event::NoteOn { key, speed: 1.0 });
            let x = render(&mut e, (0.06 * SR) as usize);
            let peak = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
            let on = x.iter().position(|v| v.abs() > 1e-6 * peak).unwrap();
            let first = x[on..on + (0.0002 * SR) as usize].iter().fold(0.0f32, |a, v| a.max(v.abs()));
            assert!(first < 0.1 * peak, "mallet {mallet}, key {key}: {:.2} of the peak in the first 0.2 ms", first / peak);
        }
    }
}
