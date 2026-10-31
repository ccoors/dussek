"""Prototype: one note as hammer (or pluck) + coupled modal stiff strings (offline, numpy).

Model (following Chaigne & Askenfelt 1994, Weinreich 1977, Bank et al. 2010):
  - each string is a hinged stiff string, decomposed into sine modes
      f_k = k·f0·sqrt(1 + B·k²),   internal loss σ_k = b1 + b3·ω_k²
  - each mode is a 2nd-order resonator driven by the hammer force at the strike point
  - hammer: point mass with nonlinear felt, F = K·[compression]^p, one shared hammer
    head striking all unison strings (with a slight per-string imbalance, as on real
    hammers that never hit all strings equally)
  - unison strings are coupled through the bridge (Weinreich): per mode k the strings
    form a small linear system  ȧ = (diag(jω_s − σ_k) − κ·1·1ᵀ)·a  where κ is the
    bridge coupling (real part: energy loss into the soundboard, imaginary part:
    reactive coupling). The in-phase normal mode loses energy fast (prompt sound),
    the others slowly (aftersound) → two-stage decay and realistic beating.
  - each string also vibrates in the horizontal polarization: weakly excited (the hammer
    never strikes perfectly perpendicular), weakly coupled to the bridge, so it decays
    almost only by internal loss. It carries most of the late aftersound.
  - output: transverse force at the bridge, summed over strings and polarizations

During hammer contact (a few ms) the strings are integrated sample by sample as
uncoupled resonators; afterwards the coupled system decays freely and is evaluated
in closed form via its eigenmodes.
"""
from dataclasses import dataclass

import functools

import numpy as np


@dataclass
class NoteParams:
    f0: float = 261.6            # Hz
    B: float = 3.0e-4            # inharmonicity coefficient
    L: float = 0.62              # string length, m
    T: float = 670.0             # tension per string, N
    string_mass: float = 3.93e-3  # kg (one string)
    n_strings: int = 3
    detune_cents: tuple = (-0.6, 0.0, 0.7)
    # internal loss (air, material): σ = b1 + b3·ω²  (1/s)
    b1: float = 0.17
    b3: float = 5e-10
    loss_q: float = 2.0          # the loss law's exponent (string_loss): 2 is b1 + b3·ω²
    # bridge coupling per string (1/s): real part = loss into the soundboard
    kappa: float = 0.9
    kappa_reactive: float = 0.3   # imaginary part relative to the real part
    # normalized bridge admittance Y_n(f) (complex, mean real part 1), e.g. board.Board:
    # per partial κ_k = κ·(Y_n(f_k) + j·kappa_reactive). None: frequency-independent κ
    admittance: object = None
    # also let the reactive part of Y_n fluctuate per partial. Off: it makes the
    # partial-to-partial decay irregularity at 200–1000 Hz about twice the measured one
    # for Salamander (no effect for Iowa), also with the fitted coupling (fine_structure.py)
    admittance_reactive: bool = False
    # output gain per partial, gain(freqs Hz) → linear (soundboard.correction_db as a
    # magnitude per mode; scales the bridge force only, not the dynamics). None: 1
    mode_gain: object = None
    # horizontal polarization: excitation relative to vertical, bridge coupling, detuning
    horiz_gain: float = 0.045
    horiz_kappa: float = 0.02
    horiz_detune_cents: float = 0.3
    # the measured board modes' admittance (board.Board.admittance_measured; normalized, as
    # `admittance`) and the share of it the horizontal polarization feels: per partial
    # κ_h = horiz_kappa + κ·horiz_share·Re Y_measured(f) (a guitar's top mode damps both)
    horiz_admittance: object = None
    horiz_share: float = 0.0
    # hammer
    hammer_mass: float = 2.97e-3  # kg
    # felt: fitted to the change of spectrum between dynamics (fit_hammer.py, C4);
    # literature (Chaigne & Askenfelt): K = 4.5e9, p = 2.5 — clearly too bright at pp
    felt_K: float = 3.5 / 5e-4 ** 2.3   # 3.5 N at 0.5 mm compression
    felt_p: float = 2.3
    hammer_balance: tuple = (1.0, 0.92, 1.06)  # per-string felt stiffness factor
    # the felt's core: K_c·max(δ − core_depth, 0)^p on top of the soft layer (felt_force; N/m^p
    # before the balance, m); 0: none (design 2026-10-20-felt-core)
    core_K: float = 0.0
    core_depth: float = 0.0
    strike_pos: float = 0.12      # relative strike position x_h / L
    f_max: float = 20000.0
    # irregular strings: each partial of each unison string off by σ·√k·g cents, g a
    # fixed Gaussian draw per (note, string, partial) (string_gauss, the engine draws the
    # same). Without it every partial beats at a rate ∝ its frequency, the beating dips
    # sweep smoothly across the partials (diagonal stripes in a spectrogram) and the note
    # sounds like a chorused oscillator pair: "string-like" (fine_structure.py fits σ)
    scatter_cents: float = 0.0
    note: int = 0
    # excitation: "hammer" (contact simulated) or "pluck": the string released from rest
    # out of a triangle with its apex at pluck_beta (pluck_amplitudes; no contact); "tangent":
    # the pluck's limit β → 0, the string pushed at its end (the clavichord: the tangent is
    # the speaking length's end; pluck_beta unused)
    excitation: str = "hammer"
    pluck_beta: float = 0.2          # pluck point re the speaking length
    pluck_fq: float = 5000.0         # Hz, quill lowpass at the reference velocity
    pluck_amp: float = 1.5e-3        # m, apex displacement
    pluck_gain_db_per_step: float = 3.0
    pluck_fq_oct_per_step: float = 0.5


_M64 = (1 << 64) - 1


def _mix64(x):
    """splitmix64 finalizer (engine: physics::mix64)."""
    x = (x + 0x9E3779B97F4A7C15) & _M64
    x = ((x ^ (x >> 30)) * 0xBF58476D1CE4E5B9) & _M64
    x = ((x ^ (x >> 27)) * 0x94D049BB133111EB) & _M64
    return x ^ (x >> 31)


@functools.lru_cache(maxsize=None)
def string_gauss(note, string, k):
    """Standard Gaussian per (note, string, partial k), deterministic (Box–Muller on two
    hashed uniforms); engine: physics::string_gauss."""
    h1 = _mix64((note << 24) | (string << 20) | int(k))
    h2 = _mix64(h1)
    u1 = ((h1 >> 11) + 1) * 2.0 ** -53
    u2 = (h2 >> 11) * 2.0 ** -53
    return np.sqrt(-2.0 * np.log(u1)) * np.cos(2.0 * np.pi * u2)


def pluck_amplitudes(k, f, beta, f_q, amp, steps=0.0, gain_db=3.0, fq_oct=0.5, tangent=False):
    """Starting displacement per partial k (frequency f) of a string released from rest out
    of a triangle with apex height amp at beta: 2·amp/(π²β(1−β))·sin(kπβ)/k², times the
    quill lowpass 1/√(1 + (f/f_q′)²). `steps` dynamic steps from the reference (log2 of
    the pluck speed): f_q′ = f_q·2^(fq_oct·steps), gain 10^(gain_db·steps/20). `tangent`:
    the limit β → 0, a ramp from amp at the struck end, 2·amp/(πk) (no comb; beta unused).
    Engine: physics::pluck_amplitude."""
    k, f = np.asarray(k, float), np.asarray(f, float)
    fq = f_q * 2.0 ** (fq_oct * steps)
    shape = (2 * amp / (np.pi * k) if tangent
             else 2 * amp / (np.pi ** 2 * beta * (1 - beta)) * np.sin(k * np.pi * beta) / k ** 2)
    return shape / np.sqrt(1 + (f / fq) ** 2) * 10 ** (gain_db * steps / 20)


OMEGA_REF = 2 * np.pi * 1000.0


def string_loss(b1, b3, w, q=2.0):
    """The string's internal loss rate (1/s) at angular frequencies w: b1 + b3·ω_ref²·(ω/ω_ref)^q
    (q = 2: b1 + b3·ω²). A q below 2 lets the high partials ring longer for the same loss at 1 kHz."""
    return b1 + b3 * OMEGA_REF ** 2 * (np.asarray(w, float) / OMEGA_REF) ** q


def felt_force(K, p, core_K, core_depth, delta):
    """The felt's force (N) at the compression delta (m; ≤ 0: no contact): K·δ^p, plus its core
    core_K·max(δ − core_depth, 0)^p, a stiff layer under the soft one. Without a core exactly
    K·δ^p. Engine: physics::felt_force."""
    d = np.maximum(delta, 0.0)
    F = K * d ** p
    if np.any(core_K):
        F = F + core_K * np.maximum(delta - core_depth, 0.0) ** p
    return F


def mode_table(p: NoteParams, sr: int):
    """Mode numbers, angular frequencies (per string) and internal loss rates."""
    k = np.arange(1, int(p.f_max / p.f0) + 1)
    k = k[k * p.f0 * np.sqrt(1 + p.B * k ** 2) < min(p.f_max, 0.45 * sr)]
    detune = 2 ** (np.array(p.detune_cents[: p.n_strings]) / 1200)
    f = detune[:, None] * k[None, :] * p.f0 * np.sqrt(1 + p.B * k[None, :] ** 2)  # (S, K)
    if p.scatter_cents > 0 and p.n_strings > 1:
        g = np.array([[string_gauss(p.note, i, kk) for kk in k] for i in range(p.n_strings)])
        f = f * 2 ** (p.scatter_cents * np.sqrt(k)[None, :] * g / 1200)
    w = 2 * np.pi * f
    sigma_int = string_loss(p.b1, p.b3, w, p.loss_q)
    return k, w, sigma_int


def coupled_eigenmodes(w, sigma_int, kappa):
    """Per mode k: eigenvalues λ (K, S) and eigenvectors V (K, S, S) of the coupled
    unison system  ȧ = (diag(jω − σ_int) − κ·1·1ᵀ)·a  (κ complex)."""
    S, K = w.shape
    Mk = np.zeros((K, S, S), dtype=complex)
    idx = np.arange(S)
    Mk[:, idx, idx] = (1j * w - sigma_int).T
    Mk -= kappa
    lam, V = np.linalg.eig(Mk)
    return lam, V


# the explicit contact step is stable while dt·√Λ/2 < 1 (Λ: the contact's linearized
# stiffness over the masses it couples, contact_ratio); it grew unbounded from about 0.8
# (Iowa's C7 at ff peaked at 330 N, its neighbors at 100–150; Grand D's NaN), so the
# contact runs in sub-steps that keep the ratio at most CONTACT_RATIO_MAX
CONTACT_RATIO_MAX = 0.5


def contact_ratio(p: NoteParams, velocity: float, sr: int, k=None, S=None):
    """dt·√Λ/2 of the contact at the audio rate: each string's felt stiffness p·K·δ^(p−1) at
    the compression δ the hammer's energy would reach on rigid strings (an upper bound,
    the felt stiffens as it is compressed), over the hammer mass (all strings) and the
    string's mass at the strike point, Σ sin²(kπx_h)/(m/2) over the simulated modes."""
    if k is None:
        k, w, _ = mode_table(p, sr)
        S = w.shape[0]
    Ks = p.felt_K * np.array(p.hammer_balance[:S])
    delta = ((p.felt_p + 1) * p.hammer_mass * velocity ** 2 / (2 * Ks.sum())) ** (1 / (p.felt_p + 1))
    kc = p.felt_p * Ks * delta ** (p.felt_p - 1)
    if p.core_K:     # the core's stiffness at the same compression (an upper bound: it stops the hammer sooner)
        kc = kc + p.felt_p * p.core_K * np.array(p.hammer_balance[:S]) * max(delta - p.core_depth, 0.0) ** (p.felt_p - 1)
    phi = np.sin(k * np.pi * p.strike_pos)
    lam = kc.max() * np.sum(phi ** 2) / (p.string_mass / 2) + kc.sum() / p.hammer_mass
    return float(np.sqrt(lam) / sr / 2)


def contact_substeps(p: NoteParams, velocity: float, sr: int, k=None, S=None):
    """Contact sub-steps per sample: 1 wherever the audio rate is stable enough."""
    return max(1, int(np.ceil(contact_ratio(p, velocity, sr, k, S) / CONTACT_RATIO_MAX)))


def free_modes(p: NoteParams, velocity: float, sr: int = 48000, substeps=None):
    """Hammer contact (simulated) and the coupled free decay after it, as eigenmodes.

    Returns a dict: k (K,) mode numbers; lam (K, E) complex eigenvalues and weight
    (K, E) complex bridge-force amplitudes, so that partial k's bridge force is
    Re Σ_e weight·exp(lam·t) with t = 0 at sample n_contact − 1 (E = 2S: vertical and
    horizontal polarizations); out_contact, force, n_contact for the contact phase."""
    k, w, sigma_int = mode_table(p, sr)
    S, K = w.shape
    dt = 1.0 / sr
    m_modal = p.string_mass / 2                       # modal mass of a sine mode
    phi_h = np.sin(k * np.pi * p.strike_pos)          # mode shapes at the strike point (K,)
    bridge = p.T * (k * np.pi / p.L) * (-1.0) ** k    # T·∂φ/∂x at x = L (K,)
    if p.mode_gain is not None:                      # output gain per partial (radiation)
        bridge = bridge * p.mode_gain(w.mean(axis=0) / (2 * np.pi))
    felt_K = p.felt_K * np.array(p.hammer_balance[:S])
    core_K = p.core_K * np.array(p.hammer_balance[:S])

    # bridge coupling per partial (K,), complex
    if p.admittance is None:
        kappa_v = np.full(K, p.kappa * (1 + 1j * p.kappa_reactive))
    else:
        y_n = p.admittance(w.mean(axis=0) / (2 * np.pi))
        if not p.admittance_reactive:
            y_n = y_n.real
        kappa_v = p.kappa * (y_n + 1j * p.kappa_reactive)

    if p.excitation in ("pluck", "tangent"):
        # released from rest at m = 0: every string of the note starts from the same shape
        a = pluck_amplitudes(k, w.mean(axis=0) / (2 * np.pi), p.pluck_beta, p.pluck_fq, p.pluck_amp,
                             np.log2(max(velocity, 1e-3)), p.pluck_gain_db_per_step, p.pluck_fq_oct_per_step,
                             tangent=p.excitation == "tangent")
        C = np.tile(a, (S, 1)).astype(complex)
        n_contact, out_contact, force = 0, np.zeros(0), np.zeros(0)
    else:
        # contact phase: uncoupled resonators with the single-string loss σ_int + Re κ,
        # in N sub-steps per sample (contact_substeps); the output keeps the last of them
        N = substeps or contact_substeps(p, velocity, sr, k, S)
        h = dt / N
        sigma = sigma_int + kappa_v.real
        wd = np.sqrt(np.maximum(w ** 2 - sigma ** 2, 1e-9))
        r = np.exp(-sigma * h)
        a1, a2 = 2 * r * np.cos(wd * h), r * r
        g = h * h / m_modal

        x0 = np.zeros((S, K))     # x[n]
        x1 = np.zeros((S, K))     # x[n-1]
        eta, eta_prev = 0.0, -velocity * h  # hammer moving towards the strings
        max_steps = int(0.02 * sr)
        force = np.zeros(max_steps)
        out_contact = np.zeros(max_steps)
        separated_for = 0
        n = 0
        for n in range(max_steps):
            for _ in range(N):
                y_h = x0 @ phi_h                                   # string displacement at strike point (S,)
                F = felt_force(felt_K, p.felt_p, core_K, p.core_depth, eta - y_h)  # force on each string (S,)
                x_new = a1 * x0 - a2 * x1 + g * F[:, None] * phi_h[None, :]
                eta_new = 2 * eta - eta_prev - h * h * F.sum() / p.hammer_mass
                x1, x0 = x0, x_new
                eta_prev, eta = eta, eta_new
            force[n] = F.sum()
            out_contact[n] = (x0 @ bridge).sum()                         # vertical
            separated_for = separated_for + 1 if F.sum() == 0 and n > 10 else 0
            if separated_for > int(0.002 * sr):
                break
        n_contact = n + 1

        # complex amplitudes at the last sub-step but one from the last two: x[m] = Re(C·z^m)
        # in sub-steps; then back to sample n_contact − 1, N − 1 sub-steps earlier
        z = np.exp((-sigma + 1j * wd) * h)
        C = x1 + 1j * (x1 * z.real - x0) / np.maximum(z.imag, 1e-12)   # (S, K)
        if N > 1:
            C = C * z ** -(N - 1)

    # coupled free decay: project onto the eigenmodes of each mode's unison system,
    # for the vertical and the (weakly excited, weakly coupled) horizontal polarization.
    # The horizontal motion is driven by the hammer too (horiz_gain × the vertical force,
    # an oblique strike), so during the few ms of contact it is horiz_gain × the vertical
    # motion (see render) and continues from C_h = g·C.
    # Switching it on only after the contact made a step in the bridge force, audible
    # as a click through a treble-heavy H.
    lams, weights = [], []
    w_h = w * 2 ** (p.horiz_detune_cents / 1200)
    kappa_h = p.horiz_kappa
    if p.horiz_admittance is not None and p.horiz_share:
        kappa_h = (p.horiz_kappa + p.kappa * p.horiz_share * p.horiz_admittance(w.mean(axis=0) / (2 * np.pi)).real)[:, None, None]
    for ww, kappa, C0 in ((w, kappa_v[:, None, None], C),
                          (w_h, kappa_h, p.horiz_gain * C)):
        lam, V = coupled_eigenmodes(ww, sigma_int, kappa)  # (K, S), (K, S, S)
        q = np.linalg.solve(V, C0.T[:, :, None])[:, :, 0]  # modal coordinates (K, S)
        lams.append(lam)
        weights.append(bridge[:, None] * V.sum(axis=1) * q)  # bridge force per eigenmode
    lam, weight = np.concatenate(lams, axis=1), np.concatenate(weights, axis=1)
    return {"k": k, "lam": lam, "weight": weight, "n_contact": n_contact,
            "out_contact": out_contact, "force": force, "S": S, "K": K, "w": w}


FREE_BLOCK = 1024   # samples per block of free_decay


def free_decay(lam, weight, n, bounds, sig_d, dt):
    """Re Σ weight·e^(λ·m·dt) over the modes (K, S) for m = 0 … n − 1, λ − σ_d per partial
    within the damped segments of `bounds` [(start, damped), …, (n, _)]. One table
    e^(λ·j·dt), j < FREE_BLOCK, per segment, and per block the weights moved to its start:
    a matrix product per block instead of an exp per mode and sample."""
    out = np.zeros(n)
    wgt = weight.ravel().astype(complex)
    j = np.arange(FREE_BLOCK)
    for (b0, on), (b1, _) in zip(bounds[:-1], bounds[1:]):
        lam_dt = ((lam - (sig_d[:, None] if on else 0)) * dt).ravel()
        table = np.exp(lam_dt[:, None] * j[None, :])            # (modes, FREE_BLOCK)
        for s in range(b0, b1, FREE_BLOCK):
            L = min(FREE_BLOCK, b1 - s)
            out[s:s + L] = ((wgt * np.exp(lam_dt * (s - b0))) @ table[:, :L]).real
        wgt = wgt * np.exp(lam_dt * (b1 - b0))
    return out


def render(p: NoteParams, velocity: float, sr: int = 48000, duration: float = 6.0, split=False,
           dampers=None, damper_sigma=None, substeps=None):
    """Render one note. velocity = hammer speed in m/s. Returns (signal, info).

    split=True: signal is (2, n), the bridge force of the vertical and of the horizontal
    polarization separately (they drive the soundboard differently, see soundboard.py);
    otherwise their sum.
    dampers: [(t_s, on), …] from which time (s, from the strike) the damper rests on the
    strings (on) or is lifted; damper_sigma(freqs) → extra decay rate (1/s) per partial
    while it rests. The closed-form decay simply continues with the changed rates."""
    fm = free_modes(p, velocity, sr, substeps)
    lam, weight, n_contact = fm["lam"], fm["weight"], fm["n_contact"]
    out_contact, force, S, K = fm["out_contact"], fm["force"], fm["S"], fm["K"]
    dt = 1.0 / sr

    n_free = int(duration * sr) - n_contact
    free = np.zeros((2, n_free + 2))
    # damping schedule in free-decay samples (index 0 = sample n_contact − 1)
    sig_d = np.zeros(K)
    if dampers and damper_sigma is not None:
        sig_d = damper_sigma(fm["w"].mean(axis=0) / (2 * np.pi))
    bounds = [(0, False)] + [(max(int(t * sr) - n_contact + 1, 0), on) for t, on in (dampers or [])]
    bounds = [(b, on) for b, on in bounds if b < n_free + 2] + [(n_free + 2, False)]
    for pol, sl in enumerate((slice(0, S), slice(S, 2 * S))):
        free[pol] = free_decay(lam[:, sl], weight[:, sl], n_free + 2, bounds, sig_d, dt)
    # out_contact holds x[1..N]; the free decay starts at x[N-1], so skip two samples.
    # During contact the horizontal motion is horiz_gain × the vertical one.
    contact = np.stack([out_contact[:n_contact], p.horiz_gain * out_contact[:n_contact]])
    signal = np.concatenate([contact, free[:, 2:]], axis=1)
    if not split:
        signal = signal.sum(axis=0)
    info = {"contact_ms": 1000 * np.count_nonzero(force[:n_contact]) / sr,
            "peak_force_N": float(force.max()) if len(force) else 0.0, "n_modes": K, "n_strings": S,
            "force": force[:n_contact]}
    return signal, info
