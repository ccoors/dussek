//! The build page: the chain's status (the project's steps, then the borrowed projects'), the
//! three ways to build, and a running build's step, time and log; Cancel restores the step's files.
use super::App;
use eframe::egui;
use pe_editor::driver::{self, Build, Event, Mode};
use pe_editor::{mapping, suggest};
use std::time::{Duration, Instant};

pub struct Running {
    build: Build,
    current: Option<(String, String, String, Instant)>,
    lines: Vec<String>,
    finished: Option<(bool, bool)>,
    tail: String,
    tail_read: Instant,
}

/// what keeps the project from being built: the project model's own problems (a string part
/// without a source of its name), sources whose map gives an error or no items, and dynamics
/// that name no layer of a source's items
fn blocked(app: &App) -> Vec<String> {
    let Some(o) = &app.open else { return Vec::new() };
    let mut out = o.project.build_problems();
    let mut unmapped = Vec::new();
    for id in o.project.source_ids() {
        let src = o.project.source_json(&id).unwrap_or_default();
        let root = src["root"].as_str().unwrap_or_default().to_string();
        match mapping::items(&src, &suggest::data_root(&app.repo.data, &root)) {
            Ok(r) if !r.items.is_empty() => out.extend(mapping::dynamics_problems(&src, &r.items)),
            _ => unmapped.push(id),
        }
    }
    if !unmapped.is_empty() {
        out.push(format!("these sources do not map: {}", unmapped.join(", ")));
    }
    out
}

/// what the last build showed, kept after it ended
pub struct LastRun {
    lines: Vec<String>,
    tail: String,
    verdict: String,
}

fn start(app: &mut App, mode: Mode) {
    let Some(o) = &mut app.open else { return };
    match Build::start(&app.repo, &o.project.name, &mode) {
        Ok(build) => {
            o.build = Some(Running { build, current: None, lines: Vec::new(), finished: None, tail: String::new(),
                tail_read: Instant::now() });
            app.message = Some(format!("building {} ({mode:?})", o.project.name));
        }
        Err(e) => app.message = Some(e),
    }
}

/// a build asked for: saved first when there are unsaved edits (the user confirms)
fn request(app: &mut App, mode: Mode) {
    let Some(o) = &mut app.open else { return };
    if o.project.dirty() {
        o.pending_build = Some(mode);
    } else {
        start(app, mode);
    }
}

/// blocked(), found again only when the project's text changed (it maps every source)
fn blocked_now(app: &mut App) -> Vec<String> {
    let Some(text) = app.open.as_ref().map(|o| o.project.text()) else { return Vec::new() };
    if let Some((t, b)) = app.open.as_ref().and_then(|o| o.blocked.as_ref()) {
        if *t == text {
            return b.clone();
        }
    }
    let b = blocked(app);
    if let Some(o) = &mut app.open {
        o.blocked = Some((text, b.clone()));
    }
    b
}

/// a build of the stale steps when nothing blocks it (the build page shows what does)
pub fn request_if_clear(app: &mut App) {
    if blocked_now(app).is_empty() {
        request(app, Mode::Stale);
    }
}

pub fn confirm_window(app: &mut App, ctx: &egui::Context) {
    let Some(o) = &mut app.open else { return };
    let Some(mode) = o.pending_build.clone() else { return };
    let (mut go, mut cancel) = (false, false);
    egui::Window::new("Unsaved edits").collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label("The steps read project.toml: save the edits and build?");
        ui.horizontal(|ui| {
            go = ui.button("Save and build").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    if cancel {
        o.pending_build = None;
    }
    if go {
        o.pending_build = None;
        app.save();
        if app.open.as_ref().is_some_and(|o| !o.project.dirty()) {
            start(app, mode);
        }
    }
}

pub fn poll(app: &mut App) {
    let mut ended = false;
    if let Some(o) = &mut app.open {
        if let Some(r) = &mut o.build {
            while let Ok(e) = r.build.events.try_recv() {
                match e {
                    Event::Start { project, step, kind, id, log } => {
                        r.current = Some((format!("{project}: {step}"), format!("{kind} {id}"), log, Instant::now()));
                    }
                    Event::Done { project, step, kind, id, seconds, .. } => {
                        r.lines.push(format!("done: {project}: {step} ({kind} {id}) {seconds:.1} s"));
                        r.current = None;
                    }
                    Event::Failed { project, step, kind, id, log, exit, seconds } => {
                        r.lines.push(format!("failed: {project}: {step} ({kind} {id}), exit {exit} after {seconds:.1} s; \
                            its files restored"));
                        r.tail = driver::log_tail(&log, 40);
                        r.current = None;
                    }
                    Event::Finished { ok, cancelled } => r.finished = Some((ok, cancelled)),
                }
            }
            if let Some((_, _, log, _)) = &r.current {
                if r.tail_read.elapsed() > Duration::from_millis(500) {
                    r.tail = driver::log_tail(log, 40);
                    r.tail_read = Instant::now();
                }
            }
            if let Some(code) = r.build.exit() {
                let (ok, cancelled) = r.finished.unwrap_or((false, false));
                let verdict = match (ok, cancelled) {
                    (true, _) => format!("{}: built", o.project.name),
                    (_, true) => format!("{}: cancelled; the running step's files restored", o.project.name),
                    _ => {
                        let err = r.build.stderr();
                        format!("{}: the build failed (exit {code}){}", o.project.name,
                            err.lines().last().map(|l| format!(": {l}")).unwrap_or_default())
                    }
                };
                o.last_run = Some(LastRun { lines: std::mem::take(&mut r.lines), tail: std::mem::take(&mut r.tail),
                    verdict: verdict.clone() });
                app.message = Some(verdict);
                ended = true;
            }
        }
        if ended {
            o.build = None;
            o.status = None;
        }
    }
    if ended {
        app.refresh_status();
    }
}

fn rows_table(ui: &mut egui::Ui, rows: &[driver::Row], building: bool) -> Option<Mode> {
    let mut mode = None;
    egui::Grid::new(ui.next_auto_id()).num_columns(4).striped(true).show(ui, |ui| {
        for r in rows {
            ui.label(&r.step);
            ui.label(format!("{} {}", r.kind, r.id));
            match &r.reason {
                None => ui.colored_label(egui::Color32::from_rgb(70, 150, 90), "fresh"),
                Some(why) => ui.colored_label(egui::Color32::from_rgb(200, 160, 40), why),
            };
            ui.horizontal(|ui| {
                if ui.add_enabled(!building, egui::Button::new("From here")).clicked() {
                    mode = Some(Mode::From(r.step.clone()));
                }
                if ui.add_enabled(!building, egui::Button::new("Only this")).clicked() {
                    mode = Some(Mode::Only(r.step.clone()));
                }
            });
            ui.end_row();
        }
    });
    mode
}

pub fn page(app: &mut App, ui: &mut egui::Ui) {
    let blocked = blocked_now(app);
    let Some(o) = &mut app.open else { return };
    let building = o.build.is_some() || !blocked.is_empty() || o.load_error.is_some();
    let mut ask: Option<Mode> = None;
    let mut cancel = false;
    let mut refresh = false;
    ui.horizontal(|ui| {
        if ui.add_enabled(!building, egui::Button::new("Build stale")).clicked() {
            ask = Some(Mode::Stale);
        }
        refresh = ui.add_enabled(o.build.is_none() && o.status_job.is_none(), egui::Button::new("Refresh status")).clicked();
        if o.status_job.is_some() {
            ui.spinner();
            ui.label("reading the status…");
        }
        if let Some(r) = &o.build {
            let label = if r.build.cancelled() { "Cancelling…" } else { "Cancel" };
            cancel = ui.add_enabled(!r.build.cancelled(), egui::Button::new(label))
                .on_hover_text("stops the running step and restores its files").clicked();
        }
    });
    for b in &blocked {
        ui.colored_label(egui::Color32::RED, format!("not built: {b}"));
    }
    if let Some(r) = &o.build {
        if let Some((what, scope, _, t)) = &r.current {
            ui.label(format!("running {what} ({scope}), {:.0} s", t.elapsed().as_secs_f32()));
        }
        for l in &r.lines {
            ui.label(l);
        }
        egui::ScrollArea::vertical().id_salt("log").max_height(260.0).stick_to_bottom(true).show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(&r.tail).monospace()).selectable(true));
        });
        ui.separator();
    } else if let Some(l) = &o.last_run {
        ui.label(egui::RichText::new(format!("last build: {}", l.verdict)).strong());
        for line in &l.lines {
            ui.label(line);
        }
        if !l.tail.is_empty() {
            egui::ScrollArea::vertical().id_salt("last log").max_height(260.0).stick_to_bottom(true).show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(&l.tail).monospace()).selectable(true));
            });
        }
        ui.separator();
    }
    match &o.status {
        None => {
            ui.label(if o.status_job.is_some() { "status: being read…" } else { "status: not read yet" });
        }
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::RED, format!("status: {e}"));
        }
        Some(Ok(s)) => {
            ui.heading(&s.project);
            if let Some(m) = rows_table(ui, &s.rows, building) {
                ask = Some(m);
            }
            for (other, rows) in &s.uses {
                egui::CollapsingHeader::new(format!("borrowed: {other} (its stale steps are built first)")).show(ui, |ui| {
                    rows_table(ui, rows, true);
                });
            }
        }
    }
    if cancel {
        if let Some(r) = &mut o.build {
            r.build.cancel();
        }
    }
    if refresh {
        app.refresh_status();
    }
    if let Some(m) = ask {
        request(app, m);
    }
}
