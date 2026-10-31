//! The pillow: a pillow or blanket against the soundboard. The body's radiated response
//! gets duller (a lowpass), quieter and shorter (extra loss in the board's modes, as an
//! exponential decay). Applied to every finished body response, and (lowpass and level
//! only, no decay) to the room, which is fed by the board's radiation. The amounts are
//! starting values, tuned by ear.

/// cutoff at full amount (Hz), level at full amount (dB), decay time constant at full amount (s)
pub const PILLOW_HZ: f64 = 600.0;
pub const PILLOW_DB: f64 = -12.0;
pub const PILLOW_TAU_S: f64 = 0.025;
/// the lowpass's cutoff with no pillow (Hz): open
const OPEN_HZ: f64 = 20000.0;

/// The slider value made safe: non-finite becomes 0, the rest is clamped to 0 … 1.
pub fn sanitize(amount: f64) -> f64 {
    if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 0.0 }
}

/// The shaping curve: steeper at the bottom of the slider (0.25 gives 0.4375, 0.5 gives 0.75).
fn shape(p: f64) -> f64 {
    1.0 - (1.0 - p).powi(2)
}

/// The lowpass cutoff for `amount` (0 … 1), never above 0.45 of the sample rate.
pub fn cutoff_hz(amount: f64, sr: f64) -> f64 {
    let q = shape(amount.clamp(0.0, 1.0));
    (OPEN_HZ.ln() * (1.0 - q) + PILLOW_HZ.ln() * q).exp().min(0.45 * sr)
}

/// Applies the pillow to a body impulse response in place: lowpass, level and extra decay;
/// `amount` ≤ 0 (or NaN) does nothing.
pub fn apply(ir: &mut [f32], amount: f64, sr: f64) {
    filter(ir, amount, sr, true);
}

/// Applies the pillow to a room impulse response in place: the same lowpass and level, but
/// no decay (the reverb tail is the room's, not the board's modes); `amount` ≤ 0 (or NaN)
/// does nothing.
pub fn apply_room(ir: &mut [f32], amount: f64, sr: f64) {
    filter(ir, amount, sr, false);
}

fn filter(ir: &mut [f32], amount: f64, sr: f64, decay: bool) {
    if !(amount > 0.0) {
        return;
    }
    let p = amount.min(1.0);
    // second-order Butterworth lowpass (RBJ biquad, Q = 1/√2), transposed direct form II
    let w0 = 2.0 * std::f64::consts::PI * cutoff_hz(p, sr) / sr;
    let (s, c) = w0.sin_cos();
    let alpha = s / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
    let a0 = 1.0 + alpha;
    let (b0, b1, b2) = ((1.0 - c) / 2.0 / a0, (1.0 - c) / a0, (1.0 - c) / 2.0 / a0);
    let (a1, a2) = (-2.0 * c / a0, (1.0 - alpha) / a0);
    let (mut z1, mut z2) = (0.0f64, 0.0f64);
    let q = shape(p);
    let gain = 10f64.powf(PILLOW_DB * q / 20.0);
    let rate = if decay { q / PILLOW_TAU_S } else { 0.0 };
    for (i, v) in ir.iter_mut().enumerate() {
        let x = *v as f64;
        let y = b0 * x + z1;
        z1 = b1 * x - a1 * y + z2;
        z2 = b2 * x - a2 * y;
        *v = (y * gain * (-rate * i as f64 / sr).exp()) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a deterministic noise burst, 0.25 s
    fn noise(sr: f64) -> Vec<f32> {
        let mut s = 12345u32;
        (0..(0.25 * sr) as usize)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 8) as f32 / 8_388_608.0 - 1.0
            })
            .collect()
    }

    fn energy(v: &[f32]) -> f64 {
        v.iter().map(|&x| (x as f64).powi(2)).sum()
    }

    /// energy of the first difference: a crude high-frequency measure
    fn hf(v: &[f32]) -> f64 {
        v.windows(2).map(|w| ((w[1] - w[0]) as f64).powi(2)).sum()
    }

    #[test]
    fn zero_is_untouched_and_nan_is_ignored() {
        let a = noise(48000.0);
        for amount in [0.0, -1.0, f64::NAN] {
            let mut b = a.clone();
            apply(&mut b, amount, 48000.0);
            assert_eq!(a, b, "{amount}");
        }
    }

    #[test]
    fn the_pillow_makes_it_duller_quieter_and_shorter_monotonically() {
        let a = noise(48000.0);
        let half = a.len() / 2;
        let mut last = (f64::INFINITY, f64::INFINITY, f64::INFINITY);
        for amount in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mut b = a.clone();
            apply(&mut b, amount, 48000.0);
            let m = (energy(&b), hf(&b) / energy(&b).max(1e-30), energy(&b[half..]) / energy(&b).max(1e-30));
            assert!(m.0 <= last.0 + 1e-9 && m.1 <= last.1 + 1e-9 && m.2 <= last.2 + 1e-9, "{amount}: {m:?} after {last:?}");
            last = m;
        }
        // at 100 %: clearly quieter, duller, shorter
        let mut full = a.clone();
        apply(&mut full, 1.0, 48000.0);
        assert!(energy(&full) < 0.1 * energy(&a), "quieter");
        assert!(hf(&full) / energy(&full) < 0.5 * hf(&a) / energy(&a), "duller");
        assert!(energy(&full[half..]) / energy(&full) < 0.1 * energy(&a[half..]) / energy(&a), "shorter");
    }

    #[test]
    fn stable_and_finite_at_every_rate_and_length() {
        for sr in [22050.0, 44100.0, 48000.0, 96000.0] {
            for len in [1usize, 2, 7, 64, 4800] {
                for amount in [0.001, 0.5, 1.0, 5.0] {
                    let mut v: Vec<f32> = noise(sr).into_iter().take(len).collect();
                    apply(&mut v, amount, sr);
                    assert!(v.iter().all(|x| x.is_finite()), "{sr} {len} {amount}");
                    assert!(cutoff_hz(amount, sr) <= 0.45 * sr + 1e-9);
                }
            }
        }
        assert!((cutoff_hz(1.0, 48000.0) - PILLOW_HZ).abs() < 1e-6);
    }

    #[test]
    fn the_curve_is_steeper_at_the_bottom() {
        let (a, b) = (cutoff_hz(0.25, 48000.0), cutoff_hz(0.5, 48000.0));
        assert!((4000.0..4600.0).contains(&a) && (1300.0..1500.0).contains(&b), "{a} {b}");
        assert_eq!(sanitize(f64::NAN), 0.0);
        assert_eq!(sanitize(7.0), 1.0);
    }

    /// a decaying noise burst (like a room tail)
    fn decaying(sr: f64) -> Vec<f32> {
        noise(sr).into_iter().enumerate().map(|(i, v)| v * (-(i as f64) / (0.08 * sr)).exp() as f32).collect()
    }

    #[test]
    fn room_is_untouched_at_zero_nan_and_negative() {
        let a = noise(48000.0);
        for amount in [0.0, -1.0, f64::NAN] {
            let mut b = a.clone();
            apply_room(&mut b, amount, 48000.0);
            assert_eq!(a, b, "{amount}");
        }
    }

    #[test]
    fn room_keeps_its_tail_and_gets_quieter_and_duller_monotonically() {
        let a = decaying(48000.0);
        let half = a.len() / 2;
        let tail = |v: &[f32]| energy(&v[half..]) / energy(v);
        let mut last = (f64::INFINITY, f64::INFINITY);
        for amount in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mut b = a.clone();
            apply_room(&mut b, amount, 48000.0);
            let m = (energy(&b), hf(&b) / energy(&b));
            assert!(m.0 <= last.0 + 1e-9 && m.1 <= last.1 + 1e-9, "{amount}: {m:?} after {last:?}");
            last = m;
        }
        let mut full = a.clone();
        apply_room(&mut full, 1.0, 48000.0);
        assert!(energy(&full) < 0.5 * energy(&a), "quieter");
        assert!(hf(&full) / energy(&full) < 0.5 * hf(&a) / energy(&a), "duller");
        assert!(tail(&full) >= 0.8 * tail(&a), "tail kept: {} vs {}", tail(&full), tail(&a));
    }

    #[test]
    fn room_is_stable_and_finite_at_every_rate_and_length() {
        for sr in [22050.0, 44100.0, 48000.0, 96000.0] {
            for len in [1usize, 2, 7, 64, 4800] {
                for amount in [0.001, 0.5, 1.0, 5.0] {
                    let mut v: Vec<f32> = noise(sr).into_iter().take(len).collect();
                    apply_room(&mut v, amount, sr);
                    assert!(v.iter().all(|x| x.is_finite()), "{sr} {len} {amount}");
                }
            }
        }
    }
}
