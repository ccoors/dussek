//! A source's page: its steps as tabs (the folder, how the files are read, the check with the
//! grid and the table), and Advanced: every key as TOML text. Adding a source: a folder under
//! data/ (or linked in), its role, its id.
use super::jobs::{Done, Job};
use super::steps::check::{self, Action, CheckState};
use super::steps::folder::{self, FolderPick};
use super::steps::{dynamics, names, reading, settings};
use super::{App, Page, key_rows_but};
use eframe::egui;
use pe_editor::audio::Measures;
use pe_editor::takes::Thresholds;
use pe_editor::mapping::{self, Report};
use pe_editor::project::{Table, json_to_toml};
use pe_editor::rule::{self, LayerStat, Preview, Reading, Rule};
use pe_editor::suggest::{self, Folder, Suggestion};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

const SOURCE_KEYS: [&str; 14] = ["dynamics", "levels", "tail_cut_s", "skip", "skip_files", "relabel", "cut_s", "gain_db", "release", "mallet",
    "octave", "extra", "layer_key", "loud_from_layer"];
const PATTERN_KEYS: [&str; 11] = ["glob", "regex", "match", "note", "layers", "dynamic", "dynamic_names", "dynamic_format",
    "prefer", "fields", "order"];
const SFZ_KEYS: [&str; 11] = ["files", "filter", "folders", "prefer_substring", "prefix", "normpath", "unique", "layers",
    "dynamic_format", "dynamic_from", "sfz_fields"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    Folder,
    Reading,
    Names,
    Dynamics,
    #[default]
    Check,
    Settings,
    Advanced,
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Tab::Folder => "Folder",
            Tab::Reading => "Reading",
            Tab::Names => "Names",
            Tab::Dynamics => "Dynamics",
            Tab::Check => "Check",
            Tab::Settings => "Source settings",
            Tab::Advanced => "Advanced",
        }
    }
}

const TABS: [Tab; 7] = [Tab::Folder, Tab::Reading, Tab::Names, Tab::Dynamics, Tab::Check, Tab::Settings, Tab::Advanced];

#[derive(Default)]
pub struct SourceView {
    pub tab: Tab,
    report: Option<Result<Report, String>>,
    measures: Measures,
    offsets: BTreeMap<String, f32>,
    check: CheckState,
    rename_to: String,
    job: Option<Job>,
    findings: Vec<suggest::Finding>,
    /// the files last measured (their paths' hash)
    measured_for: u64,
    /// a suggestion that came in while a build ran: applied once the project is editable again
    pending: Option<Suggestion>,
    folder: FolderPick,
    sfz_files: Option<Vec<String>>,
    /// the ways of reading left (their keys, restored on switching back)
    stash: rule::Stash,
    /// the rule and the source it was read from
    rule: Option<(Json, Rule)>,
    stats: Vec<LayerStat>,
    preview: Option<Result<Preview, String>>,
    sfz_folders: Vec<(String, usize)>,
}

fn cache_of(app: &App) -> PathBuf {
    app.open.as_ref().map(|o| app.repo.projects.join(&o.project.name).join("cache/editor/audio.json")).unwrap_or_default()
}

/// the source's items again (after an edit), the measurements of new files in the background,
/// and the rule as the pages show it
fn remap(app: &mut App, id: &str) {
    let cache = cache_of(app);
    let Some(o) = &mut app.open else { return };
    let src = o.project.source_json(id).unwrap_or(Json::Null);
    let root = suggest::data_root(&app.repo.data, src["root"].as_str().unwrap_or_default());
    let rep = mapping::items(&src, &root);
    let v = o.sources.entry(id.to_string()).or_default();
    if let Ok(r) = &rep {
        // every file the names match (a layer left out shows its level too), else the items'
        let mut paths: Vec<PathBuf> = match src["map"]["kind"].as_str() {
            Some("pattern") => mapping::pattern_matches(&root, &src["map"]).map(|(m, _)| m.into_iter().map(|x| PathBuf::from(x.path)).collect()).unwrap_or_default(),
            _ => Vec::new(),
        };
        if paths.is_empty() {
            paths = r.items.iter().filter_map(|i| i["path"].as_str().map(PathBuf::from)).collect();
        }
        // the spare takes too: checked like the rest, heard before one is used
        for p in r.spares.values().flatten().filter_map(|i| i["path"].as_str().map(PathBuf::from)) {
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            paths.hash(&mut h);
            h.finish()
        };
        if v.job.is_none() && key != v.measured_for {
            v.measured_for = key;
            v.job = Some(Job::start("measuring", paths, cache, Done::Measured));
        }
        v.offsets = suggest::pitch_offsets(&r.items, &v.measures);
    }
    v.report = Some(rep);
    v.stats = rule::layer_stats(&src, &root, &v.measures);
    if v.rule.as_ref().is_none_or(|(s, _)| *s != src) {
        let found: Vec<String> = v.stats.iter().map(|s| s.value.clone()).collect();
        v.rule = Some((src.clone(), Rule::read(&src, &found)));
    }
    v.preview = None;
    v.sfz_folders = Vec::new();
    if let Some((_, r)) = &mut v.rule {
        match &mut r.reading {
            Reading::Names(n) => {
                let p = rule::preview(n, &root);
                if let Ok(p) = &p {
                    n.fill_texts(p);
                }
                v.preview = Some(p);
            }
            Reading::Sfz(_) => v.sfz_folders = rule::sfz_folders(&src, &root),
            Reading::Custom => {}
        }
    }
}

/// a rule edited on a page into the project: applied, its layers synced with what the new map
/// reads, written (only the keys that changed)
pub fn commit_rule(app: &mut App, id: &str, mut r: Rule) {
    let Some(o) = &mut app.open else { return };
    let Some(src) = o.project.source_json(id) else { return };
    let root = suggest::data_root(&app.repo.data, src["root"].as_str().unwrap_or_default());
    let v = o.sources.entry(id.to_string()).or_default();
    let first = r.apply(&src);
    let found: Vec<String> = rule::layer_stats(&first, &root, &v.measures).into_iter().map(|s| s.value).collect();
    r.sync_layers(&first["map"], &found);
    match o.project.update_source(id, &r.apply(&src)) {
        // the rule as edited stays (its rows in their places), read from the source it made
        Ok(_) => v.rule = o.project.source_json(id).map(|s| (s, r)),
        Err(e) => app.message = Some(e),
    }
    remap(app, id);
}

/// the proposal into the project: the source's fields and its whole map replaced
fn apply(o: &mut super::Open, id: &str, s: &Suggestion) -> Result<(), String> {
    let mut new = o.project.source_json(id).ok_or("no such source")?;
    for k in ["root", "levels", "dynamics", "release"] {
        if let Some(v) = s.source.get(k) {
            new[k] = v.clone();
        }
    }
    new["map"] = s.source["map"].clone();
    o.project.update_source(id, &new).map(|_| ())
}

/// the suggestion for a source: its folder (or `folder`), read from `sfz` (None: the names)
fn suggest_for(app: &mut App, id: &str, folder: Option<Folder>, sfz: Option<Option<String>>) {
    let cache = cache_of(app);
    let Some(o) = &mut app.open else { return };
    let Some(src) = o.project.source_json(id) else { return };
    let Some((f0, s0)) = suggest::folder_of(&src) else {
        app.message = Some("the source has no root and glob (or SFZ file) to suggest from".into());
        return;
    };
    let v = o.sources.entry(id.to_string()).or_default();
    if folder.is_some() {
        // the ways kept read the old folder
        v.stash.folder_changed();
    }
    let folder = folder.unwrap_or(f0);
    let sfz = sfz.unwrap_or(s0);
    v.sfz_files = None;
    v.job = Some(Job::suggestion(app.repo.data.clone(), folder, sfz, cache));
}

fn suggest_now(app: &mut App, id: &str) {
    suggest_for(app, id, None, None);
}

fn poll_job(app: &mut App, id: &str) -> bool {
    let editable = app.editable();
    let Some(o) = &mut app.open else { return false };
    let Some(v) = o.sources.get_mut(id) else { return false };
    let (s, fresh) = match v.job.as_ref().and_then(|j| j.poll()) {
        Some(done) => {
            v.job = None;
            match done {
                Done::Measured(m) => {
                    v.measures = m;
                    return true;
                }
                Done::Failed(e) => {
                    app.message = Some(format!("suggestion: {e}"));
                    return true;
                }
                Done::Suggested(s, m) => {
                    v.measures = m;
                    (s, true)
                }
            }
        }
        None => match v.pending.take() {
            Some(s) => (s, false),
            None => return false,
        },
    };
    // the project is read-only while a build runs: the suggestion waits
    if !editable {
        v.pending = Some(s);
        if fresh {
            app.message = Some(format!("{id}: the suggestion is applied once the build ends"));
        }
        return false;
    }
    v.findings = s.findings.clone();
    v.rule = None;
    if let Err(e) = apply(o, id, &s) {
        app.message = Some(format!("suggestion: {e}"));
    }
    o.edits = Default::default();
    true
}

/// another way of reading chosen: the current one's keys stashed; the chosen one's restored
/// from its stash, else suggested
fn switch_reading(app: &mut App, id: &str, choice: Option<String>) {
    let Some(o) = &mut app.open else { return };
    let Some(src) = o.project.source_json(id) else { return };
    let v = o.sources.entry(id.to_string()).or_default();
    v.stash.put(&src);
    if let Some(new) = v.stash.restore(&src, choice.as_deref()) {
        if let Err(e) = o.project.update_source(id, &new) {
            app.message = Some(e);
        }
        remap(app, id);
    } else {
        suggest_for(app, id, None, Some(choice));
    }
}

pub fn page(app: &mut App, ui: &mut egui::Ui, id: &str) {
    if app.open.as_ref().is_none_or(|o| !o.sources.contains_key(id)) || poll_job(app, id) {
        remap(app, id);
    }
    let editable = app.editable();
    let data = app.repo.data.clone();
    let repo = app.repo.clone();
    let mut act: Option<Action> = None;
    let mut run_suggest = false;
    let mut rename: Option<String> = None;
    let mut new_folder: Option<Folder> = None;
    let mut new_choice: Option<Option<String>> = None;
    let mut changed = false;
    let mut edited: Option<Rule> = None;
    let mut reset = false;
    {
        let Some(o) = &mut app.open else { return };
        let src = o.project.source_json(id).unwrap_or(Json::Null);
        let src_t = Table::Source(id.into());
        let map_t = Table::Map(id.into());
        let thresholds = Thresholds::from_project(o.project.to_json().get("take_checks"));
        let v = o.sources.entry(id.to_string()).or_default();
        ui.horizontal(|ui| {
            ui.heading(id);
            ui.label(format!("{} {}", src["role"].as_str().unwrap_or_default(),
                src.get("mallet").and_then(Json::as_str).map(|m| format!("\"{m}\"")).unwrap_or_default()));
            if let Some(Ok(r)) = &v.report {
                ui.label(format!("· {} items", r.items.len()));
            }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut v.rename_to).desired_width(120.0).hint_text("new id"));
            if ui.add_enabled(editable, egui::Button::new("Rename")).clicked() {
                rename = Some(v.rename_to.trim().to_string());
            }
            run_suggest = ui.add_enabled(editable && v.job.is_none(), egui::Button::new("Suggest again"))
                .on_hover_text("propose the whole rule from the folder's files again (the current one is replaced)").clicked();
        });
        if let Some(job) = &v.job {
            job.bar(ui);
        }
        ui.horizontal(|ui| {
            for t in TABS {
                ui.selectable_value(&mut v.tab, t, t.title());
            }
        });
        ui.separator();
        match v.tab {
            Tab::Folder => {
                if let Some((f, _)) = suggest::folder_of(&src) {
                    ui.label(format!("Now: data/{}{}", f.root, if f.sub.is_empty() { String::new() } else { format!("/{}", f.sub) }));
                }
                new_folder = folder::page(ui, &repo, &mut v.folder, editable && v.job.is_none());
                if new_folder.is_some() {
                    ui.label("The rule is proposed again for the new folder.");
                }
            }
            Tab::Reading => {
                let files = v.sfz_files.get_or_insert_with(|| {
                    suggest::folder_of(&src).map(|(f, _)| suggest::sfz_files(&data, &Folder { root: f.root, sub: String::new() })).unwrap_or_default()
                });
                new_choice = reading::page(ui, files, &rule::reading_of(&src), editable && v.job.is_none());
            }
            Tab::Check => {
                act = check::page(ui, &mut v.check, v.report.as_ref(), &v.measures, &v.offsets, &v.findings, &thresholds, editable);
            }
            Tab::Advanced => {
                super::steps::hint(ui, "Every key as TOML text, for what the other tabs do not show. A value is applied as soon as it parses.");
                let kind = src["map"]["kind"].as_str().unwrap_or_default().to_string();
                ui.label(egui::RichText::new("The source").strong());
                changed |= key_rows_but(ui, &mut o.project, &src_t, &SOURCE_KEYS, &["id"], &mut o.edits, editable);
                ui.label(egui::RichText::new(format!("Its map ({kind}: switch it on the Reading tab)")).strong());
                let known: &[&str] = if kind == "sfz" { &SFZ_KEYS } else { &PATTERN_KEYS };
                changed |= key_rows_but(ui, &mut o.project, &map_t, known, &["kind"], &mut o.edits, editable);
            }
            Tab::Names => {
                if let Some((_, r)) = &v.rule {
                    let mut r = r.clone();
                    match names::page(ui, &mut r, v.preview.as_ref(), &v.sfz_folders, false, editable && v.job.is_none()) {
                        names::Out::Changed => edited = Some(r),
                        names::Out::ResetToParts => reset = true,
                        names::Out::EditByHand => v.tab = Tab::Advanced,
                        names::Out::Nothing => {}
                    }
                }
                check::findings(ui, &v.findings);
            }
            Tab::Dynamics => {
                if let Some((_, r)) = &v.rule {
                    let mut r = r.clone();
                    if dynamics::page(ui, &mut r, &v.stats, editable && v.job.is_none(), v.job.is_some()) {
                        edited = Some(r);
                    }
                }
            }
            Tab::Settings => changed |= settings::page(ui, &mut o.project, id, editable),
        }
    }
    if let Some(a) = act {
        apply_action(app, id, a);
    }
    if let Some(r) = edited {
        commit_rule(app, id, r);
    }
    if reset {
        suggest_for(app, id, None, Some(None));
    }
    if let Some(new) = rename.filter(|n| !n.is_empty()) {
        if let Some(o) = &mut app.open {
            match o.project.rename_source(&app.repo, id, &new) {
                Ok(()) => {
                    if let Some(v) = o.sources.remove(id) {
                        o.sources.insert(new.clone(), v);
                    }
                    o.page = Page::Source(new);
                    o.edits = Default::default();
                }
                Err(e) => app.message = Some(e),
            }
        }
        return;
    }
    if let Some(f) = new_folder {
        let sfz = suggest::sfz_files(&app.repo.data, &f).into_iter().next();
        suggest_for(app, id, Some(f), Some(sfz));
    }
    if let Some(c) = new_choice {
        switch_reading(app, id, c);
    }
    if run_suggest {
        suggest_now(app, id);
    }
    if changed {
        remap(app, id);
    }
}

/// the source's left-out takes (its skip_files)
fn skip_files(p: &pe_editor::project::Project, t: &Table) -> Vec<String> {
    p.get(t, "skip_files").and_then(|v| v.as_array().cloned()).unwrap_or_default().iter()
        .filter_map(|v| v.as_str().map(String::from)).collect()
}

/// a click in the check page: play the file, leave a key or a take out, put a take back, relabel a
/// file's key, move the take checks' thresholds
fn apply_action(app: &mut App, id: &str, a: Action) {
    match a {
        Action::Play(p) => match &app.player {
            Ok(pl) => {
                if let Err(e) = pl.play(&p) {
                    app.message = Some(e);
                }
            }
            Err(e) => app.message = Some(format!("no playback: {e}")),
        },
        Action::Skip(note) => {
            let Some(o) = &mut app.open else { return };
            let t = Table::Source(id.into());
            let mut skip: Vec<i64> = o.project.get(&t, "skip").and_then(|v| v.as_array().cloned())
                .unwrap_or_default().iter().filter_map(Json::as_i64).collect();
            skip.push(note);
            skip.sort();
            skip.dedup();
            let _ = o.project.set(&t, "skip", json_to_toml(&json!(skip)));
            remap(app, id);
        }
        Action::SkipFile(rel) => {
            let Some(o) = &mut app.open else { return };
            let t = Table::Source(id.into());
            let mut files = skip_files(&o.project, &t);
            if !files.contains(&rel) {
                files.push(rel);
            }
            let _ = o.project.set(&t, "skip_files", json_to_toml(&json!(files)));
            remap(app, id);
        }
        Action::UnskipFile(rel) => {
            let Some(o) = &mut app.open else { return };
            let t = Table::Source(id.into());
            let files: Vec<String> = skip_files(&o.project, &t).into_iter().filter(|f| *f != rel).collect();
            let _ = if files.is_empty() { o.project.remove(&t, "skip_files") } else { o.project.set(&t, "skip_files", json_to_toml(&json!(files))) };
            remap(app, id);
        }
        Action::Thresholds(th) => {
            let Some(o) = &mut app.open else { return };
            if let Err(e) = pe_editor::takes::write_thresholds(&mut o.project, &th) {
                app.message = Some(e);
            }
        }
        Action::Relabel(note, to) => {
            let Some(o) = &mut app.open else { return };
            let t = Table::Source(id.into());
            let mut rel = o.project.get(&t, "relabel").unwrap_or(json!({}));
            rel[note.to_string()] = json!(to);
            let _ = o.project.set(&t, "relabel", json_to_toml(&rel));
            remap(app, id);
        }
    }
}
