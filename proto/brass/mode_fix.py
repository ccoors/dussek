"""A slide that lost its note (usage: BRASS=<name> mode_fix.py [apply|lips] [mute]): the retune moves a note's slide by what it plays off. When
the note had locked to the next mode over, that walks the slide away: the note then sits between two modes, where nothing holds
it. Lists the keys and dynamics whose nearest mode is over 50 cents from the note; apply puts them where their neighbors are; lips
then tunes what plays off by the lips' setting, over the live player (rt_cells.json)."""
import os, sys
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import live, midi_check, rt_tables, spec

LIMIT = float(os.environ.get("LIMIT", 50.0))
DYNS = ("pp", "mf", "ff")
MUTE = next((a for a in sys.argv[1:] if a not in ("apply", "lips")), "open") if __name__ == "__main__" else "open"


def offsets(mute="open"):
    tb = rt_tables.load(mute)
    out = {}
    for k in rt_tables.TABLE:
        n = tb["notes"][k]
        for d in DYNS:
            c = 1200 * np.log2(np.abs(n["modes"][d][0].imag) / (2 * np.pi) / n["f"])
            out[(k, d)] = float(c[np.argmin(np.abs(c))])
    return out


AIR = {"pp": 40, "mf": 75, "ff": 114}
DCS = (0.0, -0.05, -0.025, -0.0125, 0.0125, 0.025, 0.05)


def played(job):
    """What a key plays at this lip setting: cents off, and the share of the held note on it."""
    k, d, dc, mute = job
    o = rt_tables.CELLS.get(f"{mute}_{k}_{d}")
    rt_tables.CELLS[f"{mute}_{k}_{d}"] = ((o or (0.0, 1.0))[0] + dc, (o or (0.0, 1.0))[1])
    x = live.render_events([(0, "cc", 11, AIR[d]), (0.02, "on", k, 90), (1.3, "off", k)], 1.3, mute, False, 1)
    if o is None:  # a worker plays many trials: each from the setting as it stands
        del rt_tables.CELLS[f"{mute}_{k}_{d}"]
    else:
        rt_tables.CELLS[f"{mute}_{k}_{d}"] = o
    f = 440 * 2 ** ((k - 69) / 12)
    w = int(0.04 * live.SR)
    cs = []
    for i in range(int(0.4 * live.SR), int(1.25 * live.SR), w):
        p, _ = midi_check.pitch_near(x[i:i + w], f)
        cs.append(1200 * np.log2(p / f) if p == p else 999.0)
    return k, d, dc, float(np.median(cs)), float(np.mean(np.abs(np.array(cs) - np.median(cs)) < 25))


def lips(mute="open"):
    rt_tables.load(mute)  # once, before the workers
    jobs = [(k, d, 0.0, mute) for k in rt_tables.TABLE for d in DYNS]
    with ProcessPoolExecutor() as ex:
        first = list(ex.map(played, jobs))
        off = [(k, d) for k, d, _, c, _ in first if abs(c) > 15]
        rows = first + list(ex.map(played, [(k, d, dc, mute) for k, d in off for dc in DCS[1:]]))
    print(spec.NAME, mute, len(off), "of", len(first), "over 15 cents; before:", round(float(np.percentile(np.abs([r[3] for r in first]), 95))), "cents at the 95th percentile")
    cells = dict(rt_tables.CELLS)
    left = 0
    for k, d in off:
        c = [r for r in rows if r[0] == k and r[1] == d and r[4] > 0.8] or [r for r in rows if r[0] == k and r[1] == d]
        b = min(c, key=lambda r: abs(r[3]))
        left += abs(b[3]) > 15
        o = cells.get(f"{mute}_{k}_{d}", (0.0, 1.0))
        cells[f"{mute}_{k}_{d}"] = [o[0] + b[2], o[1]]
    spec.save("cells", cells)
    print(" still over 15 cents:", left)


if __name__ == "__main__":
    if "lips" in sys.argv:
        lips(MUTE)
        sys.exit()
    off = offsets(MUTE)
    bad = {a: c for a, c in off.items() if abs(c) > LIMIT}
    print(spec.NAME, MUTE, len(bad), "of", len(off), "over", LIMIT, "cents:", {f"{k}_{d}": round(c) for (k, d), c in bad.items()})
    if "apply" in sys.argv:
        key, pre = ("tuning", "") if MUTE == "open" else ("tuning_mutes", MUTE + "_")
        tune = spec.load(key)
        for (k, d), c in bad.items():
            near = [off[(q, d)] for q in rt_tables.TABLE if (q, d) not in bad and abs(q - k) <= 2]
            want = float(np.median(near)) if near else 15.0
            tune[f"{pre}{k}_{d}"] = tune.get(f"{pre}{k}_{d}", 0.0) + (c - want)
        spec.save(key, tune)
