"""How loud is a key for how hard it is struck? (usage: BRASS=<name> level_check.py [mute]): through the live player, with the virtual player,
each key's sustain at velocities over the keyboard's range, against the steps between the recording's layers. Prints the level by
register, the step per 10 of velocity, and the worst jump; writes out/level_<name>_<mute>.json."""
import json, multiprocessing, os, sys
import numpy as np
import soundfile as sf
from concurrent.futures import ProcessPoolExecutor
import live, rt_tables, sim, spec

HERE = os.path.dirname(os.path.abspath(__file__))
SR = sim.SR
VEL = (20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 127)
NAMES = "C C# D D# E F F# G G# A A# B".split()



def one(a):
    key, v, mute = a
    x = live.render_events([(0.05, "on", key, v), (1.3, "off", key)], 1.4, mute, virtual=True)
    s = x[int(0.6 * SR):int(1.2 * SR)]
    return a, float(20 * np.log10(np.std(s) + 1e-12))


def recording():
    """{key: {layer: level}}: VSCO's layers keep their levels, and Iowa's."""
    return {int(k): {d: c["level"] for d, c in v.items()} for k, v in spec.load_cache("recording").items()}


if __name__ == "__main__":
    mute = sys.argv[1] if len(sys.argv) > 1 else "open"
    rt_tables.load(mute)
    keys = rt_tables.TABLE[::2]
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        res = dict(ex.map(one, [(k, v, mute) for k in keys for v in VEL], chunksize=4))
    L = np.array([[res[(k, v, mute)] for v in VEL] for k in keys])
    third = len(keys) // 3
    print(f"{spec.NAME} {mute}: velocity {list(VEL)}")
    for name, rows in (("low", L[:third]), ("middle", L[third:2 * third]), ("high", L[2 * third:])):
        med = np.median(rows, axis=0)
        print(f"  {name:6s} level re v64-ish {np.round(med - med[4], 1).tolist()}")
        print(f"  {name:6s} dB per 10 of velocity {np.round(np.diff(med) / np.diff(VEL) * 10, 1).tolist()}")
    d = np.diff(L, axis=1) / np.diff(VEL) * 10
    w = np.unravel_index(np.argmax(d), d.shape)
    print(f"  worst slope {d[w]:.1f} dB per 10 at key {keys[w[0]]}, {VEL[w[1]]} to {VEL[w[1] + 1]}; any drops over 1 dB: {[(keys[i], VEL[j]) for i, j in zip(*np.nonzero(np.diff(L, axis=1) < -1))][:12]}")
    print(f"  range v20 to v127 per key: median {np.median(L[:, -1] - L[:, 0]):.1f} dB, min {np.min(L[:, -1] - L[:, 0]):.1f}, max {np.max(L[:, -1] - L[:, 0]):.1f}")
    print("  v80 over the keys:", dict(zip(keys[::3], np.round(L[::3, 6] - np.median(L[:, 6])).astype(int).tolist())))
    print("  v127 over the keys:", dict(zip(keys[::3], np.round(L[::3, -1] - np.median(L[:, -1])).astype(int).tolist())))
    r = recording()
    ks = sorted(k for k in r if "ff" in r[k] and "pp" in r[k])
    print("  recording, pp to ff by key:", {k: round(r[k]["ff"] - r[k]["pp"], 1) for k in ks}, "median", round(float(np.median([r[k]["ff"] - r[k]["pp"] for k in ks])), 1))
    if all("mf" in r[k] for k in ks):
        print("  recording, pp to mf, mf to ff, medians:", round(float(np.median([r[k]["mf"] - r[k]["pp"] for k in ks if "mf" in r[k]])), 1), round(float(np.median([r[k]["ff"] - r[k]["mf"] for k in ks if "mf" in r[k]])), 1))
    print("  recording, ff over the keys:", {k: round(r[k]["ff"] - float(np.median([r[q]["ff"] for q in ks])), 0) for k in ks[::3]})
    json.dump({"vel": VEL, "keys": keys, "level": L.tolist()}, open(os.path.join(spec.OUT, f"level_{spec.NAME}_{mute}.json"), "w"))
