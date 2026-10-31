"""The per-key correction fitted in bands above the tracked partials (soundboard.band_centers,
band_levels, identify with [strings] band_correction): the analysis tracks 60 partials, which end
at 2.2–4.5 kHz in A0–C2, where the recordings' attack lay 12–24 dB over the engine's at 4–16 kHz
(design 2026-10-20-bass-high-band)."""
import numpy as np

import soundboard as SB


def test_band_centers_start_above_the_last_partial():
    c = SB.band_centers(4477.0)
    assert c[0] >= 1.12 * 4477.0 and c[-1] <= 16000
    assert np.allclose(np.diff(np.log2(c)), 1 / 3)


def test_no_bands_above_the_top():
    assert len(SB.band_centers(15000.0)) == 0


def test_band_levels_of_white_noise_are_flat():
    x = np.random.default_rng(2).standard_normal(48000)
    lv = SB.band_levels(x, 48000, 0, SB.band_centers(1000.0))
    # the attack's 0.13 s: each band of white noise scatters by about ±0.5 dB
    assert lv.max() - lv.min() < 1.5


def test_band_rows_reach_the_correction():
    # the correction takes the rows wherever they are: +10 dB at 6–10 kHz on notes 30–40 only
    rng = np.random.default_rng(3)
    notes = rng.integers(21, 109, 3000).astype(float)
    f = 2 ** rng.uniform(np.log2(100), np.log2(15000), 3000)
    resid = np.where((notes >= 30) & (notes <= 40) & (f >= 6000) & (f <= 10000), 10.0, 0.0)
    corr, _ = SB.identify_correction(f, notes, resid)
    C = np.array(corr["C_db"])
    i8k = int(np.argmin(np.abs(SB.GRID_HZ - 8000)))
    at = lambda n: np.interp(n, corr["notes"], C[:, i8k])  # noqa: E731
    assert at(33) > 5 and abs(at(60)) < 2


def test_a_band_at_the_noise_floor_is_left_out():
    # Grand D's pp and mp lie at their noise floor above 6 kHz in the bass (0–3 dB over it): rows
    # there are the noise against a nearly silent model, +50 … +75 dB, and the correction (one for
    # every dynamic) carried them to f, 12–28 dB over the recording at 11–16 kHz
    rng = np.random.default_rng(4)
    sr = 48000
    t = np.arange(4 * sr) / sr
    x = np.sin(2 * np.pi * 5000 * t) * np.exp(-3 * t) + 1e-4 * rng.standard_normal(len(t))
    c = SB.band_centers(2000.0)
    lv, clear = SB.clear_band_levels(x, sr, 0, c)
    assert np.allclose(lv, SB.band_levels(x, sr, 0, c))
    i5k, i12k = int(np.argmin(np.abs(c - 5000))), int(np.argmin(np.abs(c - 12500)))
    assert clear[i5k] and not clear[i12k]
