#!/usr/bin/env python3
"""Lists the notes of the rendered brass pieces that sound off their written pitch: each stem's notes
(from its MIDI file) against its render (from mix.py), the strongest frequency within a minor third
of the note 250 ms in, the note settled (notes of 0.35 s and longer; earlier, a low note's attack and
the last note's ring fill the window).

    python3 albums/brass/notes.py [piece …] [--cents 40] [--variant LABEL]"""
import argparse
import os

import mido
import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
N = 4096
NAMES = "C C# D D# E F F# G G# A A# B".split()


def notes(path):
    """(start s, end s, key, velocity) of a monophonic stem."""
    out, on, now = [], {}, 0.0
    for msg in mido.MidiFile(path):
        now += msg.time
        if msg.type == "note_on" and msg.velocity > 0:
            on[msg.note] = (now, msg.velocity)
        elif msg.type in ("note_off", "note_on") and msg.note in on:
            t0, v = on.pop(msg.note)
            out.append((t0, now, msg.note, v))
    return sorted(out)


def cents(x, sr, t, key):
    f0 = 440 * 2 ** ((key - 69) / 12)
    i = int(t * sr)
    seg = x[i:i + N]
    if len(seg) < N:
        return None
    fr = np.fft.rfftfreq(4 * N, 1 / sr)
    s = np.abs(np.fft.rfft(seg * np.hanning(N), 4 * N))
    band = (fr > f0 * 2 ** (-3.5 / 12)) & (fr < f0 * 2 ** (3.5 / 12))
    k = int(np.argmax(np.where(band, s, 0)))
    if s[k] < 1e-6 * s.max() or k == 0:
        return None
    a, b, c = np.log(s[k - 1:k + 2] + 1e-12)
    d = 0.5 * (a - c) / (a - 2 * b + c)
    return 1200 * np.log2((fr[k] + d * (fr[1] - fr[0])) / f0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pieces", nargs="*")
    ap.add_argument("--cents", type=float, default=40.0)
    ap.add_argument("--variant", metavar="LABEL", help="the renders of mix.py --variant LABEL")
    args = ap.parse_args()
    pieces = args.pieces or sorted(d for d in os.listdir(os.path.join(HERE, "midi")) if os.path.isdir(os.path.join(HERE, "midi", d)))
    for piece in pieces:
        total = off = 0
        for f in sorted(os.listdir(os.path.join(HERE, "midi", piece))):
            stem = f[:-4]
            out = f"{piece}@{args.variant}" if args.variant else piece
            x, sr = sf.read(os.path.join(HERE, "out", out, f"{stem}.wav"), dtype="float64")
            x = x.mean(axis=1)
            bad = []
            for t0, t1, key, vel in notes(os.path.join(HERE, "midi", piece, f)):
                if t1 - t0 < 0.35:
                    continue
                total += 1
                late = cents(x, sr, t0 + 0.25, key)
                if late is not None and abs(late) > args.cents:
                    bad.append((t0, key, vel, late))
            off += len(bad)
            for t0, key, vel, late in bad:
                print(f"{piece} {stem:22s} {t0:6.2f} s  {NAMES[key % 12]}{key // 12 - 1:<2} v{vel:3d}  at 250 ms {late:+5.0f} c")
        print(f"{piece}: {off} of {total} notes over {args.cents:.0f} c")


if __name__ == "__main__":
    main()
