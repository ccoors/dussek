//! The Rust players against `live.py`: `PE_BRASS_PORT` a folder `proto/brass/port_check.py` wrote (the events, Python's random
//! numbers, the result), else this returns early.
use std::process::Command;

#[test]
fn the_port_is_the_player() {
    let Some(dir) = std::env::var_os("PE_BRASS_PORT") else { return };
    let out = Command::new(env!("CARGO_BIN_EXE_brass_check")).arg(dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}
