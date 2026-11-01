#!/usr/bin/env python3
"""Split a recording session's passes (docs/recording-a-piano.md: one long file per pass, the keys
in the sheet's order, each struck and rung out) into one file per strike, named by key, dynamic and
take, and write an SFZ file over the takes kept (the set's map, its release regions the dampers'
and the release noise's source).

Strikes: the broadband level (30 Hz up, FRAME_S frames, SMOOTH_S smoothed) rising RISE_DB over
its lowest of the RISE_BEFORE_S before. A rise is a strike when a harmonic series rises with it:
`rise_scores`, each key's comb score on the spectrum's rise over CHECK_S after it re the PRE_S
before, less the same between the partials (a note still ringing scores nothing; a bass note's dense
spectrum no treble key's, its odd partials no octave's; the comb's floor a local median, the room's rumble
no series), at least NOTE_SCORE, and not within ECHO_S after a strike and GHOST_DB under it; else a
noise (a chair, a breath, a pedal, a hand on the ringing case), which ends the take before it where
that take has died to within TAIL_DB of the floor. A strike's sample is the steepest rise of a
FINE_S level near the rise (`refine`).

Labels: within a pass the sheet's keys rise, each struck once or more; the strikes are labeled
together (each key's score re the strike's best, the in-order labeling with the largest total, a
key repeated for its takes). A strike whose own score at a key within NEIGHBOR semitones of its
label is WRONG_DB over its label's is a wrong note.

A take is left out (its file under discarded/, its reason in the JSON):
- cut: against the median decay of its key's and its neighbors' takes (each re its level over
  LEVEL_S), within the first SHORT_FRAC of the time that decay lies TAIL_DB over the floor, it
  falls CUT_DB under it and stays there (stopped while it rang: a mis-strike, replayed) or its file
  ends while it still sounds (`cut_short`); a long take stopped near its end is no cut;
- soft: GHOST_DB under its key's loudest take (a key touched, not struck);
- wrong: a wrong note;
- clipped: a sample at full scale or a flat top near it (`clipped`), where its key has another take
  at its dynamic;
- spare: past the first MAX_TAKES of its key and dynamic.
The kept takes of a key and dynamic are ranked by how near their level (over LEVEL_S) lies to the
mean of the neighboring keys' (the sheet: "the more even one kept"); the SFZ gives the first rank
seq_position 1, so a map's seq_position filter plays it and `skip_files` falls back to the next.

Releases (`--releases`): a pass of notes held a few seconds and released. Each take's release is
its steepest fall (RELEASE_FALL_DB within RELEASE_FALL_S, past RELEASE_AFTER_S), and the file
written from REL_PRE_S before it (release/<key>_<dynamic>_<take>.wav): the SFZ's release regions.
`--damped-to KEY` leaves out the keys above it (no damper: their release stops nothing).

The pedal pass (`--pedal`): its events as pedal/down_<n>.wav and pedal/up_<n>.wav by the strings'
ring after each (`split_pedal`), the files a source's `pedal = { kind = "files", … }` names
(analysis/pedal_noise.py).

The keys around the breaks (the sheet's second table), played at various velocities: `--dynamic
auto` gives each take the dynamic whose level at its key the sheet's passes already split into
<out> put nearest (`assign_dynamics`), and leaves one out that lies between them.

The silent releases (`--silent`): keys pressed without their note and released, the key's return
and the damper landing on still strings alone (`split_silent`): no pitch to label them by, so in
the order of `--keys`, one key per event; an event within SAME_KEY_S of the one before is that key
again (a second take); a note struck by mistake (a series as a strike's), an event the file's end
cuts (the recorder's stop) and every event before `--from` are left out. Written to
release_silent/<key>_<take>.wav, at most SILENT_S long, outside the SFZ (`release_noise.py` reads
them).

A tongue drum (`--tongues`): a strike is labeled by its fundamental alone over its first
TONGUE_CHECK_S (the tongues' upper modes are no series, the drum's other tongues ring in sympathy
within half a second, the one a fifth above among them), and a rise GHOST_DB under the strike
before while that still rings TAIL_DB over the floor is the ring's (a touch), no strike.

A pass recorded at another gain (`--gain-db`, the recorder's level turned down for loud takes):
its takes measured and written that much louder, at the session's gain (as float where that
passes full scale); a clipped take is judged on the samples as recorded.

A second pedal pass continues the numbering of the files already in pedal/ (split again, a pass
keeps its numbers: its JSON's `first`); the left-out events' files carry the pass's name.

Usage:
  split_session.py <pass.wav> <out> --dynamic pp [--keys 21:108:3]
  split_session.py <releases.wav> <out> --dynamic mf --releases [--damped-to 88]
  split_session.py <breaks.wav> <out> --dynamic auto --keys 50,88,89   (after the sheet's passes)
  split_session.py <sustain_pedal.wav> <out> --pedal
  split_session.py <key_releases.wav> <out> --silent --from 30 [--keys 21:108:3]
  split_session.py <redos.wav> <out> --dynamic auto --keys 81,93,108 --gain-db 4.3
  split_session.py <mf.wav> <out> --dynamic mf --tongues --keys 60,72,62,74,…   (the drum's order)
  split_session.py --sfz <out> [--name set.sfz]     (the SFZ over every pass split into <out>)
"""
import argparse
import glob
import json
import os
import re

import numpy as np
import soundfile as sf
from scipy.ndimage import maximum_filter1d, minimum_filter1d, percentile_filter, uniform_filter1d
from scipy.signal import butter, find_peaks, sosfiltfilt

import analyze as A

FRAME_S = 0.005
SMOOTH_S = 0.02
HIGHPASS_HZ = 30.0
RISE_DB = 15.0
RISE_BEFORE_S = (0.03, 0.3)
MIN_GAP_S = 0.5
FINE_S = 0.002
RISE_FINE_S = 0.01
REFINE_S = (0.3, 0.03)
REFINE_AFTER_S = 0.05
CHECK_S = (0.03, 1.0)       # the spectrum after a rise …
PRE_S = (-0.5, -0.02)       # … re the one before it
MID_WEIGHT = 0.5
NOTE_SCORE = 12.0           # the mean rise (dB over the local floor) at the series' partials
N_PARTIALS = 8
CENTS = (-40, -20, 0, 20, 40)
B_GRID = (0.0, 1e-4, 4e-4, 1.5e-3, 5e-3)
TOL = 0.012                 # a partial's peak within this fraction of its frequency
FLOOR_HZ = 150.0            # the comb's floor: the median over this band around each bin
NEIGHBOR = 2
WRONG_DB = 6.0
TAIL_DB = 10.0
CUT_LAST_S = 0.3
SHORT_FRAC = 0.5
CURVE_S = 0.1
CUT_DB = 15.0
CUT_HOLD_S = 1.0
GHOST_DB = 20.0
ECHO_S = 2.0
FLOOR_PCT = 5
MAX_TAKES = 4
LEVEL_S = (0.0, 0.5)
CLIP = 0.999               # a sample within 0.01 dB of full scale
FLAT_DB = 1.0              # … or a flat top within this of it: the recorder's own ceiling (0.993)
FLAT_RUN = 3
WRITE_PRE_S = 0.05
RELEASE_FALL_DB = 10.0
RELEASE_FALL_S = 0.3
RELEASE_AFTER_S = 0.5
REL_PRE_S = 0.05
KEYS_MIN, KEYS_MAX = 21, 108
PEDAL_RING_HZ = (56.0, 280.0)
PEDAL_RING_DB = 3.0
PEDAL_BEFORE_S = (-1.0, -0.1)
PEDAL_AFTER_S = (0.5, 1.5)
PEDAL_TAIL_S = 3.0
AUTO_TOL_DB = 4.0
TONGUE_CHECK_S = (0.03, 0.2)
SAME_KEY_S = 3.0
SILENT_S = 2.0
NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def key_name(k):
    return f"{NAMES[k % 12]}{k // 12 - 1}"


def level_db(x, sr):
    """The broadband level (mono, 30 Hz up) in FRAME_S frames, smoothed over SMOOTH_S (dB)."""
    m = x.mean(axis=1) if x.ndim == 2 else x
    m = sosfiltfilt(butter(2, HIGHPASS_HZ, "highpass", fs=sr, output="sos"), m)
    h = int(FRAME_S * sr)
    n = len(m) // h
    p = (m[: n * h] ** 2).reshape(n, h).mean(axis=1)
    return 10 * np.log10(uniform_filter1d(p, max(int(SMOOTH_S / FRAME_S), 1)) + 1e-20)


def rises(e):
    """(frame, rise dB) of every rise of RISE_DB over the lowest of the RISE_BEFORE_S before."""
    a, b = int(RISE_BEFORE_S[0] / FRAME_S), int(RISE_BEFORE_S[1] / FRAME_S)
    w = b - a
    low = minimum_filter1d(e, w, origin=(w - 1) // 2)        # the lowest of e[i-w+1 … i]
    before = np.r_[np.full(a, e[0]), low[:-a]] if a else low
    r = e - before
    pk, _ = find_peaks(r, height=RISE_DB, distance=max(int(MIN_GAP_S / FRAME_S), 1))
    return [(int(i), float(r[i])) for i in pk]


def _spectrum_db(seg, sr, nfft):
    win = np.hanning(len(seg))
    return 10 * np.log10(np.abs(np.fft.rfft(seg * win, nfft)) ** 2 / np.sum(win ** 2) + 1e-30)


def rise_scores(m, sr, at, end=None, tongues=False):
    """Per key 21–108: the comb score of the spectrum's rise at sample `at` (mono `m`): over its
    first N_PARTIALS partials (the cents and B grids), the mean of each partial's peak over the
    local floor after the rise, at most its rise over the spectrum before it, less the same between
    the partials (dB). `tongues`: the fundamental alone over TONGUE_CHECK_S, nothing between."""
    check = TONGUE_CHECK_S if tongues else CHECK_S
    n_partials, mid_weight = (1, 0.0) if tongues else (N_PARTIALS, MID_WEIGHT)
    a, b = at + int(check[0] * sr), at + int(check[1] * sr)
    if end is not None:
        b = min(b, end)
    if b - a < int(0.1 * sr):
        return {k: 0.0 for k in range(KEYS_MIN, KEYS_MAX + 1)}
    nfft = 1 << int(np.ceil(np.log2((b - a) * 4)))
    bin_hz = sr / nfft
    after = _spectrum_db(m[a:b], sr, nfft)
    pa, pb = max(at + int(PRE_S[0] * sr), 0), max(at + int(PRE_S[1] * sr), 0)
    before = _spectrum_db(m[pa:pb], sr, nfft) if pb - pa > int(0.05 * sr) else np.full_like(after, -300.0)
    step = 8
    # the floor's band as many resolution cells wide as over CHECK_S (a short window's peaks are wider)
    floor_hz = FLOOR_HZ * (CHECK_S[1] - CHECK_S[0]) / (check[1] - check[0])
    w = max(int(floor_hz / bin_hz / step), 3)
    floor = np.interp(np.arange(len(after)), np.arange(0, len(after), step), percentile_filter(after[::step], 50, size=w))
    lvl = np.clip(np.minimum(after - floor, after - before), 0, None)
    n = np.arange(1, n_partials + 1)
    out = {}
    for k in range(KEYS_MIN, KEYS_MAX + 1):
        f0 = A.midi_to_hz(k)
        lv = maximum_filter1d(lvl, size=2 * int(TOL * f0 / bin_hz) + 1)
        best = 0.0
        for c in CENTS:
            for B in B_GRID:
                F = f0 * 2 ** (c / 1200) * n * np.sqrt(1 + B * n * n)
                ok = F < 0.45 * sr
                if not ok.any():
                    continue
                s = lv[np.rint(F[ok] / bin_hz).astype(int)]
                # between the partials: a dense spectrum (a bass note's under a treble key, the odd
                # partials under its octave) fills the gaps as well as the series
                mid = lv[np.rint(F[ok] * (n[ok] + 0.5) / n[ok] / bin_hz).astype(int).clip(0, len(lv) - 1)]
                # the treble: a series without its fundamental is another key's (an octave below
                # takes every other partial of the one struck)
                sc = (s.mean() if k < 72 else 0.5 * (s.mean() + s[0])) - mid_weight * mid.mean()
                best = max(best, float(sc))
        out[k] = best
    return out


def label(S, keys):
    """The in-order labels (indices into `keys`, non-decreasing) of the strikes with the largest
    total score; S[i][j]: strike i's score at keys[j] re its best."""
    n, m = len(S), len(keys)
    best = np.full((n, m), -np.inf)
    back = np.zeros((n, m), int)
    best[0] = S[0]
    for i in range(1, n):
        run = np.maximum.accumulate(best[i - 1])
        arg = np.array([int(np.argmax(best[i - 1][: j + 1])) for j in range(m)])
        best[i] = run + S[i]
        back[i] = arg
    j = int(np.argmax(best[-1]))
    out = [j]
    for i in range(n - 1, 0, -1):
        j = back[i, j]
        out.append(j)
    return out[::-1]


def fine_level(m, sr):
    """The level (30 Hz up) in FINE_S frames (dB)."""
    m = sosfiltfilt(butter(2, HIGHPASS_HZ, "highpass", fs=sr, output="sos"), m)
    f = int(FINE_S * sr)
    n = len(m) // f
    return 10 * np.log10((m[: n * f] ** 2).reshape(n, f).mean(axis=1) + 1e-20)


def refine(fine, sr, at, after=0):
    """A strike's sample: the frame before the steepest rise over RISE_FINE_S on the fine level within
    REFINE_S of the rise's frame (that frame lies up to 0.17 s after the hammer: the smoothed level
    peaks late, and a key's thump comes before it), not before the sample `after`."""
    f = int(FINE_S * sr)
    lf = int(RISE_FINE_S / FINE_S)
    c = at // f
    a = max(c - int(REFINE_S[0] / FINE_S), lf, after // f + lf)
    b = min(c + int(REFINE_S[1] / FINE_S), len(fine))
    if b <= a:
        return at
    r = fine[a:b] - fine[a - lf:b - lf]
    return (a + int(np.argmax(r)) - 1) * f


def strikes(x, sr, tongues=False):
    """The strikes and the noises of a pass: dicts with the sample, the rise, the key scores."""
    m = x.mean(axis=1)
    e = level_db(x, sr)
    h = int(FRAME_S * sr)
    found = []
    rs = rises(e)
    fine = fine_level(m, sr)
    # each searched after the previous rise's frame (a weak rise just after a strike, the strike's own)
    ats = [refine(fine, sr, i * h, rs[n - 1][0] * h + int(REFINE_AFTER_S * sr) if n else 0) for n, (i, _) in enumerate(rs)]
    for n, ((i, r), at) in enumerate(zip(rs, ats)):
        nxt = ats[n + 1] - int(0.02 * sr) if n + 1 < len(rs) else None
        sc = rise_scores(m, sr, at, nxt, tongues)
        found.append({"at": at, "rise_db": r, "scores": sc, "note": max(sc.values()) >= NOTE_SCORE})
    return e, found


def at_gain(x, gain_db):
    """The samples gain_db louder."""
    return x * 10 ** (gain_db / 20) if gain_db else x


def split_pass(x, sr, keys, release=False, mixed=False, tongues=False, gain_db=0.0):
    """The pass's takes: a dict per strike (key, take, start/end samples, the reasons it is left out).
    `mixed`: the keys struck at various velocities (no soft or spare takes); `tongues`: a tongue
    drum's; `gain_db`: measured that much louder (a clipped take still judged as recorded)."""
    recorded, x = x, at_gain(x, gain_db)
    e, found = strikes(x, sr, tongues)
    h = int(FRAME_S * sr)
    floor = float(np.percentile(e, FLOOR_PCT))
    m = x.mean(axis=1)
    lv = lambda at: 10 * np.log10(np.mean(m[at:at + int(LEVEL_S[1] * sr)] ** 2) + 1e-20)  # noqa: E731
    hits = []
    for s in found:
        # a rise within ECHO_S of a strike and GHOST_DB under it is the ringing note's (a damper
        # touching, a hand on the case), no strike
        if s["note"] and hits and lv(s["at"]) < lv(hits[-1]["at"]) - GHOST_DB:
            ringing = tongues and e[max(s["at"] // h - int(0.3 / FRAME_S), 0):max(s["at"] // h - 2, 1)].mean() > floor + TAIL_DB
            if s["at"] - hits[-1]["at"] < ECHO_S * sr or ringing:
                s["note"] = False
        if s["note"]:
            hits.append(s)
    noises = [s["at"] for s in found if not s["note"]]
    if not hits:
        return [], floor
    S = np.array([[s["scores"][k] for k in keys] for s in hits])
    S -= S.max(axis=1, keepdims=True)
    labels = label(S, keys)
    takes = []
    for i, (s, j) in enumerate(zip(hits, labels)):
        k = keys[j]
        nxt = hits[i + 1]["at"] if i + 1 < len(hits) else len(x)
        # each take ends WRITE_PRE_S before the next strike or noise (where the next file begins)
        pre = int(WRITE_PRE_S * sr)
        end = nxt - pre if nxt < len(x) else nxt
        # a noise after the note has died ends it there
        for q in noises:
            if s["at"] + int(CHECK_S[1] * sr) < q < end and e[q // h - int(0.3 / FRAME_S):q // h - 2].mean() < floor + TAIL_DB:
                end = q - pre
                break
        # the scores again over the take alone (a strike right before the next)
        sc = s["scores"] if nxt - s["at"] > CHECK_S[1] * sr else rise_scores(m, sr, s["at"], nxt, tongues)
        why = []
        near = {q: sc[q] for q in range(k - NEIGHBOR, k + NEIGHBOR + 1) if q in sc}
        own = max(near, key=near.get)
        if own != k and near[own] - sc[k] >= WRONG_DB:
            why.append(f"wrong: sounds {key_name(own)}")
        seg = x[max(s["at"] - pre, 0):end]           # as written (the strike may lie a little early)
        if clipped(recorded[max(s["at"] - pre, 0):end]):
            why.append("clipped")
        a, b = s["at"] + int(LEVEL_S[0] * sr), s["at"] + int(LEVEL_S[1] * sr)
        t = {"key": k, "name": key_name(k), "start": s["at"], "end": end, "rise_db": s["rise_db"],
             "score": sc[k], "level_db": float(10 * np.log10(np.mean(m[a:b] ** 2) + 1e-20)),
             "peak_dbfs": float(A.db(np.abs(seg).max())), "length_s": (end - s["at"]) / sr,
             "curve": curve(e[s["at"] // h:end // h]),
             "end_db": float(e[max(end // h - int(CUT_LAST_S / FRAME_S), 0):max(end // h - 2, 1)].mean() - floor),
             "why": why}
        if release:
            t.update(release_at(x, sr, s["at"], end))
        takes.append(t)
    number(takes, floor, cut=not release, mixed=mixed)
    return takes, floor


def clipped(seg):
    """A sample at full scale (CLIP), or in a channel FLAT_RUN equal samples in a row at its peak
    within FLAT_DB of full scale (a ceiling of the recorder's own: the converter's last code)."""
    a = np.abs(seg if seg.ndim == 2 else seg[:, None])
    if a.max() >= CLIP:
        return True
    for c in a.T:
        top = c.max()
        if top >= 10 ** (-FLAT_DB / 20):
            run = np.convolve((c == top).astype(int), np.ones(FLAT_RUN, int), mode="valid")
            if (run == FLAT_RUN).any():
                return True
    return False


def curve(e):
    """A take's level (dB) per CURVE_S from its strike, smoothed over CURVE_S."""
    n = max(int(CURVE_S / FRAME_S), 1)
    m = len(e) // n
    return (10 * np.log10((10 ** (e[: m * n] / 10)).reshape(m, n).mean(axis=1) + 1e-30)).tolist()


def cut_short(t, ref, floor):
    """Why a take is cut short against its key's reference decay `ref` (dB re the take's level per
    CURVE_S), or None. Where the reference lies TAIL_DB over the floor (its sounding time), within
    the first SHORT_FRAC of it: the take falls CUT_DB under it and stays there CUT_HOLD_S (stopped
    while it rang; a beat's dip comes back), or its file ends there still TAIL_DB over the floor
    (the next strike or the file's end came early). A take that dies sooner on its own, or one
    stopped late, is no cut."""
    c = np.asarray(t["curve"])
    r = t["level_db"] + ref
    sounding = np.nonzero(r > floor + TAIL_DB)[0]
    if not len(sounding):
        return None
    until = int(SHORT_FRAC * (sounding[-1] + 1))
    hold = int(CUT_HOLD_S / CURVE_S)
    below = c[1:min(len(c), len(r))] < r[1:min(len(c), len(r))] - CUT_DB
    for i in np.nonzero(below)[0]:
        if i + 1 > until:
            break
        if below[i:i + hold].all():
            return f"cut: {CUT_DB:.0f} dB under its key's decay from {(i + 1) * CURVE_S:.1f} s"
    if len(c) < until and t["end_db"] > CUT_DB:
        return f"cut: ends at {len(c) * CURVE_S:.1f} s still {t['end_db']:.0f} dB over the floor"
    return None


def number(takes, floor, cut=True, mixed=False):
    """Each take's number within its key (the order struck); a take cut short against the median
    decay (re each take's level) of its key's and its neighbors' takes (`cut_short`), one GHOST_DB
    under its key's loudest soft; a clipped take kept where its key has no other; past MAX_TAKES
    spare; then the kept ones ranked (`rank`) by evenness."""
    by_key = {}
    for t in takes:
        by_key.setdefault(t["key"], []).append(t)
    keys = sorted(by_key)
    loudest = {k: max(t["level_db"] for t in by_key[k]) for k in keys}
    for i, k in enumerate(keys):
        group = [t for q in keys[max(i - 1, 0):i + 2] for t in by_key[q] if not t["why"]]
        n = max(len(t["curve"]) for t in group) if group else 0
        rel = np.full((len(group), n), np.nan)
        for j, t in enumerate(group):
            rel[j, :len(t["curve"])] = np.asarray(t["curve"]) - t["level_db"]
        ref = np.nanmedian(rel, axis=0) if group else np.zeros(0)
        for n, t in enumerate(by_key[k], 1):
            t["take"] = n
            why = cut_short(t, ref, floor) if cut and len(ref) else None
            if why:
                t["why"].append(why)
            if not mixed and t["level_db"] < loudest[k] - GHOST_DB:
                t["why"].append(f"soft: {loudest[k] - t['level_db']:.0f} dB under its key's loudest")
    for k, ts in by_key.items():
        good = [t for t in ts if not t["why"]]
        if not good:
            clipped = [t for t in ts if t["why"] == ["clipped"]]
            for t in clipped[:1]:
                t["why"] = []
                t["note"] = "clipped, its key's only take"
        kept = [t for t in ts if not t["why"]]
        for t in [] if mixed else kept[MAX_TAKES:]:
            t["why"].append("spare")
    mean = {k: np.mean([t["level_db"] for t in by_key[k] if not t["why"]] or [np.nan]) for k in keys}
    for i, k in enumerate(keys):
        nb = [mean[q] for q in keys[max(i - 1, 0):i + 2] if q != k and np.isfinite(mean[q])]
        ref = np.mean(nb) if nb else mean[k]
        kept = sorted((t for t in by_key[k] if not t["why"]), key=lambda t: abs(t["level_db"] - ref))
        for r, t in enumerate(kept, 1):
            t["rank"] = r


def release_at(x, sr, at, end):
    """The release within a take: its steepest fall of RELEASE_FALL_DB within RELEASE_FALL_S, past
    RELEASE_AFTER_S after the strike (dict with `release`, the sample, and its fall)."""
    e = level_db(x[at:end], sr)
    lag = int(RELEASE_FALL_S / FRAME_S)
    a = int(RELEASE_AFTER_S / FRAME_S)
    if len(e) <= a + lag:
        return {"release": None}
    # the fall to each frame from the highest of the lag frames before it
    hi = maximum_filter1d(e, lag, origin=(lag - 1) // 2)
    fall = np.r_[np.zeros(1), hi[:-1]] - e
    i = a + int(np.argmax(fall[a:]))
    if fall[i] < RELEASE_FALL_DB:
        return {"release": None}
    # the fall's start: the last frame before it within 1 dB of the highest
    seg = e[i - lag:i]
    j = i - lag + int(np.nonzero(seg >= seg.max() - 1.0)[0][-1])
    return {"release": at + j * int(FRAME_S * sr), "release_fall_db": float(fall[i])}


def sheet_levels(out):
    """{dynamic: {key: mean level of its kept takes}} over the sheet's passes split into `out`."""
    ref = {}
    for path in glob.glob(os.path.join(out, "*.split.json")):
        doc = json.load(open(path))
        if doc.get("releases") or doc.get("pedal") or doc.get("silent") or doc["dynamic"] == "auto":
            continue
        for t in doc["takes"]:
            if not t["why"]:
                ref.setdefault(doc["dynamic"], {}).setdefault(t["key"], []).append(t["level_db"])
    return {d: {k: float(np.mean(v)) for k, v in ks.items()} for d, ks in ref.items()}


def assign_dynamics(takes, levels):
    """A pass at various velocities (around the breaks): each kept take's dynamic the one whose level
    at its key (the sheet's takes, interpolated between the nearest keys either side) lies nearest,
    within AUTO_TOL_DB; then ranked per key and dynamic by that distance."""
    for t in takes:
        if t["why"]:
            continue
        near = {}
        for d, ks in levels.items():
            lo = [k for k in ks if k <= t["key"]]
            hi = [k for k in ks if k >= t["key"]]
            if not lo or not hi:
                continue
            a, b = max(lo), min(hi)
            ref = ks[a] if a == b else ks[a] + (ks[b] - ks[a]) * (t["key"] - a) / (b - a)
            near[d] = t["level_db"] - ref
        if not near:
            t["why"].append("level: no sheet takes either side")
            continue
        d = min(near, key=lambda q: abs(near[q]))
        if abs(near[d]) > AUTO_TOL_DB:
            t["why"].append(f"level: {near[d]:+.1f} dB from {d}, the nearest dynamic")
            continue
        t["dynamic"], t["off_db"] = d, float(near[d])
    groups = {}
    for t in takes:
        if not t["why"]:
            groups.setdefault((t["key"], t["dynamic"]), []).append(t)
    for ts in groups.values():
        for r, t in enumerate(sorted(ts, key=lambda t: abs(t["off_db"])), 1):
            t["rank"] = r


def write_takes(x, sr, takes, out, dynamic, release=False, stem=""):
    """The takes' files: <dynamic>/<key>_<dynamic>_<take>.wav (a take of `auto`: its own dynamic, the
    pass's name before its number), the left-out ones under discarded/."""
    pre = int(WRITE_PRE_S * sr)
    for t in takes:
        dyn = t.get("dynamic", dynamic)
        base = f"{t['name']}_{dyn}_{stem if dynamic == 'auto' else ''}{t['take']}.wav"
        if release:
            if t.get("release") is None:
                t["why"].append("no release found")
            folder = "release" if not t["why"] else os.path.join("discarded", "release")
            a = t["release"] - int(REL_PRE_S * sr) if t.get("release") is not None else t["start"] - pre
        else:
            folder = dyn if not t["why"] else os.path.join("discarded", dyn)
            a = t["start"] - pre
        os.makedirs(os.path.join(out, folder), exist_ok=True)
        t["file"] = os.path.join(folder, base)
        seg = x[max(a, 0):t["end"]]
        sf.write(os.path.join(out, t["file"]), seg, sr, subtype="FLOAT" if np.abs(seg).max() >= 1.0 else "PCM_24")


def split_pedal(x, sr):
    """A pedal pass's events: each rise (no pitch asked), a pedal-down where the strings' band
    PEDAL_RING_HZ rings PEDAL_RING_DB louder over PEDAL_AFTER_S than over PEDAL_BEFORE_S (the dampers
    lifted, the strings set ringing), a pedal-up where it falls as much (they landed); a down needs
    PEDAL_TAIL_S to the next event (the ring's window). The rest are left out (unclear)."""
    m = x.mean(axis=1)
    e = level_db(x, sr)
    h = int(FRAME_S * sr)
    fine = fine_level(m, sr)
    rs = rises(e)
    ats = [refine(fine, sr, i * h, rs[n - 1][0] * h + int(REFINE_AFTER_S * sr) if n else 0) for n, (i, _) in enumerate(rs)]
    band = sosfiltfilt(butter(4, PEDAL_RING_HZ, "bandpass", fs=sr, output="sos"), m)
    lv = lambda a, b: float(10 * np.log10(np.mean(band[max(a, 0):b] ** 2) + 1e-30)) if b > a else np.nan  # noqa: E731
    pre = int(WRITE_PRE_S * sr)
    out = {"down": 0, "up": 0}
    events = []
    for n, ((i, r), at) in enumerate(zip(rs, ats)):
        nxt = ats[n + 1] if n + 1 < len(ats) else len(x)
        before = lv(at + int(PEDAL_BEFORE_S[0] * sr), at + int(PEDAL_BEFORE_S[1] * sr))
        after = lv(at + int(PEDAL_AFTER_S[0] * sr), min(at + int(PEDAL_AFTER_S[1] * sr), nxt - pre))
        change = after - before
        t = {"start": at, "end": nxt - pre if nxt < len(x) else nxt, "rise_db": r, "ring_change_db": change,
             "peak_dbfs": float(A.db(np.abs(x[at:nxt]).max())), "why": []}
        if change >= PEDAL_RING_DB:
            kind = "down"
            if nxt - at < PEDAL_TAIL_S * sr:
                t["why"].append(f"the next event {(nxt - at) / sr:.1f} s after it (the ring wants {PEDAL_TAIL_S:g})")
        elif change <= -PEDAL_RING_DB:
            kind = "up"
        else:
            kind = "unclear"
            t["why"].append(f"the ring changes {change:+.1f} dB")
        t["kind"] = kind
        if not t["why"]:
            out[kind] += 1
            t["take"] = out[kind]
        events.append(t)
    return events


def pedal_first(out, own):
    """The numbers a pedal pass's kept files start after, per kind: the ones it had (`own`, its JSON
    from before), else the highest in pedal/ (the passes split before it)."""
    if own is not None:
        return own.get("first", {})
    first = {}
    for path in glob.glob(os.path.join(out, "pedal", "*.wav")):
        got = re.fullmatch(r"(down|up)_(\d+)\.wav", os.path.basename(path))
        if got:
            first[got[1]] = max(first.get(got[1], 0), int(got[2]))
    return first


def write_pedal(x, sr, events, out, stem, first):
    """The events' files: the kept ones numbered after `first`, the left-out ones under discarded/
    with the pass's name."""
    pre = int(WRITE_PRE_S * sr)
    for n, t in enumerate(events, 1):
        folder = "pedal" if not t["why"] else os.path.join("discarded", "pedal")
        base = f"{t['kind']}_{t['take'] + first.get(t['kind'], 0)}.wav" if not t["why"] else f"{t['kind']}_{stem}_event{n}.wav"
        os.makedirs(os.path.join(out, folder), exist_ok=True)
        t["file"] = os.path.join(folder, base)
        sf.write(os.path.join(out, t["file"]), x[max(t["start"] - pre, 0):t["end"]], sr, subtype="PCM_24")


def split_silent(x, sr, keys, start_s=0.0):
    """A pass of silent releases: a take per event from `start_s` on, labeled in the order of `keys`
    (an event within SAME_KEY_S of the one before is its key's next take); a note (struck by mistake)
    and an event the file's end cuts within SILENT_S, still TAIL_DB over the floor, left out. The
    events must match the keys."""
    e, found = strikes(x, sr)
    h = int(FRAME_S * sr)
    floor = float(np.percentile(e, FLOOR_PCT))
    m = x.mean(axis=1)
    found = [s for s in found if s["at"] >= start_s * sr]
    takes = []
    for n, s in enumerate(found):
        nxt = found[n + 1]["at"] - int(WRITE_PRE_S * sr) if n + 1 < len(found) else len(x)
        end = min(nxt, s["at"] + int(SILENT_S * sr))
        seg = x[s["at"]:end]
        t = {"start": s["at"], "end": end, "rise_db": s["rise_db"], "peak_dbfs": float(A.db(np.abs(seg).max())),
             "level_db": float(10 * np.log10(np.mean(m[s["at"]:end] ** 2) + 1e-20)), "why": []}
        if s["note"]:
            k = max(s["scores"], key=s["scores"].get)
            t["why"].append(f"a note: sounds {key_name(k)} (struck by mistake)")
        elif end == len(x) and end - s["at"] < SILENT_S * sr and e[max(end // h - int(CUT_LAST_S / FRAME_S), 0):max(end // h - 2, 1)].mean() > floor + TAIL_DB:
            t["why"].append(f"cut by the file's end {(end - s['at']) / sr:.1f} s after it")
        takes.append(t)
    groups = []
    for t in takes:
        if t["why"]:
            continue
        if groups and t["start"] - groups[-1][-1]["start"] < SAME_KEY_S * sr:
            groups[-1].append(t)
        else:
            groups.append([t])
    if len(groups) != len(keys):
        at = ", ".join(f"{g[0]['start'] / sr:.2f}" for g in groups)
        raise ValueError(f"{len(groups)} events for {len(keys)} keys (events at {at} s; --from skips the ones before the pass)")
    for k, g in zip(keys, groups):
        for n, t in enumerate(g, 1):
            t.update({"key": k, "name": key_name(k), "take": n})
    return takes


def write_silent(x, sr, takes, out):
    """The silent releases' files: release_silent/<key>_<take>.wav, the left-out ones under discarded/."""
    pre = int(WRITE_PRE_S * sr)
    for n, t in enumerate(takes, 1):
        folder = "release_silent" if not t["why"] else os.path.join("discarded", "release_silent")
        base = f"{t['name']}_{t['take']}.wav" if not t["why"] else f"event{n}.wav"
        os.makedirs(os.path.join(out, folder), exist_ok=True)
        t["file"] = os.path.join(folder, base)
        sf.write(os.path.join(out, t["file"]), x[max(t["start"] - pre, 0):t["end"]], sr, subtype="PCM_24")


def keys_arg(text):
    """'21:108:3' → [21, 24, …, 108]; '38,39,88,89' → those."""
    if ":" in text:
        a, b, s = (int(v) for v in text.split(":"))
        return list(range(a, b + 1, s))
    return [int(v) for v in text.split(",")]


VELOCITY = {"pp": (1, 50), "p": (1, 50), "mf": (51, 100), "f": (101, 127), "ff": (101, 127)}


def write_sfz(out, name):
    """The SFZ over every pass split into `out` (their JSON files): an attack region per kept take
    (its dynamic's velocity range), a release region per release take; per key and dynamic
    seq_position 1, 2, … by the takes' ranks (a clipped take after every clean one, a sheet pass's
    before an `auto` one's)."""
    lines = ["// written by analysis/split_session.py: the takes kept, seq_position 1 the most even", ""]
    groups = {}
    for path in sorted(glob.glob(os.path.join(out, "*.split.json"))):
        doc = json.load(open(path))
        if doc.get("pedal") or doc.get("silent"):
            continue
        for t in doc["takes"]:
            if t["why"] or "rank" not in t:
                continue
            kind = "release" if doc.get("releases") else "attack"
            dyn = t.get("dynamic", doc["dynamic"])
            groups.setdefault((kind, t["key"], dyn), []).append((t.get("note", "").startswith("clipped"), doc["dynamic"] == "auto", t["rank"], t["file"]))
    count = {"attack": 0, "release": 0}
    for kind in ("attack", "release"):
        rows = sorted((k, d, g) for (q, k, d), g in groups.items() if q == kind)
        if not rows:
            continue
        lines += [f"<group> trigger={kind}"]
        for k, d, g in rows:
            lo, hi = VELOCITY[d]
            for seq, (*_, f) in enumerate(sorted(g), 1):
                lines.append(f"<region> sample={f} pitch_keycenter={k} lokey={k} hikey={k} lovel={lo} hivel={hi} seq_position={seq}")
                count[kind] += 1
        lines.append("")
    with open(os.path.join(out, name), "w") as fh:
        fh.write("\n".join(lines))
    return count["attack"], count["release"]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("wav", nargs="?")
    ap.add_argument("out")
    ap.add_argument("--dynamic", default="mf")
    ap.add_argument("--keys", default="21:108:3", type=keys_arg, help="the sheet's keys in order")
    ap.add_argument("--releases", action="store_true")
    ap.add_argument("--damped-to", type=int, help="releases: the highest key with a damper")
    ap.add_argument("--pedal", action="store_true", help="a pedal pass: its events as pedal-down and pedal-up takes")
    ap.add_argument("--silent", action="store_true", help="silent releases: keys released without their note")
    ap.add_argument("--from", dest="start_s", type=float, default=0.0, help="silent releases: the pass's start (s)")
    ap.add_argument("--tongues", action="store_true", help="a tongue drum's pass")
    ap.add_argument("--gain-db", type=float, default=0.0, help="the pass recorded this much under the session's gain")
    ap.add_argument("--sfz", action="store_true", help="write the SFZ over the passes split into out")
    ap.add_argument("--name", default="session.sfz")
    args = ap.parse_args()
    if args.sfz:
        n_att, n_rel = write_sfz(args.out, args.name)
        print(f"wrote {os.path.join(args.out, args.name)}: {n_att} attack regions, {n_rel} release regions")
        return
    x, sr = sf.read(args.wav, always_2d=True)
    stem = re.sub(r"\.wav$", "", os.path.basename(args.wav))
    if args.silent:
        takes = split_silent(x, sr, args.keys, args.start_s)
        write_silent(x, sr, takes, args.out)
        with open(os.path.join(args.out, f"{stem}.split.json"), "w") as fh:
            json.dump({"pass": os.path.basename(args.wav), "silent": True, "dynamic": "silent", "sr": sr,
                       "from_s": args.start_s, "takes": takes}, fh, indent=1)
        for t in takes:
            print(f"  {t['start'] / sr:8.2f} s {t.get('name', '-'):>4s} take {t.get('take', '-')} peak {t['peak_dbfs']:6.1f} {t['file']} {'; '.join(t['why'])}")
        return
    if args.pedal:
        events = split_pedal(x, sr)
        own = os.path.join(args.out, f"{stem}.split.json")
        first = pedal_first(args.out, json.load(open(own)) if os.path.exists(own) else None)
        write_pedal(x, sr, events, args.out, stem, first)
        with open(os.path.join(args.out, f"{stem}.split.json"), "w") as fh:
            json.dump({"pass": os.path.basename(args.wav), "pedal": True, "dynamic": "pedal", "sr": sr, "first": first, "takes": events}, fh, indent=1)
        for t in events:
            print(f"  {t['start'] / sr:8.2f} s {t['kind']:>7s} ring {t['ring_change_db']:+5.1f} dB peak {t['peak_dbfs']:6.1f} {t['file']} {'; '.join(t['why'])}")
        return
    takes, floor = split_pass(x, sr, args.keys, args.releases, args.dynamic == "auto", args.tongues, args.gain_db)
    x = at_gain(x, args.gain_db)
    if args.releases and args.damped_to:
        for t in takes:
            if t["key"] > args.damped_to:
                t["why"].append(f"undamped: above {key_name(args.damped_to)}")
    if args.dynamic == "auto":
        assign_dynamics(takes, sheet_levels(args.out))
    write_takes(x, sr, takes, args.out, args.dynamic, args.releases, stem)
    doc = {"pass": os.path.basename(args.wav), "dynamic": args.dynamic, "releases": args.releases, "sr": sr,
           "floor_db": floor, **({"gain_db": args.gain_db} if args.gain_db else {}), **({"tongues": True} if args.tongues else {}), "takes": [{k: v for k, v in t.items() if k != "curve"} for t in takes]}
    with open(os.path.join(args.out, f"{stem}.split.json"), "w") as fh:
        json.dump(doc, fh, indent=1)
    kept = [t for t in takes if not t["why"]]
    print(f"{args.wav}: {len(takes)} strikes, {len(kept)} kept over {len({t['key'] for t in kept})} keys; floor {floor:.1f} dB")
    for t in takes:
        print(f"  {t['start'] / sr:8.1f} s {t['name']:>4s} take {t['take']} rank {t.get('rank', '-')} "
              f"score {t['score']:5.1f} level {t['level_db']:6.1f} peak {t['peak_dbfs']:6.1f} "
              f"length {t['length_s']:5.1f} s end +{t['end_db']:.0f} dB {'; '.join(t['why'])}")


if __name__ == "__main__":
    main()
