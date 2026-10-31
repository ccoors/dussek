#!/usr/bin/env python3
"""Decay of the sound *between* the string partials: soundboard + room response.

The hammer knock and the string attack excite the soundboard (and the room) broadband.
Away from the string partials, the recording then holds only that response, so its
decay rate per frequency band is the body/room decay time T60(f) that a radiation
impulse response needs.

Per recording (f0 high enough that the gaps between partials are wide): STFT with a
Blackman-Harris window (sidelobes < −92 dB, so partial leakage stays far below the
body response), bins within ±GUARD bins of any partial are masked, the remaining bins
are summed per 1/3-octave band. The decay rate is the slope of the energy decay curve
(Schroeder) over its first 15 dB, from the band's peak, limited to the part above the
noise floor. Result: median T60 per band over notes, written to the part's cache (body.json).

Usage: body_decay.py <profile>
"""
import argparse
import json
import os
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.signal import get_window

import analyze as A
import datasets as D
import project as P

HERE = os.path.dirname(os.path.abspath(__file__))
NFFT = 4096
HOP = 256
GUARD = 5                                  # bins around each partial (window main lobe: ±4)
BANDS = 50.0 * 2 ** (np.arange(0, 28) / 3)  # 1/3-octave centers, 50 Hz … ~32 kHz
MIN_F0 = 180.0
EDC_DB = 15.0


def band_decays(s):
    x, sr = sf.read(s["path"], always_2d=True)
    x = x.sum(axis=1)
    if s.get("tail_cut_s"):
        x = x[: len(x) - int(s["tail_cut_s"] * sr)]
    on = int(s["onset_s"] * sr)
    x = x[max(on - NFFT // 2, 0):]
    win = get_window("blackmanharris", NFFT)
    frames = (len(x) - NFFT) // HOP
    if frames < 50:
        return []
    idx = np.arange(NFFT)[None, :] + HOP * np.arange(frames)[:, None]
    P = np.abs(np.fft.rfft(x[idx] * win, axis=1)) ** 2
    freqs = np.fft.rfftfreq(NFFT, 1 / sr)
    bin_hz = sr / NFFT
    mask = np.ones(len(freqs), bool)
    n = np.arange(1, int(sr / 2 / s["f0"]) + 2)
    fp = n * s["f0"] * np.sqrt(1 + s["B"] * n ** 2)
    for f in fp[fp < sr / 2]:
        k = int(round(f / bin_hz))
        mask[max(k - GUARD, 0):k + GUARD + 1] = False
    mask[: int(30 / bin_hz)] = False
    t = np.arange(frames) * HOP / sr
    out = []
    for fc in BANDS:
        sel = mask & (freqs >= fc * 2 ** (-1 / 6)) & (freqs < fc * 2 ** (1 / 6))
        if sel.sum() < 3 or fc * 2 ** (1 / 6) > sr / 2:
            continue
        e = P[:, sel].sum(axis=1)
        floor = np.median(e[-max(frames // 10, 5):])
        i0 = int(np.argmax(e[: int(0.1 * sr / HOP) + 1]))     # band peak within 100 ms of onset
        if 10 * np.log10(e[i0] / max(floor, 1e-30)) < EDC_DB + 10:
            continue                                         # not enough range above the floor
        ee = np.maximum(e[i0:] - floor, 0)
        edc = 10 * np.log10(np.maximum(np.cumsum(ee[::-1])[::-1], 1e-30) / ee.sum())
        j = int(np.argmax(edc < -EDC_DB))
        if j < 4:
            continue
        slope = np.polyfit(t[i0:i0 + j] - t[i0], edc[:j], 1)[0]   # dB/s, negative
        if slope < 0:
            out.append((float(fc), float(-60 / slope), s["note"]))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dataset", choices=D.ids())
    args = ap.parse_args()
    data = P.load_cache(args.dataset, "analysis")
    # the loudest recordings (a set of many layers: from `loud_from_layer` on)
    lo = P.source(args.dataset).get("loud_from_layer")
    if lo:
        ff = lambda s: s["layer"] >= lo  # noqa: E731
    else:
        ff = lambda s: D.dynamic_of(args.dataset, s) == D.loudest(args.dataset)  # noqa: E731
    # each file's cut as the analysis made it (the set's own or the detected fade)
    tail = D.tail_cut(args.dataset)
    samples = [dict(s, tail_cut_s=s.get("tail_cut_s", tail), path=D.sample_path(args.dataset, s)) for s in data["samples"]
               if "error" not in s and "label_mismatch" not in s and ff(s) and s["f0"] >= MIN_F0]
    with Pool() as pool:
        rows = [r for rs in pool.map(band_decays, samples) for r in rs]
    res = {}
    for fc in BANDS:
        v = [t60 for f, t60, _ in rows if f == float(fc)]
        if len(v) >= 3:
            res[f"{fc:.0f}"] = {"t60_median_s": float(np.median(v)), "t60_q25_s": float(np.percentile(v, 25)),
                                "t60_q75_s": float(np.percentile(v, 75)), "n": len(v)}
            print(f"{fc:7.0f} Hz  T60 {np.median(v):6.3f} s  (IQR {np.percentile(v, 25):.3f}–"
                  f"{np.percentile(v, 75):.3f}, n={len(v)})")
    P.write_cache(args.dataset, "body", {"bands": res, "rows": rows}, indent=1)
    path = P.cache_path(args.dataset, "body")
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
