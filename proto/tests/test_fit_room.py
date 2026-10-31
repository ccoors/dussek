import numpy as np

import fit_room as FR


def two_slope(times, level, t_late, t_early, early_db):
    """The course of a two-part room, dB: late and early power added."""
    t = np.asarray(times)
    return level + 10 * np.log10(10 ** (-6 * t / t_late) + 10 ** (early_db / 10) * 10 ** (-6 * t / t_early))


def test_the_compensation_restores_h():
    # a filter whose smoothed spectrum is H plus a tilt: the compensation is minus the tilt
    grid = np.array([100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0])
    H_db = np.array([-10.0, -5.0, 0.0, -2.0, -6.0, -12.0, -20.0])
    tilt = np.array([6.0, 4.0, 2.0, 0.0, -2.0, -4.0, -6.0])
    f = np.geomspace(50, 12800, 4000)
    power = 10 ** (np.interp(np.log2(f), np.log2(grid), H_db + tilt) / 10)
    c = FR.compensation(f, power, grid, H_db)
    assert np.allclose(c - np.median(c), -(tilt - np.median(tilt)), atol=0.5), c


def test_a_band_at_its_noise_floor_sets_no_target():
    times = np.array(FR.TIMES)
    course = {fc: two_slope(times, -40.0, 1.0, 0.2, 15.0) for fc in FR.BANDS}
    course[8000] = np.full(len(times), -63.0)              # flat: the recording's noise
    assert FR.usable(course) == [fc for fc in FR.BANDS if fc != 8000]


def test_the_compensation_gives_back_what_the_cuts_take(monkeypatch):
    import scale
    import soundboard as SB
    p = scale.Profile("grand_e")
    room = {**p.felt_fit["room"], "eq_db": [[1000.0, 0.0], [2000.0, 0.0]]}
    p.felt_fit = {**p.felt_fit, "room": room}
    assert np.all(FR.filter_comp(p) == 0.0)
    p.felt_fit = {**p.felt_fit, "room": {**room, "eq_db": [[250.0, 0.0], [1000.0, -20.0], [4000.0, 0.0]]}}
    c = FR.filter_comp(p)
    g = np.array(p.felt_fit["soundboard"]["grid_hz"])
    at = lambda f: np.interp(np.log2(f), np.log2(g), c)  # noqa: E731
    assert 0.5 < at(1000.0) <= 3.5, at(1000.0)                  # the room's half of the partials, at most 3 dB
    assert abs(at(250.0)) < 1.0 and abs(at(8000.0)) < 1.0, (at(250.0), at(8000.0))  # the normalization


def test_where_the_strings_onset_overfills_the_room_is_cut():
    R, O, N = np.array([1e-5]), np.array([1e-4]), np.array([1e-7])
    c = FR.cuts(R, O, N)
    assert abs(c[0] - 10 * np.log10((1e-5 - 1e-7) / (1e-4 - 1e-7))) < 1e-9
    # without the room still over the recording: not the room's excess, kept but for KEEP_CUT_DB
    assert FR.cuts(R, O, np.array([2e-5]))[0] == -FR.KEEP_CUT_DB
    # under the recording: no boost
    assert FR.cuts(np.array([1e-3]), O, N)[0] == 0.0
    # past the deepest cut: MAX_CUT_DB
    assert FR.cuts(np.array([1e-9]), O, np.array([1e-10]))[0] == -FR.MAX_CUT_DB


def test_the_room_document_holds_the_cuts():
    d = FR.room_doc(np.full(len(FR.BANDS), -2.0))
    assert d["eq_db"][0] == [250.0, -2.0] and d["gain"] == 1.0 and "strike" not in d


# a room band without a target (the recording's noise floor there) holds its neighbor's cut: left at
# 0 dB, Grand E's 8 kHz band sat uncut beside a −19.9 dB cut at 4 kHz; with the decay held long up
# there the room carried the top octave 4.4 dB over the dry sound, heard as a rattle (2026-10-22)
def test_a_band_without_target_holds_the_cut_below():
    eq = np.array([0.0, 0.0, 0.0, -9.05, -19.86, 0.0])
    has = np.array([True, True, True, True, True, False])
    assert FR.hold_untargeted(eq, has).tolist() == [0.0, 0.0, 0.0, -9.05, -19.86, -19.86]


def test_the_lowest_bands_hold_the_cut_above():
    eq = np.array([0.0, 0.0, -6.0, -3.0, 0.0, 0.0])
    has = np.array([False, False, True, True, True, True])
    assert FR.hold_untargeted(eq, has).tolist() == [-6.0, -6.0, -6.0, -3.0, 0.0, 0.0]


def test_no_target_anywhere_cuts_nothing():
    eq = np.zeros(6)
    assert FR.hold_untargeted(eq, np.zeros(6, bool)).tolist() == [0.0] * 6
