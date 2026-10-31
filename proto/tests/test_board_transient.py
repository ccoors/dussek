"""The board's response moved between the radiation and the correction ([board] transient_db, the
export's): each partial keeps its gain, only the sound between the partials changes (design log step
181: Grand G's treble onsets out through its fitted response's 100–300 Hz hump)."""
import numpy as np
import pytest

import export
import soundboard as SB


def test_the_shift_is_an_octave_around_each_row_as_the_trim():
    f = np.array([40.0, 80.0, 160.0, 320.0, 640.0, 1000.0])
    t = {"transient_db": [[80.0, -15.0], [160.0, -15.0], [320.0, -15.0]]}
    got = export.board_transient_db(f, t)
    # the trim's shape (its own rows within the trim's limit)
    half = {"trim_db": [[c, db / 2] for c, db in t["transient_db"]]}
    assert got == pytest.approx(2 * export.board_trim_db(f, half), abs=1e-12)
    assert got[1:4] == pytest.approx([-15.0] * 3) and got[0] == pytest.approx(0.0) and got[-1] == pytest.approx(0.0)


def test_the_correction_takes_the_opposite_so_the_partials_keep_their_gain():
    grid = np.array(SB.GRID_HZ)
    shift = export.board_transient_db(grid, {"transient_db": [[160.0, -15.0]]})
    rows = np.random.default_rng(1).normal(size=(3, len(grid)))
    notes, out = export.shift_correction(([21.0, 60.0, 99.0], rows), shift)
    assert notes == [21.0, 60.0, 99.0]
    # radiation + correction, as a partial meets them: unchanged
    assert np.allclose(out + shift[None, :], rows)
    # without a correction: one carrying only the opposite
    notes, out = export.shift_correction(None, shift)
    assert list(notes) == list(SB.CORR_NOTES) and np.allclose(out[0], -shift)


def test_no_shift_keeps_the_correction_and_the_board_section_checks_its_keys():
    corr = ([21.0], np.zeros((1, 4)))
    assert export.shift_correction(corr, np.zeros(4)) is corr
    assert not np.any(export.board_transient_db(np.ones(4), {"trim_db": [[63.0, -3.0]]}))
    with pytest.raises(ValueError):
        export.board_transient_db(np.ones(2), {"transient_db": [[160.0, -30.0]]})
    with pytest.raises(ValueError):
        export.board_trim_db(np.ones(2), {"trim_db": [], "boost_db": []})


def test_the_room_takes_the_shift_on_its_level_exactly():
    # the room's level rows (linear in log frequency, held beyond the ends) plus the shift, at both
    # sets' breakpoints: the sum everywhere between
    t = {"transient_db": [[80.0, -15.0], [160.0, -15.0]]}
    rows = [[250.0, -7.0], [500.0, 0.0], [1000.0, 0.0]]
    out = export.shift_room_eq(rows, t)
    f = np.array([20.0, 50.0, 100.0, 200.0, 300.0, 700.0, 4000.0])
    lf = np.log2(f)
    got = np.interp(lf, np.log2([r[0] for r in out]), [r[1] for r in out])
    want = np.interp(lf, np.log2([r[0] for r in rows]), [r[1] for r in rows]) + export.board_transient_db(f, t)
    assert got == pytest.approx(want, abs=0.01)
    assert export.shift_room_eq(rows, {"trim_db": [[63.0, -3.0]]}) is rows
    assert export.shift_room_eq([], t)[0][1] == pytest.approx(0.0)
