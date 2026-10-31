//! A key rendered by the linked engine sounds at the export's frequency (the committed marimba;
//! returns early without it).
use pe_editor::audio;
use pe_editor::compare::{Renderer, Strike};
use pe_editor::project::Repo;

#[test]
fn a_rendered_key_sounds_at_its_exported_frequency() {
    let repo = Repo::from_env();
    let export = repo.root.join("instruments/marimba");
    let Ok(text) = std::fs::read_to_string(export.join("instrument.json")) else { return };
    let j: serde_json::Value = serde_json::from_str(&text).unwrap();
    let note = j["notes"].as_array().unwrap().iter().find(|n| n["note"] == 60).unwrap();
    let f0 = note["string"]["f0"].as_f64().unwrap() as f32;
    let mut r = Renderer::new(&export).unwrap();
    let t = std::time::Instant::now();
    let x = r.render(&Strike { key: 60, speed: 1.0, stops: None, mallet: None, room: None }, 48000, 3.0).unwrap();
    eprintln!("3 s rendered in {:.2} s", t.elapsed().as_secs_f32());
    assert_eq!(x.len(), 3 * 48000);
    let on = audio::onset(&x);
    assert!(on < 4800, "onset at {on}");
    let peaks = audio::peaks(&x, on + 2400, 48000, 48000.0);
    let near = peaks.iter().filter(|p| (1200.0 * (p[0] / f0).log2()).abs() < 50.0).map(|p| p[0]).next()
        .unwrap_or_else(|| panic!("no peak near {f0} Hz: {peaks:?}"));
    let cents = 1200.0 * (near / f0).log2();
    assert!(cents.abs() < 5.0, "{near} Hz is {cents:+.1} cents off {f0} Hz");
}

#[test]
fn a_missing_export_is_an_error() {
    assert!(Renderer::new(std::path::Path::new("/nonexistent/export")).is_err());
}

#[test]
fn a_reused_engine_renders_as_a_fresh_one() {
    // one engine per worker, silenced between pairs: a key struck again after its own ring (the
    // bells ring for seconds) sounds exactly as from a fresh engine
    let repo = Repo::from_env();
    let export = repo.root.join("instruments/bells");
    if !export.join("instrument.json").is_file() {
        return;
    }
    let s = Strike { key: 72, speed: 1.0, stops: None, mallet: None, room: None };
    let fresh = Renderer::new(&export).unwrap().render(&s, 48000, 1.0).unwrap();
    let mut r = Renderer::new(&export).unwrap();
    r.render(&s, 48000, 1.0).unwrap();
    let again = r.render(&s, 48000, 1.0).unwrap();
    assert_eq!(again.len(), fresh.len());
    // sample for sample: the corpus's flags must not depend on which pairs a worker rendered before
    let differ = fresh.iter().zip(&again).filter(|(a, b)| a != b).count();
    assert_eq!(differ, 0, "the reused engine's render differs in {differ} samples");
}

#[test]
fn a_key_outside_the_keyboard_is_refused() {
    let repo = Repo::from_env();
    let export = repo.root.join("instruments/marimba");
    if !export.join("instrument.json").is_file() {
        return;
    }
    let mut r = Renderer::new(&export).unwrap();
    for key in [20, 109] {
        assert!(r.render(&Strike { key, speed: 1.0, stops: None, mallet: None, room: None }, 48000, 0.1).is_err(), "key {key}");
    }
}

#[test]
fn a_stereo_render_averages_to_the_measured_one() {
    // the A/B plays the render in stereo; the measurement reads its two channels' mean
    let repo = Repo::from_env();
    let export = repo.root.join("instruments/marimba");
    if !export.join("instrument.json").is_file() {
        return;
    }
    let s = Strike { key: 60, speed: 1.0, stops: None, mallet: None, room: None };
    let mono = Renderer::new(&export).unwrap().render(&s, 48000, 1.0).unwrap();
    let (l, r) = Renderer::new(&export).unwrap().render_stereo(&s, 48000, 1.0).unwrap();
    assert_eq!((l.len(), r.len()), (mono.len(), mono.len()));
    assert!(l.iter().zip(&r).zip(&mono).all(|((a, b), m)| (0.5 * (a + b) - m).abs() < 1e-6));
    assert!(l.iter().zip(&r).any(|(a, b)| (a - b).abs() > 1e-4), "the two channels differ");
}

#[test]
fn a_dry_pair_renders_without_a_room() {
    // the marimba's key 96 dies within 0.3 s; the room rings on after it (at 0.25–0.5 s: −84 dB
    // with the room, −113 dB without): compare 0.3–0.6 s
    let repo = Repo::from_env();
    let export = repo.root.join("instruments/marimba");
    if !export.join("instrument.json").is_file() {
        return;
    }
    let s = |room| Strike { key: 96, speed: 0.5, stops: None, mallet: None, room };
    let wet = Renderer::new(&export).unwrap().render(&s(None), 48000, 1.0).unwrap();
    let dry = Renderer::new(&export).unwrap().render(&s(Some(0.0)), 48000, 1.0).unwrap();
    let e = |x: &[f32]| x[14400..28800].iter().map(|v| v * v).sum::<f32>();
    assert!(e(&wet) > 0.0 && e(&dry) < 0.1 * e(&wet), "the dry tail {:.3e} vs the roomed {:.3e}", e(&dry), e(&wet));
}

#[test]
fn a_reused_renderer_starts_from_the_exports_parameters() {
    // a pair with the second mallet and no room, then one with neither: as from a fresh renderer
    let repo = Repo::from_env();
    let export = repo.root.join("instruments/marimba");
    if !export.join("instrument.json").is_file() {
        return;
    }
    let plain = Strike { key: 72, speed: 0.5, stops: None, mallet: None, room: None };
    let mut r = Renderer::new(&export).unwrap();
    r.render(&Strike { mallet: Some(1), room: Some(0.0), ..plain.clone() }, 48000, 0.5).unwrap();
    let reused = r.render(&plain, 48000, 0.5).unwrap();
    let fresh = Renderer::new(&export).unwrap().render(&plain, 48000, 0.5).unwrap();
    assert!(reused == fresh, "the previous pair's mallet or room carried over");
}
