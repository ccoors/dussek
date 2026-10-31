//! The board's bass (design 2026-10-23-board-knobs, step 148): a gain on a hammer piano's strings
//! below 125 Hz, the strike's thud through the board kept; on a morph; the strings' own decay
//! stays; the extremes stay finite and die away.
mod common;

use dussek::board_bass::shape;
use dussek::mic::MicPosition;
use dussek::{Engine, Event, Instrument, Params, RadiationCache, Tuning};
use realfft::RealFftPlanner;

const SR: f64 = 48000.0;
const OCTAVES: [f64; 8] = [63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

fn dir(name: &str) -> String {
    format!("{}/../instruments/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn tuning(board_bass_db: f64, mic: MicPosition) -> Tuning {
    Tuning { board_bass_db, mic, ..Tuning::default() }
}

/// `keys` struck together at `velocity` (the knock and the variation off): left and right over `secs`
fn play(mut e: Engine, keys: &[u8], velocity: f64, secs: f64) -> (Vec<f32>, Vec<f32>) {
    e.set_params(Params { knock: 0.0, variation: 0.0, ..Params::default() });
    for &key in keys {
        let speed = e.hammer_speed(key, velocity);
        e.queue(0, Event::NoteOn { key, speed });
    }
    let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
    let (mut ls, mut rs) = (Vec::new(), Vec::new());
    for _ in 0..(secs * SR / 256.0).ceil() as usize {
        e.process(&mut l, &mut r);
        ls.extend_from_slice(&l);
        rs.extend_from_slice(&r);
    }
    (ls, rs)
}

fn render(name: &str, t: Tuning, keys: &[u8], velocity: f64) -> Option<(Vec<f32>, Vec<f32>)> {
    Some(play(common::engine(&dir(name), SR, t)?, keys, velocity, 1.0))
}

/// the power spectrum of a channel (Hann over its whole length)
fn power(x: &[f32]) -> Vec<f64> {
    let n = x.len();
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(n);
    let mut input: Vec<f64> = x.iter().enumerate().map(|(i, &v)| v as f64 * (std::f64::consts::PI * i as f64 / n as f64).sin().powi(2)).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut input, &mut out).expect("fft");
    out.iter().map(|c| c.norm_sqr()).collect()
}

/// both channels' power per octave (dB)
fn octaves((l, r): &(Vec<f32>, Vec<f32>)) -> Vec<f64> {
    let (pl, pr) = (power(l), power(r));
    let hz = SR / l.len() as f64;
    OCTAVES
        .iter()
        .map(|&c| {
            let (lo, hi) = ((c / 2f64.sqrt() / hz) as usize, (c * 2f64.sqrt() / hz) as usize);
            10.0 * (pl[lo..hi].iter().zip(&pr[lo..hi]).map(|(a, b)| a + b).sum::<f64>() + 1e-30).log10()
        })
        .collect()
}

fn change(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(x, y)| y - x).collect()
}

/// both channels' power at the partials 1–4 of `f0` (dB, the peak within ±1.5 %)
fn partials((l, r): &(Vec<f32>, Vec<f32>), f0: f64) -> [f64; 4] {
    let (pl, pr) = (power(l), power(r));
    let hz = SR / l.len() as f64;
    std::array::from_fn(|i| {
        let f = f0 * (i + 1) as f64;
        let (lo, hi) = ((f * 0.985 / hz) as usize, (f * 1.015 / hz) as usize + 1);
        10.0 * (lo..hi).map(|k| pl[k] + pr[k]).fold(0.0, f64::max).log10()
    })
}

fn fundamental_over_partials(p: &[f64; 4]) -> f64 {
    let rest = 10.0 * ((10f64.powf(p[1] / 10.0) + 10f64.powf(p[2] / 10.0) + 10f64.powf(p[3] / 10.0)) / 3.0).log10();
    p[0] - rest
}

fn f0(key: u8) -> f64 {
    440.0 * 2f64.powf((key as f64 - 69.0) / 12.0)
}

/// both channels' power below `hz` over the first `secs` (dB, unwindowed)
fn low_power((l, r): &(Vec<f32>, Vec<f32>), hz: f64, secs: f64) -> f64 {
    let n = (secs * SR) as usize;
    let len = 8192.max(n.next_power_of_two());
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(len);
    let mut sum = 0.0;
    for x in [l, r] {
        let mut input: Vec<f64> = x[..n].iter().map(|&v| v as f64).chain(std::iter::repeat(0.0)).take(len).collect();
        let mut out = fft.make_output_vec();
        fft.process(&mut input, &mut out).expect("fft");
        sum += out[1..(hz * len as f64 / SR) as usize].iter().map(|c| c.norm_sqr()).sum::<f64>();
    }
    10.0 * (sum + 1e-30).log10()
}

#[test]
fn shipped_is_bit_identical() {
    let Some(a) = render("grand_a", Tuning::default(), &[60], 80.0) else { return };
    let b = render("grand_a", tuning(0.0, MicPosition::Recording), &[60], 80.0).unwrap();
    assert!(a == b, "no bass changed the sound");
    // a harpsichord takes none
    let Some(h0) = render("harpsichord", Tuning::default(), &[60], 80.0) else { return };
    let h1 = render("harpsichord", tuning(24.0, MicPosition::Recording), &[60], 80.0).unwrap();
    assert!(h0 == h1, "a harpsichord took the bass");
}

#[test]
fn bass_plus_20_on_grand_f() {
    let Some(a) = render("grand_c", Tuning::default(), &[33], 80.0) else { return };
    let b = render("grand_c", tuning(20.0, MicPosition::Recording), &[33], 80.0).unwrap();
    // the fundamental (55 Hz) takes the shape's share there, partial 2 a little, 3 and 4 none
    let expected = 20.0 * shape(55.0) - 10.0 * ((10f64.powf(20.0 * shape(110.0) / 10.0) + 2.0) / 3.0).log10();
    let rise = fundamental_over_partials(&partials(&b, f0(33))) - fundamental_over_partials(&partials(&a, f0(33)));
    assert!((rise - expected).abs() <= 3.0, "the fundamental re partials 2–4 rose {rise:.1} dB, the shape gives {expected:.1}");
    let d = change(&octaves(&a), &octaves(&b));
    for (c, x) in OCTAVES.iter().zip(&d).filter(|(c, _)| (250.0..=4000.0).contains(*c)) {
        assert!(x.abs() <= 1.0, "{c} Hz moved {x:+.1} dB");
    }
}

#[test]
fn bass_stays_off_a_higher_note_s_strike() {
    // the strike's thud through the board under G3, above the bass's range: +20 dB of bass on
    // the board made it pop (Grand C at Player: below 120 Hz the attack's first 30 ms rose 13.5 dB;
    // under C3 11 dB, as loud as the note)
    let Some(a) = render("grand_c", tuning(0.0, MicPosition::Player), &[55], 80.0) else { return };
    let b = render("grand_c", tuning(20.0, MicPosition::Player), &[55], 80.0).unwrap();
    let rise = low_power(&b, 120.0, 0.03) - low_power(&a, 120.0, 0.03);
    assert!(rise.abs() <= 1.5, "G3's attack below 120 Hz rose {rise:+.1} dB");
}

#[test]
fn bass_costs_no_radiation_build() {
    let Some(inst) = common::load(&dir("grand_a")) else { return };
    let mut cache = RadiationCache::default();
    for db in [0.0, 20.0, -12.0] {
        Engine::with_cache(inst.clone(), SR, tuning(db, MicPosition::Player), &mut cache).expect("engine");
    }
    assert_eq!(cache.builds(), 1, "the bass rebuilt the board's responses");
}

#[test]
fn the_strings_keep_their_decay() {
    let Some(inst) = common::load(&dir("grand_a")) else { return };
    let build = |db: f64| Engine::with_cache(inst.clone(), SR, tuning(db, MicPosition::Recording), &mut RadiationCache::default()).expect("engine");
    let (a, b) = (build(0.0), build(20.0));
    let note = |e: &Engine| e.instrument().notes.iter().find(|n| n.note == 33).expect("A1").phys.clone();
    let (pa, pb) = (note(&a), note(&b));
    assert_eq!(pa.vert.lam, pb.vert.lam, "the strings' vertical decay");
    assert_eq!(pa.horiz.lam, pb.horiz.lam, "the strings' horizontal decay");
    assert_ne!(pa.bridge, pb.bridge, "the correction reaches the strings' output");
}

#[test]
fn a_morph_takes_the_bass() {
    let (a, b) = (dir("grand_a"), dir("grand_d"));
    let Ok(inst) = Instrument::morph(&[(a.as_str(), 0.5), (b.as_str(), 0.5)]) else { return };
    let at = |db: f64| play(Engine::with_cache(inst.clone(), SR, tuning(db, MicPosition::Recording), &mut RadiationCache::default()).expect("engine"), &[33], 80.0, 1.0);
    let (x, y) = (partials(&at(0.0), f0(33)), partials(&at(12.0), f0(33)));
    let expected = 12.0 * shape(f0(33));
    assert!((y[0] - x[0] - expected).abs() <= 1.5, "A1's fundamental rose {:.1} dB, the shape gives {expected:.1}", y[0] - x[0]);
}

#[test]
fn extremes_stay_finite_and_die_away() {
    let root = format!("{}/../instruments", env!("CARGO_MANIFEST_DIR"));
    let Ok(rd) = std::fs::read_dir(&root) else { return };
    let mut pianos: Vec<String> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| common::load(&dir(n)).is_some_and(|i| i.is_hammer_piano()))
        .collect();
    pianos.sort();
    let knobs = [24.0, -12.0];
    let mics = [MicPosition::Recording, MicPosition::Player];
    let mut cases: Vec<(String, MicPosition, Option<f64>)> = Vec::new();
    for p in &pianos {
        for &m in &mics {
            cases.push((p.clone(), m, None));
            cases.extend(knobs.iter().map(|&k| (p.clone(), m, Some(k))));
        }
    }
    let results = common::par(cases.len(), |i| {
        let (p, m, k) = &cases[i];
        let e = common::engine(&dir(p), SR, tuning(k.unwrap_or_default(), *m)).expect("a hammer piano");
        let (l, r) = play(e, &[24, 60, 96], 127.0, 1.0);
        let finite = l.iter().chain(&r).all(|v| v.is_finite());
        let peak = l.iter().chain(&r).fold(0.0f32, |a, v| a.max(v.abs())) as f64;
        // per 0.2 s: the chord ends under its loudest, and after it no frame rises more than 3 dB
        // over the lowest before it (no tail that grows; a bass string builds up for 0.2–0.4 s and
        // beats)
        let energy = |a: usize, b: usize| 10.0 * (l[a..b].iter().chain(&r[a..b]).map(|v| (*v as f64).powi(2)).sum::<f64>() + 1e-30).log10();
        let fifth = l.len() / 5;
        let frames: Vec<f64> = (0..5).map(|i| energy(i * fifth, (i + 1) * fifth)).collect();
        let top = (0..5).max_by(|&a, &b| frames[a].total_cmp(&frames[b])).unwrap();
        let mut low = frames[top];
        let rise = frames[top..].iter().fold(0.0f64, |r, &f| {
            low = low.min(f);
            r.max(f - low)
        });
        (finite, peak, frames[4] < frames[top] && rise <= 3.0)
    });
    for (i, (p, m, k)) in cases.iter().enumerate() {
        let Some(k) = k else { continue };
        let shipped = (i / (knobs.len() + 1)) * (knobs.len() + 1);
        let (finite, peak, dies) = results[i];
        assert!(finite, "{p} {m:?} {k:+} dB: not finite");
        // loudness follows the bass, as with an equalizer: the peak may grow by it, not more
        // (6 dB to spare)
        let bound = results[shipped].1 * 10f64.powf((k.max(0.0) + 6.0) / 20.0);
        assert!(peak < bound, "{p} {m:?} {k:+} dB: peak {peak:.3} against {:.3} shipped", results[shipped].1);
        assert!(dies, "{p} {m:?} {k:+} dB: the chord grows after its loudest 0.2 s");
    }
}
