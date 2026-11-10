//! A brass export's tables and what the players read (`proto/export_brass.py`, the `brass` block of
//! instrument.json), looked up by key without allocating.

use super::streams::Streams;
use crate::instrument::{ArrRef, Data, Error, err};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const DYNS: [&str; 3] = ["pp", "mf", "ff"];
const KEYS: usize = 128;

#[derive(Clone, Deserialize)]
struct RawComplex {
    re: ArrRef,
    im: ArrRef,
}

#[derive(Clone, Deserialize)]
struct RawModes {
    sp: RawComplex,
    rc: RawComplex,
}

#[derive(Clone, Deserialize)]
struct RawNote {
    f: f64,
    settings: HashMap<String, [f64; 2]>,
    modes: HashMap<String, RawModes>,
    h: HashMap<String, ArrRef>,
    r: ArrRef,
    #[serde(default)]
    open: Option<HashMap<String, ArrRef>>,
}

#[derive(Clone, Deserialize)]
struct RawMute {
    notes: HashMap<String, RawNote>,
}

/// An export's `pose`: degrees from the player's front toward the right and above level, the player's ears from the
/// bell's mouth in m (x right, y front, z up).
#[derive(Clone, Deserialize)]
struct RawPose {
    azimuth: f64,
    elevation: f64,
    ears: [f64; 3],
}

fn pose_of(raw: Option<&RawPose>, radius: f64) -> crate::mic::BellPose {
    let d = crate::mic::BellPose::DEFAULT;
    let mut pose = raw.map_or(d, |p| crate::mic::BellPose {
        azimuth: p.azimuth.round() as i16,
        elevation: p.elevation.round() as i16,
        ears: p.ears.map(|x| (x * 100.0).round() as i16),
        radius_mm: d.radius_mm,
    });
    pose.radius_mm = (radius * 1000.0).round().clamp(1.0, 2000.0) as u16;
    pose
}

#[derive(Clone, Deserialize)]
struct RawLips {
    ratio: Vec<f64>,
    q: Vec<f64>,
    sign: Vec<f64>,
    comp: Vec<f64>,
}

fn yes() -> bool {
    true
}

/// The `brass` block as written.
#[derive(Clone, Deserialize)]
pub(crate) struct RawBrass {
    table: Vec<u8>,
    range: [u8; 2],
    fitted: [u8; 2],
    slide: bool,
    gain_db: f64,
    width: f64,
    y0: f64,
    soft: f64,
    rho: f64,
    nl: f64,
    mu: f64,
    mu_f: f64,
    mu_p: f64,
    center: f64,
    #[serde(default)]
    vel_air: Option<[Vec<f64>; 2]>,
    #[serde(default)]
    scoop_span: Option<f64>,
    /// the attack's pressure rising over this many periods of the note (at least 8 ms; 0: 8 ms)
    #[serde(default)]
    attack_rise_periods: f64,
    /// over this frequency the rise blends back to 8 ms within an octave (0: no knee)
    #[serde(default)]
    attack_rise_hz: f64,
    /// a strong attack starting under the note's pressure (`compare5.attack_for`'s lean-in)
    #[serde(default = "yes")]
    attack_lean: bool,
    /// a tongued note on a release starts from rest: the bore's and the lips' state cleared
    #[serde(default)]
    attack_from_rest: bool,
    /// the attack's slow rise starting from this share of its pressure, not from none (0 … 1; blended out over the knee's
    /// octave as the rise is)
    #[serde(default)]
    attack_rise_from: f64,
    #[serde(default)]
    noise: Option<Vec<f64>>,
    bell_radius: f64,
    /// where the bell points and where the player's ears are (optional: `BellPose::DEFAULT`)
    #[serde(default)]
    pose: Option<RawPose>,
    room_feed: f64,
    lips: RawLips,
    dec: Vec<f64>,
    makeup_db: HashMap<String, f64>,
    /// the mutes in the project's order, "open" first
    order: Vec<String>,
    speak: HashMap<String, f64>,
    level: HashMap<String, HashMap<String, serde_json::Value>>,
    cells: HashMap<String, [f64; 2]>,
    at: HashMap<String, (Vec<f64>, Vec<f64>, f64)>,
    mutes: HashMap<String, RawMute>,
}

/// The attack's rise to its pressure, s: `periods` periods of the note at least 8 ms, blended back to 8 ms over the octave above
/// `knee` (`live.attack_rise`). A low note rising fast shocks the bore into ringing louder than it then holds; a middle one
/// rising slowly speaks late.
pub fn attack_rise(periods: f64, knee: f64, f: f64) -> f64 {
    const FAST: f64 = 0.008;
    if periods <= 0.0 {
        return FAST;
    }
    let slow = (periods / f).max(FAST);
    if knee <= 0.0 || f <= knee {
        return slow;
    }
    let w = (f / knee).log2().min(1.0);
    (slow.ln() * (1.0 - w) + FAST.ln() * w).exp()
}

/// The share of its top the attack's rise starts from: `share` under `knee`, blended out over the octave above it, as the
/// rise is blended back to 8 ms (`live.attack_share`). The share takes the slow rise's wait under the lips' threshold
/// away; an 8 ms rise has none to take, and a step of air would start it. No slow rise (`periods` 0): none.
pub fn attack_share(share: f64, periods: f64, knee: f64, f: f64) -> f64 {
    if periods <= 0.0 {
        return 0.0;
    }
    if knee <= 0.0 || f <= knee {
        return share;
    }
    share * (1.0 - (f / knee).log2().min(1.0))
}

impl RawBrass {
    /// What the report says of it (`info`), from instrument.json alone.
    pub(crate) fn facts(&self) -> crate::info::BrassInfo {
        crate::info::BrassInfo {
            low: self.range[0],
            high: self.range[1],
            mutes: self.order.clone(),
            plunger: self.order.iter().any(|m| m == "plunger"),
            slide: self.slide,
            experimental: false,
            pose: pose_of(self.pose.as_ref(), self.bell_radius),
        }
    }
}

/// One key's modes at one dynamics: the poles and their residues.
#[derive(Clone)]
pub struct Modes {
    pub sp_re: Box<[f64]>,
    pub sp_im: Box<[f64]>,
    pub rc_re: Box<[f64]>,
    pub rc_im: Box<[f64]>,
}

impl Modes {
    pub fn len(&self) -> usize {
        self.sp_re.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sp_re.is_empty()
    }
}

/// One key under one mute.
#[derive(Clone)]
pub struct Note {
    pub f: f64,
    /// per dynamics the slide's lip offset and the air's factor (`muted.setting`)
    pub settings: [(f64, f64); 3],
    pub modes: [Modes; 3],
    /// what leaves the bell, per dynamics (the plunger's: shut)
    pub h: [Arc<[f32]>; 3],
    /// the impedance the modes leave out
    pub r: Arc<[f32]>,
    /// the plunger's open bell
    pub open: Option<[Arc<[f32]>; 3]>,
}

/// The lips' two modes (`fit4.lips`).
#[derive(Clone, Default)]
pub struct Lips {
    pub ratio: Vec<f64>,
    pub q: Vec<f64>,
    pub sign: Vec<f64>,
    pub comp: Vec<f64>,
}

/// A table key's level: the output's dB at the air's four anchors, and each dynamics' own step.
#[derive(Clone, Copy, Default)]
pub struct Level {
    pub anchors: [f64; 4],
    pub step: [f64; 3],
}

/// A table key's air and lips over the dynamics (`rt_tables.levels_at`).
#[derive(Clone, Copy, Default)]
pub struct At {
    pub lp: [f64; 3],
    pub ce: [f64; 3],
    pub top: f64,
}

pub struct Tables {
    /// the mutes in the export's order, "open" first
    pub mutes: Vec<String>,
    /// the keys with tables, rising
    pub table: Vec<u8>,
    /// the lowest and highest key that plays
    pub range: [u8; 2],
    /// the lowest and highest key fitted
    pub fitted: [u8; 2],
    pub slide: bool,
    pub gain_db: f64,
    pub width: f64,
    pub y0: f64,
    pub soft: f64,
    pub rho: f64,
    pub nl: f64,
    pub mu: f64,
    pub mu_f: f64,
    pub mu_p: f64,
    pub center: f64,
    pub vel_air: Option<[Vec<f64>; 2]>,
    pub scoop_span: Option<f64>,
    /// the attack's pressure rising over this many periods of the note (at least 8 ms; 0: 8 ms)
    pub attack_rise_periods: f64,
    /// over this frequency the rise blends back to 8 ms within an octave (0: no knee)
    pub attack_rise_hz: f64,
    /// a strong attack starting under the note's pressure
    pub attack_lean: bool,
    /// a tongued note on a release starts from rest
    pub attack_from_rest: bool,
    /// the attack's rise starting from this share of its pressure
    pub attack_rise_from: f64,
    pub noise: Option<Vec<f64>>,
    pub bell_radius: f64,
    /// where the bell points as played, its radius the bell's
    pub pose: crate::mic::BellPose,
    /// the direct sound's level over what the room's filter leaves of it, on a played note
    pub room_feed: f64,
    pub lips: Lips,
    /// the anti-alias filter from the oversampled rate
    pub dec: Vec<f64>,
    /// per mute, dB
    pub makeup_db: Vec<f64>,
    /// the export's level is there
    pub has_level: bool,
    notes: Vec<Vec<Note>>,
    /// per key the place of the nearest table key (the lower of two as near)
    nearest: [u8; KEYS],
    speak_air: Box<[[f64; 3]; KEYS]>,
    speak_first: Box<[[f64; 3]; KEYS]>,
    /// per mute, key and dynamics: the live repairs
    cells: Vec<Box<[[Option<(f64, f64)>; 3]; KEYS]>>,
    /// per mute and table place
    level: Vec<Vec<Level>>,
    at: Vec<At>,
    /// the breath and the wander, made once when first asked for (a build's, never the audio thread's)
    streams: OnceLock<Streams>,
}

fn dyn_of<'a, T>(m: &'a HashMap<String, T>, d: &str, key: &str) -> Result<&'a T, Error> {
    m.get(d).ok_or_else(|| err(format!("brass: key {key}: no {d}")))
}

fn filter(data: &Data, r: &ArrRef) -> Result<Arc<[f32]>, Error> {
    Ok(Arc::from(data.f32(r)?))
}

fn read_note(n: &RawNote, data: &Data, key: &str) -> Result<Note, Error> {
    let mut settings = [(0.0, 0.0); 3];
    for (i, d) in DYNS.iter().enumerate() {
        let s = dyn_of(&n.settings, d, key)?;
        settings[i] = (s[0], s[1]);
    }
    let modes = |d: &str| -> Result<Modes, Error> {
        let m = dyn_of(&n.modes, d, key)?;
        Ok(Modes {
            sp_re: data.f64(&m.sp.re)?.into(),
            sp_im: data.f64(&m.sp.im)?.into(),
            rc_re: data.f64(&m.rc.re)?.into(),
            rc_im: data.f64(&m.rc.im)?.into(),
        })
    };
    let three = |m: &HashMap<String, ArrRef>| -> Result<[Arc<[f32]>; 3], Error> {
        Ok([filter(data, dyn_of(m, "pp", key)?)?, filter(data, dyn_of(m, "mf", key)?)?, filter(data, dyn_of(m, "ff", key)?)?])
    };
    Ok(Note {
        f: n.f,
        settings,
        modes: [modes("pp")?, modes("mf")?, modes("ff")?],
        h: three(&n.h)?,
        r: filter(data, &n.r)?,
        open: n.open.as_ref().map(three).transpose()?,
    })
}

impl Tables {
    pub(crate) fn load(raw: &RawBrass, data: &Data) -> Result<Tables, Error> {
        if raw.table.is_empty() || !raw.table.windows(2).all(|w| w[0] < w[1]) {
            return Err(err("brass: the table's keys must rise"));
        }
        let mutes = raw.order.clone();
        if mutes.first().map(String::as_str) != Some("open") {
            return Err(err("brass: open comes first"));
        }
        let mut notes = Vec::with_capacity(mutes.len());
        for m in &mutes {
            let rm = raw.mutes.get(m).ok_or_else(|| err(format!("brass: no tables for {m}")))?;
            let mut row = Vec::with_capacity(raw.table.len());
            for k in &raw.table {
                let key = k.to_string();
                let n = rm.notes.get(&key).ok_or_else(|| err(format!("brass: {m} has no key {key}")))?;
                row.push(read_note(n, data, &key)?);
            }
            notes.push(row);
        }
        let mut nearest = [0u8; KEYS];
        for (k, slot) in nearest.iter_mut().enumerate() {
            let mut best = (i32::MAX, 0usize);
            for (i, t) in raw.table.iter().enumerate() {
                let d = (i32::from(*t) - k as i32).abs();
                if d < best.0 {
                    best = (d, i);
                }
            }
            *slot = best.1 as u8;
        }
        let mut speak_air = Box::new([[1.0; 3]; KEYS]);
        let mut speak_first = Box::new([[0.0; 3]; KEYS]);
        for k in 0..KEYS {
            for (i, d) in DYNS.iter().enumerate() {
                if let Some(v) = raw.speak.get(&format!("{k}_{d}")) {
                    speak_air[k][i] = *v;
                }
                if let Some(v) = raw.speak.get(&format!("{k}_{d}_first")) {
                    speak_first[k][i] = *v;
                }
            }
        }
        let mut cells = Vec::with_capacity(mutes.len());
        for m in &mutes {
            // the plunger plays as open
            let name = if m == "plunger" { "open" } else { m.as_str() };
            let mut row: Box<[[Option<(f64, f64)>; 3]; KEYS]> = Box::new([[None; 3]; KEYS]);
            for (k, slot) in row.iter_mut().enumerate() {
                for (i, d) in DYNS.iter().enumerate() {
                    slot[i] = raw.cells.get(&format!("{name}_{k}_{d}")).map(|c| (c[0], c[1]));
                }
            }
            cells.push(row);
        }
        let has_level = !raw.level.is_empty();
        let mut level = Vec::with_capacity(mutes.len());
        for m in &mutes {
            let t = raw.level.get(m).or_else(|| raw.level.get("open"));
            let mut row = vec![Level::default(); raw.table.len()];
            if let Some(t) = t {
                for (i, k) in raw.table.iter().enumerate() {
                    if let Some(a) = t.get(&k.to_string()).and_then(|v| v.as_array()) {
                        if a.len() != 4 {
                            return Err(err(format!("brass: {m} key {k}: a level of {} anchors", a.len())));
                        }
                        for (j, x) in a.iter().enumerate() {
                            row[i].anchors[j] = x.as_f64().unwrap_or(0.0);
                        }
                    }
                    for (j, d) in DYNS.iter().enumerate() {
                        row[i].step[j] = t.get(&format!("{k}_{d}")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    }
                }
            }
            level.push(row);
        }
        let mut at = Vec::with_capacity(raw.table.len());
        for k in &raw.table {
            let (lp, ce, top) = raw.at.get(&k.to_string()).ok_or_else(|| err(format!("brass: no air for key {k}")))?;
            if lp.len() != 3 || ce.len() != 3 {
                return Err(err(format!("brass: key {k}: the air's anchors")));
            }
            at.push(At { lp: [lp[0], lp[1], lp[2]], ce: [ce[0], ce[1], ce[2]], top: *top });
        }
        let makeup_db = mutes.iter().map(|m| raw.makeup_db.get(m).copied().unwrap_or(0.0)).collect();
        Ok(Tables {
            mutes,
            table: raw.table.clone(),
            range: raw.range,
            fitted: raw.fitted,
            slide: raw.slide,
            gain_db: raw.gain_db,
            width: raw.width,
            y0: raw.y0,
            soft: raw.soft,
            rho: raw.rho,
            nl: raw.nl,
            mu: raw.mu,
            mu_f: raw.mu_f,
            mu_p: raw.mu_p,
            center: raw.center,
            vel_air: raw.vel_air.clone(),
            scoop_span: raw.scoop_span,
            attack_rise_periods: raw.attack_rise_periods,
            attack_rise_hz: raw.attack_rise_hz,
            attack_lean: raw.attack_lean,
            attack_from_rest: raw.attack_from_rest,
            attack_rise_from: raw.attack_rise_from.clamp(0.0, 1.0),
            noise: raw.noise.clone(),
            bell_radius: raw.bell_radius,
            pose: pose_of(raw.pose.as_ref(), raw.bell_radius),
            room_feed: raw.room_feed,
            lips: Lips { ratio: raw.lips.ratio.clone(), q: raw.lips.q.clone(), sign: raw.lips.sign.clone(), comp: raw.lips.comp.clone() },
            dec: raw.dec.clone(),
            makeup_db,
            has_level,
            notes,
            nearest,
            speak_air,
            speak_first,
            cells,
            level,
            at,
            streams: OnceLock::new(),
        })
    }

    /// The instrument's streams.
    pub fn streams(&self) -> Streams {
        self.streams.get_or_init(|| Streams::generate(self, 1)).clone()
    }

    /// The mute's place in `mutes`.
    pub fn mute_index(&self, name: &str) -> Result<usize, Error> {
        self.mutes.iter().position(|m| m == name).ok_or_else(|| err(format!("brass: no mute {name}")))
    }

    /// A table key under a mute (the nearest, off the table).
    pub fn note(&self, mute: usize, key: u8) -> &Note {
        &self.notes[mute][self.place(key)]
    }

    /// The place of the nearest table key.
    pub fn place(&self, key: u8) -> usize {
        usize::from(self.nearest[usize::from(key.min(127))])
    }

    /// The nearest table key.
    pub fn near(&self, key: u8) -> u8 {
        self.table[self.place(key)]
    }

    /// Does this key play?
    pub fn plays(&self, key: u8) -> bool {
        (self.range[0]..=self.range[1]).contains(&key)
    }

    pub fn speak_air(&self, key: u8, d: usize) -> f64 {
        self.speak_air[usize::from(key.min(127))][d]
    }

    pub fn speak_first(&self, key: u8, d: usize) -> f64 {
        self.speak_first[usize::from(key.min(127))][d]
    }

    pub fn cell(&self, mute: usize, key: u8, d: usize) -> Option<(f64, f64)> {
        self.cells[mute][usize::from(key.min(127))][d]
    }

    /// The level at a table place under a mute.
    pub fn level(&self, mute: usize, place: usize) -> &Level {
        &self.level[mute][place]
    }

    /// The air and lips at a table place.
    pub fn at(&self, place: usize) -> &At {
        &self.at[place]
    }

    /// The lips' mass per area for a note of f Hz (`spec.lip_mu`).
    pub fn lip_mu(&self, f: f64) -> f64 {
        self.mu * (self.mu_f / f).powf(self.mu_p)
    }
}
