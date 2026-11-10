"""The cases that land off their note (usage: BRASS=<name> land_fix.py [mute ...]): the holding repair looks at a held note, the tuning at
its pitch; neither sees a slur that catches the next mode. Per key and dynamics with a case off, a search over the lips and the air: the fewest
cases off, then the fewest air levels not holding, then the note's pitch, then the least change. Writes rt_cells.json."""
import multiprocessing, sys
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import auto_player, compare5, rt_stability, rt_sweep, rt_tables, spec

VELS = (30, 64, 100, 127)
DCS = (-0.2, -0.15, -0.1, -0.075, -0.05, -0.025, 0.0, 0.025, 0.05)
AIRS = (0.8, 1.0, 1.25)
DYNS = ("pp", "mf", "ff")


def band(v, m):
    x = max(0.0, min(1.0, (min(127.0, auto_player.level_of(v, m)) / 127 - 0.2) / 0.7))
    return min(compare5.DYN, key=lambda k: abs(compare5.DYN[k] - x))


def attempt(a):
    mute, key, dyn, dc, air = a
    with spec.trying(rt_tables.CELLS, f"{mute}_{key}_{dyn}", (dc, air)):
        return measure(a)


def measure(a):
    mute, key, dyn, dc, air = a
    off, cents = 0, []
    for v in VELS:
        if band(v, key) != dyn:
            continue
        for k in rt_sweep.KINDS:
            _, c = rt_sweep.one((key, v, k, mute, True))
            if c != 0 or k == "tongue":
                off += abs(c) > 40
                cents.append(abs(c) if abs(c) < 900 else 900)
    loose = 0
    for lv in rt_stability.BANDS[dyn]:
        _, r = rt_stability.one((key, lv, 110, mute, False))
        loose += not rt_stability.stable(r, lv)
    return a, off, loose, float(np.median(cents)) if cents else 0.0


def fix(mute):
    rt_tables.load(mute)
    jobs = [(m, v, k, mute, True) for m in rt_tables.TABLE for v in VELS for k in rt_sweep.KINDS]
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        res = dict(ex.map(rt_sweep.one, jobs, chunksize=6))
    cells = sorted({(k[0], band(k[1], k[0])) for k, c in res.items() if abs(c) > 40})
    print(f"{spec.NAME} {mute}: {sum(abs(c) > 40 for c in res.values())} of {len(res)} off, in {cells}", flush=True)
    jobs = [(mute, key, dyn, dc, air) for key, dyn in cells for dc in DCS for air in AIRS]
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        tried = list(ex.map(attempt, jobs, chunksize=2))
    for key, dyn in cells:
        rows = [r for r in tried if r[0][1] == key and r[0][2] == dyn]
        a, off, loose, c = min(rows, key=lambda r: (r[1], r[2], round(r[3] / 15), abs(r[0][3]) + abs(np.log(r[0][4]))))
        rt_tables.CELLS[f"{mute}_{key}_{dyn}"] = (a[3], a[4])
        print(f"  {key} {dyn}: lips {a[3]:+.3f}, air x{a[4]}: {off} off, {loose} not holding, {c:.0f} cents", flush=True)
    all_cells = spec.load("cells", {})
    all_cells.update({k: v for k, v in rt_tables.CELLS.items() if k.startswith(mute + "_")})
    spec.save("cells", all_cells)


if __name__ == "__main__":
    for m in sys.argv[1:] or ["open"]:
        fix(m)
