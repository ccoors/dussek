import numpy as np

import analyze as A
import datasets as D
import scale


def test_flemish_loaders_find_both_registers():
    root = D.root("flemish8")
    eight = D.items("flemish8")
    four = D.items("flemish4")
    assert len(eight) == 28 and len(four) == 26
    assert {s["note"] for s in eight} == {s["key"] for s in eight}
    assert all(s["note"] == s["key"] + 12 for s in four)
    assert all("/High/" in s["relpath"] for s in four)


def test_other_vcsl_sets_unchanged():
    assert len(D.items("harpsichord")) == 61
    assert len(D.items("upright_a")) == 90


def test_analysis_keeps_key_and_sfz_volume():
    it = D.items("flemish4")[0]
    r = A.analyze(it)
    assert r["key"] == it["key"] and r["sfz_volume_db"] == it["sfz_volume_db"]


def test_4ft_design_is_half_the_8ft_an_octave_up():
    for n in range(33, 121):
        assert scale.n_strings(n, "harpsichord_4") == 1
        assert abs(scale.speaking_length(n, "harpsichord_4") - 0.5 * scale.speaking_length(n - 12, "harpsichord_8")) < 1e-12
        assert 20 < scale.tension(n, "harpsichord_4") < 60
    assert scale.design_of("flemish4") == "harpsichord_4" and scale.design_of("flemish8") == "harpsichord_8"
    assert scale.strings("flemish4")["board_seed"] == scale.strings("flemish8")["board_seed"]
