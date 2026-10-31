import os

import pytest

import datasets as D

ROOT = D.root("grand_c")
pytestmark = pytest.mark.skipif(not os.path.isdir(ROOT), reason="Headroom Piano not in data/")


def test_headroom_loads_its_close_microphone_at_three_layers():
    items = D.items("grand_c")
    assert sorted({i["note"] for i in items}) == list(range(21, 109, 3))
    assert sorted({i["dynamic"] for i in items}) == ["L1", "L2", "L5"] and len(items) == 3 * 30
    assert all(" CLOSE " in os.path.basename(i["path"]) and os.path.exists(i["path"]) for i in items)
    # pp, mf at the engine's MIDI 60 (layer 2: velocities 60–89), ff at 124 (layer 5: 120–127)
    assert D.dynamics("grand_c") == {"pp": "L1", "mf": "L2", "ff": "L5"} and D.levels_recorded("grand_c")
