//! The bells (`proto/export_bars.py --profile bells`): a glockenspiel's steel bars, eight
//! modes whose ratios drift over the keys, two mallets (plastic, brass), no dampers. Needs the
//! export (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/bells");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

#[test]
fn drifting_modes_two_mallets_no_dampers() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == pianoengine::mic::Body::Bells && !inst.motor);
    assert_eq!(inst.mallets.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["plastic", "brass"]);
    // the strong mode near 7 at C5 falls towards 5.4; the torsional one from 11.3 to 4
    let r = |key: usize, j: usize| inst.notes[key - 21].spec.string.modes.as_ref().unwrap().ratio[j];
    assert!(r(72, 2) > 7.0 && r(96, 2) < 5.7, "{} {}", r(72, 2), r(96, 2));
    assert!(r(72, 4) > 11.0 && r(108, 4) < 4.2, "{} {}", r(72, 4), r(108, 4));
    assert!(inst.notes.iter().all(|n| !n.has_damper));
    // a key's release changes nothing; the two mallets sound different
    let strike = |mallet: u32, up: bool| -> Vec<f32> {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, mallet, ..Params::default() });
        let speed = e.hammer_speed(84, 100.0);
        e.queue(0, Event::NoteOn { key: 84, speed });
        if up {
            e.queue(1200, Event::KeyUp { key: 84 });
        }
        render(&mut e, 12000)
    };
    let plastic = strike(0, false);
    assert!(plastic.iter().all(|v| v.is_finite()) && plastic.iter().any(|&v| v.abs() > 1e-4));
    assert_eq!(strike(0, true), plastic);
    assert_ne!(strike(1, false), plastic);
}
