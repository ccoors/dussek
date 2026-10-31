#!/usr/bin/env python3
"""Release noise (key return, damper landing, hammer settling) from the Salamander
"hammer noise releases" rel1…rel88.wav (one per key, normalized, triggered at key-up).

Per key:
  - impacts: local maxima of the broadband envelope (2 ms RMS, smoothed) within 18 dB of
    the strongest, ≥ 25 ms apart: time after the trigger, level re the strongest, and an
    exponential decay time (from the drop by 10 dB after each impact)
  - spectrum: 1/3-octave band powers of the whole sample, dB re the strongest band
  - level re the note: the samples are normalized, so the level comes from the
    instrument's SFZ (release group at −37 dB, amp_veltrack 82 %; notes 73 %) and the
    recorded peak of the note's layer: ratio_db[dyn] = release peak − note peak as the
    sampler plays them (layers v1/v8/v16 at velocities 14/60/124). This is the sample
    author's mix by ear, the only reference there is.
A project's `[noises] perspective_db` ([[Hz, dB], …]) tilts every band spectrum, the release's and
the pedal's (Salamander's microphones 12 cm over the strings against a player's position: design
log step 121); its `[release_noise] level_db` moves every ratio (Salamander's: −12 dB, fitted against
Grand E's release recordings through the engine, design log step 120).
Other sets with their own release samples (a source's `release = { kind = "sfz", … }`): the
release region of each recorded note, its level re the note as the sampler plays both
(`vcsl_keys`).
Writes the part's release cache (release.json).

Usage: release_noise.py <part>
"""
import json
import os
import sys

import numpy as np
import soundfile as sf

import analyze as A
import datasets as D
import project as P

HERE = os.path.dirname(os.path.abspath(__file__))
BANDS = 25.0 * 2 ** (np.arange(0, 30) / 3)   # 25 Hz … ~20 kHz
SFZ_VOLUME_DB = -37.0
VELTRACK_REL, VELTRACK_NOTE = 0.82, 0.73
LAYER_VEL = {"pp": (1, 14), "mf": (8, 60), "ff": (16, 124)}


def sfz_gain_db(vel, track):
    """SFZ default velocity curve: amplitude (vel/127)², scaled by amp_veltrack."""
    return 40 * np.log10(vel / 127) * track


def impacts(x, sr):
    w = int(0.002 * sr)
    env = np.sqrt(np.convolve(x ** 2, np.ones(w) / w, mode="same"))
    env = np.convolve(env, np.ones(5 * w // 2) / (5 * w // 2), mode="same")
    edb = 20 * np.log10(env / env.max() + 1e-12)
    sep = int(0.025 * sr)
    out = []
    for i in np.argsort(edb)[::-1]:
        if edb[i] < -18 or len(out) >= 5:
            break
        if all(abs(i - j) >= sep for j, _, _ in out) and edb[i] >= edb[max(i - sep, 0):i + sep].max() - 1e-9:
            after = np.nonzero(edb[i:] < edb[i] - 10)[0]
            t10 = (after[0] if len(after) else len(edb) - i) / sr
            out.append((i, float(edb[i]), float(t10 / np.log(10 ** 0.5))))   # amplitude e-folding time
    return [(i / sr, lv, tau) for i, lv, tau in sorted(out)]


def bands(x, sr):
    P = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / sr)
    b = np.array([P[(f >= fc * 2 ** (-1 / 6)) & (f < fc * 2 ** (1 / 6))].sum() for fc in BANDS])
    return 10 * np.log10(b / b.max() + 1e-12)


def rel_keys(pid):
    """The release samples rel1…rel88.wav of the part's set (Salamander's)."""
    notes = P.load_cache(pid, "analysis")["samples"]
    peaks = {(s["note"], s["layer"]): s["peak_dbfs"] for s in notes if "error" not in s}
    out = []
    for k in range(1, 89):
        note = 20 + k
        x, sr = sf.read(os.path.join(D.root(pid), f"rel{k}.wav"), always_2d=True)
        mono = x.sum(axis=1)
        peak_dbfs = float(A.db(np.abs(x).max()))
        sampled = min({n for n, _ in peaks}, key=lambda n: abs(n - note))
        ratio = {}
        for dyn, (layer, vel) in LAYER_VEL.items():
            if (sampled, layer) in peaks:
                ratio[dyn] = (peak_dbfs + SFZ_VOLUME_DB + sfz_gain_db(vel, VELTRACK_REL)) \
                             - (peaks[(sampled, layer)] + sfz_gain_db(vel, VELTRACK_NOTE))
        out.append({"note": note, "impacts": impacts(mono, sr), "band_db": bands(mono, sr).tolist(),
                    "peak_dbfs": peak_dbfs, "ratio_db": ratio})
    return out


def partial_freqs(s, f_max):
    """A recorded note's partials (Hz) up to f_max: the analysis' measured ones, then f0 and B's
    model above the last."""
    got = [p["freq"] for p in s.get("partials", []) if p.get("freq")]
    n = np.arange(len(got) + 1, int(f_max / s["f0"]) + 2)
    return np.concatenate([got, n * s["f0"] * np.sqrt(1 + s.get("B", 0.0) * n ** 2)])


def without_partials(x, sr, freqs, guard_bins=2.5):
    """x with its note's partials taken out (STFT bins within guard_bins of a partial zeroed, overlap-
    added back): a release sample holds the string ringing out under the damper, which the engine
    plays as the note's own decay; measured as noise it played the note again as a whisper of 0.2–0.5 s
    (Harpsichord D, design log step 182). The frame holds 8 periods of the lowest partial."""
    from scipy.signal import istft, stft
    f_lo = max(float(np.min(freqs)), 20.0)
    n = int(2 ** np.ceil(np.log2(max(8 * sr / f_lo, 1024))))
    f, _, Z = stft(x, sr, window="hann", nperseg=n, noverlap=n * 3 // 4)
    keep = np.ones(len(f), bool)
    for fp in freqs:
        keep &= np.abs(f - fp) > guard_bins * sr / n
    _, y = istft(Z * keep[:, None], sr, window="hann", nperseg=n, noverlap=n * 3 // 4)
    return y[: len(x)]


def vcsl_keys(profile):
    """Release samples of a VCSL set (one per recorded note; keys between take the nearest), their
    note's partials taken out (`without_partials`). Level re the note as the sampler plays them:
    (release peak + its SFZ volume) − (note peak + its SFZ volume), both with amp_veltrack 0, for the
    recorded layer (mf)."""
    sfz = D.parse_sfz(os.path.join(D.root(profile), D.release_sfz(profile)))
    regions = [r for r in sfz if r.get("trigger") == "release" and "sample" in r]
    # the note's attack region volume (the analysis does not keep it for one-layer sets)
    attack_db = {int(r["pitch_keycenter"]): float(r.get("volume", 0)) for r in sfz
                 if r.get("trigger", "attack") == "attack" and "sample" in r}
    notes = P.load_cache(profile, "analysis")["samples"]
    # keyed by the key played (the 4′'s samples are analyzed at their sounding note)
    note_db = {s.get("key", s["note"]): s["peak_dbfs"] + attack_db[s.get("key", s["note"])] for s in notes
               if "error" not in s and s.get("key", s["note"]) in attack_db}
    by_key = {s.get("key", s["note"]): s for s in notes if "error" not in s}
    rec = {}
    for r in regions:
        x, sr = sf.read(os.path.join(D.root(profile), r["sample"].replace("\\", "/")), always_2d=True)
        n = int(r["pitch_keycenter"])
        if n in note_db:
            mono = without_partials(x.sum(axis=1), sr, partial_freqs(by_key[n], sr / 2))
            rec[n] = (mono, sr, float(A.db(np.abs(mono).max())) + float(r.get("volume", 0)))
    out = []
    for note in range(21, 109):
        src = min(rec, key=lambda n: abs(n - note))
        mono, sr, rel_db = rec[src]
        out.append({"note": note, "impacts": impacts(mono, sr), "band_db": bands(mono, sr).tolist(),
                    "peak_dbfs": rel_db, "ratio_db": {"mf": rel_db - note_db[src]}})
    return out


def with_perspective(band_db, bands_hz, curve):
    """Band levels (dB re the strongest) moved by a perspective curve [[Hz, dB], …]: 0 below its
    first point, interpolated in log frequency, held beyond its last; re the strongest again."""
    if not curve:
        return np.asarray(band_db, float)
    f, d = np.array(curve, float).T
    out = np.asarray(band_db, float) + np.interp(np.log2(bands_hz), np.log2(f), d, left=0.0, right=d[-1])
    return out - out.max()


def perspective(pid):
    """The project's [noises] perspective_db, or none."""
    return P.section(pid, "noises").get("perspective_db", [])


def with_level(keys, level_db):
    """The keys with every ratio moved by level_db (the project's [release_noise] level_db)."""
    return [{**k, "ratio_db": {d: v + level_db for d, v in k["ratio_db"].items()}} for k in keys]


def main():
    profile = sys.argv[1]
    out = rel_keys(profile) if P.source(profile)["release"]["kind"] == "rel_files" else vcsl_keys(profile)
    # a level re the sample author's mix, measured against release recordings ([release_noise])
    out = with_level(out, float(P.section(profile, "release_noise").get("level_db", 0.0)))
    # recorded closer than the instruments that borrow it ([noises] perspective_db)
    curve = perspective(profile)
    out = [{**k, "band_db": with_perspective(k["band_db"], BANDS, curve).tolist()} for k in out]
    P.write_cache(profile, "release", {"bands_hz": BANDS.tolist(), "keys": out}, indent=1)
    path = P.cache_path(profile, "release")
    imp = [len(k["impacts"]) for k in out]
    dyns = sorted({d for k in out for d in k["ratio_db"]})
    r = {d: np.median([k["ratio_db"][d] for k in out if d in k["ratio_db"]]) for d in dyns}
    print(f"wrote {path}: impacts per key {min(imp)}–{max(imp)} (median {np.median(imp):.0f}); "
          f"median level re note: " + ", ".join(f"{d} {v:.1f} dB" for d, v in r.items()))


if __name__ == "__main__":
    main()
