//! The tuner's stretch retunes each note by the offset computed from the instrument's
//! inharmonicity, scaled by `Tuning::stretch`; 0 leaves the strings as fitted. Needs the
//! exported instruments (skipped without).

use pianoengine::stretch::offsets;
use pianoengine::{Instrument, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

fn f1(inst: &Instrument) -> Vec<f64> {
    inst.notes.iter().map(|n| n.phys.f_mean[0]).collect()
}

#[test]
fn stretch_moves_each_note_by_its_offset_and_zero_changes_nothing() {
    let Ok(mut inst) = Instrument::load(DIR) else { return };
    inst.compile(48000.0, Tuning::default());
    let plain = f1(&inst);
    let b: Vec<f64> = inst.notes[..88].iter().map(|n| n.spec.string.b).collect();
    let c = offsets(21, &b);
    for s in [1.0, -1.0, 0.5] {
        inst.compile(48000.0, Tuning { stretch: s, ..Tuning::default() });
        for (i, (f, p)) in f1(&inst).iter().zip(&plain).enumerate().take(88) {
            let want = 2f64.powf(s * c[i] / 1200.0);
            assert!((f / p - want).abs() < 1e-12, "stretch {s}, note {}: {} vs {want}", 21 + i, f / p);
        }
    }
    inst.compile(48000.0, Tuning { stretch: 0.0, ..Tuning::default() });
    assert_eq!(f1(&inst), plain);
    assert_eq!(c[48], 0.0, "A4 stays at the concert pitch");
}

/// The recording's own stretch is about −19 cents at A0 and +25 at C8: the computed one
/// is that of a well-tuned grand (the model's check against a real tuning)
#[test]
fn the_computed_stretch_is_that_of_a_well_tuned_grand() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    let b: Vec<f64> = inst.notes[..88].iter().map(|n| n.spec.string.b).collect();
    let c = offsets(21, &b);
    assert!((c[0] + 19.0).abs() < 6.0, "A0 {}", c[0]);
    assert!((c[87] - 25.0).abs() < 6.0, "C8 {}", c[87]);
}
