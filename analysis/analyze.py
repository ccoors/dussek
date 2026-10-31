#!/usr/bin/env python3
"""Phase 0 analysis: extract per-partial parameters from piano recordings.

For every note/velocity sample this estimates:
  - f0 and the inharmonicity coefficient B (f_n = n·f0·sqrt(1 + B·n²))
  - per partial: frequency, initial level, decay rate(s), usable dynamic range
  - a confidence value per measurement, so that low-quality datasets can
    contribute only what they capture reliably

Usage:
  analyze.py <source> [<dir>] [-o out.json]   (the source's analysis in its project's cache)
"""
import argparse
import json
import os
import re
import sys
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.ndimage import maximum_filter1d
from scipy.signal import butter, sosfilt

NOTE_OFFSETS = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
MAX_PARTIALS = 60
HOP_S = 0.02


# ---------------------------------------------------------------- datasets

def parse_note(name):
    """'A0', 'D#1', 'Bb0' -> MIDI note number."""
    m = re.fullmatch(r"([A-G])([#b]?)(-?\d)", name)
    if not m:
        raise ValueError(name)
    letter, acc, octave = m.groups()
    midi = 12 * (int(octave) + 1) + NOTE_OFFSETS[letter]
    return midi + {"#": 1, "b": -1, "": 0}[acc]


# ---------------------------------------------------------------- helpers

def midi_to_hz(n):
    return 440.0 * 2 ** ((n - 69) / 12)


def db(x):
    return 20 * np.log10(np.maximum(x, 1e-12))


def parabolic_peak(mag, k):
    """Sub-bin peak position and magnitude by parabolic interpolation (in dB)."""
    if k <= 0 or k >= len(mag) - 1:
        return float(k), mag[k]
    a, b, c = db(mag[k - 1]), db(mag[k]), db(mag[k + 1])
    denom = a - 2 * b + c
    p = 0.5 * (a - c) / denom if denom != 0 else 0.0
    return k + p, 10 ** ((b - 0.25 * (a - c) * p) / 20)


def find_onset(x):
    """First point where the short-term peak envelope exceeds 10 % of its maximum."""
    env = np.abs(x).max(axis=1)
    return int(np.argmax(env > 0.1 * env.max()))


# ---------------------------------------------------------------- partial frequencies / B

def global_search(spec, bin_hz, floor, f0_nominal, nyq_limit, n_max=8):
    """Coarse grid search over f0 (±250 cents around the label) and B for the partial
    series that best lines up with spectral peaks, using the first few partials.
    Robust against a weak or missing fundamental; refined afterwards."""
    level = np.maximum(db(spec) - db(floor), 0)
    # tolerate misalignment of ±1.5 % of f0 (grid resolution; semitones are ~6 % apart)
    level = maximum_filter1d(level, size=2 * int(0.015 * f0_nominal / bin_hz) + 1)
    cents = np.arange(-250, 251, 2)
    f0s = f0_nominal * 2 ** (cents / 1200)
    Bs = np.concatenate([[0.0], np.logspace(-5, -1.5, 100)])
    n = np.arange(1, n_max + 1)
    F = f0s[:, None, None] * n[None, None, :] * np.sqrt(1 + Bs[None, :, None] * n[None, None, :] ** 2)
    valid = F < nyq_limit
    s = level[np.clip(np.rint(F / bin_hz).astype(int), 0, len(level) - 1)]
    score = np.where(valid, s, 0).sum(axis=2) / np.maximum(valid.sum(axis=2), 1)
    # the tolerance creates plateaus of equal score: break ties towards the label and small B
    score -= 1e-3 * np.abs(cents)[:, None] + 1e-4 * np.arange(len(Bs))[None, :]
    i, j = np.unravel_index(np.argmax(score), score.shape)
    return f0s[i], Bs[j]


PEAK_TOL = 0.04   # a partial's peak within this fraction of its predicted frequency


def estimate_partials(x, sr, onset, f0_nominal):
    """Find partial frequencies in the first part of the sound and fit f0, B.

    Uses (f_n / n)² = f0² + f0²·B·n², which is linear in n².
    """
    seg_len = int(sr * float(np.clip(40.0 / f0_nominal, 0.25, 1.5)))
    start = onset + int(0.03 * sr)  # skip the hammer transient
    seg = x[start:start + seg_len]
    if len(seg) < seg_len // 2:
        return None
    win = np.hanning(len(seg))[:, None]
    nfft = 1 << int(np.ceil(np.log2(len(seg) * 8)))
    spec = np.sqrt((np.abs(np.fft.rfft(seg * win, nfft, axis=0)) ** 2).sum(axis=1))
    bin_hz = sr / nfft
    floor = np.median(spec)
    nyq_limit = 0.45 * sr

    f0, B = global_search(spec, bin_hz, floor, f0_nominal, nyq_limit)
    found = {}
    for n in range(1, MAX_PARTIALS + 1):
        pred = n * f0 * np.sqrt(1 + B * n * n)
        if pred > nyq_limit:
            break
        half = 0.12 * f0
        lo, hi = int((pred - half) / bin_hz), int((pred + half) / bin_hz) + 1
        if lo < 1 or hi >= len(spec):
            break
        k = lo + int(np.argmax(spec[lo:hi]))
        kk, mag = parabolic_peak(spec, k)
        if db(mag) - db(floor) < 12:  # not clearly above the spectral floor
            continue
        # the global search puts f0 within ~1.5 %: a peak further off than PEAK_TOL of the
        # partial's frequency is another one (a weak or missing fundamental: Harpsichord C's D1
        # took a peak 8 % below at 33.6 Hz, and the fit followed it to f0 −182 cents, B 0.03;
        # the bass of several pianos alike). Relative to f0 instead, the treble's higher
        # partials (their B still rough) failed: 2–8 more takes per piano set without partials
        if abs(kk * bin_hz - pred) > PEAK_TOL * n * f0:
            continue
        found[n] = kk * bin_hz
        if len(found) >= 3:
            ns = np.array(sorted(found))
            fs = np.array([found[i] for i in ns])
            y, xx = (fs / ns) ** 2, ns.astype(float) ** 2
            slope, icpt = np.polyfit(xx, y, 1)
            if icpt > 0:
                f0, B = np.sqrt(icpt), max(slope / icpt, 0.0)
    if len(found) < 3:
        return None
    ns = np.array(sorted(found))
    fs = np.array([found[i] for i in ns])
    pred = ns * f0 * np.sqrt(1 + B * ns ** 2)
    resid_cents = 1200 * np.log2(fs / pred)
    return {"f0": f0, "B": B, "partials": dict(zip(ns.tolist(), fs.tolist())),
            "fit_resid_cents": float(np.sqrt(np.mean(resid_cents ** 2)))}


# ---------------------------------------------------------------- envelopes / decay

def partial_envelopes(x, sr, freqs, f0):
    """Per-partial magnitude envelopes (power-summed over channels) via STFT."""
    win_s = float(np.clip(8.0 / f0, 0.04, 0.3))
    n = int(win_s * sr)
    hop = int(HOP_S * sr)
    nfft = 1 << int(np.ceil(np.log2(n * 2)))
    win = np.hanning(n)
    win_gain = win.sum() / 2
    bins = np.array(freqs) * nfft / sr
    frames = range(0, len(x) - n, hop)
    env = np.empty((len(freqs), len(frames)))
    for j, s in enumerate(frames):
        spec = np.abs(np.fft.rfft(x[s:s + n] * win[:, None], nfft, axis=0)) / win_gain
        p = np.sqrt((spec ** 2).sum(axis=1))
        for i, b in enumerate(bins):
            k = int(round(b))
            env[i, j] = p[max(k - 2, 0):k + 3].max()
    t = (np.array(frames) + n / 2) / sr
    return t, env


def fit_decay(t, e_db, floor_db):
    """Fit single- and two-stage linear decays (in dB) to one partial envelope."""
    ipk = int(np.argmax(e_db[: max(len(e_db) // 4, 3)]))
    peak = e_db[ipk]
    range_db = peak - floor_db
    # smooth over ~0.1 s to find where the envelope sinks into the noise
    k = 5
    sm = np.convolve(e_db, np.ones(k) / k, mode="same")
    below = np.nonzero(sm[ipk:] < floor_db + 6)[0]
    end = ipk + (below[0] if len(below) else len(sm) - ipk)
    start = ipk + int(0.05 / HOP_S)
    reached_floor = len(below) > 0
    if end - start < 10:
        return {"level_db": float(peak), "range_db": float(range_db), "conf": 0.0}
    tt, yy = t[start:end], e_db[start:end]
    a1, b1 = np.polyfit(tt, yy, 1)
    r1 = yy - (a1 * tt + b1)
    res = {"level_db": float(peak), "range_db": float(range_db),
           "decay_db_s": float(-a1), "fit_dur_s": float(tt[-1] - tt[0]),
           "resid_db": float(np.std(r1)), "reached_floor": bool(reached_floor)}

    # two-stage: continuous piecewise-linear fit, breakpoint on a grid
    best = None
    for ib in range(5, len(tt) - 5, max(1, len(tt) // 50)):
        A = np.column_stack([np.ones_like(tt), tt - tt[0], np.maximum(tt - tt[ib], 0)])
        coef, *_ = np.linalg.lstsq(A, yy, rcond=None)
        err = np.std(yy - A @ coef)
        if best is None or err < best[0]:
            best = (err, ib, coef)
    if best is not None:
        err, ib, (_, s1, ds) = best
        s2 = s1 + ds
        # accept only a clearly better fit with a fast-then-slow decay
        if err < 0.6 * res["resid_db"] and s1 < s2 < 0 and s1 < 0:
            res.update({"decay1_db_s": float(-s1), "decay2_db_s": float(-s2),
                        "t_break_s": float(tt[ib] - tt[0])})
            res["resid_db"] = float(err)

    res["conf"] = float(np.clip((range_db - 10) / 30, 0, 1)
                        * np.clip(res["fit_dur_s"] / 1.0, 0, 1)
                        * np.exp(-res["resid_db"] / 6))

    # energy decay curve (Schroeder backward integration): robust against beating.
    # Slope fitted over the first 20 dB, leaving out the last 10 % where the
    # truncated integration bends down.
    energy = 10 ** (yy / 10)
    edc = np.cumsum(energy[::-1])[::-1]
    edc_db = 10 * np.log10(edc / edc[0])
    sel = (edc_db > -20) & (np.arange(len(edc_db)) < 0.9 * len(edc_db))
    if sel.sum() >= 5:
        a_e, _ = np.polyfit(tt[sel], edc_db[sel], 1)
        res["edc_decay_db_s"] = float(-a_e)
        res["edc_range_db"] = float(-edc_db[sel].min())
    return res


# ---------------------------------------------------------------- per file

FADE_BAND_HZ = (10000.0, 16000.0)  # no partials left there by the file's end: the noise alone
FADE_FRAME_S = 0.05
# the noise plateau: FADE_REF_S before the end, or in short files FADE_REF_FRAC of the file
# (past the attack; their fades are shorter, 0.4–1.5 s), whichever lies later
FADE_REF_S = (4.0, 2.5)
FADE_REF_FRAC = (0.4, 0.75)
FADE_MIN_S = 1.0
FADE_DROP_DB = 3.0


def fade_cut_s(x, sr):
    """Seconds to drop at the file's end: trailing digital silence and the fade-out before
    it (VCSL, Salamander and others fade their files over the last ~2 s). The noise floor
    is read from the last second (analyze): inside a fade it reads 20–40 dB low, and the
    noise then counts as the partials' own decay. The fade starts where the noise in
    FADE_BAND_HZ last stays within FADE_DROP_DB of its plateau (FADE_REF_S, FADE_REF_FRAC)."""
    m = x.mean(axis=1) if x.ndim > 1 else x
    nz = np.nonzero(m)[0]
    if len(nz) == 0:
        return 0.0
    end = nz[-1] + 1
    zeros = (len(m) - end) / sr
    hi = min(FADE_BAND_HZ[1], 0.45 * sr)
    if end < FADE_MIN_S * sr or hi <= FADE_BAND_HZ[0]:
        return zeros
    h = sosfilt(butter(4, [FADE_BAND_HZ[0], hi], "bandpass", fs=sr, output="sos"), m[:end])
    fr = int(FADE_FRAME_S * sr)
    n = end // fr
    level = 10 * np.log10(np.mean(h[end - n * fr:].reshape(n, fr) ** 2, axis=1) + 1e-30)
    a, b = (max(int(f * n), n - int(t / FADE_FRAME_S)) for f, t in zip(FADE_REF_FRAC, FADE_REF_S))
    ref = float(np.median(level[a:b]))
    above = np.nonzero(level[a:] >= ref - FADE_DROP_DB)[0]
    last = a + (above[-1] if len(above) else 0)
    return zeros + (n - 1 - last) * FADE_FRAME_S


def analyze(item):
    x, sr = sf.read(item["path"], always_2d=True)
    # a set recorded at a gain of its own per layer: its levels re the others' (datasets)
    x = x * 10 ** (-item.get("gain_db", 0.0) / 20)
    # the set's own cut (Salamander) or the detected fade, whichever is longer; kept in the
    # result so that every later reader of the file (body_decay, fit_coupling) cuts the same
    cut = max(item["tail_cut_s"], fade_cut_s(x, sr))
    if cut:
        x = x[: len(x) - int(cut * sr)]
    peak_dbfs = float(db(np.abs(x).max()))
    onset = find_onset(x)
    # a set tuned to an A of its own (the clavichord: 415 Hz): the tuning re that pitch, so the
    # instrument plays at the engine's concert pitch with the set's own deviations
    f0_nom = midi_to_hz(item["note"]) * item.get("a4", 440.0) / 440.0
    out = {k: item[k] for k in ("note", "layer", "layers", "dynamic")}
    out["tail_cut_s"] = float(cut)
    for k in ("relpath", "key", "sfz_volume_db", "gain_db", "a4"):
        if k in item:
            out[k] = item[k]
    out.update({"file": os.path.basename(item["path"]), "sr": sr,
                "duration_s": len(x) / sr, "onset_s": onset / sr, "peak_dbfs": peak_dbfs})

    est = estimate_partials(x, sr, onset, f0_nom)
    if est is None:
        out["error"] = "too few partials found"
        return out
    n_found = len(est["partials"])
    tuning = float(1200 * np.log2(est["f0"] / f0_nom))
    out.update({"f0": est["f0"], "B": est["B"], "tuning_cents": tuning,
                "B_fit_resid_cents": est["fit_resid_cents"], "n_partials_found": n_found,
                # B needs many partials; few partials or a poor fit -> unreliable
                "B_conf": float(np.clip((n_found - 3) / 10, 0, 1)
                                * np.exp(-est["fit_resid_cents"] / 5))})
    # stretch tuning stays well within ±50 cents; beyond that the file is likely mislabeled,
    # provided the partial series fits well (a poor fit means detection failed instead)
    measured_note = item["note"] + int(round(tuning / 100))
    if measured_note != item["note"] and n_found >= 6 and est["fit_resid_cents"] < 5:
        out["label_mismatch"] = measured_note

    ns = sorted(est["partials"])
    freqs = [est["partials"][n] for n in ns]
    t, env = partial_envelopes(x, sr, freqs, est["f0"])
    env_db = db(env) - peak_dbfs  # levels relative to the file peak
    t_on = onset / sr
    pre = t < t_on - 0.05
    partials = []
    for i, n in enumerate(ns):
        e = env_db[i]
        post = e[t >= t_on]
        # ignore frames that are digital silence (some files are zero-padded at either end)
        tail = post[post > -150][-int(1.0 / HOP_S):]
        pre_e = e[pre & (e > -150)]
        floors = [np.median(seg) for seg in (tail, pre_e) if len(seg) >= 5]
        if not floors:  # partial never rises above digital silence (e.g. clean synthetic signals)
            floors = [-150.0]
        floor_db = float(min(floors))
        fit = fit_decay(t[t >= t_on] - t_on, post, floor_db)
        partials.append({"n": n, "freq": freqs[i], "floor_db": floor_db, **fit})
    out["partials"] = partials
    p1 = next((p for p in partials if p["n"] == 1), None)
    out["usable_range_db_p1"] = p1["range_db"] if p1 else None
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    import datasets
    import project as P
    ap.add_argument("dataset", choices=datasets.ids())
    ap.add_argument("root", nargs="?", help="the set's directory (default: its project's)")
    ap.add_argument("-o", "--out", help="a file instead of the project's cache")
    ap.add_argument("-j", "--jobs", type=int, default=os.cpu_count())
    args = ap.parse_args()

    root = args.root or datasets.root(args.dataset)
    items = datasets.items(args.dataset)
    if not items:
        sys.exit(f"no files found in {root}")
    with Pool(args.jobs) as pool:
        results = []
        for i, r in enumerate(pool.imap(analyze, items), 1):
            results.append(r)
            print(f"\r{i}/{len(items)}", end="", file=sys.stderr)
    print(file=sys.stderr)
    doc = {"dataset": args.dataset, "samples": results}
    if args.out:
        P.write_json(args.out, doc, indent=1)
        out = args.out
    else:
        P.write_cache(args.dataset, "analysis", doc, indent=1)
        out = P.cache_path(args.dataset, "analysis")
    errors = [r["file"] for r in results if "error" in r]
    print(f"wrote {out}: {len(results)} samples, {len(errors)} errors {errors[:5]}")


if __name__ == "__main__":
    main()
