#!/usr/bin/env python3
"""Quality survey of a piano sample set before fitting it (analysis only).

Runs `analyze.analyze` on every note/velocity file and summarizes per register and
layer: duration, the fundamental's usable decay range and whether its decay reaches
the noise floor before the file ends, how the file ends (a fade-out or a cut while
still sounding), the noise floor re the peak, tuning, partials found, mislabeled notes.

Usage: survey.py <dataset> <root> [-o out.json]
"""
import argparse
import json
import os
import sys
from multiprocessing import Pool

import numpy as np
import soundfile as sf

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import analyze as A  # noqa: E402
import datasets  # noqa: E402

REGISTERS = [(21, 40, "bass A0–E2"), (41, 64, "mid F2–E4"), (65, 88, "upper F4–E6"), (89, 108, "treble F6–C8")]


def ending(path):
    """Level of the last 0.2 s re the 1 s before it (dB): about 0 when the file is cut
    while still sounding, strongly negative after a fade-out or a full decay."""
    x, sr = sf.read(path, always_2d=True)
    m = x.mean(axis=1)
    w = int(0.2 * sr)
    if len(m) < 6 * w:
        return None
    last = np.sqrt(np.mean(m[-w:] ** 2)) + 1e-12
    before = np.sqrt(np.mean(m[-6 * w:-w] ** 2)) + 1e-12
    return float(20 * np.log10(last / before))


def work(item):
    r = A.analyze(item)
    r["sfz_volume_db"] = item.get("sfz_volume_db", 0.0)
    r["ending_db"] = ending(item["path"])
    p1 = next((p for p in r.get("partials", []) if p["n"] == 1), None)
    r["p1"] = {k: p1.get(k) for k in ("range_db", "decay_db_s", "reached_floor", "floor_db", "edc_decay_db_s")} if p1 else None
    r["n_decays"] = sum(1 for p in r.get("partials", []) if p.get("conf", 0) > 0.3)
    r.pop("partials", None)
    return r


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dataset", choices=datasets.ids())
    ap.add_argument("root")
    ap.add_argument("-o", "--out")
    args = ap.parse_args()
    items = datasets.items(args.dataset)
    if not items:
        sys.exit("no files found")
    with Pool() as pool:
        rows = pool.map(work, items)
    if args.out:
        json.dump(rows, open(args.out, "w"), indent=1)

    sr = sorted({r["sr"] for r in rows})
    layers = sorted({r["dynamic"] for r in rows}, key=lambda d: min(r["layer"] for r in rows if r["dynamic"] == d))
    notes = sorted({r["note"] for r in rows})
    print(f"{args.dataset}: {len(rows)} files, {len(notes)} notes ({A_name(notes[0])}–{A_name(notes[-1])}), "
          f"layers {layers}, sample rates {sr}")
    errors = [r for r in rows if "error" in r]
    mism = [r for r in rows if "label_mismatch" in r]
    print(f"  analysis failed: {len(errors)}; label mismatches: "
          + (", ".join(f"{r['file']}→{A_name(r['label_mismatch'])}" for r in mism) or "none"))
    ok = [r for r in rows if "error" not in r and r["p1"]]
    print(f"  {'register':14s} {'layer':>6s} {'n':>3s} {'dur s':>6s} {'p1 range dB':>11s} {'p1 to floor':>11s}"
          f" {'end dB':>7s} {'floor dB':>8s} {'decays':>6s} {'tune ¢':>7s} {'peak dBFS':>9s} {'+sfz vol':>8s}")
    for lo, hi, name in REGISTERS:
        for d in layers:
            sel = [r for r in ok if lo <= r["note"] <= hi and r["dynamic"] == d]
            if not sel:
                continue
            med = lambda k: np.median([k(r) for r in sel])  # noqa: E731
            to_floor = np.mean([bool(r["p1"]["reached_floor"]) for r in sel])
            ends = [r["ending_db"] for r in sel if r["ending_db"] is not None]
            print(f"  {name:14s} {d:>6s} {len(sel):3d} {med(lambda r: r['duration_s']):6.1f}"
                  f" {med(lambda r: r['p1']['range_db']):11.0f} {100 * to_floor:10.0f}%"
                  f" {np.median(ends) if ends else float('nan'):7.1f} {med(lambda r: r['p1']['floor_db']):8.0f}"
                  f" {med(lambda r: r['n_decays']):6.0f} {med(lambda r: r['tuning_cents']):+7.1f}"
                  f" {med(lambda r: r['peak_dbfs']):9.1f} {med(lambda r: r['peak_dbfs'] + r['sfz_volume_db']):8.1f}")


def A_name(n):
    return ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][n % 12] + str(n // 12 - 1)


if __name__ == "__main__":
    main()
