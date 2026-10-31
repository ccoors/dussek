//! Tuner's stretch, computed from an instrument's own inharmonicity.
//!
//! A stiff string's partials are sharp (f_k = k·f0·√(1 + B·k²)), so a tuner stretches the
//! octaves: the note an octave up is tuned so that a partial of the lower note beats
//! against nothing. Matching partial 2m of the lower note n with partial m of the upper
//! one, n + 12, puts their fundamentals
//!
//!   f0(n + 12) / f0(n) = 2 · √((1 + 4m²·B(n)) / (1 + m²·B(n + 12))).
//!
//! Tuners match higher partials in the bass (m ≈ 6 below A1, a 12:6 octave) and low ones in
//! the treble, where the upper partials are too high to hear (m = 1, a plain 2:1). `ORDER`
//! interpolates that between the anchor notes. The octave excess spread over the twelve
//! semitones and summed outwards from A4 (the concert pitch, which stays put) gives each
//! note's offset in cents: flat bass, sharp treble, 0 at A4. On a grand fitted from a
//! well-tuned recording the curve lands within a few cents of the recording's own
//! stretch (grand_a: −19 / +25 cents at A0 / C8 recorded, −20 / +25 computed).

/// (note, order m of the partial match): linear in between, clamped at the ends
const ORDER: [(f64, f64); 5] = [(21.0, 6.0), (45.0, 3.5), (69.0, 2.0), (93.0, 1.5), (108.0, 1.0)];
const A4: i32 = 69;
/// B is smoothed over ±this many notes in log B before use: the fitted values scatter
const SMOOTH: usize = 3;

fn order(note: f64) -> f64 {
    if note <= ORDER[0].0 {
        return ORDER[0].1;
    }
    for w in ORDER.windows(2) {
        if note <= w[1].0 {
            let t = (note - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + t * (w[1].1 - w[0].1);
        }
    }
    ORDER[ORDER.len() - 1].1
}

/// Offsets in cents for the notes `lo, lo + 1, …` with inharmonicities `b` (one register).
/// Fewer than 14 notes, or non-positive or non-finite B: zeros.
pub fn offsets(lo: u8, b: &[f64]) -> Vec<f64> {
    let n = b.len();
    if n < 14 || b.iter().any(|&x| !(x > 0.0 && x.is_finite())) {
        return vec![0.0; n];
    }
    // smoothed B (geometric mean over the neighbors, the edges repeated)
    let lb: Vec<f64> = b.iter().map(|x| x.ln()).collect();
    let sb: Vec<f64> = (0..n)
        .map(|i| {
            let s: f64 = (0..=2 * SMOOTH).map(|j| lb[(i + j).saturating_sub(SMOOTH).min(n - 1)]).sum();
            (s / (2 * SMOOTH + 1) as f64).exp()
        })
        .collect();
    // cents of excess of the octave starting at note index i
    let excess = |i: usize| {
        let j = i.min(n - 13); // near the top: the last octave's excess
        let m = order(lo as f64 + i as f64);
        1200.0 * (((1.0 + 4.0 * m * m * sb[j]) / (1.0 + m * m * sb[j + 12])).sqrt()).log2()
    };
    let r = (A4 - lo as i32).clamp(0, n as i32 - 1) as usize;
    let mut c = vec![0.0; n];
    for i in r + 1..n {
        c[i] = c[i - 1] + excess(i - 1) / 12.0;
    }
    for i in (0..r).rev() {
        c[i] = c[i + 1] - excess(i) / 12.0;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B(n) of a typical piano: 3e-4 up to middle C, then ×2.6 per octave (2.6e-2 at C8),
    /// and rising a little in the lowest bass
    fn typical() -> Vec<f64> {
        (21..=108)
            .map(|n| {
                let n = n as f64;
                3e-4 * ((n - 60.0).max(0.0) * 0.0796).exp() * (1.0 + (36.0 - n).max(0.0) / 30.0)
            })
            .collect()
    }

    #[test]
    fn a4_stays_and_the_bass_is_flat_and_the_treble_sharp() {
        let c = offsets(21, &typical());
        assert_eq!(c.len(), 88);
        assert_eq!(c[48], 0.0); // A4
        assert!(c[0] < -5.0 && c[0] > -60.0, "A0 {}", c[0]);
        assert!(c[87] > 5.0 && c[87] < 60.0, "C8 {}", c[87]);
        assert!(c[..48].iter().all(|&x| x < 0.0) && c[49..].iter().all(|&x| x > 0.0));
    }

    #[test]
    fn no_inharmonicity_no_stretch() {
        let c = offsets(21, &vec![1e-12; 88]);
        assert!(c.iter().all(|x| x.abs() < 1e-6));
    }

    #[test]
    fn the_octave_matches_the_partials() {
        // a constant B: the octave from A4 up (indices 48 → 60) is the twelve semitone steps
        // of the excess 1200·log2 √((1+4m²B)/(1+m²B)) at each note's order m, a twelfth each
        let c = offsets(21, &vec![5e-4; 88]);
        let oct = c[60] - c[48];
        let want: f64 = (48..60).map(|i| 1200.0 * ((1.0 + 4.0 * order(21.0 + i as f64).powi(2) * 5e-4) / (1.0 + order(21.0 + i as f64).powi(2) * 5e-4)).sqrt().log2() / 12.0).sum();
        assert!((oct - want).abs() < 1e-9, "{oct} vs {want}");
    }

    #[test]
    fn bad_input_is_no_stretch() {
        assert!(offsets(21, &[1e-4; 5]).iter().all(|&x| x == 0.0));
        let mut b = typical();
        b[10] = f64::NAN;
        assert!(offsets(21, &b).iter().all(|&x| x == 0.0));
        b[10] = -1.0;
        assert!(offsets(21, &b).iter().all(|&x| x == 0.0));
    }
}
