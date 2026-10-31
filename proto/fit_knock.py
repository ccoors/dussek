#!/usr/bin/env python3
"""Fit the attack's transient to the recordings: on the hammer pianos the hammer's impact on the
board (fit_impact), on the plucked and tangent instruments the action "knock".

The impact (soundboard.impact_force): each strike's felt force, lowpassed, × gain × (v / v_ff)^0.5,
on the bridge with the strings' force, through the board and the room. Its cutoff by the attack
frame from A4 up (an overshoot 3×), its level by the treble's 200–700 Hz attack peak re the note's
from C6 up (attack_noise.low_peak_db; medians per dynamic, an overshoot 3×), then 6 dB under, the
user's choice. A force pulse into the board once sounded like a ping-pong ball on every key (one
1–2 ms half-sine for every key); the felt force lasts 5 ms and is dark in the bass, 2.4 ms and
bright at C7, shorter at ff.

The knock, as follows (the plucked and tangent instruments):

Hammer, shank and key hitting the action and the keybed shake frame and soundboard:
a short broadband knock at every attack, masked by the strings in the bass but clearly
audible in the treble (Askenfelt). In the recordings the treble attack carries ~20 dB
more transient energy at 0.4–3 kHz than the string model alone (attack_compare.py).

Model (soundboard.knock_burst): a short noise burst per key, exponentially decaying
(decay time) and lowpass-shaped (cutoff), level amp·v^q (v = hammer speed, m/s), from a
knee note upwards (fading out over the octave below). It is added to the radiated sound
directly: a first version drove the soundboard and room with a force pulse, whose
100+ ms ring filled the gaps between the partials (the recordings' knock is over within
~10–20 ms) and sounded like a ping-pong ball.

Band powers add (knock and partials are incoherent): string-only band powers per
(note, dynamic) and knock-only band powers per (note, decay, cutoff) are computed
once; level, q and knee are then a grid search.

Error: the attack-frame spectrum, i.e. what a spectrogram shows in the first 15 ms
(attack_noise.frame_db): per 1/3-octave band 250 Hz–16 kHz, level re the frame's
strongest band, synth vs recording; where the recording is at its noise floor only a
synthetic level above it counts (frame_error). An earlier error on the attack *excess*
over the steady sound was blind in the bass and middle, where the strings mask the
knock in power but it still shows as a broadband stripe (and is heard).

Usage: fit_knock.py --profile grand_a
"""
import argparse
import json
import os
import sys
from multiprocessing import Pool

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import attack_noise as AN  # noqa: E402
import fit_hammer as FH  # noqa: E402
import scale  # noqa: E402
import soundboard as SB  # noqa: E402

SR = 48000
PRE_S = 0.1
MIN_NOTE = 21
# knee: full knock level from this note up, fading out over the octave below (the
# recordings show the knock in the treble only; a dB/octave slope could not remove it
# from the bass without weakening it in the lower treble). Not below 60: for Salamander
# the frame error prefers a knock everywhere, since it fills an energy deficit at
# 1–3 kHz in the first ms of bass notes, but the recordings' spectrograms show no knock
# there (the deficit is more likely the strings' own attack, i.e. the hammer).
KNEES = [60, 66, 72, 78, 84]
DECAYS = [0.004, 0.008, 0.016]
CUTOFFS = [1000.0, 1500.0, 2000.0, 3000.0]
QS = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5]

# The hammer pianos have no knock but the hammer's impact on the board (soundboard.impact_force):
# the knock, a noise burst flat to its cutoff, stood in for the treble's thump under the
# fundamentals and overshot the recordings by up to 10 dB at 1.6 kHz, a click (step 125); the
# impact is each strike's own felt force, lowpassed, through the board as the strings' force is,
# heard as part of the note (step 126).
# an overshoot (over the recording in a band or in the low band's peak) counts this much more than a
# shortfall: the user preferred the thump under the recordings to over them
OVER_WEIGHT = 3.0
# the impact's lowpass, chosen by the attack frame from A4 up (open: none). Grand D's board passes
# the felt force bright: open it overshot 1.25–2 kHz by 10 dB; the fit's 500 Hz was the user's choice
IMPACT_CUTOFFS = [300.0, 500.0, 700.0, 1000.0, 1500.0, 2000.0, 3000.0, None]
IMPACT_SHAPE_FROM = 69
# the user's choice on Salamander: 6 dB under the level the recordings' peak asks for, and softer
# below ff, × (velocity / ff speed)^0.5 (−11 dB at mf, −17 dB at pp); the felt force itself grows
# with the speed, the recordings' thump re the note even more at soft strikes
IMPACT_PREF_DB = -6.0
IMPACT_GROWTH = 0.5


# the knock's level and growth (the hammer pianos): fitted to the recordings' attack peak at
# 200–700 Hz re the note's (attack_noise.low_peak_db) from this note up, where no partial lies
# there. The frame criterion alone filled the band energy with a short burst whose peak then
# stood over the recordings' (Osiris 6 dB at ff) or under (Grand F 11 dB at pp)
LOW_PEAK_FROM = 84


def low_peak_of(y, sr=SR):
    """`attack_noise.low_peak_window` of a render (mono or (channels, n)) from its first sample."""
    y = np.asarray(y)
    return AN.low_peak_window(y.mean(axis=0) if y.ndim == 2 else y, sr)


def level_fit(cases, rec, speeds, qs, knee, sr=SR, over=OVER_WEIGHT):
    """(amp, q) of AMPS × qs whose low-band peaks (strings + amp·v^q·knee factor·knock, per
    (note, dynamic) in `cases`: mono strings and unit knock from the strike) come nearest the
    recordings' (`rec`, dB): per dynamic the median over its notes against the recordings' median
    (one recording without a transient does not decide), rms over the dynamics, a median over the
    recordings' × `over` (the user preferred Grand D's knock 2–6 dB under its recordings to one up
    to 4 dB over). The band filter is linear: the strings' and the knock's band signals add."""
    from scipy.signal import butter, sosfiltfilt
    sos = butter(6, AN.LOW_BAND_HZ, btype="band", fs=sr, output="sos")
    nb, nn = int(AN.LOW_PEAK_S * sr), int(AN.NOTE_PEAK_S * sr)
    keys = [k for k in cases if k in rec and np.isfinite(rec[k])]
    pre = {}
    for k in keys:
        s, kn = (np.asarray(x)[:nn] for x in cases[k])
        pre[k] = (sosfiltfilt(sos, s)[:nb], sosfiltfilt(sos, kn)[:nb], s, kn)
    dyns = sorted({k[1] for k in keys})
    amps = np.asarray(AMPS)[:, None]
    best = (np.inf, None, None)
    for q in qs:
        err = np.zeros(len(AMPS))
        for d in dyns:
            ratios = []
            for k in (k for k in keys if k[1] == d):
                bs, bk, s, kn = pre[k]
                g = amps * speeds[d] ** q * knock_note_factor(k[0], knee)
                band = np.abs(bs[None] + g * bk[None]).max(axis=1)
                full = np.abs(s[None] + g * kn[None]).max(axis=1)
                ratios.append(20 * np.log10(band / np.maximum(full, 1e-30)))
            e = np.median(np.array(ratios), axis=0) - np.median([rec[k] for k in keys if k[1] == d])
            err += np.where(e > 0, over * e, e) ** 2
        j = int(np.argmin(err))
        if err[j] < best[0]:
            best = (err[j], float(AMPS[j]), q)
    return best[1], best[2]


def hammered(design):
    """Struck by hammers (no quill, finger or tangent): the impact instead of a knock."""
    return not (design in scale.PLUCKED or design in scale.TANGENT)
# one recorded layer: q is not identifiable, [strings] fixed_q (the harpsichords' ~10 dB per
# dynamic step, Penttinen 2006)
AMPS = 2.0 ** np.arange(-16, 6, 0.5)
FRAME_FLOOR_DB = -70.0
# all bands up to 16 kHz: above ~4 kHz the recordings have no attack transient, and a
# fit limited to 0.25–4 kHz let the knock spill broadband clicks up to 10 kHz
BAND_SEL = (np.array(AN.BANDS) >= 250) & (np.array(AN.BANDS) <= 16000)


def frame_error(syn, rec, noise, over=1.0):
    """Per band: synthetic vs recorded attack-frame level (dB re the frame's strongest
    band, both clipped at FRAME_FLOOR_DB), an overshoot × `over`. Where the recording
    is within 6 dB of its noise floor it only shows that nothing is there: only synthetic
    levels above it count."""
    syn = np.nan_to_num(syn, nan=FRAME_FLOOR_DB).clip(FRAME_FLOOR_DB)
    rec_c = np.nan_to_num(rec, nan=FRAME_FLOOR_DB).clip(FRAME_FLOOR_DB)
    at_floor = np.nan_to_num(rec - noise, nan=99.0) < 6
    d = syn - rec_c
    d = np.where(d > 0, over * d, d)
    return np.where(at_floor, np.maximum(d, 0), d)


def knock_note_factor(note, knee):
    return SB.knock_knee(note, knee)


def pad(y):
    return np.concatenate([np.zeros((2, int(PRE_S * SR))), y], axis=1).T


def string_powers(args):
    dataset, note, dyn = args
    os.environ["SOUNDBOARD_MODE"] = "full"
    prof = scale.Profile(dataset)
    y, _ = SB.render(prof, note, prof.hammer_speeds()[dyn], 0.4, stereo=True, knock=False)
    x = pad(y)
    onset = A.find_onset(x) / SR
    return (note, dyn), onset, AN.early_late(x, SR, onset)


def knock_powers(args):
    note, onset, decay, cutoff = args
    k = SB.knock_burst(note, 1.0, decay, cutoff, int(0.4 * SR))
    return (note, decay, cutoff), AN.early_late(pad(k), SR, onset)


def knock_error(strings, knocks, rec, speeds, decay, cutoff, q, amp, knee, over=1.0):
    """rms attack-frame error (dB) of the strings plus a knock: one grid point."""
    e = []
    for key, onset, (es, ls, lts) in strings:
        ek, lk, ltk = knocks[(key[0], decay, cutoff)]
        g = (amp * speeds[key[1]] ** q * knock_note_factor(key[0], knee)) ** 2
        p = es + g * ek
        syn = 10 * np.log10(np.maximum(p, 1e-30) / np.nanmax(p))
        r, nz = rec[key]
        e.append(frame_error(syn, r, nz, over)[BAND_SEL])
    return float(np.sqrt(np.mean(np.concatenate(e) ** 2)))


def grid_errors(strings, knocks, rec, speeds, qs, cutoffs=CUTOFFS, over=1.0):
    """knock_error over the whole grid (DECAYS × cutoffs × qs × AMPS × KNEES), every amp
    and recording of a (decay, cutoff, q, knee) cell in one array operation."""
    keys = [key for key, _, _ in strings]
    es = np.array([x[0] for _, _, x in strings])[None]                       # (1, R, B)
    r = np.array([rec[k][0] for k in keys])[None]
    nz = np.array([rec[k][1] for k in keys])[None]
    v = np.array([speeds[k[1]] for k in keys])
    amps = np.asarray(AMPS)[:, None]
    out = np.zeros((len(DECAYS), len(cutoffs), len(qs), len(AMPS), len(KNEES)))
    for a, decay in enumerate(DECAYS):
        for b, cutoff in enumerate(cutoffs):
            ek = np.array([knocks[(k[0], decay, cutoff)][0] for k in keys])[None]
            for c, q in enumerate(qs):
                for d, knee in enumerate(KNEES):
                    f = np.array([knock_note_factor(k[0], knee) for k in keys])
                    g = (amps * v ** q * f) ** 2                              # (A, R)
                    p = es + g[:, :, None] * ek
                    syn = 10 * np.log10(np.maximum(p, 1e-30) / np.nanmax(p, axis=2, keepdims=True))
                    err = frame_error(syn, r, nz, over)[:, :, BAND_SEL]
                    out[a, b, c, :, d] = np.sqrt(np.mean(err.reshape(len(AMPS), -1) ** 2, axis=1))
    return out


def impact_render(args):
    """A strike's strings alone (stereo, from the strike) and its unit impact through the board
    (gain 1, growth 0, open), the onset, and the strings' attack-frame band powers."""
    dataset, note, dyn = args
    os.environ["SOUNDBOARD_MODE"] = "full"
    prof = scale.Profile(dataset)
    v = prof.hammer_speeds()[dyn]
    y, info = SB.render(prof, note, v, 0.4, stereo=True, knock=False)
    f = np.zeros(y.shape[1])
    f[:len(info["force"])] = info["force"]
    unit = SB.radiate(prof, f, np.zeros_like(f), SR)[:, :y.shape[1]]
    x = pad(y)
    onset = A.find_onset(x) / SR
    return (note, dyn), (y, unit, onset, AN.band_power(x, SR, onset + AN.EARLY_S[0], onset + AN.EARLY_S[1])[0])


def lowpass(y, cutoff):
    from scipy.signal import butter, sosfilt
    return y if cutoff is None else sosfilt(butter(2, cutoff, fs=SR, output="sos"), y, axis=-1)


def impact_shape(renders, rec, speeds):
    """The cutoff (IMPACT_CUTOFFS) whose impact, softened below ff (IMPACT_GROWTH), best fills the
    attack frames from IMPACT_SHAPE_FROM up (frame_error, an overshoot × OVER_WEIGHT, the level
    free): (cutoff, its frame error)."""
    keys = [k for k in renders if k[0] >= IMPACT_SHAPE_FROM]
    vff = speeds["ff"]
    best = (np.inf, None)
    for cutoff in IMPACT_CUTOFFS:
        e_k = {}
        for k in keys:
            _, unit, onset, _ = renders[k]
            e_k[k] = AN.band_power(pad(lowpass(unit, cutoff)), SR, onset + AN.EARLY_S[0], onset + AN.EARLY_S[1])[0]
        for amp in AMPS:
            err = []
            for k in keys:
                g = (amp * (speeds[k[1]] / vff) ** IMPACT_GROWTH) ** 2
                p = renders[k][3] + g * e_k[k]
                syn = 10 * np.log10(np.maximum(p, 1e-30) / np.nanmax(p))
                err.append(frame_error(syn, *rec[k], OVER_WEIGHT)[BAND_SEL])
            e = float(np.sqrt(np.mean(np.concatenate(err) ** 2)))
            if e < best[0]:
                best = (e, cutoff)
    return best[1], best[0]


def fit_impact(profile, rows):
    """The impact block (soundboard.impact_force): the shape by the attack frame, the level by the
    treble's low-band peak (level_fit, growth 0) and then IMPACT_PREF_DB under it."""
    arr = lambda v: np.array([np.nan if x is None else x for x in v])  # noqa: E731
    rec = {(r["note"], r["dyn"]): (arr(r["frame_db"]), arr(r["noise_db"])) for r in rows if r["note"] >= IMPACT_SHAPE_FROM}
    low = {(r["note"], r["dyn"]): r["low_peak_db"] for r in rows
           if r.get("low_peak_db") is not None and r["note"] >= LOW_PEAK_FROM}
    with Pool() as pool:
        renders = dict(pool.map(impact_render, [(profile, n, d) for n, d in rec]))
    speeds = scale.Profile(profile).hammer_speeds()
    cutoff, err = impact_shape(renders, rec, speeds)
    cases = {k: (renders[k][0].mean(axis=0), lowpass(renders[k][1], cutoff).mean(axis=0)) for k in low if k in renders}
    g, _ = level_fit(cases, low, speeds, [0.0], knee=MIN_NOTE)
    return {"gain": float(g * 10 ** (IMPACT_PREF_DB / 20)), "cutoff_hz": cutoff, "growth": IMPACT_GROWTH,
            "err_db": err, "fitted_gain": float(g)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    args = ap.parse_args()
    rows = FH.P.load_cache(args.profile, "attack")["rows"]
    if hammered(scale.design_of(args.profile)):
        im = fit_impact(args.profile, rows)
        print(f"{args.profile}: impact {im['gain']:.4g} × felt force (fitted {im['fitted_gain']:.4g}, {IMPACT_PREF_DB:+.0f} dB), "
              f"lowpass {im['cutoff_hz'] or 'open'}, attack frame error {im['err_db']:.1f} dB")
        FH.P.update_fit(args.profile, {"impact": im}, drop=("knock",))
        print(f"wrote the impact block of {FH.P.fit_path(args.profile)}")
        return
    arr = lambda v: np.array([np.nan if x is None else x for x in v])  # noqa: E731
    rec = {(r["note"], r["dyn"]): (arr(r["frame_db"]), arr(r["noise_db"]))
           for r in rows if r["note"] >= MIN_NOTE}
    qs = scale.strings(args.profile).get("fixed_q", QS)
    with Pool() as pool:
        strings = pool.map(string_powers, [(args.profile, n, d) for n, d in rec])
        onset_of = {key[0]: o for key, o, _ in strings}
        knocks = dict(pool.map(knock_powers, [(n, o, d, c) for n, o in onset_of.items()
                                              for d in DECAYS for c in CUTOFFS]))
    speeds = scale.Profile(args.profile).hammer_speeds()

    e0 = knock_error(strings, knocks, rec, speeds, DECAYS[0], CUTOFFS[0], 1.0, 0.0, 21)
    grid = grid_errors(strings, knocks, rec, speeds, qs)      # (decay, cutoff, q, amp, knee)
    i = np.unravel_index(np.argmin(grid), grid.shape)   # the first minimum: as min() over the tuples
    best = (float(grid[i]), DECAYS[i[0]], CUTOFFS[i[1]], qs[i[2]], AMPS[i[3]], KNEES[i[4]])
    e, decay, cutoff, q, amp, knee = best
    print(f"{args.profile}: attack frame error {e0:.1f} dB (strings only) → {e:.1f} dB with knock")
    print(f"  knock: level {amp:.3g} · v^{q} from note {knee} up (fading out below), "
          f"decay {decay * 1000:.0f} ms, cutoff {cutoff:.0f} Hz")
    FH.P.update_fit(args.profile, {"knock": {"amp": amp, "q": q, "decay_s": decay, "cutoff_hz": cutoff, "knee_note": knee,
                                             "err_db": e, "err0_db": e0}})
    print(f"wrote the knock block of {FH.P.fit_path(args.profile)}")


if __name__ == "__main__":
    main()
