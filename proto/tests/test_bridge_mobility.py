"""The bridge's mobility along the keyboard (scale.mobility_factor): a factor per key on the
bridge admittance below 500 Hz, log-interpolated between the coupling's mobility@<note>
anchors (design 2026-10-20-bridge-mobility). The engine pins the same table (physics.rs)."""
import numpy as np
import pytest

import resonance
import scale

CP = {"mobility@24": 1.0, "mobility@36": 4.0, "mobility@48": 2.0}
# the table the engine's test pins too (physics.rs mobility_follows_its_table)
TABLE = [((36, 0.0), 4.0), ((30, 0.0), 2.0), ((36, 500.0), 2.5), ((42, 250.0), 2.720872588),
         ((60, 100.0), 1.998402556), ((20, 1000.0), 1.0), ((36, 2000.0), 1.011673152), ((108, 0.0), 2.0)]


def test_the_mobility_factor_follows_its_table():
    for (note, f), want in TABLE:
        assert scale.mobility_factor(CP, note, f) == pytest.approx(want, abs=1e-9)


def test_no_anchors_no_factor():
    assert scale.mobility_factor({}, 36, 100.0) == 1.0
    assert scale.mobility_anchors({"kf2": 0.8, "horiz@36": 1.0}) == []


def test_the_notes_admittance_carries_the_factor(monkeypatch):
    prof = scale.Profile("grand_a")
    cp = {**prof.coupling(), **CP}
    monkeypatch.setattr(prof, "coupling", lambda: cp)
    f = np.array([65.0, 131.0, 262.0, 1000.0])
    got = scale.note_params(prof, 36).admittance(f)
    assert np.allclose(got, prof.board.admittance(f) * scale.mobility_factor(cp, 36, f))


def test_without_anchors_the_note_keeps_the_boards_own_admittance():
    prof = scale.Profile("grand_a")
    assert scale.note_params(prof, 36).admittance == prof.board.admittance


def test_the_sympathetic_strings_feel_it(monkeypatch):
    prof = scale.Profile("grand_a")
    f0, _, k0 = resonance.note_modes(prof, 36)
    cp = {**prof.coupling(), **CP}
    monkeypatch.setattr(prof, "coupling", lambda: cp)
    f1, _, k1 = resonance.note_modes(prof, 36)
    assert np.allclose(f0, f1)
    assert np.allclose(k1.real / k0.real, scale.mobility_factor(cp, 36, f0), rtol=1e-9)


def test_no_mobility_doc_without_anchors():
    import export
    assert export.mobility_doc({"kf2": 0.8}) is None


def test_the_mobility_doc_lists_the_anchors():
    import export
    assert export.mobility_doc(CP) == {"anchors": [24, 36, 48], "factors": [1.0, 4.0, 2.0], "fc_hz": 500.0}
