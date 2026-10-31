import numpy as np

import datasets as D
import scale


def test_loudest_dynamic():
    assert [D.loudest(p) for p in ("grand_a", "grand_b", "upright_a", "grand_f", "upright_b")] == ["ff"] * 5
    assert D.loudest("harpsichord") == "mf"


def test_design_curves_are_plausible_over_the_keyboard():
    for n in range(21, 109):
        assert scale.n_strings(n, "harpsichord_8") == 1
        L, T = scale.speaking_length(n, "harpsichord_8"), scale.tension(n, "harpsichord_8")
        assert 0.05 < L < 2.5 and 30 < T < 120
        assert 0.04 <= scale.pluck_beta(n) <= 0.5
    assert abs(scale.speaking_length(72, "harpsichord_8") - 0.36) < 1e-12


def test_harpsichord_note_params_pluck():
    prof = scale.Profile("harpsichord")
    for n in (21, 29, 60, 89, 108):
        p = scale.note_params(prof, n)
        assert p.excitation == "pluck" and p.n_strings == 1
        assert abs(p.pluck_fq - scale.K_Q * p.f0) < 1e-9 and p.pluck_amp == scale.PLUCK_AMP
        assert all(np.isfinite([p.f0, p.B, p.L, p.T, p.string_mass, p.b1, p.b3, p.kappa]))


def test_piano_note_params_unchanged():
    p = scale.note_params(scale.Profile("grand_a"), 60)
    assert p.excitation == "hammer" and p.n_strings == 3
