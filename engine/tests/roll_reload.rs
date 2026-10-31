//! A changed export of the same name and profile (a rolled piano's next candidate, a re-export)
//! is a new instrument to the engine: a new engine for it takes nothing over from the old one, so
//! the old body's responses are not kept; the same files read again are still taken over.
//! Needs the exported instruments (skipped without them).

use dussek::roll::files::Export;
use dussek::{Engine, Instrument, Tuning};
use std::path::Path;

const A: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

fn engine(i: Instrument) -> Engine {
    Engine::with_tuning(i, 48000.0, Tuning::default()).unwrap()
}

#[test]
fn a_changed_export_is_not_adopted() {
    if !Path::new(A).exists() {
        return;
    }
    let orig = Export::read(Path::new(A)).unwrap();
    let mut changed = orig.clone();
    let r = changed.responses().unwrap();
    let ir: Vec<f64> = r[0].ir.iter().map(|v| v * 0.5).collect();
    changed.set_response(r[0].pol, r[0].channel, &ir).unwrap();
    assert_eq!(changed.json["profile"], orig.json["profile"]);
    let mut old = engine(orig.instrument().unwrap());
    let mut new = engine(changed.instrument().unwrap());
    assert!(!new.adopt(&mut old), "the new candidate took the old one's responses over");
    // the same files again (a tuning change rebuilds from them): taken over as before
    let mut old = engine(orig.instrument().unwrap());
    let mut again = engine(orig.instrument().unwrap());
    assert!(again.adopt(&mut old));
}
