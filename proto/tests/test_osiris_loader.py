import os

import pytest

import datasets as D

ROOT = D.root("grand_g")
pytestmark = pytest.mark.skipif(not os.path.isdir(ROOT), reason="Osiris Piano not in data/")


def test_osiris_loads_the_unfiltered_pedal_up_inner_microphone():
    items = D.items("grand_g")
    notes = {i["note"] for i in items}
    assert min(notes) == 21 and max(notes) == 108 and len(notes) == 52        # the white keys
    assert {i["dynamic"] for i in items} == {"vl1", "vl2"} and len(items) == 52 + 51   # no B3 vl1
    assert all("/UC-noisy/A/" in i["path"] and os.path.exists(i["path"]) for i in items)
    # the file names count octaves one lower: "C3" plays middle C
    c4 = next(i for i in items if i["note"] == 60 and i["dynamic"] == "vl1")
    assert os.path.basename(c4["path"]) == "Piano_UC_MicA_C3_vl1.flac"
    assert D.dynamics("grand_g") == {"mf": "vl1", "ff": "vl2"} and D.levels_recorded("grand_g")
