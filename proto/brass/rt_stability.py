"""Does a held note stay a note? Every key, for each air level and strike, through the live player: the pitch over the
sustain (share of frames off the note, spread), period doubling (energy between the harmonics), the level's
swing. usage: rt_stability.py [mute ...]; writes out/stability_<mute>.json and prints the unstable cases."""
import json, multiprocessing, os, sys
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import live, midi_check, rt_tables, sim, spec

HERE = os.path.dirname(os.path.abspath(__file__))
SR = sim.SR
KEYS = range(rt_tables.TABLE[0] - spec.S.get("below", 6), rt_tables.TABLE[-1] + spec.S.get("above", 0) + 1)
BANDS = {"pp": (40,), "mf": (60, 85), "ff": (95, 110, 127)}
AIR = (40, 70, 95, 110, 127)
VEL = (70, 110, 127)


def in_range(by_key):
    """Of a {key: value}, the keys that can be played: the others stay quiet."""
    return {k: v for k, v in by_key.items() if spec.S["low"] <= k <= spec.S["high"]}


def stats(x, f):
    s = x[int(0.5 * SR):int(1.4 * SR)]
    if np.std(s) < 1e-5:
        return dict(off=1.0, spread=999.0, sub=0.0, swing=0.0)
    w = int(0.04 * SR)
    cs = []
    for k in range(0, len(s) - w, int(0.02 * SR)):
        p, _ = midi_check.pitch_near(s[k:k + w], f)
        cs.append(1200 * np.log2(p / f) if p == p else 999.0)
    cs = np.array(cs)
    X = np.abs(np.fft.rfft(s * np.hanning(len(s))))
    F = np.fft.rfftfreq(len(s), 1 / SR)
    around = lambda a: X[(F > a - f * 0.06) & (F < a + f * 0.06)].max()
    harm = max(around(f * k) for k in (1, 2, 3))
    sub = 20 * np.log10(max(around(f * k) for k in (0.5, 1.5, 2.5, 3.5)) / harm)
    e = np.sqrt(np.convolve(s ** 2, np.ones(882) / 882, "valid"))
    swing = 20 * np.log10(np.percentile(e, 98) / max(np.percentile(e, 2), 1e-9))
    return dict(off=float(np.mean(np.abs(cs) > 50)), spread=float(np.percentile(cs, 95) - np.percentile(cs, 5)), sub=float(sub), swing=float(swing))


def stable(r, air=127):
    return r["off"] < 0.1 and r["spread"] < 35 and r["sub"] < -22 and r["swing"] < (9 if air < 50 else 6)


def one(a):
    key, air, vel, mute, virt = a
    x = live.render_events([(0.0, "cc", 11, air), (0.05, "on", key, vel), (1.45, "off", key)], 1.6, mute, virtual=virt)
    return a, stats(x, 440 * 2 ** ((key - 69) / 12))


def swell(a):
    """Into the note soft, the air all the way up and down again: does it stay on the note all along?"""
    key, mute = a
    ev = [(0.0, "cc", 11, 40), (0.05, "on", key, 90)]
    for i in range(61):
        ev.append((0.3 + i * 0.05, "cc", 11, 40 + 87 * i / 60))
    for i in range(61):
        ev.append((3.3 + i * 0.05, "cc", 11, 127 - 87 * i / 60))
    ev.append((6.4, "off", key))
    x = live.render_events(ev, 6.6, mute, virtual=False)
    f = 440 * 2 ** ((key - 69) / 12)
    w = int(0.04 * SR)
    cs = []
    for k in range(int(0.3 * SR), int(6.3 * SR), int(0.04 * SR)):
        p, _ = midi_check.pitch_near(x[k:k + w], f)
        cs.append(1200 * np.log2(p / f) if p == p else 999.0)
    return a, float(np.mean(np.abs(np.array(cs)) > 50))


def try_cell(a):
    mute, key, dyn, dc, air = a
    with spec.trying(rt_tables.CELLS, f"{mute}_{key}_{dyn}", (dc, air)):
        return held(a)


def held(a):
    mute, key, dyn, dc, air = a
    ok = 0
    for lv in BANDS[dyn]:
        _, r = one((key, lv, 110, mute, False))
        ok += stable(r, lv)
    # slurred into it from a tone either side as well
    for iv in (-2, 2):
        x = live.render_events([(0.0, "cc", 11, BANDS[dyn][-1]), (0.05, "on", key + iv, 90), (0.5, "on", key, 90), (0.52, "off", key + iv), (1.75, "off", key)], 1.9, mute, virtual=False)
        ok += stable(stats(x[int(0.3 * SR):], 440 * 2 ** ((key - 69) / 12)), BANDS[dyn][-1])
    return a, ok, len(BANDS[dyn]) + 2


def repair(mute):
    """The key and dynamics that do not hold: the lips a little off and the air a little different."""
    res = run(mute)
    band = lambda air: "pp" if air < 50 else ("mf" if air < 95 else "ff")
    cells = sorted({(k[0], band(k[1])) for k, r in res.items() if not stable(r, k[1])})
    jobs = [(mute, key, dyn, dc, air) for key, dyn in cells for dc in (-0.2, -0.15, -0.1, -0.05, -0.025, 0.0, 0.025, 0.05) for air in (0.6, 0.8, 1.0, 1.25)]
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        tried = list(ex.map(try_cell, jobs, chunksize=4))
    left = []
    for key, dyn in cells:
        rows = [(a, ok, n) for a, ok, n in tried if a[1] == key and a[2] == dyn]
        a, ok, n = max(rows, key=lambda r: (r[1], -abs(r[0][3]) - abs(np.log(r[0][4]))))
        rt_tables.CELLS[f"{mute}_{key}_{dyn}"] = (a[3], a[4])
        if ok < n:
            left.append((key, dyn, ok, n))
    all_cells = spec.load("cells", {})
    all_cells.update({k: v for k, v in rt_tables.CELLS.items() if k.startswith(mute + "_")})
    spec.save("cells", all_cells)
    print(f"{mute}: {len(cells)} key and dynamics repaired; not all cases held: {left}", flush=True)


def run(mute):
    rt_tables.load(mute)
    jobs = [(k, a, v, mute, False) for k in KEYS for a in AIR for v in VEL]
    with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
        return dict(ex.map(one, jobs, chunksize=6))


if __name__ == "__main__":
    args = sys.argv[1:]
    fix = "repair" in args
    names = [a for a in args if a != "repair"] or ["open", "straight", "cup", "wah", "wah_stem", "bucket"]
    for mute in names:
        if fix:
            repair(mute)
        res = run(mute)
        bad = {k: r for k, r in res.items() if not stable(r, k[1])}
        with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
            sw = dict(ex.map(swell, [(k, mute) for k in KEYS], chunksize=2))
        lost = {k[0]: round(v, 2) for k, v in sw.items() if v > 0.1}
        keys = in_range(dict(sorted(__import__('collections').Counter(k[0] for k in bad).items())))
        print(f"{mute}: {sum(keys.values())} of {len(res)} not holding in the range, {keys}; swell up and down off the note on keys {in_range(lost)}", flush=True)
        json.dump({f"{k[0]}_{k[1]}_{k[2]}": r for k, r in res.items()}, open(os.path.join(spec.OUT, f"stability_{mute}.json"), "w"))
