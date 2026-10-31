//! The Compare page's logic (no UI): the exported instrument rendered key by key with the linked
//! engine, measured as its recordings are (`audio::measure_long`), the differences and the flags.
use crate::audio::{self, Long, Partial};
use crate::project::{Project, Repo, write_atomic};
use crate::{mapping, suggest};
use pianoengine::{Engine, Event, Instrument, Params, RadiationCache, SILENCE_S};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// the engine's sources, hashed at build time (build.rs): part of the cache's key
pub const ENGINE_HASH: &str = env!("PE_ENGINE_HASH");

/// how a pair is played: the key struck (the key played, not the sounding note), its hammer or
/// mallet speed (m/s), the stops of a register (several parts), the mallet (several mallets) and
/// the room
#[derive(Clone, Debug, PartialEq)]
pub struct Strike {
    pub key: u8,
    pub speed: f64,
    pub stops: Option<u32>,
    pub mallet: Option<u32>,
    /// the room's level for this render (a dry set's: 0; none: the export's)
    pub room: Option<f32>,
}

/// an export loaded once, rendered from again and again: one engine per sample rate, silenced
/// between renders (building one took 0.13 s, 85 % of a pair's time; rendering 2 s 0.03 s)
pub struct Renderer {
    inst: Instrument,
    cache: RadiationCache,
    engine: Option<(u32, Engine)>,
    /// the engine's own parameters: each render starts from them (a reused engine keeps the
    /// previous pair's stops, mallet and room otherwise)
    defaults: Option<Params>,
}

const BLOCK: usize = 512;
/// a reused engine strikes on a multiple of this (a multiple of BLOCK and of the convolution's
/// 2048-sample ring)
const ALIGN: usize = 4096;

impl Renderer {
    pub fn new(export: &Path) -> Result<Renderer, String> {
        let inst = Instrument::load(export).map_err(|e| format!("{}: {e}", export.display()))?;
        Ok(Renderer { inst, cache: RadiationCache::default(), engine: None, defaults: None })
    }

    /// one key from silence at `rate`, held for `seconds`, no pedal: mono (the two outputs' mean,
    /// as the measurement reads it), the engine's latency trimmed
    pub fn render(&mut self, s: &Strike, rate: u32, seconds: f32) -> Result<Vec<f32>, String> {
        let (l, r) = self.render_stereo(s, rate, seconds)?;
        Ok(l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect())
    }

    /// as `render`, its two outputs (the A/B plays them)
    pub fn render_stereo(&mut self, s: &Strike, rate: u32, seconds: f32) -> Result<(Vec<f32>, Vec<f32>), String> {
        if !KEYS.contains(&s.key) {
            return Err(format!("key {} is outside the keyboard (A0–C8)", s.key));
        }
        let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        let e = match &mut self.engine {
            Some((sr, e)) if *sr == rate => {
                // the one before faded out and cleared (strings, the bank, the body's and the
                // room's tails): then as from a fresh engine
                e.silence();
                for _ in 0..((SILENCE_S * rate as f64) as usize).div_ceil(BLOCK) + 1 {
                    e.process(&mut l, &mut r);
                }
                // the strike on a multiple of ALIGN samples, as a fresh engine's at 0: its block
                // schedules (the convolution's ring) at the same phase, the render exact
                let mut left = (ALIGN as u64 - e.now() % ALIGN as u64) as usize % ALIGN;
                while left > 0 {
                    let n = left.min(BLOCK);
                    e.process(&mut l[..n], &mut r[..n]);
                    left -= n;
                }
                e
            }
            slot => {
                let tuning = self.inst.tuning;
                let e = Engine::with_cache(self.inst.clone(), rate as f64, tuning, &mut self.cache).map_err(|e| e.to_string())?;
                &mut slot.insert((rate, e)).1
            }
        };
        let d = *self.defaults.get_or_insert_with(|| e.params());
        // the instrument as fitted: without a strike's own variation (the level tables' renders too)
        e.set_params(Params { stops: s.stops.unwrap_or(d.stops), mallet: s.mallet.unwrap_or(d.mallet), room: s.room.unwrap_or(d.room), variation: 0.0, ..d });
        e.queue(0, Event::NoteOn { key: s.key, speed: s.speed });
        let n = (seconds * rate as f32) as usize;
        let lat = e.latency();
        let (mut l, mut r) = (vec![0.0f32; n + lat], vec![0.0f32; n + lat]);
        for (cl, cr) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
            e.process(cl, cr);
        }
        Ok((l[lat..].to_vec(), r[lat..].to_vec()))
    }
}

/// the engine's keys (A0–C8; its C interface refuses others, `Event::NoteOn` expects them)
pub const KEYS: std::ops::RangeInclusive<u8> = 21..=108;

/// the dynamics a fit has speeds for
pub const FITTED: [&str; 3] = ["pp", "mf", "ff"];

/// one mapped recording and how its render is played
#[derive(Clone, Debug)]
pub struct Pair {
    pub source: String,
    pub path: PathBuf,
    /// the sounding note (the mapping's `note`)
    pub note: i64,
    /// the fitted dynamic (pp, mf, ff)
    pub dynamic: String,
    pub strike: Strike,
}

pub struct Pairs {
    pub list: Vec<Pair>,
    /// items whose layer no fitted dynamic names
    pub skipped: usize,
    /// items whose key lies outside the keyboard (A0–C8: a set named octaves low)
    pub outside: usize,
    /// why there are none (no fit with speeds, nothing mapped)
    pub none_why: Option<String>,
}

fn read_json(path: &Path) -> Option<Json> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// the stop (bit of the engine's `stops`) that plays register `r` of an export: its `stop`, else
/// its place in the list
fn register_stop(inst: Option<&Json>, r: usize) -> u32 {
    inst.and_then(|i| i["registers"][r]["stop"].as_u64()).map_or(r as u32, |s| s as u32)
}

/// every source's mapped items whose layer a fitted dynamic names: the main sources at the fit's
/// `hammer_speeds`, a mallet source (`role = "mallet"`, in order) at its mallet's own speeds with
/// its index; several parts each with its register's stop
pub fn pairs(repo: &Repo, project: &Project) -> Pairs {
    let j = project.to_json();
    // recorded anechoic: rendered without a room, as recorded
    let room = (j["project"]["recorded_dry"].as_bool() == Some(true)).then_some(0.0f32);
    let parts = j.get("part").and_then(Json::as_array).cloned().unwrap_or_default();
    // the export: each part's register plays on the stop it names (a recorded second 8′: the 8′ II's)
    let export = read_json(&export_dir(repo, &project.name).join("instrument.json"));
    let (mut list, mut skipped, mut outside, mut problems) = (Vec::new(), 0usize, 0usize, Vec::<String>::new());
    for (pi, part) in parts.iter().enumerate() {
        let Some(pid) = part["id"].as_str() else { continue };
        let Some(fit) = read_json(&repo.projects.join(&project.name).join("fits").join(format!("{pid}.json"))) else {
            problems.push(format!("the part {pid} has no fit (build the project first)"));
            continue;
        };
        if !fit["hammer_speeds"].is_object() {
            problems.push(format!("the part {pid}'s fit has no speeds to play the recorded dynamics at"));
            continue;
        }
        let stops = (parts.len() > 1).then_some(1u32 << register_stop(export.as_ref(), pi));
        let mut mallet_index = 0u32;
        for src in part.get("source").and_then(Json::as_array).into_iter().flatten() {
            let role = src["role"].as_str().unwrap_or("main");
            let (mallet, speeds) = match role {
                "main" => (None, fit["hammer_speeds"].clone()),
                "mallet" => {
                    let i = mallet_index;
                    mallet_index += 1;
                    let own = &fit["mallets"][i as usize]["speeds"];
                    (Some(i), if own.is_object() { own.clone() } else { fit["hammer_speeds"].clone() })
                }
                _ => continue,
            };
            let root = src["root"].as_str().unwrap_or_default();
            let Ok(rep) = mapping::items(src, &suggest::data_root(&repo.data, root)) else { continue };
            // the item's layer (its `dynamic`, or its `layer` number with `layer_key = "layer"`,
            // as `mapping::dynamics_problems` reads it) → the fitted dynamic
            let layer_key = if src["layer_key"].as_str() == Some("layer") { "layer" } else { "dynamic" };
            let named: Vec<(&Json, &String)> = src["dynamics"].as_object().into_iter().flatten()
                .filter(|(k, _)| FITTED.contains(&k.as_str())).map(|(k, v)| (v, k)).collect();
            for it in &rep.items {
                let Some((_, dynamic)) = it.get(layer_key).and_then(|layer| named.iter().find(|(l, _)| *l == layer)) else {
                    skipped += 1;
                    continue;
                };
                let (Some(path), Some(note), Some(speed)) = (it["path"].as_str(), it["note"].as_i64(), speeds[dynamic.as_str()].as_f64()) else { continue };
                let key = it.get("key").and_then(Json::as_i64).unwrap_or(note);
                let Some(key) = u8::try_from(key).ok().filter(|k| KEYS.contains(k)) else {
                    outside += 1;
                    continue;
                };
                list.push(Pair { source: src["id"].as_str().unwrap_or_default().to_string(), path: path.into(), note,
                    dynamic: (*dynamic).clone(), strike: Strike { key, speed, stops, mallet, room } });
            }
        }
    }
    // a project with one unfitted part and one fitted part shows the fitted part's pairs
    let none_why = list.is_empty().then(|| problems.first().cloned()
        .unwrap_or_else(|| "no mapped recording of a fitted dynamic (pp, mf, ff)".into()));
    Pairs { list, skipped, outside, none_why }
}

/// a key is flagged past these, re the instrument's own medians (`Medians`): set from the committed
/// instruments (2026-10-15, editor/tests/compare_corpus.rs): 0–5 % of their pairs flagged on most
pub const LEVEL_DB: f32 = 12.0;
pub const PITCH_CENTS: f32 = 25.0;
pub const DECAY_RATIO: f32 = 4.0;
/// the length (render / recording, re the median), either way (a stopped recording's: only shorter);
/// set with the user from the committed instruments (2026-10-15: 173 of 3089 pairs, 5.6 %)
pub const LENGTH_RATIO: f32 = 2.0;
pub const SPECTRUM_DB: f32 = 12.0;
/// the spectrum compares levels clamped here (dB re the strongest): a faint partial missing
/// counts little, a strong one fully
pub const SPECTRUM_FLOOR_DB: f32 = -40.0;
/// the decay compares the partials this strong (dB re the strongest): a fainter one rings in the
/// noise (a marimba's at −53 dB read a late rate of 0.003/s)
pub const DECAY_PARTIAL_DB: f32 = -30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fundamental {
    Both,
    Neither,
    RecordingOnly,
    RenderOnly,
}

/// a pair's render against its recording
#[derive(Clone, Debug, PartialEq)]
pub struct Diff {
    /// render − recording (dB; the median over all pairs is taken off when flagging)
    pub level_db: f32,
    /// the render's fundamental re the recording's (cents)
    pub pitch_cents: Option<f32>,
    pub fundamental: Fundamental,
    /// per segment, the geometric mean over the partials with a rate on both sides of render /
    /// recording (above 1: the render dies faster)
    pub early: Option<f32>,
    /// for the hover text, not flagged (a room tail or a stop overstates it)
    pub late: Option<f32>,
    /// the render's length re the recording's
    pub length: Option<f32>,
    /// the recording was stopped: its length is a lower bound (only a shorter render tells)
    pub length_stopped: bool,
    /// the rms over the recording's partials above their noise (PARTIAL_SNR_DB) of render −
    /// recording (dB re each one's strongest, both clamped at SPECTRUM_FLOOR_DB)
    pub spectrum_db: f32,
}

/// rates at or below this (1/s) say nothing: a recording's partial ringing in the noise, a render's
/// partial rising or beating over the window (its slope clamped at 0; a piano's unisons beat over the
/// late window's few seconds). A key whose render rings on is therefore not flagged on its decay
const RATE_FLOOR: f32 = 1e-3;

/// the geometric mean of render / recording over the partials with a rate on both sides
fn geo_ratio(pairs: impl Iterator<Item = (Option<f32>, Option<f32>)>) -> Option<f32> {
    let logs: Vec<f32> = pairs.filter_map(|(ren, rec)| match (ren, rec) {
        (Some(a), Some(b)) if a > RATE_FLOOR && b > RATE_FLOOR => Some((a / b).ln()),
        _ => None,
    }).collect();
    (!logs.is_empty()).then(|| (logs.iter().sum::<f32>() / logs.len() as f32).exp())
}

pub fn diff(rec: &Long, ren: &Long) -> Diff {
    let (fundamental, pitch_cents) = match (rec.fundamental_hz, ren.fundamental_hz) {
        (Some(a), Some(b)) => (Fundamental::Both, Some(1200.0 * (b / a).log2())),
        (None, None) => (Fundamental::Neither, None),
        (Some(_), None) => (Fundamental::RecordingOnly, None),
        (None, Some(_)) => (Fundamental::RenderOnly, None),
    };
    let both = || rec.partials.iter().zip(&ren.partials);
    // a partial in the recording's noise is its noise, not its brightness
    let heard = || both().filter(|(a, _)| a.snr_db >= audio::PARTIAL_SNR_DB);
    let n = heard().count().max(1) as f32;
    // each side re its own strongest heard partial: a rumble under the note (the strongest peak of
    // an Iowa pp take) set the recording's 0 dB, the render's fundamental the render's
    let top = |side: &dyn Fn(&(&Partial, &Partial)) -> f32| heard().map(|x| side(&x)).filter(|db| *db > audio::UNMATCHED_DB)
        .fold(f32::NEG_INFINITY, f32::max);
    let (ta, tb) = (top(&|x| x.0.db), top(&|x| x.1.db));
    let (ta, tb) = (if ta.is_finite() { ta } else { 0.0 }, if tb.is_finite() { tb } else { 0.0 });
    let clamp = |db: f32| db.max(SPECTRUM_FLOOR_DB);
    let spectrum_db = (heard().map(|(a, b)| (clamp(b.db - tb) - clamp(a.db - ta)).powi(2)).sum::<f32>() / n).sqrt();
    Diff {
        level_db: ren.level_db - rec.level_db,
        pitch_cents,
        fundamental,
        early: geo_ratio(both().filter(|(a, _)| a.db >= DECAY_PARTIAL_DB).map(|(a, b)| (b.early, a.early))),
        late: geo_ratio(both().filter(|(a, _)| a.db >= DECAY_PARTIAL_DB).map(|(a, b)| (b.late, a.late))),
        length: match (rec.length_s, ren.length_s) {
            (Some(a), Some(b)) if a > 1e-3 => Some(b / a),
            _ => None,
        },
        length_stopped: rec.stopped,
        spectrum_db,
    }
}

/// how the whole instrument differs from its recordings: each measure's median over its pairs (the
/// level per dynamic: a dynamic's offset is the velocity curve's). A key is flagged where it
/// differs from these, not from zero: an instrument brighter or faster everywhere flags nothing
#[derive(Clone, Debug, PartialEq)]
pub struct Medians {
    pub level: BTreeMap<String, f32>,
    pub pitch: f32,
    /// ratios (render / recording)
    pub early: f32,
    pub late: f32,
    /// over the pairs whose recording was not stopped
    pub length: f32,
    pub spectrum: f32,
    /// how far the recordings' levels are trusted, and with octave groups each run's level
    /// difference (dynamic, key / 12), with key groups each key's reference (dynamic, key)
    pub groups: LevelGroups,
    pub runs: BTreeMap<(String, i64), f32>,
}

impl Default for Medians {
    /// against zero: no offsets
    fn default() -> Medians {
        Medians { level: BTreeMap::new(), pitch: 0.0, early: 1.0, late: 1.0, length: 1.0, spectrum: 0.0,
            groups: LevelGroups::Whole, runs: BTreeMap::new() }
    }
}

impl Medians {
    pub fn level_of(&self, dynamic: &str) -> f32 {
        self.level.get(dynamic).copied().unwrap_or(0.0)
    }

    /// what a key's level difference is judged against: the dynamic's median, its run's (octave
    /// groups: a run's own gain is not the instrument's), the key's loudest dynamic's and the step
    /// to it (key groups; the loudest itself: nothing), or nothing (normalized files)
    pub fn level_ref(&self, dynamic: &str, key: i64) -> Option<f32> {
        match self.groups {
            LevelGroups::Whole => Some(self.level_of(dynamic)),
            LevelGroups::Octave => Some(self.runs.get(&(dynamic.to_string(), key.div_euclid(12))).copied().unwrap_or(self.level_of(dynamic))),
            LevelGroups::Key => self.runs.get(&(dynamic.to_string(), key)).copied(),
            LevelGroups::File => None,
        }
    }
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 { v[m] } else { 0.5 * (v[m - 1] + v[m]) })
}

/// the medians of an instrument's pairs (each with its fitted dynamic)
pub fn medians(rows: &[(&str, &Diff)]) -> Medians {
    let mut level = BTreeMap::new();
    for k in FITTED {
        if let Some(m) = median(rows.iter().filter(|r| r.0 == k).map(|r| r.1.level_db).collect()) {
            level.insert(k.to_string(), m);
        }
    }
    let ratio = |f: fn(&Diff) -> Option<f32>| median(rows.iter().filter_map(|r| f(r.1).map(f32::ln)).collect()).map_or(1.0, f32::exp);
    Medians {
        level,
        pitch: median(rows.iter().filter_map(|r| r.1.pitch_cents).collect()).unwrap_or(0.0),
        early: ratio(|d| d.early),
        late: ratio(|d| d.late),
        length: median(rows.iter().filter(|r| !r.1.length_stopped).filter_map(|r| r.1.length.map(f32::ln)).collect()).map_or(1.0, f32::exp),
        spectrum: median(rows.iter().map(|r| r.1.spectrum_db).collect()).unwrap_or(0.0),
        groups: LevelGroups::Whole,
        runs: BTreeMap::new(),
    }
}

/// the medians per source with its level groups (a source left out of `groups`: whole); with
/// octave groups each run's level median besides, with key groups each key's reference
pub fn medians_grouped(rows: &[(&Pair, &Diff)], groups: &BTreeMap<String, LevelGroups>) -> BTreeMap<String, Medians> {
    let mut ms = medians_by_source(&rows.iter().map(|(p, d)| (p.source.as_str(), p.dynamic.as_str(), *d)).collect::<Vec<_>>());
    for (src, m) in ms.iter_mut() {
        m.groups = groups.get(src).copied().unwrap_or(LevelGroups::Whole);
        if m.groups == LevelGroups::Octave {
            let mut runs: BTreeMap<(String, i64), Vec<f32>> = BTreeMap::new();
            for (p, d) in rows.iter().filter(|(p, _)| &p.source == src) {
                runs.entry((p.dynamic.clone(), (p.strike.key as i64).div_euclid(12))).or_default().push(d.level_db);
            }
            m.runs = runs.into_iter().filter_map(|(k, v)| median(v).map(|x| (k, x))).collect();
        } else if m.groups == LevelGroups::Key {
            m.runs = key_refs(&rows.iter().filter(|(p, _)| &p.source == src).copied().collect::<Vec<_>>());
        }
    }
    ms
}

/// the medians per source (its pairs: source id, fitted dynamic, difference): each mallet or register
/// judged against itself, its own overall difference shown, not flagged on every key
pub fn medians_by_source(rows: &[(&str, &str, &Diff)]) -> BTreeMap<String, Medians> {
    let mut ids: Vec<&str> = rows.iter().map(|r| r.0).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter().map(|id| {
        let own: Vec<(&str, &Diff)> = rows.iter().filter(|r| r.0 == id).map(|r| (r.1, r.2)).collect();
        (id.to_string(), medians(&own))
    }).collect()
}

/// what is off in a pair, re the instrument's medians: the level and pitch as differences, the
/// decays as ratios, the spectrum as its excess
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Off {
    Level(f32),
    Pitch(f32),
    Early(f32),
    Length(f32),
    Spectrum(f32),
}

impl Off {
    pub fn text(&self) -> String {
        match self {
            Off::Level(l) => format!("level {l:+.1} dB"),
            Off::Pitch(c) => format!("pitch {c:+.0} cents"),
            Off::Early(r) => format!("early decay ×{r:.2}"),
            Off::Length(r) => format!("length ×{r:.2}"),
            Off::Spectrum(s) => format!("spectrum {s:+.1} dB rms"),
        }
    }

    /// how far past its threshold (1 at it)
    fn badness(&self) -> f32 {
        match *self {
            Off::Level(l) => l.abs() / LEVEL_DB,
            Off::Pitch(c) => c.abs() / PITCH_CENTS,
            Off::Early(r) => r.ln().abs() / DECAY_RATIO.ln(),
            Off::Length(r) => r.ln().abs() / LENGTH_RATIO.ln(),
            Off::Spectrum(s) => s / SPECTRUM_DB,
        }
    }
}

/// what is off in a pair (its fitted dynamic), re the instrument's medians (the level re the
/// dynamic's median, whatever the source's level groups)
pub fn offs(d: &Diff, dynamic: &str, m: &Medians) -> Vec<Off> {
    offs_re(d, Some(m.level_of(dynamic)), m)
}

/// what is off in a pair re its source's medians and level groups (the level re its run's median
/// or not judged: `Medians::level_ref`)
pub fn offs_at(d: &Diff, p: &Pair, m: &Medians) -> Vec<Off> {
    offs_re(d, m.level_ref(&p.dynamic, p.strike.key as i64), m)
}

fn offs_re(d: &Diff, level_ref: Option<f32>, m: &Medians) -> Vec<Off> {
    let mut out = Vec::new();
    if let Some(level) = level_ref.map(|r| d.level_db - r).filter(|l| l.abs() > LEVEL_DB) {
        out.push(Off::Level(level));
    }
    if let Some(c) = d.pitch_cents.map(|c| c - m.pitch).filter(|c| c.abs() > PITCH_CENTS) {
        out.push(Off::Pitch(c));
    }
    let past = |r: f32| r > DECAY_RATIO || r < 1.0 / DECAY_RATIO;
    if let Some(r) = d.early.map(|r| r / m.early).filter(|r| past(*r)) {
        out.push(Off::Early(r));
    }
    if let Some(r) = d.length.map(|r| r / m.length) {
        // a stopped recording's length is a lower bound: only a shorter render is off
        let off = if d.length_stopped { r < 1.0 / LENGTH_RATIO } else { r > LENGTH_RATIO || r < 1.0 / LENGTH_RATIO };
        if off {
            out.push(Off::Length(r));
        }
    }
    if d.spectrum_db - m.spectrum > SPECTRUM_DB {
        out.push(Off::Spectrum(d.spectrum_db - m.spectrum));
    }
    out
}

/// the worst of what is off, re its threshold (0: nothing)
pub fn badness(d: &Diff, dynamic: &str, m: &Medians) -> f32 {
    offs(d, dynamic, m).iter().map(Off::badness).fold(0.0, f32::max)
}

/// as `badness`, re the pair's source's level groups
pub fn badness_at(d: &Diff, p: &Pair, m: &Medians) -> f32 {
    offs_at(d, p, m).iter().map(Off::badness).fold(0.0, f32::max)
}

/// raised when the render or the measurement changes: the cache's entries are measured again
pub const COMPARE_VERSION: u32 = 11;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub recording: Long,
    pub render: Long,
}

/// how the A/B plays the renders: as rendered, each at its recording's level, or with its source's
/// median level difference for the dynamic taken off (the instrument's output gain, not a key's
/// own difference: what a level correction changes stays audible)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LevelMatch {
    Off,
    Pair,
    Instrument,
}

/// the render's and the original's gains (dB) as played; `orig_level`: the original's level over
/// its first 0.5 s, `median_db`: the pair's source's median render − recording for its dynamic
pub fn playback_gains(mode: LevelMatch, e: &Entry, orig_level: Option<f32>, median_db: f32) -> (f32, f32) {
    match mode {
        LevelMatch::Off => (0.0, 0.0),
        LevelMatch::Pair => (e.recording.level_db - e.render.level_db, orig_level.map_or(0.0, |o| e.recording.level_db - o)),
        LevelMatch::Instrument => (-median_db, -median_db),
    }
}

pub fn export_dir(repo: &Repo, project: &str) -> PathBuf {
    repo.root.join("instruments").join(project)
}

/// the export before the last build (the exports keep it when their output changes): the page's
/// Original
pub fn previous_export_dir(repo: &Repo, project: &str) -> PathBuf {
    repo.projects.join(project).join("cache").join("previous-export")
}

pub fn cache_path(repo: &Repo, project: &str) -> PathBuf {
    repo.projects.join(project).join("cache").join("compare.json")
}

fn stamp(path: &Path) -> String {
    std::fs::metadata(path).ok().map(|m| {
        let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
        format!("{}:{t}", m.len())
    }).unwrap_or_default()
}

#[derive(Default, Serialize, Deserialize)]
struct CacheFile {
    /// COMPARE_VERSION, the engine's hash and the export's stamps: anything else, all again
    key: String,
    /// by pair id: the recording's stamp and the entry
    entries: BTreeMap<String, (String, Entry)>,
}

fn cache_key(export: &Path) -> String {
    format!("{COMPARE_VERSION}/{ENGINE_HASH}/{}/{}", stamp(&export.join("instrument.json")), stamp(&export.join("data.bin")))
}

fn pair_id(p: &Pair) -> String {
    // the stops where an instrument has registers (a register's stop may move: a recorded second 8′)
    let stops = p.strike.stops.map(|s| format!("|s{s}")).unwrap_or_default();
    format!("{}|{}|{}|{}|{}{stops}", p.source, p.path.display(), p.dynamic, p.strike.key, p.strike.room.unwrap_or(-1.0))
}

/// how long a recording's render runs: as long as the recording sounds, and long enough to show a
/// render LENGTH_RATIO times as long as the recording against a median of up to 2 (at twice the
/// recording's length a render ringing on read ×2 at most, never flagged)
pub fn render_seconds(recording: &Long) -> f32 {
    (recording.end_s.max(2.0 * LENGTH_RATIO * recording.length_s.unwrap_or(0.0)) + 0.5).min(audio::LONG_MAX_S)
}

/// one pair: the recording measured over as long as it sounds, the render as long and measured
/// over the same times
pub fn compare_one(pair: &Pair, r: &mut Renderer) -> Result<Entry, String> {
    let clip = audio::decode(&pair.path, Some(audio::LONG_MAX_S + 10.0))?;
    let x = clip.mono();
    let rate = clip.rate as f32;
    let recording = audio::measure_long(&x, rate, pair.note as f64);
    // from silence: the render's onset is within its first milliseconds
    let y = r.render(&pair.strike, clip.rate, render_seconds(&recording))?;
    // its shape measured at the recording's level (the measures' floors are absolute: a quiet
    // instrument's soft treble under them read 11–17× long), its level as rendered
    let level_db = audio::level_db(&y, audio::onset(&y), (0.5 * rate) as usize);
    let g = if level_db.is_finite() { 10f32.powf((recording.level_db - level_db) / 20.0) } else { 1.0 };
    let scaled: Vec<f32> = y.iter().map(|v| v * g).collect();
    let mut render = audio::measure_like(&scaled, rate, pair.note as f64, &recording);
    render.level_db = level_db;
    Ok(Entry { recording, render })
}

/// the workers for a run: one per physical core (half the logical ones; with 16 workers on 8
/// cores a run took longer than with 8: the engines' convolution buffers share the cache)
pub fn threads() -> usize {
    (std::thread::available_parallelism().map_or(4, |n| n.get()) / 2).max(1)
}

pub struct Run<'a> {
    pub pairs: &'a [Pair],
    pub export: &'a Path,
    pub cache: &'a Path,
    pub threads: usize,
    /// pairs done (from the cache or rendered)
    pub progress: &'a AtomicUsize,
    /// pairs rendered (not from the cache)
    pub rendered: &'a AtomicUsize,
    /// stops the workers between pairs
    pub cancel: &'a AtomicBool,
}

/// every pair through `sink` (its index in `pairs`): the cached ones first, the others rendered on
/// `threads` workers, each with the export loaded once; the cache written at the end, also when
/// cancelled
pub fn run(r: &Run, sink: &(dyn Fn(usize, Result<Entry, String>) + Sync)) -> Result<(), String> {
    let key = cache_key(r.export);
    let mut file: CacheFile = std::fs::read_to_string(r.cache).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if file.key != key {
        file = CacheFile { key, entries: BTreeMap::new() };
    }
    let mut todo = Vec::new();
    for (i, p) in r.pairs.iter().enumerate() {
        match file.entries.get(&pair_id(p)) {
            Some((s, e)) if *s == stamp(&p.path) => {
                sink(i, Ok(e.clone()));
                r.progress.fetch_add(1, Ordering::Relaxed);
            }
            _ => todo.push(i),
        }
    }
    let first = Renderer::new(r.export)?;
    let file = Mutex::new(file);
    let next = AtomicUsize::new(0);
    // joined one by one: a worker that panics (in the sink, or past `compare_one`'s guard) ends
    // the run with an error, the others' work still cached
    let stopped: Vec<String> = std::thread::scope(|s| {
        let mut firsts = vec![Some(first)];
        for _ in 1..r.threads.max(1).min(todo.len().max(1)) {
            firsts.push(None);
        }
        let workers: Vec<_> = firsts.into_iter().map(|slot| {
            let (todo, next, file) = (&todo, &next, &file);
            s.spawn(move || {
                let mut renderer = match slot {
                    Some(x) => x,
                    None => match Renderer::new(r.export) {
                        Ok(x) => x,
                        Err(_) => return,
                    },
                };
                loop {
                    if r.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&i) = todo.get(k) else { return };
                    let p = &r.pairs[i];
                    // a fault in the engine or a decoder fails the pair, not the run; the renderer
                    // is made again (its engine may be left half-way)
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compare_one(p, &mut renderer)))
                        .unwrap_or_else(|e| {
                            if let Ok(x) = Renderer::new(r.export) {
                                renderer = x;
                            }
                            Err(format!("the comparison stopped on a fault: {}", panic_text(&e)))
                        });
                    if let Ok(e) = &res {
                        file.lock().unwrap_or_else(|e| e.into_inner()).entries.insert(pair_id(p), (stamp(&p.path), e.clone()));
                    }
                    r.rendered.fetch_add(1, Ordering::Relaxed);
                    r.progress.fetch_add(1, Ordering::Relaxed);
                    sink(i, res);
                }
            })
        }).collect();
        workers.into_iter().filter_map(|w| w.join().err().map(|e| panic_text(&e))).collect()
    });
    let file = file.into_inner().unwrap_or_else(|e| e.into_inner());
    if let Some(dir) = r.cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    write_atomic(r.cache, &serde_json::to_string(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    match stopped.first() {
        Some(e) => Err(format!("a worker stopped: {e}")),
        None => Ok(()),
    }
}

fn panic_text(e: &Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic".into())
}

/// a page's level correction is clamped to this (dB)
pub const CORRECTION_LIMIT_DB: f32 = 24.0;
/// the running median along the keys: a key and up to this many recorded keys on either side
pub const CORRECTION_NEIGHBORS: usize = 2;
/// a dynamic with fewer recorded keys gets no correction (a curve needs neighbors)
pub const CORRECTION_MIN_KEYS: usize = 3;

/// a source's level corrections (`[part.source.level_corrections]`): dB per recorded key and
/// dynamic, linear between the keys, the ends held
#[derive(Clone, Debug, PartialEq)]
pub struct LevelTable {
    pub keys: Vec<i64>,
    pub db: BTreeMap<String, Vec<f32>>,
}

impl LevelTable {
    pub fn at(&self, dynamic: &str, key: i64) -> f32 {
        let Some(v) = self.db.get(dynamic) else { return 0.0 };
        if self.keys.is_empty() || v.len() != self.keys.len() {
            return 0.0;
        }
        let i = self.keys.partition_point(|k| *k <= key);
        if i == 0 {
            return v[0];
        }
        if i == self.keys.len() {
            return v[i - 1];
        }
        let (k0, k1) = (self.keys[i - 1], self.keys[i]);
        v[i - 1] + (key - k0) as f32 / (k1 - k0) as f32 * (v[i] - v[i - 1])
    }

    pub fn to_json(&self) -> Json {
        let mut o = serde_json::Map::new();
        o.insert("keys".into(), json!(self.keys));
        for d in FITTED {
            if let Some(v) = self.db.get(d) {
                o.insert(d.into(), json!(v.iter().map(|x| (*x as f64 * 10.0).round() / 10.0).collect::<Vec<_>>()));
            }
        }
        Json::Object(o)
    }

    pub fn from_json(j: &Json) -> Option<LevelTable> {
        let keys: Vec<i64> = j.get("keys")?.as_array()?.iter().filter_map(Json::as_i64).collect();
        let db = FITTED.iter().filter_map(|d| {
            let v: Vec<f32> = j.get(*d)?.as_array()?.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect();
            (v.len() == keys.len()).then(|| (d.to_string(), v))
        }).collect();
        Some(LevelTable { keys, db })
    }
}

/// the corrections an export carries, per source: each of its recorded keys' note's `level_db`
/// (its register's notes, its mallet's triple; a note without one: 0 dB). What the renders
/// contain: a proposal starts from these, never from a table written but not yet built
pub fn corrections_in_export(inst: &Json, pairs: &[Pair]) -> BTreeMap<String, LevelTable> {
    let mut out: BTreeMap<String, LevelTable> = BTreeMap::new();
    for p in pairs {
        let register = p.strike.stops.map_or(0, |b| {
            let regs = inst.get("registers").and_then(Json::as_array).map_or(0, Vec::len);
            (0..regs).find(|&r| register_stop(Some(inst), r) == b.trailing_zeros()).unwrap_or(0)
        });
        let notes = match inst.get("registers").and_then(Json::as_array) {
            Some(regs) => regs.get(register).and_then(|r| r["notes"].as_array()),
            None => inst["notes"].as_array(),
        };
        let note = notes.and_then(|ns| ns.iter().find(|n| n["note"].as_i64() == Some(p.strike.key as i64)));
        let triple: [f32; 3] = note.and_then(|n| n["level_db"].as_array()).filter(|t| !t.is_empty()).map_or([0.0; 3], |t| {
            let m = (p.strike.mallet.unwrap_or(0) as usize).min(t.len() - 1);
            let v = |i: usize| t[m].get(i).and_then(Json::as_f64).unwrap_or(0.0) as f32;
            [v(0), v(1), v(2)]
        });
        let t = out.entry(p.source.clone()).or_insert_with(|| LevelTable { keys: Vec::new(), db: BTreeMap::new() });
        let key = p.strike.key as i64;
        if let Err(at) = t.keys.binary_search(&key) {
            t.keys.insert(at, key);
            for (i, d) in FITTED.iter().enumerate() {
                t.db.entry(d.to_string()).or_default().insert(at, triple[i]);
            }
        }
    }
    out
}

/// a source's current level corrections
pub fn level_table_of(src: &Json) -> Option<LevelTable> {
    src.get("level_corrections").and_then(LevelTable::from_json)
}

/// how far a source's recorded levels can be trusted: as a whole (recorded), within each octave
/// run C…B (each run recorded at its own gain or touch: the Iowa percussion's), within each key
/// (each key scaled by one gain: the steps between its dynamics recorded, Grand A's), or not at
/// all (every file normalized to its peak)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LevelGroups {
    Whole,
    Octave,
    Key,
    File,
}

/// a source's level groups: its `level_groups` ("octave", "key", "file", "whole"), else from its
/// `levels` (per_key: each key's steps; normalized: none to trust)
pub fn level_groups_of(src: &Json) -> LevelGroups {
    match (src["level_groups"].as_str(), src["levels"].as_str()) {
        (Some("octave"), _) => LevelGroups::Octave,
        (Some("key"), _) => LevelGroups::Key,
        (Some("file"), _) => LevelGroups::File,
        (Some(_), _) => LevelGroups::Whole,
        (None, Some("per_key")) => LevelGroups::Key,
        (None, Some("normalized")) => LevelGroups::File,
        (None, _) => LevelGroups::Whole,
    }
}

/// with key groups, what each key's softer dynamics are judged against: the key's loudest
/// recorded dynamic's difference plus the source's median step to it (the steps between a key's
/// dynamics are recorded, its own gain is not); the loudest dynamic itself has none
fn key_refs(rows: &[(&Pair, &Diff)]) -> BTreeMap<(String, i64), f32> {
    let Some(top) = FITTED.iter().rev().find(|k| rows.iter().any(|(p, _)| p.dynamic == **k)) else { return BTreeMap::new() };
    let tops: BTreeMap<i64, f32> = rows.iter().filter(|(p, _)| p.dynamic == *top).map(|(p, d)| (p.strike.key as i64, d.level_db)).collect();
    let mut out = BTreeMap::new();
    for k in FITTED.iter().filter(|k| *k != top) {
        let steps: Vec<(i64, f32)> = rows.iter().filter(|(p, _)| p.dynamic == *k)
            .filter_map(|(p, d)| tops.get(&(p.strike.key as i64)).map(|t| (p.strike.key as i64, d.level_db - t))).collect();
        let Some(step) = median(steps.iter().map(|s| s.1).collect()) else { continue };
        for (key, _) in steps {
            out.insert((k.to_string(), key), tops[&key] + step);
        }
    }
    out
}

/// from this sway a proposal is marked to be checked by ear before it is written (dB): the
/// marimba's approved tables swayed up to 4.6, the Bells' plastic 7.4 (2026-10-16)
pub const ONE_KEY_SWAY_WARN_DB: f32 = 5.0;

/// how far one recorded key alone moves a source's proposal: the largest change of its table at
/// any key when that key (one dynamic) is left out, and which it was
#[derive(Clone, Debug, PartialEq)]
pub struct Sway {
    pub db: f32,
    pub dynamic: String,
    pub key: i64,
}

/// each source's sway (level_corrections recomputed with each of its keys left out in turn)
pub fn one_key_sway(rows: &[(&Pair, &Diff)], current: &BTreeMap<String, LevelTable>, groups: &BTreeMap<String, LevelGroups>)
    -> BTreeMap<String, Sway> {
    let full = level_corrections(rows, current, groups);
    let mut out = BTreeMap::new();
    for (src, t) in &full {
        let own: Vec<(&Pair, &Diff)> = rows.iter().filter(|(p, _)| &p.source == src).copied().collect();
        let mut worst = Sway { db: 0.0, dynamic: String::new(), key: 0 };
        for (p, _) in &own {
            let less: Vec<(&Pair, &Diff)> = own.iter().filter(|(q, _)| !(q.dynamic == p.dynamic && q.strike.key == p.strike.key)).copied().collect();
            let Some(lt) = level_corrections(&less, current, groups).remove(src) else { continue };
            for (d, v) in &t.db {
                for (k, x) in t.keys.iter().zip(v) {
                    let moved = (lt.at(d, *k) - x).abs();
                    if moved > worst.db {
                        worst = Sway { db: moved, dynamic: p.dynamic.clone(), key: p.strike.key as i64 };
                    }
                }
            }
        }
        out.insert(src.clone(), worst);
    }
    out
}

/// the new level corrections per source: per dynamic each recorded key's residual (its level re
/// its source's and dynamic's median; with octave groups re its run's median, so a run's own gain
/// is not copied; with key groups re the key's loudest dynamic and the median step to it, the
/// loudest kept as it is), a running median along the keys, the current correction minus it,
/// clamped and rounded (the running median then a ¼ ½ ¼ pass); a dynamic with fewer than
/// CORRECTION_MIN_KEYS keys keeps none, a source whose files are normalized none at all
pub fn level_corrections(rows: &[(&Pair, &Diff)], current: &BTreeMap<String, LevelTable>, groups: &BTreeMap<String, LevelGroups>)
    -> BTreeMap<String, LevelTable> {
    let ms = medians_by_source(&rows.iter().map(|(p, d)| (p.source.as_str(), p.dynamic.as_str(), *d)).collect::<Vec<_>>());
    let mut out: BTreeMap<String, LevelTable> = BTreeMap::new();
    for (src, m) in &ms {
        let grouping = groups.get(src).copied().unwrap_or(LevelGroups::Whole);
        if grouping == LevelGroups::File {
            continue;
        }
        let mut per_dyn: BTreeMap<String, Vec<(i64, f32)>> = BTreeMap::new();
        let own: Vec<(&Pair, &Diff)> = rows.iter().filter(|(p, _)| &p.source == src).copied().collect();
        // key groups: each key's softer dynamics re its loudest (which keeps no correction)
        let refs = (grouping == LevelGroups::Key).then(|| key_refs(&own));
        for (p, d) in &own {
            let key = p.strike.key as i64;
            let r = match &refs {
                Some(refs) => match refs.get(&(p.dynamic.clone(), key)) { Some(r) => *r, None => continue },
                None => m.level_of(&p.dynamic),
            };
            per_dyn.entry(p.dynamic.clone()).or_default().push((key, d.level_db - r));
        }
        if grouping == LevelGroups::Octave {
            // each run's own gain taken off (its median), then each run offset to carry on where the
            // one below ends (the mean of the two keys either side of the boundary): the slope
            // within a run is followed across the runs, no run's gain nor a step at its boundary
            // (centered alone, a tilt repeated in each run left a sawtooth of 3-5 dB steps)
            for pts in per_dyn.values_mut() {
                pts.sort_by_key(|p| p.0);
                let mut runs: BTreeMap<i64, Vec<(i64, f32)>> = BTreeMap::new();
                for (k, r) in pts.iter() {
                    runs.entry(k.div_euclid(12)).or_default().push((*k, *r));
                }
                for v in runs.values_mut() {
                    let m = median(v.iter().map(|p| p.1).collect()).unwrap_or(0.0);
                    v.iter_mut().for_each(|p| p.1 -= m);
                }
                let edge = |v: &[(i64, f32)]| v.iter().map(|p| p.1).sum::<f32>() / v.len().max(1) as f32;
                let mut offset: BTreeMap<i64, f32> = BTreeMap::new();
                let mut prev: Option<(f32, &Vec<(i64, f32)>)> = None;
                for (o, v) in &runs {
                    let off = prev.map_or(0.0, |(po, pv)| po + edge(&pv[pv.len().saturating_sub(2)..]) - edge(&v[..v.len().min(2)]));
                    offset.insert(*o, off);
                    prev = Some((off, v));
                }
                let mid = median(offset.values().copied().collect()).unwrap_or(0.0);
                *pts = runs.iter().flat_map(|(o, v)| { let off = offset[o] - mid; v.iter().map(move |(k, r)| (*k, *r + off)) }).collect();
            }
        }
        let mut keys: Vec<i64> = per_dyn.values().flatten().map(|(k, _)| *k).collect();
        keys.sort();
        keys.dedup();
        let cur = current.get(src);
        let mut db = BTreeMap::new();
        for (dynamic, mut pts) in per_dyn {
            pts.sort_by_key(|p| p.0);
            pts.dedup_by_key(|p| p.0);
            if pts.len() < CORRECTION_MIN_KEYS {
                continue;
            }
            let smooth: Vec<(i64, f32)> = (0..pts.len()).map(|i| {
                let lo = i.saturating_sub(CORRECTION_NEIGHBORS);
                let hi = (i + CORRECTION_NEIGHBORS + 1).min(pts.len());
                (pts[i].0, median(pts[lo..hi].iter().map(|p| p.1).collect()).unwrap_or(0.0))
            }).collect();
            // then ¼ ½ ¼ over the neighbors (the ends: their own and one): a run played with
            // alternating sticking, every other key louder, kept its alternation through the
            // median (three of five of one kind)
            let smooth: Vec<(i64, f32)> = (0..smooth.len()).map(|i| {
                let (a, b) = (smooth[i.saturating_sub(1)].1, smooth[(i + 1).min(smooth.len() - 1)].1);
                (smooth[i].0, 0.25 * a + 0.5 * smooth[i].1 + 0.25 * b)
            }).collect();
            let at = |k: i64| {
                // the smoothed residual at any table key: linear between the dynamic's keys, held
                let t = LevelTable { keys: smooth.iter().map(|p| p.0).collect(), db: [(dynamic.clone(), smooth.iter().map(|p| p.1).collect())].into() };
                t.at(&dynamic, k)
            };
            let v: Vec<f32> = keys.iter().map(|k| {
                let now = cur.map_or(0.0, |c| c.at(&dynamic, *k));
                ((now - at(*k)).clamp(-CORRECTION_LIMIT_DB, CORRECTION_LIMIT_DB) * 10.0).round() / 10.0
            }).collect();
            db.insert(dynamic, v);
        }
        if !db.is_empty() {
            out.insert(src.clone(), LevelTable { keys, db });
        }
    }
    out
}
