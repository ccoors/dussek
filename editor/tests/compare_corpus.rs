//! Every committed instrument against its own recordings (those whose set is in data/): each
//! measure's distribution re each source's medians printed (the thresholds were set from it,
//! 2026-10-15), and each instrument's flagged pairs held under a recorded ceiling. Renders and measurements are cached
//! in target/compare-cache (minutes the first time).
use pe_editor::compare::{self, Run};
use pe_editor::project::{self, Project, Repo};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize};

/// flagged pairs per instrument, 2026-10-15, the length at ×2, the levels by their groups (an
/// instrument measuring worse fails the test)
const CEILINGS: &[(&str, usize)] = &[
    // its set noise-reduced and high-passed below 100–150 Hz (the bass's fundamentals gone), its files
    // ending after ~4 s: early decays and lengths read past what the recordings hold (step 196)
    ("harpsichord_e", 23),
    ("balafon", 0),
    ("harpsichord_f", 4),
    ("bells", 7),
    ("chimes", 1),
    ("clavichord", 15),
    ("crotales", 3),
    ("dantranh", 18),
    ("flemish", 1),
    ("french", 0),
    ("glockenspiel", 1),
    ("glockenspiel_b", 1),
    // 21 before its unisons widened toward the top key (step 183): its top keys at mf 1–2 dB quieter
    // re the recording, over the −12 dB line
    ("grand_f", 23),
    ("guitar", 20),
    ("handchimes", 2),
    ("harp", 5),
    ("harpsichord", 0),
    ("grand_c", 12),
    // 44 before its aftersound's level came to vary along the keyboard (step 127)
    // 45 before the grands were voiced even (step 139): a key's level is the curve's, not its
    // recording's
    ("grand_b", 46),
    ("italian", 0),
    ("kalimba", 9),
    ("upright_a", 13),
    ("leverharp", 6),
    // 2 before its room was cut 15–27 dB above 1 kHz (design log step 123): three early decays
    ("upright_c", 4),
    // its own session (step 204): A0's early decay ×0.1–0.2 at every dynamic, F♯2 pp's and C3's lengths
    // ×0.1–0.4, the rest at the length's edges (its renders a median ×1.74 its recordings' length:
    // the uprights' ring by ear, 150–200 %)
    ("upright_d", 17),
    ("marimba", 5),
    ("mavembe", 3),
    ("mbira", 4),
    ("nyunga", 3),
    // 4 before its aftersound's level came to vary along the keyboard (step 127): its renders ring a
    // median ×1.41 its recordings' length, G#7 and C8 short re that, B5's early decay
    ("grand_g", 6),
    // 48 before its knock became the hammer's thump under the recordings (step 126): soft treble peaks
    // 50 before its felt was solved with pp, mf and ff (step 142): its levels and brightness moved;
    // 51 before its treble's upper partials were trimmed (step 143): D#6 mf at length ×0.44 (its time
    // to −40 dB unchanged, 0.48 s)
    // 52 before its unisons widened toward the top key (step 183): its top keys at mf 1–2 dB quieter
    // re the recording (93–108), the rest pairs at the length's and the levels' edges as its medians moved
    ("grand_d", 58),
    ("psaltery", 0),
    // 12 before the board's low modes rang (step 184): C2's early decay ×0.2 (its board's 59 Hz mode
    // under it, −36 dB re the fundamental over 1 s, was −55), F♯3 mf's pitch read +25 cents (the
    // engine's partials 1–3 unmoved, −10.5 / −3.8 / −6.9 cents re equal)
    ("grand_a", 13),
    ("grand_e", 15),
    // 2026-10-31 (design log step 208): its files normalized (levels not judged) and cut while
    // still sounding (94 of 264 lengths stopped): lengths (G♯1 pp, F♯3 ff, C♯6 mf and ff), pitches
    // at the top keys (±26–30 cents); 11 before its bass by ear (+4 dB of the Bass control's shape):
    // C♯1 pp's pitch read −77 cents (its spectrum +14.4 dB rms), G♯1 mf's length ×0.49
    ("grand_h", 13),
    // 14 at its first export (step 209: C4 pp and mf +12 dB re the medians, its recording 5 dB under its
    // neighbors); with the felt solved on the attack its soft treble quieter: A6–D♯7 pp −13 … −18 dB
    ("grand_i", 16),
    ("strumstick", 9),
    // 2 before its knock became the hammer's thump (step 126): C7 at mf and G7 at pp 12 and 19 dB under
    // the recordings, the knock had carried their peaks; 3 before its ring and room by ear (steps
    // 154–155: 150 % and an assumed living room): B♭0's early decay ×0.24 of its recording's at pp and ff
    ("upright_b", 4),
    ("vibraphone", 4),
    ("xylophone", 34),
];

fn pct(v: &mut Vec<f32>, q: f32) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * q).round() as usize]
}

/// an instrument's pairs and their differences; a pair that failed, or the run, fails the test (a
/// regression that breaks renders would otherwise lower the flagged counts); none: no pairs or no
/// export (a project without a fit of its own, a set not in data/)
fn compare_all(repo: &Repo, name: &str, export: &std::path::Path) -> Option<(Vec<compare::Pair>, Vec<(usize, compare::Diff)>)> {
    let p = Project::load(repo, name).ok()?;
    let pairs = compare::pairs(repo, &p).list;
    if pairs.is_empty() || !export.join("instrument.json").is_file() {
        return None;
    }
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/compare-cache").join(format!("{name}-{}.json",
        export.file_name().unwrap().to_string_lossy()));
    let (pr, re, ca) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicBool::new(false));
    let (got, failed) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
    let threads = compare::threads();
    compare::run(&Run { pairs: &pairs, export, cache: &cache, threads, progress: &pr, rendered: &re, cancel: &ca },
        &|i, r| match r {
            Ok(e) => got.lock().unwrap().push((i, compare::diff(&e.recording, &e.render))),
            Err(e) => failed.lock().unwrap().push(format!("{}: {e}", pairs[i].path.display())),
        }).unwrap_or_else(|e| panic!("{name}: the run failed: {e}"));
    let failed = failed.into_inner().unwrap();
    assert!(failed.is_empty(), "{name}: {} pairs not compared:\n{}", failed.len(), failed.join("\n"));
    Some((pairs, got.into_inner().unwrap()))
}

#[test]
fn the_committed_instruments_hold_up() {
    let repo = Repo::from_env();
    let (mut rows, mut over) = (Vec::new(), Vec::new());
    let (mut lv, mut pc, mut er, mut lt, mut sp) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for name in project::project_names(&repo) {
        let Some((pairs, diffs)) = compare_all(&repo, &name, &compare::export_dir(&repo, &name)) else { continue };
        // each source (a mallet, a register) against its own medians
        // each source's level groups as the page reads them (a run's gain, a normalized file's level not judged)
        let p = Project::load(&repo, &name).unwrap();
        let groups = p.source_ids().into_iter().filter_map(|id| p.source_json(&id).map(|j| (id, compare::level_groups_of(&j)))).collect();
        let ms = compare::medians_grouped(&diffs.iter().map(|(i, d)| (&pairs[*i], d)).collect::<Vec<_>>(), &groups);
        // each measure re the instrument's medians, as the flags read it
        let (mut l, mut p, mut e, mut t, mut s) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (i, d) in &diffs {
            let m = &ms[&pairs[*i].source];
            l.extend(m.level_ref(&pairs[*i].dynamic, pairs[*i].strike.key as i64).map(|r| (d.level_db - r).abs()));
            p.extend(d.pitch_cents.map(|c| (c - m.pitch).abs()));
            e.extend(d.early.map(|r| (r / m.early).ln().abs().exp()));
            // a stopped recording's length is a lower bound: left out, as of the medians
            t.extend(d.length.filter(|_| !d.length_stopped).map(|r| (r / m.length).ln().abs().exp()));
            s.push(d.spectrum_db - m.spectrum);
        }
        let offs = |i: usize, d: &compare::Diff| compare::offs_at(d, &pairs[i], &ms[&pairs[i].source]);
        let flagged: Vec<String> = diffs.iter().filter(|(i, d)| !offs(*i, d).is_empty())
            .map(|(i, d)| format!("{} {} {} ({})", pairs[*i].note, pairs[*i].dynamic, pairs[*i].source,
                offs(*i, d).iter().map(|o| o.text()).collect::<Vec<_>>().join(", "))).collect();
        let medians = ms.iter().map(|(src, m)| format!("{src}: level {}, pitch {:+.0} cents, early ×{:.2}, length ×{:.2}, spectrum {:.1} dB",
            m.level.iter().map(|(k, v)| format!("{k} {v:+.1}")).collect::<Vec<_>>().join(" "), m.pitch, m.early, m.length, m.spectrum)).collect::<Vec<_>>().join("; ");
        let stopped = diffs.iter().filter(|(_, d)| d.length_stopped).count();
        let by_length = diffs.iter().filter(|(i, d)| offs(*i, d).iter().any(|o| matches!(o, compare::Off::Length(_)))).count();
        rows.push(format!("{name}: {} pairs, {stopped} stopped; medians: {medians} | re them p99: level {:.1}, pitch {:.0}, early ×{:.2}, length ×{:.2}, spectrum {:+.1} | flagged {} ({by_length} by length)",
            diffs.len(),
            pct(&mut l.clone(), 0.99), pct(&mut p.clone(), 0.99), pct(&mut e.clone(), 0.99), pct(&mut t.clone(), 0.99), pct(&mut s.clone(), 0.99), flagged.len()));
        for f in flagged.iter().take(8) {
            rows.push(format!("    {f}"));
        }
        lv.extend(l); pc.extend(p); er.extend(e); lt.extend(t); sp.extend(s);
        match CEILINGS.iter().find(|c| c.0 == name) {
            Some((_, c)) if flagged.len() > *c => over.push(format!("{name}: {} flagged, ceiling {c}", flagged.len())),
            None => over.push(format!("(\"{name}\", {}),", flagged.len())),
            _ => {}
        }
    }
    eprintln!("{}", rows.join("\n"));
    eprintln!("all, re the medians: level p99 {:.1} dB, pitch p99 {:.0} cents, early p99 ×{:.2}, length p95 ×{:.2} p99 ×{:.2} (not stopped), spectrum p99 {:+.1} dB",
        pct(&mut lv, 0.99), pct(&mut pc, 0.99), pct(&mut er, 0.99), pct(&mut lt.clone(), 0.95), pct(&mut lt, 0.99), pct(&mut sp, 0.99));
    assert!(over.is_empty(), "over a ceiling, or none recorded:\n{}", over.join("\n"));
}
