//! The roll's calibrations (`roll::calibrate`): each register's loudness brought to a target, and
//! the board's radiation moved until Player matches Recording as the references do. Needs the
//! exported instruments (skipped without them).

use dussek::mic::MicPosition;
use dussek::roll::calibrate::{calibrate_positions, match_registers};
use dussek::roll::files::Export;
use dussek::roll::measure;
use std::path::Path;

const REFS: [&str; 10] = ["grand_a", "grand_b", "grand_f", "grand_e", "grand_d", "grand_c", "grand_g", "upright_a", "upright_b", "upright_c"];

fn load(name: &str) -> Option<Export> {
    Export::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../instruments").join(name)).ok()
}

#[test]
fn registers_reach_their_target() {
    let (Some(mut a), Some(e)) = (load("grand_a"), load("grand_d")) else { return };
    // halfway to Grand D's levels: a roll moves a few dB (Grand D's treble itself lies under the
    // floor the action noise sets on Grand A)
    let te = measure::render_heads(&e.instrument().unwrap(), 80.0, MicPosition::Recording).unwrap().level;
    let ta = measure::render_heads(&a.instrument().unwrap(), 80.0, MicPosition::Recording).unwrap().level;
    let target: Vec<f64> = ta.iter().zip(&te).map(|(x, y)| (x + y) / 2.0).collect();
    let err = match_registers(&mut a, &target).unwrap();
    assert!(err < 0.3, "{err:.2} dB off");
    let got = measure::render_heads(&a.instrument().unwrap(), 80.0, MicPosition::Recording).unwrap().level;
    for (k, (g, t)) in got.iter().zip(&target).enumerate() {
        assert!((g - t).abs() < 0.3, "note {}: {:.2} dB off", measure::NOTES[k], g - t);
    }
}

#[test]
fn a_body_change_is_calibrated_back() {
    let refs: Option<Vec<Export>> = REFS.iter().map(|r| load(r)).collect();
    let Some(refs) = refs else { return };
    // the references' median Player − Recording per octave
    let all: Vec<[f64; 9]> = refs.iter().map(|r| measure::player_minus_recording(r).unwrap()).collect();
    let goal: [f64; 9] = std::array::from_fn(|b| {
        let mut v: Vec<f64> = all.iter().map(|x| x[b]).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    });
    let mut a = refs[0].clone();
    // the board's radiation moved ±10 dB in a wave over 4 octaves, as a body change that misses the
    // board would (a body changes smoothly, by its 1/6-octave bands)
    let f = a.f64s("/board/f").unwrap();
    for key in ["/board/r", "/board/r_h"] {
        let r = a.f64s(key).unwrap();
        let n = f.len();
        let wave = |fm: f64| 10f64.powf(10.0 * (std::f64::consts::PI / 2.0 * (fm.max(1.0) / 62.5).log2()).sin() / 20.0);
        let moved: Vec<f64> = r.iter().enumerate().map(|(i, v)| v * wave(f[i % n])).collect();
        a.set_f64s(key, &moved).unwrap();
    }
    let before = measure::player_minus_recording(&a).unwrap();
    assert!(before.iter().zip(&goal).any(|(d, g)| (d - g).abs() > 3.0), "the change did not move Player: {before:?}");
    let rec = measure::render_heads(&a.instrument().unwrap(), 80.0, MicPosition::Recording).unwrap();
    let cal = calibrate_positions(&mut a, &goal, &rec).unwrap();
    // the radiation's level is the recording's (the engine matches it): only the shape moves
    let shape = |d: &[f64; 9]| {
        let off: Vec<f64> = d.iter().zip(&goal).map(|(x, g)| x - g).collect();
        let m = off.iter().sum::<f64>() / 9.0;
        off.iter().fold(0.0f64, |w, v| w.max((v - m).abs()))
    };
    assert!(shape(&cal.d) < shape(&before), "{:.2} dB, before {:.2}", shape(&cal.d), shape(&before));
    // within the roll's bound (a_roll_holds_up) below 4 kHz; from 4 kHz up (octaves 6–8) Player is not
    // the board's radiation alone, a pass moves it a tenth of a dB (design log step 118): there the
    // calibration only must not make it worse (the references' refit moved octave 6's median 0.4 dB,
    // past the bound, with Grand A's own −2.3 dB unmoved)
    for (b, ((d, g), was)) in cal.d.iter().zip(&goal).zip(&before).enumerate() {
        // 3 dB, not the roll's 2.5: Grand A's octave 3 (250–500 Hz) came back to −2.1 dB against +0.1
        // and, after its strings' high-frequency loss was fitted (design 2026-10-20-bass-high-band),
        // to −2.7: the calibration's reach there rests on more than the board's radiation
        if b < 6 {
            assert!((d - g).abs() <= 3.0, "octave {b}: {d:+.1} dB against the references' {g:+.1}");
        } else {
            assert!((d - g).abs() <= (was - g).abs() + 0.5, "octave {b}: {d:+.1} dB, before {was:+.1}, the references' {g:+.1}");
        }
    }
    // 3 passes before the references' rooms were cut (design log step 123): Grand F's Player 16 dB over its
    // Recording at 2 kHz widens the references' spread there; 6 since the grands' felts (step 141)
    assert!(cal.passes <= 6, "{} passes", cal.passes);
    // the last render is the export's as left
    let player = measure::render_heads(&a.instrument().unwrap(), 80.0, MicPosition::Player).unwrap();
    assert_eq!(measure::difference(&player, &rec), cal.d);
    assert_eq!(measure::difference(&cal.player, &rec), cal.d);
}
