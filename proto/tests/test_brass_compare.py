"""What a rebuild's comparison lists (proto/brass/compare_tables.py)."""
import copy
import importlib.util
import os
import sys

import project as P

sys.path.append(os.path.join(P.REPO, "proto", "brass"))

_spec = importlib.util.spec_from_file_location("compare_tables", os.path.join(P.REPO, "proto", "brass", "compare_tables.py"))
compare_tables = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(compare_tables)
differences = compare_tables.differences

BANDS = [-20.0, -18.0, -16.0, -14.0, -12.0, -10.0, -8.0]
TABLES = {"tables": {"open": {"60": {d: {"bands": list(BANDS)} for d in ("pp", "mf", "ff")}}}, "level": {}, "gain_db": 0.0}


def test_equal_tables_differ_nowhere():
    assert differences(TABLES, TABLES) == []


def test_a_band_over_a_db_is_listed():
    louder = copy.deepcopy(TABLES)
    louder["tables"]["open"]["60"]["ff"]["bands"][3] += 2.0  # 1 to 2 kHz
    got = differences(TABLES, louder)
    assert len(got) == 1
    assert got[0][:4] == ("open", 60, "ff", 1000.0) and abs(got[0][4] - 2.0) < 1e-9


def test_under_a_db_is_not_listed():
    other = copy.deepcopy(TABLES)
    other["tables"]["open"]["60"]["pp"]["bands"] = [v + 0.5 for v in BANDS]
    assert differences(TABLES, other) == []


def test_the_output_s_level_counts():
    # the same filter 2 dB up through the fit's level at mf is 2 dB in every band at mf
    other = copy.deepcopy(TABLES)
    other["level"] = {"open": {"60": [0.0, 2.0, 0.0, 0.0]}}
    got = differences(TABLES, other)
    assert {d[2] for d in got} == {"mf"} and len(got) == len(BANDS)


def test_a_key_on_one_side_is_listed():
    other = copy.deepcopy(TABLES)
    del other["tables"]["open"]["60"]
    assert differences(TABLES, other) == [("open", 60, "*", 0.0, float("inf"))]
