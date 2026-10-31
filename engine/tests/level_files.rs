//! Every exported instrument's level.json holds the ff level the engine plays it at
//! (`Instrument::ff_level`, which a morph reads instead of measuring) and a hammer piano's level
//! over its hammer speed (`Instrument::level_curve`, which maps its velocity): a change to the
//! engine that moves either fails here until its level.json is written again.

mod common;
use std::path::{Path, PathBuf};

use dussek::Instrument;
use dussek::instrument::stored_level;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments");

/// what is wrong with `dir`'s stored level (None: it matches within 0.1 dB)
fn mismatch(dir: &Path) -> Option<String> {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let refresh = format!("cargo run --release --manifest-path engine/Cargo.toml --bin level -- instruments/{name}");
    let inst = match Instrument::load(dir) {
        Ok(i) => i,
        Err(e) => return Some(format!("{name}: {e}")),
    };
    let (plays, curve) = match inst.ff_level().and_then(|db| Ok((db, inst.level_curve()?))) {
        Ok(x) => x,
        Err(e) => return Some(format!("{name}: {e}")),
    };
    // the table as loaded (a hammer piano's) against the engine's within 0.2 dB
    let (a, b) = (&inst.dynamics, &curve);
    let off = a.keys != b.keys
        || a.ln_speed.len() != b.ln_speed.len()
        || a.ln_speed.iter().zip(&b.ln_speed).any(|(x, y)| (x - y).abs() > 1e-6)
        || a.db.iter().flatten().zip(b.db.iter().flatten()).any(|(x, y)| (x - y).abs() > 0.2);
    if off {
        return Some(format!("{name}: the stored level table is not the one the engine plays: run {refresh}"));
    }
    match stored_level(dir) {
        None => Some(format!("{name}: no level.json (plays {plays:.2} dB): run {refresh}")),
        Some(db) if (db - plays).abs() > 0.1 => Some(format!("{name}: stored {db:.2} dB, plays {plays:.2} dB: run {refresh}")),
        Some(_) => None,
    }
}

#[test]
fn every_stored_level_matches_the_engine() {
    let Ok(entries) = std::fs::read_dir(ROOT) else { return };
    let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("instrument.json").exists()).collect();
    dirs.sort();
    let wrong: Vec<String> = common::par(dirs.len(), |i| mismatch(&dirs[i])).into_iter().flatten().collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn an_altered_level_fails_the_check() {
    let a = Path::new(ROOT).join("grand_a");
    if !a.exists() {
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("level_altered").join("grand_a");
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["instrument.json", "data.bin"] {
        std::fs::copy(a.join(f), dir.join(f)).unwrap();
    }
    let db = Instrument::load(&dir).unwrap().ff_level().unwrap() + 1.0;
    std::fs::write(dir.join("level.json"), format!("{{\"ff_db\": {db}, \"keys\": [48, 60, 72]}}")).unwrap();
    assert!(mismatch(&dir).is_some_and(|m| m.contains("stored")));
}
