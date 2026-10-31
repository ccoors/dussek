//! The Check page's model: the grid's cells (one state each, by priority) and the table's rows
//! (spare takes after their take, left-out takes at their key).
use pe_editor::audio::Measures;
use pe_editor::mapping::Item;
use pe_editor::view::{self, Cell};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn item(note: i64, layer: i64, rel: &str) -> Item {
    json!({"path": format!("/s/{rel}"), "relpath": rel, "note": note, "layer": layer, "layers": 1, "dynamic": "mf"})
        .as_object().unwrap().clone()
}

#[test]
fn cell_priority_left_out_pitch_flagged_spare_several_one() {
    let items = vec![item(60, 1, "a.wav"), item(61, 1, "b1.wav"), item(61, 1, "b2.wav"), item(62, 1, "c.wav"),
        item(63, 1, "d1.wav"), item(63, 1, "d2.wav"), item(64, 1, "e.wav"), item(66, 1, "g.wav")];
    let suspect: BTreeSet<String> = ["/s/c.wav".to_string(), "/s/g.wav".to_string()].into();
    let flagged: BTreeSet<String> = ["/s/d2.wav".to_string()].into();
    let spares: BTreeSet<String> = ["e.wav".to_string()].into();
    let left_out = vec![item(65, 1, "f.wav"), item(66, 1, "g0.wav")];
    let g = view::grid(&items, &suspect, &flagged, &left_out, &spares);
    let cell = |n: i64| g.cells.get(&(n, 1)).map(|c| c.0);
    assert_eq!(cell(60), Some(Cell::Present));
    assert_eq!(cell(61), Some(Cell::Duplicate));
    assert_eq!(cell(62), Some(Cell::Suspect));
    assert_eq!(cell(63), Some(Cell::Flagged));
    assert_eq!(cell(64), Some(Cell::Spare));
    assert_eq!(cell(65), Some(Cell::LeftOut));
    // a left-out take where another take of the key and layer is read: the read one's state
    assert_eq!(cell(66), Some(Cell::Suspect));
}

#[test]
fn spares_are_listed_after_their_take() {
    let items = vec![item(60, 1, "a1.wav"), item(62, 1, "c.wav")];
    let spares: BTreeMap<String, Vec<Item>> = [("a1.wav".to_string(), vec![item(60, 1, "a2.wav"), item(60, 1, "a3.wav")])].into();
    let left_out = vec![item(61, 1, "b.wav")];
    let flaws: BTreeMap<String, String> = [("/s/c.wav".to_string(), "rises 18 dB at 6.2 s".to_string())].into();
    let rows = view::rows(&items, &BTreeMap::new(), &Measures::default(), None, &flaws, &spares, &left_out);
    let files: Vec<&str> = rows.iter().map(|r| r.file.as_str()).collect();
    assert_eq!(files, ["a1.wav", "a2.wav", "a3.wav", "b.wav", "c.wav"]);
    assert_eq!(rows[1].spare_of.as_deref(), Some("a1.wav"));
    assert!(rows[0].spare_of.is_none() && !rows[0].left_out);
    assert!(rows[3].left_out);
    assert_eq!(rows[4].flaw.as_deref(), Some("rises 18 dB at 6.2 s"));
}
