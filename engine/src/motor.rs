//! The vibraphone's motor: a disc in the top of every resonator, all on one shaft. A disc
//! across the tube (closed) takes the resonator off the bar's fundamental, upright (open)
//! leaves it on, so the fundamentals of all bars swell and fade together, twice per turn
//! of the shaft (a tremolo, `rate` in pulses per second), and their pitch moves a little
//! with the tube's loading. The bars' upper modes and the strike radiate from the bars
//! themselves and are left alone. The voices render their fundamentals (and the
//! fundamentals' harmonics) into a bank of their own (`Voice::render`'s `acc_t`); the engine
//! multiplies its sum by `gains` per sample and detunes its modes by `detune` per block.
//!
//! No recording with the motor running: the depth, the dip's shape and the pitch are
//! starting values tuned by ear. Switched off, the discs stop open (as a player leaves
//! them); the motor spins up and down in MOTOR_SPIN_S, the depth fades in MOTOR_RAMP_S.

use std::f64::consts::PI;

/// the fundamental's level with the discs across the tubes, re open (depth 1)
pub const MOTOR_DIP_DB: f64 = 12.0;
/// the resonator's share against the disc's opening u (0 closed … 1 open): u^MOTOR_OPEN_EXP;
/// a little opening gives most of it back, so the dips are narrow and the peaks broad
pub const MOTOR_OPEN_EXP: f64 = 0.4;
/// the fundamental's pitch with the discs closed re open (cents, depth 1)
pub const MOTOR_CENTS: f64 = -3.0;
/// time constants: the shaft's speed after a change, the depth switching on or off
pub const MOTOR_SPIN_S: f64 = 0.4;
pub const MOTOR_RAMP_S: f64 = 0.15;

#[derive(Clone)]
pub struct Motor {
    /// the discs' angle (0: across the tube), its speed (rad/s) and the depth now
    theta: f64,
    omega: f64,
    depth: f64,
    dt: f64,
    spin: f64,
    ramp: f64,
}

impl Motor {
    pub fn new(sr: f64) -> Self {
        let dt = 1.0 / sr;
        Motor {
            theta: 0.5 * PI,
            omega: 0.0,
            depth: 0.0,
            dt,
            spin: 1.0 - (-dt / MOTOR_SPIN_S).exp(),
            ramp: 1.0 - (-dt / MOTOR_RAMP_S).exp(),
        }
    }

    /// The discs stopped, open.
    pub fn reset(&mut self) {
        self.theta = 0.5 * PI;
        self.omega = 0.0;
        self.depth = 0.0;
    }

    /// Nothing to modulate: off and faded out (the gains are 1, no detune).
    pub fn idle(&self) -> bool {
        self.depth == 0.0
    }

    /// The fundamental's gain at the discs' opening, depth d.
    fn gain(d: f64, theta: f64) -> f64 {
        let open = 1.0 - theta.cos().abs();
        let floor = 10f64.powf(-MOTOR_DIP_DB / 20.0);
        1.0 - d * (1.0 - floor) * (1.0 - open.powf(MOTOR_OPEN_EXP))
    }

    /// Run gains.len() samples: the motor `on` at `rate` pulses per second, `depth` 0 … 1.
    pub fn run(&mut self, on: bool, rate: f64, depth: f64, gains: &mut [f64]) {
        let target = if on { depth.clamp(0.0, 1.0) } else { 0.0 };
        if self.depth == 0.0 && target == 0.0 {
            // stopped: the discs come to rest open
            self.reset();
            gains.fill(1.0);
            return;
        }
        // two pulses per turn: |cos θ| has period π
        let w = if on { PI * rate.clamp(0.1, 20.0) } else { self.omega };
        for g in gains.iter_mut() {
            self.omega += self.spin * (w - self.omega);
            self.depth += self.ramp * (target - self.depth);
            if target == 0.0 && self.depth < 1e-4 {
                self.depth = 0.0;
            }
            self.theta = (self.theta + self.omega * self.dt) % PI;
            *g = Self::gain(self.depth, self.theta);
        }
    }

    /// The resonator's relative frequency offset now (for the block that follows).
    pub fn detune(&self) -> f64 {
        if self.depth == 0.0 {
            return 0.0;
        }
        let closed = (1.0 - Self::gain(1.0, self.theta)) / (1.0 - 10f64.powf(-MOTOR_DIP_DB / 20.0));
        2f64.powf(MOTOR_CENTS * self.depth * closed / 1200.0) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulses_at_the_rate_and_dips_by_the_depth() {
        let sr = 48000.0;
        let mut m = Motor::new(sr);
        let mut g = vec![0.0; (4.0 * sr) as usize];
        m.run(true, 5.0, 1.0, &mut g);
        // after spin-up and ramp: dips to the floor, peaks at 1, five dips per second
        let tail = &g[(2.0 * sr) as usize..(3.0 * sr) as usize];
        let (lo, hi) = tail.iter().fold((1.0f64, 0.0f64), |(a, b), &x| (a.min(x), b.max(x)));
        assert!((20.0 * lo.log10() + MOTOR_DIP_DB).abs() < 0.5 && hi > 0.999, "{lo} {hi}");
        let floor = 10f64.powf(-MOTOR_DIP_DB / 20.0);
        let dips = tail.windows(3).filter(|w| w[1] < w[0] && w[1] <= w[2] && w[1] < floor + 0.05).count();
        assert_eq!(dips, 5);
        // the dips narrower than a sinusoid's between the same levels (above −3 dB 43 % of the time)
        assert!(tail.iter().filter(|&&x| x > 0.707).count() as f64 > 0.47 * tail.len() as f64);
        // smooth: no step above what the dip's slope gives
        assert!(g.windows(2).all(|w| (w[1] - w[0]).abs() < 0.01));
    }

    #[test]
    fn switched_off_it_fades_out_and_rests_open() {
        let sr = 48000.0;
        let mut m = Motor::new(sr);
        let mut g = vec![0.0; 96000];
        m.run(true, 6.0, 1.0, &mut g);
        assert!(!m.idle());
        m.run(false, 6.0, 1.0, &mut g);
        assert!(g.windows(2).all(|w| (w[1] - w[0]).abs() < 0.01));
        assert!(m.idle() && m.detune() == 0.0);
        m.run(false, 6.0, 1.0, &mut g);
        assert!(g.iter().all(|&x| x == 1.0));
    }
}
