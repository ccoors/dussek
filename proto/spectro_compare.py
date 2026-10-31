#!/usr/bin/env python3
"""Whole-demo spectrograms, recordings above synthesis (linear 0–16 kHz, like an
Audacity spectrogram view), for checking attacks and broadband artifacts by eye.

Usage: spectro_compare.py --profile grand_b [--mode full] [--seconds 10]
"""
import argparse
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import soundfile as sf
from scipy.signal import spectrogram

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True)
    ap.add_argument("--mode", default="full")
    ap.add_argument("--seconds", type=float, default=10.0)
    args = ap.parse_args()
    fig, axs = plt.subplots(2, 1, figsize=(20, 9), sharex=True)
    for ax, tag in zip(axs, ("recordings", args.mode)):
        x, sr = sf.read(os.path.join(HERE, "out", f"demo_{args.profile}_{tag}.wav"))
        x = x.sum(axis=1)[: int(args.seconds * sr)]
        f, t, S = spectrogram(x, sr, nperseg=2048, noverlap=1792)
        S = 10 * np.log10(S + 1e-20)
        ax.pcolormesh(t, f / 1000, S, vmin=S.max() - 90, vmax=S.max(), shading="auto", cmap="magma")
        ax.set(ylim=(0, 16), ylabel="kHz", title=f"demo_{args.profile}_{tag}")
    axs[-1].set_xlabel("s")
    out = os.path.join(HERE, "out", f"spectro_{args.profile}_{args.mode}.png")
    fig.savefig(out, dpi=55, bbox_inches="tight")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
