//! The take checks on every committed source whose set is in data/: each check's scores per
//! source printed, the known flawed takes flagged at the defaults, each source's flags per hundred
//! takes held under a recorded ceiling (an instrument flagging more fails the test). Measurements
//! are cached in target/takes-cache (minutes the first time).
use pe_editor::{audio, mapping, project, suggest, takes};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

/// flags per hundred takes at the defaults, per source (2026-10-16: decay 4.5, rise 4.5, brightness 4.0,
/// agreed with the user after the late rise came to count only a climb; the harp, the strumstick
/// and Upright Y raised when the rise came to read nothing below −90 dBFS: their blips out of
/// digital silence had widened the spread their rises are scored in)
const CEILINGS: &[(&str, f32)] = &[
    ("harpsichord_e_lower", 5.0),
    ("harpsichord_e_upper", 0.0),
    ("balafon_hard", 0.0),
    ("balafon_soft", 0.0),
    ("balafon_trad", 0.0),
    ("harpsichord_f_left", 0.0),
    ("harpsichord_f_right", 0.0),
    ("bells_brass", 7.4),
    ("bells_plastic", 3.3),
    ("chimes", 0.0),
    ("chimes_check", 5.6),
    ("clavichord", 2.5),
    ("crotales", 0.0),
    ("dantranh", 2.1),
    ("english", 7.7),
    ("english_buff", 0.0),
    ("flemish4", 0.0),
    ("flemish8", 7.2),
    ("french", 3.6),
    ("glockenspiel", 0.0),
    ("glockenspiel_b", 5.6),
    ("grand_f", 4.4),
    ("guitar", 8.4),
    ("handchimes", 10.6),
    ("harp", 4.8),
    ("harpsichord", 3.3),
    ("grand_c", 1.2),
    ("grand_b", 2.7),
    ("italian", 6.3),
    ("kalimba", 11.2),
    ("upright_a", 0.0),
    ("leverharp", 0.0),
    ("upright_c", 0.0),
    ("upright_d", 2.0),
    ("marimba_cord", 0.6),
    ("marimba_rubber", 3.3),
    ("marimba_yarn", 1.7),
    ("mavembe", 5.0),
    ("mbira", 0.0),
    ("nyunga", 0.0),
    ("grand_g", 0.0),
    ("grand_d", 0.4),
    ("psaltery", 9.1),
    ("grand_a", 8.2),
    ("grand_e", 2.4),
    ("grand_h", 2.0),
    ("grand_i", 1.2),
    ("strumstick", 17.6),
    ("upright_b", 5.0),
    ("vibraphone", 2.4),
    ("vibraphone_damp", 0.8),
    ("xylophone_hardrubber", 0.8),
    ("xylophone_rosewood", 0.8),
];

/// the known flawed takes (source, relpath): flagged at the defaults (Grand A's F#2 v1 and v2 were
/// caught only through a fast-then-slow decay read as a rise, which the check no longer does: left
/// out by ear instead)
const KNOWN: &[(&str, &str)] = &[
    ("guitar", "notes/guitar.s5/pp/49.wav"),
    ("grand_a", "F#7v1.wav"),
];
/// Bells A#6 ff (plastic): found by key and dynamic (its file name is the split run's)
const KNOWN_KEY: &[(&str, i64, &str)] = &[("bells_plastic", 94, "ff")];

fn pct(v: &mut [f32], q: f32) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * q).round() as usize]
}

#[test]
fn the_takes_of_the_committed_sources_hold_up() {
    let repo = project::Repo::from_env();
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/takes-cache");
    std::fs::create_dir_all(&cache).unwrap();
    let mut rows = Vec::new();
    let mut over = Vec::new();
    let mut missed = Vec::new();
    let mut sources = project::all_sources(&repo);
    sources.sort_by(|a, b| a.1["id"].as_str().cmp(&b.1["id"].as_str()));
    for (_, src) in sources {
        let id = src["id"].as_str().unwrap_or_default().to_string();
        let root = suggest::data_root(&repo.data, src["root"].as_str().unwrap_or_default());
        if !std::path::Path::new(&root).is_dir() {
            continue;
        }
        let Ok(rep) = mapping::items(&src, &root) else { continue };
        if rep.items.is_empty() {
            continue;
        }
        let paths: Vec<PathBuf> = rep.items.iter().filter_map(|i| i["path"].as_str().map(PathBuf::from)).collect();
        let m = audio::measure_all(&paths, &cache.join(format!("{id}.json")), &AtomicUsize::new(0));
        let offsets = suggest::pitch_offsets(&rep.items, &m);
        let none = Default::default();
        let scored = takes::score(&takes::inputs(&rep.items, &none, &m, &offsets));
        let flags = takes::flags(&scored, &takes::Thresholds::DEFAULT);
        let n = rep.items.len();
        let mut line = format!("{id:24} {n:4} takes");
        for c in [takes::Check::Decay, takes::Check::Rise, takes::Check::Brightness] {
            let mut s: Vec<f32> = scored.values().flatten().filter(|x| x.check == c).map(|x| x.score).collect();
            let k = flags.values().filter(|v| v.iter().any(|x| x.check == c)).count();
            // the flags at other thresholds (to calibrate the defaults)
            let at: Vec<String> = [3.0f32, 3.5, 4.5, 5.0].iter().map(|t| format!("{}", s.iter().filter(|x| **x > *t).count())).collect();
            line += &format!(" | {c:?} p90 {:4.1} p99 {:4.1} flags {k:3} (3/3.5/4.5/5: {})", pct(&mut s.clone(), 0.9), pct(&mut s, 0.99), at.join("/"));
        }
        // a late rise flagged at the window's start (1–1.2 s) was a fast-then-slow decay, not a rise
        let early = flags.values().flatten().filter(|x| x.check == takes::Check::Rise && x.at_s.is_some_and(|t| t < 1.2)).count();
        line += &format!(" | rise flags before 1.2 s {early}");
        let per100 = 100.0 * flags.len() as f32 / n as f32;
        line += &format!(" | flagged {:3} = {per100:4.1} per 100", flags.len());
        rows.push(line);
        let known: Vec<String> = KNOWN.iter().filter(|(s, _)| *s == id).map(|(_, r)| r.to_string())
            .chain(KNOWN_KEY.iter().filter(|(s, _, _)| *s == id).filter_map(|(_, k, d)| rep.items.iter()
                .find(|i| i["note"].as_i64() == Some(*k) && i["dynamic"].as_str() == Some(*d)))
                .filter_map(|i| i["relpath"].as_str().map(String::from)))
            .collect();
        for r in known {
            let s = scored.get(&r).map(|v| v.iter().map(|x| format!("{:?} {:.1}", x.check, x.score)).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            rows.push(format!("    known {r}: {s}{}", if flags.contains_key(&r) { "  FLAGGED" } else { "" }));
            if !flags.contains_key(&r) {
                missed.push(format!("{id} {r}"));
            }
        }
        match CEILINGS.iter().find(|c| c.0 == id) {
            Some((_, c)) if per100 > *c + 1e-3 => over.push(format!("{id}: {per100:.1} per 100, ceiling {c}")),
            None => over.push(format!("(\"{id}\", {:.1}),", per100.ceil())),
            _ => {}
        }
    }
    eprintln!("{}", rows.join("\n"));
    assert!(missed.is_empty(), "known flawed takes not flagged at the defaults:\n{}", missed.join("\n"));
    assert!(over.is_empty(), "over a ceiling, or none recorded:\n{}", over.join("\n"));
}
