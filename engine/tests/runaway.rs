//! The runaway guards (`voice::RUNAWAY`, the engine's output guard) never silence what is played:
//! every exported instrument, hard repeats (soft and loud in turn, 16 a second on four keys) with the
//! pedal down, Live re-strike on and off, at the Volume's top (+24 dB); the output finite. And the
//! output guard clears a runaway at once. Skipped without the exports.

use pianoengine::{Engine, Event, Params, Tuning};

mod common;
use common::par;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn exported() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(ROOT)
        .map(|d| d.flatten().filter(|e| e.path().join("data.bin").exists()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// (voices silenced, the output finite)
fn hammer(name: &str, restrike: bool) -> Option<(usize, bool)> {
    let inst = common::load(&format!("{ROOT}{name}"))?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    e.set_params(Params { restrike: if restrike { 1.0 } else { 0.0 }, volume: 15.85, ..Params::default() });
    e.queue(0, Event::PedalDown);
    let n = 6 * SR as usize;
    for (i, t) in (0..n - 9600).step_by(3000).enumerate() {
        for key in [36, 60, 84, 103] {
            let speed = e.hammer_speed(key, if i % 2 == 0 { 30.0 } else { 127.0 });
            e.queue(t as u64, Event::NoteOn { key, speed });
        }
    }
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    Some((e.stats().runaways, l.iter().chain(&r).all(|x| x.is_finite())))
}

#[test]
fn playing_hard_never_trips_the_guard() {
    let names = exported();
    let r = par(2 * names.len(), |j| hammer(&names[j / 2], j % 2 == 1));
    let bad: Vec<String> = r
        .iter()
        .enumerate()
        .filter_map(|(j, x)| match x {
            Some((0, true)) | None => None,
            Some((n, fin)) => Some(format!("{} (re-strike {}): {n} silenced, finite {fin}", names[j / 2], j % 2 == 1)),
        })
        .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn the_output_guard_clears_a_runaway_at_once() {
    // an instrument 60 dB over its calibration stands in for a runaway (the guard rises with the
    // Volume, not with it): the first sample past +20 dBFS clears every sound, the output is silent
    // from there, and it is counted
    let Some(mut inst) = common::load(&format!("{ROOT}grand_a")) else { return };
    inst.output_gain_db += 60.0;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    let speed = e.hammer_speed(60, 127.0);
    e.queue(0, Event::NoteOn { key: 60, speed });
    let n = SR as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    assert!(l.iter().chain(&r).all(|x| x.abs() <= 10.0), "never past the guard");
    let cut = l.iter().rposition(|x| *x != 0.0).map_or(0, |i| i + 1);
    assert!(cut < n / 2 && e.stats().runaways >= 1, "cleared at once (silent from sample {cut}), counted");
    assert_eq!(e.stats().voices, 0);
}
