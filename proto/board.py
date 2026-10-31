"""Synthetic soundboard: one set of modes that both loads the strings at the bridge and
radiates the sound.

A real soundboard's modes are not known from recordings, only their statistics:
modal density roughly constant (a plate; ribbed piano soundboards ~1 mode per 15–20 Hz,
Ege, Boutillon & Rébillat 2013) and modal damping (fitted, see OVERLAP). We draw one
random modal set per instrument with those statistics. Each mode m has
  - frequency ω_m, decay σ_m = ω_m·η(f)/2
  - amplitude φ_m of its mode shape at the bridge (Gaussian: a random point on the board)
  - amplitude r_cm of its radiation towards microphone c (Gaussian, independent per c),
    drawn apart from the rest (`rad_seed`): the radiation can be drawn again while the modes
    and their coupling to the strings stay as they are

Bridge admittance (driving-point mobility): Y(ω) = Σ φ_m² · jω / (ω_m² − ω² + 2jωσ_m).
Its mean real part is density/4 (per Hz) independent of damping, so Y is normalized by
that: mean Re Y_n = 1. The bridge coupling of a string partial at f is then κ·Y_n(f):
partials near a board mode lose energy fast, partials between modes ring long, as on a
real piano.

Radiated sound (impulse response bridge force → microphone c):
h_c(t) = Σ φ_m·r_cm·a(f_m)·e^{−σ_m t}·cos(ω_m t), with a(f) setting the mean spectral
envelope to the identified transfer H(f). A partial on a board mode both decays faster
and radiates more: radiation and coupling are consistent.

A draw of the radiation has notches: summed modes cancel at some frequencies, 30 dB deep
in one channel. With a note's fundamental in one, its upper partials take over on that
side (Grand F's D#7: its fundamental 31 dB under the response around it in the left
channel, its second partial 11 dB over; the recording has that partial 18 dB under the
fundamental). The recordings show no such notch, so per instrument the radiation's draw is
the one of NOTCH_TRIES whose deepest notch at a fundamental (88 keys, both channels) is the
least deep (`pick_rad_seed`, run by soundboard.py; the profile's "radiation" block): about
−12 dB, where one draw has its deepest at −20 … −36 dB.

Measured modes (`modes`, fit_body_modes.py): a few strong low modes a recording shows (a
guitar's top at ~220 Hz, its air at ~105 Hz; a 2.1 m grand's board at ~113 Hz), where the
partials of every note decay faster than the random draw says. Each is {f (Hz), q, g = φ²}
(σ = πf/q; the random modes' φ² is 1 on average), appended after the random ones (their
draws stay as they were).

The low modes' loss factor (`eta_low`, a project's [board] low_eta, the export's): each random mode
rings as with that loss factor where it rings longer than the constant overlap gives (below
overlap / (density·eta_low): 450 Hz at 4 %). A grand's board glued into its rim rings T60 ≈ 0.5 s
(Plath 2019, Bader & Plath 2020: Schroeder over the whole response), the overlap alone ~0.12 s; by
ear on the pianos 4 % (design log step 184). Such a board's low modes radiate to both microphones in
phase (`coherent_low`). They radiate at unit weight (`rad`): the engine radiates every
mode through a unit-variance shape (mic.rs mode_shape), so φ acts on the coupling alone.
"""
import os

import numpy as np
from scipy.signal import fftconvolve

DENSITY = 1 / 18.0    # modes per Hz
ETA = 0.02            # modal loss factor (used if OVERLAP is None)
# Constant modal overlap density·η(f)·f instead of constant η: fitted statistically
# (fine_structure.py). With constant η = 2 % the bridge coupling fluctuates far more
# below 1 kHz and less above than the recorded decay irregularity says; constant
# overlap 1 matches both. Equivalent to the same decay rate σ = π·overlap/density
# (~57 1/s, T60 ≈ 0.12 s) for every mode.
OVERLAP = 1.0
F_LO, F_HI = 35.0, 22000.0
F_MIN = 20.0
# the radiation's draws tried for the one with the least deep notch at a fundamental
NOTCH_TRIES = 64


H_FILTER_N = 4096     # taps of the transfer's filter at 48 kHz (scaled with the rate)


def min_phase(H_db, n):
    """Minimum-phase FIR (length n) with magnitude H_db on rfftfreq(n), via the cepstrum."""
    log_mag = np.log(10 ** (np.asarray(H_db) / 20))
    cep = np.fft.irfft(log_mag, n)
    fold = np.zeros(n)
    fold[0], fold[n // 2] = cep[0], cep[n // 2]
    fold[1:n // 2] = 2 * cep[1:n // 2]
    return np.fft.irfft(np.exp(np.fft.rfft(fold)), n)


def h_filter(H_grid_hz, H_db, sr):
    """The transfer H as a minimum-phase FIR at rate sr (H_FILTER_N taps at 48 kHz; the engine's
    mic.rs h_filter is the same)."""
    n = 1 << int(np.ceil(np.log2(H_FILTER_N * sr / 48000)))
    f = np.fft.rfftfreq(n, 1 / sr)
    return min_phase(np.interp(np.log2(np.clip(f, 1, None)), np.log2(H_grid_hz), H_db), n)


# admittance tables per board (its constructor's arguments: the draw is deterministic),
# shared by every Board drawn alike in this process
_GRIDS = {}


# a ringing board's low modes radiate to the microphone pair in phase (wavelengths of meters): fully
# below COHERENT_HZ[0], fading to independent draws at COHERENT_HZ[1] (linear in log frequency).
# Heavily damped, a band held many overlapping modes and their independent draws averaged to
# "unrelated"; ringing, one mode holds a band and its draws' signs set the pair's phase (Grand B's
# 63 Hz octave's mono sum 12 dB under the channels' power, the rolls' bass out of phase)
COHERENT_HZ = (100.0, 400.0)


def coherent_low(f, r):
    """The radiation draws r (channels, modes) with every channel's leaning on the first's below
    COHERENT_HZ: r_c = ρ·r_0 + √(1 − ρ²)·r_c, ρ 1 at the low end, 0 at the high (unit variance kept)."""
    lo, hi = np.log2(COHERENT_HZ[0]), np.log2(COHERENT_HZ[1])
    rho = np.clip((hi - np.log2(np.maximum(np.asarray(f, float), 1e-9))) / (hi - lo), 0.0, 1.0)
    out = np.array(r, float)
    out[1:] = rho * out[0] + np.sqrt(1 - rho ** 2) * out[1:]
    return out


class Board:
    def __init__(self, seed=1, density=DENSITY, eta=ETA, overlap=OVERLAP, channels=2, modes=None, rad_seed=0,
                 eta_low=None):
        # the table holds the random modes only (the measured ones are summed exactly)
        self._key = (seed, density, eta, overlap, channels, eta_low)
        rng = np.random.default_rng(seed)
        n = int((F_HI - F_LO) * density)
        # jittered grid: plate modes repel each other, spacings are not Poisson; at least F_MIN (a
        # sparse board's jitter, the strumstick's 200 Hz spacing, put its first mode at −0.9 Hz)
        self.f = np.maximum(F_LO + (np.arange(n) + rng.uniform(-0.4, 0.4, n)) / density, F_MIN)
        eta_f = eta if overlap is None else overlap / (density * self.f)
        self.sigma = np.pi * self.f * eta_f
        if eta_low is not None:     # the low modes ring as with eta_low where that is longer
            self.sigma = np.minimum(self.sigma, np.pi * self.f * eta_low)
        self.phi = rng.standard_normal(n)
        # the radiation from a draw of its own (rad_seed), the modes left as they are
        rrng = np.random.default_rng([seed, 2, rad_seed])
        self.r = rrng.standard_normal((channels, n))
        self.density = density
        # radiation of the horizontal polarization: it drives the board in-plane (bridge
        # rocking), so it reaches the microphones through other, independent amplitudes
        self.r_h = rrng.standard_normal((channels, n))
        if eta_low is not None:     # the ringing low modes reach both microphones in phase
            self.r, self.r_h = coherent_low(self.f, self.r), coherent_low(self.f, self.r_h)
        self.seed, self.rad_seed = seed, rad_seed
        self._grid = _GRIDS.get(self._key)
        self.rad = self.phi.copy()
        self.n_random = n
        self.modes = [dict(m) for m in (modes or [])]
        if self.modes:
            mf = np.array([m["f"] for m in self.modes], float)
            ms = np.pi * mf / np.array([m["q"] for m in self.modes], float)
            mp = np.sqrt([m["g"] for m in self.modes])
            # their radiation from a draw of its own: the random modes' draws stay as they were
            mrng = np.random.default_rng([seed, 1])
            self.f = np.concatenate([self.f, mf])
            self.sigma = np.concatenate([self.sigma, ms])
            self.phi = np.concatenate([self.phi, mp])
            self.rad = np.concatenate([self.rad, np.ones(len(mf))])
            rm, rhm = mrng.standard_normal((channels, len(mf))), mrng.standard_normal((channels, len(mf)))
            if eta_low is not None:
                rm, rhm = coherent_low(mf, rm), coherent_low(mf, rhm)
            self.r = np.concatenate([self.r, rm], axis=1)
            self.r_h = np.concatenate([self.r_h, rhm], axis=1)

    def admittance_raw(self, f, modes=slice(None)):
        w = 2 * np.pi * np.asarray(f, float)[..., None]
        wm = 2 * np.pi * self.f[modes]
        return (self.phi[modes] ** 2 * 1j * w / (wm ** 2 - w ** 2 + 2j * w * self.sigma[modes])).sum(axis=-1)

    def admittance(self, f):
        """Normalized bridge admittance Y_n(f), mean real part 1 (complex, same shape as f).
        Tabulated once on a fine grid (0.2 Hz below 2 kHz, coarser above, where modes
        are broad), then interpolated."""
        if self._grid is None:
            g = np.concatenate([np.arange(1.0, 2000.0, 0.2), np.arange(2000.0, 24000.0, 2.0)])
            rnd = slice(0, self.n_random)
            y = np.concatenate([self.admittance_raw(c, rnd) for c in np.array_split(g, 64)])
            self._grid = _GRIDS[self._key] = (g, y / (self.density / 4))
        g, y = self._grid
        f = np.asarray(f, float)
        out = np.interp(f, g, y.real) + 1j * np.interp(f, g, y.imag)
        if self.modes:
            out = out + self.admittance_raw(f, slice(self.n_random, None)) / (self.density / 4)
        return out

    def admittance_measured(self, f):
        """The measured modes' part of Y_n(f) (complex; 0 without them)."""
        f = np.asarray(f, float)
        if not self.modes:
            return np.zeros(f.shape, complex)
        return self.admittance_raw(f, slice(self.n_random, None)) / (self.density / 4)

    def impulse_response(self, H_grid_hz, H_db, sr=48000, length_s=0.5, polarization="v"):
        """Radiation IR per channel (channels, n), unit energy per channel, for the bridge
        force of the vertical ("v") or horizontal ("h") polarization: the modes at flat
        weights, then H as one minimum-phase filter (h_filter). With H as the modes' weights
        their tails (each ~50 dB under its peak, summed over the treble's strong modes) filled
        the bass 12–22 dB over H where H spans 60 dB (the guitar's E2 rang on its fundamental)."""
        r = self.r if polarization == "v" else self.r_h
        n = int(length_s * sr)
        t = np.arange(n) / sr
        keep = self.f < 0.45 * sr
        # mean spectral density of a decaying mode ∝ 1/σ: compensate so that the average power
        # spectrum is flat
        a = np.sqrt(self.sigma[keep])
        out = np.zeros((r.shape[0], n))
        for s in range(0, keep.sum(), 64):
            sl = slice(s, s + 64)
            fm, sm = self.f[keep][sl], self.sigma[keep][sl]
            modes = np.exp(-sm[:, None] * t) * np.cos(2 * np.pi * fm[:, None] * t)
            out += ((self.rad[keep][sl] * a[sl])[None, :] * r[:, keep][:, sl]) @ modes
        h = h_filter(H_grid_hz, H_db, sr)
        out = np.stack([fftconvolve(c, h)[:n] for c in out])
        return out / np.sqrt((out ** 2).sum(axis=1, keepdims=True))


def notch_depths(ir, sr, freqs):
    """Per channel and frequency: the response's level re its median over a third of an
    octave around it (dB; negative: in a notch), averaged in power over ±1 cent (a unison's
    strings)."""
    n = 1 << int(np.ceil(np.log2(16 * ir.shape[1])))
    f = np.fft.rfftfreq(n, 1 / sr)
    lf = np.log2(np.maximum(f, 1.0))
    freqs = np.asarray(freqs, float)
    out = np.zeros((ir.shape[0], len(freqs)))
    for c in range(ir.shape[0]):
        p = np.abs(np.fft.rfft(ir[c], n)) ** 2
        db = 10 * np.log10(p + 1e-30)
        for i, x in enumerate(freqs):
            band = np.abs(lf - np.log2(x)) < 1 / 6
            at = np.interp(x * 2 ** (np.linspace(-1, 1, 5) / 1200), f, p).mean()
            out[c, i] = 10 * np.log10(at + 1e-30) - np.median(db[band])
    return out


def pick_rad_seed(make, H_grid_hz, H_db, fundamentals, sr=48000, tries=NOTCH_TRIES):
    """Of the radiation draws 0 … tries−1, the one whose vertical response has its deepest
    notch at a fundamental (re its surroundings, any channel) least deep. make(rad_seed) →
    Board. Returns (rad_seed, its deepest notch dB, the first draw's)."""
    worst = []
    for k in range(tries):
        ir = make(k).impulse_response(H_grid_hz, H_db, sr, polarization="v")
        worst.append(float(notch_depths(ir, sr, fundamentals).min()))
    k = int(np.argmax(worst))
    return k, worst[k], worst[0]
