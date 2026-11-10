"""A note that speaks at once (usage: BRASS=<name> speak_fit.py): per key and dynamics, the air spaced so that the dynamics differ,
then the least push at the attack that speaks within 60 ms (recordings: mostly under that) without standing out over the held note
by more than 6 dB. More air for a note that will not speak was tried and dropped: neighboring keys then jump by up to 13 dB. Writes inst/<name>/rt_speak.json, read by the live player."""
import os
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import live, rt_tables, spec, speech

SR = live.SR
AIR = {"pp": 40, "mf": 75, "ff": 114}
DYNS = ("pp", "mf", "ff")
FIRSTS = (0.0, 1.3, 1.6, 2.0, 2.5, 3.0)  # 0: the attack as it is
TARGET, OVER = 0.06, 6.0
STEP = float(os.environ.get("AIR_STEP", 1.3))  # each dynamics at least this much more air than the last


def spaced():
    """The air factor that spaces each key's dynamics, from the table's own air."""
    tb = rt_tables.load("open")
    out = {}
    for k in rt_tables.TABLE:
        lp = [np.exp(v) * rt_tables.CELLS.get(f"open_{k}_{d}", (0, 1))[1] for v, d in zip(tb["at"][k][0], DYNS)]
        want = 0.0
        for v, d in zip(lp, DYNS):
            want = max(v, want)
            out[f"{k}_{d}"] = float(want / v)
            want *= STEP
    return out


def one(job):
    key, dyn, first, table = job
    live.SPEAK.clear()
    live.SPEAK.update(table)
    live.SPEAK[f"{key}_{dyn}_first"] = first
    x = live.render_events([(0, "cc", 11, AIR[dyn]), (0.02, "on", key, 50), (1.6, "off", key)], 1.6, "open", False, 1)
    s, held, ref = speech.speech(x[int(0.02 * SR):], 440 * 2 ** ((key - 69) / 12))
    w = int(0.03 * SR)
    peak = max(20 * np.log10(np.std(x[i:i + w]) + 1e-9) for i in range(0, int(0.25 * SR), w // 2))
    return key, dyn, first, s, held, float(peak - ref)


def choose(rows):
    ok = [r for r in rows if r[3] is not None and r[3] <= TARGET and r[5] <= OVER]
    if ok:
        return ok[0]
    some = [r for r in rows if r[3] is not None and r[5] <= OVER] or rows[:1]  # the quickest that stays under the overshoot
    return min(some, key=lambda r: r[3] if r[3] is not None else 9)


if __name__ == "__main__":
    keys = [k for k in range(spec.S.get("low", 55), spec.S.get("high", 88) + 1) if k in rt_tables.TABLE]
    air = spaced()
    with ProcessPoolExecutor() as ex:
        res = list(ex.map(one, [(k, d, f, air) for k in keys for d in DYNS for f in FIRSTS]))
    out = dict(air)
    print(spec.NAME, "key: per dynamics air x, ms to speak as it was -> now, push (overshoot dB)")
    for k in keys:
        line = f"{k:4d} "
        for d in DYNS:
            rows = [r for r in res if r[0] == k and r[1] == d]
            best = choose(rows)
            out[f"{k}_{d}_first"] = best[2]
            fm = lambda r: f"{'--' if r[3] is None else int(1000 * r[3])}"
            line += f" {d} x{air[f'{k}_{d}']:.2f} {fm(rows[0]):>4}->{fm(best):>4} p{best[2]:.1f} ({best[5]:4.1f})"
        print(line, flush=True)
    spec.save("speak", out)
