//! The noise parts of an instrument, synthesized at load from their exported parameters
//! (ports of `proto/release.py` `burst`, `proto/soundboard.py` `knock_burst`, `room_ir`,
//! `min_phase`). The realizations differ from the prototype's (own random generator);
//! the statistics are the same. All at the reference rate; the engine resamples.

use crate::instrument::RoomSpec;
use std::f64::consts::PI;

use realfft::RealFftPlanner;
use realfft::num_complex::Complex64;

/// Gaussian noise: xoshiro256** seeded through splitmix64, Box–Muller pairs.
#[derive(Clone)]
pub struct Rng {
    s: [u64; 4],
    spare: Option<f64>,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut z = seed;
        let mut next = || {
            z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            x ^ (x >> 31)
        };
        Rng { s: [next(), next(), next(), next()], spare: None }
    }

    fn next_u64(&mut self) -> u64 {
        let r = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        r
    }

    /// uniform in (0, 1]
    fn uniform(&mut self) -> f64 {
        ((self.next_u64() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }

    /// uniform in [0, 1)
    pub fn uniform01(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn gauss(&mut self) -> f64 {
        if let Some(x) = self.spare.take() {
            return x;
        }
        let r = (-2.0 * self.uniform().ln()).sqrt();
        let a = 2.0 * PI * self.uniform();
        self.spare = Some(r * a.sin());
        r * a.cos()
    }

    pub fn gauss_vec(&mut self, n: usize) -> Vec<f64> {
        (0..n).map(|_| self.gauss()).collect()
    }
}

/// 2nd-order Butterworth high-pass (bilinear, as scipy's butter(2, fc, "high")).
#[derive(Clone, Copy, Debug)]
pub struct HighPass {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl HighPass {
    pub fn new(fc: f64, sr: f64) -> Self {
        let w = 2.0 * PI * fc / sr;
        let (cw, alpha) = (w.cos(), w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2));
        let a0 = 1.0 + alpha;
        HighPass {
            b: [(1.0 + cw) / 2.0 / a0, -(1.0 + cw) / a0, (1.0 + cw) / 2.0 / a0],
            a: [-2.0 * cw / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }

    pub fn reset(&mut self) {
        self.z = [0.0; 2];
    }
}

/// np.sinc
fn sinc(x: f64) -> f64 {
    if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) }
}

/// Coherence of a diffuse field at an AB pair (spacing m) at frequency f.
fn coherence(f: f64, spacing: f64) -> f64 {
    sinc(2.0 * f * spacing / 343.0)
}

/// np.interp: piecewise linear through sorted xp, clamped at the ends.
pub fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    if x <= xp[0] {
        return fp[0];
    }
    let last = xp.len() - 1;
    if x >= xp[last] {
        return fp[last];
    }
    let i = xp.partition_point(|&v| v <= x) - 1;
    fp[i] + (x - xp[i]) / (xp[i + 1] - xp[i]) * (fp[i + 1] - fp[i])
}

fn rfftfreq(n: usize, sr: f64) -> Vec<f64> {
    (0..n / 2 + 1).map(|i| i as f64 * sr / n as f64).collect()
}

/// Spectral helper with numpy's conventions (irfft includes the 1/n).
struct Fft {
    planner: RealFftPlanner<f64>,
}

impl Fft {
    fn new() -> Self {
        Fft { planner: RealFftPlanner::new() }
    }

    fn rfft(&mut self, x: &[f64]) -> Vec<Complex64> {
        let f = self.planner.plan_fft_forward(x.len());
        let mut buf = x.to_vec();
        let mut out = f.make_output_vec();
        f.process(&mut buf, &mut out).unwrap();
        out
    }

    fn irfft(&mut self, spec: &[Complex64], n: usize) -> Vec<f64> {
        let f = self.planner.plan_fft_inverse(n);
        let mut buf = f.make_input_vec();
        let m = buf.len().min(spec.len());
        buf[..m].copy_from_slice(&spec[..m]);
        // the bins at DC and (even n) Nyquist are real for a real signal
        buf[0].im = 0.0;
        if n.is_multiple_of(2) {
            let last = buf.len() - 1;
            buf[last].im = 0.0;
        }
        let mut out = f.make_output_vec();
        f.process(&mut buf, &mut out).unwrap();
        out.iter_mut().for_each(|v| *v /= n as f64);
        out
    }

    /// Stereo noise of length n with an AB pair's coherence, optionally spectrally shaped.
    fn coherent_pair(&mut self, rng: &mut Rng, n: usize, sr: f64, spacing: f64, shape: impl Fn(f64) -> f64) -> [Vec<f64>; 2] {
        let a = self.rfft(&rng.gauss_vec(n));
        let b = self.rfft(&rng.gauss_vec(n));
        let f = rfftfreq(n, sr);
        let mut l = Vec::with_capacity(a.len());
        let mut r = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            let c = coherence(f[i], spacing);
            let g = shape(f[i]);
            l.push(a[i] * g);
            r.push((a[i] * c + b[i] * (1.0 - c * c).max(0.0).sqrt()) * g);
        }
        [self.irfft(&l, n), self.irfft(&r, n)]
    }

    /// Minimum-phase FIR (length n) with magnitude h_db on rfftfreq(n), via the cepstrum.
    fn min_phase(&mut self, h_db: &[f64], n: usize) -> Vec<f64> {
        let log_mag: Vec<Complex64> = h_db.iter().map(|&d| Complex64::new(d * std::f64::consts::LN_10 / 20.0, 0.0)).collect();
        let cep = self.irfft(&log_mag, n);
        let mut fold = vec![0.0; n];
        fold[0] = cep[0];
        fold[n / 2] = cep[n / 2];
        for i in 1..n / 2 {
            fold[i] = 2.0 * cep[i];
        }
        let spec: Vec<Complex64> = self.rfft(&fold).into_iter().map(Complex64::exp).collect();
        self.irfft(&spec, n)
    }

    /// Linear convolution, truncated to x.len().
    fn convolve_head(&mut self, x: &[f64], h: &[f64]) -> Vec<f64> {
        let n = (x.len() + h.len()).next_power_of_two();
        let mut xa = x.to_vec();
        xa.resize(n, 0.0);
        let mut ha = h.to_vec();
        ha.resize(n, 0.0);
        let (xs, hs) = (self.rfft(&xa), self.rfft(&ha));
        let prod: Vec<Complex64> = xs.iter().zip(&hs).map(|(a, b)| a * b).collect();
        let mut y = self.irfft(&prod, n);
        y.truncate(x.len());
        y
    }
}

/// taps of the body's transfer filter at 48 kHz (scaled with the rate; `proto/board.py` H_FILTER_N)
const H_FILTER_N: usize = 4096;

/// The transfer H (log2 grid, dB) as a minimum-phase filter on each row (`proto/board.py`
/// `h_filter`: the board's modes are summed at flat weights, H shapes them afterwards).
pub fn body_filter(log_grid: &[f64], h_db: &[f64], sr: f64, rows: &mut [Vec<f64>]) {
    let mut fft = Fft::new();
    let n = ((H_FILTER_N as f64 * sr / 48000.0) as usize).next_power_of_two();
    let h_eq: Vec<f64> = rfftfreq(n, sr).iter().map(|&f| interp(f.max(1.0).log2(), log_grid, h_db)).collect();
    let h = fft.min_phase(&h_eq, n);
    for row in rows {
        *row = fft.convolve_head(row, &h);
    }
}

fn to_f32_stereo(ch: &[Vec<f64>; 2], scale: f64) -> Vec<f32> {
    ch[0].iter().chain(&ch[1]).map(|&v| (v * scale) as f32).collect()
}

/// Mechanical noise from its statistics (`release.noise_burst`; release noise of a key,
/// pedal thumps): impacts (t0 s, level dB, decay s) shape the envelope of coherent
/// stereo noise, filtered to 1/3-octave band levels; channel-major stereo, peak 1.
#[allow(clippy::too_many_arguments)]
pub fn noise_burst(
    seed: u64,
    impacts: &[[f64; 3]],
    band_db: &[f64],
    bands_hz: &[f64],
    length_s: f64,
    rise_s: f64,
    spacing: f64,
    sr: f64,
) -> Vec<f32> {
    let mut fft = Fft::new();
    let mut rng = Rng::new(seed);
    let n = (length_s * sr) as usize;
    let env: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / sr;
            impacts
                .iter()
                .filter(|im| t >= im[0])
                .map(|&[t0, lv, tau]| {
                    let dt = t - t0;
                    10f64.powf(lv / 20.0) * (0.5 * PI * (dt / rise_s).clamp(0.0, 1.0)).sin().powi(2) * (-dt / tau.max(1e-3)).exp()
                })
                .sum()
        })
        .collect();
    let mut x = fft.coherent_pair(&mut rng, n, sr, spacing, |_| 1.0);
    for c in &mut x {
        c.iter_mut().zip(&env).for_each(|(v, e)| *v *= e);
    }
    // band powers → power density (bandwidth ∝ f); its dB value is the filter magnitude
    let nf = 4096;
    let psd_db: Vec<f64> = band_db.iter().zip(bands_hz).map(|(b, f)| b - 10.0 * (f / bands_hz[0]).log10()).collect();
    let log_bands: Vec<f64> = bands_hz.iter().map(|f| f.log2()).collect();
    let shape_db: Vec<f64> = rfftfreq(nf, sr).iter().map(|&f| interp(f.max(1.0).log2(), &log_bands, &psd_db)).collect();
    let h = fft.min_phase(&shape_db, nf);
    let y = [fft.convolve_head(&x[0], &h), fft.convolve_head(&x[1], &h)];
    let peak = y.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs()));
    to_f32_stereo(&y, 1.0 / peak.max(1e-30))
}

/// Action knock of one key (`soundboard.knock_burst`, amp 1): coherent stereo noise,
/// 8th-order lowpass at cutoff, 2nd-order highpass at 80 Hz, soft onset, exponential
/// decay; length 10 decay times; channel-major stereo.
pub fn knock_burst(note: u8, decay_s: f64, cutoff_hz: f64, rise_s: f64, spacing: f64, sr: f64) -> Vec<f32> {
    let mut fft = Fft::new();
    let mut rng = Rng::new(1000 + note as u64);
    let m = (10.0 * decay_s * sr) as usize;
    let shape = |f: f64| {
        let hp = (f / 80.0).powi(2) / (1.0 + (f / 80.0).powi(4)).sqrt();
        hp / (1.0 + (f / cutoff_hz).powi(16)).sqrt()
    };
    let mut b = fft.coherent_pair(&mut rng, m, sr, spacing, shape);
    let peak = b.iter().flatten().fold(0.0f64, |a, v| a.max(v.abs())).max(1e-12);
    for c in &mut b {
        for (i, v) in c.iter_mut().enumerate() {
            let t = i as f64 / sr;
            let rise = (0.5 * PI * (t / rise_s).clamp(0.0, 1.0)).sin().powi(2);
            *v *= rise * (-t / decay_s).exp() / peak;
        }
    }
    to_f32_stereo(&b, 1.0)
}

/// Length of the room's envelope filter at 48 kHz (`soundboard.ROOM_EQ_N`).
const ROOM_EQ_N: usize = 8192;

/// Diffuse room response (`soundboard.room_ir`): complex noise in the STFT domain
/// (512-point frames, hop 256), per bin the measured decay T60(f), AB-pair coherence;
/// then the spectral envelope H as one minimum-phase filter (`ROOM_EQ_N` at the
/// reference rate: the frames' 94 Hz bins could not follow a steep low end); unit
/// energy per channel; then the room's own level `eq_db` on top (a cut takes energy away).
/// `coupling` scales the inter-channel coherence (1 for omni microphones).
pub fn room_ir(seed: u64, spec: &RoomSpec, spacing: f64, coupling: f64, sr: f64) -> [Vec<f64>; 2] {
    let (t60, grid_hz, h_db) = (&spec.t60, &spec.grid_hz, &spec.h_db);
    let mut fft = Fft::new();
    let mut rng = Rng::new(seed);
    let n = (spec.length_s * sr) as usize;
    let nfft = n.next_power_of_two();
    let (hop, frame) = (256, 512);
    let frames = nfft / hop;
    let freqs = rfftfreq(frame, sr);
    let log_t60_f: Vec<f64> = t60.iter().map(|p| p[0].ln()).collect();
    let log_t60: Vec<f64> = t60.iter().map(|p| p[1].ln()).collect();
    let log_grid: Vec<f64> = grid_hz.iter().map(|f| f.log2()).collect();
    let bins: Vec<(f64, f64)> = freqs
        .iter()
        .map(|&f| {
            let fc = f.max(1.0);
            let t = interp(fc.ln(), &log_t60_f, &log_t60).exp();
            (t, coherence(f, spacing) * coupling)
        })
        .collect();
    let win: Vec<f64> = (0..frame).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (frame - 1) as f64).cos()).collect();
    let mut ir = [vec![0.0; frames * hop + frame], vec![0.0; frames * hop + frame]];
    for k in 0..frames {
        let t = (k * hop) as f64 / sr;
        let mut l = Vec::with_capacity(bins.len());
        let mut r = Vec::with_capacity(bins.len());
        for &(t60, c) in &bins {
            let a = Complex64::new(rng.gauss(), rng.gauss());
            let b = Complex64::new(rng.gauss(), rng.gauss());
            let g = (-6.91 * t / t60).exp();
            l.push(a * g);
            r.push((a * c + b * (1.0 - c * c).max(0.0).sqrt()) * g);
        }
        for (ch, spec) in [l, r].iter().enumerate() {
            let y = fft.irfft(spec, frame);
            for (i, v) in y.iter().enumerate() {
                ir[ch][k * hop + i] += v * win[i];
            }
        }
    }
    // the envelope filter keeps its resolution in Hz at any rate
    let neq = ((ROOM_EQ_N as f64 * sr / 48000.0) as usize).next_power_of_two();
    let h_eq: Vec<f64> = rfftfreq(neq, sr).iter().map(|&f| interp(f.max(1.0).log2(), &log_grid, h_db)).collect();
    let h = fft.min_phase(&h_eq, neq);
    let eq = (!spec.eq_db.is_empty()).then(|| {
        let f: Vec<f64> = spec.eq_db.iter().map(|p| p[0].log2()).collect();
        let v: Vec<f64> = spec.eq_db.iter().map(|p| p[1]).collect();
        let q: Vec<f64> = rfftfreq(neq, sr).iter().map(|&x| interp(x.max(1.0).log2(), &f, &v)).collect();
        fft.min_phase(&q, neq)
    });
    for c in &mut ir {
        c.truncate(n);
        *c = fft.convolve_head(c, &h);
        let e = c.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-30);
        c.iter_mut().for_each(|v| *v /= e);
        if let Some(q) = &eq {
            *c = fft.convolve_head(c, q);
        }
    }
    ir
}
