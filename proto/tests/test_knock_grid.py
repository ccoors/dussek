import itertools

import numpy as np

import fit_knock as FK


def synthetic(seed=2):
    rng = np.random.default_rng(seed)
    B = len(FK.BAND_SEL)
    keys = [(n, d) for n in (40, 62, 70, 81, 95, 104) for d in ("mf", "ff")]
    strings = [(k, 0.1, (10 ** rng.uniform(-6, 0, B), None, None)) for k in keys]
    for _, _, (es, _, _) in strings[::4]:
        es[3] = np.nan                                   # a band without a level
    knocks = {(k[0], d, c): (10 ** rng.uniform(-4, 0, B), None, None)
              for k in keys for d in FK.DECAYS for c in FK.CUTOFFS}
    rec = {k: (rng.uniform(-60, 0, B), rng.uniform(-90, -50, B)) for k in keys}
    for k in keys[::3]:
        rec[k][0][5] = np.nan
    return strings, knocks, rec, {"mf": 2.1, "ff": 5.0}


def test_the_vectorized_grid_equals_the_point_errors():
    strings, knocks, rec, speeds = synthetic()
    qs = [0.5, 1.5]
    grid = FK.grid_errors(strings, knocks, rec, speeds, qs)
    for (a, d), (b, c), (i, q), (j, amp), (k, knee) in itertools.product(
            enumerate(FK.DECAYS), enumerate(FK.CUTOFFS), enumerate(qs), list(enumerate(FK.AMPS))[::7], enumerate(FK.KNEES)):
        e = FK.knock_error(strings, knocks, rec, speeds, d, c, q, amp, knee)
        assert abs(grid[a, b, i, j, k] - e) < 1e-9 * max(e, 1.0)


def test_an_overshoot_costs_more_than_a_shortfall():
    # over the recording in a band is heard as a click; under it leaves a band thin
    rec = np.array([-20.0, -20.0])
    noise = np.full(2, -90.0)
    over = FK.frame_error(np.array([-10.0, -20.0]), rec, noise, FK.OVER_WEIGHT)
    under = FK.frame_error(np.array([-30.0, -20.0]), rec, noise, FK.OVER_WEIGHT)
    assert over[0] == FK.OVER_WEIGHT * 10.0 and under[0] == -10.0
    assert FK.OVER_WEIGHT >= 2


def test_the_low_band_peak_is_the_transient_re_the_note():
    import attack_noise as AN
    sr = 48000
    t = np.arange(int(0.5 * sr)) / sr
    # C6, no partial in the band, rising over 3 ms as a hammer's contact (an instant step is a click
    # in every band); a 400 Hz thump at a tenth of its level
    note = np.sin(2 * np.pi * 1047 * t) * np.sin(0.5 * np.pi * np.clip(t / 0.003, 0, 1)) ** 2
    burst = 0.1 * np.sin(2 * np.pi * 400 * t) * np.sin(np.pi * np.clip(t / 0.01, 0, 1)) ** 2
    x = np.zeros(int(0.6 * sr))
    x[int(0.1 * sr):] = note + burst
    got = AN.low_peak_db(x, sr, 0.1)
    assert abs(got - 20 * np.log10(0.1)) < 1.5, got
    # the note alone: its fundamental stays out of the band
    x[int(0.1 * sr):] = note
    assert AN.low_peak_db(x, sr, 0.1) < -30


def rise(t):
    """a hammer's contact: 3 ms (an instant step is a click in every band)"""
    return np.sin(0.5 * np.pi * np.clip(t / 0.003, 0, 1)) ** 2


def test_the_level_fit_finds_the_knock_the_recordings_hold():
    # strings and a unit knock per (note, dynamic); the recordings made with a known level and q
    sr = 48000
    t = np.arange(int(0.3 * sr)) / sr
    rng = np.random.default_rng(3)
    k = rng.standard_normal(len(t)) * np.exp(-t / 0.004)
    speeds = {"pp": 0.5, "ff": 5.0}
    cases = {}
    for n in (84, 91, 96):
        for d, v in speeds.items():
            s = v ** 1.2 * np.sin(2 * np.pi * FK.A.midi_to_hz(n) * t) * np.exp(-t / 0.5) * rise(t)
            cases[(n, d)] = (s, k)
    amp, q = 2.0 ** -3, 1.5
    rec = {key: FK.low_peak_of(s + amp * speeds[key[1]] ** q * kk, sr) for key, (s, kk) in cases.items()}
    got_amp, got_q = FK.level_fit(cases, rec, speeds, [1.0, 1.5, 2.0], knee=60, sr=sr)
    assert (got_amp, got_q) == (amp, q)


def test_the_level_fit_errs_under_the_recordings():
    # recordings no single level fits (pp asks for 12 dB more knock than ff, both over the string's
    # own onset in the band, ~-24 dB re its peak): the fit stays nearer
    # the quieter, as the user preferred a knock under the recordings' to one over them
    sr = 48000
    t = np.arange(int(0.3 * sr)) / sr
    k = FK.SB.knock_burst(84, 1.0, 0.004, 500.0, len(t)).mean(axis=0)
    speeds = {"pp": 0.5, "ff": 5.0}
    cases, rec = {}, {}
    for n in (84, 91, 96):
        for d, v in speeds.items():
            s = v * np.sin(2 * np.pi * FK.A.midi_to_hz(n) * t) * np.exp(-t / 0.5) * rise(t)
            cases[(n, d)] = (s, k)
            rec[(n, d)] = FK.low_peak_of(s + (1.0 if d == "pp" else 2.0 ** -2) * v * k, sr)
    even, _ = FK.level_fit(cases, rec, speeds, [1.0], knee=60, sr=sr, over=1.0)
    under, _ = FK.level_fit(cases, rec, speeds, [1.0], knee=60, sr=sr)
    assert under < even


def test_one_note_without_a_transient_does_not_silence_the_knock():
    # the fit compares each dynamic's median over the notes: a single recording with almost no
    # transient (here −60 dB) would otherwise, as an overshoot × 3 squared, pull the knock to nothing
    sr = 48000
    t = np.arange(int(0.3 * sr)) / sr
    k = FK.SB.knock_burst(84, 1.0, 0.004, 500.0, len(t)).mean(axis=0)
    speeds = {"mf": 2.0}
    cases, rec = {}, {}
    for n in (84, 86, 88, 91, 93):
        s = 2.0 * np.sin(2 * np.pi * FK.A.midi_to_hz(n) * t) * np.exp(-t / 0.5) * rise(t)
        cases[(n, "mf")] = (s, k)
        rec[(n, "mf")] = FK.low_peak_of(s + 0.5 * 2.0 * k, sr)
    rec[(93, "mf")] = -60.0
    amp, _ = FK.level_fit(cases, rec, speeds, [1.0], knee=60, sr=sr)
    assert amp == 0.5


def test_the_hammer_pianos_fit_an_impact_the_others_a_knock():
    for design in ("grand", "upright"):
        assert FK.hammered(design)
    for design in ("harpsichord_8", "classical_guitar", "lever_harp", "unfretted_clavichord"):
        assert not FK.hammered(design)
    # the knock keeps its first criterion: the plucked instruments were never heard against another
    assert FK.CUTOFFS == [1000.0, 1500.0, 2000.0, 3000.0] and FK.QS == [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5]
    assert FK.frame_error(np.array([-10.0]), np.array([-20.0]), np.array([-90.0]))[0] == 10.0


def test_the_impact_s_shape_and_level_are_the_user_s_choice():
    # 6 dB under the recordings' peak, softer below ff; the cutoffs reach 300 Hz and open
    assert FK.IMPACT_PREF_DB == -6.0 and FK.IMPACT_GROWTH == 0.5
    assert 500.0 in FK.IMPACT_CUTOFFS and None in FK.IMPACT_CUTOFFS
