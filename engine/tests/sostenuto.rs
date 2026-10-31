//! The sostenuto pedal holds up the dampers that are lifted when it goes down, and only
//! those, until it is up again. Needs the exported instruments (skipped without them).

use pianoengine::{Engine, Event, Instrument, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const SR: f64 = 32000.0;

/// RMS (dB) of the left channel over seconds `a`…`b` of a piece made of (time, event).
fn level(events: &[(f64, Event)], a: f64, b: f64) -> Option<f64> {
    let inst = Instrument::load(DIR).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    for &(t, ev) in events {
        e.queue((t * SR) as u64, ev);
    }
    let lat = e.latency();
    let n = (b * SR) as usize + lat;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    e.process(&mut l, &mut r);
    let w = &l[lat + (a * SR) as usize..lat + (b * SR) as usize];
    Some(10.0 * (w.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / w.len() as f64 + 1e-30).log10())
}

fn on(key: u8) -> Event {
    Event::NoteOn { key, speed: 3.0 }
}

const C4: u8 = 60;
use Event::{KeyUp, PedalDown, PedalUp, SostenutoDown, SostenutoUp};

#[test]
fn a_held_key_rings_on_after_its_release() {
    let plain = [(0.0, on(C4)), (1.0, KeyUp { key: C4 })];
    let sost = [(0.0, on(C4)), (0.5, SostenutoDown), (1.0, KeyUp { key: C4 })];
    let (Some(p), Some(s)) = (level(&plain, 3.0, 4.0), level(&sost, 3.0, 4.0)) else { return };
    assert!(s > p + 20.0, "damped {p:.1} dB, held by the sostenuto {s:.1} dB");
}

#[test]
fn a_key_struck_after_the_press_is_damped_as_usual() {
    let late = [(0.5, SostenutoDown), (0.6, on(C4)), (1.0, KeyUp { key: C4 })];
    let held = [(0.0, on(C4)), (0.5, SostenutoDown), (1.0, KeyUp { key: C4 })];
    let (Some(l), Some(h)) = (level(&late, 3.0, 4.0), level(&held, 3.0, 4.0)) else { return };
    assert!(l < h - 40.0, "struck after the press {l:.1} dB, held {h:.1} dB");
}

#[test]
fn releasing_the_pedal_drops_the_dampers() {
    let piece = [(0.0, on(C4)), (0.5, SostenutoDown), (1.0, KeyUp { key: C4 }), (2.0, SostenutoUp)];
    let held = [(0.0, on(C4)), (0.5, SostenutoDown), (1.0, KeyUp { key: C4 })];
    let (Some(ringing), Some(after), Some(never_up)) = (level(&piece, 1.5, 2.0), level(&piece, 3.0, 4.0), level(&held, 3.0, 4.0)) else { return };
    assert!(ringing > after + 20.0, "ringing {ringing:.1} dB, after the pedal's release {after:.1} dB");
    assert!(never_up > after + 20.0, "held on {never_up:.1} dB, released at 2 s {after:.1} dB");
}

#[test]
fn the_sustain_pedal_down_at_the_press_gets_everything_caught() {
    // C4 is released under the sustain pedal, which then comes up: only the sostenuto
    // (pressed while the dampers were off) keeps the string free
    let with = [(0.0, PedalDown), (0.1, on(C4)), (0.4, KeyUp { key: C4 }), (0.5, SostenutoDown), (0.8, PedalUp)];
    let without = [(0.0, PedalDown), (0.1, on(C4)), (0.4, KeyUp { key: C4 }), (0.8, PedalUp)];
    let (Some(w), Some(wo)) = (level(&with, 3.0, 4.0), level(&without, 3.0, 4.0)) else { return };
    assert!(w > wo + 20.0, "with the sostenuto {w:.1} dB, without {wo:.1} dB");
}
