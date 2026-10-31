#!/usr/bin/env python3
"""The early drop of the low partials, the recordings against the engine: a partial's level
at 0.05–0.15 s minus its level at 0.8–1.2 s after the onset (the prompt sound falling to the
aftersound), partials 1–6, every recorded note A0–B4 at the mf and ff layers, the engine at
the Recording position at the layers' own velocities (instrument.json `velocity`).

In the recordings, partials 2–4 of C2–C3 fall 10–25 dB within 0.5 s; the engine's barely
did (design 2026-10-20-bridge-mobility). The same at mf and ff, in both.

Usage: early_drop.py grand_a grand_b … [--instrument DIR] [-o rows.json]
"""
import argparse
import json
import os
import subprocess
import sys
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.signal import stft

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..")
sys.path.insert(0, os.path.join(REPO, "analysis"))
import datasets as D  # noqa: E402
import project as P  # noqa: E402

N_PARTIALS = 6
EARLY, LATE = (0.05, 0.15), (0.8, 1.2)   # s after the onset
NOTES = (21, 71)                         # A0–B4
DYNAMICS = ("mf", "ff")
SR = 48000
HOLD_S = 2.0
MIDI2WAV = os.path.join(REPO, "engine", "target", "release", "midi2wav")
REGISTERS = [("A0-B1", 21, 35), ("C2-B2", 36, 47), ("C3-B4", 48, 71)]
BANDS_HZ = [(50, 90), (90, 160), (160, 280), (280, 500), (500, 900)]


def drops(x, sr, f0, B, n_partials=N_PARTIALS):
    """Per partial: its level over EARLY minus over LATE (dB), nan where not found. x: mono, the
    onset where it first reaches 5 % of its peak; each partial's bin the strongest within
    ±max(3 %, 3 Hz) of k·f0·√(1 + B·k²) over the first 0.3 s, read ±1 bin."""
    x = np.asarray(x, float)
    x = x / (np.abs(x).max() + 1e-12)
    i = int(np.argmax(np.abs(x) > 0.05))
    x = x[i:i + int(1.4 * sr)]
    if len(x) < int(1.3 * sr):
        return [np.nan] * n_partials
    nper = int(0.04 * sr)
    f, t, Z = stft(x, sr, nperseg=nper, noverlap=nper * 7 // 8, nfft=1 << 15)
    A = np.abs(Z)
    # t: the frames' centers re the onset (scipy pads half a frame at the start)
    mean0 = A[:, t < 0.3].mean(axis=1)
    out = []
    for k in range(1, n_partials + 1):
        fk = k * f0 * np.sqrt(1 + B * k * k)
        w = max(0.03 * fk, 3.0)
        m = (f > fk - w) & (f < fk + w)
        if not m.any():
            out.append(np.nan)
            continue
        j = np.flatnonzero(m)[np.argmax(mean0[m])]
        tr = A[max(j - 1, 0):j + 2].max(axis=0) ** 2
        early = tr[(t >= EARLY[0]) & (t < EARLY[1])].mean()
        late = tr[(t >= LATE[0]) & (t < LATE[1])].mean()
        out.append(float(10 * np.log10(early / late)) if late > 0 and early > 0 else np.nan)
    return out


def layer_velocity(instrument_dir, dynamic):
    """The MIDI velocity the instrument plays a recorded layer at (velocity.midi: pp, mf, ff)."""
    with open(os.path.join(instrument_dir, "instrument.json")) as fh:
        midi = json.load(fh)["velocity"]["midi"]
    return int(midi[("pp", "mf", "ff").index(dynamic)])


def render(instrument_dir, note, velocity, workdir):
    """The engine's note at the Recording position (mono), cached in workdir."""
    import mido
    base = os.path.join(workdir, f"{os.path.basename(os.path.normpath(instrument_dir))}_{note}_{velocity}")
    if not os.path.exists(base + ".wav"):
        m = mido.MidiFile()
        tr = mido.MidiTrack()
        m.tracks.append(tr)
        tr.append(mido.Message("note_on", note=note, velocity=velocity, time=240))
        tr.append(mido.Message("note_off", note=note, velocity=0, time=int(HOLD_S * 960)))
        m.save(base + ".mid")
        subprocess.run([MIDI2WAV, instrument_dir, base + ".mid", base + ".wav", "--rate", str(SR),
                        "--tuning", "mic=0", "--bits", "32"], check=True, capture_output=True)
    x, sr = sf.read(base + ".wav", always_2d=True)
    return x.mean(axis=1), sr


def _job(args):
    project, instrument_dir, s, dyn, workdir = args
    x, sr = sf.read(D.sample_path(project, s), always_2d=True)
    f0, B = s["f0"], s.get("B") or 0.0
    eng = render(instrument_dir, s["note"], layer_velocity(instrument_dir, dyn), workdir)
    return {"project": project, "note": s["note"], "dynamic": dyn,
            "rec": drops(x.mean(axis=1), sr, f0, B), "eng": drops(*eng, f0, B)}


def measure(project, instrument, workdir):
    """Rows {project, note, dynamic, rec, eng}: the first take of every recorded note in NOTES at
    each of DYNAMICS (samples with an error or a label mismatch left out)."""
    if not os.path.exists(MIDI2WAV):
        raise SystemExit(f"{MIDI2WAV} missing: build it with `just engine`")
    os.makedirs(workdir, exist_ok=True)
    jobs, seen = [], set()
    for s in P.load_cache(project, "analysis")["samples"]:
        if "error" in s or "label_mismatch" in s or not NOTES[0] <= s["note"] <= NOTES[1]:
            continue
        dyn = D.dynamic_of(project, s)
        if dyn in DYNAMICS and (s["note"], dyn) not in seen:
            seen.add((s["note"], dyn))
            jobs.append((project, instrument, s, dyn, workdir))
    with Pool() as pool:
        return pool.map(_job, jobs, chunksize=4)


def _arr(v):
    return np.array([np.nan if x is None else x for x in v], float)


def summary(rows):
    """The two tables: per register and dynamic, partials 1–6 recording/engine (medians over notes)
    and the partials 2–4 gap (recording − engine); per register and partial frequency band, the gap
    (median over partials 1–6 and both dynamics; the nominal pitch)."""
    out = []
    for proj in dict.fromkeys(r.get("project", "") for r in rows):
        mine = [r for r in rows if r.get("project", "") == proj]
        out.append(f"== {proj}: early drop (0.05–0.15 s vs 0.8–1.2 s), dB, recording/engine")
        for dyn in DYNAMICS:
            for reg, lo, hi in REGISTERS:
                sel = [(_arr(r["rec"]), _arr(r["eng"])) for r in mine if r["dynamic"] == dyn and lo <= r["note"] <= hi]
                if not sel:
                    continue
                rec = np.nanmedian([a for a, _ in sel], axis=0)
                eng = np.nanmedian([b for _, b in sel], axis=0)
                gap = np.nanmedian([np.nanmean(a[1:4] - b[1:4]) for a, b in sel])
                out.append(f"  {dyn} {reg} n={len(sel):2d} " + " ".join(f"{a:5.1f}/{b:5.1f}" for a, b in zip(rec, eng))
                           + f"   p2-4 gap {gap:+5.1f}")
        out.append("  gap by band (Hz): " + "  ".join(f"{a}-{b}" for a, b in BANDS_HZ))
        for reg, lo, hi in REGISTERS:
            cells = []
            for a, b in BANDS_HZ:
                v = [ra[k] - ea[k] for r in mine if lo <= r["note"] <= hi
                     for ra, ea in [(_arr(r["rec"]), _arr(r["eng"]))] for k in range(len(ra))
                     if a <= (k + 1) * 440.0 * 2 ** ((r["note"] - 69) / 12) < b]
                v = [x for x in v if np.isfinite(x)]
                cells.append(f"{np.median(v):+5.1f} ({len(v):2d})" if len(v) >= 3 else "     -    ")
            out.append(f"  {reg}  " + "  ".join(cells))
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("projects", nargs="+")
    ap.add_argument("--instrument", help="the instrument directory (one project; default instruments/<project>)")
    ap.add_argument("-o", "--out", help="the rows as JSON")
    ap.add_argument("--workdir", default=os.path.join(HERE, "out", "early_drop_renders"))
    args = ap.parse_args()
    if args.instrument and len(args.projects) > 1:
        ap.error("--instrument takes one project")
    rows = []
    for proj in args.projects:
        inst = args.instrument or os.path.join(REPO, "instruments", proj)
        rows += measure(proj, os.path.abspath(inst), args.workdir)
    print(summary(rows))
    if args.out:
        with open(args.out, "w") as fh:
            json.dump(rows, fh)


if __name__ == "__main__":
    main()
