//! The names page: the parts of the files' names, each with its role; the octave, or the key the
//! numbers count from; the take kept; what the group makes; what the files read as. For an SFZ:
//! the folders its regions use, the one preferred, its region filters.
use super::hint;
use eframe::egui;
use pe_editor::pyfmt::py_str;
use pe_editor::rule::{Names, PartRole, Preview, Reading, Rule, Sfz, is_separator};
use pe_editor::view::note_label;
use serde_json::Value as Json;
use std::collections::BTreeSet;

pub enum Out {
    Nothing,
    Changed,
    ResetToParts,
    EditByHand,
}

const MENU: [PartRole; 9] = [PartRole::Fixed, PartRole::NoteName, PartRole::Midi, PartRole::Counted, PartRole::Dynamic,
    PartRole::Take, PartRole::Group, PartRole::AnyDigits, PartRole::AnyLetters];

const WARN: egui::Color32 = egui::Color32::from_rgb(200, 140, 40);

pub fn role_label(r: &PartRole) -> String {
    match r {
        PartRole::Fixed => "fixed text".into(),
        PartRole::NoteName => "note name (C4, F#3)".into(),
        PartRole::Midi => "note number: MIDI (60 = C4)".into(),
        PartRole::Counted => "note number: counted from a key".into(),
        PartRole::Dynamic => "dynamic".into(),
        PartRole::Take => "take (round robin)".into(),
        PartRole::Group => "group: one source per value".into(),
        PartRole::AnyDigits => "any digits".into(),
        PartRole::AnyLetters => "any letters".into(),
        PartRole::Other => "pattern written by hand".into(),
        PartRole::Field(n) => format!("field {n}"),
    }
}

/// a role in a few words (a closed menu, a column's title)
fn short_label(r: &PartRole) -> String {
    match r {
        PartRole::Fixed => "fixed text".into(),
        PartRole::NoteName => "note name".into(),
        PartRole::Midi => "MIDI number".into(),
        PartRole::Counted => "counted number".into(),
        PartRole::Dynamic => "dynamic".into(),
        PartRole::Take => "take".into(),
        PartRole::Group => "group".into(),
        PartRole::AnyDigits => "any digits".into(),
        PartRole::AnyLetters => "any letters".into(),
        PartRole::Other => "by hand".into(),
        PartRole::Field(n) => format!("field {n}"),
    }
}

pub fn page(ui: &mut egui::Ui, rule: &mut Rule, preview: Option<&Result<Preview, String>>, sfz_folders: &[(String, usize)],
            wizard: bool, editable: bool) -> Out {
    match &mut rule.reading {
        Reading::Names(n) => names(ui, n, preview, wizard, editable),
        Reading::Sfz(s) => sfz(ui, s, sfz_folders, editable),
        Reading::Custom => custom(ui, editable),
    }
}

fn custom(ui: &mut egui::Ui, editable: bool) -> Out {
    hint(ui, "This map is written in a way the parts cannot show (an alternative at the top level, an anchor): \
        edit it under Advanced, or let the editor propose the parts from the file names again.");
    let mut out = Out::Nothing;
    ui.horizontal(|ui| {
        if ui.add_enabled(editable, egui::Button::new("Reset to the parts")).on_hover_text("propose the rule from the file names again (the map is replaced)").clicked() {
            out = Out::ResetToParts;
        }
        if ui.button("Open Advanced").clicked() {
            out = Out::EditByHand;
        }
    });
    out
}

fn names(ui: &mut egui::Ui, n: &mut Names, preview: Option<&Result<Preview, String>>, wizard: bool, editable: bool) -> Out {
    hint(ui, "Each part of the files' names has a role: the note, the dynamic, the take, or text that is the same in every file. \
        The editor proposed them; change any that are wrong.");
    let mut out = Out::Nothing;
    let p = match preview {
        Some(Ok(p)) => Some(p),
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::RED, format!("the names: {e}"));
            None
        }
        None => None,
    };
    let mut set: Option<(usize, PartRole)> = None;
    ui.horizontal_wrapped(|ui| {
        for (i, part) in n.parts.iter().enumerate() {
            if is_separator(part) {
                ui.label(egui::RichText::new(&part.text).monospace().weak());
                continue;
            }
            // a block of a fixed size, so the row knows where to wrap
            ui.allocate_ui_with_layout(egui::vec2(124.0, 52.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                let shown = if part.text.is_empty() { part.pattern.clone() } else { part.text.clone() };
                ui.label(egui::RichText::new(shown).monospace().strong()).on_hover_text(format!("as a regex: {}", part.pattern));
                let mut pick = part.role.clone();
                ui.add_enabled_ui(editable, |ui| {
                    egui::ComboBox::from_id_salt(("part role", i)).selected_text(short_label(&pick)).width(110.0).show_ui(ui, |ui| {
                        for r in MENU.iter().filter(|r| wizard || **r != PartRole::Group) {
                            ui.selectable_value(&mut pick, r.clone(), role_label(r));
                        }
                    });
                });
                if pick != part.role {
                    set = Some((i, pick));
                }
            });
        }
    });
    if let Some((i, role)) = set {
        let empty = BTreeSet::new();
        let values = p.and_then(|p| p.values.get(i)).unwrap_or(&empty);
        n.set_role(i, role, values);
        out = Out::Changed;
    }
    ui.add_space(6.0);
    let note = n.parts.iter().find(|p| matches!(p.role, PartRole::NoteName | PartRole::Midi | PartRole::Counted)).map(|p| p.role.clone());
    match note {
        Some(PartRole::Counted) => {
            ui.horizontal(|ui| {
                ui.label("The number 1 is the key");
                let mut first = n.offset + 1;
                if ui.add_enabled(editable, egui::DragValue::new(&mut first).range(0..=127).clamp_existing_to_range(false)).changed() {
                    n.offset = first - 1;
                    out = Out::Changed;
                }
                ui.label(note_label(first));
            });
        }
        Some(_) => {
            ui.horizontal(|ui| {
                ui.label("Octave");
                let mut shift = n.shift;
                let label = |s: i64| if s == 0 { "as written".to_string() } else { format!("{s:+} semitones") };
                ui.add_enabled_ui(editable, |ui| {
                    egui::ComboBox::from_id_salt("octave").selected_text(label(shift)).show_ui(ui, |ui| {
                        let mut opts: Vec<i64> = (-4..=4).map(|k| k * 12).collect();
                        if !opts.contains(&shift) {
                            opts.push(shift);
                            opts.sort();
                        }
                        for s in opts {
                            ui.selectable_value(&mut shift, s, label(s));
                        }
                    });
                });
                if shift != n.shift {
                    n.shift = shift;
                    out = Out::Changed;
                }
                super::wrapped(ui, egui::RichText::new("the keys the names say, moved by octaves (the findings say what the pitch shows)").weak());
            });
        }
        None => {
            ui.colored_label(WARN, "No part is the note: choose the part that names the key.");
        }
    }
    if n.parts.iter().any(|p| p.role == PartRole::Take) {
        let mut lowest = n.prefer.iter().any(|x| x == "take");
        if ui.add_enabled(editable, egui::Checkbox::new(&mut lowest, "Keep only the lowest take of each key and layer")).changed() {
            if lowest {
                n.prefer.push("take".into());
            } else {
                n.prefer.retain(|x| x != "take");
            }
            out = Out::Changed;
        }
    }
    if let Some(g) = &n.group {
        ui.label(format!("Group: {} values, one source each at Finish: {}", g.len(), g.join(", ")));
    }
    if let Some(p) = p {
        ui.add_space(6.0);
        ui.label(format!("{} files read", p.rows.len()));
        let cols: Vec<usize> = (0..n.parts.len()).filter(|&i| n.parts[i].role != PartRole::Fixed).collect();
        egui::Grid::new("names preview").striped(true).show(ui, |ui| {
            ui.strong("File");
            for &i in &cols {
                ui.strong(short_label(&n.parts[i].role));
            }
            ui.end_row();
            for (rel, texts) in p.rows.iter().take(8) {
                ui.label(egui::RichText::new(rel.rsplit('/').next().unwrap_or(rel)).monospace());
                for &i in &cols {
                    ui.label(texts.get(i).cloned().unwrap_or_default());
                }
                ui.end_row();
            }
        });
        if !p.other.is_empty() {
            egui::CollapsingHeader::new(format!("{} files have another shape and are not read", p.other.len())).show(ui, |ui| {
                for f in p.other.iter().take(200) {
                    ui.label(egui::RichText::new(f).monospace());
                }
            });
        }
    }
    if !wizard && ui.link("Edit the regex by hand (Advanced)").clicked() {
        out = Out::EditByHand;
    }
    out
}

fn filter_text(f: &Json) -> String {
    let op = f["opcode"].as_str().unwrap_or("?");
    if let Some(list) = f.get("in").and_then(Json::as_array) {
        format!("{op} is {}", list.iter().map(py_str).collect::<Vec<_>>().join(" or "))
    } else if let Some(lt) = f.get("lt") {
        format!("{op} below {lt}")
    } else if let Some(eq) = f.get("eq") {
        format!("{op} = {eq}")
    } else {
        op.to_string()
    }
}

fn sfz(ui: &mut egui::Ui, s: &mut Sfz, folders: &[(String, usize)], editable: bool) -> Out {
    hint(ui, "The SFZ file names a sample for every key and velocity. When its samples sit in several folders \
        (microphone positions, pedal states), choose the ones to read.");
    let mut out = Out::Nothing;
    ui.label(format!("SFZ file: {}", s.files.join(", ")));
    // a folder holding every region (the set's own) is no choice, unless the map names it
    let all = folders.iter().map(|(_, n)| *n).max().unwrap_or(0);
    // and a folder the map names that holds none of this file's regions
    let mut shown: Vec<(String, usize)> = folders.iter().filter(|(f, n)| *n < all || s.folders.contains(f)).cloned().collect();
    shown.extend(s.folders.iter().filter(|f| !folders.iter().any(|(x, _)| x == *f)).map(|f| (f.clone(), 0)));
    if !shown.is_empty() || s.prefer.is_some() {
        ui.label("The regions use files in these folders:");
        for (f, count) in &shown {
            let mut on = s.folders.contains(f);
            if ui.add_enabled(editable, egui::Checkbox::new(&mut on, format!("{f} ({count} regions)"))).changed() {
                if on {
                    s.folders.push(f.clone());
                } else {
                    s.folders.retain(|x| x != f);
                }
                out = Out::Changed;
            }
        }
        ui.label(egui::RichText::new("none ticked: every folder").weak());
        ui.horizontal(|ui| {
            ui.label("Where a key has a sample in several, prefer");
            let mut pick = s.prefer.clone();
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt("sfz prefer").selected_text(pick.clone().unwrap_or_else(|| "none".into())).show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, None, "none");
                    for (f, _) in folders {
                        ui.selectable_value(&mut pick, Some(format!("/{f}/")), f);
                    }
                });
            });
            if pick != s.prefer {
                s.prefer = pick;
                out = Out::Changed;
            }
        });
    }
    if !s.filter.is_empty() {
        ui.label("Regions kept only where:");
        let mut remove = None;
        for (k, f) in s.filter.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(filter_text(f));
                if ui.add_enabled(editable, egui::Button::new("Remove")).clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            s.filter.remove(k);
            out = Out::Changed;
        }
    }
    out
}
