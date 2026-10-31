#!/usr/bin/env python3
"""The bass above 1 kHz, the recordings against the engine (design 2026-10-20-bass-high-band):

- the attack's band levels (0.02–0.15 s after the onset) up to 16 kHz re 200–500 Hz: the
  recordings' A1 and C2 at ff lay 12–24 dB over the engine's at 4–16 kHz;
- how long the bands last: the partials' level at 0.3, 0.6 and 1.5 s re 0.02–0.07 s, the median per
  band (C2's 4.5–6.5 kHz: −6/−10/−24 dB recorded, −16/−22/−34 in the engine);
- the energy between the partials (outside ±6 Hz of each) re the partials', 0.05–0.55 s, and over
  a set's layers its growth (dB per dB of the partials): ~1 at 0.5–2 kHz in A0–A1 (the phantom
  partials), ~0 at 2–8 kHz.

The engine renders at the Recording position at the layers' own velocities (instrument.json
`velocity`); each signal's partials are found on itself.

Usage: bass_check.py grand_a … [--param phantom=0.2] [--notes 21,33,36] [-o rows.json]
"""
import argparse
import hashlib
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
from early_drop import MIDI2WAV, layer_velocity  # noqa: E402

ATTACK = (0.02, 0.15)
REF_HZ = (200, 500)
ATTACK_BANDS = [(1000, 2000), (2000, 4000), (4000, 6000), (6000, 8000), (8000, 11000), (11000, 16000)]
LEFT_BANDS = [(500, 1000), (1000, 2000), (2000, 3000), (3000, 4500), (4500, 6500)]
LEFT_AT = (0.3, 0.6, 1.5)
LEFT_REF = (0.02, 0.07)
BETWEEN = (0.05, 0.55)
BETWEEN_BANDS = [(500, 2000), (2000, 4000), (4000, 8000)]
NEAR_HZ = 6.0          # Blackman over 0.5 s: its main lobe ±6 Hz, the estimator's floor −56 dB
NOTES = (21, 47)       # A0–B2
DYNAMICS = ("mf", "ff")
# the noise: the file's middle, as felt_bands reads it (the sets fade to digital silence at the end)
NOISE_SPAN = (0.5, 0.9)
SR = 48000


def _spectrum(x, sr, t0, t1, window=np.hanning, nfft=None):
    seg = np.asarray(x[int(t0 * sr): int(t1 * sr)], float)
    n = nfft or len(seg)
    p = np.abs(np.fft.rfft(seg * window(len(seg)), n)) ** 2
    return np.fft.rfftfreq(n, 1 / sr), p


def attack_bands(x, sr, on):
    """The attack's band levels (dB) re its 200–500 Hz band, ATTACK_BANDS."""
    f, p = _spectrum(np.asarray(x)[on:], sr, *ATTACK)
    ref = p[(f >= REF_HZ[0]) & (f < REF_HZ[1])].mean()
    return [float(10 * np.log10(p[(f >= a) & (f < b)].mean() / ref)) for a, b in ATTACK_BANDS]


def attack_noise(x, sr, on):
    """The recording's noise per ATTACK_BANDS on attack_bands' scale (dB re the attack's 200–500 Hz):
    the median over frames as long as the attack within NOISE_SPAN of the file, after the note
    has died at high frequencies and before the sets' fade to digital silence (felt_bands)."""
    x = np.asarray(x, float)
    f, p = _spectrum(x[on:], sr, *ATTACK)
    ref = p[(f >= REF_HZ[0]) & (f < REF_HZ[1])].mean()
    n = int((ATTACK[1] - ATTACK[0]) * sr)
    a, b = (int(s * len(x)) for s in NOISE_SPAN)
    frames = []
    for i in range(a, max(b - n, a + 1), n):
        g, q = _spectrum(x[i:i + n], sr, 0.0, n / sr)
        frames.append([q[(g >= lo) & (g < hi)].mean() for lo, hi in ATTACK_BANDS])
    return [float(10 * np.log10(v / ref)) for v in np.median(np.array(frames), axis=0)]


def band_left(x, sr, on, fk):
    """{band: [level at each of LEFT_AT re LEFT_REF (dB), the median over its partials]} (nan without
    partials); each partial's power from an STFT of 20 ms frames, its bin ±1."""
    x = np.asarray(x, float)[on:]
    nper = int(0.02 * sr)
    f, t, Z = stft(x, sr, nperseg=nper, noverlap=nper - nper // 8, nfft=4096)
    A = np.abs(Z) ** 2
    out = {}
    for lo, hi in LEFT_BANDS:
        rows = []
        for v in fk[(fk >= lo) & (fk < hi)]:
            j = int(np.argmin(np.abs(f - v)))
            tr = A[max(j - 1, 0):j + 2].max(axis=0)
            ref = tr[(t >= LEFT_REF[0]) & (t < LEFT_REF[1])].mean()
            if ref <= 0:
                continue
            rows.append([10 * np.log10(tr[(t >= a - 0.05) & (t < a + 0.05)].mean() / ref) for a in LEFT_AT])
        out[(lo, hi)] = list(np.median(rows, axis=0)) if rows else [np.nan] * len(LEFT_AT)
    return out


def between(x, sr, on, fk):
    """The energy outside ±NEAR_HZ of the partials re inside them (dB), BETWEEN_BANDS, BETWEEN."""
    f, p = _spectrum(np.asarray(x)[on:], sr, *BETWEEN, window=np.blackman, nfft=1 << 19)
    near = np.zeros(len(f), bool)
    for v in fk:
        near |= np.abs(f - v) < NEAR_HZ
    return [float(10 * np.log10(p[(f >= a) & (f < b) & ~near].sum() / p[(f >= a) & (f < b) & near].sum()))
            for a, b in BETWEEN_BANDS]


def partial_level(x, sr, on, fk, band):
    """The partials' energy (dB) within a band, BETWEEN's window: the growth's abscissa."""
    f, p = _spectrum(np.asarray(x)[on:], sr, *BETWEEN, window=np.blackman, nfft=1 << 19)
    near = np.zeros(len(f), bool)
    for v in fk:
        near |= np.abs(f - v) < NEAR_HZ
    return float(10 * np.log10(p[(f >= band[0]) & (f < band[1]) & near].sum()))


def growth(levels, ratios):
    """The least-squares slope of ratios over levels (dB/dB)."""
    return float(np.polyfit(np.asarray(levels, float), np.asarray(ratios, float), 1)[0])


def own_partials(x, sr, on, s):
    """This signal's partials below 9 kHz: the sample's series (its measured partials, else the
    stretched series) scaled to the signal's own tuning (the median ratio of partials 2–8, each
    found within ±1 %), each refined to its strongest bin within ±max(0.4 %, 3 Hz) over 0.2–1.2 s."""
    meas = {q["n"]: q["freq"] for q in s["partials"]}
    f0, B = s["f0"], s.get("B") or 0.0
    fk = np.array([meas.get(k, k * f0 * np.sqrt(1 + B * k * k)) for k in range(1, 400)])
    f, p = _spectrum(np.asarray(x)[on:], sr, 0.2, 1.2, nfft=1 << 19)

    def peak(v, w):
        m = (f > v - w) & (f < v + w)
        return f[m][np.argmax(p[m])] if m.any() else v

    r = np.median([peak(fk[k], 0.01 * fk[k]) / fk[k] for k in range(1, 8)])
    return np.array([peak(v * r, max(0.004 * v, 3.0)) for v in fk if v * r < 9000])


def onset(x):
    x = np.asarray(x, float)
    return int(np.argmax(np.abs(x) > np.abs(x).max() * 0.05))


def render(instrument_dir, note, velocity, params, workdir, stereo=False):
    """The engine's note at the Recording position (mono, or its two channels), cached by the
    instrument's content, the note, the velocity and the parameters."""
    import mido
    h = hashlib.sha1()
    for name in ("instrument.json", "data.bin"):
        with open(os.path.join(instrument_dir, name), "rb") as fh:
            h.update(fh.read())
    h.update(f"{note} {velocity} {params}".encode())
    base = os.path.join(workdir, h.hexdigest()[:16])
    if not os.path.exists(base + ".wav"):
        m = mido.MidiFile()
        tr = mido.MidiTrack()
        m.tracks.append(tr)
        tr.append(mido.Message("note_on", note=note, velocity=velocity, time=240))
        tr.append(mido.Message("note_off", note=note, velocity=0, time=3 * 960))
        m.save(base + ".mid")
        cmd = [MIDI2WAV, instrument_dir, base + ".mid", base + ".wav", "--rate", str(SR), "--tuning", "mic=0", "--bits", "32"]
        if params:
            cmd += ["--param", params]
        subprocess.run(cmd, check=True, capture_output=True)
    x, sr = sf.read(base + ".wav", always_2d=True)
    return (x if stereo else x.mean(axis=1)), sr


def _measures(x, sr, s):
    on = onset(x)
    fk = own_partials(x, sr, on, s)
    return {"attack": attack_bands(x, sr, on), "left": {f"{a}-{b}": v for (a, b), v in band_left(x, sr, on, fk).items()},
            "between": between(x, sr, on, fk), "levels": [partial_level(x, sr, on, fk, b) for b in BETWEEN_BANDS]}


def _job(args):
    project, instrument, s, dyn, params, workdir, engine = args
    x, sr = sf.read(D.sample_path(project, s), always_2d=True)
    x = x.mean(axis=1)
    row = {"project": project, "note": s["note"], "dynamic": dyn, "layer": s.get("layer"), "rec": _measures(x, sr, s)}
    row["rec"]["noise"] = attack_noise(x, sr, onset(x))
    if engine:
        y, sre = render(instrument, s["note"], layer_velocity(instrument, dyn), params, workdir)
        row["eng"] = _measures(y, sre, s)
    return row


def measure(project, instrument, params, workdir, notes=None):
    """Rows per recorded note in NOTES (or `notes`): the mf and ff layers against the engine, and every
    other layer (the recording alone) for the growth."""
    if not os.path.exists(MIDI2WAV):
        raise SystemExit(f"{MIDI2WAV} missing: build it with `just engine`")
    os.makedirs(workdir, exist_ok=True)
    jobs, seen = [], set()
    for s in P.load_cache(project, "analysis")["samples"]:
        n = s["note"]
        if "error" in s or "label_mismatch" in s or not (notes and n in notes or not notes and NOTES[0] <= n <= NOTES[1]):
            continue
        dyn = D.dynamic_of(project, s)
        key = (n, D.layer_key(project, s))
        if key in seen:
            continue
        seen.add(key)
        jobs.append((project, instrument, s, dyn, params, workdir, dyn in DYNAMICS))
    with Pool() as pool:
        return pool.map(_job, jobs, chunksize=2)


def summary(rows):
    out = []
    for proj in dict.fromkeys(r["project"] for r in rows):
        mine = [r for r in rows if r["project"] == proj]
        out.append(f"== {proj}: the attack's bands re 200–500 Hz (dB), recording / engine; bands {ATTACK_BANDS}")
        for r in sorted((r for r in mine if "eng" in r), key=lambda r: (r["dynamic"], r["note"])):
            out.append(f"  {r['dynamic']} {r['note']}: " + " ".join(f"{a:6.1f}/{b:6.1f}" for a, b in zip(r["rec"]["attack"], r["eng"]["attack"])))
        out.append(f"  how long the bands last: level at {LEFT_AT} s re {LEFT_REF} s (dB), recording | engine")
        for r in sorted((r for r in mine if "eng" in r and r["dynamic"] == "ff"), key=lambda r: r["note"]):
            cells = []
            for band in r["rec"]["left"]:
                a, b = r["rec"]["left"][band], r["eng"]["left"][band]
                cells.append(f"{band}: " + "/".join(f"{v:4.0f}" for v in a) + " | " + "/".join(f"{v:4.0f}" for v in b))
            out.append(f"  ff {r['note']}: " + "   ".join(cells))
        out.append(f"  between the partials re the partials (dB), recording / engine; bands {BETWEEN_BANDS}")
        for r in sorted((r for r in mine if "eng" in r), key=lambda r: (r["dynamic"], r["note"])):
            out.append(f"  {r['dynamic']} {r['note']}: " + " ".join(f"{a:6.1f}/{b:6.1f}" for a, b in zip(r["rec"]["between"], r["eng"]["between"])))
        by_note = {}
        for r in mine:
            by_note.setdefault(r["note"], []).append(r)
        lines = []
        for n, rs in sorted(by_note.items()):
            if len(rs) < 3:
                continue
            g = [growth([r["rec"]["levels"][b] for r in rs], [r["rec"]["between"][b] for r in rs]) for b in range(len(BETWEEN_BANDS))]
            lines.append(f"  {n} ({len(rs)} layers): " + " ".join(f"{v:+.2f}" for v in g))
        if lines:
            out.append("  the recording's growth between the partials (dB/dB), bands as above")
            out += lines
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("projects", nargs="+")
    ap.add_argument("--param", default="", help="midi2wav's --param for the engine (e.g. phantom=0.2)")
    ap.add_argument("--notes", help="comma-separated MIDI notes (default every recorded one A0–B2)")
    ap.add_argument("-o", "--out")
    ap.add_argument("--workdir", default=os.path.join(HERE, "out", "bass_check_renders"))
    args = ap.parse_args()
    notes = [int(n) for n in args.notes.split(",")] if args.notes else None
    rows = []
    for proj in args.projects:
        rows += measure(proj, os.path.abspath(os.path.join(REPO, "instruments", proj)), args.param, args.workdir, notes)
    print(summary(rows))
    if args.out:
        with open(args.out, "w") as fh:
            json.dump(rows, fh)


if __name__ == "__main__":
    main()
