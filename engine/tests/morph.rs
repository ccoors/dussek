//! An instrument from weighted instruments (`Instrument::morph`): a morph of one is the instrument,
//! the weights are normalized and named, what cannot morph is refused; the rules keep every value
//! between the sides and the order of the sides does not matter. Needs the exported instruments
//! (skipped without them).

mod common;
use dussek::physics::Excitation;
use dussek::mic::MicPosition;
use dussek::{Engine, Event, Instrument, RadiationCache, Tuning};

const A: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const E: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_d");
const C: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_f");
const UPRIGHT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/upright_a");
const HARPSICHORD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/flemish");
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/registers_fixture");

/// key 60, then a chord with the pedal down; both channels appended
fn render(inst: Instrument) -> Vec<f32> {
    let mut e = Engine::with_tuning(inst, 48000.0, Tuning::default()).expect("engine");
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    let (mut out_l, mut out_r) = (Vec::new(), Vec::new());
    for block in 0..120 {
        match block {
            0 => e.queue(0, Event::NoteOn { key: 60, speed: 2.0 }),
            20 => {
                e.queue(0, Event::PedalDown);
                e.queue(0, Event::NoteOn { key: 48, speed: 2.0 });
                e.queue(0, Event::NoteOn { key: 64, speed: 2.0 });
            }
            _ => {}
        }
        e.process(&mut l, &mut r);
        out_l.extend_from_slice(&l);
        out_r.extend_from_slice(&r);
    }
    out_l.extend(out_r);
    out_l
}

#[test]
fn a_morph_of_one_is_the_instrument() {
    let Ok(a) = Instrument::load(A) else { return };
    let plain = render(a);
    assert!(plain.iter().any(|s| s.abs() > 1e-3));
    assert_eq!(render(Instrument::morph(&[(A, 1.0)]).unwrap()), plain);
    assert_eq!(render(Instrument::morph(&[(A, 1.0), (E, 0.0)]).unwrap()), plain);
    assert_eq!(render(Instrument::morph(&[(A, 1.0), (E, 1e-5)]).unwrap()), plain); // rounds to 0
}

#[test]
fn weights_are_normalized_and_named() {
    let Ok(m) = Instrument::morph(&[(A, 2.0), (E, 6.0)]) else { return };
    let full = |d: &str| std::fs::canonicalize(d).unwrap().display().to_string();
    assert_eq!(m.profile, format!("morph:{}=0.2500,{}=0.7500", full(A), full(E)));
}

#[test]
fn two_folders_of_one_name_are_two_morphs() {
    // the radiation cache keys on the profile: a folder given as "." or another folder of the same
    // name must not share another's entries
    let Ok(m) = Instrument::morph(&[(A, 0.5), (E, 0.5)]) else { return };
    let dot = Instrument::morph(&[(format!("{A}/."), 0.5), (E.to_string(), 0.5)]).unwrap();
    assert_eq!(dot.profile, m.profile);
    let elsewhere = copies("same_name", &[A], false);
    let other = Instrument::morph(&[(elsewhere[0].clone(), 0.5), (E.into(), 0.5)]).unwrap();
    assert_ne!(other.profile, m.profile);
}

#[test]
fn what_cannot_morph_is_refused_by_name() {
    if Instrument::load(A).is_err() {
        return;
    }
    for (other, why) in [(HARPSICHORD, "flemish: cannot morph"), (FIXTURE, "registers_fixture: cannot morph")] {
        let e = Instrument::morph(&[(A, 0.5), (other, 0.5)]).err().expect("refused").to_string();
        assert!(e.contains(why), "{e}");
    }
    assert!(Instrument::morph(&[(A, -0.1), (E, 1.0)]).is_err());
    assert!(Instrument::morph(&[(A, 0.0), (E, 0.0)]).is_err());
    assert!(Instrument::morph::<&str>(&[]).is_err());
}

/// the per-key strings and hammers, the hammer speeds and the correction (not the room's t60: its
/// length follows the union of the grids; not the release peaks: they carry the measured level)
fn keyed(i: &Instrument) -> Vec<f64> {
    let mut v = Vec::new();
    for n in &i.notes {
        let s = &n.spec.string;
        v.extend([s.f0, s.b, s.b1, s.b3, s.kappa, s.t]);
        if let Excitation::Hammer(h) = &n.spec.excitation {
            v.extend([h.mass, h.felt_k, h.strike_pos]);
        }
    }
    v.extend(i.velocity_speed);
    v.extend(i.globals.correction.iter().flat_map(|c| c.c_db.iter().copied()));
    v
}

#[test]
fn the_order_of_the_sides_does_not_matter() {
    let Ok(x) = Instrument::morph(&[(A, 0.3), (E, 0.7)]) else { return };
    let y = Instrument::morph(&[(E, 0.7), (A, 0.3)]).unwrap();
    let (kx, ky) = (keyed(&x), keyed(&y));
    assert_eq!(kx.len(), ky.len());
    for (p, q) in kx.iter().zip(&ky) {
        assert!((p - q).abs() <= 1e-9 * p.abs().max(q.abs()), "{p} {q}");
    }
}

#[test]
fn every_scale_lies_between_the_sides() {
    let Ok(m) = Instrument::morph(&[(A, 0.2), (E, 0.5), (C, 0.3)]) else { return };
    let sides: Vec<Vec<f64>> = [A, E, C].iter().map(|d| keyed(&Instrument::load(d).unwrap())).collect();
    for (i, v) in keyed(&m).iter().enumerate() {
        let (lo, hi) = sides.iter().map(|s| s[i]).fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(x), h.max(x)));
        assert!(v.is_finite() && *v >= lo - 1e-12 * lo.abs() && *v <= hi + 1e-12 * hi.abs(), "entry {i}: {v} not in {lo}…{hi}");
    }
}

#[test]
fn level_corrections_blend_with_none() {
    // Grand A has level corrections, an upright without any halfway gives half of A's. Every piano
    // carries its own since the uprights were voiced even (step 168): Upright A's taken off a copy
    let Ok(a) = Instrument::load(A) else { return };
    let none = std::env::temp_dir().join(format!("pe-morph-none-{}", std::process::id()));
    std::fs::create_dir_all(&none).unwrap();
    for f in ["data.bin", "level.json"] {
        std::fs::copy(std::path::Path::new(UPRIGHT).join(f), none.join(f)).unwrap();
    }
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(UPRIGHT).join("instrument.json")).unwrap()).unwrap();
    for n in doc["notes"].as_array_mut().unwrap() {
        n.as_object_mut().unwrap().remove("level_db");
    }
    std::fs::write(none.join("instrument.json"), doc.to_string()).unwrap();
    let m = Instrument::morph(&[(A, 0.5), (none.to_str().unwrap(), 0.5)]);
    let _ = std::fs::remove_dir_all(&none);
    let m = m.unwrap();
    let k = a.notes.iter().position(|n| n.level_db.first().is_some_and(|t| t[0].abs() > 1.0)).expect("a correction at pp");
    assert!((m.notes[k].level_db[0][0] - 0.5 * a.notes[k].level_db[0][0]).abs() < 1e-9);
}

/// the board's modes reordered by frequency (an export's are not guaranteed sorted)
fn sorted_by_f(b: &dussek::physics::Board) -> dussek::physics::Board {
    let mut o: Vec<usize> = (0..b.f.len()).collect();
    o.sort_by(|&i, &j| b.f[i].total_cmp(&b.f[j]));
    let by = |v: &[f64]| o.iter().map(|&i| v[i]).collect::<Vec<f64>>();
    let mut s = b.clone();
    s.f = by(&b.f);
    s.sigma = by(&b.sigma);
    s.phi = by(&b.phi);
    s.r = [by(&b.r[0]), by(&b.r[1])];
    s.r_h = [by(&b.r_h[0]), by(&b.r_h[1])];
    s
}

fn close(x: &[f64], y: &[f64]) -> bool {
    x.len() == y.len() && x.iter().zip(y).all(|(p, q)| (p - q).abs() <= 1e-9 * p.abs().max(q.abs()).max(1e-300))
}

#[test]
fn a_morph_with_itself_gives_the_instrument_back() {
    let Ok(a) = Instrument::load(A) else { return };
    let m = Instrument::morph(&[(A, 0.5), (A, 0.5)]).unwrap();
    let (ba, bm) = (sorted_by_f(&a.globals.board), &m.globals.board);
    assert!(close(&ba.f, &bm.f) && close(&ba.sigma, &bm.sigma) && close(&ba.phi, &bm.phi));
    assert!(close(&ba.r[0], &bm.r[0]) && close(&ba.r_h[1], &bm.r_h[1]));
    assert!(close(&keyed(&a), &keyed(&m)));
    for (x, y) in a.radiation_v.body.ir[0].iter().zip(&m.radiation_v.body.ir[0]) {
        assert!((x - y).abs() <= 1e-6, "{x} {y}");
    }
    // and everything else a render reaches (the noises, dampers, pedal, room): the same sound
    let (ra, rm) = (render(a), render(m));
    let peak = ra.iter().fold(0.0f32, |p, x| p.max(x.abs()));
    let worst = ra.iter().zip(&rm).fold(0.0f32, |w, (x, y)| w.max((x - y).abs()));
    assert!(worst <= 1e-4 * peak, "worst {worst} against a peak of {peak}");
}

#[test]
fn an_upright_fading_out_leaves_the_grand_s_board() {
    let (Ok(a), Ok(m)) = (Instrument::load(A), Instrument::morph(&[(A, 0.999), (UPRIGHT, 0.001)])) else { return };
    let fa = sorted_by_f(&a.globals.board).f;
    assert_eq!(m.globals.board.f.len(), fa.len()); // 1 220, not 844
    for (x, y) in fa.iter().zip(&m.globals.board.f) {
        assert!((y / x - 1.0).abs() < 2e-3, "{x} → {y}");
    }
}

#[test]
fn a_small_step_moves_the_board_and_responses_a_little() {
    let (Ok(x), Ok(y)) = (Instrument::morph(&[(A, 0.70), (E, 0.30)]), Instrument::morph(&[(A, 0.69), (E, 0.31)])) else { return };
    for (p, q) in x.globals.board.f.iter().zip(&y.globals.board.f) {
        assert!((q / p - 1.0).abs() < 0.05);
    }
    let e = |v: &[f32]| v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
    let d: Vec<f32> = x.radiation_v.body.ir[0].iter().zip(&y.radiation_v.body.ir[0]).map(|(p, q)| p - q).collect();
    assert!(e(&d) < 0.01 * e(&x.radiation_v.body.ir[0]), "{} of {}", e(&d), e(&x.radiation_v.body.ir[0]));
}

#[test]
fn halfway_s_board_is_the_geometric_mean_by_rank() {
    let (Ok(a), Ok(e), Ok(m)) = (Instrument::load(A), Instrument::load(E), Instrument::morph(&[(A, 0.5), (E, 0.5)])) else { return };
    let (fa, fe) = (sorted_by_f(&a.globals.board).f, sorted_by_f(&e.globals.board).f);
    let want: Vec<f64> = fa.iter().zip(&fe).map(|(x, y)| (x * y).sqrt()).collect();
    assert!(close(&want, &m.globals.board.f));
}

/// the level at ff away from what the morph's own level measures (keys 48, 60 and 72 at the recording's
/// position): keys 36, 66 and 84 at the ff anchor's speed at the player's position, each one's peak
/// over 2 s, the mean in dB
fn ff_level(inst: Instrument) -> f64 {
    let speed = inst.velocity_speed[2];
    let mut sum = 0.0;
    let t = Tuning { mic: MicPosition::Player, ..Tuning::default() };
    for key in [36, 66, 84] {
        let mut e = Engine::with_tuning(inst.clone(), 48000.0, t).expect("engine");
        e.queue(0, Event::NoteOn { key, speed });
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let mut p = 0.0f32;
        for _ in 0..375 {
            e.process(&mut l, &mut r);
            p = l.iter().chain(&r).fold(p, |m, x| m.max(x.abs()));
        }
        sum += 20.0 * (p as f64).log10();
    }
    sum / 3.0
}

#[test]
fn halfway_plays_as_loud_as_its_ends() {
    let Ok(m) = Instrument::morph(&[(A, 0.5), (E, 0.5)]) else { return };
    let l = common::par(3, |i| {
        ff_level(match i {
            0 => Instrument::load(A).unwrap(),
            1 => Instrument::load(E).unwrap(),
            _ => m.clone(),
        })
    });
    println!("ff level: A {:.2} dB, E {:.2} dB, halfway {:.2} dB", l[0], l[1], l[2]);
    // 1.5 dB before the rooms' cuts (design log step 123): a morph averages the sides' cuts and their
    // compensations apart, and at Player across these keys halfway now sits ~1 dB lower than it did
    assert!((l[2] - 0.5 * (l[0] + l[1])).abs() <= 2.5, "A {:.1} E {:.1} halfway {:.1} dB", l[0], l[1], l[2]);
}

#[test]
fn each_morph_has_its_own_cache_entry() {
    let Ok(x) = Instrument::morph(&[(A, 0.5), (E, 0.5)]) else { return };
    let y = Instrument::morph(&[(A, 0.25), (E, 0.75)]).unwrap();
    let mut cache = RadiationCache::default();
    let at = |cache: &mut RadiationCache, i: &Instrument| {
        let mut t = i.tuning;
        t.mic = MicPosition::Player;
        Engine::with_cache(i.clone(), 48000.0, t, cache).expect("engine");
    };
    at(&mut cache, &x);
    at(&mut cache, &y);
    assert_eq!(cache.builds(), 2);
    at(&mut cache, &Instrument::morph(&[(A, 0.5), (E, 0.5)]).unwrap());
    assert_eq!(cache.builds(), 2);
}

/// copies of instruments `dirs` in the test's temporary directory `name`, each with or without its
/// level.json (written from `Instrument::ff_level`)
fn copies(name: &str, dirs: &[&str], with_level: bool) -> Vec<std::path::PathBuf> {
    dirs.iter()
        .map(|d| {
            let src = std::path::Path::new(d);
            let dst = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name).join(src.file_name().unwrap());
            std::fs::create_dir_all(&dst).unwrap();
            for f in ["instrument.json", "data.bin"] {
                std::fs::copy(src.join(f), dst.join(f)).unwrap();
            }
            let _ = std::fs::remove_file(dst.join("level.json"));
            if with_level {
                let db = Instrument::load(&dst).unwrap().ff_level().unwrap();
                std::fs::write(dst.join("level.json"), format!("{{\"ff_db\": {db}, \"keys\": [48, 60, 72]}}")).unwrap();
            }
            dst
        })
        .collect()
}

#[test]
fn stored_levels_give_the_measured_level() {
    if Instrument::load(A).is_err() {
        return;
    }
    let with = copies("stored_with", &[A, E], true);
    let without = copies("stored_without", &[A, E], false);
    let m1 = Instrument::morph(&[(&with[0], 0.5), (&with[1], 0.5)]).unwrap();
    let m2 = Instrument::morph(&[(&without[0], 0.5), (&without[1], 0.5)]).unwrap();
    let (l1, l2) = (m1.ff_level().unwrap(), m2.ff_level().unwrap());
    assert!((l1 - l2).abs() <= 0.1, "{l1} against {l2}");
}

#[test]
fn the_body_follows_the_main_instrument() {
    let (Ok(a), Ok(m)) = (Instrument::load(A), Instrument::morph(&[(A, 0.3), (UPRIGHT, 0.7)])) else { return };
    assert_eq!(m.body(), a.body());
    let u = Instrument::morph(&[(A, 0.0), (UPRIGHT, 1.0)]).unwrap(); // the main at weight 0
    assert_eq!(u.body(), a.body());
}

/// an engine for `inst` with keys 48, 60 and 64 struck and held for 40 blocks; the last block's peak
fn holding(inst: Instrument) -> (Engine, f32) {
    let mut e = Engine::with_tuning(inst, 48000.0, Tuning::default()).expect("engine");
    for key in [48, 60, 64] {
        e.queue(0, Event::NoteOn { key, speed: 2.0 });
    }
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    let mut p = 0.0;
    for _ in 0..40 {
        e.process(&mut l, &mut r);
        p = l.iter().chain(&r).fold(0.0f32, |m, x| m.max(x.abs()));
    }
    (e, p)
}

/// the peak of `e`'s next block
fn next_peak(e: &mut Engine) -> f32 {
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    e.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|x| x.is_finite()));
    l.iter().chain(&r).fold(0.0f32, |m, x| m.max(x.abs()))
}

#[test]
fn another_mix_does_not_take_the_notes_over() {
    // the strings' state does not carry between different strings (Grand G's held chord played 21 dB
    // too loud in Grand D): another mix is another instrument, the plugin fades the old engine out
    let Ok(x) = Instrument::morph(&[(A, 0.5), (E, 0.5)]) else { return };
    let (mut old, _) = holding(x);
    let mut new = Engine::with_tuning(Instrument::morph(&[(A, 0.7), (E, 0.3)]).unwrap(), 48000.0, Tuning::default()).unwrap();
    assert!(!new.adopt(&mut old));
}

#[test]
fn the_same_mix_in_another_tuning_takes_the_notes_over() {
    let Ok(x) = Instrument::morph(&[(A, 0.5), (E, 0.5)]) else { return };
    let (mut old, before) = holding(x.clone());
    let t = Tuning { pitch_ratio: 442.0 / 440.0, ..Tuning::default() };
    let mut new = Engine::with_tuning(x, 48000.0, t).unwrap();
    assert!(new.adopt(&mut old));
    let after = next_peak(&mut new);
    assert!(after > 1e-3 && (20.0 * (after / before).log10()).abs() < 6.0, "{before} → {after}");
}

#[test]
fn another_instrument_still_refuses() {
    let (Ok(a), Ok(e)) = (Instrument::load(A), Instrument::load(E)) else { return };
    let (mut old, _) = holding(a);
    let mut new = Engine::with_tuning(e, 48000.0, Tuning::default()).unwrap();
    assert!(!new.adopt(&mut old));
}

#[test]
fn a_grand_upright_handover_never_panics() {
    let Ok(x) = Instrument::morph(&[(A, 0.6), (UPRIGHT, 0.4)]) else { return };
    let (mut old, _) = holding(x);
    let mut new = Engine::with_tuning(Instrument::morph(&[(A, 0.4), (UPRIGHT, 0.6)]).unwrap(), 48000.0, Tuning::default()).unwrap();
    let _ = new.adopt(&mut old);
    for _ in 0..20 {
        next_peak(&mut new);
    }
}

