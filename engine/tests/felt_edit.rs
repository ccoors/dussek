//! The felt as the player bends it, at the build (design 2026-10-22-felt-controls): each key's felt
//! edited, the level table and each key's ff level measured again, so every key keeps its loudness
//! at every velocity while its tone changes; Hammer hardness on a hammer piano the same way.
mod common;

use dussek::felt::{FeltCurve, FeltEdit, LIMITS, Quantity, apply};
use dussek::instrument::LEVEL_WINDOW_S;
use dussek::{Engine, Event, Instrument, Params, RadiationCache, Tuning};

const SR: f64 = 48000.0;

fn dir(name: &str) -> String {
    format!("{}/../instruments/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn curve(pts: &[(u8, f64)]) -> FeltCurve {
    let mut c = FeltCurve { n: pts.len(), ..Default::default() };
    for (i, &(k, b)) in pts.iter().enumerate() {
        (c.keys[i], c.bend[i]) = (k, b);
    }
    c
}

/// stiffness +12 dB below C3, a 6 N core added at C4–C6, the exponent +0.5 everywhere
fn far_edit() -> FeltEdit {
    let mut e = FeltEdit::default();
    e.curves[Quantity::Stiffness as usize] = curve(&[(21, 12.0), (48, 12.0), (60, 0.0)]);
    e.curves[Quantity::CoreForce as usize] = curve(&[(60, 6.0), (84, 6.0)]);
    e.curves[Quantity::Exponent as usize] = curve(&[(21, 0.5)]);
    e
}

fn edited(inst: &Instrument, edit: &FeltEdit) -> Instrument {
    let rows = inst.level_rows().expect("shipped rows");
    inst.with_felt_edit(edit, &rows).expect("edited")
}

/// `key` struck at `velocity` through the instrument's own level table, the strings alone (the
/// knock off, as the table is measured): the left and right output over `secs`
fn render(inst: &Instrument, key: u8, velocity: f64, secs: f64) -> Vec<f32> {
    let mut e = Engine::with_cache(inst.clone(), SR, Tuning::default(), &mut RadiationCache::default()).expect("engine");
    e.set_params(Params { knock: 0.0, variation: 0.0, ..Params::default() });
    let speed = e.hammer_speed(key, velocity);
    e.queue(0, Event::NoteOn { key, speed });
    let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
    let mut out = Vec::new();
    for _ in 0..(secs * SR / 256.0).ceil() as usize {
        e.process(&mut l, &mut r);
        out.extend(l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]));
    }
    out
}

fn peak_db(x: &[f32]) -> f64 {
    20.0 * (x.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64).log10()
}

/// every 6th key at velocities 20, 32, 64, 112: the largest |edited − shipped| peak level (dB)
fn worst_level_change(a: &Instrument, b: &Instrument) -> f64 {
    let cases: Vec<(u8, f64)> = (21..=108).step_by(6).flat_map(|k| [20.0, 32.0, 64.0, 112.0].map(|v| (k, v))).collect();
    let d = common::par(cases.len(), |i| {
        let (k, v) = cases[i];
        (peak_db(&render(b, k, v, LEVEL_WINDOW_S)) - peak_db(&render(a, k, v, LEVEL_WINDOW_S))).abs()
    });
    d.into_iter().fold(0.0, f64::max)
}

#[test]
fn no_edit_is_bit_identical() {
    let Some(inst) = common::load(&dir("grand_f")) else { return };
    let rows = (Default::default(), Vec::new()); // not read: nothing to measure
    let same = inst.with_felt_edit(&FeltEdit::default(), &rows).expect("edit");
    assert_eq!(render(&inst, 45, 90.0, 0.3), render(&same, 45, 90.0, 0.3));
}

#[test]
fn felt_round_trips_through_the_instrument() {
    let Some(inst) = common::load(&dir("grand_f")) else { return };
    let mut back = inst.clone();
    back.set_felt(&inst.felt());
    for (a, b) in inst.notes.iter().zip(&back.notes) {
        let close = |x: f64, y: f64| (x - y).abs() <= 1e-12 * x.abs().max(y.abs()).max(1e-300);
        assert!(a.felt_k.iter().zip(&b.felt_k).all(|(x, y)| close(*x, *y)), "key {}", a.note);
        assert!(a.core_k.iter().zip(&b.core_k).all(|(x, y)| close(*x, *y)), "key {}", a.note);
        assert!(close(a.felt_p, b.felt_p) && close(a.core_depth, b.core_depth), "key {}", a.note);
    }
    // the shipped felt as the report reads it: every key struck, Grand F's bass core gone by C3
    let f = inst.felt();
    assert!(f.iter().all(|k| k.hammer) && f[0].core && !f[60 - 21].core && f[0].f05 > 1.0);
}

#[test]
fn loudness_is_kept_under_a_far_edit() {
    let Some(inst) = common::load(&dir("grand_f")) else { return };
    let worst = worst_level_change(&inst, &edited(&inst, &far_edit()));
    assert!(worst <= 1.0, "a key's level moved {worst:.2} dB");
    // the edit alone, not leveled, moves them: the check can fail
    let mut raw = inst.clone();
    raw.set_felt(&apply(&inst.felt(), &far_edit()));
    let moved = worst_level_change(&inst, &raw);
    assert!(moved > 2.0, "unleveled, the far edit moved the levels only {moved:.2} dB");
}

#[test]
fn loudness_is_kept_on_a_morph() {
    // a morph plays its sides' blended level table, not its own measured one: the edit keeps the blend's
    let (a, b) = (dir("grand_a"), dir("grand_d"));
    if common::load(&a).is_none() || common::load(&b).is_none() {
        return;
    }
    let inst = Instrument::morph(&[(a.as_str(), 0.5), (b.as_str(), 0.5)]).expect("morph");
    for e in [FeltEdit { hardness_db: 0.01, ..Default::default() }, far_edit()] {
        let worst = worst_level_change(&inst, &edited(&inst, &e));
        assert!(worst <= 1.0, "{}: a key's level moved {worst:.2} dB", e.format());
    }
}

#[test]
fn hammer_hardness_keeps_the_loudness() {
    let Some(inst) = common::load(&dir("grand_f")) else { return };
    for db in [9.0, -12.0] {
        let worst = worst_level_change(&inst, &edited(&inst, &FeltEdit { hardness_db: db, ..Default::default() }));
        assert!(worst <= 1.0, "hardness {db} dB: a key's level moved {worst:.2} dB");
    }
}

/// the power (dB) of `x` (interleaved stereo, mono-summed) between `lo` and `hi` Hz over its first 8192 frames
fn band_db(x: &[f32], lo: f64, hi: f64) -> f64 {
    let n = 8192;
    let m: Vec<f64> = x.chunks(2).take(n).map(|c| (c[0] + c[1]) as f64).collect();
    let (b0, b1) = ((lo * n as f64 / SR) as usize, (hi * n as f64 / SR) as usize);
    let mut p = 0.0;
    for b in b0..b1 {
        let w = 2.0 * std::f64::consts::PI * b as f64 / n as f64;
        let (re, im) = m.iter().enumerate().fold((0.0, 0.0), |(r, i), (t, v)| (r + v * (w * t as f64).cos(), i - v * (w * t as f64).sin()));
        p += re * re + im * im;
    }
    10.0 * p.log10()
}

#[test]
fn the_tone_does_change() {
    // the leveling keeps the loudness, not the tone: a stiffer bass is brighter
    let Some(inst) = common::load(&dir("grand_f")) else { return };
    let mut e = FeltEdit::default();
    e.curves[Quantity::Stiffness as usize] = curve(&[(21, 12.0), (48, 12.0), (60, 0.0)]);
    let bright = |i: &Instrument| {
        let x = render(i, 36, 112.0, 0.2);
        band_db(&x, 2000.0, 4000.0) - band_db(&x, 200.0, 500.0)
    };
    let (a, b) = (bright(&inst), bright(&edited(&inst, &e)));
    assert!(b - a >= 3.0, "2–4 kHz re 200–500 Hz: {a:.1} → {b:.1} dB");
}

#[test]
fn felt_on_a_harpsichord_changes_nothing() {
    let Some(inst) = common::load(&dir("flemish")) else { return };
    assert!(!inst.is_hammer_piano());
    let rows = (Default::default(), Vec::new());
    let same = inst.with_felt_edit(&far_edit(), &rows).expect("edit");
    assert_eq!(render(&inst, 60, 90.0, 0.3), render(&same, 60, 90.0, 0.3));
}

#[test]
fn extremes_stay_finite() {
    // every hammer piano, every limit at once with the hardness at its end, then the other way:
    // every 4th key and the top one at velocity 127, the strike variation at its widest
    let root = format!("{}/../instruments", env!("CARGO_MANIFEST_DIR"));
    let mut pianos: Vec<String> = std::fs::read_dir(&root).map_or(Vec::new(), |d| d.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).map(|e| e.path().display().to_string()).collect());
    pianos.retain(|d| common::load(d).is_some_and(|i| i.is_hammer_piano()));
    pianos.sort();
    let hard = {
        let mut e = FeltEdit { hardness_db: 12.0, ..Default::default() };
        e.curves[Quantity::Stiffness as usize] = curve(&[(21, LIMITS.stiffness_db.1)]);
        e.curves[Quantity::Exponent as usize] = curve(&[(21, 10.0)]);
        e.curves[Quantity::CoreForce as usize] = curve(&[(21, 1000.0)]);
        e.curves[Quantity::CoreDepth as usize] = curve(&[(21, -10.0)]);
        e
    };
    let soft = {
        let mut e = FeltEdit { hardness_db: -24.0, ..Default::default() };
        e.curves[Quantity::Stiffness as usize] = curve(&[(21, LIMITS.stiffness_db.0)]);
        e.curves[Quantity::Exponent as usize] = curve(&[(21, -10.0)]);
        e
    };
    // one instrument per piano and felt, one radiation cache per piano (the felt leaves the body
    // as it is): a copy per case took 15 GB
    let mut insts = Vec::new();
    for d in &pianos {
        for (j, e) in [&hard, &soft].into_iter().enumerate() {
            let mut x = common::load(d).unwrap();
            x.set_felt(&apply(&x.felt(), e));
            insts.push((d.clone(), j, x));
        }
    }
    let keys: Vec<u8> = (21..=108).step_by(4).chain([108]).collect();
    let cases: Vec<(usize, u8)> = (0..insts.len()).flat_map(|i| keys.iter().map(move |&k| (i, k))).collect();
    let caches: std::sync::Mutex<std::collections::HashMap<String, RadiationCache>> = Default::default();
    let bad: Vec<String> = common::par(cases.len(), |i| {
        let (n, k) = cases[i];
        let (d, j, x) = &insts[n];
        let mut cache = caches.lock().unwrap().remove(d).unwrap_or_default();
        let mut e = Engine::with_cache(x.clone(), SR, Tuning::default(), &mut cache).expect("engine");
        caches.lock().unwrap().insert(d.clone(), cache);
        e.set_params(Params { knock: 0.0, variation: 4.0, ..Params::default() });
        let speed = e.hammer_speed(k, 127.0);
        e.queue(0, Event::NoteOn { key: k, speed });
        let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let mut out = Vec::new();
        for _ in 0..(0.3 * SR / 256.0).ceil() as usize {
            e.process(&mut l, &mut r);
            out.extend(l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]));
        }
        let finite = out.iter().all(|v| v.is_finite());
        (!finite || peak_db(&out) > 12.0).then(|| format!("{d} {} key {k}: {:.1} dB", ["hard", "soft"][*j], peak_db(&out)))
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(bad.is_empty(), "{bad:?}");
}
