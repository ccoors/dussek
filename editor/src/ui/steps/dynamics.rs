//! The dynamics page: the set's layers, each with its files and its measured level; which are
//! kept and which is pp, mf and ff; whether the levels are as recorded or normalized.
use super::hint;
use eframe::egui;
use pe_editor::pyfmt::py_str;
use pe_editor::rule::{Dyn, Layer, LayerStat, Rule};
use serde_json::json;

const WARN: egui::Color32 = egui::Color32::from_rgb(200, 140, 40);

/// a layer row's statistics: by its value in the names, else by its name
fn stat<'a>(l: &Layer, stats: &'a [LayerStat]) -> Option<&'a LayerStat> {
    stats.iter().find(|s| l.raw.as_ref() == Some(&s.value) || (l.raw.is_none() && json!(s.value) == l.name))
}

/// a layer's level as the table shows it
fn level_text(level_db: Option<f32>, measuring: bool) -> String {
    match level_db {
        Some(d) => format!("{d:.1} dB"),
        None if measuring => "measuring…".into(),
        None => "not measured".into(),
    }
}

/// the page; `measuring`: the files are being measured (the levels follow)
pub fn page(ui: &mut egui::Ui, rule: &mut Rule, stats: &[LayerStat], editable: bool, measuring: bool) -> bool {
    hint(ui, "The set's layers: keep the ones the instrument is fitted from, and say which is pp, mf and ff (at least one). \
        The editor proposed the softest, the middle and the loudest kept layer.");
    let mut changed = false;
    let mut keep_set: Option<(usize, bool)> = None;
    let mut use_set: Option<(usize, Option<Dyn>)> = None;
    let label = |d: Option<Dyn>| d.map(|d| d.key().to_string()).unwrap_or_else(|| "not used".into());
    egui::Grid::new("layers").striped(true).num_columns(5).show(ui, |ui| {
        for t in ["Layer", "Files", "Level (median, re the loudest)", "Keep", "Used as"] {
            ui.strong(t);
        }
        ui.end_row();
        for (i, l) in rule.layers.iter().enumerate() {
            let name = py_str(&l.name);
            match &l.raw {
                Some(r) if *r != name => ui.label(format!("{r} (named {name})")),
                _ => ui.label(name),
            };
            let s = stat(l, stats);
            ui.label(s.map(|s| s.files.to_string()).unwrap_or_default());
            ui.label(level_text(s.and_then(|s| s.level_db), measuring));
            let mut keep = l.keep;
            let k = ui.add_enabled(editable && l.raw.is_some(), egui::Checkbox::without_text(&mut keep));
            if k.on_hover_text("left out: the map does not read the layer's files").changed() {
                keep_set = Some((i, keep));
            }
            let mut pick = l.used_as;
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt(("used as", i)).width(60.0).selected_text(label(pick)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, None, "not used");
                    for d in Dyn::ALL {
                        ui.selectable_value(&mut pick, Some(d), d.key());
                    }
                });
            });
            if pick != l.used_as {
                use_set = Some((i, pick));
            }
            ui.end_row();
        }
    });
    if let Some((i, k)) = keep_set {
        rule.set_keep(i, k);
        changed = true;
    }
    if let Some((i, d)) = use_set {
        rule.use_as(i, d);
        changed = true;
    }
    let measured = stats.iter().any(|s| s.level_db.is_some());
    ui.horizontal(|ui| {
        let b = ui.add_enabled(editable && measured, egui::Button::new("Order by the measured level"));
        if b.on_hover_text("the kept layers softest first: the fits read the layers in this order").clicked() {
            rule.order_by_level(&|l| stat(l, stats).and_then(|s| s.level_db));
            changed = true;
        }
        if ui.add_enabled(editable, egui::Button::new("Propose pp, mf and ff again")).clicked() {
            rule.propose_dynamics();
            changed = true;
        }
    });
    if !rule.layers.iter().any(|l| l.used_as.is_some()) {
        ui.colored_label(WARN, "Choose at least one of pp, mf and ff.");
    }
    ui.separator();
    let special = rule.levels.as_deref().filter(|l| !["recorded", "per_key", "normalized"].contains(l)).map(String::from);
    ui.horizontal(|ui| {
        ui.label("The recordings' levels:");
        match &special {
            Some(other) => {
                ui.label(format!("{other} (edit it under Advanced)"));
            }
            None => {
                let mut lv = rule.levels.clone();
                ui.add_enabled_ui(editable, |ui| {
                    ui.radio_value(&mut lv, Some("recorded".to_string()), "as recorded");
                    ui.radio_value(&mut lv, Some("per_key".to_string()), "each key normalized")
                        .on_hover_text("Each key scaled by one gain: the steps between its layers are as recorded, its level against the other keys is not");
                    ui.radio_value(&mut lv, Some("normalized".to_string()), "normalized");
                });
                if lv != rule.levels {
                    rule.levels = lv;
                    changed = true;
                }
            }
        }
    });
    let kept: Vec<f32> = rule.layers.iter().filter(|l| l.keep).filter_map(|l| stat(l, stats)?.level_db).collect();
    if kept.len() >= 2 {
        let spread = kept.iter().copied().fold(f32::MIN, f32::max) - kept.iter().copied().fold(f32::MAX, f32::min);
        let why = if spread < 1.0 {
            "the kept layers' medians lie within 1 dB of each other (as a normalized set's do)".to_string()
        } else {
            format!("the kept layers' medians span {spread:.0} dB (a normalized set's lie within about 1 dB)")
        };
        super::wrapped(ui, egui::RichText::new(why).weak());
    }
    changed
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_level_not_there_yet_says_why() {
        assert_eq!(super::level_text(Some(-3.04), false), "-3.0 dB");
        assert_eq!(super::level_text(None, true), "measuring…");
        assert_eq!(super::level_text(None, false), "not measured");
    }
}
