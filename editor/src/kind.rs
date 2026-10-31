//! The wizard's guess from the recordings it measured: which answer to "How is it played?" to
//! pre-select, and which donor of an answer is the closest (bars: their overtones; strings: their
//! key range). It reads the files the rule maps as notes, one per key.
use crate::audio::{Measures, midi_hz, score};
use crate::mapping::Item;
use crate::project::{self, Kind, Project, Repo, Table};
use crate::pyfmt::py_str;
use serde_json::Value as Json;
use std::collections::BTreeMap;

/// a key's file as the guess reads it: its fundamental as measured and its peaks
#[derive(Clone, Debug)]
pub struct Key {
    pub note: i64,
    pub f1: f32,
    pub peaks: Vec<[f32; 2]>,
    pub nyquist: f32,
    /// the fundamental's decay rate (1/s), when measured
    pub sigma: Option<f32>,
}

/// fewer keys than this: no guess
const MIN_KEYS: usize = 3;
/// a key's fundamental: the strongest peak within this of the key's pitch (a = 415 Hz is −100)
const F1_CENTS: f32 = 150.0;
/// an upper mode's peak counts at this level at least (dB re the file's strongest)
const PEAK_DB: f32 = -40.0;
/// a donor within this of the best score ties with it (the fallback wins a tie)
const TIE: f32 = 0.05;
/// a donor's track explains a mode within this: the kind's closeness, not the instrument's (the
/// tracks are derived from the recordings; the donor's own tolerances are its bars': 3–4 % on the
/// bells, 30 % on the xylophone's untuned top, which "explained" any bar set's second mode)
const KIND_TOL: f32 = 0.08;
/// the set's upper modes a key is scored by: its strongest, peaks within 3 % of a stronger one
/// merged; a mode lies 1.3 times the fundamental or more above it
const SET_MODES: usize = 4;
/// a key's fundamental decays as the donor's when within this factor (steel bars 0.5–1.5/s at
/// C6, wooden ones 15–18/s)
const DECAY_FACTOR: f32 = 3.0;

/// one file per key (the layer used as mf, else the middle layer), with the fundamental found
/// near the key's pitch (keys without one are left out)
pub fn sample_keys(items: &[Item], m: &Measures, mf: Option<&Json>) -> Vec<Key> {
    let mut by_note: BTreeMap<i64, Vec<&Item>> = BTreeMap::new();
    for it in items {
        if let Some(n) = it["note"].as_i64() {
            by_note.entry(n).or_default().push(it);
        }
    }
    let mut out = Vec::new();
    for (note, its) in by_note {
        let pick = mf.and_then(|v| its.iter().find(|it| py_str(&it["dynamic"]) == py_str(v) || it["layer"] == *v))
            .or_else(|| its.iter().min_by_key(|it| {
                let (l, n) = (it["layer"].as_i64().unwrap_or(1), it["layers"].as_i64().unwrap_or(1));
                (2 * l - (n + 1)).abs()
            }));
        let Some(me) = pick.and_then(|it| m.ok.get(it["path"].as_str()?)) else { continue };
        let target = midi_hz(note as f64);
        let f1 = me.peaks.iter().filter(|p| (1200.0 * (p[0] / target).log2()).abs() <= F1_CENTS)
            .max_by(|a, b| a[1].total_cmp(&b[1])).map(|p| p[0]);
        if let Some(f1) = f1 {
            let sigma = me.decays.iter().find(|d| (1200.0 * (d[0] / f1).log2()).abs() <= 40.0).map(|d| d[1]);
            out.push(Key { note, f1, peaks: me.peaks.clone(), nyquist: me.nyquist, sigma });
        }
    }
    out
}

/// a track's nominal ratio at a key (proto/bars.py's nominal): a number, or [key, ratio] anchors
/// interpolated in log ratio, held beyond them
pub fn nominal(track: &Json, note: i64) -> f32 {
    if let Some(r) = track.as_f64() {
        return r as f32;
    }
    let pts: Vec<(f64, f64)> = track.as_array().into_iter().flatten()
        .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?.ln()))).collect();
    let n = note as f64;
    let Some(first) = pts.first() else { return 1.0 };
    if n <= first.0 {
        return first.1.exp() as f32;
    }
    for w in pts.windows(2) {
        if n <= w[1].0 {
            let t = (n - w[0].0) / (w[1].0 - w[0].0);
            return (w[0].1 + t * (w[1].1 - w[0].1)).exp() as f32;
        }
    }
    pts.last().unwrap().1.exp() as f32
}

/// a profile explains a key's peaks this well at least (the pitch check's threshold)
const TELLS: f32 = 0.5;
/// a key no profile explains: an upper peak this loud at least (dB re the strongest) and this far
/// from every whole-number ratio (cents) is a bar's mode
const LOOSE_DB: f32 = -30.0;
const LOOSE_CENTS: f32 = 40.0;

/// per key that tells: whether it is a bar's. Its profile where one explains it; else a strong
/// upper peak between the harmonics makes it a bar's (the crotales' 2.15 and 3.75), upper peaks
/// at whole numbers with the octave among them a string's (a string's treble: the fundamental and
/// a weak octave, which a bar's three-partial profile explained least poorly: the grands' keys
/// counted as bars, their shares up to 0.45); whole numbers without the octave (a xylophone's
/// tuned 3 and 6) and no upper peak tell nothing
fn told(keys: &[Key]) -> Vec<bool> {
    keys.iter().filter_map(|k| {
        let (s, p) = score(&k.peaks, k.f1, k.nyquist);
        if s >= TELLS {
            return Some(p > 0);
        }
        let limit = (0.9 * k.nyquist).min(16000.0);
        let ups: Vec<f32> = k.peaks.iter().filter(|q| q[1] >= LOOSE_DB && q[0] >= 1.3 * k.f1 && q[0] <= limit)
            .map(|q| q[0] / k.f1).collect();
        let off = |r: f32, n: f32| (1200.0 * (r / n).log2()).abs();
        if ups.iter().any(|r| off(*r, r.round()) > LOOSE_CENTS) {
            return Some(true);
        }
        ups.iter().any(|r| off(*r, 2.0) <= LOOSE_CENTS).then_some(false)
    }).collect()
}

/// the share of the keys that tell whose overtones are a bar's (0 when none tells)
pub fn bar_share(keys: &[Key]) -> f32 {
    let t = told(keys);
    if t.is_empty() {
        return 0.0;
    }
    t.iter().filter(|b| **b).count() as f32 / t.len() as f32
}

/// a key's upper modes: its strongest peaks (PEAK_DB or more) from 1.3 times the fundamental up
/// to the profile's limit, peaks within 3 % of a stronger one merged, strongest first
fn upper_modes(k: &Key) -> Vec<f32> {
    let limit = (0.9 * k.nyquist).min(16000.0);
    let mut out: Vec<f32> = Vec::new();
    for p in &k.peaks {
        if p[1] >= PEAK_DB && p[0] >= 1.3 * k.f1 && p[0] <= limit && !out.iter().any(|f| (p[0] / f - 1.0).abs() < 0.03) {
            out.push(p[0]);
        }
        if out.len() == SET_MODES {
            break;
        }
    }
    out
}

/// how well a donor's tracks match the keys' modes (0 … 1), both ways: each key's upper modes
/// explained by a track within KIND_TOL (the strongest weighing most: 1/rank), and the donor's
/// first upper track (its main mode) found among them (weight 1)
pub fn overtone_score(keys: &[Key], tracks: &Json, _tol: &Json) -> f32 {
    let Some(ts) = tracks.as_array() else { return 0.0 };
    let (mut num, mut den) = (0.0f32, 0.0f32);
    for k in keys {
        let modes = upper_modes(k);
        let at = |j: usize| nominal(&ts[j], k.note) * k.f1;
        let close = |a: f32, b: f32| (a / b - 1.0).abs() < KIND_TOL;
        for (i, m) in modes.iter().enumerate() {
            let w = 1.0 / (i + 1) as f32;
            den += w;
            if (1..ts.len()).any(|j| close(*m, at(j))) {
                num += w;
            }
        }
        let limit = (0.9 * k.nyquist).min(16000.0);
        if ts.len() > 1 && at(1) <= limit {
            den += 1.0;
            if modes.iter().any(|m| close(*m, at(1))) {
                num += 1.0;
            }
        }
    }
    if den == 0.0 { 0.0 } else { num / den }
}

/// a donor's fitted fundamental decay rate per key (fits/<donor>.json: keys[].sigma[0])
pub fn donor_decays(repo: &Repo, donor: &str) -> BTreeMap<i64, f32> {
    let path = repo.projects.join(donor).join("fits").join(format!("{donor}.json"));
    let fit: Json = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    fit["keys"].as_array().into_iter().flatten()
        .filter_map(|k| Some((k["note"].as_i64()?, k["sigma"].get(0)?.as_f64()? as f32)))
        .collect()
}

/// the share of the keys whose fundamental decays as the donor's (within DECAY_FACTOR); None when
/// no key's decay was measured
pub fn decay_score(keys: &[Key], decays: &BTreeMap<i64, f32>) -> Option<f32> {
    let pairs: Vec<(f32, f32)> = keys.iter().filter_map(|k| Some((k.sigma?, *decays.get(&k.note)?)))
        .filter(|(a, b)| *a > 0.0 && *b > 0.0).collect();
    if pairs.is_empty() {
        return None;
    }
    let agree = pairs.iter().filter(|(a, b)| (a / b).ln().abs() <= DECAY_FACTOR.ln()).count();
    Some(agree as f32 / pairs.len() as f32)
}

/// a bars donor's tracks and tolerances: its [bars], or with tracks = "auto" its fit's
pub fn donor_tracks(repo: &Repo, donor: &str) -> Option<(Json, Json)> {
    let p = Project::load(repo, donor).ok()?;
    let t = p.get(&Table::top("bars"), "tracks")?;
    if t.as_str() == Some("auto") {
        let fit: Json = serde_json::from_str(&std::fs::read_to_string(repo.projects.join(donor).join("fits").join(format!("{donor}.json"))).ok()?).ok()?;
        return Some((fit["tracks"]["tracks"].clone(), fit["tracks"]["tol"].clone()));
    }
    Some((t, p.get(&Table::top("bars"), "tol")?))
}

/// how close a bars donor is: its overtones' score times the share of keys decaying as its do
pub fn donor_score(repo: &Repo, donor: &str, keys: &[Key]) -> f32 {
    let overtones = donor_tracks(repo, donor).map_or(0.0, |(t, tol)| overtone_score(keys, &t, &tol));
    overtones * decay_score(keys, &donor_decays(repo, donor)).unwrap_or(1.0)
}

/// the keys a donor's export was recorded over (instruments/<donor>/instrument.json)
pub fn recorded_range(repo: &Repo, donor: &str) -> Option<(i64, i64)> {
    let doc: Json = serde_json::from_str(&std::fs::read_to_string(repo.root.join("instruments").join(donor).join("instrument.json")).ok()?).ok()?;
    Some((doc["recorded"][0].as_i64()?, doc["recorded"][1].as_i64()?))
}

/// the closest donor of an answer and why: bars by their overtones, strings by their key range;
/// fewer than MIN_KEYS keys, or a tie: the fallback
pub fn nearest_donor(repo: &Repo, kind: &Kind, keys: &[Key]) -> (String, String) {
    if keys.len() < MIN_KEYS || kind.donors.len() == 1 {
        return (kind.fallback.clone(), "the usual one for this kind".into());
    }
    let bars = project::family(repo, &kind.fallback).as_deref() == Some("bars");
    let (lo, hi) = (keys.first().unwrap().note, keys.last().unwrap().note);
    let scores: Vec<(f32, &String)> = kind.donors.iter().map(|d| {
        let s = if bars {
            donor_score(repo, d, keys)
        } else {
            recorded_range(repo, d).map_or(0.0, |(a, b)| (hi.min(b) - lo.max(a) + 1).max(0) as f32 / (hi - lo + 1) as f32)
        };
        (s, d)
    }).collect();
    let best = scores.iter().map(|x| x.0).fold(0.0f32, f32::max);
    let fallback_ties = scores.iter().any(|(s, d)| **d == kind.fallback && best - s <= TIE);
    let pick = if fallback_ties { kind.fallback.clone() } else { scores.iter().find(|x| x.0 == best).unwrap().1.clone() };
    let why = if bars { "the closest overtones and decay" } else { "the closest key range" };
    (pick, if fallback_ties && best > 0.0 { format!("{why}, the usual one") } else { why.into() })
}

/// what the wizard pre-selects
#[derive(Clone, Debug, PartialEq)]
pub struct Guess {
    pub kind: Option<String>,
    pub why: String,
}

/// a bar's overtones on two thirds of the keys: the bar-like answer whose closest donor explains
/// them best (on a tie, or none explaining them, the earlier); else nothing (a string's are a
/// piano's as much as a harp's)
pub fn preselect(repo: &Repo, kinds: &[Kind], keys: &[Key]) -> Guess {
    if keys.len() < MIN_KEYS {
        return Guess { kind: None, why: "too few keys measured to guess".into() };
    }
    if told(keys).len() < MIN_KEYS {
        return Guess { kind: None, why: "the overtones do not tell a bar from a string: choose how it is played".into() };
    }
    let share = bar_share(keys);
    if share < 1.0 / 3.0 {
        return Guess { kind: None, why: "the overtones are a string's: choose how it is played".into() };
    }
    if share < 2.0 / 3.0 {
        return Guess { kind: None, why: "the overtones are not clearly a bar's: choose how it is played".into() };
    }
    let best = kinds.iter().filter(|k| k.bar_like).map(|k| {
        let (d, _) = nearest_donor(repo, k, keys);
        (donor_score(repo, &d, keys), k)
    }).fold(None, |best: Option<(f32, &Kind)>, x| match best {
        Some(b) if b.0 >= x.0 => Some(b),
        _ => Some(x),
    });
    match best {
        Some((_, k)) => Guess { kind: Some(k.id.clone()), why: "the overtones are a bar's, not a string's".into() },
        None => Guess { kind: None, why: String::new() },
    }
}
