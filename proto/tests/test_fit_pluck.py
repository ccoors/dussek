import numpy as np

import fit_pluck as FP
import string_model as M


def synthetic(beta, seed=1):
    k = np.arange(1, 41)
    a = M.pluck_amplitudes(k, 200.0 * k, beta, 1e12, 1.0)
    rng = np.random.default_rng(seed)
    lv = 20 * np.log10(np.abs(a) + 0.02 * np.abs(a).max() / k ** 2) - 3 * np.log2(k) + rng.normal(0, 1.5, len(k))
    return k, lv


def test_recovers_the_pluck_point():
    for beta in (0.13, 0.2, 0.31):
        k, lv = synthetic(beta)
        b, err, err0 = FP.fit_beta(k, lv, np.ones(len(k)))
        assert abs(b - beta) < 0.01, (beta, b)
        assert err0 - err > 3


def test_curve_covers_the_keyboard_in_range():
    per_note = {n: {"beta": 0.12 + (n - 29) * 0.005, "err_db": 2.0, "err0_db": 6.0} for n in range(29, 90, 3)}
    c = FP.beta_curve(per_note)
    assert c["notes"] == list(range(21, 109))
    assert all(0.04 <= b <= 0.5 for b in c["beta"])


def test_curve_ignores_double_beta_outliers():
    trend = lambda n: 0.09 + (n - 29) * 0.003  # noqa: E731
    per_note = {n: {"beta": trend(n), "err_db": 2.0, "err0_db": 6.0} for n in range(29, 82)}
    for n in (29, 39, 47, 74):                 # comb ambiguity: 2β fits nearly as well
        per_note[n]["beta"] = 2 * trend(n)
    c = FP.beta_curve(per_note)
    for n in (29, 40, 60, 80):
        assert abs(c["beta"][n - 21] - trend(n)) < 0.015, (n, c["beta"][n - 21], trend(n))



# the recorded harpsichord's per-note fits (β, gain dB): 2β-ambiguous notes cluster at 39–49
RECORDED = {29: (0.226, 2.61), 30: (0.448, 1.33), 31: (0.088, 4.59), 33: (0.088, 5.09), 34: (0.090, 5.49), 35: (0.090, 3.75), 36: (0.092, 3.93), 38: (0.096, 1.60), 39: (0.224, 3.07), 41: (0.098, 2.57), 42: (0.098, 4.96), 43: (0.198, 6.46), 44: (0.102, 3.55), 45: (0.104, 3.30), 46: (0.312, 2.07), 47: (0.212, 3.68), 48: (0.430, 1.70), 49: (0.220, 4.52), 50: (0.112, 5.88), 51: (0.116, 4.46), 52: (0.118, 4.10), 53: (0.120, 2.69), 54: (0.124, 4.00), 55: (0.126, 3.38), 56: (0.130, 1.58), 57: (0.132, 3.27), 58: (0.138, 4.82), 60: (0.150, 5.79), 61: (0.154, 5.37), 62: (0.160, 2.64), 63: (0.166, 3.36), 64: (0.170, 3.05), 65: (0.118, 7.42), 66: (0.184, 3.56), 67: (0.190, 3.30), 68: (0.196, 1.66), 69: (0.202, 2.46), 70: (0.210, 4.19), 71: (0.216, 3.27), 72: (0.222, 4.98), 74: (0.470, 3.79), 75: (0.138, 3.10), 76: (0.248, 5.55), 77: (0.442, 2.26), 78: (0.260, 5.81), 79: (0.264, 4.11), 81: (0.388, 3.95)}


def test_curve_survives_clustered_double_beta_notes():
    per_note = {n: {"beta": b, "err_db": 0.0, "err0_db": g} for n, (b, g) in RECORDED.items()}
    c = FP.beta_curve(per_note)
    # the single-β notes there measure 0.096…0.12 (44: 0.102, 45: 0.104)
    for n in range(40, 49):
        assert 0.09 <= c["beta"][n - 21] <= 0.125, (n, c["beta"][n - 21])
