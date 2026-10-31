//! The stems (`Mix::STEMS`, `midi2wav --stems`): the struck strings, the ringing, the room and the
//! noises, each rendered alone, sum to the full render, at the recording's position and at one whose
//! radiation runs per bridge zone; and each carries sound.

use dussek::mic::MicPosition;
use dussek::{Engine, Event, Instrument, Mix, Params, Tuning};
use std::path::Path;

const SR: f64 = 48000.0;

fn load(name: &str) -> Option<Instrument> {
    Instrument::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(name)).ok()
}

/// 3 s: the pedal down, a chord, a key released (its noise), the pedal up (its thump); the left channel
fn render(inst: &Instrument, mic: MicPosition, mix: Mix) -> Vec<f32> {
    let mut e = Engine::with_tuning(inst.clone(), SR, Tuning { mic, ..Tuning::default() }).unwrap();
    e.set_params(Params { rattle: 1.0, ..Params::default() });
    e.set_mix(mix);
    e.queue(0, Event::PedalDown);
    for key in [48u8, 55, 64] {
        let speed = e.hammer_speed(key, 100.0);
        e.queue(4800, Event::NoteOn { key, speed });
    }
    e.queue(48000, Event::KeyUp { key: 64 });
    e.queue(96000, Event::PedalUp);
    let n = (3.0 * SR) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

#[test]
fn the_stems_sum_to_the_full_render() {
    let Some(inst) = load("grand_a") else { return };
    for mic in [MicPosition::Recording, MicPosition::Player] {
        let full = render(&inst, mic, Mix::ALL);
        let peak = full.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let mut sum = vec![0.0f32; full.len()];
        for (name, mix) in Mix::STEMS {
            let x = render(&inst, mic, mix);
            let p = x.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(p > peak * 1e-3, "{mic:?}: the {name} stem is silent ({p:e} re {peak:e})");
            sum.iter_mut().zip(&x).for_each(|(s, x)| *s += x);
        }
        let err = sum.iter().zip(&full).fold(0.0f32, |m, (s, f)| m.max((s - f).abs()));
        assert!(err < peak * 1e-5, "{mic:?}: the stems' sum off by {err:e} (peak {peak:e})");
    }
}
