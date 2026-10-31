import numpy as np

import fit_body_modes as FB


def lorentz_residuals(rng, f, peaks):
    """ln(measured/model) with a Lorentzian excess per (f0, width, height) and noise."""
    r = 0.15 * rng.standard_normal(len(f))
    for f0, w, h in peaks:
        r += np.log1p(h / (1 + ((f - f0) / w) ** 2))
    return r


def test_a_band_where_every_note_decays_faster_is_found():
    rng = np.random.default_rng(1)
    f = 80 * 2 ** rng.uniform(0, 3, 400)
    r = lorentz_residuals(rng, f, [(220.0, 8.0, 4.0)])
    bands = FB.detect_bands(f, r, np.ones_like(f), rng=np.random.default_rng(2))
    assert len(bands) == 1 and abs(bands[0]["f"] - 220.0) < 10.0 and bands[0]["p"] < 0.01


def test_noise_alone_finds_no_band():
    rng = np.random.default_rng(3)
    f = 80 * 2 ** rng.uniform(0, 3, 400)
    r = 0.25 * rng.standard_normal(len(f))
    assert FB.detect_bands(f, r, np.ones_like(f), rng=np.random.default_rng(4)) == []


def test_two_bands_come_strongest_first():
    rng = np.random.default_rng(5)
    f = 60 * 2 ** rng.uniform(0, 3, 600)
    r = lorentz_residuals(rng, f, [(105.0, 6.0, 1.0), (225.0, 8.0, 4.0)])
    bands = FB.detect_bands(f, r, np.ones_like(f), rng=np.random.default_rng(6))
    assert [round(b["f"] / 10) for b in bands[:2]] == [22, 10] or [round(b["f"] / 10) for b in bands[:2]] == [23, 10]


def test_edc_slope_of_a_single_exponential_is_its_rate():
    # amplitude e^(−σt): 8.686·σ dB/s
    sigma = 3.0
    t = np.arange(0, 6, 0.01)
    p = np.exp(-2 * sigma * t)
    assert abs(FB.edc_slope_db_s(t, p) - 8.686 * sigma) < 0.05
