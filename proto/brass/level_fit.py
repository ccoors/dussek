"""The level at each air (usage: BRASS=<name> level_fit.py): a model's level grows less with the air than a recording's, whose layers keep
their levels; and a note's table is chosen by its level, each table's filter and slide its own, so the level steps where the choice
changes (up to 5 dB). Per mute and key: each table's step closed, then one output level at each of the air's anchors: the recording's range from
pp to ff, mf in the middle of it in dB (even per velocity; the layers' names are not levels), half of the recording's rise over the keys (a
keyboard player's even instrument, as with the mutes' levels), the top TOP_DB over ff. A mute keeps its median level against open, at most MUTE_TOP over it with the makeup (live.MAKEUP): a big bell's mute has its resonance under the note's strong harmonics, and no mute makes a horn much louder. It changes
no note's playing. Writes inst/<name>/rt_level.json, read by the live player."""
import multiprocessing
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import level_check, live, rt_tables, sim, spec

SR = sim.SR
AIRS = tuple(127 * x for x in live.LEVEL_X)
TOP_DB, KEY_SHARE, MID, MUTE_TOP = 3.0, 0.5, 0.5, 3.0
SWITCH = (49.0, 51.0, 93.5, 95.5)  # the air either side of where the table changes: pp to mf, mf to ff


def one(a):
    key, air, mute = a
    x = live.render_events([(0.0, "cc", 11, air), (0.05, "on", key, 90), (1.3, "off", key)], 1.4, mute, virtual=False)
    return a, float(20 * np.log10(np.std(x[int(0.6 * SR):int(1.2 * SR)]) + 1e-12))


def smooth(xs, ys, keys, span=4.0):
    """Between the sampled notes, eased over a span of semitones."""
    k = np.arange(keys[0], keys[-1] + 1)
    y = np.interp(k, xs, ys)
    w = np.exp(-0.5 * (np.subtract.outer(k, k) / span) ** 2)
    return dict(zip(k.tolist(), ((w @ y) / w.sum(axis=1)).tolist()))


def wanted(keys):
    """Per key, the level at each anchor, re the instrument's median ff."""
    r = level_check.recording()
    ks = sorted(r)
    lo, hi = min(min(ks), keys[0]), max(max(ks), keys[-1])
    layer = lambda d: smooth([k for k in ks if d in r[k]], [r[k][d] for k in ks if d in r[k]], [lo, hi])
    ff, pp = layer("ff"), layer("pp")
    mid = float(np.median([ff[k] for k in keys]))
    even = {k: KEY_SHARE * (ff[k] - mid) for k in keys}
    return np.array([[even[k] - (ff[k] - pp[k]), even[k] - (1 - MID) * (ff[k] - pp[k]), even[k], even[k] + TOP_DB] for k in keys])


def fit(mute, want, opened=None):
    """Returns the mute's table and its levels at the anchors with its steps closed."""
    keys = rt_tables.TABLE
    rt_tables.load(mute)
    live.LEVEL.clear()
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        res = dict(ex.map(one, [(k, a, mute) for k in keys for a in SWITCH], chunksize=2))
    steps = [[res[(k, SWITCH[1], mute)] - res[(k, SWITCH[0], mute)], res[(k, SWITCH[3], mute)] - res[(k, SWITCH[2], mute)]] for k in keys]
    trims = {}  # up to the middle table's level: pp by the step, ff less it
    for k, (a, b) in zip(keys, steps):
        trims[f"{k}_pp"], trims[f"{k}_ff"] = a, -b
    live.LEVEL.update({"open": {**{str(k): [0.0] * 4 for k in keys}, **trims}})
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        res = dict(ex.map(one, [(k, a, mute) for k in keys for a in AIRS], chunksize=2))
    M = np.array([[res[(k, a, mute)] for a in AIRS] for k in keys])
    if opened is None:  # the open horn's loudness at mf stays where it was
        G = want - M
        G -= np.median(G[:, 1])
    else:
        over = float(np.median(M - opened["M"]))
        print(f"{spec.NAME} {mute}: {over:.1f} dB against open in the model, {live.MAKEUP[mute]:.1f} made up", flush=True)
        G = want + opened["at"] + min(over, MUTE_TOP - live.MAKEUP[mute]) - M
    print(f"{spec.NAME} {mute}: the tables' steps, largest", np.round(np.max(np.abs(steps), axis=0), 1).tolist(), "median", np.round(np.median(np.abs(steps), axis=0), 1).tolist(),
          "| model pp mf ff top re ff", np.round(np.median(M - M[:, 2:3], axis=0), 1).tolist(), "wanted", np.round(np.median(want - want[:, 2:3], axis=0), 1).tolist(),
          "| gain", np.round(G.min(axis=0)).astype(int).tolist(), np.round(G.max(axis=0)).astype(int).tolist(), flush=True)
    t = {str(k): np.round(g, 2).tolist() for k, g in zip(keys, G)}
    t.update({k: round(v, 2) for k, v in trims.items()})
    return t, dict(M=M, at=(M + G - want))


if __name__ == "__main__":
    want = wanted(rt_tables.TABLE)
    out = {}
    out["open"], opened = fit("open", want)
    for k in spec.S["mutes"]:  # the plunger plays as open, its hand on what leaves
        if k == "plunger":
            continue
        out[k], _ = fit(k, want, opened)
    spec.save("level", out)
