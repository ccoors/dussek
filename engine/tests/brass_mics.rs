//! The microphones on a bell: `PE_BRASS_EXPORT` a scratch export (or a committed one), else these return early.
use dussek::brass::bell;
use dussek::mic::{BellPose, Body, MicPosition};
use dussek::{Engine, Event, Instrument, Mix, Tuning};
use std::path::PathBuf;

const SR: f64 = 44100.0;

fn export() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PE_BRASS_EXPORT") {
        return Some(PathBuf::from(p));
    }
    let p = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/tenor_trombone"));
    p.join("instrument.json").exists().then_some(p)
}

/// The export at the fit's own level: these hold the engine to the players' own sound (the export's gain: `output_gain.rs`).
fn load() -> Option<Instrument> {
    let mut inst = Instrument::load(export()?).ok()?;
    inst.output_gain_db = 0.0;
    Some(inst)
}

fn phrase(t: Tuning, mix: Mix, room: f32) -> Option<(Vec<f32>, Vec<f32>)> {
    let inst = load()?;
    let key = inst.brass.as_ref().unwrap().near(60);
    let mut e = Engine::with_tuning(inst, SR, t).unwrap();
    e.set_mix(mix);
    let mut p = e.params();
    p.room = room;
    e.set_params(p);
    let speed = e.hammer_speed(key, 80.0);
    e.queue(0, Event::NoteOn { key, speed });
    e.queue((1.5 * SR) as u64, Event::KeyUp { key });
    let n = (1.8 * SR) as usize;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    e.process(&mut l, &mut r);
    Some((l, r))
}

const DRY: Mix = Mix { room: false, ..Mix::ALL };

/// dB in a band over the note's steady part, of both channels.
fn band(x: &(Vec<f32>, Vec<f32>), lo: f64, hi: f64) -> f64 {
    let (a, b) = ((0.3 * SR) as usize, (1.4 * SR) as usize);
    let n = (b - a).next_power_of_two() / 2;
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut p = 0.0;
    for c in [&x.0, &x.1] {
        let mut buf: Vec<f64> = c[a..a + n].iter().enumerate().map(|(i, v)| f64::from(*v) * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())).collect();
        let mut sp = fft.make_output_vec();
        fft.process(&mut buf, &mut sp).unwrap();
        for (i, s) in sp.iter().enumerate() {
            let f = i as f64 * SR / n as f64;
            if f >= lo && f < hi {
                p += s.norm_sqr();
            }
        }
    }
    10.0 * (p + 1e-30).log10()
}

fn at(pos: MicPosition) -> Tuning {
    Tuning { mic: pos, ..Tuning::default() }
}

#[test]
fn recording_is_the_fitted_sound() {
    let Some(a) = phrase(Tuning::default(), DRY, 1.0) else { return };
    let inst = load().unwrap();
    let key = inst.brass.as_ref().unwrap().near(60);
    let tables = inst.brass.clone().unwrap();
    let mut s = dussek::brass::family::section(&tables, 0);
    s.post(dussek::brass::section::Ev::On(key, inst_velocity()));
    let mut mix = [[0.0; 64]; 2];
    let mut want = Vec::new();
    for _ in 0..200 {
        s.step(&mut mix);
        want.extend(mix[0].iter().map(|x| x.tanh() as f32));
    }
    assert_eq!(&a.0[64..64 + want.len()], &want[..], "no filter, no delay");
}

fn inst_velocity() -> f64 {
    80.0
}

#[test]
fn audience_is_darker_from_4_khz() {
    let (Some(r), Some(a)) = (phrase(Tuning::default(), DRY, 1.0), phrase(at(MicPosition::Audience), DRY, 1.0)) else { return };
    let level = |x: &(Vec<f32>, Vec<f32>)| band(x, 100.0, 2000.0);
    let up = |x: &(Vec<f32>, Vec<f32>)| band(x, 4000.0, 8000.0) - level(x);
    let dark = up(&r) - up(&a);
    assert!((1.5..6.0).contains(&dark), "{dark} dB darker from 4 kHz at Audience, 3 to 4 by mic_test");
}

#[test]
fn player_is_behind_the_bell() {
    let (Some(r), Some(p)) = (phrase(Tuning::default(), DRY, 1.0), phrase(at(MicPosition::Player), DRY, 1.0)) else { return };
    let tilt = |x: &(Vec<f32>, Vec<f32>)| band(x, 6000.0, 12000.0) - band(x, 100.0, 1000.0);
    let down = tilt(&r) - tilt(&p);
    assert!(down > 15.0, "{down} dB down at 8 kHz behind the bell");
}

#[test]
fn room_at_0_db_stands_14_db_under() {
    let dir = Tuning::default();
    let (Some(d), Some(all)) = (phrase(dir, DRY, 1.0), phrase(dir, Mix::ALL, 1.0)) else { return };
    // the room alone: what the whole has over the direct path
    let room = (
        all.0.iter().zip(&d.0).map(|(a, b)| a - b).collect::<Vec<f32>>(),
        all.1.iter().zip(&d.1).map(|(a, b)| a - b).collect::<Vec<f32>>(),
    );
    let db = band(&room, 20.0, 20000.0) - band(&d, 20.0, 20000.0);
    assert!((db + 14.0).abs() < 1.5, "the room stands {db} dB against the direct sound");
}

#[test]
fn moving_a_mic_does_not_click() {
    let Some(inst) = load() else { return };
    let key = inst.brass.as_ref().unwrap().near(60);
    let mut a = Engine::with_tuning(inst, SR, Tuning::default()).unwrap();
    a.set_mix(DRY);
    let speed = a.hammer_speed(key, 80.0);
    a.queue(0, Event::NoteOn { key, speed });
    let n = (0.5 * SR) as usize;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    a.process(&mut l, &mut r);
    let mut b = Engine::with_tuning(load().unwrap(), SR, at(MicPosition::Audience)).unwrap();
    b.set_mix(DRY);
    assert!(b.adopt(&mut a), "one instrument takes over");
    let (mut l2, mut r2) = (vec![0.0; n], vec![0.0; n]);
    b.process(&mut l2, &mut r2);
    let peak = l.iter().chain(&l2).fold(0.0f32, |m, x| m.max(x.abs()));
    let mut prev = l[n - 1];
    for (i, x) in l2.iter().take(4000).enumerate() {
        assert!((x - prev).abs() < 0.6 * peak.max(0.01), "a jump at {i}: {} of {peak}", (x - prev).abs());
        prev = *x;
    }
    assert!(l2[n / 2..].iter().any(|x| x.abs() > 1e-4), "the note goes on");
}

#[test]
fn the_geometry_is_the_positions_heard() {
    let p = Body::Bell(BellPose::default()).placement(MicPosition::Audience, 0.5, 0.0, 1.0).unwrap();
    assert_eq!(p.mics, [[-0.25, 6.0, 0.0], [0.25, 6.0, 0.0]]);
    let p = Body::Bell(BellPose::default()).placement(MicPosition::Player, 0.5, 0.0, 1.0).unwrap();
    assert!((p.spacing - 0.18).abs() < 1e-9 && p.mics[0][1] < 0.0);
    assert!(Body::Bell(BellPose::default()).placement(MicPosition::Recording, 0.5, 0.0, 1.0).is_none());
    assert!(!Body::Bell(BellPose::default()).has_lid());
    assert!(bell::beam_db(8000.0, 0.0, 0.06).abs() < 1e-9);
    assert!(bell::beam_db(8000.0, 2.8, 0.06) < -15.0);
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

#[test]
fn the_spacing_is_the_pair_s_in_meters() {
    // as on every other body: what is set, what a drag writes back; at the player's ears and at the bell no wider than
    // the head and the bell allow
    let at = |pos, spacing| Body::Bell(BellPose::default()).placement(pos, spacing, 0.0, 1.0).unwrap();
    let a = at(MicPosition::Audience, 0.3);
    assert!((a.spacing - 0.3).abs() < 1e-9 && (dist(a.mics[0], a.mics[1]) - 0.3).abs() < 1e-9);
    for (pos, widest) in [(MicPosition::Player, 0.18), (MicPosition::Hammers, 0.1), (MicPosition::Tail, 0.1)] {
        let p = at(pos, 0.5);
        assert!((p.spacing - widest).abs() < 1e-9 && (p.spacing_max - widest).abs() < 1e-9, "{pos:?}: {} of {}", p.spacing, p.spacing_max);
        assert!((at(pos, 0.06).spacing - 0.06).abs() < 1e-9, "{pos:?} narrower");
        // a drag's value (the spacing shown) puts the pair where it was
        assert!((at(pos, p.spacing).spacing - p.spacing).abs() < 1e-9);
    }
}

#[test]
fn a_mic_at_the_bell_does_not_clear_the_sound() {
    // a custom pair dragged into the bell, the level going with the distance: a loud note stays a note, under the engine's guard
    let at = |x: f64| dussek::mic::Mic::toward_board([x, 0.02, 0.0]);
    let setup = dussek::mic::MicSetup { mics: [at(-0.01), at(0.01)], ..dussek::mic::MicSetup::default() };
    let t = Tuning { mic: MicPosition::Custom, mic_setup: setup, keep_direct: true, ..Tuning::default() };
    // the trumpet: louder at the output than the tenor, where the old floor stayed under the guard
    let Ok(inst) = Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/trumpet")) else { return };
    let key = inst.brass.as_ref().unwrap().near(60);
    let mut e = Engine::with_tuning(inst, SR, t).unwrap();
    let speed = e.hammer_speed(key, 127.0);
    e.queue(0, Event::NoteOn { key, speed });
    let n = 44100;
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    e.process(&mut l, &mut r);
    assert_eq!(e.stats().runaways, 0, "the guard stayed out of it");
    assert!(l[n / 2..].iter().any(|x| x.abs() > 0.01), "the note goes on");
}

/// dB over 2 to 8 kHz of one channel's steady part.
fn side(x: &[f32]) -> f64 {
    band(&(x.to_vec(), vec![0.0; x.len()]), 2000.0, 8000.0)
}

#[test]
fn the_audience_hears_the_bell_in_the_middle() {
    // 1.5 m aside, the pair's nearer microphone was nearer the beam too: every bell sounded to the left
    let Some(a) = phrase(at(MicPosition::Audience), DRY, 1.0) else { return };
    let tilt = side(&a.0) - side(&a.1);
    assert!(tilt.abs() < 0.3, "{tilt} dB left over right from 2 to 8 kHz at Audience");
}

/// A bell turned up and to the player's right, its player's ears under it and to the left (a tuba's).
const UP: BellPose = BellPose { azimuth: 90, elevation: 75, ears: [-20, 0, -40], radius_mm: 220 };
/// A bell turned back and to the player's right, at the hip (a horn's).
const BACK: BellPose = BellPose { azimuth: 135, elevation: -10, ears: [-30, 10, 35], radius_mm: 150 };

fn center(p: &dussek::mic::Placement) -> [f64; 3] {
    [0, 1, 2].map(|i| 0.5 * (p.mics[0][i] + p.mics[1][i]))
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

#[test]
fn the_player_and_the_audience_stay_where_the_player_is() {
    for pose in [UP, BACK, BellPose::default()] {
        let ears = pose.ears.map(|c| f64::from(c) / 100.0);
        let p = Body::Bell(pose).placement(MicPosition::Player, 0.5, 0.0, 1.0).unwrap();
        assert!(near(center(&p), ears), "{:?}", center(&p));
        let a = Body::Bell(pose).placement(MicPosition::Audience, 0.5, 0.0, 1.0).unwrap();
        assert!(near(center(&a), [0.0, 6.0, ears[2]]), "{:?}", center(&a));
        assert!(near(a.spread, [1.0, 0.0, 0.0]));
    }
}

#[test]
fn the_close_positions_turn_with_the_bell() {
    for pose in [UP, BACK] {
        let u = pose.axis();
        let c = center(&Body::Bell(pose).placement(MicPosition::Hammers, 0.5, 0.0, 1.0).unwrap());
        let along = c[0] * u[0] + c[1] * u[1] + c[2] * u[2];
        assert!((along - 0.3).abs() < 1e-9, "Close 1 {along} m along the axis");
        let c2 = center(&Body::Bell(pose).placement(MicPosition::Tail, 0.5, 0.0, 1.0).unwrap());
        let along2 = c2[0] * u[0] + c2[1] * u[1] + c2[2] * u[2];
        assert!((along2 - 0.02).abs() < 1e-9, "Close 2 {along2} m along the axis");
    }
    assert!(near(UP.axis(), [75f64.to_radians().cos(), 0.0, 75f64.to_radians().sin()]));
}

fn extent(pts: &[[f64; 2]]) -> (f64, f64) {
    let pts: Vec<_> = pts.iter().filter(|p| p[0].is_finite()).collect();
    let w = pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max) - pts.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
    let h = pts.iter().map(|p| p[1]).fold(f64::MIN, f64::max) - pts.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
    (w, h)
}

#[test]
fn the_drawing_shows_where_the_bell_points() {
    // from above, a bell pointing forward is deeper than wide; one pointing up shows its mouth, about round
    let d = Body::Bell(BellPose { elevation: 0, ..BellPose::default() }).drawing(0.0, 1.0);
    let (w, h) = extent(&d.case_top);
    assert!(h > 2.0 * w, "forward from above: {w} wide, {h} deep");
    let d = Body::Bell(BellPose { azimuth: 0, ears: [0, 0, -40], ..UP }).drawing(0.0, 1.0);
    let (w, h) = extent(&d.case_top);
    assert!((0.6..1.7).contains(&(h / w)) && w > 0.4, "up from above: {w} wide, {h} deep");
    let (fw, fh) = extent(&d.outline_front);
    assert!(fh > 1.5 * fw, "up from the front: {fw} wide, {fh} high");
    assert!(d.case_top.len() <= 24 && d.outline_front.len() <= 32);
    assert!((d.floor_z - (-0.4 - 1.2)).abs() < 1e-9, "the floor under the player's ears");
}

#[test]
fn a_bell_turned_up_is_darker_in_front() {
    let Some(mut inst) = load() else { return };
    let play = |inst: &Instrument| {
        let key = inst.brass.as_ref().unwrap().near(60);
        let mut e = Engine::with_tuning(inst.clone(), SR, at(MicPosition::Audience)).unwrap();
        e.set_mix(DRY);
        e.queue(0, Event::NoteOn { key, speed: e.hammer_speed(key, 80.0) });
        let n = (1.8 * SR) as usize;
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        e.process(&mut l, &mut r);
        (l, r)
    };
    let tilt = |x: &(Vec<f32>, Vec<f32>)| band(x, 2000.0, 8000.0) - band(x, 100.0, 1000.0);
    inst.body_kind = Body::Bell(BellPose::default());
    let forward = tilt(&play(&inst));
    inst.body_kind = Body::Bell(UP);
    let up = tilt(&play(&inst));
    assert!(forward - up > 6.0, "{} dB darker from 2 kHz with the bell up", forward - up);
}

#[test]
fn a_bell_seen_along_its_side_has_an_even_tube() {
    // a tuba's bell, up and turned a little: from the front its axis lies almost in the view, its rings
    // seen nearly edge-on; the tube between the mouth and the end is as wide as the profile, not a wave
    let pose = BellPose { azimuth: 60, elevation: 75, ears: [-20, 0, -40], radius_mm: 220 };
    let d = Body::Bell(pose).drawing(0.0, 1.0);
    let u = pose.axis();
    let l = (u[0] * u[0] + u[2] * u[2]).sqrt();
    let side = [-u[2] / l, u[0] / l];
    let half = d.outline_front.len() / 2;
    let widths: Vec<f64> = d.outline_front[..half].iter().map(|p| p[0] * side[0] + p[1] * side[1]).collect();
    let tube = 0.012 * pose.radius() / 0.09;
    for (i, w) in widths.iter().enumerate().skip(1).take(half - 2) {
        assert!(*w >= 0.8 * tube, "point {i}: {w:.4} m against the tube's {tube:.4} m; all {widths:.3?}");
    }
}
