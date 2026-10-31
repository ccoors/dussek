//! An instrument's ff level (`Instrument::ff_level`) written to its level.json, which a morph
//! reads instead of measuring the instrument itself, and a hammer piano's level over its hammer
//! speed (`Instrument::level_curve`, `speed_db`), which maps its velocity. The exports run it on
//! what they write.
//!
//! Usage: level <instrument dir>…

use std::path::Path;

use pianoengine::Instrument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        return Err("usage: level <instrument dir>…".into());
    }
    for dir in &dirs {
        let doc = Instrument::load(dir).and_then(|i| i.level_doc()).map_err(|e| format!("{dir}: {e}"))?;
        let db = doc["ff_db"].as_f64().unwrap_or(f64::NAN);
        let text = serde_json::to_string_pretty(&doc)?;
        std::fs::write(Path::new(dir).join("level.json"), text + "\n").map_err(|e| format!("{dir}: {e}"))?;
        println!("{dir}: {db:.2} dB");
    }
    Ok(())
}
