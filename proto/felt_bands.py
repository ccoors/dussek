"""The attack above 2 kHz, the felt fit's band target (design 2026-10-20-felt-core).

The recordings' bass is already bright at mf and keeps brightening above 2 kHz at ff, where the
felt fit's partials (2–20) do not reach: on A1 they end near 1.1 kHz. Per band, the level change
between two dynamics minus the fundamental's (ΔB_b − ΔL_1), as fit_hammer's ΔL_n − ΔL_1: the
board, the room and the microphones cancel. Below 2 kHz the bass's brightening holds the phantom
partials (design log step 131), which the fit's render has not; a band counts only where both of a
pair's recorded bands lie CLEAN_DB over the recording's own noise there.
"""
import numpy as np
import soundfile as sf

import fit_hammer as FH

BANDS = [(2000, 4000), (4000, 6000), (6000, 8000), (8000, 11000), (11000, 16000)]
ATTACK = (0.02, 0.15)
CLEAN_DB = 10.0
# the noise: the median of 0.1 s frames over this share of the file, after the note has died at high
# frequencies and before the sets' fade to digital silence (Grand A's C2: −143 dB at 4–6 kHz for most
# of 18–22 s, −167 in its last 0.3 s)
NOISE_SPAN = (0.5, 0.9)
NOISE_FRAME_S = 0.1


def onset(x):
    """The first sample above 5 % of the peak (bass_check.onset)."""
    x = np.asarray(x, float)
    return int(np.argmax(np.abs(x) > np.abs(x).max() * 0.05))


def _density(seg, sr):
    """Per band, 10·log10 of the mean power density (per Hz) of a Hann-windowed segment."""
    seg = np.asarray(seg, float)
    w = np.hanning(len(seg))
    p = np.abs(np.fft.rfft(seg * w)) ** 2 / (sr * np.sum(w ** 2))
    f = np.fft.rfftfreq(len(seg), 1 / sr)
    return np.array([10 * np.log10(p[(f >= a) & (f < b)].mean() + 1e-30) for a, b in BANDS])


def band_levels(x, sr, on):
    """The attack's band levels (dB, power per Hz) over ATTACK after the onset `on`."""
    return _density(np.asarray(x)[on + int(ATTACK[0] * sr): on + int(ATTACK[1] * sr)], sr)


def noise_levels(x, sr):
    """The recording's noise per band: the median of the 0.1 s frames' densities over NOISE_SPAN of
    the signal (an overestimate where the note still rings there: fewer bands count)."""
    x = np.asarray(x)
    hop = int(NOISE_FRAME_S * sr)
    a, b = (int(f * len(x)) for f in NOISE_SPAN)
    frames = [_density(x[i:i + hop], sr) for i in range(a, max(b - hop, a + 1), hop)]
    return np.median(np.array(frames), axis=0)


def recorded_bands(profile, note):
    """Per dynamic {"bands", "noise", "l1"}: the attack's bands and the noise of the sample the felt
    fit takes for that dynamic (fit_profile_felt.measured_targets' filters), the fundamental's level
    as fit_hammer.partial_levels reads it."""
    keys = FH.D.dynamics(profile)
    out = {}
    for s in FH.P.load_cache(profile, "analysis")["samples"]:
        if s["note"] != note or "error" in s or "label_mismatch" in s:
            continue
        if s["B_fit_resid_cents"] >= 8 or s["n_partials_found"] < 6:
            continue
        for dyn, key in keys.items():
            l1 = FH.partial_levels(s).get(1)
            if FH.D.layer_key(profile, s) != key or l1 is None:
                continue
            x, sr = sf.read(FH.D.sample_path(profile, s), always_2d=True)
            x = x.mean(axis=1)
            out[dyn] = {"bands": band_levels(x, sr, onset(x)), "noise": noise_levels(x, sr), "l1": float(l1)}
    return out


def band_error(model, rec, pairs):
    """The rms over the pairs (hi, lo) and their clean bands of the model's ΔB_b − ΔL_1 against the
    recording's; None without a clean band. `model` per dynamic {"bands", "l1"}, `rec` as
    recorded_bands gives it."""
    errs = []
    for hi, lo in pairs:
        if not all(d in model and d in rec for d in (hi, lo)):
            continue
        r, m = rec, model
        clean = ((r[hi]["bands"] - r[hi]["noise"] >= CLEAN_DB) & (r[lo]["bands"] - r[lo]["noise"] >= CLEAN_DB))
        dm = (m[hi]["bands"] - m[lo]["bands"]) - (m[hi]["l1"] - m[lo]["l1"])
        dr = (r[hi]["bands"] - r[lo]["bands"]) - (r[hi]["l1"] - r[lo]["l1"])
        errs += list((dm - dr)[clean])
    return float(np.sqrt(np.mean(np.square(errs)))) if errs else None
