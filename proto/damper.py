"""Dampers: extra decay rate of each string partial while the damper rests on the string.

Measured from a set's release recordings (analysis/damper_decay.py): per partial the first
fall after the key's release, the damper's own (the room rings on after it). On the grand
lent as `dampers` (Grand E's release recordings) 80–120 dB/s, slower in the bass; the uprights
borrow it too (Upright A's takes hold their level 0.2 s before they fall, read as 20–40 dB/s:
heard as no damper). Smoothed over (note, log frequency) with a Gaussian kernel (weights:
confidence). Strings above LAST_DAMPED_NOTE have no dampers; a plucked instrument's jacks
carry a damper on every key (the same loss curve, held into the top octaves by the kernel).
The sets without release recordings borrow a lender's ([uses] dampers).
"""
import json
import os
from functools import lru_cache

import numpy as np
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analysis"))
import project as P  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
DB_PER_NEPER = 20 / np.log(10)
LAST_DAMPED_NOTE = 88
WIDTH_NOTES = 6.0
WIDTH_OCT = 0.5


@lru_cache(maxsize=4)
def _data(pid):
    """The damper decays the part plays: its own measurement, or the one it borrows ([uses] dampers)."""
    d = P.load_cache(P.provider(pid, "dampers"), "damper")
    cols = d["columns"]
    r = np.array([[row[cols.index(c)] for c in ("note", "freq", "fall_db_s", "conf", "range_db")]
                  for row in d["rows"]], float)
    r = r[r[:, 2] > 0]
    return r[:, 0], np.log2(r[:, 1]), np.log(r[:, 2]), r[:, 3]


# the harp has no dampers: note-off is the player's hand on the string, flat over frequency
# and slower than a felt damper's first drop (assumed: 60 dB in about 0.5 s)
HAND_DB_S = 120.0


def hand_sigma(freqs):
    """Decay rate (1/s, amplitude) of partials at freqs (Hz) under the harpist's hand."""
    return np.full(len(np.atleast_1d(freqs)), HAND_DB_S / DB_PER_NEPER)


def has_damper(note, plucked=False):
    return plucked or note <= LAST_DAMPED_NOTE


def sigma(note, freqs, part, plucked=False):
    """Extra decay rate (1/s, amplitude) of partials at freqs (Hz) of a damped string;
    0 for notes without dampers."""
    freqs = np.atleast_1d(np.asarray(freqs, float))
    if not has_damper(note, plucked):
        return np.zeros(len(freqs))
    n, lf, ld, c = _data(part)
    out = np.empty(len(freqs))
    for i, f in enumerate(np.log2(np.maximum(freqs, 20.0))):
        w = c * np.exp(-0.5 * (((n - note) / WIDTH_NOTES) ** 2 + ((lf - f) / WIDTH_OCT) ** 2))
        if w.sum() < 1e-6:   # far from all data (e.g. very high partials): nearest-note median
            near = np.abs(n - note) <= 3
            out[i] = np.exp(np.median(ld[near] if near.any() else ld))
        else:
            out[i] = np.exp((w * ld).sum() / w.sum())   # geometric mean, dB/s
    return out / DB_PER_NEPER
