//! The window: the project list (left), the open project's pages (Project, one per source,
//! Build), a status line; the file is checked every second for outside edits.
pub mod build;
pub mod compare;
pub mod jobs;
pub mod projects;
pub mod roll;
pub mod source;
pub mod steps;
pub mod wizard;

use eframe::egui;
use pe_editor::driver;
use pe_editor::player::Player;
use pe_editor::project::{self, Disk, Project, Repo, Table, Template};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Project,
    Source(String),
    Build,
    Compare,
}

/// the rule form's text fields (the value as typed) and their parse errors, by table and key
#[derive(Default)]
pub struct Edits {
    text: HashMap<String, String>,
    errors: HashMap<String, String>,
    pub adding: HashMap<String, String>,
}

pub struct Open {
    pub project: Project,
    pub page: Page,
    pub status: Option<Result<driver::Status, String>>,
    /// the status being read (build.py runs on a thread of its own)
    pub status_job: Option<driver::StatusJob>,
    /// what keeps the project from being built, for the file's text it was found for
    pub blocked: Option<(String, Vec<String>)>,
    pub build: Option<build::Running>,
    pub sources: HashMap<String, source::SourceView>,
    /// the file changed on disk while there are unsaved edits: the user chooses
    pub conflict: bool,
    /// the file on disk does not parse: the last good view stays, read-only
    pub load_error: Option<String>,
    pub edits: Edits,
    pub pending_build: Option<driver::Mode>,
    /// what the last build showed (its lines, the log's tail, how it ended)
    pub last_run: Option<build::LastRun>,
    /// the Compare page's state (made when first shown; it follows the export, not the file)
    pub compare: Option<compare::CompareView>,
}

impl Open {
    /// the file read again: the pages' state dropped, a source page left when its source is gone
    fn reloaded(&mut self) {
        self.edits = Edits::default();
        self.sources.clear();
        self.page = page_still_there(self.page.clone(), &self.project.source_ids());
    }
}

/// the page after a reload: a source's page only while the source is there
fn page_still_there(page: Page, ids: &[String]) -> Page {
    match page {
        Page::Source(id) if !ids.contains(&id) => Page::Project,
        p => p,
    }
}

pub struct App {
    pub repo: Repo,
    pub names: Vec<String>,
    pub templates: Vec<Template>,
    /// the menu groups the projects use
    pub groups: Vec<String>,
    /// each family's steps (projects/steps.toml), read when first shown
    pub chains: HashMap<String, Vec<String>>,
    pub open: Option<Open>,
    pub message: Option<String>,
    pub player: Result<Player, String>,
    /// the wizard (a new instrument, or new sources of the open project) while it runs
    pub wizard: Option<wizard::WizardView>,
    /// a project chosen in the list while the wizard runs: opened once the user confirms
    pub leave_wizard: Option<String>,
    /// the New rolled piano page while it is open (no project), or a rolled project's controls
    pub roll: Option<roll::RollView>,
    /// Space and Tab pressed this frame while the Compare page is shown (play or stop, swap): taken
    /// out of the input before egui sees them, which would move the keyboard focus with Tab and
    /// click the focused widget with Space
    pub compare_keys: (bool, bool),
    /// the window was asked to close while there are unsaved edits or a build runs
    closing: bool,
    last_disk_check: Instant,
}

impl App {
    pub fn new(repo: Repo) -> App {
        App {
            names: project::project_names(&repo),
            templates: project::templates(&repo),
            groups: project::menu_groups(&repo),
            chains: HashMap::new(),
            repo,
            open: None,
            message: None,
            player: Player::new(),
            wizard: None,
            leave_wizard: None,
            roll: None,
            compare_keys: (false, false),
            closing: false,
            last_disk_check: Instant::now(),
        }
    }

    pub fn open_project(&mut self, name: &str) {
        if let Some(o) = &self.open {
            if o.build.is_some() {
                self.message = Some(format!("{}: a build runs (cancel it first)", o.project.name));
                return;
            }
            if o.project.dirty() {
                self.message = Some(format!("{}: save or discard the edits first", o.project.name));
                return;
            }
        }
        match Project::load(&self.repo, name) {
            Ok(project) => {
                self.open = Some(Open {
                    project,
                    page: Page::Project,
                    status: None,
                    status_job: None,
                    blocked: None,
                    build: None,
                    sources: HashMap::new(),
                    conflict: false,
                    load_error: None,
                    edits: Edits::default(),
                    pending_build: None,
                    last_run: None,
                    compare: None,
                });
                self.refresh_status();
                self.message = None;
            }
            Err(e) => self.message = Some(format!("{name}: {e}")),
        }
    }

    /// the wizard for a new instrument (refused, as opening another project, while the open one
    /// has unsaved edits or a build runs)
    pub fn start_wizard(&mut self) {
        if let Some(o) = &self.open {
            if o.build.is_some() {
                self.message = Some(format!("{}: a build runs (cancel it first)", o.project.name));
                return;
            }
            if o.project.dirty() {
                self.message = Some(format!("{}: save or discard the edits first", o.project.name));
                return;
            }
        }
        self.wizard = Some(wizard::WizardView::new(&self.repo, pe_editor::wizard::Wizard::new_instrument(), pe_editor::wizard::measures_cache()));
    }

    /// the status read again in the background (the one shown stays until it is in)
    pub fn refresh_status(&mut self) {
        if let Some(o) = &mut self.open {
            o.status_job = Some(driver::StatusJob::start(&self.repo, &o.project.name));
            o.blocked = None;
        }
    }

    fn poll_status(&mut self) {
        let Some(o) = &mut self.open else { return };
        let Some(s) = o.status_job.as_ref().and_then(|j| j.poll()) else { return };
        if o.status_job.as_ref().is_some_and(|j| j.project == o.project.name) {
            o.status = Some(s);
        }
        o.status_job = None;
    }

    pub fn editable(&self) -> bool {
        self.open.as_ref().is_some_and(|o| o.build.is_none() && o.load_error.is_none())
    }

    pub fn save(&mut self) {
        let Some(o) = &mut self.open else { return };
        match o.project.save() {
            Ok(()) => {
                self.message = Some(format!("{} saved", o.project.name));
                self.refresh_status();
            }
            Err(project::SaveError::Conflict) => o.conflict = true,
            Err(e) => self.message = Some(format!("save: {e}")),
        }
    }

    fn check_disk(&mut self) {
        if self.last_disk_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_disk_check = Instant::now();
        let Some(o) = &mut self.open else { return };
        match o.project.disk() {
            Disk::Same => {}
            Disk::Missing => self.message = Some(format!("{} is gone from disk", o.project.path.display())),
            Disk::Changed if o.project.dirty() => o.conflict = true,
            Disk::Changed => match o.project.reload() {
                Ok(()) => {
                    o.load_error = None;
                    o.reloaded();
                    self.message = Some(format!("{} reloaded (changed on disk)", o.project.name));
                    self.refresh_status();
                }
                Err(e) => o.load_error = Some(e.to_string()),
            },
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let editable = self.editable();
        let wizard = self.wizard.is_some();
        let repo = self.repo.clone();
        let Some(o) = &mut self.open else {
            ui.heading("No project open");
            return;
        };
        let mut save = false;
        let mut discard = false;
        ui.horizontal(|ui| {
            ui.heading(format!("{}{}", o.project.name, if o.project.dirty() { " •" } else { "" }));
            save = ui.add_enabled(editable && o.project.dirty(), egui::Button::new("Save")).clicked();
            discard = ui.add_enabled(editable && o.project.dirty(), egui::Button::new("Discard edits")).clicked();
            ui.separator();
            if wizard {
                ui.label("the wizard is open (finish or cancel it)");
                return;
            }
            ui.selectable_value(&mut o.page, Page::Project, "Project");
            for id in o.project.source_ids() {
                ui.selectable_value(&mut o.page, Page::Source(id.clone()), id);
            }
            ui.selectable_value(&mut o.page, Page::Build, if o.build.is_some() { "Build (running)" } else { "Build" });
            let export = pe_editor::compare::export_dir(&repo, &o.project.name).join("instrument.json").is_file();
            ui.add_enabled_ui(export, |ui| ui.selectable_value(&mut o.page, Page::Compare, "Compare"))
                .response.on_disabled_hover_text("no export yet: build the project first (instruments/<name>/)");
        });
        if let Some(e) = &o.load_error {
            ui.colored_label(egui::Color32::RED, format!("project.toml does not parse: {e} (shown as last loaded, read-only)"));
        }
        if discard {
            let _ = o.project.reload();
            o.reloaded();
        }
        if save {
            self.save();
        }
    }

    /// closing with unsaved edits or a running build: the user chooses (a build is cancelled on
    /// quitting: its running step's files restored)
    fn close_window(&mut self, ctx: &egui::Context) {
        let pending = self.wizard.is_some() || self.open.as_ref().is_some_and(|o| o.project.dirty() || o.build.is_some());
        if ctx.input(|i| i.viewport().close_requested()) && pending && !self.closing {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing = true;
        }
        if !self.closing {
            return;
        }
        let name = self.open.as_ref().map(|o| o.project.name.clone()).unwrap_or_default();
        let dirty = self.open.as_ref().is_some_and(|o| o.project.dirty());
        let building = self.open.as_ref().is_some_and(|o| o.build.is_some());
        let in_wizard = self.wizard.is_some();
        let (mut save, mut quit, mut stay) = (false, false, false);
        egui::Window::new("Quit").collapsible(false).resizable(false).show(ctx, |ui| {
            if dirty {
                ui.label(format!("{name} has unsaved edits."));
            }
            if in_wizard {
                ui.label("The wizard's answers are lost.");
            }
            if building {
                ui.label("A build runs: quitting cancels it (its running step's files are restored).");
            }
            ui.horizontal(|ui| {
                save = dirty && !building && ui.button("Save and quit").clicked();
                quit = ui.button(if dirty { "Quit without saving" } else { "Quit" }).clicked();
                stay = ui.button("Stay").clicked();
            });
        });
        if save {
            self.save();
            quit = self.open.as_ref().is_some_and(|o| !o.project.dirty());
        }
        if stay {
            self.closing = false;
        }
        if quit {
            wizard::cancel_wizard(self);
            self.open = None; // a running build is cancelled and waited for (Build's Drop)
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// a project chosen in the list while the wizard is open: the user confirms losing its answers
    fn leave_window(&mut self, ctx: &egui::Context) {
        let Some(name) = self.leave_wizard.clone() else { return };
        let (mut go, mut stay) = (false, false);
        egui::Window::new("Leave the wizard?").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("Its answers are lost (nothing was written; a folder it linked in is unlinked).");
            ui.horizontal(|ui| {
                go = ui.button(format!("Leave it and open {name}")).clicked();
                stay = ui.button("Stay").clicked();
            });
        });
        if go {
            wizard::cancel_wizard(self);
            self.leave_wizard = None;
            self.open_project(&name);
        }
        if stay {
            self.leave_wizard = None;
        }
    }

    fn conflict_window(&mut self, ctx: &egui::Context) {
        let Some(o) = &mut self.open else { return };
        if !o.conflict {
            return;
        }
        let (mut keep, mut take) = (false, false);
        egui::Window::new("Changed on disk").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(format!("{} changed on disk while you have unsaved edits.", o.project.path.display()));
            ui.horizontal(|ui| {
                keep = ui.button("Keep mine (save over it)").clicked();
                take = ui.button("Take the file's (drop my edits)").clicked();
            });
        });
        if keep {
            if let Err(e) = o.project.save_over() {
                self.message = Some(format!("save: {e}"));
            }
            o.conflict = false;
        }
        if take {
            match o.project.reload() {
                Ok(()) => o.load_error = None,
                Err(e) => o.load_error = Some(e.to_string()),
            }
            o.reloaded();
            o.conflict = false;
        }
    }
}

/// a table's keys as rows: the value as TOML text, applied as typed whenever it parses; a value
/// that does not parse stays in its field (red) with the error. Returns true on a change.
pub fn key_rows(ui: &mut egui::Ui, project: &mut Project, t: &Table, known: &[&str], edits: &mut Edits, enabled: bool) -> bool {
    key_rows_but(ui, project, t, known, &[], edits, enabled)
}

/// key_rows without the keys `shown_elsewhere` (a source's id, role and kind have their own controls)
pub fn key_rows_but(ui: &mut egui::Ui, project: &mut Project, t: &Table, known: &[&str], shown_elsewhere: &[&str],
                    edits: &mut Edits, enabled: bool) -> bool {
    let mut changed = false;
    let tag = format!("{t:?}");
    let mut remove = None;
    egui::Grid::new(format!("keys {tag}")).num_columns(3).striped(true).show(ui, |ui| {
        for (key, value) in project.keys(t).into_iter().filter(|(k, _)| !shown_elsewhere.contains(&k.as_str())) {
            let id = format!("{tag}/{key}");
            ui.label(&key);
            let text = edits.text.entry(id.clone()).or_insert_with(|| value.clone());
            let bad = edits.errors.contains_key(&id);
            let field = egui::TextEdit::singleline(text).text_color_opt(bad.then_some(egui::Color32::RED))
                .font(egui::TextStyle::Monospace);
            let w = (ui.available_width() - 110.0).clamp(200.0, 560.0);
            let r = ui.add_enabled(enabled, |ui: &mut egui::Ui| ui.add_sized([w, 22.0], field));
            // applied as typed, whenever the text parses (the grid follows; a click on a tab changes
            // the page before the field could report losing its focus)
            if (r.changed() || r.lost_focus()) && *text != value {
                match text.parse::<toml_edit::Value>() {
                    Ok(v) => {
                        if project.set(t, &key, v).is_ok() {
                            changed = true;
                        }
                        edits.errors.remove(&id);
                    }
                    Err(e) => {
                        edits.errors.insert(id.clone(), e.to_string());
                    }
                }
            }
            // left without a change, or applied: the row shows the project's value again (an edit made by
            // another control, Skip this key or Suggest, is not overwritten by stale text)
            if !r.has_focus() && !edits.errors.contains_key(&id) {
                edits.text.remove(&id);
            }
            if ui.add_enabled(enabled, egui::Button::new("Remove")).on_hover_text("remove the key").clicked() {
                remove = Some(key.clone());
            }
            ui.end_row();
            if let Some(e) = edits.errors.get(&id) {
                ui.label("");
                ui.colored_label(egui::Color32::RED, e);
                ui.end_row();
            }
        }
    });
    if let Some(k) = remove {
        let _ = project.remove(t, &k);
        changed = true;
    }
    let present: Vec<String> = project.keys(t).into_iter().map(|(k, _)| k).collect();
    let free: Vec<&&str> = known.iter().filter(|k| !present.iter().any(|p| p == **k)).collect();
    if enabled && !free.is_empty() {
        ui.horizontal(|ui| {
            let pick = edits.adding.entry(tag.clone()).or_default();
            egui::ComboBox::from_id_salt(format!("add {tag}"))
                .selected_text(if pick.is_empty() { "add a key…" } else { pick.as_str() })
                .show_ui(ui, |ui| {
                    for k in &free {
                        ui.selectable_value(pick, k.to_string(), **k);
                    }
                });
            if ui.add_enabled(!pick.is_empty(), egui::Button::new("Add")).clicked() {
                let _ = project.set(t, pick, toml_edit::Value::from(""));
                pick.clear();
                changed = true;
            }
        });
    }
    changed
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.compare_keys = (false, false);
        // a text field typing keeps them
        let on_compare = self.wizard.is_none() && self.open.as_ref().is_some_and(|o| o.page == Page::Compare);
        if !on_compare || ctx.text_edit_focused() {
            return;
        }
        let keys = &mut self.compare_keys;
        raw.events.retain(|e| match e {
            egui::Event::Key { key: k @ (egui::Key::Space | egui::Key::Tab), pressed, repeat, modifiers, .. } if modifiers.is_none() => {
                if *pressed && !*repeat {
                    if *k == egui::Key::Space { keys.0 = true } else { keys.1 = true }
                }
                false
            }
            egui::Event::Text(t) => t != " ",
            _ => true,
        });
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.check_disk();
        self.poll_status();
        build::poll(self);
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        egui::Panel::top("bar").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.label(self.message.clone().unwrap_or_default());
        });
        egui::Panel::left("projects").resizable(true).default_size(230.0).show(ui, |ui| projects::list(self, ui));
        egui::CentralPanel::default().show(ui, |ui| {
            let page = self.open.as_ref().map(|o| o.page.clone());
            if self.wizard.is_some() {
                egui::ScrollArea::vertical().id_salt("wizard").show(ui, |ui| wizard::page(self, ui));
                return;
            }
            if self.roll.as_ref().is_some_and(|r| r.existing.is_none()) {
                egui::ScrollArea::vertical().id_salt("roll").show(ui, |ui| roll::page(self, ui));
                return;
            }
            egui::ScrollArea::vertical().id_salt("page").show(ui, |ui| match page {
                None => {
                    ui.label("Open a project on the left, or make a new one.");
                }
                Some(Page::Project) => projects::page(self, ui),
                Some(Page::Source(id)) => source::page(self, ui, &id),
                Some(Page::Build) => build::page(self, ui),
                Some(Page::Compare) => compare::page(self, ui),
            });
        });
        let ctx = ui.ctx().clone();
        build::confirm_window(self, &ctx);
        self.conflict_window(&ctx);
        self.leave_window(&ctx);
        self.close_window(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    fn a_page_of_a_source_gone_falls_back_to_the_project() {
        let ids = ["a".to_string()];
        assert_eq!(super::page_still_there(Page::Source("b".into()), &ids), Page::Project);
        assert_eq!(super::page_still_there(Page::Source("a".into()), &ids), Page::Source("a".into()));
        assert_eq!(super::page_still_there(Page::Build, &ids), Page::Build);
    }
}
