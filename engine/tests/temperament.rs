//! A temperament retunes every note by its pitch class's offset (cents re equal), compiled
//! into the strings like the concert pitch; equal temperament leaves them exactly as they
//! were. Needs the exported instruments (skipped without).

use pianoengine::temperament::offsets;
use pianoengine::{Instrument, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

#[test]
fn temperament_moves_each_note_by_its_offset() {
    let Ok(mut inst) = Instrument::load(DIR) else { return };
    inst.compile(48000.0, Tuning::default());
    let equal: Vec<f64> = inst.notes.iter().map(|n| n.f_mean[0]).collect();
    let off = offsets(1, 3); // quarter-comma meantone on E♭
    inst.compile(48000.0, Tuning { temperament: off, ..Tuning::default() });
    for (n, f) in inst.notes.iter().zip(&equal) {
        let want = 2f64.powf(off[n.note as usize % 12] / 1200.0);
        assert!((n.f_mean[0] / f - want).abs() < 1e-12, "note {}: {} vs {}", n.note, n.f_mean[0] / f, want);
    }
}

#[test]
fn equal_temperament_changes_nothing() {
    let Ok(mut inst) = Instrument::load(DIR) else { return };
    inst.compile(48000.0, Tuning::default());
    let before: Vec<Vec<f64>> = inst.notes.iter().map(|n| n.w.clone()).collect();
    inst.compile(48000.0, Tuning { temperament: offsets(0, 5), ..Tuning::default() });
    for (n, w) in inst.notes.iter().zip(&before) {
        assert_eq!(&n.w, w);
    }
}

#[test]
fn as_recorded_plays_each_key_at_its_recorded_pitch() {
    // the kalimba plays at equal temperament, its tines recorded up to 65 cents off the names:
    // "As recorded" adds each key's own deviation from the curve it plays (`NoteSpec::recorded_cents`)
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/kalimba");
    let Ok(mut inst) = Instrument::load(dir) else { return };
    let i = pianoengine::temperament::NAMES.iter().position(|&n| n == "As recorded").expect("an As recorded entry");
    assert_eq!(i, pianoengine::temperament::AS_RECORDED);
    assert_eq!(offsets(i, 4), [0.0; 12]);
    inst.compile(48000.0, Tuning::default());
    let equal: Vec<f64> = inst.notes.iter().map(|n| n.f_mean[0]).collect();
    inst.compile(48000.0, Tuning { as_recorded: true, ..Tuning::default() });
    for (n, f) in inst.notes.iter().zip(&equal) {
        let want = 2f64.powf(n.spec.recorded_cents / 1200.0);
        assert!((n.f_mean[0] / f - want).abs() < 1e-12, "note {}: {} vs {}", n.note, n.f_mean[0] / f, want);
    }
    assert!(inst.notes.iter().any(|n| n.spec.recorded_cents.abs() > 20.0), "the tines' own tuning");
    // the spec's temperament index sets it
    let t = pianoengine::spec::tuning(Tuning::default(), &format!("temperament={i}")).unwrap();
    assert!(t.as_recorded && t.temperament == [0.0; 12]);
}
