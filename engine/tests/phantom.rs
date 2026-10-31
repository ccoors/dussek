//! The phantom partials' level per instrument (`Instrument::phantom`): the engine plays the
//! instrument's level plus the control's (design 2026-10-20-bass-high-band), both in the bass.
use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn render(own: f64, wear: f32) -> Option<Vec<f32>> {
    render_keys(own, wear, &[33, 36])
}

fn render_keys(own: f64, wear: f32, keys: &[u8]) -> Option<Vec<f32>> {
    let mut inst = Instrument::load(format!("{ROOT}grand_a")).ok()?;
    inst.phantom = own;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    e.set_params(Params { phantom: wear, ..Params::default() });
    for &key in keys {
        let speed = e.hammer_speed(key, 115.0);
        e.queue(0, Event::NoteOn { key, speed });
    }
    let n = 24000;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    Some(l.into_iter().chain(r).collect())
}

#[test]
fn the_instruments_phantom_adds_to_the_control() {
    let (Some(own), Some(wear), Some(none)) = (render(0.2, 0.0), render(0.0, 0.2), render(0.0, 0.0)) else { return };
    let d = own.iter().zip(&wear).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(d < 1e-6, "the instrument's 0.2 against the control's 0.2: {d}");
    let moved = own.iter().zip(&none).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(moved > 1e-4, "the level has an effect: {moved}");
}

#[test]
fn the_instruments_phantom_stays_in_the_bass() {
    // measured in A0–A2, none from C3 (design log step 131): the treble's voices keep the shared path
    let (Some(on), Some(off)) = (render_keys(0.2, 0.0, &[72, 84]), render_keys(0.0, 0.0, &[72, 84])) else { return };
    assert_eq!(on, off);
    let (Some(on), Some(off)) = (render_keys(0.2, 0.0, &[45]), render_keys(0.0, 0.0, &[45])) else { return };
    assert!(on.iter().zip(&off).any(|(a, b)| (a - b).abs() > 1e-6), "A2 keeps its phantom partials");
}

#[test]
fn the_controls_phantom_stays_in_the_bass() {
    // over all keys it put loud treble notes 20–30 dB over their neighbors (design log step 190)
    let (Some(on), Some(off)) = (render_keys(0.0, 1.0, &[60, 84, 96]), render_keys(0.0, 0.0, &[60, 84, 96])) else { return };
    assert_eq!(on, off);
    let (Some(on), Some(off)) = (render_keys(0.0, 1.0, &[42]), render_keys(0.0, 0.0, &[42])) else { return };
    assert!(on.iter().zip(&off).any(|(a, b)| (a - b).abs() > 1e-6), "F♯2 plays some");
}
