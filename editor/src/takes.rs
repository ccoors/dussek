//! The take checks: each take of a source gets three numbers (its decay against the neighboring
//! keys at its layer, a late rise above its decay's line, its brightness against its key's other
//! layers), each scored in robust spreads above the source's median, so one default serves every
//! kind of instrument. A take is flagged where a score passes its threshold; nothing is left out
//! by the checks (the Check page's "Leave this take out" does that, by ear).
use crate::audio::{ENV_HOP_S, Measure, Measures, band_hz};
use crate::mapping::Item;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

/// one take to check: a mapped item, or a spare take of one (scored against the mapped takes)
pub struct TakeInput<'a> {
    pub relpath: &'a str,
    pub note: i64,
    pub layer: i64,
    pub f0: f32,
    pub tail_cut_s: f32,
    pub spare: bool,
    pub m: Option<&'a Measure>,
}

/// a take's own numbers (none where its recording is too short for one)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Numbers {
    pub slope_db_s: Option<f32>,
    pub rise_db: Option<f32>,
    pub rise_at_s: Option<f32>,
    pub tilt_db: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Check {
    Decay,
    Rise,
    Brightness,
}

/// a check's result: `value` the take's own number (decay: its slope minus its neighbors' median,
/// dB/s; rise: dB above its decay's line; brightness: dB above the line through its key's other
/// layers), `score` how many robust spreads its size lies above the source's median
#[derive(Clone, Debug, PartialEq)]
pub struct Scored {
    pub check: Check,
    pub value: f32,
    pub at_s: Option<f32>,
    pub score: f32,
}

/// the decay's line: 0.1–1.5 s after the onset
const SLOPE_FROM_S: f32 = 0.1;
const SLOPE_TO_S: f32 = 1.5;
/// the late rise: from 1 s on, at least 20 frames, the residuals smoothed over 5 frames (0.25 s)
const RISE_FROM_S: f32 = 1.0;
const RISE_MIN_FRAMES: usize = 20;
const RISE_SMOOTH: usize = 5;
/// the Theil–Sen line through at most this many frames
const RISE_PAIRS_FRAMES: usize = 60;
/// a rise counts as far as the level climbs above its own lowest over this long before (frames):
/// a decay that falls fast, then slowly lies above the line through its tail but never rises
/// (14 of 67 rise flags at the first defaults sat at 1.1 s)
const RISE_LOOKBACK: usize = 10;
/// the lowest level a rise is read from, dBFS (about a 16-bit step): a file that falls into
/// digital silence and then blips by a step rose 17–23 dB inaudibly (Grand F's D4 v2)
const RISE_LEVEL_FLOOR_DB: f32 = -90.0;
/// the neighboring keys on each side a decay is compared with (at least two in all)
const NEIGHBORS: usize = 2;
/// the other layers of its key a take's brightness is judged against: with two, the line through
/// them is extrapolated to an outer layer (Grand C's ranks 1–3 for layers 1, 2 and 5 flagged a
/// tenth of its takes)
const BRIGHTNESS_OTHERS: usize = 3;
/// the smallest spread a score is taken in: a very even source does not flag its smallest outliers
pub const DECAY_FLOOR: f32 = 2.0;
pub const RISE_FLOOR: f32 = 2.0;
pub const BRIGHTNESS_FLOOR: f32 = 2.0;

pub fn numbers(m: &Measure, f0: f32, tail_cut_s: f32) -> Numbers {
    let env = &m.env_db;
    let t = |i: usize| i as f32 * ENV_HOP_S;
    let mut out = Numbers::default();
    // the decay: a least-squares line over 0.1–1.5 s
    let pts: Vec<(f32, f32)> = (0..env.len()).filter(|&i| t(i) >= SLOPE_FROM_S && t(i) <= SLOPE_TO_S).map(|i| (t(i), env[i])).collect();
    if pts.last().is_some_and(|p| p.0 >= SLOPE_TO_S - ENV_HOP_S) && pts.len() >= 3 {
        out.slope_db_s = Some(line(&pts).0);
    }
    // the late rise: a robust line from 1 s to what the analysis reads, the largest smoothed residual
    let env: Vec<f32> = env.iter().map(|v| v.max(RISE_LEVEL_FLOOR_DB)).collect();
    let end = env.len().saturating_sub((tail_cut_s.max(0.0) / ENV_HOP_S).round() as usize);
    let from = (RISE_FROM_S / ENV_HOP_S).round() as usize;
    if end > from && end - from >= RISE_MIN_FRAMES {
        let idx: Vec<usize> = (from..end).collect();
        let pick: Vec<usize> = (0..RISE_PAIRS_FRAMES.min(idx.len()))
            .map(|k| idx[k * (idx.len() - 1) / (RISE_PAIRS_FRAMES.min(idx.len()) - 1).max(1)]).collect();
        let mut slopes = Vec::new();
        for (a, &i) in pick.iter().enumerate() {
            for &j in &pick[a + 1..] {
                if j > i {
                    slopes.push((env[j] - env[i]) / (t(j) - t(i)));
                }
            }
        }
        let s = median(&mut slopes).unwrap_or(0.0);
        let b = median(&mut idx.iter().map(|&i| env[i] - s * t(i)).collect::<Vec<_>>()).unwrap_or(0.0);
        let res: Vec<f32> = idx.iter().map(|&i| env[i] - (s * t(i) + b)).collect();
        let h = RISE_SMOOTH / 2;
        // the level smoothed as the residuals are, over the whole envelope (the look-back reaches
        // before the window)
        let lvl: Vec<f32> = (0..env.len()).map(|i| {
            let (lo, hi) = (i.saturating_sub(h), (i + h + 1).min(env.len()));
            env[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
        }).collect();
        let (mut best, mut at) = (f32::MIN, 0.0);
        for k in h..res.len().saturating_sub(h) {
            let above = res[k - h..=k + h].iter().sum::<f32>() / RISE_SMOOTH as f32;
            let i = idx[k];
            let climbed = lvl[i] - lvl[i.saturating_sub(RISE_LOOKBACK)..i].iter().copied().fold(f32::INFINITY, f32::min);
            let v = above.min(climbed);
            if v > best {
                best = v;
                at = t(i);
            }
        }
        if best > f32::MIN {
            out.rise_db = Some(best);
            out.rise_at_s = Some(at);
        }
    }
    // the brightness: the partials from the 4th up against the first two and a half
    let band = |lo: f32, hi: f32| -> Option<f32> {
        let p: f32 = (0..m.bands_db.len()).filter(|&k| band_hz(k) >= lo && band_hz(k) < hi).map(|k| 10f32.powf(m.bands_db[k] / 10.0)).sum();
        (p > 0.0).then(|| 10.0 * p.log10())
    };
    if f0 > 0.0 {
        if let (Some(hi), Some(lo)) = (band(4.0 * f0, (16.0 * f0).min(15000.0)), band(0.7 * f0, 2.5 * f0)) {
            out.tilt_db = Some(hi - lo);
        }
    }
    out
}

/// (slope, intercept) of the least-squares line through `pts`
fn line(pts: &[(f32, f32)]) -> (f32, f32) {
    let n = pts.len() as f32;
    let (mx, my) = (pts.iter().map(|p| p.0).sum::<f32>() / n, pts.iter().map(|p| p.1).sum::<f32>() / n);
    let var: f32 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if var == 0.0 {
        return (0.0, my);
    }
    let s = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f32>() / var;
    (s, my - s * mx)
}

fn median(v: &mut [f32]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 { v[m] } else { 0.5 * (v[m - 1] + v[m]) })
}

/// what the scores of `takes` read, hashed: each take's file, key, layer, pitch, tail cut and
/// measurement (the Check page scores again when it changes; counts alone missed a relabel)
pub fn inputs_key(takes: &[TakeInput]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for t in takes {
        (t.relpath, t.note, t.layer, t.f0.to_bits(), t.tail_cut_s.to_bits(), t.spare).hash(&mut h);
        if let Some(m) = t.m {
            m.env_db.iter().chain(&m.bands_db).for_each(|v| v.to_bits().hash(&mut h));
        }
    }
    h.finish()
}

/// every take's checks, scored against the source's mapped takes (keyed by relpath)
pub fn score(takes: &[TakeInput]) -> BTreeMap<String, Vec<Scored>> {
    let nums: Vec<Numbers> = takes.iter().map(|t| t.m.map(|m| numbers(m, t.f0, t.tail_cut_s)).unwrap_or_default()).collect();
    let mapped: Vec<usize> = (0..takes.len()).filter(|&i| !takes[i].spare).collect();
    // the raw values per check (signed), each with its time where it has one
    let mut values: Vec<Vec<(Check, f32, Option<f32>)>> = vec![Vec::new(); takes.len()];
    for i in 0..takes.len() {
        let t = &takes[i];
        // the decay against the nearest mapped keys at its layer (its own key left out)
        if let Some(s) = nums[i].slope_db_s {
            let mut same: Vec<(i64, f32)> = mapped.iter().filter(|&&j| takes[j].layer == t.layer && takes[j].note != t.note)
                .filter_map(|&j| nums[j].slope_db_s.map(|v| (takes[j].note, v))).collect();
            same.sort_by_key(|p| p.0);
            let below: Vec<f32> = same.iter().rev().filter(|p| p.0 < t.note).take(NEIGHBORS).map(|p| p.1).collect();
            let above: Vec<f32> = same.iter().filter(|p| p.0 > t.note).take(NEIGHBORS).map(|p| p.1).collect();
            let mut nb: Vec<f32> = below.into_iter().chain(above).collect();
            if nb.len() >= 2 {
                values[i].push((Check::Decay, s - median(&mut nb).unwrap(), None));
            }
        }
        if let Some(r) = nums[i].rise_db {
            values[i].push((Check::Rise, r, nums[i].rise_at_s));
        }
        // the brightness against the line through its key's other mapped layers
        if let Some(b) = nums[i].tilt_db {
            let others: Vec<(f32, f32)> = mapped.iter()
                .filter(|&&j| j != i && takes[j].note == t.note && takes[j].layer != t.layer)
                .filter_map(|&j| nums[j].tilt_db.map(|v| (takes[j].layer as f32, v))).collect();
            if others.len() >= BRIGHTNESS_OTHERS {
                let (s, c) = line(&others);
                values[i].push((Check::Brightness, b - (s * t.layer as f32 + c), None));
            }
        }
    }
    let mut out = BTreeMap::new();
    for check in [Check::Decay, Check::Rise, Check::Brightness] {
        let floor = match check { Check::Decay => DECAY_FLOOR, Check::Rise => RISE_FLOOR, Check::Brightness => BRIGHTNESS_FLOOR };
        let mut sizes: Vec<f32> = mapped.iter().flat_map(|&j| values[j].iter().filter(|v| v.0 == check).map(|v| v.1.abs())).collect();
        let Some(med) = median(&mut sizes.clone()) else { continue };
        let mut dev: Vec<f32> = sizes.iter_mut().map(|v| (*v - med).abs()).collect();
        let spread = (1.4826 * median(&mut dev).unwrap_or(0.0)).max(floor);
        for (i, t) in takes.iter().enumerate() {
            for &(c, v, at) in values[i].iter().filter(|v| v.0 == check) {
                out.entry(t.relpath.to_string()).or_insert_with(Vec::new)
                    .push(Scored { check: c, value: v, at_s: at, score: (v.abs() - med) / spread });
            }
        }
    }
    out
}

/// how many robust spreads above the source's median flag a take, per check
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thresholds {
    pub decay: f32,
    pub rise: f32,
    pub brightness: f32,
}

/// the project table's keys
const KEYS: [(&str, Check); 3] = [("decay", Check::Decay), ("rise", Check::Rise), ("brightness", Check::Brightness)];
/// a threshold's range (the sliders')
pub const THRESHOLD_MIN: f32 = 1.0;
pub const THRESHOLD_MAX: f32 = 20.0;

impl Thresholds {
    pub const DEFAULT: Thresholds = Thresholds { decay: 4.5, rise: 4.5, brightness: 4.0 };

    pub fn of(&self, c: Check) -> f32 {
        match c {
            Check::Decay => self.decay,
            Check::Rise => self.rise,
            Check::Brightness => self.brightness,
        }
    }

    fn set(&mut self, c: Check, v: f32) {
        match c {
            Check::Decay => self.decay = v,
            Check::Rise => self.rise = v,
            Check::Brightness => self.brightness = v,
        }
    }

    /// from the project's `[take_checks]` (missing or not a number: the default; clamped)
    pub fn from_project(t: Option<&Json>) -> Thresholds {
        let mut out = Thresholds::DEFAULT;
        for (k, c) in KEYS {
            if let Some(v) = t.and_then(|t| t.get(k)).and_then(Json::as_f64) {
                out.set(c, (v as f32).clamp(THRESHOLD_MIN, THRESHOLD_MAX));
            }
        }
        out
    }

    /// the project's `[take_checks]`: only the thresholds moved from the default (none: no table)
    pub fn to_project(&self) -> Option<Json> {
        let mut o = serde_json::Map::new();
        for (k, c) in KEYS {
            if self.of(c) != Thresholds::DEFAULT.of(c) {
                // the f32's shortest decimal (3.7, not 3.700000047683716)
                o.insert(k.into(), json!(self.of(c).to_string().parse::<f64>().unwrap_or(self.of(c) as f64)));
            }
        }
        (!o.is_empty()).then_some(Json::Object(o))
    }
}

impl Default for Thresholds {
    fn default() -> Thresholds {
        Thresholds::DEFAULT
    }
}

/// a project's `[take_checks]` set to `t` (the table gone where every threshold is the default)
pub fn write_thresholds(p: &mut crate::project::Project, t: &Thresholds) -> Result<(), String> {
    let Some(Json::Object(keys)) = t.to_project() else {
        p.remove_table("take_checks");
        return Ok(());
    };
    // in place: the table keeps its position and comments, a threshold back at its default goes
    let table = crate::project::Table::top("take_checks");
    for (k, _) in KEYS {
        match keys.get(k) {
            Some(v) => p.set(&table, k, crate::project::json_to_toml(v))?,
            None if p.get(&table, k).is_some() => p.remove(&table, k)?,
            None => {}
        }
    }
    Ok(())
}

/// the takes with a check past its threshold: those checks, the strongest first
pub fn flags(scored: &BTreeMap<String, Vec<Scored>>, t: &Thresholds) -> BTreeMap<String, Vec<Scored>> {
    scored.iter().filter_map(|(k, v)| {
        let mut f: Vec<Scored> = v.iter().filter(|s| s.score > t.of(s.check)).cloned().collect();
        f.sort_by(|a, b| b.score.total_cmp(&a.score));
        (!f.is_empty()).then(|| (k.clone(), f))
    }).collect()
}

/// a flag in words, with the take's own number
pub fn describe(s: &Scored) -> String {
    match s.check {
        Check::Decay => format!("decays {:.1} dB/s {} than its neighbors", s.value.abs(), if s.value < 0.0 { "faster" } else { "slower" }),
        Check::Rise => format!("rises {:.0} dB at {:.1} s", s.value, s.at_s.unwrap_or(0.0)),
        Check::Brightness => format!("{:.1} dB {} than its key's other layers", s.value.abs(), if s.value > 0.0 { "brighter" } else { "darker" }),
    }
}

/// the takes of a source to check: its items and their spares (f0 from the key and its measured
/// pitch offset)
pub fn inputs<'a>(items: &'a [Item], spares: &'a BTreeMap<String, Vec<Item>>, m: &'a Measures, offsets: &BTreeMap<String, f32>)
    -> Vec<TakeInput<'a>> {
    let one = |it: &'a Item, spare: bool| -> Option<TakeInput<'a>> {
        let note = it.get("note")?.as_i64()?;
        let path = it.get("path")?.as_str()?;
        let cents = offsets.get(path).copied().unwrap_or(0.0);
        Some(TakeInput {
            relpath: it.get("relpath")?.as_str()?,
            note,
            layer: it.get("layer")?.as_i64()?,
            f0: 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0 + cents / 1200.0),
            tail_cut_s: it.get("tail_cut_s").and_then(Json::as_f64).unwrap_or(0.0) as f32,
            spare,
            m: m.ok.get(path),
        })
    };
    items.iter().filter_map(|it| one(it, false))
        .chain(spares.values().flatten().filter_map(|it| one(it, true))).collect()
}
