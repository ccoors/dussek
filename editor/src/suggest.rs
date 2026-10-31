//! The suggestion: for a sample folder, a proposed source (its map and fields) and the findings a
//! person should look at. It never maps on its own: the proposal fills the same form a person
//! types into, and the mapper gives the items. Pitch only decides between the readings the names
//! allow; it never names a note by itself.
use crate::audio::{Measures, deviation_cents, midi_hz, score};
use crate::mapping::{self, Item};
use crate::pyfmt::{basename, note_name};
use regex::Regex;
use serde_json::{Map, Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub text: String,
    pub files: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub value: String,
    pub map: Json,
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub source: Json,
    pub findings: Vec<Finding>,
    pub groups: Vec<Group>,
}

/// a folder under data/: the source's root (the set's top folder) and the path below it
#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub root: String,
    pub sub: String,
}

impl Folder {
    /// a folder given relative to data/ ("vcsl/Chordophones/Concert Harp")
    pub fn from_data_path(rel: &str) -> Folder {
        let rel = rel.trim_matches('/');
        match rel.split_once('/') {
            Some((r, s)) => Folder { root: r.into(), sub: s.into() },
            None => Folder { root: rel.into(), sub: String::new() },
        }
    }

    pub fn dir(&self, data: &Path) -> PathBuf {
        if self.sub.is_empty() { data.join(&self.root) } else { data.join(&self.root).join(&self.sub) }
    }
}

/// the mapper's root for a source root (Python: os.path.join(REPO, "data", root))
pub fn data_root(data: &Path, root: &str) -> String {
    format!("{}/{}", data.to_string_lossy().trim_end_matches('/'), root.trim_matches('/'))
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, keep: &dyn Fn(&Path) -> bool) {
    if depth > 12 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            walk(&p, depth + 1, out, keep);
        } else if keep(&p) {
            out.push(p);
        }
    }
}

fn files_under(data: &Path, f: &Folder, keep: &dyn Fn(&Path) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    walk(&f.dir(data), 0, &mut out, keep);
    let root = data.join(&f.root);
    let mut rel: Vec<String> = out.iter()
        .filter_map(|p| p.strip_prefix(&root).ok().map(|r| r.to_string_lossy().into_owned()))
        .collect();
    rel.sort();
    rel
}

/// the audio files under the folder, relative to the root, sorted (hidden names left out)
pub fn audio_files(data: &Path, f: &Folder) -> Vec<String> {
    files_under(data, f, &crate::audio::is_audio)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Class {
    Letters,
    Digits,
    Sep,
}

#[derive(Clone, Debug, PartialEq)]
struct Token {
    class: Class,
    text: String,
}

fn class_of(c: char) -> Class {
    if "_-. /".contains(c) {
        Class::Sep
    } else if c.is_ascii_digit() {
        Class::Digits
    } else {
        Class::Letters
    }
}

fn tokens(s: &str) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    for c in s.chars() {
        let k = class_of(c);
        match out.last_mut() {
            Some(t) if t.class == k => t.text.push(c),
            _ => out.push(Token { class: k, text: c.to_string() }),
        }
    }
    out
}

pub fn tokenize(s: &str) -> Vec<String> {
    tokens(s).into_iter().map(|t| t.text).collect()
}

#[derive(Clone, Debug, PartialEq)]
enum Role {
    Literal,
    Note,
    /// the digits of a note name (the Note column emits both)
    Inside,
    Number,
    DynWords(Vec<String>),
    DynNumber(String),
    Take,
    WildDigits,
    WildLetters,
    Grouping(Vec<String>),
    Unset,
}

const DYN_PREFIXES: [&str; 9] = ["v", "vl", "l", "vel", "velocity", "dyn", "layer", "level", "lvl"];
const TAKE_PREFIXES: [&str; 6] = ["rr", "r", "take", "t", "seq", "alt"];
static NOTE_LETTERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Ga-g][#bs]?$").unwrap());

fn dyn_rank(w: &str) -> Option<usize> {
    let w = w.to_ascii_lowercase();
    // ppp … fff (the marks or the Italian words), then soft < medium/med < loud/hard
    match w.as_str() {
        "ppp" | "pianississimo" => Some(0), "pp" | "pianissimo" => Some(1), "p" | "piano" => Some(2),
        "mp" | "mezzopiano" => Some(3), "mf" | "mezzoforte" => Some(4), "f" | "forte" => Some(5),
        "ff" | "fortissimo" => Some(6), "fff" | "fortississimo" => Some(7),
        "soft" => Some(10), "medium" | "med" => Some(11), "loud" | "hard" => Some(12),
        _ => None,
    }
}

/// the beginning and the end every word shares (each word keeps something between them)
fn affixes(words: &[String]) -> (String, String) {
    let Some(first) = words.first() else { return Default::default() };
    let mut pre: String = first.clone();
    for w in words {
        while !w.starts_with(&pre) {
            pre.pop();
        }
    }
    let rest: Vec<&str> = words.iter().map(|w| &w[pre.len()..]).collect();
    let mut suf: String = rest[0].to_string();
    for w in &rest {
        while !w.ends_with(&suf) {
            suf.remove(0);
        }
    }
    if rest.iter().any(|w| w.len() <= suf.len()) {
        return Default::default();
    }
    (pre, suf)
}

fn finding(text: impl Into<String>, files: Vec<String>) -> Finding {
    Finding { text: text.into(), files }
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}

/// one file's tokens with its relpath and its measured level and peaks
struct File<'a> {
    rel: String,
    toks: Vec<Token>,
    level: Option<f32>,
    m: Option<&'a crate::audio::Measure>,
}

pub fn dynamics_for(layers: &[String]) -> Json {
    match layers.len() {
        0 => json!({}),
        1 => json!({"mf": layers[0]}),
        2 => json!({"mf": layers[0], "ff": layers[1]}),
        n => json!({"pp": layers[0], "mf": layers[(n - 1) / 2], "ff": layers[n - 1]}),
    }
}

/// the levels' order of a words column: Some(words, softest first) when the levels separate them
/// (spread ≥ 3 dB) and order them alike on ≥ 75 % of the keys
fn level_order(files: &[File], col: usize, key_col: Option<usize>) -> Option<Vec<String>> {
    let key = |f: &File| key_col.map(|k| f.toks[k].text.clone()).unwrap_or_default();
    let mut by_key: BTreeMap<String, Vec<(String, f32)>> = BTreeMap::new();
    for f in files {
        by_key.entry(key(f)).or_default().push((f.toks[col].text.clone(), f.level?));
    }
    let mut dev: BTreeMap<String, Vec<f32>> = BTreeMap::new();
    for v in by_key.values() {
        let mean = v.iter().map(|x| x.1).sum::<f32>() / v.len() as f32;
        for (w, l) in v {
            dev.entry(w.clone()).or_default().push(l - mean);
        }
    }
    let means: BTreeMap<String, f32> = dev.into_iter().map(|(w, d)| (w, d.iter().sum::<f32>() / d.len() as f32)).collect();
    let (lo, hi) = means.values().fold((f32::MAX, f32::MIN), |(a, b), &m| (a.min(m), b.max(m)));
    if hi - lo < 3.0 {
        return None;
    }
    let mut order: Vec<String> = means.keys().cloned().collect();
    order.sort_by(|a, b| means[a].total_cmp(&means[b]));
    let rank: HashMap<&String, usize> = order.iter().enumerate().map(|(i, w)| (w, i)).collect();
    let (mut same, mut n) = (0, 0);
    for v in by_key.values().filter(|v| v.len() >= 2) {
        let mut s = v.clone();
        s.sort_by(|a, b| a.1.total_cmp(&b.1));
        n += 1;
        if s.windows(2).all(|w| rank[&w[0].0] <= rank[&w[1].0]) {
            same += 1;
        }
    }
    (n > 0 && same as f32 >= 0.75 * n as f32).then_some(order)
}

/// a reading of the note column: the map's `note` table and each file's note under it
fn readings(files: &[File], col: usize, number: bool) -> Vec<(Json, Vec<Option<i64>>)> {
    // a note name spans its letters and the octave's digit (the next column)
    let vals: Vec<String> = files.iter()
        .map(|f| if number { f.toks[col].text.clone() } else { format!("{}{}", f.toks[col].text, f.toks[col + 1].text) })
        .collect();
    if !number {
        return [0i64, 12, -12, 24, 36].iter().map(|&s| {
            let spec = if s == 0 { json!({"group": "note"}) } else { json!({"group": "note", "shift": s}) };
            let notes = vals.iter().map(|v| note_name(v).ok().map(|n| n + s)).collect();
            (spec, notes)
        }).collect();
    }
    let nums: Vec<i64> = vals.iter().filter_map(|v| v.parse().ok()).collect();
    let (lo, hi) = (nums.iter().copied().min().unwrap_or(0), nums.iter().copied().max().unwrap_or(0));
    let mut offsets: Vec<i64> = ((-lo)..=(127 - hi)).collect();
    offsets.sort_by_key(|o| o.abs());
    offsets.into_iter().map(|o| {
        let spec = if o == 0 { json!({"group": "midi", "style": "midi"}) } else { json!({"group": "index", "style": "index", "offset": o}) };
        let notes = vals.iter().map(|v| v.parse::<i64>().ok().map(|n| n + o)).collect();
        (spec, notes)
    }).collect()
}

/// the set's tuning: the median distance of the files' strongest partials from the semitones of
/// a = 440 Hz (±50 cents; 0 within ±20 cents)
fn set_tuning(files: &[File], sample: &[usize]) -> f32 {
    let devs: Vec<f32> = sample.iter().filter_map(|&i| {
        let p = files[i].m?.peaks.first()?;
        let c = 1200.0 * (p[0] / 440.0).log2();
        Some(c - 100.0 * (c / 100.0).round())
    }).collect();
    let t = median(devs).unwrap_or(0.0);
    if t.abs() <= 20.0 { 0.0 } else { t }
}

/// the reading that explains the spectra best (the plain one when nothing is measured, or when
/// no profile explains them: a median score below 0.5)
fn choose_reading(files: &[File], col: usize, number: bool, findings: &mut Vec<Finding>) -> (Json, Vec<Option<i64>>) {
    let rs = readings(files, col, number);
    if rs.is_empty() {
        findings.push(finding("the numbers span more than the 128 MIDI keys: no MIDI key reading fits them all \
            (are they takes?)", Vec::new()));
        return (json!({"group": "midi", "style": "midi"}), vec![None; files.len()]);
    }
    let step = (files.len() / 200).max(1);
    let sample: Vec<usize> = (0..files.len()).step_by(step).filter(|&i| files[i].m.is_some()).collect();
    if sample.is_empty() {
        findings.push(finding("pitch not measured: the plain reading of the names is taken", Vec::new()));
        return rs.into_iter().next().unwrap();
    }
    // (score, |tuning|, index, tuning)
    // the set's tuning; names fix the semitone, so a name reading may also sit a semitone off
    // (a = 415 Hz: −100 cents); a number reading's offset takes whole semitones itself
    let t0 = set_tuning(files, &sample);
    let tunings: Vec<f32> = if number { vec![t0] } else { vec![t0, t0 - 100.0, t0 + 100.0] };
    let sample = &sample;
    let tunings = &tunings;
    let mut scored: Vec<(f32, f32, usize, f32)> = rs.iter().enumerate().flat_map(|(k, (_, notes))| {
        tunings.iter().map(move |&t| {
            let detune = 2f32.powf(t / 1200.0);
            let s: Vec<f32> = sample.iter().filter_map(|&i| {
                let m = files[i].m?;
                Some(score(&m.peaks, midi_hz(notes[i]? as f64) * detune, m.nyquist).0)
            }).collect();
            (median(s).unwrap_or(0.0), t.abs(), k, t)
        }).collect::<Vec<_>>()
    }).collect();
    // the best score; within 1 % of it the smaller tuning offset, then the plainer reading
    let top = scored.iter().map(|x| x.0).fold(0.0f32, f32::max);
    scored.sort_by(|a, b| {
        let (na, nb) = (a.0 >= 0.99 * top, b.0 >= 0.99 * top);
        nb.cmp(&na).then(if na && nb { a.1.total_cmp(&b.1).then(a.2.cmp(&b.2)) } else { b.0.total_cmp(&a.0) })
    });
    let (best, _, k, t) = scored[0];
    if best < 0.5 {
        findings.push(finding(format!("the pitch does not settle the octave (the best reading scores {best:.2} \
            of 1): the plain reading of the names is kept"), Vec::new()));
        return rs.into_iter().next().unwrap();
    }
    if let Some(&(second, _, j, _)) = scored.iter().find(|x| x.2 != k && (x.3 - t).abs() < 50.0) {
        if second > 0.8 * best {
            findings.push(finding(format!("octave uncertain: {} scores {:.2}, {} scores {:.2}",
                rs[k].0, best, rs[j].0, second), Vec::new()));
        }
    }
    if t.abs() > 30.0 {
        findings.push(finding(format!("the set is tuned {t:+.0} cents from a = 440 Hz (a = {:.0} Hz)",
            440.0 * 2f32.powf(t / 1200.0)), Vec::new()));
    }
    rs.into_iter().nth(k).unwrap()
}

pub fn suggest_names(data: &Path, f: &Folder, files: &[String], m: &Measures) -> Suggestion {
    let mut findings = Vec::new();
    let root_dir = data_root(data, &f.root);
    let abs = |rel: &str| format!("{root_dir}/{rel}");
    let dir_prefix = if f.sub.is_empty() { String::new() } else { format!("{}/", f.sub) };
    let (release, notes): (Vec<&String>, Vec<&String>) =
        files.iter().partition(|r| basename(r).to_ascii_lowercase().starts_with("rel"));
    let mut source = Map::new();
    source.insert("role".into(), json!("main"));
    source.insert("root".into(), json!(f.root));
    let glob_dir = |s: &str| glob::Pattern::escape(s);
    if notes.is_empty() {
        findings.push(finding(format!("no audio files ({}) under {}", crate::audio::EXTENSIONS.join(", "),
            f.dir(data).display()), Vec::new()));
        source.insert("levels".into(), json!("recorded"));
        source.insert("dynamics".into(), json!({"mf": "mf"}));
        let glob = if f.sub.is_empty() { "*.wav".to_string() } else { format!("{}/*.wav", glob_dir(&f.sub)) };
        source.insert("map".into(), json!({"kind": "pattern", "glob": glob, "regex": r"(?P<note>[A-G]#?\d)\.wav",
            "note": {"group": "note"}, "dynamic": "mf", "order": "note"}));
        return Suggestion { source: Json::Object(source), findings, groups: Vec::new() };
    }
    if !release.is_empty() {
        source.insert("release".into(), json!({"kind": "rel_files"}));
    }
    let below = |r: &str| r.strip_prefix(&dir_prefix).unwrap_or(r).to_string();
    let multi = notes.iter().any(|r| below(r).contains('/'));
    // the extension most files have
    let mut exts: BTreeMap<String, usize> = BTreeMap::new();
    for r in &notes {
        *exts.entry(r.rsplit_once('.').map(|(_, e)| e.to_string()).unwrap_or_default()).or_default() += 1;
    }
    let ext = exts.iter().max_by_key(|(e, n)| (**n, std::cmp::Reverse((*e).clone()))).map(|(e, _)| e.clone()).unwrap_or_default();
    let other_ext: Vec<String> = notes.iter().filter(|r| !r.ends_with(&format!(".{ext}"))).map(|r| r.to_string()).collect();
    if !other_ext.is_empty() {
        findings.push(finding(format!("{} files are not .{ext}: left out", other_ext.len()), other_ext));
    }
    let stem = |r: &str| {
        let s = if multi { below(r) } else { basename(r).to_string() };
        s.strip_suffix(&format!(".{ext}")).unwrap_or(&s).to_string()
    };
    // the files by shape; the most common one is analyzed
    let mut shapes: BTreeMap<Vec<(Class, String)>, Vec<File>> = BTreeMap::new();
    for r in notes.iter().filter(|r| r.ends_with(&format!(".{ext}"))) {
        let toks = tokens(&stem(r));
        let shape = toks.iter().map(|t| (t.class, if t.class == Class::Sep { t.text.clone() } else { String::new() })).collect();
        let mm = m.ok.get(&abs(r));
        shapes.entry(shape).or_default().push(File { rel: r.to_string(), toks, level: mm.map(|x| x.level_db), m: mm });
    }
    let best_shape = shapes.iter().max_by_key(|(_, v)| v.len()).map(|(k, _)| k.clone()).unwrap();
    let mut others: Vec<String> = shapes.iter().filter(|(k, _)| **k != best_shape)
        .flat_map(|(_, v)| v.iter().map(|f| f.rel.clone())).collect();
    let mut rows = shapes.remove(&best_shape).unwrap();
    let n = best_shape.len();
    // a column of note names on most files (≥ 80 %): the files that break it are of another form
    let is_note = |f: &File, i: usize| NOTE_LETTERS.is_match(&f.toks[i].text) && f.toks[i + 1].text.len() == 1;
    let split = (0..n.saturating_sub(1)).find(|&i| {
        let k = rows.iter().filter(|f| is_note(f, i)).count();
        best_shape[i].0 == Class::Letters && best_shape[i + 1].0 == Class::Digits && k * 5 >= rows.len() * 4 && k < rows.len()
    });
    if let Some(i) = split {
        let (keep, out): (Vec<File>, Vec<File>) = rows.into_iter().partition(|f| is_note(f, i));
        others.extend(out.into_iter().map(|f| f.rel));
        rows = keep;
    }
    if !others.is_empty() {
        others.sort();
        findings.push(finding(format!("{} files have another form of name: left out", others.len()), others));
    }
    let distinct = |i: usize| rows.iter().map(|f| f.toks[i].text.clone()).collect::<BTreeSet<String>>();
    let mut roles = vec![Role::Unset; n];
    for i in 0..n {
        if best_shape[i].0 == Class::Sep || distinct(i).len() == 1 {
            roles[i] = Role::Literal;
        }
    }
    // the note: a name (letters, then one digit), else the number with the most values
    let mut note_col = None;
    for i in 0..n.saturating_sub(1) {
        if best_shape[i].0 == Class::Letters && best_shape[i + 1].0 == Class::Digits
            && (roles[i] == Role::Unset || roles[i + 1] == Role::Unset)
            && distinct(i).iter().all(|v| NOTE_LETTERS.is_match(v))
            && distinct(i + 1).iter().all(|v| v.len() == 1)
        {
            roles[i] = Role::Note;
            roles[i + 1] = Role::Inside;
            note_col = Some(i);
            break;
        }
    }
    let mut number = false;
    if note_col.is_none() {
        let best = (0..n).filter(|&i| roles[i] == Role::Unset && best_shape[i].0 == Class::Digits)
            .max_by_key(|&i| (distinct(i).len(), std::cmp::Reverse(i)));
        if let Some(i) = best.filter(|&i| distinct(i).len() >= 4) {
            roles[i] = Role::Number;
            note_col = Some(i);
            number = true;
        }
    }
    let mut dyn_col: Option<usize> = None;
    let mut take_col: Option<usize> = None;
    // numbers after a prefix: a layer or a take
    for i in 1..n {
        if roles[i] != Role::Unset || best_shape[i].0 != Class::Digits || best_shape[i - 1].0 != Class::Letters || roles[i - 1] != Role::Literal {
            continue;
        }
        let prefix = rows[0].toks[i - 1].text.to_ascii_lowercase();
        if dyn_col.is_none() && DYN_PREFIXES.contains(&prefix.as_str()) {
            roles[i] = Role::DynNumber(rows[0].toks[i - 1].text.clone());
            dyn_col = Some(i);
        } else if take_col.is_none() && TAKE_PREFIXES.contains(&prefix.as_str()) {
            roles[i] = Role::Take;
            take_col = Some(i);
        }
    }
    // words: known dynamics (as they are, or behind a part all of them share: PedalOffForte),
    // else ordered by level, else a grouping token
    let mut affix: Vec<(String, String)> = vec![Default::default(); n];
    for i in 0..n {
        if roles[i] != Role::Unset || best_shape[i].0 != Class::Letters {
            continue;
        }
        let words: Vec<String> = distinct(i).into_iter().collect();
        let known = |ws: &[String]| ws.iter().all(|w| dyn_rank(w).is_some());
        let (pre, suf) = affixes(&words);
        let stripped: Vec<String> = words.iter().map(|w| w[pre.len()..w.len() - suf.len()].to_string()).collect();
        let (words_dyn, cut) = if known(&words) {
            (Some(words.clone()), (String::new(), String::new()))
        } else if !(pre.is_empty() && suf.is_empty()) && known(&stripped) {
            (Some(stripped), (pre, suf))
        } else {
            (None, Default::default())
        };
        if let (None, Some(mut w)) = (dyn_col, words_dyn) {
            w.sort_by_key(|w| dyn_rank(w));
            roles[i] = Role::DynWords(w);
            affix[i] = cut;
            dyn_col = Some(i);
        } else if words.len() <= 8 {
            match (dyn_col, level_order(&rows, i, note_col)) {
                (None, Some(order)) => {
                    roles[i] = Role::DynWords(order);
                    dyn_col = Some(i);
                }
                _ => roles[i] = Role::Grouping(words),
            }
        } else {
            roles[i] = Role::WildLetters;
        }
    }
    // other numbers: a take where a key and dynamic repeat, else a wildcard
    let key_of = |f: &File| {
        let k = note_col.map(|c| f.toks[c].text.clone()).unwrap_or_default();
        let d = dyn_col.map(|c| f.toks[c].text.clone()).unwrap_or_default();
        (k, d)
    };
    for i in 0..n {
        if roles[i] != Role::Unset {
            continue;
        }
        let mut per: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
        for f in &rows {
            per.entry(key_of(f)).or_default().insert(f.toks[i].text.clone());
        }
        roles[i] = if take_col.is_none() && per.values().any(|s| s.len() > 1) {
            take_col = Some(i);
            Role::Take
        } else {
            Role::WildDigits
        };
    }
    // the reading of the note column
    let (note_spec, _) = match note_col {
        Some(c) => choose_reading(&rows, c, number, &mut findings),
        None => {
            findings.push(finding("no column of the names reads as a note: write the regex by hand", Vec::new()));
            (json!({"group": "note"}), Vec::new())
        }
    };
    let note_group = note_spec["group"].as_str().unwrap_or("note").to_string();
    // the regex, the glob, the groups
    let regex_of = |group_value: Option<&str>| {
        let mut rx = String::new();
        if multi {
            rx.push_str(&regex::escape(&dir_prefix));
        }
        for i in 0..n {
            let t = &rows[0].toks[i].text;
            match &roles[i] {
                Role::Literal => rx.push_str(&regex::escape(t)),
                Role::Note => rx.push_str(r"(?P<note>[A-Ga-g][#bs]?\d)"),
                Role::Inside => {}
                Role::Number => rx.push_str(&format!(r"(?P<{note_group}>\d+)")),
                Role::DynWords(w) => {
                    let mut alts: Vec<String> = w.iter().map(|x| regex::escape(x)).collect();
                    alts.sort_by_key(|a| std::cmp::Reverse(a.len()));
                    let (pre, suf) = &affix[i];
                    rx.push_str(&format!("{}(?P<dynamic>{}){}", regex::escape(pre), alts.join("|"), regex::escape(suf)));
                }
                Role::DynNumber(_) => rx.push_str(r"(?P<dynamic>\d+)"),
                Role::Take => rx.push_str(r"(?P<take>\d+)"),
                Role::WildDigits => rx.push_str(r"\d+"),
                Role::WildLetters => rx.push_str(r"[^/_\-. 0-9]+"),
                Role::Grouping(v) => rx.push_str(&regex::escape(group_value.unwrap_or(&v[0]))),
                Role::Unset => rx.push_str(".+?"),
            }
        }
        rx.push_str(&regex::escape(&format!(".{ext}")));
        rx
    };
    let glob = match (multi, f.sub.is_empty()) {
        (false, true) => format!("*.{ext}"),
        (false, false) => format!("{}/*.{ext}", glob_dir(&f.sub)),
        (true, true) => format!("**/*.{ext}"),
        (true, false) => format!("{}/**/*.{ext}", glob_dir(&f.sub)),
    };
    let (layers, fmt): (Option<Vec<String>>, Option<String>) = match dyn_col.map(|c| &roles[c]) {
        Some(Role::DynWords(w)) => (Some(w.clone()), None),
        Some(Role::DynNumber(prefix)) => {
            let c = dyn_col.unwrap();
            let mut v: Vec<String> = distinct(c).into_iter().collect();
            v.sort_by_key(|x| x.parse::<i64>().unwrap_or(0));
            (Some(v), Some(format!("{prefix}{{}}")))
        }
        _ => (None, None),
    };
    let map_for = |group_value: Option<&str>| {
        let mut map = Map::new();
        map.insert("kind".into(), json!("pattern"));
        map.insert("glob".into(), json!(glob));
        map.insert("regex".into(), json!(regex_of(group_value)));
        if multi {
            map.insert("match".into(), json!("relpath"));
        }
        map.insert("note".into(), note_spec.clone());
        match &layers {
            Some(l) => {
                map.insert("layers".into(), json!(l));
            }
            None => {
                map.insert("dynamic".into(), json!("mf"));
            }
        }
        if let Some(fmt) = &fmt {
            map.insert("dynamic_format".into(), json!(fmt));
        }
        if take_col.is_some() {
            map.insert("prefer".into(), json!(["take"]));
        }
        map.insert("order".into(), json!("note"));
        Json::Object(map)
    };
    let mut groups = Vec::new();
    if let Some(values) = roles.iter().find_map(|r| match r { Role::Grouping(v) => Some(v.clone()), _ => None }) {
        findings.push(finding(format!("the files fall into {} groups by a word of their names: {} (one source per group?)",
            values.len(), values.join(", ")), Vec::new()));
        groups = values.iter().map(|v| Group { value: v.clone(), map: map_for(Some(v)) }).collect();
    }
    source.insert("map".into(), map_for(None));
    let dyn_names: Vec<String> = layers.clone().map(|l| match &fmt {
        Some(f) => l.iter().map(|x| f.replace("{}", x)).collect(),
        None => l,
    }).unwrap_or_else(|| vec!["mf".into()]);
    source.insert("dynamics".into(), dynamics_for(&dyn_names));
    let mut source = Json::Object(source);
    let items = mapping::items(&source, &root_dir).map(|r| r.items).unwrap_or_default();
    let (levels, lf) = level_findings(&items, m);
    findings.extend(lf);
    findings.extend(pitch_findings(&items, m));
    // keys in the file's order: role, root, levels, dynamics, release, map
    let mut ordered = Map::new();
    for k in ["role", "root"] {
        ordered.insert(k.into(), source[k].clone());
    }
    ordered.insert("levels".into(), json!(levels));
    for k in ["dynamics", "release", "map"] {
        if let Some(v) = source.get(k) {
            ordered.insert(k.into(), v.clone());
        }
    }
    source = Json::Object(ordered);
    Suggestion { source, findings, groups }
}

fn level_of(m: &Measures, it: &Item) -> Option<f32> {
    m.ok.get(it["path"].as_str()?).map(|x| x.level_db)
}

/// "normalized" when every layer's median level is within 1 dB of the others; a finding when the
/// level falls with the layer on more than a quarter of the keys
pub fn level_findings(items: &[Item], m: &Measures) -> (String, Vec<Finding>) {
    let mut out = Vec::new();
    let mut by_note: BTreeMap<i64, Vec<(i64, f32, String)>> = BTreeMap::new();
    let mut by_layer: BTreeMap<i64, Vec<f32>> = BTreeMap::new();
    for it in items {
        let Some(l) = level_of(m, it) else { continue };
        let (note, layer) = (it["note"].as_i64().unwrap_or(0), it["layer"].as_i64().unwrap_or(0));
        by_note.entry(note).or_default().push((layer, l, it["relpath"].as_str().unwrap_or_default().to_string()));
        by_layer.entry(layer).or_default().push(l);
    }
    let multi: Vec<&Vec<(i64, f32, String)>> = by_note.values().filter(|v| v.len() >= 2).collect();
    let falling: Vec<String> = multi.iter().filter_map(|v| {
        let mut s = (*v).clone();
        s.sort_by_key(|x| x.0);
        s.windows(2).any(|w| w[1].1 < w[0].1 - 0.5).then(|| s[0].2.clone())
    }).collect();
    if !multi.is_empty() && falling.len() * 4 > multi.len() {
        out.push(finding(format!("the level falls with the layer on {} of {} keys: is the dynamics' order right?",
            falling.len(), multi.len()), falling));
    }
    let medians: Vec<f32> = by_layer.into_values().filter_map(median).collect();
    let levels = if medians.len() >= 2 && medians.iter().cloned().fold(f32::MIN, f32::max) - medians.iter().cloned().fold(f32::MAX, f32::min) < 1.0 {
        out.push(finding("every layer within 1 dB of the others: the set is normalized", Vec::new()));
        "normalized"
    } else {
        "recorded"
    };
    (levels.into(), out)
}

/// per item: its strongest partial's cents off its key, less the set's typical offset
pub fn pitch_offsets(items: &[Item], m: &Measures) -> BTreeMap<String, f32> {
    let raw: Vec<(String, f32)> = items.iter().filter_map(|it| {
        let path = it["path"].as_str()?;
        let me = m.ok.get(path)?;
        let dev = deviation_cents(&me.peaks, midi_hz(it["note"].as_i64()? as f64), me.nyquist)?;
        Some((path.to_string(), dev))
    }).collect();
    let typical = median(raw.iter().map(|x| x.1).collect()).unwrap_or(0.0);
    raw.into_iter().map(|(p, d)| (p, d - typical)).collect()
}

/// the files more than 50 cents off the set's typical offset
pub fn pitch_findings(items: &[Item], m: &Measures) -> Vec<Finding> {
    let off = pitch_offsets(items, m);
    let rel: HashMap<&str, &str> = items.iter()
        .filter_map(|it| Some((it["path"].as_str()?, it["relpath"].as_str()?))).collect();
    let bad: Vec<String> = off.iter().filter(|(_, c)| c.abs() > 50.0)
        .map(|(p, _)| rel.get(p.as_str()).copied().unwrap_or(p).to_string()).collect();
    if bad.is_empty() {
        return Vec::new();
    }
    vec![finding(format!("{} files more than 50 cents off the set's pitch: pitch suspect (mislabeled or mistuned?)", bad.len()), bad)]
}

/// the SFZ files under the folder, relative to the root, sorted
pub fn sfz_files(data: &Path, f: &Folder) -> Vec<String> {
    files_under(data, f, &|p: &Path| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("sfz")))
}

static DEFAULT_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"default_path=(\S*)").unwrap());

pub fn suggest_sfz(data: &Path, f: &Folder, sfz: &str, m: &Measures) -> Result<Suggestion, String> {
    let root_dir = data_root(data, &f.root);
    let path = format!("{root_dir}/{sfz}");
    let text = crate::sfz::expand(Path::new(&path))?;
    let regions: Vec<crate::sfz::Region> = crate::sfz::parse(&text).into_iter().filter(|r| r.contains_key("sample")).collect();
    if regions.is_empty() {
        return Err(format!("{sfz}: no regions with a sample"));
    }
    let values = |op: &str| regions.iter().filter_map(|r| r.get(op).cloned()).collect::<BTreeSet<String>>();
    let mut filter = Vec::new();
    if values("trigger").iter().any(|v| v != "attack") {
        filter.push(json!({"opcode": "trigger", "default": "attack", "in": ["attack"]}));
    }
    if !values("locc64").is_empty() {
        filter.push(json!({"opcode": "locc64", "default": "0", "lt": 64}));
    }
    if values("seq_position").iter().any(|v| v != "1") {
        filter.push(json!({"opcode": "seq_position", "default": "1", "in": ["1"]}));
    }
    if !values("lorand").is_empty() {
        filter.push(json!({"opcode": "lorand", "default": "0", "eq": 0.0}));
    }
    let mut prefix = sfz.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    if let Some(c) = DEFAULT_PATH.captures(&text) {
        prefix.push_str(&c[1].replace('\\', "/"));
    }
    let normpath = regions.iter().any(|r| r["sample"].replace('\\', "/").contains("./"));
    let mut map = Map::new();
    map.insert("kind".into(), json!("sfz"));
    map.insert("files".into(), json!([sfz]));
    if !filter.is_empty() {
        map.insert("filter".into(), json!(filter));
    }
    if !prefix.is_empty() {
        map.insert("prefix".into(), json!(prefix));
    }
    if normpath {
        map.insert("normpath".into(), json!(true));
    }
    let mut findings = Vec::new();
    // several regions on one key and velocity range: the sample folders tell them apart
    let kept: Vec<&crate::sfz::Region> = regions.iter().filter(|r| {
        filter.iter().all(|f| {
            let op = f["opcode"].as_str().unwrap();
            let v = r.get(op).cloned().unwrap_or_else(|| f["default"].as_str().unwrap().to_string());
            if let Some(list) = f.get("in") {
                list.as_array().unwrap().iter().any(|x| x.as_str() == Some(v.as_str()))
            } else if let Some(lt) = f.get("lt") {
                v.parse::<f64>().is_ok_and(|x| x < lt.as_f64().unwrap())
            } else {
                v.parse::<f64>().is_ok_and(|x| x == f["eq"].as_f64().unwrap())
            }
        })
    }).collect();
    let mut slots: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for r in &kept {
        // the key as the mapper takes it (the SFZ default 60 where a region leaves it out)
        let key = (r.get("pitch_keycenter").cloned().unwrap_or_else(|| "60".into()), r.get("lovel").cloned().unwrap_or_default());
        slots.entry(key).or_default().insert(r["sample"].clone());
    }
    let crowded: Vec<&BTreeSet<String>> = slots.values().filter(|s| s.len() > 1).collect();
    if !crowded.is_empty() {
        let folders: BTreeSet<String> = crowded.iter().flat_map(|s| s.iter())
            .filter_map(|s| s.replace('\\', "/").rsplit_once('/').map(|(d, _)| d.rsplit('/').next().unwrap_or(d).to_string()))
            .collect();
        findings.push(finding(format!("{} keys have several regions per velocity range: the sample folders are {} \
            (choose them with folders or prefer_substring)", crowded.len(), folders.into_iter().collect::<Vec<_>>().join(", ")),
            crowded.iter().flat_map(|s| s.iter().cloned()).collect()));
    }
    let mut source = Map::new();
    source.insert("role".into(), json!("main"));
    source.insert("root".into(), json!(f.root));
    source.insert("levels".into(), json!("recorded"));
    if values("trigger").contains("release") {
        source.insert("release".into(), json!({"kind": "sfz", "file": sfz}));
    }
    source.insert("map".into(), Json::Object(map));
    let probe = Json::Object(source.clone());
    let items = match mapping::items(&probe, &root_dir) {
        Ok(r) => r.items,
        Err(e) => {
            // the folders help only where several regions share a key (the finding above)
            let advice = if crowded.is_empty() { "" } else { ": choose the sample folders (folders or prefer_substring)" };
            findings.push(finding(format!("the map gives no items yet ({e}){advice}"), Vec::new()));
            Vec::new()
        }
    };
    // the layer count most keys have
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for it in &items {
        if it["layer"] == 1 {
            *counts.entry(it["layers"].as_i64().unwrap_or(1)).or_default() += 1;
        }
    }
    // the most common count; on a tie the smaller (duplicates inflate a key's count)
    let n = counts.iter().max_by_key(|(n, c)| (**c, std::cmp::Reverse(**n))).map(|(n, _)| *n).unwrap_or(1);
    if counts.len() > 1 {
        findings.push(finding(format!("the keys have between {} and {} layers", counts.keys().next().unwrap(),
            counts.keys().last().unwrap()), Vec::new()));
    }
    let names: Vec<String> = (1..=n).map(|k| format!("{k}/{n}")).collect();
    let (levels, lf) = level_findings(&items, m);
    findings.extend(lf);
    findings.extend(pitch_findings(&items, m));
    let mut ordered = Map::new();
    ordered.insert("role".into(), json!("main"));
    ordered.insert("root".into(), json!(f.root));
    ordered.insert("levels".into(), json!(levels));
    ordered.insert("dynamics".into(), dynamics_for(&names));
    if let Some(r) = source.get("release") {
        ordered.insert("release".into(), r.clone());
    }
    ordered.insert("map".into(), source["map"].clone());
    Ok(Suggestion { source: Json::Object(ordered), findings, groups: Vec::new() })
}

pub fn suggest(data: &Path, f: &Folder, sfz: Option<&str>, m: &Measures) -> Result<Suggestion, String> {
    match sfz {
        Some(s) => suggest_sfz(data, f, s, m),
        None => Ok(suggest_names(data, f, &audio_files(data, f), m)),
    }
}

/// what the suggestion measures: every audio file of a named set; an SFZ set's proposed items
pub fn files_to_measure(data: &Path, f: &Folder, sfz: Option<&str>) -> Vec<PathBuf> {
    let root = data.join(&f.root);
    match sfz {
        None => audio_files(data, f).into_iter().map(|r| root.join(r)).collect(),
        Some(s) => {
            let Ok(sug) = suggest_sfz(data, f, s, &Measures::default()) else { return Vec::new() };
            mapping::items(&sug.source, &data_root(data, &f.root)).map(|r| r.items).unwrap_or_default()
                .iter().filter_map(|it| it["path"].as_str().map(PathBuf::from)).collect()
        }
    }
}

/// a glob's path component as the name it matches, when it is one name: escapes ("[[]", "[*]")
/// read as their character; None for a wildcard
fn unescape_glob(c: &str) -> Option<String> {
    let ch: Vec<char> = c.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '[' && i + 2 < ch.len() && ch[i + 2] == ']' {
            out.push(ch[i + 1]);
            i += 3;
        } else if "*?[".contains(ch[i]) {
            return None;
        } else {
            out.push(ch[i]);
            i += 1;
        }
    }
    Some(out)
}

/// a committed source's folder: its root and the glob's literal directories (SFZ: its first file)
pub fn folder_of(src: &Json) -> Option<(Folder, Option<String>)> {
    let root = src.get("root")?.as_str()?.to_string();
    let map = src.get("map")?;
    match map.get("kind")?.as_str()? {
        "sfz" => Some((Folder { root, sub: String::new() }, Some(map["files"].get(0)?.as_str()?.to_string()))),
        _ => {
            let glob = map.get("glob")?.as_str()?;
            let parts: Vec<&str> = glob.split('/').collect();
            let sub: Vec<String> = parts[..parts.len() - 1].iter().map_while(|p| unescape_glob(p)).collect();
            Some((Folder { root, sub: sub.join("/") }, None))
        }
    }
}
