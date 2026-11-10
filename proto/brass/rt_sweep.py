"""Every note at every loudness through the live player, tongued and slurred, with and without the virtual
player: which land? usage: rt_sweep.py [mute]"""
import multiprocessing, sys
import numpy as np
import live, midi_check, rt_tables, sim
from concurrent.futures import ProcessPoolExecutor

SR = sim.SR
KINDS = ("tongue", "up", "down", "up05", "down05", "up07", "down07", "up12", "down12")


def one(a):
    m, v, kind, mute, virt = a
    if kind == "tongue":
        ev, on = [(0.05, "on", m, v), (0.95, "off", m)], 0.05
    else:
        iv = int(kind[-2:]) if kind[-1].isdigit() else 2
        o = m + (iv if kind.startswith("down") else -iv)
        if o < rt_tables.TABLE[0] or o > rt_tables.TABLE[-1]:
            return (m, v, kind, virt), 0
        ev, on = [(0.05, "on", o, v), (0.5, "on", m, v), (0.52, "off", o), (1.15, "off", m)], 0.5
    x = live.render_events(ev, 1.3, mute, virtual=virt)
    a0, b0 = int((on + 0.2) * SR), int((on + 0.55) * SR)
    f = 440 * 2 ** ((m - 69) / 12)
    p, _ = midi_check.pitch_near(x[a0:b0], f)
    return (m, v, kind, virt), round(1200 * np.log2(p / f)) if p == p else 9999


if __name__ == "__main__":
    mute = sys.argv[1] if len(sys.argv) > 1 else "open"
    rt_tables.load(mute)  # once, before the workers fork
    jobs = [(m, v, k, mute, virt) for m in rt_tables.TABLE for v in (30, 64, 100, 127) for k in KINDS for virt in (False, True)]
    with ProcessPoolExecutor(8, mp_context=multiprocessing.get_context("fork")) as ex:
        res = dict(ex.map(one, jobs))
    for virt in (False, True):
        off = {k: v for k, v in res.items() if k[3] == virt and abs(v) > 40}
        print(f"{mute} virtual={virt}: {sum(k[3] == virt for k in res)} cases, {len(off)} off", sorted((k[0], k[1], k[2], v) for k, v in off.items())[:30], {x: sum(k[2] == x for k in off) for x in KINDS})
