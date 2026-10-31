//! C interface for the plugin wrapper (`plugin/dussek.h`).
//!
//! Threading: `pe_create`/`pe_destroy` and the cache functions load and free (not
//! real-time safe; call them from a worker thread). All other functions are real-time safe and must be called from one
//! thread at a time (the audio thread). Events are timed in frames relative to the
//! start of the next `pe_process` call and must be given in time order.

use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::felt::{FeltEdit, FeltKeys, KeyFelt};
use crate::instrument::{Dynamics, FeltLeveling};
use crate::{Engine, Event, Instrument, Params, RadiationCache, Tuning, Wear};
use crate::mic::{Body, MicPosition};
use crate::telemetry::{self, Frame, Region};
use crate::velocity::{MAX_POINTS, VelocityCurve};

pub struct PeEngine {
    engine: Engine,
    curve: VelocityCurve,
}

/// A velocity curve's points as plain values (`vin`/`vout`: `in` is a Rust keyword); `n` of them
/// count (2–8, `velocity::VelocityCurve::from_points`'s rules; any other: the straight line).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeVelocityCurve {
    pub n: u32,
    pub vin: [u8; MAX_POINTS],
    pub vout: [u8; MAX_POINTS],
}

/// the curve the C struct describes (None: it breaks the rules)
fn curve_of(c: &PeVelocityCurve) -> Option<VelocityCurve> {
    let n = c.n as usize;
    if n > MAX_POINTS {
        return None;
    }
    let mut pts = [(0u8, 0u8); MAX_POINTS];
    for (k, p) in pts.iter_mut().enumerate().take(n) {
        *p = (c.vin[k], c.vout[k]);
    }
    VelocityCurve::from_points(&pts[..n])
}

fn c_curve(v: &VelocityCurve) -> PeVelocityCurve {
    let mut c = PeVelocityCurve { n: v.points().len() as u32, vin: [0; MAX_POINTS], vout: [0; MAX_POINTS] };
    for (k, (i, o)) in v.points().iter().enumerate() {
        c.vin[k] = *i;
        c.vout[k] = *o;
    }
    c
}

/// Parameters as plain values (gains linear; 1 = as fitted).
#[repr(C)]
pub struct PeParams {
    pub room: f32,
    pub knock: f32,
    pub release: f32,
    pub pedal: f32,
    pub pedal_ring: f32,
    pub sympathetic: f32,
    pub volume: f32,
    /// hammer felt stiffness factor (1 = as fitted), from the next strike
    pub hardness: f32,
    /// live wear 0…1: hammers, voicing evenness, dampers, action noise
    pub hammer_wear: f32,
    pub voicing_wear: f32,
    pub damper_wear: f32,
    pub action_wear: f32,
    pub phantom: f32,
    /// optional defects 0…1: hammer grooves, damper buzz, rattle
    pub grooves: f32,
    pub damper_buzz: f32,
    pub rattle: f32,
    /// stops: bit 0 the 8′ (register 0), bit 1 the 4′, bit 2 the 8′ II; one-register instruments ignore it
    pub stops: u32,
    /// the time a damper takes to fall onto the strings, × the normal (1; 0 instant)
    pub damper_fall: f32,
    /// the prepared piano (live): the preparation (`Prep` index: 0 none, 1 plucked, 2 muted,
    /// 3 harmonic, 4 bolt, 5 paper) and its parameters: pluck position (fraction of the string)
    /// and brightness (×), how hard the mute rests (0…1), the harmonic's number, the paper's buzz
    pub prep: u32,
    pub pluck_pos: f32,
    pub pluck_bright: f32,
    pub mute: f32,
    pub harmonic: u32,
    pub paper: f32,
    /// the harpsichords' buff stop: 1 on (from the next plucks), 0 off
    pub buff: u32,
    /// Harpsichord B's lute stop: 1 on (from the next plucks; wins over the 8′ II stop), 0 off
    pub lute: u32,
    /// the vibraphone's motor: 1 on, 0 off; its rate (pulses per second) and depth (0…1)
    pub motor: u32,
    pub motor_rate: f32,
    pub motor_depth: f32,
    /// the mallet's index (instruments with several: the marimba's yarn, cord, rubber)
    pub mallet: u32,
    /// the clavichord's Bebung: a held key's pitch rise (cents) at full pressure (0 … 200)
    pub bebung: f32,
    /// the pitch wheel's range (semitones, 0 … 12; the đàn tranh, whose strings bend)
    pub bend_range: f32,
    /// the audio input's level (linear, 0 off … 16): the input of `pe_process` drives the strings
    pub input: f32,
    /// a hammer piano's level range over the velocity (dB from 1 to 127, `engine::DYNAMICS_RANGE`)
    pub dynamics: f32,
    /// each strike slightly different, 0 none … 1 as measured … 4 (`Params::variation`)
    pub variation: f32,
    /// a re-strike meets the strings or the bar where they ring: 0 off, 1 on (`Params::restrike`)
    pub restrike: f32,
}

/// The morphs a cache keeps (a point dragged back to a recent mix builds no morph).
const CACHED_INSTRUMENTS: usize = 4;

/// What engine builds keep for the next one (worker thread): the last few loaded or morphed
/// instruments, by recipe (most recent first), and the radiation responses.
#[derive(Default)]
pub struct PeCache {
    /// (recipe, a plain instrument, the instrument)
    insts: Vec<(String, bool, Instrument)>,
    rad: RadiationCache,
    /// instruments loaded or morphed (a cache miss), for the tests
    built: usize,
    /// (recipe, the level rows with the shipped felt): a felt edit's leveling re them
    rows: Vec<(String, (Dynamics, Vec<f64>))>,
    /// (recipe with the felt edit, the instrument with it): the Original toggle switches back unmeasured
    edited: Vec<(String, Instrument)>,
    /// level rows measured, for the tests
    measured: usize,
    /// (the felt edit, its leveling as last measured, on any recipe): a quick build's stand-in
    leveling: Vec<(String, FeltLeveling)>,
    /// builds take a felt's leveling as last measured instead of measuring it (`pe_cache_set_quick`)
    quick: bool,
    /// the last build took a stand-in leveling: its own is still to be measured
    provisional: bool,
}

impl PeCache {
    fn recipe(sides: &[(String, f64)]) -> String {
        sides.iter().map(|(d, w)| format!("{d}={w:.4}")).collect::<Vec<_>>().join(",")
    }

    /// the instrument of `sides` with the player's felt (`Instrument::with_felt_edit`; none or an
    /// empty edit: as shipped): kept by recipe and edit, the shipped felt's level rows by recipe
    fn instrument_with(&mut self, sides: &[(String, f64)], felt: Option<&FeltEdit>) -> Result<Instrument, String> {
        self.provisional = false;
        let inst = self.instrument(sides)?;
        let Some(edit) = felt.filter(|e| !e.is_shipped() && inst.is_hammer_piano()) else { return Ok(inst) };
        let recipe = Self::recipe(sides);
        let felt_key = format!("{}|{}", edit.format(), edit.hardness_db);
        let key = format!("{recipe}|{felt_key}");
        if let Some(i) = self.edited.iter().position(|(k, _)| *k == key) {
            let hit = self.edited.remove(i);
            self.edited.insert(0, hit);
            return Ok(self.edited[0].1.clone());
        }
        if self.quick
            && let Some((_, lv)) = self.leveling.iter().find(|(k, _)| *k == felt_key)
        {
            // a morph's pad dragged: the felt's leveling as last measured stands in, unmeasured
            self.provisional = true;
            return Ok(inst.with_felt_leveling(edit, lv));
        }
        let rows = match self.rows.iter().position(|(k, _)| *k == recipe) {
            Some(i) => self.rows[i].1.clone(),
            None => {
                let r = inst.level_rows().map_err(|e| e.to_string())?;
                self.measured += 1;
                self.rows.insert(0, (recipe, r.clone()));
                self.rows.truncate(CACHED_INSTRUMENTS);
                r
            }
        };
        let lv = inst.felt_leveling(edit, &rows).map_err(|e| e.to_string())?;
        let made = inst.with_felt_leveling(edit, &lv);
        self.measured += 1;
        self.edited.insert(0, (key, made.clone()));
        self.edited.truncate(CACHED_INSTRUMENTS);
        self.leveling.retain(|(k, _)| *k != felt_key);
        self.leveling.insert(0, (felt_key, lv));
        self.leveling.truncate(CACHED_INSTRUMENTS);
        Ok(made)
    }

    /// the instrument of `sides` (directory, weight): kept, or loaded or morphed and kept
    fn instrument(&mut self, sides: &[(String, f64)]) -> Result<Instrument, String> {
        let key = Self::recipe(sides);
        if let Some(i) = self.insts.iter().position(|(k, _, _)| *k == key) {
            let hit = self.insts.remove(i);
            self.insts.insert(0, hit);
        } else {
            let inst = Instrument::morph(sides).map_err(|e| e.to_string())?;
            self.built += 1;
            // a plain instrument takes the place of another (browsing keeps one loaded, as before the morph)
            let plain = sides.len() == 1;
            if plain {
                self.insts.retain(|(_, p, _)| !p);
            }
            self.insts.insert(0, (key, plain, inst));
            self.insts.truncate(CACHED_INSTRUMENTS);
        }
        Ok(self.insts[0].2.clone())
    }

    #[cfg(test)]
    fn built(&self) -> usize {
        self.built
    }
}

/// The standalone's command line after its program name (`argc` strings from `argv`): `render` and
/// midi2wav's arguments (`dussek render instruments/grand_a in.mid out.wav …`). Renders as
/// midi2wav does and returns its exit status (2 for anything but `render`). Prints to the process's
/// stdout and stderr; any thread, not the audio thread.
///
/// # Safety
/// `argv` points at `argc` NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_cli(argc: i32, argv: *const *const c_char) -> i32 {
    if argv.is_null() {
        return 2;
    }
    let args: Vec<String> = (0..argc.max(0) as usize)
        .map(|i| unsafe { CStr::from_ptr(*argv.add(i)) }.to_string_lossy().into_owned())
        .collect();
    if args.first().map(String::as_str) != Some("render") {
        eprintln!("usage: dussek render INSTRUMENT IN.mid OUT.wav [options]   (dussek render --help)");
        return 2;
    }
    crate::midi2wav::main(std::iter::once("dussek render".to_string()).chain(args.into_iter().skip(1)))
}

/// At most `n` threads per build (an engine, a morph, a felt's measurement) from now on, for the
/// whole process; 0: every core (`threads::set_build_threads`). Any thread.
#[unsafe(no_mangle)]
pub extern "C" fn pe_set_build_threads(n: u32) {
    crate::threads::set_build_threads(n as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn pe_cache_new() -> *mut PeCache {
    Box::into_raw(Box::new(PeCache::default()))
}

/// Quick builds (`quick`): a felt edit's leveling (its level table and per-key shifts, measured in
/// seconds) is taken as last measured for that felt, on whatever morph point, instead of measured;
/// `pe_cache_provisional` says whether a build took it. For a morph's pad dragged with a felt set:
/// each point plays at once, the last one measured after.
///
/// # Safety
/// `c` from `pe_cache_new` (or null) and not in use by another thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_cache_set_quick(c: *mut PeCache, quick: bool) {
    // SAFETY: the caller's cache
    if let Some(c) = unsafe { c.as_mut() } {
        c.quick = quick;
    }
}

/// Whether the cache's last build took a felt's leveling as last measured (`pe_cache_set_quick`):
/// its own is still to be measured.
///
/// # Safety
/// `c` from `pe_cache_new` (or null) and not in use by another thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_cache_provisional(c: *const PeCache) -> bool {
    // SAFETY: the caller's cache
    unsafe { c.as_ref() }.is_some_and(|c| c.provisional)
}

/// # Safety
/// `c` must come from `pe_cache_new` (or be null) and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_cache_free(c: *mut PeCache) {
    if !c.is_null() {
        // SAFETY: allocated by pe_cache_new
        drop(unsafe { Box::from_raw(c) });
    }
}

fn write_error(buf: *mut c_char, len: usize, msg: &str) {
    if buf.is_null() || len == 0 {
        return;
    }
    let n = msg.len().min(len - 1);
    // SAFETY: the caller provides `len` writable bytes at `buf`
    unsafe {
        std::ptr::copy_nonoverlapping(msg.as_ptr().cast::<c_char>(), buf, n);
        *buf.add(n) = 0;
    }
}

/// Free microphones (`Tuning::mic_setup`): see `PeTuning::mic_position` 5.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeMicSetup {
    /// left, right: x, y, z (m)
    pub pos: [[f64; 3]; 2],
    /// 0 omni … 0.5 cardioid … 1 figure-8
    pub pattern: [f64; 2],
    /// degrees: 0 along +y (from the player's seat into the piano), positive towards +x
    pub yaw: [f64; 2],
    /// degrees above the horizon
    pub pitch: [f64; 2],
    /// the upright's wall (y, m)
    pub wall: f64,
}

impl Default for PeMicSetup {
    fn default() -> Self {
        PeMicSetup::from_engine(&crate::mic::MicSetup::default())
    }
}

impl PeMicSetup {
    pub fn from_engine(s: &crate::mic::MicSetup) -> Self {
        PeMicSetup {
            pos: [s.mics[0].pos, s.mics[1].pos],
            pattern: [s.mics[0].pattern, s.mics[1].pattern],
            yaw: [s.mics[0].yaw, s.mics[1].yaw],
            pitch: [s.mics[0].pitch, s.mics[1].pitch],
            wall: s.wall,
        }
    }

    /// The engine's setup, not clamped to the instrument yet; non-finite values become the defaults.
    pub fn to_engine(&self) -> crate::mic::MicSetup {
        let d = crate::mic::MicSetup::default();
        let ok = |x: f64, lo: f64, hi: f64, def: f64| if x.is_finite() { x.clamp(lo, hi) } else { def };
        let mic = |i: usize| {
            let dm = d.mics[i];
            crate::mic::Mic {
                pos: [
                    ok(self.pos[i][0], -100.0, 100.0, dm.pos[0]),
                    ok(self.pos[i][1], -100.0, 100.0, dm.pos[1]),
                    ok(self.pos[i][2], -100.0, 100.0, dm.pos[2]),
                ],
                pattern: ok(self.pattern[i], 0.0, 1.0, dm.pattern),
                yaw: ok(self.yaw[i], -360.0, 360.0, dm.yaw),
                pitch: ok(self.pitch[i], -90.0, 90.0, dm.pitch),
            }
        };
        crate::mic::MicSetup { mics: [mic(0), mic(1)], wall: ok(self.wall, 0.0, 100.0, d.wall), lid: d.lid }
    }
}

/// String tuning, compiled into the engine (changing it means a new engine).
#[repr(C)]
pub struct PeTuning {
    /// A4 in Hz (the instruments are measured re 440)
    pub a4_hz: f64,
    /// unison detuning re the measured one (1 = as measured, 0 = pure)
    pub unison_width: f64,
    /// compiled wear 0…1 (`physics::Wear`): unisons, tuning, strings, soundboard; and
    /// the variant (seed of the random draws, also of the per-key voicing)
    pub unison_wear: f64,
    pub tuning_wear: f64,
    pub string_wear: f64,
    pub board_wear: f64,
    pub falsebeat_wear: f64,
    pub wear_seed: u32,
    /// microphone position: 0 recording, 1 player, 2 audience (open lid), 3 close:
    /// hammers, 4 close: tail (under the lid), 5 free microphones
    pub mic_position: u32,
    /// spacing of the microphone pair (m; not at the recording's position; close: ≤ 1.1)
    pub mic_spacing: f64,
    /// horizontal distance from the soundboard's center, close positions: height above
    /// it (m; 0: the position's default)
    pub mic_distance: f64,
    /// level reference over the distance: 0 total (direct + room), 1 direct sound
    pub distance_level: u32,
    /// temperament (`temperament::NAMES` index, 0 equal) and the key it is centered on
    /// (0 C … 11 B)
    pub temperament: u32,
    pub temperament_key: u32,
    /// tuner's stretch on top of the recording's own (`Tuning::stretch`), −1 … 10
    pub stretch: f64,
    /// the prepared piano's bolt: its mass 0 … 1 (0 none) and position along the string (fraction)
    pub bolt_mass: f64,
    pub bolt_pos: f64,
    /// the free microphones, used when `mic_position` is 5
    pub mic_setup: PeMicSetup,
    /// the lid: 0 closed … 1 open (grand: full stick; upright: top lid open), above 1 taken off
    pub lid: f64,
    /// the pillow: 0 none … 1
    pub pillow: f64,
    /// bars: 1 all 88 keys spread along the frame, 0 its own range (`Tuning::spread_keys`)
    pub spread_keys: u32,
    /// the strings' ring re the fitted one (`Tuning::sustain`), 0.25 … 8; 0 (zero-initialized): 1
    pub sustain: f64,
    /// the board's bass (`Tuning::board_bass_db`, `board_bass`) on hammer pianos, −12 … 24 dB;
    /// 0 (zero-initialized): as shipped
    pub board_bass_db: f64,
    /// a Scala `.scl` file (UTF-8 path; null or empty: none), played where `temperament` is
    /// `pe_temperament_count()` (the plugin's "Scala file"), centered on `temperament_key`; a key map
    /// beside it of the same name (`.kbm`) lays it on the keys. Unreadable: equal temperament
    pub scale_file: *const c_char,
}

/// The engine's tuning from the C one: non-finite values become the defaults, the rest
/// is clamped to its range.
fn tuning_from(t: &PeTuning) -> Tuning {
    let ok = |x: f64, lo: f64, hi: f64, d: f64| if x.is_finite() { x.clamp(lo, hi) } else { d };
    Tuning {
        pitch_ratio: ok(t.a4_hz, 380.0, 500.0, 440.0) / 440.0,
        unison_width: ok(t.unison_width, 0.0, 5.0, 1.0),
        temperament: crate::temperament::offsets(t.temperament as usize, t.temperament_key as usize),
        as_recorded: t.temperament as usize == crate::temperament::AS_RECORDED,
        stretch: ok(t.stretch, -1.0, 10.0, 0.0),
        bolt: ok(t.bolt_mass, 0.0, 1.0, 0.0),
        bolt_pos: ok(t.bolt_pos, 0.02, 0.5, 0.31),
        wear: Wear {
            unisons: ok(t.unison_wear, 0.0, 1.0, 0.0),
            tuning: ok(t.tuning_wear, 0.0, 1.0, 0.0),
            strings: ok(t.string_wear, 0.0, 1.0, 0.0),
            soundboard: ok(t.board_wear, 0.0, 1.0, 0.0),
            false_beats: ok(t.falsebeat_wear, 0.0, 1.0, 0.0),
            seed: t.wear_seed as u64,
        },
        mic: crate::mic::MicPosition::from_index(t.mic_position),
        mic_spacing: ok(t.mic_spacing, 0.0, 10.0, 0.5),
        mic_distance: ok(t.mic_distance, 0.0, 20.0, 0.0),
        mic_setup: t.mic_setup.to_engine(),
        keep_direct: t.distance_level == 1,
        lid: crate::mic::lid_value(t.lid),
        pillow: crate::pillow::sanitize(t.pillow),
        spread_keys: t.spread_keys != 0,
        sustain: if t.sustain > 0.0 { ok(t.sustain, crate::physics::MIN_SUSTAIN, crate::physics::MAX_SUSTAIN, 1.0) } else { 1.0 },
        board_bass_db: crate::board_bass::sanitize(t.board_bass_db),
        keys: scale_from(t),
        ..Tuning::default()
    }
}

/// The Scala file's keys where the tuning plays one (`PeTuning::scale_file`).
fn scale_from(t: &PeTuning) -> Option<[f64; 88]> {
    if t.temperament as usize != crate::temperament::NAMES.len() || t.scale_file.is_null() {
        return None;
    }
    // SAFETY: non-null, the caller's C string
    let path = unsafe { CStr::from_ptr(t.scale_file) }.to_str().ok().filter(|p| !p.is_empty())?;
    crate::temperament::load_scale(std::path::Path::new(path), t.temperament_key as usize % 12).ok().map(|s| s.0)
}

/// What the Scala file at `path` (UTF-8) holds, for the UI: its note count (> 0) with "description ·
/// N notes · key map NAME.kbm" in `buf`, or 0 with the error there (`len` bytes, may be 0).
///
/// # Safety
/// `path` a valid C string or null; `buf` null or `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_scale_info(path: *const c_char, buf: *mut c_char, len: usize) -> u32 {
    let text = if path.is_null() {
        Err("no file".to_string())
    } else {
        // SAFETY: the caller's C string
        let p = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
        crate::temperament::load_scale(std::path::Path::new(&p), 0).map(|(_, d, n, kbm)| {
            let map = kbm.and_then(|k| k.file_name().map(|f| format!(" · key map {}", f.to_string_lossy()))).unwrap_or_default();
            let d = if d.is_empty() { String::new() } else { format!("{d} · ") };
            (format!("{d}{n} notes{map}"), n as u32)
        })
    };
    match text {
        Ok((s, n)) => {
            write_error(buf, len, &s);
            n
        }
        Err(e) => {
            write_error(buf, len, &e);
            0
        }
    }
}

/// Load the instrument in directory `dir` (UTF-8 path) for sample rate `sr` and
/// `tuning` (may be null: as measured). `cache` (may be null) keeps the loaded instrument
/// and the radiation for the next call: an engine for the same instrument with another
/// string tuning is then quick to build. Returns null on failure, with a message in `err`
/// (`err_len` bytes, may be null).
///
/// # Safety
/// `dir` must be a valid C string; `tuning` null or valid; `cache` null or from
/// `pe_cache_new` and not in use by another thread; `err` null or `err_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_create(
    dir: *const c_char,
    sr: f64,
    tuning: *const PeTuning,
    cache: *mut PeCache,
    err: *mut c_char,
    err_len: usize,
) -> *mut PeEngine {
    // SAFETY: one directory and its weight, the rest passed on as the caller gave them
    unsafe { pe_create_morph(&dir, &1.0, 1, sr, tuning, std::ptr::null(), cache, err, err_len) }
}

/// The (directory, weight) pairs of `count` entries of `dirs` and `weights`.
///
/// # Safety
/// `count` entries in each array, each directory a valid C string or null.
unsafe fn sides_from(dirs: *const *const c_char, weights: *const f32, count: u32) -> Result<Vec<(String, f64)>, String> {
    if dirs.is_null() || weights.is_null() || count == 0 {
        return Err("no instrument directory".into());
    }
    let mut sides = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        // SAFETY: the caller guarantees `count` entries in each array
        let (d, w) = unsafe { (*dirs.add(i), *weights.add(i)) };
        if d.is_null() {
            return Err("no instrument directory".into());
        }
        // SAFETY: checked non-null; the caller guarantees a valid C string
        match unsafe { CStr::from_ptr(d) }.to_str() {
            Ok(s) => sides.push((s.to_owned(), w as f64)),
            Err(_) => return Err("instrument path is not UTF-8".into()),
        }
    }
    Ok(sides)
}

/// As `pe_create`, for an instrument between the instruments in `dirs` at `weights` (`count` of
/// each; `Instrument::morph`: the first is the main instrument, a single one at weight 1 is that
/// instrument as loaded). `cache` keeps the last few instruments by recipe.
///
/// # Safety
/// `dirs` must hold `count` valid C strings and `weights` `count` numbers; `tuning`, `cache` and
/// `err` as for `pe_create`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn pe_create_morph(
    dirs: *const *const c_char,
    weights: *const f32,
    count: u32,
    sr: f64,
    tuning: *const PeTuning,
    felt: *const PeFelt,
    cache: *mut PeCache,
    err: *mut c_char,
    err_len: usize,
) -> *mut PeEngine {
    // SAFETY: null or a valid felt (the caller guarantees it)
    let felt = unsafe { felt.as_ref() }.map(edit_from);
    let tuning = if tuning.is_null() {
        Tuning::default()
    } else {
        // SAFETY: checked non-null; the caller guarantees validity
        tuning_from(unsafe { &*tuning })
    };
    // SAFETY: the caller's arrays
    let sides = match unsafe { sides_from(dirs, weights, count) } {
        Ok(s) => s,
        Err(msg) => {
            write_error(err, err_len, &msg);
            return std::ptr::null_mut();
        }
    };
    // SAFETY: null or a live cache used by this thread only (the caller guarantees it)
    let cache = unsafe { cache.as_mut() };
    let made = catch_unwind(AssertUnwindSafe(|| -> Result<PeEngine, String> {
        let name = sides[0].0.clone();
        let mut own = PeCache::default();
        let c = cache.unwrap_or(&mut own);
        let inst = c.instrument_with(&sides, felt.as_ref()).map_err(|e| if sides.len() == 1 { format!("{name}: {e}") } else { e })?;
        let engine = Engine::with_cache(inst, sr, tuning, &mut c.rad).map_err(|e| e.to_string())?;
        Ok(PeEngine { engine, curve: VelocityCurve::STRAIGHT })
    }));
    match made {
        Ok(Ok(e)) => Box::into_raw(Box::new(e)),
        Ok(Err(msg)) => {
            write_error(err, err_len, &msg);
            std::ptr::null_mut()
        }
        Err(_) => {
            write_error(err, err_len, "internal error while loading the instrument");
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `e` must come from `pe_create` (or be null) and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_destroy(e: *mut PeEngine) {
    if !e.is_null() {
        // SAFETY: allocated by pe_create
        drop(unsafe { Box::from_raw(e) });
    }
}

/// Let `e` (fresh from `pe_create`, not yet processed) take over from `old`, the engine
/// it replaces: sounding strings, keys, pedal and output go on without a break (at the
/// new tuning; a new microphone setup crossfades). Returns false, taking nothing, when
/// the engines do not match (another instrument or rate). Real-time safe; `old` may
/// only be destroyed afterwards.
///
/// # Safety
/// `e` and `old` must be distinct live engines.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_adopt(e: *mut PeEngine, old: *mut PeEngine) -> bool {
    if e.is_null() || old.is_null() || e == old {
        return false;
    }
    // SAFETY: the caller passes two distinct live engines
    let (e, old) = unsafe { (&mut *e, &mut *old) };
    e.curve = old.curve;
    catch_unwind(AssertUnwindSafe(|| e.engine.adopt(&mut old.engine))).unwrap_or(false)
}

/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_latency(e: *const PeEngine) -> u32 {
    // SAFETY: the caller passes a live engine
    unsafe { &*e }.engine.latency() as u32
}

/// # Safety
/// `e` must be a live engine, `p` a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_set_params(e: *mut PeEngine, p: *const PeParams) {
    // SAFETY: the caller passes a live engine and valid parameters
    let (e, p) = unsafe { (&mut *e, &*p) };
    e.engine.set_params(params_from(p));
}

/// Set the velocity curve (a struct that breaks its rules: the straight line). Real-time safe.
///
/// # Safety
/// `e` must be a live engine, `c` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_set_velocity_curve(e: *mut PeEngine, c: *const PeVelocityCurve) {
    // SAFETY: the caller passes a live engine and a valid curve
    let (e, c) = unsafe { (&mut *e, &*c) };
    e.curve = curve_of(c).unwrap_or(VelocityCurve::STRAIGHT);
}

/// Read a curve's text (`in:out` pairs between commas or spaces; empty: the straight line) into
/// `out`; false for unreadable text (and `out` the straight line). Any thread.
///
/// # Safety
/// `text` a valid C string (or null: unreadable), `out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_velocity_curve_parse(text: *const c_char, out: *mut PeVelocityCurve) -> bool {
    // SAFETY: the caller passes a valid C string or null, and a writable out
    let parsed = if text.is_null() { None } else { unsafe { CStr::from_ptr(text) }.to_str().ok().and_then(VelocityCurve::try_parse) };
    unsafe { *out = c_curve(&parsed.unwrap_or(VelocityCurve::STRAIGHT)) };
    parsed.is_some()
}

/// A curve's text (`"1:1,64:80,127:127"`) as a C string in `buf` (`len` bytes, may be 0); returns
/// its length without the terminator (≥ `len`: cut). A struct that breaks the rules: the straight line.
///
/// # Safety
/// `c` valid; `buf` null or `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_velocity_curve_format(c: *const PeVelocityCurve, buf: *mut c_char, len: usize) -> usize {
    // SAFETY: the caller passes a valid curve
    let text = curve_of(unsafe { &*c }).unwrap_or(VelocityCurve::STRAIGHT).format();
    write_error(buf, len, &text);
    text.len()
}

/// The velocity played for `velocity` (1–127, clamped; a real number, for drawing the curve).
/// A struct that breaks the rules: the straight line.
///
/// # Safety
/// `c` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_velocity_curve_eval(c: *const PeVelocityCurve, velocity: f32) -> f32 {
    // SAFETY: the caller passes a valid curve
    curve_of(unsafe { &*c }).unwrap_or(VelocityCurve::STRAIGHT).at(velocity as f64) as f32
}

/// One quantity's bends (`felt::FeltCurve`): `n` points (at most 8), keys strictly rising 21–108.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PeFeltCurve {
    pub n: u32,
    pub key: [u8; 8],
    pub bend: [f32; 8],
}

/// The player's felt (`felt::FeltEdit`): the stiffness (dB), the exponent, the core's force (N) and
/// depth (mm) as bends; `hardness_db` Hammer hardness on a hammer piano (the live hardness then 1).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PeFelt {
    pub curve: [PeFeltCurve; 4],
    pub hardness_db: f32,
}

/// Each key's felt (`felt::KeyFelt`), A0 first.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeFeltKeys {
    pub f05: [f32; 88],
    pub p: [f32; 88],
    pub core05: [f32; 88],
    pub core_depth_mm: [f32; 88],
    pub hammer: [u8; 88],
    pub core: [u8; 88],
}

impl Default for PeFeltKeys {
    fn default() -> Self {
        PeFeltKeys { f05: [0.0; 88], p: [0.0; 88], core05: [0.0; 88], core_depth_mm: [0.0; 88], hammer: [0; 88], core: [0; 88] }
    }
}

/// The edit of a C felt: a curve that breaks the rules (keys not rising in 21–108, more than 8,
/// a bend not finite) has no points; a hardness not finite is 0 dB.
fn edit_from(f: &PeFelt) -> FeltEdit {
    let mut e = FeltEdit { hardness_db: if f.hardness_db.is_finite() { f.hardness_db as f64 } else { 0.0 }, ..Default::default() };
    for (c, out) in f.curve.iter().zip(e.curves.iter_mut()) {
        let n = c.n as usize;
        let ok = n <= 8
            && c.key[..n].iter().all(|k| (21..=108).contains(k))
            && c.key[..n].windows(2).all(|w| w[1] > w[0])
            && c.bend[..n].iter().all(|b| b.is_finite());
        if ok {
            out.n = n;
            out.keys[..n].copy_from_slice(&c.key[..n]);
            (0..n).for_each(|i| out.bend[i] = c.bend[i] as f64);
        }
    }
    e
}

fn felt_to(e: &FeltEdit) -> PeFelt {
    let mut f = PeFelt { hardness_db: e.hardness_db as f32, ..Default::default() };
    for (c, out) in e.curves.iter().zip(f.curve.iter_mut()) {
        out.n = c.n as u32;
        out.key[..c.n].copy_from_slice(&c.keys[..c.n]);
        (0..c.n).for_each(|i| out.bend[i] = c.bend[i] as f32);
    }
    f
}

fn keys_from(k: &PeFeltKeys) -> FeltKeys {
    std::array::from_fn(|i| KeyFelt {
        hammer: k.hammer[i] != 0,
        f05: k.f05[i] as f64,
        p: k.p[i] as f64,
        core05: k.core05[i] as f64,
        core_depth_mm: k.core_depth_mm[i] as f64,
        core: k.core[i] != 0,
    })
}

fn keys_to(k: &FeltKeys) -> PeFeltKeys {
    let mut out = PeFeltKeys::default();
    for (i, f) in k.iter().enumerate() {
        out.f05[i] = f.f05 as f32;
        out.p[i] = f.p as f32;
        out.core05[i] = f.core05 as f32;
        out.core_depth_mm[i] = f.core_depth_mm as f32;
        out.hammer[i] = f.hammer as u8;
        out.core[i] = f.core as u8;
    }
    out
}

/// Read the "felt" state's text into `out` (the hardness 0); false for unreadable text (`out`: no
/// bends). Any thread.
///
/// # Safety
/// `text` a valid C string or null; `out` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_felt_parse(text: *const c_char, out: *mut PeFelt) -> bool {
    // SAFETY: checked non-null; the caller guarantees a valid C string
    let t = if text.is_null() { Some("") } else { unsafe { CStr::from_ptr(text) }.to_str().ok() };
    let e = t.and_then(FeltEdit::try_parse);
    // SAFETY: the caller passes a valid out
    unsafe { *out = e.as_ref().map_or_else(PeFelt::default, felt_to) };
    e.is_some()
}

/// The felt's text (`pe_felt_parse` reads it; the hardness not in it) in `buf` (`len` bytes, may be
/// 0); returns its length (≥ len: cut). Any thread.
///
/// # Safety
/// `f` valid; `buf` valid for `len` bytes (or `len` 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_felt_format(f: *const PeFelt, buf: *mut c_char, len: usize) -> usize {
    // SAFETY: the caller passes a valid felt
    let text = edit_from(unsafe { &*f }).format();
    write_error(buf, len, &text);
    text.len()
}

/// Each key's shipped felt of the instrument between `dirs` at `weights` (as `pe_create_morph`
/// builds it; a single one at weight 1 is that instrument). Not real-time safe; any thread. False
/// (`out` untouched): not loaded, with a message in `err`.
///
/// # Safety
/// `count` entries in `dirs` and `weights`, each a valid C string; `out` valid; `err` valid for
/// `err_len` bytes (or null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_felt_shipped(
    dirs: *const *const c_char,
    weights: *const f32,
    count: u32,
    out: *mut PeFeltKeys,
    err: *mut c_char,
    err_len: usize,
) -> bool {
    // SAFETY: the caller's arrays, as pe_create_morph reads them
    let sides = match unsafe { sides_from(dirs, weights, count) } {
        Ok(s) => s,
        Err(msg) => {
            write_error(err, err_len, &msg);
            return false;
        }
    };
    match catch_unwind(|| Instrument::morph(&sides).map(|i| i.felt())) {
        Ok(Ok(k)) => {
            // SAFETY: the caller passes a valid out
            unsafe { *out = keys_to(&k) };
            true
        }
        Ok(Err(e)) => {
            write_error(err, err_len, &e.to_string());
            false
        }
        Err(_) => {
            write_error(err, err_len, "internal error while loading the instrument");
            false
        }
    }
}

/// `felt` bent onto `shipped` (`felt::apply`): what the build plays, for drawing it. Any thread.
///
/// # Safety
/// `shipped`, `felt` and `out` valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_felt_apply(shipped: *const PeFeltKeys, felt: *const PeFelt, out: *mut PeFeltKeys) {
    // SAFETY: the caller passes valid structs
    let (k, f) = unsafe { (&*shipped, &*felt) };
    let r = keys_to(&crate::felt::apply(&keys_from(k), &edit_from(f)));
    // SAFETY: as above
    unsafe { *out = r };
}

/// True when `e` plays a hammer piano (`Instrument::is_hammer_piano`): its felt and Hammer
/// hardness take the build's path (`pe_create_morph`'s felt), loudness kept. Any thread.
///
/// # Safety
/// `e` a live engine or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_hammer_piano(e: *const PeEngine) -> bool {
    // SAFETY: checked non-null; the caller guarantees a live engine
    !e.is_null() && unsafe { &*e }.engine.instrument().is_hammer_piano()
}

/// The engine's parameters from the C ones: non-finite values become the defaults, the
/// rest is clamped to its range.
fn params_from(p: &PeParams) -> Params {
    let clean = |x: f32, hi: f32| if x.is_finite() { x.clamp(0.0, hi) } else { 1.0 };
    Params {
        room: clean(p.room, 4.0),
        knock: clean(p.knock, 16.0),
        release: clean(p.release, 16.0),
        pedal: clean(p.pedal, 16.0),
        pedal_ring: clean(p.pedal_ring, 16.0),
        sympathetic: clean(p.sympathetic, 4.0),
        volume: clean(p.volume, 16.0),
        hardness: if p.hardness.is_finite() { p.hardness.clamp(0.05, 20.0) } else { 1.0 },
        hammer_wear: clean(p.hammer_wear, 1.0),
        voicing_wear: clean(p.voicing_wear, 1.0),
        damper_wear: clean(p.damper_wear, 1.0),
        action_wear: clean(p.action_wear, 1.0),
        phantom: clean(p.phantom, 1.0),
        grooves: clean(p.grooves, 1.0),
        damper_buzz: clean(p.damper_buzz, 1.0),
        rattle: clean(p.rattle, 1.0),
        stops: p.stops,
        damper_fall: clean(p.damper_fall, 8.0),
        prep: crate::engine::Prep::from_index(p.prep),
        pluck_pos: if p.pluck_pos.is_finite() { p.pluck_pos.clamp(0.02, 0.5) } else { 0.2 },
        pluck_bright: if p.pluck_bright.is_finite() { p.pluck_bright.clamp(0.1, 10.0) } else { 1.0 },
        mute: clean(p.mute, 1.0),
        harmonic: p.harmonic.clamp(2, 12),
        paper: clean(p.paper, 1.0),
        buff: p.buff != 0,
        lute: p.lute != 0,
        motor: p.motor != 0,
        motor_rate: if p.motor_rate.is_finite() { p.motor_rate.clamp(0.1, 20.0) } else { 5.0 },
        motor_depth: clean(p.motor_depth, 1.0),
        mallet: p.mallet,
        bebung: if p.bebung.is_finite() { p.bebung.clamp(0.0, 200.0) } else { 30.0 },
        bend_range: if p.bend_range.is_finite() { p.bend_range.clamp(0.0, 12.0) } else { 2.0 },
        input: if p.input.is_finite() { p.input.clamp(0.0, 16.0) } else { 0.0 },
        dynamics: if p.dynamics.is_finite() {
            p.dynamics.clamp(crate::engine::DYNAMICS_RANGE.0 as f32, crate::engine::DYNAMICS_RANGE.1 as f32)
        } else {
            crate::velocity::DYNAMICS_DB as f32
        },
        variation: if p.variation.is_finite() { p.variation.clamp(0.0, 4.0) } else { crate::engine::VARIATION_DEFAULT },
        restrike: if p.restrike > 0.5 { 1.0 } else { 0.0 },
    }
}

/// The `midi2wav` options for these settings (`spec::format`: `--velocity-curve`, `--param`,
/// `--tuning`, only what differs from the defaults), as a C string in `buf` (`len` bytes, may be 0).
/// Returns the string's length without its terminator: a result ≥ `len` means it was cut.
///
/// # Safety
/// `p` and `t` must be valid, `curve` valid or null (the straight line); `buf` null or `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_format_spec(p: *const PeParams, t: *const PeTuning, curve: *const PeVelocityCurve, buf: *mut c_char, len: usize) -> usize {
    // SAFETY: the caller passes valid settings
    let (p, t) = unsafe { (&*p, &*t) };
    let curve = if curve.is_null() { VelocityCurve::STRAIGHT } else { curve_of(unsafe { &*curve }).unwrap_or(VelocityCurve::STRAIGHT) };
    // a4 and the temperament as `tuning_from` reads them (it keeps the ratio and the offsets)
    let a4 = if t.a4_hz.is_finite() { t.a4_hz.clamp(380.0, 500.0) } else { 440.0 };
    let index = t.temperament as usize;
    let temperament = if index < crate::temperament::NAMES.len() { (index, t.temperament_key as usize % 12) } else { (0, 0) };
    let mut line = crate::spec::format(&params_from(p), &curve, &tuning_from(t), a4, temperament);
    if scale_from(t).is_some() {
        // SAFETY: non-null (scale_from read it), the caller's C string
        let path = unsafe { CStr::from_ptr(t.scale_file) }.to_string_lossy().replace('\'', "'\\''");
        let sep = if line.is_empty() { "" } else { " " };
        line += &format!("{sep}--scl '{path}' --key {}", t.temperament_key % 12);
    }
    write_error(buf, len, &line);
    line.len()
}

fn queue(e: &mut PeEngine, frame: u32, ev: Event) {
    let at = e.engine.now() + frame as u64;
    e.engine.queue(at, ev);
}

/// MIDI note on (velocity 0 = note off). Keys outside 21…108 are ignored.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_note_on(e: *mut PeEngine, frame: u32, key: u8, velocity: u8) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    if !(21..=108).contains(&key) {
        return;
    }
    if velocity == 0 {
        queue(e, frame, Event::KeyUp { key });
        return;
    }
    e.engine.note_seen(velocity);
    let speed = e.engine.hammer_speed(key, e.curve.apply(velocity));
    queue(e, frame, Event::NoteOn { key, speed });
}

/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_note_off(e: *mut PeEngine, frame: u32, key: u8) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    if (21..=108).contains(&key) {
        queue(e, frame, Event::KeyUp { key });
    }
}

/// Key release with its MIDI release velocity (1…127, 64 normal; 0: not known): how fast the
/// damper falls and how loud the release thump is.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_note_off_velocity(e: *mut PeEngine, frame: u32, key: u8, velocity: u8) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    if (21..=108).contains(&key) {
        if velocity > 0 {
            queue(e, frame, Event::ReleaseVelocity { key, velocity: velocity.min(127) });
        }
        queue(e, frame, Event::KeyUp { key });
    }
}

/// Sustain pedal down/up.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_pedal(e: *mut PeEngine, frame: u32, down: bool) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    queue(e, frame, if down { Event::PedalDown } else { Event::PedalUp });
}

/// Sustain pedal position 0 (up) … 1 (down), e.g. CC64 / 127: half-pedalling between.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_pedal_depth(e: *mut PeEngine, frame: u32, depth: f32) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    let depth = if depth.is_finite() { depth.clamp(0.0, 1.0) } else { 0.0 };
    queue(e, frame, Event::Pedal { depth });
}

/// Soft pedal position 0 (up) … 1 (down), e.g. CC67 / 127: una corda on a grand, the hammer
/// rail on an upright; acts on the notes struck from then on.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_soft_pedal(e: *mut PeEngine, frame: u32, depth: f32) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    let depth = if depth.is_finite() { depth.clamp(0.0, 1.0) } else { 0.0 };
    queue(e, frame, Event::SoftPedal { depth });
}

/// The pitch wheel, −1 … 1 (MIDI's 14 bits re the middle / 8192): the sounding notes bent by
/// ± the range (`PeParams::bend_range`; the đàn tranh, others ignore it).
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_pitch_bend(e: *mut PeEngine, frame: u32, value: f32) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    queue(e, frame, Event::PitchBend { value });
}

/// The mod wheel (MIDI CC1 / 127), 0 … 1: the clavichord's Bebung on every held key (as the
/// channel pressure; others ignore it).
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_mod_wheel(e: *mut PeEngine, frame: u32, value: f32) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    queue(e, frame, Event::ModWheel { value });
}

/// Channel pressure (aftertouch / 127), 0 … 1: the Bebung on every held key.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_channel_pressure(e: *mut PeEngine, frame: u32, value: f32) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    queue(e, frame, Event::ChannelPressure { value });
}

/// Polyphonic key pressure (/ 127), 0 … 1: the Bebung on this key. Keys outside 21…108 are ignored.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_key_pressure(e: *mut PeEngine, frame: u32, key: u8, value: f32) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    if (21..=108).contains(&key) {
        queue(e, frame, Event::KeyPressure { key, value });
    }
}

/// Sostenuto pedal down/up (MIDI CC66): down holds up the dampers that are lifted at that
/// moment until it is up again.
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_sostenuto(e: *mut PeEngine, frame: u32, down: bool) {
    // SAFETY: the caller passes a live engine
    let e = unsafe { &mut *e };
    queue(e, frame, if down { Event::SostenutoDown } else { Event::SostenutoUp });
}

/// All keys and the pedal up, now (MIDI "all notes off", transport stop).
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_all_keys_up(e: *mut PeEngine) {
    // SAFETY: the caller passes a live engine
    unsafe { &mut *e }.engine.all_keys_up();
}

/// Panic: everything faded to silence over `SILENCE_S` and cleared (`Engine::silence`).
///
/// # Safety
/// `e` must be a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_silence(e: *mut PeEngine) {
    // SAFETY: the caller passes a live engine
    unsafe { &mut *e }.engine.silence();
}

/// Render `frames` stereo frames; `input` (mono, `frames` floats, or null: none) excites the
/// strings at `PeParams::input`. It may not alias `left` or `right`.
///
/// # Safety
/// `e` must be a live engine; `left`/`right` must hold `frames` floats each, `input` too
/// unless it is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_process(e: *mut PeEngine, input: *const f32, left: *mut f32, right: *mut f32, frames: u32) {
    // SAFETY: the caller passes a live engine and buffers of `frames` floats
    let (e, x, l, r) = unsafe {
        (
            &mut *e,
            if input.is_null() { &[][..] } else { std::slice::from_raw_parts(input, frames as usize) },
            std::slice::from_raw_parts_mut(left, frames as usize),
            std::slice::from_raw_parts_mut(right, frames as usize),
        )
    };
    // a panic must not unwind into the host; output silence instead
    if catch_unwind(AssertUnwindSafe(|| e.engine.process_input(x, l, r))).is_err() {
        l.fill(0.0);
        r.fill(0.0);
    }
}

/// Number of temperaments (`PeTuning::temperament` runs 0 … count − 1; 0 is equal).
#[unsafe(no_mangle)]
pub extern "C" fn pe_temperament_count() -> u32 {
    crate::temperament::NAMES.len() as u32
}

/// Name of temperament `index` (a static UTF-8 C string; null when out of range).
#[unsafe(no_mangle)]
pub extern "C" fn pe_temperament_name(index: u32) -> *const c_char {
    const NAMES: [&std::ffi::CStr; 9] = [
        c"Equal",
        c"Meantone 1/4 comma",
        c"Meantone 1/6 comma",
        c"Werckmeister III",
        c"Kirnberger III",
        c"Vallotti",
        c"Young II",
        c"Lehman (Bach)",
        c"As recorded",
    ];
    NAMES.get(index as usize).map_or(std::ptr::null(), |n| n.as_ptr())
}


/// The microphone drawing for the UI (dussek.h)
#[repr(C)]
pub struct PeMicGeometry {
    pub body: u32,
    pub has_pair: u32,
    pub mics: [[f64; 3]; 2],
    pub axis: [f64; 3],
    pub origin: [f64; 3],
    pub distance: f64,
    pub distance_min: f64,
    pub distance_max: f64,
    pub spread: [f64; 3],
    pub spacing: f64,
    pub spacing_max: f64,
    pub height_drag: u32,
    pub case_top_count: u32,
    pub case_top: [[f64; 2]; 24],
    pub keys_top: [f64; 4],
    pub case_front: [f64; 4],
    pub keys_front: [f64; 4],
    pub lid_front: [[f64; 2]; 2],
    pub floor_z: f64,
    pub wall_y: f64,
    pub markers: [[f64; 2]; 5],
    /// the second view's horizontal axis: 0 x (from the front), 1 y (from the side: the harp)
    pub front_axis: u32,
    /// the second view's outline (a, z) in polygons separated by NaN points; 0: case_front
    pub outline_front_count: u32,
    pub outline_front: [[f64; 2]; 32],
    /// 1: the free pair (position 5)
    pub custom: u32,
    pub pattern: [f64; 2],
    pub yaw: [f64; 2],
    pub pitch: [f64; 2],
}

/// The bodies by the C interface's number (`mic::BODIES`).
const BODIES: [Body; 17] = crate::mic::BODIES;

/// The body for the C interface's number (`BODIES`; out of range: the grand).
fn body_of(body: u32) -> Body {
    BODIES.get(body as usize).copied().unwrap_or(Body::Grand)
}

/// Each body's name by the C interface's number, as an instrument.json names it (`Body::from_name`).
const BODY_NAMES: [&std::ffi::CStr; 17] = [
    c"grand", c"upright", c"harp", c"leverharp", c"vibraphone", c"marimba", c"xylophone", c"bells", c"crotales",
    c"guitar", c"chimes", c"handchimes", c"kalimba", c"balafon", c"psaltery", c"dantranh", c"strumstick",
];

/// A body's name (the presets' files name a microphone preset's body by it); null out of range.
#[unsafe(no_mangle)]
pub extern "C" fn pe_body_name(body: u32) -> *const std::ffi::c_char {
    BODY_NAMES.get(body as usize).map_or(std::ptr::null(), |n| n.as_ptr())
}

/// The drawing and the pair where the engine puts it (stateless, any thread); false:
/// body or position out of range.
///
/// # Safety
/// `out` must point to a writable `PeMicGeometry`; `custom` null or valid (used when
/// `position` is 5).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_mic_geometry(
    body: u32,
    position: u32,
    spacing: f64,
    distance: f64,
    lid: f64,
    custom: *const PeMicSetup,
    out: *mut PeMicGeometry,
) -> bool {
    if out.is_null() || body as usize >= BODIES.len() || position > 5 {
        return false;
    }
    let b = body_of(body);
    let lid = crate::mic::lid_value(lid);
    let pos = MicPosition::from_index(position);
    let custom_setup = (pos == MicPosition::Custom).then(|| {
        // SAFETY: null or valid (the caller guarantees it)
        let c = unsafe { custom.as_ref() }.map_or_else(crate::mic::MicSetup::default, PeMicSetup::to_engine);
        b.clamp_setup(crate::mic::MicSetup { lid, ..c })
    });
    let setup = custom_setup.or_else(|| b.preset_setup(pos, spacing, distance, lid));
    let d = b.drawing(custom_setup.map_or_else(|| b.wall_of(pos), |s| s.wall), lid);
    let mut case_top = [[0.0; 2]; 24];
    for (dst, src) in case_top.iter_mut().zip(&d.case_top) {
        *dst = *src;
    }
    let mut outline_front = [[0.0; 2]; 32];
    for (dst, src) in outline_front.iter_mut().zip(&d.outline_front) {
        *dst = *src;
    }
    let p = b.placement(pos, spacing, distance, lid);
    let nan3 = [f64::NAN; 3];
    let g = PeMicGeometry {
        body,
        has_pair: setup.is_some() as u32,
        mics: setup.map_or([nan3; 2], |s| [s.mics[0].pos, s.mics[1].pos]),
        axis: p.map_or(nan3, |p| p.axis),
        origin: p.map_or(nan3, |p| p.origin),
        distance: p.map_or(f64::NAN, |p| p.distance),
        distance_min: p.map_or(f64::NAN, |p| p.range.0),
        distance_max: p.map_or(f64::NAN, |p| p.range.1),
        spread: p.map_or(nan3, |p| p.spread),
        spacing: p.map_or_else(
            || setup.map_or(f64::NAN, |s| crate::mic::dist(s.mics[0].pos, s.mics[1].pos)),
            |p| p.spacing,
        ),
        spacing_max: p.map_or(f64::NAN, |p| p.spacing_max),
        height_drag: p.is_some_and(|p| p.height) as u32,
        case_top_count: d.case_top.len().min(24) as u32,
        case_top,
        keys_top: d.keys_top,
        case_front: d.case_front,
        keys_front: d.keys_front,
        lid_front: d.lid_front,
        floor_z: d.floor_z,
        wall_y: d.wall_y.unwrap_or(f64::NAN),
        markers: d.markers,
        front_axis: d.front_axis as u32,
        outline_front_count: d.outline_front.len().min(32) as u32,
        outline_front,
        custom: custom_setup.is_some() as u32,
        pattern: setup.map_or([0.0; 2], |s| [s.mics[0].pattern, s.mics[1].pattern]),
        yaw: setup.map_or([0.0; 2], |s| [s.mics[0].yaw, s.mics[1].yaw]),
        pitch: setup.map_or([0.0; 2], |s| [s.mics[0].pitch, s.mics[1].pitch]),
    };
    unsafe { out.write(g) };
    true
}

/// Clamps a free-microphone setup to what the instrument allows (the same rule as the
/// engine applies); false: nothing changed or the body is out of range.
///
/// # Safety
/// `setup` must point to a writable `PeMicSetup`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_mic_clamp(body: u32, lid: f64, setup: *mut PeMicSetup) -> bool {
    if setup.is_null() || body as usize >= BODIES.len() {
        return false;
    }
    let b = body_of(body);
    let lid = crate::mic::lid_value(lid);
    // SAFETY: checked non-null; the caller guarantees validity
    let s = unsafe { &mut *setup };
    let clamped = PeMicSetup::from_engine(&b.clamp_setup(crate::mic::MicSetup { lid, ..s.to_engine() }));
    let changed = clamped != *s;
    *s = clamped;
    changed
}

/// Size of the diagnostics region in bytes (`pe_telemetry_init`, `pe_telemetry_attach`).
#[unsafe(no_mangle)]
pub extern "C" fn pe_telemetry_region_size() -> usize {
    telemetry::region_size()
}

/// Prepare a freshly mapped region (zeroes it and writes the header): once, by its creator.
///
/// # Safety
/// `base` must point to `size` writable bytes, 8-aligned, in use by no reader or writer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_init(base: *mut u8, size: usize) -> bool {
    unsafe { Region::init(base, size) }.is_some()
}

/// The engine writes telemetry into the region while its enabled flag is set; `base` null
/// detaches. Real-time safe. `rate`: the sample rate.
///
/// # Safety
/// `e` must be live; a non-null `base` must be a region prepared by `pe_telemetry_init`
/// that outlives the attachment (detach or destroy the engine first).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_attach(e: *mut PeEngine, base: *mut u8, size: usize, rate: f64) -> bool {
    let e = unsafe { &mut *e };
    if base.is_null() {
        e.engine.attach_telemetry(None, rate);
        return true;
    }
    match unsafe { Region::open(base, size) } {
        Some(r) => {
            e.engine.attach_telemetry(Some(r), rate);
            true
        }
        None => false,
    }
}

/// What the plugin shows without shared memory: held and ringing keys, voices, pedal.
#[repr(C)]
pub struct PeDiag {
    pub keys: [u32; telemetry::KEY_WORDS],
    pub voices: u32,
    pub pedal: f32,
    /// 1 while the sostenuto pedal is down, else 0
    pub sostenuto: f32,
    /// the soft pedal's depth 0 … 1
    pub soft: f32,
}

/// # Safety
/// `e` must be live and `out` writable. Real-time safe (audio thread).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_diag(e: *const PeEngine, out: *mut PeDiag) {
    let (e, out) = unsafe { (&*e, &mut *out) };
    out.keys = e.engine.key_words();
    let s = e.engine.stats();
    out.voices = s.voices as u32;
    out.pedal = e.engine.pedal_depth();
    out.sostenuto = e.engine.sostenuto_down() as u8 as f32;
    out.soft = e.engine.soft_depth();
}

/// Reader side (UI): switch the engine's telemetry on or off.
///
/// # Safety
/// `base` must be a mapped region of `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_set_enabled(base: *mut u8, size: usize, on: bool) -> bool {
    match unsafe { Region::open(base, size) } {
        Some(r) => {
            r.set_enabled(on);
            true
        }
        None => false,
    }
}

/// Reader side: the newest frame not yet taken (false: none, or `out` null/misaligned).
///
/// # Safety
/// `base` a mapped region of `size` bytes; `out`, if not null, writable (a `PeTelemetryFrame`) and
/// 8-byte aligned (a misaligned `out` is refused, not written).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_read_frame(base: *mut u8, size: usize, out: *mut Frame) -> bool {
    if out.is_null() || !out.is_aligned() {
        return false;
    }
    match unsafe { Region::open(base, size) } {
        Some(r) => r.read_latest(unsafe { &mut *out }),
        None => false,
    }
}

/// Reader side: interleaved stereo output frames after `*cursor`; returns the number of frames
/// written to `out` (≤ `max_frames`). A new reader starts its cursor at `pe_telemetry_audio_head`
/// (0 would first return up to a ring of old audio). 0 also for a null or misaligned `out`/`cursor`.
///
/// # Safety
/// `base` a mapped region of `size` bytes; `cursor`, if not null, valid and 8-byte aligned;
/// `out`, if not null, writable for `2 · max_frames` floats and 4-byte aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_read_audio(base: *mut u8, size: usize, cursor: *mut u64, out: *mut f32, max_frames: u32) -> u32 {
    if cursor.is_null() || !cursor.is_aligned() || out.is_null() || !out.is_aligned() {
        return 0;
    }
    match unsafe { Region::open(base, size) } {
        Some(r) => {
            let out = unsafe { std::slice::from_raw_parts_mut(out, 2 * max_frames as usize) };
            r.read_audio(unsafe { &mut *cursor }, out) as u32
        }
        None => 0,
    }
}

/// Reader side: the writer's published audio position (frames since the region was prepared), 0
/// for an invalid region. A reader that starts its cursor here gets only audio written afterwards.
///
/// # Safety
/// `base` a mapped region of `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_audio_head(base: *mut u8, size: usize) -> u64 {
    unsafe { Region::open(base, size) }.map_or(0, |r| r.audio_head())
}

/// Reader side: the notes written so far (0 for an invalid region); a reader that starts its
/// cursor here gets only the notes played afterwards.
///
/// # Safety
/// `base` a mapped region of `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_notes_head(base: *mut u8, size: usize) -> u64 {
    unsafe { Region::open(base, size) }.map_or(0, |r| r.notes_head())
}

/// Reader side: the incoming note-on velocities after `*cursor`, oldest first, into `out`
/// (≤ `max`); returns how many. Behind by more than the ring: the newest. 0 also for an invalid
/// region or a null or misaligned `cursor`, a null `out`.
///
/// # Safety
/// `base` a mapped region of `size` bytes; `cursor`, if not null, valid and 8-byte aligned;
/// `out`, if not null, writable for `max` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_read_notes(base: *mut u8, size: usize, cursor: *mut u64, out: *mut u8, max: u32) -> u32 {
    if cursor.is_null() || !cursor.is_aligned() || out.is_null() {
        return 0;
    }
    match unsafe { Region::open(base, size) } {
        Some(r) => r.read_notes(unsafe { &mut *cursor }, unsafe { std::slice::from_raw_parts_mut(out, max as usize) }) as u32,
        None => 0,
    }
}

/// Writer side (the plugin's audio thread): slot `index` (< 128) of the value mirror, its sequence
/// one up. False for an invalid region or an index out of range.
///
/// # Safety
/// `base` a mapped region of `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_set_value(base: *mut u8, size: usize, index: u32, value: f32) -> bool {
    match unsafe { Region::open(base, size) } {
        Some(r) if (index as usize) < telemetry::VALUE_SLOTS => {
            r.set_value(index as usize, value);
            true
        }
        _ => false,
    }
}

/// Reader side: slot `index`'s sequence (0: never set) and value. False for an invalid region, an
/// index out of range or a null output.
///
/// # Safety
/// `base` a mapped region of `size` bytes; `seq` and `value`, if not null, valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_read_value(base: *mut u8, size: usize, index: u32, seq: *mut u32, value: *mut f32) -> bool {
    if seq.is_null() || value.is_null() {
        return false;
    }
    match unsafe { Region::open(base, size) }.and_then(|r| r.read_value(index as usize)) {
        Some((s, v)) => {
            unsafe {
                *seq = s;
                *value = v;
            }
            true
        }
        None => false,
    }
}

/// Frames the engine skipped or dropped because the reader was behind (the ring was full).
///
/// # Safety
/// `base` a mapped region of `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_dropped(base: *mut u8, size: usize) -> u32 {
    unsafe { Region::open(base, size) }.map_or(0, |r| r.dropped())
}

/// Creator: record the owning process id in the region (once, after `pe_telemetry_init`), so that
/// `pe_telemetry_owner` can tell a region left by a dead process from a live one.
///
/// # Safety
/// `base` a mapped region of `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_set_owner(base: *mut u8, size: usize, pid: u32) -> bool {
    match unsafe { Region::open(base, size) } {
        Some(r) => {
            r.set_owner(pid);
            true
        }
        None => false,
    }
}

/// The owner recorded by `pe_telemetry_set_owner`; 0 if none, or not a valid region.
///
/// # Safety
/// `base` a mapped region of `size` bytes (read access suffices).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_telemetry_owner(base: *mut u8, size: usize) -> u32 {
    unsafe { Region::open(base, size) }.map_or(0, |r| r.owner())
}

/// A control's label in the instrument's own words (dussek.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PeLabel {
    pub symbol: [c_char; 32],
    pub name: [c_char; 64],
    /// empty: the control's own
    pub description: [c_char; 1024],
}

/// What an instrument has (`info::Info` and its menu block; dussek.h).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PeInstrumentInfo {
    pub body: u32,
    pub exciter: u32,
    pub recorded_lo: u32,
    pub recorded_hi: u32,
    pub registers: u32,
    /// the stops that play a register (bits of `stops`; none with one register)
    pub stops: u32,
    pub buff: bool,
    pub lute: bool,
    pub motor: bool,
    pub bend: bool,
    pub bebung: bool,
    pub dampers: bool,
    pub bars: bool,
    pub spread: bool,
    pub lid: bool,
    pub soft_pedal: bool,
    pub mallet_count: u32,
    /// the menu block's display names where given, else the file's
    pub mallets: [[c_char; 32]; 8],
    /// bit k: Prep k (1 Plucked … 5 Paper) acts
    pub preparations: u32,
    /// bit `info::Control`: the control has no effect
    pub inert: u32,
    pub has_menu: bool,
    pub hidden: bool,
    /// the menu block's `reversed_keys`: dark naturals, light sharps
    pub reversed_keys: bool,
    pub name: [c_char; 64],
    pub group: [c_char; 64],
    pub label_count: u32,
    pub labels: [PeLabel; 8],
    /// which instruments morph together (`Info::morph_family`; 0: this one does not)
    pub morph_family: u32,
    /// a hammer piano: the felt bends (`pe_create_morph`'s felt)
    pub hammer_piano: bool,
}

impl Default for PeInstrumentInfo {
    fn default() -> Self {
        // SAFETY: integers, bools and char arrays: all-zero is a valid value
        unsafe { std::mem::zeroed() }
    }
}

/// `s` into `dst` NUL-terminated: the longest prefix that fits and ends on a character boundary.
fn copy_str(dst: &mut [c_char], s: &str) {
    if dst.is_empty() {
        return;
    }
    let mut n = s.len().min(dst.len() - 1);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    for (d, &b) in dst.iter_mut().zip(&s.as_bytes()[..n]) {
        *d = b as c_char;
    }
    dst[n] = 0;
}

impl PeInstrumentInfo {
    fn from_info(i: &crate::info::Info) -> Self {
        let mut o = PeInstrumentInfo {
            body: i.body.index(),
            exciter: i.exciter as u32,
            recorded_lo: i.recorded.0 as u32,
            recorded_hi: i.recorded.1 as u32,
            registers: i.registers as u32,
            stops: i.stops,
            buff: i.buff,
            lute: i.lute,
            motor: i.motor,
            bend: i.bend,
            bebung: i.bebung,
            dampers: i.dampers,
            bars: i.bars,
            spread: i.spread,
            lid: i.lid,
            soft_pedal: i.soft_pedal,
            preparations: i.preparations.iter().fold(0, |m, &k| m | 1 << k),
            inert: i.inert,
            morph_family: i.morph_family,
            hammer_piano: i.hammer_piano,
            ..Default::default()
        };
        let shown = i.menu.as_ref().map(|m| &m.mallets).filter(|m| m.len() == i.mallets.len()).unwrap_or(&i.mallets);
        o.mallet_count = shown.len().min(o.mallets.len()) as u32;
        for (d, s) in o.mallets.iter_mut().zip(shown) {
            copy_str(d, s);
        }
        if let Some(m) = &i.menu {
            o.has_menu = true;
            o.hidden = m.hidden;
            o.reversed_keys = m.reversed_keys;
            copy_str(&mut o.name, &m.name);
            copy_str(&mut o.group, &m.group);
            o.label_count = m.labels.len().min(o.labels.len()) as u32;
            for (d, (sym, l)) in o.labels.iter_mut().zip(&m.labels) {
                copy_str(&mut d.symbol, sym);
                copy_str(&mut d.name, &l.name);
                copy_str(&mut d.description, l.description.as_deref().unwrap_or(""));
            }
        }
        o
    }
}

/// What an instrument has (`info::info`, dussek.h), from its instrument.json alone (any
/// thread). false: the file is missing or rejected, with a message in `err` (`out` untouched).
///
/// # Safety
/// `dir` a valid C string; `out` writable; `err` null or `err_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pe_instrument_info(dir: *const c_char, out: *mut PeInstrumentInfo, err: *mut c_char, err_len: usize) -> bool {
    if dir.is_null() || out.is_null() {
        write_error(err, err_len, "no instrument directory");
        return false;
    }
    // SAFETY: checked non-null; the caller guarantees a valid C string
    let Ok(dir) = unsafe { CStr::from_ptr(dir) }.to_str() else {
        write_error(err, err_len, "instrument path is not UTF-8");
        return false;
    };
    match catch_unwind(|| crate::info::info(dir).map(|i| PeInstrumentInfo::from_info(&i))) {
        Ok(Ok(i)) => {
            // SAFETY: checked non-null; the caller guarantees it is writable
            unsafe { *out = i };
            true
        }
        Ok(Err(e)) => {
            write_error(err, err_len, &format!("{dir}: {e}"));
            false
        }
        Err(_) => {
            write_error(err, err_len, "internal error while reading the instrument");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::Frame;

    #[test]
    fn a_scala_file_plays_where_the_menu_says_so() {
        let dir = std::env::temp_dir().join(format!("pe-ffi-scale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let scl = dir.join("19 equal.scl");
        let text: String = std::iter::once("19 equal\n19\n".to_string()).chain((1..=19).map(|i| format!("{:.5}\n", 1200.0 * i as f64 / 19.0))).collect();
        std::fs::write(&scl, text).unwrap();
        let path = std::ffi::CString::new(scl.to_str().unwrap()).unwrap();
        let mut t = PeTuning { scale_file: path.as_ptr(), ..zeroed_tuning() };
        t.temperament = crate::temperament::NAMES.len() as u32; // "Scala file"
        let keys = tuning_from(&t).keys.expect("the file's scale");
        assert!(keys[69 - 21].abs() < 1e-9 && (keys[70 - 21] - (1200.0 / 19.0 - 100.0)).abs() < 1e-4, "{} {}", keys[69 - 21], keys[70 - 21]); // the file: 5 decimals
        t.temperament = 3; // a named temperament: the file is not played
        assert!(tuning_from(&t).keys.is_none());
        t.temperament = crate::temperament::NAMES.len() as u32;
        // the command line carries it, quoted
        let (p, mut buf) = (zeroed_params(), vec![0 as c_char; 1024]);
        // SAFETY: valid settings and buffer
        let n = unsafe { pe_format_spec(&p, &t, std::ptr::null(), buf.as_mut_ptr(), buf.len()) };
        let line = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned();
        assert!(n > 0 && line.contains("--scl '") && line.contains("19 equal.scl' --key 0"), "{line}");
        let mut info = vec![0 as c_char; 256];
        // SAFETY: a valid path and buffer
        assert_eq!(unsafe { pe_scale_info(path.as_ptr(), info.as_mut_ptr(), info.len()) }, 19);
        assert!(unsafe { CStr::from_ptr(info.as_ptr()) }.to_string_lossy().starts_with("19 equal · 19 notes"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn temperament_names_match_the_table() {
        assert_eq!(pe_temperament_count() as usize, crate::temperament::NAMES.len());
        for (i, name) in crate::temperament::NAMES.iter().enumerate() {
            // SAFETY: static C strings from pe_temperament_name
            let c = unsafe { std::ffi::CStr::from_ptr(pe_temperament_name(i as u32)) };
            assert_eq!(c.to_str().unwrap(), *name);
        }
        assert!(pe_temperament_name(99).is_null());
    }

    #[test]
    fn stops_reach_the_engine() {
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/registers_fixture")).unwrap();
        // SAFETY: valid C string, no tuning, cache or error buffer
        let e = unsafe { pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0) };
        if e.is_null() {
            return; // no export
        }
        let p = PeParams {
            room: 1.0,
            knock: 1.0,
            release: 1.0,
            pedal: 1.0,
            pedal_ring: 1.0,
            sympathetic: 1.0,
            volume: 1.0,
            hardness: 1.0,
            hammer_wear: 0.0,
            voicing_wear: 0.0,
            damper_wear: 0.0,
            action_wear: 0.0,
            phantom: 0.0,
            grooves: 0.0,
            damper_buzz: 0.0,
            rattle: 0.0,
            stops: 3,
            damper_fall: 1.0,
            prep: 0,
            pluck_pos: 0.2,
            pluck_bright: 1.0,
            mute: 0.6,
            harmonic: 2,
            paper: 0.5,
            buff: 0,
            lute: 0,
            motor: 0,
            motor_rate: 5.0,
            motor_depth: 1.0,
            mallet: 0,
            bebung: 30.0,
            bend_range: 2.0,
            input: 0.0,
            dynamics: 45.0,
            variation: 0.5,
            restrike: 0.0,
        };
        // SAFETY: a live engine from pe_create, valid parameters; destroyed once
        unsafe {
            pe_set_params(e, &p);
            assert_eq!((*e).engine.params().stops, 3);
            pe_destroy(e);
        }
    }

    /// the curve (1,1),(64,100),(127,127) as the C struct
    fn steep_low() -> PeVelocityCurve {
        let mut c = PeVelocityCurve { n: 3, vin: [0; 8], vout: [0; 8] };
        c.vin[..3].copy_from_slice(&[1, 64, 127]);
        c.vout[..3].copy_from_slice(&[1, 100, 127]);
        c
    }

    /// 0.5 s of note 60 at `velocity` from a fresh engine (None: no export)
    fn render_note(curve: Option<&PeVelocityCurve>, velocity: u8) -> Option<Vec<f32>> {
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")).unwrap();
        // SAFETY: valid C string, no tuning, cache or error buffer
        let e = unsafe { pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0) };
        if e.is_null() {
            return None;
        }
        let (mut l, mut r) = (vec![0.0f32; 24000], vec![0.0f32; 24000]);
        // SAFETY: a live engine, buffers of the given length; destroyed once
        unsafe {
            if let Some(c) = curve {
                pe_set_velocity_curve(e, c);
            }
            pe_note_on(e, 0, 60, velocity);
            for k in (0..24000).step_by(256) {
                let n = 256.min(24000 - k);
                pe_process(e, std::ptr::null(), l[k..].as_mut_ptr(), r[k..].as_mut_ptr(), n as u32);
            }
            pe_destroy(e);
        }
        Some(l)
    }

    #[test]
    fn a_curve_set_through_the_c_api_reshapes_the_strike() {
        let (Some(curved), Some(straight)) = (render_note(Some(&steep_low()), 64), render_note(None, 100)) else { return };
        assert!(curved.iter().zip(&straight).all(|(a, b)| (a - b).abs() <= 1e-6));
        assert!(straight.iter().any(|x| x.abs() > 1e-3));
    }

    #[test]
    fn the_curve_text_crosses_the_c_api() {
        let mut c = PeVelocityCurve { n: 0, vin: [0; 8], vout: [0; 8] };
        let bad = std::ffi::CString::new("bad").unwrap();
        // SAFETY: valid C strings and out pointers
        unsafe {
            assert!(!pe_velocity_curve_parse(bad.as_ptr(), &mut c));
            assert_eq!((c.n, &c.vin[..2], &c.vout[..2]), (2, &[1u8, 127][..], &[1u8, 127][..]));
            let text = std::ffi::CString::new("1:1,64:100,127:127").unwrap();
            assert!(pe_velocity_curve_parse(text.as_ptr(), &mut c));
            let mut buf = [0 as c_char; 64];
            let n = pe_velocity_curve_format(&c, buf.as_mut_ptr(), buf.len());
            assert_eq!(std::ffi::CStr::from_ptr(buf.as_ptr()).to_str().unwrap(), "1:1,64:100,127:127");
            assert_eq!(n, 18);
            assert!((pe_velocity_curve_eval(&c, 64.0) - 100.0).abs() < 1e-4);
            let mut wrong = c;
            wrong.vin[1] = 200; // not a curve: evaluated straight
            assert!((pe_velocity_curve_eval(&wrong, 64.0) - 64.0).abs() < 1e-4);
        }
    }

    #[test]
    fn adopting_carries_the_curve() {
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/registers_fixture")).unwrap();
        // SAFETY: valid C string; two live engines, destroyed once each
        unsafe {
            let a = pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
            if a.is_null() {
                return;
            }
            let b = pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
            pe_set_velocity_curve(a, &steep_low());
            assert!(pe_adopt(b, a));
            assert_eq!((*b).curve, (*a).curve);
            assert!(!(*b).curve.is_straight());
            pe_destroy(a);
            pe_destroy(b);
        }
    }

    #[test]
    fn lute_reaches_the_engine() {
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/flemish")).unwrap();
        // SAFETY: valid C string, no tuning, cache or error buffer
        let e = unsafe { pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0) };
        if e.is_null() {
            return; // no export
        }
        let p = PeParams {
            room: 1.0, knock: 1.0, release: 1.0, pedal: 1.0, pedal_ring: 1.0, sympathetic: 1.0, volume: 1.0, hardness: 1.0,
            hammer_wear: 0.0, voicing_wear: 0.0, damper_wear: 0.0, action_wear: 0.0, phantom: 0.0,
            grooves: 0.0, damper_buzz: 0.0, rattle: 0.0, stops: 5, damper_fall: 1.0,
            prep: 0, pluck_pos: 0.2, pluck_bright: 1.0, mute: 0.6, harmonic: 2, paper: 0.5, buff: 0, lute: 1,
            motor: 0, motor_rate: 5.0, motor_depth: 1.0, mallet: 0,
            bebung: 30.0,
            bend_range: 2.0,
            input: 0.0,
            dynamics: 45.0,
            variation: 0.5,
            restrike: 0.0,
        };
        // SAFETY: a live engine from pe_create, valid parameters; destroyed once
        unsafe {
            pe_set_params(e, &p);
            assert!((*e).engine.params().lute);
            assert_eq!((*e).engine.params().stops, 5);
            pe_destroy(e);
        }
    }

    #[test]
    fn buff_reaches_the_engine() {
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/harpsichord")).unwrap();
        // SAFETY: valid C string, no tuning, cache or error buffer
        let e = unsafe { pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0) };
        if e.is_null() {
            return; // no export
        }
        let p = PeParams {
            room: 1.0,
            knock: 1.0,
            release: 1.0,
            pedal: 1.0,
            pedal_ring: 1.0,
            sympathetic: 1.0,
            volume: 1.0,
            hardness: 1.0,
            hammer_wear: 0.0,
            voicing_wear: 0.0,
            damper_wear: 0.0,
            action_wear: 0.0,
            phantom: 0.0,
            grooves: 0.0,
            damper_buzz: 0.0,
            rattle: 0.0,
            stops: 1,
            damper_fall: 1.0,
            prep: 0,
            pluck_pos: 0.2,
            pluck_bright: 1.0,
            mute: 0.6,
            harmonic: 2,
            paper: 0.5,
            buff: 1,
            lute: 0,
            motor: 0,
            motor_rate: 5.0,
            motor_depth: 1.0,
            mallet: 0,
            bebung: 30.0,
            bend_range: 2.0,
            input: 0.0,
            dynamics: 45.0,
            variation: 0.5,
            restrike: 0.0,
        };
        // SAFETY: a live engine from pe_create, valid parameters; destroyed once
        unsafe {
            pe_set_params(e, &p);
            assert!((*e).engine.params().buff);
            pe_destroy(e);
        }
    }

    #[test]
    fn mic_geometry_matches_the_placement() {
        let mut g = std::mem::MaybeUninit::<PeMicGeometry>::zeroed();
        assert!(!unsafe { pe_mic_geometry(BODIES.len() as u32, 1, 0.5, 0.0, 1.0, std::ptr::null(), g.as_mut_ptr()) });
        assert!(!unsafe { pe_mic_geometry(0, 6, 0.5, 0.0, 1.0, std::ptr::null(), g.as_mut_ptr()) });
        assert!(unsafe { pe_mic_geometry(0, 0, 0.5, 0.0, 1.0, std::ptr::null(), g.as_mut_ptr()) });
        assert_eq!(unsafe { g.assume_init_ref() }.has_pair, 0);
        for (body, b) in BODIES.iter().enumerate().map(|(i, &b)| (i as u32, b)) {
            for pos in 1..5u32 {
                assert!(unsafe { pe_mic_geometry(body, pos, 0.7, 1.3, 1.0, std::ptr::null(), g.as_mut_ptr()) });
                let g = unsafe { g.assume_init_ref() };
                let p = b.placement(MicPosition::from_index(pos), 0.7, 1.3, 1.0).unwrap();
                assert_eq!(g.has_pair, 1);
                assert_eq!(g.mics, p.mics);
                assert_eq!((g.distance, g.distance_min, g.distance_max), (p.distance, p.range.0, p.range.1));
                assert_eq!(g.height_drag, p.height as u32);
                let d = b.drawing(b.wall_of(MicPosition::from_index(pos)), 1.0);
                assert_eq!(g.case_top_count as usize, d.case_top.len());
                assert_eq!((g.front_axis as usize, g.outline_front_count as usize), (d.front_axis, d.outline_front.len()));
                assert_eq!(g.wall_y.is_nan(), d.wall_y.is_none());
            }
        }
    }

    #[test]
    fn custom_mic_geometry_and_clamp() {
        let mut s = PeMicSetup::default();
        s.pos = [[-1.0, -2.0, 0.5], [1.0, -2.0, 0.5]];
        let mut g = std::mem::MaybeUninit::<PeMicGeometry>::zeroed();
        assert!(unsafe { pe_mic_geometry(0, 5, 0.0, 0.0, 1.0, &s, g.as_mut_ptr()) });
        let g = unsafe { g.assume_init() };
        assert_eq!((g.custom, g.has_pair), (1, 1));
        assert_eq!(g.mics, [[-1.0, -2.0, 0.5], [1.0, -2.0, 0.5]]);
        // pattern and aim round-trip through the geometry
        let mut sa = s;
        sa.pattern = [0.5, 1.0];
        sa.yaw = [-45.0, 45.0];
        sa.pitch = [-10.0, 20.0];
        let mut ga = std::mem::MaybeUninit::<PeMicGeometry>::zeroed();
        assert!(unsafe { pe_mic_geometry(0, 5, 0.0, 0.0, 1.0, &sa, ga.as_mut_ptr()) });
        let ga = unsafe { ga.assume_init() };
        assert_eq!((ga.pattern, ga.yaw, ga.pitch), ([0.5, 1.0], [-45.0, 45.0], [-10.0, 20.0]));
        // an out-of-range position comes back clamped
        let mut sc = s;
        sc.pos[0][0] = 100.0;
        let mut gc = std::mem::MaybeUninit::<PeMicGeometry>::zeroed();
        assert!(unsafe { pe_mic_geometry(0, 5, 0.0, 0.0, 1.0, &sc, gc.as_mut_ptr()) });
        assert_eq!(unsafe { gc.assume_init() }.mics[0][0], 20.0);
        // a null setup is the default pair, still valid
        let mut g2 = std::mem::MaybeUninit::<PeMicGeometry>::zeroed();
        assert!(unsafe { pe_mic_geometry(1, 5, 0.0, 0.0, 1.0, std::ptr::null(), g2.as_mut_ptr()) });
        // the clamp reports a change, and agrees with the engine's
        let mut bad = PeMicSetup::default();
        bad.pos[0] = [100.0, f64::NAN, -9.0];
        assert!(unsafe { pe_mic_clamp(0, 1.0, &mut bad) });
        assert!(bad.pos[0].iter().all(|v| v.is_finite()) && bad.pos[0][0] == 20.0);
        assert!(!unsafe { pe_mic_clamp(0, 1.0, &mut bad) }, "already clamped");
        assert!(!unsafe { pe_mic_clamp(2, 1.0, &mut bad) });
        // a preset ignores the custom pair
        let mut gp = std::mem::MaybeUninit::<PeMicGeometry>::zeroed();
        assert!(unsafe { pe_mic_geometry(0, 1, 0.5, 0.0, 1.0, &s, gp.as_mut_ptr()) });
        assert_eq!(unsafe { gp.assume_init() }.custom, 0);
    }

    #[test]
    fn lid_reaches_the_geometry_and_the_clamp() {
        let call = |lid: f64| {
            let mut g = std::mem::MaybeUninit::<PeMicGeometry>::zeroed();
            assert!(unsafe { pe_mic_geometry(0, 3, 0.5, 0.0, lid, std::ptr::null(), g.as_mut_ptr()) });
            unsafe { g.assume_init() }
        };
        // Close 1: the pair under the lid; closed, it sits at the minimum height; the drawing's lid line is flat
        let (open, shut) = (call(1.0), call(0.0));
        assert!(shut.mics[0][2] < open.mics[0][2] + 1e-12 && (shut.mics[0][2] - 0.08).abs() < 1e-9, "{:?} {:?}", open.mics[0], shut.mics[0]);
        assert!(shut.lid_front[1][1] < open.lid_front[1][1]);
        // half the lid lies strictly between
        let half = call(0.5);
        assert!(shut.lid_front[1][1] < half.lid_front[1][1] && half.lid_front[1][1] < open.lid_front[1][1]);
        // a non-finite lid is the open lid
        let nan = call(f64::NAN);
        assert_eq!(nan.mics, open.mics);
        // the clamp depends on the lid: a point at 0.3 m under a closed lid is pulled down
        let mut s = PeMicSetup::default();
        s.pos[0] = [0.0, 0.0, 0.3];
        assert!(unsafe { pe_mic_clamp(0, 0.0, &mut s) } && s.pos[0][2] <= 0.1);
        let mut t = PeMicSetup::default();
        t.pos[0] = [0.0, 0.0, 0.3];
        unsafe { pe_mic_clamp(0, 1.0, &mut t) };
        // the open lid is 0.65 m above the point, so the height is not pulled down at all
        assert!(t.pos[0][2] > 0.1 && (t.pos[0][2] - 0.3).abs() < 1e-12, "{:?}", t.pos[0]);
    }

    fn zeroed_tuning() -> PeTuning {
        // SAFETY: PeTuning is plain numbers; all-zero is a valid value
        let mut t: PeTuning = unsafe { std::mem::zeroed() };
        t.a4_hz = 440.0;
        t
    }

    #[test]
    fn tuning_from_sanitizes_lid_and_pillow() {
        // non-finite → defaults (open lid, no pillow); out-of-range values clamp, a lid above open is taken off
        let t = PeTuning { lid: f64::NAN, pillow: f64::INFINITY, ..zeroed_tuning() };
        let e = tuning_from(&t);
        assert_eq!((e.lid, e.pillow), (1.0, 0.0));
        let t = PeTuning { lid: -2.0, pillow: 9.0, ..zeroed_tuning() };
        let e = tuning_from(&t);
        assert_eq!((e.lid, e.pillow), (0.0, 1.0));
        let t = PeTuning { lid: 3.0, ..zeroed_tuning() };
        assert_eq!(tuning_from(&t).lid, crate::mic::LID_OFF);
    }

    #[test]
    fn the_strike_variation_is_clamped() {
        let p = |v: f32| PeParams { variation: v, ..zeroed_params() };
        assert_eq!(params_from(&p(0.0)).variation, 0.0);
        assert_eq!(params_from(&p(2.0)).variation, 2.0);
        assert_eq!(params_from(&p(99.0)).variation, 4.0);
        assert_eq!(params_from(&p(f32::NAN)).variation, crate::engine::VARIATION_DEFAULT);
    }

    #[test]
    fn every_body_has_the_name_its_instrument_json_gives() {
        for (i, n) in BODY_NAMES.iter().enumerate() {
            assert_eq!(Body::from_name(Some(n.to_str().unwrap())), BODIES[i], "{n:?}");
            assert_eq!(unsafe { std::ffi::CStr::from_ptr(pe_body_name(i as u32)) }, *n);
        }
        assert!(pe_body_name(BODIES.len() as u32).is_null());
    }

    fn zeroed_params() -> PeParams {
        // SAFETY: PeParams is plain numbers; all-zero is a valid value
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn a_zeroed_board_bass_is_as_shipped() {
        assert_eq!(tuning_from(&zeroed_tuning()).board_bass_db, 0.0);
    }

    #[test]
    fn the_board_bass_is_sanitized() {
        assert_eq!(tuning_from(&PeTuning { board_bass_db: -40.0, ..zeroed_tuning() }).board_bass_db, -12.0);
        assert_eq!(tuning_from(&PeTuning { board_bass_db: f64::NAN, ..zeroed_tuning() }).board_bass_db, 0.0);
    }

    #[test]
    fn a_zeroed_sustain_is_as_fitted() {
        // zero-initialized (or non-finite): 1, the ring as fitted; the rest clamps to 0.25 … 8
        assert_eq!(tuning_from(&zeroed_tuning()).sustain, 1.0);
        assert_eq!(tuning_from(&PeTuning { sustain: f64::NAN, ..zeroed_tuning() }).sustain, 1.0);
        assert_eq!(tuning_from(&PeTuning { sustain: 2.5, ..zeroed_tuning() }).sustain, 2.5);
        assert_eq!(tuning_from(&PeTuning { sustain: 99.0, ..zeroed_tuning() }).sustain, 8.0);
    }

    #[test]
    fn mic_setup_sizes_are_the_abi() {
        assert_eq!(std::mem::size_of::<PeMicSetup>(), 13 * 8);
        assert_eq!(std::mem::size_of::<PeTuning>(), 272, "PeTuning");
        assert_eq!(std::mem::size_of::<PeMicGeometry>(), 1360, "PeMicGeometry");
    }

    /// a prepared region and an engine attached to it, telemetry left off (None: no export)
    fn engine_with_region(mem: &mut Vec<u64>) -> Option<(*mut PeEngine, *mut u8, usize)> {
        let size = pe_telemetry_region_size();
        *mem = vec![0u64; size / 8 + 2];
        let base = mem.as_mut_ptr().cast::<u8>();
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")).unwrap();
        // SAFETY: `mem` is the caller's and outlives the engine; valid C string
        unsafe {
            assert!(pe_telemetry_init(base, size));
            let e = pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
            if e.is_null() {
                return None;
            }
            assert!(pe_telemetry_attach(e, base, size, 48000.0));
            Some((e, base, size))
        }
    }

    #[test]
    fn incoming_velocities_reach_the_reader() {
        let mut mem = Vec::new();
        let Some((e, base, size)) = engine_with_region(&mut mem) else { return };
        let mut out = [0u8; 16];
        // SAFETY: a live engine attached to `mem`'s region; destroyed once
        unsafe {
            let mut cursor = pe_telemetry_notes_head(base, size);
            pe_note_on(e, 0, 60, 20);
            pe_note_on(e, 0, 62, 64);
            pe_note_on(e, 0, 64, 127);
            pe_note_on(e, 0, 60, 0); // a key-up: no note
            pe_note_off(e, 0, 62);
            assert_eq!(pe_telemetry_read_notes(base, size, &mut cursor, out.as_mut_ptr(), 16), 3);
            assert_eq!(&out[..3], &[20, 64, 127]);
            assert_eq!(pe_telemetry_read_notes(base, size, &mut cursor, out.as_mut_ptr(), 16), 0);
            assert_eq!(pe_telemetry_read_notes(base, size, std::ptr::null_mut(), out.as_mut_ptr(), 16), 0);
            assert_eq!(pe_telemetry_read_notes(base, size, &mut cursor, std::ptr::null_mut(), 16), 0);
            pe_destroy(e);
        }
    }

    #[test]
    fn incoming_velocities_survive_an_overrun() {
        let mut mem = Vec::new();
        let Some((e, base, size)) = engine_with_region(&mut mem) else { return };
        let mut out = [0u8; 128];
        // SAFETY: as above
        unsafe {
            let mut cursor = pe_telemetry_notes_head(base, size);
            for i in 0..100u8 {
                pe_note_on(e, 0, 21 + (i % 80), 1 + i);
            }
            // the newest NOTE_SLOTS − 1 (the oldest slot may be under the next write)
            let n = pe_telemetry_read_notes(base, size, &mut cursor, out.as_mut_ptr(), 128) as usize;
            assert_eq!(n, telemetry::NOTE_SLOTS - 1);
            assert_eq!(out[n - 1], 100);
            assert_eq!(out[0], 100 - n as u8 + 1);
            let mut late = pe_telemetry_notes_head(base, size);
            assert_eq!(pe_telemetry_read_notes(base, size, &mut late, out.as_mut_ptr(), 128), 0);
            pe_destroy(e);
        }
    }

    /// the diagnostics C ABI end to end, on an aligned buffer standing in for the mapping
    #[test]
    fn telemetry_ffi_round_trip() {
        use std::ptr::null_mut;
        let size = pe_telemetry_region_size();
        assert_eq!(size, telemetry::region_size());
        let mut mem = vec![0u64; size / 8 + 2];
        let base = mem.as_mut_ptr().cast::<u8>();
        let mut f = Frame::boxed();
        let mut cursor = 0u64;
        let mut audio = vec![0.0f32; 2 * 4096];
        // SAFETY: `mem` outlives every call; pointers are valid except where a test passes null/misaligned on purpose
        unsafe {
            assert!(!pe_telemetry_init(null_mut(), size));
            assert!(!pe_telemetry_init(base, size - 1)); // undersized
            assert!(!pe_telemetry_init(base.add(1), size)); // misaligned
            assert!(!pe_telemetry_set_enabled(base, size, true)); // not prepared yet
            assert_eq!((pe_telemetry_owner(base, size), pe_telemetry_audio_head(base, size), pe_telemetry_dropped(base, size)), (0, 0, 0));
            assert!(!pe_telemetry_set_owner(base, size, 7));
            assert!(pe_telemetry_init(base, size));
            assert!(pe_telemetry_set_enabled(base, size, true));
            assert!(Region::open(base, size).unwrap().enabled());
            assert!(pe_telemetry_set_enabled(base, size, false));
            assert!(!Region::open(base, size).unwrap().enabled());
            assert!(!pe_telemetry_set_enabled(base, size - 8, true)); // an undersized view is refused
            assert!(pe_telemetry_set_owner(base, size, 4242));
            assert_eq!(pe_telemetry_owner(base, size), 4242);
            // the value mirror: slot 3 set twice, an out-of-range slot and null outputs refused
            let (mut seq, mut value) = (9u32, 9.0f32);
            assert!(pe_telemetry_read_value(base, size, 3, &mut seq, &mut value) && (seq, value) == (0, 0.0));
            assert!(pe_telemetry_set_value(base, size, 3, 0.75) && pe_telemetry_set_value(base, size, 3, 1.5));
            assert!(pe_telemetry_read_value(base, size, 3, &mut seq, &mut value) && (seq, value) == (2, 1.5));
            assert!(!pe_telemetry_set_value(base, size, 128, 1.0));
            assert!(!pe_telemetry_read_value(base, size, 128, &mut seq, &mut value));
            assert!(!pe_telemetry_read_value(base, size, 3, null_mut(), &mut value));
            assert!(!pe_telemetry_read_value(base, size, 3, &mut seq, null_mut()));
            assert!(!pe_telemetry_set_value(null_mut(), size, 3, 1.0));
            // nothing yet; null and misaligned out/cursor are refused, not dereferenced
            assert!(!pe_telemetry_read_frame(base, size, &mut *f));
            assert!(!pe_telemetry_read_frame(base, size, null_mut()));
            assert!(!pe_telemetry_read_frame(base, size, (&raw mut *f).byte_add(4)));
            assert_eq!(pe_telemetry_read_audio(base, size, &mut cursor, audio.as_mut_ptr(), 100), 0);
            assert_eq!(pe_telemetry_read_audio(base, size, null_mut(), audio.as_mut_ptr(), 100), 0);
            assert_eq!(pe_telemetry_read_audio(base, size, &mut cursor, null_mut(), 100), 0);
            assert_eq!(pe_telemetry_read_audio(base, size, (&raw mut cursor).byte_add(4), audio.as_mut_ptr(), 100), 0);
            // the writer side through the Rust API: audio, head, drops
            let r = Region::open(base, size).unwrap();
            r.push_audio(&[[0.25f32, -0.25]; 50]);
            assert_eq!(pe_telemetry_audio_head(base, size), 50);
            assert_eq!(pe_telemetry_read_audio(base, size, &mut cursor, audio.as_mut_ptr(), 100), 50);
            assert_eq!((audio[0], audio[1], cursor), (0.25, -0.25, 50));
            r.count_drop();
            assert_eq!(pe_telemetry_dropped(base, size), 1);
        }
        // with an engine: attach, a note, frames, audio, pe_diag, detach
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a")).unwrap();
        // SAFETY: valid C string, no tuning, cache or error buffer
        let e = unsafe { pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), null_mut(), null_mut(), 0) };
        if e.is_null() {
            return; // no export
        }
        // SAFETY: a live engine from pe_create, destroyed once; the region outlives the attachment
        unsafe {
            assert!(!pe_telemetry_attach(e, base.add(8), size - 8, 48000.0)); // not a region
            assert!(pe_telemetry_attach(e, base, size, 48000.0));
            assert!(pe_telemetry_set_enabled(base, size, true));
            let mut d = std::mem::MaybeUninit::<PeDiag>::zeroed().assume_init();
            pe_diag(e, &mut d);
            assert_eq!((d.keys, d.voices, d.pedal), ([0; telemetry::KEY_WORDS], 0, 0.0));
            pe_note_on(e, 0, 69, 100);
            pe_pedal_depth(e, 0, 1.0);
            let head = pe_telemetry_audio_head(base, size);
            let (mut l, mut rr) = (vec![0.0f32; 512], vec![0.0f32; 512]);
            for _ in 0..40 {
                pe_process(e, std::ptr::null(), l.as_mut_ptr(), rr.as_mut_ptr(), 512);
            }
            pe_diag(e, &mut d);
            assert_eq!(d.keys[48 / 12] >> (2 * (48 % 12)) & 0b11, 0b11, "A4 held and ringing");
            assert!(d.voices >= 1 && d.pedal > 0.99);
            assert_eq!(d.sostenuto, 0.0);
            assert_eq!(d.soft, 0.0);
            pe_soft_pedal(e, 0, 0.5);
            pe_process(e, std::ptr::null(), l.as_mut_ptr(), rr.as_mut_ptr(), 512);
            pe_diag(e, &mut d);
            assert_eq!(d.soft, 0.5);
            pe_soft_pedal(e, 0, 0.0);
            pe_sostenuto(e, 0, true);
            pe_process(e, std::ptr::null(), l.as_mut_ptr(), rr.as_mut_ptr(), 512);
            pe_diag(e, &mut d);
            assert_eq!(d.sostenuto, 1.0);
            pe_sostenuto(e, 0, false);
            pe_process(e, std::ptr::null(), l.as_mut_ptr(), rr.as_mut_ptr(), 512);
            pe_diag(e, &mut d);
            assert_eq!(d.sostenuto, 0.0);
            // with a frame and audio pending, bad pointers are refused and nothing is consumed
            assert!(!pe_telemetry_read_frame(base, size, null_mut()));
            assert!(!pe_telemetry_read_frame(base, size, (&raw mut *f).byte_add(4)));
            assert_eq!(pe_telemetry_read_audio(base, size, null_mut(), audio.as_mut_ptr(), 4096), 0);
            assert_eq!(pe_telemetry_read_audio(base, size, &mut cursor, null_mut(), 4096), 0);
            assert_eq!(pe_telemetry_read_audio(base, size, (&raw mut cursor).byte_add(4), audio.as_mut_ptr(), 4096), 0);
            assert!(pe_telemetry_read_frame(base, size, &mut *f));
            assert!(f.voices >= 1 && f.rate == 48000.0);
            assert!(pe_telemetry_audio_head(base, size) > head);
            let n = pe_telemetry_read_audio(base, size, &mut cursor, audio.as_mut_ptr(), 4096);
            assert!(n > 0);
            assert!(pe_telemetry_dropped(base, size) >= 1); // nobody read during the 40 x 512 blocks: the 8-slot ring filled and the skipped frames were counted
            // detach (null base) stops the writing
            assert!(pe_telemetry_attach(e, null_mut(), 0, 48000.0));
            let _ = pe_telemetry_read_frame(base, size, &mut *f);
            let head = pe_telemetry_audio_head(base, size);
            for _ in 0..40 {
                pe_process(e, std::ptr::null(), l.as_mut_ptr(), rr.as_mut_ptr(), 512);
            }
            assert_eq!(pe_telemetry_audio_head(base, size), head);
            assert!(!pe_telemetry_read_frame(base, size, &mut *f));
            pe_destroy(e);
        }
    }

    #[test]
    fn telemetry_frame_matches_the_header() {
        assert_eq!(std::mem::size_of::<telemetry::Frame>(), 44224); // plugin/src/dussek.h: PeTelemetryFrame
        assert_eq!(std::mem::size_of::<PeDiag>(), 4 * 8 + 4 + 4 + 4 + 4);
    }
}

#[cfg(test)]
mod info_tests {
    use super::*;
    use std::ffi::CString;

    fn call(dir: &str) -> (bool, PeInstrumentInfo, String) {
        let d = CString::new(dir).unwrap();
        let mut out = PeInstrumentInfo::default();
        let mut err = [0 as c_char; 256];
        // SAFETY: a valid C string, `out` and `err` writable
        let ok = unsafe { pe_instrument_info(d.as_ptr(), &mut out, err.as_mut_ptr(), err.len()) };
        // SAFETY: NUL-terminated by pe_instrument_info (or zeroed)
        let e = unsafe { CStr::from_ptr(err.as_ptr()) }.to_string_lossy().into_owned();
        (ok, out, e)
    }

    fn text(buf: &[c_char]) -> String {
        // SAFETY: the fields are NUL-terminated
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap().to_owned()
    }

    #[test]
    fn a_committed_instrument() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/marimba");
        if !std::path::Path::new(dir).exists() {
            return;
        }
        let (ok, i, _) = call(dir);
        assert!(ok && i.mallet_count == 3 && i.bars && !i.lid && !i.dampers);
        // "yarn" from the file before the menu block, "Yarn" from it after
        assert!(text(&i.mallets[0]).eq_ignore_ascii_case("yarn"));
        assert_eq!(i.body, crate::mic::Body::Marimba.index());
        assert_eq!(i.preparations, 1 << 3 | 1 << 4);
    }

    #[test]
    fn a_missing_directory_is_an_error() {
        let (ok, _, e) = call("/nonexistent/pe-instrument");
        assert!(!ok && e.contains("instrument.json"));
    }

    #[test]
    fn long_strings_truncate_on_a_char_boundary() {
        let mut buf = [0 as c_char; 8];
        copy_str(&mut buf, "aaaaaaé"); // 'é' is 2 bytes: 6 + 2 > 7 usable
        assert_eq!(text(&buf), "aaaaaa");
        copy_str(&mut buf, "abc");
        assert_eq!(text(&buf), "abc");
    }
}

#[cfg(test)]
mod morph_tests {
    use super::*;
    use std::ffi::CString;

    const A: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
    const E: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_d");
    const C: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_f");

    /// an engine for the morph of `sides` through `cache` (null: none)
    fn create(sides: &[(&str, f32)], cache: *mut PeCache) -> *mut PeEngine {
        let dirs: Vec<CString> = sides.iter().map(|s| CString::new(s.0).unwrap()).collect();
        let ptrs: Vec<*const c_char> = dirs.iter().map(|d| d.as_ptr()).collect();
        let w: Vec<f32> = sides.iter().map(|s| s.1).collect();
        // SAFETY: valid arrays of `sides.len()`, no tuning or error buffer
        unsafe { pe_create_morph(ptrs.as_ptr(), w.as_ptr(), sides.len() as u32, 48000.0, std::ptr::null(), std::ptr::null(), cache, std::ptr::null_mut(), 0) }
    }

    /// key 60 at velocity 100, 40 blocks of 256 (left and right appended); the engine freed
    fn render(e: *mut PeEngine) -> Vec<f32> {
        let (mut l, mut r, mut out) = (vec![0.0f32; 256], vec![0.0f32; 256], Vec::new());
        // SAFETY: a live engine from pe_create/pe_create_morph, buffers of 256 frames
        unsafe {
            pe_note_on(e, 0, 60, 100);
            for _ in 0..40 {
                pe_process(e, std::ptr::null(), l.as_mut_ptr(), r.as_mut_ptr(), 256);
                out.extend_from_slice(&l);
                out.extend_from_slice(&r);
            }
            pe_destroy(e);
        }
        out
    }

    #[test]
    fn one_entry_is_pe_create() {
        if !std::path::Path::new(A).exists() {
            return;
        }
        let dir = CString::new(A).unwrap();
        // SAFETY: valid C string, no tuning, cache or error buffer
        let plain = unsafe { pe_create(dir.as_ptr(), 48000.0, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0) };
        assert_eq!(render(create(&[(A, 1.0)], std::ptr::null_mut())), render(plain));
    }

    #[test]
    fn a_recent_recipe_is_not_morphed_again() {
        if !std::path::Path::new(A).exists() {
            return;
        }
        let cache = pe_cache_new();
        for sides in [[(A, 0.5), (E, 0.5)], [(A, 0.7), (E, 0.3)], [(A, 0.5), (E, 0.5)]] {
            // SAFETY: an engine just created
            unsafe { pe_destroy(create(&sides, cache)) };
        }
        // another tuning of a recent recipe: no new morph
        // SAFETY: numbers and integers: all-zero is a valid PeTuning (tuning_from clamps it)
        let mut t: PeTuning = unsafe { std::mem::zeroed() };
        t.a4_hz = 442.0;
        let dirs = [CString::new(A).unwrap(), CString::new(E).unwrap()];
        let ptrs = [dirs[0].as_ptr(), dirs[1].as_ptr()];
        // SAFETY: valid arrays of two, a valid tuning, a live cache
        unsafe { pe_destroy(pe_create_morph(ptrs.as_ptr(), [0.5f32, 0.5].as_ptr(), 2, 48000.0, &t, std::ptr::null(), cache, std::ptr::null_mut(), 0)) };
        // SAFETY: from pe_cache_new
        assert_eq!(unsafe { (*cache).built() }, 2);
        unsafe { pe_cache_free(cache) };
    }

    #[test]
    fn plain_instruments_take_one_place() {
        // browsing the instruments keeps one loaded (as before the morph), the morphs keep theirs
        if !std::path::Path::new(A).exists() {
            return;
        }
        let cache = pe_cache_new();
        for sides in [&[(A, 1.0)][..], &[(C, 1.0)], &[(E, 1.0)], &[(A, 0.5), (E, 0.5)], &[(A, 1.0)], &[(A, 0.5), (E, 0.5)]] {
            // SAFETY: an engine just created
            unsafe { pe_destroy(create(sides, cache)) };
        }
        // SAFETY: from pe_cache_new
        assert_eq!(unsafe { (*cache).built() }, 5); // A again loads (C, then E took its place); the morph is kept
        unsafe { pe_cache_free(cache) };
    }

    #[test]
    fn the_cache_keeps_four() {
        if !std::path::Path::new(A).exists() {
            return;
        }
        let cache = pe_cache_new();
        for sides in [[(A, 0.5), (E, 0.5)], [(A, 0.6), (E, 0.4)], [(A, 0.7), (E, 0.3)], [(A, 0.5), (C, 0.5)], [(A, 0.6), (C, 0.4)], [(A, 0.5), (E, 0.5)]] {
            // SAFETY: an engine just created
            unsafe { pe_destroy(create(&sides, cache)) };
        }
        // SAFETY: from pe_cache_new
        assert_eq!(unsafe { (*cache).built() }, 6);
        unsafe { pe_cache_free(cache) };
    }

    const HARPSICHORD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/flemish");

    fn felt_of(text: &str) -> PeFelt {
        let t = CString::new(text).unwrap();
        let mut f = PeFelt::default();
        // SAFETY: a C string, a struct to fill
        assert!(unsafe { pe_felt_parse(t.as_ptr(), &mut f) });
        f
    }

    #[test]
    fn felt_text_round_trips_through_c() {
        let text = "s:21=0,72=-3;p:36=0.25;c:21=6,39=6,48=0;d:";
        let f = felt_of(text);
        let mut buf = [0 as c_char; 256];
        // SAFETY: a valid felt, a buffer of its length
        let n = unsafe { pe_felt_format(&f, buf.as_mut_ptr(), buf.len()) };
        // SAFETY: NUL-terminated by pe_felt_format
        assert_eq!(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap(), text);
        assert_eq!(n, text.len());
        let bad = CString::new("s:60=1,40=2").unwrap();
        let mut g = felt_of(text);
        // SAFETY: as above
        assert!(!unsafe { pe_felt_parse(bad.as_ptr(), &mut g) });
        assert_eq!(g, PeFelt::default());
    }

    /// the shipped felt of `sides` through the C API
    fn shipped(sides: &[(&str, f32)]) -> PeFeltKeys {
        let dirs: Vec<CString> = sides.iter().map(|s| CString::new(s.0).unwrap()).collect();
        let ptrs: Vec<*const c_char> = dirs.iter().map(|d| d.as_ptr()).collect();
        let w: Vec<f32> = sides.iter().map(|s| s.1).collect();
        let mut out = PeFeltKeys::default();
        // SAFETY: arrays of `sides.len()`, a struct to fill
        assert!(unsafe { pe_felt_shipped(ptrs.as_ptr(), w.as_ptr(), sides.len() as u32, &mut out, std::ptr::null_mut(), 0) });
        out
    }

    #[test]
    fn shipped_felt_of_a_grand_has_hammers_and_its_core() {
        if !std::path::Path::new(C).exists() {
            return;
        }
        let k = shipped(&[(C, 1.0)]);
        assert!(k.hammer.iter().all(|&h| h == 1));
        assert!(k.core[0] == 1 && k.core[60 - 21] == 0 && k.f05[0] > 1.0 && k.core_depth_mm[60 - 21] == 1.0);
        let h = shipped(&[(HARPSICHORD, 1.0)]);
        assert!(h.hammer.iter().all(|&h| h == 0));
    }

    #[test]
    fn apply_through_c_matches_rust() {
        if !std::path::Path::new(C).exists() {
            return;
        }
        let k = shipped(&[(C, 1.0)]);
        let mut f = felt_of("s:21=6,60=0;p:21=0.2;c:60=3;d:21=-0.2");
        f.hardness_db = 2.0;
        let mut out = PeFeltKeys::default();
        // SAFETY: valid structs
        unsafe { pe_felt_apply(&k, &f, &mut out) };
        let want = crate::felt::apply(&keys_from(&k), &edit_from(&f));
        for i in 0..88 {
            assert!((out.f05[i] as f64 - want[i].f05).abs() <= 1e-5 * want[i].f05.abs() + 1e-9, "key {}", i + 21);
            assert!((out.p[i] as f64 - want[i].p).abs() < 1e-5 && (out.core05[i] as f64 - want[i].core05).abs() < 1e-4);
            assert!((out.core_depth_mm[i] as f64 - want[i].core_depth_mm).abs() < 1e-5 && (out.core[i] == 1) == want[i].core);
        }
    }

    #[test]
    fn null_felt_builds_the_shipped_engine() {
        if !std::path::Path::new(C).exists() {
            return;
        }
        let dir = CString::new(C).unwrap();
        let empty = PeFelt::default();
        // SAFETY: one directory, no tuning, a cache-less build
        let (a, b) = unsafe {
            (
                pe_create_morph(&dir.as_ptr(), &1.0, 1, 48000.0, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 0),
                pe_create_morph(&dir.as_ptr(), &1.0, 1, 48000.0, std::ptr::null(), &empty, std::ptr::null_mut(), std::ptr::null_mut(), 0),
            )
        };
        assert_eq!(render(a), render(b));
    }

    #[test]
    fn hammer_piano_is_reported() {
        if !std::path::Path::new(C).exists() || !std::path::Path::new(HARPSICHORD).exists() {
            return;
        }
        for (d, want) in [(C, true), (HARPSICHORD, false)] {
            let dir = CString::new(d).unwrap();
            let mut info: PeInstrumentInfo = unsafe { std::mem::zeroed() };
            // SAFETY: a C string, a struct to fill, no error buffer
            assert!(unsafe { pe_instrument_info(dir.as_ptr(), &mut info, std::ptr::null_mut(), 0) });
            let e = create(&[(d, 1.0)], std::ptr::null_mut());
            // SAFETY: a live engine, freed after
            assert_eq!((info.hammer_piano, unsafe { pe_hammer_piano(e) }), (want, want), "{d}");
            unsafe { pe_destroy(e) };
        }
    }

    #[test]
    fn a_felt_edit_is_measured_once() {
        if !std::path::Path::new(C).exists() {
            return;
        }
        let mut cache = PeCache::default();
        let sides = [(C.to_string(), 1.0)];
        let e = crate::felt::FeltEdit { hardness_db: 6.0, ..Default::default() };
        // the shipped felt: nothing measured
        cache.instrument_with(&sides, None).unwrap();
        cache.instrument_with(&sides, Some(&crate::felt::FeltEdit::default())).unwrap();
        assert_eq!(cache.measured, 0);
        // an edit: the shipped rows and the edited instrument's, once
        cache.instrument_with(&sides, Some(&e)).unwrap();
        assert_eq!(cache.measured, 2);
        // back to the shipped felt and to the edit again (the Original toggle): kept
        cache.instrument_with(&sides, None).unwrap();
        cache.instrument_with(&sides, Some(&e)).unwrap();
        assert_eq!(cache.measured, 2);
        // another edit: its own rows only
        cache.instrument_with(&sides, Some(&crate::felt::FeltEdit { hardness_db: -6.0, ..Default::default() })).unwrap();
        assert_eq!(cache.measured, 3);
    }

    #[test]
    fn a_quick_build_takes_the_felts_leveling_as_last_measured() {
        const E: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_e");
        if !std::path::Path::new(C).exists() || !std::path::Path::new(E).exists() {
            return;
        }
        let mut cache = PeCache::default();
        let e = crate::felt::FeltEdit { hardness_db: 6.0, ..Default::default() };
        let at = |w: f64| [(C.to_string(), 1.0 - w), (E.to_string(), w)];
        // quick, but nothing measured for this felt yet: measured
        cache.quick = true;
        cache.instrument_with(&at(0.3), Some(&e)).unwrap();
        assert!(!cache.provisional && cache.measured == 2);
        // the pad dragged: each point at once, on the leveling as measured
        let quick = cache.instrument_with(&at(0.4), Some(&e)).unwrap();
        cache.instrument_with(&at(0.5), Some(&e)).unwrap();
        assert!(cache.provisional && cache.measured == 2);
        // let go: the last point measured, its own leveling
        cache.quick = false;
        let exact = cache.instrument_with(&at(0.4), Some(&e)).unwrap();
        assert!(!cache.provisional && cache.measured == 4);
        // the stand-in is near: the levels within a few dB, the felt the same
        let d = exact.notes.iter().zip(&quick.notes).filter_map(|(a, b)| Some((a.level_db.first()?[1] - b.level_db.first()?[1]).abs())).fold(0.0, f64::max);
        assert!(d < 3.0, "{d} dB");
        assert_eq!(exact.notes[40].felt_k, quick.notes[40].felt_k);
    }
}
