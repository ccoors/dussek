//! The wizard's Finish on a fixture folder: the project from a template, one source per group
//! value, the menu, the measurements' cache; an id already used leaves no project behind.
use pe_editor::audio::Measures;
use pe_editor::project::{self, Project, Repo, Table};
use pe_editor::rule::{self, Reading};
use pe_editor::wizard::{self, Step, Wizard};
use pe_editor::{mapping, suggest};
use serde_json::json;

/// a temporary repository: copies of the committed projects, a data folder with a two-group set
fn fixture() -> (tempfile::TempDir, Repo) {
    let real = Repo::from_env();
    let d = tempfile::tempdir().unwrap();
    for n in project::project_names(&real) {
        std::fs::create_dir_all(d.path().join("projects").join(&n)).unwrap();
        std::fs::copy(real.projects.join(&n).join("project.toml"), d.path().join("projects").join(&n).join("project.toml")).unwrap();
    }
    std::fs::copy(real.projects.join("steps.toml"), d.path().join("projects/steps.toml")).unwrap();
    std::fs::copy(real.projects.join("kinds.toml"), d.path().join("projects/kinds.toml")).unwrap();
    for m in ["cord", "yarn"] {
        for n in ["C4", "D4", "E4", "F4"] {
            let p = d.path().join(format!("data/zzset/Bar_{m}_{n}_rr1.wav"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
    }
    let repo = Repo { root: real.root.clone(), projects: d.path().join("projects"), data: d.path().join("data") };
    (d, repo)
}

fn bars_template(repo: &Repo) -> String {
    project::templates(repo).into_iter().find(|t| t.family == "bars").unwrap().name
}

fn wizard_for(repo: &Repo, name: &str) -> Wizard {
    let mut w = Wizard::new_instrument();
    w.name = name.into();
    w.id = name.into();
    let folder = suggest::Folder::from_data_path("zzset");
    let files = suggest::audio_files(&repo.data, &folder);
    let s = suggest::suggest_names(&repo.data, &folder, &files, &Measures::default());
    let root = suggest::data_root(&repo.data, &folder.root);
    let found: Vec<String> = rule::layer_stats(&s.source, &root, &Measures::default()).into_iter().map(|x| x.value).collect();
    w.folder = Some(folder);
    w.apply_suggestion(s, &found);
    // the answer after the files are read, as on the pages
    w.choose(repo, "mallet", &bars_template(repo), "");
    w.menu_name = "ZZ Bars".into();
    w.menu_group = "Percussion".into();
    w
}

#[test]
fn finish_writes_one_source_per_group_value() {
    let (d, repo) = fixture();
    let w = wizard_for(&repo, "zz_wiz");
    assert_eq!(w.rule.as_ref().unwrap().group_values(), Some(vec!["cord".to_string(), "yarn".to_string()]));
    let cache = d.path().join("wizard-audio.json");
    std::fs::write(&cache, r#"{"version": 2, "files": {}}"#).unwrap();
    let p = wizard::finish(&repo, &w, &cache).unwrap();
    assert_eq!(p.source_ids(), ["zz_wiz", "zz_wiz_yarn"]);
    let root = suggest::data_root(&repo.data, "zzset");
    for (id, value) in [("zz_wiz", "cord"), ("zz_wiz_yarn", "yarn")] {
        let src = p.source_json(id).unwrap();
        let items = mapping::items(&src, &root).unwrap().items;
        assert_eq!(items.len(), 4, "{id}");
        assert!(items.iter().all(|i| i["relpath"].as_str().unwrap().contains(value)), "{id} reads only its group");
        assert_eq!(src["role"], json!(w.role));
    }
    assert_eq!(p.get(&Table::top("menu"), "name").unwrap(), json!("ZZ Bars"));
    assert_eq!(p.get(&Table::top("menu"), "group").unwrap(), json!("Percussion"));
    assert_eq!(p.get(&Table::top("menu"), "mallets").unwrap(), json!(["Cord", "Yarn"]), "one name per mallet source");
    assert!(repo.projects.join("zz_wiz/cache/editor/audio.json").is_file(), "the measurements come along");
    assert!(!p.dirty(), "Finish saves");
    let again = Project::load(&repo, "zz_wiz").unwrap();
    assert_eq!(again.source_ids().len(), 2);
}

#[test]
fn finish_refuses_a_taken_id_and_leaves_nothing() {
    let (d, repo) = fixture();
    let mut w = wizard_for(&repo, "zz_wiz2");
    let taken = project::all_sources(&repo)[0].1["id"].as_str().unwrap().to_string();
    w.id = taken;
    assert!(wizard::finish(&repo, &w, &d.path().join("none.json")).is_err());
    assert!(!repo.projects.join("zz_wiz2").exists(), "no project directory is left behind");
}

#[test]
fn the_steps_and_what_blocks_next() {
    let (_d, repo) = fixture();
    let ids = project::ids(&repo);
    let mut w = Wizard::new_instrument();
    assert_eq!(w.steps().first(), Some(&Step::Folder), "the folder first: the guess needs the recordings");
    assert_eq!(w.blocker(&ids, None, Step::Folder, None).as_deref(), Some("choose the folder"));
    assert_eq!(w.blocker(&ids, None, Step::Played, None).as_deref(), Some("choose how it is played"));
    w.choose(&repo, "mallet", &bars_template(&repo), "the closest overtones");
    assert_eq!(w.blocker(&ids, None, Step::Played, None), None);
    assert_eq!(w.menu_group, "Percussion", "the menu group follows the donor's");
    assert_eq!(w.blocker(&ids, None, Step::Menu, None).as_deref(), Some("the instrument needs a name"));
    w.name = "Bad Name".into();
    assert!(w.blocker(&ids, None, Step::Menu, None).is_some(), "not a plain folder name");
    let w = wizard_for(&repo, "zz_ok");
    assert!(!w.steps().contains(&Step::Dynamics), "one layer: no dynamics step");
    let order = w.steps();
    let at = |s: Step| order.iter().position(|x| *x == s).unwrap();
    assert!(at(Step::Names) < at(Step::Played) && at(Step::Played) < at(Step::Check) && at(Step::Check) < at(Step::Menu));
    assert_eq!(w.blocker(&ids, None, Step::Names, Some(0)).as_deref(), Some("the rule reads no files"));
    assert_eq!(w.blocker(&ids, None, Step::Names, Some(4)), None);
    let p = Project::load(&repo, &bars_template(&repo)).unwrap();
    let a = Wizard::add_to(&repo, &p);
    assert_eq!(a.steps().first(), Some(&Step::Folder));
    assert!(!a.steps().contains(&Step::Menu) && !a.steps().contains(&Step::Played));
    assert!(project::check_source_id(&repo, &p, &a.id).is_ok(), "the proposed id is free");
    assert!(matches!(w.rule.as_ref().unwrap().reading, Reading::Names(_)));
}

#[test]
fn the_project_helpers() {
    let (_d, repo) = fixture();
    let groups = project::menu_groups(&repo);
    assert!(groups.contains(&"Percussion".to_string()) && groups.windows(2).all(|w| w[0] < w[1]));
    let chain = project::chain(&repo, "guitar");
    assert_eq!(chain.iter().filter(|s| s.as_str() == "polarizations").count(), 1, "polarizations#2 is polarizations");
    assert!(project::template_source(&repo, &bars_template(&repo)).unwrap()["role"].is_string());
    assert!(project::families(&repo).iter().any(|(_, f)| f == "struck"));
}

#[test]
fn add_to_a_project_with_an_odd_folder_name_proposes_a_valid_id() {
    let (d, repo) = fixture();
    let odd = d.path().join("projects/Odd-Name");
    std::fs::create_dir_all(&odd).unwrap();
    std::fs::copy(repo.projects.join(bars_template(&repo)).join("project.toml"), odd.join("project.toml")).unwrap();
    let p = Project::load(&repo, "Odd-Name").unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let r = repo.clone();
    std::thread::spawn(move || {
        let _ = tx.send(Wizard::add_to(&r, &Project::load(&r, "Odd-Name").unwrap()).id);
    });
    let id = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("add_to returns (it looped before)");
    assert!(project::check_source_id(&repo, &p, &id).is_ok(), "{id:?}");
}


#[test]
fn one_mallet_has_no_mallet_names_and_an_added_one_is_appended() {
    let (d, repo) = fixture();
    let mut w = wizard_for(&repo, "zz_one");
    // one source: the group part made fixed text (its first value, cord)
    if let Some(Reading::Names(n)) = w.rule.as_mut().map(|r| &mut r.reading) {
        let g = n.parts.iter().position(|p| p.role == pe_editor::rule::PartRole::Group).unwrap();
        n.set_role(g, pe_editor::rule::PartRole::Fixed, &Default::default());
    }
    w.mallet = Some("cord".into());
    let mut p = wizard::finish(&repo, &w, &d.path().join("none.json")).unwrap();
    assert_eq!(p.source_ids(), ["zz_one"]);
    assert!(p.get(&Table::top("menu"), "mallets").is_none(), "the template's names would be wrong for one mallet");
    // a second mallet added: both named
    w.add_to = Some("zz_one".into());
    w.id = "zz_one_2".into();
    w.mallet = Some("felt".into());
    wizard::add_into(&mut p, &repo, &w).unwrap();
    assert_eq!(p.get(&Table::top("menu"), "mallets").unwrap(), json!(["Cord", "Felt"]));
    // a third appended to the list as it stands (names edited by hand stay)
    p.set(&Table::top("menu"), "mallets", project::json_to_toml(&json!(["Cord yarn", "Felt"]))).unwrap();
    w.id = "zz_one_3".into();
    w.mallet = Some("rubber".into());
    wizard::add_into(&mut p, &repo, &w).unwrap();
    assert_eq!(p.get(&Table::top("menu"), "mallets").unwrap(), json!(["Cord yarn", "Felt", "Rubber"]));
}

#[test]
fn a_failed_finish_leaves_a_folder_that_was_there() {
    let (d, repo) = fixture();
    let left = repo.projects.join("zz_left/cache/kept.txt");
    std::fs::create_dir_all(left.parent().unwrap()).unwrap();
    std::fs::write(&left, "x").unwrap();
    let mut w = wizard_for(&repo, "zz_left");
    w.id = project::all_sources(&repo)[0].1["id"].as_str().unwrap().to_string();
    assert!(wizard::finish(&repo, &w, &d.path().join("none.json")).is_err());
    assert!(left.is_file(), "the leftover cache is not Finish's to delete");
    assert!(!repo.projects.join("zz_left/project.toml").exists(), "what Finish wrote is gone");
}

#[test]
fn a_failed_cache_copy_keeps_the_saved_project() {
    let (d, repo) = fixture();
    let w = wizard_for(&repo, "zz_copy");
    // cache/ a file: the measurements cannot be copied in
    std::fs::create_dir_all(repo.projects.join("zz_copy")).unwrap();
    std::fs::write(repo.projects.join("zz_copy/cache"), "").unwrap();
    let cache = d.path().join("wizard-audio.json");
    std::fs::write(&cache, r#"{"version": 2, "files": {}}"#).unwrap();
    assert!(wizard::finish(&repo, &w, &cache).is_ok(), "the copy is a cache: measuring again only costs time");
    assert!(repo.projects.join("zz_copy/project.toml").is_file());
}

#[test]
fn finish_copies_only_the_folders_measurements() {
    let (d, repo) = fixture();
    let w = wizard_for(&repo, "zz_meas");
    let root = suggest::data_root(&repo.data, "zzset");
    let entry = json!({"size": 0, "mtime_ns": 0, "measure": null, "error": "x"});
    let mine = format!("{root}/Bar_cord_C4_rr1.wav");
    let cache = d.path().join("wizard-audio.json");
    std::fs::write(&cache, json!({"version": 2, "files": {mine.clone(): entry, "/elsewhere/a.wav": entry}}).to_string()).unwrap();
    wizard::finish(&repo, &w, &cache).unwrap();
    let copied: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(repo.projects.join("zz_meas/cache/editor/audio.json")).unwrap()).unwrap();
    let keys: Vec<&String> = copied["files"].as_object().unwrap().keys().collect();
    assert_eq!(keys, [&mine]);
}

#[test]
fn taken_ids_stop_the_last_step() {
    let (_d, repo) = fixture();
    let ids = project::ids(&repo);
    let mut w = wizard_for(&repo, "zz_ids");
    assert_eq!(w.blocker(&ids, None, Step::Menu, Some(4)), None);
    let taken = project::all_sources(&repo)[0].1["id"].as_str().unwrap().to_string();
    w.id = taken.clone();
    assert!(w.blocker(&ids, None, Step::Menu, Some(4)).is_some_and(|b| b.contains(&taken)), "found before Finish");
    // adding to a project: its own sources (saved or not) count
    let p = Project::load(&repo, &bars_template(&repo)).unwrap();
    let mut a = wizard_for(&repo, "zz_ids");
    a.add_to = Some(p.name.clone());
    a.id = p.source_ids()[0].clone();
    assert!(a.blocker(&ids, Some(&p), Step::Folder, None).is_some(), "the id typed on the folder step");
    assert!(a.blocker(&ids, Some(&p), Step::Check, Some(4)).is_some());
}

#[test]
fn each_group_value_is_checked_with_its_own_skips() {
    let (_d, repo) = fixture();
    let mut w = wizard_for(&repo, "zz_grp");
    let root = suggest::data_root(&repo.data, "zzset");
    let reads = |w: &Wizard| mapping::items(&w.current().unwrap(), &root).unwrap().items[0]["relpath"].as_str().unwrap().to_string();
    assert!(reads(&w).contains("cord"), "the first value is shown first");
    w.shown = Some("yarn".into());
    assert!(reads(&w).contains("yarn"));
    w.skip(60);
    w.relabel(62, 63);
    let srcs = w.sources().unwrap();
    assert!(srcs[0].1.get("skip").is_none() && srcs[0].1.get("relabel").is_none(), "cord: none of yarn's");
    assert_eq!(srcs[1].1["skip"], json!([60]));
    assert_eq!(srcs[1].1["relabel"], json!({"62": 63}));
    assert_eq!(w.current().unwrap()["skip"], json!([60]), "the page shows the shown value's");
}

#[test]
fn a_group_value_that_reads_nothing_is_named() {
    let (_d, repo) = fixture();
    let mut w = wizard_for(&repo, "zz_none");
    let root = suggest::data_root(&repo.data, "zzset");
    assert!(w.sources_reading_nothing(&root).is_empty());
    if let Some(Reading::Names(n)) = w.rule.as_mut().map(|r| &mut r.reading) {
        n.group.as_mut().unwrap().push("felt".into());
    }
    assert_eq!(w.sources_reading_nothing(&root), ["zz_none_felt"], "felt has no files");
}

#[test]
fn a_failed_suggestion_says_why() {
    let (_d, repo) = fixture();
    let ids = project::ids(&repo);
    let mut w = Wizard::new_instrument();
    w.folder = Some(suggest::Folder::from_data_path("zzset"));
    assert_eq!(w.blocker(&ids, None, Step::Reading, None).as_deref(), Some("the suggestion is still running"));
    w.suggest_error = Some("a.sfz: no regions with a sample".into());
    let b = w.blocker(&ids, None, Step::Reading, None).unwrap();
    assert!(b.contains("no regions with a sample") && !b.contains("running"), "{b}");
}

#[test]
fn a_second_answer_replaces_the_first_donor() {
    let (_d, repo) = fixture();
    let mut w = Wizard::new_instrument();
    let kinds = project::kinds(&repo);
    let strings = kinds.iter().find(|k| !k.bar_like).unwrap();
    w.choose(&repo, "mallet", &bars_template(&repo), "the closest overtones");
    assert_eq!(w.role, "mallet");
    w.choose(&repo, &strings.id, &strings.fallback, "the usual one");
    assert_eq!((w.kind.as_str(), w.template.as_str()), (strings.id.as_str(), strings.fallback.as_str()));
    assert_eq!(w.role, "main", "the second donor's role");
    assert_eq!(w.mallet, None);
}

#[test]
fn the_menu_step_checks_every_id_from_the_name() {
    let (_d, repo) = fixture();
    let mut w = wizard_for(&repo, "zz_grp2");
    // the group's second source is name_yarn: taken by a project of that name
    let ids: std::collections::HashMap<String, String> = [("zz_grp2_yarn".to_string(), "other".to_string())].into();
    assert!(w.blocker(&ids, None, Step::Menu, Some(4)).is_some_and(|b| b.contains("zz_grp2_yarn")));
    w.name = "zz_grp3".into();
    w.id = w.name.clone();
    assert_eq!(w.blocker(&ids, None, Step::Menu, Some(4)), None);
}

#[test]
fn bars_get_auto_tracks_and_the_mallet_their_dynamics_need() {
    let (d, repo) = fixture();
    let w = wizard_for(&repo, "zz_auto");
    let p = wizard::finish(&repo, &w, &d.path().join("none.json")).unwrap();
    let bars = Table::top("bars");
    assert_eq!(p.get(&bars, "tracks").unwrap(), json!("auto"));
    assert!(p.get(&bars, "tol").is_none() && p.get(&bars, "tied").is_none() && p.get(&bars, "pp_mirror").is_none());
    assert!(p.get(&bars, "mallet").is_some(), "one recorded dynamic: an assumed mallet");
    assert!(p.text().contains("tracks = \"auto\""));
    assert!(p.text().lines().next().unwrap().contains(&w.template), "the first line names the donor");
}

#[test]
fn a_string_set_borrows_what_it_lacks() {
    let (_d, repo) = fixture();
    let kinds = project::kinds(&repo);
    let grand = kinds.iter().find(|k| k.id == "grand").unwrap();
    let uses = Table::top("uses");
    let skip = |p: &Project| -> Vec<String> { p.get(&Table::top("project"), "skip").and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter().filter_map(|v| v.as_str().map(String::from)).collect() };
    // without release samples, one dynamic
    let mut p = project::new_from_donor(&repo, "zz_s1", &grand.fallback, "").unwrap();
    wizard::own_values(&mut p, &repo, &grand.fallback, &[("zz_s1".into(), json!({"dynamics": {"mf": "1/1"}}))]).unwrap();
    for s in ["release_noise", "dampers", "pedal_noise"] {
        assert!(skip(&p).contains(&s.to_string()), "{s} skipped");
    }
    for r in ["release", "dampers", "pedal", "felt"] {
        assert!(p.get(&uses, r).is_some(), "{r} borrowed");
    }
    // with release samples, three dynamics
    let mut q = project::new_from_donor(&repo, "zz_s2", &grand.fallback, "").unwrap();
    wizard::own_values(&mut q, &repo, &grand.fallback, &[("zz_s2".into(), json!({"dynamics": {"pp": "a", "mf": "b", "ff": "c"}, "release": {"kind": "sfz", "file": "x.sfz"}}))]).unwrap();
    assert!(!skip(&q).contains(&"release_noise".to_string()), "its own release samples are analyzed");
    assert!(q.get(&uses, "release").is_none() && q.get(&uses, "felt").is_none());
    assert!(skip(&q).contains(&"dampers".to_string()), "the dampers are a lender's (measured from release recordings)");
}

#[test]
fn a_new_instruments_folder_step_needs_no_id() {
    // the id follows the name, typed on the last step
    let (_d, repo) = fixture();
    let mut w = Wizard::new_instrument();
    w.folder = Some(suggest::Folder::from_data_path("zzset"));
    assert_eq!(w.blocker(&project::ids(&repo), None, Step::Folder, None), None);
}

#[test]
fn the_register_fit_follows_the_recorded_levels() {
    // register fits the recorded levels (or a borrowed reference): a donor's skip is its own set's
    let (_d, repo) = fixture();
    let skip = |p: &Project| -> Vec<String> { p.get(&Table::top("project"), "skip").and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter().filter_map(|v| v.as_str().map(String::from)).collect() };
    let kinds = project::kinds(&repo);
    let grand = kinds.iter().find(|k| k.id == "grand").unwrap();
    let mut p = project::new_from_donor(&repo, "zz_r1", &grand.fallback, "").unwrap();
    wizard::own_values(&mut p, &repo, &grand.fallback, &[("zz_r1".into(), json!({"dynamics": {"pp": "a", "mf": "b", "ff": "c"}, "levels": "recorded"}))]).unwrap();
    assert!(!skip(&p).contains(&"register".to_string()), "recorded levels: register is fitted");
    let harp = kinds.iter().find(|k| k.id == "harp").unwrap();
    let mut q = project::new_from_donor(&repo, "zz_r2", &harp.fallback, "").unwrap();
    wizard::own_values(&mut q, &repo, &harp.fallback, &[("zz_r2".into(), json!({"dynamics": {"mf": "a", "ff": "b"}, "levels": "normalized"}))]).unwrap();
    assert!(skip(&q).contains(&"register".to_string()), "normalized, no reference: nothing to fit");
}

#[test]
fn new_recordings_forget_the_answer() {
    // another folder (or SFZ file) after the answer: the donor was chosen from the old files
    let (_d, repo) = fixture();
    let mut w = wizard_for(&repo, "zz_again");
    assert!(!w.kind.is_empty() && !w.template.is_empty());
    let folder = suggest::Folder::from_data_path("zzset");
    let files = suggest::audio_files(&repo.data, &folder);
    let s = suggest::suggest_names(&repo.data, &folder, &files, &Measures::default());
    w.apply_suggestion(s, &[]);
    assert!(w.kind.is_empty() && w.template.is_empty(), "asked again on the page");
}

fn skips(p: &Project) -> Vec<String> {
    p.get(&Table::top("project"), "skip").and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter().filter_map(|v| v.as_str().map(String::from)).collect()
}

#[test]
fn a_donor_that_borrows_lends_what_it_borrows() {
    // Grand F borrows Grand E's dampers and Salamander's release and pedal; Osiris its felt from
    // Grand C: a set made from them borrows from those lenders, not from the donor
    let (_d, repo) = fixture();
    let uses = Table::top("uses");
    let mut p = project::new_from_donor(&repo, "zz_b1", "grand_f", "").unwrap();
    wizard::own_values(&mut p, &repo, "grand_f", &[("zz_b1".into(), json!({"dynamics": {"mf": "a"}}))]).unwrap();
    assert_eq!(p.get(&uses, "dampers").unwrap(), json!("grand_e"));
    for r in ["release", "pedal"] {
        assert_eq!(p.get(&uses, r).unwrap(), json!("grand_a"), "{r}");
    }
    assert_eq!(p.get(&uses, "felt").unwrap(), json!("grand_f"), "one dynamic: the donor's own felt");
    let mut q = project::new_from_donor(&repo, "zz_b2", "grand_g", "").unwrap();
    wizard::own_values(&mut q, &repo, "grand_g", &[("zz_b2".into(), json!({"dynamics": {"mf": "a"}}))]).unwrap();
    assert_eq!(q.get(&uses, "felt").unwrap(), json!("grand_c"), "the donor's lender");
}

#[test]
fn a_harpsichord_set_analyzes_its_own_releases() {
    // the plucked chain has release_noise (no dampers or pedal_noise step): own release samples
    // are analyzed, none borrows; the donor's buff stop stays
    let (_d, repo) = fixture();
    let uses = Table::top("uses");
    let mut p = project::new_from_donor(&repo, "zz_h1", "harpsichord", "").unwrap();
    wizard::own_values(&mut p, &repo, "harpsichord", &[("zz_h1".into(), json!({"dynamics": {"mf": "1/1"}, "levels": "recorded",
        "release": {"kind": "sfz", "file": "x.sfz"}}))]).unwrap();
    assert!(!skips(&p).contains(&"release_noise".to_string()) && p.get(&uses, "release").is_none());
    assert!(!skips(&p).iter().any(|s| s == "dampers" || s == "pedal_noise"), "not the plucked chain's steps");
    assert!(p.get(&uses, "buff").is_some() && p.get(&uses, "felt").is_none());
    let mut q = project::new_from_donor(&repo, "zz_h2", "harpsichord", "").unwrap();
    wizard::own_values(&mut q, &repo, "harpsichord", &[("zz_h2".into(), json!({"dynamics": {"mf": "1/1"}, "levels": "recorded"}))]).unwrap();
    assert!(skips(&q).contains(&"release_noise".to_string()));
    assert_eq!(q.get(&uses, "release").unwrap(), json!("harpsichord"), "no release samples: the donor's");
}

#[test]
fn bars_dynamics_decide_the_mallet_and_pp_mirror() {
    let (_d, repo) = fixture();
    let bars = Table::top("bars");
    let made = |name: &str, dynamics: serde_json::Value| {
        let mut p = project::new_from_donor(&repo, name, "handchimes", "").unwrap();
        let mut src = json!({});
        if !dynamics.is_null() {
            src["dynamics"] = dynamics;
        }
        wizard::own_values(&mut p, &repo, "handchimes", &[(name.into(), src)]).unwrap();
        p
    };
    let two = made("zz_d2", json!({"mf": "a", "ff": "b"}));
    assert_eq!(two.get(&bars, "pp_mirror").unwrap(), json!(true));
    assert!(two.get(&bars, "mallet").is_none(), "two dynamics: the mallet is measured");
    let three = made("zz_d3", json!({"pp": "a", "mf": "b", "ff": "c"}));
    assert!(three.get(&bars, "pp_mirror").is_none() && three.get(&bars, "mallet").is_none());
    let none = made("zz_d0", serde_json::Value::Null);
    assert!(none.get(&bars, "mallet").is_some(), "no dynamics named: one recorded, an assumed mallet");
}

#[test]
fn a_name_typed_keeps_a_menu_name_typed() {
    let mut w = Wizard::new_instrument();
    w.set_name("glock");
    assert_eq!((w.id.as_str(), w.menu_name.as_str()), ("glock", "glock"), "the menu follows the name");
    w.menu_name = "Glockenspiel".into();
    w.set_name("glockenspiel");
    assert_eq!(w.id, "glockenspiel");
    assert_eq!(w.menu_name, "Glockenspiel", "a menu name typed by hand stays");
}

#[test]
fn the_wizard_writes_skip_files() {
    let (_d, repo) = fixture();
    let mut w = wizard_for(&repo, "zz_skipf");
    w.shown = Some("yarn".into());
    w.skip_file("a.wav");
    w.skip_file("b.wav");
    w.skip_file("a.wav");
    let srcs = w.sources().unwrap();
    assert!(srcs[0].1.get("skip_files").is_none(), "cord: none of yarn's");
    assert_eq!(srcs[1].1["skip_files"], json!(["a.wav", "b.wav"]));
    w.unskip_file("a.wav");
    w.unskip_file("b.wav");
    assert!(w.sources().unwrap()[1].1.get("skip_files").is_none(), "the key goes with its last file");
}

#[test]
fn a_new_project_does_not_take_the_donors_credit() {
    // the [license] block names the donor's recordings: a set made from it starts without one
    // (its LICENSE.md says "not stated" until the block names its own)
    let (_d, repo) = fixture();
    let p = project::new_from_donor(&repo, "zz_l1", "upright_c", "").unwrap();
    assert_eq!(p.get(&Table::top("license"), "set"), None);
    assert!(!p.text().contains("[license]"));
}
