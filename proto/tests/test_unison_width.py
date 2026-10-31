"""The unison's width set by ear ([unison] width in a part's project, the export's; the fits never see
it): the fitted detune scaled as the plugin's Unison width scales it. Grand B's beats ran 25–60 %
faster than its recording's and its partials 4–6 wobbled ~1 dB deeper; the user heard 60 % as right
(2026-10-22). `treble`, a factor along the keys: the recordings' unisons beat faster toward the top key
than the fitted detune, measured below the top octaves, says (design log step 183)."""
import pytest

import scale


def _c4():
    return scale.note_params(scale.Profile("grand_b"), 60)


def test_the_width_scales_the_detune():
    p = _c4()
    q = scale.with_unison(p, {"width": 0.6})
    assert q.detune_cents == pytest.approx(tuple(0.6 * c for c in p.detune_cents))
    assert q.horiz_detune_cents == p.horiz_detune_cents


def test_no_table_changes_nothing():
    p = _c4()
    assert scale.with_unison(p, {}) == p and scale.with_unison(p, None) == p


def test_a_bad_width_is_refused():
    for bad in ({"width": 0.0}, {"width": 3.0}, {"width": "0.6"}, {"spread": 0.6},
                {"treble": [[90, 2.0], [80, 1.0]]}, {"treble": [[90, 20.0]]}, {"treble": []}, {"treble": [90, 2.0]}):
        with pytest.raises(ValueError):
            scale.with_unison(_c4(), bad)


def test_the_treble_ramp_scales_the_detune_along_the_keys():
    prof = scale.Profile("grand_b")
    table = {"treble": [[78, 1.0], [102, 4.0]]}
    for note, k in ((60, 1.0), (78, 1.0), (90, 2.0), (102, 4.0), (108, 4.0)):
        p = scale.note_params(prof, note)
        assert scale.with_unison(p, table).detune_cents == pytest.approx(tuple(k * c for c in p.detune_cents))
    p = scale.note_params(prof, 90)
    both = scale.with_unison(p, {"width": 0.6, **table})
    assert both.detune_cents == pytest.approx(tuple(1.2 * c for c in p.detune_cents))


def test_the_export_plays_the_width():
    import export
    import project as P
    prof = scale.Profile("grand_b")
    peaks = {n: {"pp": 1.0, "mf": 1.0, "ff": 1.0} for n in range(21, 109)}
    plain = export.export_note(prof, 60, export.Blob(), prof.hammer_speeds(), peaks, None)
    P.override("grand_b", "unison", {"width": 0.6})
    try:
        doc = export.export_note(prof, 60, export.Blob(), prof.hammer_speeds(), peaks, None,
                                 unison=P.section("grand_b", "unison"))
    finally:
        P.clear_overrides()
    assert doc["string"]["detune_cents"] == pytest.approx([0.6 * c for c in plain["string"]["detune_cents"]])
