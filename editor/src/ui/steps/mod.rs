//! The pages a source is set up with: the wizard shows them in order, a source's page as tabs.
//! Each page draws one part of the rule and says what changed.
pub mod check;
pub mod dynamics;
pub mod folder;
pub mod names;
pub mod played;
pub mod reading;
pub mod settings;

use eframe::egui;

/// a text wrapped at the window's visible edge (left of the scroll bar), however wide a row above
/// made the page (a table, a long row grows the width the next labels wrap at)
pub fn wrapped(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    let s = &ui.spacing().scroll;
    let bar = s.bar_width + s.bar_inner_margin + s.bar_outer_margin;
    let w = (ui.clip_rect().right() - bar - ui.cursor().left()).clamp(120.0, ui.available_width().max(120.0));
    ui.allocate_ui(egui::vec2(w, 0.0), |ui| {
        ui.add(egui::Label::new(text).wrap());
    });
}

/// the line at a page's top saying what it is for
pub fn hint(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(6.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(text);
    });
    ui.add_space(6.0);
}
