//! From the fits' 44.1 kHz to the engine's rate: a windowed sinc, 64 taps, its phases interpolated. A bypass at 44.1 kHz.

use super::core::SR;

const TAPS: usize = 64;
const PHASES: usize = 512;
/// the Kaiser window's beta: about 80 dB down past the transition
const BETA: f64 = 8.0;

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term) = (1.0, 1.0);
    for k in 1..40 {
        term *= (x / 2.0 / k as f64).powi(2);
        sum += term;
    }
    sum
}

/// A stream of `C` channels.
pub struct Stream<const C: usize> {
    /// in samples per out sample, as whole numbers
    step_in: u64,
    step_out: u64,
    table: Vec<f64>,
    hist: Vec<[f64; C]>,
    mask: usize,
    /// input samples written
    written: u64,
    /// the next output's place: `at` input samples and `rem / step_out`
    at: u64,
    rem: u64,
    bypass: bool,
}

impl<const C: usize> Stream<C> {
    pub fn new(sr: f64) -> Stream<C> {
        let (step_in, step_out) = (SR as u64, sr as u64);
        let bypass = step_in == step_out;
        // the cutoff below the lower Nyquist, leaving the window's transition room
        let fc = (20600.0f64).min(0.467 * sr) / SR;
        let mut table = vec![0.0; (PHASES + 1) * TAPS];
        if !bypass {
            let i0 = bessel_i0(BETA);
            for p in 0..=PHASES {
                let frac = p as f64 / PHASES as f64;
                let row = &mut table[p * TAPS..(p + 1) * TAPS];
                let mut sum = 0.0;
                for (j, c) in row.iter_mut().enumerate() {
                    // the tap j lies at (j - TAPS / 2 + 1) - frac from the output's place
                    let x = j as f64 + 1.0 - TAPS as f64 / 2.0 - frac;
                    let r = x / (TAPS as f64 / 2.0);
                    let w = if r.abs() < 1.0 { bessel_i0(BETA * (1.0 - r * r).sqrt()) / i0 } else { 0.0 };
                    let a = std::f64::consts::PI * 2.0 * fc * x;
                    *c = w * if x.abs() < 1e-12 { 2.0 * fc } else { a.sin() / (std::f64::consts::PI * x) };
                    sum += *c;
                }
                row.iter_mut().for_each(|c| *c /= sum);
            }
        }
        let n = 4 * TAPS;
        Stream { step_in, step_out, table, hist: vec![[0.0; C]; n], mask: n - 1, written: 0, at: 0, rem: 0, bypass }
    }

    /// The input's delay at the output, in input samples.
    pub fn latency(&self) -> usize {
        if self.bypass { 0 } else { TAPS / 2 }
    }

    /// The same at the output's rate, in its samples (rounded).
    pub fn latency_out(&self) -> usize {
        (self.latency() as f64 * self.step_out as f64 / self.step_in as f64).round() as usize
    }

    /// One input frame in; each output frame it makes through `out`.
    #[inline]
    pub fn push(&mut self, x: [f64; C], mut out: impl FnMut([f64; C])) {
        if self.bypass {
            out(x);
            return;
        }
        self.hist[self.written as usize & self.mask] = x;
        self.written += 1;
        // the output at `at` needs the input through at + TAPS / 2
        while self.at + (TAPS as u64) / 2 < self.written {
            let frac = self.rem as f64 / self.step_out as f64 * PHASES as f64;
            let (i, w) = (frac as usize, frac.fract());
            let (a, b) = (&self.table[i * TAPS..(i + 1) * TAPS], &self.table[(i + 1) * TAPS..(i + 2) * TAPS]);
            let first = self.at as i64 + 1 - (TAPS as i64) / 2;
            let mut y = [0.0; C];
            for j in 0..TAPS {
                let at = first + j as i64;
                if at < 0 {
                    continue;
                }
                let c = a[j] + (b[j] - a[j]) * w;
                let h = &self.hist[at as usize & self.mask];
                for k in 0..C {
                    y[k] += c * h[k];
                }
            }
            out(y);
            self.rem += self.step_in;
            self.at += self.rem / self.step_out;
            self.rem %= self.step_out;
        }
    }

    /// From nothing.
    pub fn clear(&mut self) {
        self.hist.iter_mut().for_each(|h| *h = [0.0; C]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone's level through the stream, dB, its transient left out.
    fn gain_db(f: f64, sr: f64) -> f64 {
        let mut s = Stream::<1>::new(sr);
        let n = 8000;
        let mut out = Vec::with_capacity(2 * n);
        for i in 0..n {
            s.push([(2.0 * std::f64::consts::PI * f * i as f64 / SR).sin()], |y| out.push(y[0]));
        }
        let tail = &out[out.len() / 4..];
        let k = (tail.len() as f64 * f / sr).floor() * sr / f;
        let tail = &tail[..(k.round() as usize).min(tail.len())];
        10.0 * (2.0 * tail.iter().map(|x| x * x).sum::<f64>() / tail.len() as f64).log10()
    }

    #[test]
    fn the_ripple_is_small_to_18_khz() {
        for sr in [48000.0, 96000.0, 88200.0] {
            for k in 0..40 {
                let f = 100.0 * (180.0f64).powf(k as f64 / 39.0);
                let g = gain_db(f, sr);
                assert!(g.abs() < 0.05, "{g} dB at {f} Hz, {sr} Hz");
            }
        }
    }

    #[test]
    fn what_cannot_be_kept_is_filtered() {
        // down to 32 kHz: nothing folds back from above its Nyquist
        assert!(gain_db(18000.0, 32000.0) < -60.0, "{}", gain_db(18000.0, 32000.0));
    }

    #[test]
    fn at_the_fits_rate_it_is_a_wire() {
        let mut s = Stream::<2>::new(SR);
        let mut out = Vec::new();
        for i in 0..100 {
            s.push([i as f64, -(i as f64)], |y| out.push(y));
        }
        assert_eq!(out.len(), 100);
        assert_eq!(out[37], [37.0, -37.0]);
    }

    #[test]
    fn the_output_keeps_its_rate() {
        for sr in [32000u64, 48000, 96000, 192000, 22050] {
            let mut s = Stream::<1>::new(sr as f64);
            let mut n = 0u64;
            for _ in 0..441000 {
                s.push([0.0], |_| n += 1);
            }
            let want = 10 * sr;
            assert!(n.abs_diff(want) < 2 * sr / 1000 + 64, "{n} samples at {sr} Hz");
        }
    }
}
