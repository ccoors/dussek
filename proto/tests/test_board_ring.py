"""The board's low modes' loss factor ([board] low_eta, the export's; the fits never see it): each
random mode rings as with it where that is longer than the fitted overlap gives (design log step 184:
a grand's board glued into its rim rings ~0.5 s, the overlap alone ~0.12 s)."""
import numpy as np
import pytest

import board as Bd
import export
import scale


def test_the_low_modes_ring_as_with_the_loss_factor_and_the_rest_stay():
    plain, ringing = Bd.Board(seed=3), Bd.Board(seed=3, eta_low=0.04)
    assert np.array_equal(plain.f, ringing.f) and np.array_equal(plain.phi, ringing.phi)
    low = plain.f < Bd.OVERLAP / (plain.density * 0.04)
    assert ringing.sigma[low] == pytest.approx(np.pi * plain.f[low] * 0.04)
    assert np.all(ringing.sigma[low] < plain.sigma[low])
    assert np.array_equal(ringing.sigma[~low], plain.sigma[~low])


def test_the_admittance_tables_are_kept_apart():
    a, b = Bd.Board(seed=5), Bd.Board(seed=5, eta_low=0.04)
    f = np.array([60.0, 120.0, 240.0])
    assert not np.allclose(a.admittance(f), b.admittance(f))
    assert np.allclose(a.admittance(f), Bd.Board(seed=5).admittance(f))


def test_the_project_value_is_checked_and_reaches_the_profile():
    assert export.board_low_eta(None) is None and export.board_low_eta({"trim_db": []}) is None
    assert export.board_low_eta({"low_eta": 0.04}) == 0.04
    for bad in (0.0, 0.5, "0.04"):
        with pytest.raises(ValueError):
            export.board_low_eta({"low_eta": bad})
    with pytest.raises(ValueError):
        export.board_trim_db(np.ones(2), {"trim_db": [], "low_etas": 0.04})
    prof = scale.Profile("grand_b", eta_low=0.04)
    assert prof.board.sigma.min() < scale.Profile("grand_b").board.sigma.min()


def test_a_ringing_board_radiates_its_low_modes_in_phase():
    plain, ringing = Bd.Board(seed=3), Bd.Board(seed=3, eta_low=0.04)
    low, high = ringing.f < Bd.COHERENT_HZ[0], ringing.f > Bd.COHERENT_HZ[1]
    for r in (ringing.r, ringing.r_h):
        assert np.allclose(r[1, low], r[0, low])
    assert np.array_equal(ringing.r[:, high], plain.r[:, high])
    assert np.array_equal(ringing.r[0], plain.r[0])
