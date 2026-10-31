#!/usr/bin/env python3
"""Pluck point per note from the recorded partial levels (the harpsichord's felt fit).

A string plucked at β (re the speaking length) has partial amplitudes ∝ sin(kπβ)/k²:
notches near k = m/β. Per note, the levels of partials 1…K_MAX (loudest layer, conf > 0.3)
are fitted as a smooth envelope (quadratic in log2 k: soundboard, quill, room) plus the
comb 20·log10(|sin(kπβ)| + NOTCH_FLOOR) over a grid of β; a note counts where the comb
lowers the residual by at least MIN_GAIN_DB. The per-note values are smoothed over the
keyboard (weights: the gain) and clipped to [0.04, 0.5]; keys without data take the
design curve (scale.pluck_beta). The "hammer speeds" are the pluck's dynamic steps (pp 0.5,
mf 1, ff 2). One layer (the harpsichords): the velocity constants are Penttinen's (2006);
two (the harp's mf and ff): fitted to the partials' level change from mf to ff
(fit_velocity).

The clavichord (scale.TANGENT): no pluck point (the tangent strikes at the speaking length's
end), the speeds and velocity constants only.

Usage: fit_pluck.py --profile harpsichord
"""
import argparse
import json
import os

import numpy as np

import fit_hammer as FH
import scale

HERE = os.path.dirname(os.path.abspath(__file__))
BETA_GRID = np.arange(0.04, 0.5001, 0.002)
# a part's range of pluck points on the grid: [strings] beta_range (the harp's, near the middle)
NOTCH_FLOOR = 0.03      # |sin| floor: recorded notches are filled (other polarization, noise)
MIN_GAIN_DB = 1.0
K_MAX = 40
CURVE_SMOOTH = 4.0      # semitones
# the comb is ambiguous: 2β (and ~β/2) fit some notes nearly as well; a value further than
# OUTLIER_FRAC from the running median over ±OUTLIER_SPAN semitones is left out
OUTLIER_FRAC = 0.3
OUTLIER_SPAN = 6
SPEEDS = {"pp": 0.5, "mf": 1.0, "ff": 2.0}
VELOCITY = {"gain_db_per_step": 3.0, "fq_oct_per_step": 0.5}
FQ_OCT_GRID = np.arange(0.0, 3.0001, 0.05)


def fit_beta(k, level_db, weight, grid=BETA_GRID):
    """(β, residual dB, residual without the comb dB) of one note's partial levels."""
    k, y, w = (np.asarray(v, float) for v in (k, level_db, weight))
    x = np.log2(k)
    X = np.column_stack([np.ones_like(x), x, x * x])
    sw = np.sqrt(w)

    def resid(v):
        coef, *_ = np.linalg.lstsq(X * sw[:, None], v * sw, rcond=None)
        r = (v - X @ coef) * sw
        return float(np.sqrt(np.sum(r ** 2) / np.sum(w)))
    errs = [resid(y - 20 * np.log10(np.abs(np.sin(k * np.pi * b)) + NOTCH_FLOOR)) for b in grid]
    i = int(np.argmin(errs))
    return float(grid[i]), errs[i], resid(y)


def velocity_delta_db(f, f_q, fq_oct):
    """Partial level change (dB, without the flat gain) one dynamic step up: the pluck's
    lowpass 1/√(1 + (f/f_q′)²) opening from f_q to f_q·2^fq_oct (string_model.pluck_amplitudes)."""
    f = np.asarray(f, float)
    return 10 * np.log10((1 + (f / f_q) ** 2) / (1 + (f / (f_q * 2.0 ** fq_oct)) ** 2))


def fit_velocity(pairs):
    """{gain_db_per_step, fq_oct_per_step} from (f / f_q, Δ level dB, weight) of the partials
    of every note recorded at mf and ff (one step apart): per fq_oct of the grid the flat
    gain by weighted least squares, the pair with the smallest residual."""
    x, d, w = (np.asarray(v, float) for v in zip(*pairs))
    best = None
    for o in FQ_OCT_GRID:
        r0 = d - velocity_delta_db(x, 1.0, o)
        g = float(np.sum(w * r0) / np.sum(w))
        err = float(np.sqrt(np.sum(w * (r0 - g) ** 2) / np.sum(w)))
        if best is None or err < best[2]:
            best = (g, float(o), err)
    return {"gain_db_per_step": round(best[0], 2), "fq_oct_per_step": round(best[1], 2), "err_db": round(best[2], 2)}


def velocity_pairs(profile, samples):
    """(f / f_q, ff − mf level dB, weight) per partial recorded with conf > 0.3 in both (the
    analysis's levels are re each file's peak). Weight: the lower conf over the note's number
    of partials (each note weighs one). Weighted by their share of the energy at mf, the
    fundamentals ruled and the lowpass's opening ran to the grid's end (3 octaves per step)."""
    import scale
    prof = scale.Profile(profile, board=False)
    by = {}
    for s in samples:
        if "error" not in s and "label_mismatch" not in s:
            by.setdefault(s["note"], {})[FH.D.dynamic_of(profile, s)] = s
    pairs = []
    for note, d in sorted(by.items()):
        if "mf" not in d or "ff" not in d:
            continue
        f_q = scale.note_params(prof, note).pluck_fq
        lo = {p["n"]: p for p in d["mf"]["partials"] if p["conf"] > 0.3}
        both = [(p, lo[p["n"]]) for p in d["ff"]["partials"]
                if p["n"] in lo and p["conf"] > 0.3 and p["n"] <= K_MAX]
        if not both:
            continue
        for p, q in both:
            d_db = p["level_db"] + d["ff"]["peak_dbfs"] - q["level_db"] - d["mf"]["peak_dbfs"]
            pairs.append((p["freq"] / f_q, d_db, min(p["conf"], q["conf"]) / len(both)))
    return pairs


def beta_curve(per_note):
    """Smoothed pluck point for keys 21…108 (design curve where no note says anything)."""
    good = {n: v for n, v in per_note.items() if v["err0_db"] - v["err_db"] >= MIN_GAIN_DB}
    # twice: where the outliers cluster (Unk A1–C#3), the first median includes them and
    # keeps a 2β note while dropping genuine ones; the second runs over the first's survivors
    keep = good
    for _ in range(2):
        med = {n: np.median([v["beta"] for m, v in keep.items() if abs(m - n) <= OUTLIER_SPAN] or [v["beta"]])
               for n, v in good.items()}
        keep = {n: v for n, v in good.items() if abs(v["beta"] - med[n]) <= OUTLIER_FRAC * med[n]}
    good = keep
    notes = list(range(21, 109))
    if not good:
        return {"notes": notes, "beta": [scale.pluck_beta(n) for n in notes]}
    ns = sorted(good)
    at = scale._smooth(ns, [good[n]["beta"] for n in ns], [good[n]["err0_db"] - good[n]["err_db"] for n in ns],
                       frac=CURVE_SMOOTH)
    lo, hi = ns[0], ns[-1]
    beta = [at(min(max(n, lo), hi)) for n in notes]           # held beyond the data
    return {"notes": notes, "beta": [float(np.clip(b, 0.04, 0.5)) for b in beta]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    args = ap.parse_args()
    samples = FH.P.load_cache(args.profile, "analysis")["samples"]
    key = FH.D.dynamics(args.profile)[FH.D.loudest(args.profile)]
    per_note = {}
    # the tangent strikes at the speaking length's end: no comb, no pluck point to fit
    tangent = scale.design_of(args.profile) in scale.TANGENT
    for s in [] if tangent else samples:
        if "error" in s or "label_mismatch" in s or FH.D.layer_key(args.profile, s) != key:
            continue
        ps = [p for p in s["partials"] if p["n"] <= K_MAX and p["conf"] > 0.3]
        if len(ps) < 8:
            continue
        lo, hi = scale.strings(args.profile).get("beta_range", (0.0, 1.0))
        grid = BETA_GRID[(BETA_GRID >= lo - 1e-9) & (BETA_GRID <= hi + 1e-9)]
        b, e, e0 = fit_beta([p["n"] for p in ps], [p["level_db"] for p in ps], [p["conf"] for p in ps], grid)
        per_note[s["note"]] = {"beta": b, "err_db": e, "err0_db": e0}
    curve = beta_curve(per_note)
    velocity = VELOCITY
    if {"mf", "ff"} <= set(FH.D.dynamics(args.profile)):
        velocity = fit_velocity(velocity_pairs(args.profile, samples))
        print(f"{args.profile}: velocity {velocity}")
    pluck = {"velocity": velocity} if tangent else {"beta_curve": curve, "per_note": {str(n): v for n, v in sorted(per_note.items())},
                                                     "velocity": velocity}
    FH.P.update_fit(args.profile, {"profile": args.profile, "hammer_speeds": SPEEDS, "pluck": pluck})
    used = sum(v["err0_db"] - v["err_db"] >= MIN_GAIN_DB for v in per_note.values())
    if tangent:
        print(f"{args.profile}: tangent, velocity {velocity}")
    else:
        print(f"{args.profile}: {len(per_note)} notes, {used} with a pluck-point notch pattern; β "
              + ", ".join(f"{n}: {b:.2f}" for n, b in zip(curve["notes"][::12], curve["beta"][::12])))
    print(f"wrote {FH.P.fit_path(args.profile)}")


if __name__ == "__main__":
    main()
