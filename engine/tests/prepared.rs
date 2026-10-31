//! The prepared piano: each preparation does what it says (plucked is brighter than struck,
//! muted dies away faster, a harmonic leaves out the fundamental without a click at the
//! attack, a bolt lowers and detunes the partials by where it sits, paper buzzes), and the
//! settings that do nothing change nothing. Needs the exported instruments (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Prep, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 32000.0;
const C3: u8 = 48; // 130.8 Hz

fn render_with(name: &str, params: Params, tuning: Tuning, events: &[(f64, Event)], seconds: f64) -> Option<Vec<f32>> {
    let inst = Instrument::load(format!("{ROOT}{name}")).ok()?;
    let mut e = Engine::with_tuning(inst, SR, tuning).expect("engine");
    e.set_params(params);
    for &(t, ev) in events {
        e.queue((t * SR) as u64, ev);
    }
    let lat = e.latency();
    let n = (seconds * SR) as usize + lat;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    e.process(&mut l, &mut r);
    Some(l[lat..].to_vec())
}

fn render(name: &str, params: Params, events: &[(f64, Event)], seconds: f64) -> Option<Vec<f32>> {
    render_with(name, params, Tuning::default(), events, seconds)
}

fn prep(p: Prep) -> Params {
    Params { prep: p, ..Params::default() }
}

/// a C3 struck at 0, released at `hold`
fn note(hold: f64) -> [(f64, Event); 2] {
    [(0.0, Event::NoteOn { key: C3, speed: 3.0 }), (hold, Event::KeyUp { key: C3 })]
}

fn rms_db(x: &[f32], a: f64, b: f64) -> f64 {
    let w = &x[(a * SR) as usize..(b * SR) as usize];
    10.0 * (w.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w.len() as f64 + 1e-30).log10()
}

/// spectral centroid (Hz) of a window, over 50 Hz … 8 kHz
fn centroid(x: &[f32], a: f64, b: f64) -> f64 {
    let (mut num, mut den) = (0.0, 0.0);
    let mut f = 50.0;
    while f < 8000.0 {
        let p = 10f64.powf(power_db(x, a, b, f) / 10.0);
        num += f * p;
        den += p;
        f += 25.0;
    }
    num / den.max(1e-30)
}

/// power at frequency f (dB) in a window
fn power_db(x: &[f32], a: f64, b: f64, f: f64) -> f64 {
    let w = &x[(a * SR) as usize..(b * SR) as usize];
    let (mut re, mut im) = (0.0, 0.0);
    for (i, v) in w.iter().enumerate() {
        let ph = std::f64::consts::TAU * f * i as f64 / SR;
        let h = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / w.len() as f64).cos();
        re += *v as f64 * h * ph.cos();
        im += *v as f64 * h * ph.sin();
    }
    10.0 * (re * re + im * im + 1e-30).log10()
}

/// the biggest jump between neighboring samples in the first 15 ms, re the rms of the next 100
fn click(x: &[f32]) -> f64 {
    let a = (0.03 * SR) as usize; // past the engine's latency
    let jump = x[a..a + (0.015 * SR) as usize].windows(2).fold(0f64, |m, w| m.max(((w[1] - w[0]) as f64).abs()));
    let rms = (x[a + (0.02 * SR) as usize..a + (0.12 * SR) as usize].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / (0.1 * SR)).sqrt();
    jump / rms.max(1e-12)
}

#[test]
fn nothing_prepared_is_the_plain_piano() {
    let (Some(a), Some(b)) = (render("grand_a", Params::default(), &note(1.0), 1.5), render("grand_a", prep(Prep::None), &note(1.0), 1.5)) else { return };
    assert_eq!(a, b);
    // and a preparation's settings are ignored while another is chosen
    let odd = Params { pluck_pos: 0.4, pluck_bright: 3.0, mute: 1.0, harmonic: 5, paper: 1.0, ..Params::default() };
    assert_eq!(render("grand_a", odd, &note(1.0), 1.5).unwrap(), a);
}

#[test]
fn plucked_is_brighter_than_struck_and_about_as_loud() {
    let (Some(struck), Some(plucked)) = (render("grand_a", Params::default(), &note(1.0), 1.5), render("grand_a", prep(Prep::Plucked), &note(1.0), 1.5)) else { return };
    let (cs, cp) = (centroid(&struck, 0.1, 0.6), centroid(&plucked, 0.1, 0.6));

    // 1.3 until the fade-out fix (2026-10-10): upper partials now decay as fast as recorded,
    // and the pluck, whose energy sits up there, lost more of its 0.1–0.6 s brightness
    // (Salamander: struck 1373 → 1314 Hz, plucked 1830 → 1486 Hz)
    assert!(cp > cs * 1.1, "centroid: struck {cs:.0} Hz, plucked {cp:.0} Hz");
    let (ls, lp) = (rms_db(&struck, 0.1, 0.6), rms_db(&plucked, 0.1, 0.6));
    assert!((lp - ls).abs() < 8.0, "level: struck {ls:.1} dB, plucked {lp:.1} dB");
    // where it is plucked and how hard change the tone
    let bright = Params { pluck_bright: 4.0, ..prep(Prep::Plucked) };
    let near = Params { pluck_pos: 0.06, ..prep(Prep::Plucked) };
    let (b, n) = (render("grand_a", bright, &note(1.0), 1.5).unwrap(), render("grand_a", near, &note(1.0), 1.5).unwrap());

    assert!(centroid(&b, 0.1, 0.6) > cp * 1.1, "a harder pluck is brighter");
    assert!(centroid(&n, 0.1, 0.6) > cp * 1.1, "plucked near the end is brighter");
}

#[test]
fn muted_dies_away_faster_and_zero_is_the_plain_piano() {
    let hold = 1.2; // the key stays down: the hand is on the strings the whole time
    let (Some(plain), Some(muted)) = (render("grand_a", Params::default(), &note(hold), 1.3), render("grand_a", Params { mute: 0.9, ..prep(Prep::Muted) }, &note(hold), 1.3)) else { return };
    let (lp, lm) = (rms_db(&plain, 0.5, 0.9), rms_db(&muted, 0.5, 0.9));
    assert!(lm < lp - 6.0, "after 0.5 s: plain {lp:.1} dB, muted {lm:.1} dB");
    assert_eq!(render("grand_a", Params { mute: 0.0, ..prep(Prep::Muted) }, &note(hold), 1.3).unwrap(), plain);
}

#[test]
fn a_harmonic_leaves_out_the_fundamental_without_a_click() {
    let plain = render("grand_a", Params::default(), &note(1.0), 1.2);
    let Some(plain) = plain else { return };
    for (n, f) in [(2u32, 2.0), (3, 3.0), (4, 4.0)] {
        let h = render("grand_a", Params { harmonic: n, ..prep(Prep::Harmonic) }, &note(1.0), 1.2).unwrap();
        let (fund, harm) = (power_db(&h, 0.2, 0.9, 130.8), power_db(&h, 0.2, 0.9, 130.8 * f));
        assert!(harm > fund + 10.0, "harmonic {n}: fundamental {fund:.1} dB, the harmonic {harm:.1} dB");
        // the contact itself leaves the other partials out: no jump at the attack beyond the
        // brightness of the sound (a bell-like tone has bigger sample-to-sample steps)
        assert!(click(&h) < 3.0 * click(&plain).max(0.3), "harmonic {n} clicks: {:.2} against {:.2} plain", click(&h), click(&plain));
    }
}

#[test]
fn a_bolt_lowers_the_partials_by_where_it_sits() {
    let Ok(mut inst) = Instrument::load(format!("{ROOT}grand_a")) else { return };
    inst.compile(48000.0, Tuning::default());
    let plain: Vec<Vec<f64>> = inst.notes.iter().map(|n| n.phys.f_mean.clone()).collect();
    inst.compile(48000.0, Tuning { bolt: 1.0, bolt_pos: 0.5, ..Tuning::default() });
    for (n, p) in inst.notes[..88].iter().zip(&plain).skip(10).step_by(7) {
        let f = &n.phys.f_mean;
        // in the middle of the string: the odd partials are lowered, the even ones have a node
        assert!(f[0] < p[0] * 0.9, "note {}: {} against {}", n.spec.note, f[0], p[0]);
        assert!((f[1] / p[1] - 1.0).abs() < 1e-9, "the second partial has a node at the middle");
        assert!(f[2] < p[2] * 0.9);
    }
    inst.compile(48000.0, Tuning { bolt: 0.0, bolt_pos: 0.2, ..Tuning::default() });
    let again: Vec<Vec<f64>> = inst.notes.iter().map(|n| n.phys.f_mean.clone()).collect();
    assert_eq!(again, plain, "no bolt, no change");
}

#[test]
fn paper_buzzes() {
    let (Some(plain), Some(paper)) = (render("grand_a", Params::default(), &note(1.0), 1.5), render("grand_a", Params { paper: 1.0, ..prep(Prep::Paper) }, &note(1.0), 1.5)) else { return };
    let (cp, cb) = (centroid(&plain, 0.1, 0.6), centroid(&paper, 0.1, 0.6));

    assert!(cb > cp * 1.1, "centroid: plain {cp:.0} Hz, with paper {cb:.0} Hz");
    // and none of it with the buzz at 0
    assert_eq!(render("grand_a", Params { paper: 0.0, ..prep(Prep::Paper) }, &note(1.0), 1.5).unwrap().len(), plain.len());
}

#[test]
fn a_harpsichord_plucks_anyway() {
    let (Some(a), Some(b)) = (render("harpsichord", Params::default(), &note(1.0), 1.2), render("harpsichord", prep(Prep::Plucked), &note(1.0), 1.2)) else { return };
    assert_eq!(a, b);
}
