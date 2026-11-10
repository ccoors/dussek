"""The fitted notes with a mute in the bell: the same lips and breath as the open trumpet, the
slide corrected by what each note then plays off (a player lips or slides a muted note in tune).
Writes tuning_mutes.json; `muted.py` prints how each mute plays."""
import multiprocessing, os, sys
import numpy as np
import sim, player, spec, sustain, fit5, compare5, mutes
from concurrent.futures import ProcessPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
TUNE = spec.load("tuning_mutes", {})
CELLS = spec.load("cells_mutes", {})  # "mute_midi_dyn": [lip center +, air x]
T5 = compare5.T5
SPLIT = {}


def candidates(midi, dyn, mute):
    """The note with the mute in, its open bore's mode and the muted bore's modes that can take its place (not the mute's own small low peak)."""
    n = compare5.tuned_note(midi, dyn)
    aim = max(n.modes, key=lambda m: -abs(np.log(m[0] / n.f)))
    n.muted(mutes.MUTES[mute])
    return n, aim, [m for m in n.modes if m[2] > 0.4 * aim[2]]


def nearest(modes, aim):
    return min(modes, key=lambda m: abs(np.log(m[0] / aim[0])))


def split(midi, mute):
    """Does the mute split this key's mode? The open note lies between the halves, and its few cents between the dynamics send them to
    different ones. Only the upper half holds, on every key tried."""
    if (midi, mute) not in SPLIT:
        sides = set()
        for d in ("pp", "mf", "ff"):
            _, aim, near = candidates(midi, d, mute)
            sides.add(nearest(near, aim)[0] > aim[0])
        SPLIT[(midi, mute)] = len(sides) == 2
    return SPLIT[(midi, mute)]


def know_splits(names):
    """Each key's split for these mutes, before workers fork: each would find them again."""
    jobs = [(int(m), n) for n in names if mutes.MUTES[n] is not None and not isinstance(mutes.MUTES[n], mutes.Plunger) for m in T5["table"] if (int(m), n) not in SPLIT]
    with ProcessPoolExecutor(8, mp_context=multiprocessing.get_context("fork")) as ex:
        SPLIT.update(zip(jobs, ex.map(split, *zip(*jobs)) if jobs else ()))


def taken(midi, dyn, mute):
    """The note with the mute in, its open bore's mode and the muted mode that takes its place."""
    n, aim, near = candidates(midi, dyn, mute)
    if split(midi, mute):
        near = [m for m in near if m[0] > aim[0]]
    return n, aim, nearest(near, aim)


def mode_cents(midi, dyn, mute):
    _, aim, m = taken(midi, dyn, mute)
    return float(1200 * np.log2(m[0] / aim[0]))


def note(midi, dyn, mute):
    if mutes.MUTES[mute] is None or isinstance(mutes.MUTES[mute], mutes.Plunger):
        return compare5.tuned_note(midi, dyn)
    n, aim, m = taken(midi, dyn, mute)
    n.retune(1200 * np.log2(m[0] / aim[0]) + TUNE.get(f"{mute}_{midi}_{dyn}", 0.0))
    return n


def eq_gain(f, mute, dyn="mf", raw=False):
    """The microphone's correction, less the beaming of a smaller opening, and what the shape cannot reach."""
    f = np.maximum(f, 1.0)
    db = mutes.aperture_eq(f, mutes.MUTES[mute]) + (0.0 if raw else mutes.residual_db(f, mute, dyn))
    return compare5.eq_curve(f, np.array(T5["eq"])) * 10 ** (db / 20)


def eq(x, mute, sr=sim.SR, dyn="mf", raw=False):
    X = np.fft.rfft(x)
    return np.fft.irfft(X * eq_gain(np.fft.rfftfreq(len(x), 1 / sr), mute, dyn, raw), len(x))


def setting(midi, dyn, mute):
    """How the player's lips and air differ on this mute for this note: (center +, air x)."""
    return CELLS.get(f"{mute}_{midi}", CELLS.get(f"{mute}_{midi}_{dyn}", (0.0, 1.0)))


def model(midi, dyn, mute, dur=2.0, seed=1, c=None, raw=False):
    c = dict(c or T5["table"][str(midi)][dyn])
    dc, air = setting(midi, dyn, mute)
    c["center"] += dc
    c["pm"] *= air
    a = compare5.attack_for(compare5.DYN[dyn])
    (x,), _ = fit5.render(note(midi, dyn, mute), c["center"], c["pm"], nls=(T5["nl"],), dur=dur, kick=a["kick"],
                          life=compare5.LIFE, seed=seed, blow=True, scoop=a["scoop"], first=a["first"], burst=a["burst"])
    return eq(x, mute, dyn=dyn, raw=raw)


def measure(job):
    m, d, mute = job
    x = model(m, d, mute)
    f = 440 * 2 ** ((m - 69) / 12)
    s = x[int(0.6 * sim.SR):int(1.8 * sim.SR)]
    st = sustain.stats(s, sim.SR, f)
    ps = np.array([player.pitch(s[k:k + 2205]) for k in range(0, len(s) - 2205, 1102)])
    c = 1200 * np.log2(ps / f)
    return job, dict(cents=float(np.median((c + 600) % 1200 - 600)), sd=st["pitch_sd_c"], noise=st["noise_db"],
                     level=float(20 * np.log10(np.std(s) + 1e-12)), bands=sustain.bands(s, sim.SR))


def steady(r):
    return abs(r["cents"]) < 30 and r["sd"] < 8 and r["noise"] < -25


def attempt(job):
    (m, d, mute), dc, air = job
    with spec.trying(CELLS, f"{mute}_{m}_{d}", (dc, air)), spec.trying(TUNE, f"{mute}_{m}_{d}"):
        _, r = measure((m, d, mute))
    return job, r


def repair(names):
    """The cells that leave their note: the player moves the lips a little and blows more or less."""
    res = run(names)
    bad = [k for k, r in res.items() if not steady(r)]
    jobs = [(k, dc, air) for k in bad for dc in (-0.05, -0.025, 0.0, 0.025, 0.05) for air in (0.7, 1.0, 1.5)]
    with ProcessPoolExecutor(8, mp_context=multiprocessing.get_context("fork")) as ex:
        tries = list(ex.map(attempt, jobs))
    fixed = 0
    for k in bad:
        ok = [(abs(j[1]) + abs(np.log(j[2])) * 20, j, r) for j, r in tries if j[0] == k and steady(r)]
        if ok:
            _, j, r = min(ok, key=lambda a: a[0])
            CELLS[f"{k[2]}_{k[0]}_{k[1]}"] = (j[1], j[2])
            TUNE[f"{k[2]}_{k[0]}_{k[1]}"] = r["cents"]
            fixed += 1
    spec.save("cells_mutes", CELLS)
    spec.save("tuning_mutes", TUNE)
    print(f"repair: {len(bad)} cells off, {fixed} found a setting")


def run(names):
    know_splits(names)
    jobs = [(int(m), d, n) for n in names for m in T5["table"] for d in ("pp", "mf", "ff")]
    # forked, so that the workers see this pass's tuning
    with ProcessPoolExecutor(8, mp_context=multiprocessing.get_context("fork")) as ex:
        return dict(ex.map(measure, jobs))


def measured_note(names):
    """The measured note, harmonics 1-6 of concert F4, against the model; on an instrument that has the note."""
    if "65" not in T5["table"]:
        return
    import fit_mutes
    o = None
    print("F4 mf, harmonics re the fundamental, difference to open: measured / model")
    for n in ["open"] + names:
        x = model(65, "mf", n)[int(0.6 * sim.SR):int(1.8 * sim.SR)]
        f = np.fft.rfftfreq(len(x), 1 / sim.SR)
        X = np.abs(np.fft.rfft(x * np.hanning(len(x))))
        h = np.array([20 * np.log10(X[(f > 349.23 * k * 0.98) & (f < 349.23 * k * 1.02)].max() + 1e-12) for k in range(1, 7)])
        h -= h[0]
        if o is None:
            o = h
        else:
            print(f"{n:9s}", "measured", np.round(fit_mutes.target(n)).astype(int) if n in fit_mutes.BARS else "-", "model", np.round(h - o).astype(int))


if __name__ == "__main__":
    names = [n for n in mutes.MUTES if n != "open" and (n in sys.argv or not set(sys.argv) & set(mutes.MUTES))]
    if "retune" in sys.argv:
        for i in range(2):
            res = run(names)
            for (m, d, n), r in res.items():
                c = r["cents"]
                # a note that left its mode (more than 150 cents off) is not a tuning matter
                if abs(c) < 150:
                    TUNE[f"{n}_{m}_{d}"] = TUNE.get(f"{n}_{m}_{d}", 0.0) + c
            spec.save("tuning_mutes", TUNE)
            print("pass", i, "off:", {n: np.round(np.percentile([abs(r["cents"]) for k, r in res.items() if k[2] == n], [50, 90, 100])).tolist() for n in names})
    if "repair" in sys.argv:
        repair(names)
    res = run(["open"] + names)
    print("mute: steady notes of", len(T5["table"]) * 3, "| level re open dB (pp mf ff) | off in tune (cents), worst | bands re open dB (<1k .. 8-16k)")
    for n in ["open"] + names:
        rows = {k: r for k, r in res.items() if k[2] == n}
        ok = [k for k, r in rows.items() if steady(r)]
        bad = sorted(set(k[0] for k in rows if k not in ok))
        lev = [np.median([rows[k]["level"] - res[(k[0], k[1], "open")]["level"] for k in rows if k[1] == d and k in ok] or [0]) for d in ("pp", "mf", "ff")]
        bd = np.median([np.array(rows[k]["bands"][:5]) + rows[k]["level"] - np.array(res[(k[0], k[1], "open")]["bands"][:5]) - res[(k[0], k[1], "open")]["level"] for k in ok if k[1] == "mf"], axis=0) if ok else []
        print(f"{n:9s} {len(ok):3d} | {lev[0]:+5.1f} {lev[1]:+5.1f} {lev[2]:+5.1f} | {max(abs(rows[k]['cents']) for k in ok):3.0f} | {np.round(bd).astype(int)} | off notes {bad[:14]}")
    measured_note(names)
