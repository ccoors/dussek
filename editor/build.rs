//! The engine's sources hashed into PE_ENGINE_HASH: the Compare page's cache follows the engine
//! (a change to its code renders again), not only the export.
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn main() {
    let engine = Path::new("../engine");
    println!("cargo:rerun-if-changed=../engine/src");
    println!("cargo:rerun-if-changed=../engine/Cargo.toml");
    let mut list = Vec::new();
    files(&engine.join("src"), &mut list);
    list.push(engine.join("Cargo.toml"));
    list.sort();
    let mut h = DefaultHasher::new();
    for p in &list {
        p.strip_prefix(engine).unwrap_or(p).hash(&mut h);
        std::fs::read(p).unwrap_or_default().hash(&mut h);
    }
    println!("cargo:rustc-env=PE_ENGINE_HASH={:016x}", h.finish());
}
