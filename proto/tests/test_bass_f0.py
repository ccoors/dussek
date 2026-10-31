import numpy as np

import analyze as A
import scale


def test_a_missing_fundamental_does_not_pull_f0_onto_a_stray_peak():
    """Harpsichord C's D1: partials 2… of 36.6 Hz, no fundamental, a small peak at 33.6 Hz:
    the refinement took the peak as the fundamental and fitted f0 −182 cents, B 0.03."""
    sr, f0 = 44100, 36.6
    t = np.arange(int(2.0 * sr)) / sr
    x = sum(np.exp(-0.5 * t) * np.sin(2 * np.pi * n * f0 * t) / n for n in range(2, 40))
    x = x + 0.03 * np.sin(2 * np.pi * 33.6 * t)
    x = np.column_stack([x, x]) * 0.1
    r = A.estimate_partials(x, sr, 0, A.midi_to_hz(26))
    assert abs(1200 * np.log2(r["f0"] / f0)) < 3 and r["B"] < 1e-4 and r["fit_resid_cents"] < 4, r   # broken: 16.6


def test_beyond_the_recorded_keys_the_trend_is_limited():
    notes = np.arange(40, 81)
    steep = 5.0 * (notes - 60.0)                     # 5 per semitone
    at = scale._smooth(notes, steep, np.ones(len(notes)), frac=3.0, slope_max=2.0)
    assert abs(at(70) - 50.0) < 1e-6                # inside: as before
    assert abs((at(90) - at(80)) - 20.0) < 1e-6 and abs((at(30) - at(40)) + 20.0) < 1e-6
    flat = scale._smooth(notes, np.zeros(len(notes)), np.ones(len(notes)), slope_max=2.0)
    assert flat(21) == 0.0 and flat(108) == 0.0


def test_a_rising_trend_never_turns_down_beyond_the_keys():
    """The loss past its measured keys: the edge's slope carried on turned down for an octave
    (Grand F's and D's top keys rang about twice as long as recorded); rising, it holds instead,
    and the bass is as without."""
    notes = np.arange(40, 81)
    hump = -0.05 * (notes - 70.0) ** 2              # falls toward both ends
    at = scale._smooth(notes, hump, np.ones(len(notes)), rising=True)
    plain = scale._smooth(notes, hump, np.ones(len(notes)))
    assert abs(at(90) - at(80)) < 1e-9 and at(30) == plain(30) and at(60) == plain(60)
    up = scale._smooth(notes, 0.1 * notes, np.ones(len(notes)), rising=True)
    assert abs((up(90) - up(80)) - 1.0) < 1e-6


def test_a_top_key_with_few_decays_takes_b1_from_them():
    """b1' from a key's few partial decays with b3 given: the confidence-weighted mean of σ − b3·ω²."""
    w = np.array([2 * np.pi * 4000.0, 2 * np.pi * 8000.0])
    b3 = 1e-8
    sig = 3.0 + b3 * w ** 2
    assert abs(scale.edge_b1(w, sig, np.array([0.9, 0.4]), b3) - 3.0) < 1e-9
    assert scale.edge_b1(w, b3 * w ** 2 - 1.0, np.ones(2), b3) == 0.05   # never below the floor
