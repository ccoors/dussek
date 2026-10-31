import numpy as np

import board

MODES = [{"f": 220.0, "q": 40.0, "g": 6.0}, {"f": 105.0, "q": 25.0, "g": 3.0}]


def test_measured_modes_join_the_random_ones_unchanged():
    a = board.Board(seed=3, density=1 / 40)
    b = board.Board(seed=3, density=1 / 40, modes=MODES)
    n = len(a.f)
    assert len(b.f) == n + len(MODES)
    for k in ("f", "sigma", "phi"):
        assert np.array_equal(getattr(b, k)[:n], getattr(a, k))
    assert np.array_equal(b.r[:, :n], a.r) and np.array_equal(b.r_h[:, :n], a.r_h)
    assert np.allclose(b.f[n:], [220.0, 105.0])
    assert np.allclose(b.sigma[n:], [np.pi * 220.0 / 40.0, np.pi * 105.0 / 25.0])
    assert np.allclose(b.phi[n:] ** 2, [6.0, 3.0])


def test_a_measured_mode_adds_its_peak_to_the_admittance():
    # Re Y at the mode's peak: φ²/(2σ), normalized by density/4
    a = board.Board(seed=3, density=1 / 40)
    b = board.Board(seed=3, density=1 / 40, modes=MODES[:1])
    sigma = np.pi * 220.0 / 40.0
    peak = 6.0 / (2 * sigma) / ((1 / 40) / 4)
    d = (b.admittance(np.array([220.0])) - a.admittance(np.array([220.0]))).real[0]
    assert abs(d - peak) < 0.02 * peak
    # far from it, nearly nothing
    far = (b.admittance(np.array([2000.0])) - a.admittance(np.array([2000.0]))).real[0]
    assert abs(far) < 0.01 * peak


def test_measured_modes_radiate_at_unit_weight():
    # the engine radiates every board mode through a unit-variance shape (mic.rs mode_shape),
    # not through φ: a strong measured mode's φ acts on the coupling alone
    b = board.Board(seed=3, density=1 / 40, modes=MODES)
    n = len(b.f) - len(MODES)
    assert np.array_equal(b.rad[:n], b.phi[:n])
    assert np.all(np.abs(b.rad[n:]) == 1.0)


def test_boards_with_other_modes_do_not_share_a_table():
    f = np.array([220.0])
    a = board.Board(seed=3, density=1 / 40, modes=MODES)
    b = board.Board(seed=3, density=1 / 40, modes=[{"f": 220.0, "q": 40.0, "g": 1.0}])
    assert not np.allclose(a.admittance(f), b.admittance(f))


def test_partials_within_a_measured_modes_band_leave_the_loss_fit():
    import scale
    modes = [{"f": 220.0, "q": 40.0, "g": 6.0}]
    # bandwidth f/q = 5.5 Hz; BAND_WIDTHS of them either side
    w = scale.BAND_WIDTHS * 220.0 / 40.0
    assert scale.in_mode_band(220.0, modes) and scale.in_mode_band(220.0 + 0.9 * w, modes)
    assert not scale.in_mode_band(220.0 + 1.1 * w, modes) and not scale.in_mode_band(440.0, modes)
    assert not scale.in_mode_band(220.0, [])


def test_a_measured_mode_damps_the_horizontal_polarization_by_its_share():
    # the guitar's G3 falls at 72 dB/s through its first 20 dB: no slow polarization is left
    # above −20 dB, so the top's mode takes the horizontal one too (its share, fitted)
    import string_model as M
    b = board.Board(seed=3, density=1 / 40, modes=[{"f": 220.0, "q": 30.0, "g": 3.0}])
    base = dict(f0=220.0, B=1e-4, L=0.65, T=56.0, string_mass=56.0 / (2 * 0.65 * 220.0) ** 2 * 0.65, n_strings=1,
                detune_cents=(0.0,), kappa=1.0, horiz_gain=0.3, admittance=b.admittance,
                excitation="pluck", pluck_beta=0.2, pluck_fq=880.0, pluck_amp=3e-3)
    p0 = M.NoteParams(**base)
    p1 = M.NoteParams(**base, horiz_admittance=b.admittance_measured, horiz_share=1.0)
    fm0, fm1 = M.free_modes(p0, 1.0), M.free_modes(p1, 1.0)
    i = list(fm0["k"]).index(1)
    # [vertical, horizontal] for one string: the horizontal rate rises by κ·Re Y_measured(f1)
    sh0, sh1 = -fm0["lam"][i, 1].real, -fm1["lam"][i, 1].real
    extra = b.admittance_measured(fm0["w"][0, i] / (2 * np.pi)).real
    assert extra > 1.0 and abs((sh1 - sh0) - extra) < 0.05 * extra
    assert abs(fm0["lam"][i, 0] - fm1["lam"][i, 0]) < 1e-9     # the vertical alike
