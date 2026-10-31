//! The suggestion on every committed source whose data is present, against the committed map.
//! The hand-made choices no suggestion infers are taken from the committed source (skip,
//! relabel, cut_s, tail_cut_s, extra, octave, its subset of layers, and for SFZ sets folders and
//! prefer_substring). A hit: the same files on the same keys, in the same layer order per key.
use pe_editor::{audio, mapping, project, pyfmt, suggest};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::AtomicUsize;

/// the hits on 2026-10-14: 35 of 41 sources (design log steps 79, 93: the guitar and Glockenspiel B
/// since a harmonic series is no bar's); the test fails below it
const RECORDED_HITS: usize = 35;

fn by_key(items: &[mapping::Item]) -> BTreeMap<i64, Vec<String>> {
    let mut m: BTreeMap<i64, Vec<(i64, String)>> = BTreeMap::new();
    for i in items {
        m.entry(i["note"].as_i64().unwrap()).or_default()
            .push((i["layer"].as_i64().unwrap(), i["path"].as_str().unwrap().to_string()));
    }
    m.into_iter().map(|(k, mut v)| { v.sort(); (k, v.into_iter().map(|x| x.1).collect()) }).collect()
}

#[test]
fn the_suggestion_against_the_committed_maps() {
    let repo = project::Repo::from_env();
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/corpus-cache");
    let (mut hits, mut misses) = (Vec::new(), Vec::new());
    let mut absent = Vec::new();
    for (_, mut src) in project::all_sources(&repo) {
        // the takes left out by ear are no part of what a suggestion can know
        src.as_object_mut().map(|o| o.remove("skip_files"));
        let id = src["id"].as_str().unwrap().to_string();
        let root = src["root"].as_str().unwrap();
        if !repo.data.join(root).is_dir() {
            absent.push(format!("{id} (data/{root})"));
            continue;
        }
        let data_root = suggest::data_root(&repo.data, root);
        let committed = mapping::items(&src, &data_root).unwrap().items;
        if committed.is_empty() {
            continue;
        }
        let (folder, sfz) = suggest::folder_of(&src).unwrap();
        let files = suggest::files_to_measure(&repo.data, &folder, sfz.as_deref());
        let m = audio::measure_all(&files, &cache.join(format!("{id}.json")), &AtomicUsize::new(0));
        let sug = match suggest::suggest(&repo.data, &folder, sfz.as_deref(), &m) {
            Ok(s) => s,
            Err(e) => {
                misses.push((id, format!("error: {e}")));
                continue;
            }
        };
        let mut p = sug.source.clone();
        for k in ["skip", "relabel", "cut_s", "tail_cut_s", "extra", "octave"] {
            if let Some(v) = src.get(k) {
                p[k] = v.clone();
            }
        }
        if sfz.is_some() {
            for k in ["folders", "prefer_substring"] {
                if let Some(v) = src["map"].get(k) {
                    p["map"][k] = v.clone();
                }
            }
        }
        if let Some(layers) = p["map"]["layers"].as_array().cloned() {
            let wanted: HashSet<String> = committed.iter().filter_map(|i| i["dynamic"].as_str().map(String::from)).collect();
            let fmt = p["map"]["dynamic_format"].as_str().unwrap_or("{}").to_string();
            // the committed subset by its dynamics' names, or by the map's own layer values
            let raw: HashSet<String> = src["map"]["layers"].as_array().into_iter().flatten()
                .filter_map(|v| v.as_str().map(String::from)).collect();
            let kept: Vec<Value> = layers.into_iter()
                .filter(|l| raw.contains(l.as_str().unwrap_or_default())
                    || pyfmt::format(&fmt, l.as_str(), None).is_ok_and(|d| wanted.contains(&d)))
                .collect();
            if !kept.is_empty() {
                p["map"]["layers"] = Value::Array(kept);
            }
        }
        let got = mapping::items(&p, &data_root).map(|r| r.items).unwrap_or_default();
        let (want, have) = (by_key(&committed), by_key(&got));
        if want == have {
            hits.push(id);
        } else {
            let wf: HashSet<&String> = want.values().flatten().collect();
            let hf: HashSet<&String> = have.values().flatten().collect();
            misses.push((id, format!("{} files committed, {} proposed: {} missing, {} extra, keys {} vs {}; {}",
                wf.len(), hf.len(), wf.difference(&hf).count(), hf.difference(&wf).count(), want.len(), have.len(),
                sug.findings.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join(" | "))));
        }
    }
    eprintln!("hits ({}): {}", hits.len(), hits.join(", "));
    for (id, why) in &misses {
        eprintln!("miss {id}: {why}");
    }
    if !absent.is_empty() {
        // the recorded hits are the whole corpus's (a fresh clone has none of its sets)
        eprintln!("the hits not checked: {} sets not in data/: {}", absent.len(), absent.join(", "));
        return;
    }
    assert!(hits.len() >= RECORDED_HITS, "{} hits, {} recorded", hits.len(), RECORDED_HITS);
}
