//! The roll's body (`roll::bands`, `roll::body`): responses split into the correction's 61 bands
//! and merged back, a body described and applied (the board's radiation following), the two
//! microphones' colors equalized. Needs the exported instruments for the later tests.

use pianoengine::roll::bands::{self, grid};
use pianoengine::roll::body::{self, Body};
use pianoengine::roll::files::Export;
use std::path::Path;

const A: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const E: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_d");
const SR: f64 = 48000.0;

fn noise(n: usize, seed: u64) -> Vec<f64> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        })
        .collect()
}

#[test]
fn split_then_merge_returns_the_response() {
    let x = noise(24000, 7);
    let y = bands::merge(&bands::split(&x, SR), SR);
    let peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let err = x.iter().zip(&y).fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
    assert_eq!(y.len(), x.len());
    assert!(err < 1e-12 * peak, "error {err}");
}

#[test]
fn masks_squares_sum_to_one() {
    let m = bands::masks(96000, SR);
    assert_eq!(m.len(), 61);
    for bin in 0..m[0].len() {
        let s: f64 = m.iter().map(|row| row[bin] * row[bin]).sum();
        assert!((s - 1.0).abs() < 1e-9, "bin {bin}: {s}");
    }
}

#[test]
fn the_grid_is_the_corrections() {
    let g = grid();
    assert_eq!(g[0], 20.0);
    assert!((g[60] - 20480.0).abs() < 1e-9);
}

fn ready() -> bool {
    Path::new(A).exists() && Path::new(E).exists()
}

#[test]
fn a_correction_of_another_layout_is_refused() {
    // the body reads the correction as the 15 notes × the 61 bands of `grid`: any other layout is
    // named, never read as that
    if !ready() {
        return;
    }
    let a = Export::read(Path::new(A)).unwrap();
    assert!(body::describe(&a).is_ok());
    let mut notes = a.clone();
    notes.json["correction"]["notes"][3] = 40.0.into();
    let mut bands = a.clone();
    bands.json["correction"]["grid_hz"].as_array_mut().unwrap().pop();
    let mut none = a.clone();
    none.json.as_object_mut().unwrap().remove("correction");
    for (what, e) in [("notes", notes), ("bands", bands), ("none", none)] {
        let msg = body::describe(&e).err().unwrap_or_else(|| panic!("{what}: read")).0;
        assert!(msg.contains("correction"), "{what}: {msg}");
    }
}

#[test]
fn applying_its_own_body_changes_nothing() {
    if !ready() {
        return;
    }
    let orig = Export::read(Path::new(A)).unwrap();
    let mut e = orig.clone();
    let own = body::describe(&e).unwrap();
    body::apply(&mut e, &own).unwrap();
    for (a, b) in orig.responses().unwrap().iter().zip(e.responses().unwrap()) {
        let peak = a.ir.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let err = a.ir.iter().zip(&b.ir).fold(0.0f64, |m, (x, y)| m.max((x - y).abs()));
        assert!(err < 1e-6 * peak, "a response moved by {err}");
    }
    // the responses are stored as f32: their levels move by rounding, the board by as little
    for (x, y) in orig.f64s("/board/r").unwrap().iter().zip(e.f64s("/board/r").unwrap()) {
        assert!((x - y).abs() <= 1e-6 * x.abs(), "the board moved");
    }
}

fn rms_db(a: &[f64], b: &[f64]) -> f64 {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    let m = d.iter().sum::<f64>() / d.len() as f64;
    (d.iter().map(|v| (v - m).powi(2)).sum::<f64>() / d.len() as f64).sqrt()
}

/// A's body moved toward E's by `share`: the levels' and the rates' rms error from body::STEER_FROM_HZ
/// up, where the steering judges them
fn moved(share: f64) -> (f64, f64) {
    let mut e = Export::read(Path::new(A)).unwrap();
    let (a, b) = (body::describe(&e).unwrap(), body::describe(&Export::read(Path::new(E)).unwrap()).unwrap());
    let mix = |x: f64, y: f64| x + share * (y - x);
    let target = Body {
        level: std::array::from_fn(|k| mix(a.level[k], b.level[k])),
        rate: std::array::from_fn(|k| mix(a.rate[k], b.rate[k])),
        corr: a.corr.iter().zip(&b.corr).map(|(&x, &y)| mix(x, y)).collect(),
    };
    body::apply(&mut e, &target).unwrap();
    let after = body::describe(&e).unwrap();
    assert!(after.corr.iter().zip(&target.corr).all(|(a, b)| (a - b).abs() < 1e-9));
    let g = grid();
    let up: Vec<usize> = (0..61).filter(|&k| g[k] >= body::STEER_FROM_HZ).collect();
    let pick = |v: &[f64; 61]| up.iter().map(|&k| v[k]).collect::<Vec<_>>();
    let hi: Vec<usize> = (0..61).filter(|&k| g[k] >= body::STEER_FROM_HZ).collect();
    let rate = (hi.iter().map(|&k| (after.rate[k] - target.rate[k]).powi(2)).sum::<f64>() / hi.len() as f64).sqrt();
    (rms_db(&pick(&after.level), &pick(&target.level)), rate)
}

#[test]
fn applying_a_nearby_body_reproduces_it() {
    if !ready() {
        return;
    }
    let (level, rate) = moved(0.5);
    assert!(level < 0.5, "levels {level:.2} dB rms off");
    assert!(rate < 15.0, "rates {rate:.1} dB/s rms off");
}

#[test]
fn applying_a_far_body_comes_close() {
    if !ready() {
        return;
    }
    let (level, rate) = moved(1.0);
    assert!(level < 1.5, "levels {level:.2} dB rms off");
    assert!(rate < 25.0, "rates {rate:.1} dB/s rms off");
}

#[test]
fn the_board_follows_the_responses() {
    if !ready() {
        return;
    }
    let orig = Export::read(Path::new(A)).unwrap();
    let mut e = orig.clone();
    let target = body::describe(&Export::read(Path::new(E)).unwrap()).unwrap();
    let before = body::levels(&orig).unwrap();
    body::apply(&mut e, &target).unwrap();
    let after = body::levels(&e).unwrap();
    let g = grid();
    let band: Vec<usize> = (0..61).filter(|&k| g[k] >= 500.0 && g[k] <= 1000.0).collect();
    let change = band.iter().map(|&k| after[k] - before[k]).sum::<f64>() / band.len() as f64;
    let f = orig.f64s("/board/f").unwrap();
    let (r0, r1) = (orig.f64s("/board/r").unwrap(), e.f64s("/board/r").unwrap());
    let mut ratios: Vec<f64> =
        (0..f.len()).filter(|&i| (500.0..=1000.0).contains(&f[i]) && r0[i] != 0.0).map(|i| 20.0 * (r1[i] / r0[i]).abs().log10()).collect();
    ratios.sort_by(f64::total_cmp);
    let median = ratios[ratios.len() / 2];
    assert!((median - change).abs() < 1.5, "the board moved {median:.1} dB, the responses {change:.1} dB");
}

#[test]
fn widening_equalizes_the_microphones() {
    if !ready() {
        return;
    }
    let orig = Export::read(Path::new(E)).unwrap();
    let mut e = orig.clone();
    body::widen(&mut e).unwrap();
    let rs = e.responses().unwrap();
    let g = grid();
    for pol in 0..2 {
        let pair: Vec<_> = rs.iter().filter(|r| r.pol == pol).collect();
        let (l, r) = (bands::band_stats(&pair[0].ir, SR).0, bands::band_stats(&pair[1].ir, SR).0);
        // from body::STEER_FROM_HZ up: below, a band of the ringing boards holds less than one mode
        for k in (0..61).filter(|&k| g[k] >= body::STEER_FROM_HZ) {
            assert!((l[k] - r[k]).abs() <= 3.0, "pol {pol}, {:.0} Hz: L−R {:.1} dB", g[k], l[k] - r[k]);
        }
        let energy = |e: &Export| e.responses().unwrap().iter().filter(|r| r.pol == pol).map(|r| r.ir.iter().map(|v| v * v).sum::<f64>()).sum::<f64>();
        assert!((energy(&e) / energy(&orig) - 1.0).abs() < 1e-6);
    }
}

#[test]
fn a_body_weighs_and_unweighs() {
    let b = Body { level: [1.0; 61], rate: [400.0; 61], corr: vec![2.0; 915] };
    let w = b.weighted();
    assert_eq!(w.len(), 61 + 61 + 915);
    assert!((w[61] - 20.0).abs() < 1e-12);
    let back = Body::unweighted(&w);
    assert!(back.corr.iter().all(|&c| (c - 2.0).abs() < 1e-12) && back.rate.iter().all(|&r| (r - 400.0).abs() < 1e-9));
}

#[test]
fn notched_fundamentals_are_lifted() {
    // three responses summed put some key's fundamental deep in a notch (117 of 140 donor sets,
    // 2026-10-17; here −18 dB, −14.6 since Grand G's response lost its low hump, step 181): lifted to
    // −12 dB by narrow peaks, the bands' color barely moved
    let dir = |n: &str| Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(n);
    let load = |n: &str| Export::read(&dir(n)).ok();
    let (Some(mut e), Some(b), Some(c), Some(d)) = (load("grand_g"), load("grand_b"), load("grand_f"), load("grand_d")) else { return };
    pianoengine::roll::parts::fine_structure(&mut e, &[&b, &c, &d]).unwrap();
    let f: Vec<f64> = pianoengine::roll::measure::fundamentals(&e).iter().map(|k| k.1).collect();
    let worst = |e: &Export| e.responses().unwrap().iter().filter(|r| r.pol == 0).map(|r| bands::notch_depths(&r.ir, SR, &f).into_iter().fold(f64::INFINITY, f64::min)).fold(f64::INFINITY, f64::min);
    assert!(worst(&e) < -13.0, "the mix: {:.1} dB", worst(&e));
    let before = e.responses().unwrap();
    body::lift_notches(&mut e).unwrap();
    assert!(worst(&e) > -13.0, "{:.1} dB", worst(&e));
    for (a, b) in before.iter().zip(e.responses().unwrap()) {
        let (la, lb) = (bands::band_stats(&a.ir, SR).0, bands::band_stats(&b.ir, SR).0);
        let off = (0..61).map(|k| (lb[k] - la[k]).abs()).fold(0.0f64, f64::max);
        assert!(off < 1.0, "a band moved {off:.2} dB");
        let (ea, eb) = (a.ir.iter().map(|v| v * v).sum::<f64>(), b.ir.iter().map(|v| v * v).sum::<f64>());
        assert!((ea / eb - 1.0).abs() < 1e-5, "the energy moved {:.2e}", ea / eb - 1.0); // stored as 32-bit floats
    }
}
