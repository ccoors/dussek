#!/usr/bin/env python3
"""Renders an ensemble demo's stems with midi2wav, one instrument each, and mixes them: each stem's
gain and place in the stereo field, then one gain that puts the mix's peak at -1 dBFS.

    python3 albums/demos/mix.py                 # renders the stems, writes the mix
    python3 albums/demos/mix.py --mix-only      # the mix again from the rendered stems
    python3 albums/demos/mix.py --video         # also each stem's video, stacked into one

Writes albums/demos/out/05-tin-roof-parade/<stem>.wav and albums/demos/out/05-tin-roof-parade.wav;
with --video also <stem>.mp4 and albums/demos/out/05-tin-roof-parade.mp4: the stems' Visualization
panels in a grid, the piano large at the top left, the other instruments as tiles down the right
and along the bottom, the title in the corner."""
import argparse
import os
import subprocess
from concurrent.futures import ThreadPoolExecutor

import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
MIDI2WAV = os.path.join(ROOT, "engine", "target", "release", "midi2wav")

NAME = "05-tin-roof-parade"
RATE = 48000
PEAK_DB = -1.0
# every stem at the player's position, the bars spread over the 88 keys' frame as the piano's strings
TUNING = "mic=1,spread=1"
# stem: (instrument directory, midi2wav's --param, gain dB, pan -1 left … 1 right); the gains set
# against the stems' levels in the tutti: the vibraphone 8 dB under the piano, the harp 4 dB under
# (raised by ear), the marimba, kalimba and bells 7 dB under, the tubular bells' strokes 8 dB
STEMS = {
    "piano": ("grand_c", "pedal=0.1,voicing=0.05", 0.0, 0.0),   # Grand C: close and clear
    "vibraphone": ("vibraphone", "motor=1", -14.5, -0.35),
    "marimba": ("marimba", "mallet=0", -18.0, 0.3),           # yarn mallets
    "harp": ("harp", "", -5.5, -0.55),
    "kalimba": ("kalimba", "", -12.0, 0.55),
    "bells": ("bells", "mallet=0", 6.0, -0.15),              # plastic mallets
    "chimes": ("chimes", "", 3.5, 0.2),
}
# the video, 1920 x 1080 on a grid of 480 x 270: (stem, label, x, y, columns, rows); the piano
# 3 x 3 cells, the others one each; the title in the corner cell
CELL = (480, 270)
TILES = [("piano", "Piano", 0, 0, 3, 3),
         ("bells", "Bells", 3, 0, 1, 1), ("chimes", "Tubular bells", 3, 1, 1, 1),
         ("kalimba", "Kalimba", 3, 2, 1, 1), ("vibraphone", "Vibraphone", 0, 3, 1, 1),
         ("harp", "Harp", 1, 3, 1, 1), ("marimba", "Marimba", 2, 3, 1, 1)]
TITLE = ("Tin Roof Parade", 3, 3)
BACKGROUND = "0x0A0A16"   # the panels' own
# the stems' videos are encoded again in the stack: near lossless here
STEM_FFMPEG = "-c:v libx264 -preset fast -crf 10 -pix_fmt yuv420p -an"


def render(stem, out_dir, video=False):
    inst, params, _, _ = STEMS[stem]
    cmd = ["nice", "-n", "19", MIDI2WAV, os.path.join(ROOT, "instruments", inst),
           os.path.join(HERE, "midi", NAME, f"{stem}.mid"), os.path.join(out_dir, f"{stem}.wav"),
           "--rate", str(RATE), "--tail", "8", "--tuning", TUNING]
    if params:
        cmd += ["--param", params]
    if video:
        w, h = next((c * CELL[0], r * CELL[1]) for s, _, _, _, c, r in TILES if s == stem)
        cmd += ["--video", os.path.join(out_dir, f"{stem}.mp4"), "--size", f"{w}x{h}",
                "--ffmpeg", STEM_FFMPEG]
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)
    print(f"{stem}  [{inst}]", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mix-only", action="store_true")
    ap.add_argument("--video", action="store_true")
    args = ap.parse_args()
    out_dir = os.path.join(HERE, "out", NAME)
    os.makedirs(out_dir, exist_ok=True)
    if not args.mix_only:
        with ThreadPoolExecutor(4) as pool:
            list(pool.map(lambda s: render(s, out_dir, args.video), STEMS))
    stems = {s: sf.read(os.path.join(out_dir, f"{s}.wav"), dtype="float64")[0] for s in STEMS}
    mix = np.zeros((max(len(a) for a in stems.values()), 2))
    for stem, a in stems.items():
        _, _, gain_db, pan = STEMS[stem]
        g = 10 ** (gain_db / 20)
        # a balance pan: the far side down, the near side kept
        mix[: len(a), 0] += a[:, 0] * g * min(1.0, 1.0 - pan)
        mix[: len(a), 1] += a[:, 1] * g * min(1.0, 1.0 + pan)
        rms = 20 * np.log10(np.sqrt(np.mean(a[np.abs(a).max(axis=1) > 1e-4] ** 2)) + 1e-12)
        print(f"{stem:11s} {rms:6.1f} dB while sounding, {gain_db:+.1f} dB in the mix")
    mix *= 10 ** (PEAK_DB / 20) / np.abs(mix).max()
    path = os.path.join(HERE, "out", f"{NAME}.wav")
    sf.write(path, mix, RATE, subtype="PCM_24")
    print(f"{path}: {len(mix) / RATE:.1f} s")
    if args.video:
        stack(out_dir, path, len(mix) / RATE)


def stack(out_dir, audio, length):
    """The stems' videos in their tiles, each labeled at its top right, the title in its cell, the
    mix as the sound."""
    font = subprocess.run(["fc-match", "-f", "%{file}", "sans"], capture_output=True, text=True, check=True).stdout
    cmd = ["nice", "-n", "19", "ffmpeg", "-v", "error", "-y"]
    filters, layout = [], []
    for i, (stem, label, x, y, c, _) in enumerate(TILES):
        cmd += ["-i", os.path.join(out_dir, f"{stem}.mp4")]
        size = 26 if c > 1 else 18
        filters.append(f"[{i}:v]drawtext=fontfile='{font}':text='{label}':fontsize={size}:"
                       f"fontcolor=white@0.6:x=w-tw-14:y=10[t{i}]")
        layout.append(f"{x * CELL[0]}_{y * CELL[1]}")
    title, tx, ty = TITLE
    n = len(TILES)
    cmd += ["-f", "lavfi", "-i", f"color=c={BACKGROUND}:s={CELL[0]}x{CELL[1]}:r=60"]
    filters.append(f"[{n}:v]drawtext=fontfile='{font}':text='{title}':fontsize=36:fontcolor=white@0.85:"
                   f"x=(w-tw)/2:y=(h-th)/2[t{n}]")
    layout.append(f"{tx * CELL[0]}_{ty * CELL[1]}")
    tiles = "".join(f"[t{i}]" for i in range(n + 1))
    filters.append(f"{tiles}xstack=inputs={n + 1}:layout={'|'.join(layout)}[v]")
    out = os.path.join(HERE, "out", f"{NAME}.mp4")
    cmd += ["-i", audio, "-filter_complex", ";".join(filters), "-map", "[v]", "-map", f"{n + 1}:a",
            "-t", f"{length:.3f}", "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart", out]
    subprocess.run(cmd, check=True)
    print(f"{out}: {4 * CELL[0]}x{4 * CELL[1]}")

if __name__ == "__main__":
    main()
