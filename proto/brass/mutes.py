"""Mutes as tube networks at the bell: no recording to fit, only the mutes' shapes.

A mute changes what the bore ends in, so it changes the input impedance (how the note plays,
its pitch, an extra low peak) as well as the sound that leaves. Each mute is a termination
(impedance at a cut in the bell, and the part of the flow there that gets out); bore.impedance
takes it in place of the open end. The dimensions are typical of a trumpet's mutes, not
measured from one.
"""
import numpy as np
import spec
from bore import RHO, C, ETA, radiation


def slices(f, S, hyd, lengths, loss=None):
    """Transfer matrix of slices with area S and hydraulic radius hyd (boundary-layer losses
    from it: an annulus of gap g loses as a pipe of radius g). loss: extra attenuation (Np/m)
    as a function of f, for an absorbing filling."""
    w = 2 * np.pi * f
    A, B, Cm, D = np.ones_like(f, dtype=complex), np.zeros_like(f, dtype=complex), np.zeros_like(f, dtype=complex), np.ones_like(f, dtype=complex)
    extra = 0.0 if loss is None else loss(f)
    for s, a, L in zip(S, hyd, lengths):
        d = 1.0433 / (a * np.sqrt(RHO * w / ETA))
        k = (w / C) * (1 + d) - 1j * ((w / C) * d + extra)
        Zc = RHO * C / s * (1 + d - 1j * d)
        c, sn = np.cos(k * L), np.sin(k * L)
        a2, b2, c2, d2 = c, 1j * Zc * sn, 1j * sn / Zc, c
        A, B, Cm, D = A * a2 + B * c2, A * b2 + B * d2, Cm * a2 + D * c2, Cm * b2 + D * d2
    return A, B, Cm, D


def tube(r0, r1, L, step=0.005):
    """A cone or cylinder as slices: areas, hydraulic radii, lengths."""
    n = max(1, int(round(L / step)))
    r = np.linspace(r0, r1, n + 1)
    r = 0.5 * (r[:-1] + r[1:])
    return np.pi * r * r, r, np.full(n, L / n)


def open_out(f, chain_abcd, a):
    """A chain ending in the open air through an opening of radius a: its input impedance,
    the flow out per flow in, and the radiation impedance."""
    A, B, Cm, D = chain_abcd
    Zr = radiation(f, a)
    return (A * Zr + B) / (Cm * Zr + D), 1 / (Cm * Zr + D), Zr


def closed(f, chain_abcd):
    """A chain ending in a rigid wall: its input impedance."""
    A, _, Cm, _ = chain_abcd
    return A / Cm


def parallel(a, b):
    return a * b / (a + b)


def annulus(tail_r, tail_L, r_tip, r_end, length, block=0.3, gap=0.003):
    """The way around a cone-shaped mute: the bell's last slices (tip first) around the mute's
    outside. Its corks (three, in the narrowest place) take `block` of the narrowest gaps."""
    x = np.cumsum(tail_L) - tail_L / 2
    ri = r_tip + (r_end - r_tip) * x / length
    ri = np.minimum(ri, tail_r - gap)
    g = tail_r - ri
    S = np.pi * (tail_r ** 2 - ri ** 2)
    S = np.where(g < 1.3 * g.min(), S * (1 - block), S)
    return S, g, tail_L, ri[-1]


class Mute:
    """cut: how far inside the bell (m from the rim) the bore stops being a plain bell.
    aperture: the radius the sound leaves through, for its beaming (see aperture_eq)."""
    cut = 0.0
    aperture = 0.0615
    beam = 1.0  # how much of that beaming the microphones hear; below 0 they stood off the open bell's axis

    def termination(self, f, tail_r, tail_L):
        """(impedance at the cut, flow out per flow at the cut, radiation impedance)."""
        raise NotImplementedError


class Straight(Mute):
    """A hollow cone, open at its tip, closed at its wide end, held in the bell by corks. The
    sound goes around it, through the gap; the hollow inside hangs off the bell as a side
    branch."""
    cut, r_tip, r_end, length = 0.07, 0.014, 0.048, 0.13
    gap, block = 0.003, 0.3

    def body(self, f, tail_r, tail_L):
        S, g, L, ri = annulus(tail_r, tail_L, self.r_tip, self.r_end, self.length, self.block, self.gap)
        cav = closed(f, slices(f, *tube(self.r_tip - 0.001, self.r_end - 0.001, self.length)))
        return slices(f, S, g, L), cav, np.sum(S[-1:]), ri

    def termination(self, f, tail_r, tail_L):
        abcd, cav, S_rim, _ = self.body(f, tail_r, tail_L)
        Za, out, Zr = open_out(f, abcd, np.sqrt(S_rim / np.pi))
        Zt = parallel(Za, cav)
        return Zt, Zt / Za * out, Zr


class Cup(Straight):
    """A straight mute with a bowl over the bell: the sound leaves the gap into the bowl and
    gets out through the slit between the bowl's rim and the bell's. The bowl is a side
    branch, the slit a small mass: together they take the treble."""
    slit, slit_L, bowl_ro, bowl_depth = 0.008, 0.015, 0.056, 0.035

    def termination(self, f, tail_r, tail_L):
        abcd, cav, _, ri = self.body(f, tail_r, tail_L)
        rim = tail_r[-1]
        Sb = np.pi * (self.bowl_ro ** 2 - (ri + 0.005) ** 2)
        bowl = closed(f, slices(f, [Sb], [self.bowl_ro - ri - 0.005], [self.bowl_depth]))
        Ss = 2 * np.pi * rim * self.slit
        Zs, out_s, Zr = open_out(f, slices(f, [Ss], [self.slit / 2], [self.slit_L]), np.sqrt(Ss / np.pi))
        Zrim = parallel(bowl, Zs)
        A, B, Cm, D = abcd
        Za = (A * Zrim + B) / (Cm * Zrim + D)
        U_rim = 1 / (Cm * Zrim + D)
        Zt = parallel(Za, cav)
        return Zt, Zt / Za * U_rim * Zrim / Zs * out_s, Zr


class Wah(Mute):
    """The cork seals the bell: everything goes through the mute, in by a neck, through a large
    chamber, out by a small hole in its face. Without a stem the hole is short; with one, a
    thin tube in the hole ends in a small cup (the player's hand on it makes the "wah":
    `hand` 0 open .. 1 almost closed)."""
    cut, neck_r, neck_L, chamber_r, chamber_L, hole_r, hole_L = 0.049, 0.016, 0.03, 0.045, 0.07, 0.012, 0.012
    stem = False
    stem_r, stem_L, cup_r, cup_L = 0.0075, 0.065, 0.024, 0.02

    def __init__(self, stem=False, hand=0.0):
        self.stem, self.hand = stem, hand
        self.aperture = self.cup_r if stem else self.hole_r

    def termination(self, f, tail_r, tail_L):
        parts = [tube(self.neck_r, self.neck_r, self.neck_L), tube(self.neck_r, self.chamber_r, 0.02),
                 tube(self.chamber_r, self.chamber_r, self.chamber_L)]
        if self.stem:
            parts += [tube(self.stem_r, self.stem_r, self.stem_L), tube(self.stem_r, self.cup_r, self.cup_L)]
            a = self.cup_r
        else:
            parts += [tube(self.hole_r, self.hole_r, self.hole_L)]
            a = self.hole_r
        if self.hand > 0:  # the hand narrows the way out: a short, narrower end
            a2 = a * (1 - 0.85 * self.hand)
            parts += [tube(a2, a2, 0.008)]
            a = a2
        S = np.concatenate([p[0] for p in parts]); h = np.concatenate([p[1] for p in parts]); L = np.concatenate([p[2] for p in parts])
        return open_out(f, slices(f, S, h, L), a)


class Bucket(Mute):
    """Clipped over the bell, a shallow can of cloth: the sound leaves the rim into the cloth
    (nearly a matched absorber to the treble) or sideways through the gap around it, a mass.
    The treble goes into the cloth, the bass past it; the bore hardly notices."""
    cut, gap, gap_L, can_r, can_L = 0.0, 0.02, 0.03, 0.07, 0.08

    @staticmethod
    def cloth(f):
        return 12.0 * np.sqrt(np.maximum(f, 1.0) / 1000.0)  # Np/m

    def termination(self, f, tail_r, tail_L):
        rim = tail_r[-1]
        can = closed(f, slices(f, [np.pi * self.can_r ** 2], [self.can_r], [self.can_L], loss=self.cloth))
        Sg = 2 * np.pi * rim * self.gap
        Zg, out, Zr = open_out(f, slices(f, [Sg], [self.gap / 2], [self.gap_L]), np.sqrt(Sg / np.pi))
        Zt = parallel(can, Zg)
        return Zt, Zt / Zg * out, Zr


class Plunger(Mute):
    """A rubber cup held by hand before the bell: the sound leaves the rim into the cup and out through the gap between them, a mass. No recording: its size
    from the usual ones, the rest by ear. Only what leaves changes, from this shut to the open bell by hand (rt_tables.py); the note plays as open: a
    cup that close would pull the pitch, and following that while a hand moves is for a real player's ear. So it needs no fit of its own."""
    cut, gap, gap_L, can_r, can_L = 0.0, 0.001, 0.012, 0.0675, 0.035
    SHUT_R = 0.015  # m: what it leaves through, sideways, for the beaming

    @staticmethod
    def rubber(f):
        return 2.0 * np.sqrt(np.maximum(f, 1.0) / 1000.0)  # Np/m

    def termination(self, f, tail_r, tail_L):
        rim = tail_r[-1]
        can = closed(f, slices(f, [np.pi * self.can_r ** 2], [self.can_r], [self.can_L], loss=self.rubber))
        Sg = 2 * np.pi * rim * self.gap
        Zg, out, Zr = open_out(f, slices(f, [Sg], [self.gap / 2], [self.gap_L]), np.sqrt(Sg / np.pi))
        Zt = parallel(can, Zg)
        return Zt, Zt / Zg * out, Zr


BELL = spec.S["bell"][1]  # the open bell's radius, m
SIZES = ("cut", "r_tip", "r_end", "length", "gap", "neck_r", "neck_L", "chamber_r", "chamber_L", "hole_r", "hole_L", "stem_r", "stem_L", "cup_r", "cup_L", "slit",
         "slit_L", "bowl_ro", "bowl_depth", "can_r", "can_L", "gap_L", "aperture")


def scaled(mute):
    """A trumpet's mute in the proportions of this bell: every length by the bells' radii."""
    k = BELL / 0.0615
    for a in SIZES:
        if hasattr(mute, a):
            setattr(mute, a, getattr(mute, a) * k)
    return mute


def _fitted(mute, name):
    """A mute with the dimensions fitted to the measured harmonics (fit_mutes.py)."""
    shapes = spec.lender("mute_shapes")
    if shapes is not None:
        for k, v in shapes.get(name, {}).items():
            setattr(mute, k, v)
        if isinstance(mute, Wah):
            mute.aperture = mute.cup_r if mute.stem else mute.hole_r
        else:
            mute.aperture = 0.03  # a ring around the mute: less beaming than the open bell
    return scaled(mute)


MUTES = {"open": None, "straight": _fitted(Straight(), "straight"), "cup": _fitted(Cup(), "cup"),
         "wah": _fitted(Wah(), "wah"), "wah_stem": _fitted(Wah(stem=True), "wah_stem"), "bucket": scaled(Bucket()), "plunger": scaled(Plunger())}


_RESIDUAL = None


def residual_db(f, name, dyn):
    """The filter for what a mute's shape cannot reach (mute_residual.py): pp, ff, mf between them."""
    global _RESIDUAL
    if _RESIDUAL is None:
        # no recordings of this one: the lender's, its frequencies scaled as the mute's sizes are
        _RESIDUAL = spec.lender("mute_residual", {})
    if name not in _RESIDUAL:
        return np.zeros_like(f)
    t = _RESIDUAL[name]
    v = np.array(t["pp"]) * 0.45 + np.array(t["ff"]) * 0.55 if dyn == "mf" else np.array(t[dyn])
    scale = 1.0 if spec.load("mute_residual") is not None else BELL / 0.0615
    return np.interp(np.log(np.maximum(f, 1.0) * scale), np.log(_RESIDUAL["centers"]), v)


def piston_gain_db(f, a):
    """On-axis gain over the power average of a piston of radius a."""
    from scipy.special import j1
    ka = np.maximum(2 * np.pi * f / C * a, 1e-6)
    return 10 * np.log10(ka * ka / (1 - j1(2 * ka) / ka))


def aperture_eq(f, mute):
    """The change in the microphone's correction: the correction belongs to the open bell,
    which beams its treble ahead; a smaller opening beams less (dB)."""
    if mute is None or mute.aperture >= BELL:
        return np.zeros_like(f)
    return mute.beam * (piston_gain_db(f, mute.aperture) - piston_gain_db(f, BELL))
