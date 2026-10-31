"""The coupling fit's late level (fit_coupling.late_level, model_late, late_gap) and the horizontal
level per octave (horiz@ anchors): with one horizontal level the fitted aftersound fell 2–15 dB
under the recordings from C5 up and lay over them in the low bass (design log step 127)."""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, os.path.join(HERE, "..", "..", "analysis"))
import analyze as A  # noqa: E402
import fit_coupling as FC  # noqa: E402
import scale  # noqa: E402


def test_a_renders_late_level_is_its_closed_form():
    # the recordings' late level is read from envelopes, the model's in closed form: the same
    # measure on both sides, or the fit chases the difference between them
    prof = scale.Profile("grand_d")
    g = prof.felt_fit["coupling"]
    for note in (60, 84):
        p = FC.note_params(prof, note, FC.global_to_note(g, note, prof.design))
        v = prof.hammer_speeds()["ff"]
        # the polarizations apart: they radiate by different paths and add in power (as the EDCs)
        y, _ = scale.M.render(p, v, sr=48000, duration=3.0, split=True)
        freqs = [n * p.f0 * np.sqrt(1 + p.B * n * n) for n in (1, 2, 3)]
        t, env_v = A.partial_envelopes(y[0][:, None], 48000, freqs, p.f0)
        _, env_h = A.partial_envelopes(y[1][:, None], 48000, freqs, p.f0)
        measured = [FC.late_level(t, ev ** 2 + eh ** 2) for ev, eh in zip(env_v, env_h)]
        model = FC.model_late(p, v, [1, 2, 3])
        assert np.allclose(measured, model, atol=1.5), (note, measured, model)


def test_a_late_level_needs_the_window():
    t = np.arange(0, 2.0, 0.01)
    assert FC.late_level(t, np.ones_like(t)) is None
    t = np.arange(0, 3.0, 0.01)
    assert abs(FC.late_level(t, np.ones_like(t))) < 1e-9
    # re the peak over the first PEAK_WINDOW_S, not its mean
    e = np.where(t < 0.02, 100.0, 1.0)
    assert np.isclose(FC.late_level(t, e), -20.0)
    # a tail sunk into the floor (zero energy after the subtraction) is the floor's level, not None
    assert FC.late_level(t, np.where(t < 0.5, 1.0, 0.0)) == FC.LATE_FLOOR_DB


def test_the_horizontal_level_follows_its_anchors():
    g = {"beta1@24": 0.2, "beta1@48": 0.2, "beta1@72": 0.2, "beta1@96": 0.2, "kf2": 1.0, "kf3": 1.0,
         "detune": 1.0, "horiz": 0.1, "reactive": 1.0, "hdetune": 0.1}
    horiz = lambda g, n: float(np.exp(FC.global_to_note(g, n)[4]))  # noqa: E731
    assert np.isclose(horiz(g, 30), 0.1) and np.isclose(horiz(g, 90), 0.1)   # no anchors: one level
    g.update({f"horiz@{a}": 1.0 for a in FC.HORIZ_ANCHORS}, **{"horiz@84": 4.0})
    assert np.isclose(horiz(g, 84), 0.4) and np.isclose(horiz(g, 60), 0.1)
    assert np.isclose(horiz(g, 72), 0.2)                # log-interpolated between anchors
    assert np.isclose(horiz(g, 108), 0.4)               # C6's level held beyond the last anchor
    assert FC.HORIZ_ANCHORS[-1] == 84                   # C7 alone, its octave's only fitted note


def test_the_profile_reads_the_anchors():
    prof = scale.Profile("grand_d")
    cp = prof.felt_fit["coupling"]
    for k in [k for k in cp if k.startswith("horiz@")]:     # the committed fit's own
        del cp[k]
    base = scale.note_params(prof, 84).horiz_gain
    cp.update({f"horiz@{a}": 1.0 for a in FC.HORIZ_ANCHORS}, **{"horiz@84": 3.0})
    assert np.isclose(scale.note_params(prof, 84).horiz_gain, 3 * base)
    assert np.isclose(scale.note_params(prof, 60).horiz_gain, base)


def test_a_notes_late_difference_takes_every_take():
    # rows (n, late level) of each recorded take; the median over all of them against the model
    prof = scale.Profile("grand_d")
    g = prof.felt_fit["coupling"]
    p = FC.note_params(prof, 72, FC.global_to_note(g, 72, prof.design))
    v = prof.hammer_speeds()["ff"]
    model = FC.model_late(p, v, [1, 2, 3])
    takes = [[(n, m - 3.0) for n, m in zip((1, 2, 3), model)] for _ in range(3)] + [[(1, None), (2, model[1] + 30.0)]]
    assert np.isclose(FC.late_diff(p, v, takes), 3.0)
    assert FC.late_diff(p, v, [[(1, None)]]) is None


def test_the_late_gap_is_scored_per_octave_by_its_median():
    # one beating note does not steer an octave: the median of its notes does
    diffs = {24: 1.0, 27: 1.0, 30: 1.0, 33: 30.0, 72: -4.0, 75: -4.0, 78: -4.0, 81: -4.0}
    assert np.isclose(FC.late_gap(diffs), (1.0 + 4.0) / 2)
    assert FC.late_gap({}) == 0.0
    # a sparse set's octave of one note counts (Upright B: a take every octave)
    assert np.isclose(FC.late_gap({60: None, 63: 2.0}), 2.0)
    # C7 up does not: the tails in their noise and the undamped strings' ring
    assert np.isclose(FC.late_gap({72: -4.0, 75: -4.0, 96: 20.0}), 4.0)
