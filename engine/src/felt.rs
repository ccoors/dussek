//! The felt as the player bends it (design 2026-10-22-felt-controls): four curves over the keyboard
//! of bends on the shipped felt, applied to each key's felt at the build. The stiffness is a ratio
//! (dB) of the force at 0.5 mm; the exponent, the core's force (N at 0.5 mm into the core) and its
//! depth (mm) are offsets. Between points a bend is linear in the key, held beyond the outer ones;
//! no points: no bend. Hammer hardness (dB) joins the stiffness and scales the core.

/// the most points of a curve
pub const MAX_POINTS: usize = 8;

/// The four quantities, in the order of `FeltEdit::curves` and the text's groups (s p c d).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantity {
    Stiffness,
    Exponent,
    CoreForce,
    CoreDepth,
}

impl Quantity {
    pub const ALL: [Quantity; 4] = [Quantity::Stiffness, Quantity::Exponent, Quantity::CoreForce, Quantity::CoreDepth];
    const LETTERS: [char; 4] = ['s', 'p', 'c', 'd'];
}

/// One quantity's bends: `n` points at keys strictly rising (21–108).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeltCurve {
    pub n: usize,
    pub keys: [u8; MAX_POINTS],
    pub bend: [f64; MAX_POINTS],
}

/// The player's felt: a curve per quantity and Hammer hardness (dB; the parameter's, not in the text).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeltEdit {
    pub curves: [FeltCurve; 4],
    pub hardness_db: f64,
}

/// One key's felt: whether a hammer strikes it, the felt's force at 0.5 mm (N) and exponent, the
/// core's force at 0.5 mm into it (N) and its depth (mm; 1.0 where there is no core).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KeyFelt {
    pub hammer: bool,
    pub f05: f64,
    pub p: f64,
    pub core05: f64,
    pub core_depth_mm: f64,
    pub core: bool,
}

pub type FeltKeys = [KeyFelt; 88];

/// The shipped depth where an instrument has no core (mm): where the player raises one.
pub const DEPTH_WITHOUT_CORE_MM: f64 = 1.0;

/// The ranges an edited felt stays in (the engine's stable strikes, `engine/tests/felt_edit.rs`).
pub struct Limits {
    /// the stiffness's bend with the hardness (dB re shipped)
    pub stiffness_db: (f64, f64),
    pub p: (f64, f64),
    pub core05: (f64, f64),
    pub core_depth_mm: (f64, f64),
}

pub const LIMITS: Limits = Limits { stiffness_db: (-36.0, 24.0), p: (1.0, 5.0), core05: (0.0, 100.0), core_depth_mm: (0.0, 4.0) };

/// The bend of `c` at `key`.
pub fn bend_at(c: &FeltCurve, key: u8) -> f64 {
    let (k, b) = (&c.keys[..c.n], &c.bend[..c.n]);
    match k.iter().position(|&x| x >= key) {
        None if c.n == 0 => 0.0,
        None => b[c.n - 1],
        Some(0) => b[0],
        Some(j) => {
            let t = (key - k[j - 1]) as f64 / (k[j] - k[j - 1]) as f64;
            b[j - 1] + t * (b[j] - b[j - 1])
        }
    }
}

/// The felt of every key with `edit` bent onto `shipped` (keys without a hammer as they are).
pub fn apply(shipped: &FeltKeys, edit: &FeltEdit) -> FeltKeys {
    let clamp = |x: f64, (lo, hi): (f64, f64)| x.clamp(lo, hi);
    let hard = 10f64.powf(edit.hardness_db / 20.0);
    let mut out = *shipped;
    for (i, (o, s)) in out.iter_mut().zip(shipped).enumerate().filter(|(_, (_, s))| s.hammer) {
        let key = 21 + i as u8;
        let bend = |q: Quantity| bend_at(&edit.curves[q as usize], key);
        o.p = clamp(s.p + bend(Quantity::Exponent), LIMITS.p);
        o.f05 = s.f05 * 10f64.powf(clamp(bend(Quantity::Stiffness) + edit.hardness_db, LIMITS.stiffness_db) / 20.0);
        o.core05 = clamp(s.core05 + bend(Quantity::CoreForce), LIMITS.core05) * hard;
        o.core = o.core05 > 0.0;
        o.core_depth_mm = clamp(s.core_depth_mm + bend(Quantity::CoreDepth), LIMITS.core_depth_mm);
    }
    out
}

impl FeltEdit {
    /// No points and the hardness at 0 dB: the shipped felt.
    pub fn is_shipped(&self) -> bool {
        self.hardness_db == 0.0 && self.curves.iter().all(|c| c.n == 0)
    }

    /// The text's edit ("s:21=0,72=-3;p:36=0.2;c:;d:"; a missing group: no points); unreadable
    /// text (a key outside 21–108 or not rising, more than `MAX_POINTS`, a bend not finite): none.
    pub fn parse(text: &str) -> FeltEdit {
        Self::try_parse(text).unwrap_or_default()
    }

    /// As `parse`, None for unreadable text.
    pub fn try_parse(text: &str) -> Option<FeltEdit> {
        let read = || -> Option<FeltEdit> {
            let mut e = FeltEdit::default();
            for group in text.split(';').map(str::trim).filter(|g| !g.is_empty()) {
                let (letter, points) = group.split_once(':')?;
                let letter = letter.trim();
                let q = Quantity::LETTERS.iter().position(|&l| letter.len() == 1 && letter.starts_with(l))?;
                let c = &mut e.curves[q];
                for point in points.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                    let (k, b) = point.split_once('=')?;
                    let (k, b): (u8, f64) = (k.trim().parse().ok()?, b.trim().parse().ok()?);
                    let rising = c.n == 0 || k > c.keys[c.n - 1];
                    if c.n == MAX_POINTS || !(21..=108).contains(&k) || !rising || !b.is_finite() {
                        return None;
                    }
                    (c.keys[c.n], c.bend[c.n]) = (k, b);
                    c.n += 1;
                }
            }
            Some(e)
        };
        read()
    }

    /// The text `parse` reads (the hardness not in it).
    pub fn format(&self) -> String {
        let group = |q: usize| {
            let c = &self.curves[q];
            let pts: Vec<String> = (0..c.n).map(|i| format!("{}={}", c.keys[i], c.bend[i] as f32)).collect();
            format!("{}:{}", Quantity::LETTERS[q], pts.join(","))
        };
        (0..4).map(group).collect::<Vec<_>>().join(";")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(pts: &[(u8, f64)]) -> FeltCurve {
        let mut c = FeltCurve { n: pts.len(), ..Default::default() };
        for (i, &(k, b)) in pts.iter().enumerate() {
            c.keys[i] = k;
            c.bend[i] = b;
        }
        c
    }

    fn edit(q: Quantity, pts: &[(u8, f64)], hardness_db: f64) -> FeltEdit {
        let mut e = FeltEdit { hardness_db, ..Default::default() };
        e.curves[q as usize] = curve(pts);
        e
    }

    /// every key struck, the same felt; a core when core05 > 0
    fn flat(f05: f64, p: f64, core05: f64) -> FeltKeys {
        let core = core05 > 0.0;
        [KeyFelt { hammer: true, f05, p, core05, core_depth_mm: if core { 0.5 } else { DEPTH_WITHOUT_CORE_MM }, core }; 88]
    }

    #[test]
    fn no_points_bend_nothing() {
        assert_eq!(bend_at(&FeltCurve::default(), 60), 0.0);
        assert!(FeltEdit::default().is_shipped());
        assert_eq!(apply(&flat(2.0, 2.2, 3.0), &FeltEdit::default()), flat(2.0, 2.2, 3.0));
    }

    #[test]
    fn bends_are_linear_between_points_and_held_beyond() {
        let c = curve(&[(36, -6.0), (60, 0.0)]);
        assert_eq!(bend_at(&c, 21), -6.0);
        assert!((bend_at(&c, 48) - -3.0).abs() < 1e-12);
        assert_eq!(bend_at(&c, 108), 0.0);
        assert_eq!(bend_at(&curve(&[(50, 2.5)]), 90), 2.5);
    }

    #[test]
    fn stiffness_scales_the_force_at_half_a_millimeter() {
        let out = apply(&flat(2.0, 2.2, 0.0), &edit(Quantity::Stiffness, &[(21, 6.0)], 0.0));
        assert!((out[0].f05 / 2.0 - 10f64.powf(6.0 / 20.0)).abs() < 1e-12);
        assert_eq!(out[0].p, 2.2);
        assert!(!edit(Quantity::Stiffness, &[(21, 6.0)], 0.0).is_shipped());
    }

    #[test]
    fn hardness_adds_to_the_stiffness_and_scales_the_core() {
        let e = FeltEdit { hardness_db: 20.0 * 2f64.log10(), ..Default::default() };
        assert!(!e.is_shipped());
        let out = apply(&flat(2.0, 2.2, 3.0), &e);
        assert!((out[5].f05 - 4.0).abs() < 1e-9 && (out[5].core05 - 6.0).abs() < 1e-9);
    }

    #[test]
    fn the_exponent_is_an_offset() {
        let out = apply(&flat(2.0, 2.2, 0.0), &edit(Quantity::Exponent, &[(21, 0.3)], 0.0));
        assert!((out[40].p - 2.5).abs() < 1e-12 && out[40].f05 == 2.0);
    }

    #[test]
    fn the_core_is_raised_from_zero_where_there_was_none() {
        let out = apply(&flat(2.0, 2.2, 0.0), &edit(Quantity::CoreForce, &[(21, 5.0)], 0.0));
        assert_eq!((out[0].core05, out[0].core, out[0].core_depth_mm), (5.0, true, 1.0));
        // a core taken down to nothing is none
        let out = apply(&flat(2.0, 2.2, 3.0), &edit(Quantity::CoreForce, &[(21, -5.0)], 0.0));
        assert_eq!((out[0].core05, out[0].core), (0.0, false));
    }

    #[test]
    fn results_are_clamped_to_the_limits() {
        let s = flat(2.0, 2.2, 3.0);
        assert_eq!(apply(&s, &edit(Quantity::Exponent, &[(21, 10.0)], 0.0))[0].p, 5.0);
        assert_eq!(apply(&s, &edit(Quantity::CoreForce, &[(21, 500.0)], 0.0))[0].core05, 100.0);
        assert_eq!(apply(&s, &edit(Quantity::CoreDepth, &[(21, -9.0)], 0.0))[0].core_depth_mm, 0.0);
        let f = apply(&s, &edit(Quantity::Stiffness, &[(21, 40.0)], 0.0))[0].f05;
        assert!((f / 2.0 - 10f64.powf(24.0 / 20.0)).abs() < 1e-9);
    }

    #[test]
    fn bends_follow_a_new_shipped_felt() {
        // the same edit over two shipped felts (a moved morph point): the same ratio and offsets
        let e = edit(Quantity::Stiffness, &[(21, 6.0)], 0.0);
        for f in [1.0, 3.0] {
            assert!((apply(&flat(f, 2.0, 0.0), &e)[10].f05 / f - 10f64.powf(0.3)).abs() < 1e-12);
        }
    }

    #[test]
    fn keys_without_a_hammer_stay_as_they_are() {
        let mut s = flat(2.0, 2.2, 0.0);
        s[3] = KeyFelt::default();
        let out = apply(&s, &edit(Quantity::Stiffness, &[(21, 6.0)], 3.0));
        assert_eq!(out[3], KeyFelt::default());
    }

    #[test]
    fn text_round_trips() {
        let mut e = FeltEdit::default();
        e.curves[0] = curve(&[(21, 0.0), (72, -3.0)]);
        e.curves[1] = curve(&[(36, 0.25)]);
        e.curves[2] = curve(&[(21, 6.0), (39, 6.0), (48, 0.0)]);
        e.curves[3] = curve(&[(60, -0.125)]);
        let back = FeltEdit::parse(&e.format());
        for q in 0..4 {
            assert_eq!(back.curves[q].n, e.curves[q].n);
            for i in 0..e.curves[q].n {
                assert_eq!(back.curves[q].keys[i], e.curves[q].keys[i]);
                assert!((back.curves[q].bend[i] - e.curves[q].bend[i]).abs() < 1e-6);
            }
        }
        assert_eq!(FeltEdit::parse("s:21=1.5").curves[0].n, 1);
    }

    #[test]
    fn unreadable_text_is_shipped() {
        for t in ["", "garbage", "s:21=x", "s:60=1,40=2", "s:20=1", "s:21=1,22=1,23=1,24=1,25=1,26=1,27=1,28=1,29=1", "s:21=inf"] {
            assert!(FeltEdit::parse(t).is_shipped(), "{t}");
        }
    }
}
