//! A new project from one template per family: the registry's checks (proto/tests) pass on it
//! and the driver lists every step as never run.
use pe_editor::project::{self, Repo, project_names};
use std::collections::BTreeMap;
use std::process::Command;

#[test]
fn a_new_project_from_each_family_passes_the_registry_checks() {
    let real = Repo::from_env();
    let d = tempfile::tempdir().unwrap();
    for n in project_names(&real) {
        std::fs::create_dir_all(d.path().join("projects").join(&n)).unwrap();
        std::fs::copy(real.projects.join(&n).join("project.toml"), d.path().join("projects").join(&n).join("project.toml")).unwrap();
    }
    std::fs::copy(real.projects.join("steps.toml"), d.path().join("projects/steps.toml")).unwrap();
    let repo = Repo { root: real.root.clone(), projects: d.path().join("projects"), data: real.data.clone() };

    let mut by_family = BTreeMap::new();
    for t in project::templates(&repo) {
        by_family.entry(t.family.clone()).or_insert(t.name.clone());
    }
    assert!(by_family.len() >= 5, "{by_family:?}");
    for (family, template) in &by_family {
        let name = format!("zz_new_{family}");
        let p = project::new_from_template(&repo, &name, template).unwrap();
        assert_eq!(p.part_count(), 1);
        assert!(p.source_ids().is_empty());
        assert_eq!(p.get(&project::Table::top("project"), "family").unwrap(), serde_json::json!(family));
        assert!(project::new_from_template(&repo, &name, template).is_err(), "the name is taken now");
        assert!(p.text().starts_with(&format!("# {name}: made from {template}")));
    }
    let st = Command::new(project::python())
        .args(["-m", "pytest", "-q", "proto/tests/test_registry.py"])
        .env("PE_PROJECTS", &repo.projects)
        .current_dir(&repo.root)
        .status()
        .unwrap();
    assert!(st.success(), "the registry checks fail on the new projects");
    for family in by_family.keys() {
        let name = format!("zz_new_{family}");
        let out = Command::new(project::python())
            .arg(repo.root.join("proto/build.py"))
            .args(["status", "--json", &name])
            .env("PE_PROJECTS", &repo.projects)
            .current_dir(&repo.root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
        let s: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let rows = s["rows"].as_array().unwrap();
        assert!(!rows.is_empty(), "{name}");
        assert!(rows.iter().all(|r| r["reason"] == "never run"), "{name}: {rows:?}");
    }
}
