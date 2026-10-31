//! Rolling a piano (no UI): the candidates (the exported hammer pianos of 88 keys, never a roll),
//! a recipe drawn by the roll-time rules or one part of it re-rolled, the preview written where the
//! plugin looks, the project written at Finish (design log steps 117–118).
use crate::project::{self, Repo, write_atomic};
use pianoengine::mic::Body;
use pianoengine::roll::files::Export;
use pianoengine::roll::generate::{Options, Report, generate_with, license, write_export};
use pianoengine::roll::recipe::Recipe;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The preview's folder under `instruments/` (git-ignored) and its menu entry.
pub const PREVIEW: &str = "rolled_preview";
pub const PREVIEW_MENU: (&str, &str) = ("Rolled piano (preview)", "Rolled pianos");
pub const GROUP: &str = "Rolled pianos";

pub struct Candidates {
    /// every exported hammer-strung piano of 88 keys that is not a roll
    pub references: Vec<String>,
    /// those of them that are grands (the strings and fine structure come from these)
    pub grands: Vec<String>,
}

pub fn instruments(repo: &Repo) -> PathBuf {
    repo.root.join("instruments")
}

pub fn candidates(repo: &Repo) -> Candidates {
    let (mut references, mut grands) = (Vec::new(), Vec::new());
    for name in project::project_names(repo) {
        if project::family(repo, &name).as_deref() == Some("rolled") {
            continue;
        }
        let dir = instruments(repo).join(&name);
        let Ok(info) = pianoengine::info::info(&dir) else { continue };
        if info.morph_family != 1 {
            continue;
        }
        if info.body == Body::Grand {
            grands.push(name.clone());
        }
        references.push(name);
    }
    Candidates { references, grands }
}

/// splitmix64: a small, seeded generator (a roll records its seed)
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// uniform in (0, 1)
    pub fn uniform(&mut self) -> f64 {
        ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// Gamma(2, 1): the sum of two exponentials
    pub fn gamma2(&mut self) -> f64 {
        -(self.uniform() * self.uniform()).ln()
    }

    pub fn normal(&mut self) -> f64 {
        (-2.0 * self.uniform().ln()).sqrt() * (2.0 * std::f64::consts::PI * self.uniform()).cos()
    }

    /// `k` distinct items of `from`, in drawn order
    fn pick<'a>(&mut self, from: &'a [String], k: usize) -> Vec<&'a String> {
        let mut idx: Vec<usize> = (0..from.len()).collect();
        for i in (1..idx.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            idx.swap(i, j);
        }
        idx.into_iter().take(k).map(|i| &from[i]).collect()
    }
}

fn round(x: f64, digits: i32) -> f64 {
    let m = 10f64.powi(digits);
    (x * m).round() / m
}

/// Three allowed grands at Dirichlet(2, 2, 2) weights (rounded to 0.001, summing to 1), drawn again
/// until none is above 0.55.
fn draw_strings(allowed: &[String], rng: &mut Rng) -> Vec<(String, f64)> {
    let k = allowed.len().min(3);
    if k == 0 {
        return Vec::new(); // nothing allowed: the roll says so (generate names it)
    }
    loop {
        let names = rng.pick(allowed, k);
        let g: Vec<f64> = (0..k).map(|_| rng.gamma2()).collect();
        let sum: f64 = g.iter().sum();
        let mut w: Vec<f64> = g.iter().map(|x| round(x / sum, 3)).collect();
        let rest: f64 = w[..k - 1].iter().sum();
        w[k - 1] = 1.0 - rest;
        if k < 3 || w.iter().all(|&x| x <= 0.55) {
            return names.into_iter().cloned().zip(w).collect();
        }
    }
}

fn frame(strings: &[(String, f64)]) -> &str {
    strings.iter().fold(None::<&(String, f64)>, |b, s| match b {
        Some(b) if b.1 >= s.1 => Some(b),
        _ => Some(s),
    }).map_or("", |s| s.0.as_str())
}

/// Three allowed grands other than the frame.
fn draw_fine(allowed: &[String], frame: &str, rng: &mut Rng) -> Vec<String> {
    let others: Vec<String> = allowed.iter().filter(|g| *g != frame).cloned().collect();
    rng.pick(&others, others.len().min(3)).into_iter().cloned().collect()
}

/// A new recipe by the roll-time rules: the strings (`draw_strings`), the fine structure, warmth
/// uniform in [−1.5, 0.5] (its warm end misbehaved), air uniform in [−1.2, 1.2], two shades
/// normal with σ 0.7; measured against every candidate.
pub fn draw(c: &Candidates, allowed: &[String], rng: &mut Rng, name: &str) -> Recipe {
    let seed = rng.0;
    let strings = draw_strings(allowed, rng);
    let fine_structure = draw_fine(allowed, frame(&strings), rng);
    Recipe {
        name: name.to_string(),
        menu: (name.to_string(), GROUP.to_string()),
        references: c.references.clone(),
        strings,
        fine_structure,
        warmth: round(-1.5 + 2.0 * rng.uniform(), 2),
        air: round(-1.2 + 2.4 * rng.uniform(), 2),
        shade: (0..2).map(|_| round(0.7 * rng.normal(), 2)).collect(),
        seed,
    }
}

/// New strings, the rest kept (a fine-structure donor that became the frame replaced).
pub fn reroll_strings(r: &mut Recipe, allowed: &[String], rng: &mut Rng) {
    r.strings = draw_strings(allowed, rng);
    let f = frame(&r.strings).to_string();
    if let Some(pos) = r.fine_structure.iter().position(|x| *x == f) {
        let spare: Vec<String> = allowed.iter().filter(|g| **g != f && !r.fine_structure.contains(g)).cloned().collect();
        match rng.pick(&spare, 1).first() {
            Some(s) => r.fine_structure[pos] = (*s).clone(),
            None => {
                r.fine_structure.remove(pos);
            }
        }
    }
}

/// A new fine structure, the rest kept.
pub fn reroll_fine(r: &mut Recipe, allowed: &[String], rng: &mut Rng) {
    r.fine_structure = draw_fine(allowed, frame(&r.strings), rng);
}

/// The donor table's slider: `name`'s strings at weight `w` (to 0.001), the others keeping their
/// proportions in the rest; a donor at 0 leaves; the only donor stays whole (nothing to give its
/// share to).
pub fn set_weight(strings: &mut Vec<(String, f64)>, name: &str, w: f64) {
    let w = round(w.clamp(0.0, 1.0), 3);
    let rest: f64 = strings.iter().filter(|s| s.0 != name).map(|s| s.1).sum();
    if rest <= 0.0 {
        if strings.is_empty() && w > 0.0 {
            strings.push((name.to_string(), 1.0));
        }
        return;
    }
    for s in strings.iter_mut().filter(|s| s.0 != name) {
        s.1 = round(s.1 * (1.0 - w) / rest, 3);
    }
    match strings.iter().position(|s| s.0 == name) {
        Some(i) => strings[i].1 = w,
        None => strings.push((name.to_string(), w)),
    }
    strings.retain(|s| s.1 > 0.0);
    // the rounding's remainder to the heaviest of the others
    let diff = 1.0 - strings.iter().map(|s| s.1).sum::<f64>();
    if let Some(h) = (0..strings.len()).filter(|&i| strings[i].0 != name).max_by(|&a, &b| strings[a].1.total_cmp(&strings[b].1)) {
        strings[h].1 = round(strings[h].1 + diff, 3);
    }
}

/// The table's fine-structure box: `name` in or out; false (nothing changed) for the last one out.
pub fn set_fine(fine: &mut Vec<String>, name: &str, on: bool) -> bool {
    match (on, fine.iter().position(|f| f == name)) {
        (true, None) => fine.push(name.to_string()),
        (false, Some(_)) if fine.len() == 1 => return false,
        (false, Some(i)) => {
            fine.remove(i);
        }
        _ => {}
    }
    true
}

/// The knobs a roll was built at (a pulled-back draw) taken into the recipe: the sliders show what
/// plays.
pub fn adopt_knobs(r: &mut Recipe, knobs: &[f64]) {
    if let [warmth, air, shade @ ..] = knobs {
        (r.warmth, r.air, r.shade) = (*warmth, *air, shade.to_vec());
    }
}

/// The export written to `dir` (instrument.json renamed last: never half a file), with its
/// level.json and LICENSE.md.
pub fn write_preview_at(dir: &Path, e: &Export, license: &str) -> Result<(), String> {
    write_export(dir, e, license).map_err(|e| e.0)
}

/// The preview's generation: each roll takes a new one, and only the newest may write (a roll that
/// a newer one, a Cancel or a Finish superseded never brings its candidate back); writes one at a
/// time.
static PREVIEW_GENERATION: Mutex<u64> = Mutex::new(0);

/// A new generation for a roll about to start (the older ones are superseded).
pub fn begin_preview() -> u64 {
    let mut g = PREVIEW_GENERATION.lock().unwrap_or_else(|p| p.into_inner());
    *g += 1;
    *g
}

/// Every roll running superseded (Cancel, Finish).
pub fn forget_preview() {
    begin_preview();
}

/// Whether `generation` is still the newest (a roll that is not stops at its next stage).
pub fn is_current(generation: u64) -> bool {
    *PREVIEW_GENERATION.lock().unwrap_or_else(|p| p.into_inner()) == generation
}

/// The recipe a preview was rolled from, beside it (the plugin reads only the export's files).
const PREVIEW_RECIPE: &str = "recipe.toml";

/// `write_preview_at` when `generation` is still the newest (false: superseded, nothing written),
/// `recipe` beside it (removed first: a recipe there always is the export's).
pub fn write_preview_if_current(dir: &Path, e: &Export, license: &str, recipe: &Recipe, generation: u64) -> Result<bool, String> {
    let g = PREVIEW_GENERATION.lock().unwrap_or_else(|p| p.into_inner());
    if *g != generation {
        return Ok(false);
    }
    let _ = std::fs::remove_file(dir.join(PREVIEW_RECIPE));
    write_preview_at(dir, e, license)?;
    write_atomic(&dir.join(PREVIEW_RECIPE), &recipe.to_toml()).map_err(|e| format!("{PREVIEW_RECIPE}: {e}"))?;
    drop(g);
    Ok(true)
}

/// The preview's export when it plays `r` (its name and menu entry aside: the user names it after
/// the last roll), else None.
fn current_preview(repo: &Repo, r: &Recipe) -> Option<Export> {
    let dir = instruments(repo).join(PREVIEW);
    let built = Recipe::parse(&std::fs::read_to_string(dir.join(PREVIEW_RECIPE)).ok()?, &r.name).ok()?;
    let same = Recipe { name: r.name.clone(), menu: r.menu.clone(), seed: r.seed, ..built };
    if same != *r {
        return None;
    }
    Export::read(&dir).ok()
}

/// The recipe rolled and written as the preview (its menu entry the preview's) unless a newer roll
/// superseded this one (`begin_preview`: then it stops at its next stage, the error
/// `generate::CANCELLED`). `pull_back`: a draw's, pulled back toward the middle until it passes the
/// realism check (the user's settings are built as set).
pub fn preview(repo: &Repo, r: &Recipe, generation: u64, pull_back: bool) -> Result<Report, String> {
    let mut shown = r.clone();
    shown.menu = (PREVIEW_MENU.0.to_string(), PREVIEW_MENU.1.to_string());
    let cancelled = || !is_current(generation);
    let (e, report, license) = generate_with(&shown, &instruments(repo), &Options { pull_back, cancelled: &cancelled }).map_err(|e| e.0)?;
    // the recipe as built (a pulled-back draw's knobs: what the sliders then show)
    let mut built = r.clone();
    adopt_knobs(&mut built, &report.knobs);
    write_preview_if_current(&instruments(repo).join(PREVIEW), &e, &license, &built, generation)?;
    Ok(report)
}

/// The preview removed (no roll writes it again: `forget_preview`).
pub fn remove_preview(repo: &Repo) {
    forget_preview();
    let _ = std::fs::remove_dir_all(instruments(repo).join(PREVIEW));
}

/// A project's name: lower case letters, digits, underscores, starting with a letter.
pub fn valid_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The project written (`projects/<name>/project.toml`) and, when the preview plays its recipe,
/// the preview's export as the project's (its profile, menu entry and license the project's: the
/// plugin lists it at once, the build that follows writes the same); the preview removed. A name
/// with a project or an export is taken.
pub fn finish(repo: &Repo, r: &Recipe) -> Result<(), String> {
    if !valid_name(&r.name) {
        return Err(format!("{}: lower case letters, digits and underscores only", r.name));
    }
    let (dir, out) = (repo.projects.join(&r.name), instruments(repo).join(&r.name));
    if dir.exists() || out.exists() {
        return Err(format!("{}: the name is taken", r.name));
    }
    let built = current_preview(repo, r);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    write_atomic(&dir.join("project.toml"), &r.to_toml()).map_err(|e| format!("project.toml: {e}"))?;
    if let Some(mut e) = built {
        e.json["profile"] = format!("rolled:{}", r.name).into();
        e.json["menu"] = serde_json::json!({ "name": r.menu.0, "group": r.menu.1 });
        write_export(&out, &e, &license(r, &instruments(repo))).map_err(|e| e.0)?;
    }
    remove_preview(repo);
    Ok(())
}
