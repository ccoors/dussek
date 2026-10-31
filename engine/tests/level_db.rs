//! A note's level correction (`level_db`, per mallet [pp, mf, ff] dB): a gain on each strike,
//! interpolated in log₂ speed between the anchor speeds, scaling the strike's new modes only.
use pianoengine::engine::level_gain;
use pianoengine::{Engine, Event, Instrument};
use std::path::{Path, PathBuf};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");

#[test]
fn level_gain_interpolates_in_log_speed() {
    let anchors = [0.25, 1.0, 4.0];
    let db = [0.0, 0.0, 6.0];
    assert!((level_gain(&db, 1.0, &anchors) - 1.0).abs() < 1e-12);
    // halfway in log₂ speed between mf (1) and ff (4): 2, +3 dB
    assert!((20.0 * level_gain(&db, 2.0, &anchors).log10() - 3.0).abs() < 1e-9);
    assert!((20.0 * level_gain(&db, 4.0, &anchors).log10() - 6.0).abs() < 1e-9);
}

#[test]
fn level_gain_holds_beyond_the_anchors() {
    let anchors = [0.25, 1.0, 4.0];
    let db = [-3.0, 0.0, 6.0];
    assert!((20.0 * level_gain(&db, 16.0, &anchors).log10() - 6.0).abs() < 1e-9);
    assert!((20.0 * level_gain(&db, 0.01, &anchors).log10() + 3.0).abs() < 1e-9);
}

/// a copy of a committed export with `level_db` set on one key (every mallet), in a temporary
/// directory (none: the test returns early)
fn patched(name: &str, key: u8, db: [f64; 3]) -> Option<PathBuf> {
    patched_or_not(name, key, Some(db))
}

/// the committed export with every note's table removed (it may carry its own corrections), and
/// `db` at `key` when given
fn patched_or_not(name: &str, key: u8, db: Option<[f64; 3]>) -> Option<PathBuf> {
    let src = Path::new(ROOT).join(name);
    let text = std::fs::read_to_string(src.join("instrument.json")).ok()?;
    let mut j: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mallets = j["mallets"].as_array().map_or(1, |m| m.len().max(1));
    let notes = if j["notes"].is_array() { j["notes"].as_array_mut().unwrap() } else { j["registers"][0]["notes"].as_array_mut().unwrap() };
    for n in notes.iter_mut() {
        n.as_object_mut().unwrap().remove("level_db");
    }
    // the phantom partials grow with the square of the strike: the strike's own scaling is measured
    // without them
    j.as_object_mut().unwrap().remove("phantom");
    let notes = if j["notes"].is_array() { j["notes"].as_array_mut().unwrap() } else { j["registers"][0]["notes"].as_array_mut().unwrap() };
    if let Some(db) = db {
        let n = notes.iter_mut().find(|n| n["note"] == key).unwrap();
        n["level_db"] = serde_json::json!(vec![db; mallets]);
    }
    let tag = db.map_or("none".to_string(), |d| format!("{}-{}-{}", d[0], d[1], d[2]));
    let dir = std::env::temp_dir().join(format!("pe-level-{name}-{key}-{tag}-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("instrument.json"), serde_json::to_string(&j).unwrap()).unwrap();
    std::fs::copy(src.join("data.bin"), dir.join("data.bin")).unwrap();
    Some(dir)
}

/// a render of strikes (time s, key, speed), mono, the latency trimmed
fn render(dir: &Path, strikes: &[(f64, u8, f64)], seconds: f64) -> Vec<f32> {
    let inst = Instrument::load(dir).unwrap();
    let mut e = Engine::new(inst, 48000.0).unwrap();
    for &(t, key, speed) in strikes {
        e.queue((t * 48000.0) as u64, Event::NoteOn { key, speed });
    }
    let n = (seconds * 48000.0) as usize + e.latency();
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        e.process(cl, cr);
    }
    l.iter().zip(&r).skip(e.latency()).map(|(a, b)| 0.5 * (a + b)).collect()
}

fn ratio_db(a: &[f32], b: &[f32]) -> f64 {
    let e = |x: &[f32]| x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
    10.0 * (e(a) / e(b)).log10()
}

#[test]
fn six_db_doubles_a_mallet_strike() {
    // the marimba: a bar's mallet strike (the pluck path), at its mf anchor speed
    let (Some(plain), Some(loud)) = (patched("marimba", 60, [0.0, 0.0, 0.0]), patched("marimba", 60, [0.0, 6.0, 0.0])) else { return };
    let inst = Instrument::load(&plain).unwrap();
    let mf = inst.mallets.first().map_or(inst.velocity_speed[1], |m| m.speed[1]);
    let (a, b) = (render(&loud, &[(0.0, 60, mf)], 1.0), render(&plain, &[(0.0, 60, mf)], 1.0));
    let d = ratio_db(&a, &b);
    assert!((d - 6.0).abs() < 0.05, "{d:.3} dB");
}

#[test]
fn six_db_doubles_a_hammer_strike() {
    // Grand A: a hammer's contact
    let (Some(plain), Some(loud)) = (patched("grand_a", 60, [0.0, 0.0, 0.0]), patched("grand_a", 60, [0.0, 6.0, 0.0])) else { return };
    let mf = Instrument::load(&plain).unwrap().velocity_speed[1];
    let (a, b) = (render(&loud, &[(0.0, 60, mf)], 1.0), render(&plain, &[(0.0, 60, mf)], 1.0));
    let d = ratio_db(&a, &b);
    assert!((d - 6.0).abs() < 0.05, "{d:.3} dB");
}

#[test]
fn a_restrike_scales_only_its_own_modes() {
    // the marimba (bars add linearly): 0 dB at mf, +6 dB at ff; a mf strike, then a ff strike at
    // 0.5 s onto its ringing. The table adds to the second strike only: what it adds on top of the
    // first's ringing equals what it adds to the second strike alone (the knock, not corrected,
    // cancels in both differences)
    let (Some(plain), Some(table)) = (patched("marimba", 60, [0.0, 0.0, 0.0]), patched("marimba", 60, [0.0, 0.0, 6.0])) else { return };
    let inst = Instrument::load(&plain).unwrap();
    let sp = inst.mallets.first().map_or(inst.velocity_speed, |m| m.speed);
    let both = [(0.0, 60, sp[1]), (0.5, 60, sp[2])];
    let alone = [(0.5, 60, sp[2])];
    let sub = |a: Vec<f32>, b: Vec<f32>| a.iter().zip(&b).map(|(x, y)| x - y).collect::<Vec<f32>>();
    let diff = sub(render(&table, &both, 1.5), render(&plain, &both, 1.5));
    let second = sub(render(&table, &alone, 1.5), render(&plain, &alone, 1.5));
    let err: Vec<f32> = diff.iter().zip(&second).map(|(d, s)| d - s).collect();
    let db = ratio_db(&err, &second);
    assert!(db < -60.0, "the re-strike's error {db:.1} dB re its own sound");
}

#[test]
fn no_table_renders_bit_identically() {
    let Some(plain) = patched("marimba", 60, [0.0, 0.0, 0.0]) else { return };
    let mf = Instrument::load(&plain).unwrap().mallets.first().map_or(1.0, |m| m.speed[1]);
    let orig = render(&patched_or_not("marimba", 60, None).unwrap(), &[(0.0, 60, mf)], 0.5);
    let zero = render(&plain, &[(0.0, 60, mf)], 0.5);
    assert_eq!(orig, zero, "0 dB everywhere must not change a sample");
}
