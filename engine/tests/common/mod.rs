//! Shared by the engine's integration tests (`mod common;`): an instrument loaded once per
//! process (a load takes 80–250 ms), engines built through one radiation cache per instrument
//! and tuning (at a microphone position a build computes its responses, ~0.1 s; with the cache
//! ~20 ms), and a map over every core for independent renders.
#![allow(dead_code)]

use pianoengine::{Engine, Instrument, RadiationCache, Tuning};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// the instrument in `dir`, loaded once per process (None: no export). A directory under the
/// repository's `instruments/` without its `instrument.json` panics: the instruments are committed,
/// so a missing one is a renamed or mistyped name, not missing data.
pub fn load(dir: &str) -> Option<Instrument> {
    let committed = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments"));
    let path = std::path::Path::new(dir);
    if path.parent().is_some_and(|p| p.canonicalize().ok() == committed.canonicalize().ok())
        && !path.join("instrument.json").exists()
    {
        panic!("no instrument at {dir}: renamed?");
    }
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Instrument>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(i) = cache.lock().unwrap().get(dir) {
        return i.clone();
    }
    let i = Instrument::load(dir).ok();
    cache.lock().unwrap().insert(dir.to_string(), i.clone());
    i
}

/// an engine for the instrument in `dir` (None: no export), as `Engine::with_tuning` builds it,
/// through the radiation cache of this instrument, rate and tuning
pub fn engine(dir: &str, sr: f64, t: Tuning) -> Option<Engine> {
    static CACHES: OnceLock<Mutex<HashMap<String, RadiationCache>>> = OnceLock::new();
    let inst = load(dir)?;
    let caches = CACHES.get_or_init(Default::default);
    let key = format!("{dir}|{sr}|{t:?}");
    // taken out while it builds (another thread with the same key builds without it)
    let mut cache = caches.lock().unwrap().remove(&key).unwrap_or_default();
    let e = Engine::with_cache(inst, sr, t, &mut cache).expect("engine");
    caches.lock().unwrap().insert(key, cache);
    Some(e)
}

/// `f` over 0..n on every core, the results in order
pub fn par<R: Send>(n: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |t| t.get()).min(n.max(1));
    let next = AtomicUsize::new(0);
    let out: Vec<Mutex<Option<R>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n {
                    break;
                }
                let r = f(i);
                *out[i].lock().unwrap() = Some(r);
            });
        }
    });
    out.into_iter().map(|m| m.into_inner().unwrap().expect("every index ran")).collect()
}
