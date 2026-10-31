//! A rolled piano from its recipe: the references measured (each piano once, kept on disk), the
//! frame's strings blended and its fine structure mixed (kept for the next roll of the same
//! donors), a body from the knobs applied and widened, each register's loudness and the other
//! positions calibrated, the realism check (on request a failing roll pulled back toward the
//! middle), the license. A roll can be cancelled between its stages.
use super::basis::Basis;
use super::body;
use super::calibrate::{calibrate_positions, match_registers};
use super::files::Export;
use super::measure::{self, Measured, Realism};
use super::parts::{blend_strings, fine_structure};
use super::recipe::Recipe;
use crate::instrument::Error;
use crate::mic::MicPosition;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

/// The error of a roll that `Options::cancelled` stopped.
pub const CANCELLED: &str = "roll: cancelled";

fn never() -> bool {
    false
}

pub struct Options<'a> {
    /// a roll whose statistics are outside the references' range pulled back toward the middle
    /// (every knob × 0.75, at most four times): for a random draw; a setting of the user's is built
    /// as set
    pub pull_back: bool,
    /// asked between the stages: true ends the roll (`CANCELLED`)
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options { pull_back: false, cancelled: &never }
    }
}

/// What a roll is measured against.
pub struct References {
    pub basis: Basis,
    pub realism: Realism,
    /// each of the 15 notes' level at velocity 80, the references' mean (dB)
    pub levels: Vec<f64>,
    /// Player minus Recording per octave, the references' median
    pub goal: [f64; 9],
    /// each knob's lowest and highest value over the references (the editor's sliders)
    pub spans: Vec<(f64, f64)>,
}

pub struct Report {
    /// the knobs as built (after any pull-back): warmth, air, the shades
    pub knobs: Vec<f64>,
    pub pulled_back: u32,
    /// what the realism check and the faults still name
    pub failures: Vec<String>,
    pub seconds: f64,
}

/// One reference measured: its body and the realism check's numbers.
#[derive(Clone)]
pub struct Piano {
    pub body: body::Body,
    pub measured: Measured,
}

impl Piano {
    pub fn measure(e: &Export) -> Result<Piano, Error> {
        Ok(Piano { body: body::describe(e)?, measured: measure::measure(e)? })
    }
}

/// A project's export, refused (an error naming it) when missing, not a hammer-strung piano of 88
/// keys and one register, or itself a roll.
fn usable(instruments: &Path, name: &str) -> Result<Export, Error> {
    let dir = instruments.join(name);
    let e = Export::read(&dir).map_err(|_| err(format!("{name}: no export in {}", instruments.display())))?;
    if crate::info::info(&dir).map(|i| i.morph_family).unwrap_or(0) != 1 {
        return Err(err(format!("{name}: not a hammer-strung piano of 88 keys")));
    }
    if e.json.get("profile").and_then(|p| p.as_str()).is_some_and(|p| p.starts_with("rolled:")) {
        return Err(err(format!("{name}: a rolled piano (rolls are not measured against rolls)")));
    }
    Ok(e)
}

/// A donor of strings or fine structure: `usable`, a grand, at the sample rate `sr` (the frame's).
fn donor(instruments: &Path, name: &str, sr: Option<f64>) -> Result<Export, Error> {
    let e = usable(instruments, name)?;
    if !crate::info::info(instruments.join(name)).is_ok_and(|i| i.body == crate::mic::Body::Grand) {
        return Err(err(format!("{name}: not a grand (strings and fine structure come from grands)")));
    }
    let own = e.json.get("reference_sr").and_then(|v| v.as_f64()).unwrap_or(48000.0);
    if let Some(sr) = sr.filter(|&sr| sr != own) {
        return Err(err(format!("{name}: its responses at {own} Hz, the frame's at {sr} Hz")));
    }
    Ok(e)
}

fn file_stamp(path: &Path) -> String {
    let m = std::fs::metadata(path).ok();
    format!("{}:{:?}", m.as_ref().map_or(0, |m| m.len()), m.and_then(|m| m.modified().ok()))
}

fn canonical(instruments: &Path, name: &str) -> PathBuf {
    let dir = instruments.join(name);
    std::fs::canonicalize(&dir).unwrap_or(dir)
}

/// What a piano's measurement depends on: this program (its renders) and the export's two files.
fn stamp(instruments: &Path, name: &str) -> String {
    let dir = canonical(instruments, name);
    let exe = std::env::current_exe().map(|p| file_stamp(&p)).unwrap_or_default();
    format!("{exe}|{}|{}|{}", dir.display(), file_stamp(&dir.join("data.bin")), file_stamp(&dir.join("instrument.json")))
}

/// Where measured pianos are kept between runs: `DUSSEK_CACHE`, else the user's cache folder
/// (`XDG_CACHE_HOME` or `~/.cache`), under `dussek/roll`.
fn cache_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = env("DUSSEK_CACHE").or_else(|| env("XDG_CACHE_HOME").map(|d| d.join("dussek"))).or_else(|| env("HOME").map(|h| h.join(".cache/dussek")))?;
    Some(base.join("roll"))
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// A measured piano written to `file` under `key` (a temporary name renamed: never half a file).
pub fn save_piano(file: &Path, key: &str, p: &Piano) -> Result<(), Error> {
    let m = &p.measured;
    let doc = json!({
        "key": key, "level": p.body.level.to_vec(), "rate": p.body.rate.to_vec(), "corr": p.body.corr,
        "stats": m.stats, "pr": m.pr.to_vec(), "note_level": m.level, "mono_vs_power": m.mono_vs_power.to_vec(),
    });
    let io = |e: std::io::Error| err(format!("{}: {e}", file.display()));
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d).map_err(io)?;
    }
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let tmp = file.with_extension(format!("{}-{}.tmp", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&tmp, doc.to_string()).map_err(io)?;
    std::fs::rename(&tmp, file).map_err(io)
}

/// The piano `save_piano` wrote under `key` (None: another key, or not readable).
pub fn load_piano(file: &Path, key: &str) -> Option<Piano> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    if v.get("key")?.as_str()? != key {
        return None;
    }
    let nums = |k: &str| -> Option<Vec<f64>> { v.get(k)?.as_array()?.iter().map(Value::as_f64).collect() };
    fn fixed<const N: usize>(x: Vec<f64>) -> Option<[f64; N]> {
        x.try_into().ok()
    }
    Some(Piano {
        body: body::Body { level: fixed(nums("level")?)?, rate: fixed(nums("rate")?)?, corr: nums("corr")? },
        measured: Measured { stats: nums("stats")?, pr: fixed(nums("pr")?)?, level: nums("note_level")?, mono_vs_power: fixed(nums("mono_vs_power")?)? },
    })
}

/// A reference measured: once per process and export (and kept on disk between runs).
fn piano(instruments: &Path, name: &str) -> Result<Arc<Piano>, Error> {
    static MEMORY: OnceLock<Mutex<HashMap<String, Arc<Piano>>>> = OnceLock::new();
    let key = stamp(instruments, name);
    let memory = MEMORY.get_or_init(Default::default);
    if let Some(p) = memory.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
        return Ok(p.clone());
    }
    let file = cache_dir().map(|d| d.join(format!("{name}-{:016x}.json", fnv(&canonical(instruments, name).to_string_lossy()))));
    let p = match file.as_deref().and_then(|f| load_piano(f, &key)) {
        Some(p) => p,
        None => {
            let p = Piano::measure(&usable(instruments, name)?)?;
            if let Some(f) = &file {
                let _ = save_piano(f, &key, &p); // a cache: a failed write measures again next time
            }
            p
        }
    };
    let p = Arc::new(p);
    memory.lock().unwrap_or_else(|p| p.into_inner()).insert(key, p.clone());
    Ok(p)
}

/// The references measured (each piano once per process and export, kept on disk between runs).
pub fn references(names: &[String], instruments: &Path) -> Result<References, Error> {
    references_with(names, instruments, &never)
}

fn references_with(names: &[String], instruments: &Path, cancelled: &(dyn Fn() -> bool + Sync)) -> Result<References, Error> {
    if names.len() < 3 {
        return Err(err("roll: at least three references"));
    }
    let mut pianos = Vec::new();
    for n in names {
        if cancelled() {
            return Err(err(CANCELLED));
        }
        pianos.push(piano(instruments, n)?);
    }
    let bodies: Vec<body::Body> = pianos.iter().map(|p| p.body.clone()).collect();
    let measured: Vec<Measured> = pianos.iter().map(|p| p.measured.clone()).collect();
    let n = measured.len() as f64;
    let levels = (0..measured[0].level.len()).map(|k| measured.iter().map(|m| m.level[k]).sum::<f64>() / n).collect();
    let goal = std::array::from_fn(|b| {
        let mut v: Vec<f64> = measured.iter().map(|m| m.pr[b]).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    });
    let basis = Basis::new(&bodies);
    let knobs: Vec<Vec<f64>> = bodies.iter().map(|b| basis.knobs(b)).collect();
    let spans = (0..basis.sd.len())
        .map(|i| knobs.iter().map(|k| k[i]).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x))))
        .collect();
    Ok(References { basis, realism: Realism::from_measured(&measured), levels, goal, spans })
}

/// The frame's strings blended and its fine structure mixed: kept for the last recipe's donors
/// (a change of the knobs alone rolls only the body).
fn parts(recipe: &Recipe, instruments: &Path) -> Result<Export, Error> {
    static LAST: Mutex<Option<(String, Export)>> = Mutex::new(None);
    let key = format!(
        "{:?}|{:?}|{}",
        recipe.strings.iter().map(|(n, w)| format!("{}={w}", stamp(instruments, n))).collect::<Vec<_>>(),
        recipe.fine_structure.iter().map(|n| stamp(instruments, n)).collect::<Vec<_>>(),
        instruments.display()
    );
    if let Some((k, e)) = LAST.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
        && *k == key
    {
        return Ok(e.clone());
    }
    let heaviest = (0..recipe.strings.len()).fold(0, |b, i| if recipe.strings[i].1 > recipe.strings[b].1 { i } else { b });
    let frame = donor(instruments, &recipe.strings[heaviest].0, None)?;
    let sr = Some(frame.json.get("reference_sr").and_then(|v| v.as_f64()).unwrap_or(48000.0));
    let donors: Vec<(Export, f64)> = recipe.strings.iter().map(|(n, w)| Ok((donor(instruments, n, sr)?, *w))).collect::<Result<_, Error>>()?;
    let fine: Vec<Export> = recipe.fine_structure.iter().map(|n| donor(instruments, n, sr)).collect::<Result<_, _>>()?;
    let mut blended = frame;
    blend_strings(&mut blended, &donors.iter().map(|(e, w)| (e, *w)).collect::<Vec<_>>())?;
    fine_structure(&mut blended, &fine.iter().collect::<Vec<_>>())?;
    *LAST.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, blended.clone()));
    Ok(blended)
}

/// The rolled piano: its export, the report, its LICENSE.md. The knobs built as set.
pub fn generate(recipe: &Recipe, instruments: &Path) -> Result<(Export, Report, String), Error> {
    generate_with(recipe, instruments, &Options::default())
}

/// `generate` with `Options`: a failing roll pulled back, a roll cancelled between its stages.
pub fn generate_with(recipe: &Recipe, instruments: &Path, opts: &Options) -> Result<(Export, Report, String), Error> {
    let t0 = Instant::now();
    let check = || if (opts.cancelled)() { Err(err(CANCELLED)) } else { Ok(()) };
    check()?;
    if recipe.strings.is_empty() || recipe.fine_structure.is_empty() {
        return Err(err(format!("{}: [roll] needs strings and a fine structure", recipe.name)));
    }
    let blended = parts(recipe, instruments)?;
    check()?;
    let refs = references_with(&recipe.references, instruments, opts.cancelled)?;

    let knobs: Vec<f64> = [recipe.warmth, recipe.air].into_iter().chain(recipe.shade.iter().copied()).collect();
    let mut scale = 1.0;
    let mut pulled_back = 0;
    // the best attempt
    struct Attempt {
        /// how far outside the range (`Realism::excess`; each fault counts one)
        off: f64,
        e: Export,
        failures: Vec<String>,
        knobs: Vec<f64>,
        pulled_back: u32,
    }
    let mut best: Option<Attempt> = None;
    loop {
        check()?;
        let mut e = blended.clone();
        // pulled back: rounded to 0.01 (what the sliders show is what was built)
        let k: Vec<f64> = if pulled_back == 0 { knobs.clone() } else { knobs.iter().map(|v| (v * scale * 100.0).round() / 100.0).collect() };
        body::apply(&mut e, &refs.basis.body(&k))?;
        check()?;
        body::widen(&mut e)?;
        body::lift_notches(&mut e)?;
        check()?;
        match_registers(&mut e, &refs.levels)?;
        check()?;
        // Recording: the whole notes once (the board's radiation does not touch them)
        let rec = measure::render_notes(&e.instrument()?, 80.0, MicPosition::Recording)?;
        check()?;
        let cal = calibrate_positions(&mut e, &refs.goal, &rec)?;
        check()?;
        let m = measure::measure_from(&e, &rec, &cal.player)?;
        let mut failures = refs.realism.check_measured(&m);
        let faults = measure::faults_measured(&e, &m.mono_vs_power)?;
        let off = refs.realism.excess(&m) + faults.len() as f64;
        failures.extend(faults);
        // a pull-back that mends less than a quarter of what is off ends it (some of it may not be
        // the knobs': a draw pulled back 4× for the treble at Player, 2026-10-17)
        let mends = best.as_ref().is_none_or(|b| off < 0.75 * b.off);
        if best.as_ref().is_none_or(|b| off < b.off) {
            best = Some(Attempt { off, e, failures, knobs: k, pulled_back });
        }
        if off == 0.0 || !opts.pull_back || pulled_back == 4 || !mends {
            break;
        }
        scale *= 0.75;
        pulled_back += 1;
    }
    let Attempt { mut e, failures, knobs: k, pulled_back, .. } = best.expect("one attempt");
    e.json["profile"] = json!(format!("rolled:{}", recipe.name));
    e.json["menu"] = json!({ "name": recipe.menu.0, "group": recipe.menu.1 });
    e.json["recorded"] = json!([21, 108]);
    let license = license(recipe, instruments);
    let report = Report { knobs: k, pulled_back, failures, seconds: t0.elapsed().as_secs_f64() };
    Ok((e, report, license))
}

/// LICENSE.md: its own license (CC BY 4.0, or CC BY-SA 4.0 when a source is ShareAlike: an
/// adaptation keeps that license, proto/license.py) and every source's (strings, fine structure,
/// the references whose bodies shaped the knobs), each under its project's name.
pub fn license(recipe: &Recipe, instruments: &Path) -> String {
    let mut sources = String::new();
    let mut seen = Vec::new();
    for name in recipe.strings.iter().map(|s| &s.0).chain(&recipe.fine_structure).chain(&recipe.references) {
        if seen.contains(name) {
            continue;
        }
        seen.push(name.clone());
        sources += &format!("\n## {name}\n\n");
        match std::fs::read_to_string(instruments.join(name).join("LICENSE.md")) {
            Ok(text) => sources += text.lines().skip_while(|l| l.starts_with("# ") || l.trim().is_empty()).collect::<Vec<_>>().join("\n").trim_end(),
            Err(_) => sources += "No license file (the project's data needs no credit).",
        }
        sources += "\n";
    }
    let (title, short, url) = if sources.contains("CC BY-SA 4.0") {
        ("Creative Commons Attribution-ShareAlike 4.0 International", "CC BY-SA 4.0", "https://creativecommons.org/licenses/by-sa/4.0/")
    } else {
        ("Creative Commons Attribution 4.0 International", "CC BY 4.0", "https://creativecommons.org/licenses/by/4.0/")
    };
    format!(
        "# License: {} (rolled)\n\nThe parameters in this directory (`instrument.json`, `data.bin`) were made from the exported\n\
         parameters of the instruments below: the strings and hammers blended, the radiation responses'\n\
         fine structure mixed, the body set along directions measured on all of them. No audio from any\n\
         recording is contained here. They are licensed under the {title} license ({short}, {url}).\n\n\
         Music made with this instrument (renders, recordings, performances) is yours: use it freely, without credit.\n{sources}",
        recipe.menu.0
    )
}

/// LICENSE.md, then the export with its level in instrument.json (its ff level, which a morph reads,
/// and its level table, which maps its velocity; instrument.json last: a plugin watching the folder
/// reads the new files together).
pub fn write_export(dir: &Path, e: &Export, license: &str) -> Result<(), Error> {
    let doc = e.instrument()?.level_doc()?;
    let io = |x: std::io::Error| err(format!("{}: {x}", dir.display()));
    std::fs::create_dir_all(dir).map_err(io)?;
    std::fs::write(dir.join("LICENSE.md"), license).map_err(io)?;
    let mut e = e.clone();
    if let Some(o) = e.json.as_object_mut() {
        o.insert("level".into(), doc);
    }
    e.write(dir)
}
