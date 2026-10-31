//! projects/kinds.toml: the wizard's answers and their donors.
use pe_editor::project::{self, Repo};

#[test]
fn every_donor_is_a_one_part_project_of_a_family_its_answer_shares() {
    let repo = Repo::from_env();
    let kinds = project::kinds(&repo);
    assert_eq!(kinds.len(), 10, "{:?}", kinds.iter().map(|k| &k.id).collect::<Vec<_>>());
    for k in &kinds {
        assert!(k.donors.contains(&k.fallback), "{}: the fallback is a donor", k.id);
        let fams: std::collections::BTreeSet<String> = k.donors.iter().map(|d| project::family(&repo, d).unwrap()).collect();
        assert!(k.donors.iter().all(|d| project::templates(&repo).iter().any(|t| &t.name == d)), "{}: one part each", k.id);
        assert_eq!(k.bar_like, fams.contains("bars"), "{}: bar_like answers are the bars family's ({fams:?})", k.id);
        if k.bar_like {
            assert_eq!(fams.len(), 1, "{}: a bars answer has only bars donors", k.id);
        }
    }
    let all: Vec<&String> = kinds.iter().flat_map(|k| &k.donors).collect();
    assert!(all.iter().all(|d| all.iter().filter(|x| x == &d).count() == 1), "each donor under one answer");
}

#[test]
fn a_missing_donor_is_left_out() {
    let real = Repo::from_env();
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("projects")).unwrap();
    std::fs::copy(real.projects.join("kinds.toml"), d.path().join("projects/kinds.toml")).unwrap();
    for n in ["bells", "harp"] {
        std::fs::create_dir_all(d.path().join("projects").join(n)).unwrap();
        std::fs::copy(real.projects.join(n).join("project.toml"), d.path().join("projects").join(n).join("project.toml")).unwrap();
    }
    let repo = Repo { root: real.root.clone(), projects: d.path().join("projects"), data: real.data.clone() };
    let kinds = project::kinds(&repo);
    let ids: Vec<&str> = kinds.iter().map(|k| k.id.as_str()).collect();
    assert_eq!(ids, ["harp", "mallet"], "answers without a donor present are not shown");
    assert_eq!(kinds[1].donors, ["bells"]);
}
