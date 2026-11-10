"""The player: a one-mass lip valve on the bore's modes, then radiation and wave steepening.

Throwaway spike. Units: Pa, m^3/s, m.
"""
import numpy as np
import bore, spec

SR = 44100
RHO = bore.RHO


_TRANSFER = {}  # the bore (slide, shorter side, mute) -> (frequencies, impedance, transfer)
_MODES = {}  # the bore and the grid -> its modes: a note's slide is found by several in a row, and the steps build each note again and again


def mute_key(mute):
    """What a termination is made of: its class and dimensions (a fit changes them in place, and a freed mute's id comes
    back with another's shape)."""
    return None if mute is None else (type(mute).__name__, tuple(sorted(vars(mute).items())))


def _remember(memo, keep, key, value):
    """At most `keep` entries, the oldest out first: a mute's fit tries thousands of shapes, and a transfer is 0.4 MB."""
    if len(memo) >= keep:
        del memo[next(iter(memo))]
    memo[key] = value
    return value


class Note:
    """The bore set up for one note: a fingering (as an extra tubing length) and its modes."""

    def __init__(self, midi, fgrid=np.arange(1, 2200, 0.5)):
        self.midi = midi
        self.mute = None  # a termination (mutes.py); set by muted()
        self.f = 440 * 2 ** ((midi - 69) / 12)
        # the fingering: the lowest open-series mode at or above the note, the valves lower it
        # an open mode up to 30 cents flat still counts (the player lips it up)
        _, base, length, self.less = spec.side(midi)
        n = max(int(np.ceil(self.f * 2 ** (-0.3 / 12) / base)), spec.S.get("nmin", 1))
        if 12 * np.log2(n * base / self.f) > spec.S.get("reach", 6.5):
            raise ValueError(f"no fingering for MIDI {midi}")
        self.n = n
        # the slide: the extra length that puts mode n on the note
        extra = length * (n * base / self.f - 1)
        for _ in range(4):
            fm = self._mode(extra, fgrid)[n - 1][0]
            extra += length * (fm / self.f - 1) * 1.0
        self.extra = extra
        self.modes = [m for m in self._mode(extra, fgrid) if m[1] > 4]

    def _bore(self, extra):
        return float(extra), float(self.less), mute_key(self.mute)

    def _mode(self, extra, fgrid):
        key = (self._bore(extra), len(fgrid), hash(fgrid.tobytes()))
        found = _MODES.get(key)
        if found is None:
            r, L = bore.profile(extra=extra, less=self.less)
            Z, _ = bore.impedance(fgrid, r, L, self.mute)
            found = _remember(_MODES, 20000, key, bore.modes(fgrid, Z, fmax=1400, fmin=spec.S["fmin"]))
        return list(found)

    def muted(self, mute, fgrid=np.arange(1, 2200, 0.5)):
        """The same slide with a mute in the bell: its modes and its transfer change."""
        self.mute = mute
        self.modes = [m for m in self._mode(self.extra, fgrid) if m[1] > 4]
        return self

    def transfer(self, freqs):
        """Input impedance and mouthpiece-to-1 m pressure transfer (complex, at freqs)."""
        key = self._bore(self.extra)
        if key not in _TRANSFER:
            r, L = bore.profile(extra=self.extra, less=self.less)
            fg = np.concatenate([np.arange(1, 4000, 1.0), np.arange(4000, 22050, 4.0), np.arange(22050, 96001, 16.0)])
            _remember(_TRANSFER, 64, key, (fg,) + bore.impedance(fg, r, L, self.mute))
        fg, Z, H = _TRANSFER[key]
        ip = lambda X: np.interp(freqs, fg, X.real, left=0) + 1j * np.interp(freqs, fg, X.imag, left=0)
        return ip(Z), ip(H)

    def retune(self, cents, fgrid=np.arange(1, 2200, 0.5)):
        """Moves the slide to lower the played pitch by `cents`."""
        total = bore.profile(extra=self.extra, less=self.less)[1].sum()
        self.extra += total * (2 ** (cents / 1200) - 1)
        self.modes = [m for m in self._mode(self.extra, fgrid) if m[1] > 4]


def modal_coeffs(modes, T=1 / SR):
    w = np.array([2 * np.pi * m[0] for m in modes])
    Q = np.array([m[1] for m in modes])
    Z = np.array([m[2] for m in modes])
    sp = -w / (2 * Q) + 1j * w * np.sqrt(1 - 1 / (4 * Q * Q))
    sm = np.conj(sp)
    r = Z * (w / Q) * sp / (sp - sm)
    E = np.exp(sp * T)
    return E, r * T / 2


def simulate(notes, seg, pm, lipf, lip_q=10.0, mu=1.5, width=0.01, y0=1.5e-4, firm=0.5, pref=3000.0, kick=None,
             breath=0.0, wander=0.0, lipwander=None, seed=1, sr=SR, soft=0.0, lips=None, burst=None):
    """Flow u and mouthpiece pressure p for sample-wise mouth pressure pm and lip frequency lipf.

    notes: list of Note; seg: per-sample index into notes (the fingering in use).
    firm: the player firms the lips with pressure, mass per area ~ (pm / pref)^firm.
    kick: per-sample lip velocity added (m/s): the tongue's release throws the lips open.
    sr: the rate the loop runs at (inputs given at that rate).
    soft: the lips close softly over this distance (m): the opening is a smooth ramp, not a kink.
    lips: the lips' modes as (frequency re lipf, Q, sign, compliance); sign +1 opens with the
    pressure across the lips (outward striking), -1 closes with it (inward); default one
    outward mode at lipf with Q lip_q.
    burst: per-sample multiplier of the breath noise (the attack's rush of air before the
    lips lock).
    breath: turbulence in the jet, a noise flow of this fraction of the flow (band 0.5–6 kHz).
    wander: the player's slow unsteadiness, a relative wobble of mouth pressure below about
    4 Hz; lipwander the same in lip tension (a fifth of wander when not given).
    """
    N = len(pm)
    rng = np.random.default_rng(seed)
    T = 1 / sr
    if breath > 0:
        from scipy.signal import butter, sosfilt
        nz = sosfilt(butter(2, [500, 6000], "bandpass", fs=sr, output="sos"), rng.standard_normal(N))
        nz *= breath / np.std(nz)
        if burst is not None:  # the breath's noise over time (the attack's rush of air)
            nz *= burst
    if wander > 0:
        from scipy.signal import butter, sosfiltfilt
        lp = butter(2, 4, "lowpass", fs=sr, output="sos")
        wp = sosfiltfilt(lp, rng.standard_normal(N)); wp *= wander / np.std(wp)
        wl = sosfiltfilt(lp, rng.standard_normal(N)); wl *= (0.2 * wander if lipwander is None else lipwander) / np.std(wl)
        pm = pm * (1 + wp)
        lipf = lipf * (1 + wl)
    mus = mu * np.maximum(pm / pref, 0.1) ** firm
    coeffs = [modal_coeffs(n.modes, T) for n in notes]
    lips = lips or [(1.0, lip_q, 1.0, 1.0)]
    E = np.concatenate([c[0] for c in coeffs])
    rT = np.concatenate([c[1] for c in coeffs])
    mcount = np.array([len(c[0]) for c in coeffs], np.int32)
    moff = np.concatenate([[0], np.cumsum(mcount)[:-1]]).astype(np.int32)
    lr, lq, ls, lc = (np.ascontiguousarray([l[i] for l in lips], float) for i in range(4))
    f64 = lambda a: np.ascontiguousarray(a, float)
    u = np.zeros(N)
    p = np.zeros(N)
    _lib().simulate(N, float(sr), f64(pm), f64(lipf), f64(mus), np.ascontiguousarray(seg, np.int32),
                    f64(kick if kick is not None else np.zeros(N)), f64(nz if breath > 0 else np.zeros(N)),
                    moff, mcount, f64(E.real), f64(E.imag), f64(rT.real), f64(rT.imag),
                    len(lips), lr, lq, ls, lc, float(width), float(y0), float(soft), float(RHO), u, p)
    return u, p


_LIB = None


def _lib():
    """The C loop (lips.c), built on first use."""
    global _LIB
    if _LIB is None:
        import ctypes
        lib = ctypes.CDLL(spec.build_c("lips.c", ["-O2"]))
        D = np.ctypeslib.ndpointer(np.float64, flags="C")
        I = np.ctypeslib.ndpointer(np.int32, flags="C")
        lib.simulate.argtypes = [ctypes.c_int, ctypes.c_double, D, D, D, I, D, D, I, I, D, D, D, D,
                                 ctypes.c_int, D, D, D, D, ctypes.c_double, ctypes.c_double,
                                 ctypes.c_double, ctypes.c_double, D, D]
        lib.simulate.restype = None
        _LIB = lib
    return _LIB


def radiate(u, notes, seg, nl=0.0, fade=0.01, sr=SR):
    """Pressure at 1 m. The flow makes the mouthpiece pressure through each fingering's
    impedance; that pressure steepens (nl: seconds per pascal) and leaves through the bell."""
    N = len(u)
    nfft = 1 << int(np.ceil(np.log2(N + sr)))
    freqs = np.fft.rfftfreq(nfft, 1 / sr)
    nf = int(fade * sr)
    tr, ws = [], []
    pin = np.zeros(nfft)
    for i, note in enumerate(notes):
        w = (seg == i).astype(float)
        if nf > 1:  # a moving average, centered
            c = np.concatenate([[0.0], np.cumsum(w)])
            lo = np.clip(np.arange(N) - nf // 2, 0, N); hi = np.clip(lo + nf, 0, N)
            w = (c[hi] - c[lo]) / nf
        Z, H = note.transfer(freqs)
        tr.append(H)
        ws.append(np.pad(w, (0, nfft - N)))
        pin += np.fft.irfft(np.fft.rfft(u * w, nfft) * Z, nfft)
    pin = steepen(pin, nl, sr=sr)
    out = np.zeros(nfft)
    for H, w in zip(tr, ws):
        out += np.fft.irfft(np.fft.rfft(pin * w) * H, nfft)
    return out[:N]


def steepen(x, amount, up=4, sr=SR):
    """Nonlinear propagation along the bore: a sample arrives earlier the higher its pressure.

    amount: seconds of advance per pascal (about 2.5e-8 for 1 m of trumpet bore)."""
    if amount <= 0:
        return x
    n = len(x)
    t = np.arange(n * up) / up
    xi = np.interp(t, np.arange(n), x)
    ta = t - amount * sr * xi
    ta = np.maximum.accumulate(ta)  # a shock: later samples cannot overtake
    return np.interp(np.arange(n), ta, xi)


def blow(n, release=0.1, sr=SR, ramp=0.008, first=0.7, lean=0.08):
    """A player's breath for one note: up to `first` of the pressure in `ramp`, then leaning
    into the rest (time constant `lean`): the treble arrives after the note (Iowa: the level
    within 20–35 ms, the treble 130–215 ms at mf and ff)."""
    t = np.arange(n) / sr
    e = np.where(t < ramp, first * np.sin(np.pi / 2 * t / ramp) ** 2,
                 first + (1 - first) * (1 - np.exp(-(t - ramp) / lean)))
    r = min(n, int(release * sr))
    e[n - r:] *= np.linspace(1, 0, r)
    return e


def envelope(n, attack=0.03, release=0.08, sr=SR):
    e = np.ones(n)
    a = min(n, int(attack * sr))
    e[:a] = np.sin(np.linspace(0, np.pi / 2, a)) ** 2
    r = min(n, int(release * sr))
    e[n - r :] *= np.linspace(1, 0, r)
    return e


def play(notes, seg, pm, lipf, kick=None, nl=0.0, os=4, **kw):
    """Simulates at os times the rate, radiates and steepens there, filters back down to SR.
    Inputs and outputs at SR. Returns the pressure at 1 m and the mouthpiece pressure."""
    from scipy.signal import resample_poly
    up = lambda a: np.repeat(a, os)
    k2 = None
    if kick is not None:
        k2 = np.zeros(len(kick) * os)
        k2[np.nonzero(kick)[0] * os] = kick[np.nonzero(kick)[0]]
    u, p = simulate(notes, up(seg), up(pm), up(lipf), kick=k2, sr=SR * os, **kw)
    x = radiate(u, notes, up(seg), nl=nl, sr=SR * os)
    if os == 1:
        return x, p
    return resample_poly(x, 1, os), resample_poly(p, 1, os)
