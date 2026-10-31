import os

import numpy as np
import pytest

import fit_buff as FB


def decaying(f0, sigmas, sr=44100, dur=3.0):
    t = np.arange(int(dur * sr)) / sr
    x = sum(np.exp(-s * t) * np.sin(2 * np.pi * (k + 1) * f0 * t) / (k + 1) for k, s in enumerate(sigmas))
    return x, sr


def test_track_recovers_decay_rates():
    sig = np.array([0.8, 1.5, 2.0, 3.0, 4.5, 6.0])
    x, sr = decaying(220.0, sig)
    s, n = FB.track(x, sr, 220.0, k_max=6)
    assert np.all(n >= FB.MIN_FRAMES)
    assert np.allclose(s, sig, rtol=0.05), s


def test_track_leaves_partials_above_the_limit_out():
    x, sr = decaying(4000.0, [2.0, 2.0, 2.0, 2.0])
    s, n = FB.track(x, sr, 4000.0, k_max=4)
    assert np.isfinite(s[:3]).all() and np.isnan(s[3]) and n[3] == 0    # 16 kHz > F_MAX_HZ


def test_fit_pad_recovers_position_and_strength():
    rng = np.random.default_rng(3)
    k = np.arange(1, 25)
    for p, a, g in ((0.08, 0.0, 3.0), (0.15, 0.0, 1.2), (0.1, 1.5, 3.0)):
        d = a + g * np.sin(k * np.pi * p) ** 2 + rng.normal(0, 0.1 * g, len(k))
        pf, af, gf, err, err0 = FB.fit_pad(k, d, np.ones(len(k)))
        assert abs(pf - p) < 0.01, (p, pf)
        assert abs(gf - g) < 0.1 * g, (g, gf)
        assert abs(af - a) < 0.15, (a, af)
        assert FB.comb_gain_db(err, err0) > FB.MIN_GAIN_DB


def test_flat_damping_is_the_flat_term():
    # the lossy end: every partial the same extra loss, no rise
    k = np.arange(1, 25)
    p, a, g, err, err0 = FB.fit_pad(k, np.full(len(k), 1.8), np.ones(len(k)))
    assert a == pytest.approx(1.8, abs=1e-6) and g == pytest.approx(0.0, abs=1e-6)
    assert FB.comb_gain_db(err, err0) < FB.MIN_GAIN_DB


def test_flat_and_strength_stay_non_negative():
    k = np.arange(1, 25)
    d = 3.0 * np.sin(k * np.pi * 0.1) ** 2 - 0.5     # a negative offset: the flat term clips at 0
    p, a, g, err, err0 = FB.fit_pad(k, d, np.ones(len(k)))
    assert a >= 0.0 and g >= 0.0


def test_fit_pad_ignores_untracked_partials():
    k = np.arange(1, 25)
    d = 2.0 * np.sin(k * np.pi * 0.1) ** 2
    d[[3, 7]] = np.nan
    w = np.ones(len(k))
    w[10] = 0.0
    d[10] = 1e6                                   # weight 0: must not matter
    p, a, g, err, err0 = FB.fit_pad(k, d, w)
    assert abs(p - 0.1) < 0.005 and abs(g - 2.0) < 0.05 and a < 0.05


def test_smooth_rise_is_not_a_comb():
    k = np.arange(1, 25)
    d = 50.0 * np.sin(k * np.pi * FB.P0) ** 2      # exactly the comb-free reference
    p, a, g, err, err0 = FB.fit_pad(k, d, np.ones(len(k)))
    assert FB.comb_gain_db(err, err0) < FB.MIN_GAIN_DB


def per_key(ps, gain_db=6.0, flat=0.0):
    k = np.arange(1, 25)
    out = {}
    for n, p in ps.items():
        d = flat + 2.0 * np.sin(k * np.pi * p) ** 2
        err = 0.1
        out[n] = {"k": k, "dsig": d, "w": np.ones(len(k)), "p": p, "g": 2.0,
                  "err": err, "err0": err * 10 ** (gain_db / 20)}
    return out


def test_curves_cover_the_keyboard_and_hold_beyond_the_data():
    pk = per_key({n: 0.06 + 0.001 * (n - 34) for n in range(34, 77, 2)}, flat=0.7)
    pos = FB.pos_curve(pk)
    flat, gain = FB.strength_curves(pk, pos)
    assert len(pos) == 88 and len(gain) == 88 and len(flat) == 88
    assert all(FB.P0 <= p <= 0.5 for p in pos) and all(g >= 0 for g in gain) and all(a >= 0 for a in flat)
    assert flat[60 - 21] == pytest.approx(0.7, abs=0.05)
    assert flat[0] == pytest.approx(flat[34 - 21], abs=1e-9)
    assert pos[0] == pytest.approx(pos[34 - 21], abs=1e-9)     # held below A♯1…
    assert pos[-1] == pytest.approx(pos[76 - 21], abs=1e-9)    # … and above E5
    assert gain[60 - 21] == pytest.approx(2.0, rel=0.05)


def test_pos_curve_drops_outliers_and_defaults_to_p0():
    ps = {n: 0.08 for n in range(34, 77, 2)}
    ps[50] = 0.4                                    # an alias far off its neighbors
    pos = FB.pos_curve(per_key(ps))
    assert abs(pos[50 - 21] - 0.08) < 0.01
    assert FB.pos_curve(per_key(ps, gain_db=0.0)) == [FB.P0] * 88


def test_t20():
    sr = 44100
    t = np.arange(3 * sr) / sr
    x = np.exp(-t / 0.5) * np.sin(2 * np.pi * 300 * t)   # 20 dB in 0.5·ln(10) s
    assert FB.t20(x, sr) == pytest.approx(0.5 * np.log(10), abs=0.02)


ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "data", "vcsl-keys", "Harpsichord, English")


@pytest.mark.skipif(not os.path.isdir(ROOT), reason="VCSL English harpsichord not downloaded")
def test_english_pairs():
    pairs = FB.pairs("harpsichord_buff")
    assert len(pairs) >= 25
    assert all(set(v) == {"normal", "buff"} for v in pairs.values())


def test_pad_stays_near_the_nut():
    # a buff batten sits a few cm from the nut: an alias further along is not a pad position
    k = np.arange(1, 13)
    d = 5.0 * np.sin(k * np.pi * 0.4) ** 2
    p, a, g, err, err0 = FB.fit_pad(k, d, np.ones(len(k)))
    assert FB.P0 <= p <= FB.P_MAX
    pos = FB.pos_curve(per_key({n: 0.4 for n in range(60, 89, 2)}))
    assert max(pos) <= FB.P_MAX
