"""The felt's exponent set by ear ([felt] p in a part's project, the export's; the fits never see
it): the force at 0.5 mm kept, its growth past it changed. Grand D's bass at 1–2 kHz opened 19–32 dB
from mp to f in the engine against 5–12 dB in the recording (the board investigation, 2026-10-21)."""
from dataclasses import replace

import numpy as np
import pytest

import scale
import string_model as M


def _c2():
    return replace(scale.note_params(scale.Profile("grand_a"), 36), core_K=0.0, core_depth=0.0)


def test_the_force_at_half_a_millimeter_is_kept():
    p = _c2()
    q = scale.with_felt(p, {"p": 1.8})
    assert q.felt_p == 1.8
    f = lambda x, d: M.felt_force(x.felt_K, x.felt_p, 0.0, 0.0, d)  # noqa: E731
    assert f(q, 5e-4) == pytest.approx(f(p, 5e-4), rel=1e-9)
    # deeper, a lower exponent pushes less; shallower, more
    assert f(q, 1.5e-3) < f(p, 1.5e-3) and f(q, 2e-4) > f(p, 2e-4)


def test_no_table_changes_nothing():
    p = _c2()
    assert scale.with_felt(p, {}) == p and scale.with_felt(p, None) == p


def test_a_lower_exponent_narrows_the_swing_between_dynamics():
    # the attack's 1–2 kHz re 200–500 Hz, from a soft strike to a hard one
    import bass_check as BC
    def swing(x):
        a, _ = M.render(x, 1.0, duration=0.3)
        b, _ = M.render(x, 6.0, duration=0.3)
        return BC.attack_bands(b, 48000, 0)[0] - BC.attack_bands(a, 48000, 0)[0]
    p = _c2()
    assert swing(scale.with_felt(p, {"p": 1.6})) < swing(p) - 3


def test_the_core_follows_the_new_exponent():
    # the core's f05 is the force 0.5 mm past its depth under the felt's exponent: set after it
    p = _c2()
    core = {"notes": [36], "f05": [10.0], "depth_mm": [0.3]}
    q = scale.with_core(scale.with_felt(p, {"p": 1.8}), core)
    assert q.core_K == pytest.approx(10.0 / 5e-4 ** 1.8)


def test_the_stiffness_scales_the_felt_as_the_hardness_knob_does():
    # the engine's Hammer hardness multiplies K by 10^(dB/20): [felt] stiffness_db the same, in the export
    p = _c2()
    q = scale.with_felt(p, {"stiffness_db": 2.5})
    assert q.felt_p == p.felt_p and q.felt_K == pytest.approx(p.felt_K * 10 ** (2.5 / 20), rel=1e-12)
    # with the exponent: the force at 0.5 mm 2.5 dB over the fitted one
    r = scale.with_felt(p, {"p": 1.7, "stiffness_db": 2.5})
    f = lambda x: M.felt_force(x.felt_K, x.felt_p, 0.0, 0.0, 5e-4)  # noqa: E731
    assert r.felt_p == 1.7 and f(r) == pytest.approx(f(p) * 10 ** (2.5 / 20), rel=1e-9)


def test_a_bad_exponent_is_refused():
    for bad in ({"p": 0.5}, {"p": 6.0}, {"p": "2"}, {"q": 2.0}, {"stiffness_db": 20.0}, {"stiffness_db": "2"}):
        with pytest.raises(ValueError):
            scale.with_felt(_c2(), bad)


def test_the_release_peaks_see_the_felt():
    import release
    assert release._peak_key("grand_a", None, {"p": 1.8}) != release._peak_key("grand_a", None)


def test_the_export_plays_the_felt():
    import export
    import project as P
    prof = scale.Profile("grand_a")
    peaks = {n: {"pp": 1.0, "mf": 1.0, "ff": 1.0} for n in range(21, 109)}
    P.override("grand_a", "felt", {"p": 1.9})
    try:
        doc = export.export_note(prof, 36, export.Blob(), prof.hammer_speeds(), peaks, None,
                                 felt=P.section("grand_a", "felt"))
    finally:
        P.clear_overrides()
    assert doc["excitation"]["felt_p"] == 1.9
