//! An instrument's own output level re the ff calibration (`output_gain_db`): a quiet piano
//! (Grand G, two soft layers ~5 dB apart) calibrated on its ff played every mf as loud as
//! the other pianos' ff. Needs the exported instruments (skipped without them).

use pianoengine::{Engine, Event, Instrument, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

fn peak(inst: Instrument) -> f32 {
    let mut e = Engine::with_tuning(inst, 48000.0, Tuning::default()).expect("engine");
    e.queue(0, Event::NoteOn { key: 60, speed: 2.0 });
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    let mut p = 0.0f32;
    for _ in 0..40 {
        e.process(&mut l, &mut r);
        p = l.iter().chain(&r).fold(p, |m, x| m.max(x.abs()));
    }
    p
}

#[test]
fn output_gain_scales_the_whole_instrument() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert_eq!(inst.output_gain_db, 0.0);
    let mut quiet = inst.clone();
    quiet.output_gain_db = -6.0;
    let ratio = peak(quiet) / peak(inst);
    assert!((ratio - 10f32.powf(-6.0 / 20.0)).abs() < 1e-3, "ratio {ratio}");
}
