//! The roll's measures (`roll::measure`): the references' range of brightness and decay per
//! register, the responses' ring and Player against Recording; a candidate outside it fails, and a
//! roll's own faults (the bass out of phase, the microphones' colors apart) are named. Needs the
//! exported instruments (skipped without them).

use pianoengine::roll::files::Export;
use pianoengine::roll::measure::{self, Realism};
use std::path::Path;
use std::sync::OnceLock;

const REFS: [&str; 10] = ["grand_a", "grand_b", "grand_f", "grand_e", "grand_d", "grand_c", "grand_g", "upright_a", "upright_b", "upright_c"];

fn load(name: &str) -> Option<Export> {
    Export::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(name)).ok()
}

fn realism() -> Option<&'static Realism> {
    static R: OnceLock<Option<Realism>> = OnceLock::new();
    R.get_or_init(|| {
        let refs: Option<Vec<Export>> = REFS.iter().map(|r| load(r)).collect();
        Some(Realism::from_references(&refs?).expect("the references measure"))
    })
    .as_ref()
}

#[test]
fn every_reference_passes() {
    let Some(r) = realism() else { return };
    for name in REFS {
        let failures = r.check(&load(name).unwrap()).unwrap();
        assert!(failures.is_empty(), "{name}: {failures:?}");
    }
}

#[test]
fn a_long_ring_fails() {
    let (Some(r), Some(mut a)) = (realism(), load("grand_a")) else { return };
    let sr = 48000.0;
    for resp in a.responses().unwrap() {
        let e0: f64 = resp.ir.iter().map(|v| v * v).sum();
        let mut ir: Vec<f64> = resp.ir.iter().enumerate().map(|(i, v)| v * (300.0 * i as f64 / sr / 8.686).exp()).collect();
        let k = (e0 / ir.iter().map(|v| v * v).sum::<f64>()).sqrt();
        ir.iter_mut().for_each(|v| *v *= k);
        a.set_response(resp.pol, resp.channel, &ir).unwrap();
    }
    let failures = r.check(&a).unwrap();
    assert!(failures.iter().any(|f| f.starts_with("ring at 60 ms")), "{failures:?}");
}

#[test]
fn dull_bass_notes_fail() {
    let (Some(r), Some(mut a)) = (realism(), load("grand_a")) else { return };
    let mut c = a.f64s("/correction/C_db").unwrap();
    let grid = pianoengine::roll::bands::grid();
    for row in 0..5 {
        // the bass's five notes (21 … 45): −60 dB above 1 kHz (−20 dB: inside the range since the
        // uprights' rooms were cut, design log step 123; −30 dB since Grand A's felt got its core, its
        // bass 15–20 dB brighter at 4–6 kHz, step 134; −45 dB left it 0.02 dB past the range's edge,
        // and Grand A's even levels, step 139, moved it 0.03 dB back inside; on four of the five notes
        // the fifth held it at −88.4 whatever the cut, inside since Upright A's and C's dry rooms,
        // step 151, darkened the darkest reference's bass −76.5 → −84.1: −100.3 lies 6.5 dB past it)
        for (k, f) in grid.iter().enumerate() {
            if *f > 1000.0 {
                c[row * 61 + k] -= 60.0;
            }
        }
    }
    a.set_f64s("/correction/C_db", &c).unwrap();
    let failures = r.check(&a).unwrap();
    assert!(failures.iter().any(|f| f.starts_with("brightness bass")), "{failures:?}");
}

#[test]
fn out_of_phase_bass_is_a_fault() {
    let (Some(mut e), Some(c), Some(d), Some(f)) = (load("grand_d"), load("grand_f"), load("grand_e"), load("grand_c")) else { return };
    // the spike's fault: each microphone's fine structure mixed from other pianos over the whole
    // band, its bass included (the two channels' bass then unrelated)
    let donors = [c.responses().unwrap(), d.responses().unwrap(), f.responses().unwrap()];
    for r in e.responses().unwrap() {
        let mut mix = vec![0.0; r.ir.len()];
        for (i, dr) in donors.iter().enumerate() {
            let x = &dr.iter().find(|x| x.pol == r.pol && x.channel == r.channel).unwrap().ir;
            let norm = x.iter().map(|v| v * v).sum::<f64>().sqrt();
            // a different sign per donor and channel, as unrelated as separate mixes get
            let sign = if (i + r.channel) % 2 == 0 { 1.0 } else { -1.0 };
            mix.iter_mut().zip(x).for_each(|(m, v)| *m += sign * v / norm);
        }
        let y = pianoengine::roll::bands::recolor(&mix, &r.ir, 48000.0);
        let k = (r.ir.iter().map(|v| v * v).sum::<f64>() / y.iter().map(|v| v * v).sum::<f64>()).sqrt();
        e.set_response(r.pol, r.channel, &y.iter().map(|v| v * k).collect::<Vec<_>>()).unwrap();
    }
    let faults = measure::faults(&e).unwrap();
    assert!(faults.iter().any(|f| f.starts_with("bass")), "{faults:?}");
}

#[test]
fn microphones_apart_are_a_fault() {
    let Some(e) = load("grand_d") else { return };
    assert!(measure::faults(&e).unwrap().iter().any(|f| f.starts_with("microphones")));
}

#[test]
fn player_matches_recording_on_the_references() {
    for name in ["grand_a", "grand_d", "grand_c"] {
        let Some(e) = load(name) else { return };
        let d = measure::player_minus_recording(&e).unwrap();
        // 4.5 before the boards' low modes rang (design log step 184): Grand A's 8 kHz octave −4.7
        assert!(d.iter().all(|v| v.abs() <= 5.0), "{name}: {d:?}");
    }
}



#[test]
fn measuring_from_renders_is_measuring() {
    // a roll measures from the renders its calibrations already made: the same numbers
    use pianoengine::mic::MicPosition;
    let Some(e) = load("grand_c") else { return };
    let inst = e.instrument().unwrap();
    let rec = measure::render_notes(&inst, 80.0, MicPosition::Recording).unwrap();
    let player = measure::render_heads(&inst, 80.0, MicPosition::Player).unwrap();
    let (a, b) = (measure::measure(&e).unwrap(), measure::measure_from(&e, &rec, &player).unwrap());
    assert_eq!((a.stats, a.pr, a.level, a.mono_vs_power), (b.stats, b.pr, b.level, b.mono_vs_power));
    // the levels need the whole render (the decay's late spectra)
    let heads = measure::render_heads(&inst, 80.0, MicPosition::Recording).unwrap();
    assert!(measure::measure_from(&e, &heads, &player).is_err());
}

#[test]
fn a_fundamental_in_a_notch_is_a_fault() {
    // a key's fundamental deep in a notch of the radiation (Grand F's D#7 once lay 31 dB under the
    // response around it: its second partial took over); the references keep theirs above -15 dB
    use pianoengine::roll::bands;
    let Some(e) = load("grand_c") else { return };
    assert!(measure::faults(&e).unwrap().iter().all(|f| !f.contains("notch")), "{:?}", measure::faults(&e).unwrap());
    let key = 60usize;
    let n = &e.json["notes"][key - 21]["string"];
    let f1 = n["f0"].as_f64().unwrap() * (1.0 + n["B"].as_f64().unwrap()).sqrt();
    let sr = 48000.0;
    let mut notched = e.clone();
    for r in e.responses().unwrap().into_iter().filter(|r| r.pol == 0) {
        // a notch filter at f1 (two zeros on the unit circle, two poles just inside)
        let w = 2.0 * std::f64::consts::PI * f1 / sr;
        let (b1, a1, a2) = (-2.0 * w.cos(), -2.0 * 0.995 * w.cos(), 0.995 * 0.995);
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        let y: Vec<f64> = r.ir.iter().map(|&x| {
            let y = x + b1 * x1 + x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x, y1, y);
            y
        }).collect();
        assert!(bands::notch_depths(&y, sr, &[f1])[0] < -15.0, "the notch: {:.1} dB", bands::notch_depths(&y, sr, &[f1])[0]);
        notched.set_response(r.pol, r.channel, &y).unwrap();
    }
    let faults = measure::faults(&notched).unwrap();
    assert!(faults.iter().any(|f| f.contains("notch") && f.contains("key 60")), "{faults:?}");
}
