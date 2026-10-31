//! The source settings page: the role, the mallet's name, the tail cut, the octave, and the
//! lists the check page fills (the keys left out, the keys relabeled) with the cuts and gains
//! per key and dynamic: each with its own control and a line on what it does.
use super::hint;
use eframe::egui;
use pe_editor::project::{Project, ROLES, Table, json_to_toml};
use pe_editor::view::note_label;
use serde_json::{Map, Value as Json, json};

fn put(p: &mut Project, t: &Table, k: &str, v: Option<Json>) -> bool {
    match v {
        Some(v) => p.set(t, k, json_to_toml(&v)).is_ok(),
        None => p.remove(t, k).is_ok(),
    }
}

fn explain(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).weak());
}

fn key_name(k: &str) -> String {
    k.trim().parse::<i64>().map(|n| format!("{} ({n})", note_label(n))).unwrap_or_else(|_| k.to_string())
}

/// an object's entries, each with Remove; the object without the removed one (None: emptied)
fn removable(ui: &mut egui::Ui, o: &Map<String, Json>, show: &dyn Fn(&str, &Json) -> String, editable: bool) -> Option<Option<Json>> {
    let mut out = None;
    for (k, v) in o {
        ui.horizontal(|ui| {
            ui.label(show(k, v));
            if ui.add_enabled(editable, egui::Button::new("Remove")).clicked() {
                let mut rest = o.clone();
                rest.remove(k);
                out = Some((!rest.is_empty()).then_some(Json::Object(rest)));
            }
        });
    }
    out
}

pub fn page(ui: &mut egui::Ui, p: &mut Project, id: &str, editable: bool) -> bool {
    hint(ui, "What the pipeline does with this source's files besides reading them. The check page's buttons add to the lists below.");
    let t = Table::Source(id.into());
    let src = p.source_json(id).unwrap_or_default();
    let mut changed = false;
    egui::Grid::new("settings").num_columns(3).striped(true).show(ui, |ui| {
        ui.label("Role");
        let old = src["role"].as_str().unwrap_or_default().to_string();
        let mut role = old.clone();
        ui.add_enabled_ui(editable, |ui| {
            egui::ComboBox::from_id_salt("role").selected_text(&role).show_ui(ui, |ui| {
                for r in ROLES {
                    ui.selectable_value(&mut role, r.to_string(), r);
                }
            });
        });
        explain(ui, "what the fits take these recordings as");
        ui.end_row();
        if role != old {
            changed |= put(p, &t, "role", Some(json!(role)));
        }
        if role == ROLES[1] {
            ui.label("Mallet");
            let mut m = src["mallet"].as_str().unwrap_or_default().to_string();
            if ui.add_enabled(editable, egui::TextEdit::singleline(&mut m).desired_width(160.0)).changed() {
                changed |= put(p, &t, "mallet", (!m.trim().is_empty()).then(|| json!(m.trim())));
            }
            explain(ui, "the mallet's name in the instrument (the menu's names for them are in [menu])");
            ui.end_row();
        }
        ui.label("Tail cut");
        let mut cut = src["tail_cut_s"].as_f64().unwrap_or(0.0);
        if ui.add_enabled(editable, egui::DragValue::new(&mut cut).range(0.0..=10.0).clamp_existing_to_range(false).speed(0.01).suffix(" s")).changed() {
            changed |= put(p, &t, "tail_cut_s", (cut > 0.0).then(|| json!(cut)));
        }
        explain(ui, "the last seconds of every file are not trusted (a fade-out at their ends)");
        ui.end_row();
        ui.label("Octave");
        let mut oct = src["octave"].as_i64().unwrap_or(0);
        if ui.add_enabled(editable, egui::DragValue::new(&mut oct).range(-48..=48).clamp_existing_to_range(false).suffix(" semitones")).changed() {
            changed |= put(p, &t, "octave", (oct != 0).then(|| json!(oct)));
        }
        explain(ui, "the files sound this far from the keys they are played on");
        ui.end_row();
    });
    let skip: Vec<i64> = src["skip"].as_array().into_iter().flatten().filter_map(Json::as_i64).collect();
    if !skip.is_empty() {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Keys left out").strong());
        explain(ui, "their files are not read");
        ui.horizontal_wrapped(|ui| {
            for &n in &skip {
                if ui.add_enabled(editable, egui::Button::new(format!("{} ({n}): read it again", note_label(n)))).clicked() {
                    let rest: Vec<i64> = skip.iter().copied().filter(|&k| k != n).collect();
                    changed |= put(p, &t, "skip", (!rest.is_empty()).then(|| json!(rest)));
                }
            }
        });
    }
    let lists: [(&str, &str, &str, fn(&str, &Json) -> String); 3] = [
        ("relabel", "Keys relabeled", "a file read as another key", |k, v| format!("{} is read as {}", key_name(k), v.as_i64().map(|n| key_name(&n.to_string())).unwrap_or_else(|| v.to_string()))),
        ("cut_s", "Tail cut per key", "the key's last seconds are not trusted", |k, v| format!("{}: {v} s", key_name(k))),
        ("gain_db", "Gain per dynamic", "the dynamic's files are taken this much quieter (their recording gain)", |k, v| format!("{k}: {v} dB")),
    ];
    for (key, title, what, show) in lists {
        if let Some(o) = src[key].as_object().filter(|o| !o.is_empty()) {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(title).strong());
            explain(ui, what);
            if let Some(rest) = removable(ui, o, &show, editable) {
                changed |= put(p, &t, key, rest);
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use pe_editor::project::Repo;

    #[test]
    fn drawing_the_page_writes_nothing() {
        // values outside the controls' ranges stay as written until the user changes them
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("projects/zz_one");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("project.toml"), "[project]\nfamily = \"bars\"\n\n[[part]]\nid = \"zz_one\"\n\n\
            [[part.source]]\nid = \"zz_src\"\nrole = \"main\"\nroot = \"r\"\ntail_cut_s = 12.5\noctave = 60\n\n\
            [part.source.map]\nkind = \"pattern\"\nglob = \"*.wav\"\nregex = '(?P<note>[A-G]\\d)\\.wav'\n\
            note = { group = \"note\" }\ndynamic = \"mf\"\n").unwrap();
        let repo = Repo { root: d.path().to_path_buf(), projects: d.path().join("projects"), data: d.path().join("data") };
        let mut p = Project::load(&repo, "zz_one").unwrap();
        let before = p.text();
        let ctx = egui::Context::default();
        let mut changed = false;
        for _ in 0..3 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                changed |= page(ui, &mut p, "zz_src", true);
            });
        }
        assert!(!changed, "the page reported a change nobody made");
        assert_eq!(p.text(), before);
    }
}
