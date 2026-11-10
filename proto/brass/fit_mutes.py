"""The mutes' shapes against the one measurement there is: harmonics 1-6 of concert F4 on a real
trumpet with and without each mute (bar charts in a 2010 student project at the University of
Illinois, read by eye; linear amplitude, re the fundamental). Only the difference to the same
player's open note counts. The fit moves each mute's dimensions inside a plausible range and
writes mutes_fit.json; the bucket has no measurement and stays as drawn."""
import os
import numpy as np
from scipy.optimize import differential_evolution
import bore, mutes, spec

HERE = os.path.dirname(os.path.abspath(__file__))
F0 = 349.23
EXTRA = 0.0
BARS = {
    "open": [3.0, 3.6, 3.35, 3.65, 2.15, 1.4],
    "straight": [3.0, 5.0, 4.7, 0.6, 8.4, 6.1],
    "cup": [6.0, 9.2, 11.4, 1.3, 4.2, 4.5],
    "wah": [16.0, 10.4, 7.3, 0.6, 12.0, 8.5],
    "wah_stem": [4.0, 1.4, 6.0, 13.3, 10.4, 6.7],
}
RANGES = {
    "straight": dict(cut=(0.05, 0.09), r_tip=(0.009, 0.02), r_end=(0.035, 0.052), length=(0.09, 0.15), gap=(0.0015, 0.006)),
    "wah": dict(cut=(0.04, 0.07), neck_r=(0.010, 0.022), neck_L=(0.01, 0.06), chamber_r=(0.035, 0.05), chamber_L=(0.04, 0.09),
                hole_r=(0.008, 0.016), hole_L=(0.005, 0.03)),
}
RANGES["cup"] = dict(RANGES["straight"], slit=(0.004, 0.02), slit_L=(0.005, 0.03), bowl_ro=(0.045, 0.06), bowl_depth=(0.015, 0.05))
RANGES["wah_stem"] = dict(RANGES["wah"], stem_r=(0.005, 0.011), stem_L=(0.03, 0.09), cup_r=(0.015, 0.03), cup_L=(0.008, 0.03))


def make(name, x):
    m = {"straight": mutes.Straight, "cup": mutes.Cup, "wah": mutes.Wah, "wah_stem": lambda: mutes.Wah(stem=True)}[name]()
    for k, v in zip(RANGES[name], x):
        setattr(m, k, float(v))
    if name == "wah_stem":
        m.aperture = m.cup_r
    elif name == "wah":
        m.aperture = m.hole_r
    return m


def harmonics(mute, fine=np.linspace(0.97, 1.03, 25)):
    """dB of Z H at the harmonics, the slide moved so that the third mode lies on the note."""
    r, L = bore.profile(extra=EXTRA)
    fs = F0 * fine
    Z, _ = bore.impedance(fs, r, L, mute)
    peak = fs[np.argmax(np.abs(Z))]
    fh = peak * np.arange(1, 7)
    Z, H = bore.impedance(fh, r, L, mute)
    return 20 * np.log10(np.abs(Z * H)) + mutes.aperture_eq(fh, mute)


OPEN = None


def rel(db):
    return db - db[0]


def target(name):
    a = 20 * np.log10(np.array(BARS[name]) / BARS[name][0])
    o = 20 * np.log10(np.array(BARS["open"]) / BARS["open"][0])
    return a - o


def cost(x, name, base):
    d = rel(harmonics(make(name, x)) - base) - target(name)
    return float(np.mean(np.clip(d, -25, 25) ** 2))


# against VSCO's recordings: the difference to the open notes in octave bands, per note and dynamics
VSCO = {"straight": "straightM-sus", "wah_stem": "harmonM-sus"}  # its pattern is the chart's stem-in one
RANGES_VSCO = {
    "straight": dict(cut=(0.04, 0.10), r_tip=(0.008, 0.025), r_end=(0.03, 0.055), length=(0.08, 0.16), gap=(0.001, 0.008), block=(0.0, 0.7), beam=(-1.5, 1.0)),
    "wah_stem": dict(cut=(0.03, 0.09), neck_r=(0.006, 0.024), neck_L=(0.005, 0.07), chamber_r=(0.03, 0.055), chamber_L=(0.03, 0.11),
                     hole_r=(0.005, 0.02), hole_L=(0.002, 0.04), beam=(-1.5, 1.0), stem_r=(0.004, 0.012), stem_L=(0.02, 0.09), cup_r=(0.012, 0.03), cup_L=(0.005, 0.03)),
}
BAND_EDGES = [(50, 1000), (1000, 2000), (2000, 4000), (4000, 8000), (8000, 16000)]


def vsco_targets(folder):
    """Bands of the muted notes minus the open notes (dB), per note, averaged over the dynamics."""
    import mute_vsco
    import glob, re
    names = sorted({re.search(r"sus_([A-G]#?-?\d)_", os.path.basename(p)).group(1) for p in glob.glob(os.path.join(mute_vsco.V, folder, "*.wav"))}, key=mute_vsco.midi_of)
    out = {}
    for n in names:
        m = mute_vsco.midi_of(n)
        ds = []
        for vel in ("v1", "v3"):
            a, o = mute_vsco.rec(folder, n, vel), mute_vsco.rec("sus", n, vel)
            if a is None or o is None:
                continue
            f = 440 * 2 ** ((m - 69) / 12)
            ds.append(mute_vsco.measure(a[0], f, a[1])[0] - mute_vsco.measure(o[0], f, o[1])[0])
        if ds and str(m) in mutes_table():
            out[m] = np.mean(ds, axis=0)
    return out


def mutes_table():
    return spec.load("table")["table"]


def extra_of(m):
    import fit4
    return fit4.note_of(m).extra


def bands_of(mute, m, extra, fine=np.linspace(0.97, 1.03, 21)):
    """dB of Z H in the octave bands over the note's harmonics, the slide moved onto the mode."""
    r, L = bore.profile(extra=extra)
    f0 = 440 * 2 ** ((m - 69) / 12)
    Z, _ = bore.impedance(f0 * fine, r, L, mute)
    peak = f0 * fine[np.argmax(np.abs(Z))]
    fh = peak * np.arange(1, int(16000 / peak) + 1)
    Z, H = bore.impedance(fh, r, L, mute)
    db = 20 * np.log10(np.abs(Z * H) + 1e-12) + mutes.aperture_eq(fh, mute)
    return fh, db


def cost_vsco(x, name, targets, extras, base):
    m = make_vsco(name, x)
    e = []
    for n, t in targets.items():
        fh, db = bands_of(m, n, extras[n])
        fo, dbo = base[n]
        o = np.interp(fh, fo, dbo)
        for (lo, hi), v in zip(BAND_EDGES, t):
            k = (fh >= lo) & (fh < hi)
            if k.any():
                e.append(np.clip(np.mean(db[k] - o[k]) - v, -30, 30))
    return float(np.mean(np.square(e)))


def make_vsco(name, x):
    m = mutes.Straight() if name == "straight" else mutes.Wah(stem=True)
    for k, v in zip(RANGES_VSCO[name], x):
        setattr(m, k, float(v))
    m.aperture = 0.03 if name == "straight" else m.cup_r
    return m


def fit_vsco():
    fitted = spec.load("mute_shapes")
    for name, folder in VSCO.items():
        targets = vsco_targets(folder)
        extras = {m: extra_of(m) for m in targets}
        base = {m: bands_of(None, m, extras[m]) for m in targets}
        rng = RANGES_VSCO[name]
        res = differential_evolution(cost_vsco, list(rng.values()), args=(name, targets, extras, base), workers=8, updating="deferred",
                                     maxiter=30, popsize=8, seed=1, tol=1e-3, polish=False)
        fitted[name] = dict(zip(rng, map(float, res.x)))
        print(name, "rms %.1f dB" % np.sqrt(res.fun), {k: round(v * 1000, 1) if k != "block" and k != "beam" else round(v, 2) for k, v in fitted[name].items()}, "notes", list(targets))
        print("   target median", np.round(np.median(list(targets.values()), axis=0)).astype(int))
    # the same mute with its stem out, which no recording here shows
    fitted["wah"] = {k: v for k, v in fitted["wah_stem"].items() if not k.startswith(("stem", "cup"))}
    spec.save("mute_shapes", fitted)


def main():
    base = harmonics(None)
    out = {}
    for name, rng in RANGES.items():
        res = differential_evolution(cost, list(rng.values()), args=(name, base), workers=8, updating="deferred",
                                     maxiter=40, popsize=8, seed=1, tol=1e-3, polish=False)
        d = rel(harmonics(make(name, res.x)) - base)
        out[name] = dict(zip(rng, map(float, res.x)))
        print(name, "rms %.1f dB" % np.sqrt(res.fun), {k: round(v * 1000, 1) for k, v in out[name].items()})
        print("   measured", np.round(target(name)).astype(int), "model", np.round(d).astype(int))
    spec.save("mute_shapes", out)


if __name__ == "__main__":
    import sys
    fit_vsco() if "vsco" in sys.argv else main()
