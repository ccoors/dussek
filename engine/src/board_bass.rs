//! The board's bass (design 2026-10-23-board-knobs, step 148): a gain on a hammer piano's strings
//! below 180 Hz, on every note, in their per-note correction at the compile, as Grand C's board
//! trims of step 147. Not on the board's responses: they carry the strike's thud too, which the
//! gain lifted into a pop under the notes above its range.
use crate::roll::bands::{BANDS, grid};

/// the knob's range (dB)
pub const RANGE_DB: (f64, f64) = (-12.0, 24.0);

/// Non-finite becomes 0 (the instrument's own), the rest is clamped to `RANGE_DB`.
pub fn sanitize(db: f64) -> f64 {
    if db.is_finite() { db.clamp(RANGE_DB.0, RANGE_DB.1) } else { 0.0 }
}

/// The knob's share at a frequency: Grand C's step 147 trim summed, normalized, its top widened by
/// ear half an octave (0.8 at 31.5 Hz and below, 1 at 45 Hz, 0.8 at 63 Hz, 0.5 at 90 Hz, 0.25 at
/// 125 Hz, 0 from 180 Hz), linear in log frequency between.
pub fn shape(f_hz: f64) -> f64 {
    const PTS: [(f64, f64); 6] = [(31.5, 0.8), (45.0, 1.0), (63.0, 0.8), (90.0, 0.5), (125.0, 0.25), (180.0, 0.0)];
    let l = f_hz.max(1.0).log2();
    if l <= PTS[0].0.log2() {
        return PTS[0].1;
    }
    for w in PTS.windows(2) {
        let (a, b) = (w[0].0.log2(), w[1].0.log2());
        if l <= b {
            return w[0].1 + (l - a) / (b - a) * (w[1].1 - w[0].1);
        }
    }
    0.0
}

/// `own` (the instrument's per-note correction; None: none) with `bass_db × shape` added at every
/// note and grid point; None: no bass.
pub fn corrected(own: Option<&crate::physics::Correction>, bass_db: f64) -> Option<crate::physics::Correction> {
    if bass_db == 0.0 {
        return None;
    }
    let mut out = own.cloned().unwrap_or_else(|| crate::physics::Correction {
        notes: vec![21.0, 108.0],
        grid_log2: grid().iter().map(|f| f.log2()).collect(),
        c_db: vec![0.0; 2 * BANDS],
    });
    let g = out.grid_log2.len();
    for row in out.c_db.chunks_mut(g) {
        for (c, &l) in row.iter_mut().zip(&out.grid_log2) {
            *c += bass_db * shape(l.exp2());
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bass_is_the_shape_on_every_note() {
        let out = corrected(None, 12.0).expect("a correction");
        let g = out.grid_log2.len();
        for (i, note) in out.notes.iter().enumerate() {
            for (j, &lf) in out.grid_log2.iter().enumerate() {
                let want = 12.0 * shape(lf.exp2());
                assert!((out.c_db[i * g + j] - want).abs() < 1e-9, "note {note}, {:.0} Hz: {:.2} dB, the shape gives {want:.2}", lf.exp2(), out.c_db[i * g + j]);
            }
        }
        assert!(corrected(None, 0.0).is_none());
    }

    #[test]
    fn bass_adds_to_the_own_correction() {
        let own = crate::physics::Correction { notes: vec![40.0, 80.0], grid_log2: vec![45f64.log2(), 1000f64.log2()], c_db: vec![1.0, 2.0, 3.0, 4.0] };
        let out = corrected(Some(&own), 10.0).expect("a correction");
        assert_eq!(out.c_db, vec![11.0, 2.0, 13.0, 4.0]);
    }

    #[test]
    fn the_knob_is_clamped() {
        assert_eq!((sanitize(-40.0), sanitize(99.0), sanitize(f64::NAN), sanitize(6.0)), (-12.0, 24.0, 0.0, 6.0));
    }

    #[test]
    fn the_shape_has_its_corners() {
        for (f, v) in [(20.0, 0.8), (31.5, 0.8), (45.0, 1.0), (63.0, 0.8), (90.0, 0.5), (125.0, 0.25), (180.0, 0.0), (500.0, 0.0)] {
            assert!((shape(f) - v).abs() < 1e-12, "{f} Hz: {}", shape(f));
        }
    }
}
