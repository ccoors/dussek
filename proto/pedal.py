#!/usr/bin/env python3
"""Sustain-pedal noise (analysis/pedal_noise.py has the measurements).

Pedal down: a mechanical thump, resynthesized from its statistics like the release noise
(two takes, alternating), plus the strings set ringing as the dampers leave them.

The recorded ring is nearly pure fundamentals of a few bass keys (D2–F2 in the
Salamander takes), hardly any lowest octave, weak harmonics, and it barely decays over
three seconds. The dampers release each string by itself (not through the bridge, which
drives all strings of a note in phase): energy goes into the unison system's slowly
decaying modes, the aftersound. Model: every key whose damper lifts gets a kick into its
full string model (the vertical polarization's coupled modes, as a struck note's free
decay starts; soundboard.coupled_modes): the strings lie still under the dampers and leave
them moving, from rest (sine phase), per string i and partial n

    x[i, n](t) = amp · bell(key; center, spread) · n^(−rolloff) · s[n] · w[i] · sin ωt,

s[n] = ±1 a fixed pseudo-random sign per partial, w the unison's strings against each other
(real, summing to zero: (S−1, −1, …)/√(S−1); a single string 1). The recorded ring has no
loud onset and stays nearly flat for seconds: its energy is in the unison's slow modes, the
ones whose bridge forces cancel (in-phase strings make the loud, fast-decaying prompt sound;
random phases put 1/S of the energy there, 15–20 dB too loud at the onset). From rest the
bridge force starts at zero: kicked from a displacement (phases e^{j(φ[n] + 2π·i/S)}) it
jumped at the lift, a click on every press once the kick reached the low bass's single
strings (Grands B, D, F and Upright C after their 2026-10-12 refit, step 51). The
string modes give the decay, the dampers stop the ring when the pedal comes up, as for
any note. Pedal up: the dampers landing, resynthesized (two takes). Levels
re the median mf note peak (the sample set's mix), as for the release noise.

The recordings are Salamander's, heard through its own radiation: its fit sets the kick's
shape (bell, rolloff), every other profile takes that shape and fits the level alone through
its own radiation (the shape of the project it borrows the pedal from, [uses] pedal). Fitting the shape too, a profile bent the bell to make up for
its radiation's differences from Salamander's: with the body response following H (step 50:
less bass) Grands B, D, F and Upright C moved it onto the bass's single strings (centers
25–27, spread 9; their line errors 5.9–7.4 dB, with Salamander's shape 6.6–9.1).

Fit per profile to the recorded ring 0.5–3 s after the pedal: the fundamentals of keys
FIT_KEYS line by line (the recorded peak within ±LINE_CENTS of the model's, energy
within ±LINE_HZ; strongest D2–F2, ~10 dB less four keys away) as targets, and the 1/3-
octave band energies as upper bounds (hum, the floor and the thump's remainder only add
to them; a band fit alone cannot tell one key's second partial from another key's
fundamental). The bounds set the partial rolloff: the radiation rises steeply with
frequency, so higher partials would otherwise dominate the ring. The tail energy of
each mode is computed analytically (decaying sinusoid × radiation response at its
frequency; modes add as energies). The fit sets the shape (bell, rolloff); the level is
then set by the ring's total tail energy in RING_HZ (the per-line dB average follows the
few lines that land on radiation peaks, 8 dB too loud for one profile).

Usage: pedal.py --profile grand_a   (fits; writes the profile's "pedal" block)
"""
import argparse
import json
import os
from functools import lru_cache

import numpy as np
import damper
import release
import scale
import project as P  # noqa: E402  (on the path through scale)
import soundboard as SB

HERE = os.path.dirname(os.path.abspath(__file__))
LENGTH_S = 1.0
SEED = {"down": 6000, "up": 6100}
CENTERS = list(range(24, 61))
SPREADS = [1.0, 1.5, 2.0, 3.0, 4.5, 6.5, 9.0, 13.0, 20.0]
ROLLOFFS = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0]
KICK_SEED = 7000
FIT_KEYS = range(28, 56)
FIT_S = 3.5
LINE_HZ = 1.0
LINE_CENTS = 25.0
RING_HZ = (56.0, 280.0)


@lru_cache(maxsize=1)
def data(pid):
    """The pedal noise the part plays: its own analysis, or the one it borrows ([uses] pedal)."""
    return P.load_cache(P.provider(pid, "pedal"), "pedal_noise")


def thump(pid, kind, take, sr=48000):
    """Pedal noise take (kind "down"/"up"), stereo (2, n), peak 1."""
    d = data(pid)
    t = d[kind][take]
    return release.noise_burst(t["impacts"], t["band_db"], d["bands_hz"], SEED[kind] + take, LENGTH_S, sr)


def mf_peak(profile):
    """Median over the keys of the radiated mf peak (model units): the level reference."""
    return float(np.median([v["mf"] for v in release.peak_table(profile).values()]))


def thump_level(profile, kind, take):
    return mf_peak(profile) * 10 ** (data(profile.dataset)[kind][take]["ratio_db"] / 20)


def bell(note, center, spread):
    return float(np.exp(-0.5 * ((note - center) / spread) ** 2))


def kick(profile, pd, note, S, K):
    """Displacement kick C (S×K, complex) of a key whose damper lifts; None if none."""
    if not damper.has_damper(note):
        return None
    n = np.arange(1, K + 1)
    sign = np.where(np.random.default_rng(KICK_SEED + note).random(K) < 0.5, -1.0, 1.0)
    w = np.ones(S) if S == 1 else np.array([np.sqrt(S - 1.0)] + [-1.0 / np.sqrt(S - 1.0)] * (S - 1))
    # from rest: Re(−jA·e^{jωt}) = A·sin ωt (as voice.rs kick)
    return -1j * pd["amp"] * bell(note, pd["center"], pd["spread"]) * n[None, :] ** (-pd["rolloff"]) * sign[None, :] * w[:, None]


def kick_modes(profile, note, sr=48000):
    """Bridge-force weights of a key's vertical modes for a unit kick without rolloff
    (scale by n^-rolloff per partial n = j + 1): w[j, e] (K×S) and the eigenvalues lam
    (K×S); as voice.rs add()."""
    cm = SB.coupled_modes(profile, note, sr)
    lam, V = cm["vert"]
    K, S = lam.shape
    C = kick(profile, {"amp": 1.0, "center": note, "spread": 1.0, "rolloff": 0.0}, note, S, K)
    if C is None:
        return None
    vinv = np.linalg.inv(V)
    q = np.einsum("jei,ij->je", vinv, C)
    return cm["bridge"][:, None] * V.sum(axis=1) * q, lam


def tail_db(pid, y, sr, ref):
    """As analysis/pedal_noise.tail_bands_db: mono sum, tail window, dB re ref²."""
    d = data(pid)
    m = y.sum(axis=0)
    a, b = d["tail_s"]
    seg = m[int(a * sr):int(b * sr)]
    w = np.hanning(len(seg))
    P = np.abs(np.fft.rfft(seg * w)) ** 2 / len(seg) / np.mean(w ** 2)
    f = np.fft.rfftfreq(len(seg), 1 / sr)
    e = np.array([2 * P[(f >= fc * 2 ** (-1 / 6)) & (f < fc * 2 ** (1 / 6))].sum() for fc in d["tail_bands_hz"]])
    return 10 * np.log10(e + 1e-30) - 20 * np.log10(ref)


def ring(profile, pd, sr=48000, duration=FIT_S):
    """Radiated ring after a pedal-down at t = 0 (every damped key kicked, pedal held),
    stereo, in the time domain: for checks and listening."""
    n = int(duration * sr)
    t = np.arange(n) / sr
    y = np.zeros(n)
    for note in range(21, 109):
        km = kick_modes(profile, note, sr)
        if km is None:
            continue
        w, lam = km
        g = pd["amp"] * bell(note, pd["center"], pd["spread"])
        if g < 1e-4 * pd["amp"]:
            continue
        w = w * np.arange(1, w.shape[0] + 1)[:, None] ** (-pd["rolloff"])
        for wj, lj in zip((g * w).ravel(), lam.ravel()):
            y += (wj * np.exp(lj * t)).real
    return SB.radiate(profile, y, np.zeros(n), sr)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True)
    args = ap.parse_args()
    os.environ["SOUNDBOARD_MODE"] = "full"
    prof = scale.Profile(args.profile)
    sr = 48000
    d = data(args.profile)
    ref = mf_peak(prof)
    t0, t1 = d["tail_s"]
    sf = np.array(d["tail_spec_hz"])
    spec = np.mean([10 ** (np.array(t["tail_spec_db"]) / 10) for t in d["down"]], axis=0)   # energy re ref²

    # radiation response of the mono sum at every mode frequency
    h = SB.make_filter(prof, "full", sr, "v").sum(axis=0)
    nfft = 1 << 19
    H = np.fft.rfft(h, nfft)
    keys, part, F, SIG, W = [], [], [], [], []
    for note in range(21, 109):
        km = kick_modes(prof, note, sr)
        if km is None:
            continue
        w, lam = km
        keys += [note] * w.size
        part += list(np.repeat(np.arange(1, w.shape[0] + 1), w.shape[1]))
        F.append((lam.imag / (2 * np.pi)).ravel()), SIG.append((-lam.real).ravel()), W.append(np.abs(w).ravel())
    keys, part = np.array(keys), np.array(part)
    F, SIG, W = np.concatenate(F), np.concatenate(SIG), np.concatenate(W)
    Hm = np.abs(H[np.round(np.abs(F) * nfft / sr).astype(int)])
    # tail energy (samples) of each radiated decaying mode, unit bell gain
    tail = 0.5 * sr * (np.exp(-2 * SIG * t0) - np.exp(-2 * SIG * t1)) / (2 * SIG) * (W * Hm) ** 2
    # band energies of the recording (upper bounds) and the bins of the modes
    bands = np.array(d["tail_bands_hz"])
    band_rec = np.mean([t["tail_db"] for t in d["down"]], axis=0)
    band_of = np.array([np.argmin(np.abs(np.log(fm / bands))) if bands[0] * 2 ** (-1 / 6) <= fm < bands[-1] * 2 ** (1 / 6)
                        else -1 for fm in np.abs(F)])
    inband = band_of >= 0
    # each fitted key's fundamental: in the recording, the peak within ±LINE_CENTS of the
    # model's (the tunings differ slightly), integrated over ±LINE_HZ; in the model, the
    # energy of the key's partial-1 modes
    rec, rows = [], []
    for key in FIT_KEYS:
        m = (keys == key) & (part == 1)
        f0 = F[m].mean()
        cand = np.abs(1200 * np.log2(sf / f0)) <= LINE_CENTS
        fp = sf[cand][np.argmax(spec[cand])]
        rec.append(spec[np.abs(sf - fp) <= LINE_HZ].sum())
        rows.append(m)
    rec = 10 * np.log10(np.array(rec))
    near = np.array(rows, float)
    lines = np.array([np.nonzero(r)[0][0] for r in rows])

    best = None
    shape = None
    if P.uses(args.profile, "pedal"):
        sp_ = P.load_fit(P.provider(args.profile, "pedal"))["pedal"]
        shape = (sp_["rolloff"], sp_["center"], sp_["spread"])
        print(f"  the kick's shape from {P.uses(args.profile, 'pedal')}: center {shape[1]}, spread {shape[2]:g}, rolloff n^-{shape[0]:g}")
    for ro in ROLLOFFS if shape is None else [shape[0]]:
        tail_r = tail * part ** (-2.0 * ro)
        for c in CENTERS if shape is None else [shape[1]]:
            for sp in SPREADS if shape is None else [shape[2]]:
                e = tail_r * np.exp(-((keys - c) / sp) ** 2)               # bell², energies
                model = 10 * np.log10(near @ e + 1e-300) - 20 * np.log10(ref)
                off = float(np.mean(rec - model))
                bm = 10 * np.log10(np.bincount(band_of[inband], e[inband], len(bands)) + 1e-300) - 20 * np.log10(ref)
                over = np.maximum(bm + off - band_rec, 0)                    # upper bounds
                err = float(np.sqrt(np.mean(np.concatenate([rec - model - off, over]) ** 2)))
                if best is None or err < best[0]:
                    best = (err, ro, c, sp, 10 ** (off / 20), model + off, bm + off)
    err, ro, c, sp, amp, fitted, band_fit = best
    # level from the total tail energy in the ring's bands
    ring_bands = (bands >= RING_HZ[0]) & (bands <= RING_HZ[1])
    lin = lambda v: np.sum(10 ** (np.asarray(v)[ring_bands] / 10))  # noqa: E731
    adj = 10 * np.log10(lin(band_rec) / lin(band_fit))
    amp *= 10 ** (adj / 20)
    fitted, band_fit = fitted + adj, band_fit + adj
    print(f"  level set by the ring's tail energy: {adj:+.1f} dB re the line fit")
    pd = {"rolloff": ro, "center": c, "spread": sp, "amp": amp}
    print(f"{args.profile}: keys around {c} (spread {sp:g}), partial rolloff n^-{ro:g}, amp {amp:.3g}, "
          f"line error {err:.1f} dB")
    print("  fundamentals (key: recorded / model dB):")
    print("   " + "  ".join(f"{keys[lines[j]]}: {rec[j]:.0f}/{fitted[j]:.0f}" for j in range(len(lines))))
    print("  bands Hz:     " + " ".join(f"{f:6.0f}" for f in bands))
    print("  recorded:     " + " ".join(f"{v:6.1f}" for v in band_rec))
    print("  model:        " + " ".join(f"{v:6.1f}" for v in band_fit))
    P.update_fit(args.profile, {"pedal": {**pd, "err_db": err}})
    print(f"wrote the pedal block of {P.fit_path(args.profile)}")


if __name__ == "__main__":
    main()
