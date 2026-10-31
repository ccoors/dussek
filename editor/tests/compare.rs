//! The Compare page's pairs (from the committed projects; each returns early without its data)
//! and its differences and flags (on measurements written by hand).
use pe_editor::audio::{Long, Partial, UNMATCHED_DB};
use pe_editor::compare::{self, Fundamental, Medians, Off};
use pe_editor::project::{Project, Repo};

/// a committed project whose first source's set is in data/ (none: the test returns early)
fn with_data(repo: &Repo, name: &str) -> Option<Project> {
    let p = Project::load(repo, name).ok()?;
    let src = p.source_json(p.source_ids().first()?)?;
    repo.data.join(src["root"].as_str()?).is_dir().then_some(p)
}

fn long(level: f32, f0: Option<f32>, parts: &[(f32, f32, Option<f32>, Option<f32>)]) -> Long {
    Long { onset_s: 0.1, level_db: level, floor_db: -80.0, end_s: 6.0, stopped_s: None, length_s: Some(1.0), length_db: 30.0, stopped: false, fundamental_hz: f0,
        partials: parts.iter().map(|&(hz, db, early, late)| Partial { hz, db, early, late, until_s: 6.0, snr_db: 60.0 }).collect() }
}

fn rec() -> Long {
    long(-20.0, Some(220.0), &[(220.0, 0.0, Some(3.0), Some(0.5)), (440.0, -6.0, Some(4.0), Some(0.8)), (660.0, -12.0, Some(5.0), Some(1.0))])
}

#[test]
fn a_recording_against_itself_differs_in_nothing() {
    let d = compare::diff(&rec(), &rec());
    assert_eq!(d.level_db, 0.0);
    assert_eq!(d.pitch_cents, Some(0.0));
    assert_eq!(d.fundamental, Fundamental::Both);
    assert_eq!((d.early, d.late), (Some(1.0), Some(1.0)));
    assert_eq!(d.spectrum_db, 0.0);
    let zero = Medians::default();
    assert!(compare::offs(&d, "mf", &zero).is_empty());
    assert_eq!(compare::badness(&d, "mf", &zero), 0.0);
}

#[test]
fn each_change_moves_its_own_measure() {
    let r = rec();
    let zero = Medians::default();
    // 13 dB louder
    let mut louder = r.clone();
    louder.level_db += 13.0;
    let d = compare::diff(&r, &louder);
    assert_eq!(d.level_db, 13.0);
    assert!(matches!(compare::offs(&d, "mf", &zero)[..], [Off::Level(l)] if (l - 13.0).abs() < 1e-4));
    // 30 cents sharp
    let mut sharp = r.clone();
    sharp.fundamental_hz = Some(220.0 * 2f32.powf(30.0 / 1200.0));
    let d = compare::diff(&r, &sharp);
    assert!((d.pitch_cents.unwrap() - 30.0).abs() < 0.01);
    assert!(matches!(compare::offs(&d, "mf", &zero)[..], [Off::Pitch(_)]));
    // past the length's threshold
    let mut long2 = r.clone();
    long2.length_s = Some(1.2 * compare::LENGTH_RATIO);
    let d = compare::diff(&r, &long2);
    assert!((d.length.unwrap() - 1.2 * compare::LENGTH_RATIO).abs() < 1e-6 && !d.length_stopped);
    assert!(matches!(compare::offs(&d, "mf", &zero)[..], [Off::Length(_)]));
    // a partial missing in the render
    let mut thin = r.clone();
    thin.partials[1] = Partial { hz: 440.0, db: UNMATCHED_DB, early: None, late: None, until_s: 6.0, snr_db: 60.0 };
    let d = compare::diff(&r, &thin);
    // the missing −6 dB partial counts at the comparison's floor
    let want = ((compare::SPECTRUM_FLOOR_DB + 6.0).powi(2) / 3.0).sqrt();
    assert!((d.spectrum_db - want).abs() < 1e-3, "{} vs {want}", d.spectrum_db);
    assert!(matches!(compare::offs(&d, "mf", &zero)[..], [Off::Spectrum(_)]));
}

#[test]
fn keys_are_flagged_against_the_instruments_medians() {
    // an instrument whose render is 9 dB brighter and dies 5 times faster late everywhere, its pp
    // 20 dB softer than its ff (each dynamic's level its own median): no key is off; one key 13 dB
    // over its dynamic's others is
    let r = rec();
    let pair = |level: f32| {
        let mut x = r.clone();
        x.level_db += level;
        for p in &mut x.partials {
            p.late = p.late.map(|l| 5.0 * l);
            p.db = if p.db < 0.0 { p.db + 9.0 } else { p.db };
        }
        compare::diff(&r, &x)
    };
    let rows: Vec<(&str, compare::Diff)> = vec![("pp", pair(-20.0)), ("pp", pair(-21.0)), ("pp", pair(-19.0)),
        ("ff", pair(0.0)), ("ff", pair(1.0)), ("ff", pair(13.5)), ("ff", pair(-0.5))];
    let m = compare::medians(&rows.iter().map(|(k, d)| (*k, d)).collect::<Vec<_>>());
    assert_eq!((m.level["pp"], m.level["ff"]), (-20.0, 0.5));
    assert!((m.late - 5.0).abs() < 1e-4 && m.spectrum > 5.0, "{m:?}");
    let off: Vec<usize> = rows.iter().enumerate().filter(|(_, (k, d))| !compare::offs(d, k, &m).is_empty()).map(|(i, _)| i).collect();
    assert_eq!(off, vec![5], "{m:?}");
    assert!(matches!(compare::offs(&rows[5].1, "ff", &m)[..], [Off::Level(l)] if (l - 13.0).abs() < 1e-4));
}

#[test]
fn a_fundamental_on_one_side_only_is_no_flag() {
    // a piano's weak bass fundamental read on one side only: no pitch, nothing flagged (an
    // octave error shows in the spectrum: its partials match nothing)
    let mut octave = rec();
    octave.fundamental_hz = None;
    let d = compare::diff(&rec(), &octave);
    assert_eq!(d.fundamental, Fundamental::RecordingOnly);
    assert_eq!(d.pitch_cents, None);
    assert!(compare::offs(&d, "mf", &Medians::default()).is_empty());
}

#[test]
fn a_faint_partials_decay_is_not_compared() {
    // a partial 50 dB down rings in the noise: its recorded late rate (near 0) says nothing
    let r = long(-20.0, Some(220.0), &[(220.0, 0.0, Some(3.0), Some(0.5)), (980.0, -50.0, Some(5.0), Some(0.003))]);
    let mut ren = r.clone();
    ren.partials[1].late = Some(6.0);
    assert_eq!(compare::diff(&r, &ren).late, Some(1.0));
}

#[test]
fn four_foot_pairs_strike_the_key_played() {
    let repo = Repo::from_env();
    let Some(p) = with_data(&repo, "flemish") else { return };
    let pairs = compare::pairs(&repo, &p);
    assert!(!pairs.list.is_empty(), "no pairs: {:?}", pairs.none_why);
    let four: Vec<_> = pairs.list.iter().filter(|x| x.source == "flemish4").collect();
    let eight: Vec<_> = pairs.list.iter().filter(|x| x.source == "flemish8").collect();
    assert!(!four.is_empty() && !eight.is_empty());
    assert!(four.iter().all(|x| x.note == x.strike.key as i64 + 12 && x.strike.stops == Some(2)), "{:?}", four[0]);
    assert!(eight.iter().all(|x| x.note == x.strike.key as i64 && x.strike.stops == Some(1)));
    assert!(pairs.list.iter().all(|x| x.dynamic == "mf" && x.strike.mallet.is_none()));
}

#[test]
fn mallet_sources_take_their_mallets_speeds() {
    let repo = Repo::from_env();
    let Some(p) = with_data(&repo, "marimba") else { return };
    let pairs = compare::pairs(&repo, &p);
    assert!(!pairs.list.is_empty(), "no pairs: {:?}", pairs.none_why);
    let fit: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(repo.projects.join("marimba/fits/marimba.json")).unwrap()).unwrap();
    for (i, id) in ["marimba_yarn", "marimba_cord", "marimba_rubber"].iter().enumerate() {
        let x = pairs.list.iter().find(|x| x.source == *id && x.dynamic == "ff").unwrap_or_else(|| panic!("no ff pair of {id}"));
        assert_eq!(x.strike.mallet, Some(i as u32));
        assert_eq!(x.strike.speed, fit["mallets"][i]["speeds"]["ff"].as_f64().unwrap());
        assert_eq!(x.strike.stops, None);
    }
}

#[test]
fn a_layered_set_compares_its_fitted_dynamics_only() {
    let repo = Repo::from_env();
    let Some(p) = with_data(&repo, "grand_a") else { return };
    let pairs = compare::pairs(&repo, &p);
    assert!(!pairs.list.is_empty(), "no pairs: {:?}", pairs.none_why);
    assert!(pairs.skipped > 0, "the other layers are counted as skipped");
    assert!(pairs.list.iter().all(|x| compare::FITTED.contains(&x.dynamic.as_str())));
}



#[test]
fn a_faint_partial_missing_counts_little() {
    // a −35 dB partial missing in the render: within 5 dB of the floor; a −6 dB one: far past it
    let r = long(-20.0, Some(220.0), &[(220.0, 0.0, None, None), (660.0, -35.0, None, None)]);
    let mut faint = r.clone();
    faint.partials[1] = Partial { hz: 660.0, db: UNMATCHED_DB, early: None, late: None, until_s: 6.0, snr_db: 60.0 };
    let d = compare::diff(&r, &faint);
    assert!((d.spectrum_db - (5.0f32.powi(2) / 2.0).sqrt()).abs() < 1e-3, "{}", d.spectrum_db);
    assert!(compare::offs(&d, "mf", &Medians::default()).is_empty());
}

#[test]
fn every_pair_strikes_a_key_of_the_keyboard() {
    // recordings sounding above C8 (a set named octaves low) are counted, not played: the engine's
    // keys are A0–C8
    let repo = Repo::from_env();
    let (mut outside, mut checked) = (0, 0);
    for name in pe_editor::project::project_names(&repo) {
        let Some(p) = with_data(&repo, &name) else { continue };
        let ps = compare::pairs(&repo, &p);
        assert!(ps.list.iter().all(|x| (21..=108).contains(&x.strike.key)), "{name}: a key outside A0–C8");
        outside += ps.outside;
        checked += 1;
    }
    eprintln!("{checked} projects, {outside} recordings outside the keyboard");
}

#[test]
fn one_render_partial_that_rises_does_not_count() {
    // a partial beating or rising over the window reads a rate of 0 (its slope clamped): it is no
    // key ringing on, and the others decay as recorded
    let r = rec();
    let mut beat = r.clone();
    beat.partials[2].late = Some(0.0);
    beat.partials[2].early = Some(0.0);
    let d = compare::diff(&r, &beat);
    assert_eq!((d.early, d.late), (Some(1.0), Some(1.0)));
}

#[test]
fn each_source_is_judged_against_its_own_medians() {
    // a second mallet whose render is 13 dB louder and 9 dB brighter than the first's on every key
    // (a mallet's own difference, not a key's): nothing flagged; a key off within a source is
    let r = rec();
    let pair = |level: f32, bright: f32| {
        let mut x = r.clone();
        x.level_db += level;
        for p in &mut x.partials {
            p.db = if p.db < 0.0 { p.db + bright } else { p.db };
        }
        compare::diff(&r, &x)
    };
    let rows: Vec<(&str, &str, compare::Diff)> = vec![
        ("yarn", "mf", pair(0.0, 0.0)), ("yarn", "mf", pair(0.5, 0.0)), ("yarn", "mf", pair(-0.5, 0.0)), ("yarn", "mf", pair(14.0, 0.0)),
        ("rubber", "mf", pair(13.0, 9.0)), ("rubber", "mf", pair(13.5, 9.0)), ("rubber", "mf", pair(12.5, 9.0)),
    ];
    let ms = compare::medians_by_source(&rows.iter().map(|(s, k, d)| (*s, *k, d)).collect::<Vec<_>>());
    assert_eq!(ms.len(), 2);
    assert!((ms["rubber"].level_of("mf") - 13.0).abs() < 1e-4 && ms["yarn"].level_of("mf").abs() < 0.3, "{ms:?}");
    let off: Vec<usize> = rows.iter().enumerate().filter(|(_, (s, k, d))| !compare::offs(d, k, &ms[*s]).is_empty()).map(|(i, _)| i).collect();
    assert_eq!(off, vec![3], "{ms:?}");
}

fn pair_at(source: &str, note: i64, dynamic: &str) -> compare::Pair {
    compare::Pair { source: source.into(), path: format!("/x/{source}-{note}-{dynamic}.wav").into(), note, dynamic: dynamic.into(),
        strike: compare::Strike { key: note as u8, speed: 1.0, stops: None, mallet: None, room: None } }
}

fn level_diff(level: f32) -> compare::Diff {
    let mut x = rec();
    x.level_db += level;
    compare::diff(&rec(), &x)
}

#[test]
fn one_odd_key_leaves_the_correction_flat() {
    // nine keys level with their median, one 8 dB off: the running median keeps the curve at 0
    let ps: Vec<compare::Pair> = (60..70).map(|n| pair_at("s", n, "mf")).collect();
    let ds: Vec<compare::Diff> = (60..70).map(|n| level_diff(if n == 65 { 8.0 } else { 0.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let t = &compare::level_corrections(&rows, &Default::default(), &Default::default())["s"];
    assert!(t.db["mf"].iter().all(|v| v.abs() < 1e-6), "{t:?}");
}

#[test]
fn a_sloped_region_is_corrected() {
    // the top five of ten keys 6 dB loud (the median of the ten: 0 … level with half): the curve
    // takes the top down, the bottom stays
    let ps: Vec<compare::Pair> = (60..70).map(|n| pair_at("s", n, "pp")).collect();
    let ds: Vec<compare::Diff> = (60..70).map(|n| level_diff(if n >= 66 { 6.0 } else { 0.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let t = &compare::level_corrections(&rows, &Default::default(), &Default::default())["s"];
    let at = |k: i64| t.at("pp", k);
    assert!(at(69) < -5.0 && at(60).abs() < 1e-6, "{t:?}");
}

#[test]
fn a_second_correction_keeps_the_first() {
    // measured again after a −6 dB correction took effect: residuals 0, the correction stays −6
    let ps: Vec<compare::Pair> = (60..65).map(|n| pair_at("s", n, "mf")).collect();
    let ds: Vec<compare::Diff> = (60..65).map(|_| level_diff(0.0)).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let mut cur = std::collections::BTreeMap::new();
    cur.insert("s".to_string(), compare::LevelTable { keys: vec![60, 64], db: [("mf".to_string(), vec![-6.0, -6.0])].into() });
    let t = &compare::level_corrections(&rows, &cur, &Default::default())["s"];
    assert!(t.db["mf"].iter().all(|v| (v + 6.0).abs() < 1e-6), "{t:?}");
}

#[test]
fn corrections_are_clamped_and_need_three_keys() {
    let ps: Vec<compare::Pair> = (60..64).map(|n| pair_at("s", n, "ff")).chain((60..62).map(|n| pair_at("s", n, "pp"))).collect();
    // ff: three keys 40 dB loud, one level: their median is 40, the level one reads −40 → +24 at most
    let ds: Vec<compare::Diff> = [40.0, 40.0, 40.0, 0.0, 9.0, 9.0].iter().map(|l| level_diff(*l)).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let t = &compare::level_corrections(&rows, &Default::default(), &Default::default())["s"];
    assert!(t.db["ff"].iter().all(|v| v.abs() <= compare::CORRECTION_LIMIT_DB + 1e-6), "{t:?}");
    assert!(!t.db.contains_key("pp"), "two pp keys: no curve");
}

#[test]
fn the_current_correction_is_read_from_the_export() {
    // what the renders carry: each recorded key's note's level_db, its mallet's triple, its
    // register's notes; a note without one: 0 dB
    let inst = serde_json::json!({"notes": [
        {"note": 60, "level_db": [[1.0, 2.0, 3.0], [-1.0, -2.0, -3.0]]},
        {"note": 62},
        {"note": 64, "level_db": [[0.5, 0.5, 0.5], [4.0, 4.0, 4.0]]},
    ]});
    let mut a = pair_at("yarn", 60, "mf");
    a.strike.mallet = Some(0);
    let mut b = pair_at("cord", 60, "mf");
    b.strike.mallet = Some(1);
    let mut c = pair_at("cord", 62, "mf");
    c.strike.mallet = Some(1);
    let mut d = pair_at("cord", 64, "mf");
    d.strike.mallet = Some(1);
    let cur = compare::corrections_in_export(&inst, &[a, b, c, d]);
    assert_eq!(cur["yarn"].at("pp", 60), 1.0);
    assert_eq!((cur["cord"].at("mf", 60), cur["cord"].at("mf", 62), cur["cord"].at("ff", 64)), (-2.0, 0.0, 4.0));
    // a second register's notes (format 6): stops bit 1
    let reg = serde_json::json!({"registers": [{"notes": [{"note": 50}]}, {"notes": [{"note": 50, "level_db": [[0.0, 6.0, 0.0]]}]}]});
    let mut four = pair_at("four", 50, "mf");
    four.strike.stops = Some(2);
    assert_eq!(compare::corrections_in_export(&reg, &[four])["four"].at("mf", 50), 6.0);
}

#[test]
fn a_stopped_pair_flags_only_a_shorter_render() {
    let mut rec = rec();
    rec.stopped = true;
    rec.length_s = Some(1.0);
    let zero = Medians::default();
    let mut longer = rec.clone();
    longer.stopped = false;
    longer.length_s = Some(2.0 * compare::LENGTH_RATIO);
    assert!(compare::offs(&compare::diff(&rec, &longer), "mf", &zero).is_empty(), "a stopped recording says nothing of a longer render");
    let mut shorter = longer.clone();
    shorter.length_s = Some(0.8 / compare::LENGTH_RATIO);
    assert!(matches!(compare::offs(&compare::diff(&rec, &shorter), "mf", &zero)[..], [Off::Length(_)]));
}

#[test]
fn the_length_median_leaves_stopped_pairs_out() {
    let mk = |ratio: f32, stopped: bool| {
        let mut a = rec();
        a.stopped = stopped;
        let mut b = rec();
        b.length_s = Some(ratio);
        compare::diff(&a, &b)
    };
    let rows = [mk(1.0, false), mk(1.0, false), mk(1.0, false), mk(9.0, true), mk(9.0, true), mk(9.0, true), mk(9.0, true)];
    let m = compare::medians(&rows.iter().map(|d| ("mf", d)).collect::<Vec<_>>());
    assert!((m.length - 1.0).abs() < 1e-6, "{m:?}");
}

#[test]
fn a_partial_in_the_noise_does_not_count_in_the_spectrum() {
    let r = rec();
    let mut thin = r.clone();
    thin.partials[1] = Partial { hz: 440.0, db: UNMATCHED_DB, early: None, late: None, until_s: 6.0, snr_db: 60.0 };
    let mut noisy_rec = r.clone();
    noisy_rec.partials[1].snr_db = 10.0; // the recording's partial is noise
    assert!(compare::diff(&r, &thin).spectrum_db > 5.0);
    assert_eq!(compare::diff(&noisy_rec, &thin).spectrum_db, 0.0);
}

#[test]
fn a_rumble_under_the_note_sets_no_spectrums_reference() {
    // the recording's levels re a rumble below the note (its partials at −21 and −27 dB), the
    // render's re its fundamental (0 and −6 dB): the same shape, no difference
    let r = rec();
    let mut rumbled = r.clone();
    for p in &mut rumbled.partials {
        p.db -= 21.0;
    }
    assert!(compare::diff(&rumbled, &r).spectrum_db.abs() < 1e-4, "{}", compare::diff(&rumbled, &r).spectrum_db);
}

#[test]
fn a_render_ringing_past_the_threshold_is_rendered_long_enough_to_show_it() {
    // a 3 s recording ending at 4 s: a render LENGTH_RATIO times as long against a median of up to
    // 2 (×2 was the cap at 2·length + 0.5 s: a render ringing on could never be flagged)
    let mut r = rec();
    r.length_s = Some(3.0);
    r.end_s = 4.0;
    assert!(compare::render_seconds(&r) >= 2.0 * compare::LENGTH_RATIO * 3.0, "{}", compare::render_seconds(&r));
}

#[test]
fn the_instrument_level_keeps_a_keys_own_difference() {
    // the instrument plays 9 dB under its recordings (its median), this key 3 dB over that: Pair
    // plays it at the recording's level, Instrument takes off the 9 dB only (the 3 dB stay audible),
    // the original with the same gain
    let mut ren = rec();
    ren.level_db = rec().level_db - 6.0;
    let e = compare::Entry { recording: rec(), render: ren };
    assert_eq!(compare::playback_gains(compare::LevelMatch::Off, &e, Some(-30.0), -9.0), (0.0, 0.0));
    assert_eq!(compare::playback_gains(compare::LevelMatch::Pair, &e, Some(-30.0), -9.0), (6.0, 10.0));
    assert_eq!(compare::playback_gains(compare::LevelMatch::Instrument, &e, Some(-30.0), -9.0), (9.0, 9.0));
}

#[test]
fn a_step_between_runs_is_not_copied() {
    // two octave runs C5–B5 and C6–B6, the second recorded 12 dB louder (another run's gain or
    // touch): grouped by octave nothing is corrected; taken whole, the upper run is cut 12 dB
    let ps: Vec<compare::Pair> = (72..96).map(|n| pair_at("s", n, "pp")).collect();
    let ds: Vec<compare::Diff> = (72..96).map(|n| level_diff(if n >= 84 { -12.0 } else { 0.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let octave = [("s".to_string(), compare::LevelGroups::Octave)].into();
    let t = &compare::level_corrections(&rows, &Default::default(), &octave)["s"];
    assert!(t.db["pp"].iter().all(|v| v.abs() < 1e-6), "{t:?}");
    let whole = &compare::level_corrections(&rows, &Default::default(), &Default::default())["s"];
    assert!(whole.at("pp", 90) > 5.0, "{whole:?}");
}

#[test]
fn a_slope_within_a_run_is_corrected() {
    // within the run C5–B5 the render falls 1 dB per key against its recordings: grouped by octave
    // the run's ends are corrected in opposite directions
    let ps: Vec<compare::Pair> = (72..84).map(|n| pair_at("s", n, "pp")).collect();
    let ds: Vec<compare::Diff> = (72..84).map(|n| level_diff(-((n - 72) as f32))).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let octave = [("s".to_string(), compare::LevelGroups::Octave)].into();
    let t = &compare::level_corrections(&rows, &Default::default(), &octave)["s"];
    assert!(t.at("pp", 72) < -3.0 && t.at("pp", 83) > 3.0, "{t:?}");
}

#[test]
fn normalized_files_are_not_corrected() {
    // every file normalized to its peak: its level says nothing, no table
    let ps: Vec<compare::Pair> = (60..70).map(|n| pair_at("s", n, "mf")).collect();
    let ds: Vec<compare::Diff> = (60..70).map(|n| level_diff(if n >= 65 { 6.0 } else { 0.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let file = [("s".to_string(), compare::LevelGroups::File)].into();
    assert!(!compare::level_corrections(&rows, &Default::default(), &file).contains_key("s"));
}

#[test]
fn a_sources_level_groups_are_read_from_its_levels() {
    use serde_json::json;
    assert_eq!(compare::level_groups_of(&json!({"levels": "recorded"})), compare::LevelGroups::Whole);
    assert_eq!(compare::level_groups_of(&json!({"levels": "normalized"})), compare::LevelGroups::File);
    assert_eq!(compare::level_groups_of(&json!({"levels": "normalized", "level_groups": "octave"})), compare::LevelGroups::Octave);
    assert_eq!(compare::level_groups_of(&json!({})), compare::LevelGroups::Whole);
}

#[test]
fn a_tilt_repeated_in_each_run_leaves_no_step_at_the_boundary() {
    // in both runs C5–B5 and C6–B6 the render rises 0.5 dB per key against its recordings (a run's
    // level true within it): the correction follows the tilt and runs on across C6, no step
    let ps: Vec<compare::Pair> = (72..96).map(|n| pair_at("s", n, "pp")).collect();
    let ds: Vec<compare::Diff> = (72..96).map(|n| level_diff(0.5 * ((n - 72) % 12) as f32)).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let octave = [("s".to_string(), compare::LevelGroups::Octave)].into();
    let t = &compare::level_corrections(&rows, &Default::default(), &octave)["s"];
    assert!((t.at("pp", 84) - t.at("pp", 83)).abs() < 1.5, "a step of {:.1} dB at C6: {t:?}", t.at("pp", 84) - t.at("pp", 83));
    assert!(t.at("pp", 95) < t.at("pp", 72) - 5.0, "the tilt corrected: {t:?}");
}

#[test]
fn every_other_key_louder_is_not_copied() {
    // a run played with alternating sticking: every other key 3 dB over, 3 dB under: the
    // correction is flat (a running median of 5 kept the alternation: three of five of one kind)
    let ps: Vec<compare::Pair> = (60..72).map(|n| pair_at("s", n, "pp")).collect();
    let ds: Vec<compare::Diff> = (60..72).map(|n| level_diff(if n % 2 == 0 { 3.0 } else { -3.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let t = &compare::level_corrections(&rows, &Default::default(), &Default::default())["s"];
    // the keys with two neighbors either side (the ends cannot tell an alternation from a slope)
    assert!(t.db["pp"][2..10].iter().all(|v| v.abs() < 1.0), "{t:?}");
}

#[test]
fn a_runs_gain_is_not_flagged() {
    // the run C6–B6 recorded 14 dB louder than C4–B5 (its render 14 dB under it): grouped by
    // octave each key is judged re its own run, nothing is off; taken whole, the run is flagged
    let ps: Vec<compare::Pair> = (60..96).map(|n| pair_at("s", n, "pp")).collect();
    let ds: Vec<compare::Diff> = (60..96).map(|n| level_diff(if n >= 84 { -14.0 } else { 0.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let octave = [("s".to_string(), compare::LevelGroups::Octave)].into();
    let ms = compare::medians_grouped(&rows, &octave);
    assert!(rows.iter().all(|(p, d)| compare::offs_at(d, p, &ms["s"]).is_empty()));
    let whole = compare::medians_grouped(&rows, &Default::default());
    assert!(rows.iter().filter(|(p, d)| !compare::offs_at(d, p, &whole["s"]).is_empty()).count() >= 12);
}

#[test]
fn a_normalized_files_level_is_not_flagged() {
    let ps: Vec<compare::Pair> = (60..70).map(|n| pair_at("s", n, "mf")).collect();
    let ds: Vec<compare::Diff> = (60..70).map(|n| level_diff(if n == 65 { 20.0 } else { 0.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let file = [("s".to_string(), compare::LevelGroups::File)].into();
    let ms = compare::medians_grouped(&rows, &file);
    assert!(rows.iter().all(|(p, d)| compare::offs_at(d, p, &ms["s"]).is_empty()));
    assert_eq!(ms["s"].level_ref("mf", 65), None);
}

/// a set scaled per key (Grand A): each key's pp and ff at `gain(key)` (its own gain), pp off
/// its step by `step(key)` more
fn per_key_rows(gain: impl Fn(i64) -> f32, step: impl Fn(i64) -> f32) -> (Vec<compare::Pair>, Vec<compare::Diff>) {
    let mut ps = Vec::new();
    let mut ds = Vec::new();
    for n in (60..108).step_by(3) {
        ps.push(pair_at("s", n, "ff"));
        ds.push(level_diff(gain(n)));
        ps.push(pair_at("s", n, "pp"));
        ds.push(level_diff(gain(n) - 6.0 + step(n)));
    }
    (ps, ds)
}

#[test]
fn a_keys_own_gain_is_not_flagged_or_corrected() {
    // every key at its own gain (±14 dB, both dynamics alike): judged per key, the steps between
    // the dynamics all match: nothing off, nothing corrected; taken whole, the gains are flagged
    let (ps, ds) = per_key_rows(|n| if n % 2 == 0 { 14.0 } else { -14.0 }, |_| 0.0);
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let key = [("s".to_string(), compare::LevelGroups::Key)].into();
    let ms = compare::medians_grouped(&rows, &key);
    assert!(rows.iter().all(|(p, d)| compare::offs_at(d, p, &ms["s"]).is_empty()));
    let t = compare::level_corrections(&rows, &Default::default(), &key);
    assert!(t.get("s").is_none_or(|t| t.db.values().flatten().all(|v| v.abs() < 1e-6)), "{t:?}");
    let whole = compare::medians_grouped(&rows, &Default::default());
    assert!(rows.iter().any(|(p, d)| !compare::offs_at(d, p, &whole["s"]).is_empty()));
}

#[test]
fn a_step_off_is_flagged_and_corrected_and_the_loudest_is_not() {
    // from C7 up the render's pp falls 14 dB further under its ff than the recording's (Grand A's
    // top pp too quiet): pp flagged there and raised; ff is the reference: never flagged, no column
    let (ps, ds) = per_key_rows(|n| (n % 5) as f32 * 3.0, |n| if n >= 96 { -14.0 } else { 0.0 });
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&ds).collect();
    let key = [("s".to_string(), compare::LevelGroups::Key)].into();
    let ms = compare::medians_grouped(&rows, &key);
    let flagged: Vec<(i64, &str)> = rows.iter().filter(|(p, d)| !compare::offs_at(d, p, &ms["s"]).is_empty())
        .map(|(p, _)| (p.note, p.dynamic.as_str())).collect();
    assert_eq!(flagged, vec![(96, "pp"), (99, "pp"), (102, "pp"), (105, "pp")]);
    assert_eq!(ms["s"].level_ref("ff", 99), None);
    let t = &compare::level_corrections(&rows, &Default::default(), &key)["s"];
    assert!(!t.db.contains_key("ff"), "{t:?}");
    assert!(t.at("pp", 105) > 8.0 && t.at("pp", 72).abs() < 1e-6, "{t:?}");
}

#[test]
fn a_set_scaled_per_key_is_judged_per_key() {
    use serde_json::json;
    assert_eq!(compare::level_groups_of(&json!({"levels": "per_key"})), compare::LevelGroups::Key);
    assert_eq!(compare::level_groups_of(&json!({"level_groups": "key"})), compare::LevelGroups::Key);
}

#[test]
fn one_key_swaying_a_proposal_is_found() {
    // three octave runs, level but for one key struck hard at the first run's top (B5, 10 dB over):
    // the runs meet at the keys either side, so leaving B5 out moves the runs above by ~5 dB, and
    // the sway names it; level runs alone sway nothing
    let ps: Vec<compare::Pair> = (72..108).map(|n| pair_at("s", n, "ff")).collect();
    let octave: std::collections::BTreeMap<String, compare::LevelGroups> = [("s".to_string(), compare::LevelGroups::Octave)].into();
    let loud: Vec<compare::Diff> = (72..108).map(|n| level_diff(if n == 83 { 10.0 } else { 0.0 })).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&loud).collect();
    let s = &compare::one_key_sway(&rows, &Default::default(), &octave)["s"];
    assert!(s.db > 4.0 && s.key == 83 && s.dynamic == "ff", "{s:?}");
    assert!(s.db >= compare::ONE_KEY_SWAY_WARN_DB, "{s:?}");
    let level: Vec<compare::Diff> = (72..108).map(|_| level_diff(0.0)).collect();
    let rows: Vec<(&compare::Pair, &compare::Diff)> = ps.iter().zip(&level).collect();
    assert!(compare::one_key_sway(&rows, &Default::default(), &octave)["s"].db < 0.05);
}
