//! The virtual player's curves for one note (`auto_player.Shape`): functions of the time since its key went down.
//! Everything in them was known the moment the key went down.

use super::tables::Tables;
use crate::noise::Rng;

/// One note's random numbers, in their final units.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShapeDraws {
    /// on the air's level, normal with 1.5
    pub level: f64,
    /// the vibrato's start, a factor 0.8 to 1.3
    pub on: f64,
    /// its depth, a factor 0.8 to 1.1
    pub depth: f64,
    /// its rate's start, 34 to 44
    pub rate: f64,
    /// the intonation, cents, normal with 6
    pub off: f64,
    /// a scoop when under 0.15, 0 to 1
    pub scoop: f64,
}

/// Where a player's random numbers come from.
pub trait DrawSource: Send {
    /// The draws of the player's `i`-th note.
    fn draw(&mut self, i: u32) -> ShapeDraws;
}

/// The engine's own: from the player's seed and the note's number.
pub struct SeededDraws {
    pub seed: u64,
}

impl DrawSource for SeededDraws {
    fn draw(&mut self, i: u32) -> ShapeDraws {
        let mut r = Rng::new(self.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(i).wrapping_mul(0xD1B5_4A32_D192_ED03));
        ShapeDraws {
            level: 1.5 * r.gauss(),
            on: 0.8 + 0.5 * r.uniform01(),
            depth: 0.8 + 0.3 * r.uniform01(),
            rate: 34.0 + 10.0 * r.uniform01(),
            off: 6.0 * r.gauss(),
            scoop: r.uniform01(),
        }
    }
}

/// Air for a key: how hard it was struck, and higher notes a little more (`auto_player.level_of`).
pub fn level_of(t: &Tables, v: f64, m: u8) -> f64 {
    let dm = (f64::from(m) - t.center).clamp(-12.0, 12.0);
    match &t.vel_air {
        // the whole of the dynamics from the keys; the higher notes' more goes to nothing at the top
        Some([x, y]) => {
            let b = crate::noise::interp(v, x, y);
            b + dm * (1.0 - b / 127.0)
        }
        None => 45.0 + 75.0 * v / 127.0 + dm,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Shape {
    pub level: f64,
    v: f64,
    slur: bool,
    from_last: Option<f64>,
    on: f64,
    depth: f64,
    rate0: f64,
    off: f64,
    scoops: bool,
}

impl Shape {
    /// `variation` scales how much this note differs from the last (1: as the player was fitted).
    #[allow(clippy::too_many_arguments)]
    pub fn new(t: &Tables, d: &ShapeDraws, m: u8, v: f64, slur: bool, fresh: bool, level: Option<f64>, last: f64, variation: f64) -> Shape {
        let keep = if fresh { 0.0 } else if slur { 0.6 } else { 0.3 }; // legato keeps more of the last note's level
        let target = level_of(t, v, m);
        let mut lv = level.map_or(target, |l| keep * l + (1.0 - keep) * target);
        lv += d.level * variation;
        let spread = |x: f64, mid: f64| mid + (x - mid) * variation;
        Shape {
            level: lv,
            v,
            slur,
            from_last: if slur { Some(last - lv) } else { None },
            on: 0.3 * spread(d.on, 1.05),
            depth: 70.0 * spread(d.depth, 0.95),
            rate0: spread(d.rate, 39.0),
            off: d.off.clamp(-12.0, 12.0) * variation,
            scoops: !slur && d.scoop < 0.15 * variation,
        }
    }

    pub fn air(&self, t: f64) -> f64 {
        let mut b = self.level + 3.0 * (t / 0.4).min(1.0) - (3.0 * (t - 1.2)).min(10.0).max(0.0);
        if let Some(f) = self.from_last {
            b += f * (-t / 0.05).exp();
        } else if !self.slur {
            b += 5.0 * (self.v / 100.0) * (-t / 0.08).exp();
        }
        b.min(127.0).max(1.0)
    }

    pub fn vibrato(&self, t: f64) -> f64 {
        self.depth * ((t - self.on) / 0.8).min(1.0).max(0.0)
    }

    pub fn vibrato_rate(&self, t: f64) -> f64 {
        self.rate0 + 5.0 * (t / 2.0).min(1.0)
    }

    pub fn bend_cents(&self, t: f64) -> f64 {
        self.off + if self.scoops { -60.0 * (-t / 0.035).exp() } else { 0.0 }
    }
}
