//! An instrument between instruments: several decoded instruments and their weights in, one
//! decoded instrument out (`Instrument::morph`; design log step 115).

use super::*;

/// The weights normalized and rounded to 1e-4 (a profile names them exactly), in their places
/// (a 0 stays).
pub(super) fn weights(w: &[f64]) -> Result<Vec<f64>, Error> {
    if w.is_empty() {
        return Err(err("morph: no instruments"));
    }
    if w.iter().any(|x| !(x.is_finite() && *x >= 0.0)) {
        return Err(err("morph: a weight is negative or not finite"));
    }
    let sum: f64 = w.iter().sum();
    if sum <= 0.0 {
        return Err(err("morph: every weight is 0"));
    }
    Ok(w.iter().map(|x| (x / sum * 1e4).round() / 1e4).collect())
}

/// The side with the largest weight (a tie: the first).
pub(super) fn heaviest(w: &[f64]) -> usize {
    (0..w.len()).fold(0, |best, i| if w[i] > w[best] { i } else { best })
}

/// The weighted mean.
pub(crate) fn arith(x: &[f64], w: &[f64]) -> f64 {
    x.iter().zip(w).map(|(x, w)| x * w).sum::<f64>() / w.iter().sum::<f64>()
}

/// The weighted geometric mean of a scale; arithmetic where a value is 0 or negative.
pub(crate) fn scale(x: &[f64], w: &[f64]) -> f64 {
    if x.iter().all(|v| *v > 0.0) {
        (x.iter().zip(w).map(|(x, w)| x.ln() * w).sum::<f64>() / w.iter().sum::<f64>()).exp()
    } else {
        arith(x, w)
    }
}

/// Whether `d` is a piano the morph knows: one register of hammer-struck strings, no stops,
/// mallets, motor, membrane, bend or measured board modes.
pub(super) fn check(d: &Decoded) -> Result<(), Error> {
    refusal(&d.raw).map_or(Ok(()), |w| Err(err(format!("{}: cannot morph: {w}", d.name))))
}

/// Why the instrument in `r` cannot morph (None: it can; `Facts::morph_family` reads this too, so the
/// plugin's report and the morph agree).
pub(super) fn refusal(r: &RawInstrument) -> Option<&'static str> {
    if r.format == 6 || r.notes.is_empty() {
        Some("not one register of notes")
    } else if r.notes.len() != 88 || r.notes.iter().enumerate().any(|(i, n)| n.note as usize != 21 + i) {
        Some("not the 88 keys 21 … 108 in order") // the notes pair by position
    } else if r.notes.iter().any(|n| !matches!((&n.excitation, &n.hammer), (Some(RawExcitation::Hammer(_)), _) | (None, Some(_)))) {
        Some("a note not struck by a hammer")
    } else if r.notes.iter().any(|n| n.string.modes.is_some()) {
        Some("a note of explicit modes")
    } else if !r.mallets.is_empty() || r.membrane.is_some() || r.motor || r.bend || r.buff.is_some() || r.lute.is_some() {
        Some("mallets, a membrane, a motor, a bend or a stop")
    } else if r.board.measured_from.is_some_and(|m| m < r.board.f.n) {
        Some("measured board modes")
    } else {
        None
    }
}

/// The morph at its sides' level: every key's peaks scaled so that its ff level (`Instrument::ff_level`) is
/// the weighted mean of theirs (`side_levels`, dB). The stored peaks are per-key means and miss
/// what the morphed physics radiates (halfway between Grand A and Grand D played 3 dB above both),
/// and the calibration level itself is no reference: the pianos sit up to 3 dB off it, so a morph
/// of a piano with itself would jump.
pub(super) fn morph_level(inst: &mut Instrument, side_levels: &[f64], w: &[f64]) -> Result<(), Error> {
    let off = inst.ff_level()? - arith(side_levels, w);
    let f = 10f64.powf(off / 20.0);
    inst.notes.iter_mut().for_each(|n| n.release.peak.iter_mut().for_each(|p| *p *= f));
    Ok(())
}

/// The morph's level table (`Instrument::dynamics`) from its sides' (`tables`: each one's table
/// and ff anchor speed): per key their levels at the same speed re their own ff speed (the tables
/// share that grid, `LEVEL_SPEEDS`), the weighted mean, at the morph's ff speed `ff`. Empty unless
/// every side has one on the same keys and grid.
pub(super) fn morph_dynamics(tables: &[(Dynamics, f64)], w: &[f64], ff: f64) -> Dynamics {
    let Some((first, s0)) = tables.first() else { return Dynamics::default() };
    let rel = |t: &Dynamics, s: f64| -> Vec<f64> { t.ln_speed.iter().map(|l| l - s.ln()).collect() };
    let r0 = rel(first, *s0);
    let same = tables.iter().all(|(t, s)| {
        !t.is_empty() && t.keys == first.keys && rel(t, *s).iter().zip(&r0).all(|(a, b)| (a - b).abs() < 1e-6) && t.ln_speed.len() == r0.len()
    });
    if !same {
        return Dynamics::default();
    }
    let db = (0..first.keys.len())
        .map(|k| (0..r0.len()).map(|j| tables.iter().zip(w).map(|((t, _), w)| w * t.db[k][j]).sum()).collect())
        .collect();
    Dynamics { keys: first.keys.clone(), ln_speed: r0.iter().map(|r| r + ff.ln()).collect(), db }
}

/// An open impact lowpass, as the morph blends it.
const OPEN_HZ: f64 = 20000.0;

/// The indices of the sides for which `has` holds, and their weights.
fn having(w: &[f64], has: impl Fn(usize) -> bool) -> (Vec<usize>, Vec<f64>) {
    (0..w.len()).filter(|&i| has(i)).map(|i| (i, w[i])).unzip()
}

/// `f` over the sides `idx`.
fn over<T>(idx: &[usize], f: impl Fn(usize) -> T) -> Vec<T> {
    idx.iter().map(|&i| f(i)).collect()
}

fn hammer_of(n: &RawNote) -> Option<&RawHammer> {
    match (&n.excitation, &n.hammer) {
        (Some(RawExcitation::Hammer(h)), _) | (None, Some(h)) => Some(h),
        _ => None,
    }
}

/// Every side's noise impacts (t0 s, level dB, decay s), each side's 10·log₁₀ of its share of the
/// weights `w` quieter, sorted by time: a side fades out continuously. (The burst sums the impacts'
/// envelopes in amplitude over one noise and is normalized at its peak: a side's share comes out
/// about √w in amplitude, not w in power.)
fn impacts(sets: &[&[[f64; 3]]], w: &[f64]) -> Vec<[f64; 3]> {
    let sum: f64 = w.iter().sum();
    let mut out: Vec<[f64; 3]> =
        sets.iter().zip(w).flat_map(|(s, w)| s.iter().map(move |i| [i[0], i[1] + 10.0 * (w / sum).log10(), i[2]])).collect();
    out.sort_by(|a, b| a[0].total_cmp(&b[0]));
    out
}

/// Per entry: `arith` where every row has the heaviest's length, else the heaviest's row.
fn rows_arith(rows: &[&[f64]], w: &[f64], h: usize) -> Vec<f64> {
    if rows.iter().all(|r| r.len() == rows[h].len()) {
        (0..rows[h].len()).map(|j| arith(&over(&(0..rows.len()).collect::<Vec<_>>(), |i| rows[i][j]), w)).collect()
    } else {
        rows[h].to_vec()
    }
}

/// A curve (frequency Hz, value) at `f`: interpolated in ln f and ln value, flat beyond its ends.
fn log_interp(curve: &[[f64; 2]], f: f64) -> f64 {
    let (first, last) = (curve[0], curve[curve.len() - 1]);
    if f <= first[0] {
        return first[1];
    }
    if f >= last[0] {
        return last[1];
    }
    let j = curve.iter().position(|p| p[0] >= f).unwrap_or(curve.len() - 1);
    let (a, b) = (curve[j - 1], curve[j]);
    let x = (f.ln() - a[0].ln()) / (b[0].ln() - a[0].ln());
    (a[1].ln() + x * (b[1].ln() - a[1].ln())).exp()
}

/// The bridges' mobility morphed: at every side's anchors, the weighted geometric mean of each side's
/// factor there (a side without anchors counts 1); None when no side has anchors.
fn morph_mobility(sides: &[Option<&RawMobility>], w: &[f64]) -> Option<RawMobility> {
    let fc_hz = sides.iter().flatten().next()?.fc_hz;
    let mut anchors: Vec<f64> = sides.iter().flatten().flat_map(|m| m.anchors.iter().copied()).collect();
    anchors.sort_by(f64::total_cmp);
    anchors.dedup();
    let at = |m: Option<&RawMobility>, note: f64| m.map_or(1.0, |m| mobility_of(m).factor(note, 0.0));
    let factors = anchors.iter().map(|&a| scale(&sides.iter().map(|m| at(*m, a)).collect::<Vec<_>>(), w)).collect();
    Some(RawMobility { anchors, factors, fc_hz })
}

/// One soundboard mode: frequency, damping, shape at the bridge, radiation to the two channels
/// (vertical, horizontal polarization).
#[derive(Clone, Copy, Debug)]
struct Mode {
    f: f64,
    sigma: f64,
    phi: f64,
    r: [f64; 2],
    r_h: [f64; 2],
}

/// A decoded instrument's board modes, sorted by frequency.
fn board_modes(d: &Decoded) -> Result<Vec<Mode>, Error> {
    let b = &d.raw.board;
    let (f, sigma, phi, r, r_h) = (d.data.f64(&b.f)?, d.data.f64(&b.sigma)?, d.data.f64(&b.phi)?, d.data.f64(&b.r)?, d.data.f64(&b.r_h)?);
    let n = f.len();
    if sigma.len() != n || phi.len() != n || r.len() != 2 * n || r_h.len() != 2 * n {
        return Err(err(format!("{}: the board's arrays differ in length", d.name)));
    }
    let mut m: Vec<Mode> =
        (0..n).map(|i| Mode { f: f[i], sigma: sigma[i], phi: phi[i], r: [r[i], r[n + i]], r_h: [r_h[i], r_h[n + i]] }).collect();
    m.sort_by(|a, b| a.f.total_cmp(&b.f));
    Ok(m)
}

/// The boards morphed: each mode of the smallest board paired with the mode at the same relative
/// rank in each larger one (frequency and damping geometric; shape and radiation the magnitudes'
/// geometric mean with the heaviest board's sign), φ × `fade`; the larger boards' other modes
/// morphed the same way among themselves, faded by their share of the weights. Sorted by frequency.
fn morph_board(boards: &[&[Mode]], w: &[f64], fade: f64) -> Vec<Mode> {
    let n = boards.iter().map(|b| b.len()).min().unwrap_or(0);
    let h = heaviest(w);
    let pick = |m: usize, i: usize| if n <= 1 { 0 } else { ((i * (m - 1)) as f64 / (n - 1) as f64).round() as usize };
    let mut out = Vec::new();
    for i in 0..n {
        let ms: Vec<&Mode> = boards.iter().map(|b| &b[pick(b.len(), i)]).collect();
        let signed = |x: &dyn Fn(&Mode) -> f64| {
            let v: Vec<f64> = ms.iter().map(|m| x(m)).collect();
            if v.iter().any(|x| *x == 0.0) {
                0.0
            } else {
                scale(&v.iter().map(|x| x.abs()).collect::<Vec<_>>(), w).copysign(v[h])
            }
        };
        out.push(Mode {
            f: scale(&ms.iter().map(|m| m.f).collect::<Vec<_>>(), w),
            sigma: scale(&ms.iter().map(|m| m.sigma).collect::<Vec<_>>(), w),
            phi: signed(&|m| m.phi) * fade,
            r: [signed(&|m| m.r[0]), signed(&|m| m.r[1])],
            r_h: [signed(&|m| m.r_h[0]), signed(&|m| m.r_h[1])],
        });
    }
    let (mut rest, mut rest_w) = (Vec::new(), Vec::new());
    for (b, wi) in boards.iter().zip(w) {
        if b.len() > n {
            let mut taken = vec![false; b.len()];
            (0..n).for_each(|i| taken[pick(b.len(), i)] = true);
            rest.push(b.iter().zip(&taken).filter(|(_, t)| !**t).map(|(m, _)| *m).collect::<Vec<Mode>>());
            rest_w.push(*wi);
        }
    }
    if !rest.is_empty() {
        let share = rest_w.iter().sum::<f64>() / w.iter().sum::<f64>();
        out.extend(morph_board(&rest.iter().map(Vec::as_slice).collect::<Vec<_>>(), &rest_w, fade * share));
    }
    out.sort_by(|a, b| a.f.total_cmp(&b.f));
    out
}

/// Measured responses mixed: each at unit energy, weighted, summed, then at the energies'
/// geometric mean (the real detail of each kept).
fn mix_responses(irs: &[&[f32]], w: &[f64]) -> Vec<f32> {
    let len = irs.iter().map(|r| r.len()).max().unwrap_or(0);
    let energy = |r: &[f32]| r.iter().map(|x| (*x as f64).powi(2)).sum::<f64>();
    let e: Vec<f64> = irs.iter().map(|r| energy(r)).collect();
    let mut y = vec![0.0f64; len];
    for ((r, e), w) in irs.iter().zip(&e).zip(w) {
        if *e > 0.0 {
            let g = w / e.sqrt();
            y.iter_mut().zip(r.iter()).for_each(|(y, x)| *y += g * *x as f64);
        }
    }
    let got: f64 = y.iter().map(|x| x * x).sum();
    let g = if got > 0.0 { (scale(&e, w) / got).sqrt() } else { 0.0 };
    y.iter().map(|x| (x * g) as f32).collect()
}

/// The morph of `sides` (checked, weights > 0 summing to 1); its body (the microphones' geometry) from
/// `main`, the instrument chosen first (its weight may be 0).
pub(super) fn morph(sides: Vec<(Decoded, f64)>, main: &Decoded) -> Result<Decoded, Error> {
    let w: Vec<f64> = sides.iter().map(|s| s.1).collect();
    let h = heaviest(&w);
    if sides.iter().any(|(d, _)| d.raw.reference_sr != sides[h].0.raw.reference_sr) {
        return Err(err("morph: the instruments' reference rates differ"));
    }
    let raws: Vec<&RawInstrument> = sides.iter().map(|s| &s.0.raw).collect();
    let datas: Vec<&Data> = sides.iter().map(|s| &s.0.data).collect();
    let all: Vec<usize> = (0..sides.len()).collect();
    let mut out = raws[h].clone();
    out.body = main.raw.body.clone();
    let mut data = Data(Vec::new());
    out.profile = format!("morph:{}", sides.iter().map(|(d, w)| format!("{}={w:.4}", d.path)).collect::<Vec<_>>().join(","));
    out.menu = None;
    let sc = |f: &dyn Fn(usize) -> f64| scale(&over(&all, f), &w);
    let ar = |f: &dyn Fn(usize) -> f64| arith(&over(&all, f), &w);

    // per key: the strings, the hammers, the dampers, the noises
    for (k, n) in out.notes.iter_mut().enumerate() {
        let ns: Vec<&RawNote> = raws.iter().map(|r| &r.notes[k]).collect();
        let st = &mut n.string;
        st.f0 = sc(&|i| ns[i].string.f0);
        st.b = sc(&|i| ns[i].string.b);
        st.l = sc(&|i| ns[i].string.l);
        st.t = sc(&|i| ns[i].string.t);
        st.mass = sc(&|i| ns[i].string.mass);
        st.b1 = sc(&|i| ns[i].string.b1);
        st.b3 = sc(&|i| ns[i].string.b3);
        st.loss_q = ar(&|i| ns[i].string.loss_q);
        st.kappa = sc(&|i| ns[i].string.kappa);
        st.horiz_gain = sc(&|i| ns[i].string.horiz_gain);
        st.horiz_kappa = sc(&|i| ns[i].string.horiz_kappa);
        st.f_max = sc(&|i| ns[i].string.f_max);
        st.scatter_cents = sc(&|i| ns[i].string.scatter_cents);
        st.kappa_reactive = ar(&|i| ns[i].string.kappa_reactive);
        st.horiz_detune_cents = ar(&|i| ns[i].string.horiz_detune_cents);
        // the string count from the heaviest; the detunings over the sides with that count
        let (same, ws) = having(&w, |i| ns[i].string.s == st.s && ns[i].string.detune_cents.len() == st.s);
        st.detune_cents = (0..st.s).map(|j| arith(&over(&same, |i| ns[i].string.detune_cents[j]), &ws)).collect();
        n.recorded_cents = ar(&|i| ns[i].recorded_cents);
        n.sustain = sc(&|i| ns[i].sustain);

        let hs: Vec<&RawHammer> = ns.iter().map(|n| hammer_of(n)).collect::<Option<_>>().ok_or_else(|| err("morph: a note without a hammer"))?;
        let balance = rows_arith(&over(&all, |i| hs[i].balance.as_slice()), &w, h);
        // the felt's core: its stiffness arithmetic (a side without one counts 0), its depth
        // weighted by each side's stiffness
        let core_k = ar(&|i| hs[i].core_k);
        let core_depth = if core_k > 0.0 { ar(&|i| hs[i].core_k * hs[i].core_depth) / core_k } else { 0.0 };
        n.excitation = Some(RawExcitation::Hammer(RawHammer {
            mass: sc(&|i| hs[i].mass),
            felt_k: sc(&|i| hs[i].felt_k),
            felt_p: ar(&|i| hs[i].felt_p),
            balance,
            strike_pos: ar(&|i| hs[i].strike_pos),
            core_k,
            core_depth,
        }));
        n.hammer = None;

        // the damper's table over the sides with one of the heaviest's length
        n.damper_sigma = match (n.has_damper, &ns[h].damper_sigma) {
            (true, Some(r0)) => {
                let (idx, wd) = having(&w, |i| ns[i].damper_sigma.as_ref().is_some_and(|r| r.n == r0.n));
                let tables = idx.iter().map(|&i| datas[i].f64(ns[i].damper_sigma.as_ref().unwrap())).collect::<Result<Vec<_>, _>>()?;
                let t: Vec<f64> = (0..r0.n).map(|j| scale(&over(&(0..tables.len()).collect::<Vec<_>>(), |t| tables[t][j]), &wd)).collect();
                Some(data.push_f64(&t))
            }
            _ => None,
        };
        n.knock = ns[h].knock.as_ref().map(|_| {
            let (idx, wk) = having(&w, |i| ns[i].knock.is_some());
            RawKnock { knee: scale(&over(&idx, |i| ns[i].knock.as_ref().unwrap().knee), &wk) }
        });
        let rel = &mut n.release;
        rel.impacts = impacts(&over(&all, |i| ns[i].release.impacts.as_slice()), &w);
        rel.band_db = rows_arith(&over(&all, |i| ns[i].release.band_db.as_slice()), &w, h);
        rel.peak = std::array::from_fn(|j| sc(&|i| ns[i].release.peak[j]));
        // (ln speed, dB) per entry, both blended: each side's speeds are ln of its own anchors, so
        // the blended column is ln of the morph's (geometric-mean) anchors
        if all.iter().all(|&i| ns[i].release.ratio_db.len() == ns[h].release.ratio_db.len()) {
            for (j, p) in rel.ratio_db.iter_mut().enumerate() {
                *p = std::array::from_fn(|c| ar(&|i| ns[i].release.ratio_db[j][c]));
            }
        }
        // the level corrections: a side without them counts 0 dB
        let len = ns.iter().map(|n| n.level_db.len()).max().unwrap_or(0);
        n.level_db = (0..len).map(|j| std::array::from_fn(|e| ar(&|i| ns[i].level_db.get(j).map_or(0.0, |t| t[e])))).collect();
        // the impact's trim (dB) over the sides with an impact, as its gain
        let (idx, wi) = having(&w, |i| raws[i].impact.is_some());
        n.impact_db = if idx.is_empty() { 0.0 } else { arith(&over(&idx, |i| ns[i].impact_db), &wi) };
    }

    // the instrument's numbers
    out.velocity.speed = std::array::from_fn(|j| sc(&|i| raws[i].velocity.speed[j]));
    out.contact.max_s = sc(&|i| raws[i].contact.max_s);
    out.contact.separation_s = sc(&|i| raws[i].contact.separation_s);
    out.output_gain_db = ar(&|i| raws[i].output_gain_db);
    out.sympathetic.transfer = sc(&|i| raws[i].sympathetic.transfer);
    out.sympathetic.f_max = sc(&|i| raws[i].sympathetic.f_max);
    out.release.length_s = sc(&|i| raws[i].release.length_s);
    out.release.rise_s = sc(&|i| raws[i].release.rise_s);
    if let Some(k) = out.knock.as_mut() {
        let (idx, wk) = having(&w, |i| raws[i].knock.is_some());
        let of = |i: usize| raws[i].knock.as_ref().unwrap();
        k.amp = scale(&over(&idx, |i| of(i).amp), &wk);
        k.q = scale(&over(&idx, |i| of(i).q), &wk);
        k.decay_s = scale(&over(&idx, |i| of(i).decay_s), &wk);
        k.cutoff_hz = scale(&over(&idx, |i| of(i).cutoff_hz), &wk);
        k.rise_s = scale(&over(&idx, |i| of(i).rise_s), &wk);
    }
    if let Some(m) = out.impact.as_mut() {
        // an open lowpass blends as 20 kHz; a blend from 16 kHz up is open again
        let (idx, wm) = having(&w, |i| raws[i].impact.is_some());
        let of = |i: usize| raws[i].impact.unwrap();
        m.gain = scale(&over(&idx, |i| of(i).gain), &wm);
        m.growth = arith(&over(&idx, |i| of(i).growth), &wm);
        let fc = scale(&over(&idx, |i| of(i).cutoff_hz.unwrap_or(OPEN_HZ)), &wm);
        m.cutoff_hz = (fc < 0.8 * OPEN_HZ).then_some(fc);
    }
    if let Some(p) = out.pedal.as_mut() {
        let (idx, wp) = having(&w, |i| raws[i].pedal.is_some());
        let of = |i: usize| raws[i].pedal.as_ref().unwrap();
        p.kick.amp = scale(&over(&idx, |i| of(i).kick.amp), &wp);
        p.kick.center = scale(&over(&idx, |i| of(i).kick.center), &wp);
        p.kick.spread = scale(&over(&idx, |i| of(i).kick.spread), &wp);
        p.kick.rolloff = scale(&over(&idx, |i| of(i).kick.rolloff), &wp);
        p.length_s = scale(&over(&idx, |i| of(i).length_s), &wp);
        for (down, takes) in [(true, &mut p.down), (false, &mut p.up)] {
            let take = |i: usize, j: usize| if down { of(i).down.get(j) } else { of(i).up.get(j) };
            for (j, t) in takes.iter_mut().enumerate() {
                let (ti, wt): (Vec<usize>, Vec<f64>) = idx.iter().zip(&wp).filter(|(i, _)| take(**i, j).is_some()).map(|(i, w)| (*i, *w)).unzip();
                let th = |i: usize| take(i, j).unwrap();
                t.impacts = impacts(&over(&ti, |i| th(i).impacts.as_slice()), &wt);
                let hh = heaviest(&wt);
                t.band_db = rows_arith(&over(&ti, |i| th(i).band_db.as_slice()), &wt, hh);
                t.ratio_db = arith(&over(&ti, |i| th(i).ratio_db), &wt);
            }
        }
    }
    // the per-note transfer correction: per point where every side has the same grid
    if let Some(c) = out.correction.as_mut() {
        let same = raws.iter().all(|r| r.correction.as_ref().is_some_and(|o| o.notes == c.notes && o.grid_hz == c.grid_hz && o.c_db.n == c.c_db.n));
        let rows = if same {
            let t = all.iter().map(|&i| datas[i].f64(&raws[i].correction.as_ref().unwrap().c_db)).collect::<Result<Vec<_>, _>>()?;
            (0..c.c_db.n).map(|j| arith(&over(&all, |i| t[i][j]), &w)).collect()
        } else {
            datas[h].f64(&c.c_db)?
        };
        c.c_db = data.push_f64(&rows);
    }
    // the room: the reverberation on every side's frequencies, its curve per point
    let room = &mut out.radiation.room;
    let mut grid: Vec<f64> = raws.iter().flat_map(|r| r.radiation.room.t60.iter().map(|p| p[0])).collect();
    grid.sort_by(f64::total_cmp);
    grid.dedup_by(|b, a| *b / *a - 1.0 < 1e-3);
    room.t60 = grid.iter().map(|&f| [f, sc(&|i| log_interp(&raws[i].radiation.room.t60, f))]).collect();
    if raws.iter().all(|r| r.radiation.room.grid_hz == room.grid_hz) {
        room.h_db = rows_arith(&over(&all, |i| raws[i].radiation.room.h_db.as_slice()), &w, h);
    }
    // its own level on every side's frequencies: a side without one 0 dB
    let union = |pts: &mut dyn Iterator<Item = f64>| {
        let mut g: Vec<f64> = pts.collect();
        g.sort_by(f64::total_cmp);
        g.dedup_by(|b, a| *b / *a - 1.0 < 1e-3);
        g
    };
    let lin = |c: &[[f64; 2]], f: f64| crate::noise::interp(f.ln(), &c.iter().map(|p| p[0].ln()).collect::<Vec<_>>(), &c.iter().map(|p| p[1]).collect::<Vec<_>>());
    let grid = union(&mut raws.iter().flat_map(|r| r.radiation.room.eq_db.iter().map(|p| p[0])));
    let eq = |i: usize, f: f64| if raws[i].radiation.room.eq_db.is_empty() { 0.0 } else { lin(&raws[i].radiation.room.eq_db, f) };
    room.eq_db = grid.iter().map(|&f| [f, ar(&|i| eq(i, f))]).collect();
    room.gain = sc(&|i| raws[i].radiation.room.gain);
    room.length_s = sc(&|i| raws[i].radiation.room.length_s);

    // the board, morphed mode by mode
    let boards = sides.iter().map(|(d, _)| board_modes(d)).collect::<Result<Vec<_>, _>>()?;
    let modes = morph_board(&boards.iter().map(Vec::as_slice).collect::<Vec<_>>(), &w, 1.0);
    let col = |f: &dyn Fn(&Mode) -> f64| modes.iter().map(f).collect::<Vec<f64>>();
    let b = &mut out.board;
    b.f = data.push_f64(&col(&|m| m.f));
    b.sigma = data.push_f64(&col(&|m| m.sigma));
    b.phi = data.push_f64(&col(&|m| m.phi));
    b.r = data.push_f64(&[col(&|m| m.r[0]), col(&|m| m.r[1])].concat());
    b.r_h = data.push_f64(&[col(&|m| m.r_h[0]), col(&|m| m.r_h[1])].concat());
    b.density = ar(&|i| raws[i].board.density);
    b.h_share = ar(&|i| raws[i].board.h_share);
    b.measured_from = None;
    out.phantom = ar(&|i| raws[i].phantom);
    b.mobility = morph_mobility(&raws.iter().map(|r| r.board.mobility.as_ref()).collect::<Vec<_>>(), &w);
    // the measured responses, mixed per channel
    for v in [true, false] {
        fn body_of(r: &RawInstrument, v: bool) -> &RawRadiation {
            if v { &r.radiation.v.body } else { &r.radiation.h.body }
        }
        let part = |r| body_of(r, v);
        let channels = part(raws[h]).channels;
        let irs = all.iter().map(|&i| datas[i].f32(&part(raws[i]).ir)).collect::<Result<Vec<_>, _>>()?;
        if all.iter().any(|&i| part(raws[i]).channels != channels) {
            return Err(err("morph: the measured responses differ in channels"));
        }
        let mixed: Vec<Vec<f32>> = (0..channels)
            .map(|c| {
                let chans: Vec<&[f32]> = all.iter().map(|&i| { let l = part(raws[i]).len; &irs[i][c * l..(c + 1) * l] }).collect();
                mix_responses(&chans, &w)
            })
            .collect();
        let len = mixed.iter().map(Vec::len).max().unwrap_or(0);
        let body = if v { &mut out.radiation.v.body } else { &mut out.radiation.h.body };
        body.len = len;
        body.ir = data.push_f32(&mixed.concat());
    }
    // the content: the sides' fingerprints at their weights
    let content = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for ((d, _), wi) in sides.iter().zip(&w) {
            d.content.hash(&mut h);
            wi.to_bits().hash(&mut h);
        }
        h.finish()
    };
    Ok(Decoded { raw: out, data, name: main.name.clone(), path: main.path.clone(), content })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
    const E: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_d");

    /// the decoded instrument (None: no export)
    fn decoded(dir: &str) -> Option<Decoded> {
        decode(Path::new(dir)).ok()
    }

    #[test]
    fn morph_blends_the_loss_exponent() {
        let (Some(mut a), Some(mut e)) = (decoded(A), decoded(E)) else { return };
        a.raw.notes[15].string.loss_q = 1.5;
        e.raw.notes[15].string.loss_q = 2.0;
        let m = morph(vec![(a.clone(), 0.5), (e.clone(), 0.5)], &a).unwrap();
        assert!((m.raw.notes[15].string.loss_q - 1.75).abs() < 1e-12);
    }

    #[test]
    fn morph_blends_the_core() {
        // the felt's core: its stiffness arithmetic (none counts 0), its depth from the cored sides
        let (Some(mut a), Some(mut e)) = (decoded(A), decoded(E)) else { return };
        let core = |d: &Decoded| hammer_of(&d.raw.notes[15]).map(|h| (h.core_k, h.core_depth));
        fn set(d: &mut Decoded, k: f64, depth: f64) {
            let n = &mut d.raw.notes[15];
            if let Some(h) = n.hammer.as_mut() {
                (h.core_k, h.core_depth) = (k, depth);
            }
            if let Some(RawExcitation::Hammer(h)) = n.excitation.as_mut() {
                (h.core_k, h.core_depth) = (k, depth);
            }
        }
        set(&mut a, 0.0, 0.0);
        set(&mut e, 0.0, 0.0);
        let m = morph(vec![(a.clone(), 0.5), (e.clone(), 0.5)], &a).unwrap();
        assert_eq!(core(&m), Some((0.0, 0.0)));
        set(&mut a, 6e8, 4e-4);
        let m = morph(vec![(a.clone(), 0.5), (e.clone(), 0.5)], &a).unwrap();
        let (k, depth) = core(&m).unwrap();
        assert!((k - 3e8).abs() < 1e-3 && (depth - 4e-4).abs() < 1e-15, "{k} {depth}");
    }

    #[test]
    fn morph_blends_the_phantom() {
        let (Some(mut a), Some(e)) = (decoded(A), decoded(E)) else { return };
        a.raw.phantom = 0.2;
        let m = morph(vec![(a.clone(), 0.5), (e.clone(), 0.5)], &a).unwrap();
        assert!((m.raw.phantom - 0.1).abs() < 1e-12);
    }

    #[test]
    fn morph_blends_the_mobility() {
        // a side without anchors counts 1: the geometric mean of 4 and 1 at equal weights is 2
        let a = RawMobility { anchors: vec![24.0, 36.0], factors: vec![1.0, 4.0], fc_hz: 500.0 };
        let half = morph_mobility(&[Some(&a), None], &[0.5, 0.5]).unwrap();
        let at = |m: &RawMobility, note: f64| mobility_of(m).factor(note, 0.0);
        assert!((at(&half, 36.0) - 2.0).abs() < 1e-9);
        assert!((at(&half, 24.0) - 1.0).abs() < 1e-9);
        assert!((at(&morph_mobility(&[Some(&a), None], &[1.0, 0.0]).unwrap(), 36.0) - 4.0).abs() < 1e-9);
        assert!((at(&morph_mobility(&[Some(&a), None], &[0.0, 1.0]).unwrap(), 36.0) - 1.0).abs() < 1e-9);
        assert!(morph_mobility(&[None, None], &[0.5, 0.5]).is_none());
        // the morph carries it into the board
        let (Some(mut ga), Some(ge)) = (decoded(A), decoded(E)) else { return };
        ga.raw.board.mobility = Some(a.clone());
        let m = morph(vec![(ga.clone(), 0.5), (ge.clone(), 0.5)], &ga).unwrap();
        assert!((at(m.raw.board.mobility.as_ref().unwrap(), 36.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_side_without_the_88_keys_is_refused() {
        // the morph pairs the notes by position: a side with a key missing or out of order cannot morph
        let Some(a) = decoded(A) else { return };
        assert!(refusal(&a.raw).is_none());
        let mut short = a.clone();
        short.raw.notes.pop();
        assert!(refusal(&short.raw).is_some());
        let mut shuffled = a.clone();
        shuffled.raw.notes.swap(10, 11);
        assert!(refusal(&shuffled.raw).is_some());
    }

    #[test]
    fn the_room_takes_every_side_s_frequencies() {
        // Grand A's and Grand D's t60 grids differ: the union has every frequency of both
        let (Some(a), Some(e)) = (decoded(A), decoded(E)) else { return };
        let m = morph(vec![(a.clone(), 0.5), (e.clone(), 0.5)], &a).unwrap();
        let f: Vec<f64> = m.raw.radiation.room.t60.iter().map(|p| p[0]).collect();
        for p in a.raw.radiation.room.t60.iter().chain(&e.raw.radiation.room.t60) {
            assert!(f.iter().any(|x| (x / p[0] - 1.0).abs() < 1e-3), "{} missing", p[0]);
        }
    }

    #[test]
    fn the_room_s_level_morphs() {
        let (Some(mut a), Some(mut e)) = (decoded(A), decoded(E)) else { return };
        a.raw.radiation.room.eq_db = vec![[200.0, 6.0], [2000.0, -6.0]];
        e.raw.radiation.room.eq_db.clear();
        let m = morph(vec![(a.clone(), 0.5), (e.clone(), 0.5)], &a).unwrap();
        // on every side's frequencies, a side without one 0 dB
        let q = m.raw.radiation.room.eq_db.iter().find(|p| (p[0] / 2000.0 - 1.0).abs() < 1e-3).expect("a point");
        assert!((q[1] + 3.0).abs() < 1e-9, "{q:?}");
    }

    #[test]
    fn a_side_without_a_correction_or_pedal_still_morphs() {
        let (Some(a), Some(mut e)) = (decoded(A), decoded(E)) else { return };
        e.raw.correction = None;
        e.raw.pedal = None;
        e.raw.impact = None;
        let m = morph(vec![(a.clone(), 0.6), (e.clone(), 0.4)], &a).unwrap(); // A heaviest: A's blocks
        assert!(m.raw.correction.is_some() && m.raw.pedal.is_some() && m.raw.impact.is_some());
        build(m).unwrap();
        let m = morph(vec![(a.clone(), 0.4), (e, 0.6)], &a).unwrap(); // E heaviest: none
        assert!(m.raw.correction.is_none() && m.raw.pedal.is_none() && m.raw.impact.is_none());
        build(m).unwrap();
    }

    #[test]
    fn the_release_ratio_follows_the_blended_speeds() {
        // each export's ln-speed column is its own (ln of its anchor speeds): both columns blend,
        // so the morph's column is ln of its geometric-mean speeds, and the ratios blend with it
        let (Some(a), Some(e)) = (decoded(A), decoded(E)) else { return };
        let m = morph(vec![(a.clone(), 0.5), (e.clone(), 0.5)], &a).unwrap();
        let k = 40;
        let (ra, re, rm) = (&a.raw.notes[k].release.ratio_db, &e.raw.notes[k].release.ratio_db, &m.raw.notes[k].release.ratio_db);
        assert!(ra.iter().zip(re).any(|(x, y)| x[0] != y[0]), "the columns differ somewhere");
        for j in 0..rm.len() {
            assert!((rm[j][0] - 0.5 * (ra[j][0] + re[j][0])).abs() < 1e-12, "column {j}");
            assert!((rm[j][1] - 0.5 * (ra[j][1] + re[j][1])).abs() < 1e-12, "ratio {j}");
        }
    }

    #[test]
    fn impacts_keep_every_side_quieter_by_its_weight() {
        let (Some(a), Some(e)) = (decoded(A), decoded(E)) else { return };
        let m = morph(vec![(a.clone(), 0.25), (e.clone(), 0.75)], &a).unwrap();
        let k = 40;
        let (na, ne) = (a.raw.notes[k].release.impacts.len(), e.raw.notes[k].release.impacts.len());
        assert_eq!(m.raw.notes[k].release.impacts.len(), na + ne);
        let top = |v: &[[f64; 3]]| v.iter().map(|i| i[1]).fold(f64::MIN, f64::max);
        let want = (top(&a.raw.notes[k].release.impacts) + 10.0 * 0.25f64.log10())
            .max(top(&e.raw.notes[k].release.impacts) + 10.0 * 0.75f64.log10());
        assert!((top(&m.raw.notes[k].release.impacts) - want).abs() < 1e-9);
    }

    #[test]
    fn boards_of_different_size_pair_by_relative_rank() {
        let mode = |f: f64| Mode { f, sigma: 1.0, phi: 1.0, r: [1.0; 2], r_h: [1.0; 2] };
        let big: Vec<Mode> = (1..=5).map(|i| mode(100.0 * i as f64)).collect(); // 100 … 500
        let small: Vec<Mode> = (1..=3).map(|i| mode(110.0 * i as f64)).collect(); // 110, 220, 330
        let m = morph_board(&[&big, &small], &[0.5, 0.5], 1.0);
        assert_eq!(m.len(), 5);
        // picked: big 0, 2, 4 with small 0, 1, 2; left: big 1, 3 at φ × 0.5
        let f: Vec<f64> = m.iter().map(|x| x.f).collect();
        let g = |a: f64, b: f64| (a * b).sqrt();
        let mut want = vec![g(100.0, 110.0), 200.0, g(300.0, 220.0), 400.0, g(500.0, 330.0)];
        want.sort_by(f64::total_cmp);
        assert!(f.iter().zip(&want).all(|(x, y)| (x - y).abs() < 1e-9), "{f:?}");
        assert!(m.iter().filter(|x| x.f == 200.0 || x.f == 400.0).all(|x| (x.phi - 0.5).abs() < 1e-12));
    }

    #[test]
    fn signs_come_from_the_heaviest_board() {
        let a = [Mode { f: 100.0, sigma: 1.0, phi: 2.0, r: [1.0, -1.0], r_h: [1.0; 2] }];
        let b = [Mode { f: 100.0, sigma: 1.0, phi: -8.0, r: [1.0, 1.0], r_h: [1.0; 2] }];
        let m = morph_board(&[&a, &b], &[0.4, 0.6], 1.0);
        assert!((m[0].phi + 8f64.powf(0.6) * 2f64.powf(0.4)).abs() < 1e-12); // b's sign
        assert!(m[0].r[1] > 0.0);
    }

    #[test]
    fn mixed_responses_keep_the_geometric_energy() {
        let a = [1.0f32, 0.0, 0.0];
        let b = [0.0f32, 2.0, 0.0];
        let y = mix_responses(&[&a, &b], &[0.5, 0.5]);
        let e: f64 = y.iter().map(|s| (*s as f64).powi(2)).sum();
        assert!((e - 2.0).abs() < 1e-6); // √(1·4)
    }

    #[test]
    fn weights_round_and_keep_their_places() {
        assert_eq!(weights(&[2.0, 6.0]).unwrap(), vec![0.25, 0.75]);
        assert_eq!(weights(&[1.0, 1e-5]).unwrap(), vec![1.0, 0.0]);
        assert_eq!(heaviest(&[0.5, 0.5]), 0);
        assert_eq!(heaviest(&[0.2, 0.8]), 1);
    }

    #[test]
    fn means_by_kind() {
        assert!((scale(&[1.0, 100.0], &[0.5, 0.5]) - 10.0).abs() < 1e-12);
        assert!((scale(&[-1.0, 3.0], &[0.5, 0.5]) - 1.0).abs() < 1e-12); // a sign: arithmetic
        assert!((arith(&[-1.0, 3.0], &[0.25, 0.75]) - 2.0).abs() < 1e-12);
    }
}
