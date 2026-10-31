//! The folder page: the system's folder picker, the folders under data/, a typed path. A folder
//! outside data/ is linked in as data/<name>.
use super::hint;
use eframe::egui;
use pe_editor::project::{self, Repo};
use pe_editor::suggest::Folder;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

#[derive(Default)]
pub struct FolderPick {
    pub typed: String,
    pub link_name: String,
    pub error: Option<String>,
    /// the folders linked into data/ here (data/<name>)
    pub linked: Vec<String>,
    picking: Option<Receiver<Option<PathBuf>>>,
    data_dirs: Option<Vec<String>>,
}

/// the folders under data/, two levels deep (the sets and their subfolders)
fn data_dirs(data: &Path) -> Vec<String> {
    let list = |d: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(d).into_iter().flatten().flatten()
            .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with(['.', '_']))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    let mut out = Vec::new();
    for top in list(data) {
        let subs = list(&data.join(&top));
        out.push(top.clone());
        out.extend(subs.into_iter().map(|s| format!("{top}/{s}")));
    }
    out
}

fn resolve(repo: &Repo, st: &mut FolderPick) -> Result<Folder, String> {
    let rel = match project::data_relative(&repo.data, &st.typed) {
        Some(r) => r,
        None => {
            project::link_folder(repo, Path::new(st.typed.trim()), st.link_name.trim())?;
            st.linked.push(st.link_name.trim().to_string());
            st.link_name.trim().to_string()
        }
    };
    let folder = Folder::from_data_path(&rel);
    if !folder.dir(&repo.data).is_dir() {
        return Err(format!("data/{rel} is not a folder"));
    }
    Ok(folder)
}

/// the folder chosen with "Use this folder"
pub fn page(ui: &mut egui::Ui, repo: &Repo, st: &mut FolderPick, editable: bool) -> Option<Folder> {
    hint(ui, "The folder holding the set's audio files (or its SFZ files). Sets live under data/; a folder elsewhere is linked in as data/<name>.");
    if let Some(rx) = &st.picking {
        match rx.try_recv() {
            Ok(Some(p)) => {
                st.typed = p.display().to_string();
                st.picking = None;
            }
            Ok(None) | Err(TryRecvError::Disconnected) => {
                st.error = Some("no folder chosen (the picker was closed, or it could not open: pick one from the list or type its path)".into());
                st.picking = None;
            }
            Err(TryRecvError::Empty) => {}
        }
    }
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label("Folder");
        let w = (ui.available_width() - 120.0).clamp(160.0, 460.0);
        ui.add_enabled(editable, egui::TextEdit::singleline(&mut st.typed).desired_width(w).hint_text("relative to data/, or an absolute path"));
        let pick = ui.add_enabled(editable && st.picking.is_none(), egui::Button::new("Choose…")).on_hover_text("open the system's folder picker");
        if pick.clicked() {
            let (tx, rx) = channel();
            let start = repo.data.clone();
            std::thread::spawn(move || {
                let _ = tx.send(rfd::FileDialog::new().set_directory(&start).pick_folder());
            });
            st.picking = Some(rx);
            st.error = None;
        }
    });
    if project::data_relative(&repo.data, &st.typed).is_none() && !st.typed.trim().is_empty() {
        ui.horizontal(|ui| {
            ui.label("Outside data/: link it in as data/");
            ui.text_edit_singleline(&mut st.link_name);
        });
    }
    if ui.add_enabled(editable && !st.typed.trim().is_empty(), egui::Button::new("Use this folder")).clicked() {
        match resolve(repo, st) {
            Ok(f) => {
                st.error = None;
                chosen = Some(f);
            }
            Err(e) => st.error = Some(e),
        }
    }
    if let Some(e) = &st.error {
        ui.colored_label(egui::Color32::RED, e);
    }
    let dirs = st.data_dirs.get_or_insert_with(|| data_dirs(&repo.data)).clone();
    egui::CollapsingHeader::new(format!("The folders under data/ ({})", dirs.len())).default_open(true).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("data dirs").max_height(260.0).show(ui, |ui| {
            for d in &dirs {
                if ui.selectable_label(st.typed == *d, d).clicked() {
                    st.typed = d.clone();
                }
            }
        });
    });
    chosen
}
