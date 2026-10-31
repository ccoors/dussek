//! The roll's strings and fine structure (`roll::parts`): each key's strings and hammers blended
//! by the morph's rules, the responses' fine structure mixed from other pianos above 100–200 Hz
//! with the frame's own bass kept. Needs the exported instruments (skipped without them).

use pianoengine::roll::bands;
use pianoengine::roll::files::Export;
use pianoengine::roll::parts::{blend_strings, fine_structure};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(name)
}

fn load(name: &str) -> Option<Export> {
    Export::read(&dir(name)).ok()
}

const SCALES: [&str; 12] = ["f0", "B", "L", "T", "mass", "b1", "b3", "kappa", "horiz_gain", "horiz_kappa", "f_max", "scatter_cents"];

fn num(v: &Value, ptr: &str) -> f64 {
    v.pointer(ptr).and_then(Value::as_f64).unwrap_or_else(|| panic!("no {ptr}"))
}

#[test]
fn one_donor_at_weight_one_gives_its_strings() {
    let (Some(mut a), Some(e)) = (load("grand_a"), load("grand_d")) else { return };
    let orig_a = a.clone();
    blend_strings(&mut a, &[(&e, 1.0)]).unwrap();
    for k in [0usize, 30, 60, 87] {
        for f in SCALES {
            let (got, want) = (num(&a.json, &format!("/notes/{k}/string/{f}")), num(&e.json, &format!("/notes/{k}/string/{f}")));
            assert!((got - want).abs() <= 1e-12 * want.abs(), "note {k} {f}: {got} against {want}");
        }
        for f in ["mass", "felt_K", "felt_p", "strike_pos"] {
            let (got, want) = (num(&a.json, &format!("/notes/{k}/excitation/{f}")), num(&e.json, &format!("/notes/{k}/excitation/{f}")));
            assert!((got - want).abs() <= 1e-12 * want.abs(), "note {k} {f}");
        }
        // what is not blended stays the frame's; the release noise's ratios follow the speeds
        for f in ["impacts", "band_db", "peak"] {
            assert_eq!(a.json.pointer(&format!("/notes/{k}/release/{f}")), orig_a.json.pointer(&format!("/notes/{k}/release/{f}")), "note {k} {f}");
        }
        assert_eq!(a.json.pointer(&format!("/notes/{k}/release/ratio_db")), e.json.pointer(&format!("/notes/{k}/release/ratio_db")));
    }
    assert_eq!(a.json.get("board"), orig_a.json.get("board"));
    assert_eq!(a.data, orig_a.data);
}

#[test]
fn weights_are_normalized() {
    let (Some(a), Some(e)) = (load("grand_a"), load("grand_d")) else { return };
    let (mut x, mut y, mut z, mut w) = (a.clone(), a.clone(), a.clone(), a.clone());
    blend_strings(&mut x, &[(&a, 2.0), (&e, 2.0)]).unwrap();
    blend_strings(&mut y, &[(&a, 0.5), (&e, 0.5)]).unwrap();
    assert_eq!(x.json, y.json);
    blend_strings(&mut z, &[(&a, 1.0), (&e, 0.0)]).unwrap();
    blend_strings(&mut w, &[(&a, 1.0)]).unwrap();
    assert_eq!(z.json, w.json);
    assert!(blend_strings(&mut w, &[(&e, 0.0)]).is_err());
}

#[test]
fn the_release_ratio_follows_the_blended_speeds() {
    // the release noise's ratio per dynamic sits over ln of that dynamic's hammer speed: blended
    // speeds take the ratios blended with them (as the morph's)
    let (Some(mut a), Some(e)) = (load("grand_a"), load("grand_d")) else { return };
    let orig_a = a.clone();
    blend_strings(&mut a, &[(&orig_a, 0.5), (&e, 0.5)]).unwrap();
    for k in [0usize, 40, 87] {
        let at = |x: &Export| x.json["notes"][k]["release"]["ratio_db"].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect::<Vec<_>>();
        let (ra, re, rm) = (at(&orig_a), at(&e), at(&a));
        assert_eq!(ra.len(), re.len());
        for j in 0..rm.len() {
            for c in 0..2 {
                assert!((rm[j][c] - 0.5 * (ra[j][c] + re[j][c])).abs() < 1e-9, "key {k}, dynamic {j}, column {c}: {} against {} and {}", rm[j][c], ra[j][c], re[j][c]);
            }
        }
    }
}

#[test]
fn scales_blend_geometrically() {
    let (Some(mut a), Some(e)) = (load("grand_a"), load("grand_d")) else { return };
    let (ta, te) = (num(&a.json, "/notes/39/string/T"), num(&e.json, "/notes/39/string/T"));
    let (pa, pe) = (num(&a.json, "/notes/39/excitation/felt_p"), num(&e.json, "/notes/39/excitation/felt_p"));
    let (sa, se) = (num(&a.json, "/velocity/speed/1"), num(&e.json, "/velocity/speed/1"));
    let a2 = a.clone();
    blend_strings(&mut a, &[(&a2, 0.5), (&e, 0.5)]).unwrap();
    assert!((num(&a.json, "/notes/39/string/T") - (ta * te).sqrt()).abs() <= 1e-12 * ta);
    assert!((num(&a.json, "/notes/39/excitation/felt_p") - (pa + pe) / 2.0).abs() <= 1e-12);
    assert!((num(&a.json, "/velocity/speed/1") - (sa * se).sqrt()).abs() <= 1e-12 * sa);
}

#[test]
fn a_different_string_count_keeps_the_frames() {
    let (Some(mut a), Some(mut e)) = (load("grand_a"), load("grand_d")) else { return };
    let s_a = num(&a.json, "/notes/30/string/S");
    let det_a = a.json.pointer("/notes/30/string/detune_cents").unwrap().clone();
    *e.json.pointer_mut("/notes/30/string/S").unwrap() = Value::from(2);
    *e.json.pointer_mut("/notes/30/string/detune_cents").unwrap() = serde_json::json!([-3.0, 3.0]);
    let a2 = a.clone();
    blend_strings(&mut a, &[(&a2, 0.5), (&e, 0.5)]).unwrap();
    assert_eq!(num(&a.json, "/notes/30/string/S"), s_a);
    assert_eq!(a.json.pointer("/notes/30/string/detune_cents").unwrap(), &det_a);
}

/// `x` lowpassed at `hz` (an FFT mask at twice the length)
fn low(x: &[f64], hz: f64) -> Vec<f64> {
    let n = 2 * x.len();
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let (f, i) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
    let mut buf = vec![0.0; n];
    buf[..x.len()].copy_from_slice(x);
    let mut spec = f.make_output_vec();
    f.process(&mut buf, &mut spec).unwrap();
    for (k, c) in spec.iter_mut().enumerate() {
        if k as f64 * 48000.0 / n as f64 > hz {
            *c = realfft::num_complex::Complex::new(0.0, 0.0);
        }
    }
    let mut out = i.make_output_vec();
    i.process(&mut spec, &mut out).unwrap();
    out[..x.len()].iter().map(|v| v / n as f64).collect()
}

fn corr(a: &[f64], b: &[f64]) -> f64 {
    let d = |x: &[f64], y: &[f64]| x.iter().zip(y).map(|(p, q)| p * q).sum::<f64>();
    d(a, b) / (d(a, a) * d(b, b)).sqrt()
}

fn mixed() -> Option<(Export, Export)> {
    let (a, c, d, f) = (load("grand_a")?, load("grand_f")?, load("grand_e")?, load("grand_c")?);
    let mut m = a.clone();
    fine_structure(&mut m, &[&c, &d, &f]).unwrap();
    Some((a, m))
}

#[test]
fn the_bass_stays_the_frames() {
    let Some((a, m)) = mixed() else { return };
    for (ra, rm) in a.responses().unwrap().iter().zip(m.responses().unwrap()) {
        let (la, lm) = (low(&ra.ir, 90.0), low(&rm.ir, 90.0));
        // (cutting the filtered response back to its length leaks a little of the mix into it)
        assert!(corr(&la, &lm) >= 0.97, "the bass changed shape: {}", corr(&la, &lm));
        let db = 10.0 * (lm.iter().map(|v| v * v).sum::<f64>() / la.iter().map(|v| v * v).sum::<f64>()).log10();
        assert!(db.abs() <= 1.5, "the bass moved {db:.2} dB");
        let sub = low(&rm.ir, 40.0);
        let share = 10.0 * (sub.iter().map(|v| v * v).sum::<f64>() / rm.ir.iter().map(|v| v * v).sum::<f64>()).log10();
        assert!(share < -30.0, "below 40 Hz: {share:.1} dB of the response");
        // the color from 300 Hz to 5 kHz stays the frame's, the fine structure is new
        let (ba, bm) = (bands::band_stats(&ra.ir, 48000.0).0, bands::band_stats(&rm.ir, 48000.0).0);
        let g = bands::grid();
        let ks: Vec<usize> = (0..61).filter(|&k| (300.0..=5000.0).contains(&g[k])).collect();
        let off = ks.iter().map(|&k| bm[k] - ba[k]).sum::<f64>() / ks.len() as f64;
        // a frame's narrow dip (Grand A's at 320 Hz) is only approached: most bands within 1 dB,
        // none beyond 2.5 dB (the body's apply sets the levels afterwards)
        let errs: Vec<f64> = ks.iter().map(|&k| (bm[k] - ba[k] - off).abs()).collect();
        let close = errs.iter().filter(|&&e| e <= 1.0).count();
        assert!(close * 10 >= ks.len() * 9, "{close} of {} bands within 1 dB", ks.len());
        for (&k, e) in ks.iter().zip(&errs) {
            assert!(*e <= 2.5, "{:.0} Hz: {e:.1} dB", g[k]);
        }
        let mid = |x: &[f64]| {
            let l = low(x, 5000.0);
            let h = low(x, 500.0);
            l.iter().zip(&h).map(|(p, q)| p - q).collect::<Vec<_>>()
        };
        assert!(corr(&mid(&ra.ir), &mid(&rm.ir)).abs() < 0.4, "the fine structure is still the frame's");
    }
}

#[test]
fn the_bass_is_in_phase() {
    let Some((a, m)) = mixed() else { return };
    let (ra, rm) = (a.responses().unwrap(), m.responses().unwrap());
    for pol in 0..2 {
        let pick = |rs: &[pianoengine::roll::files::Response], c: usize| low(&rs.iter().find(|r| r.pol == pol && r.channel == c).unwrap().ir, 150.0);
        let (ca, cm) = (corr(&pick(&ra, 0), &pick(&ra, 1)), corr(&pick(&rm, 0), &pick(&rm, 1)));
        assert!((ca - cm).abs() <= 0.05, "pol {pol}: the bass's L–R correlation {cm:.2}, the frame's {ca:.2}");
    }
}
