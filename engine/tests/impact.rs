//! The hammer's impact on the board (a piano's thump, instrument.json's `impact`): each strike's
//! felt force, lowpassed, × gain × (speed / the ff anchor's)^growth, on the bridge with the
//! strings' force, through the board and the room. The Action knock control sets it.

use dussek::{Engine, Event, Instrument, Params, Tuning};
use std::path::{Path, PathBuf};

const SR: f64 = 48000.0;

fn dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(name)
}

/// a copy of `name` without its impact (None: no export)
fn without_impact(name: &str) -> Option<PathBuf> {
    let src = dir(name);
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("no-impact-{name}"));
    std::fs::create_dir_all(&d).ok()?;
    std::fs::copy(src.join("data.bin"), d.join("data.bin")).ok()?; // the level: instrument.json's
    let mut j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(src.join("instrument.json")).ok()?).ok()?;
    j.as_object_mut()?.remove("impact")?;
    std::fs::write(d.join("instrument.json"), j.to_string()).ok()?;
    Some(d)
}

/// a copy of `name` with `impact_db` set per key by `trim` (None: no export)
fn with_trim(name: &str, tag: &str, trim: impl Fn(u8) -> f64) -> Option<PathBuf> {
    let src = dir(name);
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("impact-trim-{tag}-{name}"));
    std::fs::create_dir_all(&d).ok()?;
    std::fs::copy(src.join("data.bin"), d.join("data.bin")).ok()?; // the level: instrument.json's
    let mut j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(src.join("instrument.json")).ok()?).ok()?;
    for n in j["notes"].as_array_mut()? {
        let key = n["note"].as_u64()? as u8;
        n["impact_db"] = serde_json::json!(trim(key));
    }
    std::fs::write(d.join("instrument.json"), j.to_string()).ok()?;
    Some(d)
}

/// 0.5 s of `key` at `velocity`, the room off, the Action knock control at `knock` (left channel)
fn render(inst: Instrument, key: u8, velocity: f64, knock: f32) -> Vec<f32> {
    let mut e = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    e.set_params(Params { knock, room: 0.0, ..Params::default() });
    let speed = e.hammer_speed(key, velocity);
    e.queue(0, Event::NoteOn { key, speed });
    let n = (0.5 * SR) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

fn load(name: &str) -> Option<Instrument> {
    Instrument::load(dir(name)).ok().filter(|i| i.impact.is_some())
}

/// band energy (dB) of `x` in [lo, hi) Hz
fn band_db(x: &[f64], lo: f64, hi: f64) -> f64 {
    let n = x.len().next_power_of_two();
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf = x.to_vec();
    buf.resize(n, 0.0);
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).unwrap();
    let e: f64 = spec.iter().enumerate().filter(|(k, _)| (lo..hi).contains(&(*k as f64 * SR / n as f64))).map(|(_, c)| c.norm_sqr()).sum();
    10.0 * e.max(1e-300).log10()
}

/// the impact alone: the render with the control at 1 minus at 0
fn impact(name: &str, key: u8, velocity: f64) -> Option<Vec<f64>> {
    let inst = load(name)?;
    let on = render(inst.clone(), key, velocity, 1.0);
    let off = render(inst, key, velocity, 0.0);
    Some(on.iter().zip(&off).map(|(a, b)| (a - b) as f64).collect())
}

#[test]
fn the_action_knock_control_at_zero_is_no_impact() {
    let (Some(inst), Some(bare)) = (load("grand_a"), without_impact("grand_a")) else { return };
    let bare = Instrument::load(bare).unwrap();
    assert!(bare.impact.is_none());
    assert_eq!(render(inst, 84, 64.0, 0.0), render(bare, 84, 64.0, 1.0));
}

#[test]
fn the_impact_is_linear_in_the_control() {
    let Some(mut inst) = load("grand_a") else { return };
    // the phantom partials grow with the square of the bridge force, the thump's among it
    inst.phantom = 0.0;
    let off = render(inst.clone(), 84, 64.0, 0.0);
    let full: Vec<f64> = render(inst.clone(), 84, 64.0, 1.0).iter().zip(&off).map(|(a, b)| (a - b) as f64).collect();
    let half: Vec<f64> = render(inst, 84, 64.0, 0.5).iter().zip(&off).map(|(a, b)| (a - b) as f64).collect();
    let peak = full.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    assert!(peak > 1e-4, "no impact: {peak}");
    assert!(full.iter().zip(&half).all(|(f, h)| (f - 2.0 * h).abs() <= 1e-3 * peak));
}

#[test]
fn grand_e_s_lowpassed_impact_is_darker_than_grand_a_s_open_one() {
    // Grand D's board passes the felt force bright: its impact is lowpassed at 500 Hz, Grand A's open
    let (Some(e), Some(a)) = (impact("grand_d", 84, 64.0), impact("grand_a", 84, 64.0)) else { return };
    let tilt = |x: &[f64]| band_db(x, 2000.0, 8000.0) - band_db(x, 200.0, 700.0);
    assert!(tilt(&e) < tilt(&a) - 10.0, "Grand D {:.1} dB, Grand A {:.1} dB above 2 kHz re 200–700 Hz", tilt(&e), tilt(&a));
}

#[test]
fn harder_strikes_thump_more() {
    let (Some(soft), Some(hard)) = (impact("grand_a", 84, 40.0), impact("grand_a", 84, 110.0)) else { return };
    let peak = |x: &[f64]| x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(peak(&hard) > 3.0 * peak(&soft));
}

#[test]
fn a_morph_carries_the_impact() {
    // Grand A and Grand G: their gains and lowpasses differ (Grand D's gain came to Grand A's when its
    // board was refitted in bands, 2026-10-22)
    let Ok(m) = Instrument::morph(&[(dir("grand_a"), 0.5), (dir("grand_g"), 0.5)]) else { return };
    let (Some(a), Some(e)) = (load("grand_a").and_then(|i| i.impact), load("grand_g").and_then(|i| i.impact)) else { return };
    let im = m.impact.expect("the morph has no impact");
    assert!(im.gain > a.gain.min(e.gain) && im.gain < a.gain.max(e.gain));
    let (ca, ce, cm) = (a.cutoff_hz.unwrap(), e.cutoff_hz.unwrap(), im.cutoff_hz.unwrap());
    assert!(cm > ca.min(ce) && cm < ca.max(ce));
}

#[test]
fn a_keys_trim_scales_its_thump_alone() {
    // the project's [impact] trim_db (Grand A's thump 12 dB down by ear, step 128): the strings untouched
    let (Some(d), Some(z)) = (with_trim("grand_a", "c6", |k| if k == 84 { -12.0 } else { 0.0 }), with_trim("grand_a", "none", |_| 0.0)) else { return };
    let (Ok(t), Ok(a)) = (Instrument::load(&d), Instrument::load(&z)) else { return };
    let thump = |inst: &Instrument, key| {
        let on = render(inst.clone(), key, 64.0, 1.0);
        let off = render(inst.clone(), key, 64.0, 0.0);
        on.iter().zip(&off).fold(0.0f64, |m, (x, y)| m.max((x - y).abs() as f64))
    };
    let ratio = thump(&t, 84) / thump(&a, 84);
    assert!((ratio - 10f64.powf(-12.0 / 20.0)).abs() < 1e-3, "C6 {ratio}");
    assert!((thump(&t, 72) / thump(&a, 72) - 1.0).abs() < 1e-6, "C5 trimmed too");
    assert_eq!(render(t.clone(), 84, 64.0, 0.0), render(a, 84, 64.0, 0.0));
}

#[test]
fn a_morph_blends_the_trims_in_db_over_the_sides_with_a_thump() {
    let Some(d) = with_trim("grand_a", "all", |_| -12.0) else { return };
    let Ok(m) = Instrument::morph(&[(d, 0.5), (dir("grand_d"), 0.5)]) else { return };
    let g = m.notes[63].impact_gain;
    assert!((g - 10f64.powf(-6.0 / 20.0)).abs() < 1e-9, "{g}");
}
