"""The soundboard step on a take without tracked partials (a dead or misread take): it has no bands
above its partials and is passed over, where max() over its empty list stopped the step."""
import numpy as np

import soundboard


def test_a_take_without_partials_gives_no_rows():
    takes = {(60, "mf"): {"partials": [], "onset_s": 0.0}}
    synth = {60: {"mf": (np.array([2000.0, 4000.0]), np.array([-20.0, -30.0]))}}
    f, note, y = soundboard.band_rows("any", takes, synth, [60])
    assert len(f) == len(note) == len(y) == 0
