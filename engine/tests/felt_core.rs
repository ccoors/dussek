//! The felt's core (`physics::felt_force`): a stiff layer under the felt's soft one brightens the
//! attack where the recordings' bass is bright (design 2026-10-20-felt-core).
use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use pianoengine::{Engine, Event, Instrument, Params, Tuning};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const SR: f64 = 48000.0;

/// Grand A's export with C2's core set (0.4 mm, 15 N at 0.5 mm past it) or taken out, whatever the
/// export holds, in a directory of its own
fn c2_dir(tag: &str, core: bool) -> Option<PathBuf> {
    let text = std::fs::read_to_string(Path::new(ROOT).join("instrument.json")).ok()?;
    let mut j: serde_json::Value = serde_json::from_str(&text).ok()?;
    let n = j["notes"].as_array_mut()?.iter_mut().find(|n| n["note"] == 36)?;
    let p = n["excitation"]["felt_p"].as_f64()?;
    if core {
        n["excitation"]["core_K"] = serde_json::json!(15.0 / 5e-4f64.powf(p));
        n["excitation"]["core_depth"] = serde_json::json!(4e-4);
    } else {
        let e = n["excitation"].as_object_mut()?;
        e.remove("core_K");
        e.remove("core_depth");
    }
    let dir = std::env::temp_dir().join(format!("pianoengine-felt-core-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    for e in std::fs::read_dir(ROOT).ok()?.flatten() {
        if e.file_name() != "instrument.json" {
            std::os::unix::fs::symlink(e.path(), dir.join(e.file_name())).ok()?;
        }
    }
    std::fs::write(dir.join("instrument.json"), serde_json::to_string(&j).ok()?).ok()?;
    Some(dir)
}

fn render(dir: &Path, key: u8, velocity: f64, params: Params) -> Option<Vec<f32>> {
    let inst = Instrument::load(dir).ok()?;
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).expect("engine");
    e.set_params(params);
    let speed = e.hammer_speed(key, velocity);
    e.queue(0, Event::NoteOn { key, speed });
    let n = 14400;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    Some(l)
}

/// mean power per DFT bin in [lo, hi) Hz over 0.02–0.15 s after the onset (the first sample above
/// 5 % of the peak, as `proto/bass_check.py`), Hann-windowed
fn band(x: &[f32], lo: f64, hi: f64) -> f64 {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let on = x.iter().position(|v| v.abs() > 0.05 * peak).unwrap_or(0);
    let seg: Vec<f64> = x[on + (0.02 * SR) as usize..on + (0.15 * SR) as usize].iter().map(|&v| v as f64).collect();
    let n = seg.len();
    let w: Vec<f64> = (0..n).map(|i| seg[i] * (0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos())).collect();
    let bins: Vec<usize> = (0..n / 2).filter(|&b| (lo..hi).contains(&(b as f64 * SR / n as f64))).collect();
    let p: f64 = bins
        .iter()
        .map(|&b| {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, v) in w.iter().enumerate() {
                let ph = 2.0 * PI * (b * i) as f64 / n as f64;
                re += v * ph.cos();
                im -= v * ph.sin();
            }
            re * re + im * im
        })
        .sum();
    p / bins.len() as f64
}

fn bright_db(x: &[f32]) -> f64 {
    10.0 * (band(x, 4000.0, 6000.0) / band(x, 200.0, 500.0)).log10()
}

#[test]
fn the_core_brightens_the_attack() {
    let (Some(bare), Some(dir)) = (c2_dir("bare", false), c2_dir("bright", true)) else { return };
    let (Some(plain), Some(cored)) = (render(&bare, 36, 124.0, Params::default()), render(&dir, 36, 124.0, Params::default())) else { return };
    let (a, b) = (bright_db(&plain), bright_db(&cored));
    assert!(b > a + 10.0, "C2's 4–6 kHz re 200–500 Hz at velocity 124: {a:.1} → {b:.1} dB");
}

#[test]
fn other_keys_are_untouched() {
    let Some(dir) = c2_dir("other", true) else { return };
    let (Some(plain), Some(cored)) = (render(Path::new(ROOT), 48, 124.0, Params::default()), render(&dir, 48, 124.0, Params::default())) else { return };
    assert_eq!(plain, cored);
}

#[test]
fn a_cored_strike_stays_finite_at_the_hardest_touch() {
    let Some(dir) = c2_dir("hard", true) else { return };
    // the Hammer hardness control's +12 dB, the hammers worn, the strikes' variation at its 400 %
    let hard = Params { hardness: 10f32.powf(12.0 / 20.0), hammer_wear: 1.0, variation: 4.0, ..Params::default() };
    let Some(x) = render(&dir, 36, 127.0, hard) else { return };
    assert!(x.iter().all(|v| v.is_finite() && v.abs() < 1.0), "peak {}", x.iter().fold(0.0f32, |m, v| m.max(v.abs())));
}
