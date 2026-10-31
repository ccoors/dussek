//! An instrument is its instrument.json and its data.bin: the level the export measured lives in
//! instrument.json (`level`), and instrument.json names the data.bin it was written with (`data`:
//! its size and SHA-256), so a data.bin from another export, or a cut one, is refused.

use std::path::{Path, PathBuf};

use dussek::Instrument;
use dussek::instrument::{stored_dynamics, stored_level};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments");

/// removes its folder when dropped
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// a copy of the export `name` in a fresh folder (`tag` names it), with data.bin replaced by `data`
/// (None: its own)
fn copy(name: &str, tag: &str, data: Option<Vec<u8>>) -> (Scratch, PathBuf) {
    let src = Path::new(ROOT).join(name);
    let tmp = std::env::temp_dir().join(format!("pe-instrument-file-{tag}-{}", std::process::id()));
    let dir = tmp.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let tmp = Scratch(tmp);
    std::fs::copy(src.join("instrument.json"), dir.join("instrument.json")).unwrap();
    let bin = data.unwrap_or_else(|| std::fs::read(src.join("data.bin")).unwrap());
    std::fs::write(dir.join("data.bin"), bin).unwrap();
    (tmp, dir)
}

#[test]
fn the_level_lives_in_instrument_json() {
    let dir = Path::new(ROOT).join("grand_a");
    if !dir.join("data.bin").exists() {
        return;
    }
    assert!(!dir.join("level.json").exists(), "level.json is gone: the level is instrument.json's");
    // Grand A's level as level.json held it before (2026-10-31): the same measurement, moved
    let db = stored_level(&dir).expect("instrument.json's level");
    assert!((db - -15.366565676475451).abs() < 0.01, "ff level {db:.3} dB");
    assert!(!stored_dynamics(&dir).is_empty(), "a hammer piano's level table");
    // a plucked instrument: its ff level, no table
    let harp = Path::new(ROOT).join("harp");
    assert!(stored_level(&harp).is_some() && stored_dynamics(&harp).is_empty());
}

#[test]
fn a_data_bin_from_elsewhere_is_refused() {
    if !Path::new(ROOT).join("grand_a/data.bin").exists() {
        return;
    }
    let own = std::fs::read(Path::new(ROOT).join("grand_a/data.bin")).unwrap();
    let (_t, ok) = copy("grand_a", "ok", None);
    assert!(Instrument::load(&ok).is_ok());
    let msg = |r: Result<Instrument, dussek::instrument::Error>| r.err().map(|e| e.to_string()).unwrap_or_default();
    // one byte short
    let (_t, cut) = copy("grand_a", "cut", Some(own[..own.len() - 1].to_vec()));
    let m = msg(Instrument::load(&cut));
    assert!(m.contains("data.bin"), "a cut data.bin: {m:?}");
    // the same size, another export's bytes
    let mut other = own.clone();
    other[own.len() / 2] ^= 1;
    let (_t, swapped) = copy("grand_a", "swapped", Some(other));
    let m = msg(Instrument::load(&swapped));
    assert!(m.contains("data.bin"), "another data.bin: {m:?}");
}

#[test]
fn an_instrument_without_the_tie_loads() {
    // a hand-made or older instrument.json names no data.bin: it loads as before
    if !Path::new(ROOT).join("grand_a/data.bin").exists() {
        return;
    }
    let (_t, dir) = copy("grand_a", "untied", None);
    let path = dir.join("instrument.json");
    let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    doc.as_object_mut().unwrap().remove("data");
    std::fs::write(&path, doc.to_string()).unwrap();
    assert!(Instrument::load(&dir).is_ok());
}
