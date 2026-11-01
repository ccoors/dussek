//! The guess on every donor's own recordings (the donor itself left out of its answer): a string
//! set never pre-selects an answer; a bars set pre-selects its own answer, none, or (counted) a
//! fellow bar answer: left out, the kalimba's answer has no donor and the hand chimes' only fellow
//! is the tubular bells. The measurements share the suggestion corpus's cache (target/corpus-cache).
use pe_editor::project::{self, Kind, Project, Repo};
use pe_editor::{audio, kind, mapping, suggest};
use std::sync::atomic::AtomicUsize;

/// the bars sets whose own answer is pre-selected, 2026-10-14: 5 (marimba, vibraphone, bells,
/// crotales, balafon; the xylophone's bar share 0.66 is just under two thirds, the tubular bells
/// give no key: a tube's pitch is no partial of it); 2026-10-29: 7 (the mbiras made the tines an
/// answer with donors besides the kalimba: the kalimba and Mbira A read as tines); the test fails
/// below it
const RECORDED_HITS: usize = 7;
/// bars sets pre-selecting a fellow bar answer, 2026-10-14: 2 (the kalimba, the hand chimes);
/// 2026-10-29: 3 (the hand chimes; Mbira B and the nyunga nyunga read as the mallets'); 2026-11-01: 4
/// (the tongue drum reads as the tubes': its tongues nearly pure, as a tube's strike note)
const RECORDED_FELLOWS: usize = 4;

#[test]
fn the_guess_on_the_donors_own_sets() {
    let repo = Repo::from_env();
    let kinds = project::kinds(&repo);
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/corpus-cache");
    let (mut hits, mut rows, mut wrong, mut fellows, mut strings) = (0, Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut absent = Vec::new();
    for k in &kinds {
        for d in &k.donors {
            let p = Project::load(&repo, d).unwrap();
            let Some(src) = p.source_ids().first().and_then(|s| p.source_json(s)) else { continue };
            let root = src["root"].as_str().unwrap_or_default();
            if !repo.data.join(root).is_dir() {
                absent.push(format!("{d} (data/{root})"));
                continue;
            }
            let items = mapping::items(&src, &suggest::data_root(&repo.data, root)).map(|r| r.items).unwrap_or_default();
            let files: Vec<std::path::PathBuf> = items.iter().filter_map(|i| i["path"].as_str().map(Into::into)).collect();
            let id = src["id"].as_str().unwrap();
            let m = audio::measure_all(&files, &cache.join(format!("{id}.json")), &AtomicUsize::new(0));
            let keys = kind::sample_keys(&items, &m, src["dynamics"].get("mf"));
            // the donor left out of its own answer
            let others: Vec<Kind> = kinds.iter().map(|x| Kind { donors: x.donors.iter().filter(|y| *y != d).cloned().collect(), ..x.clone() })
                .filter(|x| !x.donors.is_empty())
                .map(|x| Kind { fallback: if x.donors.contains(&x.fallback) { x.fallback.clone() } else { x.donors[0].clone() }, ..x })
                .collect();
            let g = kind::preselect(&repo, &others, &keys);
            let near = others.iter().find(|x| x.id == k.id).map(|x| kind::nearest_donor(&repo, x, &keys).0).unwrap_or_default();
            rows.push(format!("{d} ({}): {} keys, bar share {:.2}, pre-selected {:?} ({}), nearest donor {near}",
                k.id, keys.len(), kind::bar_share(&keys), g.kind, g.why));
            if !k.bar_like {
                strings.push((d.clone(), g.why.clone()));
            }
            match (&g.kind, k.bar_like) {
                (Some(x), _) if *x == k.id => hits += 1,
                (Some(x), true) => fellows.push(format!("{d}: {x} pre-selected, its answer is {}", k.id)),
                (Some(x), false) => wrong.push(format!("{d}: {x} pre-selected, its answer is {}", k.id)),
                (None, _) => {}
            }
        }
    }
    eprintln!("{}", rows.join("\n"));
    assert!(wrong.is_empty(), "a string set pre-selected an answer:\n{}", wrong.join("\n"));
    // a string set reads as a string's (a piano's treble once read "not clearly a bar's")
    let unclear: Vec<String> = strings.iter().filter(|(_, why)| !why.contains("a string's")).map(|(d, why)| format!("{d}: {why}")).collect();
    assert!(unclear.is_empty(), "string sets not read as a string's:\n{}", unclear.join("\n"));
    eprintln!("fellow answers: {}", fellows.join("; "));
    assert!(fellows.len() <= RECORDED_FELLOWS, "{} fellow answers, {RECORDED_FELLOWS} recorded:\n{}", fellows.len(), fellows.join("\n"));
    if !absent.is_empty() {
        // the recorded hits are the whole corpus's (a fresh clone has none of its sets)
        eprintln!("the hits not checked: {} sets not in data/: {}", absent.len(), absent.join(", "));
        return;
    }
    assert!(hits >= RECORDED_HITS, "{hits} hits, {RECORDED_HITS} recorded");
}

#[test]
fn the_bars_projects_made_from_a_donor_keep_its_body() {
    // a bars project no answer lists (one the wizard made): the answer it would be pre-selected with
    // gives it a donor of its own body
    let repo = Repo::from_env();
    let kinds = project::kinds(&repo);
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/corpus-cache");
    let mut names: Vec<String> = std::fs::read_dir(&repo.projects).unwrap().flatten()
        .filter(|e| e.path().join("project.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let mut checked = Vec::new();
    for name in names.iter().filter(|n| !kinds.iter().any(|k| k.donors.contains(n))) {
        if project::family(&repo, name).as_deref() != Some("bars") {
            continue;
        }
        let p = Project::load(&repo, name).unwrap();
        let Some(src) = p.source_ids().first().and_then(|s| p.source_json(s)) else { continue };
        let root = src["root"].as_str().unwrap_or_default();
        if !repo.data.join(root).is_dir() {
            continue;
        }
        let items = mapping::items(&src, &suggest::data_root(&repo.data, root)).map(|r| r.items).unwrap_or_default();
        let files: Vec<std::path::PathBuf> = items.iter().filter_map(|i| i["path"].as_str().map(Into::into)).collect();
        let m = audio::measure_all(&files, &cache.join(format!("{}.json", src["id"].as_str().unwrap())), &AtomicUsize::new(0));
        let keys = kind::sample_keys(&items, &m, src["dynamics"].get("mf"));
        let g = kind::preselect(&repo, &kinds, &keys);
        let answer = kinds.iter().find(|k| Some(&k.id) == g.kind.as_ref()).unwrap_or_else(|| panic!("{name}: {}", g.why));
        let (donor, _) = kind::nearest_donor(&repo, answer, &keys);
        let body = |n: &str| Project::load(&repo, n).unwrap().get(&project::Table::top("project"), "body");
        let scores: Vec<String> = answer.donors.iter().map(|d| format!("{d} {:.2}", kind::donor_score(&repo, d, &keys))).collect();
        eprintln!("{name}: {} keys, {}: {donor} ({})", keys.len(), answer.id, scores.join(", "));
        assert_eq!(body(&donor), body(name), "{name}: set up like {donor}");
        checked.push(name.clone());
    }
    eprintln!("checked: {}", checked.join(", "));
}
