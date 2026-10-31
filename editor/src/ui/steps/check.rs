//! The check page: the findings, the keyboard grid (with its legend), the take checks' sliders,
//! the item table (a click plays the file; a click on a column's title sorts by it, again the
//! other way), for the selected row: leave the key or the take out, use a spare take, put a take
//! back, or say which key it really is; what the map leaves out.
use super::hint;
use eframe::egui;
use pe_editor::audio::Measures;
use pe_editor::mapping::Report;
use pe_editor::suggest::Finding;
use pe_editor::takes::{self, Check, Scored, Thresholds};
use pe_editor::view::{self, Cell, Col};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// the cells' colors (Okabe–Ito, a set color-blind readers tell apart), each also a shape
const BLUE: egui::Color32 = egui::Color32::from_rgb(0, 114, 178);
const ORANGE: egui::Color32 = egui::Color32::from_rgb(230, 159, 0);
const VERMILLION: egui::Color32 = egui::Color32::from_rgb(213, 94, 0);
const PURPLE: egui::Color32 = egui::Color32::from_rgb(204, 121, 167);


pub struct CheckState {
    pub note: Option<i64>,
    pub row: Option<usize>,
    pub relabel_to: i64,
    pub sort: (Col, bool),
    /// the scores and what they were computed from (takes::inputs_key)
    scored: Option<(u64, BTreeMap<String, Vec<Scored>>)>,
}

impl Default for CheckState {
    fn default() -> Self {
        CheckState { note: None, row: None, relabel_to: 60, sort: (Col::Key, false), scored: None }
    }
}

pub enum Action {
    Play(PathBuf),
    Skip(i64),
    Relabel(i64, i64),
    /// a take left out (its relpath into the source's skip_files)
    SkipFile(String),
    /// a left-out take read again
    UnskipFile(String),
    /// the take checks' thresholds moved
    Thresholds(Thresholds),
}

pub fn findings(ui: &mut egui::Ui, list: &[Finding]) {
    if list.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("Findings ({})", list.len())).default_open(true).show(ui, |ui| {
        for f in list {
            super::wrapped(ui, format!("• {}", f.text));
            for file in f.files.iter().take(6) {
                ui.label(egui::RichText::new(format!("    {file}")).monospace());
            }
            if f.files.len() > 6 {
                ui.label(format!("    … and {} more", f.files.len() - 6));
            }
        }
    });
}

pub fn page(ui: &mut egui::Ui, st: &mut CheckState, report: Option<&Result<Report, String>>, m: &Measures,
            offsets: &BTreeMap<String, f32>, found: &[Finding], thresholds: &Thresholds, editable: bool) -> Option<Action> {
    hint(ui, "Click a key in the grid to list its files, a row to hear it, a column's title to sort by it. A flagged take \
        is only a suspect: listen, then leave it out or keep it.");
    findings(ui, found);
    let mut act = None;
    match report {
        None => {
            ui.label("mapping…");
        }
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::RED, format!("the map: {e} (the project cannot be built until it maps)"));
        }
        Some(Ok(rep)) => {
            (_, st.note) = view::clamp_selection(None, st.note, &rep.items);
            ui.label(format!("{} items", rep.items.len()));
            // the take checks: scored once per mapping and measurement, flagged per the thresholds
            let inputs = takes::inputs(&rep.items, &rep.spares, m, offsets);
            let key = takes::inputs_key(&inputs);
            if st.scored.as_ref().is_none_or(|(k, _)| *k != key) {
                st.scored = Some((key, takes::score(&inputs)));
            }
            let scored = &st.scored.as_ref().unwrap().1;
            let flags = takes::flags(scored, thresholds);
            let path_of: BTreeMap<&str, &str> = rep.items.iter().chain(rep.spares.values().flatten())
                .filter_map(|i| Some((i["relpath"].as_str()?, i["path"].as_str()?))).collect();
            let flaws: BTreeMap<String, String> = flags.iter().filter_map(|(rel, f)| {
                Some((path_of.get(rel.as_str())?.to_string(), f.iter().map(takes::describe).collect::<Vec<_>>().join("; ")))
            }).collect();
            let flagged: BTreeSet<String> = flaws.keys().cloned().collect();
            let suspect: BTreeSet<String> = offsets.iter().filter(|(_, c)| c.abs() > 50.0).map(|(p, _)| p.clone()).collect();
            let in_use: BTreeSet<String> = rep.replaced.keys().cloned().collect();
            legend(ui);
            let g = view::grid(&rep.items, &suspect, &flagged, &rep.left_out, &in_use);
            if let Some(k) = draw_grid(ui, &g, st.note) {
                st.note = if st.note == Some(k) { None } else { Some(k) };
                st.row = None;
            }
            if let Some(t) = sliders(ui, thresholds, &flags, rep, editable) {
                act = Some(Action::Thresholds(t));
            }
            let mut rows = view::rows(&rep.items, offsets, m, st.note, &flaws, &rep.spares, &rep.left_out);
            view::sort_rows(&mut rows, st.sort.0, st.sort.1);
            if st.row.is_some_and(|i| !rows.iter().any(|r| r.index == i)) {
                st.row = None;
            }
            if let Some(i) = draw_table(ui, &rows, st.row, &mut st.sort) {
                st.row = Some(i);
                act = rows.iter().find(|r| r.index == i).map(|r| Action::Play(PathBuf::from(&r.path)));
            }
            if let Some(r) = st.row.and_then(|i| rows.iter().find(|r| r.index == i)) {
                if let Some(a) = row_actions(ui, st, r, rep, editable) {
                    act = Some(a);
                }
            }
            if !rep.left_out_missing.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Left out but not in the set: {}", rep.left_out_missing.join(", ")));
                    for f in &rep.left_out_missing {
                        if ui.add_enabled(editable, egui::Button::new(format!("Forget {}", f.rsplit('/').next().unwrap_or(f))))
                            .on_hover_text("removes the name from the source's skip_files").clicked() {
                            act = Some(Action::UnskipFile(f.clone()));
                        }
                    }
                });
            }
            if !rep.unmatched.is_empty() {
                egui::CollapsingHeader::new(format!("Files the names do not fit ({})", rep.unmatched.len())).show(ui, |ui| {
                    for f in rep.unmatched.iter().take(200) {
                        ui.label(egui::RichText::new(f).monospace());
                    }
                });
            }
            if !rep.dropped.is_empty() {
                egui::CollapsingHeader::new(format!("Left out ({})", rep.dropped.len())).show(ui, |ui| {
                    for (f, why) in rep.dropped.iter().take(200) {
                        ui.label(egui::RichText::new(format!("{f}: {why}")).monospace());
                    }
                });
            }
        }
    }
    act
}

/// the buttons for the selected row: a read take (leave its key or itself out, use its spare,
/// relabel), a spare (what it would stand in for), a left-out take (put it back)
fn row_actions(ui: &mut egui::Ui, st: &mut CheckState, r: &view::RowView, rep: &Report, editable: bool) -> Option<Action> {
    let mut act = None;
    ui.horizontal(|ui| {
        ui.label(format!("{} ({}):", view::note_label(r.note), r.note));
        if r.left_out {
            if ui.add_enabled(editable, egui::Button::new("Put this take back")).on_hover_text("the file is read again (removed from the source's skip_files)").clicked() {
                act = Some(Action::UnskipFile(r.file.clone()));
            }
            return;
        }
        if let Some(of) = &r.spare_of {
            ui.label(format!("a spare take of {}: used when that one is left out", of.rsplit('/').next().unwrap_or(of)));
            return;
        }
        if ui.add_enabled(editable, egui::Button::new("Leave this key out")).on_hover_text("the key's files are not read (the source's skip)").clicked() {
            act = Some(Action::Skip(r.note));
        }
        let spare = rep.spares.get(&r.file).is_some_and(|v| !v.is_empty());
        let (label, tip) = if spare {
            ("Use the spare take", "this take is left out and the next take of its key and layer is read")
        } else {
            ("Leave this take out", "the file is not read (the source's skip_files)")
        };
        if ui.add_enabled(editable, egui::Button::new(label)).on_hover_text(tip).clicked() {
            act = Some(Action::SkipFile(r.file.clone()));
        }
        ui.separator();
        ui.label("It is really");
        ui.add(egui::DragValue::new(&mut st.relabel_to).range(0..=127));
        ui.label(view::note_label(st.relabel_to));
        if ui.add_enabled(editable, egui::Button::new("Relabel")).on_hover_text("the file is read as this key (the source's relabel)").clicked() {
            act = Some(Action::Relabel(r.note, st.relabel_to));
        }
    });
    act
}

/// the three checks' thresholds, each with how many of the source's takes it flags; Reset
fn sliders(ui: &mut egui::Ui, t: &Thresholds, flags: &BTreeMap<String, Vec<Scored>>, rep: &Report, editable: bool) -> Option<Thresholds> {
    let read: BTreeSet<&str> = rep.items.iter().filter_map(|i| i["relpath"].as_str()).collect();
    let count = |c: Check| flags.iter().filter(|(k, v)| read.contains(k.as_str()) && v.iter().any(|s| s.check == c)).count();
    let mut new = *t;
    ui.label(egui::RichText::new("Flag takes").strong());
    egui::Grid::new("take checks").num_columns(2).show(ui, |ui| {
        let rows = [
            (Check::Decay, "Decay", "a decay faster or slower than the neighboring keys' at its layer"),
            (Check::Rise, "Late rise", "the level rising above its decay late in the note (a noise, a buzz, a second strike)"),
            (Check::Brightness, "Brightness", "brighter or darker than the line through its key's other layers"),
        ];
        for (c, name, what) in rows {
            ui.label(format!("{name} ({} flagged)", count(c)))
                .on_hover_text(format!("{what}; flagged above this many spreads of the source's own takes (higher flags fewer)"));
            let mut v = new.of(c);
            ui.add_enabled(editable, egui::Slider::new(&mut v, takes::THRESHOLD_MIN..=takes::THRESHOLD_MAX).step_by(0.5));
            match c {
                Check::Decay => new.decay = v,
                Check::Rise => new.rise = v,
                Check::Brightness => new.brightness = v,
            }
            ui.end_row();
        }
    });
    if ui.add_enabled(editable && *t != Thresholds::DEFAULT, egui::Button::new("Reset")).on_hover_text("the defaults").clicked() {
        new = Thresholds::DEFAULT;
    }
    (new != *t).then_some(new)
}

/// one cell of the grid or the legend, its state as a shape
fn draw_cell(painter: &egui::Painter, r: egui::Rect, cell: Option<Cell>, faint: egui::Color32) {
    match cell {
        None => {
            painter.rect_filled(r, 1.0, faint);
        }
        Some(Cell::Present) => {
            painter.rect_filled(r, 1.0, BLUE);
        }
        Some(Cell::Duplicate) => {
            // a split cell: two bars
            let mid = r.center().y;
            painter.rect_filled(egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, mid - 0.5)), 0.0, ORANGE);
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(r.min.x, mid + 0.5), r.max), 0.0, ORANGE);
        }
        Some(Cell::Suspect) => {
            // hollow, a thick outline
            painter.rect_stroke(r.shrink(1.0), 0.0, egui::Stroke::new(2.0, VERMILLION), egui::StrokeKind::Inside);
        }
        Some(Cell::Flagged) => {
            // striped
            painter.rect_filled(r, 1.0, BLUE);
            let p = painter.with_clip_rect(r);
            let mut x = r.left() - r.height();
            while x < r.right() {
                p.line_segment([egui::pos2(x, r.bottom()), egui::pos2(x + r.height(), r.top())], egui::Stroke::new(1.5, PURPLE));
                x += 4.0;
            }
        }
        Some(Cell::LeftOut) => {
            // dashed and empty
            let c = r.shrink(0.5);
            let pts = [c.left_top(), c.right_top(), c.right_bottom(), c.left_bottom(), c.left_top()];
            painter.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0, egui::Color32::GRAY), 2.0, 2.0));
        }
        Some(Cell::Spare) => {
            // a dot in the corner
            painter.rect_filled(r, 1.0, BLUE);
            let rad = (r.width().min(r.height()) * 0.18).max(1.2);
            painter.circle_filled(egui::pos2(r.right() - rad - 1.0, r.top() + rad + 1.0), rad, egui::Color32::WHITE);
        }
    }
}

/// the grid's states, drawn as in the grid, with their names
fn legend(ui: &mut egui::Ui) {
    let faint = ui.visuals().faint_bg_color;
    ui.horizontal_wrapped(|ui| {
        for (cell, name) in [(Cell::Present, "one file"), (Cell::Duplicate, "several files"),
            (Cell::Suspect, "pitch suspect (more than 50 cents off its key)"), (Cell::Flagged, "flagged take"),
            (Cell::LeftOut, "take left out"), (Cell::Spare, "spare take in use")] {
            let (r, _) = ui.allocate_exact_size(egui::vec2(13.0, 12.0), egui::Sense::hover());
            draw_cell(ui.painter(), r, Some(cell), faint);
            ui.label(name);
            ui.add_space(6.0);
        }
    });
}

/// one key per column, one layer per row; a click selects the key (and again clears it)
fn draw_grid(ui: &mut egui::Ui, g: &view::Grid, selected: Option<i64>) -> Option<i64> {
    let keys = (g.hi - g.lo + 1) as f32;
    // clear of the scroll bar, room for the last octave's label
    let s = &ui.spacing().scroll;
    let room = ui.available_width() - (s.bar_width + s.bar_inner_margin + s.bar_outer_margin) - 14.0;
    let w = (room / keys).clamp(4.0, 14.0);
    let h = 12.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w * keys, h * g.layers as f32 + 14.0), egui::Sense::click());
    // the last octave's label reaches past the last key
    let painter = ui.painter_at(rect.with_max_x(rect.right() + 14.0));
    let faint = ui.visuals().faint_bg_color;
    for n in g.lo..=g.hi {
        let x = rect.left() + (n - g.lo) as f32 * w;
        if n % 12 == 0 {
            painter.text(egui::pos2(x, rect.bottom() - 7.0), egui::Align2::LEFT_CENTER, view::note_label(n),
                egui::FontId::monospace(9.0), ui.visuals().weak_text_color());
        }
        for l in 1..=g.layers {
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + (l - 1) as f32 * h), egui::vec2(w - 1.0, h - 1.0));
            draw_cell(&painter, r, g.cells.get(&(n, l)).map(|c| c.0), faint);
        }
        if selected == Some(n) {
            painter.rect_stroke(egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w - 1.0, h * g.layers as f32)),
                0.0, egui::Stroke::new(1.5, ui.visuals().strong_text_color()), egui::StrokeKind::Outside);
        }
    }
    let pos = resp.interact_pointer_pos().filter(|_| resp.clicked())?;
    Some(g.lo + ((pos.x - rect.left()) / w) as i64)
}

/// the item table; returns the clicked row's index; a click on a title sorts by it
fn draw_table(ui: &mut egui::Ui, rows: &[view::RowView], selected: Option<usize>, sort: &mut (Col, bool)) -> Option<usize> {
    // in an area of its own clear of the page's scroll bar (it floats over the right edge; the
    // page itself cannot narrow below the full-width hint above)
    let s = &ui.spacing().scroll;
    let w = ui.available_width() - (s.bar_width + s.bar_inner_margin + s.bar_outer_margin);
    ui.allocate_ui(egui::vec2(w, ui.available_height()), |ui| table_in(ui, rows, selected, sort)).inner
}

fn table_in(ui: &mut egui::Ui, rows: &[view::RowView], selected: Option<usize>, sort: &mut (Col, bool)) -> Option<usize> {
    use egui_extras::{Column, TableBuilder};
    let mut clicked = None;
    // the table's own scroll bar beside its last column, not over it
    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
    // labels select text on a click by default: in the table the click selects the row
    ui.style_mut().interaction.selectable_labels = false;
    // short titles, their units on hover: the table fits a 1600 px window with the file names readable
    let titles = [("File", Some(Col::File), ""), ("Key", Some(Col::Key), ""), ("Dynamic", Some(Col::Dynamic), ""),
        ("Layer", Some(Col::Dynamic), ""), ("Pitch", Some(Col::Pitch), "cents off the key; "),
        ("Level", Some(Col::Level), "dBFS, RMS over the first 0.5 s; "), ("Check", None, "")];
    let weak = ui.visuals().weak_text_color();
    TableBuilder::new(ui)
        .id_salt("items")
        .striped(true)
        .sense(egui::Sense::click())
        .max_scroll_height(360.0)
        // the file takes what the others leave (its name truncated, the path on hover): at
        // 240 points the table was wider than a 1600 px window and pushed the page past its edge
        .column(Column::remainder().at_least(100.0).clip(true))
        .column(Column::auto())
        .columns(Column::auto(), 4)
        .column(Column::initial(240.0).at_least(120.0).clip(true))
        .header(20.0, |mut h| {
            for (t, col, unit) in titles {
                h.col(|ui| {
                    let Some(col) = col else {
                        ui.label(egui::RichText::new(t).strong()).on_hover_text("why a take is flagged");
                        return;
                    };
                    let on = sort.0 == col;
                    let mut text = egui::RichText::new(if on && sort.1 { format!("{t} (desc.)") } else { t.to_string() }).strong();
                    if on {
                        text = text.underline();
                    }
                    let tip = format!("{unit}sort by this column (again: the other way)");
                    if ui.add(egui::Button::new(text).frame(false)).on_hover_text(tip).clicked() {
                        *sort = if on { (col, !sort.1) } else { (col, false) };
                    }
                });
            }
        })
        .body(|body| {
            body.rows(18.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                row.set_selected(selected == Some(r.index));
                let dim = r.left_out || r.spare_of.is_some();
                let text = |s: String| if dim { egui::RichText::new(s).color(weak) } else { egui::RichText::new(s) };
                row.col(|ui| {
                    let name = r.file.rsplit('/').next().unwrap_or(&r.file);
                    let shown = if r.left_out { format!("{name} (left out)") } else if r.spare_of.is_some() { format!("↳ {name} (spare)") } else { name.to_string() };
                    ui.label(text(shown).monospace()).on_hover_text(&r.file);
                });
                row.col(|ui| {
                    ui.label(text(format!("{} ({})", r.name, r.note)));
                });
                row.col(|ui| {
                    ui.label(text(r.dynamic.clone()));
                });
                row.col(|ui| {
                    ui.label(text(r.layer.clone()));
                });
                row.col(|ui| {
                    let t = r.cents.map(|c| format!("{c:+.0}")).unwrap_or_default();
                    if r.cents.is_some_and(|c| c.abs() > 50.0) {
                        ui.colored_label(VERMILLION, t);
                    } else {
                        ui.label(text(t));
                    }
                });
                row.col(|ui| {
                    ui.label(text(r.level_db.map(|l| format!("{l:.1}")).unwrap_or_default()));
                });
                row.col(|ui| {
                    if let Some(f) = &r.flaw {
                        ui.label(text(f.clone())).on_hover_text(f);
                    }
                });
                if row.response().clicked() {
                    clicked = Some(r.index);
                }
            });
        });
    clicked
}
