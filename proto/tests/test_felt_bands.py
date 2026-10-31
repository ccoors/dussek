"""The attack's bands above 2 kHz (felt_bands), the felt fit's band target: each band's level change
between adjacent dynamics minus the fundamental's, where the recording's bands lie clear of its
noise (design 2026-10-20-felt-core)."""
import numpy as np
import pytest

import felt_bands as FB


def test_a_tone_lands_in_its_band():
    t = np.arange(48000) / 48000
    lv = FB.band_levels(np.sin(2 * np.pi * 5000 * t), 48000, 0)
    assert int(np.argmax(lv)) == 1 and lv[1] - np.delete(lv, 1).max() >= 40


def test_band_levels_do_not_depend_on_the_rate():
    rng = np.random.default_rng(4)
    # the same density per Hz: white noise of variance ∝ the rate
    a = FB.band_levels(rng.standard_normal(44100) * np.sqrt(44100), 44100, 0)
    b = FB.band_levels(rng.standard_normal(48000) * np.sqrt(48000), 48000, 0)
    assert np.all(np.abs(a - b) < 1.0)


def test_noise_levels_read_the_tail():
    rng = np.random.default_rng(5)
    sr = 48000
    t = np.arange(3 * sr) / sr
    noise = 1e-3 * rng.standard_normal(len(t))
    x = np.sin(2 * np.pi * 3000 * t) * np.exp(-8 * t) + noise
    assert np.all(np.abs(FB.noise_levels(x, sr) - FB.band_levels(noise, sr, 0)) < 1.5)


def _rec(bands, noise, l1):
    return {"bands": np.array(bands, float), "noise": np.array(noise, float), "l1": l1}


def _model(bands, l1):
    return {"bands": np.array(bands, float), "l1": l1}


def test_band_error_ignores_noisy_bands():
    clean = [-40.0] * 5
    noisy = [-75.0] * 5
    floor = [-80.0] * 5
    rec = {"pp": _rec(noisy, floor, -30), "mf": _rec(clean, floor, -20), "ff": _rec([-30.0] * 5, floor, -15)}
    model = {"pp": _model([-99.0] * 5, -30), "mf": _model(clean, -20), "ff": _model([-30.0] * 5, -15)}
    # pp's bands lie 5 dB over its noise: the pair mf − pp says nothing, ff − mf matches
    assert FB.band_error(model, rec, [("mf", "pp"), ("ff", "mf")]) == pytest.approx(0.0)
    assert FB.band_error(model, rec, [("mf", "pp")]) is None


def test_band_error_is_zero_for_the_same_change():
    floor = [-90.0] * 5
    rec = {"mf": _rec([-50, -60, -65, -70, -75], floor, -20), "ff": _rec([-30, -35, -40, -50, -60], floor, -12)}
    # a constant filter (+7 dB) and another level (+3 dB) cancel in the change between dynamics
    model = {d: _model(r["bands"] + 7, r["l1"] + 3) for d, r in rec.items()}
    assert FB.band_error(model, rec, [("ff", "mf")]) == pytest.approx(0.0, abs=1e-12)
    model["ff"]["bands"] = model["ff"]["bands"] - 4
    assert FB.band_error(model, rec, [("ff", "mf")]) == pytest.approx(4.0)


def test_recorded_bands_of_grand_a():
    # the recording's C2 brightens at 4–6 kHz from mf to ff by ~26 dB re its 200–500 Hz
    # (bass_check's attack table): far more than the fundamental grows
    try:
        rec = FB.recorded_bands("grand_a", 36)
    except (FileNotFoundError, KeyError) as e:
        pytest.skip(f"no data: {e}")
    assert {"pp", "mf", "ff"} <= set(rec)
    d = (rec["ff"]["bands"][1] - rec["mf"]["bands"][1]) - (rec["ff"]["l1"] - rec["mf"]["l1"])
    assert d >= 15, d


def test_noise_levels_ignore_a_fade_to_silence():
    # the sets fade their files to digital silence: the noise is the hiss before the fade
    # (Grand A's C2: −143 dB at 4–6 kHz for most of 18–22 s, −167 in the last 0.3 s)
    rng = np.random.default_rng(6)
    sr = 48000
    t = np.arange(10 * sr) / sr
    noise = 1e-3 * rng.standard_normal(len(t))
    x = np.sin(2 * np.pi * 3000 * t) * np.exp(-8 * t) + noise
    x[-sr:] *= np.linspace(1, 0, sr) ** 4
    assert np.all(np.abs(FB.noise_levels(x, sr) - FB.band_levels(noise, sr, 0)) < 1.5)
