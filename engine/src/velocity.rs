//! The velocity curve: incoming MIDI velocity → the velocity played, through up to `MAX_POINTS`
//! control points. A monotone cubic through them (Fritsch–Carlson with Butland's weights): it never
//! falls and never overshoots a point. Fixed arrays, no allocation outside `format`.

/// the most control points, the ends included
pub const MAX_POINTS: usize = 8;

/// The Dynamics control's default: the level range (dB) from velocity 1 to 127. Salamander's 16
/// recorded layers as its set plays them (its velocity map, amp_veltrack 73) fit 45 dB.
pub const DYNAMICS_DB: f64 = 45.0;

/// The velocity a hammer piano's recorded ff plays at (the export's ff anchor): there it keeps its
/// recorded speed and level at any Dynamics range.
pub const FF_VELOCITY: f64 = 124.0;

/// A hammer piano's level (dB re velocity 127) at `velocity` (1 … 127), its range `range_db`
/// from 1 to 127: linear in √velocity. The recordings' own maps fit this shape: Salamander's
/// 16 layers within 1.2 dB, Headroom's within 1.4 dB.
pub fn dynamics_db(velocity: f64, range_db: f64) -> f64 {
    let r = |v: f64| (v / 127.0).sqrt();
    -range_db * (1.0 - r(velocity.clamp(1.0, 127.0))) / (1.0 - r(1.0))
}

/// The curve: points (in, out) at whole velocities 1–127, in strictly ascending, the ends at in 1
/// and 127, out not falling; the tangents at the points, computed once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityCurve {
    n: usize,
    points: [(u8, u8); MAX_POINTS],
    tangents: [f64; MAX_POINTS],
}

impl VelocityCurve {
    /// velocity in = velocity played
    pub const STRAIGHT: VelocityCurve = VelocityCurve {
        n: 2,
        points: [(1, 1), (127, 127), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
        tangents: [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    };

    /// None unless 2–MAX_POINTS points, in strictly ascending from 1 to 127, every value 1–127,
    /// out not falling
    pub fn from_points(points: &[(u8, u8)]) -> Option<VelocityCurve> {
        let n = points.len();
        if !(2..=MAX_POINTS).contains(&n) || points[0].0 != 1 || points[n - 1].0 != 127 {
            return None;
        }
        if points.iter().any(|p| !(1..=127).contains(&p.0) || !(1..=127).contains(&p.1))
            || points.windows(2).any(|w| w[1].0 <= w[0].0 || w[1].1 < w[0].1)
        {
            return None;
        }
        let mut c = VelocityCurve { n, points: [(0, 0); MAX_POINTS], tangents: [0.0; MAX_POINTS] };
        c.points[..n].copy_from_slice(points);
        let (x, y) = (|k: usize| points[k].0 as f64, |k: usize| points[k].1 as f64);
        let h = |k: usize| x(k + 1) - x(k);
        let d = |k: usize| (y(k + 1) - y(k)) / h(k);
        c.tangents[0] = d(0);
        c.tangents[n - 1] = d(n - 2);
        for k in 1..n - 1 {
            let (d0, d1) = (d(k - 1), d(k));
            c.tangents[k] = if d0 * d1 <= 0.0 {
                0.0
            } else {
                let (w1, w2) = (2.0 * h(k) + h(k - 1), h(k) + 2.0 * h(k - 1));
                (w1 + w2) / (w1 / d0 + w2 / d1)
            };
        }
        Some(c)
    }

    /// `in:out` pairs between commas or spaces; empty or unreadable: STRAIGHT. No allocation.
    pub fn parse(text: &str) -> VelocityCurve {
        Self::try_parse(text).unwrap_or(Self::STRAIGHT)
    }

    /// as `parse`, None for unreadable text (the empty text is STRAIGHT)
    pub fn try_parse(text: &str) -> Option<VelocityCurve> {
        let mut pts = [(0u8, 0u8); MAX_POINTS];
        let mut n = 0;
        for tok in text.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()) {
            let (a, b) = tok.split_once(':')?;
            let (a, b) = (a.parse::<u8>().ok()?, b.parse::<u8>().ok()?);
            if n == MAX_POINTS {
                return None;
            }
            pts[n] = (a, b);
            n += 1;
        }
        if n == 0 { Some(Self::STRAIGHT) } else { Self::from_points(&pts[..n]) }
    }

    pub fn points(&self) -> &[(u8, u8)] {
        &self.points[..self.n]
    }

    /// `"1:1,64:80,127:127"` (allocates: not for the audio thread)
    pub fn format(&self) -> String {
        self.points().iter().map(|(a, b)| format!("{a}:{b}")).collect::<Vec<_>>().join(",")
    }

    pub fn is_straight(&self) -> bool {
        *self == Self::STRAIGHT
    }

    /// The velocity played for a MIDI velocity, in [1, 127]; 0 (a key-up) stays 0.
    pub fn apply(&self, velocity: u8) -> f64 {
        if velocity == 0 { 0.0 } else { self.at(velocity as f64) }
    }

    /// The curve at any velocity in [1, 127] (clamped there), for drawing it.
    pub fn at(&self, velocity: f64) -> f64 {
        let x = velocity.clamp(1.0, 127.0);
        let pts = self.points();
        let k = (0..self.n - 1).find(|&k| x <= pts[k + 1].0 as f64).unwrap_or(self.n - 2);
        let (x0, x1) = (pts[k].0 as f64, pts[k + 1].0 as f64);
        let (y0, y1) = (pts[k].1 as f64, pts[k + 1].1 as f64);
        let h = x1 - x0;
        let t = (x - x0) / h;
        let (t2, t3) = (t * t, t * t * t);
        let y = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * h * self.tangents[k]
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * h * self.tangents[k + 1];
        y.clamp(1.0, 127.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_straight_curve_is_the_identity() {
        for v in 1..=127u8 {
            assert!((VelocityCurve::STRAIGHT.apply(v) - v as f64).abs() < 1e-9);
        }
    }

    #[test]
    fn the_points_are_followed_and_it_rises_between_them() {
        let c = VelocityCurve::from_points(&[(1, 1), (40, 20), (90, 110), (127, 127)]).unwrap();
        for (i, o) in [(1u8, 1.0), (40, 20.0), (90, 110.0), (127, 127.0)] {
            assert!((c.apply(i) - o).abs() < 1e-9);
        }
        let ys: Vec<f64> = (1..=127u8).map(|v| c.apply(v)).collect();
        assert!(ys.windows(2).all(|w| w[1] >= w[0])); // never falls
        assert!((40..=90).all(|v| (20.0..=110.0).contains(&c.apply(v)))); // no overshoot
    }

    #[test]
    fn the_ends_keep_a_note_on_a_note_on() {
        let c = VelocityCurve::from_points(&[(1, 30), (127, 100)]).unwrap();
        assert_eq!(c.apply(1), 30.0);
        assert_eq!(c.apply(127), 100.0);
        assert_eq!(c.apply(0), 0.0);
        let low = VelocityCurve::from_points(&[(1, 1), (127, 1)]).unwrap();
        assert!((1..=127u8).all(|v| low.apply(v) >= 1.0));
    }

    #[test]
    fn the_text_round_trips_and_bad_text_is_straight() {
        let c = VelocityCurve::parse("1:1 64:80,127:127");
        assert_eq!(c.points(), &[(1, 1), (64, 80), (127, 127)]);
        assert_eq!(c.format(), "1:1,64:80,127:127");
        assert_eq!(VelocityCurve::parse(&c.format()), c);
        for bad in ["", "x", "64:80", "1:1,127:200", "1:1,64:80,40:90,127:127", "1:1,64:0,127:127",
                    "1:1,2:2,3:3,4:4,5:5,6:6,7:7,8:8,127:127", "1:50,64:20,127:127"] {
            assert_eq!(VelocityCurve::parse(bad), VelocityCurve::STRAIGHT, "{bad}");
        }
    }
}
