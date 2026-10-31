"""Struck bars (the vibraphone, the marimba): per key the parameters of the engine's mode
table, fitted from the mode analysis (analysis/modes.py) of Iowa MIS's sets (`config`, the project's [bars]:
the sets, the mode tracks, a damped set or none).

The marimba: struck with three kinds of mallet (yarn, cord, rubber: one set each), no
dampers. The bars are the same whichever mallet: ratios, losses and levels from all the
sets together, the mallet's lowpass and its speeds per set (`fit_mallet`, one fit: the
levels re the first set's mf). Its bass bars have a fourth mode near 18.7·f1.

A key is one bar, its sound three bending modes (TRACKS: ratios ≈ 1 : 4 : 10 re the
fundamental; the third is tuned only in the low range and drifts flat above F♯4). The
other peaks the analysis finds are the fundamental's harmonics h·f1 (a nonlinearity:
where the second mode is off 4.00 the peaks sit at 3·f1 and 5·f1, not at f2 ± f1) and,
where a harmonic meets a mode, that mode. Other bars do not ring along audibly: with the
pedal down an unrelated bar's fundamental rises 80–90 dB below the struck one's, and the
bar two octaves up (its fundamental within 2 cents of the struck bar's second mode) leaves
no tail of its own in that band.

Per mode and key, smoothed over the keyboard (`_curve`) and carried on beyond the
recorded keys (C3–F6) with limited slopes:
  - ratio re the fundamental (measured), the fundamental's tuning (cents)
  - loss σ (1/s): the energy-decay-curve rate with the pedal down (the analysis's
    vibraphone set), and the damper's extra loss: pedal up (vibraphone_damp) minus that
  - level: the mode's measured level minus the mallet model's amplitude at the
    take's speed, over the takes (`out_db`: the engine's output weight of the mode)
The harmonics (`fit_harmonics`), h = 2 … H_TOP, with the damper lifted (the vibraphone set)
and resting (vibraphone_damp): per key the level re the fundamental at ff (smoothed,
held beyond the recorded keys; at the strike: the measured level plus its faster decay
over the early window), and per harmonic its decay re the fundamental's and its growth: how
much its level re the fundamental changes per dB of the engine's fundamental from mf to ff
(the recordings' ff is ~9 dB over their mf, the mallet's fundamental ~6). Free they behave
as squares of the fundamental (decay ×2, level re f1 +1 dB per dB of the recorded f1, 2·f1
−43 dB at ff); damped they are stronger (2·f1 −30 dB) and hardly grow (the felt's contact).
Bands where a mode lies within H_CLEAR are left out of the fit (the fit's mode or the take's
own), and a key whose band is not clear gets no such harmonic (in the engine it would beat
with that mode, which the recordings' modes do not); nor do keys beyond a harmonic's
measured span (4·f1 is clear of the second mode only from A♯5 up). The modes' sums and
differences are not harmonics' rivals: in the bass, where f2 − f1 lies 0.1–0.4 % off 3·f1,
the peak sits at 3.000·f1 (3 s spectra, each take's own f1), and 5·f1 likewise. In the bass
the odd harmonics lead (C3 ff: 3·f1 −33 dB, 5·f1 −44, 7·f1 −50, 2·f1 −51): the resonator
tube's own resonances; the treble has 2·f1 (−42) and little else.
The mallet (one for the whole instrument): the modes start directly (the engine's pluck
path with `bar`), each at its output weight × 2^steps (amplitude ∝ speed: GAIN_DB_PER_STEP)
through a lowpass of order `order` whose cutoff f_q rises `fq_oct_per_step` octaves per
doubling of the speed (steps = log2 speed, mf at speed 1): a harder strike, a shorter
contact, a brighter sound (the third mode's share grows ~40 dB from pp to ff in the bass).
Fitted to the modes' level changes across the dynamics, per key and mode. A contact
simulation (a mass on a felt, as the hammer) fitted the same changes only with a 2–4 ms
contact, which left the upper modes 70–100 dB down in its force spectrum, where their levels
hang on the pulse's finest details: the engine's contact at 48 kHz came out 20–30 dB off.
"""
import argparse
import json
import os
from functools import lru_cache

import numpy as np
from scipy.optimize import least_squares
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analysis"))
import project as P  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
NEPER_DB = 20 / np.log(10)          # dB per neper (amplitude)

TRACKS = (1.0, 4.0, 10.0)
TRACK_TOL = (0.03, 0.12, 0.25)      # relative to the nominal ratio


def config(profile):
    """The bars' fit settings ([bars]) with the sets from the part's sources: the mallets' sets
    in their order (the first the reference and the default), their names, the pedal-up (damped)
    set or None, the mode tracks and their tolerances (the project's comments say why)."""
    srcs = [P.source(s) for s in P.source_ids(profile)]
    cfg = dict(P.section(profile, "bars"))
    if "tied" in cfg:
        cfg["tied"] = {int(k): v for k, v in cfg["tied"].items()}
    return {**cfg, "sets": [s["id"] for s in srcs if s["role"] == "mallet"],
            "mallets": [s["mallet"] for s in srcs if s["role"] == "mallet"],
            "damp": next((s["id"] for s in srcs if s["role"] == "damped"), None)}


SPAN = (36, 108)                    # beyond the recordings: carried on, limited
FRAC = 3.0                          # semitones: the keyboard smoothing's width
# per semitone at most, beyond the recorded keys
TUNING_SLOPE = 1.0                  # cents
LOG_RATIO_SLOPE = 0.005
RATIO_MIN_DEV = 0.01                # the ratios' robust deviation at least (log)
HARMONIC_DECAY = 2.5                # a harmonic of the fundamental decays slower than this × it
# [bars] tracks = "auto": the tracks derived from the takes' modes (derive_tracks)
AUTO_HARMONIC_DB = -25.0            # a whole-number track this far under the fundamental, or decaying
                                    # as slowly (HARMONIC_DECAY) on most keys: its harmonic (the
                                    # marimba's 2, 3, 5 and 7, the xylophone's 2 at −29 dB; its tuned 3
                                    # at −11 dB and the marimba's 4 at −13 dB are their own modes)
AUTO_HELD = 0.2                     # a track on this share of the keys at least (and on 3): a bar's high
                                    # modes leave the analysis's range in the treble (the marimba's 10
                                    # on 27 of 61 keys, its 18.7 on 9)
AUTO_GAP = 0.15                     # two tracks on keys apart, their ratios this close: one mode
AUTO_JOIN = 0.06                    # a mode joins a track within this of its ratio at the nearest key
AUTO_MERGE = AUTO_JOIN              # one key's modes this close are one mode (its takes, its layers:
                                    # the bells' takes differ by up to 6 %)
AUTO_UPPER = 6                      # upper tracks kept at most, the strongest
AUTO_THIN = 3                       # semitones between a track's anchors at least
AUTO_TOL = (0.03, 0.12)             # a derived track's tolerance: twice its scatter, within these
FAST_DECAY_WEIGHT = 0.3             # a short-window decay's weight in the loss curves (a fit's conf: 0.1–0.6)
LOG_SIGMA_SLOPE = 0.08
LEVEL_SLOPE = 1.0                   # dB, never rising outward
# a mode found only on part of the recorded keys (the marimba's fourth to A♯2, its third to
# D4) fades beyond them by this much per semitone (carried on, the fourth mode at 18·f1 rang
# −23 dB re the fundamental at C4, where the recordings have none)
MODE_FADE_DB = 6.0

# the bar: aluminum (2700 kg/m³), 38 × 13 mm, undercut to ¾ of its mass; F3 0.38 m long
BAR_LEN_F3 = 0.38
BAR_KG_PER_M = 2700 * 0.038 * 0.013 * 0.75
# the mallet: amplitude ∝ speed (dB per doubling), mf at speed 1
GAIN_DB_PER_STEP = 20 * np.log10(2.0)
DYNAMICS = ("pp", "mf", "ff")
F_REF = 261.63                      # the mallet's cutoff is given for a bar of this f1 (C4)
# the most the first mallet's lowpass cuts a bar's fundamental at mf: deeper cuts, fitted from the
# dynamics' spread of a few hand-played takes, were made up by the modes' levels and came back
# as +40 dB under a harder mallet (the crotales' top keys; design log step 191)
FUNDAMENTAL_CUT_DB = 3.0
# the harmonics: counted where the band's early level is H_SNR_DB over its floor and no mode
# or modes' sum or difference is within H_CLEAR (relative); a harmonic needs H_MIN_KEYS keys
H_SNR_DB = 12.0
H_CLEAR = 0.012
H_MIN_KEYS = 6
H_FRAC = 6.0                        # semitones: their smoothing's width (single takes scatter ±5 dB)
H_EARLY_MID_S = 0.14                # the analysis's early window's middle (modes.HARM_EARLY_S)
H_NONE_DB = -120.0
# a take counts for a harmonic's level only where its correction back to the strike (the
# harmonic's faster decay over the early window) is at most this (the xylophone's top keys
# fall 300–400 dB/s: +50 dB, which made 2·f1 −11 dB re the fundamental)
H_MAX_CORR_DB = 10.0
H_TOP = 7
# a harmonic's growth where one dynamic was recorded: a free bar's (the vibraphone's, 1 dB per dB)
ASSUMED_GROWTH = 1.0


def _ok(s):
    return "error" not in s and "label_mismatch" not in s


@lru_cache(maxsize=None)
def samples(dataset):
    """The set's usable takes, each with its dynamic as the fits name it (pp, mf, ff): the source's
    dynamics table maps those to the layers' keys (the layer's dynamic, or with layer_key =
    "layer" its number; an SFZ set's are "1/3" …). A take of a layer the table leaves out is not
    one of the fits' takes."""
    src = P.source(dataset)
    key = "layer" if src.get("layer_key") == "layer" else "dynamic"
    names = {str(v): k for k, v in src["dynamics"].items()}
    return [{**s, "dynamic": names[str(s.get(key))]} for s in P.load_cache(dataset, "analysis")["samples"]
            if _ok(s) and str(s.get(key)) in names]


def track(s, j, tracks=TRACKS, tols=TRACK_TOL):
    """The sample's mode of track j: the strongest within TRACK_TOL of its nominal ratio
    that is not the fundamental's harmonic (a whole-number ratio decaying less than
    HARMONIC_DECAY × the fundamental: from A5 up the second mode falls below 4 and the 4th
    harmonic, at 4.000, often outdoes it; a mode tuned to a whole number decays 3–25 × faster
    than the fundamental), or None."""
    r0, tol = nominal(tracks[j], s.get("note")), tols[j]
    c = [m for m in s["modes"] if abs(m["ratio"] / r0 - 1) < tol]
    f1 = min(s["modes"], key=lambda m: abs(m["ratio"] - 1), default=None)
    d1 = decay(f1) if f1 else None
    like = lambda m: m.get("harmonic") and d1 and decay(m) is not None and decay(m) < HARMONIC_DECAY * d1  # noqa: E731
    c = [m for m in c if not like(m)] or c
    return max(c, key=lambda m: m["level_db"]) if c else None


def nominal(t, note):
    """A track's nominal ratio at a key: a number, or (key, ratio) anchors interpolated in log
    ratio (held beyond them: the bells' modes drift, the torsional one from 11.3 to 4.0)."""
    if np.isscalar(t):
        return t
    k, r = zip(*t)
    return float(np.exp(np.interp(note, k, np.log(r))))


def decay(m):
    """dB/s of a mode (energy decay curve, else the line fit), or None."""
    return m.get("edc_decay_db_s") or m.get("decay_db_s")


def _curve(notes, values, weights, slope, no_rise=False, frac=FRAC):
    """Smoothed over the keyboard (Gaussian-weighted local lines), beyond the outermost
    notes carried on with the edge's slope over 6 semitones, at most `slope` per semitone;
    `no_rise`: never rising outward."""
    notes, values, weights = (np.asarray(a, float) for a in (notes, values, weights))

    def inner(n):
        w = weights * np.exp(-0.5 * ((notes - n) / frac) ** 2)
        X = np.column_stack([np.ones_like(notes), notes - n])
        coef, *_ = np.linalg.lstsq(X * np.sqrt(w)[:, None], values * np.sqrt(w), rcond=None)
        return coef[0]

    lo, hi = notes.min(), notes.max()
    span = min(6.0, hi - lo)
    up = float(np.clip((inner(hi) - inner(hi - span)) / span, -slope, slope))
    down = float(np.clip((inner(lo + span) - inner(lo)) / span, -slope, slope))
    if no_rise:
        up, down = min(up, 0.0), max(down, 0.0)
    v_hi, v_lo = inner(hi), inner(lo)
    return lambda n: v_hi + up * (n - hi) if n > hi else v_lo - down * (lo - n) if n < lo else inner(n)


def _robust(notes, values, weights, slope, min_dev=0.0, **kw):
    """_curve after dropping the values more than 3 robust deviations (at least `min_dev`)
    off a first pass."""
    first = _curve(notes, values, weights, slope, **kw)
    res = np.array([v - first(n) for n, v in zip(notes, values)])
    mad = max(np.median(np.abs(res - np.median(res))) * 1.4826, min_dev) + 1e-9
    keep = np.abs(res) < 3 * mad
    if keep.sum() >= 4:
        return _curve(np.asarray(notes)[keep], np.asarray(values)[keep], np.asarray(weights)[keep], slope, **kw)
    return first


def _usable(m):
    """A track's mode the fit can take: found, decaying, its confidence over 0.1."""
    return bool(m and decay(m) and m.get("conf", 0) > 0.1)


def missing_tracks(takes, tracks, tols):
    """The tracks no take shows a usable mode of (`track`, `_usable`)."""
    return [j for j in range(len(tracks)) if not any(_usable(track(s, j, tracks, tols)) for s in takes)]


def missing_text(takes, tracks, tols):
    """Why the fit cannot go on: the tracks no take shows, and the takes' own modes (their
    ratios to the fundamental) to set [bars] tracks from: a project made from another
    instrument's template keeps its tracks."""
    fmt = lambda t: f"{t:.2f}" if np.isscalar(t) else f"{t[0][1]:.2f} at key {t[0][0]} to {t[-1][1]:.2f} at key {t[-1][0]}"  # noqa: E731
    lines = [f"track {j} ({fmt(tracks[j])} × the fundamental, ±{100 * tols[j]:.0f} %) is in no take: "
             "[bars] tracks do not fit this set (made from another instrument's template?)"
             for j in missing_tracks(takes, tracks, tols)]
    lines.append("the takes' modes (ratio to the fundamental, the confident ones):")
    for s in sorted(takes, key=lambda s: s["note"]):
        rs = sorted(m["ratio"] for m in s["modes"] if m.get("conf", 0) > 0.5)
        lines.append(f"  key {s['note']}: " + " ".join(f"{r:.2f}" for r in rs))
    return "\n".join(lines)


def derive_tracks(takes, lower=False):
    """The tracks and their tolerances from the takes' own modes ([bars] tracks = "auto"): each
    key's modes with a decay (a fit's or the short window's: the analysis's confidence rates the
    fit, not the mode), their takes' merged, followed from key to key within AUTO_JOIN of the ratio
    at the nearest key (a track split by keys without it joined again); those on AUTO_HELD of the
    keys that are not the fundamental's harmonics, the AUTO_UPPER strongest (keys × amplitude re
    the take's fundamental, none taking a stronger one's peak), in the order of their ratios;
    anchors every AUTO_THIN semitones; the tolerance twice the median scatter about them. `lower`
    (lower_free: the tubes): the modes below the reference (0.2 … 0.87 of it) too."""
    by_note = {}
    for s in takes:
        f1 = min(s["modes"], key=lambda m: abs(m["ratio"] - 1), default=None)
        if f1 is None:
            continue
        d1 = decay(f1)
        by_note.setdefault(s["note"], []).extend(
            {**m, "level_db": m["level_db"] - f1["level_db"], "_usable": _usable(m),
             "_slow": bool(d1 and (decay(m) or m.get("fast_decay_db_s") or 0) < HARMONIC_DECAY * d1)}
            for m in s["modes"] if (m["ratio"] > 1.15 or (lower and 0.2 < m["ratio"] < 0.87))
            and (decay(m) or m.get("fast_decay_db_s")))
    tracks = []                     # each: [(note, ratio, level_db re f1, slow, usable)]
    for note in sorted(by_note):
        merged = []
        for m in sorted(by_note[note], key=lambda m: -m["level_db"]):
            if not any(abs(m["ratio"] / x["ratio"] - 1) < AUTO_MERGE for x in merged):
                merged.append(m)
        for m in merged:
            best = None
            for t in tracks:
                if t[-1][0] == note:
                    continue
                d = abs(m["ratio"] / t[-1][1] - 1)
                if d < AUTO_JOIN and (best is None or d < best[0]):
                    best = (d, t)
            if best:
                best[1].append((note, m["ratio"], m["level_db"], m["_slow"], m["_usable"]))
            else:
                tracks.append([(note, m["ratio"], m["level_db"], m["_slow"], m["_usable"])])

    med = lambda t, i: float(np.median([x[i] for x in t]))  # noqa: E731
    # a mode split by keys where it was not found: one track again
    tracks.sort(key=lambda t: t[0][0])
    joined = []
    for t in tracks:
        mate = next((u for u in joined if not {x[0] for x in u} & {x[0] for x in t}
                     and abs(med(t, 1) / med(u, 1) - 1) < AUTO_GAP and t[0][0] > u[-1][0]), None)
        if mate is not None:
            mate.extend(t)
        else:
            joined.append(list(t))

    def harmonic(t):
        r = med(t, 1)
        return abs(r - round(r)) < 0.01 * r and (med(t, 2) < AUTO_HARMONIC_DB or np.mean([x[3] for x in t]) > 0.5)

    # (the fit's curves need a decay fit on some keys: `_usable`)
    held = [t for t in joined if len(t) >= max(3, AUTO_HELD * len(by_note)) and not harmonic(t)
            and sum(x[4] for x in t) >= 2]
    # the strongest first: held on many keys, loud
    held = sorted(held, key=lambda t: -len(t) * 10 ** (med(t, 2) / 20))
    cands = []
    for t in held:
        notes = [x[0] for x in t]
        anchors = []
        for n, r, *_ in t:
            if not anchors or n - anchors[-1][0] >= AUTO_THIN or n == notes[-1]:
                anchors.append([n, round(r, 3)])
        track = anchors[0][1] if len(anchors) == 1 else anchors
        dev = [abs(np.log(r / nominal(track, n))) for n, r, *_ in t]
        cands.append((track, round(float(np.clip(2 * np.median(dev), *AUTO_TOL)), 3)))
    # a track that would take a peak a stronger one takes (the fit picks each track's strongest peak
    # within its tolerance) is that mode twice: left out
    kept = [(1.0, AUTO_TOL[0])]
    for c in cands:
        if len(kept) > AUTO_UPPER:
            break
        if not any(_shares_a_peak(s, c, k) for s in takes for k in kept):
            kept.append(c)
    upper = sorted(kept[1:], key=lambda c: nominal(c[0], 60))
    return [1.0] + [c[0] for c in upper], [AUTO_TOL[0]] + [c[1] for c in upper]


def _shares_a_peak(take, a, b):
    """Whether two (track, tol) pick the same mode of a take."""
    ma, mb = track(take, 0, [a[0]], [a[1]]), track(take, 0, [b[0]], [b[1]])
    return ma is not None and ma is mb


def bar_mass(f1):
    return BAR_KG_PER_M * BAR_LEN_F3 * np.sqrt(174.61 / f1)


def cutoff_scale(f1, beta, f_q=None, order=None):
    """The mallet's cutoff on a bar of fundamental f1 re one at F_REF (a lighter bar, a
    shorter contact): (f1/F_REF)^beta; with the first mallet's f_q and order, raised where its
    lowpass would cut the fundamental at mf by more than FUNDAMENTAL_CUT_DB (all the mallets
    scaled alike, as the engine scales another mallet's by the note's f_q)."""
    f1 = np.asarray(f1, float)
    sc = (f1 / F_REF) ** beta
    if f_q is None:
        return sc
    floor = f1 / np.sqrt(10 ** (FUNDAMENTAL_CUT_DB / (10 * order)) - 1)
    return np.maximum(sc, floor / f_q)


def mallet_db(f, steps, f_q, fq_oct, order, scale=1.0):
    """The mallet's level (dB) at frequency f for a strike `steps` doublings of the speed
    from mf (as the engine's physics::pluck_amplitude with `bar`), its cutoff × `scale`
    (`cutoff_scale`: the export puts it into each note's f_q)."""
    fq = f_q * scale * 2.0 ** (fq_oct * steps)
    return GAIN_DB_PER_STEP * steps - 10 * order * np.log10(1 + (np.asarray(f, float) / fq) ** 2)


class Fit:
    """An instrument's fit: per track the smoothed curves, the mallets, the levels."""

    def __init__(self, profile):
        cfg = config(profile)
        self.tracks, self.tols, self.mallet_names = cfg["tracks"], cfg.get("tol"), cfg["mallets"]
        # tracks = "auto": derived from the takes below (a project the editor's wizard made)
        self.auto_tracks = self.tracks == "auto"
        self.scale_cutoff = cfg["scale_cutoff"]
        # (track numbers: meaningless for tracks derived)
        self.tied = {} if self.auto_tracks else cfg.get("tied", {})
        # two dynamics (mf, ff): pp as far below mf as ff is above
        self.pp_mirror = cfg.get("pp_mirror", False)
        # no damped takes: an assumed damper loss (dB/s, every mode)
        self.damper_db_s = cfg.get("damper_db_s")
        # modes below the reference may outlast it (the tubes' second and third)
        self.lower_free = cfg.get("lower_free", False)
        # the reference mode (track 0) at this multiple of the key's pitch (the tubes' fourth: 2)
        self.ref_ratio = cfg.get("ref_ratio", 1.0)
        # the modes' slow beating (split pairs: the analysis's beat_db, beat_hz), `fit_splits`
        self.has_splits = cfg.get("splits", False)
        # one dynamic recorded (normalized takes): the mallet's speeds and lowpass assumed
        # (`mallet`: speeds, f_q, fq_oct_per_step, order, beta), its harmonics' levels carried
        # from mf to ff by their growth
        self.assumed_mallet = cfg.get("mallet")
        # the keys played at equal temperament (the kalimba's tines scatter ±50 cents)
        self.equal_tuning = cfg.get("equal_tuning", False)
        # the instrument's other tines ring along (the analysis's `halo`), `fit_halo`
        self.has_halo = cfg.get("halo", False)
        # the rattle between the lines per recorded key (the kalimba's tines, the balafon's gourds)
        self.has_rattle = cfg.get("rattle", self.has_halo)
        # the strike's own modes per take, the tracks' and harmonics' aside (`fit_strike`)
        self.has_strike = cfg.get("strike", False)
        # a strike mode's line at least this far over the spectrum around it (the analysis's
        # `line_db`; None: every peak): the balafon's buzz throws up peaks 2–13 dB over its median
        self.strike_line_db = cfg.get("strike_line_db")
        # the strike's modes from every take, not only the reference's (`fit_strike_all`)
        self.strike_all = cfg.get("strike_all", False)
        # the losses held beyond the recorded keys (6 bars: carried on, the treble's slowed again)
        self.hold_sigma = cfg.get("hold_sigma", False)
        # the upper modes' levels from the spectrum (`mode_level`)
        self.spec_levels = cfg.get("spec_levels", False)
        # the mallets' cutoff at least this (Hz)
        self.fq_min = cfg.get("fq_min", 100.0)
        fast = cfg.get("fast_decays", False)
        # every take tagged with its set (its mallet)
        sus = [{**s, "_set": k} for k, ds in enumerate(cfg["sets"]) for s in samples(ds)]
        dmp = [{**s, "_set": 0} for s in samples(cfg["damp"])] if cfg["damp"] else []
        self.damped = bool(dmp) or self.damper_db_s is not None
        track = self.track
        notes = sorted({s["note"] for s in sus})
        self.recorded = (notes[0], notes[-1])
        n_t = [s["note"] for s in sus]
        self._samples = sus
        self._tuning = (lambda n: 0.0) if self.equal_tuning else _robust(n_t, [s["tuning_cents"] for s in sus], np.ones(len(sus)), TUNING_SLOPE)
        self._ratio, self._sigma, self._damper, self._sigma_span = [], [], [], []
        def own_f1_decay(s):
            m = track(s, 0)
            return decay(m) if m else None

        def fast_only(j):
            # takes whose mode j the regular fit could not decay at all (conf 0), the short-window one could
            if j == 0 or not fast:
                return []
            out = []
            for s in sus:
                m = track(s, j)
                if m and not (decay(m) and m.get("conf", 0) > 0.1) and m.get("fast_decay_db_s"):
                    out.append((s["note"], {**m, "conf": 0.0, "decay_db_s": None, "edc_decay_db_s": None}, own_f1_decay(s)))
            return out

        if self.auto_tracks:
            self.tracks, self.tols = derive_tracks(sus, lower=bool(self.lower_free))
            print(f"{profile}: tracks derived {json.dumps(self.tracks)}, tol {self.tols}")
            if len(self.tracks) < 2:
                raise SystemExit(f"{profile}: no mode besides the fundamental is held by a fifth of the keys "
                                 f"(three at least) and not the fundamental's harmonic\n"
                                 + missing_text(sus, self.tracks, self.tols))
        if missing_tracks(sus, self.tracks, self.tols):
            raise SystemExit(f"{profile}: " + missing_text(sus, self.tracks, self.tols))
        for j in range(len(self.tracks)):
            rows = [(s["note"], track(s, j), own_f1_decay(s)) for s in sus]
            rows = [(n, m, d1) for n, m, d1 in rows if _usable(m)]
            nn = [n for n, _, _ in rows]
            w = [m["conf"] for _, m, _ in rows]
            # (the middle tuned to 0.04 %: without a floor the treble's 1–2 % scatter is all outliers)
            self._ratio.append(_robust(nn, [np.log(m["ratio"]) for _, m, _ in rows], w, LOG_RATIO_SLOPE, min_dev=RATIO_MIN_DEV))
            # the loss: an upper mode's decay slower than its own take's fundamental is a failed fit
            # (the xylophone's treble modes fall 550–740 dB/s, three to five times the fundamental,
            # gone before the analysis's 40 ms windows can follow them; it read 70–100 dB/s): the
            # analysis's short-window decay instead (modes.fast_decay), where it has one
            def loss(m, d1):
                dm = decay(m)
                if dm and (j == 0 or d1 is None or dm >= d1):
                    return dm, m["conf"]
                fd = m.get("fast_decay_db_s") if fast else None
                return (fd, FAST_DECAY_WEIGHT) if fd and (d1 is None or fd >= d1) else (None, 0.0)
            srows = [(n, *loss(m, d1)) for n, m, d1 in [(n, m, d1) for n, m, d1 in rows] + fast_only(j)]
            srows = [(n, d, c) for n, d, c in srows if d]
            sn = [n for n, _, _ in srows]
            self._sigma.append(_robust(sn, [np.log(d / NEPER_DB) for _, d, _ in srows], [c for _, _, c in srows],
                                       LOG_SIGMA_SLOPE))
            self._sigma_span.append((min(sn), max(sn)))
            rows = [(n, m) for n, m, _ in rows]
            if not dmp:
                continue
            # the damper: the pedal-up take's loss minus the pedal-down curve at its note
            drows = [(s["note"], track(s, j)) for s in dmp]
            drows = [(n, m) for n, m in drows if m and decay(m) and m.get("conf", 0) > 0.05]
            extra = [(n, decay(m) / NEPER_DB - np.exp(self._sigma[j](n)), m.get("conf", 0.1)) for n, m in drows]
            extra = [(n, e, c) for n, e, c in extra if e > 0]
            if len(extra) >= 6:
                self._damper.append(_robust([e[0] for e in extra], [np.log(e[1]) for e in extra],
                                            [e[2] for e in extra], LOG_SIGMA_SLOPE))
            else:   # too few: the fundamental's
                self._damper.append(self._damper[0])
        self.set_speeds, self.set_mallets = self.fit_mallet(sus)
        self.speeds, self.mallet = self.set_speeds[0], self.set_mallets[0]
        self._level = self.fit_levels(sus)
        self.harmonic_numbers, self._harm = self.fit_harmonics(sus, dmp)
        self._splits = self.fit_splits(sus) if self.has_splits else None
        self._halo = self.fit_halo(sus) if self.has_halo else None
        # the rattle per recorded key (the analysis's `rattle_db`; two takes of a pitch averaged; with
        # several sets and dynamics the first set's mf, the membrane's reference)
        self._rattle = {}
        refs = [s_ for s_ in sus if s_["_set"] == 0 and s_["dynamic"] == "mf"]
        for s_ in refs or sus:
            if "rattle_db" in s_:
                self._rattle.setdefault(s_["note"], []).append(s_["rattle_db"])
        self._strike = cap_strike(self.fit_strike_all(sus) if self.strike_all else self.fit_strike(sus)) if self.has_strike else None
        self._rise = self.fit_rise(sus) if self.has_strike else None

    def track(self, s, j):
        return track(s, j, self.tracks, self.tols)

    def mode_level(self, s, j, m):
        """Take s's mode m of track j at the onset (dBFS): the decay fit's level (`level_db`). With
        `spec_levels` an upper mode's from the spectrum instead, re the fundamental's there, both
        taken back to the onset by their decays (`spec_correction`): the balafon's upper modes
        start with a spike (700–1200 dB/s over its first 30 ms) before their 80–120 dB/s, and the
        decay fit's line through both read them 5–20 dB over the spectrum (C5's third mode at the
        fundamental's level where 10–50 ms after the strike it lies 13–48 dB under)."""
        if not self.spec_levels or j == 0:
            return m["level_db"] + s["peak_dbfs"]
        f1m = self.track(s, 0)
        if not f1m:
            return m["level_db"] + s["peak_dbfs"]
        rel = (m["spec_level_db"] - spec_correction(decay(m) or 0.0)) - (f1m["spec_level_db"] - spec_correction(decay(f1m) or 0.0))
        return f1m["level_db"] + s["peak_dbfs"] + rel

    def steps(self, s):
        """A take's dynamic steps (log2 of its set's speed)."""
        return np.log2(self.set_speeds[s["_set"]][s["dynamic"]])

    # ---- per key
    def f1(self, n):
        """The reference mode's frequency (the fundamental; the tubes' fourth mode, `ref_ratio` × the pitch)."""
        return self.ref_ratio * 440.0 * 2 ** ((n - 69) / 12) * 2 ** (self._tuning(n) / 1200)

    def recorded_cents(self, n):
        """Key n's own pitch re the curve it plays (`f1`, scale.recorded_cents): its takes over
        every set; 0 unrecorded."""
        import scale
        return scale.recorded_cents(self._samples, n, self._tuning)

    def ratios(self, n):
        return np.array([1.0] + [float(np.exp(self._ratio[j](n))) for j in range(1, len(self.tracks))])

    def sigmas(self, n):
        """Per mode the loss (1/s). An upper mode never outlasts the fundamental (every
        measured key of the three instruments has it 1.4–25 × faster), and above the keys its
        decay was measured on it keeps its ratio to the fundamental's from there (the
        xylophone's treble modes die within milliseconds, too fast to fit: carried on, the
        second mode's loss fell to a third of the fundamental's and rang over it)."""
        if self.hold_sigma:
            n = min(max(n, self.recorded[0]), self.recorded[1])
        s = np.array([float(np.exp(c(n))) for c in self._sigma])
        s1 = s[0]
        for j in range(1, len(s)):
            if self.lower_free and nominal(self.tracks[j], n) < 1.0:
                continue
            hi = self._sigma_span[j][1]
            if n > hi:
                s[j] = max(s[j], s1 * float(np.exp(self._sigma[j](hi) - self._sigma[0](hi))))
            s[j] = max(s[j], s1)
        return s

    def damper_sigmas(self, n):
        if self.damper_db_s is not None:
            return np.full(len(self.tracks), self.damper_db_s / NEPER_DB)
        if not self._damper:
            return np.zeros(len(self.tracks))
        return np.array([float(np.exp(c(n))) for c in self._damper])

    def freqs(self, n):
        return self.f1(n) * self.ratios(n)

    def level_db(self, n, steps, mallet=None):
        m, ref = mallet or self.mallet, self.mallet
        return mallet_db(self.freqs(n), steps, m["f_q"], m["fq_oct_per_step"], m["order"],
                         cutoff_scale(self.f1(n), m.get("beta", 0.0), ref["f_q"], ref["order"]))

    def out_db(self, n):
        """Per mode the output weight (dB): measured level − the model's amplitude. Beyond the
        recorded keys the fundamental's level at ff is held, and the modes' balance at mf, as
        at the nearest recorded key (carried on, the fundamental fell towards the bass while
        the upper modes held, and below the mallet's cutoff they grew: quiet low keys, their
        overtones on top; held at mf, the top keys came out 12 dB over the range at ff, the
        mallet's lowpass opening on their high fundamentals)."""
        lo, hi = self.recorded
        if lo <= n <= hi:
            return np.array([c(n) for c in self._level]) + self._fade(n)
        edge = lo if n < lo else hi
        at = np.array([c(edge) for c in self._level])
        balance = at + self.level_db(edge, 0.0) - (at[0] + self.level_db(edge, 0.0)[0])
        m = self.level_db(n, 0.0)
        s_ff = np.log2(self.speeds["ff"])
        out_1 = at[0] + self.level_db(edge, s_ff)[0] - self.level_db(n, s_ff)[0]
        return out_1 + balance - (m - m[0]) + self._fade(n)

    # ---- fits
    def fit_mallet(self, sus):
        """Per set (mallet) the speeds of its pp, mf and ff (the first set's mf: 1) and its
        lowpass: the modes' level changes across the dynamics and sets re the first set's mf,
        per key and mode (mode 1's mostly set the speeds, the upper modes' the lowpass). One
        dynamic recorded: the assumed mallet."""
        if self.assumed_mallet:
            m = self.assumed_mallet
            self.no_mf, self.mallet_rms_db = False, 0.0
            return [dict(m["speeds"])], [{k: float(m[k]) for k in ("f_q", "fq_oct_per_step", "order", "beta")}]
        by = {}
        for s in sus:
            for j in range(len(self.tracks)):
                m = self.track(s, j)
                if m and m.get("range_db", 0) > 20:
                    by.setdefault((s["note"], j), {})[(s["_set"], s["dynamic"])] = self.mode_level(s, j, m)
        # a first set without mf (the crotales: pp and ff): its mf midway between them in log
        # speed (speed 1), the level changes re its ff
        self.no_mf = not any(s["_set"] == 0 and s["dynamic"] == "mf" for s in sus)
        ref = (0, "ff" if self.no_mf else "mf")
        rows = [(n, j, k, lv[k] - lv[ref]) for (n, j), lv in by.items() if ref in lv for k in lv if k != ref]
        n_sets = len(self.mallet_names)
        f = np.array([self.freqs(n)[j] for n, j, _, _ in rows])
        f1 = np.array([self.f1(n) for n, _, _, _ in rows])
        diff = np.array([r[3] for r in rows])
        # per row its set and its dynamic's index among the set's free steps
        st_idx = np.array([r[2][0] * 3 + DYNAMICS.index(r[2][1]) for r in rows])
        set_idx = np.array([r[2][0] for r in rows])
        wgt = np.array([1.0 if r[1] == 0 else 0.5 for r in rows])

        def unpack(x):
            # per set: steps of pp, mf, ff (the first set's mf fixed at 0), log f_q, octaves, order;
            # then the cutoff's scaling with the bar (all sets; 0 without)
            steps = np.insert(x[:3 * n_sets - 1], 1, 0.0).reshape(n_sets, 3)
            lp = x[3 * n_sets - 1:6 * n_sets - 1].reshape(n_sets, 3)
            beta = x[6 * n_sets - 1] if self.scale_cutoff else 0.0
            # a set recorded at one dynamic (the bells' brass): its others spaced as its source set's,
            # its cutoff opening as that one's
            if self.no_mf:
                steps[0, 0] = -steps[0, 2]
            if self.pp_mirror:
                steps[0, 0] = -steps[0, 2]
            for k, src in self.tied.items():
                steps[k] = steps[k, 1] + steps[src] - steps[src, 1]
                lp[k, 1] = lp[src, 1]
            return steps, lp, beta

        def resid(x):
            steps, lp, beta = unpack(x)
            st = steps.reshape(-1)[st_idx]
            lfq, oct_ = lp[set_idx, 0], lp[set_idx, 1]
            # one order for all the mallets (the head's shape; per mallet it traded off against the cutoff)
            order = lp[0, 2]
            sc = cutoff_scale(f1, beta, np.exp(lp[0, 0]), order)
            st_ref = steps[0, DYNAMICS.index(ref[1])]
            model = mallet_db(f, st, np.exp(lfq), oct_, order, sc) - mallet_db(f, st_ref, np.exp(lp[0, 0]), lp[0, 1], order, sc)
            return (model - diff) * wgt

        x0 = [-2.5, 1.5] + [-0.5, 0.0, 1.0] * (n_sets - 1) + [np.log(1500.0), 1.0, 2.0] * n_sets + [0.5] * self.scale_cutoff
        lo = [-8.0, 0.2] + [-8.0, -3.0, -2.0] * (n_sets - 1) + [np.log(self.fq_min), 0.0, 0.5] * n_sets + [-1.0] * self.scale_cutoff
        hi = [-0.2, 5.0] + [2.0, 3.0, 5.0] * (n_sets - 1) + [np.log(20000.0), 3.0, 6.0] * n_sets + [2.5] * self.scale_cutoff
        # soft_l1: the dynamics were played by hand, single takes stray by 10 dB and more
        r = least_squares(resid, x0, loss="soft_l1", f_scale=3.0, bounds=(lo, hi))
        self.mallet_rms_db = float(np.sqrt(np.mean((resid(r.x) / wgt) ** 2)))
        steps, lp, beta = unpack(r.x)
        speeds = [{d: float(2 ** steps[k, i]) for i, d in enumerate(DYNAMICS)} for k in range(n_sets)]
        mallets = [{"f_q": float(np.exp(lp[k, 0])), "fq_oct_per_step": float(lp[k, 1]), "order": float(lp[0, 2]),
                    **({"beta": float(beta)} if self.scale_cutoff else {})} for k in range(n_sets)]
        return speeds, mallets

    def _fade(self, n):
        """Per mode the fade (dB) at key n beyond the keys it was measured on (none where they
        reach to within FRAC of the recorded keys' end)."""
        lo, hi = self.recorded
        out = np.zeros(len(self.tracks))
        for j, (a, b) in enumerate(self._level_span):
            if j == 0:
                continue
            if a > lo + FRAC and n < a:
                out[j] -= MODE_FADE_DB * (a - n)
            if b < hi - FRAC and n > b:
                out[j] -= MODE_FADE_DB * (n - b)
        return out

    def fit_levels(self, sus):
        rows = {j: [] for j in range(len(self.tracks))}
        for s in sus:
            lv = self.level_db(s["note"], self.steps(s), self.set_mallets[s["_set"]])
            for j in range(len(self.tracks)):
                m = self.track(s, j)
                if m and m.get("range_db", 0) > 15:
                    w = m.get("conf", 0.1) * (0.3 if s["dynamic"] == "pp" else 1.0)
                    rows[j].append((s["note"], self.mode_level(s, j, m) - lv[j], w))
        self._level_span = [(min(r[0] for r in rows[j]), max(r[0] for r in rows[j])) for j in range(len(self.tracks))]
        return [_robust([r[0] for r in rows[j]], [r[1] for r in rows[j]], [r[2] for r in rows[j]],
                        LEVEL_SLOPE, no_rise=True) for j in range(len(self.tracks))]

    def clear(self, n, h):
        """No mode of key n (but the fundamental) within H_CLEAR of h·f1."""
        return all(abs(h / x - 1) > H_CLEAR for x in self.ratios(n)[1:])

    def fit_harmonics(self, sus, dmp):
        """Per harmonic number, free and damped: (level curve at ff, growth, decay ×)."""
        def take_clear(s, h):
            # nor one of the take's own modes (a peak decaying as a mode does, not as a harmonic:
            # near A5 the second mode is still at 4.00 where the fit's curve has left it)
            ms = sorted(s["modes"], key=lambda m: abs(m["ratio"] - 1))
            d1 = decay(ms[0]) if ms else None
            return not any(abs(h / m["ratio"] - 1) < H_CLEAR and decay(m) and d1 and decay(m) >= HARMONIC_DECAY * d1
                           for m in ms[1:])

        # the engine's fundamental from `lo` to ff with set st's mallet (dB)
        def rise(n, st, lo):
            sp, ml = self.set_speeds[st], self.set_mallets[st]
            return self.level_db(n, np.log2(sp["ff"]), ml)[0] - self.level_db(n, np.log2(sp[lo]), ml)[0]

        def law(samples, h):
            if not samples:
                return None
            rows = [(s["note"], (s["_set"], s["dynamic"]), hm) for s in samples for hm in s.get("harmonics", [])
                    if hm["h"] == h and hm["snr_db"] >= H_SNR_DB and self.clear(s["note"], h) and take_clear(s, h)]
            ratios = [hm["decay_db_s"] / hm["f1_decay_db_s"] for _, _, hm in rows
                      if hm["decay_db_s"] and hm["f1_decay_db_s"] and hm["decay_db_s"] > 0 and hm["f1_decay_db_s"] > 0.5]
            k = float(np.clip(np.median(ratios), 1.0, h)) if len(ratios) >= 3 else 2.0
            # at the strike: the early window's level plus the harmonic's faster decay to it
            corr = lambda hm: (k - 1) * (hm["f1_decay_db_s"] or 0.0) * H_EARLY_MID_S  # noqa: E731
            at = lambda hm: hm["level_db"] + corr(hm)  # noqa: E731
            rows = [r for r in rows if corr(r[2]) <= H_MAX_CORR_DB]
            # one dynamic (mf): carried to ff as a free bar's harmonic grows (ASSUMED_GROWTH dB per
            # dB of the fundamental)
            if self.assumed_mallet:
                rows = [(n, (st, "ff"), {**hm, "level_db": hm["level_db"] + ASSUMED_GROWTH * rise(n, st, "mf")})
                        for n, (st, _), hm in rows]
            ff = [(n, at(hm)) for n, (_, d), hm in rows if d == "ff"]
            if len({n for n, _ in ff}) < H_MIN_KEYS:
                return None
            curve = _robust([n for n, _ in ff], [v for _, v in ff], np.ones(len(ff)), 0.0, frac=H_FRAC)
            span = (min(n for n, _ in ff), max(n for n, _ in ff))
            by = {}
            for n, (st, d), hm in rows:
                by.setdefault((n, st), {})[d] = at(hm)

            # per take pair: re the engine's fundamental from mf (else pp) to ff with that set's mallet
            low = lambda v: "pp" if self.no_mf else "mf"  # noqa: E731
            g = [(v["ff"] - v[low(v)]) / rise(n, st, low(v)) for (n, st), v in by.items()
                 if "ff" in v and low(v) in v and rise(n, st, low(v)) > 1.0]
            growth = float(np.clip(np.median(g), 0.0, 4.0)) if len(g) >= 4 else ASSUMED_GROWTH if self.assumed_mallet else None
            return curve, growth, k, span

        hs, laws = [], []
        for h in range(2, H_TOP + 1):
            free, damped = law(sus, h), law(dmp, h)
            if free is None and damped is None:
                continue
            hs.append(h)
            laws.append((free, damped))
        # a growth from too few keys: the second harmonic's
        g2 = [laws[0][i][1] if laws and laws[0][i] else 1.0 for i in (0, 1)]
        laws = [tuple(None if l is None else (l[0], g2[i] if l[1] is None else l[1], l[2], l[3]) for i, l in enumerate(p))
                for p in laws]
        return hs, laws

    def _covers(self, span, n):
        """Key n within a harmonic's measured span (± FRAC), above it (held towards the treble: there
        the takes decay too fast for the early window, H_MAX_CORR_DB, though the harmonics sound), or
        below the recorded keys where the span reaches them."""
        (a, b), (lo, hi) = span, self.recorded
        return a - FRAC <= n or (n < lo and a <= lo + FRAC)

    def harmonics(self, n):
        """Key n's harmonics: per number the free and damped (level dB re f1 at ff, growth, decay ×)."""
        out = {"h": list(self.harmonic_numbers)}
        for tag, i in (("", 0), ("damped_", 1)):
            laws = [p[i] for p in self._harm]
            out[tag + "level_db"] = [float(l[0](n)) if l and self.clear(n, h) and self._covers(l[3], n) else H_NONE_DB
                                     for h, l in zip(self.harmonic_numbers, laws)]
            out[tag + "growth"] = [float(l[1]) if l else 1.0 for l in laws]
            out[tag + "decay"] = [float(l[2]) if l else 2.0 for l in laws]
        return out

    def fit_splits(self, sus):
        """Per track: the measured (beat rate Hz, swing dB) per recorded key (the median over its
        takes) and the track's medians and log spread of the rate (for the other keys)."""
        out = []
        for j in range(len(self.tracks)):
            by = {}
            for s in sus:
                m = self.track(s, j)
                if m and "beat_db" in m:
                    by.setdefault(s["note"], []).append((m["beat_hz"], m["beat_db"]))
            keys = {n: (float(np.median([v[0] for v in vs])), float(np.median([v[1] for v in vs]))) for n, vs in by.items()}
            hz = [v[0] for v in keys.values()] or [0.5]
            db = [v[1] for v in keys.values()] or [0.5]
            spread = float(np.std(np.log(hz))) if len(hz) >= 3 else 0.5
            out.append({"keys": keys, "hz": float(np.median(hz)), "db": float(np.median(db)), "spread": spread})
        # a track measured on fewer than 3 keys: the median swing over all tracks
        all_db = float(np.median([d for t in out for _, d in t["keys"].values()]))
        for t in out:
            if len(t["keys"]) < 3:
                t["db"] = all_db
        return out

    def splits(self, n):
        """Key n's split per track: (Δf Hz, share of the mode in its partner) (`physics` split: the
        copy carries the share, the mode 1 − share). A swing of std d dB is a partner at about
        q = d / SPLIT_DB_PER_Q of the mode's amplitude (|1 + q·e^{iφ}| in dB, small q); share
        q / (1 + q). Keys without a take: the track's median swing, a rate drawn about its median
        (seeded per key and track: a rate does not interpolate), the sign of Δf drawn too."""
        hz, share = [], []
        for j, t in enumerate(self._splits or []):
            rng = np.random.default_rng(SPLIT_SEED + 97 * n + j)
            f, d = t["keys"].get(n, (t["hz"] * float(np.exp(t["spread"] * rng.standard_normal())), t["db"]))
            q = min(d / SPLIT_DB_PER_Q, SPLIT_Q_MAX)
            hz.append(float(f) * (1.0 if rng.uniform() < 0.5 else -1.0))
            share.append(float(q / (1.0 + q)))
        return hz, share

    def fit_halo(self, sus):
        """The other tines ringing along: per take and tine the analysis's level re the take's
        fundamental over its window, taken back to the onset by both fitted decays
        (`halo_correction`); a level per tine (how readily it rings) plus a smooth term over the
        plucked key, L = H_j + g(i), by alternating medians (HALO_ITER). A tine not heard in a take
        counts HALO_CENSOR_DB below the level it would have needed (the treble's tines ring in 6 of
        26 takes: from the heard ones alone they came out the loudest). The tines: the recorded
        notes; one never heard ringing gets the quietest's level less HALO_UNHEARD_DB."""
        rows = []
        for s in sus:
            d1 = self.sigmas(s["note"])[0] * NEPER_DB
            for h in s.get("halo", []):
                dj = self.sigmas(h["note"])[0] * NEPER_DB
                lv = h["level_db"] - (0.0 if h.get("heard", True) else HALO_CENSOR_DB)
                rows.append((s["note"], h["note"], lv + halo_correction(d1) - halo_correction(dj), h.get("heard", True)))
        tines = sorted({s["note"] for s in sus})
        H = {j: 0.0 for j in tines}
        keys = sorted({i for i, _, _, _ in rows})
        g = {i: 0.0 for i in keys}
        heard = {j for _, j, _, h in rows if h}
        for _ in range(HALO_ITER):
            g = {i: float(np.median([l - H[j] for a, j, l, _ in rows if a == i])) for i in keys}
            for j in heard:
                H[j] = float(np.median([l - g[i] for i, b, l, _ in rows if b == j]))
            mid = float(np.median([H[j] for j in heard]))
            H = {j: v - mid for j, v in H.items()}
        low = min(H[j] for j in heard)
        H = {j: (v if j in heard else low - HALO_UNHEARD_DB) for j, v in H.items()}
        curve = _robust(keys, [g[i] for i in keys], np.ones(len(keys)), LEVEL_SLOPE, no_rise=True)
        return {"H": H, "g": curve}

    def rattle(self, n):
        """Key n's rattle (dB re its fundamental, the analysis's `rattle_db`): the nearest recorded
        key's (each tine its own: −45 … −81 dB, no trend over the keys; the balafon's gourds −38 …
        −56 at mf), or None."""
        if not self._rattle:
            return None
        near = min(self._rattle, key=lambda m: (abs(m - n), m))
        return float(np.mean(self._rattle[near]))

    def halo(self, n):
        """Key n's ringing tines: (ratio re its fundamental, loss 1/s, level dB re its fundamental
        at the onset), those above HALO_MIN_DB; not the tine at its own pitch."""
        out = []
        if not self._halo:
            return out
        for j, hj in self._halo["H"].items():
            if j == n:
                continue
            lv = self._halo["g"](n) + hj
            if lv > HALO_MIN_DB:
                out.append((self.f1(j) / self.f1(n), float(self.sigmas(j)[0]), float(lv)))
        return out

    def fit_strike(self, sus):
        """Per recorded take the modes the tracks and harmonics leave out, as measured (the hand
        chimes' tubes: a dozen modes each, no two tubes alike, so no track over the keys): those
        of the analysis's spectrum (its peak re the fundamental's, `spec_level_db`, taken back to
        the onset by both decays, `spec_correction`; the decay fit's `level_db` read weak modes
        beside a strong fundamental 20–30 dB high: G♯5's 1.24 and 1.34 at −25 and −15 dB re it,
        −54 and −47 in the spectrum) and the strike's short-lived ones (`strike`: the clapper's
        hit, 1–16 kHz, gone within 0.02–0.2 s). With `strike_line_db` only lines that far over the
        spectrum around them (the balafon: its buzz's own peaks left out). A strike peak beside a mode of the spectrum (STRIKE_SAME) counts only
        where it falls STRIKE_FAST × faster: that mode's first, fast stage. Left out: within the
        tolerance of a track or STRIKE_SAME of a whole-number ratio up to H_TOP (the fit's
        harmonics), and anything under STRIKE_MIN_DB. Per take [(ratio, loss 1/s, level dB re the
        fundamental at the onset)]."""
        out = {}
        # several sets and dynamics (the balafon): the first set's mf, as the levels' reference
        refs = [s for s in sus if s["_set"] == 0 and s["dynamic"] == "mf"]
        for s in refs or sus:
            n = s["note"]
            f1m = self.track(s, 0)
            if not f1m:
                continue
            tracked = [self.track(s, j) for j in range(len(self.tracks))]
            own = lambda r: (any(abs(r / nominal(self.tracks[j], n) - 1) < self.tols[j] for j in range(len(self.tracks)))  # noqa: E731
                             or any(abs(r / h - 1) < STRIKE_SAME for h in range(2, H_TOP + 1)))
            rows, slow = [], []
            for m in s["modes"]:
                d = decay(m)
                if any(m is t for t in tracked) or own(m["ratio"]) or not d or d <= 0:
                    continue
                if self.strike_line_db is not None and m.get("line_db", 0.0) < self.strike_line_db:
                    continue
                slow.append((m["ratio"], d))
                lv = (m["spec_level_db"] - spec_correction(d)) - (f1m["spec_level_db"] - spec_correction(decay(f1m) or 0.0))
                if lv > STRIKE_MIN_DB:
                    rows.append((m["ratio"], d / NEPER_DB, lv))
            for p in s.get("strike", []):
                r, d = p["ratio"], p["decay_db_s"]
                if d is None or d <= 0 or own(r) or p["level_db"] <= STRIKE_MIN_DB:
                    continue
                if self.strike_line_db is not None and p.get("line_db", 0.0) < self.strike_line_db:
                    continue
                if any(abs(r / rs - 1) < STRIKE_SAME and d < STRIKE_FAST * ds for rs, ds in slow):
                    continue
                rows.append((r, d / NEPER_DB, p["level_db"]))
            out[n] = rows
        return out

    def fit_strike_all(self, sus):
        """`fit_strike` over every take (the balafon: a line the traditional mallet at mf leaves
        under the buzz, the hard one sets ringing: C♯4's 10.5, 13.2 and 23.0, F5's 6.4, 7.6 and
        10.5). Per take its lines (`strike_line_db` over the spectrum around them), their levels
        re the fundamental taken to the reference's mallet and dynamic (the first set's mf) by
        the mallets' fitted lowpasses at both frequencies (STRIKE_MIN_DB on the level as heard: at
        the reference C♯4's 23.0 lies 40 dB under the hard mallet's); a line within STRIKE_SAME
        in at least STRIKE_MIN_TAKES takes is a mode: its ratio, loss and level the takes' medians."""
        out = {}
        ref = self.set_mallets[0]
        for n in sorted({s["note"] for s in sus}):
            f1 = self.f1(n)
            own = lambda r: (any(abs(r / nominal(self.tracks[j], n) - 1) < self.tols[j] for j in range(len(self.tracks)))  # noqa: E731
                             or any(abs(r / h - 1) < STRIKE_SAME for h in range(2, H_TOP + 1)))

            def lp(m, steps, f):
                return float(mallet_db(f, steps, m["f_q"], m["fq_oct_per_step"], m["order"],
                                       cutoff_scale(f1, m.get("beta", 0.0), ref["f_q"], ref["order"])))
            seen = []
            for k, s in enumerate(t for t in sus if t["note"] == n):
                f1m = self.track(s, 0)
                if not f1m:
                    continue
                m_s, st = self.set_mallets[s["_set"]], np.log2(self.set_speeds[s["_set"]][s["dynamic"]])
                to_ref = lambda r: (lp(ref, 0.0, f1 * r) - lp(ref, 0.0, f1)) - (lp(m_s, st, f1 * r) - lp(m_s, st, f1))  # noqa: E731
                tracked = [self.track(s, j) for j in range(len(self.tracks))]
                for m in s["modes"]:
                    d = decay(m)
                    if (any(m is t for t in tracked) or own(m["ratio"]) or not d or d <= 0
                            or m.get("line_db", 0.0) < self.strike_line_db):
                        continue
                    lv = (m["spec_level_db"] - spec_correction(d)) - (f1m["spec_level_db"] - spec_correction(decay(f1m) or 0.0))
                    if lv > STRIKE_MIN_DB:
                        seen.append((m["ratio"], d, lv + to_ref(m["ratio"]), k))
                for p in s.get("strike", []):
                    r, d = p["ratio"], p["decay_db_s"]
                    if (d is None or d <= 0 or own(r) or p.get("line_db", 0.0) < self.strike_line_db
                            or p["level_db"] <= STRIKE_MIN_DB):
                        continue
                    seen.append((r, d, p["level_db"] + to_ref(r), k))
            seen.sort()
            rows, group = [], []
            for x in seen + [None]:
                if group and (x is None or x[0] / group[-1][0] - 1 > STRIKE_SAME):
                    if len({g[3] for g in group}) >= STRIKE_MIN_TAKES:
                        rows.append((float(np.median([g[0] for g in group])), float(np.median([g[1] for g in group])) / NEPER_DB,
                                     float(np.median([g[2] for g in group]))))
                    group = []
                if x is not None:
                    group.append(x)
            out[n] = rows
        return out

    def fit_rise(self, sus):
        """Per recorded take the fundamental's swell (the analysis's `rise`): its envelope
        e^{−σ1·t}·(1 − e^{−σ·t})², two partners at its frequency (−2 at σ1 + σ, +1 at σ1 + 2σ): a
        quadratic start (the low tubes' first 8 ms lie 20–38 dB under the swell's top: 1 − e^{−σ·t}
        missed them by up to 3.4 dB rms, the square by 0.5–1.1, C5 2.3: its first 4 ms at −38 dB,
        then a jump). σ by a grid search (RISE_SIGMA) on the levels in the analysis's windows (RISE_WIN_S
        Hann, from the onset, RISE_HOP_S apart; each down to RISE_FLOOR_DB), from the onset until
        RISE_TAIL windows past the first within RISE_TOP_DB of the largest, both aligned there. A
        swell only where the first windows lie RISE_MIN_DB under the largest (in the treble the
        8 ms windows' 125 Hz take in the strike's modes beside the fundamental: its level there
        swings, it does not swell). {note: σ (1/s), None without}."""
        out = {}
        w = int(RISE_WIN_S * RISE_SR)
        hop = int(RISE_HOP_S * RISE_SR)
        han = np.hanning(w)
        for s in sus:
            e = s.get("rise")
            if not e:
                continue
            e = np.maximum(np.array(e), RISE_FLOOR_DB)
            if e[:RISE_HEAD].mean() > RISE_MIN_DB:
                out[s["note"]] = None
                continue
            top = int(np.argmax(e >= RISE_TOP_DB))
            end = min(top + RISE_TAIL, len(e))
            t = np.arange(end * hop + w) / RISE_SR
            best = (np.inf, None)
            for sg in RISE_SIGMA:
                g = (1 - np.exp(-sg * t)) ** 2
                m = 20 * np.log10(np.array([abs((g[i * hop:i * hop + w] * han).sum()) for i in range(end)]) + 1e-12)
                m = np.maximum(m - m[top] + e[top], RISE_FLOOR_DB)
                best = min(best, (float(((m - e[:end]) ** 2).mean()), float(sg)))
            out[s["note"]] = best[1]
        return out

    def rise(self, n):
        """Key n's swell σ (1/s): the nearest recorded take's (`fit_rise`), or None."""
        if not self._rise:
            return None
        return self._rise[min(self._rise, key=lambda m: (abs(m - n), m))]

    def strike(self, n):
        """Key n's strike modes: the nearest recorded take's (ratio, loss 1/s, level dB re the
        fundamental at the onset); within the keys a whole tone apart the one below on a tie."""
        if not self._strike:
            return []
        near = min(self._strike, key=lambda m: (abs(m - n), m))
        return [(r, sg, lv) for r, sg, lv in self._strike[near] if self.f1(n) * r < STRIKE_F_MAX]

    def summary(self):
        return {"recorded": self.recorded, "mallets": {name: {"speeds": sp, **ml} for name, sp, ml in
                                                       zip(self.mallet_names, self.set_speeds, self.set_mallets)},
                "mallet_rms_db": self.mallet_rms_db}


def spec_correction(decay_db_s):
    """dB from a mode's level at the onset to its peak in the analysis's spectrum
    (modes.SPEC_S from 5 ms, Blackman-Harris weighted, the mode decaying)."""
    t = np.linspace(SPEC_WIN[0], SPEC_WIN[1], 500)
    w = np.blackman(len(t))
    return float(20 * np.log10((w * 10 ** (-decay_db_s * t / 20)).sum() / w.sum()))


def halo_correction(decay_db_s):
    """dB from a mode's level at the onset to its peak over the analysis's halo window
    (modes.HALO_S, Hann-weighted, the mode decaying)."""
    t = np.linspace(HALO_S[0], HALO_S[1], 500)
    w = np.hanning(len(t))
    return float(20 * np.log10((w * 10 ** (-decay_db_s * t / 20)).sum() / w.sum()))


# the analysis's spectrum (modes.py: SPEC_S from 5 ms after the onset)
SPEC_WIN = (0.005, 0.305)
# the halo: analysis/modes.py's window; its fit's rounds; a tine never heard ringing this far under
# the quietest heard; ringing tines kept down to this (dB re the plucked one's fundamental)
HALO_S = (0.05, 0.55)
HALO_ITER = 10
HALO_UNHEARD_DB = 6.0
HALO_CENSOR_DB = 6.0
HALO_MIN_DB = -60.0
# the strike's modes (`fit_strike`): one mode within this (relative), a strike peak this much
# faster than the mode beside it its first stage, none under this (dB re the fundamental), none
# above this (Hz); over every take (`fit_strike_all`) a line in at least this many
STRIKE_SAME = 0.015
STRIKE_FAST = 3.0
STRIKE_MIN_DB = -40.0
STRIKE_MIN_TAKES = 2
STRIKE_F_MAX = 20000.0
# a take's strike over its neighbors': its loudest mode at most this far over the median of its four
# nearest takes' loudest (two each side), the take's modes lowered alike (`cap_strike`)
STRIKE_OUTLIER_DB = 10.0
def cap_strike(out):
    """The takes' strikes ({note: [(ratio, loss, level dB)]}) with a take's far over its
    neighbors' lowered: its loudest mode to STRIKE_OUTLIER_DB over the median of its four nearest
    takes' loudest (two each side, the ends from one), its modes alike. A hit unlike the others
    (the hand chimes' A5: a line at 1151 Hz as loud as the fundamental, the tubes around it 26–38
    dB under theirs; its file, normalized to its peak, hid it) is no trait of the instrument; the
    bass's louder strikes follow the register and stay."""
    top = {n: max(lv for _, _, lv in rows) for n, rows in out.items() if rows}
    notes = sorted(top)
    capped = dict(out)
    for i, n in enumerate(notes):
        below, above = notes[:i][::-1], notes[i + 1:]
        near = below[:2] + above[:2]
        near += (above[2:] if len(below) < 2 else below[2:])[: 4 - len(near)]
        if not near:
            continue
        line = float(np.median([top[m] for m in near])) + STRIKE_OUTLIER_DB
        if top[n] > line:
            capped[n] = [(r, sg, lv - (top[n] - line)) for r, sg, lv in out[n]]
    return capped


# the fundamental's swell (`fit_rise`): the analysis's windows (modes.FAST_WIN_S, FAST_HOP_S), its
# floor, the grids
RISE_WIN_S = 0.008
RISE_HOP_S = 0.002
RISE_SR = 48000
RISE_FLOOR_DB = -25.0
RISE_HEAD = 3
RISE_MIN_DB = -6.0
RISE_TOP_DB = -1.0
RISE_TAIL = 5
RISE_SIGMA = np.geomspace(10.0, 2000.0, 80)

# the splits: std (dB) of 20·log10|1 + q·e^{iφ}| per unit q (20/ln 10/√2), at most SPLIT_Q_MAX
SPLIT_DB_PER_Q = 20 / np.log(10) / np.sqrt(2)
SPLIT_Q_MAX = 0.6
SPLIT_SEED = 4471

KEYS = range(21, 109)


def save(fit, profile):
    """The fit per key (21…108) as the part's fit document (what export reads)."""
    # (the recorded keys within the keyboard's: the bells' run to E8)
    doc = {"profile": profile, "recorded": [max(fit.recorded[0], KEYS[0]), min(fit.recorded[1], KEYS[-1])], "hammer_speeds": fit.speeds,
           "mallet": {**fit.mallet, "gain_db_per_step": GAIN_DB_PER_STEP, "rms_db": fit.mallet_rms_db},
           # every mallet (the first is "mallet" and "hammer_speeds" above)
           "mallets": [{"name": name, "speeds": sp, **ml} for name, sp, ml in zip(fit.mallet_names, fit.set_speeds, fit.set_mallets)],
           "damped": fit.damped,
           # tracks = "auto": the tracks the fit derived and used
           **({"tracks": {"tracks": fit.tracks, "tol": fit.tols}} if fit.auto_tracks else {}),
           "keys": [{"note": n, "f1": fit.f1(n), "ratios": fit.ratios(n).tolist(), "sigma": fit.sigmas(n).tolist(),
                     "damper_sigma": fit.damper_sigmas(n).tolist(), "out_db": fit.out_db(n).tolist(),
                     "bar_mass": bar_mass(fit.f1(n)), "harmonics": fit.harmonics(n),
                     "recorded_cents": round(fit.recorded_cents(n), 2),
                     **(dict(zip(("split_hz", "split_share"), fit.splits(n))) if fit.has_splits else {}),
                     **({"halo": halo_doc(fit, n)} if fit.has_halo else {}),
                     **({"rattle_db": fit.rattle(n)} if fit.has_rattle else {}),
                     **({"strike": extra_doc(fit, n, fit.strike(n)), "rise": fit.rise(n)} if fit.has_strike else {})} for n in KEYS]}
    P.write_fit(profile, doc)
    return doc


def halo_doc(fit, n):
    """Key n's ringing tines as extra modes (`extra_doc`)."""
    return extra_doc(fit, n, fit.halo(n))


def extra_doc(fit, n, h):
    """Extra modes of key n, h: [(ratio, loss, level dB re the fundamental at the onset)]: ratio,
    loss and output weight (dB; the level with the mallet's lowpass taken out at both
    frequencies, as `out_db`)."""
    f1 = fit.f1(n)
    m = fit.mallet
    sc = cutoff_scale(f1, m.get("beta", 0.0), m["f_q"], m["order"])
    lp = lambda f: float(mallet_db(f, 0.0, m["f_q"], m["fq_oct_per_step"], m["order"], sc))  # noqa: E731
    o1 = float(fit.out_db(n)[0])
    return {"ratio": [r for r, _, _ in h], "sigma": [s for _, s, _ in h],
            "out_db": [o1 + lp(f1) - lp(f1 * r) + lv for r, _, lv in h]}


def load(profile):
    return P.load_fit(profile)


def main():
    ap = argparse.ArgumentParser(description="Fit an instrument's bars and write its fit document")
    ap.add_argument("--profile", required=True, help="the bars' part")
    args = ap.parse_args()
    fit = Fit(args.profile)
    save(fit, args.profile)
    print(json.dumps(fit.summary()))


if __name__ == "__main__":
    main()
