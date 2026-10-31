//! Panic (`Engine::silence`): everything stops within the fade, nothing comes back, and the
//! next notes play as from a fresh engine. Needs the exports (skipped without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn engine(name: &str) -> Option<Engine> {
    let inst = Instrument::load(format!("{ROOT}{name}")).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    // the room on: its tail is the longest thing to stop
    e.set_params(Params { room: 1.0, ..Params::default() });
    Some(e)
}

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l.iter().zip(&r).map(|(a, b)| a.abs().max(b.abs())).collect()
}

/// A loud chord held on the pedal (the keys still down at the panic).
fn chord(e: &mut Engine) {
    e.queue(0, Event::PedalDown);
    for key in [33, 45, 57, 64, 69, 76, 88] {
        let speed = e.hammer_speed(key, 120.0);
        e.queue(10, Event::NoteOn { key, speed });
    }
}

#[test]
fn panic_stops_everything_within_the_fade() {
    for name in ["grand_a", "harp", "guitar"] {
        let Some(mut e) = engine(name) else { continue };
        chord(&mut e);
        let before = render(&mut e, 24000);
        let loud = before.iter().fold(0.0f32, |m, &v| m.max(v));
        assert!(loud > 1e-3, "{name}: the chord sounds");
        e.silence();
        let after = render(&mut e, 96000);
        let fade = (dussek::SILENCE_S * SR) as usize;
        // within the fade it falls (no jump up), after it nothing at all, for 2 s
        assert!(after[..fade].iter().all(|&v| v <= 1.05 * loud), "{name}");
        let rest = after[fade + 256..].iter().fold(0.0f32, |m, &v| m.max(v));
        assert!(rest == 0.0, "{name}: {rest} after the fade");
    }
}

#[test]
fn the_fade_has_no_click() {
    // no step: the output's sample-to-sample change within the fade stays as small as the
    // chord's own before it
    let Some(mut e) = engine("grand_a") else { return };
    chord(&mut e);
    let (mut l, mut r) = (vec![0.0f32; 24000], vec![0.0f32; 24000]);
    e.process(&mut l, &mut r);
    let step = |x: &[f32]| x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let own = step(&l[12000..]);
    e.silence();
    let (mut l2, mut r2) = (vec![0.0f32; 2400], vec![0.0f32; 2400]);
    e.process(&mut l2, &mut r2);
    let joined: Vec<f32> = l[23999..].iter().chain(&l2).copied().collect();
    assert!(step(&joined) <= 1.2 * own, "{} re {own}", step(&joined));
}

#[test]
fn after_a_panic_notes_play_as_from_a_fresh_engine() {
    let (Some(mut a), Some(mut fresh)) = (engine("grand_a"), engine("grand_a")) else { return };
    chord(&mut a);
    render(&mut a, 24000);
    a.silence();
    render(&mut a, 48000);
    let speed = a.hammer_speed(60, 90.0);
    a.queue(0, Event::NoteOn { key: 60, speed });
    fresh.queue(0, Event::NoteOn { key: 60, speed });
    let (x, y) = (render(&mut a, 24000), render(&mut fresh, 24000));
    let peak = y.iter().fold(0.0f32, |m, &v| m.max(v));
    let diff = x.iter().zip(&y).fold(0.0f32, |m, (p, q)| m.max((p - q).abs()));
    assert!(peak > 1e-3 && diff < 0.05 * peak, "difference {diff} re {peak}");
}
