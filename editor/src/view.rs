//! What the mapping view draws, computed from the items: the keyboard grid's cells (a key and
//! layer: present, several files, or pitch suspect) and the item table's rows.
use crate::audio::Measures;
use crate::mapping::Item;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

pub fn note_label(n: i64) -> String {
    format!("{}{}", NAMES[n.rem_euclid(12) as usize], n.div_euclid(12) - 1)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cell {
    Present,
    Duplicate,
    Suspect,
    Flagged,
    LeftOut,
    Spare,
}

/// where several states meet in a cell, the one shown: left out, pitch suspect, flagged, spare,
/// several files, one
fn rank(c: Cell) -> u8 {
    match c {
        Cell::LeftOut => 5,
        Cell::Suspect => 4,
        Cell::Flagged => 3,
        Cell::Spare => 2,
        Cell::Duplicate => 1,
        Cell::Present => 0,
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub lo: i64,
    pub hi: i64,
    pub layers: i64,
    pub cells: BTreeMap<(i64, i64), (Cell, Vec<usize>)>,
}

/// the grid: per key and layer its items and one state (`suspect`, `flagged`: paths; `spares`:
/// relpaths of items standing in for a left-out take; `left_out`: items read no more, shown where
/// nothing else of their key and layer is read)
pub fn grid(items: &[Item], suspect: &BTreeSet<String>, flagged: &BTreeSet<String>, left_out: &[Item], spares: &BTreeSet<String>) -> Grid {
    let mut cells: BTreeMap<(i64, i64), (Cell, Vec<usize>)> = BTreeMap::new();
    let (mut lo, mut hi, mut layers) = (21, 108, 1);
    let mut span = |it: &Item| -> Option<(i64, i64)> {
        let (n, l) = (it["note"].as_i64()?, it["layer"].as_i64()?);
        lo = lo.min(n);
        hi = hi.max(n);
        layers = layers.max(l).max(it.get("layers").and_then(|v| v.as_i64()).unwrap_or(1));
        Some((n, l))
    };
    for (i, it) in items.iter().enumerate() {
        let Some(at) = span(it) else { continue };
        let c = cells.entry(at).or_insert((Cell::Present, Vec::new()));
        c.1.push(i);
        let path = it["path"].as_str().unwrap_or_default();
        let own = if suspect.contains(path) {
            Cell::Suspect
        } else if flagged.contains(path) {
            Cell::Flagged
        } else if it["relpath"].as_str().is_some_and(|r| spares.contains(r)) {
            Cell::Spare
        } else if c.1.len() > 1 {
            Cell::Duplicate
        } else {
            Cell::Present
        };
        if rank(own) > rank(c.0) {
            c.0 = own;
        }
    }
    for it in left_out {
        if let Some(at) = span(it) {
            cells.entry(at).or_insert((Cell::LeftOut, Vec::new()));
        }
    }
    Grid { lo, hi, layers, cells }
}

/// the selected row and key after the items changed: a row no longer there (or no longer on the
/// selected key) and a key with no items are cleared
pub fn clamp_selection(row: Option<usize>, note: Option<i64>, items: &[Item]) -> (Option<usize>, Option<i64>) {
    let note = note.filter(|n| items.iter().any(|i| i["note"].as_i64() == Some(*n)));
    let row = row.filter(|&r| r < items.len() && note.is_none_or(|n| items[r]["note"].as_i64() == Some(n)));
    (row, note)
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowView {
    pub index: usize,
    pub file: String,
    pub note: i64,
    pub name: String,
    pub dynamic: String,
    pub layer: String,
    pub layer_n: i64,
    pub cents: Option<f32>,
    pub level_db: Option<f32>,
    /// the file's absolute path (to play it)
    pub path: String,
    /// why it is flagged, in words
    pub flaw: Option<String>,
    /// for a spare take: the relpath of the take it would stand in for
    pub spare_of: Option<String>,
    /// a take the source leaves out (its skip_files)
    pub left_out: bool,
}

/// the table's rows: each item (`index` its place in `items`), its spare takes after it, the
/// left-out takes at their key (`index` past the items for those); `flaws` by path
pub fn rows(items: &[Item], offsets: &BTreeMap<String, f32>, m: &Measures, note: Option<i64>, flaws: &BTreeMap<String, String>,
            spares: &BTreeMap<String, Vec<Item>>, left_out: &[Item]) -> Vec<RowView> {
    let view = |i: usize, it: &Item, spare_of: Option<String>, left: bool| -> Option<RowView> {
        let n = it["note"].as_i64()?;
        if note.is_some_and(|k| k != n) {
            return None;
        }
        let path = it["path"].as_str().unwrap_or_default();
        Some(RowView {
            index: i,
            file: it["relpath"].as_str().unwrap_or_default().to_string(),
            note: n,
            name: note_label(n),
            dynamic: it["dynamic"].as_str().map(String::from).unwrap_or_else(|| it["dynamic"].to_string()),
            layer: format!("{}/{}", it["layer"], it["layers"]),
            layer_n: it["layer"].as_i64().unwrap_or(0),
            cents: offsets.get(path).copied(),
            level_db: m.ok.get(path).map(|x| x.level_db),
            path: path.to_string(),
            flaw: flaws.get(path).cloned(),
            spare_of,
            left_out: left,
        })
    };
    let mut out = Vec::new();
    let mut extra = items.len();
    let mut left: Vec<&Item> = left_out.iter().collect();
    left.sort_by_key(|it| it["note"].as_i64().unwrap_or(0));
    let mut left = left.into_iter().peekable();
    for (i, it) in items.iter().enumerate() {
        let n = it["note"].as_i64().unwrap_or(0);
        while let Some(l) = left.next_if(|l| l["note"].as_i64().unwrap_or(0) < n) {
            out.extend(view(extra, l, None, true));
            extra += 1;
        }
        out.extend(view(i, it, None, false));
        let rel = it["relpath"].as_str().unwrap_or_default();
        for sp in spares.get(rel).into_iter().flatten() {
            out.extend(view(extra, sp, Some(rel.to_string()), false));
            extra += 1;
        }
    }
    for l in left {
        out.extend(view(extra, l, None, true));
        extra += 1;
    }
    out
}

/// a table column's order
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Col {
    File,
    Key,
    Dynamic,
    Pitch,
    Level,
}

/// natural order: digit runs compare as numbers ("2.wav" before "10.wav"), the rest by character
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn digits(it: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut s = String::new();
        while let Some(&c) = it.peek() {
            if !c.is_ascii_digit() {
                break;
            }
            s.push(c);
            it.next();
        }
        s
    }
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let (s, t) = (digits(&mut x), digits(&mut y));
                let (s0, t0) = (s.trim_start_matches('0'), t.trim_start_matches('0'));
                let o = s0.len().cmp(&t0.len()).then_with(|| s0.cmp(t0)).then_with(|| s.len().cmp(&t.len()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

/// measured values by value, the missing ones last in either direction
fn measured_cmp(a: Option<f32>, b: Option<f32>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => if descending { y.total_cmp(&x) } else { x.total_cmp(&y) },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// the rows in a column's order: keys by MIDI number, dynamics by layer (softest first), files in
/// natural order, the pitch by the size of its deviation, the level by value (unmeasured last);
/// ties by key, layer and file
pub fn sort_rows(rows: &mut [RowView], col: Col, descending: bool) {
    let dir = |o: Ordering| if descending { o.reverse() } else { o };
    rows.sort_by(|a, b| {
        let first = match col {
            Col::File => dir(natural_cmp(&a.file, &b.file)),
            Col::Key => dir(a.note.cmp(&b.note)),
            Col::Dynamic => dir(a.layer_n.cmp(&b.layer_n)),
            Col::Pitch => measured_cmp(a.cents.map(f32::abs), b.cents.map(f32::abs), descending),
            Col::Level => measured_cmp(a.level_db, b.level_db, descending),
        };
        first.then_with(|| a.note.cmp(&b.note)).then_with(|| a.layer_n.cmp(&b.layer_n)).then_with(|| natural_cmp(&a.file, &b.file))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(path: &str, note: i64, layer: i64) -> Item {
        json!({"path": path, "relpath": path, "note": note, "layer": layer, "layers": 2, "dynamic": "mf"})
            .as_object().unwrap().clone()
    }

    #[test]
    fn note_labels() {
        assert_eq!([21, 60, 61, 108].map(note_label), ["A0", "C4", "C#4", "C8"]);
    }

    #[test]
    fn the_grid_marks_duplicates_and_suspects() {
        let items = vec![item("a", 60, 1), item("b", 60, 1), item("c", 62, 2), item("d", 110, 1)];
        let suspect: BTreeSet<String> = ["c".to_string()].into();
        let g = grid(&items, &suspect, &BTreeSet::new(), &[], &BTreeSet::new());
        assert_eq!((g.lo, g.hi, g.layers), (21, 110, 2));
        assert_eq!(g.cells[&(60, 1)], (Cell::Duplicate, vec![0, 1]));
        assert_eq!(g.cells[&(62, 2)].0, Cell::Suspect);
        assert_eq!(g.cells[&(110, 1)].0, Cell::Present);
        assert!(!g.cells.contains_key(&(61, 1)));
    }

    #[test]
    fn a_selection_outside_the_items_is_cleared() {
        // the top key skipped (or a map edit): the selected row and key may be gone
        let items = vec![item("a", 60, 1), item("c", 62, 2)];
        assert_eq!(clamp_selection(Some(1), Some(62), &items), (Some(1), Some(62)));
        assert_eq!(clamp_selection(Some(2), Some(64), &items), (None, None));
        assert_eq!(clamp_selection(Some(0), Some(62), &items), (None, Some(62)), "the row is no longer on the key");
    }

    #[test]
    fn rows_filtered_by_key() {
        let items = vec![item("a", 60, 1), item("c", 62, 2)];
        let offsets: BTreeMap<String, f32> = [("c".to_string(), 12.5)].into();
        let r = rows(&items, &offsets, &Measures::default(), Some(62), &BTreeMap::new(), &BTreeMap::new(), &[]);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].index, r[0].name.as_str(), r[0].layer.as_str(), r[0].cents), (1, "D4", "2/2", Some(12.5)));
    }

    fn row(file: &str, note: i64, layer: i64, cents: Option<f32>) -> RowView {
        RowView { index: 0, file: file.into(), note, name: note_label(note), path: String::new(), flaw: None, spare_of: None, left_out: false, dynamic: ["pp", "mf", "ff"][(layer - 1) as usize].into(),
            layer: format!("{layer}/3"), layer_n: layer, cents, level_db: None }
    }

    fn files(rows: &[RowView]) -> Vec<&str> {
        rows.iter().map(|r| r.file.as_str()).collect()
    }

    #[test]
    fn keys_sort_by_pitch_not_by_name() {
        let mut r = vec![row("a", 57, 1, None), row("b", 56, 1, None), row("c", 61, 1, None), row("d", 60, 1, None)];
        sort_rows(&mut r, Col::Key, false);
        assert_eq!(files(&r), ["b", "a", "d", "c"], "G#3 before A3, C4 before C#4");
        sort_rows(&mut r, Col::Key, true);
        assert_eq!(files(&r), ["c", "d", "a", "b"]);
    }

    #[test]
    fn dynamics_sort_by_layer_softest_first() {
        let mut r = vec![row("a", 60, 3, None), row("b", 60, 1, None), row("c", 60, 2, None)];
        sort_rows(&mut r, Col::Dynamic, false);
        assert_eq!(files(&r), ["b", "c", "a"], "pp, mf, ff (not ff, mf, pp)");
    }

    #[test]
    fn files_sort_in_natural_order() {
        let mut r = vec![row("x/10.wav", 60, 1, None), row("x/2.wav", 61, 1, None), row("x/1.wav", 62, 1, None)];
        sort_rows(&mut r, Col::File, false);
        assert_eq!(files(&r), ["x/1.wav", "x/2.wav", "x/10.wav"]);
        assert_eq!(natural_cmp("a02", "a2"), std::cmp::Ordering::Greater, "equal numbers: the longer text after");
        assert_eq!(natural_cmp("v9_b", "v10_a"), std::cmp::Ordering::Less);
    }

    #[test]
    fn missing_values_sort_last_both_ways() {
        let mut r = vec![row("a", 60, 1, None), row("b", 61, 1, Some(-80.0)), row("c", 62, 1, Some(12.0))];
        sort_rows(&mut r, Col::Pitch, true);
        assert_eq!(files(&r), ["b", "c", "a"], "the largest deviation first, the unmeasured last");
        sort_rows(&mut r, Col::Pitch, false);
        assert_eq!(files(&r), ["c", "b", "a"], "the unmeasured last again");
    }
}
