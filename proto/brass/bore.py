"""A trumpet-like bore: its profile, input impedance, modes and radiation (frequency domain).

Throwaway spike. Lossy cylinder slices (Keefe's wide-pipe losses), an unflanged open end
(Levine–Schwinger fit), modes picked from the impedance peaks.
"""
import numpy as np
import spec

RHO, C, ETA = 1.2, 343.0, 1.85e-5


def bessel_bell(r0, r1, length, gamma, n):
    # r(x) = A / (x0 - x)^gamma from r0 at x = 0 to r1 at x = length
    q = (r1 / r0) ** (1 / gamma)
    x0 = q * length / (q - 1)
    a = r0 * x0 ** gamma
    x = np.linspace(0, length, n)
    return x, a / (x0 - x) ** gamma


def profile(extra=0.0, less=0.0, step=0.002):
    """Radii of slices (m) and their lengths, mouthpiece first. `extra` adds valve tubing, `less` is the shorter side's."""
    s = spec.S
    # mouthpiece: cup (as a short wide cylinder), throat, backbore; leadpipe
    parts = list(s["mouth"])  # (length, radius in, radius out)
    # cylindrical section with the valves' tubing
    L, rc = s["cyl"]
    parts.append((L - less + extra, rc, rc))
    r_bell = rc
    if s["cone"]:
        parts.append((s["cone"][0], rc, s["cone"][1]))
        r_bell = s["cone"][1]
    radii, lengths = [], []
    for L, ra, rb in parts:
        n = 1 if ra == rb else max(1, int(round(L / step)))  # a cylinder is one slice
        r = np.linspace(ra, rb, n + 1)
        radii += list(0.5 * (r[:-1] + r[1:]))
        lengths += [L / n] * n
    # bell
    x, r = bessel_bell(r_bell, s["bell"][1], s["bell"][0], s["bell"][2], 400)
    radii += list(0.5 * (r[:-1] + r[1:]))
    lengths += list(np.diff(x))
    return np.array(radii), np.array(lengths)


def radiation(f, a):
    k = 2 * np.pi * f / C
    ka = k * a
    # the fit holds to ka ~ 3.5; beyond it nothing returns from the open end
    mag = np.clip((1 + 0.2 * ka - 0.084 * ka**2) / (1 + 0.2 * ka + (0.5 - 0.084) * ka**2), 0, 1)
    lend = a * 0.6133 * (1 + 0.044 * ka**2) / (1 + 0.19 * ka**2)
    R = -mag * np.exp(-2j * k * lend)
    Zc = RHO * C / (np.pi * a * a)
    return Zc * (1 + R) / (1 - R)


def chain(f, radii, lengths):
    """The whole bore's transfer matrix (input side first) at frequencies f."""
    w = 2 * np.pi * f
    A = np.ones_like(f, dtype=complex)
    B = np.zeros_like(A)
    Cm = np.zeros_like(A)
    D = np.ones_like(A)
    for a, L in zip(radii, lengths):
        S = np.pi * a * a
        rv = a * np.sqrt(RHO * w / ETA)
        d = 1.0433 / rv
        k = (w / C) * (1 + d) - 1j * (w / C) * d
        Zc = RHO * C / S * (1 + d - 1j * d)
        c, s = np.cos(k * L), np.sin(k * L)
        a2, b2, c2, d2 = c, 1j * Zc * s, 1j * s / Zc, c
        A, B, Cm, D = A * a2 + B * c2, A * b2 + B * d2, Cm * a2 + D * c2, Cm * b2 + D * d2
    return A, B, Cm, D


def impedance(f, radii, lengths, mute=None):
    """Input impedance, and the radiated pressure at 1 m per unit mouthpiece pressure.
    mute: a termination (mutes.py) in place of the open end."""
    out = 1.0
    if mute is None:
        A, B, Cm, D = chain(f, radii, lengths)
        Zr = radiation(f, radii[-1])
        Zt = Zr
    else:
        i = len(lengths) - int(np.searchsorted(np.cumsum(lengths[::-1]), mute.cut))
        A, B, Cm, D = chain(f, radii[:i], lengths[:i])
        Zt, out, Zr = mute.termination(f, radii[i:] if i < len(radii) else radii[-1:], lengths[i:] if i < len(radii) else lengths[-1:] * 0)
    Zin = (A * Zt + B) / (Cm * Zt + D)
    Ub_over_pin = out / (A * Zt + B)
    # the pressure at 1 m of a source radiating the bell's power evenly (the power average
    # over directions): sqrt(W rho c / 4 pi) with W = Re(Zr) |Ub|^2; for a small bell it is
    # the monopole's rho w |Ub| / 4 pi, for a large one it levels off (the treble beams ahead)
    H = 1j * np.sqrt(np.maximum(Zr.real, 0) * RHO * C / (4 * np.pi)) * Ub_over_pin
    return Zin, H


def modes(f, Zin, fmax=2000.0, fmin=40.0):
    """Peaks of |Zin| as (freq, Q, peak impedance), each fitted from its half-power width."""
    m = np.abs(Zin)
    out = []
    for i in range(1, len(f) - 1):
        if f[i] > fmax or f[i] < fmin:
            continue
        if m[i] > m[i - 1] and m[i] >= m[i + 1]:
            # parabolic refine on the log magnitude
            la, lb, lc = np.log(m[i - 1 : i + 2])
            di = 0.5 * (la - lc) / (la - 2 * lb + lc)
            fp = f[i] + di * (f[1] - f[0])
            half = m[i] / np.sqrt(2)
            lo = i
            while lo > 0 and m[lo] > half:
                lo -= 1
            hi = i
            while hi < len(f) - 1 and m[hi] > half:
                hi += 1
            if lo == 0 or hi == len(f) - 1:
                continue
            Q = fp / (f[hi] - f[lo])
            out.append((fp, Q, float(m[i])))
    return out


if __name__ == "__main__":
    f = np.arange(1, 4000, 0.5)
    r, L = profile()
    print(f"length {L.sum():.3f} m, bell radius {r[-1]*1000:.1f} mm")
    Zin, H = impedance(f, r, L)
    ref = 233.08 / 2  # B-flat 2 series (the trumpet's open series)
    for fp, Q, Z in modes(f, Zin):
        n = round(fp / ref)
        print(f"{fp:8.1f} Hz  Q {Q:5.1f}  |Z| {Z/1e6:6.1f} MΩ  n={n:2d}  {1200*np.log2(fp/(n*ref)) if n else 0:+6.0f} c")
