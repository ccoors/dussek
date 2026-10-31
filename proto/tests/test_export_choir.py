import numpy as np

import export
import scale
import soundboard as SB


def test_choir_point_is_one_row_nearer_the_nut():
    assert export.choir_beta(0.2, 0.6) == 0.2 - export.D_ROW / 0.6
    assert export.choir_beta(0.03, 0.2) == export.BETA_MIN          # floored


def test_lute_point_near_the_nut_and_before_the_8ii_row():
    assert export.lute_beta(0.2, 0.6) == export.D_LUTE / 0.6        # long string: the distance
    assert export.lute_beta(0.3, 0.06) == export.LUTE_FRAC * 0.3    # short string: capped
    assert export.lute_beta(0.2, 3.0) == export.BETA_MIN            # very long: floored


def test_detune_is_seeded_and_bounded():
    d = [export.choir_detune(n) for n in range(21, 109)]
    assert d == [export.choir_detune(n) for n in range(21, 109)]
    assert all(abs(x) <= export.DETUNE_CENTS for x in d)
    assert max(abs(x) for x in d) > 0.5 * export.DETUNE_CENTS     # not all near 0


def test_lute_and_choir_points_on_every_key():
    prof = scale.Profile("flemish8")
    doc = export.lute_doc(prof, 2)
    assert doc["register"] == 2 and len(doc["beta"]) == 88 and doc["gain_db"] == export.LUTE_DB
    for n, bl in zip(range(21, 109), doc["beta"]):
        p = SB.note_params(prof, n)
        b2 = export.choir_beta(p.pluck_beta, p.L)
        assert export.BETA_MIN <= bl <= max(export.LUTE_FRAC * b2, export.BETA_MIN) and bl < 0.5, n
        assert export.BETA_MIN <= b2 < p.pluck_beta, n


def test_choir_note_carries_its_point_and_tuning():
    prof = scale.Profile("flemish8")
    blob = export.Blob()
    peaks = export.release.peak_table(prof)
    main = export.export_note(prof, 60, blob, prof.hammer_speeds(), peaks, None)
    ov = export.choir_overrides(prof, 60)
    ii = export.export_note(prof, 60, blob, prof.hammer_speeds(), peaks, None, **ov)
    assert ii["excitation"]["beta"] == ov["pluck_beta"] < main["excitation"]["beta"]
    cents = 1200 * np.log2(ii["string"]["f0"] / main["string"]["f0"])
    assert abs(cents - ov["retune_cents"]) < 1e-9
    assert ii["knock"] is None and ii["has_damper"]
