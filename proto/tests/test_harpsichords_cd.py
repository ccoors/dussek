import os

import datasets as D


def test_the_french_and_italian_harpsichords_load_one_layer_with_releases():
    for profile, n, lo, hi in (("french", 28, 26, 84), ("italian", 32, 30, 83)):
        items = D.items(profile)
        notes = sorted({i["note"] for i in items})
        assert len(items) == len(notes) == n and (notes[0], notes[-1]) == (lo, hi)
        assert all(os.path.exists(i["path"]) and "/Sustains/" in i["path"] for i in items)
        assert D.dynamics(profile) == {"mf": "1/1"} and D.levels_recorded(profile)
        assert D.release_sfz(profile)        # its own release samples (release_noise.py)
