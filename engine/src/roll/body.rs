//! A piano's body as the roll sees it: per band its level and decay rate (the mean over the 2
//! polarizations × 2 microphones) and the per-note correction (15 notes × 61). Applied to an
//! export band by band (a gain and an exponential: its fine structure and its microphones'
//! differences stay), the board's radiation following; widening gives both microphones one color.
use super::bands::{self, BANDS, grid};
use super::files::Export;
use super::measure::NOTES;
use crate::instrument::Error;

#[derive(Clone)]
pub struct Body {
    pub level: [f64; BANDS],
    pub rate: [f64; BANDS],
    /// 15 notes × 61 (dB)
    pub corr: Vec<f64>,
}

/// a rate weighs as the level it changes over 50 ms
const RATE_W: f64 = 0.05;

fn corr_w() -> f64 {
    (61.0f64 / 915.0).sqrt()
}

fn rate_of(e: &Export) -> f64 {
    e.json.get("reference_sr").and_then(|v| v.as_f64()).unwrap_or(48000.0)
}

/// The responses' band levels, the mean over polarizations and microphones (dB, no mean removed).
pub fn levels(e: &Export) -> Result<[f64; BANDS], Error> {
    Ok(stats(e)?.0)
}

fn stats(e: &Export) -> Result<([f64; BANDS], [f64; BANDS]), Error> {
    let sr = rate_of(e);
    let rs = e.responses()?;
    let all: Vec<(Vec<f64>, Vec<f64>)> = std::thread::scope(|s| {
        let h: Vec<_> = rs.iter().map(|r| s.spawn(move || bands::band_stats(&r.ir, sr))).collect();
        h.into_iter().map(|h| h.join().expect("band stats")).collect()
    });
    let n = all.len() as f64;
    Ok((std::array::from_fn(|k| all.iter().map(|a| a.0[k]).sum::<f64>() / n), std::array::from_fn(|k| all.iter().map(|a| a.1[k]).sum::<f64>() / n)))
}

/// The export's body: the level with its mean over the bands from 80 Hz up removed, the
/// correction with its own mean removed.
pub fn describe(e: &Export) -> Result<Body, Error> {
    Ok(describe_raw(e)?.0)
}

/// The correction read as the roll reads it: the 15 notes of `measure::NOTES` × the 61 bands of
/// `grid` (any other layout refused, named).
fn correction(e: &Export) -> Result<Vec<f64>, Error> {
    let bad = |what: &str| Error(format!("roll: the correction {what}: not the 15 notes × 61 bands a roll reads"));
    let nums = |ptr: &str| e.json.pointer(ptr).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_f64()).collect::<Vec<f64>>());
    let notes = nums("/correction/notes").ok_or_else(|| bad("has no notes"))?;
    if notes.len() != NOTES.len() || notes.iter().zip(NOTES).any(|(a, b)| *a != b as f64) {
        return Err(bad("notes differ"));
    }
    let g = nums("/correction/grid_hz").ok_or_else(|| bad("has no bands"))?;
    if g.len() != BANDS || g.iter().zip(grid()).any(|(a, b)| (a - b).abs() > 1e-6 * b) {
        return Err(bad("bands differ"));
    }
    let c = e.f64s("/correction/C_db").map_err(|_| bad("has no values"))?;
    if c.len() != NOTES.len() * BANDS {
        return Err(bad("values differ in number"));
    }
    Ok(c)
}

/// The body and the responses' levels before any mean is removed (one measurement for both).
fn describe_raw(e: &Export) -> Result<(Body, [f64; BANDS]), Error> {
    let mut corr = correction(e)?;
    let (mut level, rate) = stats(e)?;
    let raw = level;
    let g = grid();
    let up: Vec<f64> = (0..BANDS).filter(|&k| g[k] >= 80.0).map(|k| level[k]).collect();
    let m = up.iter().sum::<f64>() / up.len() as f64;
    level.iter_mut().for_each(|v| *v -= m);
    let cm = corr.iter().sum::<f64>() / corr.len() as f64;
    corr.iter_mut().for_each(|v| *v -= cm);
    Ok((Body { level, rate, corr }, raw))
}

impl Body {
    /// One vector, the parts weighed alike: rates × 0.05 s, the correction × √(61/915).
    pub fn weighted(&self) -> Vec<f64> {
        let w = corr_w();
        self.level.iter().copied().chain(self.rate.iter().map(|r| r * RATE_W)).chain(self.corr.iter().map(|c| c * w)).collect()
    }

    pub fn unweighted(v: &[f64]) -> Body {
        let w = corr_w();
        Body {
            level: std::array::from_fn(|k| v[k]),
            rate: std::array::from_fn(|k| v[BANDS + k] / RATE_W),
            corr: v[2 * BANDS..].iter().map(|c| c / w).collect(),
        }
    }
}

/// `target` applied to the export: up to four passes (the bands overlap), the best one kept (a
/// pass that makes the error worse ends it); in each, each response's bands scaled
/// by the level change and multiplied by exp(−Δrate t) (Δlevel within ±40 dB, Δrate within
/// ±300 dB/s), merged, set back to its energy; the correction becomes the target's (the export's
/// own mean kept); the board's radiation moves with every pass, each mode by its band's change of
/// energy in the responses (the modes' damping stays: it sets how the strings couple).
pub fn apply(e: &mut Export, target: &Body) -> Result<(), Error> {
    let (mut own, mut raw) = describe_raw(e)?;
    let mut best = (error(&own, target), e.clone());
    for _ in 0..4 {
        (own, raw) = apply_once(e, target, &own, &raw)?;
        let err = error(&own, target);
        if err >= best.0 {
            break;
        }
        best = (err, e.clone());
    }
    *e = best.1;
    Ok(())
}

/// The steering judges the levels from here up: below it a ringing board's band holds less than one
/// mode (the pianos' boards at low_eta 4 %, design log step 184), whose level the band filters set
/// only roughly (moved fully from Grand A's body to Grand D's, 3–8 dB off at 80–230 Hz)
pub const STEER_FROM_HZ: f64 = 250.0;

/// How far a body is from the target: the levels' and the rates' rms from STEER_FROM_HZ up (the
/// levels each side's mean removed; the rates weighed as in the basis, × 0.05 s).
pub fn error(b: &Body, target: &Body) -> f64 {
    let g = grid();
    let up: Vec<f64> = (0..BANDS).filter(|&k| g[k] >= STEER_FROM_HZ).map(|k| b.level[k] - target.level[k]).collect();
    let m = up.iter().sum::<f64>() / up.len() as f64;
    let level = (up.iter().map(|d| (d - m).powi(2)).sum::<f64>() / up.len() as f64).sqrt();
    let hi: Vec<f64> = (0..BANDS).filter(|&k| g[k] >= STEER_FROM_HZ).map(|k| b.rate[k] - target.rate[k]).collect();
    let rate = (hi.iter().map(|d| d * d).sum::<f64>() / hi.len() as f64).sqrt();
    level + RATE_W * rate
}

/// One pass from `own` (the export's body) and `before` (its responses' levels, no mean removed);
/// the body and the levels it ends at.
fn apply_once(e: &mut Export, target: &Body, own: &Body, before: &[f64; BANDS]) -> Result<(Body, [f64; BANDS]), Error> {
    let sr = rate_of(e);
    let p = bands::pad(sr);
    let step = 0.6;
    let dl: Vec<f64> = (0..BANDS).map(|k| (step * (target.level[k] - own.level[k])).clamp(-40.0, 40.0)).collect();
    // below 100 Hz the rates are measured but not applied: there the bands' own filters set them
    // (every reference rings alike, 90–120 dB/s), and following them diverged
    let g = grid();
    let dr: Vec<f64> = (0..BANDS).map(|k| if g[k] < 100.0 { 0.0 } else { (target.rate[k] - own.rate[k]).clamp(-300.0, 300.0) }).collect();
    let rs = e.responses()?;
    let new: Vec<Vec<f64>> = std::thread::scope(|s| {
        let h: Vec<_> = rs
            .iter()
            .map(|r| {
                let (dl, dr) = (&dl, &dr);
                s.spawn(move || {
                    let mut b = bands::split(&r.ir, sr);
                    // the exponential held at the response's ends: in the padding beyond it, a slower
                    // decay would raise the bands' ringing tails by up to 10^15, which the merge's
                    // second filtering brings back into the response
                    let end = r.ir.len() as f64 / sr;
                    for (k, row) in b.iter_mut().enumerate() {
                        // the decay changes the band's shape over time, not its energy: the level is
                        // the level gain's alone (else each pass's level step fought the last's decay)
                        let e0: f64 = row.iter().map(|v| v * v).sum();
                        for (i, v) in row.iter_mut().enumerate() {
                            let t = ((i as f64 - p as f64) / sr).clamp(0.0, end);
                            *v *= 10f64.powf(-dr[k] * t / 20.0);
                        }
                        let e1: f64 = row.iter().map(|v| v * v).sum();
                        let g = 10f64.powf(dl[k] / 20.0) * (e0 / e1.max(1e-300)).sqrt();
                        row.iter_mut().for_each(|v| *v *= g);
                    }
                    let y = bands::merge(&b, sr);
                    let (e0, e1) = (r.ir.iter().map(|v| v * v).sum::<f64>(), y.iter().map(|v| v * v).sum::<f64>());
                    let k = (e0 / e1.max(1e-300)).sqrt();
                    y.into_iter().map(|v| v * k).collect()
                })
            })
            .collect();
        h.into_iter().map(|h| h.join().expect("a response")).collect()
    });
    for (r, y) in rs.iter().zip(&new) {
        e.set_response(r.pol, r.channel, y)?;
    }
    let corr = e.f64s("/correction/C_db")?;
    let cm = corr.iter().sum::<f64>() / corr.len() as f64;
    e.set_f64s("/correction/C_db", &target.corr.iter().map(|c| c + cm).collect::<Vec<_>>())?;
    let (body, after) = describe_raw(e)?;
    let change: Vec<f64> = (0..BANDS).map(|k| after[k] - before[k]).collect();
    scale_board(e, &change)?;
    Ok((body, after))
}

/// The board's radiation (`r`, `r_h`: both microphones) times 10^(Δ/20), Δ per band interpolated in
/// log frequency at each mode.
pub fn scale_board(e: &mut Export, change: &[f64]) -> Result<(), Error> {
    let lg: Vec<f64> = grid().iter().map(|f| f.log2()).collect();
    let f = e.f64s("/board/f")?;
    let gains: Vec<f64> = f.iter().map(|&fm| 10f64.powf(interp(fm.max(1.0).log2(), &lg, change) / 20.0)).collect();
    for key in ["/board/r", "/board/r_h"] {
        let r = e.f64s(key)?;
        let n = gains.len();
        let scaled: Vec<f64> = r.iter().enumerate().map(|(i, v)| v * gains[i % n]).collect();
        e.set_f64s(key, &scaled)?;
    }
    Ok(())
}

/// Linear interpolation, the ends held.
pub fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[xs.len() - 1] {
        return ys[ys.len() - 1];
    }
    let i = xs.partition_point(|&v| v <= x) - 1;
    ys[i] + (ys[i + 1] - ys[i]) * (x - xs[i]) / (xs[i + 1] - xs[i])
}

/// Per polarization, both microphones' responses given the same color (the two's power mean per
/// band; each keeps its own fine structure, so the two stay as uncorrelated as they were), four
/// passes; the pair's energy kept. The polarizations and the responses in parallel.
pub fn widen(e: &mut Export) -> Result<(), Error> {
    let sr = rate_of(e);
    let rs = e.responses()?;
    let pairs: Vec<Vec<&super::files::Response>> = (0..2).map(|pol| rs.iter().filter(|r| r.pol == pol).collect()).collect();
    let each = |irs: &[Vec<f64>], f: &(dyn Fn(usize, &[f64]) -> Vec<f64> + Sync)| -> Vec<Vec<f64>> {
        std::thread::scope(|s| {
            let h: Vec<_> = irs.iter().enumerate().map(|(i, x)| s.spawn(move || f(i, x))).collect();
            h.into_iter().map(|h| h.join().expect("a response")).collect()
        })
    };
    let widened: Vec<Vec<Vec<f64>>> = std::thread::scope(|s| {
        let h: Vec<_> = pairs
            .iter()
            .map(|rs| {
                s.spawn(move || {
                    let e0: f64 = rs.iter().map(|r| r.ir.iter().map(|v| v * v).sum::<f64>()).sum();
                    let mut irs: Vec<Vec<f64>> = rs.iter().map(|r| r.ir.clone()).collect();
                    for _ in 0..4 {
                        let lv = each(&irs, &|_, x| bands::band_stats(x, sr).0);
                        let target: Vec<f64> = (0..BANDS).map(|k| 10.0 * (lv.iter().map(|l| 10f64.powf(l[k] / 10.0)).sum::<f64>() / lv.len() as f64).log10()).collect();
                        irs = each(&irs, &|i, x| {
                            let mut b = bands::split(x, sr);
                            for (k, row) in b.iter_mut().enumerate() {
                                let g = 10f64.powf((target[k] - lv[i][k]).clamp(-20.0, 20.0) / 20.0);
                                row.iter_mut().for_each(|v| *v *= g);
                            }
                            bands::merge(&b, sr)
                        });
                    }
                    let e1: f64 = irs.iter().map(|x| x.iter().map(|v| v * v).sum::<f64>()).sum();
                    let k = (e0 / e1.max(1e-300)).sqrt();
                    irs.into_iter().map(|x| x.into_iter().map(|v| v * k).collect()).collect()
                })
            })
            .collect();
        h.into_iter().map(|h| h.join().expect("a polarization")).collect()
    });
    for (rs, irs) in pairs.iter().zip(&widened) {
        for (r, x) in rs.iter().zip(irs) {
            e.set_response(r.pol, r.channel, x)?;
        }
    }
    Ok(())
}

/// Each response's fundamentals kept out of its notches (`proto/tests/test_radiation_notches.py`
/// asks the exports for −15 dB at most): a mixed fine structure put some key's fundamental below
/// −15 dB for most donor sets (117 of 140, 2026-10-17), the body's decay and the widening dig new
/// ones, and mixes weighed against it moved the bands' color and the bass's phase. Each response's
/// energy kept; done after the widening (the last change to the responses).
pub fn lift_notches(e: &mut Export) -> Result<(), Error> {
    let sr = rate_of(e);
    let f: Vec<f64> = super::measure::fundamentals(e).iter().map(|k| k.1).collect();
    let rs = e.responses()?;
    let lifted: Vec<Vec<f64>> = std::thread::scope(|s| {
        let h: Vec<_> = rs
            .iter()
            .map(|r| {
                let f = &f;
                s.spawn(move || {
                    let y = lift(r.ir.clone(), f, sr);
                    let k = (r.ir.iter().map(|v| v * v).sum::<f64>() / y.iter().map(|v| v * v).sum::<f64>().max(1e-300)).sqrt();
                    y.into_iter().map(|v| v * k).collect()
                })
            })
            .collect();
        h.into_iter().map(|h| h.join().expect("a response")).collect()
    });
    for (r, y) in rs.iter().zip(&lifted) {
        e.set_response(r.pol, r.channel, y)?;
    }
    Ok(())
}

/// A fundamental from 120 Hz up (below, the bass is the frame's) deeper than −12 dB in a notch of
/// `x` lifted to −12 dB by a narrow peak (Q 100) at it, three passes.
fn lift(mut x: Vec<f64>, fundamentals: &[f64], sr: f64) -> Vec<f64> {
    let f: Vec<f64> = fundamentals.iter().copied().filter(|&f| f >= 120.0 && f < 0.45 * sr).collect();
    for _ in 0..3 {
        let depth = bands::notch_depths(&x, sr, &f);
        let deep: Vec<(f64, f64)> = f.iter().zip(&depth).filter(|(_, d)| **d < -12.0).map(|(f, d)| (*f, -12.0 - d)).collect();
        if deep.is_empty() {
            break;
        }
        for (f0, gain_db) in deep {
            // the peaking biquad of the Audio EQ Cookbook
            let a = 10f64.powf(gain_db / 40.0);
            let w0 = 2.0 * std::f64::consts::PI * f0 / sr;
            let alpha = w0.sin() / (2.0 * 100.0);
            let (b0, b1, b2) = (1.0 + alpha * a, -2.0 * w0.cos(), 1.0 - alpha * a);
            let (a0, a1, a2) = (1.0 + alpha / a, -2.0 * w0.cos(), 1.0 - alpha / a);
            let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
            for v in x.iter_mut() {
                let y = (b0 * *v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0;
                (x2, x1, y2, y1) = (x1, *v, y1, y);
                *v = y;
            }
        }
    }
    x
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn scale_board_keeps_the_mobility() {
        // a roll edits its donor's export in place: the board's mobility travels with it
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
        let Ok(mut e) = Export::read(Path::new(dir)) else { return };
        let mob = serde_json::json!({"anchors": [24.0, 36.0], "factors": [1.0, 4.0], "fc_hz": 500.0});
        e.json["board"]["mobility"] = mob.clone();
        scale_board(&mut e, &vec![3.0; BANDS]).unwrap();
        assert_eq!(e.json["board"]["mobility"], mob);
    }
}
