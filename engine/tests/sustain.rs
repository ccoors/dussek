//! The Sustain control (`Tuning::sustain`): every mode's decay rate divided by it, the ring lengthened
//! beyond the recordings (design log step 129) while the attack
//! keeps its level and the pitch stays.

use dussek::{Engine, Event, Instrument, Params, Tuning};
use std::path::Path;

const SR: f64 = 48000.0;

fn load(name: &str) -> Option<Instrument> {
    Instrument::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(name)).ok()
}

/// 3 s of `key` at velocity 80 held, the room off; (attack peak dB over 50 ms, level dB over 1.5–2.5 s)
fn levels(inst: Instrument, key: u8, sustain: f64) -> (f64, f64) {
    let mut e = Engine::with_tuning(inst, SR, Tuning { sustain, ..Tuning::default() }).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    let speed = e.hammer_speed(key, 80.0);
    e.queue(0, Event::NoteOn { key, speed });
    let n = (3.0 * SR) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    let peak = l[..(0.05 * SR) as usize].iter().fold(0.0f64, |m, x| m.max(x.abs() as f64));
    let late = &l[(1.5 * SR) as usize..(2.5 * SR) as usize];
    let rms = (late.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / late.len() as f64).sqrt();
    (20.0 * peak.log10(), 20.0 * rms.log10())
}

#[test]
fn a_longer_sustain_rings_longer_and_keeps_the_attack() {
    let Some(inst) = load("grand_d") else { return };
    // the bass too: the internal loss divided alone left it within 1–2 dB (the coupling sets its decay)
    for key in [36u8, 60, 84] {
        let (p1, l1) = levels(inst.clone(), key, 1.0);
        let (p2, l2) = levels(inst.clone(), key, 2.0);
        assert!((p2 - p1).abs() < 0.5, "key {key}: attack {p1:.1} → {p2:.1} dB");
        assert!(l2 > l1 + 3.0, "key {key}: 1.5–2.5 s {l1:.1} → {l2:.1} dB");
    }
}

#[test]
fn a_shorter_sustain_rings_shorter() {
    let Some(inst) = load("grand_a") else { return };
    let (_, l1) = levels(inst.clone(), 72, 1.0);
    let (_, l2) = levels(inst, 72, 0.5);
    assert!(l2 < l1 - 3.0, "{l1:.1} → {l2:.1} dB");
}

#[test]
fn every_mode_keeps_its_frequency_and_rings_longer() {
    // the decay rates alone: the coupling's real part divided moved the modes (the unison's strings
    // lock by it, the bridge's pitch pull follows: C5 −5 cents at 4×). The sounding pitch over a
    // note's first seconds still follows its in-phase mode for longer, as a longer prompt sound would
    let Some(inst) = load("grand_d") else { return };
    let (mut a, mut b) = (inst.clone(), inst);
    a.compile(SR, Tuning::default());
    b.compile(SR, Tuning { sustain: 4.0, ..Tuning::default() });
    for key in [36usize, 72, 96] {
        let (na, nb) = (&a.notes[key - 21].phys, &b.notes[key - 21].phys);
        for (pa, pb) in [(&na.vert, &nb.vert), (&na.horiz, &nb.horiz)] {
            assert_eq!(pa.lam.len(), pb.lam.len());
            for (x, y) in pa.lam.iter().zip(&pb.lam) {
                assert_eq!(x.im, y.im, "key {key}");
                assert!((y.re - x.re / 4.0).abs() <= 1e-12 * x.re.abs(), "key {key}: {} → {}", x.re, y.re);
            }
        }
    }
}

#[test]
fn sustain_one_is_as_fitted() {
    assert_eq!(Tuning::default().sustain, 1.0);
}

#[test]
fn a_keys_own_sustain_is_the_controls_on_it() {
    // the project's [sustain] by ear (design log step 154): a key's factor times the control's
    let Some(inst) = load("grand_d") else { return };
    assert!(inst.notes.iter().all(|n| n.spec.sustain == 1.0), "an export without [sustain] is as fitted");
    let modes = |sustain_key: f64, control: f64| {
        let mut i = inst.clone();
        i.notes.iter_mut().for_each(|n| n.spec.sustain = sustain_key);
        i.compile(SR, Tuning { sustain: control, ..Tuning::default() });
        i
    };
    let (own, control, both, four) = (modes(2.0, 1.0), modes(1.0, 2.0), modes(2.0, 2.0), modes(1.0, 4.0));
    for key in [36usize, 72, 96] {
        let k = key - 21;
        for (a, b) in [(&own, &control), (&both, &four)] {
            let (na, nb) = (&a.notes[k].phys, &b.notes[k].phys);
            assert_eq!(na.vert.lam, nb.vert.lam, "key {key}");
            assert_eq!(na.horiz.lam, nb.horiz.lam, "key {key}");
        }
    }
}
