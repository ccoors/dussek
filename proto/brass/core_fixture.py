#!/usr/bin/env python3
"""The fixture engine/tests/brass_core.rs holds the Rust core to: a synthetic note through rt.c. Numbers from a formula,
no recording. Usage (from anywhere): BRASS_PROJECT=horn core_fixture.py <out.json>"""
import ctypes
import json
import math
import sys

import numpy as np

import live

CTRL, BLOCKS = live.CTRL, 64
OS = live.OS
D = ctypes.POINTER(ctypes.c_double)


def noise_at(n):
    """A rich, repeatable stream, rounded to f32 as an export's is."""
    i = np.arange(n, dtype=float)
    return np.asarray(0.015 * np.sin(i * 0.37) * np.cos(i * 0.011) + 0.01 * np.sin(i * i * 1e-5), dtype=np.float32)


def filt(n, seed, decay):
    i = np.arange(n, dtype=float)
    return np.asarray(np.sin(i * (0.3 + 0.1 * seed)) * np.exp(-i / decay) / (1 + seed), dtype=np.float32)


def modes(f0, m):
    """Poles and residues as rt_tables.modes_of gives them, at the oversampled rate with E and r per sample."""
    w = 2 * np.pi * f0 * np.arange(1, m + 1)
    Q = 30.0 + 2.0 * np.arange(m)
    Z = 2e7 / (1 + 0.3 * np.arange(m))
    sp = -w / (2 * Q) + 1j * w * np.sqrt(1 - 1 / (4 * Q * Q))
    rc = Z * (w / Q) * sp / (sp - np.conj(sp))
    return sp, rc


def main(out):
    lib = live.lib()
    state = lib.rt_new()
    sr, T = 44100.0, 1 / (44100.0 * OS)
    sp, rc = modes(220.0, 15)
    E = np.exp(sp * T)
    r = rc * T / 2
    lr, lq, ls, lc = np.array([1.0, 1.1]), np.array([5.0, 5.0]), np.array([1.0, -1.0]), np.array([1.0, 0.5])
    params = dict(width=0.011, y0=0.00015, soft=1e-4, rho=1.2, sr=sr, nl=0.25, mu=1.5)
    noise = noise_at(4096 * OS + 1).astype(np.float64)
    dec = np.sin(np.arange(1, 97) * np.pi / 97) ** 2
    dec = dec / dec.sum()
    filters = [(filt(256, 0, 40.0), filt(64, 0, 10.0)), (filt(256, 1, 60.0), filt(64, 1, 12.0)), (filt(200, 2, 50.0), filt(48, 2, 9.0))]
    keep = [(a.astype(np.float64), b.astype(np.float64)) for a, b in filters]
    def to(c, i):
        h, rr = keep[i]
        lib.rt_filters(state, h.ctypes.data_as(D), len(h), rr.ctypes.data_as(D), len(rr))
    to(0, 0)
    change = {16: 1, 18: 2, 40: 0}
    y, ctl = [], []
    pm, lip, burst = 0.0, 200.0, 1.0
    for b in range(BLOCKS):
        if b in change:
            to(None, change[b])
        pm1 = 2500.0 * min(1.0, (b + 1) / 12) * (1 + 0.1 * math.sin(b * 0.4))
        lip1 = 220.0 * (1 + 0.01 * math.sin(b * 0.2))
        burst1 = 1.0 + 5.0 * math.exp(-b / 10)
        kick = 0.5 if b in (0, 30) else 0.0
        o = np.zeros(CTRL)
        lib.rt_process(state, CTRL, pm, pm1, lip, lip1, burst, burst1, kick, params["mu"], len(E), live.ptr(E.real.copy()), live.ptr(E.imag.copy()), live.ptr(r.real.copy()),
                       live.ptr(r.imag.copy()), 2, live.ptr(lr), live.ptr(lq), live.ptr(ls), live.ptr(lc), params["width"], params["y0"], params["soft"], params["rho"], sr,
                       live.ptr(noise), len(noise), params["nl"], live.ptr(dec), len(dec), live.ptr(o))
        ctl.append([pm, pm1, lip, lip1, burst, burst1, kick])
        pm, lip, burst = pm1, lip1, burst1
        y.append(o)
    json.dump({"params": params, "E": [E.real.tolist(), E.imag.tolist()], "r": [r.real.tolist(), r.imag.tolist()],
               "lips": [lr.tolist(), lq.tolist(), ls.tolist(), lc.tolist()], "dec": dec.tolist(),
               "filters": [[a.tolist(), b.tolist()] for a, b in keep], "change": {str(k): v for k, v in change.items()},
               "ctl": ctl, "noise_n": len(noise), "out": np.concatenate(y).tolist()}, open(out, "w"))


if __name__ == "__main__":
    main(sys.argv[1])
