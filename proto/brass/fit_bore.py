"""A bore's free sizes (usage: BRASS=horn fit_bore.py): where a real instrument's maker tuned its bell and cones, the search
puts the modes on the harmonic series (modes 2-12, or as high as the cutoff allows), at the instrument's overall length.
Prints sizes to copy into spec.py; only the bore's shape, no recording, is involved."""
import numpy as np
from concurrent.futures import ProcessPoolExecutor
from scipy.optimize import minimize
import bore, spec

F = np.arange(20, 1100, 0.5)


def set_sizes(x):
    s = spec.S
    s["cyl"] = (x[0], s["cyl"][1])
    s["bell"] = (x[1], s["bell"][1], x[2])
    if s["cone"]:
        s["cone"] = (x[3], x[4])


def cost(x, full=False):
    if spec.S["cone"] and x[4] < spec.S["cyl"][1] * 1.2:  # a cone widens
        return 1e7 * (1 + spec.S["cyl"][1] * 1.2 - x[4])
    set_sizes(x)
    e, rows = 0.0, []
    for _, base, _, less in spec.S["sides"]:
        r, L = bore.profile(less=less)
        Z, _ = bore.impedance(F, r, L)
        ms = bore.modes(F, Z, fmax=1100, fmin=spec.S["fmin"])
        if len(ms) < 8:
            return 1e6
        c = np.array([1200 * np.log2(ms[i - 1][0] / (i * base)) for i in range(2, min(13, len(ms) + 1))])
        rows.append(np.round(c).astype(int).tolist())
        e += np.sum(c ** 2) + 2 * (c[0] * 2) ** 2  # the lowest modes matter the most to the fingerings
    return (e, rows) if full else e


def start(seed):
    rng = np.random.default_rng(seed)
    s = spec.S
    x0 = np.array([s["cyl"][0], s["bell"][0], s["bell"][2]] + ([s["cone"][0], s["cone"][1]] if s["cone"] else []))
    x = x0 * rng.uniform(0.8, 1.25, len(x0))
    if s["cone"]:
        x[4] = max(x[4], s["cyl"][1] * 1.3)
    r = minimize(cost, x, method="Nelder-Mead", options=dict(maxiter=120, xatol=1e-3, fatol=1.0))
    return float(r.fun), r.x.tolist()


if __name__ == "__main__":
    with ProcessPoolExecutor(12) as ex:
        res = sorted(ex.map(start, range(24)))
    e, x = res[0]
    ec, rows = cost(np.array(x), True)
    names = ["cyl length", "bell length", "bell gamma", "cone length", "cone end radius"]
    print(spec.NAME, "cost", round(ec), {n: round(v, 4) for n, v in zip(names, x)})
    print("modes 2.. in cents off the series:", rows, "length", [round(bore.profile(less=s[3])[1].sum(), 3) for s in spec.S["sides"]])
