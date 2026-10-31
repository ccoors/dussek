import os

import numpy as np
import pytest

import string_model as M

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = os.path.join(HERE, "..", "out", "pluck_baseline")


def test_amplitudes_closed_form_and_notch():
    k = np.arange(1, 11)
    a = M.pluck_amplitudes(k, 220.0 * k, 0.2, 1e12, 1e-3)
    expected = 2e-3 / (np.pi ** 2 * 0.16) * np.sin(k * np.pi * 0.2) / k ** 2
    assert np.allclose(a, expected, rtol=1e-9)
    assert abs(a[4]) < 1e-12 * abs(a[0]) and abs(a[9]) < 1e-12 * abs(a[0])   # k = 5, 10


def test_amplitudes_parity_values():
    # the same numbers as engine physics::tests::pluck_amplitudes_match_prototype
    k = np.arange(1, 9)
    a = M.pluck_amplitudes(k, 220.0 * k, 0.2, 4400.0, 1.5e-3, 0.5)
    ref = [1.325979119379e-03, 5.349556777366e-04, 2.367215228236e-04, 8.179817851556e-05,
           1.082384929774e-20, -3.574542459254e-05, -4.204097785880e-05, -3.180193549273e-05]
    assert np.allclose(a, ref, rtol=1e-9, atol=1e-22)


def test_tangent_amplitudes_parity_values():
    # the tangent (the pluck's limit β → 0): 2·amp/(πk) through the lowpass, no comb; the same
    # numbers as engine physics::tests::tangent_amplitudes_match_prototype
    k = np.arange(1, 9)
    a = M.pluck_amplitudes(k, 220.0 * k, 0.2, 4400.0, 1.5e-3, 0.5, tangent=True)
    ref = [1.133934203080e-03, 5.654722884298e-04, 3.753379956579e-04, 2.798045640777e-04,
           2.221318046052e-04, 1.834099449534e-04, 1.555372366619e-04, 1.344643228438e-04]
    assert np.allclose(a, ref, rtol=1e-9, atol=1e-22)
    # the limit of the triangle: a pluck at β = 1e-4 has the same low partials
    tri = M.pluck_amplitudes(k, 220.0 * k, 1e-4, 4400.0, 1.5e-3, 0.5)
    assert np.allclose(a, tri, rtol=1e-4)


def test_velocity_raises_level_and_brightness():
    k = np.array([1, 40])
    f = 100.0 * k
    lo = M.pluck_amplitudes(k, f, 0.3, 2000.0, 1e-3, -1.0)
    hi = M.pluck_amplitudes(k, f, 0.3, 2000.0, 1e-3, 1.0)
    assert 5.5 < 20 * np.log10(hi[0] / lo[0]) < 6.5
    assert abs(hi[1] / hi[0]) > abs(lo[1] / lo[0])


def _level(y, f, sr=48000):
    t = np.arange(len(y)) / sr
    win = np.hanning(len(y))
    return 20 * np.log10(abs(np.sum(y * win * np.exp(-2j * np.pi * f * t))) + 1e-30)


def test_pluck_render_has_the_pluck_point_notch():
    p = M.NoteParams(f0=220.0, B=1e-6, n_strings=1, detune_cents=(0.0,), hammer_balance=(1.0,),
                     excitation="pluck", pluck_beta=0.2, pluck_fq=1e9)
    y, info = M.render(p, 1.0, duration=0.5)
    assert info["contact_ms"] == 0 and np.all(np.isfinite(y)) and np.abs(y).max() > 0
    lv = [_level(y, 220.0 * k * np.sqrt(1 + 1e-6 * k * k)) for k in (4, 5, 6)]
    assert lv[1] < min(lv[0], lv[2]) - 20


@pytest.mark.skipif(not os.path.exists(os.path.join(BASE, "proto_c4.npy")), reason="no baseline (Task 1)")
def test_hammer_render_unchanged():
    import scale
    p = scale.Profile("grand_a")
    y, _ = M.render(scale.note_params(p, 60), 3.0, duration=1.0)
    # equal up to rounding (the free decay is summed per block since, string_model.free_decay)
    b = np.load(os.path.join(BASE, "proto_c4.npy"))
    assert np.max(np.abs(y - b)) < 1e-11 * np.max(np.abs(b))


def test_pluck_render_is_the_closed_form_from_m_2():
    # engine tests/pluck.rs::harpsichord_pluck_matches_the_closed_form checks the same sum
    p = M.NoteParams(f0=220.0, B=1e-5, n_strings=1, detune_cents=(0.0,), hammer_balance=(1.0,),
                     excitation="pluck", pluck_beta=0.23, pluck_fq=3000.0, horiz_gain=0.0)
    y, _ = M.render(p, 1.0, duration=0.01)
    fm = M.free_modes(p, 1.0)
    k = fm["k"]
    a = M.pluck_amplitudes(k, fm["w"].mean(axis=0) / (2 * np.pi), 0.23, 3000.0, p.pluck_amp)
    bridge = p.T * (k * np.pi / p.L) * (-1.0) ** k
    lam = fm["lam"][:, 0]
    n = np.arange(len(y))
    expected = (bridge[:, None] * a[:, None] * np.exp(lam[:, None] * (n[None, :] + 2) / 48000)).real.sum(axis=0)
    assert np.allclose(y, expected, rtol=1e-9, atol=1e-12 * np.abs(expected).max())
