"""The early drop of the low partials (proto/early_drop.py): a partial's level at 0.05–0.15 s
minus at 0.8–1.2 s, the recordings against the engine (the bass's prompt sound, design
2026-10-20-bridge-mobility)."""
import os

import numpy as np
import pytest

import early_drop as ED

REPO = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")


def test_an_exponential_decay_drops_by_its_rate_over_the_windows():
    sr = 48000
    t = np.arange(int(2.0 * sr)) / sr
    x = np.concatenate([np.zeros(int(0.2 * sr)), np.sin(2 * np.pi * 100.0 * t) * 10 ** (-20.0 * t / 20)])
    d = ED.drops(x, sr, 100.0, 0.0)
    # each window's mean energy: 20 dB/s over the 0.9 s between their centers, less the late
    # window's mean over its 0.4 s lying above its center value more than the early 0.1 s's
    a = 20.0 / 10 * np.log(10)                 # the energy's decay rate (1/s)
    mean = lambda t0, t1: (np.exp(-a * t0) - np.exp(-a * t1)) / (a * (t1 - t0))
    assert d[0] == pytest.approx(10 * np.log10(mean(*ED.EARLY) / mean(*ED.LATE)), abs=0.2)   # 17.45 dB
    assert all(np.isnan(v) or v < 60 for v in d[1:])


def test_the_windows_are_timed_from_the_onset():
    # 0 dB until 0.10 s, −20 dB after: the early window (0.05–0.15 s) half each, 17 dB over the late
    sr = 48000
    t = np.arange(int(2.0 * sr)) / sr
    x = np.concatenate([np.zeros(int(0.2 * sr)), np.sin(2 * np.pi * 100.0 * t) * np.where(t < 0.1, 1.0, 0.1)])
    assert ED.drops(x, sr, 100.0, 0.0)[0] == pytest.approx(10 * np.log10(0.5 + 0.5 * 0.01) + 20, abs=0.5)


def test_the_layers_play_at_the_instruments_own_velocities():
    assert ED.layer_velocity(os.path.join(REPO, "instruments", "grand_a"), "mf") == 60
    assert ED.layer_velocity(os.path.join(REPO, "instruments", "grand_a"), "ff") == 124
