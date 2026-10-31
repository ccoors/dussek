#!/usr/bin/env python3
"""Summarize analysis results: label mismatches, and overview plots across the keyboard.

Usage: summarize.py <analysis.json> <analysis.json> [-o proto/out]   (projects/<name>/cache/<source>/analysis.json)
"""
import argparse
import json
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def note_name(n):
    return f"{NAMES[n % 12]}{n // 12 - 1}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="+")
    ap.add_argument("-o", "--outdir", default="proto/out")
    args = ap.parse_args()

    data = {}
    for f in args.files:
        d = json.load(open(f))
        data[d["dataset"]] = [s for s in d["samples"] if "error" not in s]

    for name, samples in data.items():
        bad = [s for s in samples if "label_mismatch" in s]
        print(f"{name}: {len(samples)} analyzed, {len(bad)} label mismatches")
        for s in bad[:15]:
            print(f"  {s['file']:22s} labeled {note_name(s['note']):4s} measures as "
                  f"{note_name(s['label_mismatch'])} ({s['tuning_cents']:+.0f} c)")

    fig, ax = plt.subplots(2, 2, figsize=(13, 8))
    colors = ("tab:orange", "tab:blue", "tab:green", "tab:red")
    for (name, samples), color in zip(data.items(), colors):
        label = name
        ok = [s for s in samples if "label_mismatch" not in s]
        top = max(s["layer"] for s in ok)
        loud = [s for s in ok if s["layer"] == top]

        # inharmonicity, sized by confidence
        n = np.array([s["note"] for s in ok]); B = np.array([s["B"] for s in ok])
        c = np.array([s["B_conf"] for s in ok])
        ax[0, 0].scatter(n, B, s=4 + 30 * c, alpha=0.5, color=color, label=label)

        # tuning deviation (stretch), loudest layer only
        ax[0, 1].scatter([s["note"] for s in loud], [s["tuning_cents"] for s in loud],
                         s=10, color=color, label=label)

        # T60 of the fundamental, loudest layer, only confident fits
        pts = [(s["note"], 60 / p["decay_db_s"]) for s in loud for p in s["partials"]
               if p["n"] == 1 and p.get("decay_db_s", 0) > 0 and p["conf"] > 0.3]
        if pts:
            ax[1, 0].scatter(*zip(*pts), s=10, color=color, label=label)

        # usable dynamic range of the fundamental, all layers
        pts = [(s["note"], s["usable_range_db_p1"]) for s in ok if s.get("usable_range_db_p1")]
        ax[1, 1].scatter(*zip(*pts), s=4, alpha=0.5, color=color, label=label)

    ax[0, 0].set(yscale="log", title="Inharmonicity B (marker size = confidence)", ylabel="B")
    ax[0, 1].set(title="Tuning deviation from equal temperament (loudest layer)", ylabel="cents")
    ax[0, 1].axhline(0, color="k", lw=0.5)
    ax[1, 0].set(yscale="log", title="T60 of fundamental (loudest layer, conf > 0.3)", ylabel="seconds")
    ax[1, 1].set(title="Usable dynamic range of fundamental (all layers)", ylabel="dB above noise floor")
    ticks = list(range(21, 109, 12))
    for a in ax.flat:
        a.set_xticks(ticks, [note_name(t) for t in ticks])
        a.grid(alpha=0.3)
        a.legend(fontsize=8)
    fig.tight_layout()
    out = os.path.join(args.outdir, "overview.png")
    fig.savefig(out, dpi=110)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
