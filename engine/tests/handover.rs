//! `Engine::adopt`: an engine taking over mid-block from one with the same tuning plays on
//! as if nothing happened; with another tuning or microphone setup it plays on without
//! a gap. Needs the exported instruments (skipped without them).

use pianoengine::{Engine, Event, Instrument, Tuning};

mod common;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const SR: f64 = 48000.0;

fn engine(t: Tuning) -> Option<Engine> {
    common::engine(DIR, SR, t)
}

fn play(e: &mut Engine) {
    e.queue(100, Event::PedalDown);
    for (key, speed) in [(48, 2.5), (55, 2.5), (60, 3.0), (69, 3.0)] {
        e.queue(1000, Event::NoteOn { key, speed });
    }
}

/// Render `n` samples in host-sized blocks, handing over to `next` after `at` samples.
fn render(mut e: Engine, n: usize, at: usize, next: Option<Engine>) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    e.process(&mut l[..at], &mut r[..at]);
    if let Some(mut next) = next {
        assert!(next.adopt(&mut e));
        e = next;
    }
    for (cl, cr) in l[at..].chunks_mut(256).zip(r[at..].chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

#[test]
fn same_tuning_continues_exactly() {
    let t = Tuning::default();
    let (Some(mut a), Some(mut b)) = (engine(t), engine(t)) else { return };
    play(&mut a);
    play(&mut b);
    let n = 48000;
    let at = 24_037; // mid-block
    let reference = render(a, n, at, None);
    let handed = render(b, n, at, engine(t));
    let peak = reference.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let diff = reference.iter().zip(&handed).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
    assert!(diff < peak * 1e-5, "difference {diff} re peak {peak}");
}

#[test]
fn new_tuning_and_mic_have_no_gap() {
    let mut other = Tuning { pitch_ratio: 446.0 / 440.0, ..Tuning::default() };
    other.mic = pianoengine::mic::MicPosition::Audience;
    let mut custom = other;
    custom.mic = pianoengine::mic::MicPosition::Custom;
    custom.mic_setup.mics[0].pos = [-3.0, -2.0, 1.0];
    let soft = Tuning { pillow: 0.7, lid: 0.3, ..other };
    // the pillow target is legitimately much quieter (about -16 dB at 0.7 with the lid at 0.3;
    // -11 before the body response followed H, step 50: its bass then passed their lowpasses):
    // its windows only have to stay above 0.1 of the level before; the others above 0.5
    for (other, floor) in [(other, 0.5), (custom, 0.5), (soft, 0.1)] {
        let Some(mut a) = engine(Tuning::default()) else { return };
        play(&mut a);
        let n = 48000;
        let at = 24_037;
        let out = render(a, n, at, engine(other));
        // level in 10 ms windows around the handover: no drop
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        let before = rms(&out[at - 480..at]);
        for k in 0..20 {
            let w = &out[at + k * 480..at + (k + 1) * 480];
            assert!(rms(w) > floor * before, "window {k}: {} re {before}", rms(w));
        }
    }
}

const KNIGHT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/upright_a");

#[test]
fn a_moved_pair_sounds_as_if_it_had_always_been_there() {
    // the same strings at another position (the Player's, its bridge zones): the new responses
    // take the old ones' input history, so after the short crossfade the output is the engine's
    // that played there all along, the strings' ring and the room's included
    let at_player = Tuning { mic: pianoengine::mic::MicPosition::Player, ..Tuning::default() };
    let mut moved = at_player;
    moved.mic = pianoengine::mic::MicPosition::Custom;
    moved.mic_setup = at_player.mics_for(pianoengine::mic::Body::Grand).unwrap();
    moved.mic_setup.mics[0].pos[0] += 0.3;
    moved.mic_setup.mics[1].pos[0] += 0.3;
    let (Some(mut a), Some(mut c)) = (engine(at_player), engine(moved)) else { return };
    play(&mut a);
    play(&mut c);
    let (n, at) = (72000, 36_037); // mid-block, notes and pedal ringing
    let handed = render(a, n, at, engine(moved));
    let there = render(c, n, at, None);
    let from = at + 4800; // past the crossfade and the long stage's pending output
    let peak = there[from..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let diff = there[from..].iter().zip(&handed[from..]).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
    assert!(diff < peak * 1e-4, "difference {diff} re peak {peak}");
}

fn upright(t: Tuning) -> Option<Engine> {
    common::engine(KNIGHT, SR, t)
}

#[test]
fn upright_and_grand_bodies_from_the_export() {
    if let Ok(i) = Instrument::load(KNIGHT) {
        assert_eq!(i.body(), pianoengine::mic::Body::Upright);
    }
    if let Ok(i) = Instrument::load(DIR) {
        assert_eq!(i.body(), pianoengine::mic::Body::Grand);
    }
}

#[test]
fn upright_positions_change_the_sound_and_recording_does_not() {
    let n = 24000;
    let out = |mic| {
        let mut e = upright(Tuning { mic, ..Tuning::default() })?;
        play(&mut e);
        Some(render(e, n, n, None))
    };
    let Some(rec) = out(pianoengine::mic::MicPosition::Recording) else { return };
    let Some(rec2) = out(pianoengine::mic::MicPosition::Recording) else { return };
    assert_eq!(rec, rec2, "Recording must stay the fitted sound, deterministically");
    for mic in [1, 2, 3, 4].map(pianoengine::mic::MicPosition::from_index) {
        let o = out(mic).unwrap();
        let diff: f32 = o.iter().zip(&rec).map(|(a, b)| (a - b).powi(2)).sum();
        let e: f32 = rec.iter().map(|a| a * a).sum();
        assert!(diff > 0.01 * e, "{mic:?} sounds like Recording");
    }
}

#[test]
fn upright_positions_switch_without_gap() {
    use pianoengine::mic::MicPosition::*;
    // positions differ in level (close 1 is ~4 dB below close 2 in the left channel),
    // so each window after the switch is compared with both positions played alone
    let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
    let (n, at) = (48000, 24_037);
    let alone = |mic| {
        let mut e = upright(Tuning { mic, ..Tuning::default() })?;
        play(&mut e);
        Some(render(e, n, n, None))
    };
    for (from, to) in [(Recording, Tail), (Tail, Hammers), (Hammers, Player), (Player, Audience)] {
        let Some(mut a) = upright(Tuning { mic: from, ..Tuning::default() }) else { return };
        play(&mut a);
        let out = render(a, n, at, upright(Tuning { mic: to, ..Tuning::default() }));
        let (fa, ta) = (alone(from).unwrap(), alone(to).unwrap());
        for k in 0..20 {
            let r = at + k * 480..at + (k + 1) * 480;
            let floor = rms(&fa[r.clone()]).min(rms(&ta[r.clone()]));
            assert!(rms(&out[r.clone()]) > 0.5 * floor, "{from:?}→{to:?} window {k}: {} re {floor}", rms(&out[r]));
        }
    }
}


#[test]
fn export_without_body_is_a_grand() {
    // exports from before the "body" field: the grand
    let Ok(json) = std::fs::read_to_string(format!("{DIR}/instrument.json")) else { return };
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v.as_object_mut().unwrap().remove("body");
    let dir = std::env::temp_dir().join(format!("pianoengine-nobody-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("instrument.json"), v.to_string()).unwrap();
    std::fs::copy(format!("{DIR}/data.bin"), dir.join("data.bin")).unwrap();
    let body = Instrument::load(&dir).map(|i| i.body());
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(body.unwrap(), pianoengine::mic::Body::Grand);
}
