#!/usr/bin/env python3
"""Sustain-pedal noise from the Salamander pedalD1/2 (pedal down) and pedalU1/2 (pedal
up) samples.

A project's `[noises] pedal_up_impacts` replaces the pedal-up takes' impacts (Salamander's: five near-
equal clicks of its own mechanism, replaced by Upright A's landing and bounces, design log step 122).

Pedal down is a mechanical thump (4–5 impacts within ~0.5 s: pedal, trapwork, damper
rail lifting) followed by seconds of ringing strings, almost all of it in the
62–125 Hz octave: the dampers leave the strings and set the free strings vibrating.
The ring is a string sound at the strings' own pitches, so the model makes it with the
sympathetic resonators (proto/pedal.py), driven by a force pulse; only the thump is
resynthesized as noise, like the release noise. Pedal up is the dampers landing.

Per take:
  - impacts (as release_noise.impacts) and the 1/3-octave band spectrum of the thump
    window (onset … last impact + 50 ms)
  - level: the samples sit in the SFZ at volume −20 dB (down) / −19 dB (up), no
    velocity tracking; ratio_db = take peak in that mix − the median mf note peak in
    the same mix (layer v8 at velocity 60, amp_veltrack 73 %)
  - pedal down only: tail band energies 0.5–3 s after onset (1/3 octaves 25 Hz…1.6 kHz)
    and the tail's power spectrum 20–500 Hz (Hann window, 0.4 Hz bins: the strings'
    lines), both in dB re (median mf note peak)² (energies over the window): the target
    for the string excitation. The 50/100/150 Hz bands carry mains hum and are flagged.
Writes the part's pedal_noise cache. Usage: pedal_noise.py <part>

Usage: pedal_noise.py
"""
import json
import os
import sys

import numpy as np
import soundfile as sf

import datasets as D
import project as P
import release_noise as RN

HERE = os.path.dirname(os.path.abspath(__file__))
VOLUME_DB = {"down": -20.0, "up": -19.0}
TAKES = {"down": ["pedalD1", "pedalD2"], "up": ["pedalU1", "pedalU2"]}
TAIL_S = (0.5, 3.0)
TAIL_BANDS = 25.0 * 2 ** (np.arange(0, 19) / 3)   # 25 Hz … 1.6 kHz (the lowest strings ring below 50 Hz)
HUM_HZ = (50.0, 100.0, 150.0)


def onset(m, sr):
    w = int(0.002 * sr)
    env = np.sqrt(np.convolve(m ** 2, np.ones(w) / w, mode="same"))
    return int(np.argmax(env > env.max() * 10 ** (-30 / 20)))


def mf_note_peak_mix_db(pid):
    notes = P.load_cache(pid, "analysis")["samples"]
    mf = [s["peak_dbfs"] for s in notes if "error" not in s and s["layer"] == 8]
    return float(np.median(mf)) + RN.sfz_gain_db(60, RN.VELTRACK_NOTE)


def tail_bands_db(m, sr, ref_db):
    seg = m[int(TAIL_S[0] * sr):int(TAIL_S[1] * sr)]
    w = np.hanning(len(seg))                           # against leakage from the strong low peaks
    P = np.abs(np.fft.rfft(seg * w)) ** 2 / len(seg) / np.mean(w ** 2)   # Parseval: Σ ≈ energy
    f = np.fft.rfftfreq(len(seg), 1 / sr)
    e = np.array([2 * P[(f >= fc * 2 ** (-1 / 6)) & (f < fc * 2 ** (1 / 6))].sum() for fc in TAIL_BANDS])
    return (10 * np.log10(e + 1e-30) - ref_db).tolist()   # 10·log10(peak²) = ref_db


LINE_SPEC_HZ = (20.0, 500.0)


def tail_spectrum_db(m, sr, ref_db):
    """Tail power spectrum (energy per bin, Hann window) in dB re ref², LINE_SPEC_HZ."""
    seg = m[int(TAIL_S[0] * sr):int(TAIL_S[1] * sr)]
    w = np.hanning(len(seg))
    P = 2 * np.abs(np.fft.rfft(seg * w)) ** 2 / len(seg) / np.mean(w ** 2)
    f = np.fft.rfftfreq(len(seg), 1 / sr)
    sel = (f >= LINE_SPEC_HZ[0]) & (f <= LINE_SPEC_HZ[1])
    return f[sel], 10 * np.log10(P[sel] + 1e-30) - ref_db


def with_impacts(takes, patterns):
    """The takes with their impacts replaced by the patterns, take i by pattern i mod their count
    (the project's [noises] pedal_up_impacts); the band spectrum and level kept."""
    if not patterns:
        return takes
    return [{**t, "impacts": patterns[i % len(patterns)]} for i, t in enumerate(takes)]


def main():
    pid = sys.argv[1]
    ref_db = mf_note_peak_mix_db(pid)
    curve = RN.perspective(pid)   # [noises] perspective_db: the thump as heard from further away
    out = {"bands_hz": RN.BANDS.tolist(), "tail_bands_hz": TAIL_BANDS.tolist(), "tail_s": list(TAIL_S),
           "hum_bands": [int(np.argmin(np.abs(TAIL_BANDS - h))) for h in HUM_HZ],
           "mf_note_peak_mix_dbfs": ref_db, "down": [], "up": []}
    for kind, takes in TAKES.items():
        for name in takes:
            x, sr = sf.read(os.path.join(D.root(pid), name + ".wav"), always_2d=True)
            m = x.sum(axis=1)
            m = m[max(onset(m, sr) - int(0.005 * sr), 0):]
            imp = RN.impacts(m[:int(0.8 * sr)], sr)
            end = int((max(t for t, _, _ in imp) + 0.05) * sr)
            peak_dbfs = float(20 * np.log10(np.abs(x).max()))
            take = {"name": name, "impacts": imp, "band_db": RN.with_perspective(RN.bands(m[:end], sr), RN.BANDS, curve).tolist(),
                    "ratio_db": peak_dbfs + VOLUME_DB[kind] - ref_db}
            if kind == "down":
                mix = m * 10 ** (VOLUME_DB[kind] / 20)
                take["tail_db"] = tail_bands_db(mix, sr, ref_db)
                f, spec = tail_spectrum_db(mix, sr, ref_db)
                take["tail_spec_db"] = spec.tolist()
                out["tail_spec_hz"] = f.tolist()
            out[kind].append(take)
    # a pedal-up rattle of the lender's own mechanism: its landing as another piano's ([noises])
    out["up"] = with_impacts(out["up"], P.section(pid, "noises").get("pedal_up_impacts", []))
    P.write_cache(pid, "pedal_noise", out, indent=1)
    path = P.cache_path(pid, "pedal_noise")
    for kind in TAKES:
        print(f"{kind}: " + "; ".join(f"{t['name']} {len(t['impacts'])} impacts, peak {t['ratio_db']:+.1f} dB re mf note"
                                      for t in out[kind]))
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
