#!/usr/bin/env python3
"""Measured board modes: the few strong low modes a recording shows, which the random board
(board.py: one statistical draw) cannot place.

On a guitar the top's main mode (~220 Hz) takes the energy of every partial near it: G3–B♭3's
fundamentals decay at 70–135 dB/s on all strings, their neighbors at ~15–30. Weaker, the same on
pianos: the three grands of one model (Grands B, D, E) decay ×1.35–1.6 faster at 101–127 Hz
than their notes' trends, over 7–15 notes each (the engine's renders: ×1.1, not significant).

Per partial of the low range (F_RANGE), the measured energy-decay rate (analysis: EDC over the
first 20 dB, the loudest layer) against the model's (string_model.free_modes
with the profile's fitted parameters, the board without measured modes, the same EDC over the
same window): r = ln(measured / model). Bands where r is high over many notes (detect_bands:
smoothed over SMOOTH_OCT, significant against shuffling r over the partials) start a mode each
(at most MAX_MODES); then all are fitted together (f, q, g = φ²; least squares on r with a
soft L1 loss) through the model itself (the unison systems and both polarizations: no
approximation of how a mode's admittance moves a decay), with the share of the modes' admittance
the horizontal polarization feels (string_model.NoteParams.horiz_share: the guitar's G3 falls
at 72 dB/s through its first 20 dB, so the top mode damps both polarizations; with the
vertical one alone the slow horizontal one carried the EDC and the mode made it slower). A mode stays if leaving it out raises
the cost by MIN_GAIN.

The profile's loss and polarization fits leave out the partials within a mode's band
(scale.in_mode_band), so run the chain, this, then the chain again (the second pass refits the
modes against the string losses without them).

Writes "body_modes" into the part's fit document (read by scale.make_board).
Usage: fit_body_modes.py --profile guitar [--clear]
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
import fit_hammer as FH  # noqa: E402
import scale  # noqa: E402
from board import Board  # noqa: E402

F_RANGE = (50.0, 600.0)     # Hz: where a board's modes are few enough to stand out
SMOOTH_OCT = 1 / 24         # the residual's smoothing over log2 f (Gaussian σ)
BAND_OCT = 1 / 12           # a band: ± this around its peak
MIN_EXCESS = np.log(1.25)   # a band's smoothed excess at least (25 % faster)
MAX_P = 0.01                # its significance against shuffled residuals
N_PERM = 2000
MAX_MODES = 3
MIN_NOTES = 3               # a band holds partials of this many notes at least
Q0 = 30.0                   # the modes' starting q (a guitar top's ~20–50)
Q_RANGE = (10.0, 100.0)     # low plate and air modes (a guitar top: ~20–60)
F_SPAN_OCT = 1 / 6          # a mode's frequency within this of its band
SHARE0 = 0.5                # the horizontal polarization's share of the modes, to start
MIN_GAIN = 0.03             # a mode stays if the cost rises this much without it
LOSS_SCALE = 0.3            # soft L1 scale of the residual (ln)
MIN_CONF = 0.3
EDC_RANGE_DB = 20.0
DT = 0.01
T_START = 0.05


def edc_slope_db_s(t, power):
    """The energy-decay curve's slope over its first EDC_RANGE_DB (dB/s of amplitude, as
    analyze.fit_decay), leaving out the last 10 % where the truncated integral bends."""
    edc = np.cumsum(power[::-1])[::-1]
    edc_db = 10 * np.log10(np.maximum(edc / edc[0], 1e-30))
    sel = (edc_db > -EDC_RANGE_DB) & (np.arange(len(edc_db)) < 0.9 * len(edc_db))
    if sel.sum() < 5:
        sel = np.arange(len(edc_db)) < max(5, len(edc_db) // 10)
    return float(-np.polyfit(t[sel], edc_db[sel], 1)[0])


def detect_bands(f, r, w, rng=None):
    """Bands of high residual: [{f, excess, p, n}] strongest first. f (Hz), r = ln(measured /
    model), w weights per partial."""
    rng = rng or np.random.default_rng(0)
    lf = np.log2(f)
    grid = np.arange(lf.min(), lf.max(), SMOOTH_OCT / 4)

    def smooth(v):
        k = np.exp(-0.5 * ((grid[:, None] - lf[None, :]) / SMOOTH_OCT) ** 2) * w[None, :]
        return (k * v[None, :]).sum(axis=1) / np.maximum(k.sum(axis=1), 1e-12)

    s = smooth(r)
    peaks = [i for i in range(1, len(grid) - 1) if s[i] >= s[i - 1] and s[i] >= s[i + 1] and s[i] > MIN_EXCESS]
    out = []
    for i in sorted(peaks, key=lambda i: -s[i]):
        band = np.abs(lf - grid[i]) <= BAND_OCT
        if band.sum() < MIN_NOTES or any(abs(np.log2(b["f"]) - grid[i]) < 2 * BAND_OCT for b in out):
            continue
        obs = np.average(r[band], weights=w[band])
        null = np.array([np.average(rng.permutation(r)[band], weights=w[band]) for _ in range(N_PERM)])
        p = float((null >= obs).mean())
        if p < MAX_P:
            out.append({"f": float(2 ** grid[i]), "excess": float(obs), "p": p, "n": int(band.sum())})
        if len(out) == MAX_MODES:
            break
    return out


# --------------------------------------------------------------------------- the model

_PROF = {}


def _profile(dataset):
    if dataset not in _PROF:
        _PROF[dataset] = scale.Profile(dataset, board=False)
    return _PROF[dataset]


def note_decays(args):
    """The model's EDC rates (dB/s) of one note's partials [(n, T)] with a board of these modes."""
    dataset, note, parts, modes, share, velocity = args
    prof = _profile(dataset)
    board = Board(seed=scale.strings(dataset).get("board_seed", 1), density=scale.BOARD_DENSITY[prof.design], modes=modes)
    params = scale.note_params(prof, note, admittance=board.admittance, horiz_admittance=board.admittance_measured,
                               horiz_share=share)
    fm = scale.M.free_modes(params, velocity)
    k, lam, wt, S = fm["k"], fm["lam"], fm["weight"], fm["S"]
    out = []
    for n, T in parts:
        i = np.nonzero(k == n)[0]
        if len(i) == 0:
            out.append(np.nan)
            continue
        t = np.arange(T_START, T_START + T, DT)
        p = 0
        for sl in (slice(0, S), slice(S, None)):    # the polarizations add in power
            comp = wt[i[0], sl][:, None] * np.exp(lam[i[0], sl][:, None] * t)
            p = p + np.abs(comp.sum(axis=0)) ** 2
        out.append(edc_slope_db_s(t, p))
    return out


def measured(prof, f_range=F_RANGE):
    """{note: [(n, freq, rate dB/s, window s, weight)]} of the loudest layer (as the polarization
    fit; the guitar's pp and mf put A2's second partial at 9 dB/s, its ff at 39, the neighbors'
    ff at 55–106: their median over the layers pulled the top's mode narrow and 15 Hz high)."""
    key = FH.D.dynamics(prof.dataset)[FH.D.loudest(prof.dataset)]
    rows = {}
    for s in (s for s in prof.samples if FH.D.layer_key(prof.dataset, s) == key):
        for p in s["partials"]:
            if p.get("edc_decay_db_s", 0) > 0 and p.get("conf", 0) > MIN_CONF and f_range[0] <= p["freq"] <= f_range[1]:
                rows.setdefault((s["note"], p["n"]), []).append((p["freq"], p["edc_decay_db_s"], p["fit_dur_s"], p["conf"]))
    out = {}
    for (note, n), v in sorted(rows.items()):
        f, d, T, c = map(np.array, zip(*v))
        out.setdefault(note, []).append((n, float(np.median(f)), float(np.median(d)), float(min(np.median(T), 8.0)), float(c.sum())))
    return out


class Fit:
    def __init__(self, dataset, pool):
        self.dataset = dataset
        self.pool = pool
        self.prof = _profile(dataset)
        self.meas = measured(self.prof)
        self.velocity = self.prof.hammer_speeds()[FH.D.loudest(dataset)]
        self.density = scale.BOARD_DENSITY[self.prof.design]
        self.f = np.array([r[1] for v in self.meas.values() for r in v])
        self.d = np.array([r[2] for v in self.meas.values() for r in v])
        self.w = np.array([r[4] for v in self.meas.values() for r in v])

    def model(self, modes, share=0.0):
        jobs = [(self.dataset, note, [(r[0], r[3]) for r in v], modes, share, self.velocity) for note, v in self.meas.items()]
        return np.array([x for res in self.pool.map(note_decays, jobs) for x in res])

    def residual(self, modes, share=0.0):
        m = self.model(modes, share)
        r = np.log(self.d) - np.log(np.maximum(m, 1e-3))
        return np.where(np.isfinite(r), r, 0.0)

    def cost(self, modes, share=0.0):
        r = self.residual(modes, share)
        z = (r / LOSS_SCALE) ** 2
        return float(np.sum(self.w * 2 * (np.sqrt(1 + z) - 1)))


def pack(modes, share):
    return np.array([v for m in modes for v in (np.log(m["f"]), np.log(m["q"]), np.log(m["g"]))] + [share])


def unpack(x):
    """(modes, the horizontal share) from the fit's vector: per mode ln f, ln q, ln g; the share last."""
    return ([{"f": float(np.exp(x[i])), "q": float(np.exp(x[i + 1])), "g": float(np.exp(x[i + 2]))} for i in range(0, len(x) - 1, 3)],
            float(x[-1]))


def start_g(fit, band, r0):
    """φ² that lifts the band's partials by its excess, from the admittance's peak: a mode at
    its peak adds κ·2g/(σ_m·density) to a partial's rate (one string; the unisons' fast
    component more, the fit corrects it)."""
    sel = np.abs(np.log2(fit.f / band["f"])) <= BAND_OCT
    rate = np.median(fit.d[sel] / np.exp(r0[sel])) / scale.DB_PER_NEPER       # model, 1/s
    lift = rate * (np.exp(band["excess"]) - 1)
    notes = sorted({note for note, v in fit.meas.items() for row in v if abs(np.log2(row[1] / band["f"])) <= BAND_OCT})
    kappa = np.median([scale.note_params(fit.prof, n).kappa for n in notes])
    sigma_m = np.pi * band["f"] / Q0
    return float(np.clip(lift * sigma_m * fit.density / (2 * max(kappa, 1e-3)), 1e-3, 1e4))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    ap.add_argument("--clear", action="store_true", help="remove the profile's measured modes")
    args = ap.parse_args()
    if args.clear:
        FH.P.update_fit(args.profile, {}, drop=("body_modes",))
        print(f"{args.profile}: measured modes removed")
        return
    with Pool() as pool:
        fit = Fit(args.profile, pool)
        r0 = fit.residual([])
        cost0 = fit.cost([])
        bands = detect_bands(fit.f, r0, fit.w)
        print(f"{args.profile}: {len(fit.f)} partials in {F_RANGE[0]:.0f}–{F_RANGE[1]:.0f} Hz, cost {cost0:.1f}; bands: "
              + (", ".join(f"{b['f']:.0f} Hz ×{np.exp(b['excess']):.2f} (p {b['p']:.3f}, {b['n']} partials)" for b in bands) or "none"))
        modes = [{"f": b["f"], "q": Q0, "g": start_g(fit, b, r0)} for b in bands]
        share = SHARE0 if modes else 0.0
        if modes:
            x0 = pack(modes, share)
            lo = np.array([v for b in bands for v in (np.log(b["f"]) - F_SPAN_OCT * np.log(2), np.log(Q_RANGE[0]), np.log(1e-4))] + [0.0])
            hi = np.array([v for b in bands for v in (np.log(b["f"]) + F_SPAN_OCT * np.log(2), np.log(Q_RANGE[1]), np.log(1e5))] + [1.0])
            res = least_squares(lambda x: np.sqrt(fit.w) * fit.residual(*unpack(x)), np.clip(x0, lo, hi), bounds=(lo, hi),
                                loss="soft_l1", f_scale=LOSS_SCALE, diff_step=0.005, max_nfev=80)
            modes, share = unpack(res.x)
            # a mode stays where it pays
            cost = fit.cost(modes, share)
            modes = [m for i, m in enumerate(modes) if fit.cost(modes[:i] + modes[i + 1:], share) > cost * (1 + MIN_GAIN)]
            share = share if modes else 0.0
        cost1 = fit.cost(modes, share)
        r1 = fit.residual(modes, share)
    for m in modes:
        print(f"  mode {m['f']:.1f} Hz  q {m['q']:.1f}  g {m['g']:.3g}")
    if modes:
        print(f"  horizontal share {share:.2f}")
    print(f"  cost {cost0:.1f} → {cost1:.1f}; rms ln residual {np.sqrt(np.average(r0 ** 2, weights=fit.w)):.3f} → "
          f"{np.sqrt(np.average(r1 ** 2, weights=fit.w)):.3f}")
    FH.P.update_fit(args.profile, {"body_modes": {"modes": modes, "h_share": share, "f_range": list(F_RANGE), "bands": bands,
                                                  "cost": [cost0, cost1]}})
    print(f"wrote the body_modes block of {FH.P.fit_path(args.profile)}")


if __name__ == "__main__":
    main()
