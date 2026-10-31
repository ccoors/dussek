#!/usr/bin/env python3
"""Attack transient (hammer/action noise) per frequency band.

Between the partials the bass has no gaps in a short window, so the transient is
separated by its time behavior instead: per 1/3-octave band, the power in the first
EARLY_S after the onset against the power in the steady window LATE (partials ringing).
The "excess" is early − late power, in dB relative to the late *total* power of the
note (so recordings and synthesis can be compared independently of absolute level).

Also the attack-frame spectrum itself (early-window band powers re its strongest band)
and the noise floor before the onset in the same reference: what a spectrogram shows at
the attack, used by fit_knock.py.

And the attack's peak at 200–700 Hz re the note's peak (`low_peak_db`): from C6 up no partial of
the note lies there, so it is the transient alone (the action and the board's thump), as a
spectrogram cannot show it: a burst decaying in 4 ms and a thump over 15 ms hold the same
energy in the frame, but the burst peaks higher and is heard as a click.

Writes the part's attack cache (attack.json): per (note, dynamic), band excess (dB, or
None where the early power does not exceed the late power), frame_db, noise_db and low_peak_db.

Usage: attack_noise.py grand_a|grand_b
"""
import argparse
import json
import os
from multiprocessing import Pool

import numpy as np
import soundfile as sf

import datasets as D
import project as P

HERE = os.path.dirname(os.path.abspath(__file__))
BANDS = 50.0 * 2 ** (np.arange(0, 28) / 3)   # 1/3-octave centers, 50 Hz … ~32 kHz
EARLY_S = (-0.002, 0.015)
LATE_S = (0.06, 0.12)
NOISE_S = (-0.019, -0.002)  # before the onset, as long as the early window
LOW_BAND_HZ = (200.0, 700.0)   # half an octave under C6, the filter's 6th order keeps its 1047 Hz out
LOW_PEAK_S = 0.05            # the transient's window after the onset
NOTE_PEAK_S = 0.3            # the note's


def band_power(x, sr, t0, t1):
    seg = x[max(int(t0 * sr), 0):int(t1 * sr)]
    seg = seg * np.hanning(len(seg))[:, None]
    P = (np.abs(np.fft.rfft(seg, axis=0)) ** 2).sum(axis=1) / len(seg)
    f = np.fft.rfftfreq(len(seg), 1 / sr)
    out = []
    for fc in BANDS:
        sel = (f >= fc * 2 ** (-1 / 6)) & (f < fc * 2 ** (1 / 6))
        out.append(P[sel].sum() if sel.any() and fc * 2 ** (1 / 6) <= sr / 2 else np.nan)
    return np.array(out), P.sum()


def early_late(x, sr, onset_s):
    """Band powers in the early and the late window, and the late total power."""
    x = x[:, None] if x.ndim == 1 else x
    e, _ = band_power(x, sr, onset_s + EARLY_S[0], onset_s + EARLY_S[1])
    late, late_tot = band_power(x, sr, onset_s + LATE_S[0], onset_s + LATE_S[1])
    return e, late, late_tot


def excess_db(e, late, late_tot):
    ex = e - late
    with np.errstate(invalid="ignore", divide="ignore"):
        return np.where(ex > 0, 10 * np.log10(ex / late_tot), np.nan)


def onset_excess(x, sr, onset_s):
    """Band excess (dB re late total power) of the attack over the steady sound."""
    return excess_db(*early_late(x, sr, onset_s))


def frame_db(x, sr, onset_s):
    """Attack-frame spectrum (band powers in the early window, dB re its strongest band)
    and the noise before the onset in the same reference (NOISE_S window; NaN if the
    file is digitally silent there)."""
    x = x[:, None] if x.ndim == 1 else x
    e, _ = band_power(x, sr, onset_s + EARLY_S[0], onset_s + EARLY_S[1])
    ref = np.nanmax(e)
    out = [10 * np.log10(np.maximum(e, 1e-30) / ref)]
    if onset_s + NOISE_S[0] >= 0:
        nz, _ = band_power(x, sr, onset_s + NOISE_S[0], onset_s + NOISE_S[1])
        # same window length as the early window: powers are comparable
        out.append(np.where(nz > 0, 10 * np.log10(np.maximum(nz, 1e-30) / ref), np.nan))
    else:
        out.append(np.full(len(e), np.nan))
    return out


def low_peak_window(w, sr):
    """The LOW_BAND_HZ peak in the first LOW_PEAK_S of `w` (mono or (n, channels), starting at the
    onset) re its peak over NOTE_PEAK_S, dB."""
    from scipy.signal import butter, sosfiltfilt
    w = w.mean(axis=1) if w.ndim == 2 else w
    w = w[:int(NOTE_PEAK_S * sr)]
    band = sosfiltfilt(butter(6, LOW_BAND_HZ, btype="band", fs=sr, output="sos"), w)
    return float(20 * np.log10(np.abs(band[:int(LOW_PEAK_S * sr)]).max() / max(np.abs(w).max(), 1e-30)))


def low_peak_db(x, sr, onset_s):
    """`low_peak_window` of the recording from its onset (EARLY_S before it)."""
    i = max(int((onset_s + EARLY_S[0]) * sr), 0)
    return low_peak_window(x[i:], sr)


def work(args):
    ds, s = args
    x, sr = sf.read(D.sample_path(ds, s), always_2d=True)
    fdb, ndb = frame_db(x, sr, s["onset_s"])
    return (s["note"], s["dyn"], onset_excess(x, sr, s["onset_s"]).tolist(), fdb.tolist(), ndb.tolist(),
            low_peak_db(x, sr, s["onset_s"]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dataset", choices=D.ids())
    args = ap.parse_args()
    data = P.load_cache(args.dataset, "analysis")["samples"]
    jobs = []
    for s in data:
        if "error" in s or "label_mismatch" in s:
            continue
        for dyn, key in D.dynamics(args.dataset).items():
            if D.layer_key(args.dataset, s) == key:
                jobs.append((args.dataset, dict(s, dyn=dyn)))
    with Pool() as pool:
        rows = pool.map(work, jobs)
    nn = lambda v: [None if not np.isfinite(x) else x for x in v]  # noqa: E731
    out = {"bands_hz": BANDS.tolist(),
           "rows": [{"note": n, "dyn": d, "excess_db": nn(e), "frame_db": nn(f), "noise_db": nn(z), "low_peak_db": lp}
                    for n, d, e, f, z, lp in rows]}
    P.write_cache(args.dataset, "attack", out)
    path = P.cache_path(args.dataset, "attack")
    print(f"wrote {path} ({len(rows)} recordings)")


if __name__ == "__main__":
    main()
