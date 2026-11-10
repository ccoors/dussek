"""An instrument beyond the trumpet, against VSCO's recordings (usage: BRASS=trombone fit_other.py [refit]): per key a grid over
the lips and the air, each cell's sustain measured in finer bands than the trumpet's fit (a low instrument's tone is mostly below
1 kHz), the recording's bands interpolated between its sampled notes; pp < mf < ff in air, one correction per band for the
microphone and the room. Writes in inst/<name>/: grids5.json, table5.json, and recording.json. VSCO's names are an octave low."""
import sys
import numpy as np
import soundfile as sf
from concurrent.futures import ProcessPoolExecutor
import fit4, fit5, sim, spec, sustain

BANDS = [(spec.S.get("band0", 50), 125), (125, 250), (250, 500), (500, 1000), (1000, 2000), (2000, 4000), (4000, 8000), (8000, 16000), (16000, 22000)]
EQ_HZ = [float(np.sqrt(a * b)) for a, b in BANDS]
CENTERS = [0.9, 0.95, 0.975, 1.0, 1.025, 1.05]
PMS = np.geomspace(40, 12000, 18)
NLS = list(spec.S.get("fit_nl", (0.0, 0.25, 0.5, 1.0)))
NAMES = "C C# D D# E F F# G G# A A# B".split()
DYN = ("pp", "mf", "ff")


def bands(x, sr):
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / sr)
    tot = sp[f > BANDS[0][0] - 10].sum()
    return [float(10 * np.log10(sp[(f >= a) & (f < b)].sum() / tot + 1e-20)) for a, b in BANDS]


def recording():
    """{key: {dyn: {bands, level}}} of the sampled notes (first round robin; the layers as the spec maps them)."""
    out = {}
    for info in spec.notes().values():
        x, sr = sf.read(info["file"])
        x = x.mean(axis=1) if x.ndim > 1 else x
        s = x[int((info["start"] + 0.5) * sr):int(min(info["start"] + 1.7, info["end"] - 0.2) * sr)]
        if len(s) >= sr // 3:
            out.setdefault(info["midi"], {})[info["dyn"]] = {"bands": bands(s, sr), "level": float(20 * np.log10(np.std(s) + 1e-12))}
    return out


def target(rec, key, dyn):
    """A key's bands and level: between the nearest sampled notes of that layer."""
    ks = sorted(k for k in rec if dyn in rec[k])
    b = np.array([rec[k][dyn]["bands"] for k in ks])
    return np.array([np.interp(key, ks, b[:, i]) for i in range(b.shape[1])]), float(np.interp(key, ks, [rec[k][dyn]["level"] for k in ks]))


def grid(midi):
    note = fit4.note_of(midi)
    cells = []
    for c in CENTERS:
        for pm in PMS:
            xs, _ = fit5.render(note, c, pm, nls=NLS)
            for nl, x in zip(NLS, xs):
                s = x[int(0.6 * sim.SR):]
                if np.std(s) < 1e-7:
                    continue
                st = sustain.stats(s, sim.SR, note.f)
                cells.append({"center": c, "pm": float(pm), "nl": nl, "cents": st["pitch_med_c"], "pitch_sd": st["pitch_sd_c"],
                              "noise_db": st["noise_db"], "level_db": float(20 * np.log10(np.std(s))), "bands": bands(s, sim.SR)})
    return midi, cells


def heard(key):
    """The bands that hold the note: below its fundamental there is only the recording's room and rumble."""
    f = 440 * 2 ** ((key - 69) / 12)
    return np.array([b > f * 0.9 and b <= 16000 for _, b in BANDS])


def fit(grids, rec, nl, eq, near=None):
    """The cheapest steady pp < mf < ff per key, the steps between them near the recording's. near: {key: {dyn: air}}, held to it: a
    bore that is steady at any air gives the bands no say in how much."""
    table = {}
    for m, cells in grids.items():
        ok = sorted([c for c in cells if c["nl"] == nl and fit5.steady(c)], key=lambda c: c["pm"])
        if len(ok) < 3:
            continue
        tg = [target(rec, m, d) for d in DYN]
        post = [fit5.corrected(c["bands"], eq) for c in ok]
        use = heard(m)
        pre = [[float(np.sum(((np.maximum(p[use], fit5.FLOOR) - np.maximum(t[0][use], fit5.FLOOR)) / 3.0) ** 2)) for p in post] for t in tg]
        if near:
            pre = [[q + 4 * np.log2(c["pm"] / near[m][d]) ** 2 for q, c in zip(row, ok)] for row, d in zip(pre, DYN)]
        lev = [c["level_db"] + 10 * np.log10(np.sum(10 ** ((np.array(c["bands"]) + eq) / 10))) for c in ok]
        step1, step2 = tg[1][1] - tg[0][1], tg[2][1] - tg[1][1]
        best, bc = None, 1e18
        for i in range(len(ok)):
            for j in range(i + 1, len(ok)):
                if ok[j]["pm"] <= ok[i]["pm"]:
                    continue
                for k in range(j + 1, len(ok)):
                    if ok[k]["pm"] <= ok[j]["pm"]:
                        continue
                    e = pre[0][i] + pre[1][j] + pre[2][k] + ((lev[j] - lev[i] - step1) / 4.0) ** 2 + ((lev[k] - lev[j] - step2) / 4.0) ** 2
                    if e < bc:
                        best, bc = (ok[i], ok[j], ok[k]), e
        table[m] = {"pp": best[0], "mf": best[1], "ff": best[2], "cost": bc}
    return table


def main():
    keys = list(range(spec.S["low"], spec.S["high"] + 1))
    grids = {}
    if spec.load_cache("grids") is not None and "regrid" not in sys.argv:
        grids = {int(k): v for k, v in spec.load_cache("grids").items() if int(k) in keys}
    missing = [k for k in keys if k not in grids]
    if missing:
        with ProcessPoolExecutor(12) as ex:
            for m, cells in ex.map(grid, missing):
                try_ok = sum(fit5.steady(c) for c in cells if c["nl"] == 0.0)
                print(f"grid {m}: {try_ok} steady", flush=True)
                grids[m] = cells
        spec.save_cache("grids", grids)
    rec = recording()
    spec.save_cache("recording", rec)
    print("recorded notes:", sorted(rec), "levels pp/mf/ff at the middle:", [round(target(rec, keys[len(keys) // 2], d)[1], 1) for d in DYN])
    steady = {m: sum(fit5.steady(c) for c in cells if c["nl"] == 0.0) for m, cells in grids.items()}
    print("steady cells per key of", len(CENTERS) * len(PMS), ":", steady)
    best = None
    for nl in NLS:
        eq = np.zeros(len(BANDS))
        for _ in range(4):
            table = fit(grids, rec, nl, eq)
            if not table:
                break
            d = [np.where(heard(m), np.maximum(target(rec, m, x)[0], fit5.FLOOR) - np.maximum(fit5.corrected(table[m][x]["bands"], eq), fit5.FLOOR), np.nan) for m in table for x in DYN]
            eq = eq + np.nan_to_num(np.nanmedian(d, axis=0))
            eq -= eq[3]
        if not table:
            print(f"steepening x{nl}: nothing fitted")
            continue
        table = fit(grids, rec, nl, eq)
        med = float(np.median([r["cost"] for r in table.values()]))
        print(f"steepening x{nl}: {len(table)} of {len(keys)} keys, median cost {med:.1f}, correction dB {np.round(eq, 1).tolist()}")
        if best is None or med < best[0]:
            best = (med, nl, eq, table)
    med, nl, eq, table = best
    if spec.S.get("even_air"):
        for _ in range(3):
            near = {m: {d: float(np.exp(np.median([np.log(table[q][d]["pm"]) for q in table if abs(q - m) <= 4]))) for d in DYN} for m in table}
            table = fit(grids, rec, nl, eq, near)
    print(f"chosen x{nl}. key dyn | bands 50..8k recording/model | air, lips, cents, level")
    for m in sorted(table):
        for d in DYN:
            c, t = table[m][d], target(rec, m, d)
            mb = fit5.corrected(c["bands"], eq)
            print(f"{m} {d} | " + " ".join(f"{a:4.0f}/{b:<4.0f}" for a, b in zip(t[0][:8], mb[:8])) + f" | {c['pm']:6.0f} {c['center']:.3f} {c['cents']:+4.0f} {c['level_db']:6.1f} rec {t[1]:6.1f}")
    print("not fitted:", [m for m in keys if m not in table])
    spec.save("table", {"nl": nl, "eq": eq.tolist(), "eq_hz": EQ_HZ, "table": table})


if __name__ == "__main__":
    if "recording" in sys.argv:  # the levels alone (the whole-spectrum fit has no use for the rest)
        spec.save_cache("recording", recording())
    else:
        main()
