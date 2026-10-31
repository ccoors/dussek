"""The bass above 1 kHz measured (proto/bass_check.py): the attack's band levels, how long the
bands last, the energy between the partials (design 2026-10-20-bass-high-band)."""
import numpy as np
import pytest

import bass_check as BC

SR = 48000


def test_a_flat_spectrum_has_its_bands_at_their_mean():
    x = np.random.default_rng(1).standard_normal(SR)
    assert all(abs(v) < 1.0 for v in BC.attack_bands(x, SR, 0))


def test_an_exponential_band_drops_by_its_rate():
    # one partial at 5 kHz decaying 20 dB/s: its band at 0.3/0.6/1.5 s re 0.02–0.07 s
    t = np.arange(2 * SR) / SR
    x = np.sin(2 * np.pi * 5000.0 * t) * 10 ** (-20.0 * t / 20)
    left = BC.band_left(x, SR, 0, np.array([5000.0]))
    for got, at in zip(left[(4500, 6500)], (0.3, 0.6, 1.5)):
        assert got == pytest.approx(-(at - 0.045) * 20.0, abs=1.0)


def test_pure_partials_leave_nothing_between():
    t = np.arange(2 * SR) / SR
    fk = 55.0 * np.arange(1, 141)
    x = sum(np.sin(2 * np.pi * f * t + k) / k for k, f in enumerate(fk, 1)) * np.exp(-t)
    # the estimator's floor (Blackman over 0.5 s, ±6 Hz kept out) lies near −56 dB
    assert all(v < -50 for v in BC.between(x, SR, 0, fk))


def test_the_growth_is_the_slope():
    assert BC.growth([0, 10, 20], [-30, -20, -10]) == pytest.approx(1.0)


def test_attack_noise_reads_the_hiss():
    # a 3 kHz tone decaying at 8 /s over white noise: the noise per band on attack_bands' scale
    # (re the attack's 200–500 Hz) is the noise's own attack level, read late in the file
    rng = np.random.default_rng(7)
    t = np.arange(10 * SR) / SR
    noise = 1e-3 * rng.standard_normal(len(t))
    x = np.sin(2 * np.pi * 3000.0 * t) * np.exp(-8.0 * t) + noise
    got = BC.attack_noise(x, SR, 0)
    assert len(got) == len(BC.ATTACK_BANDS)
    want = BC.attack_bands(noise, SR, 0)
    assert all(abs(g - w) < 1.5 for g, w in zip(got, want)), (got, want)
    # the tone's band lies far over it, the others at it
    over = np.array(BC.attack_bands(x, SR, 0)) - got
    assert over[1] > 30 and abs(over[3]) < 1.5
