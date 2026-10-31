"""Even levels: a piano voiced to one level curve along the keyboard (design 2026-10-21-even-loudness).

Each key's partials pass through the board's response and the seeded body; where they meet a peak
or a notch the key comes out louder or quieter (Grand A at mf: 2–2.6 dB rms around a smooth trend,
B7 10 dB under its neighbors). A voicer evens that out at the hammers. Per dynamic, the keys'
levels as the engine plays them (instrument.json's `level`: each key's own ff level and its row over the hammer
speed), a trend through the ff levels that keeps the keyboard's slow shape, and per key the trend
minus its level, plus a seeded scatter (a hammer's voicing acts on the whole key): added to the
key's `level_db` correction at every dynamic, a shift of its level (the velocity follows each key's
own ff, so its speeds and tone stay).
"""
import numpy as np
from scipy.ndimage import median_filter

DYNAMICS = ("pp", "mf", "ff")
DEFAULTS = {"window_keys": 9, "scatter_db": 0.0, "seed": 1, "target_keys": None, "target_db": None}
KEYS = range(21, 109)


def settings(table):
    """DEFAULTS updated by a project's [even_levels]; ValueError naming a bad key or value."""
    out = dict(DEFAULTS)
    for k, v in (table or {}).items():
        if k not in DEFAULTS:
            raise ValueError(f"[even_levels]: unknown key {k!r} (known: {', '.join(DEFAULTS)})")
        out[k] = v
    w, s, seed = out["window_keys"], out["scatter_db"], out["seed"]
    if isinstance(w, bool) or not isinstance(w, int) or w < 3 or w % 2 == 0:
        raise ValueError(f"[even_levels]: window_keys must be an odd number of keys ≥ 3, not {w!r}")
    if isinstance(s, bool) or not isinstance(s, (int, float)) or not s >= 0:
        raise ValueError(f"[even_levels]: scatter_db must be a number ≥ 0, not {s!r}")
    if isinstance(seed, bool) or not isinstance(seed, int):
        raise ValueError(f"[even_levels]: seed must be an integer, not {seed!r}")
    tk, td = out["target_keys"], out["target_db"]
    if tk is not None or td is not None:
        ok = (isinstance(tk, list) and isinstance(td, list) and len(tk) == len(td) >= 2
              and all(isinstance(k, int) for k in tk) and all(b > a for a, b in zip(tk, tk[1:]))
              and all(isinstance(v, (int, float)) and not isinstance(v, bool) for v in td))
        if not ok:
            raise ValueError("[even_levels]: target_keys and target_db go together: rising keys, a value (dB) each")
    return out


def key_levels(level_doc, speeds):
    """Per dynamic the 88 keys' levels (dB): each key's own ff level plus its row of the level table at
    that dynamic's anchor speed (linear in ln speed). `speeds`: the export's velocity speeds, pp mf ff."""
    if "key_ff_db" not in level_doc or "speed_db" not in level_doc:
        raise ValueError("the level needs key_ff_db and speed_db (a hammer piano's, from the engine's level tool)")
    ff = np.asarray(level_doc["key_ff_db"], float)
    tab = level_doc["speed_db"]
    ln = np.log(np.asarray(tab["speed"], float))
    rows = np.asarray(tab["db"], float)
    return {d: ff + np.array([np.interp(np.log(s), ln, r) for r in rows]) for d, s in zip(DYNAMICS, speeds)}


def _extend(x, n):
    """`x` with n keys added at each end on the straight line fitted to its outer n keys (a slope
    kept, an alternation averaged out: a held end value bends a sloped keyboard)."""
    def line(y):
        # once refitted without the keys far off it (a notch, a peak), as the trend leaves them out
        i = np.arange(n)
        c = np.polyfit(i, y, 1)
        r = y - np.polyval(c, i)
        mad = 1.4826 * np.median(np.abs(r - np.median(r)))
        keep = np.abs(r) <= 2.5 * mad if mad > 0 else np.ones(n, bool)
        return np.polyfit(i[keep], y[keep], 1) if keep.sum() >= 2 else c
    lo, hi = line(x[:n]), line(x[-n:])
    return np.concatenate([np.polyval(lo, np.arange(-n, 0)), x, np.polyval(hi, np.arange(n, 2 * n))])


def trend(levels, window_keys):
    """The keyboard's slow shape: a running median over `window_keys` keys, then one ¼ ½ ¼ pass, the
    ends extended on the outer keys' straight line."""
    x = np.asarray(levels, float)
    m = median_filter(_extend(x, window_keys), size=window_keys, mode="nearest")
    return np.convolve(m, [0.25, 0.5, 0.25], mode="same")[window_keys:window_keys + len(x)]


def corrections(level_doc, speeds, table):
    """88 rows [pp, mf, ff] (dB): the ff trend (or the piano's median plus a target shape,
    target_keys/target_db) minus each key's ff level, plus its scatter draw, the same at every dynamic. A piano with a level table plays each velocity at the speed where the key
    sounds a set level under its own ff (Engine::hammer_speed): the ff correction moves the key's
    level at every velocity; a different pp or mf one would only move its speed there, its tone."""
    st = settings(table)
    ff = key_levels(level_doc, speeds)["ff"]
    scatter = np.random.default_rng(st["seed"]).normal(0.0, st["scatter_db"], len(KEYS)) if st["scatter_db"] else np.zeros(len(KEYS))
    if st["target_keys"]:   # a shape set from outside (the register of other pianos), re the median
        aim = np.median(ff) + np.interp(np.array(KEYS), st["target_keys"], st["target_db"])
    else:                   # the piano's own slow shape
        aim = trend(ff, st["window_keys"])
    c = aim - ff + scatter
    return np.column_stack([c] * len(DYNAMICS)).tolist()


def merge(level_db, row):
    """A note's corrections (one [pp, mf, ff] per mallet; None: none) with `row` added to each."""
    return [[float(a + b) for a, b in zip(m, row)] for m in (level_db or [[0.0, 0.0, 0.0]])]
