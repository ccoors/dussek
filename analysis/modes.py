#!/usr/bin/env python3
"""Modes of a struck bar (vibraphone, marimba, …): the analysis of `analyze.py` without
its string template f_n = n·f0·√(1 + B·n²).

Per recording:
  - the spectrum of the first SPEC_S after the onset (Blackman-Harris, zero-padded to
    NFFT): peaks at least PROMINENCE_DB above their surroundings and within FLOOR_DB of
    the strongest, at most MAX_MODES, at or above MIN_RATIO × the nominal pitch;
    frequencies by parabolic interpolation
  - the fundamental: the strongest peak within ±FUND_CENTS of the nominal pitch; the
    file is flagged `label_mismatch` when the strongest peak below 1.5 × nominal lies a
    whole number of semitones away instead. Its frequency is then refined on SETTLE_S
    (where the take still rings there: within SETTLE_DROP_DB of its level at 0.1 s; a
    damped take is in its noise by then): in the first SPEC_S the bass keys' fundamental reads
    up to 8 cents low (C3; C4 1 cent), at every dynamic, and settles within 0.3 s (the
    resonator's own mode beside it, decaying fast); the other modes' ratios are to the
    settled one
  - peaks closer than MERGE_WIN envelope resolutions are one mode (a cluster: split modes
    beat; the strongest gives the frequency, `cluster` counts them); closer ones read the
    same envelope (its STFT picks the largest bin within ±2: the fundamental's leakage
    came out as modes 4 % beside it at its own level and decay)
  - per mode the envelope and decay fit of `analyze` (single and two-stage, energy
    decay curve), its ratio to the fundamental, and whether it is a harmonic (an integer
    ratio within HARMONIC_CENTS: bar modes are not, a nonlinearity's products are); for the
    upper modes also a short-window decay (`fast_decay`: FAST_WIN_S windows every FAST_HOP_S
    at the mode's frequency, from its peak until FAST_RANGE_DB down or FAST_FLOOR_DB over the
    floor): the xylophone's treble modes fall 550–740 dB/s, gone before analyze's 40 ms
    windows can follow them; and its line's height over the spectrum around it (`line_db`,
    `line_db()`: noise between the lines peaks ~10 dB over its median, the balafon's buzz too)

The fundamental's harmonics h·f1 (h = 2 … H_MAX: the bar's nonlinearity; `harmonics`):
the band of each (±HARM_BAND of h·f1, a HARM_WIN_S Hann STFT: narrow enough to tell 3·f1
from f2 − f1 where the second mode is a few tenths of a percent off 4) and the
fundamental's alike, the level re the fundamental's over HARM_EARLY_S, the decay (from
0.1 s until HARM_RANGE_DB down or HARM_FLOOR_DB above the band's floor) and how far the
early level is above that floor. Whether a harmonic band holds a mode or the modes' sum
instead is the fit's to judge (proto/bars.py: it knows the bar's modes).

A set may widen the fundamental's search (`fund_cents`, the items': the kalimba's tines lie up to
65 cents off their names). A set may name its reference mode at a multiple of the labeled pitch (`ref_ratio`, the items'):
the tubular bells' heard pitch, the strike note, lies at half their fourth mode, where no mode
is (their modes 4, 5, 6 near 2 : 3 : 4 re it). Its "fundamental" is then that mode: found near
ref_ratio × the label, the tuning re there, the ratios re it (the modes below it included, from
MIN_RATIO × the label), no harmonics (the tubes have none of a nonlinearity). Their modes beat
slowly (split pairs, a fraction of a hertz apart): per mode the envelope's swing (`beat_db`: the
std of a narrow band's level around a quadratic trend over BEAT_S, where within BEAT_RANGE_DB of
its peak; BEAT_BAND_S its smoothing) and its rate (`beat_hz`: the swing's strongest component).

A set whose items carry `halo` (the kalimba): its tines ring along when one is plucked (the
pluck's kick through the bridge). Per take, at each other take's fundamental (its own tine), the
spectrum's peak over HALO_S (Hann; within ±HALO_TOL, counted where HALO_SNR_DB over the ring
around it, HALO_RING), re the take's own fundamental there (proto/bars.py takes both back to the
onset by their fitted decays); the struck tine's own modes and harmonics left out (HALO_SKIP
ratios). `halo`: [{note, freq, heard, level_db}] (not heard: the level it would have needed).

Such a set, and one whose items carry `rattle` (the balafon's gourds), also gets its rattle (the
kalimba's tines on the bridge, the gourds' buzz): `rattle_db`, the median power
from RATTLE_BAND over RATTLE_S re the fundamental's peak over the first 0.1 s (Hann, both).

A set whose items carry `strike` (the hand chimes): the strike's short-lived peaks, which the
SPEC_S spectrum misses (the clapper's hit sets the tube's high modes ringing for 0.02–0.2 s, 1 to
16 kHz). The spectrum of the first STRIKE_S after the onset (Blackman-Harris): peaks above
STRIKE_MIN_RATIO × f1, STRIKE_PROMINENCE_DB over their surroundings, at most STRIKE_MAX, the
strongest first; per peak its level re the fundamental (each one's largest in the first 30 ms
of the `fast_decay` windows) and its `fast_decay` (None: it fell less than 10 dB, a slow mode).
`strike`: [{freq, ratio, level_db, decay_db_s}]. And the fundamental's swell (its tubes' low
keys: −20 to −38 dB in the first 8 ms, within 3 dB after 20–40 ms): `rise`, its level in
`fast_decay`'s windows from the onset on, RISE_S long, re their largest (dB, every FAST_HOP_S).

Output: the source's analysis cache (projects/<name>/cache/<source>/analysis.json), the samples' schema of analyze.py with "modes" in
place of "partials" (n: rank by frequency, freq, ratio, harmonic, cluster, + decay fit)
and "harmonics" (h, level_db re f1, decay_db_s, f1_decay_db_s, snr_db).

Usage: modes.py <dataset>
"""
import argparse
import json
import os
import sys
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.signal import find_peaks, get_window

import analyze as A
import datasets as D
import project as P

SPEC_S = 0.3
NFFT = 1 << 18
PROMINENCE_DB = 10.0
FLOOR_DB = 70.0
MAX_MODES = 16
MIN_RATIO = 0.5
FUND_CENTS = 60.0
HARMONIC_CENTS = 15.0
ENV_WIN_S = 0.04   # partial_envelopes' window at f0 > 200 Hz: 25 Hz resolution
MERGE_WIN = 2.5
SETTLE_S = (0.3, 3.0)
FAST_WIN_S = 0.008
FAST_HOP_S = 0.002
FAST_RANGE_DB = 30.0
FAST_FLOOR_DB = 10.0
SETTLE_DROP_DB = 20.0
H_MAX = 7
HARM_WIN_S = 0.3
HARM_HOP_S = 0.01
HARM_BAND = 0.002
HARM_EARLY_S = (0.08, 0.2)
HARM_RANGE_DB = 25.0
HARM_FLOOR_DB = 8.0
BEAT_S = (0.3, 8.0)
BEAT_RANGE_DB = 45.0
BEAT_BAND_S = 0.08
BEAT_HZ = (0.1, 10.0)
HALO_S = (0.05, 0.55)
HALO_TOL = 0.004
HALO_RING = (0.02, 0.06)
HALO_SNR_DB = 10.0
HALO_SKIP = (2.0, 3.0, 4.0, 5.0, 6.3, 6.4, 6.5)
STRIKE_S = 0.06
STRIKE_MIN_RATIO = 1.1
STRIKE_PROMINENCE_DB = 8.0
STRIKE_SPACING_HZ = 60.0
STRIKE_FLOOR_DB = 60.0
STRIKE_MAX = 24
RISE_S = 0.1
RATTLE_BAND = (1000.0, 4000.0)
RATTLE_S = (0.05, 0.3)
LINE_SPAN = 0.1


def spectrum(x, sr, onset):
    """The SPEC_S spectrum from 5 ms after the onset (Blackman-Harris): frequencies, magnitude."""
    a = onset + int(0.005 * sr)
    seg = x[a:a + int(SPEC_S * sr)].sum(axis=1)
    win = get_window("blackmanharris", len(seg))
    return np.fft.rfftfreq(NFFT, 1 / sr), np.abs(np.fft.rfft(seg * win, NFFT))


def line_db(f, lv, fk, width_hz):
    """A peak's height over the spectrum around it (dB): its top within ±width_hz of fk re the
    median within ±LINE_SPAN × fk. Noise alone peaks ~10 dB over its median (the most of ~10³
    exponentially distributed bins); a mode's line stands well over that."""
    top = lv[(f > fk - width_hz) & (f < fk + width_hz)]
    ring = lv[(f > fk * (1 - LINE_SPAN)) & (f < fk * (1 + LINE_SPAN))]
    return float(top.max() - np.median(ring)) if len(top) and len(ring) else 0.0


def find_modes(x, sr, onset, f_nom):
    """(frequency, level dB re strongest) of the spectral peaks, low to high."""
    f, mag = spectrum(x, sr, onset)
    lv = A.db(mag / mag.max() + 1e-15)
    lv = np.where(f >= MIN_RATIO * f_nom, lv, -300)
    bin_hz = sr / NFFT
    pk, _ = find_peaks(lv, prominence=PROMINENCE_DB, distance=max(int(4 / bin_hz), 1))
    pk = [k for k in pk if lv[k] > -FLOOR_DB]
    pk = sorted(pk, key=lambda k: -lv[k])[:MAX_MODES]
    out = []
    for k in sorted(pk):
        kk, m = A.parabolic_peak(mag, k)
        out.append((kk * bin_hz, float(A.db(m / mag.max()))))
    return out


def settled(x, sr, onset, f):
    """The peak near f over SETTLE_S after the onset (None: the take is shorter than 1.3 s,
    or its fundamental fell more than SETTLE_DROP_DB from 0.1 s to SETTLE_S[0])."""
    a, b = onset + int(SETTLE_S[0] * sr), onset + int(SETTLE_S[1] * sr)
    if len(x) < onset + int(1.3 * sr):
        return None

    def band(t0):
        seg = x[onset + int(t0 * sr):onset + int((t0 + 0.1) * sr)].sum(axis=1)
        mag = np.abs(np.fft.rfft(seg * get_window("hann", len(seg)), 1 << 16))
        fr = np.fft.rfftfreq(1 << 16, 1 / sr)
        return (mag[np.abs(fr - f) < max(0.01 * f, 15.0)] ** 2).sum()
    if 10 * np.log10(band(0.05) / (band(SETTLE_S[0]) + 1e-30)) > SETTLE_DROP_DB:
        return None
    seg = x[a:min(b, len(x))].sum(axis=1)
    mag = np.abs(np.fft.rfft(seg * get_window("hann", len(seg)), 1 << 21))
    fr = np.fft.rfftfreq(1 << 21, 1 / sr)
    sel = np.nonzero(np.abs(fr - f) < 0.01 * f)[0]
    k = sel[0] + int(np.argmax(mag[sel]))
    kk, _ = A.parabolic_peak(mag, k)
    return float(kk * sr / (1 << 21))


def fast_envelope(m, sr, f, frames):
    """dB per FAST_WIN_S Hann window every FAST_HOP_S of the component at f (one DFT bin each)."""
    w, hop = int(FAST_WIN_S * sr), int(FAST_HOP_S * sr)
    ph = np.hanning(w) * np.exp(-2j * np.pi * f * np.arange(w) / sr)
    return np.array([20 * np.log10(abs((m[i * hop:i * hop + w] * ph).sum()) + 1e-20) for i in range(frames)])


def fast_decay(x, sr, onset, f):
    """dB/s of the component at f from its peak in the first 30 ms: 8 ms Hann windows every 2 ms
    (a single DFT bin each), a line through the fall until FAST_RANGE_DB down or FAST_FLOOR_DB
    over the floor (its last 10 % ), None when it falls less than 10 dB."""
    m = x[max(onset - int(0.005 * sr), 0):].sum(axis=1)
    w, hop = int(FAST_WIN_S * sr), int(FAST_HOP_S * sr)
    frames = min((len(m) - w) // hop, int(1.0 / FAST_HOP_S))
    if frames < 20:
        return None
    e = fast_envelope(m, sr, f, frames)
    t = np.arange(frames) * FAST_HOP_S
    i0 = int(np.argmax(e[:int(0.03 / FAST_HOP_S)]))
    floor = float(np.median(e[-max(frames // 10, 3):]))
    stop = np.nonzero(e[i0:] < max(e[i0] - FAST_RANGE_DB, floor + FAST_FLOOR_DB))[0]
    j = stop[0] if len(stop) else frames - i0
    if j < 4 or e[i0] - e[i0 + j - 1] < 10.0:
        return None
    return float(-np.polyfit(t[i0:i0 + j], e[i0:i0 + j], 1)[0])


def harmonic_bands(x, sr, onset, f1):
    """Per harmonic h·f1 below 0.45·sr: level re the fundamental's band, decay, and the
    early level's height over the band's floor (its 2nd percentile)."""
    m = x[onset:].sum(axis=1)
    n, hop = int(HARM_WIN_S * sr), int(HARM_HOP_S * sr)
    frames = (len(m) - n) // hop
    if frames < 20:
        return []
    win = np.hanning(n)
    f = np.fft.rfftfreq(n, 1 / sr)
    hs = [h for h in range(1, H_MAX + 1) if h * f1 < 0.45 * sr]
    sel = [np.abs(f - h * f1) <= max(HARM_BAND * h * f1, 1.5 * sr / n) for h in hs]
    env = np.empty((len(hs), frames))
    for a in range(0, frames, 256):
        b = min(a + 256, frames)
        idx = np.arange(n)[None, :] + hop * np.arange(a, b)[:, None]
        P = np.abs(np.fft.rfft(m[idx] * win, axis=1)) ** 2
        for i, s_ in enumerate(sel):
            env[i, a:b] = 10 * np.log10(P[:, s_].sum(axis=1) + 1e-30)
    t = (np.arange(frames) * hop + n / 2) / sr
    early = (t > HARM_EARLY_S[0]) & (t < HARM_EARLY_S[1])

    def dec(e, floor):
        i0 = int(np.argmax(t > 0.1))
        stop = np.nonzero(e[i0:] < max(e[i0] - HARM_RANGE_DB, floor + HARM_FLOOR_DB))[0]
        j = stop[0] if len(stop) else len(e) - i0
        return float(-np.polyfit(t[i0:i0 + j], e[i0:i0 + j], 1)[0]) if j > 6 else None

    floors = np.percentile(env, 2, axis=1)
    d1 = dec(env[0], floors[0])
    out = []
    for i, h in enumerate(hs[1:], 1):
        out.append({"h": h, "level_db": float(env[i][early].mean() - env[0][early].mean()),
                    "decay_db_s": dec(env[i], floors[i]), "f1_decay_db_s": d1,
                    "snr_db": float(env[i][early].mean() - floors[i])})
    return out


def beating(x, sr, onset, f):
    """(std dB, rate Hz) of the envelope's swing at f (the heterodyned band, BEAT_BAND_S Hann),
    over BEAT_S where within BEAT_RANGE_DB of its peak; None with too little of it."""
    m = x[onset:].mean(axis=1)
    t = np.arange(len(m)) / sr
    k = np.hanning(int(BEAT_BAND_S * sr))
    step = int(0.01 * sr)
    z = np.convolve(m * np.exp(-2j * np.pi * f * t), k / k.sum(), mode="same")[::step]
    tt = t[::step]
    e = 20 * np.log10(np.abs(z) + 1e-12)
    sel = (tt > BEAT_S[0]) & (tt < BEAT_S[1]) & (e > e.max() - BEAT_RANGE_DB)
    if sel.sum() < 100:
        return None
    y = e[sel] - np.polyval(np.polyfit(tt[sel], e[sel], 2), tt[sel])
    F = np.abs(np.fft.rfft(y * np.hanning(len(y)), 8 * len(y)))
    fr = np.fft.rfftfreq(8 * len(y), 0.01)
    ok = (fr > BEAT_HZ[0]) & (fr < BEAT_HZ[1])
    return float(np.std(y)), float(fr[ok][np.argmax(F[ok])])


def clusters(peaks, res_hz):
    """Merge peaks closer than res_hz: (freq of the strongest, level, count)."""
    out = []
    for f, lv in peaks:
        if out and f - out[-1]["last"] < res_hz:
            c = out[-1]
            c["count"] += 1
            c["last"] = f
            if lv > c["level"]:
                c["freq"], c["level"] = f, lv
        else:
            out.append({"freq": f, "level": lv, "count": 1, "last": f})
    return out


def analyze_modes(item):
    x, sr = sf.read(item["path"], always_2d=True)
    peak_dbfs = float(A.db(np.abs(x).max()))
    onset = A.find_onset(x)
    f_label = A.midi_to_hz(item["note"])
    ref = item.get("ref_ratio", 1.0)
    f_nom = ref * f_label
    out = {k: item[k] for k in ("note", "layer", "layers", "dynamic")}
    for k in ("relpath",):
        if k in item:
            out[k] = item[k]
    out.update({"file": os.path.basename(item["path"]), "sr": sr, "duration_s": len(x) / sr,
                "onset_s": onset / sr, "peak_dbfs": peak_dbfs, "tail_cut_s": 0.0})
    if ref != 1.0:
        out["ref_ratio"] = ref
    peaks = find_modes(x, sr, onset, f_label)
    near = [(f, lv) for f, lv in peaks if abs(1200 * np.log2(f / f_nom)) < item.get("fund_cents", FUND_CENTS)]
    # (tubes: the reference near ref_ratio × the label is the check; their strongest mode below
    # 1.5 × it is often the fifth, ~7 semitones up)
    low = [(f, lv) for f, lv in peaks if f < 1.5 * f_nom] if ref == 1.0 else []
    if low:
        fs, _ = max(low, key=lambda p: p[1])
        semis = 12 * np.log2(fs / f_nom)
        if abs(semis) > 0.5 and abs(semis - round(semis)) < 0.3:
            out["label_mismatch"] = item["note"] + int(round(semis))
    if not near:
        out["error"] = "no fundamental near the nominal pitch"
        return out
    f1 = max(near, key=lambda p: p[1])[0]
    f1 = settled(x, sr, onset, f1) or f1
    out.update({"f0": float(f1), "tuning_cents": float(1200 * np.log2(f1 / f_nom))})
    modes = [m for m in clusters(peaks, MERGE_WIN / ENV_WIN_S) if m["freq"] >= (0.9 * f1 if ref == 1.0 else MIN_RATIO * f_label)]
    freqs = [m["freq"] for m in modes]
    t, env = A.partial_envelopes(x, sr, freqs, max(f1, 200.0))
    env_db = A.db(env) - peak_dbfs
    t_on = onset / sr
    fs_, mag_ = spectrum(x, sr, onset)
    lv_ = A.db(mag_ / mag_.max() + 1e-15)
    res = []
    for i, m in enumerate(modes):
        post = env_db[i][t >= t_on]
        tail = post[post > -150][-int(1.0 / A.HOP_S):]
        floor_db = float(np.median(tail)) if len(tail) >= 5 else -150.0
        fit = A.fit_decay(t[t >= t_on] - t_on, post, floor_db)
        r = m["freq"] / f1
        harm = round(r) >= 1 and abs(1200 * np.log2(r / round(r))) < HARMONIC_CENTS
        fast = fast_decay(x, sr, onset, m["freq"]) if r > 1.5 else None
        beat = beating(x, sr, onset, m["freq"]) if ref != 1.0 else None
        res.append({"n": i + 1, "freq": float(m["freq"]), "ratio": float(r), "harmonic": bool(harm),
                    "cluster": m["count"], "spec_level_db": m["level"], "line_db": line_db(fs_, lv_, m["freq"], 4.0 / SPEC_S),
                    "floor_db": floor_db, **fit,
                    **({"fast_decay_db_s": fast} if fast else {}),
                    **({"beat_db": beat[0], "beat_hz": beat[1]} if beat else {})})
    out["modes"] = res
    out["harmonics"] = harmonic_bands(x, sr, onset, f1) if ref == 1.0 else []
    return out


def halo(item, result, tines):
    """The other tines' levels (dB re this take's fundamental over HALO_S) in this take.
    `tines`: (note, frequency) of every take's own fundamental."""
    x, sr = sf.read(item["path"], always_2d=True)
    m = x.sum(axis=1)
    onset = A.find_onset(x)
    seg = m[onset + int(HALO_S[0] * sr):onset + int(HALO_S[1] * sr)]
    nfft = 1 << 18
    lv = A.db(np.abs(np.fft.rfft(seg * np.hanning(len(seg)), nfft)) + 1e-12)
    f = np.fft.rfftfreq(nfft, 1 / sr)

    def at(fc):
        sel = np.abs(f - fc) < HALO_TOL * fc
        ring = (np.abs(f - fc) > HALO_RING[0] * fc) & (np.abs(f - fc) < HALO_RING[1] * fc)
        return float(lv[sel].max()), float(np.median(lv[ring]))
    f1 = result["f0"]
    l1, _ = at(f1)
    out = []
    for note, fj in tines:
        r = fj / f1
        if abs(r - 1) < 0.01 or fj > 0.45 * sr or any(abs(r / h - 1) < 0.01 for h in HALO_SKIP):
            continue
        lj, floor = at(fj)
        heard = lj - floor > HALO_SNR_DB
        # not heard: the level it would have needed (proto/bars.py places it below)
        out.append({"note": note, "freq": fj, "heard": bool(heard), "level_db": (lj if heard else floor + HALO_SNR_DB) - l1})
    return out


def strike(item, result):
    """The strike's short-lived peaks (STRIKE_S): [{freq, ratio, level_db re f1, decay_db_s}]."""
    x, sr = sf.read(item["path"], always_2d=True)
    onset = A.find_onset(x)
    f1 = result["f0"]
    seg = x[onset:onset + int(STRIKE_S * sr)].sum(axis=1)
    nfft = 1 << 16
    mag = np.abs(np.fft.rfft(seg * get_window("blackmanharris", len(seg)), nfft))
    f = np.fft.rfftfreq(nfft, 1 / sr)
    lv = A.db(mag / mag.max() + 1e-15)
    pk, _ = find_peaks(lv, prominence=STRIKE_PROMINENCE_DB, distance=max(int(STRIKE_SPACING_HZ * nfft / sr), 1))
    pk = [k for k in pk if STRIKE_MIN_RATIO * f1 < f[k] < 0.45 * sr and lv[k] > -STRIKE_FLOOR_DB]
    pk = sorted(pk, key=lambda k: -lv[k])[:STRIKE_MAX]
    m = x[max(onset - int(0.005 * sr), 0):].sum(axis=1)
    early = int(0.03 / FAST_HOP_S)
    e1 = fast_envelope(m, sr, f1, early).max()
    out = []
    for k in pk:
        fk = float(f[k])
        out.append({"freq": fk, "ratio": fk / f1, "level_db": float(fast_envelope(m, sr, fk, early).max() - e1),
                    "line_db": line_db(f, lv, fk, 4.0 / STRIKE_S), "decay_db_s": fast_decay(x, sr, onset, fk)})
    return out


def rattle_db(x, sr, f1):
    """Median power over RATTLE_BAND in RATTLE_S re the fundamental's peak over the first 0.1 s
    (dB, Hann windows, x: mono from the onset)."""
    nfft = 1 << 15
    f = np.fft.rfftfreq(nfft, 1 / sr)
    s = x[:int(0.1 * sr)]
    ref = (np.abs(np.fft.rfft(s * np.hanning(len(s)), nfft)) ** 2)[(f > 0.95 * f1) & (f < 1.05 * f1)].max()
    s = x[int(RATTLE_S[0] * sr):int(RATTLE_S[1] * sr)]
    p = np.abs(np.fft.rfft(s * np.hanning(len(s)), nfft)) ** 2
    return float(10 * np.log10(np.median(p[(f >= RATTLE_BAND[0]) & (f < RATTLE_BAND[1])]) / ref))


def rise(item, result):
    """The fundamental's level in `fast_decay`'s windows from the onset over RISE_S, re their largest (dB)."""
    x, sr = sf.read(item["path"], always_2d=True)
    m = x[A.find_onset(x):].sum(axis=1)
    e = fast_envelope(m, sr, result["f0"], int(RISE_S / FAST_HOP_S))
    return (e - e.max()).tolist()


def too_many_flagged(results):
    """Why the fits cannot use the analysis, or None: more than half its files flagged (the
    committed sets flag at most 3 %; a map whose keys are off by octaves flags nearly all)."""
    bad = [r for r in results if "error" in r or "label_mismatch" in r]
    if 2 * len(bad) <= len(results):
        return None
    return (f"{len(bad)} of {len(results)} files have no fundamental at their key's pitch: the map's keys are "
            "probably off (by octaves?). Check the source's octave on the editor's names page, or an SFZ's key offsets.")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dataset", choices=D.ids())
    ap.add_argument("-j", "--jobs", type=int, default=os.cpu_count())
    args = ap.parse_args()
    items = D.items(args.dataset)
    if not items:
        sys.exit("no files found")
    with Pool(args.jobs) as pool:
        results = list(pool.imap(analyze_modes, items))
    if any(it.get("halo") for it in items):
        tines = [(r["note"], r["f0"]) for r in results if "f0" in r]
        for it, r in zip(items, results):
            if "f0" in r:
                r["halo"] = halo(it, r, tines)
    for it, r in zip(items, results):
        if (it.get("halo") or it.get("rattle")) and "f0" in r:
            x, sr = sf.read(it["path"], always_2d=True)
            r["rattle_db"] = rattle_db(x[A.find_onset(x):].sum(axis=1), sr, r["f0"])
        if it.get("strike") and "f0" in r:
            r["strike"], r["rise"] = strike(it, r), rise(it, r)
    P.write_cache(args.dataset, "analysis", {"dataset": args.dataset, "kind": "modes", "samples": results}, indent=1)
    out = P.cache_path(args.dataset, "analysis")
    bad = [(r["file"], r.get("error") or r.get("label_mismatch")) for r in results if "error" in r or "label_mismatch" in r]
    print(f"wrote {out}: {len(results)} samples, flagged {bad}")
    msg = too_many_flagged(results)
    if msg:
        sys.exit(msg)


if __name__ == "__main__":
    main()
