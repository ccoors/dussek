//! The wizard's page: its steps listed on the left (a step done can be clicked to go back), the
//! step's page, Back and Next (Finish at the end) below. The suggestion runs when the folder or
//! the reading is chosen; nothing is written before Finish.
use super::jobs::{Done, Job};
use super::steps::check::{self, Action, CheckState};
use super::steps::folder::{self, FolderPick};
use super::steps::{self, dynamics, names, played, reading};
use super::{App, Page};
use eframe::egui;
use pe_editor::audio::Measures;
use pe_editor::kind::{self, Guess, Key};
use pe_editor::mapping::{self, Report};
use pe_editor::project::{self, Repo};
use pe_editor::rule::{self, LayerStat, Preview, Reading};
use pe_editor::suggest;
use pe_editor::wizard::{self, Step, Wizard};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const WARN: egui::Color32 = egui::Color32::from_rgb(200, 140, 40);

pub struct WizardView {
    pub w: Wizard,
    pub step: Step,
    /// the measurements' cache: the wizard's own, or the open project's when adding to it
    cache: PathBuf,
    folder: FolderPick,
    job: Option<Job>,
    measures: Measures,
    report: Option<Result<Report, String>>,
    stats: Vec<LayerStat>,
    preview: Option<Result<Preview, String>>,
    sfz_folders: Vec<(String, usize)>,
    offsets: BTreeMap<String, f32>,
    check: CheckState,
    counts: Option<(usize, usize)>,
    new_group: String,
    /// every id in use (project::ids), read when the wizard opens
    ids: HashMap<String, String>,
    /// the sources of a group that read no file
    empty: Vec<String>,
    repo: Repo,
    /// the answers to "How is it played?" (projects/kinds.toml)
    kinds: Vec<project::Kind>,
    /// what the recordings pre-select, from `keys`: the mapped items, one file per key
    guess: Guess,
    keys: Vec<Key>,
    /// the projects' names in the plugin's menu, by project
    menu_names: BTreeMap<String, String>,
}

impl WizardView {
    pub fn new(repo: &Repo, w: Wizard, cache: PathBuf) -> WizardView {
        let step = w.steps()[0];
        WizardView { w, step, cache, folder: FolderPick::default(), job: None, measures: Measures::default(), report: None,
            stats: Vec::new(), preview: None, sfz_folders: Vec::new(), offsets: BTreeMap::new(), check: CheckState::default(),
            counts: None, new_group: String::new(), ids: project::ids(repo), empty: Vec::new(), repo: repo.clone(),
            kinds: project::kinds(repo), guess: Guess { kind: None, why: String::new() }, keys: Vec::new(),
            menu_names: project::templates(repo).into_iter().map(|t| (t.name, t.menu_name)).collect() }
    }

    /// an answer chosen (the guess's when none was): its closest donor, or the one picked
    fn take_answer(&mut self, kind_id: &str, donor: Option<String>) {
        let Some(k) = self.kinds.iter().find(|k| k.id == kind_id).cloned() else { return };
        let (d, why) = match donor {
            Some(d) => (d, "chosen".to_string()),
            None => kind::nearest_donor(&self.repo, &k, &self.keys),
        };
        let repo = self.repo.clone();
        self.w.choose(&repo, &k.id, &d, &why);
    }

    /// the wizard left: the folders it linked into data/ are unlinked, but `keep` (the one Finish used)
    pub fn unlink(&mut self, repo: &Repo, keep: Option<&str>) -> Option<String> {
        let mut errors = Vec::new();
        for name in std::mem::take(&mut self.folder.linked) {
            if Some(name.as_str()) != keep {
                if let Err(e) = project::unlink_folder(repo, &name) {
                    errors.push(e);
                }
            }
        }
        (!errors.is_empty()).then(|| errors.join("; "))
    }

    fn root(&self, data: &Path) -> String {
        self.w.folder.as_ref().map(|f| suggest::data_root(data, &f.root)).unwrap_or_default()
    }

    /// what the rule reads now: the items, the layers' statistics, the parts' preview
    fn refresh(&mut self, data: &Path) {
        let Some(src) = self.w.current() else {
            self.report = None;
            return;
        };
        let root = self.root(data);
        let rep = mapping::items(&src, &root);
        if let Ok(r) = &rep {
            self.offsets = suggest::pitch_offsets(&r.items, &self.measures);
        }
        if let Ok(r) = &rep {
            self.keys = kind::sample_keys(&r.items, &self.measures, src["dynamics"].get("mf"));
            self.guess = kind::preselect(&self.repo, &self.kinds, &self.keys);
        }
        self.report = Some(rep);
        self.empty = if self.w.rule.as_ref().is_some_and(|r| r.group_values().is_some()) { self.w.sources_reading_nothing(&root) } else { Vec::new() };
        self.stats = rule::layer_stats(&src, &root, &self.measures);
        self.preview = None;
        self.sfz_folders = Vec::new();
        if let Some(r) = &mut self.w.rule {
            match &mut r.reading {
                Reading::Names(n) => {
                    let p = rule::preview(n, &root);
                    if let Ok(p) = &p {
                        n.fill_texts(p);
                    }
                    self.preview = Some(p);
                }
                Reading::Sfz(_) => self.sfz_folders = rule::sfz_folders(&src, &root),
                Reading::Custom => {}
            }
        }
    }

    /// after a parts edit: the layers synced with what the changed map reads
    fn settle(&mut self, data: &Path) {
        let root = self.root(data);
        if let Some(src) = self.w.current() {
            let found: Vec<String> = rule::layer_stats(&src, &root, &self.measures).into_iter().map(|s| s.value).collect();
            if let Some(r) = &mut self.w.rule {
                r.sync_layers(&src["map"], &found);
            }
        }
        self.refresh(data);
    }

    fn suggest(&mut self, data: &Path) {
        let Some(f) = self.w.folder.clone() else { return };
        self.w.rule = None;
        self.w.suggest_error = None;
        self.report = None;
        self.job = Some(Job::suggestion(data.to_path_buf(), f, self.w.sfz.clone(), self.cache.clone()));
    }

    /// the job's result taken in; an error to show
    fn poll(&mut self, data: &Path) -> Option<String> {
        let done = self.job.as_ref()?.poll()?;
        self.job = None;
        match done {
            Done::Suggested(s, m) => {
                self.measures = m;
                let root = self.root(data);
                let found: Vec<String> = rule::layer_stats(&s.source, &root, &self.measures).into_iter().map(|x| x.value).collect();
                self.w.apply_suggestion(s, &found);
                self.refresh(data);
                None
            }
            Done::Measured(m) => {
                self.measures = m;
                self.refresh(data);
                None
            }
            Done::Failed(e) => {
                // shown where Next waits (the step's blocker), not only in the status line
                self.w.suggest_error = Some(e.clone());
                Some(format!("suggestion: {e}"))
            }
        }
    }
}

fn folder_text(f: &suggest::Folder) -> String {
    if f.sub.is_empty() { format!("data/{}", f.root) } else { format!("data/{}/{}", f.root, f.sub) }
}

pub fn page(app: &mut App, ui: &mut egui::Ui) {
    let repo = app.repo.clone();
    let groups = app.groups.clone();
    let open = app.open.as_ref().map(|o| &o.project);
    let Some(v) = &mut app.wizard else { return };
    if let Some(e) = v.poll(&repo.data) {
        app.message = Some(e);
    }
    let steps = v.w.steps();
    if !steps.contains(&v.step) {
        v.step = *steps.iter().rev().find(|s| **s != Step::Dynamics).unwrap_or(&steps[0]);
    }
    let at = steps.iter().position(|s| *s == v.step).unwrap_or(0);
    let items = match &v.report {
        Some(Ok(r)) => Some(r.items.len()),
        _ => None,
    };
    let mut go: Option<Step> = None;
    let (mut finish_now, mut cancel, mut settle, mut refresh, mut resuggest) = (false, false, false, false, false);
    let mut play: Option<PathBuf> = None;
    let mut picked_answer: Option<(String, Option<String>)> = None;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            ui.heading(if v.w.add_to.is_some() { "Add a source" } else { "New instrument" });
            for (k, s) in steps.iter().enumerate() {
                let text = format!("{}. {}", k + 1, s.title());
                if k < at {
                    if ui.selectable_label(false, text).on_hover_text("back to this step").clicked() {
                        go = Some(*s);
                    }
                } else if k == at {
                    ui.label(egui::RichText::new(text).strong());
                } else {
                    ui.label(egui::RichText::new(text).weak());
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            // the step's page wraps within what is left of the window
            ui.set_max_width(ui.available_width());
            ui.heading(v.step.title());
            match v.step {
                Step::Played => {
                    let donor = (!v.w.template.is_empty()).then(|| (v.w.template.as_str(), v.w.donor_why.as_str()));
                    picked_answer = played::page(ui, &v.kinds, &v.w.kind, &v.guess, donor, &v.menu_names);
                }
                Step::Folder => {
                    if let Some(f) = folder::page(ui, &repo, &mut v.folder, v.job.is_none()) {
                        let audio = suggest::audio_files(&repo.data, &f).len();
                        let sfz = suggest::sfz_files(&repo.data, &f);
                        v.counts = Some((audio, sfz.len()));
                        v.w.sfz = sfz.first().cloned();
                        v.w.sfz_files = sfz;
                        v.w.folder = Some(f);
                        resuggest = true;
                    }
                    if let Some(f) = &v.w.folder {
                        ui.label(format!("Chosen: {}", folder_text(f)));
                    }
                    if let Some((a, s)) = v.counts {
                        ui.label(format!("{a} audio files, {s} SFZ files"));
                    }
                    if v.w.add_to.is_some() {
                        ui.horizontal(|ui| {
                            ui.label("The new source's id");
                            ui.text_edit_singleline(&mut v.w.id);
                        });
                    }
                }
                Step::Reading => {
                    if let Some(c) = reading::page(ui, &v.w.sfz_files, &v.w.sfz, v.job.is_none()) {
                        v.w.sfz = c;
                        resuggest = true;
                    }
                }
                Step::Names => {
                    if let Some(r) = &mut v.w.rule {
                        match names::page(ui, r, v.preview.as_ref(), &v.sfz_folders, true, v.job.is_none()) {
                            names::Out::Changed => settle = true,
                            names::Out::ResetToParts => {
                                v.w.sfz = None;
                                resuggest = true;
                            }
                            names::Out::EditByHand | names::Out::Nothing => {}
                        }
                    }
                    check::findings(ui, &v.w.findings);
                }
                Step::Dynamics => {
                    if let Some(r) = &mut v.w.rule {
                        refresh = dynamics::page(ui, r, &v.stats, v.job.is_none(), v.job.is_some());
                    }
                }
                Step::Check => {
                    if let Some(values) = v.w.rule.as_ref().and_then(|r| r.group_values()).filter(|g| g.len() > 1) {
                        let old = v.w.shown_value().unwrap_or_default();
                        let mut shown = old.clone();
                        ui.horizontal_wrapped(|ui| {
                            ui.label("The source of");
                            egui::ComboBox::from_id_salt("wizard shown").selected_text(&shown).show_ui(ui, |ui| {
                                for x in &values {
                                    ui.selectable_value(&mut shown, x.clone(), x);
                                }
                            });
                            ui.label(egui::RichText::new("each value is a source of its own: its skips and relabels are its own; \
                                the layers and dynamics are the same for all").weak());
                        });
                        if shown != old {
                            v.w.shown = Some(shown);
                            refresh = true;
                        }
                    }
                    let thresholds = v.w.thresholds;
                    match check::page(ui, &mut v.check, v.report.as_ref(), &v.measures, &v.offsets, &v.w.findings, &thresholds, true) {
                        Some(Action::Play(p)) => play = Some(p),
                        Some(Action::Skip(n)) => {
                            v.w.skip(n);
                            refresh = true;
                        }
                        Some(Action::Relabel(from, to)) => {
                            v.w.relabel(from, to);
                            refresh = true;
                        }
                        Some(Action::SkipFile(rel)) => {
                            v.w.skip_file(&rel);
                            refresh = true;
                        }
                        Some(Action::UnskipFile(rel)) => {
                            v.w.unskip_file(&rel);
                            refresh = true;
                        }
                        Some(Action::Thresholds(t)) => v.w.thresholds = t,
                        None => {}
                    }
                }
                Step::Menu => {
                    steps::hint(ui, "The instrument's name (its folder under projects/), and how the plugin lists it. Finish writes the project.");
                    egui::Grid::new("wizard menu").num_columns(2).show(ui, |ui| {
                        ui.label("Name");
                        let mut name = v.w.name.clone();
                        if ui.add(egui::TextEdit::singleline(&mut name).hint_text("lower case, digits, _ (the project's folder)")).changed() {
                            v.w.set_name(&name);
                        }
                        ui.end_row();
                        ui.label("In the menu");
                        ui.text_edit_singleline(&mut v.w.menu_name);
                        ui.end_row();
                        ui.label("Group");
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("wizard group").selected_text(&v.w.menu_group).show_ui(ui, |ui| {
                                for g in &groups {
                                    ui.selectable_value(&mut v.w.menu_group, g.clone(), g);
                                }
                            });
                            ui.add(egui::TextEdit::singleline(&mut v.new_group).desired_width(160.0).hint_text("or a new group"));
                            if ui.add_enabled(!v.new_group.trim().is_empty(), egui::Button::new("Use")).clicked() {
                                v.w.menu_group = v.new_group.trim().to_string();
                                v.new_group.clear();
                            }
                        });
                        ui.end_row();
                    });
                    ui.checkbox(&mut v.w.build_now, "Build it now (the whole chain; minutes)");
                }
            }
            if let Some(j) = &v.job {
                j.bar(ui);
            }
            ui.add_space(10.0);
            ui.separator();
            let blocker = v.w.blocker(&v.ids, open, v.step, items).or_else(|| (v.step == Step::Check && !v.empty.is_empty())
                .then(|| format!("{} read no files: pick one above to see why", v.empty.join(", "))));
            ui.horizontal(|ui| {
                if at > 0 && ui.button("Back").clicked() {
                    go = Some(steps[at - 1]);
                }
                if at + 1 < steps.len() {
                    if ui.add_enabled(blocker.is_none(), egui::Button::new(format!("Next: {}", steps[at + 1].title()))).clicked() {
                        go = Some(steps[at + 1]);
                    }
                } else if ui.add_enabled(blocker.is_none(), egui::Button::new("Finish")).clicked() {
                    finish_now = true;
                }
                if ui.button("Cancel").on_hover_text("leave the wizard: nothing is written (a folder linked in is unlinked)").clicked() {
                    cancel = true;
                }
            });
            if let Some(b) = &blocker {
                ui.colored_label(WARN, b);
            }
        });
    });
    if settle {
        v.settle(&repo.data);
    } else if refresh {
        v.refresh(&repo.data);
    }
    if resuggest {
        v.suggest(&repo.data);
    }
    if let Some((k, d)) = picked_answer {
        v.take_answer(&k, d);
    }
    if let Some(s) = go {
        v.step = s;
        // the page opens on the answer the recordings pre-select
        if s == Step::Played && v.w.kind.is_empty() {
            if let Some(k) = v.guess.kind.clone() {
                v.take_answer(&k, None);
            }
        }
    }
    if let Some(p) = play {
        match &app.player {
            Ok(pl) => {
                if let Err(e) = pl.play(&p) {
                    app.message = Some(e);
                }
            }
            Err(e) => app.message = Some(format!("no playback: {e}")),
        }
    }
    if cancel {
        cancel_wizard(app);
    } else if finish_now {
        finish(app);
    }
}

/// the wizard left without Finish: nothing it made stays
pub fn cancel_wizard(app: &mut App) {
    if let Some(mut v) = app.wizard.take() {
        if let Some(e) = v.unlink(&app.repo, None) {
            app.message = Some(format!("unlink: {e}"));
        }
    }
}

fn finish(app: &mut App) {
    let Some(mut v) = app.wizard.take() else { return };
    let used = v.w.folder.as_ref().map(|f| f.root.split('/').next().unwrap_or_default().to_string());
    if v.w.add_to.is_none() {
        match wizard::finish(&app.repo, &v.w, &v.cache) {
            Ok(_) => {
                v.unlink(&app.repo, used.as_deref());
                app.names = project::project_names(&app.repo);
                app.groups = project::menu_groups(&app.repo);
                app.templates = project::templates(&app.repo);
                app.open_project(&v.w.name);
                if v.w.build_now {
                    if let Some(o) = &mut app.open {
                        o.page = Page::Build;
                    }
                    super::build::request_if_clear(app);
                }
            }
            Err(e) => {
                app.message = Some(format!("finish: {e}"));
                app.wizard = Some(v);
            }
        }
        return;
    }
    let Some(o) = &mut app.open else { return };
    match wizard::add_into(&mut o.project, &app.repo, &v.w) {
        Ok(ids) => {
            v.unlink(&app.repo, used.as_deref());
            o.page = Page::Source(ids[0].clone());
            o.edits = Default::default();
            app.message = Some(format!("added {} (save to keep)", ids.join(", ")));
        }
        Err(e) => {
            app.message = Some(format!("add: {e}"));
            app.wizard = Some(v);
        }
    }
}
