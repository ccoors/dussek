#!/usr/bin/env python3
"""The buff stop's loss per key, from VCSL's English harpsichord recorded without (profile
`english`) and with its buff (`english_buff`; VCSL calls it "Lute", but it damps and
darkens at the same level, as a buff does).

Per key with both recordings, each partial's decay rate σ (1/s) in both: its level per
FRAME_S Hann frame (the maximum within ±0.3·f0 of k·f0) from ATTACK_S after the peak, a
line fitted until the level first falls TRACK_DB below its maximum (at least MIN_FRAMES
frames; partials below F_MAX_HZ). Δσ(k) = σ_buff − σ_normal is fitted by a pad touching
the string at p ≤ P_MAX (near the nut) plus a flat part a (the leather as a lossy end, the
same on every partial): Δσ ≈ a + g·sin²(kπp), a, g ≥ 0 (weights: the frames tracked in both). P0 is the comb-free
reference (sin²(kπp) ≈ (kπp)² there, a smooth rise); a key's p counts where it lowers the
residual re P0 by MIN_GAIN_DB. The p curve over the keyboard is smoothed as the pluck
curve (two median passes against aliases); then a and g per key at the curve's p, smoothed
too.
Both are held beyond the recorded keys.

Output: the part's fit document (projects/buff/fits/buff.json) (notes 21…108, flat and gain 1/s, pos re the speaking length, and
per recorded key the diagnostics).

Usage: fit_buff.py
"""
import argparse
import json
import os
import sys

import numpy as np
import soundfile as sf

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import datasets as DS  # noqa: E402
import scale  # noqa: E402
import project as P  # noqa: E402

FRAME_S = 0.1
ATTACK_S = 0.08
TRACK_DB = 35.0
MIN_FRAMES = 4
F_MAX_HZ = 15000.0
K_MAX = 24
P0 = 0.02
# the pad's position stays near the nut: a batten a few cm from it is p ≈ 0.03 on a C4 string
# and grows as the strings shorten (≈ 0.09 at the top of the recorded compass); the treble's
# few partials let the comb also fit aliases further along (0.2 … 0.4), which are no buff
P_MAX = 0.15
P_GRID = np.arange(P0, P_MAX + 1e-4, 0.002)
MIN_GAIN_DB = 1.0
CURVE_SMOOTH = 4.0      # semitones
OUTLIER_FRAC = 0.3
OUTLIER_SPAN = 6
DB_PER_NEPER = 20 / np.log(10)


def mono(path):
    x, sr = sf.read(path)
    return (x.mean(1) if x.ndim > 1 else x), sr


def track(x, sr, f0, k_max=K_MAX):
    """(σ 1/s, frames) per partial 1…k_max; σ NaN and 0 frames where not tracked."""
    w = int(FRAME_S * sr)
    hop, nfft = w // 2, 1 << 16
    ip = int(np.argmax(np.abs(x)))
    starts = list(range(ip + int(ATTACK_S * sr), len(x) - w, hop))
    sig, frames = np.full(k_max, np.nan), np.zeros(k_max)
    if len(starts) < MIN_FRAMES:
        return sig, frames
    win = np.hanning(w)
    F = np.array([np.abs(np.fft.rfft(x[i:i + w] * win, nfft)) for i in starts])
    t = (np.array(starts) - ip) / sr
    fr = np.fft.rfftfreq(nfft, 1 / sr)
    for k in range(1, k_max + 1):
        if k * f0 > min(F_MAX_HZ, 0.45 * sr):
            break
        m = (fr > (k - 0.3) * f0) & (fr < (k + 0.3) * f0)
        a = 20 * np.log10(F[:, m].max(1) + 1e-12)
        below = np.nonzero(a < a.max() - TRACK_DB)[0]
        end = int(below[0]) if len(below) else len(a)
        if end < MIN_FRAMES:
            continue
        sig[k - 1] = -np.polyfit(t[:end], a[:end], 1)[0] / DB_PER_NEPER
        frames[k - 1] = end
    return sig, frames


def t20(x, sr):
    """Time (s) from the peak of the 10 ms RMS envelope until it first falls 20 dB below."""
    h = int(0.01 * sr)
    e = 20 * np.log10(np.sqrt(np.mean(x[: len(x) // h * h].reshape(-1, h) ** 2, axis=1)) + 1e-12)
    ip = int(np.argmax(e))
    below = np.nonzero(e[ip:] < e[ip] - 20)[0]
    return float(below[0] * 0.01) if len(below) else float("nan")


def _clean(k, d, w):
    k, d, w = (np.asarray(v, float) for v in (k, d, w))
    ok = np.isfinite(d) & (w > 0)
    return k[ok], d[ok], w[ok]


def _ag_err(k, d, w, p):
    """Flat part a ≥ 0 and pad strength g ≥ 0 at p (weighted least squares on a + g·sin²(kπp))
    and the weighted rms residual."""
    s = np.sin(k * np.pi * p) ** 2
    err = lambda a, g: float(np.sqrt(np.sum(w * (d - a - g * s) ** 2) / np.sum(w)))  # noqa: E731
    sw = np.sqrt(w)
    (a, g), *_ = np.linalg.lstsq(np.column_stack([np.ones_like(s), s]) * sw[:, None], d * sw, rcond=None)
    if a >= 0 and g >= 0:
        return float(a), float(g), err(a, g)
    # on the boundary: one of the two at 0, the other by least squares (clipped at 0)
    g0 = max(float(np.sum(w * d * s) / max(np.sum(w * s * s), 1e-12)), 0.0)
    a0 = max(float(np.sum(w * d) / np.sum(w)), 0.0)
    return min(((0.0, g0, err(0.0, g0)), (a0, 0.0, err(a0, 0.0))), key=lambda f: f[2])


def fit_pad(k, dsig, w):
    """(p, a, g, err, err0): the best pad position on P_GRID, the flat part and the pad's
    strength there, and the weighted rms residual (1/s) there and at the comb-free P0."""
    k, d, w = _clean(k, dsig, w)
    best = min(((p, *_ag_err(k, d, w, p)) for p in P_GRID), key=lambda f: f[3])
    return float(best[0]), best[1], best[2], best[3], _ag_err(k, d, w, P0)[2]


def comb_gain_db(err, err0):
    return float(20 * np.log10(max(err0, 1e-12) / max(err, 1e-12)))


def _held(at, ns):
    lo, hi = ns[0], ns[-1]
    return [at(min(max(n, lo), hi)) for n in range(21, 109)]


def pos_curve(per_key):
    """Smoothed pad position for keys 21…108 (P0 where no key shows a comb)."""
    good = {n: v for n, v in per_key.items() if comb_gain_db(v["err"], v["err0"]) >= MIN_GAIN_DB}
    keep = good
    for _ in range(2):
        med = {n: np.median([v["p"] for m, v in keep.items() if abs(m - n) <= OUTLIER_SPAN] or [v["p"]])
               for n, v in good.items()}
        keep = {n: v for n, v in good.items() if abs(v["p"] - med[n]) <= OUTLIER_FRAC * med[n]}
    if not keep:
        return [P0] * 88
    ns = sorted(keep)
    at = scale._smooth(ns, [keep[n]["p"] for n in ns],
                       [comb_gain_db(keep[n]["err"], keep[n]["err0"]) for n in ns], frac=CURVE_SMOOTH)
    return [float(np.clip(p, P0, P_MAX)) for p in _held(at, ns)]


def strength_curves(per_key, pos):
    """(flat, gain) per key 21…108 (1/s): a and g fitted at the curve's p, each smoothed
    (weights: frames)."""
    ns = sorted(per_key)
    fits = [_ag_err(*_clean(per_key[n]["k"], per_key[n]["dsig"], per_key[n]["w"]), pos[n - 21]) for n in ns]
    wt = [float(np.sum(_clean(per_key[n]["k"], per_key[n]["dsig"], per_key[n]["w"])[2])) for n in ns]
    curve = lambda v: [float(max(x, 0.0)) for x in _held(scale._smooth(ns, v, wt, frac=CURVE_SMOOTH), ns)]  # noqa: E731
    return curve([f[0] for f in fits]), curve([f[1] for f in fits])


def pairs(pid):
    """note → {"normal": path, "buff": path}: the part's two sets (roles main and buff), keys
    recorded both ways."""
    by = {}
    for sid in P.source_ids(pid):
        role = {"main": "normal", "buff": "buff"}[P.source(sid)["role"]]
        for it in DS.items(sid):
            by.setdefault(it["note"], {})[role] = it["path"]
    return {n: v for n, v in sorted(by.items()) if len(v) == 2}


def _num(x):
    return float(x) if np.isfinite(x) else None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--profile", required=True, help="the buff fit's part")
    args = ap.parse_args()
    per_key = {}
    for n, paths in pairs(args.profile).items():
        f0 = 440.0 * 2 ** ((n - 69) / 12)
        (xa, sr), (xb, _) = mono(paths["normal"]), mono(paths["buff"])
        sa, na = track(xa, sr, f0)
        sb, nb = track(xb, sr, f0)
        k, d, w = np.arange(1, K_MAX + 1), sb - sa, np.minimum(na, nb)
        if np.sum(np.isfinite(d) & (w > 0)) < 3:
            print(f"{n}: too few partials tracked, left out")
            continue
        p, a, g, err, err0 = fit_pad(k, d, w)
        per_key[n] = {"k": k, "dsig": d, "w": w, "p": p, "a": a, "g": g, "err": err, "err0": err0,
                      "t20_ratio": t20(xb, sr) / t20(xa, sr)}
        print(f"{n}: p {p:.3f}  a {a:5.2f}  g {g:6.2f} /s  comb {comb_gain_db(err, err0):+5.1f} dB  "
              f"t20 ratio {per_key[n]['t20_ratio']:.2f}")
    pos = pos_curve(per_key)
    flat, gain = strength_curves(per_key, pos)
    out = {"notes": list(range(21, 109)), "flat": flat, "gain": gain, "pos": pos,
           "keys": {str(n): {"p": v["p"], "a": v["a"], "g": v["g"], "err": v["err"], "err0": v["err0"],
                             "t20_ratio": _num(v["t20_ratio"]), "dsig": [_num(x) for x in v["dsig"]]}
                    for n, v in per_key.items()}}
    P.write_fit(args.profile, out)
    print(f"pos {min(pos):.3f} … {max(pos):.3f}, flat {min(flat):.2f} … {max(flat):.2f} /s, gain {min(gain):.2f} … {max(gain):.2f} /s")


if __name__ == "__main__":
    main()
