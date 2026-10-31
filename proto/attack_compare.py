#!/usr/bin/env python3
"""Attack transient per band, synthesis vs recordings (analysis/attack_noise.py).

Prints the median band excess (dB re the steady power of the note; "–" = no excess)
per register and dynamic, recording vs synthesis.

Usage: attack_compare.py --profile grand_a
"""
import argparse
import json
import os
import sys
from multiprocessing import Pool

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import attack_noise as AN  # noqa: E402
import fit_hammer as FH  # noqa: E402
import project as P  # noqa: E402
import scale  # noqa: E402
import soundboard as SB  # noqa: E402

SHOW_HZ = [100, 200, 400, 800, 1600, 3200, 6400, 12800]
REGISTERS = {"bass ≤44": (21, 44), "middle 45–72": (45, 72), "treble ≥73": (73, 108)}
PRE_S = 0.1


def synth_excess(args):
    dataset, note, dyn = args
    os.environ["SOUNDBOARD_MODE"] = "full"
    prof = scale.Profile(dataset)
    y, _ = SB.render(prof, note, prof.hammer_speeds()[dyn], 0.4, stereo=True)
    x = np.concatenate([np.zeros((2, int(PRE_S * 48000))), y], axis=1).T
    return note, dyn, AN.onset_excess(x, 48000, A.find_onset(x) / 48000).tolist()


def table(rows, title):
    bands = np.array(AN.BANDS)
    idx = [int(np.argmin(abs(bands - f))) for f in SHOW_HZ]
    print(title)
    print(" " * 22 + "".join(f"{f / 1000:>7.1f}k" for f in SHOW_HZ))
    for reg, (lo, hi) in REGISTERS.items():
        for dyn in ("pp", "mf", "ff"):
            ex = np.array([[np.nan if v is None else v for v in e] for n, d, e in rows
                           if d == dyn and lo <= n <= hi])
            if not len(ex):
                continue
            cells = []
            for i in idx:
                col = ex[:, i]
                # median over notes, counting "no excess" as very low
                med = np.median(np.where(np.isnan(col), -99, col))
                cells.append(f"{med:8.0f}" if med > -90 else "       –")
            print(f"  {reg:13s} {dyn:3s}  " + "".join(cells))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    args = ap.parse_args()
    rec = P.load_cache(args.profile, "attack")["rows"]
    rec_rows = [(r["note"], r["dyn"], r["excess_db"]) for r in rec]
    jobs = [(args.profile, n, d) for n, d, _ in rec_rows]
    with Pool() as pool:
        syn_rows = pool.map(synth_excess, jobs)
    table(rec_rows, f"{args.profile}: attack excess, recordings (dB re steady power)")
    table(syn_rows, f"{args.profile}: attack excess, synthesis")


if __name__ == "__main__":
    main()
