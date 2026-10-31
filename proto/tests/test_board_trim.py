"""The board's tone trimmed by ear ([board] trim_db in a part's project, the export's; the fits never
see it): per row a band center and dB, the octave around it, linear in log frequency to the next
octave (adjacent rows join flat), added to every key's correction (the partials' gain). Grand F's
bass sat 3–4.5 dB heavy at 63–125 Hz re its recording, heard as boomy (2026-10-22)."""
import numpy as np
import pytest

import export
import soundboard as SB


def test_the_trim_is_an_octave_around_each_row():
    t = {"trim_db": [[63.0, -3.0], [126.0, -3.0]]}
    f = np.array([31.5, 63.0, 63.0 * 2 ** 0.5, 126.0, 126.0 * 2 ** 0.5, 252.0, 1000.0])
    assert export.board_trim_db(f, t) == pytest.approx([0.0, -3.0, -3.0, -3.0, -1.5, 0.0, 0.0], abs=1e-9)


def test_the_trim_adds_to_every_key():
    grid = np.array(SB.GRID_HZ)
    rows = np.zeros((3, len(grid)))
    notes, out = export.trim_correction(([21.0, 60.0, 99.0], rows), {"trim_db": [[63.0, -3.0]]}, grid)
    assert notes == [21.0, 60.0, 99.0]
    curve = export.board_trim_db(grid, {"trim_db": [[63.0, -3.0]]})
    assert np.allclose(out, rows + curve)


def test_without_a_correction_the_trim_makes_one():
    grid = np.array(SB.GRID_HZ)
    notes, out = export.trim_correction(None, {"trim_db": [[63.0, -3.0]]}, grid)
    assert list(notes) == list(SB.CORR_NOTES) and np.allclose(out[0], export.board_trim_db(grid, {"trim_db": [[63.0, -3.0]]}))


def test_no_trim_keeps_the_correction():
    corr = ([21.0], np.zeros((1, 4)))
    assert export.trim_correction(corr, {}, np.ones(4)) is corr and export.trim_correction(None, None, np.ones(4)) is None


def test_a_row_with_keys_fades_in_over_them():
    # [Hz, dB, key at 0, key at full]: the treble's upper partials cut, the middle left alone
    t = {"trim_db": [[4000.0, -4.0, 60.0, 72.0]]}
    f = np.array([4000.0])
    assert [export.board_trim_db(f, t, note)[0] for note in (48.0, 60.0, 66.0, 72.0, 84.0)] == \
        pytest.approx([0.0, 0.0, -2.0, -4.0, -4.0], abs=1e-9)
    # reversed, the bass: full below, gone above
    t = {"trim_db": [[250.0, -3.0, 52.0, 40.0]]}
    assert [export.board_trim_db(np.array([250.0]), t, note)[0] for note in (28.0, 46.0, 60.0)] == \
        pytest.approx([-3.0, -1.5, 0.0], abs=1e-9)


def test_a_row_with_keys_spares_the_fundamental():
    # the top octave's fundamentals lie in the treble row's band: a row with keys trims a key's
    # upper partials only, from √2·f0 (0) to 2·f0 (full), so the key's loudness stays
    t = {"trim_db": [[4000.0, -4.0, 60.0, 72.0]]}
    f0 = 440.0 * 2 ** ((105 - 69) / 12)   # A7, 3520 Hz
    got = export.board_trim_db(np.array([f0, f0 * 2 ** 0.5, f0 * 2 ** 0.75, 2 * f0]), t, 105.0)
    hat = export.board_trim_db(np.array([2 * f0]), {"trim_db": [[4000.0, -4.0]]})[0]
    assert got == pytest.approx([0.0, 0.0, 0.5 * export.board_trim_db(np.array([f0 * 2 ** 0.75]), t, 72.0)[0], hat], abs=1e-9)
    # a row without keys trims the fundamentals too (Grand F's boom)
    assert export.board_trim_db(np.array([f0]), {"trim_db": [[4000.0, -4.0]]}, 105.0)[0] < -3.0


def test_a_ramped_trim_follows_the_correction_anchors():
    grid = np.array(SB.GRID_HZ)
    t = {"trim_db": [[4000.0, -4.0, 60.0, 72.0]]}
    notes, out = export.trim_correction(([21.0, 66.0, 99.0], np.zeros((3, len(grid)))), t, grid)
    assert np.allclose(out[0], 0.0)
    # above each anchor's second partial, the plain row times the fade
    for row, note, db in ((1, 66.0, -2.0), (2, 99.0, -4.0)):
        up = grid >= 2 * 440.0 * 2 ** ((note - 69) / 12)
        assert np.allclose(out[row][up], export.board_trim_db(grid[up], {"trim_db": [[4000.0, db]]}))


def test_a_bad_trim_is_refused():
    for bad in ({"trim_db": [[63.0, -20.0]]}, {"trim_db": [[0.0, -3.0]]}, {"trim_db": "63:-3"}, {"trim": [[63.0, -3.0]]},
                {"trim_db": [[63.0, -3.0, 60.0]]}, {"trim_db": [[63.0, -3.0, 60.0, 60.0]]}):
        with pytest.raises(ValueError):
            export.board_trim_db(np.array([100.0]), bad)


def test_a_block_is_swapped_in_place():
    # the export measures the even levels without the trim, then swaps the trimmed correction in: the
    # same size, every other block where it was
    b = export.Blob()
    a = b.add(np.ones(3))
    c = b.add(np.zeros((2, 4)))
    d = b.add(np.arange(5.0))
    before = b"".join(b.parts)
    b.replace(c, np.full((2, 4), -3.0))
    after = b"".join(b.parts)
    assert len(after) == len(before) and after[:c["o"]] == before[:c["o"]] and after[d["o"]:] == before[d["o"]:]
    assert np.frombuffer(after[c["o"]:c["o"] + 64], "<f8").tolist() == [-3.0] * 8
    with pytest.raises(ValueError):
        b.replace(c, np.zeros(3))
