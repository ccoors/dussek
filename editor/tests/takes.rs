//! The take checks on synthetic measurements (no audio): a decay unlike its neighbors, a late
//! burst, a layer out of the brightness order; takes without enough data; the thresholds.
use pe_editor::audio::{self, Measure};
use pe_editor::takes::{self, Check, TakeInput, Thresholds};
use serde_json::json;

/// a measurement whose level falls `slope` dB/s for `seconds`, with `bump` dB added over
/// [from, to) s, and a spectrum `tilt` dB brighter above 4 × f0 = 800 Hz
fn meas(slope: f32, seconds: f32, bump: Option<(f32, f32, f32)>, tilt: f32) -> Measure {
    let n = (seconds / audio::ENV_HOP_S) as usize;
    let env_db = (0..n).map(|i| {
        let t = i as f32 * audio::ENV_HOP_S;
        let b = bump.filter(|(a, z, _)| t >= *a && t < *z).map_or(0.0, |(_, _, db)| db);
        -10.0 + slope * t + b
    }).collect();
    let bands_db = (0..80).map(|k| if audio::band_hz(k) >= 800.0 { -40.0 + tilt } else { -10.0 }).collect();
    Measure { onset_s: 0.0, level_db: -10.0, nyquist: 24000.0, peaks: vec![], decays: vec![], env_db, bands_db }
}

fn inputs<'a>(ms: &'a [(i64, i64, String, Measure)]) -> Vec<TakeInput<'a>> {
    ms.iter().map(|(note, layer, rel, m)| TakeInput { relpath: rel, note: *note, layer: *layer, f0: 200.0, tail_cut_s: 0.0,
        spare: false, m: Some(m) }).collect()
}

fn flagged(ms: &[(i64, i64, String, Measure)], check: Check) -> Vec<String> {
    let f = takes::flags(&takes::score(&inputs(ms)), &Thresholds::DEFAULT);
    let mut v: Vec<String> = f.iter().filter(|(_, s)| s.iter().any(|x| x.check == check)).map(|(k, _)| k.clone()).collect();
    v.sort();
    v
}

#[test]
fn a_decay_unlike_its_neighbors_is_flagged() {
    let ms: Vec<_> = (60..72).map(|n| {
        let slope = if n == 66 { -30.0 } else if n % 2 == 0 { -10.5 } else { -9.5 };
        (n, 1, format!("{n}.wav"), meas(slope, 2.0, None, 0.0))
    }).collect();
    assert_eq!(flagged(&ms, Check::Decay), ["66.wav"]);
    let s = takes::score(&inputs(&ms));
    let d = s["66.wav"].iter().find(|x| x.check == Check::Decay).unwrap();
    assert!(takes::describe(d).contains("faster"), "{}", takes::describe(d));
}

#[test]
fn a_late_burst_is_flagged_and_a_clean_decay_is_not() {
    let ms: Vec<_> = (60..72).map(|n| {
        let bump = (n == 64).then_some((5.0, 5.5, 15.0));
        (n, 1, format!("{n}.wav"), meas(-3.0, 8.0, bump, 0.0))
    }).collect();
    assert_eq!(flagged(&ms, Check::Rise), ["64.wav"]);
    let s = takes::score(&inputs(&ms));
    let r = s["64.wav"].iter().find(|x| x.check == Check::Rise).unwrap();
    assert!((r.at_s.unwrap() - 5.25).abs() < 0.35, "{:?}", r.at_s);
    // the climb above the level just before the burst (the decay keeps falling under it)
    assert!(r.value > 13.0 && r.value < 16.0, "{}", r.value);
    assert!(takes::describe(r).starts_with("rises 1") && takes::describe(r).contains(" dB at 5."), "{}", takes::describe(r));
}

#[test]
fn a_layer_out_of_the_brightness_order_is_flagged() {
    let mut ms = Vec::new();
    for n in 60..70 {
        for l in 1..=6 {
            let extra = if n == 63 && l == 3 { 12.0 } else { 0.0 };
            ms.push((n, l, format!("{n}_{l}.wav"), meas(-10.0, 2.0, None, 2.0 * l as f32 + extra)));
        }
    }
    assert_eq!(flagged(&ms, Check::Brightness), ["63_3.wav"]);
    let s = takes::score(&inputs(&ms));
    let b = s["63_3.wav"].iter().find(|x| x.check == Check::Brightness).unwrap();
    assert!(takes::describe(b).contains("brighter"), "{}", takes::describe(b));
}

#[test]
fn takes_without_enough_data_are_not_flagged() {
    let short = meas(-10.0, 0.8, None, 0.0);
    let ok = meas(-10.0, 2.0, None, 0.0);
    let ins = vec![
        TakeInput { relpath: "none.wav", note: 60, layer: 1, f0: 200.0, tail_cut_s: 0.0, spare: false, m: None },
        TakeInput { relpath: "short.wav", note: 61, layer: 1, f0: 200.0, tail_cut_s: 0.0, spare: false, m: Some(&short) },
        TakeInput { relpath: "alone.wav", note: 62, layer: 1, f0: 200.0, tail_cut_s: 0.0, spare: false, m: Some(&ok) },
    ];
    let s = takes::score(&ins);
    assert!(s.values().flatten().all(|x| x.score.is_finite() && x.value.is_finite()), "{s:?}");
    assert!(takes::flags(&s, &Thresholds::DEFAULT).is_empty());
    assert!(takes::flags(&takes::score(&[]), &Thresholds::DEFAULT).is_empty());
}

#[test]
fn an_even_source_flags_nothing() {
    let ms: Vec<_> = (60..80).map(|n| (n, 1, format!("{n}.wav"), meas(if n == 70 { -10.5 } else { -10.0 }, 2.0, None, 0.0))).collect();
    assert!(takes::flags(&takes::score(&inputs(&ms)), &Thresholds::DEFAULT).is_empty());
}

#[test]
fn thresholds_round_trip_through_the_project() {
    assert_eq!(Thresholds::from_project(None), Thresholds::DEFAULT);
    assert_eq!(Thresholds::DEFAULT.to_project(), None);
    let moved = Thresholds { rise: 6.0, ..Thresholds::DEFAULT };
    assert_eq!(moved.to_project(), Some(json!({"rise": 6.0})));
    assert_eq!(Thresholds::from_project(moved.to_project().as_ref()), moved);
    let odd = Thresholds::from_project(Some(&json!({"decay": "x", "rise": 99.0})));
    assert_eq!(odd.decay, Thresholds::DEFAULT.decay);
    assert_eq!(odd.rise, 20.0);
}

#[test]
fn inputs_take_the_key_and_its_pitch_offset() {
    use pe_editor::audio::Measures;
    use std::collections::BTreeMap;
    let it = |rel: &str| json!({"path": format!("/s/{rel}"), "relpath": rel, "note": 69, "layer": 1, "tail_cut_s": 0.5})
        .as_object().unwrap().clone();
    let items = vec![it("a.wav")];
    let spares: BTreeMap<String, Vec<pe_editor::mapping::Item>> = [("a.wav".to_string(), vec![it("b.wav"), it("c.wav")])].into();
    let offsets: BTreeMap<String, f32> = [("/s/a.wav".to_string(), 1200.0)].into();
    let m = Measures::default();
    let ins = takes::inputs(&items, &spares, &m, &offsets);
    assert_eq!(ins.len(), 3);
    assert!((ins[0].f0 - 880.0).abs() < 0.01 && !ins[0].spare && ins[0].tail_cut_s == 0.5);
    assert!(ins[1].spare && ins[2].spare && (ins[1].f0 - 440.0).abs() < 0.01);
}

#[test]
fn three_layers_are_too_few_to_judge_brightness() {
    // with three layers the line through the other two is extrapolated to the outer one: a set
    // like Grand C (layers 1, 2 and 5 as ranks 1–3) flagged a tenth of its takes; four or more layers
    let mut ms = Vec::new();
    for n in 60..70 {
        for l in 1..=3 {
            let extra = if n == 63 && l == 3 { 12.0 } else { 0.0 };
            ms.push((n, l, format!("{n}_{l}.wav"), meas(-10.0, 2.0, None, 2.0 * l as f32 + extra)));
        }
    }
    let s = takes::score(&inputs(&ms));
    assert!(s.values().flatten().all(|x| x.check != Check::Brightness), "no brightness scores with three layers");
}

#[test]
fn the_score_key_follows_what_the_scores_read() {
    // the Check page recomputes the scores when this key changes: a relabel, a tail cut or a new
    // measurement changes it even where the counts stay the same (the flags went stale)
    let a = meas(-10.0, 2.0, None, 0.0);
    let b = meas(-12.0, 2.0, None, 0.0);
    let one = |note: i64, cut: f32, m: &'static Measure| vec![TakeInput { relpath: "x.wav", note, layer: 1, f0: 200.0, tail_cut_s: cut, spare: false, m: Some(m) }];
    let (a, b): (&'static Measure, &'static Measure) = (Box::leak(Box::new(a)), Box::leak(Box::new(b)));
    let base = takes::inputs_key(&one(60, 0.0, a));
    assert_eq!(base, takes::inputs_key(&one(60, 0.0, a)));
    assert_ne!(base, takes::inputs_key(&one(61, 0.0, a)), "a relabel");
    assert_ne!(base, takes::inputs_key(&one(60, 0.5, a)), "a tail cut");
    assert_ne!(base, takes::inputs_key(&one(60, 0.0, b)), "another measurement");
}

#[test]
fn a_fast_then_slow_decay_is_no_rise() {
    // a decay that falls fast, then slowly lies above the line through its long tail at the start
    // of the window; it never rises: no flag (14 of 67 rise flags sat at 1.1 s, Grand A's F#2 among them)
    let knee = |n: i64| {
        let n_frames = (8.0 / audio::ENV_HOP_S) as usize;
        let env_db = (0..n_frames).map(|i| {
            let t = i as f32 * audio::ENV_HOP_S;
            if n == 64 { -10.0 - 30.0 * t.min(2.0) - 3.0 * (t - 2.0).max(0.0) } else { -10.0 - 3.0 * t }
        }).collect();
        let bands_db = (0..80).map(|_| -10.0).collect();
        Measure { onset_s: 0.0, level_db: -10.0, nyquist: 24000.0, peaks: vec![], decays: vec![], env_db, bands_db }
    };
    let ms: Vec<_> = (60..72).map(|n| (n, 1, format!("{n}.wav"), knee(n))).collect();
    assert!(flagged(&ms, Check::Rise).is_empty(), "{:?}", flagged(&ms, Check::Rise));
}

#[test]
fn a_rise_below_a_16_bit_step_is_no_rise() {
    // a note decays into digital silence (−120 dB), then one-step blips reach −97 dBFS: no rise (Grand
    // C's D4 v2 rose 17 dB at 22.5 s, inaudible); the same blips at −60 dBFS are one
    let blip = |at_db: f32| {
        let env_db: Vec<f32> = (0..(30.0 / audio::ENV_HOP_S) as usize).map(|i| {
            let t = i as f32 * audio::ENV_HOP_S;
            if (22.0..22.5).contains(&t) { at_db } else { (-20.0 - 8.0 * t).max(-120.0) }
        }).collect();
        let m = Measure { onset_s: 0.0, level_db: -20.0, nyquist: 24000.0, peaks: vec![], decays: vec![], env_db, bands_db: vec![] };
        takes::numbers(&m, 100.0, 0.0).rise_db.unwrap()
    };
    assert!(blip(-97.0) < 1.0, "{}", blip(-97.0));
    assert!(blip(-60.0) > 20.0, "{}", blip(-60.0));
}

#[test]
fn thresholds_are_written_as_shown_and_in_place() {
    use pe_editor::project::{Project, Repo};
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("projects/p")).unwrap();
    std::fs::write(dir.path().join("projects/p/project.toml"),
        "[project]\nfamily = \"struck\"\n\n# the checks, by ear\n[take_checks]\nrise = 6.0\n\n[strings]\ndesign = \"grand\"\n").unwrap();
    let repo = Repo { root: dir.path().into(), projects: dir.path().join("projects"), data: dir.path().join("data") };
    let mut p = Project::load(&repo, "p").unwrap();
    // a slider's f32 3.7 is written 3.7 (not 3.700000047683716); the table keeps its place and comment
    takes::write_thresholds(&mut p, &Thresholds { decay: 3.7, rise: 6.0, ..Thresholds::DEFAULT }).unwrap();
    assert_eq!(p.text(), "[project]\nfamily = \"struck\"\n\n# the checks, by ear\n[take_checks]\nrise = 6.0\ndecay = 3.7\n\n[strings]\ndesign = \"grand\"\n");
    // back at the defaults: the key goes, then the table
    takes::write_thresholds(&mut p, &Thresholds { rise: 6.0, ..Thresholds::DEFAULT }).unwrap();
    assert!(p.text().contains("[take_checks]\nrise = 6.0\n\n[strings]"), "{}", p.text());
    takes::write_thresholds(&mut p, &Thresholds::DEFAULT).unwrap();
    assert!(!p.text().contains("take_checks"), "{}", p.text());
}
