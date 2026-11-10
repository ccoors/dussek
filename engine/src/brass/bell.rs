//! What leaves the bell, heard from a place: its beam by angle (a piston's envelope without its nulls, which a flared bell
//! does not have, going on falling behind it; no recording to fit), the distance's level, and the room fed with the bell's
//! power, darker than its axis. The fitted sound is the axis at 1.5 m, where Iowa's microphone stood.

use crate::mic::{Mic, MicSetup};
use crate::noise::min_phase_fir;

const C: f64 = 343.0;
/// m, on the axis: where the fit's sound is
const REF: f64 = 1.5;
/// the room's level against the direct sound at REF, on the axis, dB
pub const ROOM_AT_REF_DB: f64 = -14.0;
/// the filters' taps at 44.1 kHz
const TAPS: usize = 512;
const NFFT: usize = 4096;
const SR: f64 = 44100.0;
/// m: the nearest level a microphone gets, Close's
const NEAREST: f64 = 0.3;

/// The beam against the axis (dB) at f Hz, `theta` radians off it, on a bell of radius `a` m.
pub fn beam_db(f: f64, theta: f64, a: f64) -> f64 {
    let x = 2.0 * (2.0 * std::f64::consts::PI * f.max(1.0) / C * a) * (theta / 2.0).sin();
    -10.0 * (1.0 + (x / 2.0).abs().powi(3)).log10()
}

/// The Bessel function J1 (Numerical Recipes' rational approximations, about 1e-8).
fn j1(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let a = x * (72362614232.0 + y * (-7895059235.0 + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * (-30.16036606))))));
        let b = 144725228442.0 + y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y))));
        a / b
    } else {
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - 2.356194491;
        let a = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * (-0.240337019e-6))));
        let b = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
        let r = (0.636619772 / ax).sqrt() * (xx.cos() * a - z * xx.sin() * b);
        if x < 0.0 { -r } else { r }
    }
}

/// On-axis gain over the power average of a piston of radius a (`mutes.piston_gain_db`), dB.
pub fn piston_gain_db(f: f64, a: f64) -> f64 {
    let ka = (2.0 * std::f64::consts::PI * f / C * a).max(1e-6);
    10.0 * (ka * ka / (1.0 - j1(2.0 * ka) / ka)).log10()
}

/// The bell's power against its axis, dB, 0 at the lowest frequencies: what feeds the room.
pub fn power_db(f: f64, a: f64) -> f64 {
    piston_gain_db(1.0, a) - piston_gain_db(f.max(1.0), a)
}

fn fir_of(db: impl Fn(f64) -> f64, level: f64, delay: usize) -> Vec<f32> {
    let grid: Vec<f64> = (0..=NFFT / 2).map(|i| db(i as f64 * SR / NFFT as f64)).collect();
    let h = min_phase_fir(&grid, NFFT);
    let mut out = vec![0.0f32; delay];
    for (i, x) in h[..TAPS].iter().enumerate() {
        // the tail's last quarter faded: an edge there is heard
        let k = (TAPS - i) as f64 / (TAPS / 4) as f64;
        let w = if k < 1.0 { (std::f64::consts::FRAC_PI_2 * k).sin().powi(2) } else { 1.0 };
        out.push((x * w * level) as f32);
    }
    out
}

/// The room's feed: its filter (unit level).
pub fn room_fir(a: f64) -> Vec<f32> {
    fir_of(|f| power_db(f, a), 1.0, 0)
}

/// A brass spectrum's rough weight (the level's reference): down 6 dB per octave from 500 Hz.
fn weight(f: f64) -> f64 {
    1.0 / (1.0 + (f / 500.0).powi(2))
}

/// One microphone's distance and angle off the bell's axis (a unit vector), and its pattern's gain toward the bell.
fn place(m: &Mic, axis: [f64; 3]) -> (f64, f64, f64) {
    let p = m.pos;
    // no near field: nearer than this it stays what this position has (a level of 1.5 / r ran into the output's guard)
    let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(NEAREST);
    let theta = ((p[0] * axis[0] + p[1] * axis[1] + p[2] * axis[2]) / r).clamp(-1.0, 1.0).acos();
    let arrive = [-p[0] / r, -p[1] / r, -p[2] / r];
    (r, theta, m.gain(arrive))
}

/// The pair's filters: each its beam, its distance's level, its pattern's gain, and the pair's difference of delay; the delay
/// they share is left out, or a player at Audience would hear every note 18 ms late. `room` the level on the room's path.
/// `keep_direct`: the level goes with the distance; else the total (direct + room) stays as at the recording's position.
/// `axis`: where the bell points (`BellPose::axis`).
pub fn pair_firs(setup: &MicSetup, a: f64, axis: [f64; 3], keep_direct: bool, room_share_db: f64) -> ([Vec<f32>; 2], f64) {
    let (ra, ta, ga) = place(&setup.mics[0], axis);
    let (rb, tb, gb) = place(&setup.mics[1], axis);
    let first = ra.min(rb);
    let delay = |r: f64| ((r - first) / C * SR).round() as usize;
    let room_power = 10f64.powf(room_share_db / 10.0);
    let mut k = 1.0;
    if !keep_direct {
        // over a brass spectrum: the pair's mean power with the room's against the axis at REF with the room's
        let (mut w, mut e) = (0.0, 0.0);
        for i in 1..(NFFT / 2) {
            let f = i as f64 * SR / NFFT as f64;
            let s = weight(f);
            let beam = |t: f64, r: f64, g: f64| (REF / r * g).powi(2) * 10f64.powf(beam_db(f, t, a) / 10.0);
            w += s;
            e += s * 0.5 * (beam(ta, ra, ga) + beam(tb, rb, gb));
        }
        k = ((1.0 + room_power) / (e / w + room_power)).sqrt();
    }
    (
        [
            fir_of(|f| beam_db(f, ta, a), k * REF / ra * ga, delay(ra)),
            fir_of(|f| beam_db(f, tb, a), k * REF / rb * gb, delay(rb)),
        ],
        k,
    )
}
