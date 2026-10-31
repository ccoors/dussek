//! The wizard's guess on synthetic measurements: the keys it reads, the answer, the donor.
use pe_editor::audio::{Measure, Measures, midi_hz};
use pe_editor::kind;
use pe_editor::mapping::Item;
use pe_editor::project::{self, Repo};
use serde_json::json;

fn take(m: &mut Measures, path: &str, f1: f32, ratios: &[f32]) {
    let mut peaks: Vec<[f32; 2]> = ratios.iter().enumerate().map(|(i, r)| [f1 * r, -6.0 * i as f32]).collect();
    peaks.sort_by(|a, b| b[1].total_cmp(&a[1]));
    m.ok.insert(path.into(), Measure { onset_s: 0.0, level_db: -10.0, nyquist: 24000.0, peaks, decays: vec![], env_db: vec![], bands_db: vec![] });
}

fn set(notes: &[i64], ratios: &[f32], cents: f32) -> (Vec<Item>, Measures) {
    let mut m = Measures::default();
    let mut items = Vec::new();
    for &n in notes {
        for (layer, d) in [(1, "p"), (2, "f")] {
            let path = format!("/s/{n}_{d}.wav");
            take(&mut m, &path, midi_hz(n as f64) * 2f32.powf(cents / 1200.0), ratios);
            items.push(json!({"path": path, "note": n, "dynamic": d, "layer": layer, "layers": 2}).as_object().unwrap().clone());
        }
    }
    (items, m)
}

#[test]
fn one_file_per_key_the_mf_layer() {
    let (items, m) = set(&[72, 74, 76], &[1.0, 2.0, 3.0], 0.0);
    let keys = kind::sample_keys(&items, &m, Some(&json!("f")));
    assert_eq!(keys.len(), 3);
    assert!(keys.iter().all(|k| (k.f1 / midi_hz(k.note as f64) - 1.0).abs() < 0.01));
}

#[test]
fn a_bars_overtones_preselect_a_bar_answer() {
    let repo = Repo::from_env();
    let (items, m) = set(&[79, 81, 84, 86, 89, 91], &[1.0, 2.74, 5.30], 0.0);
    let keys = kind::sample_keys(&items, &m, Some(&json!("f")));
    assert!(kind::bar_share(&keys) > 0.66);
    let g = kind::preselect(&repo, &project::kinds(&repo), &keys);
    let k = g.kind.expect("a bar's overtones pre-select an answer");
    assert!(project::kinds(&repo).iter().any(|x| x.id == k && x.bar_like), "{k}");
    assert!(!g.why.is_empty());
}

#[test]
fn a_strings_overtones_preselect_nothing() {
    let repo = Repo::from_env();
    let (items, m) = set(&[48, 52, 55, 60, 64, 67], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 0.0);
    let keys = kind::sample_keys(&items, &m, None);
    assert!(kind::bar_share(&keys) < 0.34);
    assert_eq!(kind::preselect(&repo, &project::kinds(&repo), &keys).kind, None);
}

#[test]
fn a_set_tuned_low_still_finds_its_fundamentals() {
    // a = 415 Hz: every file 101 cents under its key
    let (items, m) = set(&[79, 81, 84, 86], &[1.0, 2.74, 5.30], -101.0);
    let keys = kind::sample_keys(&items, &m, Some(&json!("p")));
    assert_eq!(keys.len(), 4);
    assert!(kind::bar_share(&keys) > 0.66);
}

#[test]
fn too_few_keys_guess_nothing() {
    let repo = Repo::from_env();
    let (items, m) = set(&[79, 81], &[1.0, 2.74, 5.30], 0.0);
    let keys = kind::sample_keys(&items, &m, None);
    let kinds = project::kinds(&repo);
    assert_eq!(kind::preselect(&repo, &kinds, &keys).kind, None);
    let mallet = kinds.iter().find(|k| k.bar_like).unwrap();
    assert_eq!(kind::nearest_donor(&repo, mallet, &keys).0, mallet.fallback);
}

#[test]
fn the_donor_with_the_closest_overtones() {
    // a donor scored by its own tracks: a set with exactly a donor's nominal ratios picks it
    let repo = Repo::from_env();
    let kinds = project::kinds(&repo);
    let k = kinds.iter().find(|k| k.bar_like && k.donors.len() > 2).unwrap();
    for d in &k.donors {
        let (tracks, _) = kind::donor_tracks(&repo, d).unwrap();
        let notes = [60i64, 64, 67, 72, 76, 79];
        let ratios_at = |n: i64| -> Vec<f32> { tracks.as_array().unwrap().iter().map(|t| kind::nominal(t, n)).collect() };
        let mut m = Measures::default();
        let mut items = Vec::new();
        for &n in &notes {
            let path = format!("/s/{n}.wav");
            take(&mut m, &path, midi_hz(n as f64), &ratios_at(n));
            items.push(json!({"path": path, "note": n, "dynamic": "mf", "layer": 1, "layers": 1}).as_object().unwrap().clone());
        }
        let keys = kind::sample_keys(&items, &m, None);
        let (got, why) = kind::nearest_donor(&repo, k, &keys);
        let best = kind::overtone_score(&keys, &tracks, &kind::donor_tracks(&repo, d).unwrap().1);
        assert!(best > 0.9, "{d}: its own tracks score {best}");
        let got_score = { let (t, tol) = kind::donor_tracks(&repo, &got).unwrap(); kind::overtone_score(&keys, &t, &tol) };
        assert!(got_score >= best - 0.05, "{d}: picked {got} ({why}), {got_score} < {best}");
    }
}

#[test]
fn a_wide_tolerance_is_no_closeness() {
    // a donor's tolerance of 30 % "explains" any mode near its track: the score counts a mode within
    // the kind's 8 % (the glockenspiel's 2.72 against a track at 3.0), the set's modes and the
    // donor's main one both ways
    let (items, m) = set(&[79, 81, 84, 86, 89], &[1.0, 2.72, 5.25], 0.0);
    let keys = kind::sample_keys(&items, &m, None);
    assert_eq!(kind::overtone_score(&keys, &json!([1.0, 3.0]), &json!([0.03, 0.3])), 0.0);
    assert!(kind::overtone_score(&keys, &json!([1.0, 2.7, 5.3]), &json!([0.03, 0.3, 0.3])) > 0.9);
    // a mode of the set no track explains counts against the donor
    assert!(kind::overtone_score(&keys, &json!([1.0, 2.7]), &json!([0.03, 0.3])) < 0.9);
}

#[test]
fn no_fitting_donor_preselects_the_first_bar_answer() {
    // a bar's overtones that no donor's tracks explain (every score 0): the first bar-like answer,
    // not the last
    let repo = Repo::from_env();
    let all = project::kinds(&repo);
    let bars: Vec<project::Kind> = all.iter().filter(|k| k.bar_like).cloned().collect();
    let (items, m) = set(&[79, 81, 84, 86, 89], &[1.0, 2.76, 5.40], 0.0);
    let keys = kind::sample_keys(&items, &m, None);
    let lonely: Vec<project::Kind> = bars.iter().map(|k| project::Kind { donors: vec![k.fallback.clone()], ..k.clone() }).collect();
    let none_fits = lonely.iter().all(|k| {
        let (t, tol) = kind::donor_tracks(&repo, &k.fallback).unwrap();
        kind::overtone_score(&keys, &t, &tol) == 0.0
    });
    let g = kind::preselect(&repo, &lonely, &keys);
    if none_fits {
        assert_eq!(g.kind.as_deref(), Some(lonely[0].id.as_str()));
    } else {
        assert!(g.kind.is_some());
    }
    // ties: the earlier answer
    let twice = vec![lonely[0].clone(), project::Kind { id: "zz_copy".into(), ..lonely[0].clone() }];
    assert_eq!(kind::preselect(&repo, &twice, &keys).kind.as_deref(), Some(lonely[0].id.as_str()));
}

#[test]
fn a_bar_share_under_two_thirds_is_not_called_a_string() {
    // the xylophone's 0.66: not clearly a bar's, but no string's either
    let repo = Repo::from_env();
    let (mut items, m) = set(&[79, 81, 84, 86, 89], &[1.0, 2.74, 5.30], 0.0);
    let (more, m2) = set(&[48, 52, 55], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 0.0);
    items.extend(more);
    let mut m = m;
    m.ok.extend(m2.ok);
    let keys = kind::sample_keys(&items, &m, Some(&json!("f")));
    let share = kind::bar_share(&keys);
    assert!(share > 0.5 && share < 2.0 / 3.0, "{share}");
    let g = kind::preselect(&repo, &project::kinds(&repo), &keys);
    assert_eq!(g.kind, None);
    assert!(g.why.contains("not clearly"), "{}", g.why);
}

/// a glockenspiel's keys as measured (VCSL's, at the keys they sound): the upper peaks (ratio to
/// the fundamental, dB re it) and the fundamental's decay rate
fn glockenspiel_keys(sigma: f32) -> Vec<kind::Key> {
    let rows: [(i64, f32, &[(f32, f32)]); 5] = [
        (79, 788.0, &[(3.04, -20.0), (5.69, -12.0), (8.94, -16.0)]),
        (84, 1054.0, &[(2.94, -20.0), (3.93, -37.0), (5.54, -39.0), (7.62, -26.0)]),
        (91, 1579.0, &[(2.88, -21.0), (3.76, -36.0), (6.19, -38.0)]),
        (96, 2112.0, &[(2.78, -23.0), (3.55, -37.0), (5.24, -39.0)]),
        (108, 4245.0, &[(2.68, -31.0), (3.14, -36.0)]),
    ];
    rows.iter().map(|(note, f1, ups)| {
        let mut peaks = vec![[*f1, 0.0]];
        peaks.extend(ups.iter().map(|(r, db)| [f1 * r, *db]));
        kind::Key { note: *note, f1: *f1, peaks, nyquist: 22050.0, sigma: Some(sigma) }
    }).collect()
}

fn mallet_scores(repo: &Repo, keys: &[kind::Key]) -> (String, String) {
    let kinds = project::kinds(repo);
    let mallet = kinds.iter().find(|k| k.id == "mallet").unwrap();
    let scores: Vec<String> = mallet.donors.iter().map(|d| format!("{d} {:.2}", kind::donor_score(repo, d, keys))).collect();
    (kind::nearest_donor(repo, mallet, keys).0, scores.join(", "))
}

#[test]
fn a_steel_glockenspiel_is_set_up_like_the_bells() {
    // its second mode 3.04 → 2.68 lies near the xylophone's tuned 3 as much as the bells' track: the
    // decay (a steel bar's 0.8/s) and the bells' modes the set shows decide
    let repo = Repo::from_env();
    let keys = glockenspiel_keys(0.8);
    let (got, scores) = mallet_scores(&repo, &keys);
    eprintln!("{scores}");
    assert_eq!(got, "bells", "{scores}");
    assert!(kind::donor_score(&repo, "bells", &keys) > 0.3, "{scores}");
}

#[test]
fn a_wooden_decay_rules_out_the_steel_donors() {
    // the same overtones dying at 17/s (a rosewood bar's): a wooden donor
    let repo = Repo::from_env();
    let (got, scores) = mallet_scores(&repo, &glockenspiel_keys(17.0));
    assert!(["xylophone", "marimba", "balafon"].contains(&got.as_str()), "{got}: {scores}");
}

#[test]
fn the_keys_carry_the_fundamentals_decay() {
    let mut m = Measures::default();
    m.ok.insert("/s/84.wav".into(), Measure { onset_s: 0.0, level_db: -10.0, nyquist: 24000.0,
        peaks: vec![[1050.0, 0.0], [3090.0, -20.0]], decays: vec![[1050.0, 0.7], [3090.0, 9.0]], env_db: vec![], bands_db: vec![] });
    let items = vec![json!({"path": "/s/84.wav", "note": 84, "dynamic": "mf", "layer": 1, "layers": 1}).as_object().unwrap().clone()];
    let keys = kind::sample_keys(&items, &m, None);
    assert_eq!(keys[0].sigma, Some(0.7));
}

/// keys with the given upper peaks (ratio, dB re the fundamental), the profiles explaining none
fn treble(ups: &[(f32, f32)]) -> Vec<kind::Key> {
    let mut m = Measures::default();
    let mut items = Vec::new();
    for n in [84i64, 86, 88, 89, 91, 93, 95, 96] {
        let path = format!("/s/{n}.wav");
        let f = midi_hz(n as f64);
        let mut peaks = vec![[f, 0.0]];
        peaks.extend(ups.iter().map(|(r, db)| [f * r, *db]));
        m.ok.insert(path.clone(), Measure { onset_s: 0.0, level_db: -10.0, nyquist: 24000.0, peaks, decays: vec![], env_db: vec![], bands_db: vec![] });
        items.push(json!({"path": path, "note": n, "dynamic": "mf", "layer": 1, "layers": 1}).as_object().unwrap().clone());
    }
    kind::sample_keys(&items, &m, None)
}

#[test]
fn a_strings_treble_is_a_strings() {
    // the fundamental and a weak octave, nothing more: every profile explains it poorly, a bar's
    // the least poorly (the grands' bar shares up to 0.45); its octave tells
    let repo = Repo::from_env();
    let keys = treble(&[(2.0, -24.0)]);
    assert_eq!(kind::bar_share(&keys), 0.0);
    let g = kind::preselect(&repo, &project::kinds(&repo), &keys);
    assert_eq!(g.kind, None);
    assert!(g.why.contains("a string's"), "{}", g.why);
}

#[test]
fn a_pure_tone_or_whole_numbers_without_the_octave_tell_nothing() {
    // the fundamental alone, or a whole-number peak no profile explains and no octave beside it
    let repo = Repo::from_env();
    for ups in [&[][..], &[(8.0, -25.0)][..]] {
        let g = kind::preselect(&repo, &project::kinds(&repo), &treble(ups));
        assert_eq!(g.kind, None);
        assert!(g.why.contains("do not tell"), "{ups:?}: {}", g.why);
    }
}

#[test]
fn a_xylophones_tuned_twelfth_is_a_bars() {
    // 1 : 3 : 6 without the octave: the free bar's bands explain it
    let repo = Repo::from_env();
    let keys = treble(&[(3.0, -20.0), (6.0, -28.0)]);
    assert_eq!(kind::bar_share(&keys), 1.0);
    assert!(kind::preselect(&repo, &project::kinds(&repo), &keys).kind.is_some());
}
