import numpy as np

import export as E


def peaks(f):
    return {n: {"pp": 0.1 * f(n), "mf": 0.5 * f(n), "ff": f(n)} for n in range(21, 109)}


def test_the_calibration_rests_on_the_recorded_keys():
    # the engine levels an instrument by the median ff peak over all 88 keys; a harpsichord
    # recorded D1–C6 (the French) has 24 extrapolated, quiet treble keys below that median:
    # its real notes came out ~6 dB too loud
    p = peaks(lambda n: 10 ** ((60 - n) / 40) if n <= 84 else 10 ** ((60 - n) / 15))
    g = E.calibration_gain_db(p, (26, 84))
    # engine: median over the 88 keys (sorted element 44) → the recorded keys' median
    all_med = sorted(v["ff"] for v in p.values())[44]
    rec_med = sorted(p[n]["ff"] for n in range(26, 85))[(85 - 26) // 2]
    assert abs(g - 20 * np.log10(all_med / rec_med)) < 1e-9 and g < -0.5
    assert abs(E.calibration_gain_db(peaks(lambda n: 1.0), (21, 108))) < 1e-12
