//! The `level` tool prints an instrument's ff level (`Instrument::ff_level`) and a hammer piano's
//! level table (`Instrument::level_curve`) as the JSON its instrument.json keeps (`level`).
//! Needs the exported instruments (skipped without them).

use std::process::Command;

use dussek::Instrument;

const A: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

#[test]
fn the_tool_prints_the_measured_level() {
    if !std::path::Path::new(A).exists() {
        return;
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("level_a");
    // empty first: the folder outlives a run (an older tool's level.json once stayed in it)
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["instrument.json", "data.bin"] {
        std::fs::copy(std::path::Path::new(A).join(f), dir.join(f)).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_level")).arg(&dir).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(files.len(), 2, "the tool writes nothing: {files:?}");
    // kept as the export keeps it, read back
    let path = dir.join("instrument.json");
    let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    doc["level"] = v.clone();
    std::fs::write(&path, doc.to_string()).unwrap();
    assert_eq!(v["keys"], serde_json::json!([48, 60, 72]));
    let inst = Instrument::load(&dir).unwrap();
    let want = inst.ff_level().unwrap();
    assert!((v["ff_db"].as_f64().unwrap() - want).abs() < 1e-9, "{v} against {want}");
    // a hammer piano: its level over the hammer speed, read back as the velocity's map
    let want = inst.level_curve().unwrap();
    assert_eq!(inst.dynamics.keys, want.keys);
    assert!(inst.dynamics.db.iter().flatten().zip(want.db.iter().flatten()).all(|(a, b)| (a - b).abs() < 1e-9));
}

#[test]
fn level_doc_holds_every_keys_ff_level() {
    if !std::path::Path::new(A).exists() {
        return;
    }
    let inst = Instrument::load(A).unwrap();
    let doc = inst.level_doc().unwrap();
    let ff: Vec<f64> = doc["key_ff_db"].as_array().expect("key_ff_db").iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(ff.len(), 88);
    assert!(ff.iter().all(|d| d.is_finite() && (-80.0..=6.0).contains(d)), "{ff:?}");
    // C4's: the peak over the level window of the table's own strike at the ff speed
    let sr = inst.reference_sr;
    let mut e = dussek::Engine::with_tuning(inst.clone(), sr, dussek::Tuning::default()).unwrap();
    e.set_params(dussek::Params { knock: 0.0, variation: 0.0, ..dussek::Params::default() });
    e.queue(0, dussek::Event::NoteOn { key: 60, speed: inst.velocity_speed[2] });
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    let mut peak = 0.0f32;
    for _ in 0..(dussek::instrument::LEVEL_WINDOW_S * sr / 256.0).ceil() as usize {
        e.process(&mut l, &mut r);
        peak = l.iter().chain(&r).fold(peak, |m, x| m.max(x.abs()));
    }
    assert!((ff[60 - 21] - 20.0 * (peak as f64).log10()).abs() < 0.01, "{} against {}", ff[39], 20.0 * (peak as f64).log10());
}
