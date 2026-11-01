import numpy as np
import os

import pytest

import datasets as D
import release_noise as RN


@pytest.mark.skipif(not os.path.exists(os.path.join(D.root("harpsichord"), "Harpsichord, Unk.sfz")), reason="no data")
def test_vcsl_release_level_includes_both_sfz_volumes():
    regions = D.parse_sfz(os.path.join(D.root("harpsichord"), "Harpsichord, Unk.sfz"))
    attack = {int(r["pitch_keycenter"]): float(r.get("volume", 0)) for r in regions
              if r.get("trigger", "attack") == "attack" and "sample" in r}
    keys = {k["note"]: k for k in RN.vcsl_keys("harpsichord")}
    import project as P
    notes = {s["note"]: s for s in P.load_cache("harpsichord", "analysis")["samples"]}
    n = max((n for n in attack if n in notes), key=lambda n: abs(attack[n]))   # the largest note volume
    expected = keys[n]["peak_dbfs"] - (notes[n]["peak_dbfs"] + attack[n])
    assert abs(keys[n]["ratio_db"]["mf"] - expected) < 1e-9, (n, attack[n])


def test_vcsl_release_keys_use_the_key_played():
    keys = {k["note"]: k for k in RN.vcsl_keys("flemish4")}
    assert set(keys) == set(range(21, 109))
    # a recorded key's ratio is finite and comes from its own release sample
    assert all(np.isfinite(k["ratio_db"]["mf"]) for k in keys.values())


def test_a_projects_level_moves_every_keys_ratio():
    keys = [{"note": 60, "ratio_db": {"pp": -30.0, "mf": -25.0}}, {"note": 61, "ratio_db": {"mf": -24.0}}]
    out = RN.with_level(keys, -12.0)
    assert [k["ratio_db"] for k in out] == [{"pp": -42.0, "mf": -37.0}, {"mf": -36.0}]
    assert keys[0]["ratio_db"]["mf"] == -25.0, "the input is left as it was"
    assert RN.with_level(keys, 0.0) == keys


def test_a_perspective_moves_the_bands_by_its_curve():
    bands = np.array([200.0, 400.0, 800.0, 1600.0, 6400.0, 20000.0])
    db = np.zeros(len(bands))
    curve = [[400.0, 0.0], [1600.0, -8.0], [6400.0, -12.0]]
    out = RN.with_perspective(db, bands, curve)
    # 0 below the curve, interpolated in log frequency, held beyond its last point
    assert np.allclose(out, [0.0, 0.0, -4.0, -8.0, -12.0, -12.0])
    # still re the strongest band
    assert out.max() == 0.0
    assert np.allclose(RN.with_perspective(db, bands, []), db)


def test_a_projects_impacts_replace_a_takes_by_turns():
    import pedal_noise as PN
    takes = [{"name": "u1", "impacts": [[0.04, -6.0, 0.01], [0.15, 0.0, 0.006]], "band_db": [0.0]},
             {"name": "u2", "impacts": [[0.08, -1.0, 0.004]], "band_db": [0.0]}]
    pattern = [[[0.04, 0.0, 0.011], [0.14, -8.0, 0.007]]]
    out = PN.with_impacts(takes, pattern)
    assert [t["impacts"] for t in out] == pattern * 2          # one pattern for every take
    assert out[0]["band_db"] == [0.0] and takes[0]["impacts"][0][1] == -6.0   # the rest kept, input untouched
    assert PN.with_impacts(takes, []) == takes


def test_silent_releases_are_leveled_against_the_notes_at_one_gain(tmp_path):
    # keys released without their note (split_session.py --silent): A0 and C1, each a thump then the
    # room's floor; a key between takes the nearest, its level the release's peak re its note's at
    # each dynamic (one noise whatever the strike: the key's return is the player's, not the hammer's)
    import soundfile as sf
    sr = 48000
    rng = np.random.default_rng(1)
    (tmp_path / "release_silent").mkdir()
    for name, db in (("A0_1", -40.0), ("C1_1", -46.0), ("C1_2", -30.0)):
        n = int(0.3 * sr)
        thump = 10 ** (db / 20) * rng.standard_normal(n) * np.exp(-np.arange(n) / (0.03 * sr))
        thump /= np.abs(thump).max() / 10 ** (db / 20)
        floor = 10 ** (-80 / 20) * rng.standard_normal(int(1.5 * sr))
        sf.write(tmp_path / "release_silent" / f"{name}.wav", np.r_[thump, floor], sr, subtype="FLOAT")
    peaks = {"pp": {21: -30.0, 24: -32.0}, "mf": {21: -10.0, 24: -12.0}}
    keys = {k["note"]: k for k in RN.silent_keys(str(tmp_path), "release_silent", peaks)}
    assert set(keys) == set(range(21, 109))
    assert abs(keys[21]["ratio_db"]["mf"] - (-40.0 + 10.0)) < 0.01
    assert abs(keys[21]["ratio_db"]["pp"] - (-40.0 + 30.0)) < 0.01
    assert abs(keys[22]["ratio_db"]["mf"] - (-40.0 + 10.0)) < 0.01          # nearest: A0
    assert abs(keys[108]["ratio_db"]["mf"] - (-46.0 + 12.0)) < 0.01         # C1's first take
    # the floor after the thump is no part of the noise's spectrum
    x, _ = sf.read(tmp_path / "release_silent" / "A0_1.wav")
    assert len(RN.trimmed(x, sr)) < 0.35 * sr
