#!/usr/bin/env python3
"""The Rust players against live.py (engine/src/bin/brass_check.rs). port_check.py <project> <mute> <folder> plays a phrase through
live.Section and writes the events, every player's streams and draws and the result; the checker replays them.
--bands: the same phrase with the engine's own random numbers, octave bands against Python's. --hold: rt_stability's grid.
--with TABLE.KEY=VALUE: a variant's override, in live.py and in the export the checker reads (export_brass.py's)."""
import argparse
import json
import multiprocessing
import os
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..")
ap = argparse.ArgumentParser()
ap.add_argument("project")
ap.add_argument("mute")
ap.add_argument("folder")
ap.add_argument("--bands", action="store_true")
ap.add_argument("--hold", action="store_true")
ap.add_argument("--with", dest="overrides", action="append", default=[], metavar="TABLE.KEY=VALUE")
args = ap.parse_args()
os.environ["BRASS_PROJECT"] = args.project
sys.path.insert(0, os.path.join(REPO, "analysis"))
import project as P  # noqa: E402

for item in args.overrides:  # before live.py reads the project
    P.override(args.project, *(lambda t, k, v: (t, {k: v}))(*P.parse_with(item)))

import numpy as np  # noqa: E402

import live  # noqa: E402
import rt_stability as R  # noqa: E402
import spec  # noqa: E402

CTRL, DT, SR = live.CTRL, live.DT, live.SR
EDGES = [100 * 2.0 ** i for i in range(8)]


def phrase():
    """A line, a chord, harmony under a slur, a swell, a glide, growl and flutter, the wheels: (s, kind, ...)."""
    c = (spec.S["low"] + spec.S["high"]) // 2
    ev, t = [], 0.1
    for m, d, v, gap in ((0, .35, 80, .06), (3, .3, 64, -.02), (7, .3, 90, -.02), (5, .25, 40, .1), (7, .25, 110, -.02), (12, 1.2, 105, .15)):
        ev += [(t, "on", c + m, v), (t + d - gap, "off", c + m)]
        t += d
    t += 0.4
    for m in (0, 4, 7):  # a chord
        ev += [(t, "on", c + m, 90), (t + 1.0, "off", c + m)]
    t += 1.5
    ev += [(t, "on", c, 80), (t + 0.25, "on", c + 2, 80), (t + 1.1, "off", c + 2), (t + 1.3, "off", c)]  # a key left down under a slur
    t += 1.8
    ev.append((t, "on", c - 2, 60))
    for i in range(41):
        ev.append((t + 0.1 + i * 0.04, "cc", 11, 40 + 87 * i / 40))
    for i in range(41):
        ev.append((t + 1.8 + i * 0.04, "cc", 11, 127 - 87 * i / 40))
    ev.append((t + 3.5, "off", c - 2))
    t += 4.0
    ev += [(t, "on", c, 70), (t + 0.45, "cc", 65, 127), (t + 0.5, "on", c + 3, 60), (t + 0.45 + 0.35, "off", c), (t + 1.3, "off", c + 3), (t + 1.4, "cc", 65, 0)]
    t += 1.9
    ev += [(t, "cc", 21, 90), (t + 0.05, "on", c + 1, 90), (t + 0.8, "off", c + 1), (t + 0.9, "cc", 21, 0)]
    t += 1.2
    ev += [(t, "cc", 23, 90), (t + 0.05, "on", c + 1, 90), (t + 0.8, "off", c + 1), (t + 0.9, "cc", 23, 0)]
    t += 1.2
    ev += [(t, "on", c - 1, 90), (t + 0.2, "pb", 0.5), (t + 0.5, "pb", 0.0), (t + 0.6, "cc", 1, 80), (t + 0.9, "cc", 1, 0), (t + 1.0, "cc", 70, 80), (t + 1.3, "cc", 70, 0),
           (t + 1.4, "at", 100), (t + 1.7, "at", 0), (t + 1.8, "off", c - 1)]
    t += 2.2
    if args.mute == "plunger":
        ev += [(t, "cc", 4, 30), (t + 0.05, "on", c, 90), (t + 0.5, "cc", 4, 110), (t + 0.9, "off", c)]
        t += 1.3
    ev += [(t, "mode", "solo"), (t + 0.1, "on", c, 90), (t + 0.5, "on", c + 2, 90), (t + 0.9, "off", c + 2), (t + 1.0, "off", c), (t + 1.3, "mode", "auto")]
    t += 1.8
    ev += [(t, "on", c, 90), (t + 0.4, "cc", 123, 0)]
    t += 1.2
    return sorted(ev, key=lambda e: e[0]), t


def play(section, ev, dur):
    n = int(dur / DT)
    out, j = np.zeros((n * CTRL, 2)), 0
    steps = []
    for k in range(n):
        while j < len(ev) and ev[j][0] <= k * DT:
            section.post(*ev[j][1:])
            steps.append([k, *ev[j][1:]])
            j += 1
        out[k * CTRL:(k + 1) * CTRL] = section.step()
    return out, steps


def shape_draws(seed, n):
    """What each note's Shape draws, in Shape's order."""
    rows = []
    for i in range(n):
        r = np.random.default_rng([seed, i])
        rows.append([r.normal(0, 1.5), r.uniform(0.8, 1.3), r.uniform(0.8, 1.1), r.uniform(34, 44), r.normal(0, 6), r.uniform()])
    return np.array(rows)


def export(folder):
    out = os.path.join(folder, "export")
    subprocess.run([sys.executable, os.path.join(HERE, "..", "export_brass.py"), "--project", args.project, "--out", out,
                    *(f"--with={o}" for o in args.overrides)], check=True, capture_output=True)
    return out


def write_voice(folder, i, v):
    np.asarray(v.noise, np.float32).tofile(os.path.join(folder, f"noise_{i}.bin"))
    for name in ("wp", "wl", "band", "jit"):
        np.asarray(getattr(v, name), np.float64).tofile(os.path.join(folder, f"{name}_{i}.bin"))
    shape_draws(v.seed, 256).tofile(os.path.join(folder, f"draws_{i}.bin"))


def replay():
    os.makedirs(args.folder, exist_ok=True)
    ev, dur = phrase()
    s = live.Section(args.mute, True, 1)
    for i, v in enumerate(s.voices):
        v.noise = np.asarray(v.noise, np.float32).astype(np.float64)
        write_voice(args.folder, i, v)
    out, steps = play(s, ev, dur)
    out.tofile(os.path.join(args.folder, "out.bin"))
    json.dump({"export": export(args.folder), "mute": args.mute, "voices": len(s.voices), "events": steps, "steps": len(out) // CTRL, "resets": s.resets},
              open(os.path.join(args.folder, "meta.json"), "w"))
    print(f"{args.project} {args.mute}: {dur:.1f} s, peak {np.abs(out).max():.3f}, resets {s.resets}, wrote {args.folder}")


def bands_of(x):
    P = (np.abs(np.fft.rfft(x, axis=0)) ** 2).sum(axis=1)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    return [float(10 * np.log10(P[(f >= a) & (f < 2 * a)].sum() + 1e-20)) for a in EDGES]


def bands():
    os.makedirs(args.folder, exist_ok=True)
    ev, dur = phrase()
    s = live.Section(args.mute, True, 1)
    py, steps = play(s, ev, dur)
    py = np.tanh(py)
    json.dump({"export": export(args.folder), "mute": args.mute, "voices": len(s.voices), "events": steps, "steps": len(py) // CTRL, "own": True},
              open(os.path.join(args.folder, "meta.json"), "w"))
    subprocess.run(["cargo", "run", "--release", "--quiet", "--manifest-path", os.path.join(REPO, "engine", "Cargo.toml"), "--bin", "brass_check", "--", args.folder, "--own"], check=True)
    rust = np.tanh(np.fromfile(os.path.join(args.folder, "rust.bin"), np.float64).reshape(-1, 2))
    a, b = bands_of(py), bands_of(rust)
    d = [round(y - x, 2) for x, y in zip(a, b)]
    print(args.project, args.mute, "bands from", [int(e) for e in EDGES], "dB of Rust over Python:", d)
    with open(os.path.join(HERE, "out", f"port_bands_{args.project}_{args.mute}.json"), "w") as f:
        json.dump({"edges": EDGES, "rust_over_python": d, "python": a}, f)


def stats_of(a):
    k, air, v, x = a
    return (k, air, v), R.stats(x, 440 * 2 ** ((k - 69) / 12))


def hold():
    os.makedirs(args.folder, exist_ok=True)
    cases = [(int(k), a, v) for k in R.KEYS for a in R.AIR for v in R.VEL]
    json.dump({"export": export(args.folder), "mute": args.mute, "cases": cases}, open(os.path.join(args.folder, "hold.json"), "w"))
    subprocess.run(["cargo", "run", "--release", "--quiet", "--manifest-path", os.path.join(REPO, "engine", "Cargo.toml"), "--bin", "brass_check", "--", args.folder, "--hold"], check=True)
    y = np.fromfile(os.path.join(args.folder, "hold.bin"), np.float64).reshape(len(cases), -1)
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        res = dict(ex.map(stats_of, [(*c, y[i]) for i, c in enumerate(cases)], chunksize=6))
    bad = {}
    for c, r in res.items():
        if not R.stable(r, c[1]):
            bad[c[0]] = bad.get(c[0], 0) + 1
    print(args.project, args.mute, f"not holding {sum(bad.values())} of {len(res)} over every key that plays: {dict(sorted(bad.items()))}; in the fitted range {sum(R.in_range(bad).values())}")
    with open(os.path.join(HERE, "out", f"port_hold_{args.project}_{args.mute}.json"), "w") as f:
        json.dump({"all": sum(bad.values()), "fitted": sum(R.in_range(bad).values()), "by_key": bad}, f)


if __name__ == "__main__":
    if args.bands:
        bands()
    elif args.hold:
        hold()
    else:
        replay()
