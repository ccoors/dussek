//! The inner loops: banks of complex one-poles in float32, structure of arrays, LANES
//! resonators side by side so the compiler vectorizes across resonators. Each lane's
//! real part is accumulated per sample into `acc` (one `Lane` per sample); the caller
//! sums the lanes once per sample after all banks have run.
//!
//! Dispatch: AVX2+FMA when the CPU has it (x86-64), otherwise the portable build
//! (with fused multiply-adds on aarch64, where they are part of the base ISA).

pub const LANES: usize = 16;
pub type Lane = [f32; LANES];

#[inline(always)]
fn cmul<const FMA: bool>(ar: f32, ai: f32, br: f32, bi: f32) -> (f32, f32) {
    if FMA { (ar.mul_add(br, -(ai * bi)), ar.mul_add(bi, ai * br)) } else { (ar * br - ai * bi, ar * bi + ai * br) }
}

/// Free decay: per sample, acc[t] += Re s, then s ← p·s.
#[inline(always)]
fn decay_body<const FMA: bool>(sr: &mut [Lane], si: &mut [Lane], pr: &[Lane], pi: &[Lane], acc: &mut [Lane]) {
    for (((sr, si), pr), pi) in sr.iter_mut().zip(si.iter_mut()).zip(pr).zip(pi) {
        let (mut r, mut i) = (*sr, *si);
        for a in acc.iter_mut() {
            for l in 0..LANES {
                a[l] += r[l];
                (r[l], i[l]) = cmul::<FMA>(r[l], i[l], pr[l], pi[l]);
            }
        }
        (*sr, *si) = (r, i);
    }
}

/// Driven: per sample, s ← b·x[t] + p·s, then acc[t] += Re s.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn driven_body<const FMA: bool>(
    sr: &mut [Lane],
    si: &mut [Lane],
    pr: &[Lane],
    pi: &[Lane],
    br: &[Lane],
    bi: &[Lane],
    x: &[f32],
    acc: &mut [Lane],
) {
    for (((((sr, si), pr), pi), br), bi) in sr.iter_mut().zip(si.iter_mut()).zip(pr).zip(pi).zip(br).zip(bi) {
        let (mut r, mut i) = (*sr, *si);
        for (a, &x) in acc.iter_mut().zip(x) {
            for l in 0..LANES {
                let (pr_, pi_) = cmul::<FMA>(r[l], i[l], pr[l], pi[l]);
                if FMA {
                    r[l] = br[l].mul_add(x, pr_);
                    i[l] = bi[l].mul_add(x, pi_);
                } else {
                    r[l] = br[l] * x + pr_;
                    i[l] = bi[l] * x + pi_;
                }
                a[l] += r[l];
            }
        }
        (*sr, *si) = (r, i);
    }
}

/// Driven with a fading drive: per sample b ← b + db, then s ← b·x[t] + p·s, acc[t] += Re s;
/// b is left at its last value.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn driven_ramp_body<const FMA: bool>(
    sr: &mut [Lane],
    si: &mut [Lane],
    pr: &[Lane],
    pi: &[Lane],
    br: &mut [Lane],
    bi: &mut [Lane],
    dbr: &[Lane],
    dbi: &[Lane],
    x: &[f32],
    acc: &mut [Lane],
) {
    for (((((((sr, si), pr), pi), br), bi), dbr), dbi) in
        sr.iter_mut().zip(si.iter_mut()).zip(pr).zip(pi).zip(br.iter_mut()).zip(bi.iter_mut()).zip(dbr).zip(dbi)
    {
        let (mut r, mut i, mut b_r, mut b_i) = (*sr, *si, *br, *bi);
        for (a, &x) in acc.iter_mut().zip(x) {
            for l in 0..LANES {
                b_r[l] += dbr[l];
                b_i[l] += dbi[l];
                let (pr_, pi_) = cmul::<FMA>(r[l], i[l], pr[l], pi[l]);
                if FMA {
                    r[l] = b_r[l].mul_add(x, pr_);
                    i[l] = b_i[l].mul_add(x, pi_);
                } else {
                    r[l] = b_r[l] * x + pr_;
                    i[l] = b_i[l] * x + pi_;
                }
                a[l] += r[l];
            }
        }
        (*sr, *si, *br, *bi) = (r, i, b_r, b_i);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
fn driven_ramp_avx2(
    sr: &mut [Lane],
    si: &mut [Lane],
    pr: &[Lane],
    pi: &[Lane],
    br: &mut [Lane],
    bi: &mut [Lane],
    dbr: &[Lane],
    dbi: &[Lane],
    x: &[f32],
    acc: &mut [Lane],
) {
    driven_ramp_body::<true>(sr, si, pr, pi, br, bi, dbr, dbi, x, acc)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
fn decay_avx2(sr: &mut [Lane], si: &mut [Lane], pr: &[Lane], pi: &[Lane], acc: &mut [Lane]) {
    decay_body::<true>(sr, si, pr, pi, acc)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
fn driven_avx2(sr: &mut [Lane], si: &mut [Lane], pr: &[Lane], pi: &[Lane], br: &[Lane], bi: &[Lane], x: &[f32], acc: &mut [Lane]) {
    driven_body::<true>(sr, si, pr, pi, br, bi, x, acc)
}

#[cfg(target_arch = "x86_64")]
fn has_avx2() -> bool {
    std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma")
}

/// Complex multiply-accumulate over spectra (interleaved complex float32): acc[k] += x[k] · h[k], the
/// convolution's products with its stored input spectra (`convolution::Stage::ahead`).
#[inline(always)]
fn cmac_body<const FMA: bool>(acc: &mut [realfft::num_complex::Complex32], x: &[realfft::num_complex::Complex32], h: &[realfft::num_complex::Complex32]) {
    for ((a, x), h) in acc.iter_mut().zip(x).zip(h) {
        let (r, i) = cmul::<FMA>(x.re, x.im, h.re, h.im);
        a.re += r;
        a.im += i;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
fn cmac_avx2(acc: &mut [realfft::num_complex::Complex32], x: &[realfft::num_complex::Complex32], h: &[realfft::num_complex::Complex32]) {
    use std::arch::x86_64::*;
    let n = acc.len().min(x.len()).min(h.len());
    let quads = n / 4;
    // SAFETY: AVX2 and FMA (the caller checked); four complex (eight floats) per step within n
    unsafe {
        let (pa, px, ph) = (acc.as_mut_ptr() as *mut f32, x.as_ptr() as *const f32, h.as_ptr() as *const f32);
        for q in 0..quads {
            let o = 8 * q;
            let xv = _mm256_loadu_ps(px.add(o));
            let hv = _mm256_loadu_ps(ph.add(o));
            let av = _mm256_loadu_ps(pa.add(o));
            // (xr + i xi)(hr + i hi): xr·(hr, hi) ∓ xi·(hi, hr)
            let xr = _mm256_moveldup_ps(xv);
            let xi = _mm256_movehdup_ps(xv);
            let hs = _mm256_permute_ps(hv, 0b1011_0001);
            let prod = _mm256_fmaddsub_ps(xr, hv, _mm256_mul_ps(xi, hs));
            _mm256_storeu_ps(pa.add(o), _mm256_add_ps(av, prod));
        }
    }
    cmac_body::<true>(&mut acc[4 * quads..n], &x[4 * quads..n], &h[4 * quads..n]);
}

/// acc[k] += x[k] · h[k] over complex spectra (AVX2+FMA where the CPU has it).
pub fn cmac(acc: &mut [realfft::num_complex::Complex32], x: &[realfft::num_complex::Complex32], h: &[realfft::num_complex::Complex32]) {
    #[cfg(target_arch = "x86_64")]
    if has_avx2() {
        // SAFETY: the CPU supports AVX2 and FMA (checked above)
        return unsafe { cmac_avx2(acc, x, h) };
    }
    cmac_body::<PORTABLE_FMA>(acc, x, h)
}

const PORTABLE_FMA: bool = cfg!(target_arch = "aarch64");

pub fn decay(sr: &mut [Lane], si: &mut [Lane], pr: &[Lane], pi: &[Lane], acc: &mut [Lane]) {
    #[cfg(target_arch = "x86_64")]
    if has_avx2() {
        // SAFETY: the CPU supports AVX2 and FMA (checked above)
        return unsafe { decay_avx2(sr, si, pr, pi, acc) };
    }
    decay_body::<PORTABLE_FMA>(sr, si, pr, pi, acc)
}

#[allow(clippy::too_many_arguments)]
pub fn driven(sr: &mut [Lane], si: &mut [Lane], pr: &[Lane], pi: &[Lane], br: &[Lane], bi: &[Lane], x: &[f32], acc: &mut [Lane]) {
    #[cfg(target_arch = "x86_64")]
    if has_avx2() {
        // SAFETY: the CPU supports AVX2 and FMA (checked above)
        return unsafe { driven_avx2(sr, si, pr, pi, br, bi, x, acc) };
    }
    driven_body::<PORTABLE_FMA>(sr, si, pr, pi, br, bi, x, acc)
}

#[allow(clippy::too_many_arguments)]
pub fn driven_ramp(
    sr: &mut [Lane],
    si: &mut [Lane],
    pr: &[Lane],
    pi: &[Lane],
    br: &mut [Lane],
    bi: &mut [Lane],
    dbr: &[Lane],
    dbi: &[Lane],
    x: &[f32],
    acc: &mut [Lane],
) {
    #[cfg(target_arch = "x86_64")]
    if has_avx2() {
        // SAFETY: the CPU supports AVX2 and FMA (checked above)
        return unsafe { driven_ramp_avx2(sr, si, pr, pi, br, bi, dbr, dbi, x, acc) };
    }
    driven_ramp_body::<PORTABLE_FMA>(sr, si, pr, pi, br, bi, dbr, dbi, x, acc)
}

/// Sum of the lanes of each sample (in double precision, as a pairwise tree: it vectorizes, where a
/// sum in the lanes' order could not).
#[inline(always)]
fn lane_sums_body(acc: &[Lane], out: &mut [f64]) {
    for (a, o) in acc.iter().zip(out) {
        let mut v = [0.0f64; LANES];
        for (d, &x) in v.iter_mut().zip(a) {
            *d = x as f64;
        }
        let mut half = LANES / 2;
        while half > 0 {
            for i in 0..half {
                v[i] += v[i + half];
            }
            half /= 2;
        }
        *o += v[0];
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
fn lane_sums_avx2(acc: &[Lane], out: &mut [f64]) {
    lane_sums_body(acc, out)
}

pub fn lane_sums(acc: &[Lane], out: &mut [f64]) {
    #[cfg(target_arch = "x86_64")]
    if has_avx2() {
        // SAFETY: the CPU supports AVX2 and FMA (checked above)
        return unsafe { lane_sums_avx2(acc, out) };
    }
    lane_sums_body(acc, out)
}

/// Lane-padded length.
pub fn padded(n: usize) -> usize {
    n.div_ceil(LANES) * LANES
}
