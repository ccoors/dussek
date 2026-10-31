//! A rolled piano's LICENSE.md: its sources' licenses under their names, and its own: CC BY 4.0, or
//! CC BY-SA 4.0 when any source is ShareAlike (an adaptation keeps that license).

use dussek::roll::generate::license;
use dussek::roll::recipe::Recipe;
use std::path::PathBuf;

const FIXTURE: &str = include_str!("fixtures/rolled/project.toml");

/// an instruments folder with one LICENSE.md per (name, license)
fn instruments(tag: &str, sources: &[(&str, &str)]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pe-roll-license-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for (name, lic) in sources {
        std::fs::create_dir_all(d.join(name)).unwrap();
        std::fs::write(d.join(name).join("LICENSE.md"), format!("# License: {name}\n\nLicensed under ({lic}, …).\n")).unwrap();
    }
    d
}

fn recipe(strings: &[&str]) -> Recipe {
    let mut r = Recipe::parse(FIXTURE, "rolled_test").unwrap();
    r.strings = strings.iter().map(|s| (s.to_string(), 1.0 / strings.len() as f64)).collect();
    r.fine_structure = vec![strings[0].to_string()];
    r.references = vec![];
    r
}

#[test]
fn a_roll_of_attribution_pianos_is_cc_by() {
    let d = instruments("by", &[("a", "CC BY 4.0"), ("b", "CC BY 4.0")]);
    let text = license(&recipe(&["a", "b"]), &d);
    assert!(text.contains("licensed under the Creative Commons Attribution 4.0 International license (CC BY 4.0"), "{text}");
    assert!(text.contains("Music made with this instrument (renders, recordings, performances) is yours"), "{text}");
    assert!(text.contains("## a") && text.contains("## b"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_sharealike_source_makes_the_roll_sharealike() {
    let d = instruments("sa", &[("a", "CC BY 4.0"), ("b", "CC BY-SA 4.0")]);
    let text = license(&recipe(&["a", "b"]), &d);
    assert!(text.contains("(CC BY-SA 4.0, https://creativecommons.org/licenses/by-sa/4.0/)"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}
