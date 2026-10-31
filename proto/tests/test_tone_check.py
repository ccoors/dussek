"""The tone per register measured (proto/tone_check.py): octave bands re the fundamental's, the lowest
fundamentals re partials 2–4, the medians per register (design log steps 143, 147, 168)."""
import numpy as np
import pytest

import tone_check as TC

SR = 48000


def _tone(f0, amps, n=SR + SR // 2):
    t = np.arange(n) / SR
    x = np.asarray(sum(a * np.sin(2 * np.pi * k * f0 * t) for k, a in enumerate(amps, 1)), float)
    x[:100] = 0.0           # the onset at sample 100
    return x


def test_bands_are_re_the_fundamentals_band():
    # C5 (523 Hz): its band is 500 Hz; partial 8 (4.2 kHz) 20 dB under the fundamental
    f, p, on = TC.power(_tone(523.25, [1.0] + [0.0] * 6 + [0.1]), SR)
    b = TC.bands(f, p, 523.25)
    assert on == 100 and min(b) == 1000.0 and 500.0 not in b     # its own band left out (0 by definition)
    assert b[4000.0] == pytest.approx(-20.0, abs=0.5)


def test_both_channels_count_by_their_power():
    # the right channel silent: the power averaged is 3 dB under the left's, every band alike
    x = _tone(261.6, [1.0, 0.5, 0.25])
    f, p, _ = TC.power(np.column_stack([x, 0 * x]), SR)
    f1, p1, _ = TC.power(x, SR)
    assert TC.bands(f, p, 261.6) == pytest.approx(TC.bands(f1, p1, 261.6), abs=1e-6)
    assert p.max() == pytest.approx(p1.max() / 2)


def test_the_lowest_fundamental_resolves_a0():
    # A0: partials 27.5 Hz apart; the fundamental 10 dB over each of partials 2–4 → 10 − 10·log10(3)
    f0 = 27.5
    f, p, _ = TC.power(_tone(f0, [1.0, 10 ** -0.5, 10 ** -0.5, 10 ** -0.5]), SR)
    assert TC.fundamental(f, p, f0 * np.arange(1, 5)) == pytest.approx(10 - 10 * np.log10(3), abs=0.5)


def test_summary_takes_medians_per_register_and_dynamic():
    rows = [{"note": 74, "dynamic": "mf", "bands": {4000.0: v}} for v in (2.0, 4.0, 9.0)]
    rows += [{"note": 60, "dynamic": "mf", "bands": {4000.0: 1.0}}]
    rows += [{"note": 22, "dynamic": "ff", "bands": {63.0: 0.0}, "fund": 12.0},
             {"note": 24, "dynamic": "ff", "bands": {63.0: 0.0}, "fund": 8.0}]
    s = TC.summary(rows)
    assert s["registers"]["mf"]["treble"] == {"n": 3, "bands": {4000.0: 4.0}}
    assert s["registers"]["mf"]["middle"] == {"n": 1, "bands": {}}       # one take: no median
    assert s["lowest"]["A0–G♯1"] == {"median": 10.0, "keys": [[22, "ff", 12.0], [24, "ff", 8.0]]}
    assert "A1–G♯2" not in s["lowest"]
