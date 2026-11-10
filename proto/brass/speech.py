"""Does a note speak? (usage: BRASS=<name> speech.py [mute]): every key from a low to a high velocity, with and without the virtual
player: how long until the note's own pitch holds at its later level, whether it ever does, how far the level swings after."""
import json, os, sys
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import live, midi_check, rt_tables, spec

SR = live.SR
MUTE = sys.argv[1] if __name__ == "__main__" and len(sys.argv) > 1 else "open"
VELS = (30, 50, 70, 90, 110, 127)


def speech(x, f):
    """Seconds from the key until the pitch (within 80 cents: vibrato stays inside) and the level (within 6 dB of the note's own
    later level) first hold for 100 ms; None when never. And the share of the last 0.8 s that holds."""
    w = max(int(0.03 * SR), int(4 / f * SR))
    hop = int(0.005 * SR)
    t, cents, level = [], [], []
    for i in range(0, len(x) - w, hop):
        s = x[i:i + w]
        level.append(20 * np.log10(np.std(s) + 1e-9))
        p, _ = midi_check.pitch_near(s, f) if np.std(s) > 1e-6 else (float("nan"), 0)
        cents.append(1200 * np.log2(p / f) if p == p else 999.0)
        t.append((i + w / 2) / SR)
    t, cents, level = map(np.array, (t, cents, level))
    ref = np.median(level[t > t[-1] - 0.5])
    good = (np.abs(cents) < 80) & (level > ref - 6)
    held = float(np.mean(good[t > t[-1] - 0.8]))
    run = int(0.1 / (hop / SR))
    first = next((t[i] - w / 2 / SR for i in range(len(t) - run) if good[i:i + run].all()), None)
    return (None if first is None else float(max(first, 0.0))), held, float(ref)


def one(job):
    key, vel, virt, mute = job
    x = live.render_events([(0.02, "on", key, vel), (1.8, "off", key)], 1.8, mute, virtual=virt)
    s, held, ref = speech(x[int(0.02 * SR):], 440 * 2 ** ((key - 69) / 12))
    return dict(key=key, vel=vel, virt=virt, speak=s, held=held, level=ref)


if __name__ == "__main__":
    keys = list(range(spec.S.get("low", 55), spec.S.get("high", 88) + 1))
    rt_tables.load(MUTE)
    jobs = [(k, v, True, MUTE) for k in keys for v in VELS]
    with ProcessPoolExecutor() as ex:
        res = list(ex.map(one, jobs))
    json.dump(res, open(os.path.join(spec.OUT, f"speech_{spec.NAME}_{MUTE}.json"), "w"))
    print(spec.NAME, "ms to speak with the virtual player; -- never; columns velocity", VELS)
    for k in keys:
        row = [r for r in res if r["key"] == k]
        print(f"{k:4d} " + " ".join(("  --" if r["speak"] is None else f"{1000 * r['speak']:4.0f}") for r in row) + "  level " + " ".join(f"{r['level']:5.0f}" for r in row))
