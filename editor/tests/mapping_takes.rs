//! A source's `skip_files`: a left-out take gives way to the next take of its key and layer, the
//! report lists the left-out takes, the spares and which spare stands in (no audio is read).
use pe_editor::mapping::{self, Item, Report};
use serde_json::{Value as Json, json};

const RR: &str = "<region> sample=C4_rr1.wav pitch_keycenter=60 lovel=1 hivel=127 seq_position=1\n\
    <region> sample=C4_rr2.wav pitch_keycenter=60 lovel=1 hivel=127 seq_position=2\n\
    <region> sample=C4_rr3.wav pitch_keycenter=60 lovel=1 hivel=127 seq_position=3\n\
    <region> sample=D4_rr1.wav pitch_keycenter=62 lovel=1 hivel=127 seq_position=1\n";

fn touch(root: &std::path::Path, rels: &[&str]) {
    for r in rels {
        let p = root.join(r);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"").unwrap();
    }
}

fn rels(items: &[Item]) -> Vec<String> {
    items.iter().map(|i| i["relpath"].as_str().unwrap().to_string()).collect()
}

fn harp() -> Json {
    json!({"kind": "pattern", "glob": "h/KSHarp_*.wav",
        "regex": r"KSHarp_(?P<note>[A-G]#?\d)_(?P<dynamic>[a-z]+)(?P<take>\d+)\.wav",
        "note": {"group": "note"}, "layers": ["mf", "f"], "prefer": ["take"], "order": "note"})
}

fn rr(skip: &[&str]) -> (tempfile::TempDir, Report) {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("set.sfz"), RR).unwrap();
    let src = json!({"id": "s", "skip_files": skip, "map": {"kind": "sfz", "files": ["set.sfz"],
        "filter": [{"opcode": "seq_position", "default": "1", "in": ["1"]}]}});
    let r = mapping::items(&src, d.path().to_str().unwrap()).unwrap();
    (d, r)
}

fn c4(r: &Report) -> Vec<String> {
    rels(&r.items.iter().filter(|i| i["note"] == json!(60)).cloned().collect::<Vec<_>>())
}

#[test]
fn a_left_out_take_gives_way_to_the_spare() {
    let d = tempfile::tempdir().unwrap();
    touch(d.path(), &["h/KSHarp_C4_mf1.wav", "h/KSHarp_C4_mf2.wav"]);
    let src = json!({"id": "harp", "map": harp(), "skip_files": ["h/KSHarp_C4_mf1.wav"]});
    let r = mapping::items(&src, d.path().to_str().unwrap()).unwrap();
    assert_eq!(rels(&r.items), ["h/KSHarp_C4_mf2.wav"]);
    assert_eq!(r.replaced.get("h/KSHarp_C4_mf2.wav").map(String::as_str), Some("h/KSHarp_C4_mf1.wav"));
}

#[test]
fn sfz_a_left_out_round_robin_gives_way_to_the_next() {
    assert_eq!(c4(&rr(&[]).1), ["C4_rr1.wav"]);
    assert_eq!(c4(&rr(&["C4_rr1.wav"]).1), ["C4_rr2.wav"]);
    assert_eq!(c4(&rr(&["C4_rr1.wav", "C4_rr2.wav"]).1), ["C4_rr3.wav"]);
}

#[test]
fn sfz_lorand_round_robins_are_takes() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("set.sfz"), "<region> sample=a.wav pitch_keycenter=60 lorand=0 hirand=0.5\n\
        <region> sample=b.wav pitch_keycenter=60 lorand=0.5 hirand=1\n").unwrap();
    let m = json!({"kind": "sfz", "files": ["set.sfz"], "filter": [{"opcode": "lorand", "default": "0", "eq": 0.0}]});
    let root = d.path().to_str().unwrap();
    assert_eq!(rels(&mapping::items(&json!({"id": "s", "map": m}), root).unwrap().items), ["a.wav"]);
    assert_eq!(rels(&mapping::items(&json!({"id": "s", "map": m, "skip_files": ["a.wav"]}), root).unwrap().items), ["b.wav"]);
}

#[test]
fn a_left_out_file_without_a_spare_is_gone() {
    let d = tempfile::tempdir().unwrap();
    touch(d.path(), &["C4.wav", "D4.wav", "E4.wav"]);
    let m = json!({"kind": "pattern", "glob": "*.wav", "regex": r"(?P<note>[A-G]#?\d)\.wav", "note": {"group": "note"},
        "dynamic": "mf", "order": "note"});
    let r = mapping::items(&json!({"id": "s", "map": m, "skip_files": ["C4.wav"]}), d.path().to_str().unwrap()).unwrap();
    assert_eq!(rels(&r.items), ["D4.wav", "E4.wav"]);
    assert_eq!(rels(&r.left_out), ["C4.wav"]);
    assert!(r.replaced.is_empty() && r.spares.is_empty());
}

#[test]
fn a_group_without_a_first_round_robin_still_maps_nothing() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("set.sfz"), "<region> sample=C4_rr2.wav pitch_keycenter=60 seq_position=2\n\
        <region> sample=D4_rr1.wav pitch_keycenter=62 seq_position=1\n").unwrap();
    let m = json!({"kind": "sfz", "files": ["set.sfz"], "filter": [{"opcode": "seq_position", "default": "1", "in": ["1"]}]});
    let r = mapping::items(&json!({"id": "s", "map": m}), d.path().to_str().unwrap()).unwrap();
    assert_eq!(rels(&r.items), ["D4_rr1.wav"]);
}

#[test]
fn the_next_spare_replaces_two_left_out_takes() {
    let (_d, r) = rr(&["C4_rr1.wav", "C4_rr2.wav"]);
    assert_eq!(c4(&r), ["C4_rr3.wav"]);
    assert_eq!(r.replaced.get("C4_rr3.wav").map(String::as_str), Some("C4_rr1.wav"));
    assert_eq!(rels(&r.left_out), ["C4_rr1.wav", "C4_rr2.wav"]);
    assert!(r.left_out_missing.is_empty());
}

#[test]
fn spares_are_listed_in_replacement_order() {
    let (_d, r) = rr(&[]);
    assert_eq!(rels(&r.spares["C4_rr1.wav"]), ["C4_rr2.wav", "C4_rr3.wav"]);
    assert!(!r.spares.contains_key("D4_rr1.wav"));
    let d = tempfile::tempdir().unwrap();
    touch(d.path(), &["h/KSHarp_C4_mf3.wav", "h/KSHarp_C4_mf1.wav", "h/KSHarp_C4_mf2.wav"]);
    let r = mapping::items(&json!({"id": "harp", "map": harp()}), d.path().to_str().unwrap()).unwrap();
    assert_eq!(rels(&r.spares["h/KSHarp_C4_mf1.wav"]), ["h/KSHarp_C4_mf2.wav", "h/KSHarp_C4_mf3.wav"]);
    // a spare is an item as the kept one is (the source's fields applied)
    assert_eq!(r.spares["h/KSHarp_C4_mf1.wav"][0]["tail_cut_s"], json!(0.0));
}

#[test]
fn a_left_out_file_not_in_the_set_is_reported_not_fatal() {
    let (_d, r) = rr(&["gone.wav"]);
    assert_eq!(c4(&r), ["C4_rr1.wav"]);
    assert_eq!(r.left_out_missing, ["gone.wav"]);
    assert!(r.left_out.is_empty());
}

#[test]
fn a_layer_left_out_whole_is_no_spare_in_use() {
    // both takes of the soft layer left out, no spare left: the loud layer keeps its rank (2 of 2),
    // and is not a spare standing in for the soft one (the grid showed it as "spare take in use")
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("set.sfz"), "<region> sample=C4_v1_rr1.wav pitch_keycenter=60 lovel=1 hivel=63 seq_position=1\n\
        <region> sample=C4_v1_rr2.wav pitch_keycenter=60 lovel=1 hivel=63 seq_position=2\n\
        <region> sample=C4_v2_rr1.wav pitch_keycenter=60 lovel=64 hivel=127 seq_position=1\n").unwrap();
    let src = json!({"id": "s", "skip_files": ["C4_v1_rr1.wav", "C4_v1_rr2.wav"], "map": {"kind": "sfz", "files": ["set.sfz"],
        "filter": [{"opcode": "seq_position", "default": "1", "in": ["1"]}]}});
    let r = mapping::items(&src, d.path().to_str().unwrap()).unwrap();
    assert_eq!(rels(&r.items), ["C4_v2_rr1.wav"]);
    assert_eq!((&r.items[0]["layer"], &r.items[0]["layers"], &r.items[0]["dynamic"]), (&json!(2), &json!(2), &json!("2/2")));
    assert!(r.replaced.is_empty(), "{:?}", r.replaced);
    assert_eq!(rels(&r.left_out), ["C4_v1_rr1.wav", "C4_v1_rr2.wav"]);
}
