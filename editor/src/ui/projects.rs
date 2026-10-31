//! The project list, the new-project window and the project page ([project], [menu], [uses] as
//! forms; the settings sections as written; the sources).
use super::{App, Page, key_rows};
use eframe::egui;
use pe_editor::project::{self, Table};

const PROJECT_KEYS: [&str; 3] = ["family", "body", "skip"];
const MENU_KEYS: [&str; 5] = ["name", "group", "labels", "mallets", "hidden"];
const USES_KEYS: [&str; 7] = ["pedal", "dampers", "release", "levels", "felt", "buff", "main"];
const SHOWN: [&str; 3] = ["strings", "bars", "export"];

pub fn list(app: &mut App, ui: &mut egui::Ui) {
    if ui.button("New instrument…").on_hover_text("a new project from a sample folder, step by step").clicked() {
        app.start_wizard();
    }
    if ui.button("New rolled piano…").on_hover_text("a new piano from the measured ones: roll, adjust by ear, name it").clicked() {
        app.wizard = None;
        app.roll = Some(super::roll::RollView::new(&app.repo));
    }
    ui.separator();
    let current = app.open.as_ref().map(|o| o.project.name.clone());
    let mut pick = None;
    egui::ScrollArea::vertical().id_salt("list").show(ui, |ui| {
        for n in &app.names {
            if ui.selectable_label(current.as_deref() == Some(n.as_str()), n).clicked() {
                pick = Some(n.clone());
            }
        }
    });
    if let Some(n) = pick {
        // leaving a roll: its preview goes (a git-ignored folder the plugin would list on)
        if app.roll.take().is_some() {
            pe_editor::roll::remove_preview(&app.repo);
        }
        if app.wizard.is_some() {
            app.leave_wizard = Some(n);
        } else {
            app.open_project(&n);
        }
    }
}

pub fn page(app: &mut App, ui: &mut egui::Ui) {
    let editable = app.editable();
    let names = app.names.clone();
    let groups = app.groups.clone();
    let Some(o) = &mut app.open else { return };
    let family = o.project.get(&Table::top("project"), "family").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    if family == "rolled" {
        let name = o.project.name.clone();
        super::roll::project_page(app, ui, &name);
        return;
    }
    let chain = app.chains.entry(family.clone()).or_insert_with(|| project::chain(&app.repo, &family)).clone();
    let (menu, uses, proj) = (Table::top("menu"), Table::top("uses"), Table::top("project"));
    let mut changed = false;
    super::steps::hint(ui, "The instrument's name and group in the plugin's menu, what it borrows from other projects, \
        and the steps it skips.");
    ui.heading("In the plugin's menu");
    egui::Grid::new("menu").num_columns(2).show(ui, |ui| {
        ui.label("Name");
        let mut name = o.project.get(&menu, "name").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        if ui.add_enabled(editable, egui::TextEdit::singleline(&mut name).desired_width(260.0)).changed() {
            changed |= o.project.set(&menu, "name", toml_edit::Value::from(name.as_str())).is_ok();
        }
        ui.end_row();
        ui.label("Group");
        let old = o.project.get(&menu, "group").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        let mut g = old.clone();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt("menu group").selected_text(&g).show_ui(ui, |ui| {
                    for x in &groups {
                        ui.selectable_value(&mut g, x.clone(), x);
                    }
                });
            });
            let typed = o.edits.adding.entry("new group".into()).or_default();
            ui.add_enabled(editable, egui::TextEdit::singleline(typed).desired_width(160.0).hint_text("or a new group"));
            if ui.add_enabled(editable && !typed.trim().is_empty(), egui::Button::new("Use")).clicked() {
                g = typed.trim().to_string();
                typed.clear();
            }
        });
        if g != old {
            changed |= o.project.set(&menu, "group", toml_edit::Value::from(g.as_str())).is_ok();
        }
        ui.end_row();
    });
    ui.heading("Borrowed from other projects");
    ui.label(egui::RichText::new("a role's results taken from another project instead of fitted from this one's files").weak());
    egui::Grid::new("uses").num_columns(2).striped(true).show(ui, |ui| {
        for k in USES_KEYS {
            ui.label(k);
            let old = o.project.get(&uses, k).and_then(|v| v.as_str().map(String::from));
            let mut pick = old.clone();
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt(("uses", k)).selected_text(pick.clone().unwrap_or_else(|| "its own".into())).show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, None, "its own");
                    for n in names.iter().filter(|n| **n != o.project.name) {
                        ui.selectable_value(&mut pick, Some(n.clone()), n);
                    }
                });
            });
            if pick != old {
                changed |= match &pick {
                    Some(n) => o.project.set(&uses, k, toml_edit::Value::from(n.as_str())).is_ok(),
                    None => o.project.remove(&uses, k).is_ok(),
                };
            }
            ui.end_row();
        }
    });
    ui.heading("Steps skipped");
    let skip: Vec<String> = o.project.get(&proj, "skip").and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter().filter_map(|v| v.as_str().map(String::from)).collect();
    ui.horizontal_wrapped(|ui| {
        for s in &chain {
            let mut on = skip.contains(s);
            if ui.add_enabled(editable, egui::Checkbox::new(&mut on, s)).changed() {
                let next = project::skip_toggled(&skip, &chain, s, on);
                changed |= if next.is_empty() {
                    o.project.remove(&proj, "skip").is_ok()
                } else {
                    o.project.set(&proj, "skip", project::json_to_toml(&serde_json::json!(next))).is_ok()
                };
            }
        }
    });
    egui::CollapsingHeader::new("Advanced: every key of [project], [menu] and [uses] as text").show(ui, |ui| {
        ui.label(egui::RichText::new("[project]").strong());
        changed |= key_rows(ui, &mut o.project, &proj, &PROJECT_KEYS, &mut o.edits, editable);
        ui.label(egui::RichText::new("[menu]").strong());
        changed |= key_rows(ui, &mut o.project, &menu, &MENU_KEYS, &mut o.edits, editable);
        ui.label(egui::RichText::new("[uses]").strong());
        changed |= key_rows(ui, &mut o.project, &uses, &USES_KEYS, &mut o.edits, editable);
    });
    for name in SHOWN {
        if let Some(text) = o.project.section_text(name) {
            egui::CollapsingHeader::new(format!("[{name}] (edit it in the file)")).show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(text).monospace()).selectable(true));
            });
        }
    }
    ui.separator();
    ui.heading("Sources");
    let ids = o.project.source_ids();
    let mut action: Option<(String, i32)> = None;
    for id in &ids {
        ui.horizontal(|ui| {
            let role = o.project.get(&Table::Source(id.clone()), "role").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            if ui.link(format!("{id} ({role})")).clicked() {
                action = Some((id.clone(), 0));
            }
            if ui.add_enabled(editable, egui::Button::new("Up")).clicked() {
                action = Some((id.clone(), -1));
            }
            if ui.add_enabled(editable, egui::Button::new("Down")).clicked() {
                action = Some((id.clone(), 1));
            }
            if ui.add_enabled(editable, egui::Button::new("Remove")).on_hover_text("remove the source").clicked() {
                action = Some((id.clone(), 2));
            }
        });
    }
    let mut add = None;
    if ui.add_enabled(editable, egui::Button::new("Add a source…")).on_hover_text("new sources from a sample folder, step by step").clicked() {
        let cache = app.repo.projects.join(&o.project.name).join("cache/editor/audio.json");
        add = Some(super::wizard::WizardView::new(&app.repo, pe_editor::wizard::Wizard::add_to(&app.repo, &o.project), cache));
    }
    if let Some((id, a)) = action {
        let r = match a {
            0 => {
                o.page = Page::Source(id.clone());
                Ok(())
            }
            -1 => o.project.move_source(&id, true),
            1 => o.project.move_source(&id, false),
            _ => o.project.remove_source(&id),
        };
        if let Err(e) = r {
            app.message = Some(e);
        }
        o.edits = Default::default();
    }
    let _ = changed;
    if let Some(w) = add {
        app.wizard = Some(w);
    }
}
