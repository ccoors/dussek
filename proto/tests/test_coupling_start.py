"""The coupling fit's choice among its starts and its jump report (fit_coupling.choose, jumps): the
fit's surface has minima of nearly equal error far apart, so a refit keeps to the committed fit's
valley unless another start is clearly better, and a large move is reported for a listen."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
import fit_coupling as FC  # noqa: E402


def test_the_committed_start_stays_unless_another_is_clearly_better():
    # (error dB, params) per start; the committed fit's start first
    committed = (8.02, {"reactive": 0.99})
    near = (7.95, {"reactive": 2.70})          # Grand A's jump: 0.07 dB for ×2.7
    assert FC.choose([committed, near], committed=0) == 0
    clearly = (7.80, {"reactive": 2.70})
    assert FC.choose([committed, clearly], committed=0) == 1
    # without a committed fit: the lowest
    assert FC.choose([committed, near], committed=None) == 1


def test_a_parameter_moving_twofold_is_a_jump():
    old = {"reactive": 0.99, "hdetune": 0.22, "kf2": 0.5}
    new = {"reactive": 2.70, "hdetune": 0.02, "kf2": 0.6}
    assert [j[0] for j in FC.jumps(old, new)] == ["hdetune", "reactive"]
    assert FC.jumps(old, old) == []
    assert FC.jumps({}, new) == [] and FC.jumps(None, new) == []    # nothing committed: nothing to compare
    assert FC.jumps({"reactive": 1.0}, {"reactive": 1.99}) == []     # under twofold
