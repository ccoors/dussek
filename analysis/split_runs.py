#!/usr/bin/env python3
"""Split the University of Iowa MIS percussion recordings (one file per octave run,
`<Instrument>.<articulation>.<dynamic>.<first><last>.aif`, chromatic) into one file per
note: data/iowa-mis-percussion/notes/<instrument>.<articulation>/<dynamic>/<midi>.wav. Files of
one note each (the crotales, `<instrument>.<note>.<dynamic>.aif`) are cut at their strike into
notes/<instrument>/<dynamic>/<midi>.wav.

Onsets: a run plays its notes one semitone apart, so note k's strike is where the energy
at its own fundamental (±BAND_FRAC) rises the most over RISE_S, searched after the previous
note's strike; then refined on a FINE_S envelope (the steepest rise over RISE_FINE_S
within a frame's length of the frame: the frames alone placed strikes up to half a frame
early). The run's broadband envelope alone was not enough: a strike on a bar
that still rings rises little, the pp runs' strikes only ~25 dB over the floor, and stray
noises (a mallet's bounce) rise as much. A note runs from PRE_S before its onset to PRE_S
before the next one (the next strike cuts its tail: the decays end 20–40 dB down). The strikes are chosen
together: per note the N_CAND strongest rises (its band's plus the broadband's), and of
them the in-order sequence, at least GAP_FRAC of the run's mean spacing apart, with the
largest total (one note at a time, a weak one took the next note's strike and squeezed
the rest).

The guitar (data/iowa-mis-guitar/, `Guitar.<dynamic>.<string>.<first><last>.stereo.aif`, one run
per string: sulE, sulA, sulD, sulG, sulB, sul_E = strings 6 … 1) is edited, not played as a run:
its notes ring out and are joined by digital silence (GAP_S), some by a splice instead (the
previous note faded out within ~0.15 s, the next one 40–70 dB louder at once). The silences
cut first; while notes are missing, the strongest level jumps (SPLICE_*) where the pitch
changes across cut too; then each piece is labeled by its pitch (note_scores), the labels
rising: notes/guitar.s<string>/<dynamic>/<midi>.wav. A run may skip a note (the pp sulE C3–B3
has no D3).

Usage: split_runs.py [instrument] (default Vibraphone; Marimba, xylophone, bells, crotale, Guitar)
"""
import glob
import json
import os
import re
import sys
from multiprocessing import Pool

import numpy as np
import soundfile as sf
from scipy.signal import butter, find_peaks, sosfiltfilt

import analyze as A

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..", "data", "iowa-mis-percussion")
GUITAR_ROOT = os.path.join(HERE, "..", "data", "iowa-mis-guitar")
GUITAR_STRINGS = {"sulE": 6, "sulA": 5, "sulD": 4, "sulG": 3, "sulB": 2, "sul_E": 1}
GAP_S = 0.05
SPLICE_FRAME_S = 0.02
SPLICE_LAG = 3          # frames: the jump from the lowest of the frames before
SPLICE_EDGE_S = 1.0     # no splice within this of a piece's ends
SPLICE_HOLD_S = 0.3
SPLICE_HOLD_DB = 15.0
SPLICE_MIN_DB = 30.0    # a splice jumps 40–70 dB; a pluck into a ringing note less
PITCH_S = 0.5           # the piece's start the pitch check reads
PITCH_PARTIALS = 6
PITCH_FLAT_CENTS = -20  # the guitar's tuning (the analysis: median −20 cents)
BAND_FRAC = 0.015
RISE_S = 0.05
FINE_S = 0.002
RISE_FINE_S = 0.01
HOP_S = 0.005
NFFT = 4096
PRE_S = 0.05
PAD_HOPS = 40
START_S = 0.005
SINGLE_SEARCH_S = 1.0
N_CAND = 25
GAP_FRAC = 0.3
CHECK_S = 0.3           # a cut's start the pitch check reads
CHECK_SPAN = 2          # its fundamental checked against this many semitones either side
CHECK_FRAC = 0.02
DOMINANCE_FROM_S = 0.03 # the pitch term: its fundamental over its neighbors' from this after a
DOMINANCE_S = 0.3       # candidate strike, over this long (the frames' bins, 23 Hz at 96 kHz, hold
                        # several bass semitones)


def fundamental_db(seg, sr, notes):
    """Per note: the peak power within CHECK_FRAC of its fundamental over `seg` (dB)."""
    P = np.abs(np.fft.rfft(seg * np.hanning(len(seg)))) ** 2
    f = np.fft.rfftfreq(len(seg), 1 / sr)
    out = []
    for n in notes:
        sel = np.abs(f - A.midi_to_hz(n)) < CHECK_FRAC * A.midi_to_hz(n)
        out.append(10 * np.log10(P[sel].max() + 1e-30) if sel.any() else -300.0)
    return np.array(out)


def pitch_mismatches(x, sr, ons, lo):
    """The cuts (from each strike to the next) whose fundamental over their first CHECK_S is not
    their label's but a neighbor's within CHECK_SPAN semitones (the bars' modes are not harmonic:
    the fundamental alone)."""
    m = x.mean(axis=1)
    out = []
    for k, a in enumerate(ons):
        b = min(ons[k + 1] if k + 1 < len(ons) else len(m), a + int(CHECK_S * sr))
        seg = m[a:b]
        if len(seg) < 64:
            continue
        if int(np.argmax(fundamental_db(seg, sr, range(lo + k - CHECK_SPAN, lo + k + CHECK_SPAN + 1)))) != CHECK_SPAN:
            out.append(k)
    return out


def onsets(x, sr, lo, count, n_cand=N_CAND, gap_frac=GAP_FRAC, pitch=False):
    """The run's strikes, one per note from `lo`. `pitch`: each candidate also scored by how far
    its note's fundamental stands over its neighbors' just after it (a stray noise in a pp run's
    silence, its rise from the floor as large as a strike's, took a note's place and shifted the
    rest by one: the marimba's rubber pp C2–B2, the vibraphone's pp C4–B4)."""
    # noise at the run's starting level in front, a whole number of hops (the frames stay where
    # they were): a strike within the first frame (the bells' plastic ff C5–B5, ~20 ms in)
    # rises over it; without it the first frames have nothing to rise from
    hop = int(HOP_S * sr)
    pad = PAD_HOPS * hop
    m = x.mean(axis=1)
    start = np.sqrt(np.mean(m[: int(START_S * sr)] ** 2))
    m = np.concatenate([start * np.random.default_rng(0).standard_normal(pad), m])
    frames = (len(m) - NFFT) // hop
    win = np.hanning(NFFT)
    f = np.fft.rfftfreq(NFFT, 1 / sr)
    bands = [np.abs(f - A.midi_to_hz(lo + k)) < max(BAND_FRAC * A.midi_to_hz(lo + k), sr / NFFT) for k in range(count)]
    e = np.empty((count, frames))
    broad = np.empty(frames)
    for j in range(frames):
        P = np.abs(np.fft.rfft(m[j * hop:j * hop + NFFT] * win)) ** 2
        broad[j] = P.sum()
        for k in range(count):
            e[k, j] = P[bands[k]].sum()
    e = 10 * np.log10(e + 1e-20)
    broad = 10 * np.log10(broad + 1e-20)
    lag = int(RISE_S / HOP_S)
    rise = np.zeros((count, frames))
    rise[:, lag:] = e[:, lag:] - e[:, :-lag]
    brise = np.zeros(frames)
    brise[lag:] = broad[lag:] - broad[:-lag]
    gap = int(gap_frac * frames / count)
    score_of = rise + brise
    # candidates per note: the strongest local maxima of its band's rise plus the broadband
    # rise (a strike rises everywhere); then the in-order choice with the largest total
    cand = []
    for k in range(count):
        score = score_of[k]
        pk, _ = find_peaks(score, distance=max(int(0.1 / HOP_S), 1))
        cand.append(sorted(sorted(pk, key=lambda j: -score[j])[:n_cand]))
    if pitch:
        score_of = score_of.copy()
        a0, n = int(DOMINANCE_FROM_S * sr), int(DOMINANCE_S * sr)
        for k in range(count):
            for j in cand[k]:
                c = j * hop + NFFT // 2 + a0
                seg = m[c:c + n]
                if len(seg) >= 64:
                    db = fundamental_db(seg, sr, (lo + k - 1, lo + k, lo + k + 1))
                    score_of[k, j] += db[1] - max(db[0], db[2])
    best = [{j: (score_of[0, j], None) for j in cand[0]}]
    for k in range(1, count):
        row = {}
        for j in cand[k]:
            prev = [(v[0], i) for i, v in best[-1].items() if j - i >= gap]
            if prev:
                v, i = max(prev)
                row[j] = (v + score_of[k, j], i)
        if not row:
            return None
        best.append(row)
    j = max(best[-1], key=lambda i: best[-1][i][0])
    path = [j]
    for k in range(count - 1, 0, -1):
        j = best[k][j][1]
        path.append(j)
    # the frames are NFFT long: refine each strike on a FINE_S envelope, at its steepest
    # rise over RISE_FINE_S within a frame's length around the frame's center
    fine = int(FINE_S * sr)
    nf = len(m) // fine
    ef = 10 * np.log10((m[: nf * fine] ** 2).reshape(nf, fine).mean(axis=1) + 1e-20)
    lf = int(RISE_FINE_S / FINE_S)
    out = []
    for j in path[::-1]:
        c = (j * hop + NFFT // 2) // fine
        a, b = max(c - NFFT // fine, lf), min(c + NFFT // (2 * fine), nf)
        r = ef[a:b] - ef[a - lf:b - lf]
        out.append(max((a + int(np.argmax(r)) - lf) * fine - pad, 0))
    return out


def note_range(name):
    """'C3B3' → (48, 59); 'C6F6' → (84, 89); 'C7' → (96, 96)."""
    parts = re.findall(r"[A-G]b?#?\d", name)
    lo = A.parse_note(parts[0])
    return lo, A.parse_note(parts[-1]) if len(parts) > 1 else lo


def level_jumps(x, sr, off):
    """(rise in dB, sample) per frame of a piece: the level re the lowest of the SPLICE_LAG
    frames before, away from the piece's ends; only where the level stays up after it (its
    median over SPLICE_HOLD_S within SPLICE_HOLD_DB of the piece's opening): a click (the pp
    sulE C3–B3 at 64.0 s, one frame 20 dB up) is no next note."""
    h = int(SPLICE_FRAME_S * sr)
    n = len(x) // h
    e = 10 * np.log10((x[: n * h] ** 2).mean(axis=1).reshape(n, h).mean(axis=1) + 1e-30)
    edge = int(SPLICE_EDGE_S / SPLICE_FRAME_S)
    hold = int(SPLICE_HOLD_S / SPLICE_FRAME_S)
    opening = np.median(e[:hold])
    out = []
    for i in range(max(SPLICE_LAG, edge), n - edge):
        j = i - SPLICE_LAG + int(np.argmin(e[i - SPLICE_LAG:i]))
        if np.median(e[i:i + hold]) >= opening - SPLICE_HOLD_DB:
            out.append((e[i] - e[j], off + j * h))
    return out


def note_scores(x, sr, notes):
    """Per candidate note: the summed peak power at its first PITCH_PARTIALS harmonics over
    the piece's first PITCH_S (dB). Peaks, not band sums: a band of a fixed relative width
    holds more bins at a higher note and pulled E2 to B2."""
    m = x[: int(PITCH_S * sr)].mean(axis=1)
    P = np.abs(np.fft.rfft(m * np.hanning(len(m)))) ** 2
    f = np.fft.rfftfreq(len(m), 1 / sr)
    out = []
    for n in notes:
        f0 = A.midi_to_hz(n) * 2 ** (PITCH_FLAT_CENTS / 1200)
        out.append(10 * np.log10(sum(P[np.abs(f - k * f0) < 0.02 * f0].max() for k in range(1, PITCH_PARTIALS + 1)) + 1e-30))
    return np.array(out)


def label(x, sr, pieces, lo, hi):
    """The notes of the pieces, strictly rising within lo…hi, with the largest summed score
    (a run may skip a note: the pp sulE C3–B3 has no D3), or None if there are more pieces than notes."""
    notes = list(range(lo, hi + 1))
    if len(pieces) > len(notes):
        return None
    S = np.array([note_scores(x[a:b], sr, notes) for a, b in pieces])
    S -= S.max(axis=1, keepdims=True)
    # best[i, j]: pieces 0…i labeled, piece i with notes[j]
    best = np.full((len(pieces), len(notes)), -np.inf)
    back = np.zeros((len(pieces), len(notes)), int)
    best[0] = S[0]
    for i in range(1, len(pieces)):
        for j in range(1, len(notes)):
            k = int(np.argmax(best[i - 1][:j]))
            best[i, j] = best[i - 1][k] + S[i, j]
            back[i, j] = k
    j = int(np.argmax(best[-1]))
    out = [j]
    for i in range(len(pieces) - 1, 0, -1):
        j = back[i, j]
        out.append(j)
    return [notes[j] for j in out[::-1]]


def best_note(x, sr, lo, hi):
    return lo + int(np.argmax(note_scores(x, sr, range(lo, hi + 1))))


def split_guitar(path):
    base = os.path.basename(path)
    dyn, string, rng = base.split(".")[1:4]
    lo, hi = note_range(rng)
    x, sr = sf.read(path, always_2d=True)
    silent = np.all(x == 0, axis=1)
    d = np.diff(np.r_[0, silent.astype(int), 0])
    gaps = [(a, b) for a, b in zip(np.nonzero(d == 1)[0], np.nonzero(d == -1)[0]) if b - a > GAP_S * sr]
    bounds = [0] + [v for g in gaps for v in g] + [len(x)]
    pieces = [(bounds[i], bounds[i + 1]) for i in range(0, len(bounds), 2) if bounds[i + 1] - bounds[i] > PRE_S * sr]
    # splices: the strongest level jumps where the pitch changes across, while notes are missing
    missing = hi - lo + 1 - len(pieces)
    if missing > 0:
        cand = sorted((c for a, b in pieces for c in level_jumps(x[a:b], sr, a)), reverse=True)
        cuts = []
        for r, c in cand:
            if len(cuts) == missing or r < SPLICE_MIN_DB:
                break
            if any(abs(c - q) < sr for q in cuts):
                continue
            a, b = next((a, b) for a, b in pieces if a < c < b)
            if best_note(x[a:c], sr, lo, hi) != best_note(x[c:b], sr, lo, hi):
                cuts.append(c)
        pieces = sorted([(a, b) for a, b in pieces if not any(a < c < b for c in cuts)]
                        + [seg for a, b in pieces if any(a < c < b for c in cuts)
                           for seg in zip([a] + sorted(c for c in cuts if a < c < b),
                                          sorted(c for c in cuts if a < c < b) + [b])])
    notes = label(x, sr, pieces, lo, hi)
    if notes is None:
        return {"file": base, "error": f"{len(pieces)} pieces for its {hi - lo + 1} notes"}
    out_dir = os.path.join(GUITAR_ROOT, "notes", f"guitar.s{GUITAR_STRINGS[string]}", dyn)
    os.makedirs(out_dir, exist_ok=True)
    out = []
    for n, (a, b) in zip(notes, pieces):
        sf.write(os.path.join(out_dir, f"{n}.wav"), x[a:b], sr, subtype="FLOAT")
        out.append({"note": n, "start_s": a / sr, "length_s": (b - a) / sr})
    res = {"file": base, "notes": out}
    skipped = sorted(set(range(lo, hi + 1)) - set(notes))
    if skipped:
        res["skipped"] = skipped
    return res


def split(path):
    base = os.path.basename(path)
    parts = base[:-4].split(".")
    if len(parts) == 3 and re.fullmatch(r"[A-G]b?\d", parts[1]):
        # one note per file (the crotales: `crotale.<note>.<dynamic>`): its strike, into notes/<instrument>/
        inst, rng, dyn = parts
        art = None
    elif len(parts) >= 4:
        inst, art, dyn, rng = parts[:4]
    else:
        # the bells' brass runs name no dynamic: taken as mf
        inst, art, rng = parts
        dyn = "mf"
    lo, hi = note_range(rng)
    x, sr = sf.read(path, always_2d=True)
    if art is None:
        # one note: its file starts on the strike (the crotales' pp lie ~35 dB under their ff,
        # the whole file searched took a noise burst seconds later), searched above half its
        # fundamental (the room's rumble rose more than a pp strike)
        sos = butter(4, 0.5 * A.midi_to_hz(lo), "highpass", fs=sr, output="sos")
        ons = onsets(sosfiltfilt(sos, x[: int(SINGLE_SEARCH_S * sr)], axis=0), sr, lo, 1)
    else:
        # a run with an uneven pace: more candidates, a looser gap
        ons = onsets(x, sr, lo, hi - lo + 1) or onsets(x, sr, lo, hi - lo + 1, 4 * N_CAND, GAP_FRAC / 2)
    if ons is None:
        return {"file": base, "error": f"fewer onsets than its {hi - lo + 1} notes"}
    # a cut holding its neighbor's note: chosen again with the pitch term (only then: the runs it
    # does not concern keep their cuts)
    wrong = pitch_mismatches(x, sr, ons, lo) if art is not None else []
    repitched = None
    if wrong:
        again = onsets(x, sr, lo, hi - lo + 1, pitch=True) or onsets(x, sr, lo, hi - lo + 1, 4 * N_CAND, GAP_FRAC / 2, pitch=True)
        if again is not None and len(pitch_mismatches(x, sr, again, lo)) < len(wrong):
            ons, repitched = again, [lo + k for k in wrong]
        wrong = pitch_mismatches(x, sr, ons, lo)
    out_dir = os.path.join(ROOT, "notes", (inst if art is None else f"{inst}.{art}").lower(), dyn)
    os.makedirs(out_dir, exist_ok=True)
    pre = int(PRE_S * sr)
    bounds = ons + [len(x) + pre]
    notes = []
    for k, (a, b) in enumerate(zip(bounds[:-1], bounds[1:])):
        seg = x[max(a - pre, 0):b - pre]
        sf.write(os.path.join(out_dir, f"{lo + k}.wav"), seg, sr, subtype="FLOAT")
        notes.append({"note": lo + k, "start_s": a / sr, "length_s": len(seg) / sr})
    res = {"file": base, "notes": notes}
    if repitched:
        res["repitched"] = repitched
    if wrong:
        res["pitch_mismatch"] = [lo + k for k in wrong]
    return res


def main():
    inst = sys.argv[1] if len(sys.argv) > 1 else "Vibraphone"
    guitar = inst.lower() == "guitar"
    root = GUITAR_ROOT if guitar else ROOT
    runs = sorted(glob.glob(os.path.join(root, f"{inst}.*.aif")))
    with Pool() as pool:
        res = pool.map(split_guitar if guitar else split, runs)
    for r in res:
        print(f"{r['file']}: " + (r["error"] if "error" in r else
                                  "lengths " + " ".join(f"{n['length_s']:.1f}" for n in r["notes"]) + " s"
                                  + (f"; skipped {r['skipped']}" if "skipped" in r else "")
                                  + (f"; cut again by pitch (held their neighbors' notes: {r['repitched']})" if "repitched" in r else "")
                                  + (f"; PITCH MISMATCH {r['pitch_mismatch']}" if "pitch_mismatch" in r else "")))
    json.dump(res, open(os.path.join(root, "notes", f"{inst.lower()}_split.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
