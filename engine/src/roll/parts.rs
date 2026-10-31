//! A roll's strings and fine structure. Each key's strings and hammers are a weighted blend of
//! other pianos' by the morph's rules (geometric means for scales, arithmetic for offsets); the
//! frame keeps everything else. The responses' fine structure above 100–200 Hz is other pianos'
//! responses mixed, given the frame's 1/6-octave color; below, the frame's own (both microphones'
//! bass in phase, as a real pair's).
use super::bands;
use super::files::Export;
use crate::instrument::Error;
use crate::instrument::morph::{arith, scale};
use realfft::num_complex::Complex;
use serde_json::Value;

fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

const SCALES: [&str; 12] = ["f0", "B", "L", "T", "mass", "b1", "b3", "kappa", "horiz_gain", "horiz_kappa", "f_max", "scatter_cents"];
const OFFSETS: [&str; 2] = ["kappa_reactive", "horiz_detune_cents"];

/// The numbers of `field` in each donor's `obj` that has one, with their weights.
fn having(objs: &[Option<&Value>], w: &[f64], field: &str) -> (Vec<f64>, Vec<f64>) {
    objs.iter().zip(w).filter_map(|(o, &w)| o.and_then(|o| o.get(field)).and_then(Value::as_f64).map(|v| (v, w))).unzip()
}

fn set(obj: &mut Value, field: &str, v: f64) {
    if let Some(slot) = obj.get_mut(field) {
        *slot = Value::from(v);
    }
}

/// The frame's strings and hammers, key by key, the donors' blended at their weights (normalized,
/// zeros dropped): scales geometrically, offsets arithmetically, the detunings over the donors
/// with the frame's string count, the level corrections with a donor's missing entry as 0 dB, the
/// hammer speeds per dynamic and the release noise's ratios over them. Every other field stays the
/// frame's.
pub fn blend_strings(frame: &mut Export, donors: &[(&Export, f64)]) -> Result<(), Error> {
    if donors.iter().any(|(_, w)| !(w.is_finite() && *w >= 0.0)) {
        return Err(err("roll: a strings weight is negative or not finite"));
    }
    let kept: Vec<(&Export, f64)> = donors.iter().filter(|(_, w)| *w > 0.0).copied().collect();
    let sum: f64 = kept.iter().map(|(_, w)| w).sum();
    if kept.is_empty() {
        return Err(err("roll: no strings donor with a weight"));
    }
    let w: Vec<f64> = kept.iter().map(|(_, x)| x / sum).collect();
    let keys = frame.json.get("notes").and_then(Value::as_array).map_or(0, Vec::len);
    for (d, _) in &kept {
        if d.json.get("notes").and_then(Value::as_array).map_or(0, Vec::len) != keys {
            return Err(err("roll: a strings donor with another number of keys"));
        }
    }
    for k in 0..keys {
        let notes: Vec<&Value> = kept.iter().map(|(d, _)| &d.json["notes"][k]).collect();
        let strings: Vec<Option<&Value>> = notes.iter().map(|n| n.get("string")).collect();
        let hammers: Vec<Option<&Value>> = notes.iter().map(|n| n.get("excitation").filter(|e| e.get("type").and_then(Value::as_str) == Some("hammer"))).collect();
        let note = &mut frame.json["notes"][k];
        let st = note.get_mut("string").ok_or_else(|| err(format!("roll: note {k} without strings")))?;
        for f in SCALES {
            let (x, ww) = having(&strings, &w, f);
            if !x.is_empty() {
                set(st, f, scale(&x, &ww));
            }
        }
        for f in OFFSETS {
            let (x, ww) = having(&strings, &w, f);
            if !x.is_empty() {
                set(st, f, arith(&x, &ww));
            }
        }
        // the loss law's exponent with the b3 it was fitted for: over every donor (one without counts
        // 2, the law as it was), written where any donor has one
        if strings.iter().any(|s| s.and_then(|s| s.get("loss_q")).is_some()) {
            let q: Vec<f64> = strings.iter().map(|s| s.and_then(|s| s.get("loss_q")).and_then(Value::as_f64).unwrap_or(2.0)).collect();
            st["loss_q"] = Value::from(arith(&q, &w));
        }
        // the detunings over the donors with the frame's string count
        let s = st.get("S").and_then(Value::as_u64).unwrap_or(0) as usize;
        let same: Vec<(Vec<f64>, f64)> = strings
            .iter()
            .zip(&w)
            .filter_map(|(o, &w)| {
                let o = (*o)?;
                let det: Vec<f64> = o.get("detune_cents")?.as_array()?.iter().filter_map(Value::as_f64).collect();
                (o.get("S").and_then(Value::as_u64) == Some(s as u64) && det.len() == s).then_some((det, w))
            })
            .collect();
        if !same.is_empty() {
            let ws: Vec<f64> = same.iter().map(|x| x.1).collect();
            let det: Vec<f64> = (0..s).map(|j| arith(&same.iter().map(|x| x.0[j]).collect::<Vec<_>>(), &ws)).collect();
            st["detune_cents"] = Value::from(det);
        }
        let objs: Vec<Option<&Value>> = notes.iter().map(|n| Some(*n)).collect();
        let (x, ww) = having(&objs, &w, "recorded_cents");
        if !x.is_empty() {
            set(note, "recorded_cents", arith(&x, &ww));
        }
        if let Some(h) = note.get_mut("excitation").filter(|e| e.get("type").and_then(Value::as_str) == Some("hammer")) {
            for f in ["mass", "felt_K"] {
                let (x, ww) = having(&hammers, &w, f);
                if !x.is_empty() {
                    set(h, f, scale(&x, &ww));
                }
            }
            for f in ["felt_p", "strike_pos"] {
                let (x, ww) = having(&hammers, &w, f);
                if !x.is_empty() {
                    set(h, f, arith(&x, &ww));
                }
            }
            let n = h.get("balance").and_then(Value::as_array).map_or(0, Vec::len);
            let rows: Vec<(Vec<f64>, f64)> = hammers
                .iter()
                .zip(&w)
                .filter_map(|(o, &w)| Some((o.as_ref()?.get("balance")?.as_array()?.iter().filter_map(Value::as_f64).collect::<Vec<f64>>(), w)))
                .filter(|(b, _)| b.len() == n)
                .collect();
            if n > 0 && !rows.is_empty() {
                let ws: Vec<f64> = rows.iter().map(|r| r.1).collect();
                h["balance"] = Value::from((0..n).map(|j| arith(&rows.iter().map(|r| r.0[j]).collect::<Vec<_>>(), &ws)).collect::<Vec<f64>>());
            }
            // the felt's core: its stiffness over every donor (one without counts 0), its depth
            // weighted by each donor's stiffness; written where the blend has one, else left out
            let get = |o: &Option<&Value>, f: &str| o.and_then(|o| o.get(f)).and_then(Value::as_f64).unwrap_or(0.0);
            let ck: Vec<f64> = hammers.iter().map(|o| get(o, "core_K")).collect();
            let k = arith(&ck, &w);
            let o = h.as_object_mut().expect("a hammer excitation is an object");
            if k > 0.0 {
                let depth: Vec<f64> = hammers.iter().zip(&ck).map(|(o, c)| c * get(o, "core_depth")).collect();
                o.insert("core_K".into(), Value::from(k));
                o.insert("core_depth".into(), Value::from(arith(&depth, &w) / k));
            } else {
                o.remove("core_K");
                o.remove("core_depth");
            }
        }
        // the level corrections: a donor without one counts 0 dB
        let rows: Vec<Vec<Vec<f64>>> = notes
            .iter()
            .map(|n| n.get("level_db").and_then(Value::as_array).map_or_else(Vec::new, |a| a.iter().map(|t| t.as_array().map_or_else(Vec::new, |t| t.iter().filter_map(Value::as_f64).collect())).collect()))
            .collect();
        let len = rows.iter().map(Vec::len).max().unwrap_or(0);
        if len > 0 {
            let lv: Vec<Vec<f64>> = (0..len).map(|j| (0..3).map(|e| arith(&rows.iter().map(|r| r.get(j).and_then(|t| t.get(e)).copied().unwrap_or(0.0)).collect::<Vec<_>>(), &w)).collect()).collect();
            note["level_db"] = Value::from(lv);
        } else if let Some(o) = note.as_object_mut() {
            o.remove("level_db");
        }
        // the release noise's ratio per dynamic over ln of its hammer speed: with the speeds blended,
        // both columns blend (as the morph's), when every donor has the frame's dynamics
        let curves: Vec<Option<Vec<[f64; 2]>>> = notes
            .iter()
            .map(|n| n.pointer("/release/ratio_db").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect()))
            .collect();
        if let Some(r) = note.pointer_mut("/release/ratio_db").and_then(Value::as_array_mut) {
            let n = r.len();
            if curves.iter().all(|c| c.as_ref().is_some_and(|c| c.len() == n)) {
                let curves: Vec<&Vec<[f64; 2]>> = curves.iter().flatten().collect();
                for (j, p) in r.iter_mut().enumerate() {
                    *p = Value::from((0..2).map(|c| arith(&curves.iter().map(|x| x[j][c]).collect::<Vec<_>>(), &w)).collect::<Vec<f64>>());
                }
            }
        }
    }
    // the hammer speeds per dynamic
    let speeds: Vec<Vec<f64>> = kept.iter().map(|(d, _)| d.json.pointer("/velocity/speed").and_then(Value::as_array).map_or_else(Vec::new, |a| a.iter().filter_map(Value::as_f64).collect())).collect();
    if let Some(sp) = frame.json.pointer_mut("/velocity/speed").and_then(Value::as_array_mut) {
        for (j, v) in sp.iter_mut().enumerate() {
            let (x, ww): (Vec<f64>, Vec<f64>) = speeds.iter().zip(&w).filter_map(|(s, &w)| s.get(j).map(|&x| (x, w))).unzip();
            if !x.is_empty() {
                *v = Value::from(scale(&x, &ww));
            }
        }
    }
    Ok(())
}

/// The frame's responses with a new fine structure: per polarization and microphone, the donors'
/// same responses each at unit energy, summed and given the frame's 1/6-octave color; below 100 Hz
/// the frame's own, above 200 Hz the mix, crossfaded linearly in log frequency between; each at the
/// frame's energy and, band by band, the frame's levels.
pub fn fine_structure(frame: &mut Export, donors: &[&Export]) -> Result<(), Error> {
    if donors.is_empty() {
        return Err(err("roll: no fine-structure donor"));
    }
    let sr = frame.json.get("reference_sr").and_then(Value::as_f64).unwrap_or(48000.0);
    let own = frame.responses()?;
    let theirs: Vec<Vec<_>> = donors.iter().map(|d| d.responses()).collect::<Result<_, _>>()?;
    let results: Vec<Result<Vec<f64>, Error>> = std::thread::scope(|scope| {
        let handles: Vec<_> = own.iter().map(|r| {
            let theirs = &theirs;
            scope.spawn(move || mixed(r, theirs, sr))
        }).collect();
        handles.into_iter().map(|h| h.join().expect("a response")).collect()
    });
    for (r, ir) in own.iter().zip(results) {
        frame.set_response(r.pol, r.channel, &ir?)?;
    }
    Ok(())
}

/// One response's new fine structure (`fine_structure`).
fn mixed(r: &super::files::Response, theirs: &[Vec<super::files::Response>], sr: f64) -> Result<Vec<f64>, Error> {
    let len = r.ir.len();
    let mut mix = vec![0.0; len];
    for t in theirs {
        let d = t.iter().find(|x| x.pol == r.pol && x.channel == r.channel).ok_or_else(|| err("roll: a donor without this response"))?;
        let norm = d.ir.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-300);
        mix.iter_mut().zip(&d.ir).for_each(|(m, v)| *m += v / norm);
    }
    let colored = bands::recolor(&mix, &r.ir, sr);
    let n = 2 * len;
    let pad = |v: &[f64]| {
        let mut y = vec![0.0; n];
        y[..len].copy_from_slice(&v[..len.min(v.len())]);
        y
    };
    let (x, y) = (bands::forward(&pad(&r.ir)), bands::forward(&pad(&colored)));
    let z: Vec<Complex<f64>> = x
        .iter()
        .zip(&y)
        .enumerate()
        .map(|(k, (a, b))| {
            let f = (k as f64 * sr / n as f64).max(1.0);
            let blend = (f / 100.0).log2().clamp(0.0, 1.0);
            a * (1.0 - blend) + b * blend
        })
        .collect();
    // then the frame's band levels in the body's own measure from 200 Hz up (recolor's box
    // smoothing is not the bands' masks): one smooth gain curve on the spectrum, 0 dB up to 180 Hz
    // (the bass is the frame's), refined over six passes since the bands overlap
    let g = bands::grid();
    let lg: Vec<f64> = g.iter().map(|f| f.log2()).collect();
    let target = bands::band_stats(&r.ir, sr).0;
    let mut gain_db = [0.0f64; bands::BANDS];
    let render = |gain_db: &[f64; bands::BANDS]| {
        let spec: Vec<Complex<f64>> = z
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let f = (k as f64 * sr / n as f64).max(1.0);
                let db = if f < 180.0 { 0.0 } else { super::body::interp(f.log2(), &lg, gain_db) };
                c * 10f64.powf(db / 20.0)
            })
            .collect();
        let mut out = bands::inverse(spec, n);
        out.truncate(len);
        out
    };
    let mut out = render(&gain_db);
    for _ in 0..6 {
        let lv = bands::band_stats(&out, sr).0;
        for k in (0..bands::BANDS).filter(|&k| g[k] >= 200.0) {
            gain_db[k] = (gain_db[k] + target[k] - lv[k]).clamp(-40.0, 40.0);
        }
        out = render(&gain_db);
    }
    let e0: f64 = r.ir.iter().map(|v| v * v).sum();
    let e1: f64 = out.iter().map(|v| v * v).sum();
    let k = (e0 / e1.max(1e-300)).sqrt();
    Ok(out.iter().map(|v| v * k).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// the committed exports with 88 keys: one with a fitted loss exponent, one without
    fn pair() -> Option<(Export, Export)> {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments"));
        let mut dirs: Vec<_> = std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).collect();
        dirs.sort();
        let all: Vec<Export> = dirs.iter().filter_map(|d| Export::read(d).ok()).filter(|e| e.json["notes"].as_array().map_or(0, Vec::len) == 88).collect();
        let has = |e: &Export| e.json["notes"][15]["string"].get("loss_q").is_some();
        let with = all.iter().find(|e| has(e))?.clone();
        let without = all.iter().find(|e| !has(e) && e.json["notes"][15].get("string").is_some())?.clone();
        Some((with, without))
    }

    #[test]
    fn the_core_blends_by_its_stiffness() {
        // the felt's core: its stiffness the weighted mean (none counts 0), its depth weighted by
        // each donor's stiffness; donors without one leave the fields absent
        let Some((a, _)) = pair() else { return };
        let mut cored = a.clone();
        let h = &mut cored.json["notes"][15]["excitation"];
        h["core_K"] = Value::from(6e8);
        h["core_depth"] = Value::from(4e-4);
        let mut plain = a.clone();
        for k in ["core_K", "core_depth"] {
            plain.json["notes"][15]["excitation"].as_object_mut().unwrap().remove(k);
        }
        let mut frame = plain.clone();
        blend_strings(&mut frame, &[(&cored, 0.5), (&plain, 0.5)]).unwrap();
        let h = &frame.json["notes"][15]["excitation"];
        assert!((h["core_K"].as_f64().unwrap() - 3e8).abs() < 1e-3, "{h}");
        assert!((h["core_depth"].as_f64().unwrap() - 4e-4).abs() < 1e-15, "{h}");
        let mut alone = plain.clone();
        blend_strings(&mut alone, &[(&plain, 1.0)]).unwrap();
        assert!(alone.json["notes"][15]["excitation"].get("core_K").is_none());
        assert!(alone.json["notes"][15]["excitation"].get("core_depth").is_none());
    }

    #[test]
    fn the_strings_loss_exponent_blends_with_their_b3() {
        // b3 is a loss at 1 kHz under the donor's own exponent: blending one without the other plays
        // one piano's b3 with another's law (a donor without an exponent counts 2)
        let Some((a, e)) = pair() else { return };
        let qa = a.json["notes"][15]["string"]["loss_q"].as_f64().unwrap_or(2.0);
        assert!(qa < 2.0, "the donor's exponent is fitted: {qa}");
        let mut frame = e.clone();
        blend_strings(&mut frame, &[(&a, 0.5), (&e, 0.5)]).unwrap();
        let q = frame.json["notes"][15]["string"]["loss_q"].as_f64().unwrap_or(2.0);
        assert!((q - (qa + 2.0) / 2.0).abs() < 1e-12, "{q}");
        // a roll of donors without exponents keeps the field absent
        let mut alone = e.clone();
        blend_strings(&mut alone, &[(&e, 1.0)]).unwrap();
        assert!(alone.json["notes"][15]["string"].get("loss_q").is_none());
    }
}
