//! The spec's acceptance: Harp A's set made into a new project from the harp template and the
//! suggestion; its items equal Harp A's; its chain builds. Run by hand (minutes):
//! cargo test --release --manifest-path editor/Cargo.toml --test acceptance -- --ignored --nocapture
use pe_editor::driver::{self, Event, Mode};
use pe_editor::project::{Project, Repo};
use pe_editor::rule::{self, Dyn};
use pe_editor::wizard::{self, Wizard};
use pe_editor::{audio, mapping, suggest};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

const NAME: &str = "zz_acceptance";

fn by_key(items: &[mapping::Item]) -> BTreeMap<i64, Vec<String>> {
    let mut m: BTreeMap<i64, Vec<(i64, String)>> = BTreeMap::new();
    for i in items {
        m.entry(i["note"].as_i64().unwrap()).or_default().push((i["layer"].as_i64().unwrap(), i["path"].as_str().unwrap().into()));
    }
    m.into_iter().map(|(k, mut v)| { v.sort(); (k, v.into_iter().map(|x| x.1).collect()) }).collect()
}

#[test]
#[ignore = "builds a whole chain (minutes); run by hand"]
fn a_new_instrument_from_harp_a_set() {
    let repo = Repo::from_env();
    let _ = std::fs::remove_dir_all(repo.projects.join(NAME));
    let _ = std::fs::remove_dir_all(repo.root.join("instruments").join(NAME));
    let harp = Project::load(&repo, "harp").unwrap();
    let theirs = harp.source_json("harp").unwrap();
    let (folder, sfz) = suggest::folder_of(&theirs).unwrap();
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/acceptance-audio.json");
    let files = suggest::files_to_measure(&repo.data, &folder, sfz.as_deref());
    let m = audio::measure_all(&files, &cache, &AtomicUsize::new(0));
    let s = suggest::suggest(&repo.data, &folder, sfz.as_deref(), &m).unwrap();
    for f in &s.findings {
        eprintln!("finding: {} ({} files)", f.text, f.files.len());
    }
    let root = suggest::data_root(&repo.data, &folder.root);
    let found: Vec<String> = rule::layer_stats(&s.source, &root, &m).into_iter().map(|x| x.value).collect();
    let mut w = Wizard::new_instrument();
    // the part's main source takes the part's name (the string fits take one id for both)
    (w.name, w.id) = (NAME.to_string(), NAME.to_string());
    (w.folder, w.sfz) = (Some(folder.clone()), sfz.clone());
    w.apply_suggestion(s, &found);
    w.choose(&repo, "harp", "harp", "the closest key range");
    // the person's choice on the dynamics page: Harp A keeps its mf and f layers, as mf and ff
    let r = w.rule.as_mut().unwrap();
    for i in 0..r.layers.len() {
        let keep = matches!(r.layers[i].raw.as_deref(), Some("mf" | "f"));
        r.set_keep(i, keep);
    }
    let at = |r: &rule::Rule, v: &str| r.layers.iter().position(|l| l.raw.as_deref() == Some(v)).unwrap();
    let (mf, f) = (at(r, "mf"), at(r, "f"));
    r.use_as(mf, Some(Dyn::Mf));
    r.use_as(f, Some(Dyn::Ff));
    let p = wizard::finish(&repo, &w, &cache).unwrap();
    let ours = p.source_json(NAME).unwrap();
    let want = mapping::items(&theirs, &root).unwrap().items;
    let got = mapping::items(&ours, &root).unwrap().items;
    assert_eq!(by_key(&got), by_key(&want), "the new project's items differ from Harp A's");
    assert!(p.build_problems().is_empty(), "{:?}", p.build_problems());
    assert!(mapping::dynamics_problems(&ours, &got).is_empty(), "{:?}", mapping::dynamics_problems(&ours, &got));
    let b = driver::Build::start(&repo, NAME, &Mode::Stale).unwrap();
    let mut last = None;
    while let Ok(e) = b.events.recv_timeout(Duration::from_secs(1800)) {
        eprintln!("{e:?}");
        last = Some(e);
    }
    assert_eq!(last, Some(Event::Finished { ok: true, cancelled: false }));
    assert!(repo.root.join("instruments").join(NAME).join("instrument.json").is_file());
    assert!(repo.root.join("instruments").join(NAME).join("data.bin").is_file());
    // passed: gone again (a failed run leaves both to look at)
    std::fs::remove_dir_all(repo.projects.join(NAME)).unwrap();
    std::fs::remove_dir_all(repo.root.join("instruments").join(NAME)).unwrap();
}
