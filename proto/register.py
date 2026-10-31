#!/usr/bin/env python3
"""Register balance and brightness on the radiated sound, synthesis vs recordings.

Per recorded note and dynamic (every `--step` semitones), the full render (string,
soundboard, room, knock: soundboard.render) and the recording are compared from
TONE_START to `--window` seconds after the onset (the tone: the knock is over by then,
and with it in the window the register fit leaned on the knock in the treble):
  - level: rms in dB. Only meaningful for sets that keep their recorded levels
    (the source's levels = "recorded"); both curves are shifted to median 0 over the keyboard.
  - band shape: octave-band levels relative to the band holding the fundamental.

This is the statistic for "muffled": the keyboard plot's partial centroid works on
partials 1–20 at the bridge and never saw a too-quiet treble or missing highs above
the 20th partial.

--fit (sets that keep their levels): the register gain, i.e. the level error averaged
over the dynamics, smoothed over the keyboard (REGISTER_SMOOTH) and stored in the
profile as a constant per note added to the soundboard correction C(f, note)
("register_gain_db" on its note anchors; the engine applies C per partial, so it needs
nothing new). The string model alone got the balance wrong for the upright: up to 30 dB
too weak in the treble re the bass, heard as a muffled, bass-heavy instrument. Run it
after soundboard.py (which rewrites C) and before fit_knock.py (the knock is fitted re
the string level).

Usage: register.py --profile upright_a [--step 3] [--window 0.5] [--fit]
"""
import json
import argparse
import os
from multiprocessing import Pool

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

import fit_hammer as FH
import sample_demo
import scale
import soundboard as SB

HERE = os.path.dirname(os.path.abspath(__file__))
SR = 48000
BANDS = 62.5 * 2 ** np.arange(9)          # octave centers 62.5 Hz … 16 kHz
TONE_START = 0.05                         # s after the onset
REGISTER_SMOOTH = 3.0                     # semitones (Gaussian width) of the register gain
# … per recorded note spacing above a whole tone: Upright Y (notes ~6 semitones apart)
# followed single loud recordings (G2 +8 dB) and clipped in the demo's ff chord
_profile = None


def band_levels(x):
    S = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / SR)
    return np.array([10 * np.log10(S[(f >= c / 2 ** 0.5) & (f < c * 2 ** 0.5)].sum() + 1e-30) for c in BANDS])


def recording(dataset, s, window):
    x, sr = sf.read(FH.D.sample_path(dataset, s), always_2d=True)
    x = x[int(s["onset_s"] * sr):].mean(axis=1) * 10 ** (-s.get("gain_db", 0.0) / 20)
    if sr != SR:
        x = resample_poly(x, SR, sr)
    return x[int(TONE_START * SR): int(window * SR)]


def can_fit(dataset):
    """Sets whose register balance can be fitted: their own levels, or a reference's."""
    return FH.D.levels_recorded(dataset) or FH.D.level_reference(dataset) is not None


def reference_sample(ref_map, note, dyn, max_dist=2):
    """The reference set's sample nearest to `note` at `dyn` (None beyond max_dist)."""
    cand = [n for n, d in ref_map if d == dyn and abs(n - note) <= max_dist]
    return ref_map[(min(cand, key=lambda n: abs(n - note)), dyn)] if cand else None


def run(args):
    dataset, note, dyn, s, window, rec = args
    global _profile
    if _profile is None:
        _profile = scale.Profile(dataset)
    y, _ = SB.render(_profile, note, _profile.hammer_speeds()[dyn], window + 0.1)
    y = y[int(TONE_START * SR): int(window * SR)]
    x = recording(*rec, window)   # the set's own recording, or the reference set's
    return note, dyn, [(10 * np.log10(np.mean(z ** 2) + 1e-30), band_levels(z)) for z in (x, y)]


def compare(dataset, step=3, window=0.5, reference=None):
    """Level and band shape per recorded note; with `reference` (a dataset), its recording
    of the nearest note stands for the recording (a normalized set's levels mean nothing)."""
    m = sample_demo.sample_map(dataset)
    ref = sample_demo.sample_map(reference) if reference else None
    notes = sorted({n for n, _ in m})
    pick = [n for n in notes if (n - notes[0]) % step == 0]
    jobs = []
    for n in pick:
        for d in ("mf", "ff"):
            if (n, d) not in m:
                continue
            rec = (dataset, m[(n, d)])
            if ref is not None:
                r = reference_sample(ref, n, d)
                if r is None:
                    continue
                rec = (reference, r)
            jobs.append((dataset, n, d, m[(n, d)], window, rec))
    with Pool() as pool:
        res = pool.map(run, jobs)
    out = {}
    for note, dyn, ((lr, br), (ls, bs)) in res:
        f0 = 440 * 2 ** ((note - 69) / 12)
        k = int(np.argmin(np.abs(np.log2(BANDS / f0))))
        out[(note, dyn)] = {"level": (lr, ls), "shape": (br - br[k], bs - bs[k]), "k0": k}
    return out


# beyond the recorded notes the level carries on with the recordings' smoothed trend over
# the outermost EXTRAP_SPAN recorded semitones, at most EXTRAP_SLOPE_MAX dB per semitone,
# and the gain is that level minus the model's own there (rendered at those keys). Held
# flat, the keys above a harpsichord's range (the Italian's ends at B5) grew louder by the
# model's own trend, 10–20 dB above its top notes at C7–C8; the local-linear smoother itself
# had run on to −93 dB at A7 (Harpsichord A, whose 61 keys stop at F6). Carrying the gain's
# own slope on (before) assumed the model flat out there: the lever harp's gain rose
# 8.5 dB over its top six recorded keys (the model's level falling there) and its keys above
# A6 then rose by the gain's slope and the model's own: ff peaks +10 dBFS at C8
EXTRAP_SPAN = 12
EXTRAP_SLOPE_MAX = 1.0


def register_gain(err, anchor_notes, frac, rec=None, model=None):
    """Register gain (dB) at the anchor notes from the level errors per recorded note
    (lists, averaged): smoothed over the keyboard (width `frac` semitones), shifted to
    median 0. Past the outermost recorded notes: with `rec` (recorded level per recorded
    note, lists) and `model` (the model's level at every anchor note and the outermost
    recorded notes) the recordings' trend of their last octave (EXTRAP_SPAN, clamped to
    EXTRAP_SLOPE_MAX; held where it would rise away from the range) minus the model's change;
    without them the gain's own trend."""
    notes = sorted(err)
    at = scale._smooth(notes, [np.mean(err[n]) for n in notes], [len(err[n]) for n in notes], frac=frac)
    lo, hi = notes[0], notes[-1]
    span = min(EXTRAP_SPAN, hi - lo)
    clamp = lambda s: float(np.clip(s, -EXTRAP_SLOPE_MAX, EXTRAP_SLOPE_MAX))  # noqa: E731
    trend = at
    if rec is not None and model is not None:
        trend = scale._smooth(notes, [np.mean(rec[n]) for n in notes], [len(rec[n]) for n in notes], frac=frac)
    up = clamp((trend(hi) - trend(hi - span)) / span) if span else 0.0
    down = clamp((trend(lo + span) - trend(lo)) / span) if span else 0.0
    if trend is not at:
        # no key beyond a recorded range louder than its outermost recorded one: the recorded
        # trend of the top octave rose on the concert harp (its short treble notes scatter),
        # Iowa and the LivingRoom, which put their top keys 5–10 dB over their last recorded note
        up, down = min(up, 0.0), max(down, 0.0)
    dm = (lambda n, edge: model[n] - model[edge]) if model is not None and rec is not None else (lambda n, edge: 0.0)  # noqa: E731

    def gain(n):
        if n > hi:
            return at(hi) + up * (n - hi) - dm(n, hi)
        if n < lo:
            return at(lo) - down * (lo - n) - dm(n, lo)
        return at(n)
    g = np.array([gain(n) for n in anchor_notes])
    return g - np.median(g)


def _model_level(args):
    dataset, note, dyn, window = args
    global _profile
    if _profile is None:
        _profile = scale.Profile(dataset)
    y, _ = SB.render(_profile, note, _profile.hammer_speeds()[dyn], window + 0.1)
    y = y[int(TONE_START * SR): int(window * SR)]
    return note, dyn, 10 * np.log10(np.mean(y ** 2) + 1e-30)


def model_levels(dataset, notes, window):
    """The model's level at notes (dB, the mean over mf and ff; only its changes are used)."""
    with Pool() as pool:
        res = pool.map(_model_level, [(dataset, n, d, window) for n in notes for d in ("mf", "ff")])
    by = {}
    for n, d, v in res:
        by.setdefault(n, []).append(v)
    return {n: float(np.mean(v)) for n, v in by.items()}


def check_finite(err):
    """Stop before a non-finite level error reaches the profile: the smoothing spreads one
    over every register gain."""
    bad = [n for n, v in sorted(err.items()) if not np.all(np.isfinite(v))]
    if bad:
        raise SystemExit(f"non-finite level errors (a diverged render?) at notes {bad}")


def fit(dataset, window):
    """Fit the register gain on every recorded note and store it (see the docstring)."""
    sb = FH.P.load_fit(dataset)["soundboard"]
    corr = sb["correction"]
    old = np.array(corr.pop("register_gain_db", np.zeros(len(corr["notes"]))))
    corr["C_db"] = (np.array(corr["C_db"]) - old[:, None]).tolist()
    FH.P.update_fit(dataset, {"soundboard": sb})          # the workers render without it
    out = compare(dataset, step=1, window=window, reference=FH.D.level_reference(dataset))
    err, rec = {}, {}
    for dyn in ("mf", "ff"):
        ks = [n for n, d in out if d == dyn]
        d = {n: out[(n, dyn)]["level"][0] - out[(n, dyn)]["level"][1] for n in ks}
        med = np.median(list(d.values()))
        rmed = np.median([out[(n, dyn)]["level"][0] for n in ks])
        for n, v in d.items():
            err.setdefault(n, []).append(v - med)
            rec.setdefault(n, []).append(out[(n, dyn)]["level"][0] - rmed)
    check_finite(err)
    notes = sorted(err)
    frac = REGISTER_SMOOTH * max(1.0, float(np.median(np.diff(notes))) / 2)
    beyond = [n for n in corr["notes"] if n < notes[0] or n > notes[-1]]
    model = model_levels(dataset, sorted(set(beyond) | {notes[0], notes[-1]}), window) if beyond else None
    g = register_gain(err, corr["notes"], frac, rec if model else None, model)
    corr["C_db"] = (np.array(corr["C_db"]) + g[:, None]).tolist()
    corr["register_gain_db"] = g.tolist()
    FH.P.update_fit(dataset, {"soundboard": sb})
    print("register gain (dB): " + ", ".join(f"{n}: {v:+.1f}" for n, v in zip(corr["notes"], g)))
    print(f"wrote {FH.P.fit_path(dataset)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    ap.add_argument("--step", type=int, default=3)
    ap.add_argument("--window", type=float, default=0.5)
    ap.add_argument("--tag", default="")
    ap.add_argument("--fit", action="store_true")
    args = ap.parse_args()
    if args.fit:
        if not can_fit(args.profile):
            raise SystemExit(f"{args.profile} does not keep its recorded levels and has no reference: nothing to fit")
        fit(args.profile, args.window)
    out = compare(args.profile, args.step, args.window)
    fig, axs = plt.subplots(3, 1, figsize=(12, 12))
    print("level: synth − recording (dB, median-matched per dynamic); "
          "highs: synth − recording, octave bands ≥ 4 kHz relative to the fundamental's band")
    for dyn, col in (("mf", "tab:blue"), ("ff", "tab:red")):
        ks = sorted(n for n, d in out if d == dyn)
        lr = np.array([out[(n, dyn)]["level"][0] for n in ks])
        ls = np.array([out[(n, dyn)]["level"][1] for n in ks])
        d = (ls - lr) - np.median(ls - lr)
        hi = []
        for n in ks:
            br, bs = out[(n, dyn)]["shape"]
            sel = (BANDS >= 4000) & (np.arange(len(BANDS)) > out[(n, dyn)]["k0"])
            # energy sum over the high bands, relative to the fundamental band
            e = lambda b: 10 * np.log10(np.sum(10 ** (b[sel] / 10)) + 1e-30) if np.any(sel) else np.nan
            hi.append(e(bs) - e(br))
        hi = np.array(hi)
        print(f"{dyn}: level error rms {np.sqrt(np.mean(d ** 2)):.1f} dB (treble ≥ C6: "
              f"{np.mean(d[np.array(ks) >= 84]):+.1f}), highs median {np.nanmedian(hi):+.1f} dB")
        for n, a, b in zip(ks, d, hi):
            print(f"  {dyn} {n:3d}  level {a:+6.1f}  highs {b:+6.1f}")
        axs[0].plot(ks, lr - np.median(lr), "s--", color=col, label=f"recording {dyn}")
        axs[0].plot(ks, ls - np.median(ls), "o-", color=col, label=f"synth {dyn}")
        axs[1].plot(ks, hi, "o-", color=col, label=dyn)
    axs[0].set(title=f"{args.profile}: level over the first {args.window} s (median 0)", ylabel="dB")
    axs[1].axhline(0, color="gray")
    axs[1].set(title="highs (bands ≥ 4 kHz re fundamental band): synth − recording", ylabel="dB")
    # mean band shape over the keyboard, by register
    for (lo, hi_), col in (((21, 48), "tab:brown"), ((48, 72), "tab:green"), ((72, 109), "tab:purple")):
        sel = [k for k in out if lo <= k[0] < hi_ and k[1] == FH.D.loudest(args.profile)]
        if sel:
            diff = np.array([out[k]["shape"][1] - out[k]["shape"][0] for k in sel])
            axs[2].plot(BANDS, np.median(diff, axis=0), "o-", color=col, label=f"notes {lo}–{hi_ - 1} ff")
    axs[2].set(xscale="log", title="band shape synth − recording (median, re fundamental band)", xlabel="Hz",
               ylabel="dB")
    axs[2].axhline(0, color="gray")
    for ax in axs:
        ax.grid(alpha=0.3)
        ax.legend()
    path = os.path.join(HERE, "out", f"register_{args.profile}{args.tag}.png")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fig.savefig(path, dpi=70, bbox_inches="tight")
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
