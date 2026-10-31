#!/usr/bin/env python3
"""Fit the diffuse room to the sound between the recorded partials.

Between a struck note's partials a recording holds a sound (measured as the median of the bins
between the partials, so a string ringing there does not count) that falls fast for the first
~0.15 s and slowly after. Measured per octave band re the partials' level at TIMES after the onset:
loudest dynamic, notes with f0 >= MIN_F0 (gaps wide enough at the analysis's resolution; the bands
below a note's fundamental are gaps whole).

The engine's gaps hold the room's response to the strings' onset. Against the recordings it
overfills the middle and treble (Grand A +9 … +12 dB at 2–4 kHz, design log step 123) and leaves
the bass short at the attack. The room is cut (eq_db, on top of its unit energy) per band where it
overfills: renders without and with the room (the strings and board alone: N) bring the room's part
to what the recording leaves over N, refined over ITERATIONS renders; where N alone is over the
recording the room is not the cause and loses at most KEEP_CUT_DB. The room keeps its decay
(measured over the first 15 dB, soundboard.room_t60) and its energy re the body. A band whose
recorded course decays slower than MIN_LATE_DB_S is the recording's noise floor: it sets no target.

Tried and dropped (step 123): raising the room's energy re the body (the partials pass through the
room too: the gaps' ratio to them saturates); an early part in the room (unlike H it colored the
partials at every microphone position but the one the compensation was fitted for); the room with
the recordings' late decay (far too much room at its energy); a strike, one sound added at every
note for the bass's shortfall, as diffuse noise or as a force pulse into the board (the same on
every key: heard as a click). The shortfall needs a source per note (the roadmap).

The partials go through the room too: comp_db (dB on the soundboard grid) gives back what the
room's cuts take from them (the full filter's smoothed spectrum without the cuts re with them), so
the cuts change the gaps only. note_params and the export add it to the per-note correction.

Writes fit "room" and cache "room" (the courses, recordings and renders).

Usage: fit_room.py --profile grand_e
"""
import argparse
import copy
import os
import sys
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.signal import get_window

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import fit_hammer as FH  # noqa: E402
import scale  # noqa: E402
import soundboard as SB  # noqa: E402

SR = 48000
NFFT, HOP, GUARD = 2048, 96, 5      # 43 ms frames, 2 ms hops; ±5 bins around each partial
MIN_F0 = 400.0
MAX_NOTES = 30
BANDS = [250, 500, 1000, 2000, 4000, 8000]            # octave bands
TIMES = [0.0, 0.02, 0.04, 0.08, 0.16, 0.24, 0.32, 0.48]
EARLY = (0.02, 0.08)
LATE = (0.16, 0.48)
ITERATIONS = 3
MAX_STEP_DB = 12.0                  # a cut's change per iteration
MAX_CUT_DB = 30.0                   # the room's deepest cut
KEEP_CUT_DB = 6.0                   # its deepest where the gaps without it are still over the recording:
                                    # the excess is the strings' or the board's own sound, not the room's
MIN_LATE_DB_S = 15.0                # slower late: the recording's noise floor, not a decay (no target)
SMOOTH_OCT = 1 / 6                  # the compensation: the filter's spectrum averaged over ±1/12 octave


def course(x, sr, f0, B):
    """The diffuse level between the partials re the partials' level (dB) per band at TIMES after
    the band's peak (within 0.1 s of the start), noise floor subtracted; NaN beyond the signal."""
    win = get_window("blackmanharris", NFFT)
    frames = (len(x) - NFFT) // HOP
    idx = np.arange(NFFT)[None, :] + HOP * np.arange(frames)[:, None]
    P = np.abs(np.fft.rfft(x[idx] * win, axis=1)) ** 2
    f = np.fft.rfftfreq(NFFT, 1 / sr)
    mask = np.ones(len(f), bool)
    n = np.arange(1, int(sr / 2 / f0) + 2)
    for p in n * f0 * np.sqrt(1 + B * n ** 2):
        if p < sr / 2:
            k = int(round(p * NFFT / sr))
            mask[max(k - GUARD, 0):k + GUARD + 1] = False
    first = int(0.1 * sr / HOP) + 1
    i_on = int(np.argmax(P[:first].sum(axis=1)))
    span = int(0.5 * sr / HOP)
    part = P[i_on:i_on + span, ~mask].sum() / span
    out = {}
    for fc in BANDS:
        sel = mask & (f >= fc / np.sqrt(2)) & (f < fc * np.sqrt(2))
        if sel.sum() < 4:
            continue
        e = np.median(P[:, sel], axis=1) * sel.sum()
        e = np.maximum(e - np.median(e[-max(frames // 10, 5):]), 1e-30)
        i0 = int(np.argmax(e[:first]))
        out[fc] = np.array([10 * np.log10(e[i0 + j:i0 + j + 2].mean() / part) if i0 + j + 2 < frames else np.nan
                            for j in (int(t * sr / HOP) for t in TIMES)])
    return out


def _in(lo_hi):
    t = np.array(TIMES)
    return (t >= lo_hi[0]) & (t <= lo_hi[1])


def usable(c):
    """The bands of a course with data at every point and a late decay faster than MIN_LATE_DB_S (a
    flat one is the recording's noise floor: a room fitted to it would add hiss)."""
    t = np.array(TIMES)
    out = []
    for fc in BANDS:
        r = c.get(fc)
        if r is None or not np.all(np.isfinite(r[_in(LATE)])) or not np.all(np.isfinite(r[_in(EARLY)])):
            continue
        if -np.polyfit(t[_in(LATE)], r[_in(LATE)], 1)[0] >= MIN_LATE_DB_S:
            out.append(fc)
    return out


def compensation(f, power, grid_hz, H_db):
    """Minus a filter's power spectrum (f, power), averaged over ±SMOOTH_OCT/2 around each grid
    point, re H (dB on grid_hz)."""
    lf = np.log2(np.maximum(f, 1e-9))
    e = np.array([power[np.abs(lf - np.log2(g)) <= SMOOTH_OCT / 2].mean() for g in grid_hz])
    return -(10 * np.log10(e) - np.asarray(H_db))


class _Room:
    """A fit with another room: every other key read from the fit as it is asked for (the driver
    checks each read)."""

    def __init__(self, fit, room):
        self.fit, self.room = fit, room

    def get(self, k, default=None):
        return self.room if k == "room" else self.fit.get(k, default)

    def __getitem__(self, k):
        return self.room if k == "room" else self.fit[k]

    def __contains__(self, k):
        return k == "room" or k in self.fit


def filter_comp(profile):
    """What the room's cuts take from the partials, given back: the full filter's smoothed spectrum
    (both polarizations, both channels) without the cuts re with them (0 without cuts)."""
    sb = profile.felt_fit["soundboard"]
    grid = np.array(sb["grid_hz"])

    def power(prof):
        p = 0
        for pol in ("v", "h"):
            h = SB.make_filter(prof, "full", SR, pol)
            p = p + (np.abs(np.fft.rfft(h, axis=1)) ** 2).sum(axis=0)
        return np.fft.rfftfreq(h.shape[1], 1 / SR), p

    room = profile.felt_fit["room"]
    if not any(v for _, v in room.get("eq_db", [])):
        return np.zeros(len(grid))
    uncut = copy.copy(profile)
    uncut.felt_fit = _Room(profile.felt_fit, {**room, "eq_db": []})
    f, p0 = power(uncut)
    _, p1 = power(profile)
    return compensation(f, p1, grid, -compensation(f, p0, grid, np.zeros(len(grid))))


def notes(dataset):
    """The loudest recordings with f0 >= MIN_F0, at most MAX_NOTES spread over the keys."""
    lo = FH.P.source(dataset).get("loud_from_layer")
    loud = FH.D.loudest(dataset)
    data = FH.P.load_cache(dataset, "analysis")["samples"]
    ok = [s for s in data if "error" not in s and "label_mismatch" not in s and s.get("f0", 0) >= MIN_F0
          and ((s["layer"] >= lo) if lo else FH.D.dynamic_of(dataset, s) == loud)]
    ok.sort(key=lambda s: s["note"])
    step = max(1, int(np.ceil(len(ok) / MAX_NOTES)))
    return ok[::step]


def recorded(job):
    dataset, s, tail = job
    x, sr = sf.read(FH.D.sample_path(dataset, s), always_2d=True)
    x = x.mean(axis=1)
    if tail:
        x = x[: len(x) - int(tail * sr)]
    on = int(s["onset_s"] * sr)
    return course(x[max(on - NFFT // 2, 0):], sr, s["f0"], s["B"])


_profile = None


def rendered(job):
    dataset, note = job
    global _profile
    if _profile is None:
        _profile = scale.Profile(dataset)
    loud = FH.D.loudest(dataset)
    y, _ = SB.render(_profile, note, _profile.hammer_speeds()[loud], max(TIMES) + 0.6, stereo=True, knock=False)
    p = SB.note_params(_profile, note)
    return course(np.concatenate([np.zeros(NFFT // 2), y.mean(axis=0)]), SR, p.f0, p.B)


def median(courses):
    return {fc: np.nanmedian(np.array([c[fc] for c in courses if fc in c]), axis=0) for fc in BANDS
            if sum(fc in c for c in courses) >= 3}


def room_doc(eq, comp=None):
    """The fit's room: its own level (eq_db, cuts; its decay stays the one measured over the first
    15 dB, its energy re the body ROOM_GAIN)."""
    doc = {"bands_hz": [float(b) for b in BANDS],
           "eq_db": [[float(b), round(float(eq[i]), 2)] for i, b in enumerate(BANDS)],
           "gain": SB.ROOM_GAIN}
    if comp is not None:
        doc["comp_db"] = [round(float(v), 2) for v in comp]
    return doc


def levels(c, window):
    """Mean power per band over the window's points (linear; NaN without the band)."""
    return np.array([np.nanmean(10 ** (c[b][_in(window)] / 10)) if b in c else np.nan for b in BANDS])


def cuts(R, O, N):
    """The room's cut per band (dB, <= 0): where the strings' onset drives the room past the recording
    (O over R), the room's part (O − N, N: rendered without the room) brought down to what the
    recording leaves over N, at most MAX_CUT_DB; where N alone is over R the room is not the cause and
    loses at most KEEP_CUT_DB."""
    want = (R - N) / np.maximum(O - N, 1e-30)
    c = np.where(R > N, 10 * np.log10(np.clip(want, 10 ** (-MAX_CUT_DB / 10), 1.0)), -KEEP_CUT_DB)
    return np.where(R >= O, 0.0, c)


def hold_untargeted(eq, targeted):
    """The cuts with each band that has no target (the recording's noise floor there) holding the cut
    of its nearest targeted band below, else above; none targeted: unchanged. Left at 0 dB, an
    untargeted top band sat uncut beside a deep cut and the room carried the top octave over the dry
    sound (Grand E's 8 kHz: a rattle)."""
    eq = np.array(eq, float)
    idx = np.flatnonzero(targeted)
    if not len(idx):
        return eq
    for i in np.flatnonzero(~np.asarray(targeted)):
        below = idx[idx < i]
        eq[i] = eq[below[-1]] if len(below) else eq[idx[idx > i][0]]
    return eq


def write(ds, eq):
    """The room into the fit, then its compensation (which the room's filter sets)."""
    FH.P.update_fit(ds, {"room": room_doc(eq)})
    comp = filter_comp(scale.Profile(ds))
    FH.P.update_fit(ds, {"room": room_doc(eq, comp)})


def renders(ds, keys):
    with Pool() as pool:
        return median(pool.map(rendered, [(ds, k) for k in keys]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    args = ap.parse_args()
    ds = args.profile
    picked = notes(ds)
    tail = FH.D.tail_cut(ds)
    with Pool() as pool:
        rec = median(pool.map(recorded, [(ds, s, s.get("tail_cut_s", tail)) for s in picked]))
    ok = usable(rec)
    rec = {b: rec[b] for b in ok}
    keys = [s.get("key", s["note"]) for s in picked]
    zero = np.zeros(len(BANDS))
    write(ds, zero)
    onset = renders(ds, keys)
    write(ds, np.full(len(BANDS), -60.0))
    bare = renders(ds, keys)
    R, O, N = (levels(c, LATE) for c in (rec, onset, bare))
    targeted = np.isfinite(R) & np.isfinite(O)
    eq = hold_untargeted(np.where(targeted, cuts(R, O, N), 0.0), targeted)
    print(f"{ds}: {len(picked)} notes; cuts " + " ".join(f"{v:.1f}" for v in eq))
    floor = np.where(R > N, -MAX_CUT_DB, -KEEP_CUT_DB)
    hist = []
    for it in range(ITERATIONS + 1):
        write(ds, eq)
        ren = renders(ds, keys)
        dl = np.array([np.nanmean((rec[b] - ren[b])[_in(LATE)]) if b in rec and b in ren else 0.0 for b in BANDS])
        de = np.array([np.nanmean((rec[b] - ren[b])[_in(EARLY)]) if b in rec and b in ren else 0.0 for b in BANDS])
        print(f"  {it}: rec − render, late " + " ".join(f"{v:+.1f}" for v in dl) + "; early "
              + " ".join(f"{v:+.1f}" for v in de) + "; cuts " + " ".join(f"{v:.1f}" for v in eq))
        hist.append({"late_db": dl.tolist(), "early_db": de.tolist(), "eq_db": eq.tolist()})
        if it == ITERATIONS:
            break
        # the cuts from the late error where the room overfills (a cut never above 0 dB)
        eq = np.where((eq < 0) | (dl < 0), np.clip(eq + np.clip(dl, -MAX_STEP_DB, MAX_STEP_DB), floor, 0.0), eq)
        eq = hold_untargeted(eq, targeted)
    FH.P.write_cache(ds, "room", {"times_s": TIMES, "bands_hz": BANDS,
                                  "recordings": {str(b): v.tolist() for b, v in rec.items()},
                                  "renders": {str(b): v.tolist() for b, v in ren.items()},
                                  "iterations": hist}, indent=1)
    print(f"wrote the room to {FH.P.fit_path(ds)}")


if __name__ == "__main__":
    main()
