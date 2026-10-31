//! Free microphones: a custom copy of a preset renders exactly like the preset; switching
//! to a custom pair mid-note has no gap. Needs the exported instruments (skipped without).

use dussek::mic::{Body, MicPosition};

mod common;
use dussek::{Event, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const SR: f64 = 48000.0;

fn render(t: Tuning) -> Option<Vec<f32>> {
    render_in(DIR, t)
}

fn render_in(dir: &str, t: Tuning) -> Option<Vec<f32>> {
    let mut e = common::engine(dir, SR, t)?;
    e.queue(100, Event::NoteOn { key: 48, speed: 2.5 });
    e.queue(2000, Event::NoteOn { key: 64, speed: 3.0 });
    let (mut l, mut r) = (vec![0.0; 24000], vec![0.0; 24000]);
    e.process(&mut l, &mut r);
    l.extend(r);
    Some(l)
}

#[test]
fn a_custom_copy_of_a_preset_is_the_preset() {
    for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
        let preset = Tuning { mic: pos, ..Tuning::default() };
        let setup = Body::Grand.preset_setup(pos, preset.mic_spacing, preset.mic_distance, 1.0).unwrap();
        let custom = Tuning { mic: MicPosition::Custom, mic_setup: setup, ..Tuning::default() };
        let (Some(a), Some(b)) = (render(preset), render(custom)) else { return };
        assert_eq!(a, b, "{pos:?}");
        assert!(a.iter().any(|v| v.abs() > 1e-4), "{pos:?} is silent");
    }
}

#[test]
fn a_cardioid_pair_pointing_away_is_quiet_and_finite() {
    use dussek::mic::{Mic, MicSetup};
    let pair = |extra_yaw: f64| {
        let m = |x: f64| {
            let b = Mic::toward_board([x, -1.25, 0.45]);
            Mic { pattern: 0.5, yaw: b.yaw + extra_yaw, ..b }
        };
        Tuning { mic: MicPosition::Custom, mic_setup: MicSetup { mics: [m(-0.25), m(0.25)], ..MicSetup::default() }, ..Tuning::default() }
    };
    let (Some(toward), Some(away)) = (render(pair(0.0)), render(pair(180.0))) else { return };
    let rms = |v: &[f32]| (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    assert!(away.iter().all(|v| v.is_finite()));
    // the diffuse room does not depend on the aim (it is the same everywhere), only the direct sound does:
    // how much quieter is the direct sound's share there, which the radiation's draw sets (0.65 before
    // step 55's, 0.70 with it; 0.81 since the board's low modes ring, step 184)
    assert!(rms(&away) < rms(&toward) * 0.85, "away {} toward {}", rms(&away), rms(&toward));
}


const UPRIGHT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/upright_a");

/// [sum of squares, position-weighted sum, peak] of a render
fn summary(v: &[f32]) -> [f64; 3] {
    let (mut ss, mut ws, mut pk) = (0.0f64, 0.0f64, 0.0f64);
    for (i, &x) in v.iter().enumerate() {
        let x = x as f64;
        ss += x * x;
        ws += x * ((i % 97) as f64 + 1.0);
        pk = pk.max(x.abs());
    }
    [ss, ws, pk]
}

/// Pinned from the code at the end of the free-microphone branch, re-pinned when the body response
/// came to follow H (step 50: 0.4–1.4 dB less energy, the bass's excess gone) and every instrument
/// was refitted, when the pedal's strings left their dampers from rest (step 51: 4e-5), and when the
/// radiation got a draw of its own, picked per instrument (step 55: −0.8 … +2.4 dB in energy, every
/// instrument refitted), and when the sympathetic strings' drive came to fade (step 75: ≤ 4e-4), and when Grand A was refitted with its level steps (step 104:
/// −2.2 dB in energy, its mid felt softer), and when it lost two takes and its coupling fit found
/// another minimum (step 106: −0.4 … +1.0 dB in energy by the microphone), and when the pianos' knock became
/// the hammer's thump through the board (step 126), and when the pianos' horizontal level came to vary
/// along the keyboard (step 127: −0.16 … +0.07 dB in energy), and when Grand A's thump was trimmed 12
/// dB by ear (step 128: −0.04 … −0.12 dB, its peaks 3–4 % lower), and when Grand A got its own phantom
/// partials at 0.05 and its level was calibrated with them (−0.12 … −0.18 dB in energy, its peaks 4–8 %
/// lower), and when its strings' high-frequency loss was fitted (+0.00 … +0.03 dB in energy), and when its
/// correction was fitted in bands above the tracked partials (−0.02 … −0.05 dB), and when an instrument's
/// own phantom partials came to stop below C3 (+0.14 … +0.17 dB, its level table again as without
/// them), and when Grand A's felt got its core by ear (step 134: +0.06 … +0.10 dB in energy, its peaks
/// 1–3 % higher), and when Grand A was voiced even (step 139: its C3 +3.3 dB, E4 +1.6 dB, the render
/// +2.2 dB in energy, the upright's rows unchanged), and when its felt was set (step 141: exponent 1.87,
/// +4.4 dB, the core at a quarter; +0.3 … +0.4 dB in energy), and when it was solved with pp too (step
/// 142: 2.09, +3.4 dB; +0.1 … +0.2 dB), and when the strings' loss reached the top keys (step 150: the
/// upright −0.01 dB in energy, the grand bit for bit), and when the uprights took Grand E's dampers and
/// Upright A VCSL's dry room (step 151: −0.5 … +0.5 dB by position), and when its felt was set by ear
/// (step 153: p 1.92, +9.2 dB; +0.3 … +0.6 dB), and when it rang 150–200 % by ear (step 154: +1.2 …
/// +1.4 dB), and when it was voiced even and its treble trimmed (step 168: −0.14 … +0.16 dB), and
/// when the instruments were set to one loudness (step 173: Upright A −2.0 dB), and when the unisons
/// widened toward the top key (step 183: keys 48 and 64 by the even levels' refit, Grand A −0.05 dB in
/// energy), when the boards' low modes rang (step 184: −0.7 … +0.8 dB in energy by position), and when a
/// pair's channels came to one level and the lid's reflection to its size (step 187: −1.0 … +1.6 dB by
/// position, the two keys' draws), and when Grand A's stale release peaks were rendered again and its
/// even levels measured with today's engine (step 200: −0.38 dB in energy, its peaks 4 % lower, the
/// upright's rows unchanged): the presets render as they do now.
/// Rows: grand then upright, each Player, Audience, Hammers, Tail.
const RENDER_GOLDEN: &[[f64; 3]] = &[
    [5.418168305755172e1, 4.379766853229107e2, 1.1241727322340012e-1], // grand_a Player
    [5.976297900424162e1, -4.828918986111198e2, 1.2599042057991028e-1], // grand_a Audience
    [3.24076837683137e1, -3.6976450917856766e2, 1.2230031192302704e-1], // grand_a Hammers
    [3.369076465696632e1, -1.6737297949033018e2, 1.2926794588565826e-1], // grand_a Tail
    [5.248380797308506e1, 1.6666879291878058e2, 1.2790453433990479e-1], // upright_a Player
    [4.3348105931304296e1, 3.529928588200378e2, 1.0310045629739761e-1], // upright_a Audience
    [3.7523428960212485e1, 3.968115175791229e2, 1.0811802744865417e-1], // upright_a Hammers
    [4.397218478413388e1, 1.3358446563470966e2, 1.0362366586923599e-1], // upright_a Tail
];

#[test]
fn presets_render_golden() {
    let mut got = Vec::new();
    for dir in [DIR, UPRIGHT_DIR] {
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
            let Some(v) = render_in(dir, Tuning { mic: pos, ..Tuning::default() }) else { return };
            got.push((dir, pos, summary(&v)));
        }
    }
    if RENDER_GOLDEN.is_empty() {
        for (d, p, g) in &got {
            println!("    [{:e}, {:e}, {:e}], // {} {p:?}", g[0], g[1], g[2], d.rsplit('/').next().unwrap());
        }
        panic!("RENDER_GOLDEN is empty: paste the printed rows into it");
    }
    assert_eq!(RENDER_GOLDEN.len(), got.len());
    for ((d, p, g), want) in got.iter().zip(RENDER_GOLDEN) {
        for k in 0..3 {
            assert!((g[k] - want[k]).abs() <= 1e-6 * want[k].abs().max(1e-12), "{d} {p:?} [{k}]: {} vs {}", g[k], want[k]);
        }
    }
}

#[test]
fn lid_changes_the_close_pair_geometry_only_through_the_angle() {
    // a custom copy of a preset at lid 0.5 renders exactly like the preset at lid 0.5
    for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
        let preset = Tuning { mic: pos, lid: 0.5, ..Tuning::default() };
        let setup = Body::Grand.preset_setup(pos, preset.mic_spacing, preset.mic_distance, 0.5).unwrap();
        let custom = Tuning { mic: MicPosition::Custom, mic_setup: setup, lid: 0.5, ..Tuning::default() };
        let (Some(a), Some(b)) = (render(preset), render(custom)) else { return };
        assert_eq!(a, b, "{pos:?}");
    }
}

#[test]
fn closing_the_lid_is_audible_and_finite_in_a_render() {
    let at = |lid: f64| Tuning { mic: MicPosition::Audience, lid, ..Tuning::default() };
    let (Some(open), Some(shut)) = (render(at(1.0)), render(at(0.0))) else { return };
    assert!(shut.iter().all(|v| v.is_finite()));
    let rms = |v: &[f32]| (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    assert!(rms(&shut) < rms(&open), "shut {} open {}", rms(&shut), rms(&open));
    // darker: the share of high-frequency energy (first difference) falls
    let hf = |v: &[f32]| v.windows(2).map(|w| ((w[1] - w[0]) as f64).powi(2)).sum::<f64>() / v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>();
    assert!(hf(&shut) < hf(&open), "hf shut {} open {}", hf(&shut), hf(&open));
}

#[test]
fn the_pillow_works_at_every_position() {
    let rms = |v: &[f32]| (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    for pos in [MicPosition::Recording, MicPosition::Player, MicPosition::Hammers] {
        let at = |pillow: f64| Tuning { mic: pos, pillow, ..Tuning::default() };
        let (Some(plain), Some(soft)) = (render(at(0.0)), render(at(1.0))) else { return };
        assert!(soft.iter().all(|v| v.is_finite()));
        let hf = |v: &[f32]| v.windows(2).map(|w| ((w[1] - w[0]) as f64).powi(2)).sum::<f64>() / v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().max(1e-30);
        eprintln!("{pos:?}: rms ratio {:.3} ({:.1} dB), hf share ratio {:.3}", rms(&soft) / rms(&plain), 20.0 * (rms(&soft) / rms(&plain)).log10(), hf(&soft) / hf(&plain));
        assert!(rms(&soft) < rms(&plain), "{pos:?}: soft {} plain {}", rms(&soft), rms(&plain));
        // the room is damped too: the total drops clearly, and darkens
        assert!(rms(&soft) < 0.35 * rms(&plain), "{pos:?}: soft {} plain {}", rms(&soft), rms(&plain));
        assert!(hf(&soft) < hf(&plain), "{pos:?}: hf share");
    }
}
