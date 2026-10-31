#!/usr/bin/env python3
"""The listening demo (demo.py: same notes, timing and dynamics, with a crude fade at
key release; no pedal, no silent keys), played from the original recordings, for A/B
comparison with the synthesis.

Local listening only: the output contains the third-party recordings and must not be
distributed (see data/SOURCES.md). Notes without a recording (Salamander has every
third note) are pitch-shifted from the nearest one, as a sampler would; Iowa labels are
corrected by the measured pitch (analysis label_mismatch).

Usage: sample_demo.py --profile grand_b
       sample_demo.py --profile flemish4 --by-key --file-level -o out/x.wav
  --by-key: samples by the key played (the 4′: each sounds an octave above its key)
  --file-level: at the files' level + SFZ volume, not normalized (to layer registers)
"""
import argparse
import json
import os
from fractions import Fraction
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

import demo
import fit_hammer as FH
import project as P  # noqa: E402  (on the path through fit_hammer)
import fit_coupling as FC

SR = demo.SR
LEAD_S = 0.03
HERE = os.path.dirname(os.path.abspath(__file__))


def sample_map(dataset, by_key=False):
    """(true note, dynamic) → analysis sample, from the Phase 0 analysis; by_key: (key
    played, dynamic)."""
    raw = P.load_cache(dataset, "analysis")["samples"]
    keys = FH.D.dynamics(dataset)
    m = {}
    for s in raw:
        if "error" in s:
            continue
        note = s.get("key", s["note"]) if by_key else s.get("label_mismatch", s["note"])
        for dyn, key in keys.items():
            if FH.D.layer_key(dataset, s) == key:
                m[(note, dyn)] = s
    return m


def play(args):
    dataset, note, dyn, hold, by_key, file_level = args
    m = sample_map(dataset, by_key)
    # a dynamic the set lacks (the upright has no pp): the nearest recorded one
    order = ["pp", "mf", "ff"]
    have = sorted({d for (_, d) in m}, key=order.index)
    dyn = min(have, key=lambda d: abs(order.index(d) - order.index(dyn)))
    avail = [n for (n, d) in m if d == dyn]
    src = min(avail, key=lambda n: (abs(n - note), n))
    s = m[(src, dyn)]
    x, sr = sf.read(FH.D.sample_path(dataset, s), always_2d=True)
    x = x * 10 ** (-s.get("gain_db", 0.0) / 20)   # the layer's own recording gain (datasets)
    if file_level:
        x = x * 10 ** (s.get("sfz_volume_db", 0.0) / 20)
    # start LEAD_S before the detected onset (bass onsets are detected a few ms late; a
    # hard cut into the rising attack made clicks) with a short fade-in over the silence
    x = x[max(int((s["onset_s"] - LEAD_S) * sr), 0):].T
    fi = int(0.01 * sr)
    x[:, :fi] *= np.linspace(0, 1, fi) ** 2
    # pitch shift by resampling (also converts 44.1 → 48 kHz)
    ratio = Fraction(sr / SR * 2 ** ((note - src) / 12)).limit_denominator(2000)
    x = resample_poly(x, ratio.denominator, ratio.numerator, axis=1)
    n = int((hold + 0.3) * SR)
    y = np.zeros((2, n))
    y[:, : min(n, x.shape[1])] = x[:, :n]
    fade = int(0.25 * SR)
    y[:, -fade:] *= np.linspace(1, 0, fade) ** 2
    return y


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    ap.add_argument("--by-key", action="store_true", help="samples by the key played, not the sounding note")
    ap.add_argument("--file-level", action="store_true", help="the files' level + SFZ volume, not normalized")
    ap.add_argument("-o", "--out", help="output path (default out/demo_<profile>_recordings.wav)")
    args = ap.parse_args()
    events, _, t_end = demo.demo_events()
    events = [e for e in events if e[2] is not None]   # silent key presses: nothing to play
    with Pool() as pool:
        notes = pool.map(play, [(args.profile, n, d, h, args.by_key, args.file_level) for _, n, d, h in events])
    mix = np.zeros((2, int(t_end * SR)))
    for (t, *_), y in zip(events, notes):
        i = int(max(t - LEAD_S, 0) * SR)                 # align the onsets
        mix[:, i:i + y.shape[1]] += y[:, : mix.shape[1] - i]
    out = args.out or os.path.join(HERE, "out", f"demo_{args.profile}_recordings.wav")
    if args.file_level:
        sf.write(out, mix.T, SR, subtype="FLOAT")
        print(f"wrote {out} ({mix.shape[1] / SR:.1f} s, peak {20 * np.log10(np.abs(mix).max()):.1f} dBFS)")
    else:
        demo.write_mix(out, mix)


if __name__ == "__main__":
    main()
