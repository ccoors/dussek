"""Even levels (proto/even_levels.py): a piano voiced to one level curve along the keyboard, each
key's correction the trend minus its level, plus a seeded scatter (design 2026-10-21-even-loudness)."""
import numpy as np
import pytest

import even_levels as EL

SPEEDS = [0.4, 1.5, 5.0]
LN = [float(np.log(s)) for s in (0.078, 0.156, 0.3125, 0.625, 1.25, 2.5, 5.0, 10.0)]


def _doc(ff):
    """A level doc: each key's ff level `ff`, flat rows (every speed as loud as ff)."""
    return {"key_ff_db": list(map(float, ff)), "speed_db": {"keys": list(range(21, 109)),
            "speed": [float(np.exp(x)) for x in LN], "db": [[0.0] * 8 for _ in range(88)]}}


def test_a_smooth_keyboard_needs_nothing():
    rows = EL.corrections(_doc(np.linspace(-20, -40, 88)), SPEEDS, {})
    assert np.allclose(rows, 0.0, atol=1e-9)


def test_a_notch_is_filled():
    ff = np.linspace(-20, -40, 88)
    ff[106 - 21] -= 6.0
    rows = np.array(EL.corrections(_doc(ff), SPEEDS, {}))
    assert rows[106 - 21] == pytest.approx([6.0] * 3, abs=0.5)
    assert np.all(np.abs(rows[[105 - 21, 107 - 21]]) < 0.5)


def test_the_shape_is_kept():
    ff = np.linspace(10, -10, 88) + np.random.default_rng(0).normal(0.0, 2.0, 88)
    rows = np.array(EL.corrections(_doc(ff), SPEEDS, {}))
    out = ff + rows[:, 2]
    assert out[0] - out[-1] == pytest.approx(20.0, abs=1.0)


def test_the_scatter_is_seeded_and_shared_by_the_dynamics():
    doc = _doc(np.linspace(-20, -40, 88))
    a = np.array(EL.corrections(doc, SPEEDS, {"scatter_db": 1.0, "seed": 3}))
    b = np.array(EL.corrections(doc, SPEEDS, {"scatter_db": 1.0, "seed": 3}))
    assert np.array_equal(a, b)
    assert np.allclose(a[:, 0], a[:, 1]) and np.allclose(a[:, 1], a[:, 2])
    assert 0.7 < a[:, 0].std() < 1.3


@pytest.mark.parametrize("table, key", [({"windw_keys": 9}, "windw_keys"), ({"window_keys": 4}, "window_keys"),
                                        ({"scatter_db": -1}, "scatter_db"), ({"seed": 1.5}, "seed")])
def test_settings_refuse_unknown_keys_and_bad_values(table, key):
    with pytest.raises(ValueError, match=key):
        EL.settings(table)


def test_levels_need_the_ff_row():
    doc = _doc(np.zeros(88))
    del doc["key_ff_db"]
    with pytest.raises(ValueError, match="key_ff_db"):
        EL.key_levels(doc, SPEEDS)


def test_levels_follow_the_rows_at_each_dynamic():
    doc = _doc(np.zeros(88))
    doc["speed_db"]["db"] = [[-12.0 + 2 * i for i in range(8)] for _ in range(88)]   # 2 dB per octave
    lv = EL.key_levels(doc, SPEEDS)
    assert lv["ff"][0] == pytest.approx(0.0, abs=1e-6)
    assert lv["mf"][0] == pytest.approx(2 * np.log2(1.5 / 5.0), abs=0.05)


def test_merge_adds_to_every_mallet():
    assert EL.merge([[1, 2, 3], [0, 0, 0]], [1, 1, 1]) == [[2, 3, 4], [1, 1, 1]]
    assert EL.merge(None, [1, 2, 3]) == [[1, 2, 3]]


def test_only_the_ff_level_is_corrected_at_every_dynamic():
    # the engine plays each velocity at the speed where the key sounds a set level under its own ff:
    # only the ff correction moves the level; a different pp or mf one would move the speed (the tone)
    doc = _doc(np.linspace(-20, -40, 88))
    doc["key_ff_db"][60 - 21] -= 4.0
    doc["speed_db"]["db"][60 - 21] = [-12.0 + 2 * i for i in range(8)]    # C4's row unlike the others (0 at ff)
    rows = np.array(EL.corrections(doc, SPEEDS, {}))
    assert np.allclose(rows[:, 0], rows[:, 2]) and np.allclose(rows[:, 1], rows[:, 2])
    assert rows[60 - 21, 2] == pytest.approx(4.0, abs=0.5)


def test_a_target_sets_the_shape():
    # a target shape (anchors re the median): every key's level lands on the piano's median plus it
    ff = np.linspace(-20, -10, 88) + np.random.default_rng(1).normal(0.0, 1.5, 88)
    table = {"target_keys": [21, 60, 108], "target_db": [3.0, 0.0, -9.0]}
    rows = np.array(EL.corrections(_doc(ff), SPEEDS, table))
    want = np.median(ff) + np.interp(np.arange(21, 109), table["target_keys"], table["target_db"])
    assert np.allclose(ff + rows[:, 2], want, atol=1e-9)


@pytest.mark.parametrize("table", [{"target_keys": [21, 60]}, {"target_keys": [60, 21], "target_db": [0, 0]},
                                   {"target_keys": [21, 60], "target_db": [0]}])
def test_a_target_needs_rising_keys_and_a_value_each(table):
    with pytest.raises(ValueError, match="target_keys and target_db"):
        EL.settings(table)
