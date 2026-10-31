//! A roll's two calibrations: each register's loudness brought to a target through the per-note
//! correction, and the board's radiation moved until Player matches Recording as the references do
//! (away from the recording's position the responses are made from the board's modes).
use super::body::interp;
use super::files::Export;
use super::measure::{self, NOTES, Notes};
use crate::instrument::Error;
use crate::mic::MicPosition;

/// Each correction row (its 15 notes are `NOTES`) shifted by a constant until the note's level over
/// its first 0.5 s at velocity 80 is `target`'s (at most 4 passes, done within 0.3 dB); the largest
/// error left.
pub fn match_registers(e: &mut Export, target: &[f64]) -> Result<f64, Error> {
    let mut worst = f64::INFINITY;
    for pass in 0..5 {
        let level = measure::render_heads(&e.instrument()?, 80.0, MicPosition::Recording)?.level;
        let err: Vec<f64> = target.iter().zip(&level).map(|(t, l)| t - l).collect();
        worst = err.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if worst < 0.3 || pass == 4 {
            break;
        }
        let mut c = e.f64s("/correction/C_db")?;
        let cols = c.len() / NOTES.len();
        for (row, d) in err.iter().enumerate() {
            c[row * cols..(row + 1) * cols].iter_mut().for_each(|v| *v += d);
        }
        e.set_f64s("/correction/C_db", &c)?;
    }
    Ok(worst)
}

/// What `calibrate_positions` reached.
pub struct Calibrated {
    /// Player − Recording per octave as left
    pub d: [f64; 9],
    /// the corrections applied
    pub passes: usize,
    /// the last Player render (the export's as left)
    pub player: Notes,
}

/// The board's radiation (`r`, `r_h`) moved per octave by Player − Recording's distance from `goal`
/// (the references' median), interpolated in log frequency between the octaves' centers, until
/// that distance's shape is within 1.0 dB or a pass gains less than 0.15 dB (at most 6 passes, the
/// best kept). Only the shape: the engine sets the radiation's level to the recording's, so its
/// mean (and the top octave, above the board's modes) does not move. `rec`: the export's notes
/// at Recording (the radiation does not touch them).
pub fn calibrate_positions(e: &mut Export, goal: &[f64; 9], rec: &Notes) -> Result<Calibrated, Error> {
    let centers: Vec<f64> = (0..9).map(|b| (62.5 * 2f64.powi(b) * 2f64.sqrt()).log2()).collect();
    let render = |e: &Export| -> Result<(Notes, [f64; 9]), Error> {
        let p = measure::render_heads(&e.instrument()?, 80.0, MicPosition::Player)?;
        let d = measure::difference(&p, rec);
        Ok((p, d))
    };
    // the distance's shape (its mean removed) and its largest deviation
    let shape = |d: &[f64; 9]| -> (Vec<f64>, f64) {
        let off: Vec<f64> = d.iter().zip(goal).map(|(x, g)| x - g).collect();
        let m = off.iter().sum::<f64>() / off.len() as f64;
        let off: Vec<f64> = off.iter().map(|v| v - m).collect();
        let worst = off.iter().fold(0.0f64, |w, v| w.max(v.abs()));
        (off, worst)
    };
    let (player, d) = render(e)?;
    let mut best = (shape(&d).1, e.clone(), Calibrated { d, passes: 0, player });
    for pass in 1..=6 {
        let (off, worst) = shape(&best.2.d);
        if worst <= 1.0 {
            break;
        }
        let mut next = best.1.clone();
        let f = next.f64s("/board/f")?;
        let gains: Vec<f64> = f.iter().map(|&fm| 10f64.powf(-interp(fm.max(1.0).log2(), &centers, &off) / 20.0)).collect();
        for key in ["/board/r", "/board/r_h"] {
            let r = next.f64s(key)?;
            let n = gains.len();
            next.set_f64s(key, &r.iter().enumerate().map(|(i, v)| v * gains[i % n]).collect::<Vec<_>>())?;
        }
        let (player, d) = render(&next)?;
        let now = shape(&d).1;
        if now < best.0 {
            best = (now, next, Calibrated { d, passes: pass, player });
        }
        // where Player is not the board's radiation alone, a pass moves a tenth of a dB (2026-10-17:
        // six passes took a body change's 2.4 dB to 1.7): not worth its render
        if worst - now < 0.15 {
            break;
        }
    }
    *e = best.1;
    Ok(best.2)
}
