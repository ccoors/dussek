"""The room's level per band trimmed by ear ([room] trim_db in a part's project, the export's; the fits
never see it): Grand C's room carried the top octave over the dry sound, its synthetic tail heard as a
rattle, though its level between the partials matched the recording's; −12.6 dB at 8 kHz cured it by
ear (2026-10-22). The partials' compensation stays as fitted, as the variant heard had it."""
import pytest

import export

ROWS = [[250.0, 0.0], [500.0, 0.0], [1000.0, 0.0], [2000.0, -2.59], [4000.0, -12.59], [8000.0, 0.0]]


def test_the_trim_adds_to_its_band():
    assert export.room_eq(ROWS, {"trim_db": [[8000, -12.59]]}) == ROWS[:5] + [[8000.0, -12.59]]


def test_no_trim_keeps_the_fit():
    assert export.room_eq(ROWS, {}) == ROWS and export.room_eq(ROWS, None) == ROWS


def test_an_unfitted_room_gets_every_band():
    # the engine holds an eq_db row's value beyond it: a lone 8 kHz row would cut every band
    out = export.room_eq([], {"trim_db": [[8000, -6.0]]})
    assert [b for b, _ in out] == [250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0] and out[-1][1] == -6.0
    assert all(v == 0.0 for _, v in out[:-1])


def test_a_lift_undoes_a_fitted_cut():
    # Upright B's room, fitted down to its dry recordings' gaps, heard as too dry (2026-10-24)
    assert export.room_eq(ROWS, {"trim_db": [[4000, 12.59], [2000, 2.59]]})[3:5] == [[2000.0, 0.0], [4000.0, 0.0]]
    assert export.room_eq(ROWS, {"trim_db": [[8000, 6.0]]})[-1] == [8000.0, 6.0]


def test_a_bad_trim_is_refused():
    for bad in ({"trim_db": [[7000, -3.0]]}, {"trim_db": [[8000, 7.0]]}, {"trim": [[8000, -3.0]]}):
        with pytest.raises(ValueError):
            export.room_eq(ROWS, bad)


def test_the_bands_are_the_room_fits():
    import fit_room
    assert tuple(float(b) for b in fit_room.BANDS) == export.ROOM_BANDS
