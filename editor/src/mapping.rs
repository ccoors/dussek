//! A sample set's items from its source's `map` (project.toml), as analysis/mapping.py gives
//! them (the parity test keeps the two equal): `sfz` (an SFZ file's regions) or `pattern` (file
//! names: a glob and a regex), then the source's `relabel`, `skip`, `skip_files` (left out before
//! the take choice: a spare take takes a left-out one's place), `tail_cut_s`, `cut_s`, `gain_db`,
//! `extra` and `octave`. Besides the items, the report lists what was left out and why, and each
//! item's spare takes.
use crate::pyfmt::{basename, format, join, normpath, note_name, py_float, py_int, py_str};
use crate::sfz::{self, Region};
use regex::Regex;
use serde_json::{Map, Value as Json, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub type Item = Map<String, Json>;

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub items: Vec<Item>,
    /// files the glob found that the regex rejects (pattern), relative to the root
    pub unmatched: Vec<String>,
    /// (relpath, or an SFZ region's sample, why) of what was left out
    pub dropped: Vec<(String, String)>,
    /// the items the source's `skip_files` leaves out, as they map without it
    pub left_out: Vec<Item>,
    /// `skip_files` entries the set does not have
    pub left_out_missing: Vec<String>,
    /// per item (its relpath), the other takes of its key and layer in the order they would take
    /// its place (none where the map keeps every file)
    pub spares: BTreeMap<String, Vec<Item>>,
    /// a spare in use (its relpath) → the left-out take it stands in for
    pub replaced: BTreeMap<String, String>,
}

/// an item with the keys its ordering needs, and its spare takes (same key and layer)
struct Row {
    note: i64,
    layer: i64,
    path: String,
    item: Item,
    spares: Vec<Row>,
}

/// the opcodes that number a key's takes (round robins): a filter on one is the take rule
const TAKE_OPCODES: [&str; 2] = ["seq_position", "lorand"];

/// the regions kept by the take rule `takes` (filters on TAKE_OPCODES), the left-out files `skip`
/// dropped first: per key and velocity range the region passing the rule, or, where that one is
/// left out, the next take not left out (the lowest value, then the sample); a group with no
/// region passing the rule keeps none. Each kept region's spares (by sample): its group's other
/// regions not left out, in that order. Without a rule, only the left-out files go.
fn take_choice(rs: Vec<Region>, takes: &[Json], skip: &HashSet<String>, rel_of: &dyn Fn(&Region) -> String,
               rep: &mut Report) -> Result<(Vec<Region>, HashMap<String, Vec<Region>>), String> {
    let mut spares = HashMap::new();
    if takes.is_empty() {
        let kept = rs.into_iter().filter(|r| {
            let left = skip.contains(&rel_of(r));
            if left {
                rep.dropped.push((rel_of(r), "left out (the source's skip_files)".into()));
            }
            !left
        }).collect();
        return Ok((kept, spares));
    }
    let key = |r: &Region| (r.get("pitch_keycenter").or_else(|| r.get("lokey")).cloned().unwrap_or_else(|| "60".into()),
        r.get("lovel").cloned().unwrap_or_else(|| "0".into()), r.get("hivel").cloned().unwrap_or_else(|| "127".into()));
    let f = &takes[0];
    let op = py_str(&f["opcode"]);
    let value = |r: &Region| -> Result<f64, String> {
        match r.get(&op) {
            Some(v) => py_float(&json!(v)),
            None => py_float(f.get("default").unwrap_or(&json!(0))),
        }
    };
    let mut order: Vec<(String, String, String)> = Vec::new();
    let mut groups: HashMap<(String, String, String), Vec<usize>> = HashMap::new();
    for (i, r) in rs.iter().enumerate() {
        let k = key(r);
        if !groups.contains_key(&k) {
            order.push(k.clone());
        }
        groups.entry(k).or_default().push(i);
    }
    let mut kept_ix: HashSet<usize> = HashSet::new();
    let mut spare_ix: HashMap<usize, Vec<usize>> = HashMap::new();
    for k in &order {
        let g = &groups[k];
        let mut passing = Vec::new();
        for &i in g {
            if takes.iter().map(|t| passes(&rs[i], t)).collect::<Result<Vec<bool>, String>>()?.iter().all(|b| *b) {
                passing.push(i);
            }
        }
        let mut rest: Vec<(f64, String, usize)> = Vec::new();
        for &i in g {
            if skip.contains(&rel_of(&rs[i])) {
                rep.dropped.push((rel_of(&rs[i]), "left out (the source's skip_files)".into()));
            } else {
                rest.push((value(&rs[i])?, rs[i]["sample"].clone(), i));
            }
        }
        rest.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let keep: Vec<usize> = passing.iter().copied().filter(|i| !skip.contains(&rel_of(&rs[*i]))).collect();
        let keep = if keep.is_empty() && !passing.is_empty() { rest.first().map(|r| vec![r.2]).unwrap_or_default() } else { keep };
        for &i in &keep {
            kept_ix.insert(i);
            let own = rel_of(&rs[i]);
            spare_ix.insert(i, rest.iter().map(|r| r.2).filter(|j| !keep.contains(j) && rel_of(&rs[*j]) != own).collect());
        }
    }
    let mut kept = Vec::new();
    let all: Vec<Option<Region>> = rs.into_iter().map(Some).collect();
    let mut all = all;
    // the spares first (cloned), then the kept regions in their order
    for (i, js) in &spare_ix {
        let list: Vec<Region> = js.iter().filter_map(|j| all[*j].clone()).collect();
        if let Some(r) = &all[*i] {
            spares.insert(r["sample"].clone(), list);
        }
    }
    for (i, r) in all.iter_mut().enumerate() {
        if kept_ix.contains(&i) {
            kept.push(r.take().unwrap());
        } else if let Some(r) = r.take() {
            if !skip.contains(&rel_of(&r)) {
                rep.dropped.push((r["sample"].replace('\\', "/"), format!("filter {op}")));
            }
        }
    }
    Ok((kept, spares))
}

fn get<'a>(m: &'a Json, k: &str) -> Option<&'a Json> {
    m.get(k).filter(|v| !v.is_null())
}

fn need<'a>(m: &'a Json, k: &str) -> Result<&'a Json, String> {
    get(m, k).ok_or_else(|| format!("map: {k} missing"))
}

fn as_str<'a>(v: &'a Json, what: &str) -> Result<&'a str, String> {
    v.as_str().ok_or_else(|| format!("{what}: not a string"))
}

/// Python truthiness of an optional value
fn truthy(v: Option<&Json>) -> bool {
    match v {
        None | Some(Json::Null) | Some(Json::Bool(false)) => false,
        Some(Json::String(s)) => !s.is_empty(),
        Some(Json::Array(a)) => !a.is_empty(),
        Some(Json::Object(o)) => !o.is_empty(),
        Some(Json::Number(n)) => n.as_f64() != Some(0.0),
        Some(_) => true,
    }
}

fn passes(r: &Region, f: &Json) -> Result<bool, String> {
    let opcode = as_str(need(f, "opcode")?, "filter opcode")?;
    let v: Json = match r.get(opcode) {
        Some(s) => Json::String(s.clone()),
        None => match get(f, "default") {
            Some(d) => d.clone(),
            None => return Ok(false),
        },
    };
    if let Some(list) = get(f, "in") {
        let s = py_str(&v);
        return Ok(list.as_array().is_some_and(|a| a.iter().any(|x| x.as_str() == Some(s.as_str()))));
    }
    if let Some(lt) = get(f, "lt") {
        return Ok(py_float(&v)? < py_float(lt)?);
    }
    if let Some(eq) = get(f, "eq") {
        return Ok(py_float(&v)? == py_float(eq)?);
    }
    Err(format!("filter {f}: needs in, lt or eq"))
}

fn sfz_rows(root: &str, m: &Json, skip: &HashSet<String>, rep: &mut Report) -> Result<Vec<Row>, String> {
    let files = need(m, "files")?.as_array().ok_or("map: files is not a list")?;
    let all_filters: Vec<Json> = get(m, "filter").and_then(Json::as_array).cloned().unwrap_or_default();
    let is_take = |f: &Json| TAKE_OPCODES.contains(&py_str(&f["opcode"]).as_str());
    let takes: Vec<Json> = all_filters.iter().filter(|f| is_take(f)).cloned().collect();
    let filters: Vec<Json> = all_filters.iter().filter(|f| !is_take(f)).cloned().collect();
    let prefix = get(m, "prefix").map(|v| py_str(v)).unwrap_or_default();
    let norm = truthy(get(m, "normpath"));
    let rel_of = |r: &Region| {
        let rel = format!("{prefix}{}", r["sample"].replace('\\', "/"));
        if norm { normpath(&rel) } else { rel }
    };
    let mut spares_of: HashMap<String, Vec<Region>> = HashMap::new();
    let folders: Option<HashSet<String>> = if truthy(get(m, "folders")) {
        Some(need(m, "folders")?.as_array().ok_or("map: folders is not a list")?
            .iter().filter_map(|v| v.as_str().map(String::from)).collect())
    } else {
        None
    };
    let pref = get(m, "prefer_substring").and_then(Json::as_str).filter(|s| !s.is_empty());
    let mut per_file: Vec<Vec<Region>> = Vec::new();
    // the takes kept were nothing left out: they number a key's layers
    let mut whole: Vec<Vec<Region>> = Vec::new();
    for name in files {
        let path = join(root, as_str(name, "map: files")?);
        let regions = sfz::parse_file(Path::new(&path))?;
        let mut rs = Vec::new();
        'region: for r in regions {
            let Some(sample) = r.get("sample") else { continue };
            for f in &filters {
                if !passes(&r, f)? {
                    rep.dropped.push((sample.replace('\\', "/"), format!("filter {}", py_str(&f["opcode"]))));
                    continue 'region;
                }
            }
            rs.push(r);
        }
        if let Some(folders) = &folders {
            rs.retain(|r| {
                let s = r["sample"].replace('\\', "/");
                let parts: Vec<&str> = s.split('/').collect();
                let keep = parts[..parts.len() - 1].iter().any(|p| folders.contains(*p));
                if !keep {
                    rep.dropped.push((s.clone(), "not in the map's folders".into()));
                }
                keep
            });
        }
        if let Some(pref) = pref {
            if rs.iter().any(|r| r["sample"].contains(pref)) {
                rs.retain(|r| {
                    let keep = r["sample"].contains(pref);
                    if !keep {
                        rep.dropped.push((r["sample"].replace('\\', "/"), format!("another sample has {pref:?}")));
                    }
                    keep
                });
            }
        }
        whole.push(take_choice(rs.clone(), &takes, &HashSet::new(), &rel_of, &mut Report::default())?.0);
        let (rs, sp) = take_choice(rs, &takes, skip, &rel_of, rep)?;
        spares_of.extend(sp);
        per_file.push(rs);
    }

    let mut out = Vec::new();

    if get(m, "layers").and_then(Json::as_str) == Some("file") {
        let fmt = as_str(need(m, "dynamic_format")?, "map: dynamic_format")?;
        let unique = get(m, "unique").and_then(Json::as_str) == Some("sample");
        let n = per_file.len() as i64;
        for (i, rs) in per_file.iter().enumerate() {
            let layer = i as i64 + 1;
            let mut seen = HashSet::new();
            for r in rs {
                let rel = rel_of(r);
                if unique && !seen.insert(rel.clone()) {
                    rep.dropped.push((rel, "the same sample again".into()));
                    continue;
                }
                let key = r.get("pitch_keycenter").or_else(|| r.get("lokey")).ok_or("a region without pitch_keycenter or lokey")?;
                let note = py_int(&json!(key))?;
                let row = |rel: String| -> Result<Row, String> {
                    let path = join(root, &rel);
                    let mut it = Item::new();
                    it.insert("path".into(), json!(path));
                    it.insert("relpath".into(), json!(rel));
                    it.insert("note".into(), json!(note));
                    it.insert("layer".into(), json!(layer));
                    it.insert("layers".into(), json!(n));
                    it.insert("dynamic".into(), json!(format(fmt, None, Some(layer))?));
                    it.insert("tail_cut_s".into(), json!(0.0));
                    Ok(Row { note, layer, path, item: it, spares: Vec::new() })
                };
                let mut kept = row(rel)?;
                for sp in spares_of.get(&r["sample"]).into_iter().flatten() {
                    kept.spares.push(row(rel_of(sp))?);
                }
                out.push(kept);
            }
        }
        return Ok(out);
    }

    let ranks = by_note(whole)?;
    let dyn_from = get(m, "dynamic_from");
    let sfz_fields = get(m, "sfz_fields").map(|v| truthy(Some(v))).unwrap_or(true);
    for (note, rs) in by_note(per_file)? {
        // the ranks of the key's takes before any is left out: a left-out layer stays empty
        let mut slots: Vec<Option<(i64, i64)>> = ranks.get(&note).into_iter().flatten().map(|r| velocities(r).map(Some)).collect::<Result<_, _>>()?;
        let n = slots.len() as i64;
        for kept in &rs {
            let v = velocities(kept)?;
            let at = slots.iter().position(|s| *s == Some(v)).ok_or_else(|| format!("{}: no layer among the key's takes", kept["sample"]))?;
            slots[at] = None;
            let layer = at as i64 + 1;
            let mut rows_here = Vec::new();
            for r in std::iter::once(kept).chain(spares_of.get(&kept["sample"]).into_iter().flatten()) {
                let rel = rel_of(r);
                let dynamic = match dyn_from {
                    Some(d) => {
                        let rx = Regex::new(as_str(need(d, "regex")?, "dynamic_from regex")?).map_err(|e| e.to_string())?;
                        let c = rx.captures(basename(&rel)).ok_or_else(|| format!("dynamic_from: no match in {rel}"))?;
                        let g = c.get(1).map(|m| m.as_str()).unwrap_or("None");
                        format(as_str(need(d, "format")?, "dynamic_from format")?, Some(g), None)?
                    }
                    None => format!("{layer}/{n}"),
                };
                let path = join(root, &rel);
                let mut it = Item::new();
                it.insert("path".into(), json!(path));
                it.insert("relpath".into(), json!(rel));
                it.insert("note".into(), json!(note));
                it.insert("layer".into(), json!(layer));
                it.insert("layers".into(), json!(n));
                it.insert("dynamic".into(), json!(dynamic));
                it.insert("tail_cut_s".into(), json!(0.0));
                if sfz_fields {
                    let lo = r.get("lovel").map(|v| py_int(&json!(v))).transpose()?.unwrap_or(0);
                    let hi = r.get("hivel").map(|v| py_int(&json!(v))).transpose()?.unwrap_or(127);
                    let vol = r.get("volume").map(|v| py_float(&json!(v))).transpose()?.unwrap_or(0.0);
                    let tune = r.get("tune").map(|v| py_float(&json!(v))).transpose()?.unwrap_or(0.0);
                    it.insert("velocity".into(), json!([lo, hi]));
                    it.insert("sfz_volume_db".into(), json!(vol));
                    it.insert("sfz_tune_cents".into(), json!(tune));
                }
                rows_here.push(Row { note, layer, path, item: it, spares: Vec::new() });
            }
            let mut row = rows_here.remove(0);
            row.spares = rows_here;
            out.push(row);
        }
    }
    Ok(out)
}

/// the regions per note (the SFZ default 60 where a region leaves it out), a sample seen again in
/// its first place, ranked by velocity
fn by_note(per_file: Vec<Vec<Region>>) -> Result<std::collections::BTreeMap<i64, Vec<Region>>, String> {
    let mut notes: std::collections::BTreeMap<i64, (Vec<Region>, HashMap<String, usize>)> = Default::default();
    for r in per_file.into_iter().flatten() {
        let note = r.get("pitch_keycenter").map(|k| py_int(&json!(k))).transpose()?.unwrap_or(60);
        let (list, index) = notes.entry(note).or_default();
        let sample = r["sample"].clone();
        match index.get(&sample) {
            Some(&i) => list[i] = r,
            None => {
                index.insert(sample, list.len());
                list.push(r);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    for (note, (rs, _)) in notes {
        let mut keyed = Vec::with_capacity(rs.len());
        for r in rs {
            keyed.push((velocities(&r)?.0, r));
        }
        keyed.sort_by_key(|(lovel, _)| *lovel);
        out.insert(note, keyed.into_iter().map(|(_, r)| r).collect());
    }
    Ok(out)
}

fn velocities(r: &Region) -> Result<(i64, i64), String> {
    let lo = r.get("lovel").map(|v| py_int(&json!(v))).transpose()?.unwrap_or(0);
    let hi = r.get("hivel").map(|v| py_int(&json!(v))).transpose()?.unwrap_or(127);
    Ok((lo, hi))
}

fn note_of(g: &HashMap<String, Option<String>>, spec: &Json) -> Result<i64, String> {
    let group = get(spec, "group").and_then(Json::as_str).unwrap_or("note");
    let v = g.get(group).ok_or_else(|| format!("no group {group:?} in the regex"))?
        .as_deref().ok_or_else(|| format!("group {group:?} did not match"))?;
    let shift = get(spec, "shift").map(py_int).transpose()?.unwrap_or(0);
    match get(spec, "style").and_then(Json::as_str).unwrap_or("name") {
        "name" => Ok(note_name(v)? + shift),
        "midi" => Ok(py_int(&json!(v))? + shift),
        "index" => Ok(py_int(&json!(v))? + get(spec, "offset").map(py_int).transpose()?.unwrap_or(0)),
        s => Err(format!("note style {s:?}")),
    }
}

/// one file the glob found and the regex matched: its path, its path relative to the root and
/// the regex's named groups
#[derive(Clone, Debug)]
pub struct Match {
    pub path: String,
    pub rel: String,
    pub groups: HashMap<String, Option<String>>,
}

/// a pattern map's files: those the regex matches (in path order) and the relpaths it rejects;
/// the map's layers are not applied (every dynamic value the files hold is there)
pub fn pattern_matches(root: &str, m: &Json) -> Result<(Vec<Match>, Vec<String>), String> {
    let rx_src = as_str(need(m, "regex")?, "map: regex")?;
    let rx = Regex::new(&format!("^(?:{rx_src})$")).map_err(|e| format!("regex: {e}"))?;
    let by_relpath = get(m, "match").and_then(Json::as_str) == Some("relpath");
    let pattern = format!("{}/{}", glob::Pattern::escape(root.trim_end_matches('/')), as_str(need(m, "glob")?, "map: glob")?);
    let opts = glob::MatchOptions { case_sensitive: true, require_literal_separator: true, require_literal_leading_dot: true };
    let mut paths: Vec<String> = glob::glob_with(&pattern, opts).map_err(|e| format!("glob: {e}"))?
        .flatten().map(|p| p.to_string_lossy().into_owned()).collect();
    paths.sort();
    let names_rx: Vec<String> = rx.capture_names().flatten().map(String::from).collect();
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let (mut out, mut rejected) = (Vec::new(), Vec::new());
    for path in paths {
        let rel = normpath(path.strip_prefix(&prefix).unwrap_or(&path));
        let target = if by_relpath { rel.as_str() } else { basename(&path) };
        let Some(c) = rx.captures(target) else {
            rejected.push(rel);
            continue;
        };
        let groups = names_rx.iter().map(|n| (n.clone(), c.name(n).map(|m| m.as_str().to_string()))).collect();
        out.push(Match { path, rel, groups });
    }
    Ok((out, rejected))
}

/// a raw dynamic value's name in the items: the map's dynamic_names, else its dynamic_format
pub fn dynamic_name(m: &Json, raw: &str) -> Result<Json, String> {
    if let Some(n) = get(m, "dynamic_names").and_then(Json::as_object).and_then(|o| o.get(raw)) {
        return Ok(n.clone());
    }
    let fmt = get(m, "dynamic_format").map(|v| py_str(v)).unwrap_or_else(|| "{}".into());
    Ok(json!(format(&fmt, Some(raw), None)?))
}

fn pattern_rows(root: &str, m: &Json, relabel: &HashMap<i64, Json>, skip: &HashSet<String>, rep: &mut Report) -> Result<Vec<Row>, String> {
    let (matches, rejected) = pattern_matches(root, m)?;
    rep.unmatched.extend(rejected);
    let layers: Option<Vec<Json>> = match get(m, "layers") {
        Some(v) => Some(v.as_array().ok_or("map: layers is not a list")?.clone()),
        None => None,
    };
    let prefer: Vec<String> = get(m, "prefer").and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let fields = get(m, "fields").and_then(Json::as_object).cloned().unwrap_or_default();
    let mut rows = Vec::new();
    // per key and layer every take, ranked: the lowest is kept, the others are its spares
    let mut takes: HashMap<(i64, i64), Vec<(Vec<i64>, String, Row)>> = HashMap::new();
    for Match { path, rel, groups: g } in matches {
        if skip.contains(&rel) {
            rep.dropped.push((rel, "left out (the source's skip_files)".into()));
            continue;
        }
        let note0 = note_of(&g, need(m, "note")?)?;
        let note_v = relabel.get(&note0).cloned().unwrap_or(json!(note0));
        let note = note_v.as_i64().ok_or("relabel: not an integer")?;
        let (layer, n, dynamic) = match &layers {
            None => (1, 1, need(m, "dynamic")?.clone()),
            Some(layers) => {
                let d = g.get("dynamic").ok_or("no group \"dynamic\" in the regex")?;
                let Some(d) = d.as_deref() else { continue };
                let Some(i) = layers.iter().position(|l| l.as_str() == Some(d)) else {
                    rep.dropped.push((rel, format!("dynamic {d:?} not in the map's layers")));
                    continue;
                };
                (i as i64 + 1, layers.len() as i64, dynamic_name(m, d)?)
            }
        };
        let mut it = Item::new();
        it.insert("path".into(), json!(path));
        it.insert("relpath".into(), json!(rel));
        it.insert("note".into(), note_v);
        it.insert("layer".into(), json!(layer));
        it.insert("layers".into(), json!(n));
        it.insert("dynamic".into(), dynamic);
        it.insert("tail_cut_s".into(), json!(0.0));
        for (k, t) in &fields {
            let v = g.get(k).ok_or_else(|| format!("no group {k:?} in the regex"))?;
            let v = if t.as_str() == Some("int") {
                json!(py_int(&json!(v.as_deref().ok_or_else(|| format!("group {k:?} did not match"))?))?)
            } else {
                v.as_deref().map(|s| json!(s)).unwrap_or(Json::Null)
            };
            it.insert(k.clone(), v);
        }
        let row = Row { note, layer, path: path.clone(), item: it, spares: Vec::new() };
        if prefer.is_empty() {
            rows.push(row);
            continue;
        }
        let mut rank = Vec::new();
        for k in &prefer {
            let v = g.get(k).and_then(|v| v.clone()).ok_or_else(|| format!("prefer: group {k:?} did not match"))?;
            rank.push(py_int(&json!(v))?);
        }
        takes.entry((note, layer)).or_default().push((rank, path.clone(), row));
    }
    // Python: the lowest (prefer values…, path) per key and layer
    for (_, mut list) in takes {
        list.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let mut list = list.into_iter().map(|(_, _, r)| r);
        let Some(mut kept) = list.next() else { continue };
        for r in list {
            rep.dropped.push((r.item["relpath"].as_str().unwrap_or_default().to_string(), "another take of this key and layer is kept".into()));
            kept.spares.push(r);
        }
        rows.push(kept);
    }
    match get(m, "order").and_then(Json::as_str).unwrap_or("note") {
        "note" => rows.sort_by(|a, b| (a.note, a.layer, &a.path).cmp(&(b.note, b.layer, &b.path))),
        "layer" => rows.sort_by(|a, b| (a.layer, a.note, &a.path).cmp(&(b.layer, b.note, &b.path))),
        "path" => rows.sort_by(|a, b| a.path.cmp(&b.path)),
        o => return Err(format!("map: order {o:?}")),
    }
    Ok(rows)
}

fn int_keyed(v: Option<&Json>) -> Result<HashMap<i64, Json>, String> {
    let mut out = HashMap::new();
    if let Some(o) = v.and_then(Json::as_object) {
        for (k, v) in o {
            out.insert(k.trim().parse::<i64>().map_err(|_| format!("not a key number: {k:?}"))?, v.clone());
        }
    }
    Ok(out)
}

/// The items of the source table `src` under its data directory `root`.
pub fn items(src: &Json, root: &str) -> Result<Report, String> {
    let m = need(src, "map")?;
    let relabel = int_keyed(get(src, "relabel"))?;
    let skip_files: HashSet<String> = get(src, "skip_files").and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let mut rep = Report::default();
    let rows = match as_str(need(m, "kind")?, "map: kind")? {
        "sfz" => {
            let mut rows = sfz_rows(root, m, &skip_files, &mut rep)?;
            for r in &mut rows {
                if let Some(v) = relabel.get(&r.note) {
                    r.item.insert("note".into(), v.clone());
                    for s in &mut r.spares {
                        s.item.insert("note".into(), v.clone());
                    }
                }
            }
            rows
        }
        "pattern" => pattern_rows(root, m, &relabel, &skip_files, &mut rep)?,
        k => return Err(format!("source {}: unknown map kind {k:?}", py_str(src.get("id").unwrap_or(&Json::Null)))),
    };
    let skip: HashSet<i64> = get(src, "skip").and_then(Json::as_array)
        .map(|a| a.iter().filter_map(Json::as_i64).collect()).unwrap_or_default();
    let cut = int_keyed(get(src, "cut_s"))?;
    let gain = get(src, "gain_db").and_then(Json::as_object);
    let extra = get(src, "extra").and_then(Json::as_object);
    let octave = src.get("octave");
    // the source's own fields on an item (its key's tail cut, its dynamic's gain, extra, octave)
    let finish = |mut item: Item| -> Result<Item, String> {
        let note = item["note"].as_i64().ok_or("an item's note is not an integer")?;
        let tail = cut.get(&note).cloned().or_else(|| get(src, "tail_cut_s").cloned()).unwrap_or(json!(0.0));
        item.insert("tail_cut_s".into(), tail);
        if let (Some(gain), Some(d)) = (gain, item["dynamic"].as_str().map(String::from)) {
            if let Some(g) = gain.get(&d) {
                item.insert("gain_db".into(), g.clone());
            }
        }
        if let Some(extra) = extra {
            for (k, v) in extra {
                item.insert(k.clone(), v.clone());
            }
        }
        if let Some(o) = octave {
            let key = item["note"].clone();
            item.insert("key".into(), key);
            item.insert("note".into(), json!(note + py_int(o)?));
        }
        Ok(item)
    };
    for Row { item, spares, .. } in rows {
        let note = item["note"].as_i64().ok_or("an item's note is not an integer")?;
        if skip.contains(&note) {
            let rel = item["relpath"].as_str().unwrap_or_default().to_string();
            rep.dropped.push((rel, format!("key {note} in the source's skip")));
            continue;
        }
        let item = finish(item)?;
        if !spares.is_empty() {
            let rel = item["relpath"].as_str().unwrap_or_default().to_string();
            rep.spares.insert(rel, spares.into_iter().map(|r| finish(r.item)).collect::<Result<_, _>>()?);
        }
        rep.items.push(item);
    }
    if !skip_files.is_empty() {
        // the left-out takes as they map without skip_files, and which spare stands in for which
        let mut whole = src.clone();
        whole.as_object_mut().map(|o| o.remove("skip_files"));
        let full = items(&whole, root)?;
        let rel = |i: &Item| i["relpath"].as_str().unwrap_or_default().to_string();
        let mut names: Vec<&String> = skip_files.iter().collect();
        names.sort();
        for name in names {
            match full.items.iter().chain(full.spares.values().flatten()).find(|i| &rel(i) == name) {
                Some(i) => rep.left_out.push(i.clone()),
                None => rep.left_out_missing.push(name.clone()),
            }
        }
        // a read take stands in for a left-out one only where it is one of that take's spares (a
        // layer left out whole moves the layers above it down a rank: no spare in use)
        for it in &rep.items {
            for (was, spares) in &full.spares {
                if skip_files.contains(was) && spares.iter().any(|s| rel(s) == rel(it)) {
                    rep.replaced.insert(rel(it), was.clone());
                }
            }
        }
    }
    Ok(rep)
}

/// the source's dynamics (pp, mf, ff → a layer's key: its dynamic, or its layer number with
/// `layer_key = "layer"`) that no item has: the fits look them up among the items
pub fn dynamics_problems(src: &Json, items: &[Item]) -> Vec<String> {
    let Some(d) = get(src, "dynamics").and_then(Json::as_object) else { return Vec::new() };
    let key = if get(src, "layer_key").and_then(Json::as_str) == Some("layer") { "layer" } else { "dynamic" };
    let have: Vec<&Json> = items.iter().filter_map(|i| i.get(key)).collect();
    let stale: Vec<String> = d.iter().filter(|(_, v)| !have.contains(v)).map(|(k, v)| format!("{k} = {v}")).collect();
    if stale.is_empty() {
        return Vec::new();
    }
    let mut names: Vec<String> = have.iter().map(|v| v.to_string()).collect();
    names.sort();
    names.dedup();
    vec![format!("the source {}'s dynamics {} name no layer of its items (they have {})",
        py_str(src.get("id").unwrap_or(&Json::Null)), stale.join(", "), names.join(", "))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn touch(root: &Path, rels: &[&str]) {
        for rel in rels {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
    }

    fn rows(r: &Report, keys: &[&str]) -> Vec<Vec<Json>> {
        r.items.iter().map(|i| keys.iter().map(|k| i.get(*k).cloned().unwrap_or(Json::Null)).collect()).collect()
    }

    #[test]
    fn pattern_matches_lists_every_value_and_the_rejects() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["h/KSHarp_C4_mf1.wav", "h/KSHarp_C4_p1.wav", "h/notes.txt.wav"]);
        let root = d.path().to_string_lossy().to_string();
        let m = json!({"kind": "pattern", "glob": "h/*.wav", "regex": r"KSHarp_(?P<note>[A-G]#?\d)_(?P<dynamic>[a-z]+)(?P<take>\d+)\.wav",
            "note": {"group": "note"}, "layers": ["mf"]});
        let (got, rejected) = pattern_matches(&root, &m).unwrap();
        let dyns: Vec<Option<String>> = got.iter().map(|x| x.groups["dynamic"].clone()).collect();
        assert_eq!(dyns, [Some("mf".to_string()), Some("p".to_string())], "the layers are not applied here");
        assert_eq!(got[0].rel, "h/KSHarp_C4_mf1.wav");
        assert!(got[0].path.ends_with("/h/KSHarp_C4_mf1.wav"));
        assert_eq!(rejected, ["h/notes.txt.wav"]);
    }

    #[test]
    fn dynamic_names_follow_the_map() {
        assert_eq!(dynamic_name(&json!({"dynamic_names": {"vl1": "pp"}}), "vl1").unwrap(), json!("pp"));
        assert_eq!(dynamic_name(&json!({"dynamic_format": "v{}"}), "2").unwrap(), json!("v2"));
        assert_eq!(dynamic_name(&json!({}), "mf").unwrap(), json!("mf"));
    }

    #[test]
    fn pattern_with_takes_layers_and_order() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["h/KSHarp_C4_mf2.wav", "h/KSHarp_C4_mf1.wav", "h/KSHarp_C4_f1.wav", "h/KSHarp_D4_p1.wav",
                          "h/KSHarp_C#3_f3.wav", "h/notes.txt", "h/.KSHarp_C4_f1.wav"]);
        let src = json!({"id": "s", "map": {"kind": "pattern", "glob": "h/KSHarp_*.wav",
            "regex": r"KSHarp_(?P<note>[A-G]#?\d)_(?P<dynamic>[a-z]+)(?P<take>\d+)\.wav",
            "note": {"group": "note"}, "layers": ["mf", "f"], "prefer": ["take"], "order": "note"}});
        let root = d.path().to_str().unwrap();
        let r = items(&src, root).unwrap();
        assert_eq!(rows(&r, &["note", "dynamic", "layer", "layers", "relpath"]), vec![
            vec![json!(49), json!("f"), json!(2), json!(2), json!("h/KSHarp_C#3_f3.wav")],
            vec![json!(60), json!("mf"), json!(1), json!(2), json!("h/KSHarp_C4_mf1.wav")],
            vec![json!(60), json!("f"), json!(2), json!(2), json!("h/KSHarp_C4_f1.wav")]]);
        for i in &r.items {
            assert_eq!(i["tail_cut_s"], json!(0.0));
            assert_eq!(i["path"], json!(format!("{root}/{}", i["relpath"].as_str().unwrap())));
            assert_eq!(i.keys().collect::<Vec<_>>(), ["path", "relpath", "note", "layer", "layers", "dynamic", "tail_cut_s"]);
        }
        assert!(r.dropped.iter().any(|(p, why)| p == "h/KSHarp_D4_p1.wav" && why.contains("layers")));
        assert!(r.dropped.iter().any(|(p, _)| p == "h/KSHarp_C4_mf2.wav"));
    }

    #[test]
    fn pattern_relpath_midi_fields_and_layer_order() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["notes/guitar.s2/mf/64.wav", "notes/guitar.s1/mf/64.wav", "notes/guitar.s2/pp/59.wav",
                          "notes/guitar.s1/pp/100.wav"]);
        let src = json!({"id": "g", "map": {"kind": "pattern", "glob": "notes/guitar.s*/*/*.wav", "match": "relpath",
            "regex": r"notes/guitar\.s(?P<string>\d)/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
            "note": {"group": "midi", "style": "midi"}, "layers": ["pp", "mf"],
            "fields": {"string": "int"}, "order": "layer"}});
        let r = items(&src, d.path().to_str().unwrap()).unwrap();
        assert_eq!(rows(&r, &["note", "dynamic", "string"]), vec![
            vec![json!(59), json!("pp"), json!(2)], vec![json!(100), json!("pp"), json!(1)],
            vec![json!(64), json!("mf"), json!(1)], vec![json!(64), json!("mf"), json!(2)]]);
    }

    #[test]
    fn pattern_index_names_format_and_one_layer() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["k/1.wav", "k/2.wav", "v/x_C4_3.wav"]);
        let src = json!({"id": "k", "map": {"kind": "pattern", "glob": "k/*.wav", "regex": r"(?P<index>\d+)\.wav",
            "note": {"group": "index", "style": "index", "offset": 20}, "dynamic": "mf", "order": "path"}});
        let r = items(&src, d.path().to_str().unwrap()).unwrap();
        assert_eq!(rows(&r, &["note", "dynamic", "layer", "layers"]), vec![
            vec![json!(21), json!("mf"), json!(1), json!(1)], vec![json!(22), json!("mf"), json!(1), json!(1)]]);
        let src = json!({"id": "v", "map": {"kind": "pattern", "glob": "v/*.wav",
            "regex": r"x_(?P<note>[A-G]\d)_(?P<dynamic>\d)\.wav", "note": {"group": "note", "shift": 12},
            "layers": ["3"], "dynamic_format": "v{}", "dynamic_names": {}, "order": "note"}});
        let r = items(&src, d.path().to_str().unwrap()).unwrap();
        assert_eq!(rows(&r, &["note", "dynamic"]), vec![vec![json!(72), json!("v3")]]);
        let src = json!({"id": "v", "map": {"kind": "pattern", "glob": "v/*.wav",
            "regex": r"x_(?P<note>[A-G]\d)_(?P<dynamic>\d)\.wav", "note": {"group": "note"}, "layers": []}});
        assert!(items(&src, d.path().to_str().unwrap()).unwrap().items.is_empty(), "an empty layers list keeps nothing");
    }

    #[test]
    fn the_sources_own_fields_apply_after_the_map() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["a/C4_f.wav", "a/D4_f.wav", "a/E4_p.wav"]);
        let src = json!({"id": "a", "map": {"kind": "pattern", "glob": "a/*.wav",
            "regex": r"(?P<note>[A-G]\d)_(?P<dynamic>[pf])\.wav", "note": {"group": "note"}, "layers": ["p", "f"]},
            "skip": [62], "relabel": {"64": 65}, "cut_s": {"60": 1.7}, "tail_cut_s": 1, "gain_db": {"p": 4.4},
            "extra": {"string": 3, "key": 0}, "octave": 12});
        let r = items(&src, d.path().to_str().unwrap()).unwrap();
        assert_eq!(r.items.len(), 2);
        let c4 = &r.items[0];
        assert_eq!(c4["note"], json!(72));
        assert_eq!(c4["key"], json!(60));
        assert_eq!(c4["tail_cut_s"], json!(1.7));
        assert_eq!(c4.keys().collect::<Vec<_>>(),
                   ["path", "relpath", "note", "layer", "layers", "dynamic", "tail_cut_s", "string", "key"]);
        let e4 = &r.items[1];
        assert_eq!(e4["note"], json!(77), "relabeled 64 → 65, then the octave");
        assert_eq!(e4["tail_cut_s"], json!(1), "an integer stays an integer");
        assert_eq!(e4["gain_db"], json!(4.4));
        assert!(r.dropped.iter().any(|(p, why)| p == "a/D4_f.wav" && why.contains("skip")));
    }

    #[test]
    fn sfz_pitch_keycenter_defaults_to_60() {
        // the SFZ default: a file that leaves it out where it is 60 (middle C's regions)
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("set.sfz"), "<region> sample=b.wav lokey=60 hikey=60\n<region> sample=a.wav pitch_keycenter=59\n").unwrap();
        let r = items(&json!({"id": "s", "map": {"kind": "sfz", "files": ["set.sfz"]}}), &d.path().to_string_lossy()).unwrap();
        assert_eq!(rows(&r, &["note", "relpath"]), vec![vec![json!(59), json!("a.wav")], vec![json!(60), json!("b.wav")]]);
    }

    #[test]
    fn sfz_ranked_layers_filters_folders_and_fields() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("x.sfz"), "<group> trigger=attack\n\
            <region> sample=Sustains\\C4_v2.wav pitch_keycenter=60 lovel=64 hivel=127 volume=-2\n\
            <region> sample=Sustains\\C4_v1.wav pitch_keycenter=60 lovel=1 hivel=63 tune=5\n\
            <region> sample=Other\\C4_v1.wav pitch_keycenter=60 lovel=1 hivel=63\n\
            <region> sample=Sustains\\C4_v1.wav pitch_keycenter=60 lovel=1 hivel=63 seq_position=2\n\
            <group> trigger=release\n<region> sample=Sustains\\C4_rel.wav pitch_keycenter=60\n").unwrap();
        let src = json!({"id": "x", "map": {"kind": "sfz", "files": ["x.sfz"],
            "filter": [{"opcode": "trigger", "default": "attack", "in": ["attack"]},
                       {"opcode": "seq_position", "default": "1", "in": ["1"]}],
            "folders": ["Sustains"], "prefix": "s/"}});
        let root = d.path().to_str().unwrap();
        let r = items(&src, root).unwrap();
        assert_eq!(rows(&r, &["relpath", "note", "layer", "layers", "dynamic", "velocity", "sfz_volume_db", "sfz_tune_cents"]), vec![
            vec![json!("s/Sustains/C4_v1.wav"), json!(60), json!(1), json!(2), json!("1/2"), json!([1, 63]), json!(0.0), json!(5.0)],
            vec![json!("s/Sustains/C4_v2.wav"), json!(60), json!(2), json!(2), json!("2/2"), json!([64, 127]), json!(-2.0), json!(0.0)]]);
        assert!(r.dropped.iter().any(|(s, why)| s == "Sustains/C4_rel.wav" && why.contains("trigger")));
        assert!(r.dropped.iter().any(|(s, why)| s == "Other/C4_v1.wav" && why.contains("folder")));
    }

    #[test]
    fn sfz_layers_by_file_dynamic_from_and_unique() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("p.sfz"), "<region> sample=a/n_vl1_x.wav lokey=40\n<region> sample=a/n_vl1_x.wav lokey=40\n").unwrap();
        std::fs::write(d.path().join("f.sfz"), "<region> sample=b/../a/m_vl2_x.wav pitch_keycenter=41\n").unwrap();
        let src = json!({"id": "p", "map": {"kind": "sfz", "files": ["p.sfz", "f.sfz"], "layers": "file",
            "dynamic_format": "vl{layer}", "unique": "sample", "normpath": true}});
        let r = items(&src, d.path().to_str().unwrap()).unwrap();
        assert_eq!(rows(&r, &["relpath", "note", "layer", "layers", "dynamic"]), vec![
            vec![json!("a/n_vl1_x.wav"), json!(40), json!(1), json!(2), json!("vl1")],
            vec![json!("a/m_vl2_x.wav"), json!(41), json!(2), json!(2), json!("vl2")]]);
        assert!(!r.items[0].contains_key("velocity"), "layers by file carry no SFZ fields");
        let src = json!({"id": "q", "map": {"kind": "sfz", "files": ["f.sfz"], "normpath": true,
            "dynamic_from": {"regex": r"_vl?(\d+)_", "format": "v{}"}, "sfz_fields": false}});
        let r = items(&src, d.path().to_str().unwrap()).unwrap();
        assert_eq!(rows(&r, &["dynamic"]), vec![vec![json!("v2")]]);
        assert!(!r.items[0].contains_key("velocity"));
    }

    #[test]
    fn pattern_root_with_brackets_and_sharps() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Set [2], A's");
        touch(&root, &["Harp, Lever/C#4 f.wav", "Harp, Lever/C4 f.wav"]);
        let src = json!({"id": "b", "map": {"kind": "pattern", "glob": "Harp, Lever/*.wav",
            "regex": r"(?P<note>[A-G]#?\d) (?P<dynamic>f)\.wav", "note": {"group": "note"}, "layers": ["f"]}});
        let r = items(&src, root.to_str().unwrap()).unwrap();
        assert_eq!(rows(&r, &["note", "relpath"]), vec![
            vec![json!(60), json!("Harp, Lever/C4 f.wav")], vec![json!(61), json!("Harp, Lever/C#4 f.wav")]]);
    }

    #[test]
    fn dynamics_must_name_layers_the_items_have() {
        let item = |d: &str, l: i64| json!({"note": 60, "dynamic": d, "layer": l}).as_object().unwrap().clone();
        let items = vec![item("mf", 1), item("f", 2)];
        let ok = json!({"dynamics": {"mf": "mf", "ff": "f"}});
        assert!(dynamics_problems(&ok, &items).is_empty());
        let stale = json!({"dynamics": {"pp": "p", "mf": "mp", "ff": "f"}});
        let p = dynamics_problems(&stale, &items);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("pp = \"p\"") && p[0].contains("mf = \"mp\""), "{p:?}");
        let by_layer = json!({"layer_key": "layer", "dynamics": {"pp": 1, "ff": 2}});
        assert!(dynamics_problems(&by_layer, &items).is_empty());
        assert_eq!(dynamics_problems(&json!({"layer_key": "layer", "dynamics": {"ff": 16}}), &items).len(), 1);
    }

    #[test]
    fn errors_where_python_raises() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), &["a/C4.wav"]);
        let root = d.path().to_str().unwrap();
        let bad_kind = json!({"id": "a", "map": {"kind": "zip"}});
        assert!(items(&bad_kind, root).is_err());
        let bad_regex = json!({"id": "a", "map": {"kind": "pattern", "glob": "a/*.wav", "regex": "(", "note": {}, "dynamic": "mf"}});
        assert!(items(&bad_regex, root).is_err());
        let no_dynamic = json!({"id": "a", "map": {"kind": "pattern", "glob": "a/*.wav",
            "regex": r"(?P<note>[A-G]\d)\.wav", "note": {"group": "note"}}});
        assert!(items(&no_dynamic, root).is_err(), "no layers and no dynamic: Python's KeyError");
        let missing_sfz = json!({"id": "a", "map": {"kind": "sfz", "files": ["none.sfz"]}});
        assert!(items(&missing_sfz, root).is_err());
    }
}
