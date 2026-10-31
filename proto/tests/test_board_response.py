import numpy as np

import board

# a guitar-like transfer: 64 dB from 80 Hz to its top at 16 kHz (the finger pluck's steep
# lowpass made up for)
GRID = 20.0 * 2 ** (np.arange(0, 10.05, 1 / 6))
H_DB = np.interp(np.log2(GRID), np.log2([20, 80, 220, 1000, 16000, 20500]), [-64, -64, -38, -42, 0, -3])


def band_deviation(b, centers, sr=48000):
    ir = b.impulse_response(GRID, H_DB, sr, length_s=1.0)
    F = (np.abs(np.fft.rfft(ir, 1 << 17, axis=1)) ** 2).mean(axis=0)
    f = np.fft.rfftfreq(1 << 17, 1 / sr)
    H2 = 10 ** (np.interp(np.log2(np.maximum(f, 1)), np.log2(GRID), H_DB) / 10)
    dev = []
    for c in centers:
        band = (f > c / 2 ** (1 / 6)) & (f < c * 2 ** (1 / 6))
        dev.append(10 * np.log10(F[band].mean() / H2[band].mean()))
    return np.array(dev) - np.median(dev)


def test_the_response_follows_the_transfer_in_band_average():
    # the modes summed with their H as weights: their tails (each ~50 dB under its peak) filled
    # the bass 12-22 dB over H where H spans 60 dB (the guitar's E2 rang on its fundamental)
    # the bass on average with H; single bands scatter by the few modes in them (the fine
    # structure: 0-6 modes per third of an octave on a sparse board), and one draw's mode
    # shapes tilt its bands by up to 6 dB: the mean over six draws (boards and radiation)
    centers = [63, 80, 100, 125, 160, 200, 400, 1000, 2000, 5000]
    for density in (1 / 72, 1 / 18):
        dev = np.mean([band_deviation(board.Board(seed=s, density=density), centers) for s in range(1, 7)], axis=0)
        assert abs(dev[:5].mean()) < 3.0 and np.all(np.abs(dev) < 6.0), (density, np.round(dev, 1))


def test_the_radiation_is_drawn_apart_from_the_modes():
    # another radiation draw leaves the modes and their coupling to the strings as they are
    a, b = board.Board(seed=4), board.Board(seed=4, rad_seed=3)
    assert np.array_equal(a.f, b.f) and np.array_equal(a.phi, b.phi) and np.array_equal(a.sigma, b.sigma)
    assert not np.allclose(a.r, b.r) and not np.allclose(a.r_h, b.r_h)
    assert np.array_equal(a.r, board.Board(seed=4, rad_seed=0).r)


def test_notch_depths_find_a_notch():
    # a comb (an echo at 1 ms): notches at every 1 kHz, the peaks halfway between
    sr = 48000
    ir = np.zeros((1, 4800))
    ir[0, 0], ir[0, 48] = 1.0, -0.98
    d = board.notch_depths(ir, sr, [1000.0, 2000.0, 1500.0, 2500.0])[0]
    assert d[0] < -20 and d[1] < -20 and d[2] > 0 and d[3] > 0, np.round(d, 1)


def test_a_sparse_board_keeps_its_modes_above_the_floor():
    # the strumstick's board (a mode per ~200 Hz): its jittered first mode fell to −0.9 Hz, and the
    # engine's microphone response failed on it
    for seed in range(1, 40):
        b = board.Board(seed=seed, density=board.DENSITY * (0.05 / 1.7) * 3.0)
        assert b.f.min() >= 20.0
