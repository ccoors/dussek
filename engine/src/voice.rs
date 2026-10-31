//! One voice per key: hammer contact (simulated sample by sample, uncoupled resonators),
//! then the coupled free decay as complex one-pole recursions, one per eigenmode of the
//! unison system (partial × string), for the vertical and the horizontal polarization.
//!
//! Mirrors `proto/string_model.py` (`free_modes`, `render`): the free decay of a mode is
//! `Re(w·e^{λ m dt})` with m = 0 at the last contact sample − 1, so the first free
//! output sample has m = 2. A damper change switches λ → λ − σ_d from that sample on.
//! A re-strike adds a new contact (from rest) whose modes are added to the running ones.
//!
//! The modes run in float32 (`kernel`); their exact state is kept in double precision
//! and re-synchronized at every engine block and before every change (strike, damper),
//! so float32 rounding never accumulates over more than one block. Modes below
//! `PRUNE` re the strongest one are left out at a strike, and modes that decay below
//! `CULL` re that reference are dropped.

use realfft::num_complex::Complex64;

use crate::instrument::{MEMBRANE_POLES, Membrane, Note, Polarization};
use crate::physics::felt_force;
use crate::kernel::{self, LANES, Lane};
use crate::noise::{HighPass, Rng};
use crate::telemetry::{KIND_COPY, KIND_HORIZ, MAX_PARTIALS, Partial};

/// the impact's lowpass keeps running this long after the last contact (its ring is under a
/// millisecond at 300 Hz)
const IMPACT_TAIL_S: f64 = 0.02;

/// Each strike slightly different (`Voice::set_variation`, at variation 1): the strike point's
/// spread (m), the unison's strings met this far apart in time (s, each string's spread), each
/// string's share of the felt (log spread). Set against Grand D's two takes per key and dynamic
/// (design log step 130).
const STRIKE_POS_SIGMA_M: f64 = 2.0e-3;
const STRIKE_TIME_SIGMA_S: f64 = 40e-6;
const STRIKE_FELT_SIGMA: f64 = 0.05;
/// the strike point drawn stays within this range (re the length)
const STRIKE_POS_RANGE: (f64, f64) = (0.02, 0.48);
/// strings per note the draws cover (a piano's unison has 3)
const MAX_STRIKE_STRINGS: usize = 8;
/// the most partials a damper lift kicks (`Voice::kick`; n^−8 above −80 dB: three)
const KICK_MAX: usize = 32;

/// The hammer's impact on the board (`Instrument::impact`): a 2nd-order Butterworth lowpass on
/// the felt force (bilinear, prewarped: the prototype's `butter(2, cutoff)`), or none (open).
#[derive(Clone, Copy, Debug, Default)]
pub struct ImpactFilter {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
    open: bool,
}

impl ImpactFilter {
    pub fn new(cutoff_hz: Option<f64>, sr: f64) -> Self {
        let Some(fc) = cutoff_hz.filter(|&f| f > 0.0 && f < 0.49 * sr) else {
            return ImpactFilter { open: true, ..Default::default() };
        };
        let k = (std::f64::consts::PI * fc / sr).tan();
        let r2 = std::f64::consts::SQRT_2;
        let norm = 1.0 / (1.0 + r2 * k + k * k);
        let b0 = k * k * norm;
        ImpactFilter { b: [b0, 2.0 * b0, b0], a: [2.0 * (k * k - 1.0) * norm, (1.0 - r2 * k + k * k) * norm], ..Default::default() }
    }

    fn tick(&mut self, x: f64) -> f64 {
        if self.open {
            return x;
        }
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }

    fn reset(&mut self) {
        self.x = [0.0; 2];
        self.y = [0.0; 2];
    }
}

/// hammer grooves: high-passed noise on the bridge force during the first GROOVE_S after
/// a strike (rise 1 ms, then e^(−t/2 ms)), GROOVE_LEVEL × the contact's peak force, at
/// amount 1 (by ear: the most it should be)
const GROOVE_S: f64 = 0.006;
const GROOVE_LEVEL: f64 = 0.25;
const GROOVE_HP: f64 = 1500.0;
/// damper buzz: when the damper lands, its worn felt chatters on the string: a dead
/// zone (DAMPER_BUZZ_TH × the strike's peak force) on the string's own bridge force,
/// high-passed, e^(−t/τ), BUZZ_LEVEL at amount 1
const BUZZ_TAU_S: f64 = 0.05;
const BUZZ_S: f64 = 0.25;
const BUZZ_TH: f64 = 0.05;
const BUZZ_LEVEL: f64 = 3.0;
const BUZZ_HP: f64 = 600.0;
/// a membrane's buzz (the balafon's gourds, `Instrument::membrane`): noise following the bar's own
/// signal (its rectified level over MEMBRANE_ENV_S), through the membrane's filters (`Membrane`)
const MEMBRANE_ENV_S: f64 = 0.005;

/// A strike's swing at the bridge (the membrane's measure of it): the modes' amplitudes `a`
/// there, their root sum of squares. Not their sum: each mode's onset partner is its antiphase
/// (`export_bars.with_extras` `onset_s`), so the sum, the force's step, is zero by design.
fn swing(a: &dyn Fn(usize) -> f64, note: &Note, kn: usize) -> f64 {
    (0..kn).map(|j| (a(j) * note.bridge[j]).powi(2)).sum::<f64>().sqrt()
}
/// phantom partials: the strings' tension modulation ∝ (transverse motion)², heard at
/// the sums of the partials: level × v²/F (F: an ff E1's peak bridge force), high-passed
const PHANTOM_HP: f64 = 150.0;

/// modes weaker than this (amplitude, re the strongest mode at the strike) are not started
const PRUNE: f64 = 1e-4;
/// a running mode is dropped below this (re the strongest mode at the last strike), looked at every
/// CULL_EVERY blocks (each voice at its own block of them: the work spread; a mode 100 dB down rings a
/// few ms longer)
const CULL: f64 = 1e-5;
const CULL_EVERY: u32 = 4;
/// The runaway guard: a voice whose mode state passes this × the largest single strike it got
/// since it was last silent (or is not finite) is silenced at the end of the block (`end_block`).
/// Strikes in random phase pile up about √N (a roll of 20 a second on a long-ringing bar: ~12 dB),
/// all in phase ~23 dB; a runaway grows without bound (the mallets' first re-strike, step 171).
const RUNAWAY: f64 = 31.6;
/// concurrent hammer contacts per key (a re-strike within the few ms of a contact)
const MAX_CONTACTS: usize = 2;
/// Live re-strike on a bar (`Voice::mallet_meets`): the mallet's head (kg; a marimba's or
/// vibraphone's yarn or cord mallet 25–45 g), its rebound (a strike at the relative speed u changes
/// the bar's surface velocity by (1 + e)·m·u / (m + M)), how long the mallet may chase a bar swinging
/// away (s), and the relative speeds a strike's brightness follows (× the mallet's)
const MALLET_KG: f64 = 0.035;
const MALLET_REBOUND: f64 = 0.5;
const MALLET_CHASE_S: f64 = 0.02;
const MALLET_GRAZE: f64 = 0.05;
const MALLET_REACH: f64 = 4.0;
/// A stiffer felt than fitted (hardness, wear) is tried on a scratch contact first; while
/// it diverges its stiffness is multiplied by `STIFFNESS_BACKOFF`, at most `BACKOFF_TRIES` times.
const STIFFNESS_BACKOFF: f64 = 0.7;
const BACKOFF_TRIES: usize = 24;
/// The explicit contact step is stable while dt·√Λ/2 < 1 (`contact_ratio`); it grew
/// unbounded from about 0.8 (Iowa's C7 at ff, Grand D's top keys), so a stiff contact runs
/// in sub-steps that keep the ratio at most CONTACT_RATIO_MAX (prototype: string_model)
const CONTACT_RATIO_MAX: f64 = 0.5;
const MAX_SUBSTEPS: usize = 64;

/// Contact-phase coefficients of a note (2nd-order resonators per string × partial).
///
/// A resonator driven by force samples, x[n+1] = a1·x[n] − a2·x[n−1] + dt²/m·F[n],
/// responds θ/sin θ (θ = ω·dt) stronger than the continuous one; the model was fitted
/// with that at the reference rate, so `gain` reproduces the reference rate's factor at
/// any other rate.
struct ContactCoeffs {
    a1: Vec<f64>,
    gain: Vec<f64>,
    a2: Vec<f64>,
    sigma: Vec<f64>,
    wd: Vec<f64>,
    g: f64,
    max_steps: usize,
    sep_steps: usize,
    dt: f64,
    reference_sr: f64,
}

impl ContactCoeffs {
    fn new(note: &Note, sr: f64, reference_sr: f64, max_s: f64, sep_s: f64) -> Self {
        let (s, k) = (note.s, note.k);
        let dt = 1.0 / sr;
        let ratio = |th: f64| if th > 1e-9 { th / th.sin() } else { 1.0 };
        let mut c = ContactCoeffs {
            a1: vec![0.0; s * k],
            gain: vec![1.0; s * k],
            a2: vec![0.0; s * k],
            sigma: vec![0.0; s * k],
            wd: vec![0.0; s * k],
            g: dt * dt / note.modal_mass,
            max_steps: (max_s * sr) as usize,
            sep_steps: (sep_s * sr) as usize,
            dt,
            reference_sr,
        };
        for idx in 0..s * k {
            let sg = note.sigma_int[idx] + note.kappa_re[idx % k];
            let w = note.w[idx];
            let d = (w * w - sg * sg).max(1e-9).sqrt();
            let r = (-sg * dt).exp();
            c.sigma[idx] = sg;
            c.wd[idx] = d;
            c.a1[idx] = 2.0 * r * (d * dt).cos();
            c.a2[idx] = r * r;
            c.gain[idx] = ratio(d / reference_sr) / ratio(d * dt);
        }
        c
    }
}

/// dt·√Λ/2 of a contact at the step dt: each string's felt stiffness p·K·δ^(p−1) at the
/// compression δ the hammer's energy would reach on rigid strings (an upper bound: the felt
/// stiffens as it is compressed), over the hammer mass (all strings) and the string's
/// mass at the strike point, Σ sin²(kπx_h)/m over the `kn` simulated partials.
/// `stiffness`: the felt factor of the strike (Contact::hardness), `p`: its exponent.
/// Prototype: string_model.contact_ratio.
fn contact_ratio(note: &Note, kn: usize, speed: f64, stiffness: f64, p: f64, dt: f64) -> f64 {
    let ks = |i: usize| stiffness * note.felt_k[i];
    let k_sum: f64 = (0..note.s).map(ks).sum();
    let delta = ((p + 1.0) * note.hammer_mass * speed * speed / (2.0 * k_sum)).powf(1.0 / (p + 1.0));
    // the felt's core at the same compression (an upper bound: it stops the hammer sooner)
    let core = (delta - note.core_depth).max(0.0).powf(p - 1.0);
    let kc = |i: usize| p * ks(i) * delta.powf(p - 1.0) + p * stiffness * note.core_k[i] * core;
    let k_max = (0..note.s).map(kc).fold(0.0, f64::max);
    let phi2: f64 = note.phi_h[..kn].iter().map(|f| f * f).sum();
    let lam = k_max * phi2 / note.modal_mass + (0..note.s).map(kc).sum::<f64>() / note.hammer_mass;
    lam.sqrt() * dt / 2.0
}

fn substeps(ratio: f64) -> usize {
    ((ratio / CONTACT_RATIO_MAX).ceil() as usize).clamp(1, MAX_SUBSTEPS)
}

/// Hammer contact phase of one strike (buffers reused across strikes).
struct Contact {
    live: bool,
    /// this strike's level correction (a linear gain on its sound and its modes)
    level: f64,
    /// this strike's impact on the board: × its felt force (0: none)
    impact: f64,
    /// felt of this strike: stiffness factor (hardness) and exponent offset (a compressed
    /// felt is stiffer and more nonlinear; the force at 0.5 mm is kept by the factor)
    hardness: f64,
    dp: f64,
    /// per string: how much of the felt force it gets (1; the string the hammer misses with
    /// the soft pedal down: less)
    weight: Vec<f64>,
    /// a finger on the string at the node of this harmonic: only its multiples move (1: all)
    harm: usize,
    /// this strike's mode shapes at its strike point (the note's `phi_h`, or drawn anew)
    phi: Vec<f64>,
    /// per string: how much later the hammer meets it, as the distance it travels meanwhile (m; the
    /// first string 0)
    offset: Vec<f64>,
    x0: Vec<f64>,
    x1: Vec<f64>,
    force: Vec<f64>,
    /// per string: where the strings ringing from earlier strikes are at the strike point this
    /// sample (m; 0 at rest: a strike as from silence), the felt's compression measured against it
    ring: Vec<f64>,
    /// the felt has touched a string (the separation counts from then: a hammer started short of
    /// a ringing string travels free first)
    touched: bool,
    /// started short of a ringing string (`Voice::strike`): the samples before the first touch,
    /// which the contact's longest time does not count
    lead: bool,
    approach: usize,
    eta: f64,
    eta_prev: f64,
    n: usize,
    separated_for: usize,
    /// sub-steps per sample (1: the note's shared coefficients) and, beyond 1, this
    /// strike's own resonator coefficients at the sub-step h = dt/sub
    sub: usize,
    h: f64,
    a1: Vec<f64>,
    a2: Vec<f64>,
    gain: Vec<f64>,
    g: f64,
}

impl Contact {
    fn new(note: &Note) -> Self {
        Contact {
            live: false,
            level: 1.0,
            impact: 0.0,
            hardness: 1.0,
            dp: 0.0,
            weight: vec![1.0; note.s],
            harm: 1,
            phi: note.phi_h.clone(),
            offset: vec![0.0; note.s],
            x0: vec![0.0; note.s * note.k],
            x1: vec![0.0; note.s * note.k],
            force: vec![0.0; note.s],
            ring: vec![0.0; note.s],
            touched: false,
            lead: false,
            approach: 0,
            eta: 0.0,
            eta_prev: 0.0,
            n: 0,
            separated_for: 0,
            sub: 1,
            h: 0.0,
            a1: vec![0.0; note.s * note.k],
            a2: vec![0.0; note.s * note.k],
            gain: vec![1.0; note.s * note.k],
            g: 0.0,
        }
    }

    /// Take over the contact of the same strings with another number of partials (the
    /// ones beyond this note's count are dropped, missing ones start at rest).
    fn copy_from(&mut self, o: &Contact) {
        self.live = o.live;
        self.impact = o.impact;
        self.hardness = o.hardness;
        self.dp = o.dp;
        let s = self.force.len();
        self.weight.copy_from_slice(&o.weight);
        self.harm = o.harm;
        self.offset.copy_from_slice(&o.offset);
        let (k, ko) = (self.x0.len() / s, o.x0.len() / s);
        let m = k.min(ko);
        self.phi[..m].copy_from_slice(&o.phi[..m]);
        self.x0.fill(0.0);
        self.x1.fill(0.0);
        for i in 0..s {
            self.x0[i * k..i * k + m].copy_from_slice(&o.x0[i * ko..i * ko + m]);
            self.x1[i * k..i * k + m].copy_from_slice(&o.x1[i * ko..i * ko + m]);
        }
        self.force.copy_from_slice(&o.force);
        self.ring.copy_from_slice(&o.ring);
        self.touched = o.touched;
        self.lead = o.lead;
        self.approach = o.approach;
        self.eta = o.eta;
        self.eta_prev = o.eta_prev;
        self.n = o.n;
        self.separated_for = o.separated_for;
        self.sub = o.sub;
        self.h = o.h;
        self.g = o.g;
        for i in 0..s {
            self.a1[i * k..i * k + m].copy_from_slice(&o.a1[i * ko..i * ko + m]);
            self.a2[i * k..i * k + m].copy_from_slice(&o.a2[i * ko..i * ko + m]);
            self.gain[i * k..i * k + m].copy_from_slice(&o.gain[i * ko..i * ko + m]);
        }
    }

    fn start(&mut self, note: &Note, c: &ContactCoeffs, kn: usize, speed: f64, hardness: f64, dp: f64) {
        self.live = true;
        // K·c^p → K·0.5mm^(−dp)·c^(p+dp): same force at 0.5 mm compression
        self.hardness = hardness * 5e-4f64.powf(-dp);
        self.dp = dp;
        let ratio = contact_ratio(note, kn, speed, self.hardness, note.felt_p + dp, c.dt);
        self.sub = substeps(ratio);
        self.h = c.dt / self.sub as f64;
        let dt = self.h;
        if self.sub > 1 {
            // the resonators at the sub-step; the response factor θ/sin θ as the prototype
            // has it at the reference rate (its own sub-steps there)
            let sub_ref = substeps(ratio * c.dt * c.reference_sr);
            let h_ref = 1.0 / (c.reference_sr * sub_ref as f64);
            let rf = |th: f64| if th > 1e-9 { th / th.sin() } else { 1.0 };
            for idx in 0..note.s * note.k {
                let (sg, d) = (c.sigma[idx], c.wd[idx]);
                let r = (-sg * dt).exp();
                self.a1[idx] = 2.0 * r * (d * dt).cos();
                self.a2[idx] = r * r;
                self.gain[idx] = rf(d * h_ref) / rf(d * dt);
            }
            self.g = dt * dt / note.modal_mass;
        }
        self.weight.fill(1.0);
        self.harm = 1;
        self.phi.copy_from_slice(&note.phi_h);
        self.offset.fill(0.0);
        self.x0.fill(0.0);
        self.x1.fill(0.0);
        self.ring.fill(0.0);
        self.touched = false;
        self.lead = false;
        self.approach = 0;
        self.eta = 0.0;
        self.eta_prev = -speed * dt;
        self.n = 0;
        self.separated_for = 0;
    }

    /// One contact sample (`sub` steps) over the partials below `kn`; returns (vertical
    /// bridge force at its end, the felt force at its end, finished).
    fn step(&mut self, note: &Note, c: &ContactCoeffs, kn: usize) -> (f64, f64, bool) {
        let (mut out, mut f_sum) = (0.0, 0.0);
        for _ in 0..self.sub {
            (out, f_sum) = self.sub_step(note, c, kn);
        }
        self.touched |= f_sum > 0.0;
        if self.lead && !self.touched {
            self.approach += 1;
        }
        self.separated_for = if f_sum == 0.0 && self.n > 10 && self.touched { self.separated_for + 1 } else { 0 };
        // a hammer chasing a string swinging away: it meets it within a period (the cap: one of the
        // contact's longest time)
        let done = self.separated_for > c.sep_steps || self.n + 1 - self.approach >= c.max_steps || self.approach >= c.max_steps;
        self.n += 1;
        (out, f_sum, done)
    }

    /// One step h of the hammer and the resonators; returns (vertical bridge force, Σ felt force).
    fn sub_step(&mut self, note: &Note, c: &ContactCoeffs, kn: usize) -> (f64, f64) {
        let (s, k) = (note.s, note.k);
        let (a1, a2, gain, g) =
            if self.sub == 1 { (&c.a1, &c.a2, &c.gain, c.g) } else { (&self.a1, &self.a2, &self.gain, self.g) };
        let mut f_sum = 0.0;
        for i in 0..s {
            let row = &self.x0[i * k..i * k + kn];
            let y_h: f64 = row.iter().zip(&self.phi).map(|(x, p)| x * p).sum();
            let cmp = (self.eta - self.offset[i] - y_h - self.ring[i]).max(0.0);
            // (hardness · weight) · K · δ^p in the order it always ran: the coreless strikes bit for bit
            let hw = self.hardness * self.weight[i];
            let f = felt_force(hw * note.felt_k[i], note.felt_p + self.dp, hw * note.core_k[i], note.core_depth, cmp);
            self.force[i] = f;
            f_sum += f;
        }
        let mut out = 0.0;
        for i in 0..s {
            let gf = g * self.force[i];
            for j in (0..kn).filter(|j| (j + 1) % self.harm == 0) {
                let idx = i * k + j;
                let x_new = a1[idx] * self.x0[idx] - a2[idx] * self.x1[idx] + gf * gain[idx] * self.phi[j];
                self.x1[idx] = self.x0[idx];
                self.x0[idx] = x_new;
                out += x_new * note.bridge[j];
            }
        }
        let dt = self.h;
        let eta_new = 2.0 * self.eta - self.eta_prev - dt * dt * f_sum / note.hammer_mass;
        self.eta_prev = self.eta;
        self.eta = eta_new;
        (out, f_sum)
    }

    /// Complex amplitude of string i, partial j at the last contact sample − 1: x[m] = Re(C·z^m)
    /// (from the last two sub-steps, then back by sub − 1 of them).
    fn amplitude(&self, c: &ContactCoeffs, idx: usize) -> Complex64 {
        let lam_h = Complex64::new(-c.sigma[idx] * self.h, c.wd[idx] * self.h);
        let z = lam_h.exp();
        let x1 = self.x1[idx];
        let a = Complex64::new(x1, (x1 * z.re - self.x0[idx]) / z.im.max(1e-12));
        if self.sub == 1 { a } else { a * (-lam_h * (self.sub - 1) as f64).exp() }
    }
}

/// Modes of one polarization. Mode ids j·S + e (partial j < kn, eigenmode e); the running
/// ones are kept densely at positions 0..n of the work arrays (`id`/`pos` map both ways).
struct Modes {
    /// the resonator bank's harmonics × this at a strike (`Voice::set_harmonics_gain`)
    harm_gain: f64,
    // per mode id
    lam: Vec<Complex64>,
    lam_d: Vec<Complex64>,
    pole: [Vec<(f32, f32)>; 2],
    pole_block: [Vec<Complex64>; 2],
    pos: Vec<u32>,
    // per position
    n: usize,
    id: Vec<u32>,
    exact: Vec<Complex64>,
    sr: Vec<Lane>,
    si: Vec<Lane>,
    pr: Vec<Lane>,
    pi: Vec<Lane>,
    /// per position: the pole over one engine block at the current damper engagement
    pb: Vec<Complex64>,
    /// samples run since `exact` was last synchronized
    since: usize,
    /// damper engagement: 0 lifted, 1 resting (λ → λ − engage·σ_damper)
    engage: f64,
    /// the buff's pads rest on the string: λ and λ_d include −σ_buff of their partial
    buff: bool,
    /// strings with a buff: λ, λ_d and their poles (f64; per sample, per block) without it,
    /// so a switch scales them by the pads' real decay and switching off restores them exactly
    /// (empty without a buff)
    base: [Vec<Complex64>; 2],
    base_pole: [Vec<Complex64>; 2],
    base_block: [Vec<Complex64>; 2],
    reference: f64,
    dt: f64,
    block: usize,
    /// per mode id: the new strike's state (scratch for `add`)
    scratch: Vec<Complex64>,
    /// false beats: ids kn·S + c·S + e copy partial `copies[c].0`'s modes, Δω off,
    /// carrying `copies[c].1` of the excitation; per partial its share (0: not split)
    n_base: usize,
    copies: Vec<(usize, f64)>,
    split_r: Vec<f64>,
    /// the first partial this bank carries (a bar's fundamental runs in its resonator's bank)
    lo: usize,
    /// relative frequency offset of every mode (the motor's discs on the resonator's bank)
    detune: f64,
    /// per position: the running modes' state through a re-strike's contact (`ring_start`) and
    /// its pole per sample
    ring: Vec<Complex64>,
    ring_pole: Vec<Complex64>,
    /// the runaway guard's ceiling on any mode's state (RUNAWAY × the largest strike's; 0: none)
    ceiling: f64,
}

const NONE: u32 = u32::MAX;

impl Modes {
    fn new(note: &Note, pol: &Polarization, kn: usize, dt: f64, block: usize) -> Self {
        let s = note.s;
        let splits: Vec<(usize, f64, f64)> = note.split.iter().copied().filter(|&(j, _, _)| j < kn).collect();
        let n_base = kn * s;
        let n_ids = n_base + splits.len() * s;
        let mut lam = Vec::with_capacity(n_ids);
        let mut lam_d = Vec::with_capacity(n_ids);
        for j in 0..kn {
            for e in 0..s {
                lam.push(pol.lam[j * s + e]);
                lam_d.push(pol.lam[j * s + e] - note.damper_sigma[j]);
            }
        }
        let mut split_r = vec![0.0; kn];
        for &(j, dw, r) in &splits {
            split_r[j] = r;
            for e in 0..s {
                let l = pol.lam[j * s + e] + Complex64::new(0.0, dw);
                lam.push(l);
                lam_d.push(l - note.damper_sigma[j]);
            }
        }
        let mut m = Modes::with_poles(lam, lam_d, !note.buff_sigma.is_empty(), dt, block);
        m.n_base = n_base;
        m.copies = splits.iter().map(|&(j, _, r)| (j, r)).collect();
        m.split_r = split_r;
        m.lo = if note.tube.is_some() { 1 } else { 0 };
        m
    }

    /// The resonator's bank of a bar (`Physics::tube`): id 0 the fundamental (partial 0 of
    /// `pol`), ids 1… the harmonics in `harm` (indices into the bar's list: those below
    /// `lim` rad/s), each at h·ω1 with its own loss free and with the damper resting.
    /// Empty for other notes.
    fn tube(note: &Note, pol: &Polarization, lim: f64, dt: f64, block: usize) -> (Self, Vec<usize>) {
        let (mut lam, mut lam_d, mut harm) = (Vec::new(), Vec::new(), Vec::new());
        if let Some(t) = note.tube.as_ref().filter(|_| note.s == 1 && !pol.lam.is_empty() && pol.lam[0].im < lim) {
            let (l1, sd) = (pol.lam[0], note.damper_sigma[0]);
            lam.push(l1);
            lam_d.push(l1 - sd);
            let hs = &t.harmonics;
            for i in 0..hs.h.len() {
                let w = l1.im * hs.h[i] as f64;
                if w < lim {
                    lam.push(Complex64::new(l1.re * hs.free[i].decay, w));
                    lam_d.push(Complex64::new((l1.re - sd) * hs.damped[i].decay, w));
                    harm.push(i);
                }
            }
        }
        (Modes::with_poles(lam, lam_d, false, dt, block), harm)
    }

    /// A bank of modes with these λ (damper lifted, resting); no false beats.
    fn with_poles(lam: Vec<Complex64>, lam_d: Vec<Complex64>, has_buff: bool, dt: f64, block: usize) -> Self {
        let n_ids = lam.len();
        let pole32 = |l: &[Complex64]| -> Vec<(f32, f32)> {
            l.iter()
                .map(|&l| {
                    let p = (l * dt).exp();
                    (p.re as f32, p.im as f32)
                })
                .collect()
        };
        let pole_b = |l: &[Complex64]| -> Vec<Complex64> { l.iter().map(|&l| (l * (dt * block as f64)).exp()).collect() };
        let chunks = kernel::padded(n_ids) / LANES;
        let keep = |v: Vec<Complex64>| if has_buff { v } else { Vec::new() };
        let pole_f = |l: &[Complex64]| -> Vec<Complex64> { l.iter().map(|&l| (l * dt).exp()).collect() };
        let base = [keep(lam.clone()), keep(lam_d.clone())];
        let base_pole = [keep(pole_f(&lam)), keep(pole_f(&lam_d))];
        let base_block = [keep(pole_b(&lam)), keep(pole_b(&lam_d))];
        Modes {
            harm_gain: 1.0,
            base,
            base_pole,
            base_block,
            pole: [pole32(&lam), pole32(&lam_d)],
            pole_block: [pole_b(&lam), pole_b(&lam_d)],
            lam,
            lam_d,
            pos: vec![NONE; n_ids],
            n: 0,
            id: vec![NONE; n_ids],
            exact: vec![Complex64::new(0.0, 0.0); n_ids],
            sr: vec![[0.0; LANES]; chunks],
            si: vec![[0.0; LANES]; chunks],
            pr: vec![[0.0; LANES]; chunks],
            pi: vec![[0.0; LANES]; chunks],
            pb: vec![Complex64::new(0.0, 0.0); n_ids],
            since: 0,
            engage: 0.0,
            buff: false,
            reference: 0.0,
            dt,
            block,
            scratch: vec![Complex64::new(0.0, 0.0); n_ids],
            n_base: n_ids,
            copies: Vec::new(),
            split_r: Vec::new(),
            lo: 0,
            detune: 0.0,
            ring: vec![Complex64::new(0.0, 0.0); n_ids],
            ring_pole: vec![Complex64::new(0.0, 0.0); n_ids],
            ceiling: 0.0,
        }
    }

    fn set_work(&mut self, p: usize, s: Complex64) {
        self.sr[p / LANES][p % LANES] = s.re as f32;
        self.si[p / LANES][p % LANES] = s.im as f32;
    }

    /// λ at the current engagement (and detune)
    fn lam_at(&self, id: usize) -> Complex64 {
        let l = self.lam[id] + (self.lam_d[id] - self.lam[id]) * self.engage;
        if self.detune == 0.0 { l } else { Complex64::new(l.re, l.im * (1.0 + self.detune)) }
    }

    fn set_pole(&mut self, p: usize) {
        let id = self.id[p] as usize;
        let ((r, i), b) = if self.detune == 0.0 && (self.engage == 0.0 || self.engage == 1.0) {
            let d = (self.engage == 1.0) as usize;
            (self.pole[d][id], self.pole_block[d][id])
        } else {
            let l = self.lam_at(id);
            let q = (l * self.dt).exp();
            ((q.re as f32, q.im as f32), (l * (self.dt * self.block as f64)).exp())
        };
        self.pr[p / LANES][p % LANES] = r;
        self.pi[p / LANES][p % LANES] = i;
        self.pb[p] = b;
    }

    /// Bring the exact states to the current sample and restart the float32 ones from them.
    fn sync(&mut self) {
        if self.since == 0 {
            return;
        }
        if self.since == self.block {
            // a whole block (the common case): each mode × its pole over a block, lane chunk by chunk
            let n = self.n;
            let chunks = self.exact[..n].chunks_mut(LANES).zip(self.pb[..n].chunks(LANES));
            for ((e, pb), (sr, si)) in chunks.zip(self.sr.iter_mut().zip(self.si.iter_mut())) {
                for (l, (e, &b)) in e.iter_mut().zip(pb).enumerate() {
                    *e *= b;
                    sr[l] = e.re as f32;
                    si[l] = e.im as f32;
                }
            }
            self.since = 0;
            return;
        }
        for p in 0..self.n {
            let id = self.id[p] as usize;
            let adv = if self.since == self.block {
                self.pb[p]
            } else {
                (self.lam_at(id) * (self.dt * self.since as f64)).exp()
            };
            self.exact[p] *= adv;
            let s = self.exact[p];
            self.set_work(p, s);
        }
        self.since = 0;
    }

    /// The running modes' state at the next sample to run, and their poles, for `ring_at`.
    fn ring_start(&mut self) {
        for p in 0..self.n {
            let l = self.lam_at(self.id[p] as usize);
            self.ring[p] = self.exact[p] * (l * (self.dt * self.since as f64)).exp();
            self.ring_pole[p] = (l * self.dt).exp();
        }
    }

    /// The partial of mode id (a false beat's copy: the partial it copies).
    fn partial(&self, id: usize, s: usize) -> usize {
        if id < self.n_base { id / s } else { self.copies[(id - self.n_base) / s].0 }
    }

    /// Where the running modes put each string at the shapes `phi` (`ring_start`'s state; `w` per
    /// mode id and string: state → displacement), into `out`.
    fn ring_at(&self, w: &[Complex64], s: usize, phi: &[f64], out: &mut [f64]) {
        out.fill(0.0);
        for p in 0..self.n {
            let id = self.id[p] as usize;
            let f = phi[self.partial(id, s)];
            for (i, o) in out.iter_mut().enumerate() {
                *o += (self.ring[p] * w[id * s + i] * f).re;
            }
        }
    }

    /// A bar's followed state summed over its modes (one string): Σ s·w and Σ s·w·λ (its
    /// displacement and velocity at the strike point, × `w`'s scale); `only` the first id alone
    /// (a resonator's bank: its fundamental, not the tube's harmonics).
    fn ring_sum(&self, w: &[Complex64], only: bool) -> (Complex64, Complex64) {
        let (mut x, mut v) = (Complex64::new(0.0, 0.0), Complex64::new(0.0, 0.0));
        for p in 0..self.n {
            let id = self.id[p] as usize;
            if (only && id != 0) || id >= w.len() {
                continue;
            }
            let a = self.ring[p] * w[id];
            x += a;
            v += a * self.lam_at(id);
        }
        (x, v)
    }

    /// A collision at `steps` samples on (`ring_start`'s state stepped there) takes `c` of each mode's
    /// velocity (x = Re s, ẋ ≈ −ω·Im s: Im s × (1 − c)), its displacement kept; back to now through
    /// the pole. `only`: the first id alone (a resonator's bank: its fundamental). Synchronized.
    fn brake(&mut self, c: f64, steps: usize, only: bool) {
        for p in 0..self.n {
            let id = self.id[p] as usize;
            if only && id != 0 {
                continue;
            }
            let back = (-self.lam_at(id) * (self.dt * steps as f64)).exp();
            self.exact[p] += Complex64::new(0.0, -c * self.ring[p].im) * back;
            let s = self.exact[p];
            self.set_work(p, s);
        }
    }

    /// The followed state one sample on.
    fn ring_step(&mut self) {
        for p in 0..self.n {
            self.ring[p] *= self.ring_pole[p];
        }
    }

    fn run(&mut self, acc: &mut [Lane]) {
        if self.n == 0 || acc.is_empty() {
            return;
        }
        let c = self.n.div_ceil(LANES);
        kernel::decay(&mut self.sr[..c], &mut self.si[..c], &self.pr[..c], &self.pi[..c], acc);
        self.since += acc.len();
    }

    fn set_engage(&mut self, engage: f64) {
        if engage == self.engage {
            return;
        }
        self.sync();
        self.engage = engage;
        for p in 0..self.n {
            self.set_pole(p);
        }
    }

    /// Every mode's frequency × (1 + d) from now on (running ones go on from their state).
    fn set_detune(&mut self, d: f64) {
        if d == self.detune {
            return;
        }
        self.sync();
        self.detune = d;
        for p in 0..self.n {
            self.set_pole(p);
        }
    }

    /// The buff's pads on or off the string: every mode's λ (free and damped) moves by
    /// ∓σ_buff of its partial and its poles follow; running modes go on from their state.
    /// Allocation-free and cheap: the poles are the stored unbuffed ones times the pads' real
    /// decay (two exp per partial); off restores them exactly. `sigma` per partial (≥ kn
    /// entries), `s` strings per partial.
    fn set_buff(&mut self, on: bool, sigma: &[f64], s: usize) {
        if on == self.buff || self.base[0].is_empty() {
            return;
        }
        self.sync();
        self.buff = on;
        let bt = self.dt * self.block as f64;
        // ids run partial by partial (copies too): the factors change only with the partial
        let mut last = (usize::MAX, 0.0, 1.0, 1.0);
        for id in 0..self.lam.len() {
            let j = if id < self.n_base { id / s } else { self.copies[(id - self.n_base) / s].0 };
            if j != last.0 {
                last = if on { (j, sigma[j], (-sigma[j] * self.dt).exp(), (-sigma[j] * bt).exp()) } else { (j, 0.0, 1.0, 1.0) };
            }
            let (_, sig, f, fb) = last;
            for c in 0..2 {
                let (lam, p) = if on {
                    (self.base[c][id] - sig, self.base_pole[c][id] * f)
                } else {
                    (self.base[c][id], self.base_pole[c][id])
                };
                if c == 0 { self.lam[id] = lam } else { self.lam_d[id] = lam }
                self.pole[c][id] = (p.re as f32, p.im as f32);
                self.pole_block[c][id] = if on { self.base_block[c][id] * fb } else { self.base_block[c][id] };
            }
        }
        for p in 0..self.n {
            self.set_pole(p);
        }
    }

    /// Add a strike's free decay (state at m = 2 after the last contact sample − 1):
    /// amplitude(i, j) per string and partial → modal weights. Must be synchronized.
    fn add(&mut self, note: &Note, pol: &Polarization, amp: &dyn Fn(usize, usize) -> Complex64, gain: f64, kn: usize) {
        debug_assert_eq!(self.since, 0);
        let s = note.s;
        // new state per mode id, then the pruning reference
        let mut reference: f64 = 0.0;
        let mut add = std::mem::take(&mut self.scratch);
        let lo = self.lo.min(kn);
        for j in lo..kn {
            for e in 0..s {
                let mut q = Complex64::new(0.0, 0.0);
                for i in 0..s {
                    q += pol.vinv[j * s * s + e * s + i] * amp(i, j) * gain;
                }
                let id = j * s + e;
                let w0 = note.bridge[j] * pol.colsum[j * s + e] * q;
                add[id] = w0 * (1.0 - self.split_r[j]) * (self.lam[id] * 2.0 * self.dt).exp();
                let cur = match self.pos[id] {
                    NONE => Complex64::new(0.0, 0.0),
                    p => self.exact[p as usize],
                };
                reference = reference.max((cur + add[id]).norm());
                // the split partial's second tone (false beats)
                if self.split_r[j] > 0.0 {
                    let c = self.copies.iter().position(|&(cj, _)| cj == j).unwrap_or(0);
                    let cid = self.n_base + c * s + e;
                    add[cid] = w0 * self.split_r[j] * (self.lam[cid] * 2.0 * self.dt).exp();
                }
            }
        }
        self.reference = reference;
        let strike = add.iter().take(kn * s).skip(lo * s).fold(0.0f64, |m, w| m.max(w.norm()));
        self.ceiling = self.ceiling.max(RUNAWAY * strike);
        // only the ids written above (partials lo…kn; the rest of the scratch is stale)
        for (id, &w) in add.iter().enumerate().take(kn * s).skip(lo * s) {
            self.apply(id, w, reference);
        }
        for c in 0..self.copies.len() {
            if (lo..kn).contains(&self.copies[c].0) {
                for e in 0..s {
                    let id = self.n_base + c * s + e;
                    self.apply(id, add[id], reference);
                }
            }
        }
        self.scratch = add;
    }

    /// A strike on the resonator's bank (`tube`): `a1` the fundamental's excitation (as
    /// `add`'s amplitude of partial 0, gain included); the harmonics are set from the
    /// fundamental's new state z (they are a function of it): |z|·10^(L/20) at phase h·arg z,
    /// L = level + growth·20·log10(|z|/a_ref), free and damped interpolated by the
    /// engagement. Must be synchronized.
    fn add_tube(&mut self, note: &Note, pol: &Polarization, a1: Complex64, harm: &[usize]) {
        debug_assert_eq!(self.since, 0);
        let Some(t) = note.tube.as_ref().filter(|_| !self.lam.is_empty()) else { return };
        let w = note.bridge[0] * pol.colsum[0] * pol.vinv[0] * a1 * (self.lam[0] * 2.0 * self.dt).exp();
        let cur = match self.pos[0] {
            NONE => Complex64::new(0.0, 0.0),
            p => self.exact[p as usize],
        };
        let z = cur + w;
        self.reference = z.norm();
        self.ceiling = self.ceiling.max(RUNAWAY * w.norm());
        self.apply(0, w, self.reference);
        if t.a_ref <= 0.0 || z.norm() == 0.0 {
            return;
        }
        let (e, r_db, arg) = (self.engage, 20.0 * (z.norm() / t.a_ref).log10(), z.arg());
        let hs = &t.harmonics;
        for (k, &i) in harm.iter().enumerate() {
            let (f, d) = (hs.free[i], hs.damped[i]);
            let level = f.level_db + (d.level_db - f.level_db) * e + (f.growth + (d.growth - f.growth) * e) * r_db;
            let q = Complex64::from_polar(z.norm() * 10f64.powf(level / 20.0) * self.harm_gain, hs.h[i] as f64 * arg);
            self.ceiling = self.ceiling.max(RUNAWAY * q.norm());
            match self.pos[k + 1] {
                NONE => self.apply(k + 1, q, self.reference),
                p => {
                    self.exact[p as usize] = q;
                    self.set_work(p as usize, q);
                }
            }
        }
    }

    /// Add w to mode id (start it if it is not running and above the pruning limit; a
    /// zero one never: a polarization without output, a bar's horizontal one).
    fn apply(&mut self, id: usize, w: Complex64, reference: f64) {
        match self.pos[id] {
            NONE => {
                if w.norm() < PRUNE * reference || w == Complex64::new(0.0, 0.0) {
                    return;
                }
                let p = self.n;
                self.n += 1;
                self.id[p] = id as u32;
                self.pos[id] = p as u32;
                self.exact[p] = w;
                self.set_pole(p);
                self.set_work(p, w);
            }
            p => {
                let p = p as usize;
                self.exact[p] += w;
                let s = self.exact[p];
                self.set_work(p, s);
            }
        }
    }

    /// Take over the running modes of `old` (the same note compiled for another tuning):
    /// each state goes on at its new pole, so the output is continuous and the strings
    /// glide to their new frequencies. A false-beat copy maps to the same partial's copy
    /// (merged into the partial when the new tuning has none); partials beyond the new
    /// rate limit are dropped. Must be fresh (no running modes).
    fn adopt(&mut self, old: &mut Modes, s: usize) {
        old.sync();
        self.reference = old.reference;
        self.ceiling = old.ceiling;
        for p in 0..old.n {
            let id = old.id[p] as usize;
            let to = if id < old.n_base {
                (id < self.n_base).then_some(id)
            } else {
                let (c, e) = ((id - old.n_base) / s, (id - old.n_base) % s);
                let j = old.copies[c].0;
                match self.copies.iter().position(|&(cj, _)| cj == j) {
                    Some(c) => Some(self.n_base + c * s + e),
                    None => (j * s + e < self.n_base).then_some(j * s + e),
                }
            };
            if let Some(to) = to {
                // reference 0: never pruned
                self.apply(to, old.exact[p], 0.0);
            }
        }
    }

    /// Drop modes below CULL re the reference (synchronized states only).
    /// Drop every mode (allocation-free).
    fn clear(&mut self) {
        for p in 0..self.n {
            self.pos[self.id[p] as usize] = NONE;
            self.id[p] = NONE;
            self.exact[p] = Complex64::new(0.0, 0.0);
            self.set_work(p, Complex64::new(0.0, 0.0));
        }
        self.n = 0;
        self.since = 0;
        self.ceiling = 0.0;
    }

    /// A mode's state past the ceiling, or not finite (synchronized).
    fn runaway(&self) -> bool {
        let lim = self.ceiling * self.ceiling;
        self.exact[..self.n].iter().any(|e| !(e.norm_sqr() <= lim))
    }

    fn cull(&mut self) {
        let lim = (CULL * self.reference).powi(2);
        let mut p = 0;
        while p < self.n {
            let a = self.exact[p].norm_sqr();
            if a >= lim && a > 0.0 {
                p += 1;
                continue;
            }
            let last = self.n - 1;
            self.pos[self.id[p] as usize] = NONE;
            if p != last {
                self.id[p] = self.id[last];
                self.pos[self.id[p] as usize] = p as u32;
                self.exact[p] = self.exact[last];
                self.pb[p] = self.pb[last];
                let (sr, si, pr, pi) = (self.sr[last / LANES][last % LANES], self.si[last / LANES][last % LANES], self.pr[last / LANES][last % LANES], self.pi[last / LANES][last % LANES]);
                self.sr[p / LANES][p % LANES] = sr;
                self.si[p / LANES][p % LANES] = si;
                self.pr[p / LANES][p % LANES] = pr;
                self.pi[p / LANES][p % LANES] = pi;
            }
            self.id[last] = NONE;
            self.set_work(last, Complex64::new(0.0, 0.0));
            self.n = last;
        }
        if self.n == 0 {
            self.ceiling = 0.0;
        }
    }
}

pub struct Voice {
    /// the next strike's level correction (linear), set before it by `Engine` from the note's
    /// `level_db`
    strike_gain: f64,
    /// the next strike's impact on the board (× its felt force; 0: none), its lowpass and the
    /// samples its tail still runs
    impact_next: f64,
    impact_lp: ImpactFilter,
    impact_tail: usize,
    coeffs: ContactCoeffs,
    contacts: Vec<Contact>,
    /// scratch contact for `stable_hardness`
    probe: Contact,
    vert: Modes,
    horiz: Modes,
    /// a bar's resonator: its fundamental and the fundamental's harmonics (`Modes::tube`;
    /// `harm` the harmonics it runs), rendered on their own for the motor's modulation
    tube: Modes,
    harm: Vec<usize>,
    /// partials below 0.45·sr (all of them at the reference rate)
    kn: usize,
    /// damper engagement (0 lifted … 1 resting)
    pub engage: f64,
    /// the buff's pads rest on these strings (set at a pluck, `set_buff`)
    pub buff: bool,
    /// sounding from a strike until its damper falls (the sympathetic bank leaves these
    /// strings to the voice meanwhile)
    pub own: bool,
    pub active: bool,
    dt: f64,
    /// largest contact bridge force since the last strike (scale of grooves and buzz)
    peak: f64,
    rng: Rng,
    /// each strike's own draws (`set_variation`), apart from the noises' stream
    strike_rng: Rng,
    variation: f64,
    groove_n: usize,
    groove_len: usize,
    groove_hp: HighPass,
    buzz_n: usize,
    buzz_len: usize,
    /// buzz decay time (s) and the harmonic number of a strike's finger node (1: none)
    buzz_tau: f64,
    harmonic: usize,
    buzz_hp: HighPass,
    phantom_hp: HighPass,
    /// blocks since its modes were last culled (`end_block`), from `cull_phase` at each start from
    /// rest (a strike from silence culls at the same blocks however long the voice has existed)
    cull_tick: u32,
    cull_phase: u32,
    /// the membrane's buzz: its level follower, filters and their coefficients (`membrane`)
    membrane_env: f64,
    membrane_hp: HighPass,
    membrane_lp: [f64; MEMBRANE_POLES],
    membrane_a: (f64, f64),
    membrane_poles: usize,
    /// the membrane (`Membrane`): its lowpass, growth and tilt; this key's strike at speed 1 (its
    /// `peak`); the last strike's gain
    membrane_spec: (f64, f64, f64),
    membrane_ref: f64,
    /// the noise's power on the bar's level and that level's peak since the strike (`Membrane::power`)
    membrane_power: f64,
    membrane_peak: f64,
    membrane_gain: f64,
    /// the mallet's share of the membrane's gain at the next strike (`Mallet::buzz`)
    membrane_mallet: f64,
    /// the other tines' ringing along (`Physics::halo_from`) at the next strike: × this
    /// (`Params::sympathetic`)
    halo_gain: f64,
    /// white noise's density re the 48 kHz the level was fitted at (unit variance per sample)
    membrane_scale: f64,
    /// a re-strike's felt meets the strings where they ring (`set_restrike`; off: as from rest)
    restrike: bool,
    /// per vertical mode id and string: its state → the string's displacement (m per unit of the
    /// strike gain): v_ie / (bridge_j · colsum_je)
    ring_w: Vec<Complex64>,
    /// the contacts follow the running modes (`Modes::ring_start`) this render
    ring_on: bool,
    /// the strike gain the running modes carry (the last strike's whose modes joined)
    mode_level: f64,
    /// a bar's resonator bank: its fundamental's state → the bar's displacement (`ring_w`'s)
    tube_w: Vec<Complex64>,
    /// the relative speed the last mallet met its bar at (`mallet_meets`)
    mallet_u: f64,
    /// voices silenced by the runaway guard (`end_block`; `Engine::stats`)
    pub runaways: u32,
}

/// Keep the `MAX_PARTIALS` strongest, sorted by `amp` (descending; any monotone measure, the caller passes the squared amplitude): a bounded insertion sort.
fn insert_partial(out: &mut [Partial; MAX_PARTIALS], n: &mut usize, p: Partial) {
    let mut i = if *n < MAX_PARTIALS {
        *n += 1;
        *n - 1
    } else if p.amp > out[MAX_PARTIALS - 1].amp {
        MAX_PARTIALS - 1
    } else {
        return;
    };
    while i > 0 && out[i - 1].amp < p.amp {
        out[i] = out[i - 1];
        i -= 1;
    }
    out[i] = p;
}

/// A strike's free decay into both polarizations and the resonator's bank (synchronized).
fn add_modes([vert, horiz, tube]: [&mut Modes; 3], harm: &[usize], note: &Note, amp: &dyn Fn(usize, usize) -> Complex64, kn: usize) {
    vert.add(note, &note.vert, amp, 1.0, kn);
    horiz.add(note, &note.horiz, amp, note.horiz_gain, kn);
    if kn > 0 {
        tube.add_tube(note, &note.vert, amp(0, 0), harm);
    }
}

impl Voice {
    pub fn new(note: &Note, sr: f64, reference_sr: f64, max_s: f64, sep_s: f64, block: usize) -> Self {
        let dt = 1.0 / sr;
        let lim = 2.0 * std::f64::consts::PI * 0.45 * sr;
        let kn = (0..note.k).take_while(|&j| (0..note.s).all(|i| note.w[i * note.k + j] < lim)).count();
        let (tube, harm) = Modes::tube(note, &note.vert, lim, dt, block);
        let mut v = Voice {
            strike_gain: 1.0,
            impact_next: 0.0,
            impact_lp: ImpactFilter::new(None, sr),
            impact_tail: 0,
            coeffs: ContactCoeffs::new(note, sr, reference_sr, max_s, sep_s),
            contacts: (0..MAX_CONTACTS).map(|_| Contact::new(note)).collect(),
            probe: Contact::new(note),
            vert: Modes::new(note, &note.vert, kn, dt, block),
            horiz: Modes::new(note, &note.horiz, kn, dt, block),
            tube,
            harm,
            kn,
            engage: 0.0,
            buff: false,
            own: false,
            active: false,
            dt,
            peak: 0.0,
            rng: Rng::new(0xb0a7 + note.note as u64),
            strike_rng: Rng::new(0x5712_4e00 ^ note.note as u64 ^ (note.register as u64) << 8),
            variation: 0.0,
            groove_n: usize::MAX,
            groove_len: (GROOVE_S * sr) as usize,
            groove_hp: HighPass::new(GROOVE_HP, sr),
            buzz_n: usize::MAX,
            buzz_len: (BUZZ_S * sr) as usize,
            buzz_tau: BUZZ_TAU_S,
            harmonic: 1,
            buzz_hp: HighPass::new(BUZZ_HP, sr),
            phantom_hp: HighPass::new(PHANTOM_HP, sr),
            cull_tick: note.note as u32 % CULL_EVERY,
            cull_phase: note.note as u32 % CULL_EVERY,
            membrane_env: 0.0,
            membrane_hp: HighPass::new(20.0, sr),
            membrane_lp: [0.0; MEMBRANE_POLES],
            membrane_a: (1.0 - (-1.0 / (MEMBRANE_ENV_S * sr)).exp(), 1.0),
            membrane_poles: 1,
            membrane_spec: (0.0, 0.0, 0.0),
            membrane_ref: 0.0,
            membrane_power: 1.0,
            membrane_peak: 0.0,
            membrane_gain: 1.0,
            membrane_mallet: 1.0,
            halo_gain: 1.0,
            membrane_scale: (sr / 48000.0).sqrt(),
            restrike: false,
            ring_w: Vec::new(),
            ring_on: false,
            mode_level: 1.0,
            tube_w: Vec::new(),
            mallet_u: 0.0,
            runaways: 0,
        };
        let s = note.s;
        let ids = v.vert.lam.len();
        v.ring_w = vec![Complex64::new(0.0, 0.0); ids * s];
        for id in 0..ids {
            let (j, e) = (v.vert.partial(id, s), id % s);
            let den = note.bridge[j] * note.vert.colsum[j * s + e];
            if den.norm() > 1e-300 {
                for i in 0..s {
                    v.ring_w[id * s + i] = note.vert.v[j * s * s + i * s + e] / den;
                }
            }
        }
        if !v.tube.lam.is_empty() {
            let den = note.bridge[0] * note.vert.colsum[0] * note.vert.vinv[0];
            v.tube_w = vec![if den.norm() > 1e-300 { den.inv() } else { Complex64::new(0.0, 0.0) }];
        }
        v.set_engage(if note.has_damper { 1.0 } else { 0.0 });
        v
    }

    /// Take over the state of `old`, the same note compiled for another tuning (both at
    /// the same rate): running modes, hammer contacts in progress, noise generators. Real-
    /// time safe; `old` must not be used afterwards.
    pub fn adopt(&mut self, old: &mut Voice, note: &Note) {
        self.set_buff(note, old.buff);
        self.set_engage(old.engage);
        self.vert.adopt(&mut old.vert, note.s);
        self.horiz.adopt(&mut old.horiz, note.s);
        self.tube.adopt(&mut old.tube, 1);
        self.tube.set_detune(old.tube.detune);
        self.vert.set_detune(old.vert.detune);
        self.horiz.set_detune(old.horiz.detune);
        for (c, o) in self.contacts.iter_mut().zip(&old.contacts) {
            c.copy_from(o);
        }
        self.impact_next = old.impact_next;
        self.impact_lp = old.impact_lp;
        self.impact_tail = old.impact_tail;
        self.own = old.own;
        self.active = old.active;
        self.peak = old.peak;
        self.rng = old.rng.clone();
        self.strike_rng = old.strike_rng.clone();
        self.variation = old.variation;
        self.restrike = old.restrike;
        self.runaways = old.runaways;
        self.ring_on = old.ring_on;
        self.mode_level = old.mode_level;
        self.groove_n = old.groove_n;
        self.groove_hp = old.groove_hp;
        self.buzz_n = old.buzz_n;
        self.buzz_hp = old.buzz_hp;
        self.phantom_hp = old.phantom_hp;
        self.cull_tick = old.cull_tick;
        self.membrane_env = old.membrane_env;
        self.membrane_hp = old.membrane_hp;
        self.membrane_lp = old.membrane_lp;
        self.membrane_a = old.membrane_a;
        self.membrane_gain = old.membrane_gain;
        self.membrane_mallet = old.membrane_mallet;
        self.membrane_peak = old.membrane_peak;
    }

    /// Whether `old` can hand its state over to this voice (same rate and strings).
    pub fn compatible(&self, old: &Voice) -> bool {
        self.dt == old.dt && self.contacts[0].force.len() == old.contacts[0].force.len()
    }

    pub fn set_engage(&mut self, engage: f64) {
        self.engage = engage;
        self.vert.set_engage(engage);
        self.horiz.set_engage(engage);
        self.tube.set_engage(engage);
    }

    /// The motor's discs: the resonator's modes × (1 + d) in frequency.
    pub fn set_tube_detune(&mut self, d: f64) {
        self.tube.set_detune(d);
    }

    /// The strings' modes × (1 + d) in frequency from now on (the clavichord's Bebung: the
    /// tangent's pressure raises the tension); ringing modes go on from their state.
    pub fn set_detune(&mut self, d: f64) {
        self.vert.set_detune(d);
        self.horiz.set_detune(d);
    }

    /// The buff stop at a pluck: on strings with a buff the whole string takes or leaves
    /// the pads' loss, its ringing modes included; others stay as they are.
    pub fn set_buff(&mut self, note: &Note, on: bool) {
        let on = on && !note.buff_sigma.is_empty();
        if on == self.buff {
            return;
        }
        self.vert.set_buff(on, &note.buff_sigma, note.s);
        self.horiz.set_buff(on, &note.buff_sigma, note.s);
        self.buff = on;
    }

    /// The hammer contact is an explicit step, which diverges (to inf and NaN, which then
    /// stays in the convolvers) once the felt is stiff enough against a light hammer and
    /// the string's own response; whether it does depends on the whole coupled system.
    /// A felt at least as soft as fitted is as stable as the fitted one; a stiffer one
    /// (hardness above 1, wear) is run on the scratch contact and backed off until it holds.
    fn stable_hardness(&mut self, note: &Note, speed: f64, hardness: f64, dp: f64) -> f64 {
        if hardness <= 1.0 && dp <= 0.0 {
            return hardness;
        }
        let mut h = hardness;
        for _ in 0..BACKOFF_TRIES {
            if self.contact_holds(note, speed, h, dp) {
                break;
            }
            h *= STIFFNESS_BACKOFF;
        }
        h
    }

    /// Run a whole contact on the scratch one: false when it diverges (non-finite, or the
    /// hammer further from the string than at its own speed it could get).
    fn contact_holds(&mut self, note: &Note, speed: f64, hardness: f64, dp: f64) -> bool {
        let reach = 2.0 * speed * self.dt * self.coeffs.max_steps as f64;
        self.probe.start(note, &self.coeffs, self.kn, speed, hardness, dp);
        loop {
            let (out, _, done) = self.probe.step(note, &self.coeffs, self.kn);
            if !out.is_finite() || !self.probe.eta.is_finite() || self.probe.eta.abs() > reach {
                return false;
            }
            if done {
                return true;
            }
        }
    }

    /// Start a hammer contact: felt stiffness × hardness (backed off to a stable one),
    /// exponent + dp (ignored when MAX_CONTACTS are already running).
    /// `miss` 0 … 1: the hammer misses one string of a unison (the last of two or three)
    /// by that much (the una corda); notes of one string are not affected.
    pub fn strike(&mut self, note: &Note, speed: f64, hardness: f64, dp: f64, miss: f64) {
        let hardness = self.stable_hardness(note, speed, hardness, dp);
        // this strike's draws (none at variation 0: the strikes as before, bit for bit)
        let v = self.variation;
        let mut d_pos = 0.0;
        let mut strings = [(0.0f64, 1.0f64); MAX_STRIKE_STRINGS];
        if v > 0.0 {
            let r = &mut self.strike_rng;
            if let Some((_, l)) = note.strike {
                d_pos = v * STRIKE_POS_SIGMA_M / l * r.gauss();
            }
            for st in strings.iter_mut().take(note.s) {
                *st = (v * STRIKE_TIME_SIGMA_S * speed * r.gauss(), (v * STRIKE_FELT_SIGMA * r.gauss()).exp());
            }
            // the first string met at the hammer's arrival: a later one only (an earlier one would
            // start compressed, a jump)
            let first = strings.iter().take(note.s).map(|s| s.0).fold(f64::INFINITY, f64::min);
            strings.iter_mut().take(note.s).for_each(|s| s.0 -= first);
        }
        if let Some(c) = self.contacts.iter_mut().find(|c| !c.live) {
            c.start(note, &self.coeffs, self.kn, speed, hardness, dp);
            c.level = self.strike_gain;
            c.impact = self.impact_next;
            c.harm = self.harmonic;
            if note.s > 1 {
                c.weight[note.s - 1] = 1.0 - miss.clamp(0.0, 1.0);
            }
            if v > 0.0 {
                if let (Some((pos, _)), true) = (note.strike, d_pos != 0.0) {
                    let p = (pos + d_pos).clamp(STRIKE_POS_RANGE.0, STRIKE_POS_RANGE.1);
                    c.phi.iter_mut().enumerate().for_each(|(j, f)| *f = ((j + 1) as f64 * std::f64::consts::PI * p).sin());
                }
                for (i, &(off, w)) in strings.iter().take(note.s).enumerate() {
                    c.offset[i] = off;
                    c.weight[i] *= w;
                }
            }
            if self.restrike && self.vert.n > 0 {
                // the hammer meets the ringing strings where they are: at the rest plane as from
                // silence, or at the nearest string where one swings towards it (else it would start
                // compressed, a jump)
                self.vert.sync();
                self.vert.ring_start();
                self.vert.ring_at(&self.ring_w, note.s, &c.phi, &mut c.ring);
                let near = c.ring.iter().zip(&c.offset).fold(0.0f64, |a, (&u, &o)| a.min(u / self.mode_level + o));
                c.eta += near;
                c.eta_prev += near;
                c.lead = true;
                c.ring.fill(0.0);
                self.ring_on = true;
            }
            self.active = true;
            self.own = true;
            self.peak = 0.0;
            self.groove_n = 0;
            self.groove_hp.reset();
        }
    }

    /// Pluck: the strings released from rest (`physics::pluck_amplitude` at log2(speed)
    /// dynamic steps, `hardness` on the quill lowpass), added onto the running modes like
    /// `kick`. No contact phase; allocation-free.
    pub fn pluck(&mut self, note: &Note, speed: f64, hardness: f64) {
        let Some(p) = note.pluck else { return };
        self.pluck_with(note, &p, speed, hardness, 1, usize::MAX);
    }

    /// A pluck with a given spec on any note (a finger inside the piano). Only the
    /// first `strings` strings of the unison are plucked; `harmonic` n lets only the
    /// multiples of n through.
    pub fn pluck_with(&mut self, note: &Note, p: &crate::physics::PluckSpec, speed: f64, hardness: f64, harmonic: usize, strings: usize) {
        let (speed, push) = if p.bar && self.restrike { self.mallet_meets(note, p, speed, hardness) } else { (speed, 1.0) };
        let steps = speed.max(1e-3).log2();
        let f = &note.f_mean;
        let (halo_from, halo) = (note.halo_from, self.halo_gain);
        let gain = self.strike_gain * push;
        // a string is released from a displacement (x = A·cos ωt); a struck bar starts from rest
        // with a velocity (x = A·sin ωt: Re(−jA·e^{jωt})): released all at once from their
        // amplitudes, the bar's modes summed to a step at the strike, a click
        let amp = |i: usize, j: usize| {
            if i >= strings || (j + 1) % harmonic.max(1) != 0 {
                return Complex64::new(0.0, 0.0);
            }
            let a = crate::physics::pluck_amplitude((j + 1) as f64, f[j], p, steps, hardness) * (if j >= halo_from { halo } else { 1.0 }) * gain;
            if p.bar { Complex64::new(0.0, -a) } else { Complex64::new(a, 0.0) }
        };
        self.sync();
        add_modes([&mut self.vert, &mut self.horiz, &mut self.tube], &self.harm, note, &amp, self.kn);
        let real = |c: Complex64| if p.bar { -c.im } else { c.re };
        self.peak = (0..self.kn).map(|j| real(amp(0, j)) * note.bridge[j]).sum::<f64>().abs() * note.s as f64;
        if p.bar && self.membrane_ref > 0.0 {
            self.membrane_strike(swing(&|j| real(amp(0, j)), note, self.kn) / self.membrane_ref);
        }
        self.active = true;
        self.own = true;
    }

    /// Live re-strike on a bar: the mallet meets the swinging bar. The bar's modes are its velocity
    /// at the strike (Σ ω·a): a strike from rest at speed v gives the surface c·v, c = (1 + e)·m /
    /// (m + M) (M the modal mass), which maps the running modes to its displacement and velocity.
    /// The mallet arrives at the rest plane now: a bar on its side meets it now; one swung away it
    /// chases (at most MALLET_CHASE_S) and meets where it catches up, at the relative speed
    /// u = v − ẏ. The collision changes the surface's velocity by c·u = c·v − c·ẏ: the running modes
    /// lose c of their velocity there (`Modes::brake`), and today's strike at v pushes, as bright as
    /// one at u (a harder, shorter contact into a bar coming towards it). Pushed at u instead, a soft
    /// roll into a light treble bar ran away: the recipe puts a bright strike into the upper modes
    /// and left the swinging fundamental unslowed. Returns (u for the brightness, the push's gain
    /// re today's strike at u); from rest (`speed`, 1).
    fn mallet_meets(&mut self, note: &Note, p: &crate::physics::PluckSpec, speed: f64, hardness: f64) -> (f64, f64) {
        if self.vert.n == 0 && self.tube.n == 0 {
            return (speed, 1.0);
        }
        // the surface's velocity Σ ω·a of a strike from rest at speed u (as this one would play)
        let (kn, gain) = (self.kn, self.strike_gain);
        let surface = |u: f64| -> f64 {
            let steps = u.max(1e-3).log2();
            (0..kn).map(|j| note.w[j] * crate::physics::pluck_amplitude((j + 1) as f64, note.f_mean[j], p, steps, hardness)).sum::<f64>()
                * gain
        };
        let own = surface(speed);
        if !(own > 0.0) {
            return (speed, 1.0);
        }
        let c = (1.0 + MALLET_REBOUND) * MALLET_KG / (MALLET_KG + note.modal_mass);
        let k = c * speed / own;
        let mut at_step = 0;
        self.sync();
        self.vert.ring_start();
        self.tube.ring_start();
        let at = |v: &Modes, t: &Modes, w: &[Complex64], tw: &[Complex64]| {
            let (x1, v1) = v.ring_sum(w, false);
            let (x2, v2) = t.ring_sum(tw, true);
            (k * (x1 + x2).re, k * (v1 + v2).re)
        };
        let n = (MALLET_CHASE_S / self.dt) as usize;
        let mut meet = speed;
        for t in 0..=n {
            let (y, dy) = at(&self.vert, &self.tube, &self.ring_w, &self.tube_w);
            if speed * self.dt * t as f64 >= y || t == n {
                meet = speed - dy;
                at_step = t;
                break;
            }
            self.vert.ring_step();
            self.tube.ring_step();
        }
        self.vert.brake(c, at_step, false);
        self.tube.brake(c, at_step, true);
        let u = meet.clamp(MALLET_GRAZE * speed, MALLET_REACH * speed);
        self.mallet_u = u;
        (u, own / surface(u))
    }

    /// The level correction of the next strike or pluck (linear; 1: none).
    pub fn set_strike_gain(&mut self, g: f64) {
        self.strike_gain = g;
    }

    /// The next strike's impact on the board: × its felt force (0: none).
    pub fn set_impact(&mut self, g: f64) {
        self.impact_next = g;
    }

    /// How much the next strikes vary (0 none, 1 as measured): the strike point, the unison's
    /// strings' timing and felt shares drawn per strike (`STRIKE_*`).
    /// A re-strike's felt meets the strings where they ring, a bar's mallet the bar where it swings
    /// (on), or as from rest (off: the strikes as before, bit for bit), from the next strike.
    pub fn set_restrike(&mut self, on: bool) {
        self.restrike = on;
    }

    pub fn set_variation(&mut self, v: f64) {
        self.variation = if v.is_finite() { v.max(0.0) } else { 0.0 };
    }

    /// The impact's lowpass (the instrument's, at this voice's rate).
    pub fn set_impact_filter(&mut self, f: ImpactFilter) {
        self.impact_lp = f;
    }

    /// The impact's lowpass ringing out after the last contact, onto `out_v`.
    fn impact_ring(&mut self, out_v: &mut [f64]) {
        for o in out_v.iter_mut() {
            if self.impact_tail == 0 {
                self.impact_lp.reset();
                return;
            }
            *o += self.impact_lp.tick(0.0);
            self.impact_tail -= 1;
        }
    }

    /// Set the strings going as the note's instrument does: a hammer strike or a pluck.
    pub fn excite(&mut self, note: &Note, speed: f64, hardness: f64, dp: f64, miss: f64) {
        self.excite_with(note, speed, hardness, dp, miss, 1);
    }

    /// The same with a finger touching the string at the node of harmonic `harmonic` (1: none).
    pub fn excite_with(&mut self, note: &Note, speed: f64, hardness: f64, dp: f64, miss: f64, harmonic: usize) {
        self.set_harmonic(harmonic);
        if note.pluck.is_some() {
            self.pluck_with(note, &note.pluck.unwrap(), speed, hardness, harmonic, usize::MAX);
        } else {
            self.strike(note, speed, hardness, dp, miss);
        }
    }

    /// The pedal lifts this key's damper: the strings leave it moving, from rest (`Pedal`):
    /// x_i = gain · n^−rolloff · ±w_i · sin ωt, the unison's strings against each other (real
    /// weights w summing to zero, (S−1, −1, …)/√(S−1)), i.e. its slowly decaying modes (forces
    /// cancelling at the bridge), as the recorded ring shows; a random sign per partial. From
    /// rest the bridge force starts at zero: kicked from a displacement (phases e^{j(φ_n +
    /// 2π·i/S)}) it jumped at the lift, a click once the kick reached the low bass's single strings.
    pub fn kick(&mut self, note: &Note, gain: f64, rolloff: f64, seed: u64) {
        // partials above −80 dB after the rolloff only (n^−8: three), the kick is in the
        // audio thread at every damper lift: their signs on the stack (no allocation; the draws
        // past them were never used)
        let kn = (1..=self.kn.min(KICK_MAX)).take_while(|&n| (n as f64).powf(-rolloff) >= 1e-4).count().max(1);
        let mut rng = crate::noise::Rng::new(seed);
        let mut sign = [0.0f64; KICK_MAX];
        for v in sign.iter_mut().take(kn.min(note.k)) {
            *v = if rng.uniform01() < 0.5 { -1.0 } else { 1.0 };
        }
        let s = note.s as f64;
        let w = |i: usize| if note.s == 1 { 1.0 } else if i == 0 { (s - 1.0).sqrt() } else { -1.0 / (s - 1.0).sqrt() };
        // sine phase: Re(−jA·e^{jωt}) = A·sin ωt
        let amp = |i: usize, j: usize| Complex64::new(0.0, -gain * ((j + 1) as f64).powf(-rolloff) * sign[j] * w(i));
        self.sync();
        self.vert.add(note, &note.vert, &amp, 1.0, kn);
        self.tube.add_tube(note, &note.vert, amp(0, 0), &self.harm);
        self.active = true;
    }

    /// Render acc.len() samples: modes into the lane accumulators (a bar's resonator into
    /// `acc_t`), the contact phase into `out_v`/`out_h` (bridge forces).
    pub fn render(&mut self, note: &Note, acc_v: &mut [Lane], acc_h: &mut [Lane], acc_t: &mut [Lane], out_v: &mut [f64], out_h: &mut [f64]) {
        let n = acc_v.len();
        let mut t0 = 0;
        // the samples after the last contact (the impact's lowpass rings on there)
        let mut free_from = 0;
        if self.contacts.iter().any(|c| c.live) {
            free_from = n;
            if self.ring_on {
                // from this render's first sample, at the damper's engagement now
                self.vert.ring_start();
            }
            for t in 0..n {
                let mut finished = false;
                let mut restarted = false;
                let (mut impact, mut impacting) = (0.0, false);
                for ci in 0..self.contacts.len() {
                    if !self.contacts[ci].live {
                        continue;
                    }
                    if self.ring_on {
                        let c = &mut self.contacts[ci];
                        self.vert.ring_at(&self.ring_w, note.s, &c.phi, &mut c.ring);
                        c.ring.iter_mut().for_each(|x| *x /= self.mode_level);
                    }
                    let (out, felt, done) = self.contacts[ci].step(note, &self.coeffs, self.kn);
                    let c = &self.contacts[ci];
                    if c.impact != 0.0 {
                        impact += c.impact * c.level * felt;
                        impacting = true;
                    }
                    let out = out * self.contacts[ci].level;
                    out_v[t] += out;
                    out_h[t] += note.horiz_gain * out;
                    self.peak = self.peak.max(out.abs());
                    if done {
                        if !finished {
                            // the running modes output this sample before the new ones join
                            self.vert.run(&mut acc_v[t0..=t]);
                            self.horiz.run(&mut acc_h[t0..=t]);
                            self.tube.run(&mut acc_t[t0..=t]);
                            t0 = t + 1;
                            self.sync();
                            finished = true;
                        }
                        let (c, coeffs) = (&self.contacts[ci], &self.coeffs);
                        let amp = |i: usize, j: usize| c.amplitude(coeffs, i * note.k + j) * c.level;
                        add_modes([&mut self.vert, &mut self.horiz, &mut self.tube], &self.harm, note, &amp, self.kn);
                        if c.level > 0.0 {
                            self.mode_level = c.level;
                        }
                        self.contacts[ci].live = false;
                        if self.ring_on {
                            // a contact still running follows the modes with this strike's joined
                            // (synchronized to the next sample)
                            self.vert.ring_start();
                            restarted = true;
                        }
                    }
                }
                if self.ring_on && !restarted {
                    self.vert.ring_step();
                }
                if impacting {
                    out_v[t] += self.impact_lp.tick(impact);
                    self.impact_tail = (IMPACT_TAIL_S / self.dt) as usize;
                }
                if !self.contacts.iter().any(|c| c.live) {
                    free_from = t + 1;
                    self.ring_on = false;
                    break;
                }
            }
        }
        if self.impact_tail > 0 {
            self.impact_ring(&mut out_v[free_from..n]);
        }
        self.vert.run(&mut acc_v[t0..]);
        self.horiz.run(&mut acc_h[t0..]);
        self.tube.run(&mut acc_t[t0..]);
    }

    fn sync(&mut self) {
        self.vert.sync();
        self.horiz.sync();
        self.tube.sync();
    }


    /// Hammer grooves (amount 0…1) on this sub-block's bridge force, after `render`.
    pub fn grooves(&mut self, amount: f64, out_v: &mut [f64]) {
        if amount <= 0.0 || self.groove_n >= self.groove_len {
            return;
        }
        let g = amount * GROOVE_LEVEL * self.peak;
        for o in out_v.iter_mut() {
            if self.groove_n >= self.groove_len {
                break;
            }
            let t = self.groove_n as f64 * self.dt;
            let env = (-(t - 0.001).max(0.0) / 0.002).exp();
            *o += g * env * self.groove_hp.tick(self.rng.gauss());
            self.groove_n += 1;
        }
    }

    /// The damper has landed: its felt chatters for BUZZ_S (if the string was struck).
    pub fn start_buzz(&mut self) {
        if self.active && self.peak > 0.0 {
            self.buzz_n = 0;
            self.buzz_len = (BUZZ_S / self.dt) as usize;
            self.buzz_tau = BUZZ_TAU_S;
            self.buzz_hp.reset();
        }
    }

    /// Paper or a loose part on the strings: the buzz goes on for the whole note.
    pub fn start_paper(&mut self) {
        self.buzz_n = 0;
        self.buzz_len = (20.0 / self.dt) as usize;
        self.buzz_tau = 3.0;
        self.buzz_hp.reset();
    }

    /// A finger touching the string at the node of harmonic n: only the partials that are
    /// multiples of n move at the next strike (n ≤ 1: all of them).
    pub fn set_harmonic(&mut self, n: usize) {
        self.harmonic = n.max(1);
    }

    pub fn buzzing(&self) -> bool {
        self.buzz_n < self.buzz_len
    }

    /// Damper buzz (amount 0…1) from this voice's own bridge force `v`, into `out`.
    pub fn buzz(&mut self, amount: f64, v: &[f64], out: &mut [f64]) {
        let th = BUZZ_TH * self.peak;
        for (o, &x) in out.iter_mut().zip(v) {
            if self.buzz_n >= self.buzz_len {
                break;
            }
            let t = self.buzz_n as f64 * self.dt;
            let dead = x.signum() * (x.abs() - th).max(0.0);
            *o += amount * BUZZ_LEVEL * (-t / self.buzz_tau).exp() * self.buzz_hp.tick(dead);
            self.buzz_n += 1;
        }
    }

    /// How strongly the other tines ring along at the next strike (`Physics::halo_from`; 1 as fitted).
    pub fn set_halo(&mut self, gain: f64) {
        self.halo_gain = gain.max(0.0);
    }

    /// How strongly the fundamental's harmonics start at the next strike (1 as fitted).
    pub fn set_harmonics_gain(&mut self, gain: f64) {
        self.tube.harm_gain = gain.max(0.0);
    }

    /// How strongly the membrane buzzes at the next strike for the same swing (the mallet's
    /// `Mallet::buzz`; 1 as fitted).
    pub fn set_buzz_gain(&mut self, gain: f64) {
        self.membrane_mallet = gain.max(0.0);
    }

    /// The membrane's filters (`Membrane`) at this voice's rate, and its reference: this key's
    /// strike at speed 1 with its own excitation (`swing`).
    pub fn set_membrane(&mut self, m: &Membrane, note: &Note) {
        let sr = 1.0 / self.dt;
        let hp = if m.hp_ratio > 0.0 { m.hp_ratio * note.f_mean[0] } else { m.hp_hz };
        self.membrane_hp = HighPass::new(hp.min(0.45 * sr), sr);
        self.membrane_poles = m.poles.clamp(1, MEMBRANE_POLES);
        self.membrane_spec = (m.lp_hz, m.growth, m.tilt);
        self.membrane_power = m.power;
        self.membrane_ref = note.pluck.map_or(0.0, |p| swing(&|j| crate::physics::pluck_amplitude((j + 1) as f64, note.f_mean[j], &p, 0.0, 1.0), note, self.kn));
        self.membrane_strike(1.0);
    }

    /// The membrane at a strike `level` × this key's strike at speed 1 (`membrane_ref`): its gain
    /// and lowpass (`Membrane`): how hard the bar swings, whichever mallet; the gain × the mallet's
    /// (`set_buzz_gain`). No-op without a membrane.
    fn membrane_strike(&mut self, level: f64) {
        let (lp, growth, tilt) = self.membrane_spec;
        if lp <= 0.0 {
            return;
        }
        let s = level.max(1e-3);
        let fc = (lp * s.powf(tilt)).min(0.45 / self.dt);
        self.membrane_a.1 = 1.0 - (-2.0 * std::f64::consts::PI * fc * self.dt).exp();
        self.membrane_gain = s.powf(growth) * self.membrane_mallet;
        self.membrane_peak = 0.0;
    }

    /// A membrane's buzz from this voice's own bridge force `v` (the bar, its resonator's
    /// fundamental included): shaped noise × the force's rectified level × `level`, into `out`.
    pub fn membrane(&mut self, level: f64, v: &[f64], out: &mut [f64]) {
        let (ae, al) = self.membrane_a;
        let k = self.membrane_poles;
        for (o, &x) in out.iter_mut().zip(v) {
            self.membrane_env += ae * (x.abs() - self.membrane_env);
            let mut n = self.membrane_hp.tick(self.membrane_scale * self.rng.gauss());
            for y in &mut self.membrane_lp[..k] {
                *y += al * (n - *y);
                n = *y;
            }
            let mut env = self.membrane_env;
            if self.membrane_power != 1.0 {
                self.membrane_peak = self.membrane_peak.max(env);
                if self.membrane_peak > 0.0 {
                    env *= (env / self.membrane_peak).powf(self.membrane_power - 1.0);
                }
            }
            *o += level * self.membrane_gain * env * n;
        }
    }

    /// Phantom partials from this voice's own bridge force `v`: level·v²/f_ref, into `out`.
    pub fn phantom(&mut self, level: f64, f_ref: f64, v: &[f64], out: &mut [f64]) {
        for (o, &x) in out.iter_mut().zip(v) {
            *o += level * self.phantom_hp.tick(x * x / f_ref);
        }
    }

    /// At an engine block boundary: synchronize, drop decayed modes, deactivate when silent.
    pub fn end_block(&mut self) {
        self.sync();
        if self.vert.runaway() || self.horiz.runaway() || self.tube.runaway() {
            self.silence();
            self.runaways += 1;
            return;
        }
        self.cull_tick += 1;
        if self.cull_tick >= CULL_EVERY {
            self.cull_tick = 0;
            self.vert.cull();
            self.horiz.cull();
            self.tube.cull();
        }
        if self.vert.n == 0 && self.horiz.n == 0 && self.tube.n == 0 && !self.contacts.iter().any(|c| c.live) && !self.buzzing() {
            self.active = false;
            self.cull_tick = self.cull_phase;
        }
    }

    /// Silence (panic): no modes, no contact, no buzz or grooves left.
    pub fn silence(&mut self) {
        self.cull_tick = self.cull_phase;
        self.vert.clear();
        self.horiz.clear();
        self.tube.clear();
        self.contacts.iter_mut().for_each(|c| c.live = false);
        self.impact_tail = 0;
        self.impact_lp.reset();
        self.groove_n = self.groove_len;
        self.buzz_n = self.buzz_len;
        self.groove_hp.reset();
        self.buzz_hp.reset();
        self.phantom_hp.reset();
        self.peak = 0.0;
        self.active = false;
        self.own = false;
    }

    pub fn dt(&self) -> f64 {
        self.dt
    }

    /// The strongest modes (frequency Hz, decay rate σ in 1/s, amplitude: the modulus of
    /// the mode's state, model units), strongest first, both polarizations; read-only.
    /// Returns (partials listed, level = the root of the sum of squares over all live
    /// modes, live modes). The voice must be synchronized: call after `end_block`.
    pub fn partials(&self, out: &mut [Partial; MAX_PARTIALS]) -> (usize, f32, usize) {
        let (mut n, mut sumsq, mut modes) = (0usize, 0.0f64, 0usize);
        for (pol, m) in [(0, &self.vert), (KIND_HORIZ, &self.horiz), (0, &self.tube)] {
            for p in 0..m.n {
                let id = m.id[p] as usize;
                let lam = m.lam_at(id);
                let amp2 = m.exact[p].norm_sqr();
                sumsq += amp2;
                modes += 1;
                let kind = pol | if id >= m.n_base { KIND_COPY } else { 0 };
                insert_partial(out, &mut n, Partial {
                    freq: (lam.im / std::f64::consts::TAU) as f32,
                    decay: (-lam.re) as f32,
                    amp: amp2 as f32, // squared for now: the order is the same, the root is taken below
                    kind,
                });
            }
        }
        for p in &mut out[..n] {
            p.amp = (p.amp as f64).sqrt() as f32;
        }
        (n, sumsq.sqrt() as f32, modes)
    }

    /// The strings' displacement per partial, squared, added into `out` (m²: the vertical modes'
    /// moduli over their bridge weights, summed in power over the unison's eigenmodes; the
    /// visualization). The voice must be synchronized: call after `end_block`.
    pub fn add_shape(&self, note: &Note, out: &mut [f32]) {
        let m = &self.vert;
        for p in 0..m.n {
            let id = m.id[p] as usize;
            if id >= m.n_base {
                continue;
            }
            let j = id / note.s;
            if j < out.len() && note.bridge[j] != 0.0 {
                out[j] += (m.exact[p].norm_sqr() / (note.bridge[j] * note.bridge[j])) as f32;
            }
        }
    }

    pub fn modes(&self) -> usize {
        self.vert.n + self.horiz.n + self.tube.n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_ratio_counts_the_core() {
        // the felt's core stiffens the contact: it never gets fewer sub-steps than without one
        let Ok(inst) = crate::Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")) else { return };
        let mut note = inst.notes[15].clone();
        note.phys.core_k.iter_mut().for_each(|k| *k = 0.0);     // C2 without a core, whatever the export holds
        let mut cored = note.clone();
        cored.phys.core_k = note.felt_k.iter().map(|k| k * 5.0).collect();
        cored.phys.core_depth = 4e-4;
        let dt = 1.0 / 48000.0;
        let plain = contact_ratio(&note, note.k, 5.0, 1.0, note.felt_p, dt);
        assert!(contact_ratio(&cored, cored.k, 5.0, 1.0, cored.felt_p, dt) > plain);
    }

    /// Strike `v1`, then `v2` `gap` samples later (re-strike `on` or off); per sample the output
    /// (modes + contact) and the second strike's largest felt force.
    fn two_strikes(inst: &crate::Instrument, key: usize, v1: f64, v2: Option<f64>, gap: usize, on: bool) -> (Vec<f64>, f64) {
        let note = &inst.notes[key];
        let mut v = Voice::new(note, 48000.0, inst.reference_sr, inst.contact_max_s, inst.contact_separation_s, 64);
        v.set_engage(0.0);
        v.set_restrike(on);
        let (mut av, mut ah, mut at) = (vec![[0.0f32; LANES]; 1], vec![[0.0f32; LANES]; 1], vec![[0.0f32; LANES]; 1]);
        let (mut ov, mut oh) = (vec![0.0f64; 1], vec![0.0f64; 1]);
        let (mut out, mut peak) = (Vec::new(), 0.0f64);
        v.strike(note, v1, 1.0, 0.0, 0.0);
        for t in 0..gap + 4800 {
            if let (true, Some(v2)) = (t == gap, v2) {
                v.strike(note, v2, 1.0, 0.0, 0.0);
            }
            for x in [&mut av, &mut ah, &mut at] {
                x[0] = [0.0; LANES];
            }
            ov[0] = 0.0;
            oh[0] = 0.0;
            v.render(note, &mut av, &mut ah, &mut at, &mut ov, &mut oh);
            out.push(av[0].iter().map(|&x| x as f64).sum::<f64>() + ov[0]);
            if t >= gap {
                peak = peak.max(v.contacts.iter().filter(|c| c.live).map(|c| c.force.iter().sum::<f64>()).fold(0.0, f64::max));
            }
        }
        (out, peak)
    }

    #[test]
    fn a_restrike_meets_the_ringing_strings() {
        // Grand A's C4, ff then pp 100 ms later at 16 phases over a period of the fundamental: off,
        // every second strike's felt force as from rest; on, the string at the strike point moves
        // up to several times faster than a pp hammer (the prototype's measure: peak force 1.2–7.7×)
        let Ok(inst) = crate::Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")) else { return };
        let key = 60 - 21;
        let period = (48000.0 / 261.6) as usize;
        let (_, fresh) = two_strikes(&inst, key, 0.4, None, 0, false);
        let mut ratios = Vec::new();
        for ph in 0..16 {
            let gap = 4800 + ph * period / 16;
            let (_, off) = two_strikes(&inst, key, 5.0, Some(0.4), gap, false);
            let (_, live) = two_strikes(&inst, key, 5.0, Some(0.4), gap, true);
            assert!((off - fresh).abs() < 1e-9 * fresh, "off: as from rest ({off} vs {fresh})");
            ratios.push(live / fresh);
        }
        let (lo, hi) = (ratios.iter().cloned().fold(f64::INFINITY, f64::min), ratios.iter().cloned().fold(0.0, f64::max));
        eprintln!("pp into ff, peak felt force re a fresh pp: {lo:.2} … {hi:.2}");
        assert!(hi > 2.0 && lo > 0.5, "{ratios:?}");
    }

    /// A bar struck at `v1`, then at `v2` `gap` samples later (Live re-strike `on`): the output
    /// per sample and the speed the second strike met the bar at.
    fn two_mallets(inst: &crate::Instrument, key: usize, v1: f64, v2: Option<f64>, gap: usize, on: bool) -> (Vec<f64>, f64) {
        let note = &inst.notes[key];
        let crate::physics::Excitation::Pluck(p) = note.spec.excitation.clone() else { panic!("a bar") };
        let mut v = Voice::new(note, 48000.0, inst.reference_sr, inst.contact_max_s, inst.contact_separation_s, 64);
        v.set_engage(0.0);
        v.set_restrike(on);
        let (mut av, mut ah, mut at) = (vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64]);
        let (mut ov, mut oh) = (vec![0.0f64; 64], vec![0.0f64; 64]);
        let (mut out, mut met) = (Vec::new(), 0.0);
        v.pluck_with(note, &p, v1, 1.0, 1, usize::MAX);
        for b in 0..(gap + 9600) / 64 {
            if let (true, Some(v2)) = (b * 64 == gap, v2) {
                v.pluck_with(note, &p, v2, 1.0, 1, usize::MAX);
                met = if on { v.mallet_u } else { v2 };
            }
            for x in [&mut av, &mut ah, &mut at] {
                x.iter_mut().for_each(|l| *l = [0.0; LANES]);
            }
            ov.fill(0.0);
            oh.fill(0.0);
            v.render(note, &mut av, &mut ah, &mut at, &mut ov, &mut oh);
            out.extend((0..64).map(|t| av[t].iter().chain(&at[t]).map(|&x| x as f64).sum::<f64>()));
        }
        (out, met)
    }

    /// A bar struck `n` times at `speed`, `every` samples apart, the damper lifted: the largest
    /// output over each second.
    fn mallet_roll(inst: &crate::Instrument, key: usize, speeds: &[f64], every: usize, n: usize, on: bool) -> Vec<f64> {
        let note = &inst.notes[key];
        let crate::physics::Excitation::Pluck(p) = note.spec.excitation.clone() else { panic!("a bar") };
        let mut v = Voice::new(note, 48000.0, inst.reference_sr, inst.contact_max_s, inst.contact_separation_s, 64);
        v.set_engage(0.0);
        v.set_restrike(on);
        let (mut av, mut ah, mut at) = (vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64]);
        let (mut ov, mut oh) = (vec![0.0f64; 64], vec![0.0f64; 64]);
        let mut peaks = vec![0.0f64; n * every / 48000];
        for b in 0..n * every / 64 {
            if (b * 64) % every == 0 {
                v.pluck_with(note, &p, speeds[(b * 64 / every) % speeds.len()], 1.0, 1, usize::MAX);
            }
            for x in [&mut av, &mut ah, &mut at] {
                x.iter_mut().for_each(|l| *l = [0.0; LANES]);
            }
            ov.fill(0.0);
            oh.fill(0.0);
            v.render(note, &mut av, &mut ah, &mut at, &mut ov, &mut oh);
            for t in 0..64 {
                let y: f64 = av[t].iter().chain(&at[t]).map(|&x| x as f64).sum();
                if let Some(m) = peaks.get_mut((b * 64 + t) / 48000) {
                    *m = m.max(y.abs());
                }
            }
        }
        peaks
    }

    #[test]
    fn a_mallet_roll_stays_bounded() {
        // bars struck 12 and 20 times a second for 8 s, the dampers up (the sustain latch): ff alone,
        // pp alone, pp and ff in turn (an arpeggiator's accents); with Live re-strike the bar's level
        // stays near the roll's without it (the treble's bars weigh about what a mallet does)
        let mut worst = Vec::new();
        for name in ["vibraphone", "marimba", "xylophone", "glockenspiel", "bells", "crotales"] {
            let Ok(inst) = crate::Instrument::load(format!("{}/../instruments/{name}", env!("CARGO_MANIFEST_DIR"))) else { return };
            let [pp, _, ff] = inst.velocity_speed;
            for key in [48 - 21, 72 - 21, 96 - 21, 107 - 21] {
                for speeds in [vec![ff], vec![pp], vec![pp, ff], vec![pp, pp, ff]] {
                    for every in [4000, 2400] {
                        let off = mallet_roll(&inst, key, &speeds, every, 8 * 48000 / every, false);
                        let on = mallet_roll(&inst, key, &speeds, every, 8 * 48000 / every, true);
                        let db = on.iter().zip(&off).map(|(a, b)| 20.0 * (a / b).log10()).fold(f64::NEG_INFINITY, f64::max);
                        if !(db < 6.0) {
                            worst.push(format!("{name} {} {speeds:?} every {every}: {db:.1} dB", key + 21));
                        }
                    }
                }
            }
        }
        assert!(worst.is_empty(), "{}", worst.join("\n"));
    }

    #[test]
    fn a_mallet_meets_the_swinging_bar() {
        // the marimba's C4, ff then pp 100 ms later at 16 points over 5.6 periods: the pp mallet meets
        // the bar at other speeds, faster where it swings towards it (0.54–1.58×: by then the upper
        // modes, which carry most of the surface's speed after a strike, have decayed); from rest,
        // the strike as before
        let Ok(inst) = crate::Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/marimba")) else { return };
        let key = 60 - 21;
        let (a, _) = two_mallets(&inst, key, 1.0, None, 4800, false);
        let (b, _) = two_mallets(&inst, key, 1.0, None, 4800, true);
        assert_eq!(a, b, "from rest: today's strike");
        let mut met = Vec::new();
        for ph in 0..16 {
            let gap = 4800 + ph * 64;
            let (_, m) = two_mallets(&inst, key, 2.1, Some(0.39), gap, true);
            met.push(m / 0.39);
        }
        let (lo, hi) = (met.iter().cloned().fold(f64::INFINITY, f64::min), met.iter().cloned().fold(0.0, f64::max));
        eprintln!("marimba C4, pp into ff: the mallet meets the bar at {lo:.2} … {hi:.2} × its own speed");
        assert!(hi > 1.15 && lo < 0.85, "{met:?}");
    }

    #[test]
    fn the_guard_silences_a_runaway() {
        // a mode's state far past the strikes', infinite or not a number: the voice is silenced at
        // the block's end and counted; a voice ringing as struck is left alone
        let Ok(inst) = crate::Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")) else { return };
        let note = &inst.notes[39];
        for bad in [2.0, f64::INFINITY, f64::NAN] {
            let mut v = Voice::new(note, 48000.0, inst.reference_sr, inst.contact_max_s, inst.contact_separation_s, 64);
            v.set_engage(0.0);
            v.strike(note, 1.5, 1.0, 0.0, 0.0);
            let (mut av, mut ah, mut at) = (vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64]);
            let (mut ov, mut oh) = (vec![0.0f64; 64], vec![0.0f64; 64]);
            for _ in 0..20 {
                v.render(note, &mut av, &mut ah, &mut at, &mut ov, &mut oh);
                v.end_block();
            }
            assert!(v.active && v.runaways == 0, "ringing as struck");
            let p = 0;
            // twice the ceiling, or not finite
            v.vert.exact[p] = if bad.is_finite() { v.vert.exact[p] * (2.0 * v.vert.ceiling / v.vert.exact[p].norm()) } else { Complex64::new(bad, 0.0) };
            v.vert.set_work(p, v.vert.exact[p]);
            v.render(note, &mut av, &mut ah, &mut at, &mut ov, &mut oh);
            v.end_block();
            assert!(!v.active && v.runaways == 1 && v.modes() == 0, "{bad}: silenced");
        }
    }

    #[test]
    fn a_restrike_from_silence_is_a_strike() {
        // on, a strike into silent strings is today's, bit for bit
        let Ok(inst) = crate::Instrument::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")) else { return };
        let (a, _) = two_strikes(&inst, 39, 1.5, None, 4800, false);
        let (b, _) = two_strikes(&inst, 39, 1.5, None, 4800, true);
        assert_eq!(a, b);
    }
}
