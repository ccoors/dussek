//! Every committed source with data read into a rule and applied back: the source comes back
//! unchanged (JSON-equal), also after a sync of its layers; the parts' probe reads every file the
//! map's regex matches. The sources the pages show (as parts or as an SFZ, not custom) have a floor.
use pe_editor::audio::Measures;
use pe_editor::rule::{self, Reading, Rule};
use pe_editor::{mapping, project, suggest};

/// the sources shown as parts or as an SFZ on 2026-10-14 (design log step 80); the test fails below it
const RECORDED_SHOWN: usize = 41;

#[test]
fn every_committed_source_reads_and_applies_back_unchanged() {
    let repo = project::Repo::from_env();
    let (mut shown, mut custom, mut checked) = (0, Vec::new(), 0);
    for (_, src) in project::all_sources(&repo) {
        let id = src["id"].as_str().unwrap().to_string();
        let r = src["root"].as_str().unwrap();
        if !repo.data.join(r).is_dir() {
            continue;
        }
        let root = suggest::data_root(&repo.data, r);
        let found: Vec<String> = rule::layer_stats(&src, &root, &Measures::default()).into_iter().map(|s| s.value).collect();
        let mut rl = Rule::read(&src, &found);
        assert_eq!(rl.apply(&src), src, "{id}: read and applied back");
        rl.sync_layers(&src["map"], &found);
        assert_eq!(rl.apply(&src), src, "{id}: after a sync of the layers");
        match &rl.reading {
            Reading::Names(n) => {
                let p = rule::preview(n, &root).unwrap();
                let (matches, _) = mapping::pattern_matches(&root, &src["map"]).unwrap();
                assert_eq!(p.rows.len(), matches.len(), "{id}: the probe reads every file the regex matches");
                shown += 1;
            }
            Reading::Sfz(_) => shown += 1,
            Reading::Custom => custom.push(id.clone()),
        }
        checked += 1;
    }
    eprintln!("{checked} sources: {shown} shown as parts or SFZ; custom: {custom:?}");
    if repo.data.join("vcsl").is_dir() {
        assert!(shown >= RECORDED_SHOWN, "{shown} shown, fewer than the recorded {RECORDED_SHOWN}");
    }
}
