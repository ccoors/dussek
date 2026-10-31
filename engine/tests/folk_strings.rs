//! The plucked folk strings (`proto/scale.py` designs "psaltery", "strumstick"): single strings
//! plucked by fingers or a pick, the guitar's body for the microphones, no dampers but the
//! player's hand at a key's release (as the harps). Need the exported instruments (skipped without).

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

fn rms(s: &[f32]) -> f64 {
    (s.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt()
}

#[test]
fn single_plucked_strings_on_bodies_of_their_own() {
    use pianoengine::mic::Body;
    for (name, keys, body) in [("psaltery", [70u8, 80, 90], Body::Psaltery), ("strumstick", [50, 62, 81], Body::Strumstick)] {
        let Ok(inst) = Instrument::load(format!("{ROOT}{name}")) else { continue };
        assert!(inst.body() == body, "{name}");
        for key in keys {
            let n = &inst.notes[key as usize - 21];
            assert!(matches!(n.spec.excitation, pianoengine::physics::Excitation::Pluck(_)), "{name} {key}");
            assert_eq!(n.spec.string.detune_cents.len(), 1, "{name} {key}: one string");
            let cents = 1200.0 * (n.spec.string.f0 / (440.0 * 2f64.powf((key as f64 - 69.0) / 12.0))).log2();
            assert!(cents.abs() < 15.0, "{name} {key}: {cents:+.1} cents");
        }
    }
}

#[test]
fn the_hand_takes_a_released_string() {
    for name in ["psaltery", "strumstick"] {
        if Instrument::load(format!("{ROOT}{name}")).is_err() {
            continue;
        }
        let key = if name == "psaltery" { 80u8 } else { 66 };
        let level = |hold: u64| -> f64 {
            let mut e = Engine::with_tuning(Instrument::load(format!("{ROOT}{name}")).unwrap(), SR, Tuning::default()).unwrap();
            e.set_params(Params { room: 0.0, ..Params::default() });
            e.queue(0, Event::NoteOn { key, speed: 1.0 });
            e.queue(hold, Event::KeyUp { key });
            let x = render(&mut e, (2.0 * SR) as usize);
            20.0 * (rms(&x[(1.5 * SR) as usize..]) / rms(&x[(0.05 * SR) as usize..(0.25 * SR) as usize])).log10()
        };
        let (held, released) = (level((2.0 * SR) as u64), level((0.3 * SR) as u64));
        assert!(released < held - 20.0, "{name}: released {released:.1} dB, held {held:.1} dB");
    }
}
