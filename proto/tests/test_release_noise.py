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
