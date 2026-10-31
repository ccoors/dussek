//! Controls without effect on an instrument (the UI hides them, spec "Plugin UI"):
//! a render with the control changed equals the one without, bit for bit. The phrase is
//! loud, pedalled, struck again and released so that every control acts on a piano (the pianos'
//! set must be empty, which checks the phrase). Skipped without the exports.
//! The report (`dussek::info`) must name exactly the measured sets, for every exported instrument.

use dussek::{Engine, Event, Instrument, Params, Prep, Tuning};

mod common;
use common::par;

/// an exported instrument, loaded once per process
fn load(name: &str) -> Option<Instrument> {
    common::load(&format!("{ROOT}{name}"))
}

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

type Change = fn(&mut Params, &mut Tuning);
const CONTROLS: &[(&str, Change)] = &[
    ("sympathetic", |p, _| p.sympathetic = 0.0),
    ("pedal_noise", |p, _| p.pedal = 0.0),
    ("knock", |p, _| p.knock = 0.0),
    ("release", |p, _| p.release = 0.0),
    ("hammer_hardness", |p, _| p.hardness = 2.0),
    ("hammer_grooves", |p, _| p.grooves = 1.0),
    ("damper_buzz", |p, _| p.damper_buzz = 1.0),
    ("rattle", |p, _| p.rattle = 1.0),
    ("wear_hammers", |p, _| p.hammer_wear = 1.0),
    ("wear_voicing", |p, _| p.voicing_wear = 1.0),
    ("wear_dampers", |p, _| p.damper_wear = 1.0),
    ("wear_action", |p, _| p.action_wear = 1.0),
    ("phantom", |p, _| p.phantom = 1.0),
    ("unison_width", |_, t| t.unison_width = 0.0),
    ("stretch", |_, t| t.stretch = 1.0),
    ("wear_unisons", |_, t| t.wear.unisons = 1.0),
    ("wear_tuning", |_, t| t.wear.tuning = 1.0),
    ("wear_strings", |_, t| t.wear.strings = 1.0),
    ("wear_soundboard", |_, t| t.wear.soundboard = 1.0),
    ("wear_false_beats", |_, t| t.wear.false_beats = 1.0),
    ("audio_input", |p, _| p.input = 1.0),
    ("dynamics", |p, _| p.dynamics = 20.0),
    ("sustain", |_, t| t.sustain = 2.0),
    ("variation", |p, _| p.variation = 2.0),
    ("board_bass", |_, t| t.board_bass_db = 12.0),
    ("restrike", |p, _| p.restrike = 1.0),
    ("pedal_ring", |p, _| p.pedal_ring = 0.0),
];

fn render(name: &str, change: Option<Change>) -> Option<Vec<f32>> {
    render_with(name, change, &[])
}

/// `render` with more events at the start (the Bebung's controllers)
fn render_with(name: &str, change: Option<Change>, extra: &[Event]) -> Option<Vec<f32>> {
    let inst = load(name)?;
    let (mut p, mut t) = (Params { stops: 3, ..Params::default() }, Tuning::default());
    if let Some(c) = change {
        c(&mut p, &mut t);
    }
    let mut e = Engine::with_tuning(inst, SR, t).expect("engine");
    e.set_params(p);
    // played velocities, as the plugin plays them (the Dynamics control maps them)
    let loud = |key| e.hammer_speed(key, 115.0);
    let (soft, middle, again) = (e.hammer_speed(76, 40.0), e.hammer_speed(64, 90.0), e.hammer_speed(60, 50.0));
    let events = [
        (0, Event::NoteOn { key: 36, speed: loud(36) }),
        (0, Event::NoteOn { key: 48, speed: loud(48) }),
        (0, Event::NoteOn { key: 60, speed: loud(60) }),
        (0, Event::NoteOn { key: 76, speed: soft }),
        (9600, Event::PedalDown),
        // C4 struck again, softer, while it rings (the re-strike)
        (12000, Event::NoteOn { key: 60, speed: again }),
        (24000, Event::KeyUp { key: 36 }),
        (24000, Event::KeyUp { key: 48 }),
        (24000, Event::KeyUp { key: 60 }),
        (24000, Event::KeyUp { key: 76 }),
        (48000, Event::PedalUp),
        (60000, Event::NoteOn { key: 64, speed: middle }),
        (66000, Event::KeyUp { key: 64 }),
    ];
    for &ev in extra {
        e.queue(0, ev);
    }
    for (t, ev) in events {
        e.queue(t, ev);
    }
    let n = 84000;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    // an audio input throughout (off unless the control turns it up)
    let x = noise(n);
    for ((cl, cr), cx) in l.chunks_mut(256).zip(r.chunks_mut(256)).zip(x.chunks(256)) {
        e.process_input(cx, cl, cr);
    }
    l.extend(r);
    Some(l)
}

/// uniform noise at −20 dBFS RMS
fn noise(n: usize) -> Vec<f32> {
    let mut s = 1u64;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * 0.1 * 3f32.sqrt()
        })
        .collect()
}


/// The preparations, each as the plugin sets it (the bolt at the plugin's default mass, 50 %).
const PREPS: &[(&str, Change)] = &[
    ("plucked", |p, _| p.prep = Prep::Plucked),
    ("muted", |p, _| p.prep = Prep::Muted),
    ("harmonic", |p, _| p.prep = Prep::Harmonic),
    ("bolt", |p, t| {
        p.prep = Prep::Bolt;
        t.bolt = 0.5;
    }),
    ("paper", |p, _| p.prep = Prep::Paper),
];

/// a phrase over the keyboard without pedal: E2, C4, E5, D6, C7 at a moderate speed, each held 0.6 s
fn render_phrase(name: &str, change: Option<Change>) -> Option<Vec<f32>> {
    let inst = load(name)?;
    let (mut p, mut t) = (Params { stops: 3, ..Params::default() }, Tuning::default());
    if let Some(c) = change {
        c(&mut p, &mut t);
    }
    let mut e = Engine::with_tuning(inst, SR, t).expect("engine");
    e.set_params(p);
    for (k, key) in [40u8, 60, 76, 86, 96].into_iter().enumerate() {
        let on = ((0.1 + 0.25 * k as f64) * SR) as u64;
        e.queue(on, Event::NoteOn { key, speed: 1.5 });
        e.queue(on + (0.6 * SR) as u64, Event::KeyUp { key });
    }
    let n = (2.0 * SR) as usize;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l.extend(r);
    Some(l)
}


/// every exported instrument (no list: a new export is covered)
fn exported() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(ROOT)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("instrument.json").is_file() && e.path().join("data.bin").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// per exported instrument whose renders both exist: whether the second differs from the first
/// (the two renders of every instrument in parallel)
fn acting(a: impl Fn(&str) -> Option<Vec<f32>> + Sync, b: impl Fn(&str) -> Option<Vec<f32>> + Sync) -> Vec<(String, bool)> {
    let names = exported();
    let r = par(2 * names.len(), |j| if j % 2 == 0 { a(&names[j / 2]) } else { b(&names[j / 2]) });
    names.into_iter().enumerate().filter_map(|(k, n)| match (&r[2 * k], &r[2 * k + 1]) {
        (Some(x), Some(y)) => Some((n, x != y)),
        _ => None,
    }).collect()
}

fn report(name: &str) -> Option<dussek::info::Info> {
    dussek::info::info(format!("{ROOT}{name}")).ok()
}

#[test]
fn the_controls_list_is_the_reports() {
    let names: Vec<&str> = CONTROLS.iter().map(|(n, _)| *n).collect();
    let report: Vec<&str> = dussek::info::CONTROLS.iter().map(|(_, s)| *s).collect();
    assert_eq!(names, report);
}

#[test]
fn the_report_names_the_controls_without_effect() {
    // every instrument's default render, then each (instrument, control): does the change leave
    // it bit-identical
    let names = exported();
    let bases = par(names.len(), |k| render(&names[k], None));
    let same = par(names.len() * CONTROLS.len(), |j| {
        let (k, c) = (j / CONTROLS.len(), j % CONTROLS.len());
        bases[k].as_ref().map(|b| render(&names[k], Some(CONTROLS[c].1)).unwrap() == *b)
    });
    let mut wrong = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let (Some(_), Some(i)) = (&bases[k], report(name)) else { continue };
        let row = &same[k * CONTROLS.len()..(k + 1) * CONTROLS.len()];
        let set: Vec<&str> = CONTROLS.iter().zip(row).filter(|(_, s)| **s == Some(true)).map(|((n, _), _)| *n).collect();
        if set != i.inert_symbols() {
            wrong.push(format!("{name}: measured {set:?}, reported {:?}", i.inert_symbols()));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_report_names_the_preparations_that_act() {
    let names = exported();
    let bases = par(names.len(), |k| render_phrase(&names[k], None));
    let acts = par(names.len() * PREPS.len(), |j| {
        let (k, p) = (j / PREPS.len(), j % PREPS.len());
        bases[k].as_ref().map(|b| render_phrase(&names[k], Some(PREPS[p].1)).unwrap() != *b)
    });
    let mut wrong = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let (Some(_), Some(i)) = (&bases[k], report(name)) else { continue };
        let acting: Vec<u8> = (0..PREPS.len()).filter(|p| acts[k * PREPS.len() + p] == Some(true)).map(|p| p as u8 + 1).collect();
        if acting != i.preparations {
            wrong.push(format!("{name}: measured {acting:?}, reported {:?} (1 plucked … 5 paper)", i.preparations));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The soft pedal: the report's `soft_pedal` where it acts (the engine ignores it on plucked notes).
#[test]
fn the_report_says_where_the_soft_pedal_acts() {
    let soft = [Event::SoftPedal { depth: 1.0 }];
    let mut wrong = Vec::new();
    for (name, acts) in acting(|n| render(n, None), |n| render_with(n, None, &soft)) {
        let Some(i) = report(&name) else { continue };
        if acts != i.soft_pedal {
            wrong.push(format!("{name}: acts {acts}, reported {}", i.soft_pedal));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The Bebung: the report's `bebung` where it acts (the tangents).
#[test]
fn the_report_says_where_the_bebung_acts() {
    let none: Change = |p, _| p.bebung = 0.0;
    let wheel = [Event::ModWheel { value: 1.0 }];
    for (name, acts) in acting(|n| render_with(n, Some(none), &wheel), |n| render_with(n, None, &wheel)) {
        let Some(i) = report(&name) else { continue };
        assert_eq!(acts, i.bebung, "{name}");
    }
}

/// The bend range: the report's `bend` where it acts (the strings bend).
#[test]
fn the_report_says_where_the_bend_acts() {
    let none: Change = |p, _| p.bend_range = 0.0;
    let wheel = [Event::PitchBend { value: 0.5 }];
    for (name, acts) in acting(|n| render_with(n, Some(none), &wheel), |n| render_with(n, None, &wheel)) {
        let Some(i) = report(&name) else { continue };
        assert_eq!(acts, i.bend, "{name}");
    }
}

/// The motor: the report's `motor` where it acts (the resonators' discs).
#[test]
fn the_report_says_where_the_motor_acts() {
    let on: Change = |p, _| {
        p.motor = true;
        p.motor_rate = 6.0;
    };
    for (name, acts) in acting(|n| render(n, None), |n| render(n, Some(on))) {
        let Some(i) = report(&name) else { continue };
        assert_eq!(acts, i.motor, "{name}");
    }
}

/// Spread all keys: the report's `spread` where it acts (at a microphone position, the bodies
/// with a range of their own: the bars, the harps, the guitars, the zithers).
#[test]
fn the_report_says_where_spreading_the_keys_acts() {
    let at: Change = |_, t| t.mic = dussek::mic::MicPosition::Hammers;
    let spread: Change = |_, t| {
        t.mic = dussek::mic::MicPosition::Hammers;
        t.spread_keys = true;
    };
    let mut wrong = Vec::new();
    for (name, acts) in acting(|n| render(n, Some(at)), |n| render(n, Some(spread))) {
        let Some(i) = report(&name) else { continue };
        if acts != i.spread {
            wrong.push(format!("{name}: acts {acts}, reported {}", i.spread));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The stops: on an instrument with registers, the report's `stops` are those that play a note (a
/// recorded second 8′ without a 4′: the 8′ and the 8′ II).
#[test]
fn the_report_says_which_stops_play() {
    // against no stop at all (a key's lifted dampers still let strings ring: the fixture's kick)
    const ALONE: [Change; 4] = [|p, _| p.stops = 0, |p, _| p.stops = 1, |p, _| p.stops = 2, |p, _| p.stops = 4];
    let names: Vec<String> = exported().into_iter().filter(|n| report(n).is_some_and(|i| i.registers > 1)).collect();
    let r = par(names.len() * 4, |j| render(&names[j / 4], Some(ALONE[j % 4])));
    let mut wrong = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let measured = (0..3).filter(|s| r[4 * k + s + 1].is_some() && r[4 * k + s + 1] != r[4 * k]).fold(0u32, |m, s| m | 1 << s);
        let reported = report(name).unwrap().stops;
        if measured != reported {
            wrong.push(format!("{name}: measured {measured:#b}, reported {reported:#b}"));
        }
    }
    assert!(names.len() >= 1 && wrong.is_empty(), "{}", wrong.join("\n"));
}
