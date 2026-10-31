//! `info::info` on the committed exports and on files edited from them (skipped without them).
use pianoengine::info::{info, Control, Exciter};
use serde_json::Value;
use std::path::PathBuf;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");

/// `instruments/<name>/instrument.json`, edited by `f`, written to a fresh directory (no data.bin: info reads none)
fn edited(name: &str, tag: &str, f: impl FnOnce(&mut Value)) -> Option<PathBuf> {
    let json = std::fs::read_to_string(format!("{ROOT}{name}/instrument.json")).ok()?;
    let mut v: Value = serde_json::from_str(&json).unwrap();
    f(&mut v);
    let dir = std::env::temp_dir().join(format!("pe-info-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("instrument.json"), serde_json::to_string(&v).unwrap()).unwrap();
    Some(dir)
}

#[test]
fn the_exciters() {
    for (name, want) in [
        ("grand_a", Exciter::Hammer),
        ("harpsichord", Exciter::Quill),
        ("harp", Exciter::Finger),
        ("guitar", Exciter::Finger),
        ("psaltery", Exciter::Pick),
        ("clavichord", Exciter::Tangent),
        ("marimba", Exciter::Mallet),
        ("chimes", Exciter::Mallet),
        ("kalimba", Exciter::Thumb),
    ] {
        if let Ok(i) = info(format!("{ROOT}{name}")) {
            assert_eq!(i.exciter, want, "{name}");
        }
    }
}

#[test]
fn the_mallets_and_the_blocks() {
    let Ok(m) = info(format!("{ROOT}marimba")) else { return };
    assert_eq!(m.mallets, ["yarn", "cord", "rubber"]);
    assert!(!m.dampers && m.bars && !m.lid && !m.buff && !m.motor);
    let Ok(f) = info(format!("{ROOT}flemish")) else { return };
    assert!(f.registers > 1 && f.buff && f.lute);
    let Ok(v) = info(format!("{ROOT}vibraphone")) else { return };
    assert!(v.motor && v.dampers);
    let Ok(d) = info(format!("{ROOT}dantranh")) else { return };
    assert!(d.bend && !d.lid);
}

#[test]
fn a_menu_block_is_read() {
    let Some(dir) = edited("harp", "menu", |v| {
        v["menu"] = serde_json::json!({"name": "Harp C", "group": "Harps", "mallets": ["A"],
            "labels": {"knock": {"name": "Finger noise", "description": "d"}, "no_such_control": {"name": "x"}}});
    }) else {
        return;
    };
    let m = info(&dir).unwrap().menu.unwrap();
    assert_eq!((m.name.as_str(), m.group.as_str(), m.hidden, m.reversed_keys), ("Harp C", "Harps", false, false));
    assert_eq!(m.mallets, ["A"]);
    // labels for any symbol are kept (the plugin ignores the ones it has no parameter for)
    assert_eq!(m.labels.len(), 2);
    assert_eq!(m.labels.iter().find(|(s, _)| s == "knock").unwrap().1.description.as_deref(), Some("d"));
}

#[test]
fn reversed_keys_are_read() {
    let Some(dir) = edited("harp", "reversed", |v| {
        v["menu"] = serde_json::json!({"name": "Harp C", "group": "Harps", "reversed_keys": true});
    }) else {
        return;
    };
    assert!(info(&dir).unwrap().menu.unwrap().reversed_keys);
}

#[test]
fn menu_types_are_checked() {
    let Some(dir) = edited("harp", "badmenu", |v| v["menu"] = serde_json::json!({"name": 3, "group": "Harps"})) else { return };
    assert!(info(&dir).is_err());
}

#[test]
fn no_menu_block_is_none() {
    let Some(dir) = edited("harp", "nomenu", |v| {
        v.as_object_mut().unwrap().remove("menu");
    }) else {
        return;
    };
    assert!(info(&dir).unwrap().menu.is_none());
}

#[test]
fn a_hammer_note_offers_the_pluck_and_the_grooves() {
    let Ok(s) = info(format!("{ROOT}grand_a")) else { return };
    assert_eq!(s.preparations, [1, 2, 3, 4, 5]);
    assert!(!s.is_inert(Control::HammerGrooves) && s.inert == 0 && s.soft_pedal);
}

#[test]
fn a_wrong_format_is_an_error() {
    let Some(dir) = edited("harp", "format", |v| v["format"] = 99.into()) else { return };
    assert!(info(&dir).is_err());
}

#[test]
fn the_report_names_the_morph_family() {
    let a = format!("{ROOT}grand_a");
    let Ok(entries) = std::fs::read_dir(ROOT) else { return };
    let mut seen = (0, 0);
    for dir in entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("instrument.json").exists()) {
        let family = info(&dir).unwrap().morph_family;
        let morphs = pianoengine::Instrument::morph(&[(dir.clone(), 0.5), (PathBuf::from(&a), 0.5)]).is_ok();
        assert_eq!(family == 1, morphs, "{}", dir.display());
        if family == 1 { seen.0 += 1 } else { seen.1 += 1 }
    }
    assert!(seen.0 >= 10 && seen.1 >= 10, "{seen:?}");
    assert_eq!(info(format!("{ROOT}upright_a")).unwrap().morph_family, 1);
    assert_eq!(info(format!("{ROOT}flemish")).unwrap().morph_family, 0);
    assert_eq!(info(format!("{ROOT}marimba")).unwrap().morph_family, 0);
}
