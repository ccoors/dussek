#!/usr/bin/env python3
"""Renders the brass pieces' stems with midi2wav, one part in one mute each, and mixes them: each
stem's gain and place in the stereo field, then one gain that puts the mix's peak at -1 dBFS.

    python3 albums/brass/mix.py                  # every piece: renders the stems, writes the mixes
    python3 albums/brass/mix.py 03-back-alley-strut
    python3 albums/brass/mix.py --mix-only       # the mixes again from the rendered stems
    python3 albums/brass/mix.py --variant attack # with the instruments' variant exports (`<instrument>@attack`, the
                                                 # snapshots folder): out/<piece>@attack.wav

Writes albums/brass/out/<piece>/<stem>.wav and albums/brass/out/<piece>.wav. Every stem plays at
Audience (Weidinger's default) with the plugin's default MIDI ties (midi2wav's on a brass
instrument): CC1 air added over the keys', CC11 air, CC70 vibrato, CC21 growl, CC23 flutter, CC4 the
plunger, CC65 the glide held."""
import argparse
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
MIDI2WAV = os.path.join(ROOT, "engine", "target", "release", "midi2wav")

RATE = 48000
PEAK_DB = -1.0
TUNING = "mic=2"
MUTE = {"open": 0, "straight": 1, "cup": 2, "wah": 3, "wah_stem": 4, "bucket": 5, "plunger": 6}
# a part: (instrument directory, gain dB, pan -1 left … 1 right); the trumpets left, the trombones
# right, the horn and tuba near the middle, the lead a little over the others (the instruments
# already play at one loudness)
PARTS = {
    "trumpet1": ("trumpet", 0.0, -0.35), "trumpet2": ("trumpet", -2.0, -0.6), "horn": ("horn", -1.0, -0.1),
    "trombone": ("tenor_trombone", -1.0, 0.3), "bass_trombone": ("bass_trombone", -2.0, 0.55),
    "tuba": ("tuba", -1.0, 0.15),
}
PIECES = ["01-harbor-parade", "02-evening-chorale", "03-back-alley-strut"]


def stem_setup(stem):
    """A stem's part, its mute and what else it plays with (`trumpet1_air` is trumpet1, open)."""
    for mute in sorted(MUTE, key=len, reverse=True):
        if stem.endswith("_" + mute):
            return stem[: -len(mute) - 1], mute
    part = next((p for p in PARTS if stem == p or stem.startswith(p + "_")), None)
    if part is None:
        raise ValueError(f"no part for stem {stem}")
    return part, "open"


def instrument_dir(inst, variant):
    if not variant:
        return os.path.join(ROOT, "instruments", inst)
    sys.path.insert(0, os.path.join(ROOT, "proto"))
    import snapshot
    path = os.path.join(snapshot.snapshots_dir(), f"{inst}@{variant}")
    if not os.path.isdir(path):
        raise SystemExit(f"no variant {path}: export it first (proto/export_brass.py --variant {variant})")
    return path


def render(piece, stem, out_dir, variant=None):
    part, mute = stem_setup(stem)
    inst = PARTS[part][0]
    cmd = ["nice", "-n", "19", MIDI2WAV, instrument_dir(inst, variant),
           os.path.join(HERE, "midi", piece, f"{stem}.mid"), os.path.join(out_dir, f"{stem}.wav"),
           "--rate", str(RATE), "--tail", "4", "--tuning", TUNING, "--param", f"brass_mute={MUTE[mute]}"]
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)
    print(f"{piece}/{stem}  [{inst}, {mute}]", flush=True)


def mix(piece, stems, out_dir, name):
    audio = {s: sf.read(os.path.join(out_dir, f"{s}.wav"), dtype="float64")[0] for s in stems}
    out = np.zeros((max(len(a) for a in audio.values()), 2))
    for stem, a in audio.items():
        part, mute = stem_setup(stem)
        _, gain_db, pan = PARTS[part]
        g = 10 ** (gain_db / 20)
        # a balance pan: the far side down, the near side kept
        out[: len(a), 0] += a[:, 0] * g * min(1.0, 1.0 - pan)
        out[: len(a), 1] += a[:, 1] * g * min(1.0, 1.0 + pan)
        on = np.abs(a).max(axis=1) > 1e-4
        rms = 20 * np.log10(np.sqrt(np.mean(a[on] ** 2)) + 1e-12) if on.any() else -120.0
        print(f"  {stem:22s} {rms:6.1f} dB while sounding, peak {20 * np.log10(np.abs(a).max() + 1e-12):6.1f} dBFS")
    gain = 10 ** (PEAK_DB / 20) / np.abs(out).max()
    out *= gain
    path = os.path.join(HERE, "out", f"{name}.wav")
    sf.write(path, out, RATE, subtype="PCM_24")
    print(f"{path}: {len(out) / RATE:.1f} s, the mix {20 * np.log10(gain):+.1f} dB to its peak")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pieces", nargs="*", default=PIECES)
    ap.add_argument("--mix-only", action="store_true")
    ap.add_argument("--variant", metavar="LABEL", help="the instruments' variant exports <instrument>@LABEL")
    args = ap.parse_args()
    for piece in args.pieces:
        name = f"{piece}@{args.variant}" if args.variant else piece
        out_dir = os.path.join(HERE, "out", name)
        os.makedirs(out_dir, exist_ok=True)
        stems = sorted(f[:-4] for f in os.listdir(os.path.join(HERE, "midi", piece)) if f.endswith(".mid"))
        if not args.mix_only:
            with ThreadPoolExecutor(4) as pool:
                list(pool.map(lambda s: render(piece, s, out_dir, args.variant), stems))
        mix(piece, stems, out_dir, name)


if __name__ == "__main__":
    main()
