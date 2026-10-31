#!/usr/bin/env python3
"""Damper decay from a set's release recordings: the sound of a ringing note from the moment its
key is released, i.e. while the damper stops the strings. The source names them
(`dampers = { kind = "sfz", file = … }`: the release regions of that SFZ, keyed by their
pitch_keycenter, the first round robin).

Per partial: the first fall, the slope of its envelope from its peak over its first FALL_DEPTH_DB
(short windows, 2.5 ms hops). The damper acts there; after it the recording's room rings on, under
the strings, and an energy-decay slope over 20 dB read the room's share (2–3× too slow: design log
step 119). Partials with less than MIN_RANGE_DB above their floor are left out.
Writes the part's damper cache (damper.json): rows (note, layer, n, freq, fall dB/s, conf, range dB).

Usage: damper_decay.py <part>
"""
import argparse
import os
from multiprocessing import Pool

import numpy as np
import soundfile as sf

import analyze as A
import datasets as D
import mapping as M
import project as P

HOP_S = 0.0025
FALL_DEPTH_DB = 12.0
# the fall read at least 10 dB above the floor (a room or noise under the strings bends it)
MIN_RANGE_DB = FALL_DEPTH_DB + 10.0
# the peak lies within this of the file's start; a fall slower than over MAX_FALL_S is no damper's
PEAK_WITHIN_S = 0.15
MAX_FALL_S = 0.6


def envelopes(x, sr, freqs, f0):
    """Per-partial magnitude envelopes (power-summed over channels), windows of 2 periods."""
    n = int(np.clip(2.0 / f0, 0.012, 0.05) * sr)
    hop = int(HOP_S * sr)
    nfft = 1 << int(np.ceil(np.log2(n * 2)))
    win = np.hanning(n)
    gain = win.sum() / 2
    bins = np.array(freqs) * nfft / sr
    frames = range(0, len(x) - n, hop)
    env = np.empty((len(freqs), len(frames)))
    for j, s in enumerate(frames):
        p = np.sqrt(((np.abs(np.fft.rfft(x[s:s + n] * win[:, None], nfft, axis=0)) / gain) ** 2).sum(axis=1))
        for i, b in enumerate(bins):
            k = int(round(b))
            env[i, j] = p[max(k - 1, 0):k + 2].max()
    return (np.array(frames) + n / 2) / sr, env


def falls(x, sr, key):
    """The first fall of each partial of a release recording (stereo array) of `key`."""
    est = A.estimate_partials(x, sr, A.find_onset(x), A.midi_to_hz(key))
    if est is None:
        return []
    ns = sorted(est["partials"])
    freqs = [est["partials"][n] for n in ns]
    t, env = envelopes(x, sr, freqs, est["f0"])
    out = []
    for i, n in enumerate(ns):
        e = 20 * np.log10(env[i] + 1e-12)
        ipk = int(np.argmax(e[:int(PEAK_WITHIN_S / HOP_S)]))
        floor = float(np.median(e[-int(0.3 / HOP_S):]))
        rng = e[ipk] - floor
        if rng < MIN_RANGE_DB:
            continue
        below = np.nonzero(e[ipk:] < e[ipk] - FALL_DEPTH_DB)[0]
        if not len(below) or below[0] < 4 or t[ipk + below[0]] - t[ipk] > MAX_FALL_S:
            continue
        j = ipk + below[0]
        slope, _ = np.polyfit(t[ipk:j + 1], e[ipk:j + 1], 1)
        if slope >= 0:
            continue
        out.append({"n": n, "freq": freqs[i], "fall_db_s": float(-slope), "range_db": float(rng),
                    "conf": float(np.clip((rng - MIN_RANGE_DB) / 30, 0.05, 1))})
    return out


def release_items(sid):
    """(path, key, layer) of the source's release recordings."""
    spec = P.source(sid).get("dampers") or {}
    if spec.get("kind") != "sfz":
        raise SystemExit(f"{sid}: no release recordings for the dampers (a source's dampers = {{ kind = \"sfz\", file = … }})")
    root = D.root(sid)
    regions = [r for r in D.parse_sfz(os.path.join(root, spec["file"]))
               if r.get("trigger") == "release" and "sample" in r and str(r.get("seq_position", "1")) == "1"]
    key = lambda k: int(k) if str(k).lstrip("-").isdigit() else M.note_name(str(k))  # noqa: E731
    return [(os.path.join(root, r["sample"].replace("\\", "/")), key(r["pitch_keycenter"]), int(r.get("lovel", 0)))
            for r in regions]


def work(item):
    path, key, layer = item
    x, sr = sf.read(path, always_2d=True)
    return [(key, layer, r["n"], r["freq"], r["fall_db_s"], r["conf"], r["range_db"]) for r in falls(x, sr, key)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("part", help="the part whose source names its release recordings")
    args = ap.parse_args()
    items = release_items(args.part)
    with Pool() as pool:
        rows = [r for rs in pool.map(work, items) for r in rs]
    P.write_cache(args.part, "damper", {"columns": ["note", "layer", "n", "freq", "fall_db_s", "conf", "range_db"],
                                        "rows": rows})
    print(f"wrote {P.cache_path(args.part, 'damper')}: {len(rows)} partials from {len(items)} files")


if __name__ == "__main__":
    main()
