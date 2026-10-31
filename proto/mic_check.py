#!/usr/bin/env python3
"""Microphone positions through the engine: a chromatic scale (mf, 0.3 s apart, held
0.25 s) per position, measured per note over its first 0.25 s:
  - level re the recording position (total level kept: should stay within a few dB),
  - left/right level difference R − L (dB) for the bass (≤ 47), middle and treble (≥ 72),
  - spectral centroid re the recording position (%), per register (bass ≤ 47, middle,
    treble ≥ 72; a keyboard-wide median hid the grand's documented +15 % mid register),
  - note-to-note unevenness: std of the level's deviation from its 5-note running median (dB),
  - clarity C50 (energy 0–50 ms vs after, dB), the direct-to-room proxy, at the
    default distance and at 1, 3.5, 10 m for player/audience.
Also writes the demo per position to proto/out/<profile-dir>/mics/.

Usage: mic_check.py [--profile upright_a]
"""
import argparse
import json
import os
import subprocess
import tempfile

import numpy as np
import soundfile as sf
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analysis"))
import project as P  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..")
RENDER = os.path.join(ROOT, "engine", "target", "release", "render")
NAMES = {0: "recording", 1: "player", 2: "audience", 3: "close1", 4: "close2"}
STEP, WIN = 0.3, 0.25


def render(profile, events_path, out, tuning):
    env = dict(os.environ, RENDER_TUNING=tuning)
    subprocess.run([RENDER, os.path.join(ROOT, "instruments", profile), events_path, out],
                   check=True, env=env, stdout=subprocess.DEVNULL)
    x, sr = sf.read(out)
    return x, sr


def scale_events(profile):
    inst = json.load(open(os.path.join(ROOT, "instruments", profile, "instrument.json")))
    mf = inst["velocity"]["speed"][1]
    notes = list(range(21, 109))
    ev = [{"t": i * STEP, "note": n, "speed": mf, "hold": WIN} for i, n in enumerate(notes)]
    return notes, {"profile": profile, "sr": 48000, "t_end": len(notes) * STEP + 2.0, "pedal": [], "events": ev}


def per_note(x, sr, notes):
    rows = []
    for i, n in enumerate(notes):
        a = int(i * STEP * sr)
        seg = x[a:a + int(WIN * sr)]
        lv = 10 * np.log10(np.mean(seg ** 2, axis=0) + 1e-20)          # per channel
        mono = seg.mean(axis=1)
        S = np.abs(np.fft.rfft(mono * np.hanning(len(mono)))) ** 2
        f = np.fft.rfftfreq(len(mono), 1 / sr)
        e50 = np.sum(mono[: int(0.05 * sr)] ** 2)
        rows.append((n, 10 * np.log10(np.mean(seg ** 2) + 1e-20), lv[1] - lv[0],
                     float((S * f).sum() / S.sum()), 10 * np.log10(e50 / max(np.sum(mono ** 2) - e50, 1e-20))))
    return np.array(rows)


def unevenness(lv):
    med = np.array([np.median(lv[max(i - 2, 0):i + 3]) for i in range(len(lv))])
    return float(np.std(lv - med))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True)
    args = ap.parse_args()
    notes, ev = scale_events(args.profile)
    tmp = tempfile.mkdtemp()
    ev_path = os.path.join(tmp, "scale.json")
    json.dump(ev, open(ev_path, "w"))
    res = {}
    for m in NAMES:
        x, sr = render(args.profile, ev_path, os.path.join(tmp, f"s{m}.wav"), f"mic={m}")
        res[m] = per_note(x, sr, notes)
    ref = res[0]
    print(f"{args.profile}: chromatic scale, mf; per position vs recording")
    print("position    level dB  R−L bass/mid/treble dB   centroid % bass/mid/treble   unevenness dB   C50 dB")
    regs = [ref[:, 0] <= 47, (ref[:, 0] > 47) & (ref[:, 0] < 72), ref[:, 0] >= 72]
    for m, r in res.items():
        print(f"{NAMES[m]:10s} {np.median(r[:, 1] - ref[:, 1]):+8.1f}  "
              + " / ".join(f"{np.median(r[g, 2]):+5.1f}" for g in regs)
              + "      " + " / ".join(f"{100 * (np.median(r[g, 3] / ref[g, 3]) - 1):+5.1f}" for g in regs)
              + f"        {unevenness(r[:, 1]):5.1f}       {np.median(r[:, 4]):+5.1f}")
    for m in (1, 2):
        cs = []
        for d in (1.0, 3.5, 10.0):
            x, sr = render(args.profile, ev_path, os.path.join(tmp, "d.wav"), f"mic={m},distance={d}")
            cs.append(f"{d:g} m: {np.median(per_note(x, sr, notes)[:, 4]):+5.1f}")
        print(f"C50 over distance, {NAMES[m]}: " + ", ".join(cs))
    out_dir = os.path.join(HERE, "out", args.profile, "mics")
    os.makedirs(out_dir, exist_ok=True)
    demo = P.cache_path(args.profile, "demo_events")
    for m, name in NAMES.items():
        render(args.profile, demo, os.path.join(out_dir, f"demo_{name}.wav"), f"mic={m}")
    print(f"demos in {out_dir}")


if __name__ == "__main__":
    main()
