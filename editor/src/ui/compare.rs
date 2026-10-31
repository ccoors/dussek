//! The Compare page: every recorded key and dynamic rendered by the linked engine and measured
//! as its recording is; the differences along the keys (four lanes), the flagged pairs, A/B.
use super::App;
use eframe::egui;
use pe_editor::audio::{self, Clip};
use pe_editor::compare::{self, Diff, Entry, LevelTable, Medians, Off, Pair, Run};
use pe_editor::player;
use pe_editor::project::Table;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// the dynamics' colors (pp, mf, ff)
const COLORS: [egui::Color32; 3] = [egui::Color32::from_rgb(80, 130, 220), egui::Color32::from_rgb(70, 160, 90), egui::Color32::from_rgb(220, 130, 50)];

struct Job {
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    rx: Receiver<(usize, Result<Entry, String>)>,
    done: Receiver<Result<(), String>>,
}

impl Drop for Job {
    /// the project closed or another opened: the workers stop between pairs (what they finished is
    /// cached)
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Recording,
    Original,
    Render,
    Both,
    All,
}

/// the selected pair's clips: the recording, the render and the export before the last build (its
/// level and onset; none: why)
struct Clips {
    pair: usize,
    rec: Clip,
    ren: Clip,
    orig: Result<(Clip, f32, f32), String>,
}

pub struct CompareView {
    pairs: Vec<Pair>,
    skipped: usize,
    outside: usize,
    none_why: Option<String>,
    results: Vec<Option<Result<Entry, String>>>,
    job: Option<Job>,
    error: Option<String>,
    /// the export's stamp the results are for (a re-export starts again), and a changed one with
    /// when it was first seen (it starts once it has held for a second and no build runs)
    export_stamp: Vec<Option<(u64, std::time::SystemTime)>>,
    changed: Option<(Vec<Option<(u64, std::time::SystemTime)>>, Instant)>,
    checked: Instant,
    shown: [bool; 3],
    /// the sources (id, the label: the menu's mallet name or the id) in order, and which are shown
    sources: Vec<(String, String)>,
    shown_sources: Vec<bool>,
    /// only the flagged pairs drawn
    flagged_only: bool,
    /// the Correct levels preview: per source its current and proposed table
    correcting: Option<BTreeMap<String, (Option<LevelTable>, LevelTable)>>,
    /// the sources Correct levels leaves out (their files normalized: their level says nothing)
    uncorrected: Vec<String>,
    /// per source how far one recorded key alone moves its proposal
    sway: BTreeMap<String, compare::Sway>,
    selected: Option<usize>,
    sort_worst_first: bool,
    level_match: compare::LevelMatch,
    /// both sides played mixed to mono (as measured); off: both in stereo
    mono: bool,
    /// the selected pair's clips once made, and what plays
    clips: Option<Clips>,
    making: Option<(usize, Receiver<Result<Clips, String>>)>,
    playing: Option<Side>,
}

/// the export's files as the cache's key reads them (size and time of both): a re-export
/// writes instrument.json first, then data.bin
fn export_stamp(app: &App, name: &str) -> Vec<Option<(u64, std::time::SystemTime)>> {
    ["instrument.json", "data.bin"].iter().map(|f| {
        let m = std::fs::metadata(compare::export_dir(&app.repo, name).join(f)).ok()?;
        Some((m.len(), m.modified().ok()?))
    }).collect()
}

impl CompareView {
    fn new(app: &App, name: &str) -> CompareView {
        let o = app.open.as_ref().unwrap();
        let ps = compare::pairs(&app.repo, &o.project);
        let mut v = CompareView {
            results: vec![None; ps.list.len()],
            pairs: ps.list, skipped: ps.skipped, outside: ps.outside, none_why: ps.none_why,
            job: None, error: None, export_stamp: export_stamp(app, name), changed: None, checked: Instant::now(),
            shown: [true; 3], sources: Vec::new(), shown_sources: Vec::new(), flagged_only: false, correcting: None, uncorrected: Vec::new(), sway: BTreeMap::new(), selected: None, sort_worst_first: true, level_match: compare::LevelMatch::Pair, mono: true,
            clips: None, making: None, playing: None,
        };
        // the menu's mallet names label the mallet sources (the fit's mallets in source order)
        let mallets: Vec<String> = o.project.get(&Table::top("menu"), "mallets").and_then(|j| j.as_array().cloned())
            .unwrap_or_default().iter().filter_map(|x| x.as_str().map(String::from)).collect();
        for p in &v.pairs {
            if !v.sources.iter().any(|s| s.0 == p.source) {
                let label = p.strike.mallet.and_then(|i| mallets.get(i as usize).cloned()).unwrap_or_else(|| p.source.clone());
                v.sources.push((p.source.clone(), label));
            }
        }
        v.shown_sources = vec![true; v.sources.len()];
        v.start(app, name);
        v
    }

    /// a pair's dynamic and source are ticked
    fn shown(&self, i: usize) -> bool {
        let p = &self.pairs[i];
        compare::FITTED.iter().position(|x| *x == p.dynamic).is_some_and(|k| self.shown[k])
            && self.sources.iter().position(|s| s.0 == p.source).is_some_and(|k| self.shown_sources[k])
    }

    fn label<'a>(&'a self, source: &'a str) -> &'a str {
        self.sources.iter().find(|s| s.0 == source).map_or(source, |s| s.1.as_str())
    }

    fn start(&mut self, app: &App, name: &str) {
        if self.pairs.is_empty() {
            return;
        }
        self.error = None;
        let (tx, rx) = channel();
        let (dtx, drx) = channel();
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (p, c) = (progress.clone(), cancel.clone());
        let pairs = self.pairs.clone();
        let export = compare::export_dir(&app.repo, name);
        let cache = compare::cache_path(&app.repo, name);
        let threads = compare::threads();
        std::thread::spawn(move || {
            let rendered = AtomicUsize::new(0);
            let tx = std::sync::Mutex::new(tx);
            let res = compare::run(&Run { pairs: &pairs, export: &export, cache: &cache, threads, progress: &p, rendered: &rendered, cancel: &c },
                &|i, r| { let _ = tx.lock().unwrap().send((i, r)); });
            let _ = dtx.send(res);
        });
        self.job = Some(Job { progress, cancel, rx, done: drx });
    }

    /// the results that came in, the job's end, a re-export (checked once a second)
    fn poll(&mut self, app: &App, name: &str) {
        if let Some(j) = &self.job {
            while let Ok((i, r)) = j.rx.try_recv() {
                self.results[i] = Some(r);
            }
            match j.done.try_recv() {
                Ok(end) => {
                    if let Err(e) = end {
                        self.error = Some(e);
                    }
                    self.job = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.error = Some("the comparison stopped without a result".into());
                    self.job = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.checked.elapsed() > Duration::from_secs(1) {
            self.checked = Instant::now();
            let now = export_stamp(app, name);
            if now == self.export_stamp {
                self.changed = None;
                return;
            }
            // a re-export: what runs is for the old one; start once the files have held for a
            // second and no build writes them
            if let Some(j) = &self.job {
                j.cancel.store(true, Ordering::Relaxed);
            }
            let building = app.open.as_ref().is_some_and(|o| o.build.is_some());
            match &self.changed {
                Some((seen, at)) if *seen == now && at.elapsed() >= Duration::from_secs(1) && !building && self.job.is_none() => {
                    *self = CompareView::new(app, name);
                }
                Some((seen, _)) if *seen == now => {}
                _ => self.changed = Some((now, Instant::now())),
            }
        }
    }

    fn diffs(&self) -> Vec<(usize, Diff)> {
        self.results.iter().enumerate()
            .filter_map(|(i, r)| match r { Some(Ok(e)) => Some((i, compare::diff(&e.recording, &e.render))), _ => None }).collect()
    }
}

/// a lane: its title, its values of a pair re its source's medians (none: no point), its range
/// and the band within the thresholds
struct Lane {
    title: String,
    value: fn(&Diff, &Pair, &Medians) -> Vec<(f32, Mark)>,
    lo: f32,
    hi: f32,
    band: (f32, f32),
    hint: &'static str,
}

/// a point's shape: a dot, a diamond (the length), a hollow diamond (the length of a stopped
/// recording: a lower bound)
#[derive(Clone, Copy, PartialEq)]
enum Mark {
    Dot,
    Diamond,
    Hollow,
}

/// the lanes; `ms`: the shown sources' labels and medians (their overall differences, in the
/// titles); `several`: the instrument has more than one source
fn lanes(ms: &[(&str, &Medians)], several: bool) -> Vec<Lane> {
    let lg = compare::DECAY_RATIO.log2();
    let each = |f: &dyn Fn(&Medians) -> String| match ms {
        [(_, m)] => f(m),
        _ => ms.iter().map(|(l, m)| format!("{l} {}", f(m))).collect::<Vec<_>>().join("; "),
    };
    let whose = match ms {
        [(label, _)] if several => format!("{label}'s"),
        [_] | [] => "the instrument's".to_string(),
        _ => "each source's".to_string(),
    };
    vec![
        Lane { title: format!("Level (dB re {whose} median per dynamic: {})", each(&|m| m.level.iter().map(|(k, v)| format!("{k} {v:+.1}")).collect::<Vec<_>>().join(" "))),
            value: |d, p, m| m.level_ref(&p.dynamic, p.strike.key as i64).map(|r| (d.level_db - r, Mark::Dot)).into_iter().collect(),
            lo: -24.0, hi: 24.0, band: (-compare::LEVEL_DB, compare::LEVEL_DB),
            hint: "render − recording over the first 0.5 s; each dynamic's median offset (the volume, the velocity curve) taken off; \
                a source recorded in octave runs (level_groups) re its run's, one whose files are normalized not shown" },
        Lane { title: format!("Pitch (cents re {whose} median: {})", each(&|m| format!("{:+.0}", m.pitch))),
            value: |d, _, m| d.pitch_cents.map(|c| (c - m.pitch, Mark::Dot)).into_iter().collect(),
            lo: -60.0, hi: 60.0, band: (-compare::PITCH_CENTS, compare::PITCH_CENTS),
            hint: "the render's fundamental re the recording's, its median (a set at another concert pitch) taken off" },
        Lane { title: format!("Decay re {whose} median (render / recording: {}; circles early decay, diamonds length, hollow: the recording was stopped)",
                each(&|m| format!("early ×{:.2} length ×{:.2}", m.early, m.length))),
            value: |d, _, m| d.early.map(|r| ((r / m.early).log2(), Mark::Dot)).into_iter()
                .chain(d.length.map(|r| ((r / m.length).log2(), if d.length_stopped { Mark::Hollow } else { Mark::Diamond }))).collect(),
            lo: -3.0, hi: 3.0, band: (-lg, lg),
            hint: "render / recording re its median, log scale (×8 at the edges): the early decay rate (above the line the key dies faster \
                than the others) and the length, the time to fall 50 dB under the first 0.5 s or as far as the recording shows (above the line it rings longer; flagged past ×2 either way, a stopped \
                recording's only below)" },
        Lane { title: format!("Spectrum (dB rms re {whose} median: {})", each(&|m| format!("{:.1}", m.spectrum))),
            value: |d, _, m| vec![(d.spectrum_db - m.spectrum, Mark::Dot)],
            lo: -12.0, hi: 24.0, band: (-12.0, compare::SPECTRUM_DB),
            hint: "the partials' levels re the strongest, render against recording, the median taken off" },
    ]
}

/// the four lanes on one key axis; returns a clicked pair
fn plot(ui: &mut egui::Ui, v: &CompareView, diffs: &[(usize, Diff)], ms: &BTreeMap<String, Medians>) -> Option<usize> {
    let (lo_key, hi_key) = v.pairs.iter().fold((i64::MAX, i64::MIN), |(a, b), p| (a.min(p.note), b.max(p.note)));
    if lo_key > hi_key {
        return None;
    }
    let keys = (hi_key - lo_key + 1) as f32;
    let s = &ui.spacing().scroll;
    let room = ui.available_width() - (s.bar_width + s.bar_inner_margin + s.bar_outer_margin) - 14.0;
    let w = (room / keys).clamp(4.0, 14.0);
    let lane_h = 90.0;
    let mut clicked = None;
    let shown: Vec<(&str, &Medians)> = v.sources.iter().zip(&v.shown_sources).filter(|(_, on)| **on)
        .filter_map(|((id, label), _)| ms.get(id).map(|m| (label.as_str(), m))).collect();
    for lane in lanes(&shown, v.sources.len() > 1) {
        ui.label(egui::RichText::new(&lane.title).strong()).on_hover_text(lane.hint);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w * keys, lane_h + 14.0), egui::Sense::click());
        let painter = ui.painter_at(rect.with_max_x(rect.right() + 14.0));
        let plot = egui::Rect::from_min_size(rect.min, egui::vec2(w * keys, lane_h));
        let y = |val: f32| plot.bottom() - (val.clamp(lane.lo, lane.hi) - lane.lo) / (lane.hi - lane.lo) * plot.height();
        painter.rect_filled(plot, 0.0, ui.visuals().faint_bg_color);
        painter.rect_filled(egui::Rect::from_x_y_ranges(plot.x_range(), y(lane.band.1)..=y(lane.band.0)), 0.0,
            ui.visuals().widgets.inactive.bg_fill);
        if lane.lo < 0.0 {
            painter.hline(plot.x_range(), y(0.0), egui::Stroke::new(1.0, ui.visuals().weak_text_color()));
        }
        for n in lo_key..=hi_key {
            if n % 12 == 0 {
                let x = plot.left() + (n - lo_key) as f32 * w;
                painter.text(egui::pos2(x, rect.bottom() - 7.0), egui::Align2::LEFT_CENTER, pe_editor::view::note_label(n),
                    egui::FontId::monospace(9.0), ui.visuals().weak_text_color());
            }
        }
        let mut hover: Option<(f32, usize)> = None;
        let pointer = resp.hover_pos();
        for (i, d) in diffs {
            let p = &v.pairs[*i];
            let (Some(dyn_i), Some(m)) = (compare::FITTED.iter().position(|x| *x == p.dynamic), ms.get(&p.source)) else { continue };
            let flagged = !compare::offs_at(d, p, m).is_empty();
            if !v.shown(*i) || (v.flagged_only && !flagged) {
                continue;
            }
            for (val, mark) in (lane.value)(d, p, m) {
                let c = egui::pos2(plot.left() + ((p.note - lo_key) as f32 + 0.5) * w, y(val));
                let r = if flagged { 4.0 } else { 2.5 };
                let diamond = |r: f32| vec![c + egui::vec2(0.0, -r), c + egui::vec2(r, 0.0), c + egui::vec2(0.0, r), c + egui::vec2(-r, 0.0)];
                match mark {
                    Mark::Dot => { painter.circle_filled(c, r, COLORS[dyn_i]); }
                    Mark::Diamond => { painter.add(egui::Shape::convex_polygon(diamond(r + 0.5), COLORS[dyn_i], egui::Stroke::NONE)); }
                    Mark::Hollow => { painter.add(egui::Shape::closed_line(diamond(r + 0.5), egui::Stroke::new(1.5, COLORS[dyn_i]))); }
                }
                if flagged {
                    painter.circle_stroke(c, r + 1.0, egui::Stroke::new(1.0, ui.visuals().strong_text_color()));
                }
                if v.selected == Some(*i) {
                    painter.circle_stroke(c, r + 3.0, egui::Stroke::new(1.5, ui.visuals().strong_text_color()));
                }
                if let Some(m) = pointer {
                    let dist = m.distance(c);
                    if dist < 6.0 && hover.is_none_or(|h| dist < h.0) {
                        hover = Some((dist, *i));
                    }
                }
            }
        }
        if let Some((_, i)) = hover {
            let p = &v.pairs[i];
            let d = &diffs.iter().find(|x| x.0 == i).unwrap().1;
            let m = &ms[&p.source];
            resp.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(format!("{} ({}) {} {}  {}", pe_editor::view::note_label(p.note), p.note, p.dynamic, v.label(&p.source), p.path.display()));
                ui.label(numbers(d, p, m));
            });
            if resp.clicked() {
                clicked = Some(i);
            }
        }
    }
    clicked
}

fn numbers(d: &Diff, p: &Pair, m: &Medians) -> String {
    let opt = |x: Option<f32>, f: &dyn Fn(f32) -> String| x.map(f).unwrap_or_else(|| "–".into());
    let ratio = |r: f32| format!("×{r:.2}");
    format!("level {} re its dynamic, pitch {}, early decay {} (re the median {}), length {}{} (re the median {}), \
        late decay {} (not flagged), spectrum {:.1} dB",
        opt(m.level_ref(&p.dynamic, p.strike.key as i64).map(|r| d.level_db - r), &|l| format!("{l:+.1} dB")), opt(d.pitch_cents, &|c| format!("{c:+.0} cents")),
        opt(d.early, &ratio), opt(d.early.map(|r| r / m.early), &ratio),
        opt(d.length, &ratio), if d.length_stopped { " (the recording was stopped)" } else { "" }, opt(d.length.map(|r| r / m.length), &ratio),
        opt(d.late, &ratio), d.spectrum_db)
}

/// the flagged pairs, the worst first (or by key); returns a clicked one
fn flagged(ui: &mut egui::Ui, v: &mut CompareView, diffs: &[(usize, Diff)], ms: &BTreeMap<String, Medians>) -> Option<usize> {
    use egui_extras::{Column, TableBuilder};
    let shown: Vec<&(usize, Diff)> = diffs.iter().filter(|(i, _)| v.shown(*i) && ms.contains_key(&v.pairs[*i].source)).collect();
    let mut rows: Vec<(usize, f32, Vec<Off>)> = shown.iter().map(|(i, d)| {
        let (p, m) = (&v.pairs[*i], &ms[&v.pairs[*i].source]);
        (*i, compare::badness_at(d, p, m), compare::offs_at(d, p, m))
    }).filter(|r| !r.2.is_empty()).collect();
    if v.sort_worst_first {
        rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    } else {
        rows.sort_by_key(|r| (v.pairs[r.0].note, v.pairs[r.0].dynamic.clone()));
    }
    ui.label(format!("{} of {} pairs shown flagged", rows.len(), shown.len()));
    let several = v.sources.len() > 1;
    let mut clicked = None;
    let s = &ui.spacing().scroll;
    let w = ui.available_width() - (s.bar_width + s.bar_inner_margin + s.bar_outer_margin);
    ui.allocate_ui(egui::vec2(w, 260.0), |ui| {
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        ui.style_mut().interaction.selectable_labels = false;
        TableBuilder::new(ui).id_salt("flagged").striped(true).sense(egui::Sense::click()).max_scroll_height(240.0)
            .column(Column::auto().at_least(80.0)).column(Column::auto().at_least(if several { 140.0 } else { 60.0 })).column(Column::remainder().at_least(100.0).clip(true))
            .header(20.0, |mut h| {
                h.col(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("Key").strong()).frame(false)).on_hover_text("sort by key").clicked() {
                        v.sort_worst_first = false;
                    }
                });
                h.col(|ui| {
                    ui.strong(if several { "Dynamic, source" } else { "Dynamic" });
                });
                h.col(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("What is off").strong()).frame(false)).on_hover_text("sort the worst first").clicked() {
                        v.sort_worst_first = true;
                    }
                });
            })
            .body(|body| {
                body.rows(18.0, rows.len(), |mut row| {
                    let (i, _, offs) = &rows[row.index()];
                    let p = &v.pairs[*i];
                    row.set_selected(v.selected == Some(*i));
                    row.col(|ui| {
                        ui.label(format!("{} ({})", pe_editor::view::note_label(p.note), p.note));
                    });
                    row.col(|ui| {
                        ui.label(if several { format!("{} {}", p.dynamic, v.label(&p.source)) } else { p.dynamic.clone() });
                    });
                    row.col(|ui| {
                        ui.label(offs.iter().map(Off::text).collect::<Vec<_>>().join(", ")).on_hover_text(p.path.display().to_string());
                    });
                    if row.response().clicked() {
                        clicked = Some(*i);
                    }
                });
            });
    });
    clicked
}

/// the selected pair's clips made on a thread (the recording decoded, the render again, and the
/// export before the last build when the project keeps one)
fn make_clips(app: &App, name: &str, pair: &Pair, i: usize) -> Receiver<Result<Clips, String>> {
    let (tx, rx) = channel();
    let (pair, export, prev) = (pair.clone(), compare::export_dir(&app.repo, name), compare::previous_export_dir(&app.repo, name));
    std::thread::spawn(move || {
        let res = (|| {
            let rec = audio::decode(&pair.path, Some(audio::LONG_MAX_S + 10.0))?;
            let m = audio::measure_long(&rec.mono(), rec.rate as f32, pair.note as f64);
            let seconds = compare::render_seconds(&m);
            let rate = rec.rate;
            let render = |dir: &std::path::Path| -> Result<Clip, String> {
                let (l, r) = compare::Renderer::new(dir)?.render_stereo(&pair.strike, rate, seconds)?;
                Ok(Clip { rate, channels: 2, data: l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect() })
            };
            let ren = render(&export)?;
            let orig = if prev.join("instrument.json").is_file() {
                render(&prev).map(|c| {
                    let x = c.mono();
                    let on = audio::onset(&x);
                    let level = audio::level_db(&x, on, (0.5 * rate as f32) as usize);
                    (c, level, on as f32 / rate as f32)
                })
            } else {
                Err("no export before the last build (a build whose export changed keeps the one before)".into())
            };
            Ok(Clips { pair: i, rec, ren, orig })
        })();
        let _ = tx.send(res);
    });
    rx
}

fn ab(ui: &mut egui::Ui, app: &App, v: &mut CompareView, ms: &BTreeMap<String, Medians>) {
    let Some(i) = v.selected else {
        ui.label("Select a pair in the plot or the table to hear it.");
        return;
    };
    let p = v.pairs[i].clone();
    let entry = match &v.results[i] { Some(Ok(e)) => Some(e.clone()), _ => None };
    // Instrument plays against the key's own reference (its run's, with octave groups)
    let median_db = ms.get(&p.source).map_or(0.0, |m| m.level_ref(&p.dynamic, p.strike.key as i64).unwrap_or(m.level_of(&p.dynamic)));
    ui.horizontal(|ui| {
        ui.strong(format!("{} ({}) {}", pe_editor::view::note_label(p.note), p.note, p.dynamic));
        let file = p.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        ui.label(egui::RichText::new(file).monospace()).on_hover_text(p.path.display().to_string());
    });
    if let Some((k, rx)) = &v.making {
        if *k == i {
            match rx.try_recv() {
                Ok(Ok(c)) => { v.clips = Some(c); v.making = None; }
                Ok(Err(e)) => { v.error = Some(e); v.making = None; }
                Err(_) => { ui.spinner(); }
            }
        }
    }
    let ready = v.clips.as_ref().is_some_and(|c| c.pair == i);
    let orig_why = v.clips.as_ref().and_then(|c| c.orig.as_ref().err().cloned());
    let player = match &app.player { Ok(p) => Some(p), Err(_) => None };
    let reason = app.player.as_ref().err().cloned().unwrap_or_default();
    let can = ready && player.is_some();
    ui.horizontal(|ui| {
        ui.label("Level");
        for (mode, label, hover) in [
            (compare::LevelMatch::Off, "Off", "as rendered: the instrument's output gain heard too"),
            (compare::LevelMatch::Pair, "Pair", "the render and the original at the recording's level (a level outlier is not heard)"),
            (compare::LevelMatch::Instrument, "Instrument", "the source's median difference for the dynamic taken off: a key's own level difference, \
                what a level correction changes, heard; the instrument's output gain not"),
        ] {
            ui.selectable_value(&mut v.level_match, mode, label).on_hover_text(hover);
        }
        ui.checkbox(&mut v.mono, "Mono").on_hover_text("every side mixed to mono, as they are measured (off: in stereo, the renders at the recorded microphones)");
    });
    ui.horizontal(|ui| {
        let gains = gains(v, entry.as_ref(), median_db);
        let mut press = |side: Side, label: &str, ui: &mut egui::Ui| {
            let needs_orig = matches!(side, Side::Original | Side::All);
            let b = ui.add_enabled(can && !(needs_orig && orig_why.is_some()), egui::Button::new(label));
            let b = match (&orig_why, player.is_none()) {
                (_, true) => b.on_disabled_hover_text(&reason),
                (Some(why), false) if needs_orig && ready => b.on_disabled_hover_text(why),
                _ => b,
            };
            let b = match side {
                Side::Original => b.on_hover_text("the instrument before the last build"),
                Side::All => b.on_hover_text("the recording, the original, the render"),
                _ => b,
            };
            if b.clicked() {
                play(v, player.unwrap(), side, gains, None);
            }
        };
        press(Side::Recording, "Recording", ui);
        press(Side::Original, "Original", ui);
        press(Side::Render, "Render", ui);
        press(Side::Both, "A then B", ui);
        press(Side::All, "All three", ui);
        let swappable = can && matches!(v.playing, Some(Side::Recording | Side::Original | Side::Render)) && player.is_some_and(|p| p.playing());
        if ui.add_enabled(swappable, egui::Button::new("Swap"))
            .on_hover_text("the next side (recording, original, render) at the same time since the onset (Tab)").clicked() {
            swap(v, player.unwrap(), gains, entry.as_ref());
        }
        if ui.add_enabled(player.is_some_and(|p| p.playing()), egui::Button::new("Stop")).clicked() {
            player.unwrap().stop();
            v.playing = None;
        }
    });
    // Space plays the recording or stops; Tab swaps (taken out of the input for this page:
    // `App::raw_input_hook`)
    if can {
        let (space, tab) = app.compare_keys;
        let pl = player.unwrap();
        let gains = gains(v, entry.as_ref(), median_db);
        if space {
            if pl.playing() { pl.stop(); v.playing = None; } else { play(v, pl, Side::Recording, gains, None); }
        }
        if tab && pl.playing() {
            swap(v, pl, gains, entry.as_ref());
        }
    }
}

/// the render's and the original's gains (dB) as the Level choice plays them; `median_db`: the
/// pair's source's median level difference for its dynamic
fn gains(v: &CompareView, e: Option<&Entry>, median_db: f32) -> (f32, f32) {
    let orig = v.clips.as_ref().and_then(|c| c.orig.as_ref().ok()).map(|o| o.1);
    e.map_or((0.0, 0.0), |e| compare::playback_gains(v.level_match, e, orig, median_db))
}

fn play(v: &mut CompareView, pl: &pe_editor::player::Player, side: Side, gains: (f32, f32), from: Option<f32>) {
    let Some(c) = &v.clips else { return };
    let prep = |x: &Clip, gain: f32| player::gained(&if v.mono { player::mono(x) } else { x.clone() }, gain);
    let (rec, ren) = (prep(&c.rec, 0.0), prep(&c.ren, gains.0));
    let orig = c.orig.as_ref().ok().map(|o| prep(&o.0, gains.1));
    match (side, &orig) {
        (Side::Recording, _) => pl.play_clip(&rec, from.unwrap_or(0.0)),
        (Side::Render, _) => pl.play_clip(&ren, from.unwrap_or(0.0)),
        (Side::Original, Some(o)) => pl.play_clip(o, from.unwrap_or(0.0)),
        (Side::Both, _) => pl.play_clip(&player::a_then_b(&rec, &ren, 0.3), 0.0),
        (Side::All, Some(o)) => pl.play_clip(&player::a_then_b(&player::a_then_b(&rec, o, 0.3), &ren, 0.3), 0.0),
        (Side::Original | Side::All, None) => return,
    }
    v.playing = Some(side);
}

/// the next side, recording → original → render (the original skipped when there is none), at the
/// same time since its onset
fn swap(v: &mut CompareView, pl: &pe_editor::player::Player, gains: (f32, f32), e: Option<&Entry>) {
    let (Some(e), Some(pos), Some(now)) = (e, pl.position_s(), v.playing) else { return };
    let orig_onset = v.clips.as_ref().and_then(|c| c.orig.as_ref().ok()).map(|o| o.2);
    let onset = |s: Side| match s {
        Side::Original => orig_onset.unwrap_or(0.0),
        Side::Render => e.render.onset_s,
        _ => e.recording.onset_s,
    };
    let next = match (now, orig_onset) {
        (Side::Recording, Some(_)) => Side::Original,
        (Side::Recording, None) | (Side::Original, _) => Side::Render,
        (Side::Render, _) => Side::Recording,
        (Side::Both | Side::All, _) => return,
    };
    play(v, pl, next, gains, Some(player::swap_to(pos, onset(now), onset(next))));
}

pub fn page(app: &mut App, ui: &mut egui::Ui) {
    let Some(name) = app.open.as_ref().map(|o| o.project.name.clone()) else { return };
    if app.open.as_ref().unwrap().compare.is_none() {
        let v = CompareView::new(app, &name);
        app.open.as_mut().unwrap().compare = Some(v);
    }
    let mut v = app.open.as_mut().unwrap().compare.take().unwrap();
    v.poll(app, &name);
    super::steps::hint(ui, "Each recorded key and dynamic against the instrument's render of it. Click a point or a row to hear the pair; \
        Space plays or stops, Tab swaps. Shaded: within the thresholds; outlined: flagged.");
    // the driver's status (read for the Build page): a stale step means a build would change the
    // export the renders come from
    let stale: Vec<String> = app.open.as_ref().and_then(|o| o.status.as_ref()).and_then(|s| s.as_ref().ok())
        .map(|s| s.rows.iter().filter(|r| r.reason.is_some()).map(|r| r.step.clone()).collect()).unwrap_or_default();
    if !stale.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(200, 160, 40), format!("stale steps ({}): the export compared here is from before them; build to compare what the project now gives",
            stale.join(", ")));
    }
    if let Some(why) = &v.none_why {
        ui.label(format!("nothing to compare: {why}"));
    }
    if let Some(e) = &v.error {
        ui.colored_label(egui::Color32::RED, e);
    }
    let diffs = v.diffs();
    let groups: BTreeMap<String, compare::LevelGroups> = app.open.as_ref().map(|o| v.sources.iter()
        .filter_map(|(id, _)| o.project.source_json(id).map(|j| (id.clone(), compare::level_groups_of(&j)))).collect()).unwrap_or_default();
    let ms = compare::medians_grouped(&diffs.iter().map(|(i, d)| (&v.pairs[*i], d)).collect::<Vec<_>>(), &groups);
    ui.horizontal(|ui| {
        if let Some(j) = &v.job {
            let done = j.progress.load(Ordering::Relaxed);
            ui.add(egui::ProgressBar::new(done as f32 / v.pairs.len().max(1) as f32).desired_width(240.0)
                .text(format!("rendering {done} / {}", v.pairs.len())));
            if ui.button("Cancel").clicked() {
                j.cancel.store(true, Ordering::Relaxed);
            }
        } else if !v.pairs.is_empty() && v.results.iter().any(Option::is_none) && ui.button("Render the rest").clicked() {
            v.start(app, &name);
        }
        if v.skipped > 0 {
            ui.label(format!("{} layers without a fitted dynamic", v.skipped));
        }
        if v.outside > 0 {
            ui.label(format!("{} recordings outside the keys A0–C8", v.outside));
        }
        for (k, d) in compare::FITTED.iter().enumerate() {
            ui.checkbox(&mut v.shown[k], egui::RichText::new(*d).color(COLORS[k]));
        }
        if v.sources.len() > 1 {
            ui.separator();
            for (k, (_, label)) in v.sources.iter().enumerate() {
                ui.checkbox(&mut v.shown_sources[k], label.as_str());
            }
        }
        ui.separator();
        ui.checkbox(&mut v.flagged_only, "Flagged only");
        ui.separator();
        let all = !v.pairs.is_empty() && v.results.iter().all(|r| matches!(r, Some(Ok(_))));
        let editable = app.editable();
        // a proposal starts from the corrections the renders carry (the export's level_db), never
        // from a table written but not built: it is never counted twice
        let same_export = export_stamp(app, &name) == v.export_stamp;
        let why = if !editable {
            Some("the project is read-only (a build runs, or the file does not parse)")
        } else if !same_export || v.job.is_some() {
            Some("the export changed since these renders: wait for them to be measured again")
        } else if !all {
            Some("every pair must be measured first (none failed, none left to render)")
        } else {
            None
        };
        if ui.add_enabled(why.is_none(), egui::Button::new("Correct levels…"))
            .on_disabled_hover_text(why.unwrap_or_default())
            .clicked()
        {
            let inst: serde_json::Value = std::fs::read_to_string(compare::export_dir(&app.repo, &name).join("instrument.json")).ok()
                .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            let current = compare::corrections_in_export(&inst, &v.pairs);
            let rows: Vec<(&Pair, &Diff)> = diffs.iter().map(|(i, d)| (&v.pairs[*i], d)).collect();
            v.uncorrected = groups.iter().filter(|(_, g)| **g == compare::LevelGroups::File).map(|(id, _)| v.label(id).to_string()).collect();
            let new = compare::level_corrections(&rows, &current, &groups);
            v.sway = compare::one_key_sway(&rows, &current, &groups);
            v.correcting = Some(new.into_iter().map(|(id, t)| (id.clone(), (current.get(&id).cloned(), t))).collect());
        }
    });
    let mut close = false;
    let mut write: Option<bool> = None; // Some(true) write, Some(false) clear
    if let Some(c) = &v.correcting {
        egui::Window::new("Correct levels").collapsible(false).show(ui.ctx(), |ui| {
            if !v.uncorrected.is_empty() {
                ui.label(format!("Not corrected (each file normalized to its peak: its level says nothing): {}", v.uncorrected.join(", ")));
            }
            if c.is_empty() && v.uncorrected.is_empty() {
                ui.label("No source has a dynamic with three or more measured keys: nothing to correct.");
            }
            for (id, (cur, new)) in c {
                ui.strong(v.label(id));
                if let Some(sw) = v.sway.get(id) {
                    let text = format!("One recorded key alone moves this table by up to {:.1} dB (leaving out {} {})",
                        sw.db, pe_editor::view::note_label(sw.key), sw.dynamic);
                    if sw.db >= compare::ONE_KEY_SWAY_WARN_DB {
                        // the words carry the warning, the color repeats it
                        ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {text}: the curve follows that key's playing; check it by ear before writing"));
                    } else {
                        ui.label(text);
                    }
                }
                for d in compare::FITTED {
                    let Some(nv) = new.db.get(d) else { continue };
                    let moved = new.keys.iter().zip(nv).filter(|(k, x)| (**x - cur.as_ref().map_or(0.0, |t| t.at(d, **k))).abs() > 1.0).count();
                    ui.horizontal(|ui| {
                        ui.label(format!("{d}: {moved} of {} keys move by more than 1 dB", new.keys.len()));
                        correction_curve(ui, &new.keys, nv, cur.as_ref().map(|t| new.keys.iter().map(|k| t.at(d, *k)).collect()));
                    });
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Write to project").on_hover_text("sets each source's [part.source.level_corrections]; Save, then Build applies them").clicked() {
                    write = Some(true);
                }
                if ui.button("Clear corrections").on_hover_text("removes every source's level corrections").clicked() {
                    write = Some(false);
                }
                close = ui.button("Cancel").clicked();
            });
        });
    }
    if let Some(w) = write {
        let today = chrono_free_today();
        if let (Some(o), Some(c)) = (app.open.as_mut(), v.correcting.as_ref()) {
            for (id, (_, new)) in c {
                let res = if w {
                    o.project.set_source_table(id, "level_corrections", &new.to_json(),
                        &format!("from the Compare page, {today}: dB per recorded key, smoothed (running median over 5 keys); the export interpolates between the keys and holds the ends"))
                } else {
                    o.project.remove_source_key(id, "level_corrections")
                };
                if let Err(e) = res {
                    v.error = Some(e);
                }
            }
        }
        close = true;
    }
    if close {
        v.correcting = None;
    }
    let failed: Vec<(usize, String)> = v.results.iter().enumerate()
        .filter_map(|(i, r)| match r { Some(Err(e)) => Some((i, e.clone())), _ => None }).collect();
    if !failed.is_empty() {
        egui::CollapsingHeader::new(format!("{} pairs not compared", failed.len())).show(ui, |ui| {
            for (i, e) in failed.iter().take(20) {
                ui.label(format!("{}: {e}", v.pairs[*i].path.display()));
            }
        });
    }
    let mut pick = plot(ui, &v, &diffs, &ms);
    ui.separator();
    if let Some(i) = flagged(ui, &mut v, &diffs, &ms) {
        pick = Some(i);
    }
    if let Some(i) = pick {
        if v.selected != Some(i) {
            v.selected = Some(i);
            v.clips = None;
            v.making = Some((i, make_clips(app, &name, &v.pairs[i].clone(), i)));
        }
    }
    ui.separator();
    ab(ui, app, &mut v, &ms);
    if v.job.is_some() || v.making.is_some() || app.player.as_ref().is_ok_and(|p| p.playing()) {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    if let Some(o) = app.open.as_mut() {
        o.compare = Some(v);
    }
}

/// a correction along the keys (dB, ±24 at the edges): the proposed one, the current one fainter
fn correction_curve(ui: &mut egui::Ui, keys: &[i64], new: &[f32], cur: Option<Vec<f32>>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(260.0, 40.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
    p.hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, ui.visuals().weak_text_color()));
    let (lo, hi) = (*keys.first().unwrap_or(&0) as f32, *keys.last().unwrap_or(&1) as f32);
    let pt = |k: i64, db: f32| egui::pos2(rect.left() + (k as f32 - lo) / (hi - lo).max(1.0) * rect.width(),
        rect.center().y - db.clamp(-24.0, 24.0) / 24.0 * rect.height() / 2.0);
    if let Some(c) = cur {
        p.add(egui::Shape::line(keys.iter().zip(&c).map(|(k, d)| pt(*k, *d)).collect(), egui::Stroke::new(1.0, ui.visuals().weak_text_color())));
    }
    p.add(egui::Shape::line(keys.iter().zip(new).map(|(k, d)| pt(*k, *d)).collect(), egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 130, 50))));
}

/// today as yyyy-mm-dd (UTC, from the system clock: no date crate in the editor)
fn chrono_free_today() -> String {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86400) as i64;
    // civil from days (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{}-{m:02}-{d:02}", if m <= 2 { y + 1 } else { y })
}
