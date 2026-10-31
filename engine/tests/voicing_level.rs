//! The voicing's wear scatters each key's level: a seeded offset per key (dB), its size
//! VOICING_LEVEL_DB · √wear, none at wear 0 (the export as it is, mint). Needs the exported
//! instruments (skipped without them).

mod common;

use dussek::{Engine, Event, Params, Tuning, Wear};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");

fn engine(seed: u64, voicing_wear: f32) -> Option<Engine> {
    let t = Tuning { wear: Wear { seed, ..Wear::default() }, ..Tuning::default() };
    let mut e = common::engine(DIR, 48000.0, t)?;
    e.set_params(Params { voicing_wear, ..Params::default() });
    Some(e)
}

fn levels(e: &Engine) -> Vec<f64> {
    (21..=108).map(|k| e.voicing_level_db(k)).collect()
}

fn sd(x: &[f64]) -> f64 {
    let m = x.iter().sum::<f64>() / x.len() as f64;
    (x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

/// key struck at mf, both channels over 0.2 s
fn render(mut e: Engine, key: u8) -> Vec<f32> {
    e.queue(0, Event::NoteOn { key, speed: 3.0 });
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    let mut out = Vec::new();
    for _ in 0..(0.2 * 48000.0 / 256.0) as usize {
        e.process(&mut l, &mut r);
        out.extend(l.iter().chain(&r));
    }
    out
}

fn peak_db(x: &[f32]) -> f64 {
    20.0 * (x.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64).log10()
}

#[test]
fn no_scatter_without_voicing_wear() {
    let (Some(a), Some(b)) = (engine(1, 0.0), engine(2, 0.0)) else { return };
    assert!(levels(&a).iter().all(|&x| x == 0.0));
    assert_eq!(render(a, 60), render(b, 60), "wear 0: the seed changes nothing");
}

#[test]
fn the_scatter_is_seeded() {
    let (Some(a), Some(b), Some(c)) = (engine(1, 0.05), engine(1, 0.05), engine(2, 0.05)) else { return };
    assert_eq!(levels(&a), levels(&b));
    assert_ne!(levels(&a), levels(&c));
    let s = sd(&levels(&a));
    assert!((0.6..=1.4).contains(&s), "σ at Normal's wear: {s:.2} dB");
}

#[test]
fn the_scatter_grows_with_the_wear() {
    let (Some(a), Some(b)) = (engine(1, 0.2), engine(1, 1.0)) else { return };
    let (sa, sb) = (sd(&levels(&a)), sd(&levels(&b)));
    assert!((1.5..=2.6).contains(&sa), "σ at 0.2: {sa:.2} dB");
    assert!((3.4..=5.8).contains(&sb), "σ at 1: {sb:.2} dB");
}

#[test]
fn the_scatter_reaches_the_sound() {
    let Some(a) = engine(1, 0.05) else { return };
    let l = levels(&a);
    let i = (0..88).max_by(|&x, &y| l[x].abs().total_cmp(&l[y].abs())).unwrap();
    let key = 21 + i as u8;
    let Some(mint) = engine(1, 0.0) else { return };
    let heard = peak_db(&render(a, key)) - peak_db(&render(mint, key));
    assert!(heard * l[i] > 0.0 && (heard - l[i]).abs() <= 0.5 * l[i].abs(),
        "key {key}: drawn {:.2} dB, heard {heard:.2} dB", l[i]);
}

/// the kind's share (`LiveWear::of`): a vibraphone's bars and a harp's strings scatter by half a
/// piano's, the same draws
#[test]
fn bars_and_strings_by_hand_scatter_half() {
    for other in ["vibraphone", "harp"] {
        let dir = format!("{}/../instruments/{other}", env!("CARGO_MANIFEST_DIR"));
        let t = Tuning { wear: Wear { seed: 1, ..Wear::default() }, ..Tuning::default() };
        let (Some(piano), Some(mut e)) = (engine(1, 0.2), common::engine(&dir, 48000.0, t)) else { continue };
        e.set_params(Params { voicing_wear: 0.2, ..Params::default() });
        for (a, b) in levels(&piano).iter().zip(levels(&e)) {
            assert!((b - 0.5 * a).abs() < 1e-12, "{other}: {b} vs half of {a}");
        }
    }
}
