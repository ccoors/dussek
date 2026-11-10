"""The phrase on round 3's player: two-mode lips at 4x, the player leaning into each tongued
note, breath and wander, the slides per note, the microphone correction."""
import os
import numpy as np
import soundfile as sf
import spec, fit5, compare5

HERE = os.path.dirname(os.path.abspath(__file__))
T5 = compare5.T5
SLUR_KICK = 0.5


def leveled():
    """Per note and dynamics, the pressure (between the grid's) that gives one loudness per
    dynamics after the microphone correction, at the table's lip setting."""
    grids = spec.load_cache("grids")
    eq = np.array(T5["eq"])
    post = lambda c: c["level_db"] + 10 * np.log10(np.sum(10 ** ((np.array(c["bands"]) + eq) / 10)))
    target = {d: float(np.median([post(T5["table"][m][d]) for m in T5["table"]])) for d in ("pp", "mf", "ff")}
    out = {}
    for m, row in T5["table"].items():
        out[m] = {}
        for d in ("pp", "mf", "ff"):
            cells = sorted([c for c in grids[m] if c["nl"] == T5["nl"] and c["center"] == row[d]["center"]
                            and fit5.steady(c)], key=lambda c: c["pm"])
            lv = np.array([post(c) for c in cells])
            lp = np.log([c["pm"] for c in cells])
            ok = np.all(np.diff(lv) > 0) and len(cells) > 1
            out[m][d] = float(np.exp(np.interp(target[d], lv, lp))) if ok else row[d]["pm"]
    return out


TOP = 2.0  # the air at CC127 over the air at ff: beyond any recording, steady on every note up to 4 x but D6


def top_of(midi):
    return 1.0 if midi >= spec.S["top_from"] else TOP


if __name__ == "__main__":
    spec.save("leveled", leveled())
