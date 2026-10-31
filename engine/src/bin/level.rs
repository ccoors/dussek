//! An instrument's level as the engine plays it, printed as JSON for its instrument.json's `level`
//! (the exports run it on what they write and store its output): its ff level
//! (`Instrument::ff_level`), which a morph reads instead of measuring the instrument itself, and a
//! hammer piano's level over its hammer speed (`Instrument::level_curve`, `speed_db`), which maps
//! its velocity.
//!
//! Usage: level <instrument dir>   (the JSON on standard output, the ff level on standard error)

use dussek::Instrument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir] = args.as_slice() else {
        return Err("usage: level <instrument dir>".into());
    };
    let doc = Instrument::load(dir).and_then(|i| i.level_doc()).map_err(|e| format!("{dir}: {e}"))?;
    eprintln!("{dir}: {:.2} dB", doc["ff_db"].as_f64().unwrap_or(f64::NAN));
    println!("{}", serde_json::to_string(&doc)?);
    Ok(())
}
