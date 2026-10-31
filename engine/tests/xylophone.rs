//! The xylophone (`proto/export_bars.py --profile xylophone`): the second mode tuned to 3,
//! two mallets (rosewood, hard rubber), no dampers. Needs the export (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/xylophone");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

#[test]
fn a_tuned_twelfth_two_mallets_no_dampers() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == pianoengine::mic::Body::Xylophone && !inst.motor);
    assert_eq!(inst.mallets.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["rosewood", "hardrubber"]);
    // tuned to 3 in the middle, untuned (below 3) at the top
    let r = |key: usize| inst.notes[key - 21].spec.string.modes.as_ref().unwrap().ratio[1];
    assert!((r(70) - 3.0).abs() < 0.01 && r(100) < 2.8, "{} {}", r(70), r(100));
    assert!(inst.notes.iter().all(|n| !n.has_damper));
    // a key's release changes nothing; the two mallets sound different
    let strike = |mallet: u32, up: bool| -> Vec<f32> {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, mallet, ..Params::default() });
        let speed = e.hammer_speed(76, 100.0);
        e.queue(0, Event::NoteOn { key: 76, speed });
        if up {
            e.queue(1200, Event::KeyUp { key: 76 });
        }
        render(&mut e, 12000)
    };
    let rose = strike(0, false);
    assert!(rose.iter().all(|v| v.is_finite()) && rose.iter().any(|&v| v.abs() > 1e-4));
    assert_eq!(strike(0, true), rose);
    assert_ne!(strike(1, false), rose);
}
