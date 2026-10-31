//! A damper lifts at once and falls onto the strings in a time that depends on the release
//! velocity and the user's factor; a quick re-strike catches it in mid-fall; the release
//! velocity also sets the release thump. Needs the exported instruments (skipped without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const SR: f64 = 32000.0;
const C4: u8 = 60;

fn render(params: Params, events: &[(f64, Event)], seconds: f64) -> Option<Vec<f32>> {
    let inst = Instrument::load(DIR).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    e.set_params(params);
    for &(t, ev) in events {
        e.queue((t * SR) as u64, ev);
    }
    let lat = e.latency();
    let n = (seconds * SR) as usize + lat;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    e.process(&mut l, &mut r);
    Some(l[lat..].to_vec())
}

/// dB of the rms over seconds a … b
fn level(x: &[f32], a: f64, b: f64) -> f64 {
    let w = &x[(a * SR) as usize..(b * SR) as usize];
    10.0 * (w.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w.len() as f64 + 1e-30).log10()
}

fn fall(f: f32) -> Params {
    Params { damper_fall: f, ..Params::default() }
}

fn note() -> (f64, Event) {
    (0.0, Event::NoteOn { key: C4, speed: 3.0 })
}

#[test]
fn a_slower_damper_lets_more_through_after_the_release() {
    let up = (1.0, Event::KeyUp { key: C4 });
    let (Some(instant), Some(normal), Some(slow)) = (
        render(fall(0.0), &[note(), up], 1.4),
        render(fall(1.0), &[note(), up], 1.4),
        render(fall(8.0), &[note(), up], 1.4),
    ) else { return };
    let (a, b, c) = (level(&instant, 1.0, 1.1), level(&normal, 1.0, 1.1), level(&slow, 1.0, 1.1));
    assert!(a < b && b < c, "100 ms after the release: instant {a:.1} dB, normal {b:.1} dB, 8× slower {c:.1} dB");
    // and the sound before the release is the same
    assert_eq!(&instant[..(0.9 * SR) as usize], &normal[..(0.9 * SR) as usize]);
}

#[test]
fn the_release_velocity_sets_the_fall() {
    let rel = |v: u8| [note(), (1.0, Event::ReleaseVelocity { key: C4, velocity: v }), (1.0, Event::KeyUp { key: C4 })];
    let (Some(fast), Some(normal), Some(slow), Some(unknown)) = (
        render(fall(1.0), &rel(127), 1.4),
        render(fall(1.0), &rel(64), 1.4),
        render(fall(1.0), &rel(10), 1.4),
        render(fall(1.0), &[note(), (1.0, Event::KeyUp { key: C4 })], 1.4),
    ) else { return };
    let w = |x: &Vec<f32>| level(x, 1.0, 1.1);
    assert!(w(&fast) < w(&normal) && w(&normal) < w(&slow), "fast {:.1}, normal {:.1}, slow {:.1}", w(&fast), w(&normal), w(&slow));
    // no velocity known: as a normal one
    assert_eq!(normal, unknown);
}

#[test]
fn a_quick_restrike_catches_the_damper_in_mid_fall() {
    // released at 1 s and the key pressed again (silently) 12 ms later, or 200 ms later:
    // the first finds the damper half way down and the string hardly damped, the second
    // finds it on the string for a long time
    let again = |t: f64| [note(), (1.0, Event::KeyUp { key: C4 }), (t, Event::KeyDown { key: C4 })];
    let never = [note()];
    let (Some(quick), Some(late), Some(free)) = (
        render(fall(1.0), &again(1.012), 1.8),
        render(fall(1.0), &again(1.2), 1.8),
        render(fall(1.0), &never, 1.8),
    ) else { return };
    let (q, l, f) = (level(&quick, 1.5, 1.7), level(&late, 1.5, 1.7), level(&free, 1.5, 1.7));
    // the 12 ms the damper falls half way cost 0.4 dB with the dampers measured from release
    // recordings (design log step 119), 2–3× the old loss
    assert!((q - f).abs() < 0.8, "a 12 ms release: {q:.2} dB, never released {f:.2} dB");
    assert!(l < q - 2.0, "a 200 ms release: {l:.2} dB, a 12 ms one {q:.2} dB");
}

#[test]
fn a_faster_release_has_a_louder_thump() {
    // the release noise alone: the render with it on, less the one with it off, at the same damper fall
    let on = |v: u8, release: f32| render(Params { release, ..fall(0.0) }, &[note(), (1.0, Event::ReleaseVelocity { key: C4, velocity: v }), (1.0, Event::KeyUp { key: C4 })], 1.3);
    let thump = |v: u8| -> Option<f64> {
        let (a, b) = (on(v, 1.0)?, on(v, 0.0)?);
        let d: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
        Some(level(&d, 1.0, 1.2))
    };
    let (Some(hard), Some(soft)) = (thump(127), thump(20)) else { return };
    assert!(hard > soft + 3.0, "thump at release velocity 127: {hard:.1} dB, at 20: {soft:.1} dB");
}

#[test]
fn the_pedal_drops_the_dampers_over_the_normal_time_too() {
    // a note held under the pedal, the pedal up at 1 s
    let ev = [(0.0, Event::PedalDown), note(), (0.5, Event::KeyUp { key: C4 }), (1.0, Event::PedalUp)];
    let (Some(instant), Some(normal)) = (render(fall(0.0), &ev, 1.4), render(fall(1.0), &ev, 1.4)) else { return };
    assert!(level(&normal, 1.0, 1.1) > level(&instant, 1.0, 1.1), "the pedal's release is not instant");
}

#[test]
fn a_damper_that_lifts_again_does_not_double_the_remnant() {
    // instant dampers: the key released and pressed again (silently) 12 ms later. The string
    // was damped for that time, so it must not be louder afterwards than one never released
    // (the sympathetic bank used to take the still sounding voice for a free string)
    let p = Params { damper_fall: 0.0, ..Params::default() };
    let again = [note(), (1.0, Event::KeyUp { key: C4 }), (1.012, Event::KeyDown { key: C4 })];
    let (Some(quick), Some(free)) = (render(p, &again, 1.8), render(p, &[note()], 1.8)) else { return };
    let (q, f) = (level(&quick, 1.5, 1.7), level(&free, 1.5, 1.7));
    assert!(q <= f + 0.05, "quickly re-lifted {q:.2} dB, never released {f:.2} dB");
}

#[test]
fn a_key_held_for_a_moment_has_a_smaller_release_thump() {
    // the release noise alone (the render with it less the one without), the same strike and
    // release velocity, the key held for 15 ms or for half a second
    let thump = |held: f64| -> Option<f64> {
        let ev = [note(), (held, Event::KeyUp { key: C4 })];
        let p = |release: f32| Params { release, ..fall(0.0) };
        let (a, b) = (render(p(1.0), &ev, held + 0.4)?, render(p(0.0), &ev, held + 0.4)?);
        let d: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
        Some(level(&d, held, held + 0.2))
    };
    let (Some(short), Some(long)) = (thump(0.015), thump(0.5)) else { return };
    assert!(short < long - 3.0, "thump after 15 ms: {short:.1} dB, after 500 ms: {long:.1} dB");
    // a key held for the full time or more has the full thump
    let Some(longer) = thump(1.5) else { return };
    assert!((longer - long).abs() < 0.5, "thump after 0.5 s {long:.1} dB, after 1.5 s {longer:.1} dB");
}
