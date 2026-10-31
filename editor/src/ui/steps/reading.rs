//! The reading page: from an SFZ file (when the folder has one) or from the file names.
use super::hint;
use eframe::egui;

/// the new choice when it changed (None: the file names)
pub fn page(ui: &mut egui::Ui, sfz_files: &[String], current: &Option<String>, editable: bool) -> Option<Option<String>> {
    hint(ui, "How the files become keys and layers: an SFZ file says it for every sample; without one, the file names do.");
    let mut pick = current.clone();
    ui.add_enabled_ui(editable, |ui| {
        if sfz_files.is_empty() {
            ui.label("The folder has no SFZ file: the file names are read.");
        }
        for f in sfz_files {
            ui.radio_value(&mut pick, Some(f.clone()), format!("From the SFZ file {f}"));
        }
        ui.radio_value(&mut pick, None, "From the file names");
    });
    (pick != *current).then_some(pick)
}
