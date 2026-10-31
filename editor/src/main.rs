//! The instrument editor (stage 3a): the projects, a source's mapping with the suggestion, and the
//! chain run through proto/build.py. The logic is the pe_editor library; this is its window.
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let repo = pe_editor::project::Repo::from_env();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 820.0]).with_title("Dussek editor"),
        ..Default::default()
    };
    eframe::run_native("dussek-editor", options, Box::new(move |_cc| Ok(Box::new(ui::App::new(repo)))))
}
