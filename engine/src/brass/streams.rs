//! What a player reads as it plays: the breath's noise and the slow wander of its air and lips (`Live.__init__`). One table
//! per instrument, each player reading from its own place, so six players cost one table's memory. The recipes of
//! `live.py`, by their filters' magnitudes on one long circular noise.

use super::core::{OS, SR};
use super::tables::Tables;
use crate::noise::{Rng, interp};
use realfft::RealFftPlanner;
use realfft::num_complex::Complex64;
use std::sync::Arc;

pub const NOISE_LEN: usize = 1 << 21;
pub const CTRL: usize = 64;
const NOISE_HZ: [f64; 5] = [750.0, 1414.0, 2828.0, 5657.0, 11314.0];

/// The tables.
#[derive(Clone)]
pub struct Streams {
    pub noise: Arc<[f32]>,
    pub wp: Arc<[f64]>,
    pub wl: Arc<[f64]>,
    pub band: Arc<[f64]>,
    pub jit: Arc<[f64]>,
}

/// A second-order Butterworth low-pass's magnitude, by the bilinear transform's frequencies (`scipy.signal.butter`).
fn lowpass(f: f64, fc: f64, fs: f64) -> f64 {
    let (w, wc) = ((std::f64::consts::PI * f / fs).tan(), (std::f64::consts::PI * fc / fs).tan());
    (1.0 / (1.0 + (w / wc).powi(4))).sqrt()
}

/// A band-pass from a second-order Butterworth's magnitude.
fn bandpass(f: f64, lo: f64, hi: f64, fs: f64) -> f64 {
    if f <= 0.0 {
        return 0.0;
    }
    let pi = std::f64::consts::PI;
    let (w, w1, w2) = ((pi * f / fs).tan(), (pi * lo / fs).tan(), (pi * hi / fs).tan());
    let x = (w * w - w1 * w2) / ((w2 - w1) * w);
    (1.0 / (1.0 + x.powi(4))).sqrt()
}

/// Gaussian noise of `n` samples through a magnitude over frequency, at a standard deviation of `std`.
fn shaped(rng: &mut Rng, n: usize, fs: f64, gain: impl Fn(f64) -> f64, std: f64) -> Vec<f64> {
    let mut x = rng.gauss_vec(n);
    let mut planner = RealFftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(n);
    let mut spec: Vec<Complex64> = fwd.make_output_vec();
    fwd.process(&mut x, &mut spec).expect("the plan's sizes");
    let df = fs / n as f64;
    for (i, c) in spec.iter_mut().enumerate() {
        *c *= gain(i as f64 * df);
    }
    spec[0] = Complex64::new(0.0, 0.0);
    if n % 2 == 0 {
        let last = spec.len() - 1;
        spec[last] = Complex64::new(spec[last].re, 0.0);
    }
    inv.process(&mut spec, &mut x).expect("the plan's sizes");
    let m = x.iter().sum::<f64>() / n as f64;
    let s = (x.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / n as f64).sqrt();
    let k = std / s.max(1e-30);
    x.iter_mut().for_each(|v| *v *= k);
    x
}

impl Streams {
    /// The instrument's table, from its seed.
    pub fn generate(t: &Tables, seed: u64) -> Streams {
        let mut rng = Rng::new(seed);
        let fs = SR * OS as f64;
        let bands = t.noise.clone();
        let log_hz: Vec<f64> = NOISE_HZ.iter().map(|f| f.ln()).collect();
        let voice = |f: f64| {
            let g = bandpass(f, 500.0, 6000.0, fs);
            // the breath's balance per band, heard against the recordings
            let b = bands.as_ref().map_or(1.0, |b| 10f64.powf(interp(f.max(1.0).ln(), &log_hz, b) / 20.0));
            (g, b)
        };
        // the recipe: 0.015 before the bands, what the bands leave of it after
        let (mut before, mut after) = (0.0, 0.0);
        for i in 1..NOISE_LEN / 2 {
            let (g, b) = voice(i as f64 * fs / NOISE_LEN as f64);
            before += g * g;
            after += (g * b) * (g * b);
        }
        let noise = shaped(&mut rng, NOISE_LEN, fs, |f| { let (g, b) = voice(f); g * b }, 0.015 * (after / before).sqrt());
        let noise: Arc<[f32]> = noise.iter().map(|x| *x as f32).collect();
        let fc = SR / CTRL as f64;
        let k = (60.0 * fc) as usize;
        // run through the filter both ways: the magnitude squared
        let wp = shaped(&mut rng, k, fc, |f| lowpass(f, 4.0, fc).powi(2), 0.05);
        let wl = shaped(&mut rng, k, fc, |f| lowpass(f, 4.0, fc).powi(2), 0.003);
        let band = shaped(&mut rng, k, fc, |f| bandpass(f, 55.0, 130.0, fc), 1.0);
        let j = rng.gauss_vec(k / 40 + 2);
        let jit: Vec<f64> = (0..k)
            .map(|i| {
                let (a, w) = (i / 40, (i % 40) as f64 / 40.0);
                j[a] + (j[a + 1] - j[a]) * w
            })
            .collect();
        Streams { noise, wp: wp.into(), wl: wl.into(), band: band.into(), jit: jit.into() }
    }
}
