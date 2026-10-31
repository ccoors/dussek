//! The Rust mapper against analysis/mapping.py on every source whose data is present: the
//! same items, field for field, in the same order (or an error where Python's is one).
use pe_editor::{mapping, project};
use serde_json::Value;
use std::process::Command;

#[test]
fn rust_items_equal_python_items_on_every_source_with_data() {
    let repo = project::Repo::from_env();
    let out = Command::new(project::python())
        .arg(repo.root.join("analysis/mapping.py"))
        .arg("--dump")
        .current_dir(&repo.root)
        .output()
        .expect("python3 runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let sources: std::collections::HashMap<String, Value> = project::all_sources(&repo)
        .into_iter()
        .map(|(_, s)| (s["id"].as_str().unwrap().to_string(), s))
        .collect();
    let mut checked = 0;
    for line in String::from_utf8(out.stdout).unwrap().lines() {
        let want: Value = serde_json::from_str(line).unwrap();
        let id = want["id"].as_str().unwrap();
        let root = want["root"].as_str().unwrap();
        let got = mapping::items(&sources[id], root);
        if let Some(e) = want.get("error") {
            assert!(got.is_err(), "{id}: Python raised {e}, Rust gave items");
            continue;
        }
        let got = got.unwrap_or_else(|e| panic!("{id}: {e}")).items;
        let want = want["items"].as_array().unwrap();
        assert_eq!(got.len(), want.len(), "{id}: item count");
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            let w = w.as_object().unwrap();
            assert_eq!(g.keys().collect::<Vec<_>>(), w.keys().collect::<Vec<_>>(), "{id} item {i}: keys");
            assert_eq!(&Value::Object(g.clone()), &Value::Object(w.clone()), "{id} item {i}");
        }
        assert!(mapping::dynamics_problems(&sources[id], &got).is_empty(), "{id}: {:?}", mapping::dynamics_problems(&sources[id], &got));
        checked += 1;
    }
    eprintln!("{checked} sources compared");
    if repo.data.join("vcsl").is_dir() {
        assert!(checked >= 20, "only {checked} sources had data");
    }
}
