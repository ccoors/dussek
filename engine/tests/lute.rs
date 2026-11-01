//! Harpsichord B's 8′ II choir (register 2) and its lute row: a second pluck point on
//! the 8′ II strings near the nut (`Params::lute`). Skipped without the exports.

use dussek::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");
const SR: f64 = 48000.0;

fn load(name: &str) -> Option<Instrument> {
    Instrument::load(format!("{ROOT}{name}")).ok()
}

#[test]
fn flemish_has_the_8ii_and_the_lute_on_it() {
    let Some(inst) = load("flemish") else { return };
    assert_eq!(inst.registers(), 3);
    assert_eq!(inst.register_names[2], "8′ II");
    assert!(inst.has_lute());
    assert!(inst.notes[..176].iter().all(|n| n.spec.lute.is_none()));
    for k in 0..88 {
        let n = &inst.notes[176 + k];
        let (beta, gain) = n.spec.lute.expect("lute on the 8′ II");
        let own = n.pluck.expect("plucked").beta;
        assert!(beta > 0.0 && beta < own && gain.is_finite(), "key {}: {beta} vs {own}", 21 + k);
        assert!(own < inst.notes[k].pluck.unwrap().beta, "the 8′ II plucks nearer the nut");
        assert!(n.knock.is_none());
    }
    for name in ["harpsichord", "grand_a"] {
        if let Some(i) = load(name) {
            assert!(!i.has_lute(), "{name}");
        }
    }
}

#[test]
fn choir_is_detuned() {
    let Some(inst) = load("flemish") else { return };
    let cents: Vec<f64> = (0..88).map(|k| 1200.0 * (inst.notes[176 + k].f_mean[0] / inst.notes[k].f_mean[0]).log2()).collect();
    assert!(cents.iter().all(|c| c.abs() <= 1.5 + 0.05), "{cents:?}");
    assert!(cents.iter().any(|c| c.abs() > 0.3), "the choirs beat");
}

#[test]
fn rejects_a_bad_lute_block() {
    let src = format!("{ROOT}flemish");
    let Ok(json) = std::fs::read_to_string(format!("{src}/instrument.json")) else { return };
    let dir = std::env::temp_dir().join(format!("pe_lute_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(format!("{src}/data.bin"), dir.join("data.bin")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut try_load = |lute: serde_json::Value| {
        doc["lute"] = lute;
        std::fs::write(dir.join("instrument.json"), doc.to_string()).unwrap();
        Instrument::load(&dir)
    };
    let good = serde_json::json!({"register": 2, "beta": vec![0.05; 88], "gain_db": -2.0});
    assert!(try_load(good).expect("a good block loads").has_lute());
    for bad in [
        serde_json::json!({"register": 3, "beta": vec![0.05; 88], "gain_db": 0.0}),
        serde_json::json!({"register": 2, "beta": vec![0.05; 87], "gain_db": 0.0}),
        serde_json::json!({"register": 2, "beta": vec![0.5; 88], "gain_db": 0.0}),
        serde_json::json!({"register": 2, "beta": vec![0.0; 88], "gain_db": 0.0}),
    ] {
        assert!(try_load(bad.clone()).is_err(), "{bad}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// quiet: no knock, release noise, sympathetic strings or room (renders compare)
fn quiet(stops: u32, lute: bool) -> Params {
    Params { stops, lute, knock: 0.0, release: 0.0, sympathetic: 0.0, room: 0.0, ..Params::default() }
}

/// n samples (left) of `events`, 256-sample blocks; `switch`: new params from that sample
fn run(inst: &Instrument, p: Params, events: &[(u64, Event)], n: usize, switch: Option<(usize, Params)>) -> Vec<f32> {
    let mut e = Engine::with_tuning(inst.clone(), SR, Tuning::default()).expect("engine");
    e.set_params(p);
    for &(t, ev) in events {
        e.queue(t, ev);
    }
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    let at = switch.map_or(n, |(t, _)| t);
    for (cl, cr) in l[..at].chunks_mut(256).zip(r[..at].chunks_mut(256)) {
        e.process(cl, cr);
    }
    if let Some((_, q)) = switch {
        e.set_params(q);
    }
    for (cl, cr) in l[at..].chunks_mut(256).zip(r[at..].chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

const C4: [(u64, Event); 1] = [(0, Event::NoteOn { key: 60, speed: 1.0 })];

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64).powi(2)).sum()
}

/// level (dB) of x at f (Hann window)
fn level(x: &[f32], f: f64) -> f64 {
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n).cos();
        let ph = 2.0 * std::f64::consts::PI * f * i as f64 / SR;
        re += w * v as f64 * ph.cos();
        im -= w * v as f64 * ph.sin();
    }
    20.0 * (re.hypot(im) + 1e-30).log10()
}

#[test]
fn the_8ii_stop_sounds_register_2() {
    let Some(inst) = load("flemish") else { return };
    let x = run(&inst, quiet(4, false), &C4, 9600, None);
    assert!(energy(&x) > 0.0);
    let none = run(&inst, quiet(0, false), &C4, 9600, None);
    assert_eq!(energy(&none), 0.0);
}

#[test]
fn lute_plucks_the_8ii_strings_nearer_the_nut() {
    // nearer the nut: the upper partials stronger re the fundamental than the 8′ II row's
    let Some(inst) = load("flemish") else { return };
    let f = &inst.notes[176 + 39].f_mean;
    let tilt = |x: &[f32]| {
        let w = &x[2400..2400 + 8192];
        let hi: f64 = (4..12).map(|k| 10f64.powf(level(w, f[k]) / 10.0)).sum();
        10.0 * hi.log10() - level(w, f[0])
    };
    let row = tilt(&run(&inst, quiet(4, false), &C4, 12000, None));
    let lute = tilt(&run(&inst, quiet(0, true), &C4, 12000, None));
    assert!(lute > row + 3.0, "lute {lute:.1} dB vs 8′ II {row:.1} dB");
}

#[test]
fn lute_wins_over_the_8ii_row() {
    let Some(inst) = load("flemish") else { return };
    let both = run(&inst, quiet(4, true), &C4, 9600, None);
    let lute = run(&inst, quiet(0, true), &C4, 9600, None);
    assert_eq!(both, lute);
}

#[test]
fn lute_at_the_rows_own_point_is_the_row() {
    let Some(mut inst) = load("flemish") else { return };
    for n in inst.notes[176..].iter_mut() {
        let own = n.pluck.unwrap().beta;
        n.spec.lute = Some((own, 0.0));
    }
    assert_eq!(run(&inst, quiet(0, true), &C4, 9600, None), run(&inst, quiet(4, false), &C4, 9600, None));
}

#[test]
fn switch_waits_for_the_next_pluck() {
    let Some(inst) = load("flemish") else { return };
    let kept = run(&inst, quiet(4, false), &C4, 24064, None);
    let switched = run(&inst, quiet(4, false), &C4, 24064, Some((12032, quiet(4, true))));
    assert_eq!(kept, switched, "a ringing string carries on");
    // the next pluck of the key is the lute's
    let again = [(0, Event::NoteOn { key: 60, speed: 1.0 }), (12032, Event::NoteOn { key: 60, speed: 1.0 })];
    let a = run(&inst, quiet(4, false), &again, 24064, None);
    let b = run(&inst, quiet(4, false), &again, 24064, Some((12032, quiet(4, true))));
    assert_eq!(a[..12032], b[..12032]);
    assert_ne!(a[12032..], b[12032..]);
}

#[test]
fn old_stops_leave_register_2_silent() {
    // a saved 8′ + 4′ (stops 3) plucks no 8′ II string: making register 2's plucks
    // 1000× louder changes nothing (it would dominate if they were plucked)
    let Some(inst) = load("flemish") else { return };
    let mut loud = inst.clone();
    for n in loud.notes[176..].iter_mut() {
        if let Some(p) = n.phys.pluck.as_mut() {
            p.amp *= 1000.0;
        }
    }
    let x = run(&inst, quiet(3, false), &C4, 9600, None);
    assert!(energy(&x) > 0.0);
    assert_eq!(x, run(&loud, quiet(3, false), &C4, 9600, None));
    assert_ne!(run(&inst, quiet(7, false), &C4, 9600, None), run(&loud, quiet(7, false), &C4, 9600, None));
}

#[test]
fn instruments_without_a_lute_ignore_it() {
    for name in ["harpsichord", "grand_a"] {
        let Some(inst) = load(name) else { continue };
        assert_eq!(run(&inst, quiet(1, false), &C4, 9600, None), run(&inst, quiet(1, true), &C4, 9600, None), "{name}");
    }
}

#[test]
fn a_lute_pluck_falls_back_as_register_2() {
    // the lute row's jack falls at key-up: a release noise for register 2
    let Some(inst) = load("flemish") else { return };
    let ev = [(0, Event::NoteOn { key: 60, speed: 1.0 }), (4864, Event::KeyUp { key: 60 })];
    let p = |release: f32| Params { stops: 0, lute: true, release, knock: 0.0, sympathetic: 0.0, room: 0.0, ..Params::default() };
    assert_ne!(run(&inst, p(1.0), &ev, 9600, None), run(&inst, p(0.0), &ev, 9600, None));
}

#[test]
fn the_lute_row_buzzes_with_paper_too() {
    // the paper lies on the strings whichever row plucks them
    let Some(inst) = load("flemish") else { return };
    let paper = Params { prep: dussek::Prep::Paper, paper: 1.0, ..quiet(0, true) };
    assert_ne!(run(&inst, paper, &C4, 24000, None), run(&inst, quiet(0, true), &C4, 24000, None));
}

#[test]
fn the_lute_row_plucks_as_hard_as_the_8ii_row() {
    // one quill force: near the nut the string gives way less (its displacement ∝ β(1−β)), so the
    // fundamental falls by sin(πβ_lute) / sin(πβ_II) and the lute plays under the row, not over it
    let Some(inst) = load("flemish") else { return };
    let n = &inst.notes[176 + 39];
    let (b_lute, _) = n.spec.lute.unwrap();
    let b_row = n.pluck.unwrap().beta;
    let want = 20.0 * ((std::f64::consts::PI * b_lute).sin() / (std::f64::consts::PI * b_row).sin()).log10();
    let f0 = n.f_mean[0];
    let row = run(&inst, quiet(4, false), &C4, 12000, None);
    let lute = run(&inst, quiet(0, true), &C4, 12000, None);
    let got = level(&lute[2400..2400 + 8192], f0) - level(&row[2400..2400 + 8192], f0);
    assert!((got - want).abs() < 1.0, "the lute's fundamental {got:.1} dB re the row's, want {want:.1}");
    assert!(energy(&lute) < energy(&row), "the lute louder than its row");
}
