#!/usr/bin/env python3
"""Renders "The Crossing"'s stems with midi2wav, one instrument each, and mixes them: each stem's gain and
place in the stereo field, then one gain that puts the mix's peak at -1 dBFS.

    python3 albums/crossover/mix.py              # renders the stems, writes the mix
    python3 albums/crossover/mix.py --mix-only   # the mix again from the rendered stems

Writes albums/crossover/out/01-the-crossing/<stem>.wav and albums/crossover/out/01-the-crossing.wav. Dussek's
stems at the player's position (the ensemble demo's), Weidinger's at Audience (its default), each brass
stem in its mute (`<part>_<mute>`) with the plugin's default MIDI ties."""
import argparse
import os
import subprocess
from concurrent.futures import ThreadPoolExecutor

import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
MIDI2WAV = os.path.join(ROOT, "engine", "target", "release", "midi2wav")

NAME = "01-the-crossing"
RATE = 48000
PEAK_DB = -1.0
MUTE = {"open": 0, "straight": 1, "cup": 2, "wah": 3, "wah_stem": 4, "bucket": 5, "plunger": 6}
# stem: (instrument directory, midi2wav's --param, --tuning, gain dB, pan -1 left … 1 right); the keyboard
# instruments near the ensemble demo's (the piano down: it accompanies; the marimba up: it carries the groove),
# the brass as in the brass album (the trumpets left, the trombones right, the horn and the tuba near the middle)
KEYS = "mic=1,spread=1"
HORNS = "mic=2"
STEMS = {
    "piano": ("grand_c", "pedal=0.1,voicing=0.05", KEYS, -3.0, 0.05),
    "harp": ("harp", "", KEYS, -7.0, -0.5),
    "vibraphone": ("vibraphone", "motor=1", KEYS, -10.0, 0.4),
    "marimba": ("marimba", "mallet=0", KEYS, -10.0, 0.25),
    "bells": ("bells", "mallet=0", KEYS, 2.0, -0.2),
    "chimes": ("chimes", "", KEYS, 0.0, 0.15),
    "trumpet1": ("trumpet", "", HORNS, 0.0, -0.35),
    "trumpet2": ("trumpet", "", HORNS, -2.0, -0.6),
    "trumpet2_cup": ("trumpet", "", HORNS, 9.0, -0.6),   # the cup mute 11 dB under open
    "horn": ("horn", "", HORNS, -4.0, -0.1),   # mostly harmony under the trumpets
    "trombone": ("tenor_trombone", "", HORNS, -1.0, 0.3),
    "bass_trombone": ("bass_trombone", "", HORNS, -2.0, 0.55),
    "tuba": ("tuba", "", HORNS, -1.0, 0.15),
}


def mute_of(stem):
    return next((m for m in sorted(MUTE, key=len, reverse=True) if stem.endswith("_" + m)), "open")


def render(stem, out_dir):
    inst, params, tuning, _, _ = STEMS[stem]
    params = ",".join(p for p in (params, f"brass_mute={MUTE[mute_of(stem)]}" if tuning == HORNS else "") if p)
    cmd = ["nice", "-n", "19", MIDI2WAV, os.path.join(ROOT, "instruments", inst),
           os.path.join(HERE, "midi", NAME, f"{stem}.mid"), os.path.join(out_dir, f"{stem}.wav"),
           "--rate", str(RATE), "--tail", "8", "--tuning", tuning]
    if params:
        cmd += ["--param", params]
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)
    print(f"{stem}  [{inst}{', ' + mute_of(stem) if tuning == HORNS else ''}]", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mix-only", action="store_true")
    args = ap.parse_args()
    out_dir = os.path.join(HERE, "out", NAME)
    os.makedirs(out_dir, exist_ok=True)
    if not args.mix_only:
        with ThreadPoolExecutor(4) as pool:
            list(pool.map(lambda s: render(s, out_dir), STEMS))
    audio = {s: sf.read(os.path.join(out_dir, f"{s}.wav"), dtype="float64")[0] for s in STEMS}
    mix = np.zeros((max(len(a) for a in audio.values()), 2))
    for stem, a in audio.items():
        gain_db, pan = STEMS[stem][3:]
        g = 10 ** (gain_db / 20)
        # a balance pan: the far side down, the near side kept
        mix[: len(a), 0] += a[:, 0] * g * min(1.0, 1.0 - pan)
        mix[: len(a), 1] += a[:, 1] * g * min(1.0, 1.0 + pan)
        on = np.abs(a).max(axis=1) > 1e-4
        rms = 20 * np.log10(np.sqrt(np.mean(a[on] ** 2)) + 1e-12) if on.any() else -120.0
        print(f"  {stem:14s} {rms:6.1f} dB while sounding, {gain_db:+.1f} dB in the mix")
    gain = 10 ** (PEAK_DB / 20) / np.abs(mix).max()
    mix *= gain
    path = os.path.join(HERE, "out", f"{NAME}.wav")
    sf.write(path, mix, RATE, subtype="PCM_24")
    print(f"{path}: {len(mix) / RATE:.1f} s, the mix {20 * np.log10(gain):+.1f} dB to its peak")


if __name__ == "__main__":
    main()
