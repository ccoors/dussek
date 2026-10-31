//! The crotales (`proto/export_bars.py --profile crotales`): bronze discs, eight modes whose
//! ratios drift over the keys, one mallet, no dampers. Needs the export (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

mod common;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/crotales");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

#[test]
fn drifting_modes_one_mallet_no_dampers() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == pianoengine::mic::Body::Crotales && !inst.motor);
    assert!(inst.mallets.is_empty());
    // the strongest mode from 3.7 at C6 to 3.2 at C8; the second from 2.14 to 1.9
    let r = |key: usize, j: usize| inst.notes[key - 21].spec.string.modes.as_ref().unwrap().ratio[j];
    assert!(r(84, 2) > 3.7 && r(108, 2) < 3.25, "{} {}", r(84, 2), r(108, 2));
    assert!(r(84, 1) > 2.1 && r(108, 1) < 1.95, "{} {}", r(84, 1), r(108, 1));
    assert!(inst.notes.iter().all(|n| !n.has_damper));
    // a key's release changes nothing
    let strike = |up: bool| -> Vec<f32> {
        let mut e = common::engine(DIR, SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        let speed = e.hammer_speed(96, 100.0);
        e.queue(0, Event::NoteOn { key: 96, speed });
        if up {
            e.queue(1200, Event::KeyUp { key: 96 });
        }
        render(&mut e, 12000)
    };
    let x = strike(false);
    assert!(x.iter().all(|v| v.is_finite()) && x.iter().any(|&v| v.abs() > 1e-4));
    assert_eq!(strike(true), x);
}

#[test]
fn close_pair_hears_the_bass_left_and_the_treble_right() {
    use pianoengine::mic::MicPosition;
    let Ok(_) = Instrument::load(DIR) else { return };
    // R re L (dB) of a key at Close 1 (the pair over the discs): the frame's own range spread
    // over the pair (the zones at the piano's keys piled C6–B6 at the bass end)
    let balance = |key: u8| -> f64 {
        let tuning = Tuning { mic: MicPosition::Hammers, ..Tuning::default() };
        let mut e = common::engine(DIR, SR, tuning).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        let speed = e.hammer_speed(key, 100.0);
        e.queue(0, Event::NoteOn { key, speed });
        let (mut l, mut r) = (vec![0.0; 12000], vec![0.0; 12000]);
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        let p = |x: &[f32]| x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
        10.0 * (p(&r) / p(&l)).log10()
    };
    // left to right along the discs, without jumps (before: the zones at the piano's keys mixed
    // two per key, the neighbors up to 13 dB apart and C8 at +22 dB)
    let b: Vec<f64> = (84..=108).step_by(2).map(balance).collect();
    assert!(b[..4].iter().all(|&x| x < 0.0) && b[9..].iter().all(|&x| x > 0.0) && b[6].abs() < 2.0, "{b:?}");
    assert!(b.iter().all(|x| x.abs() < 14.0) && b.windows(2).all(|w| (w[1] - w[0]).abs() < 9.0), "{b:?}");
}

/// R re L (dB) of `key` at Close 1, the keys spread along the discs or not.
fn balance_at(key: u8, spread_keys: bool) -> f64 {
    use pianoengine::mic::MicPosition;
    let tuning = Tuning { mic: MicPosition::Hammers, spread_keys, ..Tuning::default() };
    let mut e = common::engine(DIR, SR, tuning).unwrap();
    e.set_params(Params { room: 0.0, ..Params::default() });
    let speed = e.hammer_speed(key, 100.0);
    e.queue(0, Event::NoteOn { key, speed });
    let (mut l, mut r) = (vec![0.0; 12000], vec![0.0; 12000]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    let p = |x: &[f32]| x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
    10.0 * (p(&r) / p(&l)).log10()
}

#[test]
fn spread_over_all_keys_moves_the_extrapolated_ones_along_the_discs() {
    let Ok(_) = Instrument::load(DIR) else { return };
    // off: everything below C6 at the bass end; on: the keyboard's middle near the middle,
    // its ends at the ends
    let off = balance_at(64, false);
    let (low, mid, high) = (balance_at(24, true), balance_at(64, true), balance_at(106, true));
    assert!(off < -5.0 && mid.abs() < 4.0 && low < mid - 3.0 && high > mid + 3.0, "{off} | {low} {mid} {high}");
}

#[test]
fn switching_the_spread_hands_over_to_the_new_positions() {
    let Ok(_) = Instrument::load(DIR) else { return };
    use pianoengine::mic::MicPosition;
    let engine = |spread_keys: bool| {
        let tuning = Tuning { mic: MicPosition::Hammers, spread_keys, ..Tuning::default() };
        let mut e = common::engine(DIR, SR, tuning).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        e
    };
    // the handover keeps the responses (the same microphones) but must take the new keys' zones:
    // a key struck after it sounds from where the new engine has it
    let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
    let mut a = engine(false);
    a.process(&mut l, &mut r);
    let mut b = engine(true);
    assert!(b.adopt(&mut a));
    for _ in 0..200 {
        b.process(&mut l, &mut r); // past the crossfade
    }
    let speed = b.hammer_speed(64, 100.0);
    b.queue(0, Event::NoteOn { key: 64, speed });
    let (mut ls, mut rs) = (vec![0.0f32; 12000], vec![0.0f32; 12000]);
    for (cl, cr) in ls.chunks_mut(256).zip(rs.chunks_mut(256)) {
        b.process(cl, cr);
    }
    let p = |x: &[f32]| x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
    let handed = 10.0 * (p(&rs) / p(&ls)).log10();
    let fresh = balance_at(64, true);
    assert!((handed - fresh).abs() < 1.0, "{handed} {fresh}");
}
