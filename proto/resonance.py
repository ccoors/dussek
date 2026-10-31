"""Sympathetic resonance: every string whose damper is lifted (sustain pedal, a held
key, the top strings without dampers) is driven by the bridge motion that the sounding
strings cause.

The unison coupling of string_model, ȧ_j = (jω − σ)·a_j − κ·Σ_i a_i, generalizes to all
strings on the bridge: in bridge-force units each partial m of any string follows
    ḟ_m = (jω_m − σ_m)·f_m − κ_m · Σ_i f_i,
driven by the total bridge force with its own coupling κ_m (the same κ the coupling fit
determined; for strings i ≠ m the reciprocal coupling κ_mi·b_m/b_i reduces to κ_m). With
the real bridge force F = Re Σ f_i as input the drive is −2κ_m·F, scaled by TRANSFER.
The sympathetic strings' feedback on each other and on the played strings is neglected.

Unisons are merged into one resonator per partial (mean frequency, S·κ: their in-phase
mode is the one the bridge drives). Partials up to F_MAX. Each resonator is a complex
one-pole filter, run segment by segment with the damper state of its note: driven
while the note's strings are free and not sounding by their own render, otherwise
decaying (with the damper's rate when it rests).
"""
from multiprocessing import Pool

import numpy as np
from scipy.signal import lfilter

import damper
import scale

F_MAX = 5000.0
# Transfer factor of the sympathetic drive: the equation above uses each string's own
# (driving-point) coupling, but the sounding strings drive the bridge elsewhere, and the
# transfer between two bridge points is weaker; the neglected back-reaction (the energy
# the free string takes up is not removed from the sounding one) also overestimates the
# response when frequencies nearly coincide. With 1.0, a silently held C3 rang about as
# loud as a struck C4. Not identifiable from the recordings (no pedal or silent-key
# samples): a perceptual choice, −20 dB.
TRANSFER = 0.1
_g = {}


def note_modes(profile, note, sr=48000):
    """(freqs Hz, σ free (1/s), κ complex (1/s)) of the partials of one note, unisons merged."""
    p = scale.note_params(profile, note)
    k, w, sig = scale.M.mode_table(p, sr)
    f = w.mean(axis=0) / (2 * np.pi)
    sel = f < F_MAX
    f, sig = f[sel], sig.mean(axis=0)[sel]
    y = p.admittance(f).real if p.admittance is not None else np.ones(len(f))
    kappa = p.n_strings * p.kappa * (y + 1j * p.kappa_reactive)
    return f, sig + kappa.real / p.n_strings, kappa


def _init(dataset, drive):
    _g["profile"] = scale.Profile(dataset)
    _g["drive"] = drive


def _note_response(args):
    """Bridge force of one note's sympathetic partials, for its damper schedule
    [(start sample, damped, active)] (active = False while the note sounds by itself)."""
    note, segments, sr = args
    f, sig, kappa = note_modes(_g["profile"], note, sr)
    if not len(f):
        return np.zeros(len(_g["drive"]))
    sig_d = damper.sigma(note, f, _g["profile"].dataset)
    x = _g["drive"]
    out = np.zeros(len(x))
    state = np.zeros(len(f), complex)
    bounds = segments + [(len(x), True, False)]
    for (s0, damped, active), (s1, _, _) in zip(bounds[:-1], bounds[1:]):
        if s1 <= s0:
            continue
        pole = np.exp((2j * np.pi * f - sig - (sig_d if damped else 0)) / sr)
        # while the note sounds by its own render, the vibration already in these strings
        # just decays (no more drive; the strike adds to it there). Zeroing it made a step,
        # i.e. a broadband click at the attack of every struck note.
        # driven only while the strings are free (no damper) and not sounding by
        # themselves; damped strings just decay (they would pick up next to nothing)
        seg = (x[s0:s1] if active and not damped else np.zeros(s1 - s0)).astype(complex)
        for m in range(len(f)):
            y, _ = lfilter([-2 * TRANSFER * kappa[m] / sr], [1, -pole[m]], seg, zi=[state[m] * pole[m]])
            out[s0:s1] += y.real
            state[m] = y[-1]
    return out


def sympathetic(profile, drive, schedules, sr=48000):
    """Sum of the sympathetic strings' bridge force. drive: total bridge force of the
    played strings; schedules: {note: [(start sample, damped, active), …]}."""
    jobs = [(n, seg, sr) for n, seg in schedules.items()
            if any((not d) and a for _, d, a in seg) or not damper.has_damper(n)]
    with Pool(initializer=_init, initargs=(profile.dataset, drive)) as pool:
        parts = pool.map(_note_response, jobs)
    return np.sum(parts, axis=0) if parts else np.zeros(len(drive))
