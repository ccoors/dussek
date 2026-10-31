#!/usr/bin/env python3
"""Fit string loss and unison coupling to the full decay curves of the recordings.

Phase 1 calibrated loss and bridge coupling to a single number per partial (the energy
decay curve slope over its first 20 dB) and inflated the trichord coupling to match it
(Phase 1: trichord κ = 3·b1'), at the cost of a too fast aftersound. Here the whole energy decay
curve (EDC) of every partial is matched: its early part is the prompt sound, its late
part the aftersound, so both are constrained at once.

The model's free decay is a sum of exponentials per partial (string_model.free_modes),
so its EDC is analytic (vertical and horizontal polarization radiate by different paths
and add in power): with a(τ) = Σ_e w_e·exp(λ_e τ) per polarization,
    ∫_t^T |a|² dτ = Σ_e,f w_e·w̄_f·(exp(s T) − exp(s t)) / s,   s = λ_e + λ̄_f,
evaluated over exactly the time window of the measured EDC (onset + 50 ms until the
partial sinks into the noise). Measured EDC: STFT envelope energy minus the noise floor
energy, backward integrated (analyze.partial_envelopes, as in the Phase 0 analysis).

Per note, five parameters (Nelder–Mead, log space):
    β1, β3   internal loss b1 = β1·b1', b3 = β3·b3' (b1', b3' = measured profile loss)
    kf       bridge coupling κ = kf·b1'
    d        unison mistuning, scaling the design pattern (−0.6, 0, +0.7 cents)
    h        horizontal polarization excitation (horiz_gain)
Error: rms over partials and time of the EDC difference in dB (down to −40 dB).

Per-note fits lower the error but are not identifiable: the beating structure has many
local minima (mistuning ×50, κ ×2000). --global fits instead one smooth parameter set
for all notes: β1 at four anchor notes (log-interpolated), κ factor for bichords and
trichords and one horizontal excitation; the mistuning scale is measured from beat rates
([strings] detune).

Then (hammer pianos) the horizontal excitation's level along the keyboard (horiz@ anchors at C2,
C4 and C6, a factor on h): the EDCs weigh the prompt sound, and with one level the aftersound fell
2–15 dB under the recordings from C5 up (design log step 127). The second stage scores the EDC
error plus the late level's gap per octave (late_gap): partials 1–3's mean energy over LATE_WINDOW
re their peak over PEAK_WINDOW_S, in every recorded take, the median of an octave's notes (single
notes beat by several dB).

Usage: fit_coupling.py --profile grand_a [--notes 48,60,72] [--global]
"""
import argparse
import json
import os
import sys
from dataclasses import replace
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.optimize import minimize

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import fit_hammer as FH  # noqa: E402
import scale  # noqa: E402


EDC_FLOOR_DB = -40.0
MIN_RANGE_DB = 30.0
N_MAX = 12
PARAMS = ("beta1", "beta3", "kf", "detune", "horiz")


def measured_edcs(dataset, s, n_max=N_MAX):
    """Per usable partial: (n, freq, t (s from onset), EDC in dB, normalized at t[0])."""
    path = FH.D.sample_path(dataset, s)
    x, sr = sf.read(path, always_2d=True)
    # the file's cut as the analysis made it (the set's own or the detected fade-out)
    cut = s.get("tail_cut_s", FH.D.tail_cut(dataset))
    if cut:
        x = x[: len(x) - int(cut * sr)]
    parts = [p for p in s["partials"] if p["n"] <= n_max and p["range_db"] >= MIN_RANGE_DB]
    if not parts:
        return []
    t, env = A.partial_envelopes(x, sr, [p["freq"] for p in parts], s["f0"])
    env_db = A.db(env) - s["peak_dbfs"]
    t_on = s["onset_s"]
    out = []
    for p, e in zip(parts, env_db):
        post, tp = e[t >= t_on], t[t >= t_on] - t_on
        ipk = int(np.argmax(post[: max(len(post) // 4, 3)]))
        sm = np.convolve(post, np.ones(5) / 5, mode="same")
        below = np.nonzero(sm[ipk:] < p["floor_db"] + 6)[0]
        end = ipk + (below[0] if len(below) else len(sm) - ipk)
        start = ipk + int(0.05 / A.HOP_S)
        if end - start < 15:
            continue
        energy = np.maximum(10 ** (post[start:end] / 10) - 10 ** (p["floor_db"] / 10), 0)
        edc = np.cumsum(energy[::-1])[::-1]
        if edc[0] <= 0:
            continue
        edc_db = 10 * np.log10(np.maximum(edc / edc[0], 1e-12))
        sel = edc_db >= EDC_FLOOR_DB
        # envelope modulation (beating) in the aftersound: std of the envelope (dB)
        # around a quadratic trend, 0.3–5 s, where it is ≥ 10 dB above the noise floor
        mm = (tp > 0.3) & (tp < 5.0) & (post > p["floor_db"] + 10)
        depth, tm = None, None
        if mm.sum() >= 50:
            tm, ym = tp[mm], post[mm]
            depth = float(np.std(ym - np.polyval(np.polyfit(tm, ym, 2), tm)))
        # the late level from the onset on, the energy floor-corrected and kept past the EDC's end (6 dB
        # over the floor): cut there, the soft takes pulled their octaves' tails down
        e_all = np.maximum(10 ** (post / 10) - 10 ** (p["floor_db"] / 10), 0)
        out.append((p["n"], p["freq"], tp[start:end][sel], edc_db[sel], tp[end - 1] + A.HOP_S,
                    depth, tm, float(np.clip(8.0 / s["f0"], 0.04, 0.3)), early_excess(tp, post, p["floor_db"]),
                    late_level(tp, e_all)))
    return out


EARLY_T = 0.05             # s after the onset (the EDCs' start)
LATE_S = (0.5, 2.5)        # the aftersound's decay line, fitted over these seconds


def early_excess(t, e_db, floor_db=-np.inf):
    """dB the envelope lies above the aftersound's decay line (fitted over LATE_S, where ≥ 10
    dB above the floor; its beats average out) extrapolated back to EARLY_T, or None."""
    m = (t > LATE_S[0]) & (t < LATE_S[1]) & (e_db > floor_db + 10)
    if m.sum() < 20:
        return None
    return float(np.interp(EARLY_T, t, e_db) - np.polyval(np.polyfit(t[m], e_db[m], 1), EARLY_T))


PEAK_WINDOW_S = 0.3           # s after the onset
LATE_WINDOW = (1.0, 2.5)
LATE_FLOOR_DB = -80.0
LATE_PARTIALS = 3             # partials 1…3 carry the aftersound's level
# C7 up left out: the recordings' tails in their noise and the undamped strings' ring (C7, the
# fit's only note in its octave, drove its octave's level to the bound)
LATE_MAX_NOTE = 95


def late_level(t, energy):
    """dB of the mean energy over LATE_WINDOW re the peak over PEAK_WINDOW_S (t: s from the onset),
    at least LATE_FLOOR_DB; None if the envelope ends before the window does."""
    if t[-1] < LATE_WINDOW[1]:
        return None
    early = energy[t < PEAK_WINDOW_S].max()
    late = energy[(t >= LATE_WINDOW[0]) & (t < LATE_WINDOW[1])].mean()
    if late <= 0 or early <= 0:
        return LATE_FLOOR_DB
    return max(float(10 * np.log10(late / early)), LATE_FLOOR_DB)


def _energy(fm, i, t0, t1):
    """Closed-form ∫ |a|² dτ over [t0, t1] of partial row i (the polarizations add in power)."""
    e = 0.0
    for sl in (slice(0, fm["S"]), slice(fm["S"], None)):
        l, ww = fm["lam"][i, sl], fm["weight"][i, sl]
        s = l[:, None] + l.conj()[None, :]
        e += (ww[:, None] * ww.conj()[None, :] / s * (np.exp(s * t1) - np.exp(s * t0))).sum().real
    return e


def _peak(fm, i):
    """The peak of |a|² over PEAK_WINDOW_S (2 ms grid) of partial row i (polarizations in power)."""
    t = np.arange(0.0, PEAK_WINDOW_S, 0.002)
    e = np.zeros_like(t)
    for sl in (slice(0, fm["S"]), slice(fm["S"], None)):
        e += np.abs(np.exp(np.outer(t, fm["lam"][i, sl])) @ fm["weight"][i, sl]) ** 2
    return e.max()


def model_late(params, velocity, ns, fm=None):
    """late_level of the model's partials `ns` (None: no such partial)."""
    fm = fm or scale.M.free_modes(params, velocity)
    out = []
    for n in ns:
        i = np.nonzero(fm["k"] == n)[0]
        if len(i) == 0:
            out.append(None)
            continue
        early = _peak(fm, i[0])
        late = _energy(fm, i[0], *LATE_WINDOW) / (LATE_WINDOW[1] - LATE_WINDOW[0])
        out.append(LATE_FLOOR_DB if late <= 0 or early <= 0 else max(float(10 * np.log10(late / early)), LATE_FLOOR_DB))
    return out


def late_diff(params, velocity, takes, fm=None):
    """The note's late level, model − recording (dB): the median over its takes' partials
    1…LATE_PARTIALS with a late level (takes: per take [(n, dB or None), …]), or None."""
    rows = [(n, d) for take in takes for n, d in take if n <= LATE_PARTIALS and d is not None]
    if not rows:
        return None
    model = dict(zip(range(1, LATE_PARTIALS + 1), model_late(params, velocity, range(1, LATE_PARTIALS + 1), fm)))
    d = [model[n] - x for n, x in rows if model.get(n) is not None]
    return float(np.median(d)) if d else None


def late_gap(diffs):
    """The mean over octaves (from note 24) of |the median of their notes' late differences| (dB;
    {note: dB or None}) below LATE_MAX_NOTE."""
    by = {}
    for n, d in diffs.items():
        if d is not None and n <= LATE_MAX_NOTE:
            by.setdefault((n - 24) // 12, []).append(d)
    return float(np.mean([abs(np.median(v)) for v in by.values()])) if by else 0.0


def model_edcs(params, velocity, meas, fm=None):
    """Analytic EDC (dB, normalized at the first time) per measured partial (`fm`: the
    note's free_modes, if already computed)."""
    fm = fm or scale.M.free_modes(params, velocity)
    k, lam, w = fm["k"], fm["lam"], fm["weight"]
    out = []
    for n, _, t, _, T, *_ in meas:
        i = np.nonzero(k == n)[0]
        if len(i) == 0:
            out.append(None)
            continue
        e = 0
        for sl in (slice(0, fm["S"]), slice(fm["S"], None)):   # polarizations add in power
            l, ww = lam[i[0], sl], w[i[0], sl]
            s = l[:, None] + l.conj()[None, :]
            c = ww[:, None] * ww.conj()[None, :] / s
            e = e + (c[None] * (np.exp(s[None] * T) - np.exp(s[None] * t[:, None, None]))).sum(axis=(1, 2)).real
        e = np.maximum(e, 1e-30)
        out.append(10 * np.log10(e / e[0]))
    return out


def model_depths(params, velocity, meas, fm=None):
    """Envelope modulation depth (dB) per measured partial, from the analytic envelope,
    smoothed like the STFT (Hann window of the analysis window length)."""
    fm = fm or scale.M.free_modes(params, velocity)
    k, lam, w = fm["k"], fm["lam"], fm["weight"]
    dt = 0.005
    tg = np.arange(0, 5.3, dt)
    out = []
    for n, _, _, _, _, depth, tm, win_s, *_ in meas:
        i = np.nonzero(k == n)[0]
        if depth is None or len(i) == 0:
            out.append(None)
            continue
        comp = w[i[0]][:, None] * np.exp(lam[i[0]][:, None] * tg)
        # the polarizations radiate by different paths (the engine's v and h bodies, each of
        # unit energy) and meet at the microphone: they beat as their amplitudes say, at a
        # phase the paths set. Added in power (as for the EDCs, whose cross terms average
        # out), a single string's polarizations never beat: the harpsichords' and the harp's
        # model depth stayed at 0.01–0.5 dB against the recordings' 2.0–2.9
        pw = np.abs(comp.sum(axis=0)) ** 2
        hw = np.hanning(max(int(win_s / dt), 3))
        pw = np.convolve(pw, hw / hw.sum(), mode="same")
        y = 10 * np.log10(np.interp(tm, tg, pw) + 1e-30)
        out.append(float(np.std(y - np.polyval(np.polyfit(tm, y, 2), tm))))
    return out


def model_excess(params, velocity, meas, fm=None):
    """early_excess of the model's analytic envelope per measured partial (polarizations in
    power, smoothed like the STFT), or None where the recording has none."""
    fm = fm or scale.M.free_modes(params, velocity)
    k, lam, w = fm["k"], fm["lam"], fm["weight"]
    dt = 0.005
    tg = np.arange(0, LATE_S[1] + 0.3, dt)
    out = []
    for m in meas:
        n, win_s, ex = m[0], m[7], m[8]
        i = np.nonzero(k == n)[0]
        if ex is None or len(i) == 0:
            out.append(None)
            continue
        comp = w[i[0]][:, None] * np.exp(lam[i[0]][:, None] * tg)
        S = fm["S"]
        pw = np.abs(comp[:S].sum(axis=0)) ** 2 + np.abs(comp[S:].sum(axis=0)) ** 2
        hw = np.hanning(max(int(win_s / dt), 3))
        pw = np.convolve(pw, hw / hw.sum(), mode="same")
        out.append(early_excess(tg, 10 * np.log10(pw + 1e-30)))
    return out


def note_params(prof, note, x, g=None):
    """The note's parameters from the log vector x (global_to_note); with the global parameters g,
    the bridge's mobility is g's (its mobility@ anchors, none: the board's own admittance), in place
    of the profile's committed one."""
    beta1, beta3, kf, d, h, *rest = np.exp(x)
    base = scale.note_params(prof, note)
    if g is not None and prof.board is not None:
        base = replace(base, admittance=scale.MobileAdmittance(prof.board.admittance, g, note)
                       if scale.mobility_anchors(g) else prof.board.admittance)
    if g is not None:   # the loss law's exponent is g's (stage 3), as its β3 is (x)
        base = replace(base, loss_q=g.get("loss_q", 2.0))
    b1p, b3p = prof.loss(note)
    S = base.n_strings
    detune = {1: (0.0,), 2: (-0.5, 0.5), 3: (-0.6, 0.0, 0.7)}[S]
    extra = dict(zip(("kappa_reactive", "horiz_detune_cents", "balance"), rest))
    if "balance" in extra:   # scales the per-string deviation of the hammer felt stiffness
        bal = extra.pop("balance")
        extra["hammer_balance"] = tuple(1 + bal * (b - 1) for b in base.hammer_balance)
    # no string scatter: fine_structure.py fits it after this fit; the one the last run left
    # in the profile made each refit's result depend on the run before it
    if prof.polarizations(note) is not None:
        # loss, coupling, the horizontal excitation and detuning measured (fit_polarizations.py):
        # the fit moves the reactive part only
        extra.pop("horiz_detune_cents", None)
        return replace(base, detune_cents=tuple(d * c for c in detune), scatter_cents=0.0, **extra)
    return replace(base, b1=beta1 * b1p, b3=beta3 * b3p, kappa=kf * b1p,
                   detune_cents=tuple(d * c for c in detune), horiz_gain=h, scatter_cents=0.0, **extra)


W_MOD = 1.0   # weight of the modulation-depth term (dB) against the EDC error (dB)
# the early-excess term (dB, same weight): [strings] w_early (the harp's: its fundamentals start
# above their aftersound's line, see its project file)


def error(x, prof, note, velocity, meas, with_mod=True, fm=None, g=None):
    """rms EDC error (dB) + W_MOD·|median modulation depth, model − recording| (dB)
    [+ w_early·|median early excess, model − recording| (dB)]. EDCs are blind to beating
    (integration smooths it), so the depth term constrains how strongly the unison and
    polarization beats modulate the aftersound."""
    params = note_params(prof, note, x, g)
    fm = fm or scale.M.free_modes(params, velocity)    # once for the EDCs and the depths
    errs = []
    for m, me in zip(meas, model_edcs(params, velocity, meas, fm)):
        if me is not None:
            errs.append(np.mean((me - m[3]) ** 2))
    if not errs:
        return 1e9
    e = float(np.sqrt(np.mean(errs)))
    if with_mod:
        pairs = [(m[5], d) for m, d in zip(meas, model_depths(params, velocity, meas, fm)) if d is not None]
        if len(pairs) >= 3:
            e += W_MOD * abs(np.median([d for _, d in pairs]) - np.median([m for m, _ in pairs]))
    w_early = scale.strings(prof.dataset).get("w_early", 0.0)
    if w_early:
        pairs = [(m[8], d) for m, d in zip(meas, model_excess(params, velocity, meas, fm)) if d is not None]
        if pairs:
            e += w_early * abs(np.median([d for _, d in pairs]) - np.median([m for m, _ in pairs]))
    return e


def fit_note(args):
    dataset, note = args
    prof = scale.Profile(dataset)
    key = FH.D.dynamics(dataset)[FH.D.loudest(dataset)]
    s = next((s for s in prof.samples if s["note"] == note
              and FH.D.layer_key(dataset, s) == key), None)
    if s is None:
        return note, None
    meas = measured_edcs(dataset, s)
    if len(meas) < 3:
        return note, None
    v = prof.hammer_speeds()[FH.D.loudest(prof.dataset)]
    S = scale.n_strings(note, scale.design_of(dataset))
    x0 = np.log([0.3, 1.0, {1: 0.6, 2: 0.7, 3: 3.0}[S], 1.0, 0.045])     # Phase 1 values
    e0 = error(x0, prof, note, v, meas)
    best = None
    for start in (x0, x0 + np.log([0.3, 1, 0.3, 2, 2])):     # two starts: Weinreich has local minima
        r = minimize(error, start, args=(prof, note, v, meas), method="Nelder-Mead",
                     options={"maxfev": 600, "xatol": 0.02, "fatol": 0.01})
        if best is None or r.fun < best.fun:
            best = r
    return note, {"err_db": best.fun, "err0_db": e0, "n_partials": len(meas),
                  **dict(zip(PARAMS, np.exp(best.x).tolist()))}


# ---------------------------------------------------------------- global fit

ANCHORS = [24, 48, 72, 96]
GLOBAL = ("beta1@24", "beta1@48", "beta1@72", "beta1@96", "kf2", "kf3", "detune", "horiz",
          "reactive", "hdetune")
BOUNDS = {"detune": (0.3, 5.0), "horiz": (0.005, 0.5), "kf2": (0.05, 20), "kf3": (0.05, 20),
          "reactive": (0.01, 20.0), "hdetune": (0.02, 3.0)}
def hammered(dataset):
    """Struck by hammers (no quill, finger or tangent): the horizontal level per octave is fitted."""
    d = scale.design_of(dataset)
    return not (d in scale.PLUCKED or d in scale.TANGENT)


def _g_prof_has_polarizations(dataset):
    """Whether the profile carries measured polarizations (fit_polarizations.py)."""
    return FH.P.has_fit(dataset) and bool(FH.P.load_fit(dataset).get("polarizations"))


# The surface has minima of nearly equal error far apart (design log step 114): two Grand A takes
# left out moved the fit 0.07 dB lower into another one (reactive 0.99 → 2.70, 3–6 cents flatter,
# more ring). A refit starts from the committed fit too and keeps to its minimum unless another
# start is clearly better, and reports a parameter that moved twofold, for a listen.
CLEARLY_BETTER_DB = 0.1
JUMP = 2.0


def choose(results, committed=None):
    """The start to keep among `results` ((error dB, …) per start): the lowest, or the committed
    fit's start (index `committed`) unless the lowest beats it by more than CLEARLY_BETTER_DB."""
    best = min(range(len(results)), key=lambda i: results[i][0])
    if committed is None or results[best][0] < results[committed][0] - CLEARLY_BETTER_DB:
        return best
    return committed


def jumps(old, new):
    """The parameters (name, old, new) that moved by JUMP or more from `old` (the committed fit;
    None or {}: nothing to compare), in name order."""
    if not old:
        return []
    out = []
    for k in sorted(new):
        o, n = old.get(k), new[k]
        if isinstance(o, (int, float)) and o > 0 and n > 0 and (n / o >= JUMP or o / n >= JUMP):
            out.append((k, o, n))
    return out


# the fingered strings: the harp's polarizations beat at 5–7 cents (G3 0.6 Hz, D4 1.25 Hz)
FINGERED_BOUNDS = {"hdetune": (0.02, 10.0)}
_g = {}


def _init(dataset, cache, late=None, cache60=None):
    _g["prof"] = scale.Profile(dataset)
    _g["meas"] = cache
    _g["late"] = late or {}
    _g["meas60"] = cache60 or {}


def global_to_note(g, note, design="grand"):
    """Per-note parameter vector (log) from the global parameters (dict)."""
    b1 = np.exp(np.interp(note, ANCHORS, np.log([g[f"beta1@{a}"] for a in ANCHORS])))
    kf = g["kf2"] if scale.n_strings(note, design) <= 2 else g["kf3"]
    h = g["horiz"] * scale.horiz_factor(g, note)
    return np.log([b1, scale.beta3_factor(g, note), kf, g["detune"], h, g["reactive"], g["hdetune"]])


def _note_error(args):
    note, g = args
    prof = _g["prof"]
    return error(global_to_note(g, note, prof.design), prof, note, prof.hammer_speeds()[FH.D.loudest(prof.dataset)], _g["meas"][note],
                 g=g)


def _note_eval(args):
    """(EDC error, late difference) of a note under the global parameters g."""
    note, g = args
    prof, meas = _g["prof"], _g["meas"][note]
    x, v = global_to_note(g, note, prof.design), prof.hammer_speeds()[FH.D.loudest(prof.dataset)]
    params = note_params(prof, note, x, g)
    fm = scale.M.free_modes(params, v)
    return error(x, prof, note, v, meas, fm=fm, g=g), late_diff(params, v, _g["late"].get(note, []), fm)


def _note_eval3(args):
    """(EDC error, late difference, early difference) of a note under the global parameters g."""
    note, g = args
    prof, meas = _g["prof"], _g["meas"][note]
    x, v = global_to_note(g, note, prof.design), prof.hammer_speeds()[FH.D.loudest(prof.dataset)]
    params = note_params(prof, note, x, g)
    fm = scale.M.free_modes(params, v)
    return (error(x, prof, note, v, meas, fm=fm, g=g), late_diff(params, v, _g["late"].get(note, []), fm),
            early_diff(params, v, meas, fm))


# two octaves apart, held beyond: one per octave followed the octaves' medians up and down (Grand D
# ×0.3 at C2, ×5.0 at C3); none above C6: C7 is the fit's only note in its octave, its recordings'
# tails in their noise and the undamped strings' ring (its factor went to the bound)
HORIZ_ANCHORS = [36, 60, 84]
HORIZ_FACTOR_BOUNDS = (0.1, 10.0)
W_LATE = 1.0     # weight of the late gap (dB) against the EDC error (dB)


def fit_horiz_levels(pool, notes, g, old):
    """The second stage: the horizontal level's factor per anchor (log-interpolated), the
    other parameters held; error: mean EDC error + W_LATE·late_gap. Returns (factors, before, after)
    with before/after (EDC error, late gap) in dB."""
    names = [f"horiz@{a}" for a in HORIZ_ANCHORS]

    def parts(x):
        gg = {**g, **dict(zip(names, np.exp(x)))}
        res = pool.map(_note_eval, [(n, gg) for n in notes])
        return float(np.mean([e for e, _ in res])), late_gap(dict(zip(notes, [d for _, d in res])))

    def total(x):
        e, gap = parts(x)
        lo, hi = HORIZ_FACTOR_BOUNDS
        pen = sum(10 * max(np.log(lo) - v, 0, v - np.log(hi)) for v in x)
        return e + W_LATE * gap + pen

    x0 = np.zeros(len(names))
    before = parts(x0)
    # the committed factors first (a refit with small input changes stays near them)
    starts = [x0] + ([np.log([old[k] for k in names])] if all(isinstance(old.get(k), (int, float)) for k in names) else [])
    best = None
    for x in starts:
        # the first simplex 0.3 (×1.35) along each factor (scipy's own steps 0.00025 from a log factor of 0)
        simplex = np.vstack([x] + [x + 0.3 * np.eye(len(x))[i] for i in range(len(x))])
        r = minimize(total, x, method="Nelder-Mead", options={"maxfev": 800, "xatol": 0.01, "fatol": 0.005,
                                                              "adaptive": True, "initial_simplex": simplex})
        print(f"  horizontal levels from {'the committed' if x is not x0 else 'one level'} → {r.fun:.2f} dB")
        if best is None or r.fun < best.fun:
            best = r
    return dict(zip(names, np.exp(best.x).tolist())), before, parts(best.x)


# the strings' high-frequency loss (design 2026-10-20-bass-high-band): b3′ is measured from each
# partial's total decay, which holds what the board takes, and the model adds its coupling on top;
# the stages above see partials ≤ N_MAX only. In the bass the high partials died within half a second
# (C2's 4.5–6.5 kHz −16 dB at 0.3 s, the recording −6). β3 at the β1 anchors and the loss law's
# exponent q, fitted to the energy decay of every partial up to the 60th with the coupling in the model
LOSS_N_MAX = 60
LOSS_Q_BOUNDS = (1.0, 2.0)
BETA3_BOUNDS = (0.1, 3.0)


def fits_high_loss(dataset):
    """A hammer piano whose project asks for the high-frequency loss fit ([strings] high_loss = true)."""
    return hammered(dataset) and bool(scale.strings(dataset).get("high_loss", False))


def _note_err60(args):
    """A note's rms EDC error (dB) over its partials up to LOSS_N_MAX under g (no depth term)."""
    note, g = args
    prof = _g["prof"]
    v = prof.hammer_speeds()[FH.D.loudest(prof.dataset)]
    return error(global_to_note(g, note, prof.design), prof, note, v, _g["meas60"][note], with_mod=False, g=g)


def fit_high_loss(pool, notes, g, old, cache):
    """The loss stage: β3 at ANCHORS and the exponent q, the rest held; error: the mean over notes of
    the rms EDC error over partials 1…LOSS_N_MAX. `cache`: the measured rows up to LOSS_N_MAX per note,
    the caller's (the pool's workers hold their own copy in _g). Returns (factors, before, after) in
    dB; without partials above N_MAX in the data, the law as it was (β3 = 1, q = 2) and (None, None)."""
    names = [f"beta3@{a}" for a in ANCHORS] + ["loss_q"]
    if not any(m[0] > N_MAX for n in notes for m in cache.get(n, [])):
        return dict(zip(names, [1.0] * len(ANCHORS) + [2.0])), None, None
    notes = [n for n in notes if n in cache]

    def as_g(x):
        return {**g, **dict(zip(names[:-1], np.exp(x[:-1]))), "loss_q": float(x[-1])}

    def parts(x):
        return float(np.mean(pool.map(_note_err60, [(n, as_g(x)) for n in notes])))

    def total(x):
        lo, hi = np.log(BETA3_BOUNDS)
        pen = sum(10 * max(lo - v, 0, v - hi) for v in x[:-1])
        pen += 10 * max(LOSS_Q_BOUNDS[0] - x[-1], 0, x[-1] - LOSS_Q_BOUNDS[1])
        return parts(x) + pen

    x0 = np.array([0.0] * len(ANCHORS) + [2.0])
    starts, committed = [x0], None
    if all(isinstance(old.get(k), (int, float)) and old[k] > 0 for k in names):
        starts.append(np.array([np.log(old[k]) for k in names[:-1]] + [old["loss_q"]]))
        committed = 1
    results = []
    for x in starts:
        steps = np.diag([0.3] * len(ANCHORS) + [-0.15])
        simplex = np.vstack([x] + [x + steps[i] for i in range(len(x))])
        r = minimize(total, x, method="Nelder-Mead", options={"maxfev": 1200, "xatol": 0.01, "fatol": 0.005,
                                                              "adaptive": True, "initial_simplex": simplex})
        print(f"  high-frequency loss from {'the committed values' if len(results) == committed else 'the law as it was'} → {r.fun:.2f} dB")
        results.append((r.fun, r))
    best = results[choose(results, committed)][1]
    factors = {**dict(zip(names[:-1], np.exp(best.x[:-1]).tolist())), "loss_q": float(best.x[-1])}
    return factors, parts(x0), parts(best.x)


def _load60(args):
    """(note, its measured EDC rows up to LOSS_N_MAX)."""
    dataset, note = args
    prof = scale.Profile(dataset)
    key = FH.D.dynamics(dataset)[FH.D.loudest(dataset)]
    s = next((s for s in prof.samples if s["note"] == note and FH.D.layer_key(dataset, s) == key), None)
    return note, (measured_edcs(dataset, s, LOSS_N_MAX) if s is not None else [])


def main_high_loss(dataset, notes):
    """The loss stage alone, from the committed coupling: writes the block."""
    with Pool() as pool:
        cache = {n: m for n, m in pool.map(_load, [(dataset, n) for n in notes]) if len(m) >= 3}
        cache60 = dict(pool.map(_load60, [(dataset, n) for n in cache]))
    notes = sorted(cache)
    g = dict(FH.P.load_fit(dataset)["coupling"])
    with Pool(initializer=_init, initargs=(dataset, cache, None, cache60)) as pool:
        factors, before, after = fit_high_loss(pool, notes, g, g, cache60)
    g.update(factors)
    print(f"  {dataset}: loss stage, EDC error over partials 1–{LOSS_N_MAX} {before} → {after} dB; "
          + " ".join(f"{k} {v:.3f}" for k, v in factors.items()))
    FH.P.update_fit(dataset, {"coupling": g})
    print(f"wrote the coupling block of {FH.P.fit_path(dataset)}")


# the third stage: the bridge's mobility along the keyboard (design 2026-10-20-bridge-mobility). The
# recordings' partials 2–4 of C2–C3 fall 10–25 dB in their first 0.5 s (the prompt sound), the
# model's barely: one admittance for every key. A factor per octave C1 … C6 on the coupling below
# 500 Hz (scale.mobility_factor), fitted with the horizontal levels (the two trade at C2: fitted
# apart, each undid the other)
MOBILITY_ANCHORS = [24, 36, 48, 60, 72, 84]
EARLY_PARTIALS = 6
W_EARLY = 1.0    # weight of the early gap (dB) against the EDC error (dB)
# the smoothness penalty per neighboring anchors' log ratio beyond ln 2: one anchor per octave
# followed the octaves' medians up and down for the horizontal levels (step 127)
W_SMOOTH = 10.0


def early_diff(params, velocity, meas, fm=None):
    """The note's early excess, model − recording (dB): the median over its partials 1…EARLY_PARTIALS
    with one in the recording, or None."""
    pairs = [(m[8], d) for m, d in zip(meas, model_excess(params, velocity, meas, fm))
             if d is not None and m[0] <= EARLY_PARTIALS]
    return float(np.median([d - r for r, d in pairs])) if pairs else None


def smooth_penalty(factors):
    """W_SMOOTH · Σ max(|ln(a_{i+1} / a_i)| − ln 2, 0) over neighboring anchors."""
    lf = np.log(np.asarray(factors, float))
    return float(W_SMOOTH * np.maximum(np.abs(np.diff(lf)) - np.log(2), 0).sum())


def fits_mobility(dataset):
    """A hammer piano whose project asks for the bridge's mobility ([strings] mobility = true)."""
    return hammered(dataset) and bool(scale.strings(dataset).get("mobility", False))


def fit_mobility(pool, notes, g, old):
    """The third stage: the bridge's mobility at MOBILITY_ANCHORS and the horizontal levels at
    HORIZ_ANCHORS, the rest held; error: mean EDC error + W_LATE·late gap + W_EARLY·early gap (the
    notes' early differences per octave, as late_gap) + the bounds' and the smoothness penalty.
    Returns (factors, before, after), before/after (EDC error, late gap, early gap) in dB."""
    mob = [f"mobility@{a}" for a in MOBILITY_ANCHORS]
    names = mob + [f"horiz@{a}" for a in HORIZ_ANCHORS]

    def parts(x):
        gg = {**g, **dict(zip(names, np.exp(x)))}
        res = pool.map(_note_eval3, [(n, gg) for n in notes])
        return (float(np.mean([e for e, _, _ in res])), late_gap(dict(zip(notes, [d for _, d, _ in res]))),
                late_gap(dict(zip(notes, [d for _, _, d in res]))))

    def total(x):
        e, gl, ge = parts(x)
        pen = 0.0
        for v, (lo, hi) in zip(x, [scale.MOBILITY_BOUNDS] * len(mob) + [HORIZ_FACTOR_BOUNDS] * len(HORIZ_ANCHORS)):
            pen += 10 * max(np.log(lo) - v, 0, v - np.log(hi))
        return e + W_LATE * gl + W_EARLY * ge + pen + smooth_penalty(np.exp(x[:len(mob)]))

    x0 = np.log([1.0] * len(mob) + [g.get(f"horiz@{a}", 1.0) for a in HORIZ_ANCHORS])
    starts, committed = [x0], None
    if all(isinstance(old.get(k), (int, float)) and old[k] > 0 for k in names):
        starts.append(np.log([old[k] for k in names]))
        committed = 1
    results = []
    for x in starts:
        simplex = np.vstack([x] + [x + 0.3 * np.eye(len(x))[i] for i in range(len(x))])
        r = minimize(total, x, method="Nelder-Mead", options={"maxfev": 1500, "xatol": 0.01, "fatol": 0.005,
                                                              "adaptive": True, "initial_simplex": simplex})
        print(f"  mobility from {'the committed anchors' if len(results) == committed else 'one level'} → {r.fun:.2f} dB")
        results.append((r.fun, r))
    best = results[choose(results, committed)][1]
    return dict(zip(names, np.exp(best.x).tolist())), parts(x0), parts(best.x)


def _mobility_report(pool, notes, g, before, after):
    """The stage's terms before and after, and each note's early difference (dB), as a dict."""
    keys = ("edc_db", "late_gap_db", "early_gap_db")
    per = pool.map(_note_eval3, [(n, g) for n in notes])
    return {"before": dict(zip(keys, before)), "after": dict(zip(keys, after)),
            "early_diff_db": {n: d for n, (_, _, d) in zip(notes, per)}}


def _print_mobility(dataset, rep, g):
    b, a = rep["before"], rep["after"]
    print(f"  {dataset}: mobility stage, EDC error {b['edc_db']:.2f} → {a['edc_db']:.2f} dB, late gap "
          f"{b['late_gap_db']:.2f} → {a['late_gap_db']:.2f} dB, early gap {b['early_gap_db']:.2f} → {a['early_gap_db']:.2f} dB")
    print("  " + " ".join(f"mobility@{x} {g[f'mobility@{x}']:.2f}" for x in MOBILITY_ANCHORS))
    print("  early difference per note (model − recording, dB): "
          + " ".join(f"{n}:{d:+.1f}" for n, d in rep["early_diff_db"].items() if d is not None))


def main_mobility(dataset, notes):
    """The third stage alone, from the committed coupling (to tune W_SMOOTH): writes the block."""
    with Pool() as pool:
        cache = {n: m for n, m in pool.map(_load, [(dataset, n) for n in notes]) if len(m) >= 3}
        late = dict(pool.map(_load_late, [(dataset, n) for n in cache]))
    notes = sorted(cache)
    g = dict(FH.P.load_fit(dataset)["coupling"])
    with Pool(initializer=_init, initargs=(dataset, cache, late)) as pool:
        factors, before, after = fit_mobility(pool, notes, g, g)
        g.update(factors)
        rep = _mobility_report(pool, notes, g, before, after)
    _print_mobility(dataset, rep, g)
    FH.P.update_fit(dataset, {"coupling": g})
    print(f"wrote the coupling block of {FH.P.fit_path(dataset)}")


def _load_late(args):
    """(note, the late levels of its partials 1…LATE_PARTIALS in every recorded take)."""
    dataset, note = args
    prof = scale.Profile(dataset)
    keys = set(FH.D.dynamics(dataset).values())
    takes = [[(m[0], m[9]) for m in measured_edcs(dataset, s) if m[0] <= LATE_PARTIALS]
             for s in prof.samples if s["note"] == note and FH.D.layer_key(dataset, s) in keys]
    return note, takes


def _load(args):
    dataset, note = args
    prof = scale.Profile(dataset)
    key = FH.D.dynamics(dataset)[FH.D.loudest(dataset)]
    s = next((s for s in prof.samples if s["note"] == note
              and FH.D.layer_key(dataset, s) == key), None)
    return note, (measured_edcs(dataset, s) if s is not None else [])


# the unison mistuning scale: [strings] detune, from the beat rates of the partial envelopes (the
# EDC fit alone cannot pin it down; Salamander's and Iowa's projects); single strings (the
# harpsichord): no unison, the scale has no effect but is looked up
def detune(dataset):
    return scale.strings(dataset)["detune"]


def fit_global(dataset, notes):
    with Pool() as pool:
        cache = {n: m for n, m in pool.map(_load, [(dataset, n) for n in notes]) if len(m) >= 3}
        late = dict(pool.map(_load_late, [(dataset, n) for n in cache])) if hammered(dataset) else {}
        cache60 = dict(pool.map(_load60, [(dataset, n) for n in cache])) if fits_high_loss(dataset) else {}
    notes = sorted(cache)
    pool = Pool(initializer=_init, initargs=(dataset, cache, late, cache60))

    names = [k for k in GLOBAL if k != "detune"]
    measured = _g_prof_has_polarizations(dataset)
    if measured:     # fit_polarizations.py set the loss, the coupling, the horizontal excitation and detuning
        names = ["reactive"]
    held = {k: v for k, v in zip(GLOBAL, [0.3] * 4 + [0.7, 3.0, detune(dataset), 0.045, 0.3, 0.3]) if k not in names and k != "detune"}

    def total(x):
        g = dict(zip(names, np.exp(x)), detune=detune(dataset), **held)
        bounds = {**BOUNDS, **(FINGERED_BOUNDS if "fingered" in scale.traits(dataset) else {})}
        pen = sum(10 * max(np.log(lo / g[k]), 0, np.log(g[k] / hi)) for k, (lo, hi) in bounds.items())
        return float(np.mean(pool.map(_note_error, [(n, g) for n in notes]))) + pen

    x0 = np.log([0.3] * 4 + [0.7, 3.0, 0.045, 0.3, 0.3])              # Phase 1 values
    # several starts, each restarted once (Nelder–Mead stalls on this rugged surface)
    starts = [[0.2, 0.3, 0.35, 0.5, 1.7, 1.0, 0.1, 0.3, 0.3], [0.15, 0.2, 0.3, 0.5, 2.0, 2.0, 0.1, 1.0, 1.0],
              [0.4, 0.4, 0.5, 0.8, 0.5, 0.6, 0.2, 0.1, 0.1], [0.2, 0.3, 0.4, 0.5, 1.5, 0.8, 0.02, 2.0, 0.5]]
    if measured:     # the reactive part alone
        x0, starts = x0[-2:-1], [[0.1], [0.3], [1.0], [3.0]]
    # the committed fit first (a refit with small input changes stays near it)
    old = FH.P.load_fit(dataset).get("coupling") or {}
    committed = None
    if all(isinstance(old.get(k), (int, float)) and old[k] > 0 for k in names):
        starts = [[old[k] for k in names]] + starts
        committed = 0
    e0 = total(x0)
    results = []
    for x in starts:
        x = np.log(x)
        for _ in range(2):
            rr = minimize(total, x, method="Nelder-Mead",
                          options={"maxfev": 1500, "xatol": 0.01, "fatol": 0.005, "adaptive": True})
            x = rr.x
        print(f"  start{' (committed)' if committed == len(results) else ''} → {rr.fun:.2f} dB")
        results.append((rr.fun, rr))
    keep = choose(results, committed)
    r = results[keep][1]
    if committed is not None and keep != committed:
        print(f"  another start is better by {results[committed][0] - r.fun:.2f} dB than the committed fit's")
    g = dict(zip(names, np.exp(r.x).tolist()), detune=detune(dataset), **held)
    late = None
    if not measured and hammered(dataset):
        factors, before, after = fit_horiz_levels(pool, notes, g, old)
        g.update(factors)
        late = {"before": {"edc_db": before[0], "gap_db": before[1]}, "after": {"edc_db": after[0], "gap_db": after[1]}}
    high_loss = None
    if not measured and fits_high_loss(dataset):
        factors, before, after = fit_high_loss(pool, notes, g, old, cache60)
        g.update(factors)
        high_loss = {"edc60_before_db": before, "edc60_after_db": after}
    mobility = None
    if not measured and fits_mobility(dataset):
        factors, before, after = fit_mobility(pool, notes, g, old)
        g.update(factors)
        mobility = _mobility_report(pool, notes, g, before, after)
    moved = jumps({k: old[k] for k in names if k in old}, {k: g[k] for k in names})
    if moved:
        what = ", ".join(f"{k} {o:.3g} → {n:.3g}" for k, o, n in moved)
        err = f", error {old['err_db']:.2f} → {r.fun:.2f} dB" if isinstance(old.get("err_db"), (int, float)) else ""
        print(f"warning: coupling of {dataset} jumped: {what}{err}; listen before committing")
    per_note = dict(zip(notes, pool.map(_note_error, [(n, g) for n in notes])))
    depth = pool.map(_note_depths, [(n, g) for n in notes])
    pool.close()
    rec = [m for rows in depth for m, _ in rows]
    mod = [d for rows in depth for _, d in rows]
    return {"params": g, "err_db": r.fun, "err0_db": e0, "per_note_err_db": per_note, "late": late, "high_loss": high_loss, "mobility": mobility,
            "mod_depth_db": {"recording": float(np.median(rec)), "model": float(np.median(mod))}}


def _note_depths(args):
    note, g = args
    prof, meas = _g["prof"], _g["meas"][note]
    d = model_depths(note_params(prof, note, global_to_note(g, note, prof.design), g), prof.hammer_speeds()[FH.D.loudest(prof.dataset)], meas)
    return [(m[5], x) for m, x in zip(meas, d) if x is not None]


def main_global(dataset, notes):
    res = fit_global(dataset, notes)
    print(f"{dataset}: global fit, mean error {res['err0_db']:.2f} → {res['err_db']:.2f} dB; "
          f"aftersound modulation depth: recording {res['mod_depth_db']['recording']:.2f} dB, "
          f"model {res['mod_depth_db']['model']:.2f} dB")
    for k, v in res["params"].items():
        print(f"  {k:9s} {v:.3f}")
    if res["late"]:
        b, a = res["late"]["before"], res["late"]["after"]
        print(f"  horizontal levels: EDC error {b['edc_db']:.2f} → {a['edc_db']:.2f} dB, late gap "
              f"{b['gap_db']:.2f} → {a['gap_db']:.2f} dB")
    if res["high_loss"]:
        h = res["high_loss"]
        print(f"  high-frequency loss: EDC error over partials 1–{LOSS_N_MAX} {h['edc60_before_db']} → {h['edc60_after_db']} dB; "
              + " ".join(f"{k} {res['params'][k]:.3f}" for k in [f"beta3@{a}" for a in ANCHORS] + ["loss_q"]))
    if res["mobility"]:
        _print_mobility(dataset, res["mobility"], res["params"])
    print("  per note: " + " ".join(f"{n}:{e:.1f}" for n, e in res["per_note_err_db"].items()))
    FH.P.write_cache(dataset, "coupling_global", res, indent=1)
    path, prof_path = FH.P.cache_path(dataset, "coupling_global"), FH.P.fit_path(dataset)
    design = scale.design_of(dataset)
    if all(scale.n_strings(n, design) == 1 for n in notes):
        # single strings only (the harpsichord): global_to_note used kf2 for them
        res["params"]["kf1"] = res["params"]["kf2"]
    FH.P.update_fit(dataset, {"coupling": {"anchors": ANCHORS, **res["params"], "err_db": res["err_db"]}})
    print(f"wrote {path} and the coupling block of {prof_path}")


MIN_NOTES = 6        # Upright Y meets 6 (36, 48, … 96) and fits sanely with them


def default_notes(dataset):
    """Every 3rd key 24–96; a set whose recordings meet fewer than MIN_NOTES of them takes
    its recorded keys in 24–96 instead (the harp's thirds met 45, 48, 69, 72 only: the bass
    anchor's β1 went to 10.9 with no note near it)."""
    notes = list(range(24, 97, 3))
    recorded = sorted({s["note"] for s in scale.Profile(dataset, board=False).samples if 24 <= s["note"] <= 96})
    return notes if len(set(notes) & set(recorded)) >= MIN_NOTES else recorded


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    ap.add_argument("--notes", help="comma-separated MIDI notes (default: every 3rd, 24–96)")
    ap.add_argument("--high-loss-only", action="store_true",
                    help="the loss stage alone (the strings' high-frequency loss) from the committed fit")
    ap.add_argument("--mobility-only", action="store_true",
                    help="the third stage alone (the bridge's mobility, the horizontal levels) from the committed fit")
    ap.add_argument("--global", dest="glob", action="store_true",
                    help="one smooth parameter set for all notes instead of per-note fits")
    args = ap.parse_args()
    notes = [int(n) for n in args.notes.split(",")] if args.notes else default_notes(args.profile)
    if args.high_loss_only:
        return main_high_loss(args.profile, notes)
    if args.mobility_only:
        return main_mobility(args.profile, notes)
    if args.glob:
        return main_global(args.profile, notes)
    with Pool() as pool:
        res = dict(pool.map(fit_note, [(args.profile, n) for n in notes]))
    print(f"{args.profile}: EDC fit per note (error before → after, dB)")
    print(" note  S   err0 →  err   β1     β3     kf    detune  horiz")
    for n, r in res.items():
        if r:
            print(f"{n:5d} {scale.n_strings(n, scale.design_of(args.profile)):2d} {r['err0_db']:6.2f} → {r['err_db']:5.2f}  "
                  f"{r['beta1']:5.2f}  {r['beta3']:5.2f}  {r['kf']:5.2f}  {r['detune']:5.2f}  {r['horiz']:.3f}")
    FH.P.write_cache(args.profile, "coupling", {str(n): r for n, r in res.items() if r}, indent=1)
    path = FH.P.cache_path(args.profile, "coupling")
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
