//! The "How is it played?" page: the answers (one pre-selected when the overtones decide it),
//! the instrument the new one is set up like, and every donor for when the guess is wrong.
use super::hint;
use eframe::egui;
use pe_editor::kind::Guess;
use pe_editor::project::Kind;
use std::collections::BTreeMap;

/// an answer chosen (with a donor, when it was picked from the full list)
pub fn page(ui: &mut egui::Ui, kinds: &[Kind], chosen: &str, guess: &Guess, donor: Option<(&str, &str)>,
            menu_names: &BTreeMap<String, String>) -> Option<(String, Option<String>)> {
    hint(ui, "How the instrument is played. The recordings decide the rest: it is set up like the closest \
        instrument of that kind, its overtones and dynamics taken from the files.");
    if !guess.why.is_empty() {
        ui.label(egui::RichText::new(&guess.why).weak());
    }
    let mut out = None;
    for k in kinds {
        if ui.radio(chosen == k.id, &k.label).clicked() && chosen != k.id {
            out = Some((k.id.clone(), None));
        }
    }
    if let Some((d, why)) = donor {
        ui.add_space(6.0);
        ui.label(format!("Set up like: {} ({why})", menu_names.get(d).map(String::as_str).unwrap_or(d)));
    }
    egui::CollapsingHeader::new("Choose the instrument it's set up like").default_open(false).show(ui, |ui| {
        for k in kinds {
            ui.label(egui::RichText::new(&k.label).strong());
            for d in &k.donors {
                let name = menu_names.get(d).map(String::as_str).unwrap_or(d);
                if ui.selectable_label(donor.is_some_and(|(x, _)| x == d), format!("{name} ({d})")).clicked() {
                    out = Some((k.id.clone(), Some(d.clone())));
                }
            }
        }
    });
    out
}
