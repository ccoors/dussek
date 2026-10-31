//! Every released instrument format keeps loading: engine/tests/formats/<format>/ holds an export
//! of each, frozen as released (never refit or re-exported). Additions to the format stay optional
//! fields with defaults; a change that cannot be optional raises `format` and adds its folder here
//! (docs/compatibility.md).

use std::path::Path;

use dussek::{Engine, Event, Instrument, Tuning};

const FORMATS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/formats");
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments");

fn format_of(dir: &Path) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("instrument.json")).ok()?).ok()?;
    v["format"].as_u64()
}

#[test]
fn every_released_format_loads_and_plays() {
    let mut seen = 0;
    for e in std::fs::read_dir(FORMATS).expect("engine/tests/formats").flatten() {
        let dir = e.path();
        if !dir.join("instrument.json").exists() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(format_of(&dir).map(|f| f.to_string()), Some(name.clone()), "{name}: the folder is named after its format");
        let inst = Instrument::load(&dir).unwrap_or_else(|e| panic!("format {name}: {e}"));
        let sr = inst.reference_sr;
        let (lo, hi) = (inst.notes.first().unwrap().note, inst.notes.last().unwrap().note);
        let mut e = Engine::with_tuning(inst, sr, Tuning::default()).unwrap();
        let keys: Vec<u8> = [36u8, 60, 84].into_iter().filter(|k| (lo..=hi).contains(k)).collect();
        assert!(!keys.is_empty(), "format {name}: none of C2, C4, C6 in {lo}…{hi}");
        for (i, &key) in keys.iter().enumerate() {
            e.queue((i * 4800) as u64, Event::NoteOn { key, speed: 1.5 });
        }
        let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let mut peak = 0.0f32;
        for _ in 0..(2.0 * sr / 256.0) as usize {
            e.process(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|x| x.is_finite()), "format {name}: not finite");
            peak = l.iter().chain(&r).fold(peak, |m, x| m.max(x.abs()));
        }
        assert!(20.0 * peak.log10() > -60.0, "format {name}: silent ({peak})");
        seen += 1;
    }
    assert!(seen >= 1, "no frozen formats in {FORMATS}");
}

#[test]
fn every_export_has_a_frozen_format() {
    // an export of a format no frozen copy has: the format is new, freeze one with it
    let Ok(entries) = std::fs::read_dir(ROOT) else { return };
    for e in entries.flatten() {
        let Some(f) = format_of(&e.path()) else { continue };
        assert!(Path::new(FORMATS).join(f.to_string()).join("instrument.json").exists(), "{}: format {f} has no frozen copy",
            e.file_name().to_string_lossy());
    }
}
