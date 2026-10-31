//! The correction's 61 bands (1/6 octave, 20 Hz … 20.5 kHz) applied to a response: raised-cosine
//! masks in log frequency whose squares sum to one, zero-phase, with half a second of padding on
//! both sides so the narrow bands' filters can ring; a band's level and decay rate; a response
//! given another's 1/6-octave color.
use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

pub const BANDS: usize = 61;

/// The band centers: 20 Hz · 2^(k/6).
pub fn grid() -> [f64; BANDS] {
    std::array::from_fn(|k| 20.0 * 2f64.powf(k as f64 / 6.0))
}

/// The padding on each side: half a second.
pub fn pad(sr: f64) -> usize {
    (sr / 2.0).round() as usize
}

/// One gain per bin of an `n`-point real FFT for each band; the end bands flat beyond the grid.
pub fn masks(n: usize, sr: f64) -> Vec<Vec<f64>> {
    let g = grid();
    let lf: Vec<f64> = (0..=n / 2).map(|i| (i as f64 * sr / n as f64).max(1.0).log2()).collect();
    (0..BANDS)
        .map(|k| {
            let c = g[k].log2();
            lf.iter()
                .map(|&l| {
                    if (k == 0 && l < c) || (k == BANDS - 1 && l > c) {
                        return 1.0;
                    }
                    let u = (l - c) * 6.0;
                    if u.abs() < 1.0 { (std::f64::consts::FRAC_PI_2 * u).cos() } else { 0.0 }
                })
                .collect()
        })
        .collect()
}

pub(crate) fn forward(x: &[f64]) -> Vec<Complex<f64>> {
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(x.len());
    let mut input = x.to_vec();
    let mut out = fft.make_output_vec();
    fft.process(&mut input, &mut out).expect("fft");
    out
}

pub(crate) fn inverse(mut spec: Vec<Complex<f64>>, n: usize) -> Vec<f64> {
    let fft = RealFftPlanner::<f64>::new().plan_fft_inverse(n);
    spec[0].im = 0.0;
    if n.is_multiple_of(2) {
        let last = spec.len() - 1;
        spec[last].im = 0.0;
    }
    let mut out = fft.make_output_vec();
    fft.process(&mut spec, &mut out).expect("ifft");
    out.iter_mut().for_each(|v| *v /= n as f64);
    out
}

/// `x` split into its bands, each `pad + len + pad` long (x starting at `pad`).
pub fn split(x: &[f64], sr: f64) -> Vec<Vec<f64>> {
    let p = pad(sr);
    let mut y = vec![0.0; x.len() + 2 * p];
    y[p..p + x.len()].copy_from_slice(x);
    let n = y.len();
    let spec = forward(&y);
    masks(n, sr).iter().map(|m| inverse(spec.iter().zip(m).map(|(c, g)| c * g).collect(), n)).collect()
}

/// The inverse of `split`: each band filtered by its mask again and summed, cut back to the
/// original span.
pub fn merge(b: &[Vec<f64>], sr: f64) -> Vec<f64> {
    let n = b[0].len();
    let p = pad(sr);
    let m = masks(n, sr);
    let mut sum = vec![Complex::new(0.0, 0.0); n / 2 + 1];
    for (row, mask) in b.iter().zip(&m) {
        for ((s, c), g) in sum.iter_mut().zip(forward(row)).zip(mask) {
            *s += c * g;
        }
    }
    inverse(sum, n)[p..n - p].to_vec()
}

/// Per band of `x`: its level (dB of its energy) and its decay rate (dB/s: minus the slope of a
/// line through its energy envelope in dB, the squared band smoothed over 2 ms, from 3 to 80 ms;
/// below 100 Hz, where a band's own filter rings for tens of ms, from 50 to 350 ms), the rates
/// then smoothed over three bands on each side of 100 Hz.
pub fn band_stats(x: &[f64], sr: f64) -> (Vec<f64>, Vec<f64>) {
    let p = pad(sr);
    let g = grid();
    let b = split(x, sr);
    let level: Vec<f64> = b.iter().map(|row| 10.0 * (row.iter().map(|v| v * v).sum::<f64>() + 1e-30).log10()).collect();
    let k = ((0.002 * sr) as usize).max(1);
    let raw: Vec<f64> = b
        .iter()
        .zip(&g)
        .map(|(row, &f)| {
            let sq: Vec<f64> = row[p..p + x.len()].iter().map(|v| v * v).collect();
            let mut cs = vec![0.0; sq.len() + 1];
            for (i, v) in sq.iter().enumerate() {
                cs[i + 1] = cs[i] + v;
            }
            // a 2 ms box centered on each sample ("same")
            let env = |i: usize| {
                let lo = i.saturating_sub(k / 2);
                let hi = (i + k - k / 2).min(sq.len());
                (cs[hi] - cs[lo]) / k as f64
            };
            let (a, z) = if f < 100.0 { (0.05, 0.35) } else { (0.003, 0.08) };
            let (i0, i1) = ((a * sr).ceil() as usize, ((z * sr).floor() as usize).min(sq.len().saturating_sub(1)));
            let (mut st, mut sy, mut stt, mut sty, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for i in i0..=i1 {
                let t = i as f64 / sr;
                let y = 10.0 * (env(i) + 1e-30).log10();
                st += t;
                sy += y;
                stt += t * t;
                sty += t * y;
                n += 1.0;
            }
            -(n * sty - st * sy) / (n * stt - st * st)
        })
        .collect();
    // smoothed over three bands, each side of 100 Hz on its own (the two windows measure differently)
    let low = |i: usize| g[i] < 100.0;
    let rate = (0..BANDS)
        .map(|i| {
            let near: Vec<f64> = [i.saturating_sub(1), i, (i + 1).min(BANDS - 1)].iter().filter(|&&j| low(j) == low(i)).map(|&j| raw[j]).collect();
            near.iter().sum::<f64>() / near.len() as f64
        })
        .collect();
    (level, rate)
}

/// Power over 1/6 octave around each bin (a box in log frequency).
fn smooth_power(p: &[f64], f: &[f64]) -> Vec<f64> {
    let mut cs = vec![0.0; p.len() + 1];
    for (i, v) in p.iter().enumerate() {
        cs[i + 1] = cs[i] + v;
    }
    let r = 2f64.powf(1.0 / 12.0);
    f.iter()
        .map(|&fc| {
            let lo = f.partition_point(|&x| x < fc / r);
            let hi = f.partition_point(|&x| x <= fc * r).max(lo + 1);
            (cs[hi] - cs[lo]) / (hi - lo) as f64
        })
        .collect()
}

/// `x` given `like`'s power smoothed over 1/6 octave (zero-phase; computed at twice the length).
pub fn recolor(x: &[f64], like: &[f64], sr: f64) -> Vec<f64> {
    let n = 2 * x.len();
    let pad = |v: &[f64]| {
        let mut y = vec![0.0; n];
        y[..v.len().min(n)].copy_from_slice(&v[..v.len().min(n)]);
        y
    };
    let (sx, sy) = (forward(&pad(x)), forward(&pad(like)));
    let f: Vec<f64> = (0..sx.len()).map(|i| (i as f64 * sr / n as f64).max(1.0)).collect();
    let (px, py) = (smooth_power(&sx.iter().map(|c| c.norm_sqr()).collect::<Vec<_>>(), &f), smooth_power(&sy.iter().map(|c| c.norm_sqr()).collect::<Vec<_>>(), &f));
    let out = inverse(sx.iter().zip(px.iter().zip(&py)).map(|(c, (a, b))| c * (b / a.max(1e-300)).sqrt()).collect(), n);
    out[..x.len()].to_vec()
}

/// Per frequency: the response's level against its median over a third of an octave around it (dB;
/// negative: in a notch), the power averaged over ±1 cent (a unison's strings), as
/// `proto/board.py` `notch_depths`.
pub fn notch_depths(ir: &[f64], sr: f64, freqs: &[f64]) -> Vec<f64> {
    let n = (16 * ir.len()).next_power_of_two();
    let mut x = vec![0.0; n];
    x[..ir.len()].copy_from_slice(ir);
    let p: Vec<f64> = forward(&x).iter().map(|c| c.norm_sqr()).collect();
    let df = sr / n as f64;
    let at = |f: f64| {
        let i = (f / df).floor() as usize;
        let t = f / df - i as f64;
        match (p.get(i), p.get(i + 1)) {
            (Some(a), Some(b)) => a + (b - a) * t,
            _ => 0.0,
        }
    };
    freqs
        .iter()
        .map(|&f| {
            let (lo, hi) = (((f * 2f64.powf(-1.0 / 6.0)) / df).ceil() as usize, ((f * 2f64.powf(1.0 / 6.0)) / df).floor() as usize);
            let mut db: Vec<f64> = p[lo.min(p.len())..(hi + 1).min(p.len())].iter().map(|v| 10.0 * (v + 1e-30).log10()).collect();
            if db.is_empty() {
                return 0.0;
            }
            db.sort_by(f64::total_cmp);
            let median = if db.len() % 2 == 1 { db[db.len() / 2] } else { 0.5 * (db[db.len() / 2 - 1] + db[db.len() / 2]) };
            let near = (0..5).map(|j| at(f * 2f64.powf((-1.0 + 0.5 * j as f64) / 1200.0))).sum::<f64>() / 5.0;
            10.0 * (near + 1e-30).log10() - median
        })
        .collect()
}
