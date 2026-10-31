"""The export's even levels (`[even_levels]`) and variants into the snapshots folder (design
2026-10-21-even-loudness)."""
import copy

import numpy as np
import pytest

import export as E

LN_SPEEDS = [0.078, 0.156, 0.3125, 0.625, 1.25, 2.5, 5.0, 10.0]


def _doc():
    notes = [{"note": k, "excitation": {"type": "hammer"}} for k in range(21, 109)]
    notes[60 - 21]["level_db"] = [[1.0, 1.0, 1.0]]
    return {"format": 5, "profile": "p", "velocity": {"midi": [14, 60, 124], "speed": [0.4, 1.5, 5.0]}, "notes": notes}


def _level(ff):
    return {"ff_db": -20.0, "keys": [48, 60, 72], "key_ff_db": list(map(float, ff)),
            "speed_db": {"keys": list(range(21, 109)), "speed": LN_SPEEDS, "db": [[0.0] * 8 for _ in range(88)]}}


def test_without_the_table_nothing_changes():
    doc = _doc()
    before = copy.deepcopy(doc)
    assert E.apply_even_levels(doc, _level(np.zeros(88)), None) is False
    assert doc == before


def test_the_table_adds_to_the_corrections():
    ff = np.full(88, -30.0)
    ff[60 - 21] -= 3.0
    doc = _doc()
    assert E.apply_even_levels(doc, _level(ff), {}) is True
    assert doc["notes"][60 - 21]["level_db"] == [pytest.approx([4.0] * 3, abs=0.5)]
    assert all(np.allclose(n["level_db"], 0.0, atol=0.5) for n in doc["notes"] if n["note"] != 60)


def test_a_piano_without_a_level_table_is_refused():
    lv = _level(np.zeros(88))
    del lv["speed_db"]
    with pytest.raises(ValueError, match="even_levels"):
        E.apply_even_levels(_doc(), lv, {})


def test_a_variant_is_named_as_a_snapshot():
    got = E.variant_target("p", "even", {"name": "Grand X", "group": "Grand pianos"}, "p", "/d")
    assert got == ("/d/p@even", {"name": "Grand X (even)", "group": "Snapshots"}, "p@even")
    for bad in ("even scattered", "a/b", ""):
        with pytest.raises(ValueError):
            E.variant_target("p", bad, None, "p", "/d")
