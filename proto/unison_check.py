#!/usr/bin/env python3
"""The unisons' beats, the engine against its recordings (design log step 183).

Per recorded key from C4 up (the loudest layer, its first take): each of partials 1–3's dB envelope
(5 ms hop, the channels' power summed) from 50 ms past its peak until it sinks to 10 dB over the
file's floor or 40 dB under its peak, a quadratic trend removed; its strongest modulation within
0.25–25 Hz (at least 1.5 periods in the window) is the beat, in cents of the partial; a residual
under 0.5 dB rms counts as no beat (0). The key's beat: the median over its partials. The engine
renders the key at the layer's velocity at the Recording position, the room off (its reflections
are no beat).

The summary: per half octave the median beat, recording and engine; and the line through key 78
(C♯6/D6, where the engine's beats still match) that the recordings' ratio over the engine's
follows from there up, ln(recording / engine) = s·(key − 78), least squares over the keys from
78 up: the factor exp(s·30) at the top key is the [unison] treble table's last row.

Usage: unison_check.py grand_b … [--instrument DIR]
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

HOP_S = 0.005
FROM_KEY = 60
RAMP_FROM = 78
TOP_KEY = 108
BEAT_HZ = (0.25, 25.0)
MIN_DEPTH_DB = 0.5


def envelope(x, sr, f):
    """(t, dB) of the partial at f: STFT (window 6 periods, 15–80 ms), the strongest bin within ±3."""
    n = int(float(np.clip(6.0 / f, 0.015, 0.08)) * sr)
    hop = int(HOP_S * sr)
    nfft = 1 << int(np.ceil(np.log2(n * 4)))
    win = np.hanning(n)
    k = int(round(f * nfft / sr))
    frames = range(0, len(x) - n, hop)
    env = np.empty(len(frames))
    for j, s in enumerate(frames):
        spec = np.abs(np.fft.rfft(x[s:s + n] * win[:, None], nfft, axis=0))
        env[j] = np.sqrt((spec[max(k - 3, 0):k + 4] ** 2).sum(axis=1)).max()
    return (np.array(frames) + n / 2) / sr, 20 * np.log10(env + 1e-12)


def beat_cents(x, sr, f):
    """The partial's beat in cents (0: none), None where it is too short to tell."""
    t, e = envelope(x, sr, f)
    ipk = int(np.argmax(e[: len(e) // 3]))
    floor = np.median(e[-int(0.3 / HOP_S):]) if t[-1] > 1.0 else -200.0
    start = ipk + int(0.05 / HOP_S)
    ok = (e > floor + 10) & (e > e[ipk] - 40)
    end = start
    while end < len(e) and ok[end:end + 10].any():
        end += 1
    if end - start < int(0.4 / HOP_S):
        return None
    tt, ee = t[start:end], e[start:end]
    r = ee - np.polyval(np.polyfit(tt, ee, 2), tt)
    if np.std(r) < MIN_DEPTH_DB:
        return 0.0
    spec = np.abs(np.fft.rfft(r * np.hanning(len(r)), 1 << 16))
    fr = np.fft.rfftfreq(1 << 16, HOP_S)
    sel = (fr >= max(BEAT_HZ[0], 1.5 / (tt[-1] - tt[0]))) & (fr <= BEAT_HZ[1])
    if not sel.any():
        return None
    return float(1200 * np.log2(1 + fr[sel][np.argmax(spec[sel])] / f))


def key_beat(x, sr, f0, B):
    out = []
    for k in (1, 2, 3):
        f = k * f0 * np.sqrt(1 + B * k * k)
        if f > 0.45 * sr:
            break
        c = beat_cents(x, sr, f)
        if c is not None:
            out.append(c)
    return float(np.median(out)) if out else None


def _job(args):
    project, instrument, s, dyn, workdir = args
    x, sr = sf.read(D.sample_path(project, s), always_2d=True)
    cut = s.get("tail_cut_s", D.tail_cut(project))
    if cut:
        x = x[: len(x) - int(cut * sr)]
    x = x[int(max(s["onset_s"] - 0.02, 0) * sr):]
    y, sry = bass_check.render(instrument, s["note"], layer_velocity(instrument, dyn), "room=0", workdir, stereo=True)
    B = s.get("B") or 0.0
    return {"note": s["note"], "rec": key_beat(x, sr, s["f0"], B), "eng": key_beat(y, sry, s["f0"], B)}


def measure(project, instrument, workdir):
    """Per recorded key from FROM_KEY up (the loudest layer, its first take): the beats in cents."""
    if not os.path.exists(MIDI2WAV):
        raise SystemExit(f"{MIDI2WAV} missing: build it with `just engine`")
    os.makedirs(workdir, exist_ok=True)
    dyn = D.dynamics(project)[D.loudest(project)]
    jobs, seen = [], set()
    for s in P.load_cache(project, "analysis")["samples"]:
        if "error" in s or "label_mismatch" in s or s["note"] < FROM_KEY or s["note"] in seen:
            continue
        if D.layer_key(project, s) == dyn:
            seen.add(s["note"])
            jobs.append((project, instrument, s, D.dynamic_of(project, s), workdir))
    with Pool() as pool:
        rows = pool.map(_job, jobs, chunksize=1)
    return sorted((r for r in rows if r["rec"] is not None and r["eng"] is not None), key=lambda r: r["note"])


def ramp_slope(rows):
    """s of ln(recording / engine) = s·(key − RAMP_FROM) over the keys from RAMP_FROM up (keys
    where either has no beat left out), None with fewer than three."""
    sel = [r for r in rows if r["note"] >= RAMP_FROM and r["rec"] > 0 and r["eng"] > 0]
    if len(sel) < 3:
        return None
    d = np.array([r["note"] - RAMP_FROM for r in sel], float)
    y = np.log([r["rec"] / r["eng"] for r in sel])
    return float((d @ y) / (d @ d))


def describe(name, rows):
    out = [f"== {name}: the unisons' beats (cents of the partial, medians per half octave)"]
    for lo in range(FROM_KEY, TOP_KEY + 1, 6):
        sel = [r for r in rows if lo <= r["note"] < lo + 6]
        if sel:
            out.append(f"  {lo:3d}–{lo + 5:3d}  n={len(sel):2d}  recording {np.median([r['rec'] for r in sel]):4.1f}"
                       f"  engine {np.median([r['eng'] for r in sel]):4.1f}")
    s = ramp_slope(rows)
    if s is not None:
        out.append(f"  recording / engine from key {RAMP_FROM} up: ×exp({s:.4f}·(key − {RAMP_FROM})), "
                   f"×{np.exp(s * (TOP_KEY - RAMP_FROM)):.2f} at {TOP_KEY}")
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
        print(describe(proj, measure(proj, inst, args.workdir)), flush=True)


if __name__ == "__main__":
    main()
