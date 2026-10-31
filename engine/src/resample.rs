//! Resampling of the exported buffers (impulse responses, noise bursts) to the host
//! rate, at load time: band-limited interpolation by zero-padding or truncating the
//! spectrum (rates in whole Hz).

use realfft::RealFftPlanner;
use realfft::num_complex::Complex64;

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Resample x from rate `from` to `to` (Hz). Samples keep their values (interpolation);
/// a response sampled at `from` must be scaled by from/to by the caller.
pub fn resample(x: &[f32], from: f64, to: f64) -> Vec<f32> {
    let (f, t) = (from.round() as u64, to.round() as u64);
    if f == t {
        return x.to_vec();
    }
    let g = gcd(f, t);
    let (up, down) = ((t / g) as usize, (f / g) as usize);
    // zero padding against wrap-around of the circular interpolation
    let n = (x.len() + 1024).div_ceil(down) * down;
    let m = n / down * up;
    let mut planner = RealFftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(m);
    let mut buf: Vec<f64> = x.iter().map(|&v| v as f64).chain(std::iter::repeat_n(0.0, n - x.len())).collect();
    let mut spec = fwd.make_output_vec();
    fwd.process(&mut buf, &mut spec).unwrap();
    let mut out_spec = inv.make_input_vec();
    let keep = spec.len().min(out_spec.len());
    out_spec[..keep].copy_from_slice(&spec[..keep]);
    // a bin at the new Nyquist frequency must be real
    if let Some(last) = out_spec.last_mut()
        && m % 2 == 0
    {
        *last = Complex64::new(last.re, 0.0);
    }
    out_spec[0].im = 0.0;
    let mut out = inv.make_output_vec();
    inv.process(&mut out_spec, &mut out).unwrap();
    let len = (x.len() as f64 * to / from).ceil() as usize;
    out.iter().take(len).map(|&v| (v / n as f64) as f32).collect()
}
