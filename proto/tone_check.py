#!/usr/bin/env python3
"""The tone per register, the engine against its recordings (design log steps 143, 147 and 168):

- within a note, its octave bands over the first second re the band of its fundamental, engine −
  recording, the median over the recorded keys per register (bass A0–B2, middle C3–B4, treble C5
  up) and dynamic: a cut on a treble fundamental only makes the note quieter, so a note's levels re
  its total would hide it;
- the lowest octaves' fundamental re partials 2–4, engine − recording, per key: a board fit holds its
  response flat below the frequency its rows reach (`data_range_hz`), and the lowest fundamentals
  come out too strong.

The engine renders at the Recording position at the layers' own velocities (instrument.json
`velocity`); the two channels' power is averaged (a mono sum cancels where the board's response
per channel differs, step 147).

Usage: tone_check.py upright_a … [--instrument DIR]
"""
import argparse
import os
import sys
from multiprocessing import Pool

import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..")
sys.path.insert(0, os.path.join(REPO, "analysis"))
import bass_check  # noqa: E402
import datasets as D  # noqa: E402
import project as P  # noqa: E402
from early_drop import MIDI2WAV, layer_velocity  # noqa: E402

CENTERS = (31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0)
REGISTERS = (("bass", 21, 47), ("middle", 48, 71), ("treble", 72, 108))
LOWEST = (("A0–G♯1", 21, 32), ("A1–G♯2", 33, 44))
DYNAMICS = ("pp", "mf", "ff")
WINDOW_S = 1.0
NFFT = 1 << 18         # 0.18 Hz at 48 kHz: A0's partials ±1 % apart are 0.27 Hz


def power(x, sr):
    """The channels' power spectra (frequencies, power, onset) over WINDOW_S from the onset, averaged."""
    x = np.asarray(x, float)
    x = x[:, None] if x.ndim == 1 else x
    on = bass_check.onset(x.mean(axis=1))
    seg = x[on:on + int(WINDOW_S * sr)]
    seg = seg * np.hanning(len(seg))[:, None]
    p = (np.abs(np.fft.rfft(seg, n=NFFT, axis=0)) ** 2).mean(axis=1)
    return np.fft.rfftfreq(NFFT, 1 / sr), p, on


def bands(f, p, f0):
    """{center: dB re the octave band holding f0} for the octave bands above f0's (its own, 0 by
    definition, would pull a register's median toward 0 where the top keys' fundamentals fill it)."""
    lv = {c: 10 * np.log10(p[(f >= c / np.sqrt(2)) & (f < c * np.sqrt(2))].sum() + 1e-30) for c in CENTERS}
    ref = min(CENTERS, key=lambda c: abs(np.log2(f0 / c)))
    return {c: float(v - lv[ref]) for c, v in lv.items() if c > ref}


def fundamental(f, p, fk):
    """The fundamental's power re partials 2–4's (dB), each the strongest bin within ±max(1 %, 1 Hz)."""
    def at(v):
        w = max(0.01 * v, 1.0)
        m = (f > v - w) & (f < v + w)
        return p[m].max() if m.any() else 1e-30
    return float(10 * np.log10(at(fk[0]) / sum(at(v) for v in fk[1:4]) + 1e-30))


def measures(x, sr, s):
    f, p, on = power(x, sr)
    out: dict = {"bands": bands(f, p, s["f0"])}
    if s["note"] <= LOWEST[-1][2]:
        fk = bass_check.own_partials(np.asarray(x, float).reshape(len(x), -1).mean(axis=1), sr, on, s)
        out["fund"] = fundamental(f, p, fk)
    return out


def _job(args):
    project, instrument, s, dyn, workdir = args
    rec, srr = sf.read(D.sample_path(project, s), always_2d=True)
    eng, sre = bass_check.render(instrument, s["note"], layer_velocity(instrument, dyn), "", workdir, stereo=True)
    r, e = measures(rec, srr, s), measures(eng, sre, s)
    row: dict = {"note": s["note"], "dynamic": dyn, "bands": {c: e["bands"][c] - v for c, v in r["bands"].items()}}
    if "fund" in r:
        row["fund"] = e["fund"] - r["fund"]
    return row


def measure(project, instrument, workdir):
    """Per recorded note and dynamic (pp, mf, ff; the first take of each) the engine − its recording."""
    if not os.path.exists(MIDI2WAV):
        raise SystemExit(f"{MIDI2WAV} missing: build it with `just engine`")
    os.makedirs(workdir, exist_ok=True)
    jobs, seen = [], set()
    for s in P.load_cache(project, "analysis")["samples"]:
        if "error" in s or "label_mismatch" in s:
            continue
        dyn = D.dynamic_of(project, s)
        if dyn in DYNAMICS and (s["note"], dyn) not in seen:
            seen.add((s["note"], dyn))
            jobs.append((project, instrument, s, dyn, workdir))
    with Pool() as pool:
        return pool.map(_job, jobs, chunksize=1)


def summary(rows):
    """{"registers": {dyn: {register: {"n": takes, "bands": {center: median dB}}}}, "lowest": {range:
    {"median": dB, "keys": [[note, dynamic, dB], …]}}}; a band's median needs two takes. The dynamics'
    medians may rest on different keys (a set records some keys at one dynamic only): compare them
    per key before reading a change with the dynamic into them."""
    regs = {}
    for dyn in DYNAMICS:
        for name, lo, hi in REGISTERS:
            sel = [r for r in rows if r["dynamic"] == dyn and lo <= r["note"] <= hi]
            if not sel:
                continue
            med = {}
            for c in CENTERS:
                v = [r["bands"][c] for r in sel if c in r["bands"]]
                if len(v) >= 2:
                    med[c] = round(float(np.median(v)), 1)
            regs.setdefault(dyn, {})[name] = {"n": len(sel), "bands": med}
    lowest = {}
    for name, lo, hi in LOWEST:
        sel = sorted((r for r in rows if lo <= r["note"] <= hi and "fund" in r), key=lambda r: (r["note"], r["dynamic"]))
        if sel:
            lowest[name] = {"median": round(float(np.median([r["fund"] for r in sel])), 1),
                            "keys": [[r["note"], r["dynamic"], round(r["fund"], 1)] for r in sel]}
    return {"registers": regs, "lowest": lowest}


def describe(name, s):
    out = [f"== {name}: within a note, octave bands re its fundamental's, engine − recording (dB, medians)"]
    for dyn, regs in s["registers"].items():
        for reg, v in regs.items():
            out.append(f"  {dyn} {reg:6s} n={v['n']:2d}  " + "  ".join(f"{c:g}:{d:+.1f}" for c, d in v["bands"].items()))
    for rng, v in s["lowest"].items():
        out.append(f"  fundamental re partials 2–4, {rng}: {v['median']:+.1f} dB; per key "
                   + " ".join(f"{n}{d}:{x:+.0f}" for n, d, x in v["keys"]))
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("projects", nargs="+")
    ap.add_argument("--instrument", help="the export to measure (default instruments/<project>; one project)")
    ap.add_argument("--workdir", default=os.path.join(HERE, "out", "bass_check_renders"))
    args = ap.parse_args()
    if args.instrument and len(args.projects) > 1:
        ap.error("--instrument measures one project's export")
    for proj in args.projects:
        inst = os.path.abspath(args.instrument or os.path.join(REPO, "instruments", proj))
        print(describe(proj, summary(measure(proj, inst, args.workdir))))


if __name__ == "__main__":
    main()
