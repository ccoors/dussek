import numpy as np

import damper_decay

SR = 48000


def release(key, fall_db_s, tail_db, tail_db_s, dur=1.2, n_partials=8):
    """A damped note as a release recording holds it: partials falling at `fall_db_s` from the
    start, under a slower tail (the room) `tail_db` below them."""
    t = np.arange(int(dur * SR)) / SR
    f0 = 440 * 2 ** ((key - 69) / 12)
    rng = np.random.default_rng(1)
    x = np.zeros_like(t)
    for n in range(1, n_partials + 1):
        a = 10 ** (-6 * np.log2(n) / 20)
        x += a * np.sin(2 * np.pi * n * f0 * t + rng.uniform(0, 2 * np.pi)) * (
            10 ** (-fall_db_s * t / 20) + 10 ** ((tail_db - tail_db_s * t) / 20))
    x += 1e-5 * rng.standard_normal(len(t))
    return np.stack([x, x], axis=1)


def test_the_fall_is_the_damper_not_the_room_after_it():
    # the strings fall at 100 dB/s, the room rings on 25 dB under them at 40 dB/s: the
    # energy-decay slope over 20 dB read 50–60 dB/s, the room's share; the first fall is the damper
    rows = damper_decay.falls(release(60, 100.0, -25.0, 40.0), SR, 60)
    rates = np.array([r["fall_db_s"] for r in rows])
    assert len(rates) >= 5
    assert np.all(np.abs(rates / 100.0 - 1) < 0.15), rates


def test_a_partial_that_never_falls_far_is_left_out():
    # a tail only 8 dB under the strings: no clean first fall to read
    rows = damper_decay.falls(release(60, 100.0, -8.0, 5.0), SR, 60)
    assert all(r["fall_db_s"] > 0 for r in rows)
    assert len(rows) <= 2
