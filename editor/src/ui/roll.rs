//! Rolling a piano: the New rolled piano page and a rolled project's page. A change stops the roll
//! running and builds the candidate in the background once it settles (400 ms, no mouse button
//! held), writing it where the plugin looks ("Rolled piano (preview)": the standalone reloads it);
//! Finish writes the project and builds it.
use super::App;
use eframe::egui;
use pe_editor::project::{self, Repo, write_atomic};
use pe_editor::roll::{self, Candidates, Rng};
use dussek::roll::generate::{CANCELLED, Report, references};
use dussek::roll::recipe::Recipe;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

type Done = Result<(Report, Vec<(f64, f64)>), String>;

/// A roll running in the background.
struct Job {
    rx: Receiver<Done>,
    started: Instant,
    /// a draw's (Roll again): pulled back until it passes the realism check, the sliders then set
    /// to what was built
    pull_back: bool,
}

pub struct RollView {
    pub recipe: Recipe,
    cands: Candidates,
    /// the references, each on or off
    refs_on: Vec<(String, bool)>,
    /// the grands the rolls draw strings and fine structure from
    allowed_on: Vec<(String, bool)>,
    /// project → its menu name
    names: HashMap<String, String>,
    rng: Rng,
    job: Option<Job>,
    /// a change waiting to roll: since when, and whether it was a draw
    changed_at: Option<(Instant, bool)>,
    /// a mouse button held at the last poll (a slider being dragged)
    held: bool,
    report: Option<Report>,
    spans: Vec<(f64, f64)>,
    error: Option<String>,
    /// the rolled project this view edits (None: a new one)
    pub existing: Option<String>,
}

fn menu_names(repo: &Repo) -> HashMap<String, String> {
    project::templates(repo).into_iter().map(|t| (t.name, t.menu_name)).collect()
}

impl RollView {
    pub fn new(repo: &Repo) -> RollView {
        let cands = roll::candidates(repo);
        let seed = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let mut rng = Rng::new(seed);
        let taken = project::project_names(repo);
        let i = (1..).find(|i| !taken.contains(&format!("rolled_{i}"))).unwrap_or(1);
        let mut recipe = roll::draw(&cands, &cands.grands, &mut rng, &format!("rolled_{i}"));
        recipe.menu.0 = format!("Rolled {i}");
        let mut v = RollView::with(repo, recipe, cands, rng, None);
        v.changed(true);
        v
    }

    pub fn from_project(repo: &Repo, name: &str) -> Result<RollView, String> {
        let text = std::fs::read_to_string(repo.projects.join(name).join("project.toml")).map_err(|e| e.to_string())?;
        let recipe = Recipe::parse(&text, name).map_err(|e| e.0)?;
        let cands = roll::candidates(repo);
        Ok(RollView::with(repo, recipe.clone(), cands, Rng::new(recipe.seed ^ 0x5EED), Some(name.to_string())))
    }

    fn with(repo: &Repo, recipe: Recipe, cands: Candidates, rng: Rng, existing: Option<String>) -> RollView {
        let refs_on = cands.references.iter().map(|r| (r.clone(), recipe.references.contains(r))).collect();
        let allowed_on = cands.grands.iter().map(|g| (g.clone(), true)).collect();
        RollView { recipe, cands, refs_on, allowed_on, names: menu_names(repo), rng, job: None, changed_at: None, held: false, report: None, spans: Vec::new(), error: None, existing }
    }

    fn allowed(&self) -> Vec<String> {
        self.allowed_on.iter().filter(|a| a.1).map(|a| a.0.clone()).collect()
    }

    fn name(&self, p: &str) -> String {
        self.names.get(p).cloned().unwrap_or_else(|| p.to_string())
    }

    /// A change: the running roll stops at its next stage (its result would be stale) and the new
    /// one waits for the change to settle. `draw`: Roll again's (pulled back until it passes).
    fn changed(&mut self, draw: bool) {
        if self.job.take().is_some() {
            roll::begin_preview();
        }
        self.changed_at = Some((Instant::now(), draw));
    }

    /// a finished roll taken in; a new one started 400 ms after the last change, once no mouse
    /// button is held (a slider rolls when let go)
    fn poll(&mut self, repo: &Repo, held: bool) {
        self.held = held;
        if let Some(done) = self.job.as_ref().and_then(|j| j.rx.try_recv().ok()) {
            let job = self.job.take().expect("a job");
            match done {
                Ok((report, spans)) => {
                    if job.pull_back && report.pulled_back > 0 {
                        roll::adopt_knobs(&mut self.recipe, &report.knobs);
                    }
                    self.report = Some(report);
                    self.spans = spans;
                    self.error = None;
                }
                Err(e) if e == CANCELLED => {}
                Err(e) => self.error = Some(e),
            }
        }
        if let Some((t, draw)) = self.changed_at
            && self.job.is_none()
            && !held
            && t.elapsed() >= Duration::from_millis(400)
        {
            self.changed_at = None;
            let (tx, rx) = channel();
            let (repo, recipe) = (repo.clone(), self.recipe.clone());
            let generation = roll::begin_preview();
            std::thread::spawn(move || {
                // the roll first (it measures the references, and stops when superseded), then the
                // sliders' spans from the references it measured
                let done = roll::preview(&repo, &recipe, generation, draw)
                    .and_then(|report| Ok((report, references(&recipe.references, &roll::instruments(&repo)).map_err(|e| e.0)?.spans)));
                let _ = tx.send(done);
            });
            self.job = Some(Job { rx, started: Instant::now(), pull_back: draw });
        }
    }

    /// the grands of the table: the candidates, and any other the recipe names, by menu name
    fn grands(&self) -> Vec<String> {
        let mut g = self.cands.grands.clone();
        for n in self.recipe.strings.iter().map(|s| &s.0).chain(&self.recipe.fine_structure) {
            if !g.contains(n) {
                g.push(n.clone());
            }
        }
        g.sort_by_key(|p| self.name(p));
        g
    }

    /// the controls (both pages): rolls, the body, the donors, the references, the verdict
    fn controls(&mut self, ui: &mut egui::Ui) {
        let (mut changed, mut draw) = (false, false);
        let can_roll = !self.allowed().is_empty();
        if !can_roll {
            ui.label("Allow at least one grand to give strings and fine structure (References and sources).");
        }
        ui.add_enabled_ui(can_roll, |ui| ui.horizontal(|ui| {
            if ui.button("Roll again").on_hover_text("new strings, a new fine structure and a new body (pulled back toward the middle until it passes the realism check)").clicked() {
                let allowed = self.allowed();
                let r = roll::draw(&self.cands, &allowed, &mut self.rng, &self.recipe.name);
                self.recipe = Recipe { name: self.recipe.name.clone(), menu: self.recipe.menu.clone(), references: self.recipe.references.clone(), ..r };
                (changed, draw) = (true, true);
            }
            if ui.button("Re-roll strings").on_hover_text("three grands' strings and hammers at new weights; the body and the fine structure stay").clicked() {
                let allowed = self.allowed();
                roll::reroll_strings(&mut self.recipe, &allowed, &mut self.rng);
                changed = true;
            }
            if ui.button("Re-roll fine structure").on_hover_text("a new fine structure (as another microphone or room); the rest stays").clicked() {
                let allowed = self.allowed();
                roll::reroll_fine(&mut self.recipe, &allowed, &mut self.rng);
                changed = true;
            }
        }));
        ui.add_space(6.0);
        ui.heading("The body");
        let span = |i: usize, s: &[(f64, f64)]| s.get(i).map_or((-2.5, 2.5), |&(a, b)| (a - 0.5, b + 0.5));
        let ((w0, w1), (a0, a1)) = (span(0, &self.spans), span(1, &self.spans));
        egui::Grid::new("roll body").num_columns(3).show(ui, |ui| {
            ui.label("cold, clear");
            changed |= ui.add(egui::Slider::new(&mut self.recipe.warmth, w0..=w1)).on_hover_text("Warmth: how much bass the body radiates against treble").changed();
            ui.label("warm, round");
            ui.end_row();
            ui.label("dry");
            changed |= ui.add(egui::Slider::new(&mut self.recipe.air, a0..=a1)).on_hover_text("Air: how long the body's response rings").changed();
            ui.label("airy");
            ui.end_row();
        });
        if self.recipe.warmth > 0.5 {
            ui.small("Toward the warm end rolls sounded unbalanced; the realism check is likely to name it.");
        }
        ui.add_space(6.0);
        ui.heading("The strings and hammers, the fine structure");
        let heaviest = self.recipe.strings.iter().fold(None::<&(String, f64)>, |b, s| match b {
            Some(b) if b.1 >= s.1 => Some(b),
            _ => Some(s),
        }).map(|s| s.0.clone());
        egui::Grid::new("roll donors").num_columns(3).striped(true).show(ui, |ui| {
            ui.label("");
            ui.label("Strings and hammers").on_hover_text("each key's strings and hammers blended from these grands at these shares");
            ui.label("Fine structure").on_hover_text("the body's responses mixed from these grands' (as another microphone or room)");
            ui.end_row();
            for g in self.grands() {
                let label = self.name(&g);
                if heaviest.as_deref() == Some(g.as_str()) {
                    ui.label(format!("{label} (frame)")).on_hover_text("the heaviest share: everything else of the strings' piano (dampers, noises, the board) comes from it");
                } else {
                    ui.label(label);
                }
                let mut pct = self.recipe.strings.iter().find(|s| s.0 == g).map_or(0.0, |s| s.1 * 100.0);
                // whole percents shown, the value left as it is (rounding it was a change every frame)
                let slider = egui::Slider::new(&mut pct, 0.0..=100.0).custom_formatter(|v, _| format!("{v:.0} %")).custom_parser(|t| t.trim_end_matches('%').trim().parse().ok());
                if ui.add(slider).changed() {
                    roll::set_weight(&mut self.recipe.strings, &g, pct / 100.0);
                    changed = true;
                }
                let mut on = self.recipe.fine_structure.contains(&g);
                let last = on && self.recipe.fine_structure.len() == 1;
                let r = ui.add_enabled(!last, egui::Checkbox::without_text(&mut on));
                if r.changed() && roll::set_fine(&mut self.recipe.fine_structure, &g, on) {
                    changed = true;
                }
                ui.end_row();
            }
        });
        if self.recipe.strings.iter().any(|s| s.1 > 0.55) {
            ui.small("One grand above 55 %: rolls like that sounded like that grand.");
        }
        egui::CollapsingHeader::new("References and sources").show(ui, |ui| {
            ui.label("Measured against (the knobs' directions and the realism check):");
            for (r, on) in &mut self.refs_on {
                let label = self.names.get(r).cloned().unwrap_or_else(|| r.clone());
                changed |= ui.checkbox(on, label).changed();
            }
            ui.label("Roll again and the re-rolls draw from:");
            for (g, on) in &mut self.allowed_on {
                let label = self.names.get(g).cloned().unwrap_or_else(|| g.clone());
                ui.checkbox(on, label);
            }
        });
        if changed {
            self.recipe.references = self.refs_on.iter().filter(|r| r.1).map(|r| r.0.clone()).collect();
            self.changed(draw);
        }
        ui.add_space(6.0);
        ui.separator();
        if let Some(j) = &self.job {
            ui.horizontal(|ui| {
                ui.spinner();
                let first = if self.spans.is_empty() { " (the first roll also measures the references: up to half a minute)" } else { "" };
                ui.label(format!("Rolling… {:.0} s{first}", j.started.elapsed().as_secs_f64()));
            });
        } else if self.changed_at.is_some() {
            ui.label(if self.held { "Rolling when you let go…" } else { "Rolling soon…" });
        }
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(200, 60, 60), e);
        }
        if let Some(r) = &self.report {
            if r.pulled_back > 0 {
                ui.label(format!("The draw was pulled back {}× toward the middle to pass the realism check: the sliders show what was built.", r.pulled_back));
            }
            if r.failures.is_empty() {
                ui.label(format!("Passes the realism check ({:.0} s).", r.seconds));
            } else {
                ui.label("Built as set; the realism check names (a guide: your ear decides):");
                for f in &r.failures {
                    ui.label(format!("  {f}"));
                }
            }
        }
        ui.label("Playing in the plugin as “Rolled piano (preview)”: it reloads within a second of each roll.");
    }
}

/// The New rolled piano page (in place of the project pages while it is open).
pub fn page(app: &mut App, ui: &mut egui::Ui) {
    let repo = app.repo.clone();
    let held = ui.input(|i| i.pointer.any_down());
    let Some(v) = &mut app.roll else { return };
    v.poll(&repo, held);
    ui.heading("New rolled piano");
    super::steps::hint(ui, "A new piano from the measured ones: strings blended from three grands, a body set by \
        warmth and air, a fine structure mixed from real responses, checked against the real pianos.");
    v.controls(ui);
    ui.add_space(6.0);
    ui.separator();
    let (mut finish, mut cancel) = (false, false);
    egui::Grid::new("roll name").num_columns(2).show(ui, |ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut v.recipe.name).on_hover_text("lower case letters, digits, underscores: the folder under projects/");
        ui.end_row();
        ui.label("In the plugin's menu");
        ui.text_edit_singleline(&mut v.recipe.menu.0);
        ui.end_row();
    });
    let taken = app.names.contains(&v.recipe.name);
    if taken {
        ui.label("That name is taken.");
    } else if !roll::valid_name(&v.recipe.name) {
        ui.label("Lower case letters, digits and underscores only.");
    }
    ui.horizontal(|ui| {
        finish = ui.add_enabled(!taken && roll::valid_name(&v.recipe.name), egui::Button::new("Finish")).on_hover_text("write the project and build it").clicked();
        cancel = ui.button("Cancel").clicked();
    });
    ui.ctx().request_repaint_after(Duration::from_millis(200));
    if cancel {
        roll::remove_preview(&repo);
        app.roll = None;
    } else if finish {
        let recipe = v.recipe.clone();
        match roll::finish(&repo, &recipe) {
            Ok(()) => {
                app.roll = None;
                app.names = project::project_names(&repo);
                app.open_project(&recipe.name);
                super::build::request_if_clear(app);
            }
            Err(e) => app.message = Some(e),
        }
    }
}

/// A rolled project's page: the same controls from its file; Save rewrites it (the build page builds it).
pub fn project_page(app: &mut App, ui: &mut egui::Ui, name: &str) {
    let repo = app.repo.clone();
    if app.roll.as_ref().and_then(|v| v.existing.as_deref()) != Some(name) {
        match RollView::from_project(&repo, name) {
            Ok(v) => app.roll = Some(v),
            Err(e) => {
                ui.label(format!("{name}: {e}"));
                return;
            }
        }
    }
    let held = ui.input(|i| i.pointer.any_down());
    let Some(v) = &mut app.roll else { return };
    v.poll(&repo, held);
    ui.heading(format!("{} (rolled)", v.recipe.menu.0));
    v.controls(ui);
    ui.add_space(6.0);
    if ui.button("Save").on_hover_text("write these settings to the project (build it on the Build page)").clicked() {
        // the file's other lines (comments, tables of its own) kept
        let path = repo.projects.join(name).join("project.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| v.recipe.update_toml(&t).map_err(|e| e.0));
        app.message = Some(match text.and_then(|t| write_atomic(&path, &t).map_err(|e| e.to_string())) {
            Ok(()) => format!("{name} saved"),
            Err(e) => format!("save: {e}"),
        });
    }
    ui.ctx().request_repaint_after(Duration::from_millis(200));
}
