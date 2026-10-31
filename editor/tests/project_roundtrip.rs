//! Every committed project file through the project model: loaded and saved unchanged.
use pe_editor::project::{Project, Repo, project_names};

#[test]
fn every_project_round_trips_byte_for_byte() {
    let repo = Repo::from_env();
    let names = project_names(&repo);
    assert!(names.len() >= 30, "found {} projects", names.len());
    for name in names {
        let p = Project::load(&repo, &name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let file = std::fs::read_to_string(&p.path).unwrap();
        assert_eq!(p.text(), file, "{name}");
        assert!(!p.dirty(), "{name}");
    }
}

#[test]
fn the_json_view_keeps_types_and_order() {
    let repo = Repo::from_env();
    let p = Project::load(&repo, &project_names(&repo)[0]).unwrap();
    let j = p.to_json();
    let parts = j["part"].as_array().expect("part is an array of tables");
    assert!(parts[0]["id"].is_string());
    let t: toml_edit::DocumentMut = "a = 1\nb = 1.5\nc = [1, \"x\"]\nd = { e = true }\n".parse().unwrap();
    let v = pe_editor::project::toml_to_json(t.as_item());
    assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"a":1,"b":1.5,"c":[1,"x"],"d":{"e":true}}"#);
}

#[test]
fn a_load_error_names_the_line() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("projects/bad")).unwrap();
    std::fs::write(dir.path().join("projects/bad/project.toml"), "[project]\nfamily = \n").unwrap();
    let repo = Repo { root: dir.path().into(), projects: dir.path().join("projects"), data: dir.path().join("data") };
    let e = Project::load(&repo, "bad").err().expect("an error");
    assert_eq!(e.line, Some(2));
}

use pe_editor::project::{self, Disk, SaveError, Table};
use toml_edit::Value;

fn temp_copy(names: &[&str]) -> (tempfile::TempDir, Repo) {
    let real = Repo::from_env();
    let d = tempfile::tempdir().unwrap();
    for n in project_names(&real) {
        if !names.is_empty() && !names.contains(&n.as_str()) {
            continue;
        }
        std::fs::create_dir_all(d.path().join("projects").join(&n)).unwrap();
        std::fs::copy(real.projects.join(&n).join("project.toml"), d.path().join("projects").join(&n).join("project.toml")).unwrap();
    }
    std::fs::copy(real.projects.join("steps.toml"), d.path().join("projects/steps.toml")).unwrap();
    let repo = Repo { root: real.root.clone(), projects: d.path().join("projects"), data: real.data.clone() };
    (d, repo)
}

#[test]
fn one_edit_changes_only_that_line() {
    let repo = Repo::from_env();
    for name in project_names(&repo) {
        let mut p = Project::load(&repo, &name).unwrap();
        if p.get(&Table::top("menu"), "name").is_none() {
            continue;
        }
        let before = p.text();
        p.set(&Table::top("menu"), "name", Value::from("Edited")).unwrap();
        let after = p.text();
        let (b, a): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
        assert_eq!(b.len(), a.len(), "{name}");
        let changed: Vec<_> = b.iter().zip(&a).filter(|(x, y)| x != y).collect();
        assert_eq!(changed.len(), 1, "{name}: {changed:?}");
        assert!(changed[0].1.starts_with("name = \"Edited\""), "{name}: {}", changed[0].1);
        assert!(p.dirty());
    }
}

#[test]
fn keys_show_values_without_their_comments() {
    let mut p = Project::load(&Repo::from_env(), &project_names(&Repo::from_env())[0]).unwrap();
    p.set(&Table::top("menu"), "group", Value::from("G")).unwrap();
    let keys = p.keys(&Table::top("menu"));
    assert!(keys.contains(&("group".to_string(), "\"G\"".to_string())), "{keys:?}");
}

#[test]
fn sources_are_added_moved_and_removed_on_the_text() {
    let (_d, repo) = temp_copy(&[]);
    let name = project_names(&repo).into_iter()
        .find(|n| Project::load(&repo, n).unwrap().source_ids().len() >= 3).expect("a project with three sources");
    let mut p = Project::load(&repo, &name).unwrap();
    let ids = p.source_ids();
    let before = p.text();
    p.move_source(&ids[1], true).unwrap();
    let moved = p.source_ids();
    assert_eq!(moved[0], ids[1]);
    assert_eq!(moved[1], ids[0]);
    let sorted = |t: &str| { let mut l: Vec<String> = t.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect(); l.sort(); l };
    assert_eq!(sorted(&before), sorted(&p.text()), "a move keeps every line");
    p.move_source(&ids[1], false).unwrap();
    assert_eq!(p.source_ids(), ids);
    p.remove_source(&ids[2]).unwrap();
    assert_eq!(p.source_ids(), ids[..2].to_vec());
    let map = serde_json::json!({"kind": "pattern", "glob": "x/*.wav", "regex": r"(?P<note>[A-G]\d)\.wav",
                                 "note": {"group": "note"}, "dynamic": "mf", "order": "note"});
    p.add_source(&repo, "zz_added", "main", "nowhere", &map).unwrap();
    assert_eq!(p.source_ids().last().unwrap(), "zz_added");
    let s = p.source_json("zz_added").unwrap();
    assert_eq!(s["map"]["note"]["group"], "note");
    assert_eq!(s["root"], "nowhere");
    p.set(&Table::Source("zz_added".into()), "levels", Value::from("recorded")).unwrap();
    p.set(&Table::Map("zz_added".into()), "glob", Value::from("y/*.wav")).unwrap();
    assert_eq!(p.source_json("zz_added").unwrap()["map"]["glob"], "y/*.wav");
    p.save().unwrap();
    let again = Project::load(&repo, &name).unwrap();
    assert_eq!(again.source_ids().last().unwrap(), "zz_added");
}

#[test]
fn save_refuses_a_file_changed_on_disk() {
    let (_d, repo) = temp_copy(&[]);
    let name = project_names(&repo)[0].clone();
    let mut p = Project::load(&repo, &name).unwrap();
    assert!(p.disk() == Disk::Same);
    let outside = format!("{}\n# edited outside\n", p.text());
    std::fs::write(&p.path, &outside).unwrap();
    assert!(p.disk() == Disk::Changed);
    p.set(&Table::top("menu"), "name", Value::from("Mine")).unwrap();
    assert!(matches!(p.save(), Err(SaveError::Conflict)));
    assert_eq!(std::fs::read_to_string(&p.path).unwrap(), outside, "the outside edit is kept");
    p.reload().unwrap();
    assert_eq!(p.text(), outside);
    assert!(!p.dirty());
    p.set(&Table::top("menu"), "name", Value::from("Mine")).unwrap();
    std::fs::write(&p.path, "[project]\nfamily = \"x\"\n").unwrap();
    p.save_over().unwrap();
    assert!(std::fs::read_to_string(&p.path).unwrap().contains("Mine"));
}

#[test]
fn ids_used_elsewhere_are_refused() {
    let repo = Repo::from_env();
    let all = project::ids(&repo);
    let (some_project, _) = all.iter().find(|(k, v)| k == v).unwrap();
    let (some_source, owner) = all.iter().find(|(k, v)| k != v).unwrap();
    assert!(project::check_project_name(&repo, some_project).is_err());
    assert!(project::check_project_name(&repo, some_source).is_err());
    assert!(project::check_project_name(&repo, "Bad-Name").is_err());
    assert!(project::check_project_name(&repo, "zz_free").is_ok());
    let other = project_names(&repo).into_iter().find(|n| n != owner).unwrap();
    let p = Project::load(&repo, &other).unwrap();
    assert!(project::check_source_id(&repo, &p, some_source).is_err());
    assert!(project::check_source_id(&repo, &p, &p.source_ids().first().cloned().unwrap_or_default()).is_err() || p.source_ids().is_empty());
    assert!(project::check_source_id(&repo, &p, "zz_free").is_ok());
}

#[test]
fn a_folder_is_linked_into_data_with_a_sources_stub() {
    let d = tempfile::tempdir().unwrap();
    let repo = Repo { root: d.path().into(), projects: d.path().join("projects"), data: d.path().join("data") };
    std::fs::create_dir_all(&repo.data).unwrap();
    std::fs::write(repo.data.join("SOURCES.md"), "# Data sources\n").unwrap();
    let outside = d.path().join("elsewhere/My Set");
    std::fs::create_dir_all(&outside).unwrap();
    let link = project::link_folder(&repo, &outside, "my-set").unwrap();
    assert_eq!(std::fs::read_link(&link).unwrap(), outside.canonicalize().unwrap());
    let sources = std::fs::read_to_string(repo.data.join("SOURCES.md")).unwrap();
    assert!(sources.contains("## my-set"), "{sources}");
    assert!(project::link_folder(&repo, &outside, "my-set").is_err(), "the name is taken");
    assert!(project::link_folder(&repo, &repo.data.join("my-set"), "again").is_err(), "already under data/");
    assert!(project::link_folder(&repo, &outside, "../up").is_err());
}

#[test]
fn a_link_made_for_nothing_is_undone() {
    let d = tempfile::tempdir().unwrap();
    let repo = Repo { root: d.path().into(), projects: d.path().join("projects"), data: d.path().join("data") };
    std::fs::create_dir_all(&repo.data).unwrap();
    std::fs::write(repo.data.join("SOURCES.md"), "# Data sources\n").unwrap();
    let outside = d.path().join("elsewhere/My Set");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("a.wav"), "").unwrap();
    project::link_folder(&repo, &outside, "my-set").unwrap();
    project::unlink_folder(&repo, "my-set").unwrap();
    assert!(repo.data.join("my-set").symlink_metadata().is_err(), "the link is gone");
    assert!(outside.join("a.wav").is_file(), "the folder itself stays");
    assert_eq!(std::fs::read_to_string(repo.data.join("SOURCES.md")).unwrap(), "# Data sources\n");
    std::fs::create_dir_all(repo.data.join("real")).unwrap();
    assert!(project::unlink_folder(&repo, "real").is_err(), "only a link is undone");
    assert!(repo.data.join("real").is_dir());
}

#[test]
fn a_string_projects_part_needs_a_source_of_its_name() {
    // the string fits address a part and its main source by one id (fit_pluck.py --profile {part})
    let repo = Repo::from_env();
    for name in project_names(&repo) {
        let p = Project::load(&repo, &name).unwrap();
        assert!(p.build_problems().is_empty(), "{name}: {:?}", p.build_problems());
    }
    let (_d, repo) = temp_copy(&[]);
    let template = project::templates(&repo).into_iter()
        .find(|t| Project::load(&repo, &t.name).unwrap().get(&Table::top("strings"), "design").is_some()).unwrap();
    let mut p = project::new_from_template(&repo, "zz_strings", &template.name).unwrap();
    let map = serde_json::json!({"kind": "pattern", "glob": "x/*.wav", "regex": ".*", "note": {"group": "note"}, "dynamic": "mf"});
    let fields = |p: &mut Project, id: &str| {
        p.set(&Table::Source(id.into()), "dynamics", project::json_to_toml(&serde_json::json!({"mf": "mf"}))).unwrap();
        p.set(&Table::Source(id.into()), "levels", Value::from("recorded")).unwrap();
    };
    p.add_source(&repo, "zz_strings_set", "main", "nowhere", &map).unwrap();
    fields(&mut p, "zz_strings_set");
    let problems = p.build_problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("zz_strings"), "{problems:?}");
    p.remove_source("zz_strings_set").unwrap();
    p.add_source(&repo, "zz_strings", "main", "nowhere", &map).unwrap();
    fields(&mut p, "zz_strings");
    assert!(p.build_problems().is_empty(), "{:?}", p.build_problems());
}

#[test]
fn an_instruments_sources_need_dynamics_and_levels() {
    // the fits read every source's dynamics (pp, mf, ff → its layers) and levels: a source without them
    // fails its first fit (KeyError: 'dynamics') after the analysis
    let (_d, repo) = temp_copy(&[]);
    let mut p = project::new_from_template(&repo, "zz_dyn", "harp").unwrap();
    let map = serde_json::json!({"kind": "pattern", "glob": "x/*.wav", "regex": ".*", "note": {"group": "note"}, "dynamic": "mf"});
    p.add_source(&repo, "zz_dyn", "main", "nowhere", &map).unwrap();
    let problems = p.build_problems();
    assert!(problems.iter().any(|x| x.contains("dynamics") && x.contains("levels")), "{problems:?}");
    p.set(&Table::Source("zz_dyn".into()), "dynamics", project::json_to_toml(&serde_json::json!({"mf": "mf"}))).unwrap();
    p.set(&Table::Source("zz_dyn".into()), "levels", Value::from("recorded")).unwrap();
    assert!(p.build_problems().is_empty(), "{:?}", p.build_problems());
}

#[test]
fn a_source_is_renamed_under_the_id_rules() {
    let (_d, repo) = temp_copy(&[]);
    let mut p = project::new_from_template(&repo, "zz_ren", "harp").unwrap();
    let map = serde_json::json!({"kind": "pattern", "glob": "x/*.wav", "regex": ".*", "note": {"group": "note"}, "dynamic": "mf"});
    p.add_source(&repo, "zz_ren_set", "main", "nowhere", &map).unwrap();
    let taken = project::ids(&repo).into_iter().find(|(k, v)| k != v).unwrap().0;
    assert!(p.rename_source(&repo, "zz_ren_set", &taken).is_err(), "an id of another project");
    p.rename_source(&repo, "zz_ren_set", "zz_ren").unwrap();
    assert_eq!(p.source_ids(), ["zz_ren"]);
    assert_eq!(p.source_json("zz_ren").unwrap()["root"], "nowhere");
}

#[test]
fn update_source_leaves_equal_values_alone() {
    let (_d, repo) = temp_copy(&["grand_d"]);
    let mut p = Project::load(&repo, "grand_d").unwrap();
    let before = p.text();
    let id = p.source_ids()[0].clone();
    let src = p.source_json(&id).unwrap();
    assert!(!p.update_source(&id, &src).unwrap(), "nothing changed");
    assert_eq!(p.text(), before);
    let mut changed = src.clone();
    changed["levels"] = serde_json::json!("normalized");
    changed["map"].as_object_mut().unwrap().remove("order");
    assert!(p.update_source(&id, &changed).unwrap());
    let after = p.text();
    let (b, a): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    assert_eq!(a.len(), b.len() - 1, "the order line is gone");
    assert!(a.contains(&r#"levels = "normalized""#));
    assert!(a.iter().filter(|l| !l.starts_with("levels")).all(|l| b.contains(l)), "every other line as it was");
}

#[test]
fn a_folder_linked_into_data_is_under_data() {
    let d = tempfile::tempdir().unwrap();
    let outside = d.path().join("outside/set");
    std::fs::create_dir_all(outside.join("sub")).unwrap();
    std::fs::create_dir_all(d.path().join("data")).unwrap();
    std::os::unix::fs::symlink(&outside, d.path().join("data/linked")).unwrap();
    let data = d.path().join("data");
    let picked = data.join("linked/sub");
    assert_eq!(project::data_relative(&data, &picked.to_string_lossy()).as_deref(), Some("linked/sub"));
    assert_eq!(project::data_relative(&data, "vcsl/x/").as_deref(), Some("vcsl/x"));
    assert_eq!(project::data_relative(&data, &outside.to_string_lossy()), None, "the set itself, by its own path, is outside");
}

#[test]
fn a_step_ticked_keeps_the_skips_the_chain_does_not_list() {
    let chain: Vec<String> = ["a", "b", "c"].map(String::from).into();
    let skip: Vec<String> = ["old", "c"].map(String::from).into();
    assert_eq!(project::skip_toggled(&skip, &chain, "a", true), ["a", "c", "old"]);
    assert_eq!(project::skip_toggled(&skip, &chain, "c", false), ["old"]);
}

#[test]
fn a_level_table_is_written_as_a_sub_table_and_read_back() {
    let (_dir, repo) = temp_copy(&["marimba"]);
    let mut p = pe_editor::project::Project::load(&repo, "marimba").unwrap();
    let id = p.source_ids()[0].clone();
    let t = serde_json::json!({"keys": [60, 62], "pp": [1.5, -0.5], "mf": [0.0, 0.0]});
    p.set_source_table(&id, "level_corrections", &t, "from the Compare page, smoothed").unwrap();
    let text = p.text();
    assert!(text.contains("[part.source.level_corrections]") && text.contains("# from the Compare page, smoothed"), "{text}");
    // after the source's map, before the next source
    let (at, map) = (text.find("[part.source.level_corrections]").unwrap(), text.find("[part.source.map]").unwrap());
    assert!(at > map && !text[map..at].contains("[[part.source]]"), "{text}");
    p.save().unwrap();
    let back = pe_editor::project::Project::load(&repo, "marimba").unwrap();
    assert_eq!(back.source_json(&id).unwrap()["level_corrections"], t);
    assert_eq!(pe_editor::compare::level_table_of(&back.source_json(&id).unwrap()).unwrap().at("pp", 61), 0.5);
    // the source's own table removed, the other sources' kept (the marimba carries its own)
    let mut back = back;
    let tables = |p: &pe_editor::project::Project| p.text().matches("[part.source.level_corrections]").count();
    let before = tables(&back);
    back.remove_source_key(&id, "level_corrections").unwrap();
    assert!(back.source_json(&id).unwrap().get("level_corrections").is_none() && tables(&back) == before - 1);
}
