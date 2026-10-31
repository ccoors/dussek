//! Loading an instrument exported by `proto/export.py`: `instrument.json` (structure and
//! small values) plus `data.bin` (little-endian arrays referenced by byte offset).

use std::fs;
use std::path::Path;

use serde::Deserialize;

pub(crate) mod morph;

use crate::felt::{self, FeltEdit, FeltKeys, KeyFelt};
use crate::noise;
use crate::physics::{BarHarmonics, BarModes, Board, Correction, Excitation, Globals, HammerSpec, HarmonicLaw, MAX_SUSTAIN, MIN_SUSTAIN, NoteSpec, Physics, PluckSpec, StringSpec, Tuning};
pub use crate::physics::{Polarization, Sympathetic};

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

/// Reference to an array in `data.bin`.
#[derive(Clone, Deserialize)]
struct ArrRef {
    o: usize,
    n: usize,
    t: String,
}

#[derive(Clone)]
struct Data(Vec<u8>);

impl Data {
    fn bytes(&self, r: &ArrRef, width: usize) -> Result<&[u8], Error> {
        self.0
            .get(r.o..r.o + r.n * width)
            .ok_or_else(|| err(format!("array at {} (n={}) beyond data.bin", r.o, r.n)))
    }

    fn f64(&self, r: &ArrRef) -> Result<Vec<f64>, Error> {
        if r.t != "f64" {
            return Err(err(format!("expected f64, got {}", r.t)));
        }
        Ok(self.bytes(r, 8)?.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect())
    }

    fn f32(&self, r: &ArrRef) -> Result<Vec<f32>, Error> {
        if r.t != "f32" {
            return Err(err(format!("expected f32, got {}", r.t)));
        }
        Ok(self.bytes(r, 4)?.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
    }

    /// `v` appended at the end; its reference
    fn push_f64(&mut self, v: &[f64]) -> ArrRef {
        let o = self.0.len();
        self.0.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        ArrRef { o, n: v.len(), t: "f64".into() }
    }

    fn push_f32(&mut self, v: &[f32]) -> ArrRef {
        let o = self.0.len();
        self.0.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        ArrRef { o, n: v.len(), t: "f32".into() }
    }
}

#[derive(Clone, Deserialize)]
struct RawKnock {
    knee: f64,
}

#[derive(Clone, Deserialize)]
struct RawMembrane {
    level: f64,
    #[serde(default = "membrane_hp_hz")]
    hp_hz: f64,
    #[serde(default)]
    hp_ratio: f64,
    #[serde(default = "membrane_lp_hz")]
    lp_hz: f64,
    #[serde(default = "membrane_poles")]
    poles: usize,
    #[serde(default)]
    growth: f64,
    #[serde(default)]
    tilt: f64,
    #[serde(default)]
    sympathetic: bool,
    #[serde(default = "membrane_power")]
    power: f64,
}

fn membrane_power() -> f64 {
    1.0
}

fn membrane_hp_hz() -> f64 {
    200.0
}

fn membrane_lp_hz() -> f64 {
    1600.0
}

fn membrane_poles() -> usize {
    2
}

/// A membrane buzzing with each bar (the balafon's gourds, `Voice::membrane`): noise at `level`
/// following the bar's own signal, high-passed at `hp_hz` (second order; with `hp_ratio` at that
/// multiple of the key's fundamental instead: the gourds buzz from each bar's pitch up), `poles` one-pole
/// lowpasses at `lp_hz` (1 … `MEMBRANE_POLES`). A harder strike buzzes more and brighter: with the
/// strike's level r re the key's own at speed 1 (the bar's swing, whichever mallet), the noise ×
/// r^`growth`, the lowpasses at `lp_hz` × r^`tilt`. `sympathetic`: its level (and the fundamental's
/// harmonics, the buzz's periodic part) on `Params::sympathetic` (the balafon's gourds; the
/// kalimba's rattle is not: its sympathetic control is its ringing tines). `power`: the noise
/// follows the bar's level to this power re its peak since the strike (a rattle buzzes while the
/// tine swings hard: 2 dies twice as fast as the tine; 1 the bar's level).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Membrane {
    pub level: f64,
    pub hp_hz: f64,
    pub hp_ratio: f64,
    pub lp_hz: f64,
    pub poles: usize,
    pub growth: f64,
    pub tilt: f64,
    pub sympathetic: bool,
    pub power: f64,
}

/// The most one-pole lowpasses a membrane's noise takes.
pub const MEMBRANE_POLES: usize = 4;

#[derive(Clone, Deserialize)]
struct RawRelease {
    /// (t0 s, level dB, decay s)
    impacts: Vec<[f64; 3]>,
    band_db: Vec<f64>,
    peak: [f64; 3],
    ratio_db: Vec<[f64; 2]>,
}

#[derive(Clone, Deserialize)]
struct RawString {
    f0: f64,
    #[serde(rename = "B")]
    b: f64,
    #[serde(rename = "L")]
    l: f64,
    #[serde(rename = "T")]
    t: f64,
    mass: f64,
    #[serde(rename = "S")]
    s: usize,
    detune_cents: Vec<f64>,
    b1: f64,
    b3: f64,
    /// the loss law's exponent (absent: 2, b1 + b3·ω²)
    #[serde(default = "two")]
    loss_q: f64,
    kappa: f64,
    kappa_reactive: f64,
    horiz_gain: f64,
    horiz_kappa: f64,
    horiz_detune_cents: f64,
    f_max: f64,
    #[serde(default)]
    scatter_cents: f64,
    /// a bar's modes (the vibraphone): ratio, sigma, phi, out per mode
    #[serde(default)]
    modes: Option<RawModes>,
}

#[derive(Clone, Deserialize)]
struct RawModes {
    ratio: Vec<f64>,
    sigma: Vec<f64>,
    phi: Vec<f64>,
    out: Vec<f64>,
    /// the fundamental's harmonics (`physics::BarHarmonics`)
    #[serde(default)]
    harmonics: Option<RawHarmonics>,
    /// per mode a slow beat: its partner Δf (Hz) and share (`BarModes::split`; the tubes)
    #[serde(default)]
    split_hz: Option<Vec<f64>>,
    #[serde(default)]
    split_share: Option<Vec<f64>>,
    /// the modes from this index on are the other tines ringing along (`BarModes::halo_from`)
    #[serde(default)]
    halo_from: Option<usize>,
}

/// Per harmonic h: level (dB re the fundamental at ff), growth (dB per dB), decay (× the
/// fundamental's loss), free and damped.
#[derive(Clone, Deserialize)]
struct RawHarmonics {
    h: Vec<u32>,
    level_db: Vec<f64>,
    growth: Vec<f64>,
    decay: Vec<f64>,
    damped_level_db: Vec<f64>,
    damped_growth: Vec<f64>,
    damped_decay: Vec<f64>,
}

impl RawHarmonics {
    fn ok(&self) -> bool {
        let n = self.h.len();
        [&self.level_db, &self.growth, &self.decay, &self.damped_level_db, &self.damped_growth, &self.damped_decay].iter().all(|v| v.len() == n)
            && self.h.iter().all(|&h| h >= 2)
            && self.decay.iter().chain(&self.damped_decay).all(|&d| d > 0.0)
    }

    fn compiled(&self) -> BarHarmonics {
        let law = |l: &[f64], g: &[f64], d: &[f64]| -> Vec<HarmonicLaw> {
            (0..self.h.len()).map(|i| HarmonicLaw { level_db: l[i], growth: g[i], decay: d[i] }).collect()
        };
        BarHarmonics {
            h: self.h.clone(),
            free: law(&self.level_db, &self.growth, &self.decay),
            damped: law(&self.damped_level_db, &self.damped_growth, &self.damped_decay),
        }
    }
}

#[derive(Clone, Deserialize)]
struct RawMallet {
    name: String,
    f_q: f64,
    fq_oct_per_step: f64,
    order: f64,
    speed: [f64; 3],
    /// its knock re the instrument's (dB)
    #[serde(default)]
    knock_db: f64,
    /// its membrane's buzz re the instrument's (dB)
    #[serde(default)]
    buzz_db: f64,
}

#[derive(Clone, Deserialize)]
struct RawHammer {
    mass: f64,
    #[serde(rename = "felt_K")]
    felt_k: f64,
    felt_p: f64,
    balance: Vec<f64>,
    strike_pos: f64,
    /// the felt's core (`physics::HammerSpec::core_k`); absent: none
    #[serde(default, rename = "core_K")]
    core_k: f64,
    #[serde(default)]
    core_depth: f64,
}

#[derive(Clone, Deserialize)]
struct RawPluck {
    beta: f64,
    f_q: f64,
    amp: f64,
    gain_db_per_step: f64,
    fq_oct_per_step: f64,
    /// a bar's modes started directly (the vibraphone's mallet)
    #[serde(default)]
    bar: bool,
    #[serde(default = "one")]
    order: f64,
    /// the clavichord's tangent (`PluckSpec::tangent`)
    #[serde(default)]
    tangent: bool,
}

fn one() -> f64 {
    1.0
}

/// Format 5: the note's excitation, tagged by "type"
#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RawExcitation {
    Hammer(RawHammer),
    Pluck(RawPluck),
}

/// the strings' loss exponent where an export has none
fn two() -> f64 {
    2.0
}

#[derive(Clone, Deserialize)]
struct RawNote {
    note: u8,
    string: RawString,
    /// format 4
    #[serde(default)]
    hammer: Option<RawHammer>,
    /// format 5
    #[serde(default)]
    excitation: Option<RawExcitation>,
    has_damper: bool,
    damper_sigma: Option<ArrRef>,
    knock: Option<RawKnock>,
    release: RawRelease,
    /// this key's membrane (rattle) re the instrument's (`Instrument::membrane`; 1 without)
    #[serde(default)]
    membrane_gain: Option<f64>,
    /// this key's hammer impact re the instrument's (dB; the project's [impact] trim_db, set by ear)
    #[serde(default)]
    impact_db: f64,
    /// the recorded key's own pitch re the curve it plays (cents; 0 unrecorded)
    #[serde(default)]
    recorded_cents: f64,
    /// this key's ring re the fitted one (the project's [sustain], set by ear; 1 without): the
    /// Sustain control's factor on it
    #[serde(default = "one")]
    sustain: f64,
    /// the level corrections per mallet (one triple: every mallet), dB at pp, mf, ff
    /// (the project's `level_corrections`)
    #[serde(default)]
    level_db: Vec<[f64; 3]>,
}

#[derive(Clone, Deserialize)]
struct RawBoard {
    f: ArrRef,
    sigma: ArrRef,
    phi: ArrRef,
    density: f64,
    r: ArrRef,
    r_h: ArrRef,
    /// the first measured mode (absent: none)
    #[serde(default)]
    measured_from: Option<usize>,
    #[serde(default)]
    h_share: f64,
    /// the bridge's mobility along the keyboard (absent: none)
    #[serde(default)]
    mobility: Option<RawMobility>,
}

/// The bridge's mobility's anchors (notes), their factors and the boost's corner (Hz).
#[derive(Clone, Deserialize)]
struct RawMobility {
    anchors: Vec<f64>,
    factors: Vec<f64>,
    fc_hz: f64,
}

/// The engine's mobility from the export's block.
fn mobility_of(r: &RawMobility) -> crate::physics::Mobility {
    crate::physics::Mobility { anchors: r.anchors.clone(), factors: r.factors.clone(), fc_hz: r.fc_hz }
}

#[derive(Clone, Deserialize)]
struct RawCorrection {
    notes: Vec<f64>,
    grid_hz: Vec<f64>,
    #[serde(rename = "C_db")]
    c_db: ArrRef,
}

/// One register (format 6): a set of strings per key, 88 notes keyed by the key played.
#[derive(Clone, Deserialize)]
struct RawRegister {
    name: String,
    notes: Vec<RawNote>,
    /// the stop that plays it (its bit of `Params::stops`: 0 the 8′, 1 the 4′, 2 the 8′ II); absent:
    /// its place in the list (a measured second 8′ without a 4′: stop 2)
    #[serde(default)]
    stop: Option<u8>,
    /// its own transfer correction (absent: the instrument's)
    #[serde(default)]
    correction: Option<RawCorrection>,
}

#[derive(Clone, Deserialize)]
struct RawVelocity {
    midi: [f64; 3],
    speed: [f64; 3],
}

#[derive(Clone, Deserialize)]
struct RawContact {
    max_s: f64,
    separation_s: f64,
}

/// The hammer's impact on the board (a piano's thump: `Instrument::impact`)
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Impact {
    /// × the felt force at the ff anchor's speed
    pub gain: f64,
    /// its lowpass (2nd-order Butterworth; None: open)
    pub cutoff_hz: Option<f64>,
    /// × (speed / the ff anchor's)^growth
    pub growth: f64,
}

#[derive(Clone, Deserialize)]
struct RawKnockParams {
    amp: f64,
    q: f64,
    decay_s: f64,
    cutoff_hz: f64,
    rise_s: f64,
}

#[derive(Clone, Deserialize)]
struct RawThump {
    impacts: Vec<[f64; 3]>,
    band_db: Vec<f64>,
    ratio_db: f64,
    seed: u64,
}

#[derive(Clone, Deserialize)]
struct RawKick {
    amp: f64,
    center: f64,
    spread: f64,
    rolloff: f64,
}

#[derive(Clone, Deserialize)]
struct RawPedal {
    kick: RawKick,
    kick_seed: u64,
    length_s: f64,
    bands_hz: Vec<f64>,
    down: Vec<RawThump>,
    up: Vec<RawThump>,
}

#[derive(Clone, Deserialize)]
struct RawReleaseParams {
    bands_hz: Vec<f64>,
    length_s: f64,
    rise_s: f64,
}

#[derive(Clone, Deserialize)]
struct RawSympathetic {
    transfer: f64,
    f_max: f64,
}

/// The buff stop (optional): pads on one register's strings, per key a flat part and a
/// strength (1/s) and a position (re the speaking length).
#[derive(Clone, Deserialize)]
struct RawBuff {
    register: usize,
    notes: Vec<u8>,
    flat: Vec<f64>,
    gain: Vec<f64>,
    pos: Vec<f64>,
}

/// The lute stop (optional): a second jack row on one register's strings, near the nut:
/// per key its pluck point (re the speaking length), and its level re that register's row.
#[derive(Clone, Deserialize)]
struct RawLute {
    register: usize,
    beta: Vec<f64>,
    gain_db: f64,
}

#[derive(Clone, Deserialize)]
struct RawRadiation {
    channels: usize,
    len: usize,
    ir: ArrRef,
}

#[derive(Clone, Deserialize)]
struct RawRadiationParts {
    body: RawRadiation,
    room_seed: u64,
}

#[derive(Clone, Deserialize)]
struct RawRoom {
    t60: Vec<[f64; 2]>,
    grid_hz: Vec<f64>,
    #[serde(rename = "H_db")]
    h_db: Vec<f64>,
    gain: f64,
    length_s: f64,
    #[serde(default)]
    eq_db: Vec<[f64; 2]>,
    /// the body's spectral envelope at the microphone positions (`mic::zones`), where the export moved
    /// part of the board's response into the correction (`[board] transient_db`); absent: `H_db`
    #[serde(rename = "body_H_db", default)]
    body_h_db: Option<Vec<f64>>,
}

impl RawRoom {
    fn spec(&self, seeds: [u64; 2]) -> RoomSpec {
        RoomSpec {
            t60: self.t60.clone(),
            grid_hz: self.grid_hz.clone(),
            h_db: self.h_db.clone(),
            length_s: self.length_s,
            seeds,
            eq_db: self.eq_db.clone(),
        }
    }
}

#[derive(Clone, Deserialize)]
struct RawRadiationSet {
    v: RawRadiationParts,
    h: RawRadiationParts,
    room: RawRoom,
}

/// The data.bin an instrument.json was written with: its size and SHA-256 (hex). Absent in a
/// hand-made or older file, which then loads unchecked.
#[derive(Clone, Deserialize)]
struct RawTie {
    bytes: usize,
    sha256: String,
}

#[derive(Clone, Deserialize)]
struct RawInstrument {
    format: u32,
    profile: String,
    /// the data.bin it was written with (`check_tie`)
    #[serde(default)]
    data: Option<RawTie>,
    /// the microphone geometry (`mic::Body::from_name`: "upright", "harp", "vibraphone", …; none: a grand)
    #[serde(default)]
    body: Option<String>,
    /// the vibraphone's motor (the discs in its resonators)
    #[serde(default)]
    motor: bool,
    /// the strings bend under the player's other hand (the đàn tranh): the pitch wheel acts
    #[serde(default)]
    bend: bool,
    /// a membrane buzzing with each bar (the balafon's gourds): its level and filters (`Membrane`)
    #[serde(default)]
    membrane: Option<RawMembrane>,
    /// mallets to choose from (`Params::mallet`; the notes' own excitation is the first's)
    #[serde(default)]
    mallets: Vec<RawMallet>,
    reference_sr: f64,
    mic_spacing: f64,
    velocity: RawVelocity,
    contact: RawContact,
    knock: Option<RawKnockParams>,
    #[serde(default)]
    impact: Option<Impact>,
    release: RawReleaseParams,
    pedal: Option<RawPedal>,
    sympathetic: RawSympathetic,
    board: RawBoard,
    correction: Option<RawCorrection>,
    damper_log2: [f64; 3],
    radiation: RawRadiationSet,
    /// formats 4 and 5: one register
    #[serde(default)]
    notes: Vec<RawNote>,
    /// format 6
    #[serde(default)]
    registers: Vec<RawRegister>,
    /// the buff stop (harpsichords)
    #[serde(default)]
    buff: Option<RawBuff>,
    /// the lute stop (Harpsichord B)
    #[serde(default)]
    lute: Option<RawLute>,
    /// output level re the ff calibration, dB (0: the median ff note at the engine's level;
    /// a quiet piano below it)
    #[serde(default)]
    output_gain_db: f64,
    /// the phantom partials' level (the Wear control's units; absent: none): part of the piano's
    /// own sound, the control adds wear on top
    #[serde(default)]
    phantom: f64,
    /// the lowest and highest recorded key (the plugin's strip; `info`)
    #[serde(default)]
    recorded: Option<[u8; 2]>,
    /// the plugin's menu entry (`info`; the engine plays without it)
    #[serde(default)]
    menu: Option<serde_json::Value>,
}

/// One pedal-noise take: burst (reference rate, channel-major stereo, peak 1) and its
/// peak level re the median mf note peak (dB).
#[derive(Clone)]
pub struct Thump {
    pub burst: std::sync::Arc<[f32]>,
    pub ratio_db: f64,
}

/// Sustain-pedal noise: thump takes for pedal down / up, and the strings' kick when
/// their dampers lift (`proto/pedal.py`): per string i of S and partial n, displacement
/// amp · bell(key) · n^−rolloff · e^{j(φ_n + 2π·i/S)}, bell = exp(−½((key − center)/spread)²),
/// φ_n pseudo-random per key (seed kick_seed + key).
#[derive(Clone)]
pub struct Pedal {
    pub kick_amp: f64,
    pub kick_center: f64,
    pub kick_spread: f64,
    pub kick_rolloff: f64,
    pub kick_seed: u64,
    pub down: Vec<Thump>,
    pub up: Vec<Thump>,
}

#[derive(Clone)]
pub struct Knock {
    pub knee: f64,
    /// unit burst at the reference rate, channel-major (2 × n); shared by the engines built from it
    pub burst: std::sync::Arc<[f32]>,
}

#[derive(Clone)]
pub struct Release {
    /// unit burst at the reference rate, channel-major (2 × n); shared by the engines built from it
    pub burst: std::sync::Arc<[f32]>,
    /// radiated peak of the note at the pp/mf/ff speeds
    pub peak: [f64; 3],
    /// (ln speed, dB re the note's peak)
    pub ratio_db: Vec<[f64; 2]>,
}

/// Everything the engine needs about one key: its physical parameters (`spec`), what
/// the physics compiler derives from them for the current rate (`phys`, also reachable
/// directly: `note.w` …), and the noise parts.
#[derive(Clone)]
pub struct Note {
    pub note: u8,
    /// the register (set of strings) this string belongs to; 0 is the main one
    pub register: u8,
    pub has_damper: bool,
    /// this key's membrane re the instrument's (the kalimba: each tine rattles its own way)
    pub membrane_gain: f64,
    /// this key's hammer impact re the instrument's `impact` (linear; the export's impact_db)
    pub impact_gain: f64,
    pub spec: NoteSpec,
    pub phys: Physics,
    pub knock: Option<Knock>,
    pub release: Release,
    /// the level corrections per mallet, dB at the pp, mf and ff anchor speeds (empty: none)
    pub level_db: Vec<[f64; 3]>,
}

impl std::ops::Deref for Note {
    type Target = Physics;
    fn deref(&self) -> &Physics {
        &self.phys
    }
}

#[derive(Clone)]
pub struct Radiation {
    pub channels: usize,
    /// impulse response per channel at the reference rate
    pub ir: Vec<Vec<f32>>,
}

/// Parameters of the diffuse room (`noise::room_ir`) and its seeds per polarization.
#[derive(Clone)]
pub struct RoomSpec {
    /// the late decay: (Hz, T60 s)
    pub t60: Vec<[f64; 2]>,
    pub grid_hz: Vec<f64>,
    pub h_db: Vec<f64>,
    pub length_s: f64,
    pub seeds: [u64; 2],
    /// the room's own level re H: (Hz, dB); empty: none
    pub eq_db: Vec<[f64; 2]>,
}


/// Radiation of one polarization: body and room, body + room = the full response.
#[derive(Clone)]
pub struct RadiationParts {
    pub body: Radiation,
    pub room: Radiation,
    /// per channel the share of the room's energy its cuts (`eq_db`) kept (1 without)
    pub room_kept: Vec<f64>,
}

/// A mallet of an instrument with several (`Instrument::mallets`): its lowpass (the cutoff on
/// a bar whose own excitation has the first mallet's: × the note's f_q re the first's) and
/// its speeds at pp, mf, ff (the velocity curve's anchors while it is chosen).
#[derive(Clone, Debug)]
pub struct Mallet {
    pub name: String,
    pub f_q: f64,
    pub fq_oct_per_step: f64,
    pub order: f64,
    pub speed: [f64; 3],
    /// its knock re the instrument's (linear: each of the balafon's mallets clicks on the wood its own way)
    pub knock: f64,
    /// its membrane's buzz re the instrument's (linear: the balafon's soft mallet sets the gourds
    /// buzzing least for the same swing)
    pub buzz: f64,
}

#[derive(Clone)]
pub struct Instrument {
    pub profile: String,
    /// a fingerprint of what the files held (a morph: of its sides' and their weights): an engine
    /// takes another's state over only for the same content (`Engine::adopt`), so a changed export
    /// of the same name (a rolled piano's next candidate, a re-export) does not keep the old body
    pub content: u64,
    /// the microphone geometry (`"body"`: `mic::Body::from_name`)
    pub body_kind: crate::mic::Body,
    /// a motor turns discs in the resonators (`motor.rs`; the vibraphone)
    pub motor: bool,
    /// struck by tangents (the clavichord: `PluckSpec::tangent`): a held key's pressure raises its
    /// pitch (`Params::bebung`)
    pub tangent: bool,
    /// the strings bend (the đàn tranh: the left hand presses behind the bridge): the pitch wheel
    /// moves the sounding notes (`Params::bend_range`)
    pub bend: bool,
    /// a membrane buzzing with each bar (the balafon's gourds: `Voice::membrane`)
    pub membrane: Option<Membrane>,
    /// the mallets `Params::mallet` picks from (empty: the notes' own excitation alone)
    pub mallets: Vec<Mallet>,
    pub reference_sr: f64,
    /// output level re the ff calibration (dB, `engine::calibration`)
    pub output_gain_db: f64,
    /// the phantom partials' level (the Wear control's units), added to the control's
    pub phantom: f64,
    pub velocity_midi: [f64; 3],
    pub velocity_speed: [f64; 3],
    /// a hammer piano's level over its hammer speed per key, as the engine plays it
    /// (`Instrument::level_curve`, stored in instrument.json's `level`); empty: none (the velocity plays the
    /// anchors)
    pub dynamics: Dynamics,
    pub contact_max_s: f64,
    pub contact_separation_s: f64,
    pub knock: Option<(f64, f64)>,
    /// a piano's thump: each strike's felt force, lowpassed, on the bridge (instead of a knock)
    pub impact: Option<Impact>,
    pub pedal: Option<Pedal>,
    pub transfer: f64,
    pub globals: Globals,
    /// the rate and tuning the notes' physics is compiled for
    pub compiled_sr: f64,
    pub tuning: Tuning,
    /// a fingerprint of every note's compiled physics (`compile`): equal ones run their voices alike,
    /// so a handover between them swaps the voices whole (`Engine::adopt`; a microphone moved)
    pub strings_print: u64,
    /// the radiated spectral envelope H (log2 Hz, dB) the body responses are shaped by
    pub body_h: (Vec<f64>, Vec<f64>),
    /// the room's synthesis parameters (re-synthesized for other microphone spacings)
    pub room_spec: RoomSpec,
    pub radiation_v: RadiationParts,
    pub radiation_h: RadiationParts,
    /// notes 21…108 of every register, register-major: index register·88 + note − 21
    pub notes: Vec<Note>,
    /// the registers' names (format 6; one unnamed register otherwise)
    pub register_names: Vec<String>,
    /// the stop (bit of `Params::stops`) that plays each register
    pub register_stops: Vec<u8>,
    /// registers ≥ 1: the globals with the register's own correction
    extra_globals: Vec<Globals>,
}

fn correction(d: &Data, c: &RawCorrection) -> Result<Correction, Error> {
    Ok(Correction {
        notes: c.notes.clone(),
        grid_log2: c.grid_hz.iter().map(|f| f.log2()).collect(),
        c_db: d.f64(&c.c_db)?,
    })
}


/// Body (exported, unit energy per channel) and room (synthesized), each divided by the
/// per-channel norm of body + √gain·room, as `soundboard.make_filter("full", split=True)`.
fn parts(d: &Data, r: &RawRadiationParts, room: &RawRoom, spacing: f64, sr: f64) -> Result<RadiationParts, Error> {
    let body = radiation(d, &r.body)?;
    if body.channels != 2 {
        return Err(err("expected a stereo body response"));
    }
    let rm = noise::room_ir(r.room_seed, &room.spec([r.room_seed; 2]), spacing, 1.0, sr);
    let g = room.gain.sqrt();
    let mut b_out = Vec::new();
    let mut r_out = Vec::new();
    // room_ir's unit energy comes before the cuts
    let room_kept: Vec<f64> = rm.iter().map(|c| c.iter().map(|v| v * v).sum()).collect();
    for (b, rc) in body.ir.iter().zip(&rm) {
        let n = b.len().max(rc.len());
        let norm = (0..n)
            .map(|i| b.get(i).map_or(0.0, |&v| v as f64) + g * rc.get(i).copied().unwrap_or(0.0))
            .map(|v| v * v)
            .sum::<f64>()
            .sqrt();
        b_out.push(b.iter().map(|&v| (v as f64 / norm) as f32).collect());
        r_out.push(rc.iter().map(|&v| (g * v / norm) as f32).collect());
    }
    Ok(RadiationParts { body: Radiation { channels: 2, ir: b_out }, room: Radiation { channels: 2, ir: r_out }, room_kept })
}

fn radiation(d: &Data, r: &RawRadiation) -> Result<Radiation, Error> {
    let all = d.f32(&r.ir)?;
    if all.len() != r.channels * r.len {
        return Err(err("radiation IR size mismatch"));
    }
    Ok(Radiation { channels: r.channels, ir: all.chunks_exact(r.len).map(<[f32]>::to_vec).collect() })
}

/// The keys whose ff level sets a morph's level (`Instrument::ff_level`, instrument.json's `level`).
pub const LEVEL_KEYS: [u8; 3] = [48, 60, 72];

/// The level table's speeds (`Instrument::level_curve`): the ff anchor's speed × 2^(k/2), octaves
/// from 1/64 of it (a Dynamics range of 60 dB asks ~50 dB below ff) to twice it. A key's level is
/// near linear in ln speed (3.5–4.5 dB per half octave on Grand F).
pub const LEVEL_SPEEDS: [i32; 8] = [-12, -10, -8, -6, -4, -2, 0, 2];

/// The window (s) over which a level table's strike is measured: its peak comes within the first
/// tens of ms (0.5 s finds Grand F's peaks within 0.3 dB of it).
pub const LEVEL_WINDOW_S: f64 = 0.2;

/// The level table's keys: every key. Its curves scatter by 1–1.7 dB from key to key around the
/// register's (Grand F), more than a row every few keys could stand for.
pub const LEVEL_TABLE_KEYS: [u8; 88] = {
    let mut k = [0u8; 88];
    let mut i = 0;
    while i < 88 {
        k[i] = 21 + i as u8;
        i += 1;
    }
    k
};

/// What a felt edit does to a hammer piano's levels (`Instrument::felt_leveling`): the level table it
/// plays, each key's level shift (dB, keys 21…108) back to the shipped felt's level at ff.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FeltLeveling {
    pub dynamics: Dynamics,
    pub shift: Vec<f64>,
}

/// A hammer piano's level over its hammer speed (`Instrument::dynamics`): per key of `keys` a row
/// of levels (dB re the key's own at the ff anchor's speed) at the speeds `ln_speed` (ascending).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dynamics {
    pub keys: Vec<u8>,
    pub ln_speed: Vec<f64>,
    pub db: Vec<Vec<f64>>,
}

impl Dynamics {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The table as the stored level's `speed_db` holds it (speeds in m/s).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "keys": self.keys, "speed": self.ln_speed.iter().map(|l| l.exp()).collect::<Vec<_>>(), "db": self.db })
    }

    /// `to_json`'s table read back (None: missing or malformed: the keys and speeds ascending, the
    /// speeds positive, a row per key of a level per speed, every number finite).
    pub fn from_json(v: &serde_json::Value) -> Option<Dynamics> {
        #[derive(serde::Deserialize)]
        struct Raw {
            keys: Vec<u8>,
            speed: Vec<f64>,
            db: Vec<Vec<f64>>,
        }
        let r: Raw = serde_json::from_value(v.clone()).ok()?;
        let ok = r.keys.len() >= 2
            && r.speed.len() >= 2
            && r.speed.len() <= LEVEL_SPEEDS.len()
            && r.keys.windows(2).all(|w| w[1] > w[0])
            && r.speed.windows(2).all(|w| w[1] > w[0])
            && r.speed.iter().all(|&s| s > 0.0 && s.is_finite())
            && r.db.len() == r.keys.len()
            && r.db.iter().all(|row| row.len() == r.speed.len() && row.iter().all(|x| x.is_finite()));
        ok.then(|| Dynamics { keys: r.keys, ln_speed: r.speed.iter().map(|s| s.ln()).collect(), db: r.db })
    }
}

/// The edited felt's level table as the shipped instrument plays its velocities: each key's edited
/// level at a speed (`edited`) is the shipped felt's at another (`measured`, inverted), and there the
/// table the instrument plays (`played`: a morph's is its sides' blend, not its own measured one)
/// gives the level. On the measured table's grid; the edited table where the played one has another.
fn played_table(played: &Dynamics, measured: &Dynamics, edited: Dynamics) -> Dynamics {
    let grid = |t: &Dynamics| t.keys == edited.keys && t.ln_speed.len() == edited.ln_speed.len() && t.db.len() == edited.db.len();
    if !grid(played) || !grid(measured) {
        return edited;
    }
    // a row's level at ln speed x: linear through its points, the end slopes beyond them
    let level_at = |xs: &[f64], ys: &[f64], x: f64| {
        let n = xs.len();
        let j = xs.windows(2).position(|w| x <= w[1]).unwrap_or(n - 2);
        let (x0, x1, y0, y1) = (xs[j], xs[j + 1], ys[j], ys[j + 1]);
        y0 + (x - x0) * (y1 - y0) / (x1 - x0)
    };
    let db = (0..edited.keys.len())
        .map(|k| {
            edited.db[k]
                .iter()
                .map(|&l| {
                    let x = crate::engine::speed_at_level(&measured.ln_speed, &measured.db[k], l);
                    level_at(&played.ln_speed, &played.db[k], x)
                })
                .collect()
        })
        .collect();
    Dynamics { db, ..edited }
}

/// The level the export measured, stored in the instrument's instrument.json (`level`: the `level`
/// tool's output, `Instrument::level_doc`); None when absent or unreadable.
fn stored(dir: &Path) -> Option<serde_json::Value> {
    let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("instrument.json")).ok()?).ok()?;
    v.get_mut("level").map(serde_json::Value::take)
}

/// The ff level stored with the instrument in `dir`: None when absent, unreadable, or measured on
/// other keys.
pub fn stored_level(dir: &Path) -> Option<f64> {
    let v = stored(dir)?;
    let keys: Vec<u64> = v.get("keys")?.as_array()?.iter().filter_map(|k| k.as_u64()).collect();
    (keys == LEVEL_KEYS.map(u64::from)).then(|| v.get("ff_db")?.as_f64()).flatten().filter(|x| x.is_finite())
}

/// The level table stored with the instrument in `dir` (its level's `speed_db`,
/// `Dynamics::to_json`): empty when absent or unreadable.
pub fn stored_dynamics(dir: &Path) -> Dynamics {
    let read = || -> Option<Dynamics> {
        let v = stored(dir)?;
        Dynamics::from_json(v.get("speed_db")?)
    };
    read().unwrap_or_default()
}

/// An instrument's files, parsed but not built (the morph works on these): instrument.json, data.bin,
/// the directory's name (messages) and its full path (a morph's profile: "." or two folders of one
/// name are told apart).
#[derive(Clone)]
struct Decoded {
    raw: RawInstrument,
    data: Data,
    name: String,
    path: String,
    /// what the files hold (`Instrument::content`)
    content: u64,
}

/// A fingerprint of an instrument's files: instrument.json's text and data.bin's bytes.
fn fingerprint(json: &[u8], data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    json.hash(&mut h);
    data.hash(&mut h);
    h.finish()
}

fn decode(dir: &Path) -> Result<Decoded, Error> {
    let raw = read_raw(dir)?;
    let data = Data(fs::read(dir.join("data.bin")).map_err(|e| err(format!("data.bin: {e}")))?);
    let full = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let name = full.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let json = fs::read(dir.join("instrument.json")).map_err(|e| err(format!("instrument.json: {e}")))?;
    check_tie(raw.data.as_ref(), &data.0)?;
    let content = fingerprint(&json, &data.0);
    Ok(Decoded { raw, data, name, path: full.display().to_string(), content })
}

/// data.bin against the size and SHA-256 its instrument.json names: another export's, or a cut one,
/// is refused (an instrument.json without them takes any)
fn check_tie(tie: Option<&RawTie>, data: &[u8]) -> Result<(), Error> {
    use sha2::{Digest, Sha256};
    let Some(t) = tie else { return Ok(()) };
    if data.len() != t.bytes {
        return Err(err(format!("data.bin: {} bytes, instrument.json was written with {}", data.len(), t.bytes)));
    }
    let hex: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
    if !hex.eq_ignore_ascii_case(&t.sha256) {
        return Err(err("data.bin: not the one instrument.json was written with (its SHA-256 differs)"));
    }
    Ok(())
}

/// instrument.json, parsed and checked (the format, the mallets), without data.bin
fn read_raw(dir: &Path) -> Result<RawInstrument, Error> {
    let json = fs::read_to_string(dir.join("instrument.json")).map_err(|e| err(format!("instrument.json: {e}")))?;
    let raw: RawInstrument = serde_json::from_str(&json).map_err(|e| err(format!("instrument.json: {e}")))?;
    if raw.mallets.iter().any(|m| !(m.f_q > 0.0 && m.order > 0.0 && m.speed.iter().all(|&v| v > 0.0))) {
        return Err(err("instrument.json: a mallet without a lowpass or speeds"));
    }
    if !(4..=6).contains(&raw.format) {
        return Err(err(format!("unsupported instrument format {}", raw.format)));
    }
    Ok(raw)
}

/// What `info` reads of a note: the blocks the engine branches on.
pub(crate) struct NoteFacts {
    /// struck by a hammer (format 4, or format 5's "hammer")
    pub hammer: bool,
    /// plucked by a tangent (the clavichord)
    pub tangent: bool,
    pub has_damper: bool,
    pub knock: bool,
    /// a release noise with impacts (a mechanism coming back up)
    pub impacts: bool,
    /// strings per note (`detune_cents`)
    pub strings: usize,
    pub kappa: f64,
    /// a bar whose modes from `halo_from` on are the other tines ringing along
    pub halo: bool,
    /// a bar whose modes each beat with a split partner (the tubes)
    pub split: bool,
}

/// What `info` reads of an instrument (instrument.json alone).
pub(crate) struct Facts {
    pub body: crate::mic::Body,
    /// 1: a hammer-strung piano the morph takes (`morph::refusal`); 0: none
    pub morph_family: u32,
    pub recorded: (u8, u8),
    pub registers: usize,
    /// the stops that play a register, as bits (none with one register)
    pub stops: u32,
    pub buff: bool,
    pub lute: bool,
    pub motor: bool,
    pub bend: bool,
    /// the instrument's knock (its level and growth)
    pub knock: bool,
    /// the hammer's impact on the board (the Action knock control sets it)
    pub impact: bool,
    pub pedal: bool,
    /// the pedal's kick sets strings ringing as their dampers lift
    pub pedal_ring: bool,
    /// a membrane on the sympathetic control (the balafon's gourds)
    pub membrane_sympathetic: bool,
    pub mallets: Vec<String>,
    /// the stored level holds a level table (`stored_dynamics`)
    pub level_table: bool,
    /// every register's notes
    pub notes: Vec<NoteFacts>,
    pub menu: Option<serde_json::Value>,
}

/// The facts of instrument `dir` (`info::info`).
pub(crate) fn facts(dir: &Path) -> Result<Facts, Error> {
    let raw = read_raw(dir)?;
    let notes: Vec<&RawNote> = if raw.format == 6 { raw.registers.iter().flat_map(|r| &r.notes).collect() } else { raw.notes.iter().collect() };
    let note = |n: &RawNote| {
        let pluck = match &n.excitation {
            Some(RawExcitation::Pluck(p)) => Some(p),
            _ => None,
        };
        let modes = n.string.modes.as_ref();
        NoteFacts {
            hammer: n.hammer.is_some() || matches!(n.excitation, Some(RawExcitation::Hammer(_))),
            tangent: pluck.is_some_and(|p| p.tangent),
            has_damper: n.has_damper,
            knock: n.knock.is_some(),
            impacts: !n.release.impacts.is_empty(),
            strings: n.string.detune_cents.len(),
            kappa: n.string.kappa,
            halo: modes.is_some_and(|m| m.halo_from.is_some_and(|h| h < m.ratio.len())),
            split: modes.is_some_and(|m| m.split_hz.is_some()),
        }
    };
    Ok(Facts {
        body: crate::mic::Body::from_name(raw.body.as_deref()),
        morph_family: morph::refusal(&raw).is_none() as u32,
        recorded: raw.recorded.map_or((21, 108), |[lo, hi]| (lo, hi)),
        registers: if raw.format == 6 { raw.registers.len() } else { 1 },
        stops: if raw.format == 6 && raw.registers.len() > 1 {
            register_stops(&raw)?.iter().fold(0, |m, s| m | 1 << s)
        } else {
            0
        },
        buff: raw.buff.is_some(),
        lute: raw.lute.is_some(),
        motor: raw.motor,
        bend: raw.bend,
        knock: raw.knock.is_some(),
        impact: raw.impact.is_some(),
        pedal: raw.pedal.is_some(),
        pedal_ring: raw.pedal.as_ref().is_some_and(|p| p.kick.amp > 0.0),
        membrane_sympathetic: raw.membrane.as_ref().is_some_and(|m| m.sympathetic),
        mallets: raw.mallets.iter().map(|m| m.name.clone()).collect(),
        level_table: !stored_dynamics(dir).is_empty(),
        notes: notes.into_iter().map(note).collect(),
        menu: raw.menu,
    })
}

/// The stop (bit of `Params::stops`) that plays each register: its own `stop`, else its place in the
/// list; distinct, below 32.
fn register_stops(raw: &RawInstrument) -> Result<Vec<u8>, Error> {
    if raw.format != 6 {
        return Ok(vec![0]);
    }
    let stops: Vec<u8> = raw.registers.iter().enumerate().map(|(i, r)| r.stop.unwrap_or(i as u8)).collect();
    if stops.iter().enumerate().any(|(i, s)| *s >= 32 || stops[..i].contains(s)) {
        return Err(err("registers: expected distinct stops below 32"));
    }
    Ok(stops)
}

/// The instrument from its decoded files: the notes' physics, the noise bursts and the room
/// synthesized.
fn build(dec: Decoded) -> Result<Instrument, Error> {
    let Decoded { raw, data: d, content, .. } = dec;
    // registers: format 6 lists them, formats 4/5 are one (the top-level notes)
    let regs: Vec<(&str, &[RawNote], Option<&RawCorrection>)> = if raw.format == 6 {
        raw.registers.iter().map(|r| (r.name.as_str(), r.notes.as_slice(), r.correction.as_ref())).collect()
    } else {
        vec![("", raw.notes.as_slice(), None)]
    };
    if regs.is_empty() || regs.len() > u8::MAX as usize {
        return Err(err("expected at least one register"));
    }
    let register_stops = register_stops(&raw)?;
    if let Some(b) = &raw.buff {
        let ok = b.register < regs.len()
            && b.notes.len() == 88
            && b.flat.len() == 88
            && b.gain.len() == 88
            && b.pos.len() == 88
            && b.notes.iter().enumerate().all(|(i, &n)| n as usize == 21 + i)
            && b.flat.iter().chain(&b.gain).all(|g| g.is_finite() && *g >= 0.0)
            && b.pos.iter().all(|p| p.is_finite() && *p > 0.0 && *p <= 0.5);
        if !ok {
            return Err(err("buff: expected an existing register, notes 21…108, flat and gain ≥ 0, 0 < pos ≤ 0.5"));
        }
    }
    if let Some(l) = &raw.lute {
        let ok = l.register < regs.len()
            && l.beta.len() == 88
            && l.beta.iter().all(|b| b.is_finite() && *b > 0.0 && *b < 0.5)
            && l.gain_db.is_finite();
        if !ok {
            return Err(err("lute: expected an existing register, 88 pluck points in (0, 0.5), a finite gain"));
        }
    }
    let raw_notes: Vec<(u8, &RawNote)> =
        regs.iter().enumerate().flat_map(|(r, (_, ns, _))| ns.iter().map(move |n| (r as u8, n))).collect();
    let mut notes = Vec::with_capacity(raw_notes.len());
    for &(register, n) in &raw_notes {
        let st = &n.string;
        let hammer = |h: &RawHammer| HammerSpec {
            mass: h.mass,
            felt_k: h.felt_k,
            felt_p: h.felt_p,
            balance: h.balance.clone(),
            strike_pos: h.strike_pos,
            core_k: h.core_k,
            core_depth: h.core_depth,
        };
        let excitation = match (&n.excitation, &n.hammer) {
            (Some(RawExcitation::Hammer(h)), _) | (None, Some(h)) => Excitation::Hammer(hammer(h)),
            (Some(RawExcitation::Pluck(p)), _) => Excitation::Pluck(PluckSpec {
                beta: p.beta,
                f_q: p.f_q,
                amp: p.amp,
                gain_db_per_step: p.gain_db_per_step,
                fq_oct_per_step: p.fq_oct_per_step,
                bar: p.bar,
                order: p.order,
                tangent: p.tangent,
            }),
            (None, None) => return Err(err(format!("note {}: no excitation", n.note))),
        };
        let excitation_ok = match &excitation {
            Excitation::Hammer(h) => h.balance.len() >= st.s,
            Excitation::Pluck(p) => p.beta > 0.0 && p.beta < 1.0 && p.order > 0.0 && (!p.bar || st.modes.is_some()),
        };
        let modes_ok = st.modes.as_ref().is_none_or(|m| {
            let n = m.ratio.len();
            n > 0
                && m.sigma.len() == n
                && m.phi.len() == n
                && m.out.len() == n
                && m.ratio.iter().all(|&r| r > 0.0)
                && m.harmonics.as_ref().is_none_or(RawHarmonics::ok)
                && m.split_hz.as_ref().is_none_or(|h| h.len() == n && h.iter().all(|x| x.is_finite()))
                && m.split_share.as_ref().is_none_or(|r| r.len() == n && r.iter().all(|x| (0.0..0.5).contains(x)))
        });
        if st.detune_cents.len() != st.s || !(1..=3).contains(&st.s) || !excitation_ok || !modes_ok {
            return Err(err(format!("note {}: inconsistent string count or excitation", n.note)));
        }
        let spec = NoteSpec {
            note: n.note,
            register,
            string: StringSpec {
                f0: st.f0,
                b: st.b,
                l: st.l,
                t: st.t,
                mass: st.mass,
                detune_cents: st.detune_cents.clone(),
                b1: st.b1,
                b3: st.b3,
                loss_q: st.loss_q,
                kappa: st.kappa,
                kappa_reactive: st.kappa_reactive,
                horiz_gain: st.horiz_gain,
                horiz_kappa: st.horiz_kappa,
                horiz_detune_cents: st.horiz_detune_cents,
                f_max: st.f_max,
                scatter_cents: st.scatter_cents,
                modes: st.modes.as_ref().map(|m| BarModes {
                    ratio: m.ratio.clone(),
                    sigma: m.sigma.clone(),
                    phi: m.phi.clone(),
                    out: m.out.clone(),
                    harmonics: m.harmonics.as_ref().map(RawHarmonics::compiled).unwrap_or_default(),
                    split: match (&m.split_hz, &m.split_share) {
                        (Some(h), Some(r)) => h.iter().zip(r).map(|(&h, &r)| (h, r)).collect(),
                        _ => Vec::new(),
                    },
                    halo_from: m.halo_from.unwrap_or(m.ratio.len()).min(m.ratio.len()),
                }),
            },
            excitation,
            has_damper: n.has_damper,
            damper_sigma: n.damper_sigma.as_ref().map(|r| d.f64(r)).transpose()?,
            buff: raw.buff.as_ref().filter(|b| b.register == register as usize).and_then(|b| {
                let i = (n.note as usize).checked_sub(21)?;
                Some((*b.flat.get(i)?, *b.gain.get(i)?, *b.pos.get(i)?))
            }),
            lute: raw.lute.as_ref().filter(|l| l.register == register as usize).and_then(|l| {
                Some((*l.beta.get((n.note as usize).checked_sub(21)?)?, l.gain_db))
            }),
            recorded_cents: if n.recorded_cents.is_finite() { n.recorded_cents.clamp(-100.0, 100.0) } else { 0.0 },
            sustain: if n.sustain.is_finite() && n.sustain > 0.0 { n.sustain.clamp(MIN_SUSTAIN, MAX_SUSTAIN) } else { 1.0 },
        };
        notes.push(Note {
            note: n.note,
            register,
            has_damper: n.has_damper,
            membrane_gain: n.membrane_gain.filter(|g| g.is_finite() && *g >= 0.0).unwrap_or(1.0),
            impact_gain: if n.impact_db.is_finite() { 10f64.powf(n.impact_db.clamp(-60.0, 24.0) / 20.0) } else { 1.0 },
            level_db: n.level_db.iter().map(|t| t.map(|v| if v.is_finite() { v.clamp(-48.0, 48.0) } else { 0.0 })).collect(),
            spec,
            phys: Physics::default(),
            // noise bursts: synthesized below, in parallel
            knock: match (&n.knock, &raw.knock) {
                (Some(k), Some(_)) => Some(Knock { knee: k.knee, burst: std::sync::Arc::from([]) }),
                _ => None,
            },
            release: Release {
                burst: std::sync::Arc::from([]),
                peak: n.release.peak,
                ratio_db: n.release.ratio_db.clone(),
            },
        });
    }
    for (r, (_, ns, _)) in regs.iter().enumerate() {
        if ns.len() != 88 || ns.iter().enumerate().any(|(i, n)| n.note as usize != 21 + i) {
            return Err(err(format!("register {r}: expected notes 21…108 in order")));
        }
    }
    let two = |r: &ArrRef| -> Result<[Vec<f64>; 2], Error> {
        let v = d.f64(r)?;
        let n = v.len() / 2;
        Ok([v[..n].to_vec(), v[n..].to_vec()])
    };
    let board_f = d.f64(&raw.board.f)?;
    // a mode at or below 0 Hz (a sparse board's jittered grid) broke the microphones' body
    // response: its spectrum's inverse transform fails on the audio thread
    if board_f.iter().any(|f| !(f.is_finite() && *f > 0.0)) {
        return Err(err("instrument.json: a board mode at or below 0 Hz"));
    }
    let measured_from = raw.board.measured_from.unwrap_or(board_f.len()).min(board_f.len());
    if let Some(m) = &raw.board.mobility {
        let sorted = m.anchors.windows(2).all(|p| p[0] < p[1]);
        if m.anchors.is_empty() || m.anchors.len() != m.factors.len() || !sorted || m.factors.iter().any(|v| !(*v > 0.0)) || !(m.fc_hz > 0.0) {
            return Err(err("instrument.json: the board's mobility needs sorted anchors, one positive factor each, a positive fc_hz"));
        }
    }
    let globals = Globals {
        board: Board {
            f: board_f,
            sigma: d.f64(&raw.board.sigma)?,
            phi: d.f64(&raw.board.phi)?,
            density: raw.board.density,
            r: two(&raw.board.r)?,
            r_h: two(&raw.board.r_h)?,
            measured_from,
            h_share: raw.board.h_share,
            mobility: raw.board.mobility.as_ref().map(mobility_of),
        },
        correction: raw.correction.as_ref().map(|c| correction(&d, c)).transpose()?,
        damper_log2: (raw.damper_log2[0], raw.damper_log2[1]),
        sym_f_max: raw.sympathetic.f_max,
    };
    let mut extra_globals = Vec::new();
    for (_, _, c) in &regs[1..] {
        let mut g = globals.clone();
        if let Some(c) = c {
            g.correction = Some(correction(&d, c)?);
        }
        extra_globals.push(g);
    }
    let register_names = regs.iter().map(|(name, _, _)| name.to_string()).collect();
    let (sp, rsr) = (raw.mic_spacing, raw.reference_sr);
    let threads = crate::threads::build_threads();
    let per = notes.len().div_ceil(threads);
    let (rad_v, rad_h) = std::thread::scope(|scope| {
        for (chunk, raws) in notes.chunks_mut(per).zip(raw_notes.chunks(per)) {
            let (kp, rp) = (&raw.knock, &raw.release);
            scope.spawn(move || {
                for (n, (_, rn)) in chunk.iter_mut().zip(raws) {
                    if let (Some(k), Some(kp)) = (n.knock.as_mut(), kp) {
                        k.burst = noise::knock_burst(n.note, kp.decay_s, kp.cutoff_hz, kp.rise_s, sp, rsr).into();
                    }
                    let r = &rn.release;
                    // a seed per register (register 0: as before)
                    let seed = 5000 + n.note as u64 + 1000 * n.register as u64;
                    n.release.burst =
                        noise::noise_burst(seed, &r.impacts, &r.band_db, &rp.bands_hz, rp.length_s, rp.rise_s, sp, rsr).into();
                }
            });
        }
        let rad = &raw.radiation;
        let h = scope.spawn(|| parts(&d, &rad.h, &rad.room, sp, rsr));
        let v = parts(&d, &rad.v, &rad.room, sp, rsr);
        (v, h.join().unwrap_or_else(|_| Err(err("room synthesis failed"))))
    });
    let thumps = |takes: &[RawThump], p: &RawPedal| -> Vec<Thump> {
        takes
            .iter()
            .map(|t| Thump {
                burst: noise::noise_burst(t.seed, &t.impacts, &t.band_db, &p.bands_hz, p.length_s, raw.release.rise_s, sp, rsr).into(),
                ratio_db: t.ratio_db,
            })
            .collect()
    };
    let pedal = raw.pedal.as_ref().map(|p| Pedal {
        kick_amp: p.kick.amp,
        kick_center: p.kick.center,
        kick_spread: p.kick.spread,
        kick_rolloff: p.kick.rolloff,
        kick_seed: p.kick_seed,
        down: thumps(&p.down, p),
        up: thumps(&p.up, p),
    });
    let mut inst = Instrument {
        body_kind: crate::mic::Body::from_name(raw.body.as_deref()),
        motor: raw.motor,
        bend: raw.bend,
        membrane: raw
            .membrane
            .as_ref()
            .filter(|m| m.level.is_finite() && m.level > 0.0)
            .map(|m| Membrane {
                level: m.level,
                hp_hz: m.hp_hz,
                hp_ratio: m.hp_ratio.max(0.0),
                lp_hz: m.lp_hz,
                poles: m.poles.clamp(1, MEMBRANE_POLES),
                growth: m.growth,
                tilt: m.tilt,
                sympathetic: m.sympathetic,
                power: m.power.clamp(0.5, 4.0),
            }),
        tangent: notes.iter().any(|n: &Note| matches!(n.spec.excitation, Excitation::Pluck(p) if p.tangent)),
        mallets: raw
            .mallets
            .iter()
            .map(|m| Mallet {
                name: m.name.clone(),
                f_q: m.f_q,
                fq_oct_per_step: m.fq_oct_per_step,
                order: m.order,
                speed: m.speed,
                knock: 10f64.powf(m.knock_db / 20.0),
                buzz: 10f64.powf(m.buzz_db / 20.0),
            })
            .collect(),
        profile: raw.profile,
        content,
        reference_sr: raw.reference_sr,
        output_gain_db: raw.output_gain_db,
        phantom: raw.phantom,
        velocity_midi: raw.velocity.midi,
        velocity_speed: raw.velocity.speed,
        dynamics: Dynamics::default(),
        contact_max_s: raw.contact.max_s,
        contact_separation_s: raw.contact.separation_s,
        knock: raw.knock.map(|k| (k.amp, k.q)),
        impact: raw.impact,
        pedal,
        transfer: raw.sympathetic.transfer,
        globals,
        compiled_sr: 0.0,
        strings_print: 0,
        tuning: Tuning::default(),
        body_h: (
            raw.radiation.room.grid_hz.iter().map(|f| f.log2()).collect(),
            raw.radiation.room.body_h_db.clone().unwrap_or_else(|| raw.radiation.room.h_db.clone()),
        ),
        room_spec: raw.radiation.room.spec([raw.radiation.v.room_seed, raw.radiation.h.room_seed]),
        radiation_v: rad_v?,
        radiation_h: rad_h?,
        notes,
        register_names,
        register_stops,
        extra_globals,
    };
    inst.compile(rsr, Tuning::default());
    Ok(inst)
}

impl Instrument {
    /// The microphone geometry for this instrument's body.
    pub fn body(&self) -> crate::mic::Body {
        self.body_kind
    }

    pub fn load(dir: impl AsRef<Path>) -> Result<Self, Error> {
        let mut inst = build(decode(dir.as_ref())?)?;
        if inst.is_hammer_piano() {
            inst.dynamics = stored_dynamics(dir.as_ref());
        }
        Ok(inst)
    }

    /// Struck by hammers alone, the hammer's speed its velocity (no mallets, tangents, bars): the
    /// instruments a level table (`dynamics`) maps the velocity of.
    pub fn is_hammer_piano(&self) -> bool {
        self.mallets.is_empty()
            && !self.tangent
            && !self.body_kind.is_bars()
            && self.notes.iter().all(|n| matches!(n.spec.excitation, crate::physics::Excitation::Hammer(_)))
    }

    /// An instrument between the instruments in `sides` (directory, weight): the weights not
    /// negative, normalized; a single one left (the others 0) is that instrument as loaded.
    /// Hammer-strung pianos only (`morph::check`).
    pub fn morph<P: AsRef<Path>>(sides: &[(P, f64)]) -> Result<Self, Error> {
        let w = morph::weights(&sides.iter().map(|s| s.1).collect::<Vec<_>>())?;
        let kept: Vec<(&Path, f64)> = sides.iter().zip(&w).filter(|(_, w)| **w > 0.0).map(|((p, _), w)| (p.as_ref(), *w)).collect();
        let first = sides[0].0.as_ref();
        if let [(dir, _)] = kept[..]
            && w[0] > 0.0
        {
            return Self::load(dir); // the main instrument alone
        }
        // the main instrument gives the body (the microphones' geometry), at any weight
        let main = decode(first)?;
        morph::check(&main)?;
        let sum: f64 = kept.iter().map(|k| k.1).sum();
        let dirs: Vec<&Path> = kept.iter().map(|k| k.0).collect();
        let mut decoded = Vec::with_capacity(kept.len());
        for (dir, w) in kept {
            let d = decode(dir)?;
            morph::check(&d)?;
            decoded.push((d, w / sum));
        }
        let w: Vec<f64> = decoded.iter().map(|d| d.1).collect();
        // each side's own level: stored with it, else built and measured (in parallel)
        let levels: Vec<Result<f64, Error>> = std::thread::scope(|scope| {
            let runs: Vec<_> = decoded
                .iter()
                .zip(&dirs)
                .map(|((d, _), dir)| scope.spawn(move || stored_level(dir).map_or_else(|| build(d.clone())?.ff_level(), Ok)))
                .collect();
            runs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(err("morph: a side's build failed")))).collect()
        });
        let levels = levels.into_iter().collect::<Result<Vec<_>, _>>()?;
        let tables: Vec<(Dynamics, f64)> = decoded.iter().zip(&dirs).map(|((d, _), dir)| (stored_dynamics(dir), d.raw.velocity.speed[2])).collect();
        let mut inst = build(morph::morph(decoded, &main)?)?;
        morph::morph_level(&mut inst, &levels, &w)?;
        inst.dynamics = morph::morph_dynamics(&tables, &w, inst.velocity_speed[2]);
        Ok(inst)
    }

    /// The instrument's level as the engine plays it (instrument.json's `level`): its ff level (`ff_level`, which a morph
    /// reads) and, a hammer piano's, its level table (`level_curve`, `speed_db`).
    pub fn level_doc(&self) -> Result<serde_json::Value, Error> {
        let mut doc = serde_json::json!({ "ff_db": self.ff_level()?, "keys": LEVEL_KEYS });
        let (table, key_ff) = self.level_rows()?;
        if !table.is_empty() {
            doc["speed_db"] = table.to_json();
            // each key's own ff level, which the table's rows are relative to (proto/even_levels.py)
            doc["key_ff_db"] = serde_json::json!(key_ff);
        }
        Ok(doc)
    }

    /// The level table (`dynamics`) as the engine plays this instrument: each of `LEVEL_TABLE_KEYS`
    /// struck at `LEVEL_SPEEDS` steps around the ff anchor's speed at the recording's position, the
    /// knock off, the peak over `LEVEL_WINDOW_S` re the key's own at the ff speed; empty unless a
    /// hammer piano.
    /// About 2.5 s on 16 threads.
    pub fn level_curve(&self) -> Result<Dynamics, Error> {
        Ok(self.level_rows()?.0)
    }

    /// Each key's felt as it plays (its first register's note; `felt::KeyFelt`): the force at
    /// 0.5 mm as the report reads it (`felt_K · 0.5 mm^p`), keys without a hammer marked so.
    pub fn felt(&self) -> FeltKeys {
        let mut out = [KeyFelt::default(); 88];
        for n in self.notes.iter().filter(|n| n.register == 0 && (21..=108).contains(&n.note)) {
            if let crate::physics::Excitation::Hammer(h) = &n.spec.excitation {
                let at = 5e-4f64.powf(h.felt_p);
                let core = h.core_k > 0.0;
                out[n.note as usize - 21] = KeyFelt {
                    hammer: true,
                    f05: h.felt_k * at,
                    p: h.felt_p,
                    core05: h.core_k * at,
                    core_depth_mm: if core { h.core_depth * 1e3 } else { felt::DEPTH_WITHOUT_CORE_MM },
                    core,
                };
            }
        }
        out
    }

    /// `keys`' felt on every note of each key (every register), its spec and its compiled physics:
    /// K = f05 / 0.5 mm^p, the core's alike (none: 0, its depth kept).
    pub fn set_felt(&mut self, keys: &FeltKeys) {
        for n in self.notes.iter_mut().filter(|n| (21..=108).contains(&n.note)) {
            let k = &keys[n.note as usize - 21];
            let crate::physics::Excitation::Hammer(h) = &mut n.spec.excitation else { continue };
            if !k.hammer {
                continue;
            }
            let at = 5e-4f64.powf(k.p);
            h.felt_p = k.p;
            h.felt_k = k.f05 / at;
            h.core_k = if k.core { k.core05 / at } else { 0.0 };
            if k.core {
                h.core_depth = k.core_depth_mm * 1e-3;
            }
            let s = n.phys.felt_k.len();
            n.phys.felt_k = h.balance.iter().take(s).map(|b| h.felt_k * b).collect();
            n.phys.core_k = h.balance.iter().take(s).map(|b| h.core_k * b).collect();
            n.phys.felt_p = h.felt_p;
            n.phys.core_depth = h.core_depth;
        }
    }

    /// This instrument with the player's felt (design 2026-10-22-felt-controls): `edit` bent onto
    /// each key's felt, the level table measured again and each key's level shifted by its ff level
    /// with the shipped felt (`shipped_rows`, `level_rows`) minus its own, so every key keeps its
    /// loudness at every velocity. The instrument as it is without an edit or a hammer piano.
    pub fn with_felt_edit(&self, edit: &FeltEdit, shipped_rows: &(Dynamics, Vec<f64>)) -> Result<Instrument, Error> {
        if edit.is_shipped() || !self.is_hammer_piano() {
            return Ok(self.clone());
        }
        Ok(self.with_felt_leveling(edit, &self.felt_leveling(edit, shipped_rows)?))
    }

    /// What `edit` does to this instrument's levels (`with_felt_edit`), measured: the level table it
    /// plays and each key's level shift. A hammer piano's.
    pub fn felt_leveling(&self, edit: &FeltEdit, shipped_rows: &(Dynamics, Vec<f64>)) -> Result<FeltLeveling, Error> {
        let mut inst = self.clone();
        inst.set_felt(&felt::apply(&self.felt(), edit));
        let (table, key_ff) = inst.level_rows()?;
        let shipped_ff = &shipped_rows.1;
        if key_ff.len() != LEVEL_TABLE_KEYS.len() || shipped_ff.len() != key_ff.len() {
            return Err(err("felt: the level rows are not every key's"));
        }
        Ok(FeltLeveling { dynamics: played_table(&self.dynamics, &shipped_rows.0, table), shift: shipped_ff.iter().zip(&key_ff).map(|(s, k)| s - k).collect() })
    }

    /// The instrument with `edit`'s felt and a leveling (`felt_leveling`: this instrument's, or
    /// another's near it standing in until it is measured, a morph's pad dragged), unmeasured.
    pub fn with_felt_leveling(&self, edit: &FeltEdit, lv: &FeltLeveling) -> Instrument {
        let mut inst = self.clone();
        if edit.is_shipped() || !self.is_hammer_piano() {
            return inst;
        }
        inst.set_felt(&felt::apply(&self.felt(), edit));
        inst.dynamics = lv.dynamics.clone();
        for n in inst.notes.iter_mut().filter(|n| (21..=108).contains(&n.note)) {
            let shift = lv.shift.get(n.note as usize - 21).copied().unwrap_or(0.0);
            if n.level_db.is_empty() {
                n.level_db.push([shift; 3]);
            } else {
                n.level_db.iter_mut().for_each(|row| row.iter_mut().for_each(|d| *d += shift));
            }
        }
        inst
    }

    /// The level table (`level_curve`) and each table key's own level at the ff anchor's speed (dB,
    /// the peak over `LEVEL_WINDOW_S`), the row the table is relative to; both empty unless a hammer
    /// piano.
    pub fn level_rows(&self) -> Result<(Dynamics, Vec<f64>), Error> {
        if !self.is_hammer_piano() {
            return Ok((Dynamics::default(), Vec::new()));
        }
        let (inst, sr) = (self, self.reference_sr);
        let ff = self.velocity_speed[2].max(1e-3);
        let speeds: Vec<f64> = LEVEL_SPEEDS.map(|k| ff * 2f64.powf(k as f64 / 2.0)).to_vec();
        let (nk, ns) = (LEVEL_TABLE_KEYS.len(), speeds.len());
        let next = std::sync::atomic::AtomicUsize::new(0);
        let threads = crate::threads::build_threads();
        // each thread its renders (key, speed index, peak dB) through its own radiation cache
        let found: Vec<Result<Vec<(usize, usize, f64)>, Error>> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..threads)
                .map(|_| {
                    let (speeds, next) = (&speeds, &next);
                    scope.spawn(move || -> Result<Vec<(usize, usize, f64)>, Error> {
                        let mut cache = crate::RadiationCache::default();
                        let mut out = Vec::new();
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if i >= nk * ns {
                                return Ok(out);
                            }
                            let (k, j) = (i / ns, i % ns);
                            let key = LEVEL_TABLE_KEYS[k];
                            let mut e = crate::Engine::for_window(inst.clone(), sr, Tuning::default(), &mut cache, LEVEL_WINDOW_S)?;
                            // the note without the action's knock: at soft strikes the knock sets the
                            // peak (Knight's C7 at −32 dB re ff, its string at −68 dB); and as fitted,
                            // without a strike's own variation
                            e.set_params(crate::Params { knock: 0.0, variation: 0.0, ..crate::Params::default() });
                            e.queue(0, crate::Event::NoteOn { key, speed: speeds[j] });
                            let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
                            let mut peak = 0.0f32;
                            for _ in 0..(LEVEL_WINDOW_S * sr / 256.0).ceil() as usize {
                                e.process(&mut l, &mut r);
                                peak = l.iter().chain(&r).fold(peak, |m, x| m.max(x.abs()));
                            }
                            if !(peak > 0.0) {
                                return Err(err(format!("level: key {key} is silent at {:.3} m/s", speeds[j])));
                            }
                            out.push((k, j, 20.0 * (peak as f64).log10()));
                        }
                    })
                })
                .collect();
            runs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(err("level: a render failed")))).collect()
        });
        let mut db = vec![vec![0.0; ns]; nk];
        for (k, j, d) in found.into_iter().collect::<Result<Vec<_>, _>>()?.into_iter().flatten() {
            db[k][j] = d;
        }
        let at_ff = LEVEL_SPEEDS.iter().position(|&k| k == 0).unwrap();
        let key_ff: Vec<f64> = db.iter().map(|row| row[at_ff]).collect();
        for row in &mut db {
            let f = row[at_ff];
            row.iter_mut().for_each(|d| *d -= f);
        }
        Ok((Dynamics { keys: LEVEL_TABLE_KEYS.to_vec(), ln_speed: speeds.iter().map(|s| s.ln()).collect(), db }, key_ff))
    }

    /// The instrument's ff level: `LEVEL_KEYS` each struck at its ff anchor's speed at the recording's
    /// position, the peak over 2 s, the mean in dB.
    pub fn ff_level(&self) -> Result<f64, Error> {
        let inst = self;
        let sr = inst.reference_sr;
        let levels: Vec<Result<f64, Error>> = std::thread::scope(|scope| {
            let runs: Vec<_> = LEVEL_KEYS
                .iter()
                .map(|&key| {
                    scope.spawn(move || -> Result<f64, Error> {
                        let mut e = crate::Engine::with_tuning(inst.clone(), sr, Tuning::default())?;
                        // as fitted, without a strike's own variation
                        e.set_params(crate::Params { variation: 0.0, ..crate::Params::default() });
                        e.queue(0, crate::Event::NoteOn { key, speed: inst.velocity_speed[2] });
                        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
                        let mut peak = 0.0f32;
                        for _ in 0..(2.0 * sr / 256.0).ceil() as usize {
                            e.process(&mut l, &mut r);
                            peak = l.iter().chain(&r).fold(peak, |m, x| m.max(x.abs()));
                        }
                        if !(peak > 0.0) {
                            return Err(err(format!("morph: key {key} is silent")));
                        }
                        Ok(20.0 * (peak as f64).log10())
                    })
                })
                .collect();
            runs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(err("morph: a level render failed")))).collect()
        });
        Ok(levels.into_iter().collect::<Result<Vec<_>, _>>()?.iter().sum::<f64>() / LEVEL_KEYS.len() as f64)
    }

    /// Number of registers (sets of strings per key; the notes are register-major).
    pub fn registers(&self) -> usize {
        self.notes.len() / 88
    }

    /// Whether any string has the buff stop's pads (`Params::buff` acts on it).
    pub fn has_buff(&self) -> bool {
        self.notes.iter().any(|n| !n.buff_sigma.is_empty())
    }

    /// Whether any string has a lute row (`Params::lute` acts on it).
    pub fn has_lute(&self) -> bool {
        self.notes.iter().any(|n| n.spec.lute.is_some())
    }

    /// (Re)derive every note's physics for sample rate `sr` and `tuning` from its
    /// physical parameters (`physics::compile`, notes in parallel).
    pub fn compile(&mut self, sr: f64, tuning: Tuning) {
        // the board's bass on a hammer piano's strings (`board_bass`)
        let colored = if self.is_hammer_piano() {
            crate::board_bass::corrected(self.globals.correction.as_ref(), crate::board_bass::sanitize(tuning.board_bass_db))
                .map(|c| crate::physics::Globals { correction: Some(c), ..self.globals.clone() })
        } else {
            None
        };
        let g = colored.as_ref().unwrap_or(&self.globals);
        let extra = &self.extra_globals;
        // the wear this kind of instrument can show
        let worn = Tuning { wear: tuning.wear.on(self.body()), ..tuning };
        let tu = &worn;
        // tuner's stretch per note, from each register's own inharmonicities; plucked
        // instruments are not tuned with a stretch (their strings are too thin for it)
        let mut stretch = vec![0.0; self.notes.len()];
        let plucked = self.notes.iter().any(|n| matches!(n.spec.excitation, crate::physics::Excitation::Pluck(_)));
        if tuning.stretch != 0.0 && !plucked {
            for r in 0..=self.notes.iter().map(|n| n.register).max().unwrap_or(0) {
                let idx: Vec<usize> = (0..self.notes.len()).filter(|&i| self.notes[i].register == r).collect();
                let b: Vec<f64> = idx.iter().map(|&i| self.notes[i].spec.string.b).collect();
                let lo = idx.first().map_or(21, |&i| self.notes[i].spec.note);
                for (&i, c) in idx.iter().zip(crate::stretch::offsets(lo, &b)) {
                    stretch[i] = tuning.stretch * c;
                }
            }
        }
        let stretch = &stretch;
        let threads = crate::threads::build_threads();
        let per = self.notes.len().div_ceil(threads);
        std::thread::scope(|scope| {
            for (ci, chunk) in self.notes.chunks_mut(per).enumerate() {
                scope.spawn(move || {
                    for (k, n) in chunk.iter_mut().enumerate() {
                        let g = if n.register == 0 { g } else { &extra[n.register as usize - 1] };
                        n.phys = crate::physics::compile(&n.spec, g, sr, tu, stretch[ci * per + k]);
                    }
                });
            }
        });
        // the harmonics' level reference: the fundamental's state after an ff strike from rest
        let ff_steps = self.velocity_speed[2].max(1e-3).log2();
        for n in &mut self.notes {
            let (f1, b1, pluck) = (n.phys.f_mean.first().copied(), n.phys.bridge.first().copied(), n.phys.pluck);
            if let (Some(t), Some(f1), Some(b1), Some(p)) = (n.phys.tube.as_mut(), f1, b1, pluck) {
                t.a_ref = (b1 * crate::physics::pluck_amplitude(1.0, f1, &p, ff_steps, 1.0)).abs();
            }
        }
        self.compiled_sr = sr;
        self.tuning = tuning;
        self.strings_print = strings_print(&self.notes, sr);
    }
}

/// Every note's compiled physics hashed (each number's bits) with the rate. The destructuring names
/// every field: one added to `Physics` does not compile until it is hashed here too.
fn strings_print(notes: &[Note], sr: f64) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let f = |h: &mut std::collections::hash_map::DefaultHasher, xs: &[f64]| {
        h.write_usize(xs.len());
        xs.iter().for_each(|x| h.write_u64(x.to_bits()));
    };
    let c = |h: &mut std::collections::hash_map::DefaultHasher, xs: &[realfft::num_complex::Complex64]| {
        h.write_usize(xs.len());
        xs.iter().for_each(|x| {
            h.write_u64(x.re.to_bits());
            h.write_u64(x.im.to_bits());
        });
    };
    h.write_u64(sr.to_bits());
    for n in notes {
        let crate::physics::Physics {
            s,
            k,
            w,
            sigma_int,
            kappa_re,
            phi_h,
            strike,
            bridge,
            modal_mass,
            felt_k,
            felt_p,
            core_k,
            core_depth,
            hammer_mass,
            horiz_gain,
            vert,
            horiz,
            damper_sigma,
            buff_sigma,
            sym,
            split,
            pluck,
            f_mean,
            tube,
            halo_from,
        } = &n.phys;
        h.write_usize(*s);
        h.write_usize(*k);
        let crate::physics::Sympathetic { f: sym_f, sigma: sym_sigma, kappa: sym_kappa, damper_sigma: sym_damper, buff_sigma: sym_buff } = sym;
        for xs in [w, sigma_int, kappa_re, phi_h, bridge, felt_k, core_k, damper_sigma, buff_sigma, f_mean, sym_f, sym_sigma, sym_damper, sym_buff] {
            f(&mut h, xs);
        }
        f(&mut h, &[*modal_mass, *felt_p, *core_depth, *hammer_mass, *horiz_gain]);
        f(&mut h, &strike.map_or([f64::NAN; 2], |(a, b)| [a, b]));
        for crate::physics::Polarization { lam, colsum, vinv, .. } in [vert, horiz] {
            for xs in [lam, colsum, vinv] {
                c(&mut h, xs);
            }
        }
        c(&mut h, sym_kappa);
        for &(j, a, b) in split {
            h.write_usize(j);
            f(&mut h, &[a, b]);
        }
        h.write_usize(*halo_from);
        // small and rare: their Debug form
        h.write(format!("{pluck:?}{tube:?}").as_bytes());
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loss_q_absent_is_two() {
        // the strings' loss exponent: absent in every export before it, the law as it was
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a/instrument.json");
        let Ok(text) = std::fs::read_to_string(dir) else { return };
        let j: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut s = j["notes"][15]["string"].clone();
        s.as_object_mut().unwrap().remove("loss_q");
        assert_eq!(serde_json::from_value::<RawString>(s.clone()).unwrap().loss_q, 2.0);
        s["loss_q"] = serde_json::json!(1.5);
        assert_eq!(serde_json::from_value::<RawString>(s).unwrap().loss_q, 1.5);
    }

    #[test]
    fn no_core_when_absent() {
        // the felt's core: absent in every export without one, none
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a/instrument.json");
        let Ok(text) = std::fs::read_to_string(dir) else { return };
        let j: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut h = j["notes"][15]["excitation"].clone();
        h.as_object_mut().unwrap().remove("type");
        h.as_object_mut().unwrap().remove("core_K");
        h.as_object_mut().unwrap().remove("core_depth");
        let r = serde_json::from_value::<RawHammer>(h.clone()).unwrap();
        assert_eq!((r.core_k, r.core_depth), (0.0, 0.0));
        h["core_K"] = serde_json::json!(5e8);
        h["core_depth"] = serde_json::json!(4e-4);
        let r = serde_json::from_value::<RawHammer>(h).unwrap();
        assert_eq!((r.core_k, r.core_depth), (5e8, 4e-4));
    }

    #[test]
    fn pushed_arrays_read_back() {
        let mut d = Data(Vec::new());
        let a = d.push_f64(&[1.5, -2.0]);
        let b = d.push_f32(&[0.25]);
        assert_eq!(d.f64(&a).unwrap(), vec![1.5, -2.0]);
        assert_eq!(d.f32(&b).unwrap(), vec![0.25]);
        assert_eq!((b.o, b.n, b.t.as_str()), (16, 1, "f32"));
    }
}
