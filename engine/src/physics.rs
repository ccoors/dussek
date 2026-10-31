//! The physics compiler: from a note's physical parameters (string, hammer, damper; the
//! scale design of `proto/scale.py`) and the instrument's soundboard to everything the
//! voice runs: partials, losses, bridge coupling, the coupled unison modes of both
//! polarizations, the sympathetic resonators. Mirrors `proto/string_model.py`
//! (`mode_table`, `coupled_eigenmodes`), `proto/soundboard.py` (`coupled_modes`,
//! `correction_db`), `proto/board.py` (`admittance`), `proto/resonance.py`
//! (`note_modes`) and `proto/damper.py` (tabulated), so it can run at any sample rate and
//! for runtime changes of the parameters.

use std::f64::consts::PI;

use realfft::num_complex::Complex64;

use crate::noise::interp;

type C = Complex64;

/// A stiff string (per unison string where it differs), hinged at both ends.
#[derive(Clone, Debug)]
pub struct StringSpec {
    /// fundamental (Hz) and inharmonicity: f_k = k·f0·√(1 + B·k²)
    pub f0: f64,
    pub b: f64,
    /// speaking length (m), tension per string (N), mass of one string (kg)
    pub l: f64,
    pub t: f64,
    pub mass: f64,
    /// unison detuning per string (cents)
    pub detune_cents: Vec<f64>,
    /// internal loss σ = b1 + b3·ω_ref²·(ω/ω_ref)^q (1/s; `string_loss`)
    pub b1: f64,
    pub b3: f64,
    pub loss_q: f64,
    /// bridge coupling per string (1/s) × normalized admittance, reactive part re real
    pub kappa: f64,
    pub kappa_reactive: f64,
    /// horizontal polarization: excitation re vertical, bridge coupling, detuning (cents)
    pub horiz_gain: f64,
    pub horiz_kappa: f64,
    pub horiz_detune_cents: f64,
    pub f_max: f64,
    /// irregular strings (fitted): each partial of each unison string off by
    /// scatter_cents·√k·`string_gauss` cents (`string_model.NoteParams.scatter_cents`)
    pub scatter_cents: f64,
    /// a bar instead of a string (None: the string's partials): its modes
    pub modes: Option<BarModes>,
}

/// A struck bar's modes (`proto/bars.py`), in place of a string's partials: frequency re f0,
/// loss σ (1/s), weight at the strike point (the contact's φ) and at the output (the
/// bridge force's, as a string partial's T·kπ/L).
#[derive(Clone, Debug, Default)]
pub struct BarModes {
    pub ratio: Vec<f64>,
    pub sigma: Vec<f64>,
    pub phi: Vec<f64>,
    pub out: Vec<f64>,
    pub harmonics: BarHarmonics,
    /// per mode (empty: none) a slow beat, as the false beats: a partner Δf (Hz) off carrying a
    /// share of the mode (0: none); the tubular bells' split pairs
    pub split: Vec<(f64, f64)>,
    /// the modes from this one on are the instrument's other tines ringing along (the kalimba's
    /// halo, `Params::sympathetic` scales them); the mode count: none
    pub halo_from: usize,
}

/// The fundamental's harmonics h·f1 (the bar's nonlinearity, `proto/bars.py`): per harmonic
/// its number, its level re the fundamental (dB) while the fundamental is at the level of
/// an ff strike from rest, how much that level rises per dB of the fundamental's (1: a
/// square law) and its loss (× the fundamental's); with the damper lifted (`free`) and
/// resting on the bar (`damped`, interpolated by the engagement at the strike).
#[derive(Clone, Debug, Default)]
pub struct BarHarmonics {
    pub h: Vec<u32>,
    pub free: Vec<HarmonicLaw>,
    pub damped: Vec<HarmonicLaw>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HarmonicLaw {
    pub level_db: f64,
    pub growth: f64,
    pub decay: f64,
}

/// A bar's resonator: its fundamental (partial 0) and the fundamental's harmonics run in
/// a bank of their own (`voice::Voice`), whose output the vibraphone's motor modulates.
#[derive(Clone, Debug, Default)]
pub struct Tube {
    pub harmonics: BarHarmonics,
    /// the fundamental's state after an ff strike from rest (the harmonics' level
    /// reference; `Instrument::compile`)
    pub a_ref: f64,
}

/// splitmix64 finalizer (`string_model._mix64`).
fn mix64(x: u64) -> u64 {
    let mut x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Standard Gaussian per (note, string, partial), the prototype's draw
/// (`string_model.string_gauss`): Box–Muller on two hashed uniforms.
pub fn string_gauss(note: u8, string: usize, k: usize) -> f64 {
    let h1 = mix64(((note as u64) << 24) | ((string as u64) << 20) | k as u64);
    let h2 = mix64(h1);
    let u1 = ((h1 >> 11) + 1) as f64 * 2f64.powi(-53);
    let u2 = (h2 >> 11) as f64 * 2f64.powi(-53);
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

#[derive(Clone, Debug)]
pub struct HammerSpec {
    pub mass: f64,
    /// felt force K·compression^p, per string × balance
    pub felt_k: f64,
    pub felt_p: f64,
    pub balance: Vec<f64>,
    /// strike position re the speaking length
    pub strike_pos: f64,
    /// the felt's core: core_k·(compression − core_depth)^p on top of the felt, per string × balance
    /// (0: none; `felt_force`)
    pub core_k: f64,
    pub core_depth: f64,
}

/// The felt's force (N) at the compression `cmp` (m; ≤ 0: no contact): k·δ^p, plus its core
/// core_k·max(δ − core_depth, 0)^p, a stiff layer under the soft one (design 2026-10-20-felt-core).
/// Without a core exactly k·δ^p. Prototype: `string_model.felt_force`.
pub fn felt_force(k: f64, p: f64, core_k: f64, core_depth: f64, cmp: f64) -> f64 {
    let f = k * cmp.max(0.0).powf(p);
    if core_k == 0.0 {
        return f;
    }
    f + core_k * (cmp - core_depth).max(0.0).powf(p)
}

/// Pluck: the string released from rest out of a triangle with its apex (height `amp`, m)
/// at `beta` of the speaking length, through the quill lowpass `f_q` (Hz) at the
/// reference velocity; per dynamic step (a doubling of the pluck speed) the level rises
/// `gain_db_per_step` and `f_q` by `fq_oct_per_step` octaves (`proto/string_model.py`).
/// `bar`: a struck bar's modes started directly (`proto/bars.py`): no string shape, each
/// mode at `amp` × its output weight (`BarModes::out`) through the mallet's lowpass of
/// `order` (1: first order, as the quill's). `tangent`: the clavichord's, the pluck's limit
/// β → 0 (the string pushed at the speaking length's end, which the tangent is): 2·amp/(πk),
/// no comb, `beta` unused.
#[derive(Clone, Copy, Debug)]
pub struct PluckSpec {
    pub beta: f64,
    pub f_q: f64,
    pub amp: f64,
    pub gain_db_per_step: f64,
    pub fq_oct_per_step: f64,
    pub bar: bool,
    pub order: f64,
    pub tangent: bool,
}

impl PluckSpec {
    /// A string's pluck (first-order quill lowpass).
    pub const fn string(beta: f64, f_q: f64, amp: f64, gain_db_per_step: f64, fq_oct_per_step: f64) -> Self {
        PluckSpec { beta, f_q, amp, gain_db_per_step, fq_oct_per_step, bar: false, order: 1.0, tangent: false }
    }
}

/// How a note's strings are set going.
#[derive(Clone, Debug)]
pub enum Excitation {
    Hammer(HammerSpec),
    Pluck(PluckSpec),
}

/// Starting displacement of partial k (frequency f) of a plucked string, `steps` dynamic
/// steps from the reference; `hardness` scales the quill lowpass (prototype:
/// `string_model.pluck_amplitudes`).
pub fn pluck_amplitude(k: f64, f: f64, p: &PluckSpec, steps: f64, hardness: f64) -> f64 {
    let fq = p.f_q * hardness * 2f64.powf(p.fq_oct_per_step * steps);
    let shape = if p.bar {
        p.amp
    } else if p.tangent {
        2.0 * p.amp / (PI * k)
    } else {
        2.0 * p.amp / (PI * PI * p.beta * (1.0 - p.beta)) * (k * PI * p.beta).sin() / (k * k)
    };
    let lowpass = if p.order == 1.0 { (1.0 + (f / fq).powi(2)).sqrt().recip() } else { (1.0 + (f / fq).powi(2)).powf(-0.5 * p.order) };
    shape * lowpass * 10f64.powf(p.gain_db_per_step * steps / 20.0)
}

#[derive(Clone, Debug)]
pub struct NoteSpec {
    pub note: u8,
    /// the register (0: the main one); a register's own wear draws
    pub register: u8,
    pub string: StringSpec,
    pub excitation: Excitation,
    pub has_damper: bool,
    /// damper loss (1/s) at log2 f = `Globals::damper_log2`.0 + i·step
    pub damper_sigma: Option<Vec<f64>>,
    /// the buff stop's pads on this string (None: no buff): flat part a and strength g (1/s),
    /// position p (re the speaking length); σ_buff(k) = a + g·sin²(kπp) (`proto/fit_buff.py`)
    pub buff: Option<(f64, f64, f64)>,
    /// the lute row on this string (None: none): its pluck point (re the speaking length)
    /// and its level (dB re the string's own row); plucks instead of the own row while
    /// `Params::lute` is on
    pub lute: Option<(f64, f64)>,
    /// the recorded key's own pitch re the curve the note plays (cents; the export's smoothed
    /// tuning, or equal temperament): added at `Tuning::as_recorded`
    pub recorded_cents: f64,
    /// this key's ring re the fitted one (the project's [sustain] by ear; 1: as fitted), times
    /// `Tuning::sustain` (design log step 154)
    pub sustain: f64,
}

/// ω_ref of the strings' loss law (rad/s)
const OMEGA_REF: f64 = 2.0 * PI * 1000.0;

/// The string's internal loss rate (1/s) at angular frequency w: b1 + b3·ω_ref²·(w/ω_ref)^q
/// (`proto/string_model.py` `string_loss`; q = 2: b1 + b3·ω²).
pub fn string_loss(b1: f64, b3: f64, w: f64, q: f64) -> f64 {
    if q == 2.0 {
        return b1 + b3 * w * w;
    }
    b1 + b3 * OMEGA_REF * OMEGA_REF * (w / OMEGA_REF).powf(q)
}

/// The bridge's mobility along the keyboard (`proto/scale.py` `mobility_factor`; design
/// 2026-10-20-bridge-mobility): a factor per key on Re Y_n, 1 + (M(note) − 1)·L(f), M
/// log-interpolated between the anchors (held beyond the ends), L(f) = 1 / (1 + (f / fc)⁴).
#[derive(Clone, Debug, PartialEq)]
pub struct Mobility {
    pub anchors: Vec<f64>,
    pub factors: Vec<f64>,
    pub fc_hz: f64,
}

impl Mobility {
    pub fn factor(&self, note: f64, f: f64) -> f64 {
        let (a, v) = (&self.anchors, &self.factors);
        let Some(last) = a.len().checked_sub(1) else { return 1.0 };
        let m = if note <= a[0] {
            v[0]
        } else if note >= a[last] {
            v[last]
        } else {
            let i = a.partition_point(|&x| x <= note) - 1;
            let t = (note - a[i]) / (a[i + 1] - a[i]);
            (v[i].ln() + t * (v[i + 1].ln() - v[i].ln())).exp()
        };
        1.0 + (m - 1.0) / (1.0 + (f / self.fc_hz).powi(4))
    }
}

/// The synthetic soundboard's modes (`proto/board.py`).
#[derive(Clone, Debug)]
pub struct Board {
    pub f: Vec<f64>,
    pub sigma: Vec<f64>,
    pub phi: Vec<f64>,
    pub density: f64,
    /// each mode's radiation to the two channels, vertical / horizontal polarization
    pub r: [Vec<f64>; 2],
    pub r_h: [Vec<f64>; 2],
    /// the first of the measured modes (`proto/fit_body_modes.py`; after the random ones),
    /// `f.len()` without them
    pub measured_from: usize,
    /// the share of the measured modes' admittance the horizontal polarization feels
    pub h_share: f64,
    /// the bridge's mobility along the keyboard (None: 1 everywhere)
    pub mobility: Option<Mobility>,
}

impl Board {
    /// Real part of the normalized bridge admittance Re Y_n(f) (mean 1):
    /// Y = Σ φ²·jω/(ω_m² − ω² + 2jωσ_m), normalized by density/4.
    pub fn admittance_re(&self, f: f64) -> f64 {
        self.admittance_re_of(f, 0)
    }

    /// The bridge's mobility at a note and frequency (1 without anchors).
    pub fn mobility(&self, note: f64, f: f64) -> f64 {
        self.mobility.as_ref().map_or(1.0, |m| m.factor(note, f))
    }

    /// The measured modes' part of Re Y_n(f) (0 without them).
    pub fn admittance_re_measured(&self, f: f64) -> f64 {
        self.admittance_re_of(f, self.measured_from)
    }

    fn admittance_re_of(&self, f: f64, from: usize) -> f64 {
        let w = 2.0 * PI * f;
        let mut y = 0.0;
        for ((&fm, &sm), &ph) in self.f.iter().zip(&self.sigma).zip(&self.phi).skip(from) {
            let wm = 2.0 * PI * fm;
            let a = wm * wm - w * w;
            let b = 2.0 * w * sm;
            y += ph * ph * w * b / (a * a + b * b);
        }
        y / (self.density / 4.0)
    }
}

/// Per-note transfer correction C(f, note) in dB on notes × log2 grid (`correction_db`).
#[derive(Clone, Debug)]
pub struct Correction {
    pub notes: Vec<f64>,
    pub grid_log2: Vec<f64>,
    /// row-major notes × grid
    pub c_db: Vec<f64>,
}

impl Correction {
    /// The row of a note (interpolated over notes per grid point).
    fn row(&self, note: f64) -> Vec<f64> {
        let g = self.grid_log2.len();
        (0..g)
            .map(|j| {
                let col: Vec<f64> = (0..self.notes.len()).map(|i| self.c_db[i * g + j]).collect();
                interp(note, &self.notes, &col)
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct Globals {
    pub board: Board,
    pub correction: Option<Correction>,
    /// damper tables: (log2 f of the first point, step)
    pub damper_log2: (f64, f64),
    /// sympathetic resonators up to this frequency (Hz)
    pub sym_f_max: f64,
}

/// Coupled unison system of one polarization, per partial k: eigenvalues `lam[k·S + e]`
/// (1/s), column sums of the eigenvectors `colsum[k·S + e]`, inverse eigenvector matrix
/// `vinv[k·S·S + e·S + i]`, the eigenvectors `v[k·S·S + i·S + e]` (string i's share of mode e:
/// where a re-strike's hammer meets the ringing strings).
#[derive(Clone, Debug, Default)]
pub struct Polarization {
    pub lam: Vec<C>,
    pub colsum: Vec<C>,
    pub vinv: Vec<C>,
    pub v: Vec<C>,
}

/// Sympathetic resonators of one note (unisons merged): frequency (Hz), loss (1/s),
/// bridge coupling (complex, 1/s), extra loss under the damper (1/s), extra loss under the
/// buff's pads (1/s; empty without a buff).
#[derive(Clone, Debug, Default)]
pub struct Sympathetic {
    pub f: Vec<f64>,
    pub sigma: Vec<f64>,
    pub kappa: Vec<C>,
    pub damper_sigma: Vec<f64>,
    pub buff_sigma: Vec<f64>,
}

/// What the voice runs. String arrays `w`, `sigma_int` (S × K, index s·K + k); per
/// partial: `kappa_re`, `phi_h`, `bridge`, `damper_sigma`.
#[derive(Clone, Debug, Default)]
pub struct Physics {
    pub s: usize,
    pub k: usize,
    pub w: Vec<f64>,
    pub sigma_int: Vec<f64>,
    pub kappa_re: Vec<f64>,
    pub phi_h: Vec<f64>,
    /// a hammer on strings: its strike point (re the length) and the length (m), from which a strike's
    /// own point is drawn (`voice::Voice::set_variation`); None for a bar or a pluck
    pub strike: Option<(f64, f64)>,
    pub bridge: Vec<f64>,
    pub modal_mass: f64,
    pub felt_k: Vec<f64>,
    pub felt_p: f64,
    /// the felt's core per string (× balance; zeros: none) and its depth (m)
    pub core_k: Vec<f64>,
    pub core_depth: f64,
    pub hammer_mass: f64,
    pub horiz_gain: f64,
    pub vert: Polarization,
    pub horiz: Polarization,
    pub damper_sigma: Vec<f64>,
    /// per partial: the buff's extra loss (1/s) while its pads rest on the string; empty
    /// without a buff
    pub buff_sigma: Vec<f64>,
    pub sym: Sympathetic,
    /// false beats: (partial, Δω rad/s, share r): the partial's modes get a copy Δω off
    /// carrying r of it (the original 1 − r), in partial order
    pub split: Vec<(usize, f64, f64)>,
    /// the pluck (None: hammer) and the partials' mean frequencies (Hz) it is shaped by
    pub pluck: Option<PluckSpec>,
    pub f_mean: Vec<f64>,
    /// a bar of one string: its fundamental and harmonics run through the resonator's bank (not a
    /// fundamental split by the bar's own pair: it runs with the other modes)
    pub tube: Option<Tube>,
    /// the partials from this one on ring along from the other tines (`BarModes::halo_from`); k: none
    pub halo_from: usize,
}

fn damper_at(spec: &NoteSpec, g: &Globals, f: f64) -> f64 {
    match &spec.damper_sigma {
        Some(tab) if spec.has_damper => {
            let x = (f.max(20.0).log2() - g.damper_log2.0) / g.damper_log2.1;
            let i = x.floor().clamp(0.0, (tab.len() - 2) as f64) as usize;
            let t = (x - i as f64).clamp(0.0, 1.0);
            tab[i] + t * (tab[i + 1] - tab[i])
        }
        _ => 0.0,
    }
}

/// Wear of the strings and the soundboard (the piano's condition, the parts compiled
/// into the strings), amounts 0 (as fitted) … 1 (worn). Per note and string the random
/// draws come from `seed` (one worn piano per seed), the amounts only scale them.
/// No measurements behind the magnitudes: plausible values for a neglected piano.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Wear {
    /// unisons drifted apart: each string ± `unison_cents(amount)`
    pub unisons: f64,
    /// notes drifted: each note ± `tuning_cents(amount)`
    pub tuning: f64,
    /// old strings: partials scattered (± STRING_CENTS·√n per string: false beats),
    /// treble loss b3 × (1 + STRING_LOSS)
    pub strings: f64,
    /// soundboard and bridge: base loss b1 × (1 + BOARD_LOSS)
    pub soundboard: f64,
    /// false beats: irregular strings split partials into two close tones (see `split`)
    pub false_beats: f64,
    pub seed: u64,
}

/// A wooden bar's tuning wear re a string's: its pitch drifts as the wood ages, σ 10 cents at
/// full wear (a quarter of `tuning_cents`; the amount scaled by the cube root)
const WOODEN_BAR_TUNING: f64 = 0.63;

impl Wear {
    /// The wear a body's kind can show (design log step 192). Bars, discs, tubes and tines have no
    /// soundboard, strings or false beats to wear; metal ones hold their pitch, wooden bars drift a
    /// little, a kalimba's tines slide in their bridge as a piano's strings drift.
    pub fn on(self, body: crate::mic::Body) -> Wear {
        use crate::mic::Body;
        if !body.is_bars() {
            return self;
        }
        let tuning = match body {
            Body::Kalimba => self.tuning,
            Body::Marimba | Body::Xylophone | Body::Balafon => self.tuning * WOODEN_BAR_TUNING,
            _ => 0.0,
        };
        Wear { tuning, strings: 0.0, soundboard: 0.0, false_beats: 0.0, ..self }
    }
}

/// How much of the live wear a body's kind can show (design log step 194), 0 … 1 of a piano's: the
/// hammers' hardening, the voicing's per-key hardness and per-key level. A piano's hammers and a
/// harpsichord's quills are voiced key by key; bars are cut and tuned even, and a hand's fingers,
/// a pick or a thumb neither harden nor are voiced: their keys differ in level alone, by half a
/// piano's (old strings, a bar's cord, a tine's seat). A mallet hardens as a hammer does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveWear {
    pub hammers: f64,
    pub voicing_hardness: f64,
    pub voicing_level: f64,
}

impl LiveWear {
    pub fn of(body: crate::mic::Body) -> LiveWear {
        use crate::mic::Body;
        let by_hand = body.is_harp() || body.is_zither() || matches!(body, Body::Guitar | Body::Strumstick | Body::Kalimba);
        match () {
            _ if by_hand => LiveWear { hammers: 0.0, voicing_hardness: 0.0, voicing_level: 0.5 },
            _ if body.is_bars() => LiveWear { hammers: 1.0, voicing_hardness: 0.0, voicing_level: 0.5 },
            _ => LiveWear { hammers: 1.0, voicing_hardness: 1.0, voicing_level: 1.0 },
        }
    }
}

/// Detuning (σ, cents) for a wear amount: steeply nonlinear (by ear: 5 cents is plenty
/// at half wear, the extremes go far beyond a honky-tonk)
pub fn unison_cents(a: f64) -> f64 {
    65.0 * a.powf(3.7)
}

pub fn tuning_cents(a: f64) -> f64 {
    40.0 * a.powi(3)
}

/// False beats at amount a, relative to the strongest approved sketch (at a = 0.55):
/// x = a/0.55; a share min(1, x) of the partials split, the second tone x…5x Hz off
/// (random sign), carrying min(1, x)/2 of the partial.
const FALSE_BEATS_REF: f64 = 0.55;
const STRING_CENTS: f64 = 0.6;
const STRING_LOSS: f64 = 2.0;
const BOARD_LOSS: f64 = 1.5;
/// a plucked string's soundboard wear re a struck one's: its own loss b1 is most of its damping
/// (a piano's strings lose theirs mostly through the bridge), so the same factor took a guitar's
/// and the harps' sustain (−18 dB at 1 s, median, at Broken; design log step 193)
const PLUCKED_BOARD_LOSS: f64 = BOARD_LOSS / 3.0;

/// Tuning applied when compiling (runtime parameters; neutral by default).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// concert pitch re the instrument's (A4 = 440 Hz × ratio): every string retuned by
    /// its tension (T ∝ f²), so the bridge force follows too
    pub pitch_ratio: f64,
    /// unison detuning re the fitted one (0 = perfectly in tune, 1 = as measured)
    pub unison_width: f64,
    /// temperament: cents re equal temperament per pitch class (C = 0), A at 0
    /// (`temperament::offsets`); all zeros: equal (the strings as fitted)
    pub temperament: [f64; 12],
    /// a scale per key (A0 … C8: cents re equal temperament, `temperament::scale_keys`; a Scala file),
    /// in place of `temperament`
    pub keys: Option<[f64; 88]>,
    /// each recorded key at its own pitch (`NoteSpec::recorded_cents`; the temperament menu's
    /// "As recorded", `temperament::AS_RECORDED`)
    pub as_recorded: bool,
    /// tuner's stretch on top of the recording's own tuning: 0 as recorded, 1 adds the
    /// stretch computed from the strings' inharmonicity (`stretch::offsets`), −1 takes
    /// the same off, in between and beyond scales it
    pub stretch: f64,
    /// the prepared piano's bolt or screw on the strings: its mass re the greatest it can be
    /// (0 none … 1) and its position along the string (fraction)
    pub bolt: f64,
    pub bolt_pos: f64,
    pub wear: Wear,
    /// microphone position and the pair's spacing (m), applied to the radiation when the
    /// engine is built
    pub mic: crate::mic::MicPosition,
    pub mic_spacing: f64,
    /// horizontal distance from the board's center, close positions: height above it
    /// (m; 0: the position's default)
    pub mic_distance: f64,
    /// level reference for positions: the direct sound (true) or the total (false)
    pub keep_direct: bool,
    /// the free microphones, used when `mic` is `Custom` (clamped when the engine is built)
    pub mic_setup: crate::mic::MicSetup,
    /// how far the lid is open: 0 closed … 1 as modeled (grand: full stick; upright: top lid open),
    /// `mic::LID_OFF` taken off
    pub lid: f64,
    /// a pillow or blanket against the soundboard: 0 none … 1 (the body response gets duller, quieter, shorter)
    pub pillow: f64,
    /// bars: all 88 keys spread along the frame (true) instead of its own range, beyond which the
    /// keys sound from its ends (`mic::Body::zone_key`)
    pub spread_keys: bool,
    /// the ring re the fitted one: every mode's decay rate (the strings' coupled unison modes, a
    /// bar's modes, the sympathetic resonators) divided by it, its frequency and its share of the
    /// strike kept, so it rings `sustain` times as long (1 as fitted; a treble ringing longer than
    /// the recordings, design log step 129). The dampers and the contact keep theirs:
    /// dividing the losses themselves moved the pitch (the unison's strings lock by the coupling's
    /// real part, C5 −5 cents at 4×) or left the bass (the internal loss alone).
    pub sustain: f64,
    /// the board's bass (dB, `board_bass`) on hammer pianos' strings; 0: as shipped
    pub board_bass_db: f64,
}

impl Tuning {
    /// The microphones the radiation is built for: a preset's, the clamped custom pair, or
    /// None for the recording's position.
    pub fn mics_for(&self, body: crate::mic::Body) -> Option<crate::mic::MicSetup> {
        use crate::mic::MicPosition;
        let lid = crate::mic::lid_value(self.lid);
        match self.mic {
            MicPosition::Recording => None,
            MicPosition::Custom => Some(body.clamp_setup(crate::mic::MicSetup { lid, ..self.mic_setup })),
            pos => body.preset_setup(pos, self.mic_spacing, self.mic_distance, lid),
        }
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            pitch_ratio: 1.0,
            unison_width: 1.0,
            temperament: [0.0; 12],
            keys: None,
            as_recorded: false,
            stretch: 0.0,
            bolt: 0.0,
            bolt_pos: 0.31,
            wear: Wear::default(),
            mic: crate::mic::MicPosition::Recording,
            mic_spacing: 0.5,
            mic_distance: 0.0,
            keep_direct: false,
            mic_setup: crate::mic::MicSetup::default(),
            lid: 1.0,
            pillow: 0.0,
            spread_keys: false,
            sustain: 1.0,
            board_bass_db: 0.0,
        }
    }
}

/// An independent random stream per (seed, string, purpose, index); the string is
/// note + 256·register (register 0: the note alone, as before registers).
fn stream(seed: u64, spec: &NoteSpec, purpose: u64, index: u64) -> crate::noise::Rng {
    let string = spec.note as u64 | (spec.register as u64) << 8;
    crate::noise::Rng::new(seed.wrapping_mul(0x9e37_79b9) ^ (string << 32) ^ (purpose << 24) ^ index)
}

/// the bolt's mass re the string at amount 1 (the greatest shift of a partial), and the loss it
/// adds to the partial it moves most (1/s)
const BOLT_MU: f64 = 0.25;
const BOLT_LOSS: f64 = 25.0;
/// the Sustain's range (`Tuning::sustain`)
pub const MIN_SUSTAIN: f64 = 0.25;
pub const MAX_SUSTAIN: f64 = 8.0;
/// smallest unison width: exactly equal strings make the coupled system degenerate
const MIN_UNISON_WIDTH: f64 = 0.01;

/// Compile a note for sample rate `sr`.
/// `stretch_cents`: the note's tuner's stretch already scaled by `Tuning::stretch`.
pub fn compile(spec: &NoteSpec, g: &Globals, sr: f64, tuning: &Tuning, stretch_cents: f64) -> Physics {
    let wear = &tuning.wear;
    let mut st = spec.string.clone();
    // the note's pitch: concert pitch and tuning drift, by its tension
    let drift = tuning_cents(wear.tuning) * stream(wear.seed, spec, 1, 0).gauss();
    let mut ratio = tuning.pitch_ratio * 2f64.powf(drift / 1200.0);
    let class = match &tuning.keys {
        Some(k) => k[(spec.note as usize).clamp(21, 108) - 21],
        None => tuning.temperament[spec.note as usize % 12],
    };
    let temper = class + if tuning.as_recorded { spec.recorded_cents } else { 0.0 };
    if temper != 0.0 {
        ratio *= 2f64.powf(temper / 1200.0);
    }
    if stretch_cents != 0.0 {
        ratio *= 2f64.powf(stretch_cents / 1200.0);
    }
    st.f0 *= ratio;
    st.t *= ratio * ratio;
    let width = tuning.unison_width.max(MIN_UNISON_WIDTH);
    let mut ru = stream(wear.seed, spec, 2, 0);
    let n_str = st.detune_cents.len();
    for c in st.detune_cents.iter_mut() {
        let d = ru.gauss();
        *c = *c * width + if n_str > 1 { unison_cents(wear.unisons) * d } else { 0.0 };
    }
    // a bolt on the string is a point mass at the position: partial k is lowered by
    // μ·sin²(kπ·pos) (a node there leaves it alone) and damped by the bolt's rattle
    let bolt = tuning.bolt > 0.0;
    let (bolt_mu, bolt_loss) = (BOLT_MU * tuning.bolt, BOLT_LOSS * tuning.bolt);
    st.b3 *= 1.0 + STRING_LOSS * wear.strings;
    let board_loss = if matches!(spec.excitation, Excitation::Pluck(_)) { PLUCKED_BOARD_LOSS } else { BOARD_LOSS };
    st.b1 *= 1.0 + board_loss * wear.soundboard;
    let control = if tuning.sustain.is_finite() { tuning.sustain.clamp(MIN_SUSTAIN, MAX_SUSTAIN) } else { 1.0 };
    let sustain = (control * spec.sustain).clamp(MIN_SUSTAIN, MAX_SUSTAIN);
    let st = &st;
    let s = st.detune_cents.len();
    // partials below min(f_max, 0.45·sr) (by the mean string's frequency); a bar's modes:
    // k is the mode's number, its frequency f0·ratio
    let f_lim = st.f_max.min(0.45 * sr);
    let bar = st.modes.as_ref();
    let freq = |k: f64| match bar {
        Some(m) => st.f0 * m.ratio[k as usize - 1],
        None => k * st.f0 * (1.0 + st.b * k * k).sqrt(),
    };
    let ks: Vec<f64> = match bar {
        Some(m) => (1..=m.ratio.len()).map(|k| k as f64).filter(|&k| freq(k) < f_lim).collect(),
        None => (1..=(st.f_max / st.f0) as usize).map(|k| k as f64).filter(|&k| freq(k) < f_lim).collect(),
    };
    let k = ks.len();
    let detune: Vec<f64> = st.detune_cents.iter().map(|c| 2f64.powf(c / 1200.0)).collect();
    let mut w = vec![0.0; s * k];
    let mut sigma_int = vec![0.0; s * k];
    for (i, d) in detune.iter().enumerate() {
        // old strings: each partial of each string slightly off (grows with the partial)
        let mut rs = stream(wear.seed, spec, 3, i as u64);
        for (j, &kk) in ks.iter().enumerate() {
            let mut scatter = if wear.strings > 0.0 {
                2f64.powf(wear.strings * STRING_CENTS * kk.sqrt() * rs.gauss() / 1200.0)
            } else {
                1.0
            };
            if st.scatter_cents > 0.0 && s > 1 {
                scatter *= 2f64.powf(st.scatter_cents * kk.sqrt() * string_gauss(spec.note, i, kk as usize) / 1200.0);
            }
            let mut wv = 2.0 * PI * d * scatter * freq(kk);
            let mut extra = 0.0;
            if bolt {
                let s2 = (kk * PI * tuning.bolt_pos.clamp(0.02, 0.5)).sin().powi(2);
                wv *= 1.0 - bolt_mu * s2;
                extra = bolt_loss * s2;
            }
            w[i * k + j] = wv;
            // a bar: the mode's own loss (worn: the string's wear factors on it)
            let own = match bar {
                Some(m) => m.sigma[kk as usize - 1] * (1.0 + BOARD_LOSS * wear.soundboard),
                None => string_loss(st.b1, st.b3, wv, st.loss_q),
            };
            sigma_int[i * k + j] = own + extra;
        }
    }
    let f_mean: Vec<f64> = (0..k).map(|j| (0..s).map(|i| w[i * k + j]).sum::<f64>() / s as f64 / (2.0 * PI)).collect();
    // the vertical polarization's coupling (and the sympathetic bank's, from y) with the bridge's
    // mobility at this key; the horizontal polarization's share of the measured modes without it
    let y: Vec<f64> = f_mean.iter().map(|&f| g.board.admittance_re(f) * g.board.mobility(spec.note as f64, f)).collect();
    let kappa_v: Vec<C> = y.iter().map(|&yv| C::new(st.kappa * yv, st.kappa * st.kappa_reactive)).collect();
    // the horizontal polarization: its own small coupling, plus its share of the measured modes
    // (a guitar's top mode damps both polarizations; string_model.free_modes)
    let kappa_h: Vec<C> = f_mean
        .iter()
        .map(|&f| {
            let m = if g.board.h_share != 0.0 { st.kappa * g.board.h_share * g.board.admittance_re_measured(f) } else { 0.0 };
            C::new(st.horiz_kappa + m, 0.0)
        })
        .collect();
    let corr_row = g.correction.as_ref().map(|c| (c.row(spec.note as f64), &c.grid_log2));
    let bridge: Vec<f64> = ks
        .iter()
        .zip(&f_mean)
        .map(|(&kk, &f)| {
            let sign = if (kk as i64) % 2 == 0 { 1.0 } else { -1.0 };
            let gain = match &corr_row {
                Some((row, grid)) => 10f64.powf(interp(f.max(1.0).log2(), grid, row) / 20.0),
                None => 1.0,
            };
            match bar {
                Some(m) => m.out[kk as usize - 1] * gain,
                None => st.t * (kk * PI / st.l) * sign * gain,
            }
        })
        .collect();

    // coupled unison modes: vertical (κ per partial), horizontal (detuned, real κ)
    let mut vert = Polarization::default();
    let mut horiz = Polarization::default();
    let hd = 2f64.powf(st.horiz_detune_cents / 1200.0);
    for j in 0..k {
        let d_v: Vec<C> = (0..s).map(|i| C::new(-sigma_int[i * k + j], w[i * k + j])).collect();
        let d_h: Vec<C> = (0..s).map(|i| C::new(-sigma_int[i * k + j], w[i * k + j] * hd)).collect();
        for (pol, d, kap) in [(&mut vert, d_v, kappa_v[j]), (&mut horiz, d_h, kappa_h[j])] {
            let (lam, v) = rank_one_eig(&d, kap);
            let vinv = inverse(&v);
            for e in 0..s {
                // the Sustain: the decay rate alone (`Tuning::sustain`)
                pol.lam.push(C::new(lam[e].re / sustain, lam[e].im));
                pol.colsum.push((0..s).map(|i| v[i][e]).sum());
            }
            for row in &vinv {
                pol.vinv.extend_from_slice(row);
            }
            for row in &v {
                pol.v.extend_from_slice(row);
            }
        }
    }

    let damper_sigma: Vec<f64> = f_mean.iter().map(|&f| damper_at(spec, g, f)).collect();
    let buff_sigma: Vec<f64> = match spec.buff {
        Some((flat, gain, pos)) => ks.iter().map(|&kk| flat + gain * (kk * PI * pos).sin().powi(2)).collect(),
        None => Vec::new(),
    };
    // sympathetic resonators: partials below sym_f_max, unisons merged
    let mut sym = Sympathetic::default();
    for j in 0..k {
        let f = f_mean[j];
        if f >= g.sym_f_max {
            continue;
        }
        let sig = (0..s).map(|i| sigma_int[i * k + j]).sum::<f64>() / s as f64;
        sym.f.push(f);
        sym.sigma.push((sig + st.kappa * y[j]) / sustain);
        sym.kappa.push(C::new(st.kappa * y[j], st.kappa * st.kappa_reactive) * s as f64);
        sym.damper_sigma.push(damper_at(spec, g, f));
        if let Some(&b) = buff_sigma.get(j) {
            sym.buff_sigma.push(b);
        }
    }

    let mut split = Vec::new();
    // a bar's own split pairs (the tubes), in partial order; the false beats on the other partials
    let own_split = |j: usize| bar.and_then(|m| m.split.get(ks[j] as usize - 1)).filter(|&&(_, r)| r > 0.0);
    let x = wear.false_beats / FALSE_BEATS_REF;
    for j in 0..k {
        if let Some(&(hz, r)) = own_split(j) {
            split.push((j, 2.0 * PI * hz * ratio, r));
            continue;
        }
        if wear.false_beats > 0.0 {
            let mut r = stream(wear.seed, spec, 4, j as u64);
            let (u, hz, sign) = (r.uniform01(), r.uniform01(), r.uniform01());
            if u < x.min(1.0) {
                let dw = 2.0 * PI * x * (1.0 + 4.0 * hz) * if sign < 0.5 { -1.0 } else { 1.0 };
                split.push((j, dw, 0.5 * x.min(1.0)));
            }
        }
    }

    let split_fundamental = k > 0 && own_split(0).is_some();
    let (core_k, core_depth) = match &spec.excitation {
        Excitation::Hammer(h) => (h.balance.iter().take(s).map(|b| h.core_k * b).collect(), h.core_depth),
        Excitation::Pluck(_) => (vec![0.0; s], 0.0),
    };
    let (phi_h, felt_k, felt_p, hammer_mass, pluck) = match &spec.excitation {
        Excitation::Hammer(h) => (
            match bar {
                Some(m) => ks.iter().map(|&kk| m.phi[kk as usize - 1]).collect(),
                None => ks.iter().map(|&kk| (kk * PI * h.strike_pos).sin()).collect(),
            },
            h.balance.iter().take(s).map(|b| h.felt_k * b).collect(),
            h.felt_p,
            h.mass,
            None,
        ),
        // no contact: the voice starts the modes directly (Voice::pluck)
        Excitation::Pluck(p) => (vec![0.0; k], vec![0.0; s], 0.0, 1.0, Some(*p)),
    };
    Physics {
        split,
        s,
        k,
        kappa_re: kappa_v.iter().map(|c| c.re).collect(),
        phi_h,
        strike: match (&spec.excitation, bar) {
            (Excitation::Hammer(h), None) => Some((h.strike_pos, st.l)),
            _ => None,
        },
        bridge,
        modal_mass: st.mass / 2.0,
        felt_k,
        felt_p,
        core_k,
        core_depth,
        hammer_mass,
        pluck,
        // (not for a fundamental with a split pair of its own, the tubes': the resonator's bank runs no
        // copies; it stays with the other modes, where its partner beats with it)
        tube: bar.filter(|_| s == 1 && k > 0 && !split_fundamental).map(|m| Tube { harmonics: m.harmonics.clone(), a_ref: 0.0 }),
        // the kept partials before the halo's first (the partials above the rate's limit dropped)
        halo_from: bar.map_or(k, |m| ks.iter().filter(|&&kk| (kk as usize) <= m.halo_from).count()),
        f_mean: f_mean.clone(),
        horiz_gain: st.horiz_gain,
        vert,
        horiz,
        damper_sigma,
        sym,
        buff_sigma,
        w,
        sigma_int,
    }
}

/// Eigen-decomposition of M = diag(d) − κ·11ᵀ (S ≤ 3): eigenvalues and eigenvectors
/// (`v[row][col]`, column e for eigenvalue e). The characteristic polynomial of this
/// rank-one update is Π(d_s − λ) − κ·Σ_s Π_{t≠s}(d_t − λ); its roots (Durand–Kerner,
/// Newton-polished) about the mean of d, the eigenvectors as null vectors of M − λ.
pub fn rank_one_eig(d: &[C], kappa: C) -> (Vec<C>, Vec<Vec<C>>) {
    let s = d.len();
    if s == 1 {
        return (vec![d[0] - kappa], vec![vec![C::new(1.0, 0.0)]]);
    }
    let c: C = d.iter().sum::<C>() / s as f64;
    let dp: Vec<C> = d.iter().map(|x| x - c).collect();
    // polynomial coefficients, ascending powers of λ
    let mul = |p: &[C], a: C| -> Vec<C> {
        // p · (a − λ)
        let mut out = vec![C::new(0.0, 0.0); p.len() + 1];
        for (i, &v) in p.iter().enumerate() {
            out[i] += v * a;
            out[i + 1] -= v;
        }
        out
    };
    let mut poly = vec![C::new(1.0, 0.0)];
    for &x in &dp {
        poly = mul(&poly, x);
    }
    for i in 0..s {
        let mut term = vec![C::new(1.0, 0.0)];
        for (t, &x) in dp.iter().enumerate() {
            if t != i {
                term = mul(&term, x);
            }
        }
        for (pc, tc) in poly.iter_mut().zip(&term) {
            *pc -= kappa * tc;
        }
    }
    let lead = poly[s];
    let monic: Vec<C> = poly.iter().map(|v| v / lead).collect();
    let eval = |x: C| monic.iter().rev().fold(C::new(0.0, 0.0), |acc, &v| acc * x + v);
    let deriv = |x: C| monic.iter().enumerate().skip(1).rev().fold(C::new(0.0, 0.0), |acc, (i, &v)| acc * x + v * i as f64);
    let scale = dp.iter().map(|x| x.norm()).fold(kappa.norm(), f64::max).max(1e-9);
    let mut roots: Vec<C> =
        (0..s).map(|i| dp[i] - kappa + C::new(0.37, 0.91) * (scale * 1e-3 * (i + 1) as f64)).collect();
    for _ in 0..500 {
        let mut delta: f64 = 0.0;
        for i in 0..s {
            let mut den = C::new(1.0, 0.0);
            for j in 0..s {
                if j != i {
                    den *= roots[i] - roots[j];
                }
            }
            if den.norm() == 0.0 {
                continue;
            }
            let step = eval(roots[i]) / den;
            roots[i] -= step;
            delta = delta.max(step.norm());
        }
        if delta <= 1e-15 * scale {
            break;
        }
    }
    for r in roots.iter_mut() {
        for _ in 0..3 {
            let dv = deriv(*r);
            if dv.norm() > 0.0 {
                *r -= eval(*r) / dv;
            }
        }
    }
    let mut v = vec![vec![C::new(0.0, 0.0); s]; s];
    for (e, &lam) in roots.iter().enumerate() {
        let a: Vec<Vec<C>> = (0..s)
            .map(|i| (0..s).map(|j| if i == j { dp[i] - kappa - lam } else { -kappa }).collect())
            .collect();
        let vec = null_vector(&a);
        for i in 0..s {
            v[i][e] = vec[i];
        }
    }
    (roots.iter().map(|r| r + c).collect(), v)
}

/// A null vector of a singular 2×2 or 3×3 matrix (the best-conditioned candidate).
fn null_vector(a: &[Vec<C>]) -> Vec<C> {
    let norm = |v: &[C]| v.iter().map(|x| x.norm_sqr()).sum::<f64>();
    let cands: Vec<Vec<C>> = if a.len() == 2 {
        vec![vec![-a[0][1], a[0][0]], vec![a[1][1], -a[1][0]]]
    } else {
        let cross = |p: &[C], q: &[C]| vec![p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
        vec![cross(&a[0], &a[1]), cross(&a[0], &a[2]), cross(&a[1], &a[2])]
    };
    cands.into_iter().max_by(|x, y| norm(x).total_cmp(&norm(y))).unwrap()
}

/// Inverse of a 1×1, 2×2 or 3×3 complex matrix (adjugate / determinant).
fn inverse(m: &[Vec<C>]) -> Vec<Vec<C>> {
    match m.len() {
        1 => vec![vec![C::new(1.0, 0.0) / m[0][0]]],
        2 => {
            let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
            vec![vec![m[1][1] / det, -m[0][1] / det], vec![-m[1][0] / det, m[0][0] / det]]
        }
        _ => {
            let c = |i: usize, j: usize| {
                let (r0, r1) = ((i + 1) % 3, (i + 2) % 3);
                let (c0, c1) = ((j + 1) % 3, (j + 2) % 3);
                m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
            };
            let det = m[0][0] * c(0, 0) + m[0][1] * c(0, 1) + m[0][2] * c(0, 2);
            (0..3).map(|i| (0..3).map(|j| c(j, i) / det).collect()).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mics_for_sanitizes_the_lid() {
        use crate::mic::{Body, MicPosition};
        for mic in [MicPosition::Audience, MicPosition::Custom] {
            for bad in [f64::NAN, f64::INFINITY, -3.0, 7.0] {
                let t = Tuning { mic, lid: bad, ..Tuning::default() };
                let s = t.mics_for(Body::Grand).unwrap();
                assert!(s.lid.is_finite() && ((0.0..=1.0).contains(&s.lid) || s.lid == crate::mic::LID_OFF), "{mic:?} {bad}");
                assert_eq!(Some(s), t.mics_for(Body::Grand), "{mic:?} {bad}");
            }
        }
    }

    /// the string scatter draws equal the prototype's (string_model.string_gauss)
    #[test]
    fn string_gauss_matches_prototype() {
        let cases = [
            ((36, 0, 1), 0.9000262255637995),
            ((36, 1, 7), 1.5262474594837947),
            ((60, 2, 30), -4.328264095926694),
            ((21, 1, 700), -1.406540703312445),
        ];
        for ((n, s, k), v) in cases {
            assert!((string_gauss(n, s, k) - v).abs() < 1e-12, "{n} {s} {k}: {}", string_gauss(n, s, k));
        }
    }

    /// the wear by kind: strings keep all of it; bars none of the soundboard's, strings' or false
    /// beats', metal ones no drift, wooden bars a quarter of a string's, a kalimba all of it
    #[test]
    fn bars_wear_by_their_kind() {
        use crate::mic::Body;
        let w = Wear { unisons: 1.0, tuning: 1.0, strings: 1.0, soundboard: 1.0, false_beats: 1.0, seed: 3 };
        assert_eq!(w.on(Body::Grand), w);
        assert_eq!(w.on(Body::Guitar), w);
        for b in [Body::Vibraphone, Body::Bells, Body::Crotales, Body::Chimes, Body::HandChimes] {
            let o = w.on(b);
            assert_eq!((o.tuning, o.strings, o.soundboard, o.false_beats, o.seed), (0.0, 0.0, 0.0, 0.0, 3), "{b:?}");
        }
        let wood = w.on(Body::Marimba);
        assert!((tuning_cents(wood.tuning) / tuning_cents(1.0) - 0.25).abs() < 0.005 && wood.soundboard == 0.0);
        assert_eq!(w.on(Body::Kalimba).tuning, 1.0);
    }

    /// the pluck's starting amplitudes equal the prototype's (string_model.pluck_amplitudes;
    /// proto/tests/test_pluck.py::test_amplitudes_parity_values)
    #[test]
    fn pluck_amplitudes_match_prototype() {
        let p = PluckSpec::string(0.2, 4400.0, 1.5e-3, 3.0, 0.5);
        let reference: [f64; 8] = [
            1.325979119379e-03, 5.349556777366e-04, 2.367215228236e-04, 8.179817851556e-05,
            1.082384929774e-20, -3.574542459254e-05, -4.204097785880e-05, -3.180193549273e-05,
        ];
        for (j, r) in reference.iter().enumerate() {
            let k = (j + 1) as f64;
            let a = pluck_amplitude(k, 220.0 * k, &p, 0.5, 1.0);
            assert!((a - r).abs() <= 1e-9 * r.abs() + 1e-22, "k={k}: {a} vs {r}");
        }
        // the hardness scales the quill lowpass: a harder quill is brighter
        assert!(pluck_amplitude(8.0, 1760.0, &p, 0.0, 2.0).abs() > pluck_amplitude(8.0, 1760.0, &p, 0.0, 1.0).abs());
    }

    /// the tangent: 2·amp/(πk) through the lowpass, no comb (proto/tests/test_pluck.py::
    /// test_tangent_amplitudes_parity_values)
    #[test]
    fn tangent_amplitudes_match_prototype() {
        let p = PluckSpec { tangent: true, ..PluckSpec::string(0.2, 4400.0, 1.5e-3, 3.0, 0.5) };
        let reference: [f64; 8] = [
            1.133934203080e-03, 5.654722884298e-04, 3.753379956579e-04, 2.798045640777e-04,
            2.221318046052e-04, 1.834099449534e-04, 1.555372366619e-04, 1.344643228438e-04,
        ];
        for (j, r) in reference.iter().enumerate() {
            let k = (j + 1) as f64;
            let a = pluck_amplitude(k, 220.0 * k, &p, 0.5, 1.0);
            assert!((a - r).abs() <= 1e-9 * r.abs(), "k={k}: {a} vs {r}");
        }
    }

    /// the measured modes' admittance (`proto/board.py` admittance_measured): theirs alone, 0
    /// without them; their peak φ²/(2σ), normalized by density/4
    #[test]
    fn measured_modes_admittance_is_theirs_alone() {
        let sigma = PI * 220.0 / 40.0;
        let mut b = Board {
            f: vec![100.0, 300.0, 220.0],
            sigma: vec![50.0, 50.0, sigma],
            phi: vec![1.0, -1.0, 6f64.sqrt()],
            density: 1.0 / 40.0,
            r: [vec![0.0; 3], vec![0.0; 3]],
            r_h: [vec![0.0; 3], vec![0.0; 3]],
            measured_from: 2,
            h_share: 0.5,
            mobility: None,
        };
        let peak = 6.0 / (2.0 * sigma) / (b.density / 4.0);
        assert!((b.admittance_re_measured(220.0) - peak).abs() < 1e-9 * peak);
        assert!(b.admittance_re(220.0) > b.admittance_re_measured(220.0));
        b.measured_from = 3;
        assert_eq!(b.admittance_re_measured(220.0), 0.0);
    }

    /// the table `proto/tests/test_loss_law.py` pins too: (b1, b3, f, q) → σ (1/s)
    const LOSS_TABLE: [((f64, f64, f64, f64), f64); 5] = [
        ((0.5, 1e-9, 500.0, 2.0), 0.509869604),
        ((0.5, 1e-9, 500.0, 1.5), 0.513957728),
        ((0.5, 1e-9, 4000.0, 1.5), 0.815827341),
        ((0.2, 2e-9, 8000.0, 1.0), 0.831654682),
        ((0.2, 2e-9, 1000.0, 1.3), 0.278956835),
    ];

    #[test]
    fn loss_follows_its_table() {
        for ((b1, b3, f, q), want) in LOSS_TABLE {
            let got = string_loss(b1, b3, 2.0 * PI * f, q);
            assert!((got - want).abs() < 1e-9, "({b1}, {b3}, {f}, {q}): {got} vs {want}");
        }
    }

    /// the table `proto/tests/test_felt_core.py` pins too: (K, p, core_K, core_depth, δ) → F (N)
    const FELT_TABLE: [((f64, f64, f64, f64, f64), f64); 5] = [
        ((1.2e8, 2.3, 0.0, 0.0, 1e-3), 15.1071049),
        ((1.2e8, 2.3, 5.9e8, 4e-4, 1e-3), 38.0474469),
        ((1.2e8, 2.3, 5.9e8, 4e-4, 3e-4), 0.947458359),
        ((3e8, 2.5, 1e9, 2e-4, 2e-3), 191.12719),
        ((1.2e8, 2.3, 5.9e8, 4e-4, -1e-4), 0.0),
    ];

    #[test]
    fn felt_law_follows_its_table() {
        for ((k, p, ck, cd, d), want) in FELT_TABLE {
            let got = felt_force(k, p, ck, cd, d);
            assert!((got - want).abs() <= 1e-8 * want.abs(), "({k}, {p}, {ck}, {cd}, {d}): {got} vs {want}");
        }
    }

    /// the table `proto/tests/test_bridge_mobility.py` pins too: (note, f) → factor
    const MOBILITY_TABLE: [((f64, f64), f64); 8] = [
        ((36.0, 0.0), 4.0),
        ((30.0, 0.0), 2.0),
        ((36.0, 500.0), 2.5),
        ((42.0, 250.0), 2.720872588),
        ((60.0, 100.0), 1.998402556),
        ((20.0, 1000.0), 1.0),
        ((36.0, 2000.0), 1.011673152),
        ((108.0, 0.0), 2.0),
    ];

    #[test]
    fn mobility_follows_its_table() {
        let m = Mobility { anchors: vec![24.0, 36.0, 48.0], factors: vec![1.0, 4.0, 2.0], fc_hz: 500.0 };
        for ((note, f), want) in MOBILITY_TABLE {
            assert!((m.factor(note, f) - want).abs() < 1e-9, "({note}, {f}): {} vs {want}", m.factor(note, f));
        }
    }

    #[test]
    fn mobility_is_one_without_anchors() {
        let mut b = Board {
            f: vec![100.0],
            sigma: vec![50.0],
            phi: vec![1.0],
            density: 1.0 / 40.0,
            r: [vec![0.0], vec![0.0]],
            r_h: [vec![0.0], vec![0.0]],
            measured_from: 1,
            h_share: 0.0,
            mobility: None,
        };
        assert_eq!(b.mobility(36.0, 100.0), 1.0);
        b.mobility = Some(Mobility { anchors: vec![36.0], factors: vec![4.0], fc_hz: 500.0 });
        assert!((b.mobility(36.0, 0.0) - 4.0).abs() < 1e-12);
    }

    /// eigenpairs satisfy M·v = λ·v and V·V⁻¹ = I, also for near-degenerate unisons
    #[allow(clippy::needless_range_loop)]
    #[test]
    fn rank_one_eigenpairs() {
        let cases = [
            (vec![C::new(-0.3, 1000.0), C::new(-0.31, 1000.4), C::new(-0.29, 1000.9)], C::new(1.2, 0.4)),
            (vec![C::new(-0.3, 52000.0), C::new(-0.3, 52000.0001)], C::new(0.8, 3.0)),
            (vec![C::new(-1.0, 300.0), C::new(-1.0, 300.0 + 1e-7), C::new(-1.0, 300.0 + 2e-7)], C::new(2.0, 0.5)),
        ];
        for (d, kap) in cases {
            let s = d.len();
            let (lam, v) = rank_one_eig(&d, kap);
            for e in 0..s {
                let n: f64 = (0..s).map(|i| v[i][e].norm_sqr()).sum::<f64>().sqrt();
                for i in 0..s {
                    let mv: C = (0..s).map(|j| (if i == j { d[i] } else { C::new(0.0, 0.0) } - kap) * v[j][e]).sum();
                    assert!((mv - lam[e] * v[i][e]).norm() <= 1e-7 * n * (1.0 + lam[e].norm()), "residual");
                }
            }
            let vi = inverse(&v);
            for i in 0..s {
                for j in 0..s {
                    let p: C = (0..s).map(|t| v[i][t] * vi[t][j]).sum();
                    let target = if i == j { 1.0 } else { 0.0 };
                    assert!((p - target).norm() < 1e-6, "inverse");
                }
            }
        }
    }
}
