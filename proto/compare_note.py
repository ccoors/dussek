#!/usr/bin/env python3
"""Render a note with the prototype model and compare it with measured recordings,
using the same analysis code that produced the measurements.

Usage: compare_note.py --profile grand_a --sets grand_a,grand_b [--note 60] [-o proto/out]
"""
import argparse
import json
import os
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import scale  # noqa: E402
import project as P  # noqa: E402
import string_model as M  # noqa: E402

import datasets as D  # noqa: E402

# the dynamics compared (each set's own layer for it, datasets.dynamics), and the hammer speeds
# (m/s) the synthesis uses for them
LAYERS = [("pp", 0.4), ("mf", 1.5), ("ff", 5.0)]
COLORS = ("tab:orange", "tab:blue", "tab:green", "tab:red")
DB_PER_NEPER = 20 / np.log(10)


def load_measured(note, sets):
    """{(set, dynamic): sample} of the note in each set's analysis."""
    out = {}
    for ds in sets:
        for s in P.load_cache(ds, "analysis")["samples"]:
            if s["note"] == note and "error" not in s and "label_mismatch" not in s:
                d = D.dynamic_of(ds, s)
                if d:
                    out[(ds, d)] = s
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--note", type=int, default=60)
    ap.add_argument("--profile", required=True, help="the part whose model renders the note")
    ap.add_argument("--sets", required=True, help="comma-separated sources to compare against")
    ap.add_argument("-o", "--outdir", default=os.path.join(HERE, "out"))
    args = ap.parse_args()
    os.makedirs(args.outdir, exist_ok=True)

    sets = args.sets.split(",")
    meas = load_measured(args.note, sets)
    params = scale.note_params(scale.Profile(args.profile), args.note)
    print(f"note {args.note} ({args.profile}): f0 {params.f0:.2f} Hz, B {params.B:.2e}, "
          f"{params.n_strings} strings, L {params.L:.2f} m, b1 {params.b1:.2f}, b3 {params.b3:.2e}, "
          f"κ {params.kappa:.2f}, felt p {params.felt_p:.2f}")

    sr = 48000
    synth = {}
    for name, v in LAYERS:
        y, info = M.render(params, v, sr=sr, duration=8.0)
        synth[name] = y
        print(f"  {name}: v = {v} m/s, contact {info['contact_ms']:.2f} ms, peak force {info['peak_force_N']:.1f} N")
    # one common gain so that relative levels between velocities are preserved
    gain = 0.9 / max(np.abs(y).max() for y in synth.values())
    results = {}
    for name, y in synth.items():
        path = os.path.join(args.outdir, f"note{args.note}_{name}.wav")
        sf.write(path, (y * gain)[:, None], sr, subtype="FLOAT")
        results[name] = A.analyze({"path": path, "note": args.note, "layer": 0, "layers": 0,
                                   "dynamic": name, "tail_cut_s": 0.0})

    # ---- plot: partial levels and decay rates, synthesis vs. measurements
    fig, ax = plt.subplots(3, 3, figsize=(15, 11), sharex=True)
    for col, (name, v) in enumerate(LAYERS):
        series = [("synth", results[name], "k", "o-")]
        for ds, color in zip(sets, COLORS):
            s = meas.get((ds, name))
            if s:
                series.append((f"{ds} {name}", s, color, "s--"))
        for label, s, color, style in series:
            ps = [p for p in s["partials"] if p["n"] <= 30]
            ax[0, col].plot([p["n"] for p in ps], [p["level_db"] for p in ps], style,
                            color=color, ms=3, lw=1, label=label)
            pd = [p for p in ps if p.get("decay_db_s", 0) > 0 and p["conf"] > 0.3]
            ax[1, col].plot([p["n"] for p in pd], [p["decay_db_s"] for p in pd], style,
                            color=color, ms=3, lw=1, label=label)
        ax[0, col].set(title=f"{name} (synth: {v} m/s) — partial level", ylabel="dB re file peak")
        ax[1, col].set(title="decay rate (conf > 0.3)", ylabel="dB/s", xlabel="partial n", yscale="log")
        for a in ax[:, col]:
            a.grid(alpha=0.3)
            a.legend(fontsize=8)
    # level change between dynamics per partial: soundboard, room and microphones are
    # linear filters and cancel out, so this isolates the hammer's nonlinearity
    pairs = [("mf", "pp"), ("ff", "mf"), ("ff", "pp")]
    def levels(s):
        return {p["n"]: p["level_db"] + s["peak_dbfs"] for p in s["partials"] if p["n"] <= 30}
    for col, (hi, lo) in enumerate(pairs):
        cur = ax[2, col]
        cands = [("synth", results[hi], results[lo], "k", "o-")]
        for ds, color in zip(sets, COLORS):
            s_hi, s_lo = meas.get((ds, hi)), meas.get((ds, lo))
            if s_hi and s_lo:
                cands.append((ds, s_hi, s_lo, color, "s--"))
        for label, s_hi, s_lo, color, style in cands:
            L_hi, L_lo = levels(s_hi), levels(s_lo)
            ns = sorted(set(L_hi) & set(L_lo))
            d = np.array([L_hi[n] - L_lo[n] for n in ns])
            cur.plot(ns, d - d[0] if len(d) else d, style, color=color, ms=3, lw=1, label=label)
        cur.set(title=f"{hi} − {lo}: level change, relative to partial 1", ylabel="dB", xlabel="partial n")
        cur.grid(alpha=0.3)
        cur.legend(fontsize=8)
    fig.tight_layout()
    out = os.path.join(args.outdir, f"note{args.note}_compare.png")
    fig.savefig(out, dpi=100)
    print(f"wrote {out} and WAVs in {args.outdir}")


if __name__ == "__main__":
    main()
