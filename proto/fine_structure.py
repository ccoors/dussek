#!/usr/bin/env python3
"""Fine structure statistics, synth vs recordings: how irregular are decay rates and
levels from one partial to the next?

A real soundboard makes each partial decay and radiate a little differently; a model
with a smooth bridge coupling and a smooth transfer function does not. Matching
partial by partial is meaningless (the synthetic board is one random draw), so the
comparison is statistical: per partial, the deviation from the running median over its
neighbors (±3 partials), as log2 ratio for decay rates (EDC, ff) and in dB for levels;
then the median absolute deviation per frequency band over all notes.

Synth variants: bridge force with frequency-independent coupling (BOARD=0), with the
synthetic board's admittance (BOARD=1), and with the board plus its radiation.

Beating (--beats, --fit-scatter): per partial (1–30) the dominant modulation rate of
its envelope over 0.3–6 s; per note the median jump of that rate between neighboring
partials re its median (beat irregularity) and the median modulation depth, on the
radiated render vs the recording (ff, notes with unisons up to BEAT_MAX_NOTE). With
equal strings every partial beats at a rate ∝ its frequency: the dips sweep smoothly
across the partials and the note sounds like a chorused oscillator pair ("string-like",
Knight C2–C3: irregularity 0.07–0.11 vs 0.36–0.63 recorded). --fit-scatter picks the
string scatter σ (string_model.NoteParams.scatter_cents) whose irregularity matches the
recordings' (median over notes) and stores it as "string_scatter_cents".

Usage: fine_structure.py [--profile grand_a] [--step 3] [--beats | --fit-scatter]
"""
import json
import argparse
import os
import sys
import tempfile
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly, stft

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import fit_hammer as FH  # noqa: E402
import scale  # noqa: E402
import soundboard as SB  # noqa: E402

BANDS = [(30, 200), (200, 500), (500, 1000), (1000, 2000), (2000, 5000)]
DURATION = 8.0


def deviations(s):
    """(freq, decay deviation in log2, level deviation in dB) per partial of one analysis."""
    ps = sorted((p for p in s["partials"] if p.get("edc_decay_db_s", 0) > 0 and p["conf"] > 0.3),
                key=lambda p: p["n"])
    if len(ps) < 7:
        return []
    f = np.array([p["freq"] for p in ps])
    ld = np.log2([p["edc_decay_db_s"] for p in ps])
    lv = np.array([p["level_db"] for p in ps])
    out = []
    for i in range(len(ps)):
        nb = slice(max(i - 3, 0), i + 4)
        out.append((f[i], ld[i] - np.median(ld[nb]), lv[i] - np.median(lv[nb])))
    return out


def synth(args):
    dataset, note, v, variant = args
    os.environ["BOARD"] = "0" if variant == "flat" else "1"
    prof = scale.Profile(dataset)
    if variant == "radiated":   # stereo, analyzed like the (stereo) recordings
        y, _ = SB.render(prof, note, v, DURATION, stereo=True)
    else:
        y, _ = scale.M.render(scale.note_params(prof, note), v, duration=DURATION)
        y = y[None, :]
    fd, path = tempfile.mkstemp(suffix=".wav")
    os.close(fd)
    sf.write(path, (y * 1e-3).T, 48000, subtype="FLOAT")
    r = A.analyze({"path": path, "note": note, "layer": 0, "layers": 0, "dynamic": "ff", "tail_cut_s": 0.0})
    os.remove(path)
    return deviations(r) if "error" not in r else []


BEAT_MAX_NOTE = 72
BEAT_S = 6.0
BEAT_K = 30
SCATTER_GRID = [0.0, 0.05, 0.1, 0.2, 0.3, 0.5, 0.8, 1.2]


def beat_stats(x, f0, B, sr=48000):
    """(irregularity, depth dB) of one note's beating (see the docstring)."""
    f, t, Z = stft(x, sr, nperseg=8192, noverlap=8192 - 480)
    sel = (t > 0.3) & (t < BEAT_S - 0.2)
    tt = t[sel]
    rates, depth = [], []
    for k in range(1, BEAT_K + 1):
        i = int(np.argmin(np.abs(f - k * f0 * np.sqrt(1 + B * k * k))))
        e = 10 * np.log10((np.abs(Z[max(i - 2, 0):i + 3]) ** 2).sum(axis=0)[sel] + 1e-30)
        e = e - np.polyval(np.polyfit(tt, e, 2), tt)          # without the decay trend
        F = np.abs(np.fft.rfft(e * np.hanning(len(e)), 8 * len(e)))
        fr = np.fft.rfftfreq(8 * len(e), tt[1] - tt[0])
        ok = (fr > 0.15) & (fr < 8)
        rates.append(fr[ok][np.argmax(F[ok])])
        depth.append(np.std(e))
    rates = np.array(rates)
    return float(np.median(np.abs(np.diff(rates))) / np.median(rates)), float(np.median(depth))


def recorded_beats(args):
    dataset, s = args
    prof = scale.Profile(dataset, board=False)
    x, sr = sf.read(FH.D.sample_path(dataset, s), always_2d=True)
    x = x[int(s["onset_s"] * sr):].mean(axis=1)
    if sr != 48000:
        x = resample_poly(x, 48000, sr)
    return beat_stats(x[: int(BEAT_S * 48000)], prof.f0(s["note"]), prof.B(s["note"]))


def synth_beats(args):
    dataset, note, scatter = args
    prof = scale.Profile(dataset)
    if scatter is not None:
        prof.felt_fit["string_scatter_cents"] = scatter
    y, _ = SB.render(prof, note, prof.hammer_speeds()["ff"], BEAT_S)
    return beat_stats(y, prof.f0(note), prof.B(note))


def beats(dataset, meas, scatters, step=1):
    notes = [n for n in sorted(meas) if n <= BEAT_MAX_NOTE and scale.n_strings(n, scale.design_of(dataset)) > 1]
    notes = notes[::step]
    with Pool() as pool:
        rec = np.array(pool.map(recorded_beats, [(dataset, meas[n]) for n in notes]))
        syn = {sc: np.array(pool.map(synth_beats, [(dataset, n, sc) for n in notes])) for sc in scatters}
    print(f"beating, {len(notes)} notes {notes[0]}–{notes[-1]} at ff: median irregularity / depth")
    print(f"  recording          {np.median(rec[:, 0]):.2f} / {np.median(rec[:, 1]):.1f} dB")
    for sc, r in syn.items():
        lab = "as profile" if sc is None else f"σ = {sc:.2f} cents"
        print(f"  {lab:18s} {np.median(r[:, 0]):.2f} / {np.median(r[:, 1]):.1f} dB")
    return rec, syn


def table(name, rows):
    rows = np.array(rows) if rows else np.zeros((0, 3))
    cells = []
    for lo, hi in BANDS:
        sel = (rows[:, 0] >= lo) & (rows[:, 0] < hi)
        if sel.sum() < 10:
            cells.append("      –      ")
        else:
            cells.append(f"{np.median(np.abs(rows[sel, 1])):.2f} / {np.median(np.abs(rows[sel, 2])):4.1f} dB")
    print(f"{name:10s} " + "  ".join(cells))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    ap.add_argument("--step", type=int, default=1, help="use every n-th measured note")
    ap.add_argument("--variants", default="flat,board,radiated")
    ap.add_argument("--beats", action="store_true", help="beat irregularity only")
    ap.add_argument("--fit-scatter", action="store_true")
    args = ap.parse_args()
    prof = scale.Profile(args.profile, board=False)
    key = FH.D.dynamics(args.profile)["ff"]
    meas = {s["note"]: s for s in prof.samples
            if FH.D.layer_key(args.profile, s) == key and s["B_fit_resid_cents"] < 8}
    if args.beats:
        beats(args.profile, meas, [None], args.step)
        return
    if args.fit_scatter:
        rec, syn = beats(args.profile, meas, SCATTER_GRID, args.step)
        target = np.median(rec[:, 0])
        # irregularity grows with σ: interpolate the crossing (log σ between grid points)
        irr = np.array([np.median(syn[sc][:, 0]) for sc in SCATTER_GRID])
        i = int(np.searchsorted(np.maximum.accumulate(irr), target))
        if i == 0:
            best = 0.0
        elif i >= len(irr):
            best = SCATTER_GRID[-1]
        else:
            a, b = SCATTER_GRID[i - 1], SCATTER_GRID[i]
            best = float(a + (b - a) * (target - irr[i - 1]) / max(irr[i] - irr[i - 1], 1e-9))
        FH.P.update_fit(args.profile, {"string_scatter_cents": best})
        print(f"string scatter σ = {best:.2f} cents (· √k per partial), wrote {FH.P.fit_path(args.profile)}")
        return
    notes = sorted(meas)[:: args.step]
    v = prof.hammer_speeds()["ff"]
    print(f"{args.profile}: {len(notes)} notes at ff; per band: median |decay deviation| (log2) / "
          f"median |level deviation|")
    print(" " * 11 + "  ".join(f"{f'{lo}–{hi} Hz':^13s}" for lo, hi in BANDS))
    table("recording", [r for n in notes for r in deviations(meas[n])])
    with Pool() as pool:
        for variant in args.variants.split(","):
            res = pool.map(synth, [(args.profile, n, v, variant) for n in notes])
            table(variant, [r for rs in res for r in rs])


if __name__ == "__main__":
    main()
