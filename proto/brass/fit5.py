"""Round 3, the fit: two-mode lips, no firming (the timbre then grows with pressure), and one
fixed output correction (the microphone's place: a trumpet beams its treble ahead), the same
for every note and dynamics, fitted from the median difference to Iowa per band. Writes
table5.json: per note pp/mf/ff cells, the correction, the steepening."""
import os
import numpy as np
import sim, player, spec, sustain, fit4

HERE = os.path.dirname(os.path.abspath(__file__))
CENTERS = [0.95, 0.975, 1.0, 1.025]
PMS = np.geomspace(60, 12000, 18)  # from 300 the soft end was cut off: just above where a note starts, it is much darker
NLS = [0.0, 0.25]
FIRM = 0.0
Q = 5.0
FLOOR = -60.0  # dB re the whole: below it a band is out of hearing and, in a recording, breath and room; the grid has no breath


def grid(midi):
    note = fit4.note_of(midi)
    cells = []
    for c in CENTERS:
        for pm in PMS:
            xs, p = render(note, c, pm, nls=NLS)
            for nl, x in zip(NLS, xs):
                s = x[int(0.6 * sim.SR):]
                if np.std(s) < 1e-7:
                    continue
                st = sustain.stats(s, sim.SR, note.f)
                cells.append({"center": c, "pm": float(pm), "nl": nl, "cents": st["pitch_med_c"],
                              "pitch_sd": st["pitch_sd_c"], "noise_db": st["noise_db"],
                              "level_db": float(20 * np.log10(np.std(s))), "bands": sustain.bands(s, sim.SR)})
    return midi, cells


def render(note, center, pm_level, nls=(0.0,), dur=1.8, kick=None, life=None, attack=0.03, seed=1, blow=False,
           scoop=0.0, scoop_t=0.04, first=0.7, y0=spec.S["y0"], soft=1e-4, burst=0.0, burst_t=0.03):
    OS = fit4.OS
    N = int(dur * sim.SR)
    seg = np.zeros(N, int)
    lo, lp = fit4.lips(center, Q)
    pm = pm_level * (sim.blow(N, first=first) if blow else sim.envelope(N, attack=attack, release=0.1))
    up = lambda a: np.repeat(a, OS)
    k2 = None
    if kick is not None:
        k2 = np.zeros(N * OS); k2[int(0.002 * sim.SR) * OS] = kick
    # the lips start slack by `scoop` cents and firm up (time constant scoop_t): the pitch slides up
    t = np.arange(N) / sim.SR
    lf = note.f * lo * 2 ** (-scoop * np.exp(-t / scoop_t) / 1200)
    u, p = sim.simulate([note], up(seg), up(pm), up(lf), kick=k2, sr=sim.SR * OS, mu=spec.lip_mu(note.f), width=spec.S["width"],
                        soft=soft, y0=y0, lips=lp, firm=FIRM, seed=seed,
                        burst=up(1 + burst * np.exp(-t / burst_t)), **(life or {}))
    from scipy.signal import resample_poly
    outs = [resample_poly(sim.radiate(u, [note], up(seg), nl=player.NL * nl, sr=sim.SR * OS), 1, OS) for nl in nls]
    return outs, resample_poly(p, 1, OS)


def steady(c):
    return abs(c["cents"]) < 30 and c["pitch_sd"] < 4 and c["noise_db"] < -30


def corrected(bands, eq):
    """Bands (dB re total) after the correction (dB per band), re the new total."""
    b = np.array(bands) + eq
    tot = 10 * np.log10(np.sum(10 ** (b / 10)))
    return b - tot


def fit(grids, ib, nl, eq):
    def cost(cell, rb):
        d = np.maximum(corrected(cell["bands"], eq)[:5], FLOOR) - np.maximum(np.array(rb[:5]), FLOOR)
        return float(np.sum((d / 3.0) ** 2))
    first = None
    for pass_ in (0, 1):
        table = {}
        ref = None
        if first:
            ref = {d: float(np.median([first[m][d]["level_db"] for m in first])) for d in ("pp", "mf", "ff")}
        for m, cells in grids.items():
            ok = sorted([c for c in cells if c["nl"] == nl and steady(c)], key=lambda c: c["pm"])
            recs = [ib.get(f"{m}_{d}") for d in ("pp", "mf", "ff")]
            if len(ok) < 3 or any(r is None for r in recs):
                continue
            pre = [[cost(c, rb) for c in ok] for rb in recs]
            best, bc = None, 1e18
            for i in range(len(ok)):
                for j in range(i + 1, len(ok)):
                    if ok[j]["pm"] <= ok[i]["pm"]:
                        continue
                    for k in range(j + 1, len(ok)):
                        if ok[k]["pm"] <= ok[j]["pm"]:
                            continue
                        e = pre[0][i] + pre[1][j] + pre[2][k]
                        if ref:
                            for c, d in ((ok[i], "pp"), (ok[j], "mf"), (ok[k], "ff")):
                                e += ((c["level_db"] - ref[d]) / 2.0) ** 2
                        if e < bc:
                            best, bc = (ok[i], ok[j], ok[k]), e
            table[m] = {"pp": best[0], "mf": best[1], "ff": best[2], "cost": bc}
        first = table
    return table


if __name__ == "__main__":
    from concurrent.futures import ProcessPoolExecutor
    midis = list(range(spec.S["low"], spec.S["high"] + 1))
    grids = spec.load_cache("grids")
    if grids is not None:
        grids = {int(k): v for k, v in grids.items()}
    else:
        with ProcessPoolExecutor(8) as ex:
            grids = dict(ex.map(grid, midis))
        spec.save_cache("grids", grids)
    ib = fit4.iowa_bands()
    best = None
    for nl in NLS:
        eq = np.zeros(len(sustain.BANDS))
        for it in range(4):
            table = fit(grids, ib, nl, eq)
            # the correction: the median difference per band, over every fitted note and dynamics
            # (what is out of hearing on both sides says nothing: a correction from it would lift the loud notes' treble)
            diffs = [np.maximum(np.array(ib[f"{m}_{d}"]), FLOOR) - np.maximum(corrected(table[m][d]["bands"], eq), FLOOR) for m in table for d in ("pp", "mf", "ff")]
            eq = eq + np.median(diffs, axis=0)
            eq -= eq[0]
        table = fit(grids, ib, nl, eq)
        med = float(np.median([r["cost"] for r in table.values()]))
        print(f"steepening x{nl}: {len(table)} notes, median cost {med:.1f}, correction dB per band {np.round(eq, 1).tolist()}")
        if best is None or med < best[0]:
            best = (med, nl, eq, table)
    med, nl, eq, table = best
    print(f"\nchosen steepening x{nl}")
    print("note dyn | bands re total <1k 1-2k 2-4k 4-8k 8-16k: iowa / model | pm  lips | cents")
    for m in sorted(table):
        for d in ("pp", "mf", "ff"):
            c, rb = table[m][d], ib[f"{m}_{d}"]
            mb = corrected(c["bands"], eq)
            print(f"{m} {d} | " + " ".join(f"{a:4.0f}/{b:<4.0f}" for a, b in zip(rb[:5], mb[:5]))
                  + f" | {c['pm']:5.0f} {c['center']:.3f} | {c['cents']:+4.0f}")
    print("not fitted:", [m for m in midis if m not in table])
    spec.save("table", {"nl": nl, "eq": eq.tolist(), "table": table})
