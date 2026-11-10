//! The engine: keys, pedal, voices, sympathetic bank, radiation, knock and release noise.
//!
//! Events are queued at absolute sample positions and applied before that sample is
//! computed. The engine renders in sub-blocks that end at every event and every `BASE`
//! boundary; radiation is convolved per `BASE` block, so the output lags the input by
//! `BASE` samples (`latency()`).

use std::collections::VecDeque;
use std::sync::Arc;

use crate::convolution::Convolver;
use crate::instrument::{Error, Instrument, Radiation};
use crate::kernel::{self, LANES, Lane};
use crate::motor::Motor;
use crate::physics::Tuning;
use crate::resample::resample;
use crate::resonance::{Bank, KeyState};
use crate::telemetry::{self, Frame, KEY_WORDS, KeyFrame, MAX_VOICES, Region, VoiceFrame};
use crate::voice::{ImpactFilter, Voice};

/// engine block (samples): the latency, and the granularity of mode re-synchronization
pub const BASE: usize = 64;
/// larger partitions of the radiation convolution
const CONV_STAGES: [usize; 1] = [1024];
const MAX_OVERLAYS: usize = 256;
/// handover (`Engine::adopt`) to other microphone settings: where the new responses take the old
/// ones' input history (`Radiator::take_history`) both sound the whole input, and the outputs
/// crossfade over OUT_FADE_S (it only hides the switch); else the input crossfades over FADE_S and
/// the old responses ring out, up to MAX_FADING at a time
const FADE_S: f64 = 0.1;
const OUT_FADE_S: f64 = 0.03;
const MAX_FADING: usize = 4;
/// panic (`Engine::silence`): the output's fade to nothing (s, raised cosine)
pub const SILENCE_S: f64 = 0.02;
/// The output guard: a sample past this (+20 dBFS, × the Volume above 0 dB) or not finite clears
/// every sound at once (a panic without its fade). What is played stays well under it: a phrase's
/// peaks lie near −6 … −17 dBFS at the Volume's 0 dB (step 173), a loud chord's some dB more. The
/// voices' own guard (`voice::RUNAWAY`) catches a mode blowing up; this one a runaway the strikes
/// themselves feed.
const OUTPUT_GUARD: f32 = 10.0;
/// pedal depth range over which the dampers leave the strings (fully resting below,
/// fully lifted above; CC64 ≈ 38…89). Half-pedalling: partial contact in between, the
/// damper loss scaled by the engagement.
const LIFT_RANGE: (f64, f64) = (0.3, 0.7);
/// engagement steps (each change recomputes the poles of the sounding modes)
const ENGAGE_STEPS: f64 = 32.0;
/// a damper falls onto the strings in this long at a normal release (MIDI release velocity
/// 64): shorter for a faster release (velocity 127: 12 ms), longer for a slower one, within
/// the limits; the user's factor scales it (0: instant). Starting values, tuned by ear.
/// the muted piano's hand or felt presses harder than a damper at rest does: the damper's
/// engagement at full strength (the damper loss scales with it). Tuned by ear.
const MUTE_ENGAGEMENT: f64 = 1.8;
/// the prepared piano's pluck: modal amplitude at a mezzo-forte (matches a struck note's level
/// within a few dB), the fingertip's lowpass (Hz), and how much harder plucks gain (dB, and
/// octaves of lowpass per velocity doubling). Starting values, tuned by ear.
const PLUCK_AMP: f64 = 0.75e-3;
const PLUCK_FQ: f64 = 2500.0;
const PLUCK_DB_PER_STEP: f64 = 5.0;
const PLUCK_OCT_PER_STEP: f64 = 0.5;
const DAMPER_FALL_S: f64 = 0.025;
const DAMPER_FALL_LIMITS: (f64, f64) = (0.006, 0.2);
/// release noise level re the strike-based one per release velocity: (v / 64)^q, within
const RELEASE_VELOCITY_Q: f64 = 0.6;
const RELEASE_VELOCITY_GAIN: (f64, f64) = (0.4, 1.8);
/// the Bebung: a pressure's pitch rise settles with this time constant (a finger's pressure, and
/// MIDI's 7-bit steps smoothed), snaps to its target within BEBUNG_SNAP_CENTS; the depth at most
/// BEBUNG_MAX_CENTS
const BEBUNG_TAU_S: f64 = 0.01;
const BEBUNG_SNAP_CENTS: f64 = 0.01;
const BEBUNG_MAX_CENTS: f32 = 200.0;
/// the pitch wheel's range at most (semitones)
const BEND_MAX_SEMITONES: f32 = 12.0;

/// A controller's pressure (0 … 1; not finite: 0).
fn pressure(v: f32) -> f64 {
    if v.is_finite() { v.clamp(0.0, 1.0) as f64 } else { 0.0 }
}

/// release noise of a key held for a short time: the key has hardly traveled, so the thump
/// is smaller. Its gain rises linearly from this at 0 s to 1 at `RELEASE_HELD_FULL_S`.
const RELEASE_HELD_MIN: f64 = 0.4;
const RELEASE_HELD_FULL_S: f64 = 0.12;
/// live wear at amount 1: felt stiffness +dB and exponent offset (compressed hammers),
/// ± dB per key (unevenly worn hammers), damper loss − share, action noise +dB
const HAMMER_WEAR_DB: f64 = 12.0;
const HAMMER_WEAR_DP: f64 = 0.6;
const VOICING_WEAR_DB: f64 = 4.0;
/// the voicing's level scatter: σ (dB) per key at wear 1, growing as √wear (1 dB at 0.05,
/// the Condition's Normal; tuned by ear)
const VOICING_LEVEL_DB: f64 = 4.5;
const DAMPER_WEAR: f64 = 0.75;
const ACTION_WEAR_DB: f64 = 9.0;
/// rattle: a loose part touching above RATTLE_TH × an ff bass note's peak bridge force
/// (E1, measured at load), high-passed, RATTLE_LEVEL at amount 1
const RATTLE_TH: f64 = 0.35;
const RATTLE_LEVEL: f64 = 0.6;
const RATTLE_HP: f64 = 800.0;
/// phantom partials at amount a: level PHANTOM_LEVEL·a/PHANTOM_REF (the strongest approved
/// sketch at a = 0.55)
const PHANTOM_LEVEL: f64 = 2.8;
const PHANTOM_REF: f64 = 0.55;
/// phantom partials play in the bass: in full up to the first key, fading to none at the second
/// (C2, C3). The recordings' energy between the partials grows with the square of the note in
/// A0–A2, not from C3 (design log step 131); over all keys the control's partials put loud treble
/// notes up to 20–30 dB over their neighbors (step 190). A voice with phantom partials leaves the
/// shared accumulators (+30–50 % load on all 88 keys).
const PHANTOM_BASS: (f64, f64) = (36.0, 48.0);

/// The share of the phantom partials key `note` plays (`PHANTOM_BASS`): 1 … 0.
fn phantom_share(note: f64) -> f64 {
    ((PHANTOM_BASS.1 - note) / (PHANTOM_BASS.1 - PHANTOM_BASS.0)).clamp(0.0, 1.0)
}

/// Damper engagement for a pedal depth: 1 resting … 0 lifted (smoothstep, quantized).
pub fn pedal_engage(depth: f64) -> f64 {
    let x = ((depth - LIFT_RANGE.0) / (LIFT_RANGE.1 - LIFT_RANGE.0)).clamp(0.0, 1.0);
    let e = 1.0 - x * x * (3.0 - 2.0 * x);
    (e * ENGAGE_STEPS).round() / ENGAGE_STEPS
}
/// the soft pedal at full depth. An upright's hammer speed factor (the hammer rail). A
/// grand's felt stiffness change (dB; the shifted hammer strikes with a less used part of
/// the felt) and hammer speed factor (less swing). Starting values, tuned by ear: no
/// recording of either.
const SOFT_UPRIGHT_SPEED: f64 = 0.40;
const SOFT_GRAND_FELT_DB: f64 = -6.0;
const SOFT_GRAND_SPEED: f64 = 0.80;
/// output calibration: the median over the keys of the ff single-note peak lands here
const FF_MEDIAN_PEAK_DBFS: f64 = -15.0;
/// the audio input at level 1: bridge force per output model unit, about the median over the pianos
/// of their strings' own bridge force over the output it makes (an ff chord: −3 … +20 dB, the
/// median 7 dB; the guitar 45 dB)
const INPUT_DRIVE: f32 = 2.0;
/// the input's reciprocal response (`input_response`): the integrator's leak (Hz), and the band
/// whose mean power is normalized to 1 (Hz)
const INPUT_LEAK_HZ: f64 = 20.0;
const INPUT_NORM_HZ: (f64, f64) = (100.0, 4000.0);

#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// strike with a hammer speed (m/s)
    NoteOn { key: u8, speed: f64 },
    /// key pressed silently (damper lifted, no strike)
    KeyDown { key: u8 },
    KeyUp { key: u8 },
    /// the release velocity (MIDI 1…127; 64 is normal) of the next `KeyUp` of this key:
    /// how fast its damper falls and how loud the release thump is. Queued before the KeyUp.
    ReleaseVelocity { key: u8, velocity: u8 },
    PedalDown,
    PedalUp,
    /// continuous sustain pedal position, 0 (up) … 1 (fully down)
    Pedal { depth: f32 },
    /// sostenuto pedal: down latches the dampers that are lifted at that moment (keys held,
    /// or everything while the sustain pedal is down) off the strings until it is up again
    SostenutoDown,
    SostenutoUp,
    /// the soft pedal (una corda on a grand, the hammer rail on an upright), 0 (up) … 1
    /// (fully down); acts on the notes struck from then on
    SoftPedal { depth: f32 },
    /// all sound off (MIDI CC120): as `Engine::silence`, the events queued after it kept
    Silence,
    /// the pitch wheel, −1 … 1: the sounding notes bent by ± `Params::bend_range` (instruments
    /// whose strings bend, `Instrument::bend`; others ignore it)
    PitchBend { value: f32 },
    /// the mod wheel (MIDI CC1), 0 … 1: the Bebung's pressure on every held key, as the channel
    /// pressure (keyboards without aftertouch)
    ModWheel { value: f32 },
    /// channel pressure (aftertouch), 0 … 1: on every held key
    ChannelPressure { value: f32 },
    /// polyphonic key pressure, 0 … 1: on this key
    KeyPressure { key: u8, value: f32 },
}

#[derive(Clone, Copy)]
enum OneShot {
    Knock,
    Release,
    PedalDown,
    PedalUp,
}

#[derive(Clone, Copy)]
struct Overlay {
    /// key index (knock, release) or take (pedal)
    note: usize,
    kind: OneShot,
    pos: usize,
    gain: f32,
}

/// How the piano is prepared (John Cage's prepared piano): one at a time, for all keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prep {
    None,
    /// plucked with a finger or nail inside the piano instead of struck by the hammer
    Plucked,
    /// a hand or felt resting on the strings: the dampers half on while the key is down
    Muted,
    /// a finger touching the string at a node while the hammer strikes: a flageolet
    Harmonic,
    /// a bolt or screw on the strings (`Tuning::bolt`: it changes the strings, compiled)
    Bolt,
    /// paper or a loose part on the strings: buzzes
    Paper,
}

impl Prep {
    pub const COUNT: u32 = 6;
    pub fn from_index(i: u32) -> Prep {
        [Prep::None, Prep::Plucked, Prep::Muted, Prep::Harmonic, Prep::Bolt, Prep::Paper].get(i as usize).copied().unwrap_or(Prep::None)
    }
}

/// User parameters (linear gains and factors; 1 = as fitted).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// recording room relative to the instrument's own radiation
    pub room: f32,
    pub knock: f32,
    pub release: f32,
    /// the sustain pedal's thumps going down and up
    pub pedal: f32,
    /// the strings' ring as the sustain pedal lifts their dampers (`Pedal`'s kick)
    pub pedal_ring: f32,
    /// sympathetic resonance coupling (× the fitted TRANSFER)
    pub sympathetic: f32,
    /// hammer felt stiffness (× the fitted; force at a given compression), from the next strike
    pub hardness: f32,
    /// live wear (the piano's condition), 0 as fitted … 1 worn: hammer felt compressed,
    /// hammers worn unevenly, damper felt worn, action noisier (`physics::Wear` has the
    /// compiled parts)
    pub hammer_wear: f32,
    pub voicing_wear: f32,
    pub damper_wear: f32,
    pub action_wear: f32,
    /// phantom partials (tension modulation of old, stiff strings) beyond the instrument's own,
    /// 0 … 1, in the bass (`PHANTOM_BASS`)
    pub phantom: f32,
    /// optional defects, 0 off … 1 (the most they should be): hammer grooves (rough
    /// attack), damper buzz (chatter when a damper lands), rattle (a loose part on the
    /// soundboard buzzing on loud passages)
    pub grooves: f32,
    pub damper_buzz: f32,
    pub rattle: f32,
    pub volume: f32,
    /// time a damper takes to fall onto the strings, × the normal (1; 0 instant)
    pub damper_fall: f32,
    /// the prepared piano: how the strings are prepared (the whole instrument), and the
    /// parameters of the preparations
    pub prep: Prep,
    /// plucked: where along the string (fraction of its length), and how bright (× the
    /// finger's lowpass, 1 = a fingertip)
    pub pluck_pos: f32,
    pub pluck_bright: f32,
    /// muted: how hard the hand rests on the strings, 0 … 1 (the damper's engagement)
    pub mute: f32,
    /// harmonic: the node a finger touches (2 the octave, 3 the twelfth, …)
    pub harmonic: u32,
    /// paper: how much it buzzes, 0 … 1
    pub paper: f32,
    /// stops: the registers a note-on excites (bit r = register r: 0 the 8′, 1 the 4′, 2 the
    /// 8′ II), live; instruments with one register ignore it
    pub stops: u32,
    /// the harpsichords' buff stop (instruments with a buff block): from the next pluck of
    /// a key, its strings carry the pads' loss (`Physics::buff_sigma`), its sympathetic
    /// resonators at once; others ignore it
    pub buff: bool,
    /// the lute stop (instruments with a lute block): from the next pluck of a key, its
    /// lute register's strings are plucked by the lute row near the nut (also without that
    /// register's stop; with it, the lute wins: one row plucks those strings)
    pub lute: bool,
    /// the vibraphone's motor (instruments with resonators: bars; others ignore it): on,
    /// its rate (pulses per second) and depth (0 … 1, `motor::MOTOR_DIP_DB` at 1); off here,
    /// on in the plugin (its defaults, chosen by ear: 7 per second, 0.7)
    pub motor: bool,
    pub motor_rate: f32,
    pub motor_depth: f32,
    /// the mallet (instruments with several, `Instrument::mallets`: its index; others ignore
    /// it), from the next strike: its lowpass, and its speeds as the velocity curve's anchors
    pub mallet: u32,
    /// the clavichord's Bebung (instruments with tangents, `Instrument::tangent`; others ignore
    /// it): the pitch rise (cents) of a held key at full pressure, the highest of the mod wheel,
    /// the channel pressure and the key's own; the recordings' vibrato swings 10–30 cents
    pub bebung: f32,
    /// the pitch wheel's range (semitones, 0 … 12; instruments whose strings bend: the đàn tranh)
    pub bend_range: f32,
    /// the audio input's level (linear, 0 off): the input drives the sympathetic bank with the
    /// strings' bridge force (`Engine::process_input`); 1 = an input as loud as the instrument's
    /// own sound at the output drives the free strings as its own strings' force does
    pub input: f32,
    /// a hammer piano's level range over the velocity (dB from 1 to 127,
    /// `velocity::dynamics_db`), from the next strike; instruments without a level table
    /// (`Instrument::dynamics`) play their recordings' anchors and ignore it
    pub dynamics: f32,
    /// each strike slightly different (0 none … 1 as measured on Grand D's takes, more exaggerates):
    /// the hammer meets the strings at a slightly other point, the unison's strings a few µs apart
    /// and with other shares of the felt (`voice::Voice::set_variation`), from the next strike
    pub variation: f32,
    /// a re-strike's felt meets the strings where they ring, a mallet the bar where it swings
    /// (> 0.5; `voice::Voice::set_restrike`, `Voice::mallet_meets`), else as from rest; from the
    /// next strike (off by default)
    pub restrike: f32,
    /// a brass instrument's own controls (`Instrument::brass`; others ignore them)
    pub brass: crate::brass::BrassParams,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            room: 1.0,
            knock: 1.0,
            release: 1.0,
            pedal: 1.0,
            pedal_ring: 1.0,
            sympathetic: 1.0,
            hardness: 1.0,
            hammer_wear: 0.0,
            voicing_wear: 0.0,
            damper_wear: 0.0,
            action_wear: 0.0,
            phantom: 0.0,
            grooves: 0.0,
            damper_buzz: 0.0,
            rattle: 0.0,
            volume: 1.0,
            damper_fall: 1.0,
            prep: Prep::None,
            pluck_pos: 0.2,
            pluck_bright: 1.0,
            mute: 0.6,
            harmonic: 2,
            paper: 0.5,
            stops: 1,
            buff: false,
            lute: false,
            motor: false,
            motor_rate: 7.0,
            motor_depth: 0.7,
            mallet: 0,
            bebung: 30.0,
            bend_range: 2.0,
            input: 0.0,
            dynamics: crate::velocity::DYNAMICS_DB as f32,
            variation: VARIATION_DEFAULT,
            restrike: 0.0,
            brass: crate::brass::BrassParams::default(),
        }
    }
}

/// Running counts, for timing and diagnostics.
pub struct Stats {
    pub voices: usize,
    pub modes: usize,
    pub sympathetic: usize,
    /// voices the runaway guard silenced since the engine started (`voice::RUNAWAY`), and outputs
    /// the output guard cut (`OUTPUT_GUARD`)
    pub runaways: usize,
}

/// Where the diagnostics go (set by `attach_telemetry`).
struct Telemetry {
    region: Region,
    rate: f64,
    /// samples between frames, and the sample position of the next
    interval: u64,
    next: u64,
    counter: u64,
}

/// A note's level correction at a strike's speed: its pp, mf, ff dB interpolated linearly in
/// log₂ speed between the anchor speeds, held beyond them; a linear factor.
pub fn level_gain(db: &[f64; 3], speed: f64, anchors: &[f64; 3]) -> f64 {
    let x = speed.max(1e-6).log2();
    let a: [f64; 3] = anchors.map(|s| s.max(1e-6).log2());
    let d = if x <= a[0] {
        db[0]
    } else if x >= a[2] {
        db[2]
    } else {
        let (lo, hi) = if x <= a[1] { (0, 1) } else { (1, 2) };
        let w = if a[hi] > a[lo] { (x - a[lo]) / (a[hi] - a[lo]) } else { 0.0 };
        db[lo] + w * (db[hi] - db[lo])
    };
    10f64.powf(d / 20.0)
}

/// Which parts reach the output (all, normally; `midi2wav --stems` renders one part at a time). The
/// sources (the struck strings, the ringing: the sympathetic bank and the rattle) through the
/// radiation's two outputs (the direct sound from the board, the room), and the one-shots mixed after
/// it (knock, release, pedal). The parts are linear and nothing heard feeds back (the bank listens
/// to the strings without driving them), so the stems sum to the full render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mix {
    pub strings: bool,
    pub ringing: bool,
    pub body: bool,
    pub room: bool,
    pub noises: bool,
}

impl Mix {
    pub const ALL: Mix = Mix { strings: true, ringing: true, body: true, room: true, noises: true };
    const NONE: Mix = Mix { strings: false, ringing: false, body: false, room: false, noises: false };
    /// The stems, named: they sum to `ALL`.
    pub const STEMS: [(&'static str, Mix); 4] = [
        ("strings", Mix { strings: true, body: true, ..Mix::NONE }),
        ("ringing", Mix { ringing: true, body: true, ..Mix::NONE }),
        ("room", Mix { strings: true, ringing: true, room: true, ..Mix::NONE }),
        ("noises", Mix { noises: true, ..Mix::NONE }),
    ];
}

pub struct Engine {
    inst: Instrument,
    voices: Vec<Voice>,
    bank: Bank,
    /// radiation of the microphone setup, and earlier setups ringing out after a handover
    /// (from the sample `fade_at` on, over `fade_len` samples; the current one over `fade_now`)
    rad: Radiator,
    fading: Vec<Radiator>,
    /// the parts that reach the output (`Mix`)
    mix: Mix,
    /// the radiation's crossfade at a handover: its start, and whether `rad` fades in at its output
    /// (it took the old one's history) rather than at its input
    fade_at: u64,
    fade_out: bool,
    fade_len: usize,
    fade_now: usize,
    /// per-sample input gains of a block (crossfade)
    gain_buf: Vec<f32>,
    /// diffuse parts (sympathetic bank, rattle) of the sub-block, spread over the zones
    diffuse: Vec<f64>,
    params: Params,
    /// model units → full scale (`FF_MEDIAN_PEAK_DBFS`)
    calibration: f32,
    /// the audio input: full scale → bridge force (`Params::input` 1); its reciprocal response
    /// (`input_response`), the block's input samples, and the previous block filtered (the drive
    /// of this block: one block late)
    input_gain: f32,
    in_conv: Convolver,
    in_buf: Vec<f32>,
    in_drive: Vec<Vec<f32>>,
    /// noise bursts at the engine rate, channel-major (2 × n)
    knock: Vec<Option<Arc<[f32]>>>,
    release: Vec<Arc<[f32]>>,
    /// pedal thumps at the engine rate (down takes, up takes) and their peak levels
    thumps_down: Vec<(Arc<[f32]>, f32)>,
    thumps_up: Vec<(Arc<[f32]>, f32)>,
    /// alternating take
    pedal_take: usize,
    key_down: [bool; 88],
    struck: [Option<f64>; 88],
    /// per key: the last strike's position and speed (the visualization's hammers)
    last_strike: [(u64, f32); 88],
    /// per key: the registers its last note-on excited (bit r = register r)
    plucked: [u32; 88],
    /// pedal depth 0 … 1, and whether the dampers are (mostly) off by it
    pedal: f64,
    pedal_lifted: bool,
    /// the sostenuto pedal is down, and the keys whose dampers it holds up
    sostenuto: bool,
    sostenuto_held: [bool; 88],
    /// the soft pedal's depth 0 … 1
    soft: f64,
    /// damper engagement per key (nominal: before damper wear), what the strings have now;
    /// its target (a damper lifts at once and falls in `fall_s`), the continuous position
    /// on the way down, the fall time of the current release, the release velocity noted
    engage: [f64; 88],
    engage_target: [f64; 88],
    damper_pos: [f64; 88],
    fall_s: [f64; 88],
    release_v: [u8; 88],
    /// when the key went down (sample position), for the release noise
    down_at: [u64; 88],
    /// per key: how its hammer is worn re the others (standard normal, from the wear seed)
    voicing: [f64; 88],
    /// per key: how loud its hammer is voiced re the others (standard normal, from the wear
    /// seed, a stream of its own)
    level_scatter: [f64; 88],
    /// buffers of a voice rendered on its own (damper buzz needs its bridge force)
    own_acc_v: Vec<Lane>,
    own_acc_h: Vec<Lane>,
    own_acc_t: Vec<Lane>,
    own_t: Vec<f64>,
    own_v: Vec<f64>,
    own_h: Vec<f64>,
    own_buzz: Vec<f64>,
    rattle_th: f64,
    /// an ff E1's peak bridge force (scale of rattle and phantom partials)
    ff_force: f64,
    rattle_hp: crate::noise::HighPass,
    rattle_out: Vec<f64>,
    queue: VecDeque<(u64, Event)>,
    now: u64,
    acc_v: Vec<Lane>,
    acc_h: Vec<Lane>,
    acc_b: Vec<Lane>,
    /// the bars' resonators (`Voice::render`'s `acc_t`), summed, and the motor's gains on them
    acc_t: Vec<Lane>,
    blk_t: Vec<f64>,
    tube_gain: Vec<f64>,
    motor: Motor,
    /// the resonators' detune the voices have, and whether any note has a resonator
    tube_detune: f64,
    has_tube: bool,
    /// the instrument has a motor (`Instrument::motor`): without, the gains stay 1
    has_motor: bool,
    /// the Bebung (instruments with tangents): the mod wheel, the channel pressure and each key's
    /// pressure (0 … 1), and each key's pitch rise (cents) on its way to what they ask
    has_tangent: bool,
    wheel: f64,
    channel_pressure: f64,
    key_pressure: [f64; 88],
    bebung: [f64; 88],
    /// the pitch wheel (instruments whose strings bend): its position (−1 … 1) and the bend (cents)
    /// on its way there
    has_bend: bool,
    wheel_bend: f64,
    bend: f64,
    blk_v: Vec<f64>,
    blk_h: Vec<f64>,
    drive: Vec<f32>,
    conv_in: Vec<f32>,
    /// radiation per block: body L/R, room L/R; one radiator's alone, to be weighted (a crossfade at
    /// the outputs)
    rad_body: Vec<Vec<f32>>,
    rad_room: Vec<Vec<f32>>,
    one_body: Vec<Vec<f32>>,
    one_room: Vec<Vec<f32>>,
    /// one-shots per block, L/R
    blk_out: Vec<Vec<f32>>,
    fill: usize,
    overlays: Vec<Overlay>,
    out_fifo: VecDeque<[f32; 2]>,
    /// panic: output samples left in the fade (0: none), and the fade's length
    silence_left: usize,
    silence_len: usize,
    /// outputs the output guard cut (`OUTPUT_GUARD`; `Stats::runaways`)
    guarded: usize,
    telemetry: Option<Telemetry>,
    /// preallocated at construction (the audio thread never allocates): the frame being built and the per-voice scratch
    tele_frame: Option<Box<Frame>>,
    tele_scratch: Vec<VoiceFrame>,
    /// the block's output pair as pushed to the audio tap (filled only while telemetry is on)
    tele_tap: [[f32; 2]; BASE],
    /// a brass instrument's players: they take the key events and write into the block before the room
    brass: Option<Box<crate::brass::Family>>,
}

type Responses = (Vec<[Vec<Vec<f32>>; 2]>, [Vec<Vec<f32>>; 2]);
/// the microphones (None: the recording's position) and the level reference
type Setup = (Option<crate::mic::MicSetup>, bool, f64);

/// How many microphone setups the cache keeps: the plugin's position menu has six.
const CACHED_SETUPS: usize = 6;

/// The radiation responses of the last few setups (by instrument, rate and microphone setup;
/// about 4 MB each), so that engines rebuilt for other string tunings, and for a position
/// switched back to, skip them (`Engine::with_cache`). A copy holds the same responses (engines
/// built in parallel each take one).
#[derive(Clone, Default)]
pub struct RadiationCache {
    /// the most recently used first
    setups: Vec<((String, f64, Setup), Responses)>,
    /// the audio input's response (`input_response`), by instrument and rate
    input: Option<((String, f64), Vec<f32>)>,
    /// the responses computed so far (a miss)
    builds: usize,
}

impl RadiationCache {
    /// how many times the responses were computed (a cache miss), for the tests
    pub fn builds(&self) -> usize {
        self.builds
    }
}

/// Radiation of one microphone setup.
struct Radiator {
    /// soundboard per bridge zone and polarization (L/R; one zone, fed by the total, at
    /// the recording's position), and the room per polarization (L/R, fed by the total)
    body: Vec<[Convolver; 2]>,
    room: Vec<Convolver>,
    /// per key: (zone a, weight a, zone b, weight b) when there are several zones
    zone_of: Vec<(usize, f64, usize, f64)>,
    /// per zone and polarization: the strings' bridge force of the block
    zone_buf: Vec<[Vec<f64>; 2]>,
    /// with zones: the diffuse part (the sympathetic bank, the rattle: spread evenly over the zones)
    /// through the zones' mean vertical response, one convolution for all, so that a zone no string
    /// feeds stays silent (its convolutions skipped); its input of the block
    diffuse: Option<Convolver>,
    diffuse_buf: Vec<f64>,
    /// the setup the responses were made for (equal setups: equal responses)
    setup: Setup,
    /// fading out: its weight at the handover, and samples left to run (`usize::MAX`: the
    /// engine's own)
    g0: f32,
    left: usize,
    /// fading out at its output (its history went to the next setup, `take_history`: it stops
    /// after the crossfade), else at its input (it rings out its responses' tails)
    out_fade: bool,
    /// engine time until which its input history holds a crossfade's ramp (it took over at its
    /// input): until then it cannot hand its history on
    ramped_until: u64,
}

impl Radiator {
    fn new(inst: &Instrument, sr: f64, tuning: &Tuning, cache: &mut RadiationCache, limit: usize) -> Self {
        let key = (inst.profile.clone(), sr, Self::setup(inst, tuning));
        match cache.setups.iter().position(|(k, _)| *k == key) {
            Some(i) => cache.setups[..=i].rotate_right(1), // to the front
            None => {
                cache.setups.insert(0, (key, radiation_for(inst, sr, tuning)));
                cache.setups.truncate(CACHED_SETUPS);
                cache.builds += 1;
            }
        }
        let (bodies, rooms) = &cache.setups[0].1;
        let body: Vec<[Convolver; 2]> = bodies
            .iter()
            .map(|pols| [Convolver::new(&cut(&pols[0], limit), BASE, &CONV_STAGES), Convolver::new(&cut(&pols[1], limit), BASE, &CONV_STAGES)])
            .collect();
        let room = vec![Convolver::new(&cut(&rooms[0], limit), BASE, &CONV_STAGES), Convolver::new(&cut(&rooms[1], limit), BASE, &CONV_STAGES)];
        let n_zones = body.len();
        // the zones' mean vertical response per channel: Σ_z h_z ∗ (d / Z) = (Σ_z h_z / Z) ∗ d
        let diffuse = (n_zones > 1).then(|| {
            let mean: Vec<Vec<f32>> = (0..bodies[0][0].len())
                .map(|ch| {
                    let len = bodies.iter().map(|z| z[0][ch].len()).max().unwrap_or(0);
                    let mut m = vec![0.0f32; len];
                    for z in bodies {
                        for (a, &x) in m.iter_mut().zip(&z[0][ch]) {
                            *a += x / n_zones as f32;
                        }
                    }
                    m
                })
                .collect();
            Convolver::new(&cut(&mean, limit), BASE, &CONV_STAGES)
        });
        let zone_of = inst
            .notes
            .iter()
            .map(|n| if n_zones > 1 { inst.body().zone_weights(n.note as f64, tuning.spread_keys) } else { (0, 1.0, 0, 0.0) })
            .collect();
        Radiator {
            body,
            room,
            zone_of,
            zone_buf: (0..n_zones).map(|_| [vec![0.0; BASE], vec![0.0; BASE]]).collect(),
            diffuse,
            diffuse_buf: vec![0.0; BASE],
            setup: Self::setup(inst, tuning),
            g0: 1.0,
            left: usize::MAX,
            out_fade: false,
            ramped_until: 0,
        }
    }

    /// No responses (what a handover leaves behind); allocates nothing.
    fn empty() -> Self {
        Radiator {
            body: Vec::new(),
            room: Vec::new(),
            zone_of: Vec::new(),
            zone_buf: Vec::new(),
            diffuse: None,
            diffuse_buf: Vec::new(),
            setup: (None, false, 0.0),
            g0: 0.0,
            left: 0,
            out_fade: false,
            ramped_until: 0,
        }
    }

    fn setup(inst: &Instrument, t: &Tuning) -> Setup {
        let pillow = crate::pillow::sanitize(t.pillow);
        match t.mics_for(inst.body()) {
            None => (None, false, pillow),
            Some(s) => (Some(s), t.keep_direct, pillow),
        }
    }

    fn live(&self) -> bool {
        self.left > 0 && !self.body.is_empty()
    }

    fn zoned(&self) -> bool {
        self.body.len() > 1
    }

    /// Take over the input history of `old` (the radiator playing until now: the same zones fed
    /// the same bridge forces): each convolver's (`Convolver::take_history`) and the block's zone
    /// input so far; false (nothing changed) where the layouts differ.
    fn take_history(&mut self, old: &Radiator) -> bool {
        let alike = self.body.len() == old.body.len()
            && self.room.len() == old.room.len()
            && self.zone_of == old.zone_of
            && match (&self.diffuse, &old.diffuse) {
                (Some(a), Some(b)) => a.same_layout(b),
                (None, None) => true,
                _ => false,
            }
            && self.body.iter().flatten().zip(old.body.iter().flatten()).all(|(a, b)| a.same_layout(b))
            && self.room.iter().zip(&old.room).all(|(a, b)| a.same_layout(b));
        if !alike {
            return false;
        }
        for (a, b) in self.body.iter_mut().flatten().zip(old.body.iter().flatten()) {
            a.take_history(b);
        }
        for (a, b) in self.room.iter_mut().zip(&old.room) {
            a.take_history(b);
        }
        for (a, b) in self.zone_buf.iter_mut().flatten().zip(old.zone_buf.iter().flatten()) {
            a.copy_from_slice(b);
        }
        if let (Some(a), Some(b)) = (self.diffuse.as_mut(), old.diffuse.as_ref()) {
            a.take_history(b);
            self.diffuse_buf.copy_from_slice(&old.diffuse_buf);
        }
        true
    }

    /// Samples the responses ring after their last input.
    fn tail(&self) -> usize {
        self.body.iter().flatten().chain(&self.room).chain(&self.diffuse).map(Convolver::tail).max().unwrap_or(0)
    }

    /// One string's bridge forces into its zones (vertical v + extra, horizontal h).
    fn feed(&mut self, key: usize, v: &[f64], extra: &[f64], h: &[f64], at: usize) {
        let (za, wa, zb, wb) = self.zone_of[key];
        for (z, w) in [(za, wa), (zb, wb)] {
            if w == 0.0 {
                continue;
            }
            let [bv, bh] = &mut self.zone_buf[z];
            for j in 0..v.len() {
                bv[at + j] += w * (v[j] + extra[j]);
                bh[at + j] += w * h[j];
            }
        }
    }

    /// The diffuse part: spread evenly over the zones (`diffuse`, their mean response).
    fn feed_diffuse(&mut self, x: &[f64], at: usize) {
        for (b, &x) in self.diffuse_buf[at..at + x.len()].iter_mut().zip(x) {
            *b += x;
        }
    }

    /// Convolve a block: the totals (v, h) or the zones, × the per-sample input gains
    /// (None: 1), added to the body and room outputs.
    #[allow(clippy::too_many_arguments)]
    fn process(&mut self, v: &[f64], h: &[f64], gain: Option<&[f32]>, conv_in: &mut [f32], body: &mut [Vec<f32>], room: &mut [Vec<f32>]) {
        let fill = |c: &mut [f32], x: &[f64]| match gain {
            Some(g) => c.iter_mut().zip(x).zip(g).for_each(|((c, &x), &g)| *c = g * x as f32),
            None => c.iter_mut().zip(x).for_each(|(c, &x)| *c = x as f32),
        };
        let zoned = self.zoned();
        for pol in 0..2 {
            fill(conv_in, if pol == 0 { v } else { h });
            self.room[pol].process(conv_in, room);
            if !zoned {
                self.body[0][pol].process(conv_in, body);
            }
        }
        if zoned {
            for (convs, bufs) in self.body.iter_mut().zip(&self.zone_buf) {
                for pol in 0..2 {
                    fill(conv_in, &bufs[pol]);
                    convs[pol].process(conv_in, body);
                }
            }
            if let Some(d) = self.diffuse.as_mut() {
                fill(conv_in, &self.diffuse_buf);
                d.process(conv_in, body);
            }
        }
    }
}

/// Smooth ramp 0 → 1 over len samples.
fn ramp(t: u64, len: usize) -> f32 {
    let x = (t as f64 / len as f64).min(1.0);
    (x * x * (3.0 - 2.0 * x)) as f32
}

/// Radiation impulse responses at the engine rate. A response applied to force samples
/// scales with the sample spacing, hence reference/sr.
fn radiation_at(r: &Radiation, from: f64, to: f64) -> Vec<Vec<f32>> {
    let scale = (from / to) as f32;
    r.ir.iter().map(|c| resample(c, from, to).into_iter().map(|x| x * scale).collect()).collect()
}

/// Soundboard responses per zone and polarization (L/R) and the room per polarization
/// (L/R) for a microphone position: the fitted responses at the recording's position,
/// per bridge zone otherwise (`mic::zones`).
#[allow(clippy::type_complexity)]
fn radiation_for(inst: &Instrument, sr: f64, tuning: &Tuning) -> Responses {
    let rsr = inst.reference_sr;
    let body = [radiation_at(&inst.radiation_v.body, rsr, sr), radiation_at(&inst.radiation_h.body, rsr, sr)];
    let mut room = [radiation_at(&inst.radiation_v.room, rsr, sr), radiation_at(&inst.radiation_h.room, rsr, sr)];
    // the room as it was before its cuts: they change the room, not the board's level at the other
    // positions (matched to the recording's total)
    let room_energy: Vec<f64> = room[0].iter().zip(&inst.radiation_v.room_kept).map(|(c, k)| c.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / k.max(1e-12)).collect();
    let (mut bodies, mut rooms) = match tuning.mics_for(inst.body()) {
        Some(setup) => {
            // a bell's direct path is the family's (`brass::bell`): the room alone here
            let bell = matches!(inst.body(), crate::mic::Body::Bell(_));
            let z = if bell {
                crate::mic::Zones { body: Vec::new(), room_gain: 1.0 }
            } else {
                crate::mic::zones(inst.body(), &inst.globals.board, &inst.body_h, [&body[0], &body[1]], &room_energy, sr, rsr, &setup, tuning.keep_direct)
            };
            let (sens, coupling) = setup.diffuse();
            let spacing = crate::mic::dist(setup.mics[0].pos, setup.mics[1].pos);
            // the room's inter-channel coherence follows the pair's spacing too (same
            // energy per channel as the fitted room)
            let rs = &inst.room_spec;
            // the two polarizations' rooms side by side (each its own noise: independent)
            let fresh_rooms: Vec<Vec<Vec<f32>>> = std::thread::scope(|scope| {
                let handles: Vec<_> = [&inst.radiation_v.room, &inst.radiation_h.room]
                    .into_iter()
                    .zip(rs.seeds)
                    .map(|(fitted, seed)| {
                        scope.spawn(move || {
                            let fresh = crate::noise::room_ir(seed, rs, spacing, coupling, rsr);
                            let ir = fresh
                                .iter()
                                .zip(&fitted.ir)
                                .map(|(c, f)| {
                                    let k = (f.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / c.iter().map(|v| v * v).sum::<f64>().max(1e-300)).sqrt();
                                    c.iter().map(|&v| (v * k) as f32).collect()
                                })
                                .collect();
                            radiation_at(&Radiation { channels: 2, ir }, rsr, sr)
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().expect("a room's thread")).collect()
            });
            for (pol, r) in fresh_rooms.into_iter().enumerate() {
                room[pol] = r;
            }
            for pol in room.iter_mut() {
                for (c, ch) in pol.iter_mut().enumerate() {
                    let k = (z.room_gain * sens[c]) as f32;
                    ch.iter_mut().for_each(|v| *v *= k);
                    if inst.body().lid_shields(setup.mics[c].pos) {
                        crate::mic::lid_room(ch, setup.lid, sr);
                    }
                }
            }
            (if bell { vec![body] } else { z.body }, room)
        }
        None => (vec![body], room),
    };
    // the pillow is on the soundboard; the room, fed by the board's radiation, gets the
    // lowpass and level but keeps its own tail
    let pillow = crate::pillow::sanitize(tuning.pillow);
    if pillow > 0.0 {
        for pols in bodies.iter_mut() {
            for ch in pols.iter_mut().flatten() {
                crate::pillow::apply(ch, pillow, sr);
            }
        }
        for ch in rooms.iter_mut().flatten() {
            crate::pillow::apply_room(ch, pillow, sr);
        }
    }
    (bodies, rooms)
}

/// The responses cut to `limit` samples (borrowed as they are when none is longer).
fn cut(irs: &[Vec<f32>], limit: usize) -> std::borrow::Cow<'_, [Vec<f32>]> {
    if irs.iter().all(|ir| ir.len() <= limit) {
        std::borrow::Cow::Borrowed(irs)
    } else {
        std::borrow::Cow::Owned(irs.iter().map(|ir| ir[..ir.len().min(limit)].to_vec()).collect())
    }
}

/// A channel-major stereo burst at the engine rate: at the instrument's own, the instrument's,
/// shared (every engine built copied every note's: 42 MB a build, which a felt's measurement made
/// 704 times beside the audio thread).
fn burst_at(b: &Arc<[f32]>, from: f64, to: f64) -> Arc<[f32]> {
    if from.round() == to.round() {
        return Arc::clone(b);
    }
    let n = b.len() / 2;
    let (l, r) = (resample(&b[..n], from, to), resample(&b[n..], from, to));
    l.into_iter().chain(r).collect()
}

/// the visualization's displacement of a bar struck at its reference dynamic (m)
const BAR_VIZ_M: f64 = 3e-3;

impl Engine {
    /// Fill `out` with the sounding voices (the `MAX_VOICES` loudest, strongest partials
    /// first), the keys, the pedal and the sample position. Read-only on the signal
    /// path; allocation-free (scratch is preallocated). Call after a block's end.
    pub fn telemetry_frame(&mut self, out: &mut Frame) {
        let mut scratch = std::mem::take(&mut self.tele_scratch);
        scratch.clear();
        let cal = self.calibration;
        for (j, voice) in self.voices.iter().enumerate().filter(|(_, v)| v.active) {
            // SAFETY: VoiceFrame is plain old data; all-zero is valid
            let mut vf: VoiceFrame = unsafe { std::mem::zeroed() };
            let (count, level, modes) = voice.partials(&mut vf.partials);
            for p in &mut vf.partials[..count] {
                p.amp *= cal;
            }
            vf.key = (j % 88) as u8;
            vf.reg = (j / 88) as u8;
            vf.count = count as u16;
            vf.level = level * cal;
            vf.modes = modes as u32;
            vf.speed = self.struck[j % 88].unwrap_or(0.0) as f32;
            if scratch.len() < scratch.capacity() {
                scratch.push(vf);
            }
        }
        scratch.sort_unstable_by(|a, b| b.level.total_cmp(&a.level));
        let n = scratch.len().min(MAX_VOICES);
        out.voices = n as u32;
        out.voice[..n].copy_from_slice(&scratch[..n]);
        out.position = self.now;
        out.pedal = self.pedal as f32;
        out.keys = self.key_words();
        self.tele_scratch = scratch;
        self.viz_keys(&mut out.key);
    }

    /// The visualization's keys: each key's strings (every register's, summed in power), its
    /// first register's damper, the sympathetic bank's ring on the same scale, the last strike
    /// (the plugin's Visualization panel, `midi2wav --video`). Call after a block's end.
    pub fn viz_keys(&self, out: &mut [KeyFrame; 88]) {
        for (i, k) in out.iter_mut().enumerate() {
            k.amp.fill(0.0);
            (k.struck_at, k.speed) = self.last_strike[i];
            let Some(note) = self.inst.notes.get(i) else {
                // a brass export: no strings to show
                k.ring.fill(0.0);
                (k.engage, k.length, k.point, k.flags) = (0.0, 0.0, 0.0, 0);
                continue;
            };
            for r in 0..self.inst.registers() {
                let j = r * 88 + i;
                if let Some(v) = self.voices.get(j).filter(|v| v.active) {
                    v.add_shape(&self.inst.notes[j], &mut k.amp);
                }
            }
            // a bar's modes are in model units: its reference strike's amplitude shown as 3 mm (a bar moves less than a string: brighter so)
            let scale = match &note.spec.excitation {
                crate::physics::Excitation::Pluck(p) if p.bar && p.amp > 0.0 => (BAR_VIZ_M / p.amp) as f32,
                _ => 1.0,
            };
            k.amp.iter_mut().for_each(|x| *x = scale * x.sqrt());
            self.bank.amplitudes(i, &mut k.ring);
            for (x, b) in k.ring.iter_mut().zip(&note.bridge) {
                *x = if *b != 0.0 { scale * *x / *b as f32 } else { 0.0 };
            }
            (k.struck_at, k.speed) = self.last_strike[i];
            k.engage = self.voices[i].engage as f32;
            k.length = note.spec.string.l as f32;
            let point = match &note.spec.excitation {
                crate::physics::Excitation::Hammer(h) => h.strike_pos,
                crate::physics::Excitation::Pluck(p) => p.beta,
            };
            k.point = point as f32;
            let hammer = matches!(note.spec.excitation, crate::physics::Excitation::Hammer(_));
            k.flags = if note.has_damper { telemetry::KEY_DAMPER } else { 0 } | if hammer { telemetry::KEY_HAMMER } else { 0 };
        }
    }

    /// Held and ringing keys, packed (`telemetry::pack_keys`); cheap, always available.
    pub fn key_words(&self) -> [u32; KEY_WORDS] {
        let mut ringing = [false; 88];
        for (j, v) in self.voices.iter().enumerate() {
            if v.active {
                ringing[j % 88] = true;
            }
        }
        if let Some(f) = &self.brass {
            for (j, r) in ringing.iter_mut().enumerate() {
                *r = f.sounding(j as u8 + 21);
            }
        }
        telemetry::pack_keys(&self.key_down, &ringing)
    }

    /// The instrument this engine plays.
    pub fn instrument(&self) -> &Instrument {
        &self.inst
    }

    pub fn new(inst: Instrument, sr: f64) -> Result<Self, Error> {
        let tuning = inst.tuning;
        Self::with_tuning(inst, sr, tuning)
    }

    /// An engine for `sr` with the strings compiled for `tuning` (concert pitch, unison
    /// width): changing those means a new engine (built off the audio thread).
    pub fn with_tuning(inst: Instrument, sr: f64, tuning: Tuning) -> Result<Self, Error> {
        Self::with_cache(inst, sr, tuning, &mut RadiationCache::default())
    }

    /// `with_tuning`, taking the radiation responses from `cache` when they were built for
    /// the same instrument, rate and microphone setup (and leaving them there).
    pub fn with_cache(inst: Instrument, sr: f64, tuning: Tuning, cache: &mut RadiationCache) -> Result<Self, Error> {
        Self::built(inst, sr, tuning, cache, usize::MAX)
    }

    /// `with_cache` for an engine that plays `secs` and no longer (a measurement): its responses
    /// cut where they can no longer reach the output in that time (the same output there, to
    /// rounding). A level table renders 704 notes of 0.2 s; transforming and playing the room's
    /// seconds for each filled the memory's bandwidth (an audio thread beside it 2–4× slower).
    pub fn for_window(inst: Instrument, sr: f64, tuning: Tuning, cache: &mut RadiationCache, secs: f64) -> Result<Self, Error> {
        Self::built(inst, sr, tuning, cache, (secs * sr).ceil() as usize + 4 * BASE)
    }

    /// `with_cache` with the responses cut to `limit` samples
    fn built(inst: Instrument, sr: f64, tuning: Tuning, cache: &mut RadiationCache, limit: usize) -> Result<Self, Error> {
        if !(8000.0..=384000.0).contains(&sr) || sr.fract() != 0.0 {
            return Err(Error(format!("unsupported sample rate {sr}")));
        }
        let mut inst = inst;
        if inst.compiled_sr != sr || inst.tuning != tuning {
            inst.compile(sr, tuning);
        }
        let rsr = inst.reference_sr;
        let rad = Radiator::new(&inst, sr, &tuning, cache, limit);
        let input_key = (inst.profile.clone(), sr);
        if cache.input.as_ref().is_none_or(|(k, _)| *k != input_key) {
            cache.input = Some((input_key, input_response(&inst, sr)));
        }
        let in_conv = Convolver::new(&cut(std::slice::from_ref(&cache.input.as_ref().expect("built above").1), limit), BASE, &CONV_STAGES);
        let voices: Vec<Voice> = inst
            .notes
            .iter()
            .map(|n| {
                let mut v = Voice::new(n, sr, rsr, inst.contact_max_s, inst.contact_separation_s, BASE);
                v.set_impact_filter(ImpactFilter::new(inst.impact.and_then(|m| m.cutoff_hz), sr));
                if let Some(m) = &inst.membrane {
                    v.set_membrane(m, n);
                }
                v
            })
            .collect();
        let knock = inst.notes.iter().map(|n| n.knock.as_ref().map(|k| burst_at(&k.burst, rsr, sr))).collect();
        let release = inst.notes.iter().map(|n| burst_at(&n.release.burst, rsr, sr)).collect();
        // thump levels: the takes' peaks re the median mf note peak (as the release noise)
        let mf = if inst.notes.is_empty() {
            1.0 // a brass export: no notes, no pedal
        } else {
            let mut v: Vec<f64> = inst.notes[..88].iter().map(|n| n.release.peak[1]).collect();
            v.sort_by(f64::total_cmp);
            v[v.len() / 2]
        };
        let thumps = |takes: &[crate::instrument::Thump]| -> Vec<(Arc<[f32]>, f32)> {
            takes.iter().map(|t| (burst_at(&t.burst, rsr, sr), (mf * 10f64.powf(t.ratio_db / 20.0)) as f32)).collect()
        };
        let (thumps_down, thumps_up) = match &inst.pedal {
            Some(p) => (thumps(&p.down), thumps(&p.up)),
            None => (Vec::new(), Vec::new()),
        };
        let inst_voices = voices.len();
        let mut e = Engine {
            bank: Bank::new(&inst, sr),
            voices,
            rad,
            fading: Vec::with_capacity(MAX_FADING),
            mix: Mix::ALL,
            fade_out: false,
            fade_at: 0,
            fade_len: (FADE_S * sr) as usize,
            fade_now: (FADE_S * sr) as usize,
            gain_buf: vec![0.0; BASE],
            diffuse: vec![0.0; BASE],
            params: Params::default(),
            calibration: calibration(&inst),
            input_gain: INPUT_DRIVE / calibration(&inst),
            in_conv,
            in_buf: vec![0.0; BASE],
            in_drive: vec![vec![0.0; BASE]],
            knock,
            release,
            thumps_down,
            thumps_up,
            pedal_take: 0,
            key_down: [false; 88],
            struck: [None; 88],
            last_strike: [(0, 0.0); 88],
            plucked: [0; 88],
            pedal: 0.0,
            pedal_lifted: false,
            sostenuto: false,
            sostenuto_held: [false; 88],
            soft: 0.0,
            engage: [-1.0; 88],
            engage_target: [-1.0; 88],
            damper_pos: [-1.0; 88],
            fall_s: [DAMPER_FALL_S; 88],
            release_v: [0; 88],
            down_at: [0; 88],
            own_acc_v: vec![[0.0; LANES]; BASE],
            own_acc_h: vec![[0.0; LANES]; BASE],
            own_acc_t: vec![[0.0; LANES]; BASE],
            own_t: vec![0.0; BASE],
            own_v: vec![0.0; BASE],
            own_h: vec![0.0; BASE],
            own_buzz: vec![0.0; BASE],
            rattle_th: 0.0,
            ff_force: 1.0,
            rattle_hp: crate::noise::HighPass::new(RATTLE_HP, sr),
            rattle_out: vec![0.0; BASE],
            voicing: {
                let mut v = [0.0; 88];
                let mut rng = crate::noise::Rng::new(tuning.wear.seed.wrapping_mul(0x9e37_79b9) ^ 0x7653);
                v.iter_mut().for_each(|x| *x = rng.gauss());
                v
            },
            level_scatter: {
                let mut v = [0.0; 88];
                let mut rng = crate::noise::Rng::new(tuning.wear.seed.wrapping_mul(0x85eb_ca6b) ^ 0x1e7e);
                v.iter_mut().for_each(|x| *x = rng.gauss());
                v
            },
            queue: VecDeque::with_capacity(1024),
            now: 0,
            acc_v: vec![[0.0; LANES]; BASE],
            acc_h: vec![[0.0; LANES]; BASE],
            acc_b: vec![[0.0; LANES]; BASE],
            acc_t: vec![[0.0; LANES]; BASE],
            blk_t: vec![0.0; BASE],
            tube_gain: vec![1.0; BASE],
            motor: Motor::new(sr),
            tube_detune: 0.0,
            has_tube: inst.notes.iter().any(|n| n.tube.is_some()),
            has_motor: inst.motor,
            has_tangent: inst.tangent,
            wheel: 0.0,
            channel_pressure: 0.0,
            key_pressure: [0.0; 88],
            bebung: [0.0; 88],
            has_bend: inst.bend,
            wheel_bend: 0.0,
            bend: 0.0,
            blk_v: vec![0.0; BASE],
            blk_h: vec![0.0; BASE],
            drive: vec![0.0; BASE],
            conv_in: vec![0.0; BASE],
            rad_body: vec![vec![0.0; BASE]; 2],
            rad_room: vec![vec![0.0; BASE]; 2],
            one_body: vec![vec![0.0; BASE]; 2],
            one_room: vec![vec![0.0; BASE]; 2],
            blk_out: vec![vec![0.0; BASE]; 2],
            fill: 0,
            overlays: Vec::with_capacity(MAX_OVERLAYS),
            out_fifo: VecDeque::with_capacity(8192),
            silence_left: 0,
            silence_len: ((SILENCE_S * sr) as usize).max(1),
            guarded: 0,
            telemetry: None,
            tele_frame: Some(Frame::boxed()),
            tele_scratch: Vec::with_capacity(inst_voices),
            tele_tap: [[0.0; 2]; BASE],
            brass: crate::brass::Family::new(&inst, sr).map(Box::new),
            inst,
        };
        e.out_fifo.extend(std::iter::repeat_n([0.0, 0.0], BASE));
        if e.brass.is_some() {
            // the variation scales the player's as it was fitted, not a strike's measured takes
            e.params.variation = crate::brass::VARIATION_AS_FITTED;
        }
        // the rattle's threshold, a string's force (a brass export has none)
        e.ff_force = if e.inst.notes.is_empty() { 1.0 } else { e.ff_peak_force(28).max(1e-30) };
        e.rattle_th = RATTLE_TH * e.ff_force;
        for i in 0..88 {
            e.update_key(i);
        }
        Ok(e)
    }

    /// Take over from `old`, the engine this one replaces (same instrument and rate, other
    /// tuning or microphone setup): keys, pedal, queued events, the sounding strings and
    /// sympathetic resonators (going on at their new frequencies), one-shots, and the
    /// output in flight. The radiation carries over when the microphone setup is the
    /// same; otherwise the old one rings out while the input crossfades to the new one.
    /// Returns false (nothing taken) when the engines do not match (another instrument, or the
    /// same name with other contents: `Instrument::content`). Real-time safe (no
    /// allocation); `old` must only be dropped afterwards.
    pub fn adopt(&mut self, old: &mut Engine) -> bool {
        let compatible = self.inst.profile == old.inst.profile
            && self.inst.content == old.inst.content
            && self.voices.len() == old.voices.len()
            && self.voices.iter().zip(&old.voices).all(|(a, b)| a.compatible(b))
            && self.fade_len == old.fade_len;
        if !compatible || self.now != 0 {
            return false;
        }
        self.params = old.params;
        if let (Some(a), Some(b)) = (&mut self.brass, &mut old.brass) {
            a.take_over(b);
        }
        self.motor = old.motor.clone();
        self.bank.set_coupling(self.params.sympathetic as f64);
        if self.inst.strings_print == old.inst.strings_print {
            // the same strings (only the radiation changed: a microphone moved): the voices and the
            // bank go over whole, whatever rings
            std::mem::swap(&mut self.voices, &mut old.voices);
            std::mem::swap(&mut self.bank, &mut old.bank);
        } else {
            for (i, note) in self.inst.notes.iter().enumerate() {
                self.voices[i].adopt(&mut old.voices[i], note);
            }
            self.bank.adopt(&old.bank);
        }
        self.key_down = old.key_down;
        self.struck = old.struck;
        self.last_strike = old.last_strike;
        self.plucked = old.plucked;
        self.pedal = old.pedal;
        self.pedal_lifted = old.pedal_lifted;
        self.sostenuto = old.sostenuto;
        self.sostenuto_held = old.sostenuto_held;
        self.soft = old.soft;
        self.pedal_take = old.pedal_take;
        self.engage_target = old.engage_target;
        self.damper_pos = old.damper_pos;
        self.fall_s = old.fall_s;
        self.release_v = old.release_v;
        self.down_at = old.down_at;
        self.wheel = old.wheel;
        self.channel_pressure = old.channel_pressure;
        self.key_pressure = old.key_pressure;
        self.bebung = old.bebung;
        self.wheel_bend = old.wheel_bend;
        self.bend = old.bend;
        // unchanged engagement: no lift kicks, no buzz; the bank and voices follow
        self.engage = old.engage;
        for i in 0..88 {
            self.update_key(i);
        }
        self.overlays.clear();
        self.overlays.extend_from_slice(&old.overlays);
        self.rattle_hp = old.rattle_hp;
        // the input's response is the instrument's (not the microphones'): its state goes on
        std::mem::swap(&mut self.in_conv, &mut old.in_conv);
        std::mem::swap(&mut self.in_buf, &mut old.in_buf);
        std::mem::swap(&mut self.in_drive, &mut old.in_drive);
        std::mem::swap(&mut self.queue, &mut old.queue);
        std::mem::swap(&mut self.out_fifo, &mut old.out_fifo);
        // a panic's fade goes on in the new engine (its end clears what it took over)
        self.silence_left = old.silence_left;
        self.now = old.now;
        self.fill = old.fill;
        self.blk_v.copy_from_slice(&old.blk_v);
        self.blk_h.copy_from_slice(&old.blk_h);
        self.mix = old.mix;
        for (a, b) in self.blk_out.iter_mut().zip(&old.blk_out) {
            a.copy_from_slice(b);
        }
        // radiation
        let fading_before = old.fading.iter().any(Radiator::live);
        // (the keys' zones too: the bars' spread over all keys keeps the responses, moves the keys)
        if self.rad.setup == old.rad.setup && self.rad.zone_of == old.rad.zone_of {
            // the same responses: keep the old ones with their history
            std::mem::swap(&mut self.rad, &mut old.rad);
            std::mem::swap(&mut self.fading, &mut old.fading);
            self.fade_at = old.fade_at;
            self.fade_out = old.fade_out;
            self.fade_now = old.fade_now;
        } else {
            let at = self.now;
            let g = if fading_before { ramp(at.saturating_sub(old.fade_at), old.fade_now) } else { 1.0 };
            // the new responses take the old ones' input history: they sound at once as if they had
            // always played, so the old ones only crossfade (at the outputs, briefly) and stop. Else
            // (another layout, or the old ones' history still ramped) the new ones fade in at their
            // input and the old ones ring out their tails
            self.fade_out = old.rad.ramped_until <= at && self.rad.take_history(&old.rad);
            self.fade_now = if self.fade_out { ((OUT_FADE_S * self.fade_len as f64 / FADE_S) as usize).max(1) } else { self.fade_len };
            let span = self.fade_now + BASE;
            if !self.fade_out {
                self.rad.ramped_until = at + (span + self.rad.tail()) as u64;
            }
            // too many at once: the oldest stop (left in `old`)
            let live = old.fading.iter().filter(|f| f.live()).count();
            for f in old.fading.iter_mut().filter(|f| f.live()).skip((live + 1).saturating_sub(MAX_FADING)) {
                f.g0 *= 1.0 - g;
                if f.g0 > 0.0 {
                    f.left = f.left.max(span + if f.out_fade { 0 } else { f.tail() });
                }
                self.fading.push(std::mem::replace(f, Radiator::empty()));
            }
            let mut r = std::mem::replace(&mut old.rad, Radiator::empty());
            r.g0 = g;
            r.out_fade = self.fade_out;
            r.left = span + if r.out_fade { 0 } else { r.tail() };
            self.fading.push(r);
            self.fade_at = at;
        }
        true
    }

    pub fn latency(&self) -> usize {
        // a brass instrument's resampling from its 44.1 kHz on top
        BASE + self.brass.as_ref().map_or(0, |f| f.latency())
    }

    /// the voicing's level offset of `key` (dB) at the current voicing wear: σ · its draw
    /// (0 at wear 0 or off the keyboard)
    pub fn voicing_level_db(&self, key: u8) -> f64 {
        if !(21..=108).contains(&key) {
            return 0.0;
        }
        let w = (self.params.voicing_wear as f64).clamp(0.0, 1.0);
        let kind = crate::physics::LiveWear::of(self.inst.body());
        VOICING_LEVEL_DB * kind.voicing_level * w.sqrt() * self.level_scatter[key as usize - 21]
    }

    /// The parts that reach the output (`Mix::ALL` normally; a stem renders one).
    pub fn set_mix(&mut self, mix: Mix) {
        self.mix = mix;
    }

    pub fn set_params(&mut self, p: Params) {
        if p.input > 0.0 && self.params.input == 0.0 {
            // the input's filter has not run while off: no stale history
            self.clear_input();
        }
        if p.sympathetic != self.params.sympathetic {
            self.bank.set_coupling(p.sympathetic as f64);
        }
        self.bank.set_buff(p.buff);
        let dampers = p.damper_wear != self.params.damper_wear || (p.damper_fall == 0.0 && self.params.damper_fall != 0.0);
        self.params = p;
        if dampers {
            for i in 0..88 {
                self.update_key(i);
            }
        }
    }

    pub fn params(&self) -> Params {
        self.params
    }

    /// Hammer speed (m/s) for `key` played at velocity 1…127 (a real number: the velocity curve's
    /// output, `velocity::VelocityCurve::apply`). A hammer piano with a level table plays the
    /// speed at which the key sounds `velocity::dynamics_db` re its own ff (`FF_VELOCITY`, the
    /// recorded ff speed) with the Dynamics range: the speeds of the table's rows on either side,
    /// interpolated over the key. Otherwise ln speed is piecewise linear through the profile's
    /// pp/mf/ff anchors, extrapolated with the outer slopes. Allocation-free.
    pub fn hammer_speed(&self, key: u8, velocity: f64) -> f64 {
        let v = velocity.clamp(1.0, 127.0);
        if self.brass.is_some() {
            // the velocity over 127; Dynamics as on the pianos: less brings the soft notes up, more takes them down
            let range = (self.params.dynamics as f64).clamp(DYNAMICS_RANGE.0, DYNAMICS_RANGE.1);
            return (v / 127.0).powf(range / crate::velocity::DYNAMICS_DB);
        }
        let d = &self.inst.dynamics;
        if !d.is_empty() && self.mallet().is_none() {
            use crate::velocity::{FF_VELOCITY, dynamics_db};
            let range = (self.params.dynamics as f64).clamp(DYNAMICS_RANGE.0, DYNAMICS_RANGE.1);
            let db = dynamics_db(v, range) - dynamics_db(FF_VELOCITY, range);
            let j = d.keys.partition_point(|&k| k <= key).clamp(1, d.keys.len() - 1);
            let (k0, k1) = (d.keys[j - 1] as f64, d.keys[j] as f64);
            let t = ((key as f64 - k0) / (k1 - k0)).clamp(0.0, 1.0);
            let x = (1.0 - t) * speed_at_level(&d.ln_speed, &d.db[j - 1], db) + t * speed_at_level(&d.ln_speed, &d.db[j], db);
            return x.exp();
        }
        let m = self.inst.velocity_midi;
        let ls = self.mallet().map_or(self.inst.velocity_speed, |m| m.speed).map(f64::ln);
        let seg = if v < m[1] { 0 } else { 1 };
        let t = (v - m[seg]) / (m[seg + 1] - m[seg]);
        (ls[seg] + t * (ls[seg + 1] - ls[seg])).exp()
    }

    /// The chosen mallet, on an instrument with several.
    fn mallet(&self) -> Option<&crate::instrument::Mallet> {
        let ms = &self.inst.mallets;
        ms.get(self.params.mallet as usize).or(ms.first())
    }

    /// Panic: every key and pedal released, and the output faded to nothing over SILENCE_S;
    /// then everything that sounds is cleared (strings, the bank's ring, the radiation's and
    /// the room's tails, one-shots), so nothing comes back. Notes played during the fade are
    /// cleared with it. Allocation-free.
    pub fn silence(&mut self) {
        self.queue.clear();
        self.sound_off();
    }

    /// The silence without touching the queue (the queued `Event::Silence`).
    fn sound_off(&mut self) {
        self.release_all();
        if self.silence_left == 0 {
            self.silence_left = self.silence_len;
        }
    }

    /// The fade's gain for the next output sample; at its end the sound is cleared.
    fn silence_gain(&mut self) -> f32 {
        if self.silence_left == 0 {
            return 1.0;
        }
        let k = (self.silence_len - self.silence_left) as f64 / self.silence_len as f64;
        self.silence_left -= 1;
        if self.silence_left == 0 {
            self.clear_sound();
        }
        (0.5 * (1.0 + (std::f64::consts::PI * k).cos())) as f32
    }

    fn clear_sound(&mut self) {
        for v in &mut self.voices {
            v.silence();
        }
        if let Some(f) = &mut self.brass {
            f.clear();
        }
        self.bank.clear();
        for r in std::iter::once(&mut self.rad).chain(self.fading.iter_mut()) {
            r.body.iter_mut().flatten().for_each(Convolver::clear);
            r.room.iter_mut().for_each(Convolver::clear);
            r.diffuse.iter_mut().for_each(Convolver::clear);
        }
        self.fading.iter_mut().for_each(|f| f.left = 0);
        self.overlays.clear();
        self.out_fifo.iter_mut().for_each(|o| *o = [0.0; 2]);
        for b in [&mut self.blk_v, &mut self.blk_h, &mut self.blk_t, &mut self.diffuse, &mut self.rattle_out] {
            b.fill(0.0);
        }
        for b in self.blk_out.iter_mut().chain(self.rad_body.iter_mut()).chain(self.rad_room.iter_mut()) {
            b.fill(0.0);
        }
        self.rattle_hp.reset();
        self.clear_input();
    }

    fn clear_input(&mut self) {
        self.in_conv.clear();
        self.in_buf.fill(0.0);
        self.in_drive[0].fill(0.0);
    }

    /// Release all keys and the pedal now (dampers fall; sounding notes decay naturally).
    pub fn all_keys_up(&mut self) {
        self.queue.clear();
        self.release_all();
    }

    fn release_all(&mut self) {
        self.pedal = 0.0;
        self.pedal_lifted = false;
        self.sostenuto = false;
        self.sostenuto_held = [false; 88];
        self.soft = 0.0;
        for i in 0..88 {
            if self.key_down[i] {
                self.apply(Event::KeyUp { key: (i + 21) as u8 });
            }
        }
        for i in 0..88 {
            self.update_key(i);
        }
    }

    /// Continue the alternation of the pedal thumps as if the pedal had been lifted and
    /// dropped `cycles` times before (for an engine that starts in the middle of a piece).
    pub fn set_pedal_cycles(&mut self, cycles: usize) {
        self.pedal_take = cycles;
    }

    /// Current sample position (events are queued relative to it).
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Queue an event at an absolute sample position (kept in time order; events at the
    /// same position apply in the order they were queued).
    pub fn queue(&mut self, at: u64, ev: Event) {
        let i = self.queue.iter().rposition(|&(t, _)| t <= at).map_or(0, |i| i + 1);
        self.queue.insert(i, (at, ev));
    }

    /// An incoming note-on's velocity (before the velocity curve) into the attached telemetry
    /// region, enabled or not (the plugin's Touch tab shows it). Real-time safe.
    pub fn note_seen(&self, velocity: u8) {
        if let Some(t) = &self.telemetry {
            t.region.push_note(velocity);
        }
    }

    /// Attach (or, with `None`, detach) the diagnostics region of the plugin. Real-time safe.
    pub fn attach_telemetry(&mut self, region: Option<Region>, rate: f64) {
        self.telemetry = region.map(|region| Telemetry {
            region,
            rate,
            interval: (rate / telemetry::FRAME_HZ).max(1.0) as u64,
            next: self.now,
            counter: 0,
        });
    }

    /// Frames built and offered to the region since the last `attach_telemetry` (skipped ones are
    /// not counted). A cheap probe for tests and `bench`.
    pub fn telemetry_frames_built(&self) -> u64 {
        self.telemetry.as_ref().map_or(0, |t| t.counter)
    }

    /// The soft pedal's depth, 0 … 1.
    pub fn soft_depth(&self) -> f32 {
        self.soft as f32
    }

    /// Whether the sostenuto pedal is down.
    pub fn sostenuto_down(&self) -> bool {
        self.sostenuto
    }

    pub fn pedal_depth(&self) -> f32 {
        self.pedal as f32
    }

    /// At the end of a block: when telemetry is on (`enabled`, read once by the caller) and a
    /// frame is due, build and publish one.
    fn telemetry_tick(&mut self, enabled: bool) {
        let Some(t) = self.telemetry.as_ref() else { return };
        if !enabled || self.now < t.next {
            return;
        }
        let (region, rate, interval) = (t.region, t.rate, t.interval);
        if !region.has_frame_space() {
            // a stalled reader: the frame would only be dropped, so do not build it, but count it
            region.count_drop();
            if let Some(t) = self.telemetry.as_mut() {
                t.next = self.now + interval;
            }
            return;
        }
        let Some(mut frame) = self.tele_frame.take() else { return };
        self.telemetry_frame(&mut frame);
        if let Some(t) = self.telemetry.as_mut() {
            t.counter += 1;
            frame.counter = t.counter;
            t.next = self.now + interval;
        }
        frame.rate = rate as f32;
        region.push_frame(&frame);
        self.tele_frame = Some(frame);
    }

    pub fn stats(&self) -> Stats {
        Stats {
            voices: self.voices.iter().filter(|v| v.active).count(),
            modes: self.voices.iter().map(Voice::modes).sum(),
            sympathetic: self.bank.running(),
            runaways: self.voices.iter().map(|v| v.runaways as usize).sum::<usize>() + self.guarded,
        }
    }

    /// Damper engagement of key i from its key and the pedal. The damper lifts off the
    /// strings at once; it falls onto them over `fall_s` × the user's factor
    /// (`advance_dampers`). When it leaves the strings (engagement falls through ½), the
    /// strings get the lift kick. The key's dampers act on the strings of every register
    /// (string j = r·88 + i).
    fn update_key(&mut self, i: usize) {
        // a brass export has no notes (its players hold the keys): nothing on the strings to update
        let Some(note) = self.inst.notes.get(i) else { return };
        // the vibraphone's damper bar too: played from a keyboard, a held key lifts it off its bar
        let held = self.key_down[i] || self.sostenuto_held[i];
        let mut e = if !note.has_damper || held { 0.0 } else { pedal_engage(self.pedal) };
        // muted strings: a hand or felt rests on the strings while the key is down
        if self.params.prep == Prep::Muted && note.has_damper && self.key_down[i] {
            e = self.params.mute as f64 * MUTE_ENGAGEMENT;
        }
        self.engage_target[i] = e;
        if e > self.engage[i] && self.engage[i] >= 0.0 && self.params.damper_fall > 0.0 {
            // landing: the damper's position carries on from where it is
            if self.damper_pos[i] < self.engage[i] {
                self.damper_pos[i] = self.engage[i];
            }
            return;
        }
        self.damper_pos[i] = e;
        self.apply_engage(i, e);
    }

    /// The dampers on their way down move on by n samples.
    fn advance_dampers(&mut self, n: usize) {
        let dt = n as f64 * self.voices[0].dt();
        let factor = self.params.damper_fall as f64;
        for i in 0..88 {
            let target = self.engage_target[i];
            if self.damper_pos[i] >= target || self.engage[i] < 0.0 {
                continue;
            }
            let pos = if factor > 0.0 { (self.damper_pos[i] + dt / (self.fall_s[i] * factor)).min(target) } else { target };
            self.damper_pos[i] = pos;
            // the poles are recomputed per engagement step, not per block
            let q = if pos >= target { target } else { (pos * ENGAGE_STEPS).floor() / ENGAGE_STEPS };
            if q != self.engage[i] {
                self.apply_engage(i, q);
            }
        }
    }

    /// The damper of key i at engagement e now (the strings, the bank, kicks and buzz).
    fn apply_engage(&mut self, i: usize, e: f64) {
        let before = self.engage[i];
        self.engage[i] = e;
        // worn damper felt damps less
        let eff = e * (1.0 - DAMPER_WEAR * self.params.damper_wear as f64);
        for r in 0..self.inst.registers() {
            let j = r * 88 + i;
            let v = &mut self.voices[j];
            v.set_engage(eff);
            // `own` (the string sounds by its own voice, so the bank leaves it alone) stays until
            // the voice has died away (`end_block`), not just until the damper lands: a damper
            // that lifts again finds the remnant still sounding
            // also when e is unchanged: a strike changes `own`
            self.bank.set_key(j, KeyState { driven: e < 1.0 && !v.own, engage: eff });
            if before > 0.5 && e <= 0.5 {
                self.lift_kick(j);
            }
            if (0.0..=0.5).contains(&before) && e > 0.5 && self.params.damper_buzz > 0.0 {
                self.voices[j].start_buzz();
            }
        }
    }

    /// The registers a note-on excites (bit r = register r): those whose stop is on (a one-register
    /// instrument: register 0).
    fn stop_mask(&self) -> u32 {
        let r = self.inst.registers();
        if r == 1 {
            return 1;
        }
        (0..r).filter(|&k| self.params.stops >> self.inst.register_stops[k] & 1 != 0).fold(0, |m, k| m | 1 << k)
    }

    /// The registers the lute row plucks at key i (`Params::lute`).
    fn lute_mask(&self, i: usize) -> u32 {
        (0..self.inst.registers()).filter(|r| self.inst.notes[r * 88 + i].spec.lute.is_some()).fold(0, |m, r| m | 1 << r)
    }

    /// The strings j (= r·88 + key − 21) ring as their dampers leave them (`Pedal`, `Voice::kick`).
    fn lift_kick(&mut self, i: usize) {
        let Some(p) = &self.inst.pedal else { return };
        let gain = self.params.pedal_ring as f64 * p.kick_amp;
        let note = &self.inst.notes[i];
        let bell = (-0.5 * ((note.note as f64 - p.kick_center) / p.kick_spread).powi(2)).exp();
        if gain * bell > 1e-4 * p.kick_amp {
            self.voices[i].kick(note, gain * bell, p.kick_rolloff, p.kick_seed + note.note as u64);
        }
    }

    /// Peak bridge force of a key struck at ff (first 30 ms; the rattle's scale).
    fn ff_peak_force(&self, key: u8) -> f64 {
        let i = key as usize - 21;
        let note = &self.inst.notes[i];
        let sr = 1.0 / self.voices[i].dt();
        let mut v = Voice::new(note, sr, self.inst.reference_sr, self.inst.contact_max_s, self.inst.contact_separation_s, BASE);
        v.set_engage(0.0);
        v.excite(note, self.inst.velocity_speed[2], 1.0, 0.0, 0.0);
        let (mut av, mut ah, mut at) = (vec![[0.0f32; LANES]; BASE], vec![[0.0f32; LANES]; BASE], vec![[0.0f32; LANES]; BASE]);
        let (mut ov, mut oh) = (vec![0.0; BASE], vec![0.0; BASE]);
        let mut peak: f64 = 0.0;
        for _ in 0..(0.03 * sr) as usize / BASE {
            av.fill([0.0; LANES]);
            ah.fill([0.0; LANES]);
            at.fill([0.0; LANES]);
            ov.fill(0.0);
            oh.fill(0.0);
            v.render(note, &mut av, &mut ah, &mut at, &mut ov, &mut oh);
            kernel::lane_sums(&av, &mut ov);
            kernel::lane_sums(&at, &mut ov);
            peak = ov.iter().fold(peak, |m, x| m.max(x.abs()));
            v.end_block();
        }
        peak
    }

    /// Pedal to a new depth: dampers follow; a thump when the dampers lift off or land.
    fn set_pedal(&mut self, depth: f64) {
        self.pedal = depth.clamp(0.0, 1.0);
        for i in 0..88 {
            self.fall_s[i] = DAMPER_FALL_S; // the pedal drops the dampers of the keys that are up
            self.update_key(i);
        }
        let lifted = pedal_engage(self.pedal) <= 0.5;
        if lifted == self.pedal_lifted {
            return;
        }
        self.pedal_lifted = lifted;
        let takes = if lifted { &self.thumps_down } else { &self.thumps_up };
        if !takes.is_empty() {
            let take = self.pedal_take % takes.len();
            let gain = takes[take].1;
            let kind = if lifted { OneShot::PedalDown } else { OneShot::PedalUp };
            self.overlay(Overlay { note: take, kind, pos: 0, gain });
            if !lifted {
                self.pedal_take += 1;
            }
        }
    }

    fn overlay(&mut self, o: Overlay) {
        if self.overlays.len() < MAX_OVERLAYS {
            self.overlays.push(o);
        }
    }

    /// An event on a brass instrument: the keys and wheels are the players'; the pedals do nothing.
    fn apply_brass(&mut self, ev: Event) {
        use crate::brass::section::Ev;
        let Some(f) = &mut self.brass else { return };
        match ev {
            Event::Silence => self.sound_off(),
            Event::NoteOn { key, speed } if (21..=108).contains(&key) => {
                let i = key as usize - 21;
                if !self.key_down[i] {
                    self.down_at[i] = self.now;
                }
                self.key_down[i] = true;
                self.last_strike[i] = (self.now.max(1), speed as f32);
                f.post(Ev::On(key, (speed * 127.0).clamp(1.0, 127.0)));
            }
            Event::KeyUp { key } if (21..=108).contains(&key) => {
                self.key_down[key as usize - 21] = false;
                f.post(Ev::Off(key));
            }
            Event::PitchBend { value } => f.post(Ev::Bend(if value.is_finite() { value.clamp(-1.0, 1.0) as f64 } else { 0.0 })),
            Event::ChannelPressure { value } => f.post(Ev::Aftertouch(pressure(value) * 127.0)),
            _ => {}
        }
    }

    fn apply(&mut self, ev: Event) {
        if self.brass.is_some() {
            self.apply_brass(ev);
            return;
        }
        match ev {
            Event::Silence => self.sound_off(),
            Event::ModWheel { value } => self.wheel = pressure(value),
            Event::PitchBend { value } => self.wheel_bend = if value.is_finite() { value.clamp(-1.0, 1.0) as f64 } else { 0.0 },
            Event::ChannelPressure { value } => self.channel_pressure = pressure(value),
            Event::KeyPressure { key, value } => {
                if (21..=108).contains(&key) {
                    self.key_pressure[key as usize - 21] = pressure(value);
                }
            }
            Event::NoteOn { key, speed } => {
                let i = key as usize - 21;
                if !self.key_down[i] {
                    self.down_at[i] = self.now;
                }
                self.key_down[i] = true;
                self.struck[i] = Some(speed);
                self.last_strike[i] = (self.now.max(1), speed as f32);
                let p = &self.params;
                // the wear this kind of instrument can show
                let kind = crate::physics::LiveWear::of(self.inst.body());
                let hammer_wear = p.hammer_wear as f64 * kind.hammers;
                let db = HAMMER_WEAR_DB * hammer_wear + VOICING_WEAR_DB * kind.voicing_hardness * p.voicing_wear as f64 * self.voicing[i];
                let hardness = p.hardness as f64 * 10f64.powf(db / 20.0);
                let dp = HAMMER_WEAR_DP * hammer_wear;
                // the voicing's level scatter (none at wear 0: the export as it is)
                let scatter = if p.voicing_wear > 0.0 { 10f64.powf(self.voicing_level_db(key) / 20.0) } else { 1.0 };
                // the soft pedal: an upright's hammers arrive slower (the rail), a grand's miss
                // one string of each unison (the shift) with the less used, softer felt
                let soft = if self.inst.notes[i].pluck.is_some() { 0.0 } else { self.soft }; // plucked: none
                let (hammer_speed, miss, felt) = if self.inst.body() == crate::mic::Body::Upright {
                    (speed * (1.0 - soft * (1.0 - SOFT_UPRIGHT_SPEED)), 0.0, 1.0)
                } else {
                    (speed * (1.0 - soft * (1.0 - SOFT_GRAND_SPEED)), soft, 10f64.powf(SOFT_GRAND_FELT_DB * soft / 20.0))
                };
                let mask = self.stop_mask() | if self.params.lute { self.lute_mask(i) } else { 0 };
                self.plucked[i] |= mask;
                for r in (0..self.inst.registers()).filter(|r| mask >> r & 1 == 1) {
                    let j = r * 88 + i;
                    let p = self.params;
                    self.voices[j].set_buff(&self.inst.notes[j], p.buff);
                    self.voices[j].set_halo(p.sympathetic as f64);
                    // a membrane's buzz (the balafon's gourds): its harmonics with its noise
                    self.voices[j].set_harmonics_gain(if self.inst.membrane.is_some_and(|m| m.sympathetic) { p.sympathetic as f64 } else { 1.0 });
                    let buzz = self.mallet().map_or(1.0, |m| m.buzz);
                    self.voices[j].set_buzz_gain(buzz);
                    let own_pluck = self.inst.notes[j].pluck.is_some(); // a harpsichord plucks anyway
                    let harm = if p.prep == Prep::Harmonic { p.harmonic.clamp(2, 12) as usize } else { 1 };
                    self.voices[j].set_harmonic(harm);
                    // the key's level correction for this mallet at this strike's speed
                    let lv = &self.inst.notes[j].level_db;
                    let gain = if lv.is_empty() {
                        1.0
                    } else {
                        // an unknown mallet is the first, as `mallet()` takes it
                        let t = lv.get(p.mallet as usize).unwrap_or(&lv[0]);
                        let anchors = self.mallet().map_or(self.inst.velocity_speed, |m| m.speed);
                        level_gain(t, speed, &anchors)
                    };
                    self.voices[j].set_strike_gain(gain * scatter);
                    // the hammer's impact on the board: × its felt force, the Action knock control's
                    let impact = self.inst.impact.map_or(0.0, |m| {
                        let action = 10f64.powf(ACTION_WEAR_DB * p.action_wear as f64 / 20.0);
                        m.gain * self.inst.notes[j].impact_gain * (speed / self.inst.velocity_speed[2]).powf(m.growth) * p.knock as f64 * action
                    });
                    self.voices[j].set_impact(impact);
                    self.voices[j].set_variation(p.variation as f64);
                    // a hammer's or a mallet's strike; the kalimba's thumb plucks
                    self.voices[j].set_restrike(p.restrike > 0.5 && self.inst.body() != crate::mic::Body::Kalimba);
                    if p.prep == Prep::Plucked && !own_pluck {
                        // plucked with a finger or nail: a lowpass of the finger's pad, harder plucks
                        // brighter and louder
                        let spec = crate::physics::PluckSpec::string(
                            (p.pluck_pos as f64).clamp(0.02, 0.5),
                            PLUCK_FQ * p.pluck_bright.clamp(0.1, 10.0) as f64,
                            PLUCK_AMP,
                            PLUCK_DB_PER_STEP,
                            PLUCK_OCT_PER_STEP,
                        );
                        self.voices[j].pluck_with(&self.inst.notes[j], &spec, hammer_speed, 1.0, harm, 1);
                    } else if let (true, Some((beta, gain_db)), Some(own)) = (p.lute, self.inst.notes[j].spec.lute, self.inst.notes[j].pluck) {
                        // the lute row: the same strings plucked near the nut by a quill as strong as
                        // the row's: at one force the string gives way as β(1−β), so less near the nut
                        let force = beta * (1.0 - beta) / (own.beta * (1.0 - own.beta));
                        let spec = crate::physics::PluckSpec { beta, amp: own.amp * force * 10f64.powf(gain_db / 20.0), ..own };
                        self.voices[j].pluck_with(&self.inst.notes[j], &spec, hammer_speed, hardness * felt, harm, usize::MAX);
                        if p.prep == Prep::Paper {
                            self.voices[j].start_paper();
                        }
                    } else if let (Some(m), Some(own), Some(first)) = (self.mallet(), self.inst.notes[j].pluck, self.inst.mallets.first()) {
                        // another mallet: its lowpass, the cutoff scaled to this bar as the first's
                        let spec = crate::physics::PluckSpec {
                            f_q: m.f_q * own.f_q / first.f_q,
                            fq_oct_per_step: m.fq_oct_per_step,
                            order: m.order,
                            ..own
                        };
                        self.voices[j].pluck_with(&self.inst.notes[j], &spec, hammer_speed, hardness * felt, harm, usize::MAX);
                    } else {
                        self.voices[j].excite_with(&self.inst.notes[j], hammer_speed, hardness * felt, dp, miss, harm);
                        if p.prep == Prep::Paper {
                            self.voices[j].start_paper();
                        }
                    }
                }
                // the key's knock (register 0's), when it plucks anything
                if let (Some(k), Some((amp, q)), true) = (&self.inst.notes[i].knock, self.inst.knock, mask != 0) {
                    // the mallet's own (the instruments with several)
                    let gain = (amp * speed.powf(q) * k.knee * self.mallet().map_or(1.0, |m| m.knock)) as f32;
                    self.overlay(Overlay { note: i, kind: OneShot::Knock, pos: 0, gain });
                }
                self.update_key(i);
            }
            Event::KeyDown { key } => {
                let i = key as usize - 21;
                if !self.key_down[i] {
                    self.down_at[i] = self.now;
                }
                self.key_down[i] = true;
                self.update_key(i);
            }
            Event::KeyUp { key } => {
                let i = key as usize - 21;
                self.key_down[i] = false;
                // the release velocity: the damper's fall and the release thump's level
                let rv = std::mem::take(&mut self.release_v[i]);
                self.fall_s[i] = fall_seconds(rv);
                // the thump: louder for a faster release, smaller for a key that was hardly down
                let held_s = self.now.saturating_sub(self.down_at[i]) as f64 * self.voices[0].dt();
                let rv_gain = release_velocity_gain(rv) * (RELEASE_HELD_MIN + (1.0 - RELEASE_HELD_MIN) * (held_s / RELEASE_HELD_FULL_S).min(1.0));
                if let Some(speed) = self.struck[i].take() {
                    // a release noise per register its note-on excited (each jack falls)
                    let plucked = self.plucked[i];
                    for r in (0..self.inst.registers()).filter(|r| plucked >> r & 1 == 1) {
                        let j = r * 88 + i;
                        let gain = (release_level(&self.inst, j, speed) * rv_gain) as f32;
                        self.overlay(Overlay { note: j, kind: OneShot::Release, pos: 0, gain });
                    }
                }
                self.plucked[i] = 0;
                self.update_key(i);
            }
            Event::ReleaseVelocity { key, velocity } => self.release_v[key as usize - 21] = velocity,
            Event::PedalDown => self.set_pedal(1.0),
            Event::PedalUp => self.set_pedal(0.0),
            Event::Pedal { depth } => self.set_pedal(depth as f64),
            Event::SoftPedal { depth } => self.soft = if depth.is_finite() { depth.clamp(0.0, 1.0) as f64 } else { 0.0 },
            Event::SostenutoDown => {
                if !self.sostenuto {
                    self.sostenuto = true;
                    // the dampers off the strings now (the lift threshold of the thumps and kicks)
                    for i in 0..88 {
                        self.sostenuto_held[i] = self.engage[i] <= 0.5;
                    }
                }
            }
            Event::SostenutoUp => {
                if self.sostenuto {
                    self.sostenuto = false;
                    let held = std::mem::replace(&mut self.sostenuto_held, [false; 88]);
                    for i in (0..88).filter(|&i| held[i]) {
                        self.update_key(i);
                    }
                }
            }
        }
    }

    /// The Bebung and the bend for the next n samples. The Bebung: each held key's pitch rise
    /// approaches depth × its pressure (BEBUNG_TAU_S), a released key's falls back (the tangent has
    /// left the string). The bend: every key's strings by the pitch wheel × the range, at the same
    /// pace (the hand's pressure). The strings' modes follow from their current state
    /// (`Voice::set_detune`).
    fn pitch_step(&mut self, n: usize) {
        let a = 1.0 - (-(n as f64) * self.voices[0].dt() / BEBUNG_TAU_S).exp();
        let toward = |c: f64, target: f64| if (target - c).abs() < BEBUNG_SNAP_CENTS { target } else { c + (target - c) * a };
        let bend_before = self.bend;
        if self.has_bend {
            let range = if self.params.bend_range.is_finite() { self.params.bend_range.clamp(0.0, BEND_MAX_SEMITONES) as f64 } else { 0.0 };
            self.bend = toward(self.bend, 100.0 * range * self.wheel_bend);
        }
        let depth = if self.params.bebung.is_finite() { self.params.bebung.clamp(0.0, BEBUNG_MAX_CENTS) as f64 } else { 0.0 };
        let common = self.wheel.max(self.channel_pressure);
        let regs = self.inst.registers();
        for i in 0..88 {
            let c = self.bebung[i];
            if self.has_tangent {
                let target = if self.key_down[i] { depth * common.max(self.key_pressure[i]) } else { 0.0 };
                if c != target {
                    self.bebung[i] = toward(c, target);
                }
            }
            if self.bebung[i] == c && self.bend == bend_before {
                continue;
            }
            let d = 2f64.powf((self.bebung[i] + self.bend) / 1200.0) - 1.0;
            for r in 0..regs {
                self.voices[r * 88 + i].set_detune(d);
            }
        }
    }

    /// Render n samples (within the current block).
    /// `input`: the audio input of these samples (empty: none)
    fn render(&mut self, n: usize, input: &[f32]) {
        if self.brass.is_some() {
            // the block's sound comes whole at its end, every event of the block queued
            self.fill += n;
            self.now += n as u64;
            if self.fill == BASE {
                self.end_block();
            }
            return;
        }
        self.advance_dampers(n);
        let r = self.fill..self.fill + n;
        let zero = [0.0; LANES];
        for a in [&mut self.acc_v, &mut self.acc_h, &mut self.acc_b, &mut self.acc_t] {
            a[..n].fill(zero);
        }
        // the motor: the resonators' gains for these samples, their detune from now on
        let motor = self.has_motor && self.has_tube && !(self.motor.idle() && !self.params.motor);
        if motor {
            let p = self.params;
            self.motor.run(p.motor, p.motor_rate as f64, p.motor_depth as f64, &mut self.tube_gain[..n]);
            let d = self.motor.detune();
            if d != self.tube_detune {
                self.tube_detune = d;
                self.voices.iter_mut().for_each(|v| v.set_tube_detune(d));
            }
        }
        if self.has_tangent || self.has_bend {
            self.pitch_step(n);
        }
        let tube_gain = &self.tube_gain[..n];
        self.blk_v[r.clone()].fill(0.0);
        self.blk_h[r.clone()].fill(0.0);
        let paper = if self.params.prep == Prep::Paper { self.params.paper as f64 } else { 0.0 };
        let (grooves, buzz) = (self.params.grooves as f64, (self.params.damper_buzz as f64).max(paper));
        // the instrument's own phantom partials plus the control's, in the bass
        let phantom_all = PHANTOM_LEVEL * (self.inst.phantom + self.params.phantom as f64) / PHANTOM_REF;
        // the buzz on the sympathetic control (no bar rings along by coupling)
        let membrane = self.inst.membrane.map_or(0.0, |m| m.level * if m.sympathetic { self.params.sympathetic as f64 } else { 1.0 });
        let ff_force = self.ff_force;
        let zoned = self.rad.zoned() || self.fading.iter().any(|f| f.live() && f.zoned());
        let fill = self.fill;
        if zoned {
            for rad in std::iter::once(&mut self.rad).chain(&mut self.fading).filter(|f| f.live() && f.zoned()) {
                for zb in rad.zone_buf.iter_mut().flatten() {
                    zb[fill..fill + n].fill(0.0);
                }
                rad.diffuse_buf[fill..fill + n].fill(0.0);
            }
        }
        for (key, (voice, note)) in self.voices.iter_mut().zip(&self.inst.notes).enumerate() {
            if !voice.active {
                continue;
            }
            let phantom = phantom_all * phantom_share(note.spec.note as f64);
            if voice.buzzing() || phantom > 0.0 || zoned || membrane > 0.0 {
                // on its own: damper buzz, phantom partials and a membrane need this string's bridge force
                let (av, ah, at, ot, ov, oh, ob) = (
                    &mut self.own_acc_v[..n],
                    &mut self.own_acc_h[..n],
                    &mut self.own_acc_t[..n],
                    &mut self.own_t[..n],
                    &mut self.own_v[..n],
                    &mut self.own_h[..n],
                    &mut self.own_buzz[..n],
                );
                av.fill([0.0; LANES]);
                ah.fill([0.0; LANES]);
                ov.fill(0.0);
                oh.fill(0.0);
                ob.fill(0.0);
                if note.tube.is_some() {
                    at.fill([0.0; LANES]);
                }
                voice.render(note, av, ah, at, ov, oh);
                kernel::lane_sums(av, ov);
                kernel::lane_sums(ah, oh);
                if note.tube.is_some() {
                    ot.fill(0.0);
                    kernel::lane_sums(at, ot);
                    for ((o, &t), &g) in ov.iter_mut().zip(ot.iter()).zip(tube_gain) {
                        *o += if motor { g * t } else { t };
                    }
                }
                voice.grooves(grooves, ov);
                if voice.buzzing() {
                    voice.buzz(buzz, ov, ob);
                }
                if phantom > 0.0 {
                    voice.phantom(phantom, ff_force, ov, ob);
                }
                if membrane > 0.0 {
                    voice.membrane(membrane * note.membrane_gain, ov, ob);
                }
                for (j, b) in self.blk_v[r.clone()].iter_mut().enumerate() {
                    *b += ov[j] + ob[j];
                }
                for (b, &x) in self.blk_h[r.clone()].iter_mut().zip(oh.iter()) {
                    *b += x;
                }
                if zoned && self.mix.strings {
                    // this string's bridge point: between two zones
                    for rad in std::iter::once(&mut self.rad).chain(&mut self.fading).filter(|f| f.live() && f.zoned()) {
                        rad.feed(key, ov, ob, oh, fill);
                    }
                }
            } else {
                voice.render(note, &mut self.acc_v[..n], &mut self.acc_h[..n], &mut self.acc_t[..n], &mut self.blk_v[r.clone()], &mut self.blk_h[r.clone()]);
                voice.grooves(grooves, &mut self.blk_v[r.clone()]);
            }
        }
        kernel::lane_sums(&self.acc_v[..n], &mut self.blk_v[r.clone()]);
        kernel::lane_sums(&self.acc_h[..n], &mut self.blk_h[r.clone()]);
        if self.has_tube {
            let bt = &mut self.blk_t[..n];
            bt.fill(0.0);
            kernel::lane_sums(&self.acc_t[..n], bt);
            for ((b, &t), &g) in self.blk_v[r.clone()].iter_mut().zip(bt.iter()).zip(tube_gain) {
                *b += if motor { g * t } else { t };
            }
        }
        // rattle: a loose part touching above a threshold of the strings' bridge force
        let rattle = self.params.rattle as f64 * RATTLE_LEVEL;
        for (o, &x) in self.rattle_out[..n].iter_mut().zip(&self.blk_v[r.clone()]) {
            *o = self.rattle_hp.tick((x - self.rattle_th).max(0.0)) * rattle;
        }
        for (d, &v) in self.drive.iter_mut().zip(&self.blk_v[r.clone()]) {
            *d = v as f32;
        }
        // the audio input: a force on the bridge besides the strings' own (not radiated itself),
        // through the reciprocal response a block late
        let g = self.params.input * self.input_gain;
        if g > 0.0 {
            self.in_buf[r.clone()][..input.len()].copy_from_slice(input);
            for (d, &x) in self.drive[..n].iter_mut().zip(&self.in_drive[0][r.clone()]) {
                *d += g * x;
            }
        }
        if !self.mix.strings {
            // the strings drive the bank (above) but are not heard
            self.blk_v[r.clone()].fill(0.0);
            self.blk_h[r.clone()].fill(0.0);
        }
        self.bank.run(&self.drive[..n], &mut self.acc_b[..n]);
        // sympathetic bank and rattle: diffuse, spread evenly over the zones
        self.diffuse[..n].copy_from_slice(&self.rattle_out[..n]);
        kernel::lane_sums(&self.acc_b[..n], &mut self.diffuse[..n]);
        if !self.mix.ringing {
            self.diffuse[..n].fill(0.0);
        }
        for (b, &x) in self.blk_v[r.clone()].iter_mut().zip(&self.diffuse[..n]) {
            *b += x;
        }
        if zoned {
            for rad in std::iter::once(&mut self.rad).chain(&mut self.fading).filter(|f| f.live() && f.zoned()) {
                rad.feed_diffuse(&self.diffuse[..n], fill);
            }
        }

        // one-shots, mixed after radiation
        let (knock, release, pdown, pup) = (&self.knock, &self.release, &self.thumps_down, &self.thumps_up);
        let out = &mut self.blk_out;
        let p = self.params;
        let action = 10f32.powf(ACTION_WEAR_DB as f32 * p.action_wear / 20.0);
        let noises = self.mix.noises;
        self.overlays.retain_mut(|o| {
            let (buf, level) = match o.kind {
                OneShot::Knock => (knock[o.note].as_deref().unwrap_or(&[]), p.knock),
                OneShot::Release => (&release[o.note][..], p.release),
                OneShot::PedalDown => (&pdown[o.note].0[..], p.pedal),
                OneShot::PedalUp => (&pup[o.note].0[..], p.pedal),
            };
            let len = buf.len() / 2;
            let m = n.min(len - o.pos);
            let g = if noises { o.gain * level * action } else { 0.0 };
            for t in 0..m {
                out[0][fill + t] += g * buf[o.pos + t];
                out[1][fill + t] += g * buf[len + o.pos + t];
            }
            o.pos += m;
            o.pos < len
        });

        self.fill += n;
        self.now += n as u64;
        if self.fill == BASE {
            self.end_block();
        }
    }

    fn end_block(&mut self) {
        // the players' block: their direct sound, the room's feed in the instrument's place
        if let Some(f) = &mut self.brass {
            let (mut l, mut r) = ([0.0; BASE], [0.0; BASE]);
            f.render(&self.params.brass, self.params.variation, BASE, &mut l, &mut r, &mut self.blk_v);
            let direct = if self.mix.body { 1.0 } else { 0.0 };
            for j in 0..BASE {
                self.blk_out[0][j] = direct * l[j] as f32;
                self.blk_out[1][j] = direct * r[j] as f32;
            }
            self.blk_h.fill(0.0);
        }
        // after a handover: the input crossfades from the old setups to this one
        let start = self.now - BASE as u64;
        if self.fading.iter().any(Radiator::live) {
            let (at, len) = (self.fade_at, self.fade_now);
            let (one_body, one_room) = (&mut self.one_body, &mut self.one_room);
            // a radiator weighted at its outputs: its own block, then added × the gains
            let mut weighted = |r: &mut Radiator, gains: &[f32], conv_in: &mut [f32], body: &mut [Vec<f32>], room: &mut [Vec<f32>]| {
                r.process(&self.blk_v, &self.blk_h, None, conv_in, one_body, one_room);
                for (acc, one) in body.iter_mut().zip(one_body.iter_mut()).chain(room.iter_mut().zip(one_room.iter_mut())) {
                    for ((a, o), &g) in acc.iter_mut().zip(one.iter_mut()).zip(gains) {
                        *a += g * *o;
                        *o = 0.0;
                    }
                }
            };
            for f in self.fading.iter_mut().filter(|f| f.live()) {
                for (j, g) in self.gain_buf.iter_mut().enumerate() {
                    *g = f.g0 * (1.0 - ramp((start + j as u64).saturating_sub(at), len));
                }
                if f.out_fade {
                    weighted(f, &self.gain_buf, &mut self.conv_in, &mut self.rad_body, &mut self.rad_room);
                } else {
                    f.process(&self.blk_v, &self.blk_h, Some(&self.gain_buf), &mut self.conv_in, &mut self.rad_body, &mut self.rad_room);
                }
                f.left = f.left.saturating_sub(BASE);
            }
            for (j, g) in self.gain_buf.iter_mut().enumerate() {
                *g = ramp((start + j as u64).saturating_sub(at), len);
            }
            if self.fade_out {
                weighted(&mut self.rad, &self.gain_buf, &mut self.conv_in, &mut self.rad_body, &mut self.rad_room);
            } else {
                self.rad.process(&self.blk_v, &self.blk_h, Some(&self.gain_buf), &mut self.conv_in, &mut self.rad_body, &mut self.rad_room);
            }
        } else {
            self.rad.process(&self.blk_v, &self.blk_h, None, &mut self.conv_in, &mut self.rad_body, &mut self.rad_room);
        }
        if self.params.input > 0.0 {
            self.in_drive[0].fill(0.0);
            self.in_conv.process(&self.in_buf, &mut self.in_drive);
            self.in_buf.fill(0.0);
        }
        let Params { room, volume, .. } = self.params;
        let volume = volume * self.calibration;
        let (body, room) = (if self.mix.body && self.brass.is_none() { 1.0 } else { 0.0 }, if self.mix.room { room } else { 0.0 });
        // the enabled flag is read once per block (relaxed): the tap and the frame agree on it
        let tele_on = self.telemetry.as_ref().is_some_and(|t| t.region.enabled());
        for j in 0..BASE {
            let l = body * self.rad_body[0][j] + room * self.rad_room[0][j] + self.blk_out[0][j];
            let r = body * self.rad_body[1][j] + room * self.rad_room[1][j] + self.blk_out[1][j];
            let o = [volume * l, volume * r];
            self.out_fifo.push_back(o);
            if tele_on {
                self.tele_tap[j] = o;
            }
        }
        if tele_on && let Some(t) = self.telemetry.as_ref() {
            t.region.push_audio(&self.tele_tap);
        }
        self.rad_body.iter_mut().for_each(|c| c.fill(0.0));
        self.rad_room.iter_mut().for_each(|c| c.fill(0.0));
        self.blk_out.iter_mut().for_each(|c| c.fill(0.0));
        self.fill = 0;
        for j in 0..self.voices.len() {
            if !self.voices[j].active {
                continue;
            }
            self.voices[j].end_block();
            if !self.voices[j].active && self.voices[j].own {
                // the voice has died away: its string is the bank's again, if the damper is off it
                self.voices[j].own = false;
                let e = self.engage[j % 88];
                if e >= 0.0 {
                    let eff = e * (1.0 - DAMPER_WEAR * self.params.damper_wear as f64);
                    self.bank.set_key(j, KeyState { driven: e < 1.0, engage: eff });
                }
            }
        }
        self.bank.cull();
        self.telemetry_tick(tele_on);
    }

    /// Render stereo output: advances the engine by left.len() samples (events apply at
    /// their own sample position); the output lags by `latency()` samples.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_input(&[], left, right);
    }

    /// `process` with an audio input (mono, as long as the output; shorter: silence after it)
    /// exciting the strings through the sympathetic bank at `Params::input`.
    pub fn process_input(&mut self, input: &[f32], left: &mut [f32], right: &mut [f32]) {
        let _ftz = FlushDenormals::new();
        let len = left.len().min(right.len());
        let (mut done, mut emitted) = (0, 0);
        while done < len {
            while let Some(&(at, ev)) = self.queue.front()
                && at <= self.now
            {
                self.queue.pop_front();
                self.apply(ev);
            }
            let mut n = (BASE - self.fill).min(len - done);
            if let Some(&(at, _)) = self.queue.front() {
                n = n.min((at - self.now) as usize);
            }
            self.render(n, input.get(done..(done + n).min(input.len())).unwrap_or(&[]));
            done += n;
            while emitted < done
                && let Some([l, r]) = self.out_fifo.pop_front()
            {
                let g = self.silence_gain();
                let (l, r) = (g * l, g * r);
                let guard = OUTPUT_GUARD * self.params.volume.max(1.0);
                if l.abs() <= guard && r.abs() <= guard {
                    (left[emitted], right[emitted]) = (l, r);
                } else {
                    // a runaway (or not a number): everything cleared now, not faded out
                    self.release_all();
                    self.clear_sound();
                    self.silence_left = 0;
                    self.guarded += 1;
                    (left[emitted], right[emitted]) = (0.0, 0.0);
                }
                emitted += 1;
            }
        }
    }
}

/// Flush denormals to zero while processing (restored on drop): decaying states and
/// response tails never hit the slow subnormal path, and the result does not depend on
/// whether the host has already set the mode (DPF does, a plain render does not).
struct FlushDenormals {
    #[cfg(target_arch = "x86_64")]
    saved: u32,
    #[cfg(target_arch = "aarch64")]
    saved: u64,
}

impl FlushDenormals {
    #[cfg(target_arch = "x86_64")]
    fn new() -> Self {
        let mut saved: u32 = 0;
        // SAFETY: reads and writes only the SSE control/status register; FTZ (bit 15) and
        // DAZ (bit 6) change results only for subnormal values
        unsafe {
            std::arch::asm!("stmxcsr [{}]", in(reg) &mut saved, options(nostack));
            let new = saved | 0x8040;
            std::arch::asm!("ldmxcsr [{}]", in(reg) &new, options(nostack, readonly));
        }
        FlushDenormals { saved }
    }

    #[cfg(target_arch = "aarch64")]
    fn new() -> Self {
        let saved: u64;
        // SAFETY: reads and writes only FPCR; FZ (bit 24) flushes subnormals to zero
        unsafe {
            std::arch::asm!("mrs {}, fpcr", out(reg) saved, options(nomem, nostack));
            std::arch::asm!("msr fpcr, {}", in(reg) saved | (1 << 24), options(nomem, nostack));
        }
        FlushDenormals { saved }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    fn new() -> Self {
        FlushDenormals {}
    }
}

impl Drop for FlushDenormals {
    fn drop(&mut self) {
        // SAFETY: restores the register value read in `new`
        #[cfg(target_arch = "x86_64")]
        unsafe {
            std::arch::asm!("ldmxcsr [{}]", in(reg) &self.saved, options(nostack, readonly));
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            std::arch::asm!("msr fpcr, {}", in(reg) self.saved, options(nomem, nostack));
        }
    }
}

/// Gain from model units to full scale, from the radiated ff peak of every key (the
/// release-noise reference table): the median key peaks at `FF_MEDIAN_PEAK_DBFS`, plus the
/// instrument's own `output_gain_db`. The brass keep their fit's own level, times that gain.
fn calibration(inst: &Instrument) -> f32 {
    if inst.brass.is_some() {
        return 10f64.powf(inst.output_gain_db / 20.0) as f32;
    }
    // register 0's notes (an added register plays at its own level re these)
    let mut ff: Vec<f64> = inst.notes[..88].iter().map(|n| n.release.peak[2]).collect();
    ff.sort_by(f64::total_cmp);
    let median = ff[ff.len() / 2];
    (10f64.powf((FF_MEDIAN_PEAK_DBFS + inst.output_gain_db) / 20.0) / median) as f32
}

/// The audio input's path to the strings, by reciprocity: a sound at the instrument moves the
/// bridge as the soundboard's response from the bridge to the microphones (the vertical
/// polarization's body at the recording position, its channels averaged) over jω (a pressure, not
/// a volume source; the leaky integrator). The bridge's admittance cancels against the strings'
/// coupling κ, which follows it. Normalized to a mean power of 1 over `INPUT_NORM_HZ`.
fn input_response(inst: &Instrument, sr: f64) -> Vec<f32> {
    let body = radiation_at(&inst.radiation_v.body, inst.reference_sr, sr);
    let len = body.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let a = (-2.0 * std::f64::consts::PI * INPUT_LEAK_HZ / sr).exp();
    let mut acc = 0.0;
    let mut h: Vec<f64> = (0..len)
        .map(|t| {
            let x = body.iter().map(|c| c.get(t).copied().unwrap_or(0.0) as f64).sum::<f64>() / body.len().max(1) as f64;
            acc = a * acc + x;
            acc
        })
        .collect();
    let n = len.next_power_of_two() * 2;
    let mut buf = h.clone();
    buf.resize(n, 0.0);
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut spec = fft.make_output_vec();
    fft.process(&mut buf, &mut spec).expect("sizes from the plan");
    // log-frequency weights (1/f): every octave of the band counts alike
    let (mut p, mut w) = (0.0, 0.0);
    for (k, c) in spec.iter().enumerate() {
        let f = k as f64 * sr / n as f64;
        if f >= INPUT_NORM_HZ.0 && f <= INPUT_NORM_HZ.1 {
            p += c.norm_sqr() / f;
            w += 1.0 / f;
        }
    }
    let g = if p > 0.0 { (w / p).sqrt() } else { 0.0 };
    h.iter_mut().for_each(|x| *x *= g);
    h.into_iter().map(|x| x as f32).collect()
}

/// Seconds a damper takes to fall after a release at MIDI velocity `v` (0: not known, as 64).
fn fall_seconds(v: u8) -> f64 {
    let v = if v == 0 { 64.0 } else { v as f64 };
    (DAMPER_FALL_S * 64.0 / v).clamp(DAMPER_FALL_LIMITS.0, DAMPER_FALL_LIMITS.1)
}

/// Factor on the release thump for release velocity `v` (0: not known: 1).
fn release_velocity_gain(v: u8) -> f64 {
    if v == 0 {
        return 1.0;
    }
    (v as f64 / 64.0).powf(RELEASE_VELOCITY_Q).clamp(RELEASE_VELOCITY_GAIN.0, RELEASE_VELOCITY_GAIN.1)
}

/// Release-noise level for a note struck at `speed`: the note's reference peak
/// (interpolated over ln speed between pp/mf/ff, clamped) × the release/note ratio.
fn release_level(inst: &Instrument, i: usize, speed: f64) -> f64 {
    let rel = &inst.notes[i].release;
    let ls = speed.ln();
    let mut anchors = [[0.0; 2]; 3];
    for (a, (s, p)) in anchors.iter_mut().zip(inst.velocity_speed.iter().zip(rel.peak)) {
        *a = [s.ln(), p.ln()];
    }
    let peak = interp(ls, &anchors).exp();
    let ratio = interp(ls, &rel.ratio_db);
    peak * 10f64.powf(ratio / 20.0)
}

/// The Dynamics control's range (dB from velocity 1 to 127)
/// the Strike variation's default: none (the measured heard as more realistic, but wanted per piece,
/// design log step 130)
pub const VARIATION_DEFAULT: f32 = 0.0;
pub const DYNAMICS_RANGE: (f64, f64) = (10.0, 70.0);

/// The ln speed at which a level table's row (`levels` dB at `ln_speed`, ascending) sounds `db`:
/// linear between its points, the levels made non-falling first; beyond its ends the end
/// segments' slopes carry on, at most 6 ln units (a factor 400) past them: a key's level falls
/// with the speed (6 dB per octave and more) below the table's 1/64 of ff, and 70 dB of range asks
/// for up to 4.5 ln units more.
pub(crate) fn speed_at_level(ln_speed: &[f64], levels: &[f64], db: f64) -> f64 {
    let mut ys = [0.0f64; crate::instrument::LEVEL_SPEEDS.len()];
    let n = ln_speed.len().min(levels.len()).min(ys.len());
    let mut top = f64::MIN;
    for (y, &l) in ys.iter_mut().zip(levels) {
        top = top.max(l);
        *y = top;
    }
    let k = ys[..n].windows(2).position(|w| db <= w[1]).unwrap_or(n - 2);
    let (x0, x1, y0, y1) = (ln_speed[k], ln_speed[k + 1], ys[k], ys[k + 1]);
    let x = if y1 > y0 { x0 + (db - y0) * (x1 - x0) / (y1 - y0) } else { x1 };
    x.clamp(ln_speed[0] - 6.0, ln_speed[n - 1] + 6.0)
}

/// Piecewise-linear interpolation through sorted points, clamped at the ends (np.interp).
fn interp(x: f64, pts: &[[f64; 2]]) -> f64 {
    if x <= pts[0][0] {
        return pts[0][1];
    }
    for w in pts.windows(2) {
        if x <= w[1][0] {
            let t = (x - w[0][0]) / (w[1][0] - w[0][0]);
            return w[0][1] + t * (w[1][1] - w[0][1]);
        }
    }
    pts[pts.len() - 1][1]
}
