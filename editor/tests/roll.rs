//! The editor's rolling (`pe_editor::roll`): the candidates are the exported hammer pianos, draws
//! keep the roll-time rules, re-rolling one part keeps the rest, the preview is never read half
//! written, Finish writes the project.
use pe_editor::project::Repo;
use pe_editor::roll::{self, Candidates, Rng};
use pianoengine::roll::files::Export;
use pianoengine::roll::recipe::Recipe;

fn cands() -> Candidates {
    Candidates {
        references: ["grand_a", "grand_b", "grand_f", "grand_e", "grand_d", "grand_c", "grand_g", "grand_h", "upright_a", "upright_b", "upright_c", "upright_d"].map(String::from).to_vec(),
        grands: ["grand_a", "grand_b", "grand_f", "grand_e", "grand_d", "grand_c", "grand_g", "grand_h"].map(String::from).to_vec(),
    }
}

#[test]
fn the_candidates_are_the_hammer_pianos() {
    let repo = Repo::from_env();
    if !repo.root.join("instruments/grand_a/data.bin").exists() {
        return;
    }
    let c = roll::candidates(&repo);
    let mut refs = c.references.clone();
    refs.sort();
    let mut want = cands().references;
    want.sort();
    assert_eq!(refs, want);
    let mut grands = c.grands.clone();
    grands.sort();
    let mut want = cands().grands;
    want.sort();
    assert_eq!(grands, want);
}

#[test]
fn a_draw_keeps_the_rules() {
    let c = cands();
    let allowed: Vec<String> = c.grands.iter().filter(|g| *g != "grand_g").cloned().collect();
    for seed in 0..200 {
        let r = roll::draw(&c, &allowed, &mut Rng::new(seed), "rolled");
        assert_eq!(r.strings.len(), 3);
        let names: Vec<&String> = r.strings.iter().map(|s| &s.0).collect();
        assert!(names.iter().all(|n| allowed.contains(n)) && names[0] != names[1] && names[1] != names[2] && names[0] != names[2]);
        let sum: f64 = r.strings.iter().map(|s| s.1).sum();
        assert!((sum - 1.0).abs() < 1e-12, "{sum}");
        assert!(r.strings.iter().all(|s| s.1 <= 0.55), "{:?}", r.strings);
        let frame = &r.strings.iter().fold(&r.strings[0], |b, s| if s.1 > b.1 { s } else { b }).0;
        assert!(!r.fine_structure.contains(frame));
        assert!(r.fine_structure.iter().all(|f| allowed.contains(f)));
        assert!((-1.5..=0.5).contains(&r.warmth) && (-1.2..=1.2).contains(&r.air));
        assert_eq!(r.references, c.references);
        assert_eq!(r.menu.1, "Rolled pianos");
    }
}

#[test]
fn rerolling_strings_keeps_the_body() {
    let c = cands();
    let mut r = roll::draw(&c, &c.grands, &mut Rng::new(7), "rolled");
    let before = r.clone();
    roll::reroll_strings(&mut r, &c.grands, &mut Rng::new(8));
    assert_eq!((r.warmth, r.air, &r.shade), (before.warmth, before.air, &before.shade));
    assert_ne!(r.strings, before.strings);
    let frame = &r.strings.iter().fold(&r.strings[0], |b, s| if s.1 > b.1 { s } else { b }).0;
    assert!(!r.fine_structure.contains(frame));
    let mut r2 = before.clone();
    roll::reroll_fine(&mut r2, &c.grands, &mut Rng::new(9));
    assert_eq!((r2.strings.clone(), r2.warmth, r2.air), (before.strings.clone(), before.warmth, before.air));
}

#[test]
fn the_preview_is_never_half_written() {
    let repo = Repo::from_env();
    let inst = repo.root.join("instruments");
    if !inst.join("grand_a/data.bin").exists() {
        return;
    }
    let a = Export::read(&inst.join("grand_a")).unwrap();
    let mut b = Export::read(&inst.join("grand_d")).unwrap();
    // a longer data.bin, so the two versions tell apart
    let f = b.f64s("/correction/C_db").unwrap();
    b.set_f64s("/correction/C_db", &[f.clone(), vec![0.0]].concat()).unwrap();
    let lens = [a.data.len(), b.data.len()];
    let dir = tempfile::tempdir().unwrap();
    let preview = dir.path().join("rolled_preview");
    roll::write_preview_at(&preview, &a, "a").unwrap();
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            for i in 0..40 {
                roll::write_preview_at(&preview, if i % 2 == 0 { &b } else { &a }, "x").unwrap();
            }
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(text) = std::fs::read_to_string(preview.join("instrument.json")) {
                assert!(serde_json::from_str::<serde_json::Value>(&text).is_ok(), "instrument.json read half written");
            }
            if let Ok(d) = std::fs::read(preview.join("data.bin")) {
                assert!(lens.contains(&d.len()), "data.bin read half written: {} bytes", d.len());
            }
        }
    });
    let left: Vec<_> = std::fs::read_dir(&preview).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
    assert!(left.is_empty());
}

#[test]
fn finish_writes_the_project() {
    let real = Repo::from_env();
    let d = tempfile::tempdir().unwrap();
    let repo = Repo { root: d.path().to_path_buf(), projects: d.path().join("projects"), data: real.data.clone() };
    std::fs::create_dir_all(repo.root.join("instruments/rolled_preview")).unwrap();
    let r = roll::draw(&cands(), &cands().grands, &mut Rng::new(3), "my_roll");
    roll::finish(&repo, &r).unwrap();
    let text = std::fs::read_to_string(repo.projects.join("my_roll/project.toml")).unwrap();
    assert_eq!(Recipe::parse(&text, "my_roll").unwrap(), r);
    assert!(!repo.root.join("instruments/rolled_preview").exists());
    assert!(roll::finish(&repo, &r).is_err(), "a taken name");
    let mut bad = r.clone();
    bad.name = "Not Allowed".into();
    assert!(roll::finish(&repo, &bad).is_err());
}

#[test]
fn a_superseded_preview_is_not_written() {
    let repo = Repo::from_env();
    let inst = repo.root.join("instruments");
    if !inst.join("grand_a/data.bin").exists() {
        return;
    }
    let a = Export::read(&inst.join("grand_a")).unwrap();
    let recipe = roll::draw(&cands(), &cands().grands, &mut Rng::new(4), "rolled");
    let dir = tempfile::tempdir().unwrap();
    let preview = dir.path().join("rolled_preview");
    let old = roll::begin_preview();
    let new = roll::begin_preview();
    assert!(!roll::write_preview_if_current(&preview, &a, "x", &recipe, old).unwrap(), "a superseded job wrote");
    assert!(!preview.exists());
    assert!(roll::write_preview_if_current(&preview, &a, "x", &recipe, new).unwrap());
    assert!(preview.join("instrument.json").exists());
    roll::forget_preview();
    assert!(!roll::write_preview_if_current(&preview, &a, "x", &recipe, new).unwrap(), "a job written after Cancel");
}

#[test]
fn a_draw_with_no_grand_allowed_does_not_panic() {
    let c = cands();
    let r = roll::draw(&c, &[], &mut Rng::new(1), "rolled");
    assert!(r.strings.is_empty() && r.fine_structure.is_empty());
    let mut r2 = roll::draw(&c, &c.grands, &mut Rng::new(2), "rolled");
    roll::reroll_strings(&mut r2, &[], &mut Rng::new(3));
    roll::reroll_fine(&mut r2, &[], &mut Rng::new(4));
    let none = Candidates { references: c.references.clone(), grands: Vec::new() };
    let r3 = roll::draw(&none, &none.grands, &mut Rng::new(5), "rolled");
    assert!(r3.strings.is_empty());
}

fn weights(s: &[(String, f64)]) -> Vec<(&str, f64)> {
    s.iter().map(|(n, w)| (n.as_str(), *w)).collect()
}

#[test]
fn a_new_donor_takes_its_share_from_the_others() {
    // the table's slider: the grand set to its weight, the others keep their proportions, the sum 1
    let mut s = vec![("grand_b".to_string(), 0.6), ("grand_f".to_string(), 0.4)];
    roll::set_weight(&mut s, "grand_c", 0.3);
    assert_eq!(weights(&s), vec![("grand_b", 0.42), ("grand_f", 0.28), ("grand_c", 0.3)]);
    roll::set_weight(&mut s, "grand_b", 0.5);
    let sum: f64 = s.iter().map(|x| x.1).sum();
    assert!((sum - 1.0).abs() < 1e-9, "{sum}");
    assert_eq!(s[0], ("grand_b".to_string(), 0.5));
    assert!((s[1].1 / s[2].1 - 0.28 / 0.3).abs() < 0.01, "{:?}", s);
}

#[test]
fn a_donor_set_to_zero_leaves() {
    let mut s = vec![("grand_b".to_string(), 0.5), ("grand_f".to_string(), 0.3), ("grand_c".to_string(), 0.2)];
    roll::set_weight(&mut s, "grand_f", 0.0);
    assert_eq!(weights(&s), vec![("grand_b", 0.714), ("grand_c", 0.286)]);
}

#[test]
fn the_only_donor_stays_whole() {
    // nothing to give its share to: another grand is raised first
    let mut s = vec![("grand_b".to_string(), 1.0)];
    roll::set_weight(&mut s, "grand_b", 0.4);
    assert_eq!(weights(&s), vec![("grand_b", 1.0)]);
    roll::set_weight(&mut s, "grand_b", 0.0);
    assert_eq!(weights(&s), vec![("grand_b", 1.0)]);
    roll::set_weight(&mut s, "grand_f", 1.0);
    assert_eq!(weights(&s), vec![("grand_f", 1.0)]);
}

#[test]
fn the_last_fine_structure_donor_stays() {
    let mut f = vec!["grand_b".to_string(), "grand_f".to_string()];
    assert!(roll::set_fine(&mut f, "grand_c", true));
    assert_eq!(f, ["grand_b", "grand_f", "grand_c"]);
    assert!(roll::set_fine(&mut f, "grand_b", false));
    assert!(roll::set_fine(&mut f, "grand_f", false));
    assert!(!roll::set_fine(&mut f, "grand_c", false), "the last one left");
    assert_eq!(f, ["grand_c"]);
    assert!(roll::set_fine(&mut f, "grand_c", true));
    assert_eq!(f, ["grand_c"], "not twice");
}

#[test]
fn a_pulled_back_draw_shows_what_was_built() {
    let mut r = roll::draw(&cands(), &cands().grands, &mut Rng::new(11), "rolled");
    roll::adopt_knobs(&mut r, &[0.1, -0.2, 0.3, -0.4]);
    assert_eq!((r.warmth, r.air, r.shade.clone()), (0.1, -0.2, vec![0.3, -0.4]));
}

#[test]
fn a_superseded_roll_stops() {
    // a change while a roll runs starts the next: the running one ends at its next stage
    let repo = Repo::from_env();
    if !repo.root.join("instruments/grand_a/data.bin").exists() {
        return;
    }
    let r = roll::draw(&cands(), &cands().grands, &mut Rng::new(12), "rolled");
    let old = roll::begin_preview();
    roll::begin_preview();
    let t = std::time::Instant::now();
    let e = roll::preview(&repo, &r, old, false).err().expect("superseded");
    assert_eq!(e, pianoengine::roll::generate::CANCELLED);
    assert!(t.elapsed().as_secs_f64() < 1.0);
}


/// A repository in a temporary folder with a preview of `built` written (an export of the real
/// instruments standing in for the roll).
fn with_preview(built: &Recipe) -> Option<(tempfile::TempDir, Repo)> {
    let real = Repo::from_env();
    let a = Export::read(&real.root.join("instruments/grand_a")).ok()?;
    let d = tempfile::tempdir().unwrap();
    let repo = Repo { root: d.path().to_path_buf(), projects: d.path().join("projects"), data: real.data.clone() };
    let generation = roll::begin_preview();
    assert!(roll::write_preview_if_current(&repo.root.join("instruments/rolled_preview"), &a, "license", built, generation).unwrap());
    Some((d, repo))
}

#[test]
fn finish_writes_the_export_of_a_current_preview() {
    // the preview plays the recipe as it stands: Finish writes it as the project's export at once
    // (the plugin lists it while the build runs), its name and menu entry its own
    let mut r = roll::draw(&cands(), &cands().grands, &mut Rng::new(5), "rolled_7");
    let Some((_d, repo)) = with_preview(&r) else { return };
    (r.name, r.menu.0) = ("my_roll".into(), "My roll".into()); // named after the last roll
    roll::finish(&repo, &r).unwrap();
    let out = repo.root.join("instruments/my_roll");
    let e = Export::read(&out).unwrap();
    assert_eq!(e.json["profile"], "rolled:my_roll");
    assert_eq!(e.json["menu"]["name"], "My roll");
    assert_eq!(e.json["menu"]["group"], "Rolled pianos");
    assert!(std::fs::read_to_string(out.join("LICENSE.md")).unwrap().starts_with("# License: My roll (rolled)"));
    assert!(out.join("level.json").exists());
    assert!(!out.join("recipe.toml").exists());
    assert!(!repo.root.join("instruments/rolled_preview").exists());
}

#[test]
fn finish_leaves_a_stale_preview_to_the_build() {
    // a change the preview does not play yet (its roll running): the build writes the export
    let mut r = roll::draw(&cands(), &cands().grands, &mut Rng::new(6), "my_roll");
    let Some((_d, repo)) = with_preview(&r) else { return };
    r.warmth += 0.25;
    roll::finish(&repo, &r).unwrap();
    assert!(repo.projects.join("my_roll/project.toml").exists());
    assert!(!repo.root.join("instruments/my_roll").exists());
}

#[test]
fn an_existing_export_is_a_taken_name() {
    let r = roll::draw(&cands(), &cands().grands, &mut Rng::new(8), "rolled_preview");
    let Some((_d, repo)) = with_preview(&r) else { return };
    assert!(roll::finish(&repo, &r).is_err(), "the preview's own folder");
    assert!(!repo.projects.join("rolled_preview").exists());
}

#[test]
fn a_pulled_back_draw_finishes_from_its_preview() {
    // the sliders take the knobs a draw was built at; the preview's recipe says the same, so Finish
    // writes its export (a real roll: the references measured, the draw pulled back)
    let real = Repo::from_env();
    if !real.root.join("instruments/grand_a/data.bin").exists() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("instruments")).unwrap();
    for n in cands().references {
        std::os::unix::fs::symlink(real.root.join("instruments").join(&n), d.path().join("instruments").join(&n)).unwrap();
    }
    let repo = Repo { root: d.path().to_path_buf(), projects: d.path().join("projects"), data: real.data.clone() };
    let mut r = roll::draw(&cands(), &cands().grands, &mut Rng::new(9), "my_roll");
    r.warmth = 1.0; // far enough to be pulled back (2.5 passed once Upright D widened the ranges)
    // the minor knobs at the middle: what they mean follows the references' bodies, and after their
    // refit seed 9's (0.06, −1.54) rang on past the range at any warmth, which no pull-back mends
    r.shade.iter_mut().for_each(|v| *v = 0.0);
    let report = roll::preview(&repo, &r, roll::begin_preview(), true).unwrap();
    assert!(report.pulled_back > 0);
    roll::adopt_knobs(&mut r, &report.knobs);
    roll::finish(&repo, &r).unwrap();
    // what the build writes next: the same files (the plugin's reload changes nothing heard)
    let written = Export::read(&repo.root.join("instruments/my_roll")).unwrap();
    let (built, _, license) = pianoengine::roll::generate::generate(&r, &repo.root.join("instruments")).unwrap();
    assert_eq!(written.json, built.json);
    assert!(written.data == built.data, "data.bin differs");
    assert_eq!(std::fs::read_to_string(repo.root.join("instruments/my_roll/LICENSE.md")).unwrap(), license);
}
