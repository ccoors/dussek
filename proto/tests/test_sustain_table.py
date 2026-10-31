"""A project's [sustain] by ear (design log step 154): each key's ring re the fitted, as the Sustain
control, linear between its anchors and held beyond."""
import pytest

import export as EX


def test_the_anchors_are_linear_between_and_held_beyond():
    t = {"percent": [[36, 150.0], [72, 150.0], [96, 200.0]]}
    assert EX.sustain_factor(t, 21) == 1.5 and EX.sustain_factor(t, 60) == 1.5
    assert EX.sustain_factor(t, 84) == pytest.approx(1.75) and EX.sustain_factor(t, 108) == 2.0
    assert EX.sustain_factor({}, 60) == 1.0 and EX.sustain_factor(None, 60) == 1.0


@pytest.mark.parametrize("bad", [{"percent": [[60, 900.0]]}, {"percent": [[60, 10.0]]}, {"percent": [[72, 150.0], [60, 150.0]]},
                                 {"percent": [[60, 150.0]], "db": 1}, {"percent": [[120, 150.0]]}, {"percent": [[60]]}])
def test_a_bad_table_is_refused(bad):
    with pytest.raises(ValueError, match="sustain"):
        EX.sustain_factor(bad, 60)


def test_the_sympathetic_ring_by_ear_scales_the_strings_drive():
    assert EX.sympathetic_factor({}) == 1.0 and EX.sympathetic_factor(None) == 1.0
    assert EX.sympathetic_factor({"percent": 25.0}) == 0.25
    # 100 % without a table: the transfer exactly as before (every other instrument's export unchanged)
    import resonance
    assert resonance.TRANSFER * EX.sympathetic_factor({}) == resonance.TRANSFER
    for bad in ({"percent": 400.0}, {"percent": -1.0}, {"percent": "25"}, {"percent": 25.0, "keys": [60]}):
        with pytest.raises(ValueError):
            EX.sympathetic_factor(bad)
