import os

import pytest

import datasets as D

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "data", "livingroom-upright")


@pytest.mark.skipif(not os.path.isdir(ROOT), reason="LivingRoom Micro SFZ not extracted")
def test_livingroom_loads_its_pedal_up_sustains_at_two_dynamics():
    items = D.items("upright_c")
    notes = {i["note"] for i in items}
    assert len(notes) == 33 and min(notes) == 22
    assert sorted({i["dynamic"] for i in items}) == ["1/2", "2/2"]
    assert len(items) == 66
    assert all("-poff-" in os.path.basename(i["path"]) for i in items)   # pedal up only
    assert all(os.path.exists(i["path"]) for i in items)
    assert D.dynamics("upright_c") == {"mf": "1/2", "ff": "2/2"} and D.levels_recorded("upright_c")
