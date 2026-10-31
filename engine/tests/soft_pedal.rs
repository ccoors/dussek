//! The soft pedal: on a grand the hammer misses one string of each unison and strikes with
//! softer felt (una corda), on an upright it arrives slower (hammer rail); it acts on the
//! notes struck after it, not on the sounding ones, and not at all on the harpsichords.
//! Needs the exported instruments (skipped without them).

use dussek::{Engine, Event, Instrument, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 32000.0;

fn render(name: &str, events: &[(f64, Event)]) -> Option<Vec<f32>> {
    let inst = Instrument::load(format!("{ROOT}{name}")).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    for &(t, ev) in events {
        e.queue((t * SR) as u64, ev);
    }
    let n = (1.5 * SR) as usize + e.latency();
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    e.process(&mut l, &mut r);
    Some(l)
}

fn level(x: &[f32]) -> f64 {
    10.0 * (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64 + 1e-30).log10()
}

/// peak of the first 100 ms (dB) and how bright they are: rms of the first difference
/// re rms (a high-pass proxy for the spectral centroid)
fn attack(x: &[f32]) -> (f64, f64) {
    let a = &x[(0.03 * SR) as usize..(0.13 * SR) as usize]; // past the engine's latency
    let peak = a.iter().fold(0f64, |m, v| m.max(v.abs() as f64));
    let rms = (a.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / a.len() as f64).sqrt();
    let d = (a.windows(2).map(|w| ((w[1] - w[0]) as f64).powi(2)).sum::<f64>() / a.len() as f64).sqrt();
    (20.0 * peak.max(1e-12).log10(), d / rms.max(1e-12))
}

const SOFT: Event = Event::SoftPedal { depth: 1.0 };
fn note(key: u8) -> Event {
    Event::NoteOn { key, speed: 3.0 }
}

#[test]
fn depth_zero_changes_nothing() {
    for name in ["grand_a", "upright_b"] {
        let (Some(a), Some(b)) = (
            render(name, &[(0.0, note(60))]),
            render(name, &[(0.0, Event::SoftPedal { depth: 0.0 }), (0.0, note(60))]),
        ) else { return };
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn a_grand_gets_quieter_with_fewer_strings_struck() {
    let (Some(a), Some(b)) = (render("grand_a", &[(0.0, note(60))]), render("grand_a", &[(0.0, SOFT), (0.0, note(60))])) else { return };
    let ((pa, ba), (pb, bb)) = (attack(&a), attack(&b));
    assert!(pb < pa - 0.3 && pb > pa - 10.0, "attack peak C4: {pa:.1} dB, una corda {pb:.1} dB");
    assert!(bb < ba * 0.95, "mellower: brightness {ba:.3} -> {bb:.3}");
}

#[test]
fn an_upright_gets_quieter_with_slower_hammers() {
    let (Some(a), Some(b)) = (render("upright_b", &[(0.0, note(60))]), render("upright_b", &[(0.0, SOFT), (0.0, note(60))])) else { return };
    let ((pa, ba), (pb, bb)) = (attack(&a), attack(&b));
    assert!(pb < pa - 2.0 && pb > pa - 15.0, "attack peak C4: {pa:.1} dB, soft pedal {pb:.1} dB");
    assert!(bb < ba, "mellower: brightness {ba:.3} -> {bb:.3}");
}

#[test]
fn sounding_notes_are_not_affected() {
    for name in ["grand_a", "upright_b"] {
        let (Some(a), Some(b)) = (render(name, &[(0.0, note(60))]), render(name, &[(0.0, note(60)), (0.5, SOFT)])) else { return };
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn the_harpsichords_ignore_it() {
    for name in ["harpsichord", "flemish"] {
        let (Some(a), Some(b)) = (render(name, &[(0.0, note(60))]), render(name, &[(0.0, SOFT), (0.0, note(60))])) else { return };
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn a_single_string_note_only_gets_the_softer_felt() {
    // A0 has one string on the grand: nothing to miss, but a quieter tone from the felt
    let (Some(a), Some(b)) = (render("grand_a", &[(0.0, note(21))]), render("grand_a", &[(0.0, SOFT), (0.0, note(21))])) else { return };
    let (la, lb) = (level(&a), level(&b));
    assert!(lb <= la + 0.01 && lb > la - 6.0, "A0: {la:.1} dB, una corda {lb:.1} dB");
}
