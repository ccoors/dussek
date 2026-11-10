"""Round 3: two-mode lips (outward below the note, inward above), simulated at 4x with soft
closure, radiated by power. Per note a grid over the lips' bracket, their Q and the mouth
pressure; each cell's sustain measured over the whole spectrum (bands to 16 kHz) at three
steepening strengths. The table: pp < mf < ff in pressure, matched to Iowa's bands and
brightness, one loudness per dynamics across the range, one steepening for all."""
import os
import numpy as np
import soundfile as sf
import sim, player, spec, sustain

HERE = os.path.dirname(os.path.abspath(__file__))
CENTERS = np.arange(0.9, 1.21, 0.05)  # the bracket's middle re the note
SPREAD = 0.2                           # outward at middle - 0.1, inward at middle + 0.1
QS = [5.0, 10.0]
PMS = np.geomspace(500, 12000, 12)
NLS = [0.0, 0.25, 0.5]                 # x player.NL
OS = 4


INWARD = 0.7  # the inward mode's compliance re the outward one's (at 1.0 a slur downward
              # could leave the lips buzzing at the inward mode's own frequency)


def lips(center, q):
    lo, hi = center - SPREAD / 2, center + SPREAD / 2
    return lo, [(1.0, q, 1.0, 1.0), (hi / lo, q, -1.0, INWARD)]


def note_of(midi):
    tuning = player.load()
    if midi in tuning:
        return player.note_for(midi, tuning)[0]
    return sim.Note(midi)


def render(note, center, q, pm_level, nls=(0.0,), dur=1.8, kick=None, life=None, attack=0.03, seed=1):
    N = int(dur * sim.SR)
    seg = np.zeros(N, int)
    lo, lp = lips(center, q)
    pm = pm_level * sim.envelope(N, attack=attack, release=0.1)
    up = lambda a: np.repeat(a, OS)
    k2 = None
    if kick is not None:
        k2 = np.zeros(N * OS); k2[int(0.002 * sim.SR) * OS] = kick
    u, p = sim.simulate([note], up(seg), up(pm), up(np.full(N, note.f * lo)), kick=k2, sr=sim.SR * OS,
                        soft=1e-4, lips=lp, seed=seed, **(life or {}))
    from scipy.signal import resample_poly
    outs = [resample_poly(sim.radiate(u, [note], up(seg), nl=player.NL * nl, sr=sim.SR * OS), 1, OS) for nl in nls]
    return outs, resample_poly(p, 1, OS)


def grid(midi):
    note = note_of(midi)
    cells = []
    for c in CENTERS:
        for q in QS:
            for pm in PMS:
                xs, p = render(note, c, q, pm, nls=NLS)
                for nl, x in zip(NLS, xs):
                    s = x[int(0.6 * sim.SR):]
                    if np.std(s) < 1e-7:
                        continue
                    st = sustain.stats(s, sim.SR, note.f)
                    cells.append({"center": float(c), "q": q, "pm": float(pm), "nl": nl,
                                  "cents": st["pitch_med_c"], "pitch_sd": st["pitch_sd_c"],
                                  "noise_db": st["noise_db"], "centroid": st["centroid"],
                                  "level_db": float(20 * np.log10(np.std(s))),
                                  "bands": sustain.bands(s, sim.SR)})
    return midi, cells


def steady(c):
    return abs(c["cents"]) < 30 and c["pitch_sd"] < 4 and c["noise_db"] < -30


def iowa_bands():
    out = {}
    for k, info in spec.notes().items():
        x, sr = sf.read(info["file"])
        x = sustain.highpass(x)
        a = int((info["start"] + 0.6) * sr)
        out[k] = sustain.bands(x[a:a + int(1.2 * sr)], sr)
    return out


def cost_cell(cell, rec_bands, rec_centroid):
    # bands 1-2k .. 8-16k, re the band below 1 kHz (the levels are relative to the whole)
    e = 0.0
    for i in (1, 2, 3, 4):
        d = (cell["bands"][i] - cell["bands"][0]) - (rec_bands[i] - rec_bands[0])
        e += (d / 4.0) ** 2
    e += (np.log(cell["centroid"] / rec_centroid) / 0.15) ** 2
    return e


def fit(grids, ib, ist, nl):
    first = {}
    for pass_ in (0, 1):
        table = {}
        if pass_:
            ref = {d: float(np.median([first[m][d]["level_db"] for m in first])) for d in ("pp", "mf", "ff")}
        for m, cells in grids.items():
            ok = sorted([c for c in cells if c["nl"] == nl and steady(c)], key=lambda c: c["pm"])
            recs = [(ib.get(f"{m}_{d}"), ist.get(f"{m}_{d}")) for d in ("pp", "mf", "ff")]
            if len(ok) < 3 or any(r[0] is None for r in recs):
                continue
            pre = [[cost_cell(c, rb, rs["centroid"]) for c in ok] for rb, rs in recs]
            best, cost = None, 1e18
            for i in range(len(ok)):
                for j in range(i + 1, len(ok)):
                    if ok[j]["pm"] <= ok[i]["pm"]:
                        continue
                    for k in range(j + 1, len(ok)):
                        if ok[k]["pm"] <= ok[j]["pm"]:
                            continue
                        e = pre[0][i] + pre[1][j] + pre[2][k]
                        if pass_:
                            for c, d in ((ok[i], "pp"), (ok[j], "mf"), (ok[k], "ff")):
                                e += ((c["level_db"] - ref[d]) / 2.0) ** 2
                        if e < cost:
                            best, cost = (ok[i], ok[j], ok[k]), e
            table[m] = {"pp": best[0], "mf": best[1], "ff": best[2], "cost": float(cost)}
        first = table
    return table


