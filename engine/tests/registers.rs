//! Instruments as lists of registers (format 6; formats 4/5 as one register) and the
//! stops. Fixture: instruments/registers_fixture (Salamander twice, export.py --add-register
//! grand_a --octave 0): two identical registers. Skipped without the exports.

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn load(name: &str) -> Option<Instrument> {
    Instrument::load(format!("{ROOT}{name}")).ok()
}

#[test]
fn format_6_loads_two_registers_register_major() {
    let Some(inst) = load("registers_fixture") else { return };
    assert_eq!(inst.registers(), 2);
    assert_eq!(inst.notes.len(), 176);
    for (i, n) in inst.notes.iter().enumerate() {
        assert_eq!(n.register as usize, i / 88);
        assert_eq!(n.note as usize, 21 + i % 88);
    }
    // identical registers compile identically (the folded correction equals the main one)
    for k in 0..88 {
        assert_eq!(inst.notes[k].w, inst.notes[88 + k].w);
        assert_eq!(inst.notes[k].bridge, inst.notes[88 + k].bridge);
    }
}

#[test]
fn formats_4_and_5_are_one_register() {
    for name in ["grand_a", "grand_b", "harpsichord"] {
        let Some(inst) = load(name) else { continue };
        assert_eq!(inst.registers(), 1, "{name}");
        assert!(inst.notes.iter().all(|n| n.register == 0));
    }
}

fn engine(name: &str, t: Tuning) -> Option<Engine> {
    Some(Engine::with_tuning(load(name)?, SR, t).expect("engine"))
}

/// render n samples with these stops (knock, release, sympathetic and the fixture's
/// damper-lift kick off: sums compare; the kick rings every register's lifted strings)
fn play(e: &mut Engine, stops: u32, events: &[(u64, Event)], n: usize) -> Vec<f32> {
    // no strike variation: each register's strikes would draw their own (two hammers)
    e.set_params(Params { stops, knock: 0.0, release: 0.0, sympathetic: 0.0, pedal: 0.0, variation: 0.0, ..Params::default() });
    for &(t, ev) in events {
        e.queue(t, ev);
    }
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

const CHORD: [(u64, Event); 3] = [
    (0, Event::NoteOn { key: 48, speed: 2.0 }),
    (0, Event::NoteOn { key: 60, speed: 2.0 }),
    (24000, Event::KeyUp { key: 60 }),
];

#[test]
fn each_stop_plays_its_register_and_both_add() {
    let (Some(mut a), Some(mut b), Some(mut c)) =
        (engine("registers_fixture", Tuning::default()), engine("registers_fixture", Tuning::default()), engine("registers_fixture", Tuning::default()))
    else {
        return;
    };
    let one = play(&mut a, 1, &CHORD, 48000);
    let other = play(&mut b, 2, &CHORD, 48000);
    let both = play(&mut c, 3, &CHORD, 48000);
    assert!(one.iter().any(|v| v.abs() > 1e-4));
    let peak = one.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    // identical registers play identically (but for the order of float sums: ~4e-7 of the peak)
    for (x, y) in one.iter().zip(&other) {
        assert!((x - y).abs() <= 1e-5 * peak, "identical registers play identically");
    }
    for (x, y) in both.iter().zip(&one) {
        assert!((x - 2.0 * y).abs() <= 1e-4 * peak, "8′+4′ is the sum");
    }
}

#[test]
fn no_stops_is_silent_on_a_register_instrument() {
    let Some(mut e) = engine("registers_fixture", Tuning::default()) else { return };
    // pedal 0: the fixture (Salamander) rings its strings when a key lifts their dampers
    e.set_params(Params { stops: 0, pedal: 0.0, ..Params::default() });
    for &(t, ev) in &CHORD {
        e.queue(t, ev);
    }
    let (mut l, mut r) = (vec![0.0; 48000], vec![0.0; 48000]);
    e.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|v| *v == 0.0), "no pluck, knock or release");
}

#[test]
fn a_single_register_instrument_ignores_the_stops() {
    let (Some(mut a), Some(mut b)) = (engine("grand_a", Tuning::default()), engine("grand_a", Tuning::default())) else { return };
    assert_eq!(play(&mut a, 1, &CHORD, 24000), play(&mut b, 0, &CHORD, 24000));
}

#[test]
fn switching_stops_leaves_ringing_strings_alone() {
    let (Some(mut a), Some(mut b)) = (engine("registers_fixture", Tuning::default()), engine("registers_fixture", Tuning::default())) else { return };
    let steady = play(&mut a, 1, &CHORD[..2], 24000);
    let mut switched = play(&mut b, 1, &CHORD[..2], 12000);
    b.set_params(Params { stops: 2, knock: 0.0, release: 0.0, sympathetic: 0.0, pedal: 0.0, ..Params::default() });
    let (mut l, mut r) = (vec![0.0; 12000], vec![0.0; 12000]);
    b.process(&mut l, &mut r);
    switched.extend_from_slice(&l);
    assert_eq!(steady, switched);
}

#[test]
fn key_up_damps_every_register() {
    let Some(mut e) = engine("registers_fixture", Tuning::default()) else { return };
    let x = play(&mut e, 3, &[(0, Event::NoteOn { key: 60, speed: 2.0 }), (9600, Event::KeyUp { key: 60 })], 96000);
    let rms = |s: &[f32]| (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
    assert!(rms(&x[86400..]) < 0.01 * rms(&x[2400..9600]));
}

#[test]
fn a_rebuild_takes_over_both_registers() {
    let (Some(mut a), Some(mut b)) =
        (engine("registers_fixture", Tuning::default()), engine("registers_fixture", Tuning { pitch_ratio: 1.01, ..Tuning::default() }))
    else {
        return;
    };
    let before = play(&mut a, 3, &CHORD[..2], 9600);
    assert!(b.adopt(&mut a));
    let (mut l, mut r) = (vec![0.0; 9600], vec![0.0; 9600]);
    b.process(&mut l, &mut r);
    let rms = |s: &[f32]| (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
    assert!(rms(&l[..2400]) > 0.3 * rms(&before[7200..]), "gap after the handover");
}

#[test]
fn registers_draw_their_own_wear() {
    let Some(mut inst) = load("registers_fixture") else { return };
    let worn = Tuning { wear: pianoengine::physics::Wear { tuning: 1.0, seed: 7, ..Default::default() }, ..Tuning::default() };
    inst.compile(SR, worn);
    // a worn instrument's registers drift apart (they beat), each by its own draw
    let differ = (0..88).filter(|&k| inst.notes[k].w != inst.notes[88 + k].w).count();
    assert!(differ > 80, "only {differ} keys differ");
}

/// The fixture with its second register on the 8′ II's stop (a measured second 8′, no 4′), in a
/// directory of its own; None without the fixture.
fn on_the_8b(tag: &str) -> Option<std::path::PathBuf> {
    let src = format!("{ROOT}registers_fixture");
    let json = std::fs::read_to_string(format!("{src}/instrument.json")).ok()?;
    let dir = std::env::temp_dir().join(format!("pe_registers_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(format!("{src}/data.bin"), dir.join("data.bin")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    doc["registers"][1]["stop"] = serde_json::json!(2);
    std::fs::write(dir.join("instrument.json"), doc.to_string()).unwrap();
    Some(dir)
}

#[test]
fn a_register_plays_on_the_stop_it_names() {
    let Some(dir) = on_the_8b("stop") else { return };
    let named = || Engine::with_tuning(Instrument::load(&dir).unwrap(), SR, Tuning::default()).unwrap();
    let Some(mut plain) = engine("registers_fixture", Tuning::default()) else { return };
    // the 8′ II's stop (4) plays the second register, as the 4′'s stop (2) plays it in the fixture
    let second = play(&mut named(), 4, &CHORD, 24000);
    assert!(second.iter().any(|v| v.abs() > 1e-4));
    assert_eq!(second, play(&mut plain, 2, &CHORD, 24000));
    // the 4′'s stop plays nothing here
    assert!(play(&mut named(), 2, &CHORD, 24000).iter().all(|v| *v == 0.0));
    let report = pianoengine::info::info(&dir).unwrap();
    assert_eq!((report.registers, report.stops), (2, 0b101));
    assert_eq!(pianoengine::info::info(format!("{ROOT}registers_fixture")).unwrap().stops, 0b11);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn rejects_two_registers_on_one_stop() {
    let Some(dir) = on_the_8b("twice") else { return };
    let path = dir.join("instrument.json");
    let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    doc["registers"][1]["stop"] = serde_json::json!(0);
    std::fs::write(&path, doc.to_string()).unwrap();
    assert!(Instrument::load(&dir).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
