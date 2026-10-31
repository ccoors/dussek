import numpy as np

import pedal


def test_the_strings_leave_the_dampers_from_rest_against_each_other():
    # x = Re(C·e^{λt}): no displacement at the lift (else the bridge force jumped: a click), and
    # the unison's strings cancel at the bridge (the ring in its slow modes)
    pd = {"amp": 1e-3, "center": 40, "spread": 6.5, "rolloff": 2.0}
    for note, S in ((40, 3), (30, 2), (22, 1)):
        C = pedal.kick(None, pd, note, S, 6)
        assert C.shape == (S, 6) and np.allclose(C.real, 0.0)
        if S > 1:
            assert np.allclose(C.sum(axis=0), 0.0)
        # the same energy per string as a unit-phase kick of the same amplitude
        assert np.isclose(np.sum(np.abs(C[:, 0]) ** 2), S * (pd["amp"] * pedal.bell(note, 40, 6.5)) ** 2)
