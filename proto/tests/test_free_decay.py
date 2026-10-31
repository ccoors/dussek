import numpy as np

import string_model as M


def reference(lam, weight, n_free, bounds, sig_d, dt):
    """The free decay summed with an exp per mode and sample (the former render loop)."""
    out = np.zeros(n_free)
    wgt = weight.copy()
    for (b0, on), (b1, _) in zip(bounds[:-1], bounds[1:]):
        lam_seg = lam - (sig_d[:, None] if on else 0)
        m = np.arange(b1 - b0)
        out[b0:b1] = (wgt.ravel()[:, None] * np.exp((lam_seg * dt).ravel()[:, None] * m[None, :])).sum(axis=0).real
        wgt = wgt * np.exp(lam_seg * dt * (b1 - b0))
    return out


def test_free_decay_matches_the_exp_per_sample():
    rng = np.random.default_rng(3)
    K, S, dt = 40, 3, 1 / 48000
    w = 2 * np.pi * rng.uniform(100, 15000, (K, S))
    lam = -rng.uniform(0.2, 30, (K, S)) + 1j * w
    weight = rng.standard_normal((K, S)) + 1j * rng.standard_normal((K, S))
    sig_d = rng.uniform(5, 50, K)
    n = 10000
    for bounds in ([(0, False), (n, False)], [(0, False), (3001, True), (7777, False), (n, False)]):
        ref = reference(lam, weight, n, bounds, sig_d, dt)
        got = M.free_decay(lam, weight, n, bounds, sig_d, dt)
        assert np.max(np.abs(got - ref)) < 1e-9 * np.max(np.abs(ref))
