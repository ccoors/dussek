//! An instrument whose board has a mode at or below 0 Hz is refused at load (its microphone
//! response failed on the audio thread: a sparse board's jittered grid once put one at −0.9 Hz).
//! Needs an exported instrument (skipped without).

use dussek::Instrument;

const SRC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/strumstick");

#[test]
fn a_board_mode_below_0_hz_is_refused() {
    if Instrument::load(SRC).is_err() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("pe_board_guard_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let json = std::fs::read_to_string(format!("{SRC}/instrument.json")).unwrap();
    let mut data = std::fs::read(format!("{SRC}/data.bin")).unwrap();
    // the board's first mode (instrument.json board.f: its offset in data.bin, f64)
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let off = v["board"]["f"]["o"].as_u64().unwrap() as usize;
    data[off..off + 8].copy_from_slice(&(-1.0f64).to_le_bytes());
    // a hand edit: instrument.json no longer names this data.bin (its `data` tie), as a hand-made file
    let mut v = v;
    v.as_object_mut().unwrap().remove("data");
    std::fs::write(dir.join("instrument.json"), v.to_string()).unwrap();
    std::fs::write(dir.join("data.bin"), &data).unwrap();
    let r = Instrument::load(&dir);
    std::fs::remove_dir_all(&dir).ok();
    let e = r.err().expect("loaded a board with a mode at −1 Hz");
    assert!(e.to_string().contains("0 Hz"), "{e}");
}
