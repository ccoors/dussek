#!/usr/bin/env python3
"""The two polarizations' decays and detuning per note, measured from the recordings.

A plucked harp string sounds two polarizations: one coupled strongly to the soundboard, the
other ringing on, a few cents apart. Per partial the envelope is their coherent sum, so it
beats and bends at once (C5's fundamental drops 24 dB in 0.24 s and comes back to −20: a
beat's first minimum, not a 100 dB/s first stage). The coupling fit (fit_coupling.py) ties
both to the measured loss b1′ (b1 = β1·b1′, κ = kf·b1′ with one kf for all keys) and its
beats ran at 1–1.5 cents where the recordings beat at 2–8.

Here, per partial of the loudest layer (the coupling fit's envelopes, measured_edcs's
window), the envelope from 20 ms after the onset until it is 6 dB above the noise floor
(at most −50 dB) is fitted as the two components' coherent sum,
    L(t) = a + 20·log10|e^(−σ_f·t) + 10^(−g/20)·e^(−σ_s·t)·e^(j(2π·Δf·t + φ))|   (dB),
σ_f ≥ σ_s (1/s), g the slower one's level below the faster one's (dB), Δf the beat (Hz).
A partial counts where this lowers the residual by MIN_GAIN_DB re one line (over the
whole decay: the beats run through it) and the beat is slow enough to be seen
(Δf ≤ MAX_BEAT_HZ); else it decays in one stage at the line's rate. The model
(string_model.free_modes): the vertical polarization decays at b1 + b3·ω² + κ·Re Y_n, the
horizontal at b1 + b3·ω² + horiz_kappa, excited horiz_gain × as strongly and detuned by
horiz_detune_cents. So per note:
    b1, b3      from the slower rates (σ_s − horiz_kappa; one stage: its rate)
    κ           the median of σ_f − (b1 + b3·ω²)
(the low_weighted trait: b1 and κ medians weighted 1/n² over all partials, a one-stage partial's κ
excess 0; b3 the upper partials' excess over b1, their rate one line's through both stages; κ
through a running median before the smoothing)
    horiz_gain  10^(−g/20)      } the mean over the partials, weights 1/n²
    hdetune     Δf in cents     }   (the low partials carry the energy)
then smoothed over the keyboard (weights: the number of partials) and held beyond the
recorded keys. The coupling fit leaves these alone where this block is present.

Writes the "polarizations" block of the part's fit document.
Usage: fit_polarizations.py --profile harp
"""
import argparse
import json
import os
import sys
from multiprocessing import Pool

import numpy as np
from scipy.optimize import least_squares

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import fit_coupling as FC  # noqa: E402
import fit_hammer as FH  # noqa: E402
import scale  # noqa: E402

DB_PER_NEPER = 20 / np.log(10)
N_MAX = 6               # partials per note
T_START = 0.02          # s after the onset
FLOOR_MARGIN_DB = 6.0
RANGE_DB = 50.0
MIN_POINTS = 20
MIN_GAIN_DB = 0.5       # the coherent pair lowers the residual re one line by at least this (dB rms)
MAX_BEAT_HZ = 8.0       # beats faster than this hide in the envelope's 20 ms hop
SMOOTH = 4.0            # semitones
MEDIAN_SPAN = 5         # semitones each side
HORIZ_RANGE = (0.01, 1.0)   # the slower polarization can carry as much as the faster one
HDETUNE_MAX = 10.0      # cents (fit_coupling.FINGERED_BOUNDS)


def coherent(t, e):
    """(σ_f, σ_s, g, Δf, residual dB, residual of one line dB, the line's rate 1/s) of an
    envelope (dB) over t (s): two decaying components' coherent sum, several starts."""
    t, e = np.asarray(t, float), np.asarray(e, float)
    line = np.polyfit(t, e, 1)
    res1 = float(np.sqrt(np.mean((np.polyval(line, t) - e) ** 2)))

    def model(p):
        a, s1, s2, g, df, ph = p
        z = np.exp(-s1 * t) + 10 ** (-g / 20) * np.exp(-s2 * t) * np.exp(1j * (2 * np.pi * df * t + ph))
        return a + 20 * np.log10(np.abs(z) + 1e-9)
    rate = max(-line[0] / DB_PER_NEPER, 0.05)
    best = None
    for g0 in (1.0, 6.0, 15.0):
        for df0 in (0.2, 0.6, 1.5, 3.0):
            for f0, s0 in ((3.0, 1.0), (1.5, 0.4)):    # the line's rate between the two, or near the fast one
                for ph0 in (0.0, np.pi / 2, -np.pi / 2, 3.0):   # the beat's phase: one start got stuck
                    r = least_squares(lambda p: model(p) - e, [e[0], f0 * rate, s0 * rate, g0, df0, ph0],
                                      bounds=([-300.0, 0.0, 0.0, -20.0, 0.01, -np.pi], [100.0, 300.0, 100.0, 40.0, MAX_BEAT_HZ, np.pi]))
                    if best is None or r.cost < best.cost:
                        best = r
    a, s1, s2, g, df, ph = best.x
    if s2 > s1:                       # name the faster one first
        s1, s2, g = s2, s1, -g
    return float(s1), float(s2), float(g), float(df), float(np.sqrt(np.mean(best.fun ** 2))), res1, float(rate)


def measure(dataset, s):
    """Per usable partial of a sample: (n, freq, envelope t, dB) as fit_coupling sees them."""
    import soundfile as sf
    x, sr = sf.read(FH.D.sample_path(dataset, s), always_2d=True)
    cut = s.get("tail_cut_s", FH.D.tail_cut(dataset))
    if cut:
        x = x[: len(x) - int(cut * sr)]
    # not within a measured board mode's band: that one takes the partial's extra loss (else
    # κ and b1 here take it in too, and the model counts it twice)
    modes = scale.body_modes(dataset)
    parts = [p for p in s["partials"] if p["n"] <= N_MAX and p["range_db"] >= FC.MIN_RANGE_DB
             and not scale.in_mode_band(p["freq"], modes)]
    if not parts:
        return []
    t, env = A.partial_envelopes(x, sr, [p["freq"] for p in parts], s["f0"])
    env_db = A.db(env) - s["peak_dbfs"]
    out = []
    for p, e in zip(parts, env_db):
        post, tp = e[t >= s["onset_s"]], t[t >= s["onset_s"]] - s["onset_s"]
        ipk = int(np.argmax(post[: max(len(post) // 4, 3)]))
        m = (tp >= max(T_START, tp[ipk])) & (post > max(p["floor_db"] + FLOOR_MARGIN_DB, post[ipk] - RANGE_DB))
        # up to where it first sinks to the floor (later points: noise, a re-pluck)
        stop = np.nonzero(~m & (tp > tp[ipk] + T_START))[0]
        if len(stop):
            m &= tp < tp[stop[0]]
        if m.sum() >= MIN_POINTS:
            out.append((p["n"], p["freq"], tp[m], post[m]))
    return out


def note_rates(dataset, s, horiz_kappa):
    """Per partial: (n, ω, σ_s − horiz_kappa, σ_f or None, g or None, cents or None, the single
    line's rate − horiz_kappa)."""
    rows = []
    for n, f, t, e in measure(dataset, s):
        sf_, ss, g, df, res, res1, single = coherent(t, e)
        if res1 - res >= MIN_GAIN_DB and df < MAX_BEAT_HZ * 0.99:
            cents = 1200 * np.log2(1 + df / f)
            rows.append((n, 2 * np.pi * f, ss - horiz_kappa, sf_, g, float(cents), single - horiz_kappa))
        else:
            rows.append((n, 2 * np.pi * f, single - horiz_kappa, None, None, None, single - horiz_kappa))
    return rows


def weighted_median(v, w):
    o = np.argsort(v)
    c = np.cumsum(np.asarray(w, float)[o])
    return float(np.asarray(v, float)[o][np.searchsorted(c, 0.5 * c[-1])])


def per_note(rows, low_weighted=False):
    """{b1, b3, kappa, horiz, hdetune, n, n_two} of one note's partial rows (None without rows)."""
    if not rows:
        return None
    w = np.array([r[1] for r in rows])
    ss = np.array([max(r[2], 0.01) for r in rows])
    if low_weighted:
        n = np.array([r[0] for r in rows], float)
        b1 = weighted_median(ss, 1 / n ** 2)
        up = n >= 2
        # the upper partials' whole decay (one line through both stages: a two-stage one's slow
        # stage, its tail 8–16 dB under, ran partials 2–3 at half the recordings' rates)
        line = np.array([max(r[6], 0.01) for r in rows])
        b3 = 0.0
        for _ in range(2):   # b1 net of b3·ω² (the larger b3 ran the fundamental 1.5 × too fast)
            b3 = max(float(np.median((line[up] - b1) / w[up] ** 2)), 0.0) if up.any() else 0.0
            b1 = max(weighted_median(ss - b3 * w ** 2, 1 / n ** 2), 0.01)
    elif len(rows) >= 3:
        (b1, b3), *_ = np.linalg.lstsq(np.column_stack([np.ones_like(w), w ** 2]), ss, rcond=None)
        if b3 < 0 or b1 <= 0:
            b1, b3 = float(np.median(ss)), 0.0
    else:
        b1, b3 = float(np.median(ss)), 0.0
    two = [r for r in rows if r[3] is not None]
    if not two:
        return {"b1": float(b1), "b3": float(b3), "kappa": 0.0, "horiz": None, "hdetune": None, "n": len(rows), "n_two": 0}
    wn = [1 / r[0] ** 2 for r in two]
    excess = [max(r[3] - r[2], 0.0) for r in two]   # σ_f − (b1 + b3·ω²); r[2] = σ_s − horiz_kappa
    if low_weighted:
        # every partial, a one-stage one as no excess (a slow one-stage fundamental keeps it low)
        allx = [max(r[3] - r[2], 0.0) if r[3] is not None else 0.0 for r in rows]
        kappa = weighted_median(allx, [1 / r[0] ** 2 for r in rows])
    else:
        kappa = float(np.median(excess))
    horiz = float(np.exp(np.average([-r[4] / 20 * np.log(10) for r in two], weights=wn)))
    hdetune = float(np.average([r[5] for r in two], weights=wn))
    return {"b1": float(b1), "b3": float(b3), "kappa": kappa, "horiz": horiz, "hdetune": hdetune,
            "n": len(rows), "n_two": len(two)}


def curves(notes_data, median_kappa=False):
    """Smoothed per-key (21…108) b1, b3, κ, horiz_gain from the per-note values (`median_kappa`:
    κ through a running median first, MEDIAN_SPAN: the strumstick's few two-stage fundamentals
    carried their κ into the one-stage notes beside them)."""
    keys = list(range(21, 109))
    ns = sorted(notes_data)
    w = np.array([notes_data[n]["n"] for n in ns], float)
    lo, hi = ns[0], ns[-1]
    held = lambda at: [float(at(min(max(k, lo), hi))) for k in keys]  # noqa: E731
    out = {"notes": keys}
    out["b1"] = list(np.exp(held(scale._smooth(ns, [np.log(notes_data[n]["b1"]) for n in ns], w, frac=SMOOTH))))
    out["b3"] = [max(v, 0.0) for v in held(scale._smooth(ns, [notes_data[n]["b3"] for n in ns], w, frac=3 * SMOOTH))]
    kap = [notes_data[n]["kappa"] for n in ns]
    if median_kappa:
        kap = [float(np.median([notes_data[m]["kappa"] for m in ns if abs(m - n) <= MEDIAN_SPAN])) for n in ns]
    out["kappa"] = [max(v, 0.0) for v in held(scale._smooth(ns, kap, w, frac=SMOOTH))]
    hn = [n for n in ns if notes_data[n]["horiz"] is not None]
    if hn:
        hw = np.array([notes_data[n]["n_two"] for n in hn], float)
        # note to note they scatter (detuning 0.8–13.6 cents, level ratio 0.10–1.5): a running
        # median first (E1's 13.6 cents alone had the smoothed bass at the bound)
        med = lambda k: [float(np.median([np.log(notes_data[m][k]) for m in hn if abs(m - n) <= MEDIAN_SPAN])) for n in hn]  # noqa: E731
        at = scale._smooth(hn, med("horiz"), hw, frac=SMOOTH)
        out["horiz"] = [float(np.clip(np.exp(at(min(max(k, hn[0]), hn[-1]))), *HORIZ_RANGE)) for k in keys]
        at = scale._smooth(hn, med("hdetune"), hw, frac=SMOOTH)
        out["hdetune"] = [float(np.clip(np.exp(at(min(max(k, hn[0]), hn[-1]))), 0.02, HDETUNE_MAX)) for k in keys]
    else:
        out["horiz"] = out["hdetune"] = [None] * len(keys)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    args = ap.parse_args()
    prof = scale.Profile(args.profile, board=False)
    horiz_kappa = scale.M.NoteParams.__dataclass_fields__["horiz_kappa"].default
    key = FH.D.dynamics(args.profile)[FH.D.loudest(args.profile)]
    samples = [s for s in prof.samples if FH.D.layer_key(args.profile, s) == key]
    with Pool() as pool:     # ~130 partials × 96 starts: 2.4 min on one core
        rates = pool.starmap(note_rates, [(args.profile, s, horiz_kappa) for s in samples])
    low = "low_weighted" in scale.traits(args.profile)
    data = {s["note"]: r for s, r in zip(samples, (per_note(x, low) for x in rates)) if r}
    c = curves(data, median_kappa=low)
    FH.P.update_fit(args.profile, {"polarizations": {**c, "per_note": {str(n): v for n, v in sorted(data.items())}}})
    print(f"{args.profile}: {len(data)} notes, two-stage partials " + ", ".join(f"{n}:{v['n_two']}/{v['n']}" for n, v in sorted(data.items())))
    for k in range(24, 109, 12):
        i = k - 21
        print(f"  {k}: b1 {c['b1'][i]:.2f}/s ({c['b1'][i] * DB_PER_NEPER:.0f} dB/s)  κ {c['kappa'][i]:.2f}/s "
              f"({c['kappa'][i] * DB_PER_NEPER:.0f} dB/s)  horiz {c['horiz'][i] if c['horiz'][i] is None else round(c['horiz'][i], 3)}"
              f"  detune {c['hdetune'][i] if c['hdetune'][i] is None else round(c['hdetune'][i], 2)} c")
    print(f"wrote the polarizations block of {FH.P.fit_path(args.profile)}")


if __name__ == "__main__":
    main()
