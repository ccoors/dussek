//! The suggestion on synthetic sets: names in every spelling, numbers, dynamics, takes, groups,
//! readings decided by the pitch, levels, and the findings.
mod audio_tones {
    include!("audio_tones.rs");
}
use audio_tones::{harmonic, write_tone};
use pe_editor::audio::{self, Measures};
use pe_editor::mapping;
use pe_editor::suggest::{self, Folder};
use serde_json::json;
use std::path::Path;
use std::sync::atomic::AtomicUsize;

fn empty_files(dir: &Path, names: &[&str]) {
    std::fs::create_dir_all(dir).unwrap();
    for n in names {
        std::fs::write(dir.join(n), b"").unwrap();
    }
}

fn run(data: &Path, sub: &str, measure: bool) -> (suggest::Suggestion, Vec<mapping::Item>) {
    let f = Folder { root: "set".into(), sub: sub.into() };
    let files = suggest::audio_files(data, &f);
    let m = if measure {
        let paths: Vec<_> = files.iter().map(|r| data.join("set").join(r)).collect();
        audio::measure_all(&paths, &data.join("cache.json"), &AtomicUsize::new(0))
    } else {
        Measures::default()
    };
    let s = suggest::suggest_names(data, &f, &files, &m);
    let items = mapping::items(&s.source, &suggest::data_root(data, "set")).unwrap().items;
    (s, items)
}

fn notes(items: &[mapping::Item]) -> Vec<(i64, String)> {
    items.iter().map(|i| (i["note"].as_i64().unwrap(), i["dynamic"].as_str().unwrap().to_string())).collect()
}

#[test]
fn tokens() {
    assert_eq!(suggest::tokenize("KSHarp_C#4_mf2"), ["KSHarp", "_", "C#", "4", "_", "mf", "2"]);
    assert_eq!(suggest::tokenize("C4v10"), ["C", "4", "v", "10"]);
    assert_eq!(suggest::tokenize("Harp, Lever/Bb0 f"), ["Harp,", " ", "Lever", "/", "Bb", "0", " ", "f"]);
}

#[test]
fn note_names_in_every_spelling_dynamic_words_and_takes() {
    let d = tempfile::tempdir().unwrap();
    empty_files(&d.path().join("set/Harp"), &["Harp_C4_mf1.wav", "Harp_C4_mf2.wav", "Harp_C4_f1.wav", "Harp_D#4_mf1.wav",
        "Harp_D#4_f1.wav", "Harp_Eb5_f1.wav", "Harp_Cs5_f1.wav", "Harp_g3_mf1.wav", "notes.txt"]);
    let (s, items) = run(d.path(), "Harp", false);
    let map = &s.source["map"];
    assert_eq!(map["kind"], "pattern");
    assert_eq!(map["glob"], "Harp/*.wav");
    assert_eq!(map["layers"], json!(["mf", "f"]));
    assert_eq!(map["prefer"], json!(["take"]));
    assert_eq!(notes(&items), [(55, "mf".into()), (60, "mf".into()), (60, "f".into()), (63, "mf".into()), (63, "f".into()),
        (73, "f".into()), (75, "f".into())]);
    assert_eq!(s.source["dynamics"], json!({"mf": "mf", "ff": "f"}));
    assert!(s.findings.iter().any(|f| f.text.contains("pitch not measured")));
}

#[test]
fn a_layer_number_after_a_prefix_and_a_round_robin() {
    let d = tempfile::tempdir().unwrap();
    empty_files(&d.path().join("set"), &["X_C4_vl1_rr1.wav", "X_C4_vl1_rr2.wav", "X_C4_vl2_rr1.wav", "X_D4_vl1_rr1.wav",
        "X_D4_vl2_rr1.wav", "X_E4_vl3_rr1.wav"]);
    let (s, items) = run(d.path(), "", false);
    assert_eq!(s.source["map"]["dynamic_format"], "vl{}");
    assert_eq!(s.source["map"]["layers"], json!(["1", "2", "3"]));
    assert_eq!(s.source["map"]["glob"], "*.wav");
    assert_eq!(notes(&items), [(60, "vl1".into()), (60, "vl2".into()), (62, "vl1".into()), (62, "vl2".into()), (64, "vl3".into())]);
    assert_eq!(s.source["dynamics"], json!({"pp": "vl1", "mf": "vl2", "ff": "vl3"}));
}

#[test]
fn midi_numbers_and_index_offsets_by_the_pitch() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set/m");
    std::fs::create_dir_all(&dir).unwrap();
    for n in 48..54 {
        write_tone(&dir.join(format!("m_{n}.wav")), audio::midi_hz(n as f64), 0.3, &harmonic(), 0.05);
    }
    let (s, items) = run(d.path(), "m", true);
    assert_eq!(s.source["map"]["note"], json!({"group": "midi", "style": "midi"}));
    assert_eq!(items.iter().map(|i| i["note"].as_i64().unwrap()).collect::<Vec<_>>(), (48..54).collect::<Vec<_>>());
    assert_eq!(s.source["map"]["dynamic"], "mf");
    let dir = d.path().join("set/k");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 1..7 {
        write_tone(&dir.join(format!("key {i}.wav")), audio::midi_hz((i + 47) as f64), 0.3, &harmonic(), 0.05);
    }
    let (s, _) = run(d.path(), "k", true);
    assert_eq!(s.source["map"]["note"], json!({"group": "index", "style": "index", "offset": 47}));
}

#[test]
fn the_octave_convention_by_the_pitch_and_a_mistuned_file() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set/o");
    std::fs::create_dir_all(&dir).unwrap();
    // named an octave low (C3 = middle C): the files sound at 60 …
    for (i, n) in ["C3", "D3", "E3", "F3", "G3", "A3"].iter().enumerate() {
        let midi = [60, 62, 64, 65, 67, 69][i] as f64 + if *n == "E3" { 1.0 } else { 0.0 };
        write_tone(&dir.join(format!("{n}.wav")), audio::midi_hz(midi), 0.3, &harmonic(), 0.05);
    }
    let (s, items) = run(d.path(), "o", true);
    assert_eq!(s.source["map"]["note"], json!({"group": "note", "shift": 12}));
    assert_eq!(items[0]["note"], 60);
    let suspect = s.findings.iter().find(|f| f.text.contains("pitch")).expect("a pitch finding");
    assert_eq!(suspect.files.len(), 1);
    assert!(suspect.files[0].ends_with("E3.wav"), "{suspect:?}");
    let off = suggest::pitch_offsets(&items, &audio::measure_all(
        &items.iter().map(|i| i["path"].as_str().unwrap().into()).collect::<Vec<std::path::PathBuf>>(),
        &d.path().join("cache.json"), &AtomicUsize::new(0)));
    assert!(off.values().filter(|c| c.abs() > 50.0).count() == 1);
}

#[test]
fn unknown_dynamic_words_ordered_by_level_and_a_contradiction() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set/w");
    std::fs::create_dir_all(&dir).unwrap();
    for (i, n) in ["C4", "D4", "E4", "F4"].iter().enumerate() {
        let f = audio::midi_hz([60, 62, 64, 65][i] as f64);
        write_tone(&dir.join(format!("{n}_firm.wav")), f, 0.8, &harmonic(), 0.05);
        write_tone(&dir.join(format!("{n}_gentle.wav")), f, 0.1, &harmonic(), 0.05);
    }
    let (s, _) = run(d.path(), "w", true);
    assert_eq!(s.source["map"]["layers"], json!(["gentle", "firm"]));
    assert_eq!(s.source["levels"], "recorded");
    let dir = d.path().join("set/x");
    std::fs::create_dir_all(&dir).unwrap();
    for (i, n) in ["C4", "D4", "E4", "F4"].iter().enumerate() {
        let f = audio::midi_hz([60, 62, 64, 65][i] as f64);
        write_tone(&dir.join(format!("{n}_ff.wav")), f, 0.1, &harmonic(), 0.05);
        write_tone(&dir.join(format!("{n}_pp.wav")), f, 0.8, &harmonic(), 0.05);
    }
    let (s, _) = run(d.path(), "x", true);
    assert_eq!(s.source["map"]["layers"], json!(["pp", "ff"]), "known words keep their order");
    assert!(s.findings.iter().any(|f| f.text.contains("level falls")), "{:?}", s.findings);
}

#[test]
fn a_grouping_token_offers_one_source_per_group() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set/g");
    std::fs::create_dir_all(&dir).unwrap();
    for (i, n) in ["C4", "D4", "E4", "F4"].iter().enumerate() {
        let f = audio::midi_hz([60, 62, 64, 65][i] as f64);
        write_tone(&dir.join(format!("Yarn_{n}.wav")), f, 0.3, &harmonic(), 0.05);
        write_tone(&dir.join(format!("Cord_{n}.wav")), f, 0.3, &harmonic(), 0.05);
    }
    let (s, items) = run(d.path(), "g", true);
    assert_eq!(s.groups.iter().map(|g| g.value.as_str()).collect::<Vec<_>>(), ["Cord", "Yarn"]);
    assert_eq!(items.len(), 4);
    assert!(items.iter().all(|i| i["relpath"].as_str().unwrap().contains("Cord_")));
    assert!(s.findings.iter().any(|f| f.text.contains("groups")));
    let yarn = &s.groups[1].map;
    let mut src = s.source.clone();
    src["map"] = yarn.clone();
    let yi = mapping::items(&src, &suggest::data_root(d.path(), "set")).unwrap().items;
    assert!(yi.iter().all(|i| i["relpath"].as_str().unwrap().contains("Yarn_")));
}

#[test]
fn files_of_another_form_and_several_folders() {
    let d = tempfile::tempdir().unwrap();
    empty_files(&d.path().join("set/r/pp"), &["C4.wav", "D4.wav"]);
    empty_files(&d.path().join("set/r/ff"), &["C4.wav", "D4.wav", "D4_alt.wav"]);
    let (s, items) = run(d.path(), "r", false);
    assert_eq!(s.source["map"]["match"], "relpath");
    assert_eq!(s.source["map"]["glob"], "r/**/*.wav");
    assert_eq!(s.source["map"]["layers"], json!(["pp", "ff"]));
    assert_eq!(items.len(), 4);
    assert!(s.findings.iter().any(|f| f.text.contains("another form") && f.files == ["r/ff/D4_alt.wav"]), "{:?}", s.findings);
}

#[test]
fn an_empty_folder_is_a_finding() {
    let d = tempfile::tempdir().unwrap();
    empty_files(&d.path().join("set/e"), &["readme.txt"]);
    let (s, items) = run(d.path(), "e", false);
    assert!(items.is_empty());
    assert!(s.findings.iter().any(|f| f.text.contains("no audio files")));
}

#[test]
fn dynamics_for_one_two_and_many_layers() {
    let l = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(suggest::dynamics_for(&l(&["f"])), json!({"mf": "f"}));
    assert_eq!(suggest::dynamics_for(&l(&["mf", "f"])), json!({"mf": "mf", "ff": "f"}));
    assert_eq!(suggest::dynamics_for(&l(&["1", "2", "3", "4"])), json!({"pp": "1", "mf": "2", "ff": "4"}));
}

fn sfz_set(d: &Path) {
    let dir = d.join("set/inst");
    std::fs::create_dir_all(dir.join("Sustains")).unwrap();
    std::fs::create_dir_all(dir.join("Other")).unwrap();
    for n in ["Sustains/C4_v1.wav", "Sustains/C4_v2.wav", "Sustains/D4_v1.wav", "Sustains/D4_v2.wav", "Sustains/C4_rel.wav",
              "Other/C4_v1.wav"] {
        std::fs::write(dir.join(n), b"").unwrap();
    }
    std::fs::write(dir.join("i.sfz"), "<control> default_path=\n<group> trigger=attack\n\
        <region> sample=Sustains\\C4_v1.wav pitch_keycenter=60 lovel=1 hivel=63 seq_position=1\n\
        <region> sample=Sustains\\C4_v1.wav pitch_keycenter=60 lovel=1 hivel=63 seq_position=2\n\
        <region> sample=Sustains\\C4_v2.wav pitch_keycenter=60 lovel=64 hivel=127\n\
        <region> sample=Other\\C4_v1.wav pitch_keycenter=60 lovel=64 hivel=127\n\
        <region> sample=Sustains\\D4_v1.wav pitch_keycenter=62 lovel=1 hivel=63\n\
        <region> sample=Sustains\\D4_v2.wav pitch_keycenter=62 lovel=64 hivel=127\n\
        <group> trigger=release\n<region> sample=Sustains\\C4_rel.wav pitch_keycenter=60\n").unwrap();
}

#[test]
fn an_sfz_proposal_filters_names_the_release_and_the_folders() {
    let d = tempfile::tempdir().unwrap();
    sfz_set(d.path());
    let f = Folder { root: "set".into(), sub: "inst".into() };
    assert_eq!(suggest::sfz_files(d.path(), &f), ["inst/i.sfz"]);
    let s = suggest::suggest_sfz(d.path(), &f, "inst/i.sfz", &Measures::default()).unwrap();
    let map = &s.source["map"];
    assert_eq!(map["files"], json!(["inst/i.sfz"]));
    assert_eq!(map["prefix"], "inst/");
    assert_eq!(map["filter"], json!([{"opcode": "trigger", "default": "attack", "in": ["attack"]},
                                     {"opcode": "seq_position", "default": "1", "in": ["1"]}]));
    assert_eq!(s.source["release"], json!({"kind": "sfz", "file": "inst/i.sfz"}));
    let dup = s.findings.iter().find(|x| x.text.contains("several regions")).expect("the duplicate finding");
    assert!(dup.text.contains("Other") && dup.text.contains("Sustains"), "{}", dup.text);
    let mut src = s.source.clone();
    src["map"]["folders"] = json!(["Sustains"]);
    let items = mapping::items(&src, &suggest::data_root(d.path(), "set")).unwrap().items;
    assert_eq!(items.len(), 4);
    assert_eq!(s.source["dynamics"], json!({"mf": "1/2", "ff": "2/2"}));
}

#[test]
fn the_folder_of_a_committed_source() {
    let (f, sfz) = suggest::folder_of(&json!({"root": "vcsl", "map": {"kind": "pattern",
        "glob": "Chordophones/Composite Chordophones/Concert Harp/KSHarp_*.wav"}})).unwrap();
    assert_eq!(f, Folder { root: "vcsl".into(), sub: "Chordophones/Composite Chordophones/Concert Harp".into() });
    assert_eq!(sfz, None);
    let (f, _) = suggest::folder_of(&json!({"root": "s/48k", "map": {"kind": "pattern", "glob": "**/*.wav"}})).unwrap();
    assert_eq!(f, Folder { root: "s/48k".into(), sub: "".into() });
    let (f, sfz) = suggest::folder_of(&json!({"root": "k", "map": {"kind": "sfz", "files": ["A, B.sfz"]}})).unwrap();
    assert_eq!((f.sub.as_str(), sfz.as_deref()), ("", Some("A, B.sfz")));
}

#[test]
fn the_entry_point_and_what_to_measure() {
    let d = tempfile::tempdir().unwrap();
    sfz_set(d.path());
    let f = Folder { root: "set".into(), sub: "inst".into() };
    let paths = suggest::files_to_measure(d.path(), &f, Some("inst/i.sfz"));
    assert_eq!(paths.len(), 5, "the proposal's items (the duplicates' folders included): {paths:?}");
    let names = suggest::files_to_measure(d.path(), &f, None);
    assert_eq!(names.len(), 6);
    assert!(suggest::suggest(d.path(), &f, Some("inst/i.sfz"), &Measures::default()).is_ok());
    assert!(suggest::suggest(d.path(), &f, Some("none.sfz"), &Measures::default()).is_err());
}

#[test]
fn a_set_tuned_away_from_a440() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set/t");
    std::fs::create_dir_all(&dir).unwrap();
    // a = 415 Hz: every key a semitone low
    for (n, midi) in [("C4", 60), ("D4", 62), ("E4", 64), ("F4", 65), ("G4", 67), ("A4", 69)] {
        write_tone(&dir.join(format!("{n}.wav")), audio::midi_hz(midi as f64 - 1.0), 0.3, &harmonic(), 0.05);
    }
    let (s, items) = run(d.path(), "t", true);
    assert_eq!(s.source["map"]["note"], json!({"group": "note"}));
    assert_eq!(items[0]["note"], 60);
    assert!(s.findings.iter().any(|f| f.text.contains("tuned") && f.text.contains("415")), "{:?}", s.findings);
    assert!(!s.findings.iter().any(|f| f.text.contains("pitch suspect")), "{:?}", s.findings);
}

#[test]
fn spectra_no_profile_explains_keep_the_plain_reading() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set/c");
    std::fs::create_dir_all(&dir).unwrap();
    // a struck plate's partials (not harmonic, not a tuned or free bar)
    for n in 84..90 {
        write_tone(&dir.join(format!("{n}.wav")), audio::midi_hz(n as f64 + 24.0), 0.3, &[(1.0, 1.0), (2.41, 0.7), (4.13, 0.5), (5.93, 0.4)], 0.05);
    }
    let (s, _) = run(d.path(), "c", true);
    assert_eq!(s.source["map"]["note"], json!({"group": "midi", "style": "midi"}));
    assert!(s.findings.iter().any(|f| f.text.contains("does not settle the octave")), "{:?}", s.findings);
}

#[test]
fn a_level_number_and_dynamic_words_behind_a_common_prefix() {
    let d = tempfile::tempdir().unwrap();
    empty_files(&d.path().join("set/l"), &["P LEVEL1 CLOSE 60.wav", "P LEVEL2 CLOSE 60.wav", "P LEVEL1 CLOSE 62.wav",
        "P LEVEL2 CLOSE 62.wav", "P LEVEL1 CLOSE 64.wav", "P LEVEL1 CLOSE 65.wav"]);
    let (s, items) = run(d.path(), "l", false);
    assert_eq!(s.source["map"]["dynamic_format"], "LEVEL{}");
    assert_eq!(s.source["map"]["layers"], json!(["1", "2"]));
    assert_eq!(items.len(), 6);
    empty_files(&d.path().join("set/p"), &["01-PedalOffForte1Close.wav", "01-PedalOffPianissimo1Close.wav",
        "01-PedalOffMezzoPiano1Close.wav", "01-PedalOffMezzoPiano2Close.wav", "02-PedalOffForte1Close.wav",
        "02-PedalOffPianissimo1Close.wav", "03-PedalOffForte1Close.wav", "04-PedalOffForte1Close.wav"]);
    let (s, items) = run(d.path(), "p", false);
    assert_eq!(s.source["map"]["layers"], json!(["Pianissimo", "MezzoPiano", "Forte"]));
    assert!(s.source["map"]["regex"].as_str().unwrap().contains("PedalOff(?P<dynamic>"), "{}", s.source["map"]["regex"]);
    assert_eq!(items.len(), 7, "the second take of 01's MezzoPiano left out");
}

#[test]
fn files_that_break_the_note_column_are_another_form() {
    let d = tempfile::tempdir().unwrap();
    empty_files(&d.path().join("set/s"), &["A0v1.wav", "A0v2.wav", "C1v1.wav", "C1v2.wav", "D#1v1.wav", "F#1v1.wav",
        "A1v1.wav", "C2v1.wav", "D#2v2.wav", "harmV2C3.wav", "rel1.wav"]);
    let (s, items) = run(d.path(), "s", false);
    assert_eq!(s.source["map"]["dynamic_format"], "v{}");
    assert_eq!(items.len(), 9);
    assert!(s.findings.iter().any(|f| f.text.contains("another form") && f.files == ["s/harmV2C3.wav"]), "{:?}", s.findings);
    assert_eq!(s.source["release"], json!({"kind": "rel_files"}));
}

#[test]
fn an_sfz_whose_regions_need_a_folder_choice_still_proposes() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set/k");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("k.sfz"), "<region> sample=Sustains/C4.wav pitch_keycenter=60\n<region> sample=Other/C4.wav lokey=60\n").unwrap();
    let f = Folder { root: "set".into(), sub: "k".into() };
    let s = suggest::suggest_sfz(d.path(), &f, "k/k.sfz", &Measures::default()).unwrap();
    // both on key 60 (the second by the SFZ default): two folders to choose from
    assert!(s.findings.iter().any(|x| x.text.contains("several regions") && x.text.contains("Other")), "{:?}", s.findings);
    let mut src = s.source.clone();
    src["map"]["folders"] = json!(["Sustains"]);
    assert_eq!(mapping::items(&src, &suggest::data_root(d.path(), "set")).unwrap().items.len(), 1);
}

#[test]
fn a_number_column_wider_than_the_keyboard_is_no_note() {
    // take001 … take200: no offset keeps every number on a MIDI key
    let d = tempfile::tempdir().unwrap();
    let names: Vec<String> = (1..=200).map(|i| format!("take{i:03}.wav")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    empty_files(&d.path().join("set/w"), &refs);
    let (s, _) = run(d.path(), "w", false);
    assert!(s.findings.iter().any(|f| f.text.contains("no MIDI key")), "{:?}", s.findings);
}

#[test]
fn the_folder_of_an_escaped_glob() {
    // the editor escapes a folder's name in the glob it writes ("Set [2]" → "Set [[]2[]]")
    let glob = format!("{}/*", glob::Pattern::escape("Set [2]/Take *1?"));
    let (f, _) = suggest::folder_of(&json!({"root": "vcsl", "map": {"kind": "pattern", "glob": glob}})).unwrap();
    assert_eq!(f.sub, "Set [2]/Take *1?");
    let (f, _) = suggest::folder_of(&json!({"root": "vcsl", "map": {"kind": "pattern", "glob": "A/B*/*.wav"}})).unwrap();
    assert_eq!(f.sub, "A");
}

#[test]
fn an_sfz_with_an_unreadable_key_says_so_and_asks_for_no_folders() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("set");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.wav"), b"").unwrap();
    std::fs::write(dir.join("i.sfz"), "<region> sample=a.wav lovel=1 hivel=127 pitch_keycenter=x\n").unwrap();
    let f = Folder { root: "set".into(), sub: String::new() };
    let s = suggest::suggest_sfz(d.path(), &f, "i.sfz", &Measures::default()).unwrap();
    let t: Vec<&str> = s.findings.iter().map(|x| x.text.as_str()).collect();
    assert!(t.iter().any(|x| x.contains("no items yet") && x.contains("\"x\"")), "the key it cannot read: {t:?}");
    assert!(!t.iter().any(|x| x.contains("folders")), "no folders to choose: {t:?}");
}
