"""Per-note model parameters across the keyboard ("scale design").

Measured quantities come from an analysis profile (one dataset = one instrument),
smoothed across the keyboard: tuning, inharmonicity B, loss b1'/b3.
Everything that cannot be measured from recordings follows smooth design curves
typical of a ~2 m grand piano or a ~125 cm upright (the scale model: [strings] design): string length, tension,
number of strings, strike position, hammer mass and felt, soundboard size. Felt curves are meant to be refined by fitting
(fit_hammer.py) at a few support notes.
"""
import json
import os
from dataclasses import replace

import numpy as np

import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analysis"))
import project as P  # noqa: E402
import string_model as M  # noqa: E402
from board import DENSITY, Board  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
DB_PER_NEPER = 20 / np.log(10)


EDGE_SPAN = 6.0   # semitones: the edge slope a limited extrapolation carries on
TUNING_SLOPE_MAX = 2.0   # cents per semitone (a piano's treble stretch: ~1–2)
# "As recorded" (`recorded_cents`): a key's own deviation from its curve at most this (a take
# measured a semitone off is a misreading, not a tuning)
RECORDED_MAX_CENTS = 100.0


def recorded_cents(samples, note, curve):
    """The takes at `note` (their `tuning_cents`): their median minus curve(note), clipped to
    ±RECORDED_MAX_CENTS; 0 without a take."""
    c = [s["tuning_cents"] for s in samples if s["note"] == note and s.get("tuning_cents") is not None]
    if not c:
        return 0.0
    return float(np.clip(np.median(c) - curve(note), -RECORDED_MAX_CENTS, RECORDED_MAX_CENTS))
LOGB_SLOPE_MAX = 0.1     # log B per semitone (B doubles in ~7 at the most)
# the fundamental's decays the keys past the measured loss take (their confidence): a top key's fit
# is short and its range small by nature (Grand G from C7: 0.04–0.37 at 12–17 dB), its rate no worse
EDGE_MIN_CONF = 0.05


def _smooth(notes, values, weights, frac=7.0, slope_max=None, rising=False):
    """Gaussian-weighted local linear regression over the keyboard (width in semitones);
    with `slope_max`, beyond the outermost notes the value there carries on with the slope
    over their last EDGE_SPAN semitones, at most slope_max per semitone. With `rising`, beyond
    the top note the value carries on with that slope, never falling (the bass as without)."""
    notes, values, weights = map(np.asarray, (notes, values, weights))
    if rising and slope_max is None:
        inner = _smooth(notes, values, weights, frac)
        hi = float(notes.max())
        span = min(EDGE_SPAN, hi - float(notes.min()))
        up = max((inner(hi) - inner(hi - span)) / span, 0.0) if span else 0.0
        return lambda n: inner(hi) + up * (n - hi) if n > hi else inner(n)
    if slope_max is not None:
        inner = _smooth(notes, values, weights, frac)
        lo, hi = float(notes.min()), float(notes.max())
        span = min(EDGE_SPAN, hi - lo)
        clip = lambda v: float(np.clip(v, -slope_max, slope_max))  # noqa: E731
        up = clip((inner(hi) - inner(hi - span)) / span) if span else 0.0
        down = clip((inner(lo + span) - inner(lo)) / span) if span else 0.0
        return lambda n: inner(hi) + up * (n - hi) if n > hi else inner(lo) - down * (lo - n) if n < lo else inner(n)

    def at(n):
        w = weights * np.exp(-0.5 * ((notes - n) / frac) ** 2)
        if w.sum() < 1e-9:
            # far beyond the data (the 4′ sounds up to C8, recorded to C7): hold the
            # value where the weights still carry it, approached in quarter semitones
            step = 0.25 * np.sign(np.clip(n, notes.min(), notes.max()) - n)
            return at(n + step) if step else np.nan
        X = np.column_stack([np.ones_like(notes), notes - n])
        coef, *_ = np.linalg.lstsq(X * np.sqrt(w)[:, None], values * np.sqrt(w), rcond=None)
        return coef[0]
    return at


def edge_b1(w, sig, conf, b3):
    """A key's b1' from its few partial decays (ω, σ in nepers/s, confidence) with b3 given:
    the confidence-weighted mean of σ − b3·ω², at least 0.05."""
    return max(float(np.sum(conf * (sig - b3 * w ** 2)) / np.sum(conf)), 0.05)


# the scale models (the strings' lengths, tensions and counts per key, the board's modal density):
# a project's [strings] design names one; its traits (fingered, picked, equal_tuning, bends,
# low_weighted) and its board seed are the project's
PLUCKED = ("harpsichord_8", "harpsichord_4", "pedal_harp", "lever_harp", "classical_guitar", "stick_dulcimer",
           "plucked_psaltery", "bridge_zither")
# struck by a tangent that stays on the string while the key is held (string_model "tangent"):
# the clavichord, double strung
TANGENT = ("unfretted_clavichord",)
# soundboard modal density (modes per Hz) ∝ board area: grand ~1.7 m², upright ~1.2 m²
# … harpsichord: between the two (assumed; a ~1.4 m² board, thinner)
BOARD_DENSITY = {"grand": DENSITY, "upright": 1 / 26.0, "harpsichord_8": float(np.sqrt(DENSITY / 26.0))}
BOARD_DENSITY["harpsichord_4"] = BOARD_DENSITY["harpsichord_8"]
# harp: a narrow soundboard down the soundbox, ~0.35 m² (assumed)
BOARD_DENSITY["pedal_harp"] = DENSITY * 0.35 / 1.7
# the lever harp: its soundbox 3/4 of the concert harp's (the engine's HarpDims::LEVER): ~0.2 m²
BOARD_DENSITY["lever_harp"] = DENSITY * 0.35 * 0.75 ** 2 / 1.7
# the guitar: a ~0.1 m² top, ~2.5 mm thin (a plate's modal density ∝ area/thickness: a quarter of
# a grand's 1.7 m² × 9 mm, assumed) — about one random mode per 70 Hz; its strong low modes
# (the top's ~220 Hz, the air's ~105 Hz) are measured (fit_body_modes.py)
BOARD_DENSITY["classical_guitar"] = DENSITY / 4
# the clavichord: a ~0.12 m² board, ~3 mm thin (assumed; as the guitar's, area/thickness re a grand's)
BOARD_DENSITY["unfretted_clavichord"] = DENSITY * (0.12 / 1.7) * (9.0 / 3.0)
# the psaltery: a triangular box ~0.1 m², ~4 mm (assumed); the strumstick: a ~0.05 m² top, ~3 mm;
# the đàn tranh: a long box ~1.1 × 0.25 m, ~4 mm (as the guitar's, area/thickness re a grand's)
BOARD_DENSITY["plucked_psaltery"] = DENSITY * (0.1 / 1.7) * (9.0 / 4.0)
BOARD_DENSITY["stick_dulcimer"] = DENSITY * (0.05 / 1.7) * (9.0 / 3.0)
BOARD_DENSITY["bridge_zither"] = DENSITY * (0.27 / 1.7) * (9.0 / 4.0)
# harpsichord pluck: quill lowpass at K_Q partials (with one recorded layer it trades
# against the soundboard's H and C, which take up the spectral envelope), apex displacement
K_Q = 20.0
PLUCK_AMP = 1.5e-3
# harp: the finger is a soft pluck, pulled further than a quill (assumed). Where its lowpass
# lies the two layers do not say (from mf to ff the fundamental rises ~6 dB, partials 2–20
# ~11–15: at 1…32 partials, a pad width of 12 mm…0.2 m or a fixed 50…1600 Hz the velocity
# fit's error is 4.96…5.14 dB, the takes' scatter); the soundboard fit takes up the rest
K_Q_HARP = 4.0
PLUCK_AMP_HARP = 3e-3
# the tangent: pushes the string ~1 mm out of its rest (assumed), its rise a lowpass at K_Q
# partials as the quill's (one recorded layer: the soundboard's H and C take up the envelope)
TANGENT_AMP = 1e-3
# its unisons' string scatter (NoteParams.scatter_cents): the pianos' fits' median (assumed;
# the recordings are in the noise after 1–1.5 s, too short for fine_structure.py's beat fit)
TANGENT_SCATTER_CENTS = 0.1




def strings(dataset):
    """The part's [strings] table (its project's, with the part's own over it)."""
    return P.section(dataset, "strings")


def design_of(dataset):
    """The part's scale model (`PLUCKED`, `TANGENT`, `speaking_length` …; default the grand's)."""
    return strings(dataset).get("design", "grand")


def traits(dataset):
    """The part's traits: fingered (a hand, no dampers), picked (a pick's lowpass), equal_tuning,
    bends (the pitch wheel acts), low_weighted (the polarization fit's low partials)."""
    return set(strings(dataset).get("traits", []))


def body_block(dataset):
    """The profile's "body_modes" block (fit_body_modes.py), or {}."""
    return P.load_fit(dataset).get("body_modes", {}) if P.has_fit(dataset) else {}


def body_modes(dataset):
    """The profile's measured board modes, or []."""
    return body_block(dataset).get("modes", [])


def body_share(dataset):
    """The share of the measured modes' admittance the horizontal polarization feels."""
    return float(body_block(dataset).get("h_share", 0.0))


# a measured mode's band (its bandwidths f/q either side): partials there decay by the mode,
# not by the string; the per-note loss fit leaves them out (else b1' takes the mode in and
# the model counts it twice)
BAND_WIDTHS = 3.0


def in_mode_band(freq, modes):
    return any(abs(freq - m["f"]) < BAND_WIDTHS * m["f"] / m["q"] for m in modes)


def radiation_seed(dataset):
    """The profile's radiation draw (soundboard.py's pick: no fundamental in a notch), or 0."""
    return int(P.load_fit(dataset).get("radiation", {}).get("seed", 0)) if P.has_fit(dataset) else 0


def make_board(dataset, rad_seed=None, eta_low=None):
    """The instrument's synthetic board; `eta_low` the low modes' loss factor (the export's [board]
    low_eta; the fits never see it)."""
    return Board(eta_low=eta_low, seed=strings(dataset).get("board_seed", 1), density=BOARD_DENSITY[design_of(dataset)],
                 modes=body_modes(dataset), rad_seed=radiation_seed(dataset) if rad_seed is None else rad_seed)


class Profile:
    """Smoothed measured parameters of one instrument (analysis dataset)."""

    def __init__(self, dataset, board=None, eta_low=None):
        samples = [s for s in P.load_cache(dataset, "analysis")["samples"]
                   if "error" not in s and "label_mismatch" not in s]
        self.dataset = dataset
        self.samples = samples
        notes = np.array([s["note"] for s in samples], float)
        # beyond the recorded keys the edge's trend at most TUNING_SLOPE_MAX / LOGB_SLOPE_MAX
        # per semitone: unlimited, Harpsichord C's one bad D1 (B 0.03, −182 cents) put A0 at
        # B 14 and −328 cents (bell-like, few partials), the lever harp's C2 (+32 cents) its
        # A0 at +106, Grand E's top keys +46
        self._tuning = _smooth(notes, [s["tuning_cents"] for s in samples],
                               [s["B_conf"] for s in samples], frac=3.0, slope_max=TUNING_SLOPE_MAX)
        if "equal_tuning" in traits(dataset):
            self._tuning = lambda n: 0.0
        self._logB = _smooth(notes, [np.log(max(s["B"], 1e-6)) for s in samples],
                             [s["B_conf"] for s in samples], frac=3.0, slope_max=LOGB_SLOPE_MAX)
        # loss: per note σ = b1' + b3·ω² fitted over all layers, then smoothed
        per_note, rows_at = {}, {}
        modes = body_modes(dataset)
        for n in sorted(set(notes.astype(int))):
            # energy-decay-curve rates: robust against beating (single-slope fits are not)
            ok = [p for s in samples if s["note"] == n for p in s["partials"]
                  if p.get("edc_decay_db_s", 0) > 0 and not in_mode_band(p["freq"], modes)]
            rows = [(2 * np.pi * p["freq"], p["edc_decay_db_s"] / DB_PER_NEPER, p["conf"]) for p in ok if p["conf"] > 0.3]
            rows_at[n] = [(2 * np.pi * p["freq"], p["edc_decay_db_s"] / DB_PER_NEPER, p["conf"])
                          for p in ok if p.get("n") == 1 and p["conf"] > EDGE_MIN_CONF]
            # partial decays per note the loss fit needs (`min_loss_rows`: the harp's treble has 1–3 per layer)
            if len(rows) >= strings(dataset).get("min_loss_rows", 8):
                w, sig, c = map(np.array, zip(*rows))
                X = np.column_stack([np.ones_like(w), w ** 2]) * c[:, None]
                (b1, b3), *_ = np.linalg.lstsq(X, sig * c, rcond=None)
                per_note[n] = (max(b1, 0.05), max(b3, 0.0), len(rows))
        ns = np.array(list(per_note), float)
        self._b3 = _smooth(ns, [v[1] for v in per_note.values()],
                           [np.sqrt(v[2]) for v in per_note.values()], frac=12.0)
        # the recorded keys outside the measured range with too few decays for both terms (a grand's
        # top 4–10 keys: the fundamental and a few partials) give b1' from their fundamental's decays
        # (EDGE_MIN_CONF) with b3 from its curve (past the measured keys that curve is no
        # measurement: on the upper partials σ − b3·ω² fell below zero); without them the edge's
        # slope carried on for up to an octave and turned down (Grand F's and D's top keys rang
        # about twice as long as recorded, step 150)
        b1_notes = dict((n, (v[0], v[2])) for n, v in per_note.items())
        if per_note:
            for n, rows in rows_at.items():
                if rows and not min(per_note) <= n <= max(per_note):
                    w, sig, c = map(np.array, zip(*rows))
                    b1_notes[n] = (edge_b1(w, sig, c, max(self._b3(n), 0.0)), len(rows))
        # the keys the loss was measured on (beyond them the curve carries its slope on: the report)
        self.loss_notes = sorted(int(n) for n in b1_notes)
        # beyond them the loss never falls toward the treble (a straight line at the edge's slope)
        self._logb1 = _smooth(list(b1_notes), [np.log(v[0]) for v in b1_notes.values()],
                              [np.sqrt(v[1]) for v in b1_notes.values()], rising=True)
        # instrument-specific felt curve and hammer speeds (fit_profile_felt.py), if fitted
        self.felt_fit = P.load_fit(dataset) if P.has_fit(dataset) else None
        # synthetic soundboard (statistics only, one random draw per instrument);
        # board=False: frequency-independent bridge coupling. Default: env BOARD (on)
        if board is None:
            board = os.environ.get("BOARD", "1") != "0"
        self.design = design_of(dataset)
        self.traits = traits(dataset)
        self.board = make_board(dataset, eta_low=eta_low) if board else None
        self.h_share = body_share(dataset)

    def f0(self, note):
        return 440.0 * 2 ** ((note - 69) / 12 + self._tuning(note) / 1200)

    def recorded_cents(self, note):
        """The recorded note's own pitch re the curve it plays (`f0`): its takes' median measured
        tuning minus the curve (cents, within ±RECORDED_MAX_CENTS); 0 for a note not recorded. The
        engine adds it at "As recorded" (the kalimba's tines, the đàn tranh's strings at equal
        temperament, a piano's keys around its smoothed stretch)."""
        return recorded_cents(self.samples, note, self._tuning)

    def B(self, note):
        return float(np.exp(self._logB(note)))

    def loss(self, note):
        return float(np.exp(self._logb1(note))), float(max(self._b3(note), 0.0))

    def felt(self, note):
        """(p, F05): the instrument's fitted felt curve, else the default design curve."""
        f = self.felt_fit
        if f is None or "felt_p" not in f:      # no felt fit (the harpsichord: plucked)
            return felt(note)
        if "felt_curve" in f:      # anchor curve (fit_profile_felt.anchor_curve), held at the ends
            c = f["felt_curve"]
            return f["felt_p"], float(2 ** np.interp(note, c["notes"], np.log2(c["f05"])))
        bass = float(np.interp(note, [21, 28, 30], [f["felt_bass_factor"]] * 2 + [1.0]))
        return f["felt_p"], bass * f["felt_f05_at_c4"] * 2 ** (f["felt_log2_slope_per_semitone"] * (note - 60))

    def pluck_beta(self, note):
        """Fitted pluck point (fit_pluck.py "beta_curve"), else the design curve."""
        c = (self.felt_fit or {}).get("pluck", {}).get("beta_curve")
        if c is None:
            return pluck_beta(note)
        return float(np.clip(np.interp(note, c["notes"], c["beta"]), 0.04, 0.5))

    def coupling(self):
        return (self.felt_fit or {}).get("coupling", DEFAULT_COUPLING)

    def polarizations(self, note):
        """{b1, b3, kappa, horiz, hdetune} at a note from the measured polarizations
        (fit_polarizations.py; the harp), or None: the coupling fit's split of b1′."""
        c = (self.felt_fit or {}).get("polarizations")
        if not c:
            return None
        at = lambda k: float(np.interp(note, c["notes"], c[k]))  # noqa: E731
        measured = c["horiz"][0] is not None
        return {"b1": at("b1"), "b3": at("b3"), "kappa": at("kappa"), "horiz": at("horiz") if measured else None,
                "hdetune": at("hdetune") if measured and c.get("hdetune") else None}

    def hammer_speeds(self):
        """Hammer speeds (m/s) for pp/mf/ff of the recordings."""
        if self.felt_fit is None:
            return {"pp": 0.4, "mf": 1.5, "ff": 5.0}
        return self.felt_fit["hammer_speeds"]


def n_strings(note, design="grand"):
    # grand: single strings A0–B0, bichords C1–G#2 (bichords fit the Salamander bass
    # better than single strings up to E1); upright (assumed): singles to D1, bichords to A#2
    if design in PLUCKED:
        return 1
    if design in TANGENT:
        return 2
    if design == "upright":
        return 1 if note <= 26 else 2 if note <= 46 else 3
    return 1 if note <= 23 else 2 if note <= 44 else 3


# the guitar (a classical guitar, 650 mm scale): the open strings 6 (E2) … 1 (E4), as
# datasets.GUITAR_OPEN; each note on the string first position puts it on
GUITAR_SCALE_M = 0.65
GUITAR_OPEN = {6: 40, 5: 45, 4: 50, 3: 55, 2: 59, 1: 64}
# normal-tension nylon and wound strings (typical sets: 54–73 N)
GUITAR_TENSION_N = {6: 72.0, 5: 71.0, 4: 69.0, 3: 56.0, 2: 54.0, 1: 70.0}


# the strumstick (a fretted dulcimer, ~0.6 m scale, assumed): open D3, A3, D4; each note on the
# string the recordings play it on (the bass string to G3, the middle to D4, the melody string above)
STRUMSTICK_SCALE_M = 0.60
STRUMSTICK_OPEN = {1: 50, 2: 57, 3: 62}
STRUMSTICK_TENSION_N = {1: 60.0, 2: 55.0, 3: 65.0}


def strumstick_string(note):
    return 1 if note < 57 else 2 if note < 64 else 3


def guitar_string(note):
    """The string first position plays a note on (datasets.guitar_string)."""
    return min((s for s, o in GUITAR_OPEN.items() if o <= note), default=6)


def guitar_fret(note):
    """The fret (semitones above the open string; below E2 negative: a longer string)."""
    return note - GUITAR_OPEN[guitar_string(note)]


def speaking_length(note, design="grand"):
    """Grand: ~1.9× per octave in the treble (C4 = 0.62 m), foreshortened below C3 (A0
    1.94 m). Upright (assumed): C4 = 0.57 m, foreshortened below E3 to 1.15 m at A0."""
    if design == "classical_guitar":
        return GUITAR_SCALE_M * 2 ** (-guitar_fret(note) / 12)
    if design == "stick_dulcimer":
        return STRUMSTICK_SCALE_M * 2 ** (-(note - STRUMSTICK_OPEN[strumstick_string(note)]) / 12)
    if design == "plucked_psaltery":
        # a plucked psaltery's steel strings (assumed): A♯4 0.32 m, ×2.15 per octave (F♯6 0.09 m)
        return 0.32 * 2.15 ** ((70 - note) / 12)
    if design == "bridge_zither":
        # 16 steel strings over movable bridges (assumed): B2 0.85 m to B5 0.25 m, ×1.5 per octave
        return 0.85 * 1.5 ** ((47 - note) / 12)
    if design == "harpsichord_4":
        # 4′: per key a string an octave above the 8′'s, about half its length (assumed)
        return 0.5 * speaking_length(note - 12, "harpsichord_8")
    if design == "lever_harp":
        # a 34-string lever harp, C2–A6 (assumed, Celtic-harp-like): A6 0.09 m, ×1.68 per
        # octave (C4 0.38 m, C2 1.06 m)
        return 0.09 * 1.68 ** ((93 - note) / 12)
    if design == "pedal_harp":
        # concert harp (assumed, pedal-harp-like): G7 0.075 m, ×1.6 per octave (C4 0.40 m,
        # C1 1.66 m); the engine's keys below C1 and above G7 follow the same curve
        return 0.075 * 1.6 ** ((103 - note) / 12)
    if design == "unfretted_clavichord":
        # C5 0.27 m, ~1.9× per octave, foreshortened below C3 (assumed: a German unfretted
        # clavichord's scale, brass strings; C2 1.25 m, D6 0.13 m)
        if note >= 48:
            return 0.27 * 1.9 ** ((72 - note) / 12)
        return 0.27 * 1.9 ** 2 * 1.3 ** ((48 - note) / 12)
    if design == "harpsichord_8":
        # C5 0.36 m, ~1.9× per octave, foreshortened below C3 (assumed, Flemish-like scale)
        if note >= 48:
            return 0.36 * 1.9 ** ((72 - note) / 12)
        return 0.36 * 1.9 ** 2 * 1.25 ** ((48 - note) / 12)
    if design == "upright":
        if note >= 52:
            return 0.57 * 1.9 ** ((60 - note) / 12)
        l52 = 0.57 * 1.9 ** (8 / 12)
        return l52 * (1.15 / l52) ** ((52 - note) / 31)
    if note >= 48:
        return 0.62 * 1.9 ** ((60 - note) / 12)
    return 0.62 * 1.9 * 1.25 ** ((48 - note) / 12)


def tension(note, design="grand"):
    if design == "classical_guitar":
        return GUITAR_TENSION_N[guitar_string(note)]
    if design == "stick_dulcimer":
        return STRUMSTICK_TENSION_N[strumstick_string(note)]
    if design == "plucked_psaltery":
        # music wire ~0.35 mm (assumed)
        return 70.0
    if design == "bridge_zither":
        # light steel strings (assumed: ~4–7 kg)
        return float(np.interp(note, [47, 83], [70.0, 45.0]))
    if design == "harpsichord_4":
        return 0.6 * tension(note - 12, "harpsichord_8")
    if design == "harpsichord_8":
        return float(np.interp(note, [21, 48, 108], [90.0, 70.0, 50.0]))
    if design == "unfretted_clavichord":
        # light brass strings, ~3–6 kg (assumed)
        return float(np.interp(note, [36, 60, 86], [55.0, 45.0, 35.0]))
    if design == "lever_harp":
        # lighter than the concert harp (assumed: a lever harp's strings at ~6–20 kg)
        return float(np.interp(note, [36, 60, 93], [200.0, 130.0, 60.0]))
    if design == "pedal_harp":
        # wound bass ~350 N to nylon treble ~60 N (assumed; a pedal harp's ~9 kN over 47 strings)
        return float(np.interp(note, [24, 48, 72, 103], [350.0, 220.0, 150.0, 60.0]))
    bass = 1300.0 if design == "upright" else 1600.0
    return float(np.interp(note, [21, 48, 108], [bass, 750.0, 700.0]))


def strike_pos(note, design="grand"):
    return float(np.interp(note, [21, 84, 108], [0.12, 0.12, 0.07]))


def pluck_beta(note):
    """Design pluck point re the speaking length: 12 % in the bass to 50 % in the treble
    (Välimäki et al. 2004), the pluck fit's fallback."""
    return float(np.interp(note, [29, 89], [0.12, 0.5]))


def hammer_mass(note, design="grand"):
    # upright hammers are lighter (assumed −15 %)
    return (0.85 if design == "upright" else 1.0) * 2.97e-3 * 2 ** ((60 - note) / 12 * 0.3)


def felt(note):
    """(exponent p, force at 0.5 mm in N).

    p and stiffness are not separately identifiable from the recordings (in the bass,
    p = 1.6 and p = 4.5 fit equally well), so p is fixed at the C4 value and only the
    stiffness was fitted per note (fit_hammer.py --p 2.3 --fine, Salamander, notes
    30–102). It rises smoothly, doubling about every 15 semitones; residuals are within
    a factor ~1.5 except around C6 (×0.5) and C#7 (×2).

    Below ~F#1 the fit wants a ~4× harder felt than the smooth curve. Plausibly the
    bass break (bass bridge, differently voiced hammers); a hypothesis from the fit only."""
    bass_factor = float(np.interp(note, [21, 28, 30], [3.0, 4.0, 1.0]))
    return 2.3, float(bass_factor * 6.69 * 2 ** (0.0658 * (note - 60)))


# Split of the measured loss b1' (energy-decay rates) into internal loss b1 = β1·b1' and
# bridge coupling κ = kf·b1', the unison mistuning and the horizontal excitation: fitted
# to the full per-partial energy decay curves (prompt sound and aftersound) of each
# instrument, fit_coupling.py --global, stored in the fit document's "coupling". Both
# instruments agree closely (trichord kf ≈ 1.0, bichord ≈ 1.7, β1 rising from ~0.15 in
# the bass to ~0.5 in the treble); the defaults below are their mean. (Phase 1 had
# β1 = 0.3 and trichord kf = 3.0, from the early decay slope only: the aftersound
# decayed ~2× too fast.)
DEFAULT_COUPLING = {"anchors": [24, 48, 72, 96], "beta1@24": 0.16, "beta1@48": 0.27,
                    "beta1@72": 0.35, "beta1@96": 0.48, "kf2": 1.7, "kf3": 1.0,
                    "detune": 1.0, "horiz": 0.1}
KF1 = 0.6   # single strings (A0–B0): not covered by the fit


def horiz_factor(cp, note):
    """The horizontal excitation's factor at a note from the coupling's horiz@<note> anchors
    (log-interpolated, held beyond the ends; fit_coupling.fit_horiz_levels), 1 without them."""
    at = sorted((int(k[6:]), v) for k, v in cp.items() if k.startswith("horiz@"))
    if not at:
        return 1.0
    return float(np.exp(np.interp(note, [a for a, _ in at], np.log([v for _, v in at]))))


# the bridge's mobility along the keyboard (fit_coupling.fit_mobility; design 2026-10-20): a factor
# per key on the admittance, its boost low-passed at MOBILITY_FC_HZ (4th order): the bass's partials
# 2–4 fall 10–25 dB in their first 0.5 s on a real board, where its bridge moves freely
MOBILITY_FC_HZ = 500.0
MOBILITY_BOUNDS = (0.25, 8.0)


def beta3_factor(cp, note):
    """The strings' high-frequency loss factor at a note from the coupling's beta3@<note> anchors
    (log-interpolated, held beyond the ends; fit_coupling.fit_high_loss), 1 without them."""
    at = sorted((int(k[6:]), v) for k, v in cp.items() if k.startswith("beta3@"))
    if not at:
        return 1.0
    return float(np.exp(np.interp(note, [a for a, _ in at], np.log([v for _, v in at]))))


def mobility_anchors(cp):
    """The coupling's mobility@<note> anchors, [(note, factor)] by note ([] without them)."""
    return sorted((int(k[9:]), float(v)) for k, v in cp.items() if k.startswith("mobility@"))


def mobility_factor(cp, note, f):
    """The bridge's mobility at a note and frequencies f (Hz): 1 + (M − 1)·L(f), M log-interpolated
    between the anchors (held beyond the ends), L(f) = 1 / (1 + (f / MOBILITY_FC_HZ)⁴); 1 without
    anchors. The engine's physics.rs `Mobility::factor` is the same."""
    at = mobility_anchors(cp)
    if not at:
        return 1.0
    m = np.exp(np.interp(note, [a for a, _ in at], np.log([v for _, v in at])))
    out = 1.0 + (m - 1.0) / (1.0 + (np.asarray(f, float) / MOBILITY_FC_HZ) ** 4)
    return float(out) if np.ndim(out) == 0 else out


class MobileAdmittance:
    """A note's bridge admittance: the board's times the mobility factor (a class, not a closure:
    note params cross process pools)."""

    def __init__(self, admittance, cp, note):
        self.admittance, self.note = admittance, note
        self.anchors = {f"mobility@{a}": v for a, v in mobility_anchors(cp)}

    def __call__(self, f):
        return self.admittance(f) * mobility_factor(self.anchors, self.note, f)


def core_at(table, note):
    """(depth m, F_c N) of the felt's core at a key from a part's [felt_core] table (the export's; the fits never see it), set by ear
    (design 2026-10-20-felt-core): notes, f05 (N at 0.5 mm past the depth), depth_mm; linear between
    its keys, held beyond; (0, 0) without one."""
    if not table.get("notes"):
        return 0.0, 0.0
    ns, f, d = table["notes"], table.get("f05", []), table.get("depth_mm", [])
    if len(f) != len(ns) or len(d) != len(ns) or list(ns) != sorted(set(ns)) or min(f) < 0 or min(d) < 0:
        raise ValueError(f"[felt_core]: notes rising, as many f05 and depth_mm, none negative: {table}")
    return (float(np.interp(note, table["notes"], table["depth_mm"])) * 1e-3,
            float(np.interp(note, table["notes"], table["f05"])))


def with_core(p, table):
    """The note's parameters with the core of the [felt_core] table at its key (the fits never see it:
    the board's refit with it in the renders tilted Grand A's levels by ±4 dB across the keys)."""
    depth, f_c = core_at(table, p.note)
    return replace(p, core_K=f_c / 5e-4 ** p.felt_p, core_depth=depth) if f_c > 0 else p


FELT_P_RANGE = (1.0, 5.0)
FELT_STIFFNESS_DB = (-12.0, 12.0)


def with_felt(p, table):
    """The note's parameters with the felt of a part's [felt] table (the export's; the fits never see
    it), set by ear: `p`, the exponent, the force at 0.5 mm kept, its growth past it changed (a lower
    exponent: less brightening from a soft strike to a hard one); `stiffness_db`, the felt's K scaled
    by 10^(dB/20), as the Hammer hardness control does (a core keeps its own f05). Unchanged without
    one."""
    if not table:
        return p
    e, db = table.get("p", p.felt_p), table.get("stiffness_db", 0.0)
    ok = (set(table) <= {"p", "stiffness_db"} and all(isinstance(v, (int, float)) for v in (e, db))
          and FELT_P_RANGE[0] <= e <= FELT_P_RANGE[1] and FELT_STIFFNESS_DB[0] <= db <= FELT_STIFFNESS_DB[1])
    if not ok:
        raise ValueError(f"[felt]: p (the exponent, {FELT_P_RANGE[0]} … {FELT_P_RANGE[1]}) and stiffness_db "
                         f"({FELT_STIFFNESS_DB[0]:+.0f} … {FELT_STIFFNESS_DB[1]:+.0f} dB): {table}")
    f05 = p.felt_K * 5e-4 ** p.felt_p * 10 ** (db / 20)
    return replace(p, felt_p=float(e), felt_K=f05 / 5e-4 ** float(e))


UNISON_WIDTH = (0.05, 2.0)
UNISON_TREBLE = (0.2, 12.0)


def unison_treble(rows, note):
    """The detune's factor at a key from a [unison] treble table ([[key, factor], …], keys rising):
    log-interpolated, held beyond its ends."""
    keys, f = zip(*rows)
    return float(np.exp(np.interp(note, keys, np.log(f))))


def with_unison(p, table):
    """The note's parameters with the unison of a part's [unison] table (the export's; the fits never
    see it): `width`, the fitted detune of its strings scaled, as the plugin's Unison width scales it
    (set by ear); `treble`, a factor on it along the keys ([[key, factor], …], log-interpolated, held
    beyond its ends): the fitted detune is measured below the top octaves, where the recordings' beats
    grow (design log step 183). Unchanged without one."""
    if not table:
        return p
    w = table.get("width", 1.0)
    rows = table.get("treble", [[p.note, 1.0]])
    ok = (table and set(table) <= {"width", "treble"} and isinstance(w, (int, float))
          and UNISON_WIDTH[0] <= w <= UNISON_WIDTH[1] and isinstance(rows, list) and rows
          and all(isinstance(r, list) and len(r) == 2 and all(isinstance(v, (int, float)) for v in r)
                  and UNISON_TREBLE[0] <= r[1] <= UNISON_TREBLE[1] for r in rows)
          and all(a[0] < b[0] for a, b in zip(rows, rows[1:])))
    if not ok:
        raise ValueError(f"[unison]: width, the fitted detune's share, {UNISON_WIDTH[0]} … {UNISON_WIDTH[1]}; "
                         f"treble, [[key, factor], …] keys rising, factors {UNISON_TREBLE[0]} … {UNISON_TREBLE[1]}: {table}")
    k = float(w) * unison_treble(rows, p.note)
    return replace(p, detune_cents=tuple(k * c for c in p.detune_cents))


def note_params(profile: Profile, note: int, **overrides) -> M.NoteParams:
    d = profile.design
    S = n_strings(note, d)
    L, T = speaking_length(note, d), tension(note, d)
    f0 = profile.f0(note)
    mu = T / (2 * L * f0) ** 2
    b1p, b3 = profile.loss(note)
    p_exp, f05 = profile.felt(note)
    cp = profile.coupling()
    beta1 = float(np.exp(np.interp(note, cp["anchors"], np.log([cp[f"beta1@{a}"] for a in cp["anchors"]]))))
    kf = {1: cp.get("kf1", KF1), 2: cp["kf2"], 3: cp["kf3"]}[S]
    detune = tuple(cp["detune"] * c for c in {1: (0.0,), 2: (-0.5, 0.5), 3: (-0.6, 0.0, 0.7)}[S])
    params = M.NoteParams(
        f0=f0, B=profile.B(note), L=L, T=T, string_mass=mu * L, n_strings=S,
        detune_cents=detune, b1=beta1 * b1p, b3=b3 * beta3_factor(cp, note), loss_q=cp.get("loss_q", 2.0), kappa=kf * b1p, horiz_gain=cp["horiz"] * horiz_factor(cp, note),
        kappa_reactive=cp.get("reactive", 0.3), horiz_detune_cents=cp.get("hdetune", 0.3),
        hammer_mass=hammer_mass(note, d), felt_p=p_exp, felt_K=f05 / 5e-4 ** p_exp,
        hammer_balance=(1.0, 0.92, 1.06)[:S], strike_pos=strike_pos(note, d),
        admittance=(None if profile.board is None else MobileAdmittance(profile.board.admittance, cp, note)
                    if mobility_anchors(cp) else profile.board.admittance),
        horiz_admittance=profile.board.admittance_measured if profile.board is not None else None,
        horiz_share=profile.h_share,
        scatter_cents=(profile.felt_fit or {}).get("string_scatter_cents", 0.0), note=note)
    pol = profile.polarizations(note)
    if pol is not None:   # measured per note: the slow polarization's loss, the fast one's coupling
        params = replace(params, b1=pol["b1"], b3=pol["b3"], kappa=pol["kappa"],
                         horiz_gain=params.horiz_gain if pol["horiz"] is None else pol["horiz"],
                         horiz_detune_cents=params.horiz_detune_cents if pol["hdetune"] is None else pol["hdetune"])
    if d in PLUCKED:
        vel = (profile.felt_fit or {}).get("pluck", {}).get("velocity", {})
        harp = "fingered" in profile.traits and "picked" not in profile.traits
        params = replace(params, excitation="pluck", pluck_beta=profile.pluck_beta(note),
                         pluck_fq=(K_Q_HARP if harp else K_Q) * f0, pluck_amp=PLUCK_AMP_HARP if "fingered" in profile.traits else PLUCK_AMP,
                         pluck_gain_db_per_step=vel.get("gain_db_per_step", 3.0),
                         pluck_fq_oct_per_step=vel.get("fq_oct_per_step", 0.5))
    if d in TANGENT:
        vel = (profile.felt_fit or {}).get("pluck", {}).get("velocity", {})
        scatter = (profile.felt_fit or {}).get("string_scatter_cents", TANGENT_SCATTER_CENTS)
        params = replace(params, excitation="tangent", pluck_fq=K_Q * f0, pluck_amp=TANGENT_AMP, scatter_cents=scatter,
                         pluck_gain_db_per_step=vel.get("gain_db_per_step", 3.0),
                         pluck_fq_oct_per_step=vel.get("fq_oct_per_step", 0.5))
    return replace(params, **overrides)
