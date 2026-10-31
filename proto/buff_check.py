#!/usr/bin/env python3
"""The buff stop against the English recordings: single notes on Harpsichord A (Unk) and
the Flemish 8′, rendered with the buff off and on (render, RENDER_PARAMS=buff=…), measured
as fit_buff measures the recordings (t₋₂₀dB per note, Δσ per partial). Per band, the
synthesized median Buff/Normal t₋₂₀dB ratio and median Δσ (partials 1–12) must lie within
the range of the English keys' values in that band (their spread is the tolerance).

Also writes listening demos: each instrument's demo phrase with the buff off and on, and
English Normal/Lute samples.

--dry: sympathetic resonance and room off (the strings alone; the default compares the full
render, as the recordings include the room and the instrument's own ringing strings).

Usage: buff_check.py [--dry]   (after cargo build --release)
"""
import json
import os
import shutil
import subprocess
import sys

import numpy as np
import soundfile as sf

import fit_buff as FB
import project as P  # noqa: E402  (on the path through fit_buff)

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..")
OUT = os.path.join(HERE, "out", "buff_dry" if "--dry" in sys.argv else "buff")
RENDER = os.path.join(REPO, "engine", "target", "release", "render")
BANDS = {"bass": (21, 44), "middle": (45, 64), "treble": (65, 108)}
HOLD_S, GAP_S = 4.0, 5.0
def targets():
    """The instruments with a buff stop ([uses] buff): instrument dir, the part whose demo events
    it plays, stops."""
    return {n: (n, P.main_part(n), 1) for n, c in P.index()[0].items() if c.get("uses", {}).get("buff")}


def band(n):
    return next(b for b, (lo, hi) in BANDS.items() if lo <= n <= hi)


DRY = "--dry" in sys.argv


def render(inst, events, out, buff, stops):
    params = f"buff={buff}" + (",sympathetic=0,room=0" if DRY else "")
    env = {**os.environ, "RENDER_PARAMS": params, "RENDER_STOPS": str(stops)}
    subprocess.run([RENDER, os.path.join(REPO, "instruments", inst), events, out], env=env, check=True,
                   stdout=subprocess.DEVNULL)


def notes_events(keys, speed, path):
    evs = [{"t": i * GAP_S, "note": int(k), "speed": speed, "hold": HOLD_S} for i, k in enumerate(keys)]
    json.dump({"sr": 48000, "t_end": len(keys) * GAP_S, "pedal": [], "events": evs}, open(path, "w"))


def measure(x, sr, n):
    f0 = 440.0 * 2 ** ((n - 69) / 12)
    return FB.track(x, sr, f0, k_max=12)[0], FB.t20(x, sr)


def stats(values):
    v = [x for x in values if np.isfinite(x)]
    return (float(np.median(v)), float(min(v)), float(max(v))) if v else (np.nan, np.nan, np.nan)


def main():
    os.makedirs(OUT, exist_ok=True)
    TARGETS = targets()
    buff = P.provider(next(iter(TARGETS.values()))[1], "buff")      # the buff fit's part
    fit = P.load_fit(buff)
    english = {int(n): v for n, v in fit["keys"].items()}
    ref = {b: {"t20": stats([v["t20_ratio"] for n, v in english.items() if band(n) == b and v["t20_ratio"] is not None]),
               "dsig": stats([np.nanmedian([d for d in v["dsig"][:12] if d is not None] or [np.nan])
                              for n, v in english.items() if band(n) == b])}
           for b in BANDS}
    keys = sorted(english)
    report, ok = {"reference": ref}, True
    for name, (inst, demo, stops) in TARGETS.items():
        speed = json.load(open(os.path.join(REPO, "instruments", inst, "instrument.json")))["velocity"]["speed"][1]
        ev = os.path.join(OUT, f"notes_{name}.json")
        notes_events(keys, speed, ev)
        x = {}
        for buff in (0, 1):
            wav = os.path.join(OUT, f"notes_{name}_buff{buff}.wav")
            render(inst, ev, wav, buff, stops)
            y, sr = sf.read(wav)
            x[buff] = y.mean(1) if y.ndim > 1 else y
        per = {b: {"t20": [], "dsig": []} for b in BANDS}
        for i, n in enumerate(keys):
            seg = slice(int(i * GAP_S * sr), int((i * GAP_S + HOLD_S) * sr))
            (s0, t0), (s1, t1) = measure(x[0][seg], sr, n), measure(x[1][seg], sr, n)
            per[band(n)]["t20"].append(t1 / t0)
            per[band(n)]["dsig"].append(np.nanmedian(s1 - s0))
        report[name] = {}
        for b in BANDS:
            got = {q: stats(per[b][q])[0] for q in ("t20", "dsig")}
            within = {q: ref[b][q][1] <= got[q] <= ref[b][q][2] for q in got}
            ok &= all(within.values())
            report[name][b] = {"synth": got, "within": within}
            print(f"{name:12} {b:7} t20 ratio {got['t20']:.2f} (English {ref[b]['t20'][0]:.2f}, "
                  f"{ref[b]['t20'][1]:.2f}…{ref[b]['t20'][2]:.2f}) {'ok' if within['t20'] else 'OUT'};  "
                  f"Δσ {got['dsig']:.2f} /s (English {ref[b]['dsig'][0]:.2f}, "
                  f"{ref[b]['dsig'][1]:.2f}…{ref[b]['dsig'][2]:.2f}) {'ok' if within['dsig'] else 'OUT'}")
        for buff in (0, 1):
            render(inst, P.cache_path(demo, "demo_events"), os.path.join(OUT, f"demo_{name}_buff{buff}.wav"), buff, stops)
    for n in (50, 60, 72):
        p = FB.pairs(buff).get(n)
        if p:
            for prof, path in p.items():
                shutil.copy(path, os.path.join(OUT, f"english_{prof}_{n}{os.path.splitext(path)[1]}"))
    json.dump(report, open(os.path.join(OUT, "check.json"), "w"), indent=1, default=float)
    print("all bands within the English range" if ok else "SOME BANDS OUTSIDE the English range")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
