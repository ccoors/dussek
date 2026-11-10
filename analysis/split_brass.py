#!/usr/bin/env python3
"""Cuts a brass project's Iowa runs into one file per note (data/iowa-mis-brass/notes/<stem>/<dyn>/<midi>.wav),
once, as split_runs.py does the percussion's: the stretches above each run's floor, then the stretches
fitted to the notes by pitch (cutting in order lost notes on the horn and the bass). A run's range is
in its file's name. A note the fit does not find is left out and the rest stay on their notes. Beside each
note, <midi>.json says where it lies in its run: the fits measure the background in the silence around it.

    python3 analysis/split_brass.py [project …]   (default: every project with a [brass] table)
"""
import glob
import json
import os
import re
import sys

import numpy as np
import soundfile as sf
from scipy.signal import butter, sosfiltfilt

import project as P

IN_THE_NOTES, NOT_A_NOTE = 30.0, 40.0  # dB under the run's top
NAMES = {"C": 0, "Db": 1, "D": 2, "Eb": 3, "E": 4, "F": 5, "Gb": 6, "G": 7, "Ab": 8, "A": 9, "Bb": 10, "B": 11}


def midi_of(name):
    m = re.match(r"([A-G]b?)(\d)", name)
    return 12 * (int(m[2]) + 1) + NAMES[m[1]]


def runs(root, stem):
    """(path, dynamics, first key, last key) of a stem's runs, in order of their names."""
    for path in sorted(glob.glob(os.path.join(glob.escape(root), "*.aif*"))):
        m = re.fullmatch(re.escape(stem) + r"\.(pp|mf|ff)\.\s*([A-G]b?\d)([A-G]b?\d)?\.aiff?", os.path.basename(path))
        if m:
            lo = midi_of(m[2])
            yield path, m[1], lo, midi_of(m[3]) if m[3] else lo


def cuts(y, sr, hop=0.01):
    """The stretches above the silence, s: a chromatic run's loudness wanders by more than a threshold on its loudest can take. The
    silence is the quietest tenth; some runs leave 0.2 s between notes of 2.3 s, and a tenth close under the top with far quieter frames
    below it lies in the notes: there the quietest 2 %. A stretch far under the top is a sound before a note, not a note: on the note's
    pitch to the search, it would take the note's place."""
    n = int(hop * sr)
    e = np.sqrt(np.mean(y[: len(y) // n * n].reshape(-1, n) ** 2, axis=1))
    db = 20 * np.log10(e + 1e-9)
    floor, low, top = np.percentile(db[db > -150], 10), np.percentile(db[db > -150], 2), np.percentile(db, 97)
    if top - floor < IN_THE_NOTES and floor - low > 15.0:
        floor = low
    above = db > floor + min(15.0, 0.3 * (top - floor))
    segs, i = [], 0
    while i < len(above):
        if above[i]:
            b = i
            while b < len(above) and (above[b] or above[b:b + 5].any()):  # a gap under 50 ms is the same note
                b += 1
            if (b - i) * hop > 0.3 and db[i:b].max() > top - NOT_A_NOTE:
                segs.append((i * hop, b * hop))
            i = b + 1
        else:
            i += 1
    return segs


def pitch(p, sr, near):
    """The period by autocorrelation, Hz, from a fifth below `near` to an octave above."""
    s = p - p.mean()
    ac = np.fft.irfft(np.abs(np.fft.rfft(s, 2 * len(s))) ** 2)[: len(s)]
    lo, hi = max(2, int(sr / (near * 2.0))), min(len(s) - 2, int(sr / (near / 1.5)))
    if hi <= lo:
        return near
    i = lo + int(np.argmax(ac[lo:hi]))
    a, b, c = ac[i - 1:i + 2]
    d = 0.5 * (a - c) / (a - 2 * b + c) if a - 2 * b + c != 0 else 0.0
    return sr / (i + d)


def align(y, sr, segs, lo, hi):
    """Each stretch on a note, in order, on the pitch: the most stretches on their notes. A lost note shifts the rest only by one."""
    notes = list(range(lo, hi + 1))
    ok = np.zeros((len(segs), len(notes)), bool)
    for i, (a, b) in enumerate(segs):
        s = y[int((a + min(0.3, (b - a) / 3)) * sr):int(min(a + 0.9, b) * sr)]
        for j, m in enumerate(notes):
            f = 440 * 2 ** ((m - 69) / 12)
            if len(s) > max(sr // 8, int(4 / f * sr)):
                ok[i, j] = abs(1200 * np.log2(pitch(s, sr, f) / f)) < 50
    best = np.zeros((len(segs) + 1, len(notes) + 1), int)
    for i in range(len(segs) - 1, -1, -1):
        for j in range(len(notes) - 1, -1, -1):
            best[i, j] = max(best[i + 1, j], best[i, j + 1], best[i + 1, j + 1] + ok[i, j])
    pairs, i, j = [], 0, 0
    while i < len(segs) and j < len(notes):
        if ok[i, j] and best[i, j] == best[i + 1, j + 1] + 1:
            pairs.append((i, notes[j]))
            i, j = i + 1, j + 1
        elif best[i, j] == best[i + 1, j]:
            i += 1
        else:
            j += 1
    return pairs


def split(root, stem, hp, out):
    """A stem's runs under `root` into `out`/<stem>/<dyn>/<midi>.wav; returns the (dynamics, key) kept."""
    kept = []
    for path, dyn, lo, hi in runs(root, stem):
        x, sr = sf.read(path, always_2d=True)
        y = sosfiltfilt(butter(4, hp, "highpass", fs=sr, output="sos"), x.mean(axis=1))
        segs = cuts(y, sr)
        pairs = align(y, sr, segs, lo, hi)
        folder = os.path.join(out, stem, dyn)
        os.makedirs(folder, exist_ok=True)
        for k, midi in pairs:
            a, b = segs[k]
            sf.write(os.path.join(folder, f"{midi}.wav"), x[int(a * sr):int(b * sr)], sr, subtype="FLOAT")
            with open(os.path.join(folder, f"{midi}.json"), "w") as f:
                json.dump({"run": os.path.basename(path), "start": a, "end": b}, f)
            kept.append((dyn, midi))
        print(os.path.basename(path), f"{len(segs)} stretches for {hi - lo + 1} notes, {len(pairs)} kept", flush=True)
    return kept


def where(note, root):
    """A cut note's place: {file (its run under `root`), start, end (s)}."""
    with open(os.path.splitext(note)[0] + ".json") as f:
        cut = json.load(f)
    return {"file": os.path.join(root, cut["run"]), "start": cut["start"], "end": cut["end"]}


def main(names):
    for name in names or [n for n in P.index()[0] if "brass" in P.config(n)]:
        b = P.config(name)["brass"]
        root = os.path.join(P.REPO, "data", "iowa-mis-brass")
        kept = split(root, b["stem"], b["hp"], os.path.join(root, "notes"))
        print(name, len(kept), "notes", {d: sum(1 for x, _ in kept if x == d) for d in ("pp", "mf", "ff")})


if __name__ == "__main__":
    main(sys.argv[1:])
