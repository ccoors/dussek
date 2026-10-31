//! Rolled pianos from a recipe (`roll::recipe`, `roll::generate`, the `roll` binary): the recipe
//! round-trips (a hand-edited file keeps its edits), a roll is deterministic, holds up against the
//! references and at the other positions, a far one is built as set (pulled back when asked), a
//! roll can be cancelled, what cannot be used is named, the references are measured once per
//! piano (kept on disk), the license credits every source. Needs the exported instruments
//! (skipped without them).

use dussek::roll::files::Export;
use dussek::roll::generate::{self, CANCELLED, Options, generate, generate_with, references, write_export};
use dussek::roll::measure;
use dussek::roll::recipe::Recipe;
use std::path::{Path, PathBuf};

fn instruments() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments")
}

fn fixture() -> Recipe {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rolled/project.toml")).unwrap();
    Recipe::parse(&text, "rolled").unwrap()
}

fn ready() -> bool {
    fixture().references.iter().all(|r| instruments().join(r).join("data.bin").exists())
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pe-roll-gen-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn the_recipe_round_trips() {
    let r = fixture();
    assert_eq!(r.name, "rolled");
    assert_eq!(r.menu, ("Rolled test".to_string(), "Rolled pianos".to_string()));
    assert_eq!(r.strings[0], ("grand_d".to_string(), 0.384));
    assert_eq!((r.warmth, r.air, r.seed), (-0.09, -0.35, 501));
    let text = r.to_toml();
    assert_eq!(Recipe::parse(&text, "rolled").unwrap(), r);
    let doc: toml_edit::DocumentMut = text.parse().unwrap();
    let uses: Vec<String> = doc["uses"].as_table().unwrap().iter().map(|(_, v)| v.as_str().unwrap().to_string()).collect();
    for p in r.references.iter().chain(r.strings.iter().map(|s| &s.0)).chain(&r.fine_structure) {
        assert!(uses.contains(p), "{p} not in [uses]");
    }
    assert_eq!(doc["project"]["family"].as_str(), Some("rolled"));
}

#[test]
fn a_roll_is_deterministic() {
    if !ready() {
        return;
    }
    let (a, _, _) = generate(&fixture(), &instruments()).unwrap();
    let (b, _, _) = generate(&fixture(), &instruments()).unwrap();
    assert_eq!(a.json, b.json);
    assert!(a.data == b.data, "data.bin differs");
}

#[test]
fn a_roll_holds_up() {
    if !ready() {
        return;
    }
    let t = std::time::Instant::now();
    let refs = references(&fixture().references, &instruments()).unwrap();
    let first = t.elapsed().as_secs_f64();
    // each knob's span over the references (the editor's sliders): Grand D is the coldest
    assert!(refs.spans[0].0 < -1.5 && refs.spans[0].1 > 1.0, "{:?}", refs.spans[0]);
    let (e, report, _) = generate(&fixture(), &instruments()).unwrap();
    eprintln!("references measured in {first:.1} s, the roll in {:.1} s", report.seconds);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.pulled_back, 0);
    assert!(measure::faults(&e).unwrap().is_empty());
    let d = measure::player_minus_recording(&e).unwrap();
    for (b, (x, g)) in d.iter().zip(&refs.goal).enumerate() {
        assert!((x - g).abs() <= 2.5, "octave {b}: {x:+.1} against {g:+.1}");
    }
    assert!(e.json["profile"].as_str().unwrap().starts_with("rolled:"));
    let dir = tmp("holds");
    write_export(&dir, &e, "license").unwrap();
    assert_eq!(dussek::info::info(&dir).unwrap().morph_family, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_far_roll_is_built_as_set() {
    // the sliders are the user's: the realism check names what is off, the knobs stay
    if !ready() {
        return;
    }
    let mut r = fixture();
    r.warmth = 3.0;
    let (_, report, _) = generate(&r, &instruments()).unwrap();
    assert_eq!(report.pulled_back, 0);
    assert_eq!(report.knobs[0], 3.0);
    assert!(!report.failures.is_empty());
}

#[test]
fn a_far_roll_is_pulled_back_when_asked() {
    // a random draw is pulled back toward the middle until it passes (warmth 10: since the uprights
    // were voiced even, step 168, warmth 4.5 fails only what no pull-back mends, Player against
    // Recording and the ring at 60 ms, against the references' ranges)
    if !ready() {
        return;
    }
    let mut r = fixture();
    r.warmth = 10.0;
    let (_, report, _) = generate_with(&r, &instruments(), &Options { pull_back: true, ..Options::default() }).unwrap();
    assert!(report.pulled_back >= 1, "{:?}", report.failures);
    assert!(report.knobs[0] < 10.0);
    assert!(report.knobs.iter().all(|k| (k * 100.0 - (k * 100.0).round()).abs() < 1e-9), "{:?}", report.knobs);
}

#[test]
fn a_pull_back_that_does_not_mend_ends() {
    // what the knobs barely set (the treble at Player, which the board's radiation barely moves) ends
    // the pull-back well before its fourth try (each try mends a quarter of what is off, or it ends);
    // the best try kept. (A donors' fault, the two microphones' bass apart, was the other case: the
    // ringing boards radiate their low modes to both in phase, design log step 184, and it is gone.)
    if !ready() {
        return;
    }
    let mut player = fixture();
    player.strings = vec![("grand_g".into(), 0.39), ("grand_b".into(), 0.32), ("grand_f".into(), 0.29)];
    player.fine_structure = vec!["grand_a".into(), "grand_b".into(), "grand_f".into()];
    (player.warmth, player.air, player.shade) = (-0.8, 0.37, vec![0.0, 0.0]);
    let (_, report, _) = generate_with(&player, &instruments(), &Options { pull_back: true, ..Options::default() }).unwrap();
    eprintln!("player: pulled back {}×, {:?}", report.pulled_back, report.failures);
    assert!(!report.failures.is_empty());
    assert!(report.pulled_back <= 2, "{} ({:?})", report.pulled_back, report.failures);
}

#[test]
fn a_cancelled_roll_stops() {
    // asked between the stages: once it says so, the roll ends without asking again
    if !ready() {
        return;
    }
    let asked = std::sync::atomic::AtomicUsize::new(0);
    let cancelled = || asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 >= 3;
    let t = std::time::Instant::now();
    let e = generate_with(&fixture(), &instruments(), &Options { cancelled: &cancelled, ..Options::default() }).err().expect("cancelled");
    assert_eq!(e.0, CANCELLED);
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 3);
    eprintln!("cancelled after {:.1} s", t.elapsed().as_secs_f64());
}

#[test]
fn the_strings_weights_reach_the_roll() {
    // the blended strings are kept between rolls (a slider's change rolls only the body): by their
    // donors and weights
    if !ready() {
        return;
    }
    let (a, _, _) = generate(&fixture(), &instruments()).unwrap();
    let mut r = fixture();
    r.strings = vec![("grand_d".into(), 0.2), ("grand_c".into(), 0.4), ("grand_b".into(), 0.4)];
    let (b, _, _) = generate(&r, &instruments()).unwrap();
    assert_ne!(a.json["notes"][40]["string"], b.json["notes"][40]["string"]);
    let mut r = fixture();
    r.fine_structure = vec!["grand_e".into(), "grand_f".into(), "grand_a".into()];
    let (c, _, _) = generate(&r, &instruments()).unwrap();
    assert!(a.data != c.data, "the fine structure did not change the responses");
}

#[test]
fn references_are_measured_once_per_piano() {
    // leaving one reference out measures nothing again
    if !ready() {
        return;
    }
    let all = fixture().references;
    references(&all, &instruments()).unwrap();
    let t = std::time::Instant::now();
    let fewer = references(&all[1..], &instruments()).unwrap();
    assert!(t.elapsed().as_secs_f64() < 3.0, "{:.1} s", t.elapsed().as_secs_f64());
    assert_eq!(fewer.spans.len(), all.len() - 2); // nine references: eight directions
}

#[test]
fn a_pianos_measurement_survives_on_disk() {
    let Some(p) = fixture().references.first().map(|n| instruments().join(n)).filter(|d| d.join("data.bin").exists()) else { return };
    let e = Export::read(&p).unwrap();
    let piano = generate::Piano::measure(&e).unwrap();
    let dir = tmp("disk");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("piano.json");
    generate::save_piano(&file, "key 1", &piano).unwrap();
    let back = generate::load_piano(&file, "key 1").expect("read back");
    assert_eq!(back.body.level, piano.body.level);
    assert_eq!(back.body.rate, piano.body.rate);
    assert_eq!(back.body.corr, piano.body.corr);
    assert_eq!((&back.measured.stats, back.measured.pr, &back.measured.level, back.measured.mono_vs_power), (&piano.measured.stats, piano.measured.pr, &piano.measured.level, piano.measured.mono_vs_power));
    // another key (the export or the program changed): measured anew
    assert!(generate::load_piano(&file, "key 2").is_none());
    std::fs::write(&file, "not json").unwrap();
    assert!(generate::load_piano(&file, "key 1").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_roll_s_export_carries_its_level_table() {
    // the velocity maps through it (`Instrument::dynamics`), as on an exported piano
    let Some(p) = fixture().references.first().map(|n| instruments().join(n)).filter(|d| d.join("data.bin").exists()) else { return };
    let e = Export::read(&p).unwrap();
    let dir = tmp("table");
    write_export(&dir, &e, "license").unwrap();
    let inst = dussek::Instrument::load(&dir).unwrap();
    assert_eq!(inst.dynamics.keys, dussek::instrument::LEVEL_TABLE_KEYS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_write_leaves_no_instrument() {
    // instrument.json comes last: a plugin watching the folder never reads a half-written one
    let Some(p) = fixture().references.first().map(|n| instruments().join(n)).filter(|d| d.join("data.bin").exists()) else { return };
    let e = Export::read(&p).unwrap();
    let dir = tmp("order");
    std::fs::create_dir_all(dir.join("LICENSE.md")).unwrap(); // a file before instrument.json fails
    assert!(write_export(&dir, &e, "license").is_err());
    assert!(!dir.join("instrument.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_cannot_be_used_is_named() {
    if !ready() {
        return;
    }
    let mut r = fixture();
    r.references.push("nosuch".into());
    assert!(generate(&r, &instruments()).err().unwrap().0.contains("nosuch"));
    let mut r = fixture();
    r.strings.push(("flemish".into(), 0.2));
    assert!(generate(&r, &instruments()).err().unwrap().0.contains("flemish"));
    // strings and fine structure come from grands (an upright's are refused)
    let mut r = fixture();
    r.strings.push(("upright_a".into(), 0.2));
    assert!(generate(&r, &instruments()).err().unwrap().0.contains("upright_a"));
    let mut r = fixture();
    r.fine_structure.push("upright_a".into());
    assert!(generate(&r, &instruments()).err().unwrap().0.contains("upright_a"));
    // a rolled export as a reference: a folder of links to the references plus a roll
    let dir = tmp("refs");
    std::fs::create_dir_all(&dir).unwrap();
    for name in fixture().references.iter().chain(["flemish".to_string()].iter()) {
        std::os::unix::fs::symlink(instruments().join(name), dir.join(name)).unwrap();
    }
    let mut roll = Export::read(&instruments().join("grand_a")).unwrap();
    roll.json["profile"] = "rolled:earlier".into();
    roll.write(&dir.join("earlier")).unwrap();
    let mut r = fixture();
    r.references.push("earlier".into());
    assert!(generate(&r, &dir).err().unwrap().0.contains("earlier"));
    // a donor at another sample rate than the frame's
    let mut fast = Export::read(&instruments().join("grand_b")).unwrap();
    fast.json["reference_sr"] = 44100.0.into();
    fast.write(&dir.join("fast")).unwrap();
    let mut r = fixture();
    r.fine_structure.push("fast".into());
    assert!(generate(&r, &dir).err().unwrap().0.contains("fast"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saving_keeps_the_files_edits() {
    // the editor's Save rewrites the recipe's values in the project file, its comments and other
    // tables kept
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rolled/project.toml");
    let text = std::fs::read_to_string(path).unwrap().replace("warmth = -0.09", "# the user's note\nwarmth = -0.09") + "\n[notes]\nkept = true\n";
    let mut r = Recipe::parse(&text, "rolled").unwrap();
    r.warmth = 0.25;
    r.strings = vec![("grand_a".into(), 0.5), ("grand_b".into(), 0.5)];
    r.menu.0 = "Renamed".into();
    let out = r.update_toml(&text).unwrap();
    assert_eq!(Recipe::parse(&out, "rolled").unwrap(), r);
    for kept in ["# a rolled piano for the roll's tests", "# the user's note", "[notes]", "kept = true"] {
        assert!(out.contains(kept), "{kept} lost:\n{out}");
    }
    let doc: toml_edit::DocumentMut = out.parse().unwrap();
    assert_eq!(doc["uses"]["strings1"].as_str(), Some("grand_a"));
    assert_eq!(doc["uses"]["strings2"].as_str(), Some("grand_b"));
    assert!(doc["uses"].get("strings3").is_none());
}

#[test]
fn the_license_credits_every_source() {
    if !ready() {
        return;
    }
    let (_, _, license) = generate(&fixture(), &instruments()).unwrap();
    for p in ["grand_d", "grand_c", "grand_b", "grand_e", "grand_f"] {
        assert!(license.contains(&format!("## {p}")), "{p}");
    }
    assert!(license.contains("Ivy Audio"));
    assert!(license.starts_with("# License: Rolled test (rolled)"));
}

#[test]
fn the_binary_writes_the_export() {
    if !ready() {
        return;
    }
    let out = tmp("bin");
    let st = std::process::Command::new(env!("CARGO_BIN_EXE_roll"))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rolled"))
        .arg("--instruments")
        .arg(instruments())
        .arg("--out")
        .arg(&out)
        .status()
        .unwrap();
    assert!(st.success());
    for f in ["instrument.json", "data.bin", "LICENSE.md"] {
        assert!(out.join(f).exists(), "{f}");
    }
    let _ = std::fs::remove_dir_all(&out);
}

