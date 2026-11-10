"""Does a slur land on its note, soft and loud? rt_stability's cases go in from a tone either side at mf and ff, on a player without
variation; a soft line through the virtual player eases the lips past the note, and where a stronger mode lies close under
it the note stays flat of it for as long as it sounds (or for good). Per key, the median pitch from 60 to 270 ms after the
slur, over slurs from either side, a falling line and six seeds: the player varies, and three missed what the engine's own numbers found. slur_fix.py [mute] prints the keys off by more than
LIMIT; slur_fix.py repair [mute] moves their lips by the least that lands them, the highest key first: a line
falling onto a key comes through the keys above it. Writes fit.cells."""
import multiprocessing
import sys
from concurrent.futures import ProcessPoolExecutor

import numpy as np

import live, rt_tables, sim, spec

SR = sim.SR
SOFT, LOUD, SEEDS = 60, 127, (1, 2, 3, 4, 5, 6)
# a loudness band: the slurs' velocity, the dynamics' cells a move goes into, the tongued notes it must leave be. The soft
# band moves pp and mf; the loud one ff alone: a line falling at ff landed under the trumpet's G-sharp 5 (115 c flat)
BANDS = {"soft": (SOFT, ("pp", "mf"), (45, 70, 100)), "loud": (LOUD, ("ff",), (100, 127))}
LIMIT, TONGUED_LIMIT = 20.0, 25.0
DCS = (0.005, 0.01, -0.005, 0.015, -0.01, 0.02, -0.015, 0.03, -0.02, 0.04, -0.03, 0.05, 0.075, 0.1)
WORKERS = 12


def median_cents(x, f, t0, t1):
    """The median pitch over a stretch, in cents re f; frames of four periods at least: a low note's are long."""
    n = max(int(0.04 * SR), int(4 / f * SR))
    cs = []
    for a in range(int(t0 * SR), max(int(t1 * SR) - n, int(t0 * SR) + 1), int(0.02 * SR)):
        s = x[a:a + n]
        s = (s - s.mean()) * np.hanning(len(s))
        c = np.fft.irfft(np.abs(np.fft.rfft(s, 2 * len(s))) ** 2)[:len(s)]
        lo, hi = int(SR / (f * 1.5)), min(int(SR / (f / 1.5)), len(s) - 2)
        k = lo + int(np.argmax(c[lo:hi]))
        d = c[k - 1] - 2 * c[k] + c[k + 1]
        cs.append(1200 * np.log2(SR / (k + (0.5 * (c[k - 1] - c[k + 1]) / d if d < 0 else 0.0)) / f))
    return float(np.median(cs))


def lines(key):
    """The slurs onto a key: what comes before it, per case."""
    out = [[key + iv] for iv in (2, 1, -1, -2)] + [[key + 3, key + 1]]
    return [ln for ln in out if all(spec.S["low"] <= k <= spec.S["high"] for k in ln)]


def landing(key, before, seed, mute="open", vel=SOFT):
    """A line of 0.25 s notes onto the key (soft unless `vel`): where the key sits."""
    ev, t = [], 0.1
    for k in before:
        ev += [(t, "on", k, vel), (t + 0.28, "off", k)]
        t += 0.25
    ev += [(t, "on", key, vel), (t + 0.5, "off", key)]
    x = live.render_events(ev, t + 0.6, mute, virtual=True, seed=seed)
    return median_cents(x, 440 * 2 ** ((key - 69) / 12), t + 0.06, t + 0.27)


def worst_slur(key, mute="open", stop=None, band="soft"):
    """The farthest from the note any slur of the band onto the key sits (signed); past `stop` it looks no further."""
    worst = 0.0
    for ln in lines(key):
        for s in SEEDS:
            c = landing(key, ln, s, mute, BANDS[band][0])
            worst = max(worst, c, key=abs)
            if stop is not None and abs(worst) > stop:
                return worst
    return worst


def worst_tongued(key, mute="open", stop=None, band="soft"):
    """The same for the key tongued (at the band's velocities): the lips' move must not take it off the note there."""
    worst = 0.0
    for v in BANDS[band][2]:
        for s in SEEDS:
            x = live.render_events([(0.05, "on", key, v), (0.55, "off", key)], 0.7, mute, virtual=True, seed=s)
            worst = max(worst, median_cents(x, 440 * 2 ** ((key - 69) / 12), 0.15, 0.5), key=abs)
            if stop is not None and abs(worst) > stop:
                return worst
    return worst


def apply(mute, key, dc, air, band="soft"):
    """On top of what the key has, in the band's dynamics."""
    for d in BANDS[band][1]:
        old = BEFORE.get(f"{mute}_{key}_{d}", (0.0, 1.0))
        rt_tables.CELLS[f"{mute}_{key}_{d}"] = (old[0] + dc, old[1] * air)


def candidates(mute, key, band="soft"):
    """The moves tried, least first: the lips only. The air sets the note's level: 0.8 of it left a soft slur's note
    2 to 6.6 dB under its line, mostly the bell's noise, 1.25 others up to 4 dB over theirs."""
    return sorted(((mute, key, dc, 1.0, band) for dc in DCS), key=lambda a: abs(a[2]))


def passes(a):
    """Does this move land the key? Stops at the first case off: most moves fail on it."""
    mute, key, dc, air, band = a
    apply(mute, key, dc, air, band)
    return a, (abs(worst_slur(key, mute, LIMIT, band)) <= LIMIT
               and abs(worst_tongued(key, mute, TONGUED_LIMIT, band)) <= TONGUED_LIMIT)


def how_far(a):
    mute, key, dc, air, band = a
    apply(mute, key, dc, air, band)
    return a, worst_slur(key, mute, band=band)


def measured(a):
    return how_far(a)[1], tongued_at(a)


def tongued_at(a):
    mute, key, dc, air, band = a
    apply(mute, key, dc, air, band)
    return worst_tongued(key, mute, band=band)


def may_take(before, slur, tongued, tongued_before):
    """A move that does not land the key: it comes closer by 10 c at least and leaves the tongued note no further off than the limit or than it was."""
    return abs(slur) <= abs(before) - 10 and abs(tongued) <= max(TONGUED_LIMIT, abs(tongued_before))


def search(mute, key, before, tongued_before, band="soft"):
    """The least move that lands the key, or else the closest that may be taken: (dc, air, slur, tongued, landed), or None."""
    moves = candidates(mute, key, band)
    with ProcessPoolExecutor(WORKERS, mp_context=multiprocessing.get_context("fork")) as ex:
        for i in range(0, len(moves), WORKERS):
            ok = [a for a, good in ex.map(passes, moves[i:i + WORKERS]) if good]
            if ok:
                return ok[0][2], ok[0][3], *list(ex.map(measured, ok[:1]))[0], True
        closest = sorted(ex.map(how_far, moves), key=lambda r: abs(r[1]))[:WORKERS]
        for (a, s), t in zip(closest, ex.map(tongued_at, [c[0] for c in closest])):
            if may_take(before, s, t, tongued_before):
                return a[2], a[3], s, t, False
    return None


def _worst(a):
    return a[0], worst_slur(a[0], a[1], band=a[2])


def scan(mute, band="soft"):
    keys = range(spec.S["low"], spec.S["high"] + 1)
    with ProcessPoolExecutor(WORKERS, mp_context=multiprocessing.get_context("fork")) as ex:
        return dict(ex.map(_worst, [(k, mute, band) for k in keys], chunksize=2))


BEFORE = {}


def repair(mute, bands=("soft", "loud")):
    """The soft band first (as before the loud one came), then the loud one on what that left. Returns each band's scan
    where nothing moved from it on: the cells are as that scan found them, so it is the scan afterwards too."""
    BEFORE.clear()
    BEFORE.update(rt_tables.CELLS)
    rt_tables.load(mute)
    scans = {}
    for band in bands:
        left, moved = {}, {}
        scans[band] = scan(mute, band)
        for key, w in sorted(((k, w) for k, w in scans[band].items() if abs(w) > LIMIT), reverse=True):
            w = worst_slur(key, mute, band=band)  # the keys above may have moved it
            if abs(w) <= LIMIT:
                continue
            found = search(mute, key, w, worst_tongued(key, mute, band=band), band)
            if found is None:
                left[key] = round(w)
                continue
            dc, air, s, t, landed = found
            if not landed:
                left[key] = round(s)
            apply(mute, key, dc, air, band)
            BEFORE.update({f"{mute}_{key}_{d}": rt_tables.CELLS[f"{mute}_{key}_{d}"] for d in BANDS[band][1]})
            moved[key] = (dc, air, round(w), round(s), round(t))
        print(f"{mute} {band}: moved {{key: (lips +, air x, worst before, after, tongued)}} {moved}; still off {left}", flush=True)
        if moved:  # a move changes what every scan before it would find (a soft slur reads the ff cells as well)
            scans.clear()
    cells = spec.load("cells", {})
    cells.update({k: v for k, v in rt_tables.CELLS.items() if k.startswith(mute + "_")})
    spec.save("cells", cells)
    return scans


if __name__ == "__main__":
    # slur_fix.py [repair] [soft|loud] [mute]: a band alone repairs (or scans) that band only
    args = sys.argv[1:]
    bands = tuple(a for a in args if a in BANDS) or ("soft", "loud")
    mute = next((a for a in args if a != "repair" and a not in BANDS), "open")
    done = repair(mute, bands) if "repair" in args else {}
    rt_tables.load(mute)
    for band in bands:
        res = done[band] if band in done else scan(mute, band)
        print(f"{mute}: {band} slurs off by more than {LIMIT:.0f} c: { {k: round(w) for k, w in res.items() if abs(w) > LIMIT} }", flush=True)
