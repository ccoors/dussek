"""The coupling fit's third stage (fit_coupling.fit_mobility): the bridge's mobility at C1 … C6 and
the horizontal levels, scored on the EDCs, the late gap and the early gap (the prompt sound: the
bass's partials 2–4 fall 10–25 dB in 0.5 s in the recordings; design 2026-10-20-bridge-mobility)."""
import numpy as np
import pytest

import fit_coupling as FC
import scale
import string_model as M


def single_string(**kw):
    return M.NoteParams(f0=196.0, B=2e-4, L=0.5, T=190.0, string_mass=190.0 / (2 * 0.5 * 196.0) ** 2 * 0.5,
                        n_strings=1, detune_cents=(0.0,), b1=1.0, b3=0.0, kappa=1.0, horiz_gain=0.5,
                        excitation="pluck", pluck_beta=0.47, pluck_fq=4 * 196.0, pluck_amp=3e-3, note=55, **kw)


def test_the_early_difference_is_the_models_minus_the_recordings_median():
    p = single_string()
    t = np.arange(0.05, 3.0, 0.01)
    # (n, freq, t, EDC, end, depth, depth times, window, early excess, late level)
    rows = [(n, 196.0 * n, t, -10.0 * t, 3.0, None, None, 0.05, r, -20.0) for n, r in ((1, 2.0), (2, 5.0), (3, -1.0))]
    rows.append((7, 196.0 * 7, t, -10.0 * t, 3.0, None, None, 0.05, 4.0, -20.0))     # beyond EARLY_PARTIALS
    model = FC.model_excess(p, 2.0, rows)
    want = np.median([m - r[8] for m, r in zip(model[:3], rows[:3])])
    assert FC.early_diff(p, 2.0, rows) == pytest.approx(want, abs=1e-9)
    assert FC.early_diff(p, 2.0, [r[:8] + (None, -20.0) for r in rows]) is None


def test_the_smoothness_penalty_starts_beyond_a_factor_of_two():
    assert FC.smooth_penalty([1.0, 1.0, 1.0]) == 0.0
    assert FC.smooth_penalty([1.0, 2.0, 1.0]) == pytest.approx(0.0, abs=1e-12)
    assert FC.smooth_penalty([1.0, 4.0, 1.0]) == pytest.approx(2 * FC.W_SMOOTH * np.log(2))


def test_the_fit_reads_its_anchors_through_g():
    prof = scale.Profile("grand_a")
    g = {**prof.coupling(), "mobility@36": 4.0}
    x = FC.global_to_note(g, 36, prof.design)
    p = FC.note_params(prof, 36, x, g)
    f = np.array([131.0])
    assert np.allclose(p.admittance(f), prof.board.admittance(f) * scale.mobility_factor(g, 36, f))
    # g replaces the profile's anchors rather than stacking on them
    p1 = FC.note_params(prof, 36, x, {**g, "mobility@36": 1.0})
    assert np.allclose(p1.admittance(f), prof.board.admittance(f))


def test_only_opted_in_hammer_pianos_fit_the_mobility(monkeypatch):
    real = FC.scale.strings
    monkeypatch.setattr(FC.scale, "strings", lambda ds: {**real(ds), "mobility": True})
    assert FC.fits_mobility("grand_a") and not FC.fits_mobility("harp")
    monkeypatch.setattr(FC.scale, "strings", lambda ds: {k: v for k, v in real(ds).items() if k != "mobility"})
    assert not FC.fits_mobility("grand_a")
