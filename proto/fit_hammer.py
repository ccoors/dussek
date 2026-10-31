#!/usr/bin/env python3
"""Fit hammer felt parameters and hammer speeds to the measured change of the spectrum
between dynamics.

Target: for each partial n, the level change between two dynamics relative to the
fundamental, ΔL_n − ΔL_1. Soundboard, room and microphones are linear filters and
cancel out in this difference, so it isolates the nonlinear hammer–string interaction.

Felt stiffness is parametrized as the force at 0.5 mm compression (F05), so that the
exponent p and the stiffness can be varied independently:  K = F05 / (0.5 mm)^p.

Usage: fit_hammer.py [--note 60]
"""
import argparse
import itertools
import json
import os
import sys
import tempfile
from multiprocessing import Pool

import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import datasets as D  # noqa: E402
import project as P  # noqa: E402
import scale  # noqa: E402

P_GRID = [1.6, 2.0, 2.3, 2.6, 3.0, 3.5, 4.0, 4.5]
F05_FACTORS = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0]       # relative to the design curve scale.felt()
# hammer speeds per dynamic, fitted at C4 (same player for all notes)
V_GRID = {"pp": [0.4], "mf": [1.5], "ff": [5.0]}
N_MAX = 20


MIN_RANGE_DB = 10.0


def partial_levels(s, measured=True):
    """Absolute partial levels (dBFS) by partial number. For recordings, only partials at
    least MIN_RANGE_DB above the noise floor: a partial buried in noise reads as the
    noise level, which understates the level change between dynamics. Synthetic signals
    have no noise floor (their "floor" estimate is just the signal tail), so no filter."""
    return {p["n"]: p["level_db"] + s["peak_dbfs"] for p in s["partials"]
            if p["n"] <= N_MAX and (not measured or p["range_db"] >= MIN_RANGE_DB)}


def rel_change(hi, lo):
    """ΔL_n − ΔL_1 for partials present in both."""
    ns = sorted(set(hi) & set(lo))
    if 1 not in ns:
        return {}
    return {n: (hi[n] - lo[n]) - (hi[1] - lo[1]) for n in ns if n > 1}


_profiles = {}


def render_levels(args):
    dataset, note, p_exp, f05, v = args
    if dataset not in _profiles:
        _profiles[dataset] = scale.Profile(dataset)
    params = scale.note_params(_profiles[dataset], note, felt_p=p_exp, felt_K=f05 / 5e-4 ** p_exp)
    y, _ = scale.M.render(params, v, duration=2.0)
    fd, path = tempfile.mkstemp(suffix=".wav")
    os.close(fd)
    # fixed gain (not normalized), so absolute level differences between renders survive
    sf.write(path, (y * 1e-3)[:, None], 48000, subtype="FLOAT")
    r = A.analyze({"path": path, "note": note, "layer": 0, "layers": 0, "dynamic": "", "tail_cut_s": 0.0})
    os.remove(path)
    return (p_exp, f05, v), partial_levels(r, measured=False)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--note", type=int, default=60)
    ap.add_argument("--profile", required=True, choices=D.ids())
    ap.add_argument("--p", type=float, nargs="*", help="fix the felt exponent(s) instead of the grid")
    ap.add_argument("--fine", action="store_true", help="finer stiffness grid (factor √2 steps)")
    ap.add_argument("--free-v", action="store_true", help="also fit the hammer speeds per dynamic")
    args = ap.parse_args()
    global P_GRID, F05_FACTORS, V_GRID
    if args.free_v:
        V_GRID = {"pp": [0.2, 0.3, 0.4, 0.6, 0.9], "mf": [0.8, 1.1, 1.5, 2.1, 3.0], "ff": [2.5, 3.5, 5.0, 7.0]}
    if args.p:
        P_GRID = args.p
    if args.fine:
        F05_FACTORS = [2 ** (i / 2) for i in range(-4, 11)]

    meas = {}
    for ds, keys in [(args.profile, D.dynamics(args.profile))]:
        samples = P.load_cache(ds, "analysis")["samples"]
        for s in samples:
            if s["note"] != args.note or "error" in s or "label_mismatch" in s:
                continue
            for dyn, key in keys.items():
                if D.layer_key(ds, s) == key:
                    meas[(ds, dyn)] = partial_levels(s)
    targets = {}
    for ds in [args.profile]:
        for hi, lo in (("mf", "pp"), ("ff", "pp")):
            if (ds, hi) in meas and (ds, lo) in meas:
                targets[(ds, hi, lo)] = rel_change(meas[(ds, hi)], meas[(ds, lo)])

    velocities = sorted({v for vs in V_GRID.values() for v in vs})
    f05_design = scale.felt(args.note)[1]
    F05_GRID = [f05_design * x for x in F05_FACTORS]
    jobs = [(args.profile, args.note, p, f, v)
            for p, f, v in itertools.product(P_GRID, F05_GRID, velocities)]
    with Pool() as pool:
        levels = dict(pool.map(render_levels, jobs))

    results = []
    for p, f in itertools.product(P_GRID, F05_GRID):
        for vpp, vmf, vff in itertools.product(V_GRID["pp"], V_GRID["mf"], V_GRID["ff"]):
            synth = {"pp": levels[(p, f, vpp)], "mf": levels[(p, f, vmf)], "ff": levels[(p, f, vff)]}
            errs = {}
            for (ds, hi, lo), tgt in targets.items():
                sc = rel_change(synth[hi], synth[lo])
                common = sorted(set(sc) & set(tgt))
                if common:
                    errs[(ds, hi, lo)] = np.sqrt(np.mean([(sc[n] - tgt[n]) ** 2 for n in common]))
            total = np.mean(list(errs.values()))
            results.append((total, p, f, vpp, vmf, vff, errs))
    results.sort(key=lambda r: r[0])

    design = next(r for r in results if r[1] == min(P_GRID, key=lambda x: abs(x - scale.felt(args.note)[0]))
                  and abs(r[2] - f05_design) < 1e-9)
    print(f"note {args.note} ({args.profile}): design curve p {design[1]} F05 {design[2]:.1f} N -> rms {design[0]:.1f} dB")
    print("best fits (rms error of ΔL_n − ΔL_1 over partials 2..20):")
    for total, p, f, vpp, vmf, vff, errs in results[:5]:
        detail = "  ".join(f"{ds[:4]} {hi}-{lo} {e:.1f}" for (ds, hi, lo), e in errs.items())
        print(f"  {total:5.1f} dB   p {p}  F05 {f:5.1f} N (K {f / 5e-4 ** p:.3g})  "
              f"v pp/mf/ff {vpp}/{vmf}/{vff} m/s   [{detail}]")


if __name__ == "__main__":
    main()
