#!/usr/bin/env python3
"""Soundboard / radiation: from bridge force to the recorded sound.

1. Identification (per instrument profile): for every note and dynamic, the measured
   partial levels minus the synthesized bridge-force partial levels give the transfer
   magnitude H at each partial frequency, plus an unknown gain per note (recordings are
   partly normalized per note). Partials of different notes overlap in frequency, so
   H(f) is identifiable across the keyboard up to one constant. Solved as regularized
   least squares: H on a log-frequency grid (piecewise linear), one gain per note.
2. Synthesis: the synthetic soundboard's modes (board.py, the same modes that set the
   bridge coupling) radiate with mean spectral envelope H, plus a diffuse room part
   whose decay T60(f) is measured between the recorded partials (analysis/body_decay.py).
   Listening showed fine structure matters more than an exact envelope: a smooth H
   filter alone still sounds like a bare string.

Usage: soundboard.py --profile grand_a      (identify H, store it in the fit document)
"""
import argparse
import json
import os
import sys
import tempfile
from multiprocessing import Pool

from dataclasses import replace

import numpy as np
import soundfile as sf
from scipy.signal import fftconvolve

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import analyze as A  # noqa: E402
import fit_hammer as FH  # noqa: E402
import damper  # noqa: E402
import scale  # noqa: E402
from board import min_phase, pick_rad_seed  # noqa: E402

GRID_HZ = 20.0 * 2 ** (np.arange(0, 10.05, 1 / 6))  # 20 Hz … 20.5 kHz, 1/6 octave
SMOOTH = 2.0      # weight of the second-difference penalty on H (dB)
SYNTH_GAIN = 1e-3  # fixed gain for synthesized bridge force before analysis
# highest partial number used: [strings] id_max_partial (40 by default; the uprights' and Iowa's 60)
EDGE_ROWS = 20     # partial levels needed beyond a frequency for it to count as covered
# … at the low edge, for sets with few bass rows: [strings] edge_rows_low (Grand K, the harp, the guitar …)


def hat_basis(f):
    """Piecewise-linear interpolation weights of frequencies f on GRID_HZ (log axis)."""
    x = np.log2(np.asarray(f) / GRID_HZ[0]) * 6
    i = np.clip(np.floor(x).astype(int), 0, len(GRID_HZ) - 2)
    w = np.clip(x - i, 0, 1)
    M = np.zeros((len(x), len(GRID_HZ)))
    M[np.arange(len(x)), i] = 1 - w
    M[np.arange(len(x)), i + 1] = w
    return M


_profile = None


# the correction in bands above the tracked partials (design 2026-10-20-bass-high-band): the analysis
# tracks 60 partials, which end at 2.2–4.5 kHz in A0–C2; there the recordings' attack lay 12–24 dB
# over the engine's at 4–16 kHz, and C had no data. Third-octave bands from 1.12 × a note's highest
# tracked partial to 16 kHz, the attack's window: recording − synthesized bridge force, as a
# partial row is
BAND_TOP_HZ = 16000.0
BAND_ATTACK = (0.02, 0.15)     # s after the onset


def band_centers(f_lo):
    """Third-octave centers 1000·2^(k/3) from 1.12·f_lo up to BAND_TOP_HZ (none above it)."""
    k0 = int(np.ceil(3 * np.log2(1.12 * max(f_lo, 1.0) / 1000.0) - 1e-9))
    k1 = int(np.floor(3 * np.log2(BAND_TOP_HZ / 1000.0) + 1e-9))
    return 1000.0 * 2.0 ** (np.arange(k0, k1 + 1) / 3) if k1 >= k0 else np.zeros(0)


def band_levels(x, sr, on, centers):
    """dB of the mean power in each third-octave band (center·2^(±1/6)) over BAND_ATTACK after `on`."""
    seg = np.asarray(x, float)[on + int(BAND_ATTACK[0] * sr): on + int(BAND_ATTACK[1] * sr)]
    p = np.abs(np.fft.rfft(seg * np.hanning(len(seg)))) ** 2
    f = np.fft.rfftfreq(len(seg), 1 / sr)
    return np.array([10 * np.log10(p[(f >= c * 2 ** (-1 / 6)) & (f < c * 2 ** (1 / 6))].mean() + 1e-30) for c in centers])


BAND_NOISE_SPAN = (0.5, 0.9)   # share of the file the noise is read in (felt_bands' span)
BAND_CLEAR_DB = 10.0           # a band row counts this far over its noise only


def clear_band_levels(x, sr, on, centers):
    """`band_levels` and, per band, whether it lies BAND_CLEAR_DB over the recording's noise there
    (the median of attack-length frames over BAND_NOISE_SPAN). Grand D's pp and mp lie at their
    floor above 6 kHz in the bass: those rows were noise against a nearly silent model."""
    x = np.asarray(x, float)
    lv = band_levels(x, sr, on, centers)
    n = int(BAND_ATTACK[1] * sr) - int(BAND_ATTACK[0] * sr)
    a, b = (int(f * len(x)) for f in BAND_NOISE_SPAN)
    frames = [band_levels(x[i:i + n], sr, -int(BAND_ATTACK[0] * sr), centers) for i in range(a, max(b - n, a + 1), n)]
    noise = np.median(np.array(frames), axis=0)
    return lv, lv >= noise + BAND_CLEAR_DB


def _onset(x):
    x = np.asarray(x, float)
    return int(np.argmax(np.abs(x) > np.abs(x).max() * 0.05))


def synth_levels(args):
    dataset, note = args
    global _profile
    if _profile is None:
        _profile = scale.Profile(dataset)
    params = scale.note_params(_profile, note)
    out, bands = {}, {}
    centers = band_centers(500.0 / 1.12)
    for dyn, v in _profile.hammer_speeds().items():
        y, _ = scale.M.render(params, v, duration=3.0)
        bands[dyn] = (centers, band_levels(y * SYNTH_GAIN, 48000, _onset(y), centers))
        fd, path = tempfile.mkstemp(suffix=".wav")
        os.close(fd)
        sf.write(path, (y * SYNTH_GAIN)[:, None], 48000, subtype="FLOAT")
        r = A.analyze({"path": path, "note": note, "layer": 0, "layers": 0, "dynamic": dyn, "tail_cut_s": 0.0})
        os.remove(path)
        if "error" not in r:
            out[dyn] = {p["n"]: (p["freq"], p["level_db"] + r["peak_dbfs"]) for p in r["partials"]}
    return note, out, bands


def identify(profile_name):
    prof = scale.Profile(profile_name)
    keys = FH.D.dynamics(profile_name)
    meas = {}
    n_max = scale.strings(profile_name).get("id_max_partial", 40)
    use_bands = bool(scale.strings(profile_name).get("band_correction", False))
    band_samples = {}
    for s in prof.samples:
        if s["B_fit_resid_cents"] >= 8:
            continue
        for dyn, key in keys.items():
            if FH.D.layer_key(profile_name, s) == key:
                meas[(s["note"], dyn)] = {p["n"]: p["level_db"] + s["peak_dbfs"] for p in s["partials"]
                                          if p["range_db"] >= FH.MIN_RANGE_DB and p["n"] <= n_max}
                band_samples[(s["note"], dyn)] = s
    notes = sorted({n for n, _ in meas})
    with Pool() as pool:
        res = pool.map(synth_levels, [(profile_name, n) for n in notes])
    synth = {n: out for n, out, _ in res}
    synth_bands = {n: b for n, _, b in res}

    rows_f, rows_note, rows_y = [], [], []
    for (note, dyn), m in meas.items():
        sl = synth.get(note, {}).get(dyn, {})
        for n, lvl in m.items():
            if n in sl:
                f, ls = sl[n]
                rows_f.append(f)
                rows_note.append(notes.index(note))
                rows_y.append(lvl - ls)
    rows_f, rows_note, rows_y = map(np.array, (rows_f, rows_note, rows_y))
    # covered range: beyond the EDGE_ROWS-th lowest/highest partial, H would rest on a
    # handful of (often noise-inflated) points (Iowa: one partial at 18 kHz made H rise
    # by 16 dB above 13 kHz, audible as clicks in the top octave); drop those rows
    fs = np.sort(rows_f)
    lo, hi = fs[scale.strings(profile_name).get("edge_rows_low", EDGE_ROWS) - 1], fs[-EDGE_ROWS]
    keep = (rows_f >= lo) & (rows_f <= hi)
    rows_f, rows_note, rows_y = rows_f[keep], rows_note[keep], rows_y[keep]

    # unknowns: H on the grid, one gain per note (the first note's gain fixed at 0)
    Hb = hat_basis(rows_f)
    G = np.zeros((len(rows_y), len(notes)))
    G[np.arange(len(rows_y)), rows_note] = 1
    X = np.hstack([Hb, G[:, 1:]])
    D = np.diff(np.eye(len(GRID_HZ)), 2, axis=0) * SMOOTH
    Xr = np.vstack([X, np.hstack([D, np.zeros((len(D), len(notes) - 1))])])
    yr = np.concatenate([rows_y, np.zeros(len(D))])
    coef, *_ = np.linalg.lstsq(Xr, yr, rcond=None)
    H = coef[: len(GRID_HZ)]
    gains = np.concatenate([[0.0], coef[len(GRID_HZ):]])
    resid = rows_y - X @ coef
    # beyond the data, hold the edge value (no extrapolation), then roll off above 16 kHz
    H = np.where(GRID_HZ < lo, np.interp(lo, GRID_HZ, H), H)
    H = np.where(GRID_HZ > hi, np.interp(hi, GRID_HZ, H), H)
    H -= 12 * np.log2(np.maximum(GRID_HZ / 16000, 1))
    c_f, c_note, c_resid = rows_f, np.array(notes)[rows_note], resid
    if use_bands:   # the bands' residuals against H as it plays (held beyond the data), before its normalization
        bf, bn, by = band_rows(profile_name, band_samples, synth_bands, notes)
        if len(bf):
            br = by - hat_basis(bf) @ H - gains[[notes.index(n) for n in bn]]
            c_f, c_note, c_resid = np.concatenate([rows_f, bf]), np.concatenate([c_note, bn]), np.concatenate([resid, br])
            print(f"  {len(bf)} band rows above the tracked partials")
    H -= H.max()  # normalize: maximum of H = 0 dB
    corr, resid2 = identify_correction(c_f, c_note, c_resid)
    resid2 = resid2[: len(rows_f)]
    print(f"{profile_name}: {len(rows_y)} partial levels from {len(meas)} recordings, "
          f"rms residual {np.sqrt(np.mean(resid ** 2)):.1f} dB, "
          f"{np.sqrt(np.mean(resid2 ** 2)):.1f} dB with the per-note correction")
    print(f"  data covers {lo:.0f} Hz … {hi:.0f} Hz")
    return {"grid_hz": GRID_HZ.tolist(), "H_db": H.tolist(), "data_range_hz": [float(lo), float(hi)],
            "note_gains_db": dict(zip(map(str, notes), gains.tolist())),
            "rms_residual_db": float(np.sqrt(np.mean(resid ** 2))),
            "correction": corr}, (rows_f, rows_y - G @ gains)


def band_rows(profile_name, band_samples, synth_bands, notes):
    """(f, note, y) of the band rows: per (note, dynamic) the recording's attack band levels from
    1.12 × its highest tracked partial up, minus the synthesized bridge force's in the same bands;
    only the bands clear of the recording's noise (`clear_band_levels`)."""
    bf, bn, by = [], [], []
    for (note, dyn), s in band_samples.items():
        # a take without tracked partials (a dead or misread take) has no bands above them
        if note not in notes or dyn not in synth_bands.get(note, {}) or not s["partials"]:
            continue
        centers = band_centers(max(p["freq"] for p in s["partials"]))
        if not len(centers):
            continue
        x, sr = sf.read(FH.D.sample_path(profile_name, s), always_2d=True)
        rec, clear = clear_band_levels(x.mean(axis=1), sr, int(s["onset_s"] * sr), centers)
        sc, sl = synth_bands[note][dyn]
        syn = np.interp(np.log2(centers), np.log2(sc), sl)
        bf += list(centers[clear])
        bn += [note] * int(clear.sum())
        by += list((rec - syn)[clear])
    return np.array(bf), np.array(bn, float), np.array(by)


CORR_NOTES = np.arange(21, 109, 6)   # note anchors of the correction
CORR_SMOOTH_F = 2.0                  # second-difference penalty along frequency
CORR_SMOOTH_N = 2.0                  # … and along the keyboard
CORR_RIDGE = 0.3                     # pulls the correction to 0 where there is no data


def identify_correction(f, note, resid):
    """Note-dependent correction C(f, note) of the transfer H, fitted to the residuals of
    the frequency-only H. H is shared by all notes, but a treble fundamental and a mid
    note's 4th partial at the same frequency want different corrections (strings end on
    different parts of the bridge; the hammer's spectrum also changes along the
    keyboard): in the treble the upper partials came out 4–14 dB too strong. Bilinear on
    (log f grid, note anchors), smooth in both directions; the residuals have zero mean
    per note (note gains were fitted), so C only redistributes within a note."""
    Bf = hat_basis(f)                                       # (rows, F)
    xn = np.clip(np.interp(note, CORR_NOTES, np.arange(len(CORR_NOTES))), 0, len(CORR_NOTES) - 1)
    i = np.clip(np.floor(xn).astype(int), 0, len(CORR_NOTES) - 2)
    w = xn - i
    Bn = np.zeros((len(f), len(CORR_NOTES)))
    Bn[np.arange(len(f)), i] = 1 - w
    Bn[np.arange(len(f)), i + 1] = w
    F, N = len(GRID_HZ), len(CORR_NOTES)
    X = (Bf[:, :, None] * Bn[:, None, :]).reshape(len(f), F * N)
    Df = np.kron(np.diff(np.eye(F), 2, axis=0), np.eye(N)) * CORR_SMOOTH_F
    Dn = np.kron(np.eye(F), np.diff(np.eye(N), 2, axis=0)) * CORR_SMOOTH_N
    R = np.eye(F * N) * CORR_RIDGE
    A = np.vstack([X, Df, Dn, R])
    b = np.concatenate([resid, np.zeros(len(Df) + len(Dn) + len(R))])
    c, *_ = np.linalg.lstsq(A, b, rcond=None)
    C = c.reshape(F, N)
    return {"notes": CORR_NOTES.tolist(), "C_db": C.T.tolist()}, resid - X @ c


def correction_rows(sb, room):
    """The per-note correction as the export writes it: C(f, note) plus the fitted room's
    compensation (fit_room.py), as (anchor notes, rows on the soundboard grid); None without
    either."""
    comp = np.array(room["comp_db"]) if room and "comp_db" in room else None
    corr = sb.get("correction") if CORRECTION else None
    if corr is None:
        return None if comp is None else (list(CORR_NOTES), np.tile(comp, (len(CORR_NOTES), 1)))
    return list(corr["notes"]), np.array(corr["C_db"]) + (0 if comp is None else comp)


def partial_gain_db(sb, room, note, freqs):
    """A partial's gain at freqs (dB): C(f, note) plus the fitted room's compensation."""
    g = correction_db(sb, note, freqs) if CORRECTION else np.zeros(len(freqs))
    if room and "comp_db" in room:
        g = g + np.interp(np.log2(np.clip(freqs, 1, None)), np.log2(sb["grid_hz"]), room["comp_db"])
    return g


def correction_db(sb, note, freqs):
    """C(f, note) in dB at freqs (0 if not identified)."""
    corr = sb.get("correction")
    if corr is None:
        return np.zeros(len(freqs))
    C = np.array(corr["C_db"])                              # (N, F)
    row = np.array([np.interp(note, corr["notes"], C[:, j]) for j in range(C.shape[1])])
    return np.interp(np.log2(np.clip(freqs, 1, None)), np.log2(sb["grid_hz"]), row)


# ---------------------------------------------------------------- synthesis

ROOM_MIN_BANDS = 6
ROOM_HOLD_HZ = 2500.0


# sets whose decay between the partials is noise (body_decay.py reads it as a room of 4–45 s): an
# assumed room, [strings] room_t60 ((band Hz, T60 s), or "vcsl_dry": VCSL's dry studio, as its
# harp measures it, 0.2–0.5 s)
VCSL_DRY_ROOM = [(250.0, 0.45), (1000.0, 0.3), (4000.0, 0.22), (16000.0, 0.18)]


def room_t60(dataset):
    """Measured decay between the partials (analysis/body_decay.py): (band Hz, T60 s) of
    the bands with a consistent estimate over notes (IQR within a factor 2); an assumed
    room where the set's gaps hold noise ([strings] room_t60)."""
    assumed = scale.strings(dataset).get("room_t60")
    if assumed:
        return np.array(VCSL_DRY_ROOM if assumed == "vcsl_dry" else assumed)
    bands = FH.P.load_cache(dataset, "body")["bands"]
    # estimates per band needed: 20, or ¾ of the most any band has (Upright Y: 14 notes)
    need = min(20, int(np.ceil(0.75 * max((b["n"] for b in bands.values()), default=0))))
    pts = [(float(f), b["t60_median_s"]) for f, b in bands.items()
           if b["n"] >= need and b["t60_q75_s"] / b["t60_q25_s"] < 2.0]
    if len(pts) >= ROOM_MIN_BANDS:
        return np.array(sorted(pts))
    # too few consistent bands (few notes, strings ringing between the partials): the
    # lower quartile per band (the room is the shortest decay there, strings only add),
    # a running median over 3 bands, held above ROOM_HOLD_HZ (air absorption: the room
    # does not ring longer there)
    q = sorted((float(f), b["t60_q25_s"]) for f, b in bands.items() if b["n"] >= need)
    f, t = np.array(q).T
    t = np.array([np.median(t[max(i - 1, 0):i + 2]) for i in range(len(t))])
    hold = t[f <= ROOM_HOLD_HZ][-1]
    t = np.where(f > ROOM_HOLD_HZ, hold, t)
    return np.column_stack([f, t])


MIC_SPACING = 0.12  # m, AB pair
ROOM_EQ_N = 8192    # length of the room's envelope filter H (5.9 Hz resolution at 48 kHz)


def room_ir(sb, t60_pts, sr=48000, length_s=2.0, seed=1, eq_db=None):
    """Diffuse room-like IR, stereo (2, n): noise with frequency-dependent exponential
    decay T60(f) (measured, held constant beyond the measured bands), spectral envelope
    H, inter-channel coherence of a diffuse field at an AB pair, sinc(2πf·d/c).
    eq_db ((Hz, dB) rows): the room's own level, on top of its unit energy (a cut takes energy away).
    The decays are built in 512-point STFT frames; H is applied afterwards as one
    minimum-phase filter (ROOM_EQ_N): in the frames, with bins 94 Hz apart, it could not
    follow the upright's steep fall below 125 Hz (−20 dB at 60 Hz), and the room
    radiated the bass fundamentals ~10 dB too strong (heard as a boomy, muffled bass)."""
    rng = np.random.default_rng(seed)
    n = int(length_s * sr)
    nfft = 1 << int(np.ceil(np.log2(n)))
    # build in the STFT domain: per band, noise with its own decay
    hop = 256
    frames = nfft // hop
    freqs = np.fft.rfftfreq(512, 1 / sr)
    t = np.arange(frames) * hop / sr
    t60 = np.exp(np.interp(np.log(np.clip(freqs, 1, None)), np.log(t60_pts[:, 0]), np.log(t60_pts[:, 1])))
    env = np.exp(-6.91 * t[:, None] / t60[None, :])                 # amplitude envelope
    noise = rng.standard_normal((2, frames, len(freqs))) + 1j * rng.standard_normal((2, frames, len(freqs)))
    coh = np.sinc(2 * freqs * MIC_SPACING / 343.0)          # np.sinc(x) = sin(πx)/(πx)
    spec = np.stack([noise[0], coh * noise[0] + np.sqrt(1 - coh ** 2) * noise[1]])
    spec *= env[None]
    win = np.hanning(512)
    ir = np.zeros((2, frames * hop + 512))
    for k in range(frames):
        ir[:, k * hop:k * hop + 512] += np.fft.irfft(spec[:, k], 512, axis=-1) * win
    lf = np.log2(np.clip(np.fft.rfftfreq(ROOM_EQ_N, 1 / sr), 1, None))
    H_db = np.interp(lf, np.log2(sb["grid_hz"]), sb["H_db"])
    h = min_phase(H_db, ROOM_EQ_N)
    ir = np.stack([fftconvolve(c[:n], h)[:n] for c in ir])
    ir = ir / np.sqrt((ir ** 2).sum(axis=1, keepdims=True))
    if eq_db is not None and len(eq_db):
        # on top of the unit energy: a cut takes energy away
        eq_db = np.asarray(eq_db, float)
        q = min_phase(np.interp(lf, np.log2(eq_db[:, 0]), eq_db[:, 1]), ROOM_EQ_N)
        ir = np.stack([fftconvolve(c, q)[:n] for c in ir])
    return ir


def smooth_filter(sb, sr=48000, n=4096):
    """Minimum-phase FIR with magnitude H (no fine structure), via the real cepstrum."""
    freqs = np.fft.rfftfreq(n, 1 / sr)
    H_db = np.interp(np.log2(np.clip(freqs, 1, None)), np.log2(sb["grid_hz"]), sb["H_db"])
    h = min_phase(H_db, n)
    return h / np.sqrt(np.sum(h ** 2))


ROOM_GAIN = 1.0   # room energy relative to the body's, "full" mode, where no room is fitted


def room_params(profile):
    """The room: the decay measured between the partials (room_t60) at ROOM_GAIN, with the fitted
    room's own level (fit_room.py: eq_db, cuts)."""
    room = (profile.felt_fit or {}).get("room") or {}
    return {"t60": room_t60(profile.dataset), "eq_db": room.get("eq_db", []), "gain": room.get("gain", ROOM_GAIN)}


def make_filter(profile, mode="full", sr=48000, polarization="v", split=False):
    """Radiation filters, (channels, n):
      smooth — minimum-phase H only (mono, no fine structure)
      body   — the synthetic soundboard's modes (profile.board), the same modes that load
               the strings at the bridge; one radiation pattern per channel
      room   — diffuse decay with the T60(f) measured between the recorded partials
      full   — body + room (ROOM_GAIN)
    The horizontal polarization (polarization="h") gets its own radiation amplitudes
    and room realization: it reaches the microphones by another path, so it does not
    beat coherently against the vertical one (that made the aftersound beat ~1.5 dB
    deeper than the recordings, audible as a slow flanger).
    split=True (mode "full"): {"body", "room"}, each with the full filter's normalization,
    so that body + room is the full filter (the engine mixes the room with a level).
    """
    sb = profile.felt_fit["soundboard"]
    if mode == "smooth":
        return smooth_filter(sb, sr)[None, :]
    parts = []
    if mode in ("body", "full"):
        board = profile.board or scale.make_board(profile.dataset)
        parts.append(board.impulse_response(sb["grid_hz"], sb["H_db"], sr, polarization=polarization))
    if mode in ("room", "full"):
        rp = room_params(profile)
        room = room_ir(sb, rp["t60"], sr, seed=11 if polarization == "v" else 21, eq_db=rp["eq_db"])
        parts.append(room * (np.sqrt(rp["gain"]) if mode == "full" else 1.0))
    h = np.zeros((2, max(p.shape[1] for p in parts)))
    for p in parts:
        h[:, :p.shape[1]] += p
    norm = np.sqrt((h ** 2).sum(axis=1, keepdims=True))
    if split:
        assert mode == "full"
        return {"body": parts[0] / norm, "room": parts[1] / norm}
    return h / norm


def apply(y, h):
    return np.stack([fftconvolve(y, hc)[: len(y)] for hc in h])


_ir_cache = {}


KNOCK_RISE_S = 0.001


def knock_burst(note, amp, decay_s, cutoff_hz, n, sr=48000):
    """Action knock as heard: a short noise burst with a soft onset (KNOCK_RISE_S),
    exponentially decaying (decay_s), lowpass-shaped (8th order at cutoff_hz, highpass
    at 80 Hz; an instant onset and shallower lowpasses made broadband stripes up to
    5–10 kHz where the recordings have nothing above 4–5 kHz), stereo (2, n) with an
    AB pair's coherence at low frequencies; a different burst for every key (seeded by
    the note), peak envelope amp. It does not go through the soundboard modes or the
    room: in the recordings it is over within ~10 ms (frame/keybed, heavily damped);
    radiated through the ringing board and room it filled the gaps between the partials
    for 100+ ms and sounded like a ping-pong ball."""
    rng = np.random.default_rng(1000 + note)
    m = min(int(10 * decay_s * sr), n)
    t = np.arange(m) / sr
    noise = rng.standard_normal((2, m))
    f = np.fft.rfftfreq(m, 1 / sr)
    shape = 1 / np.sqrt(1 + (f / cutoff_hz) ** 16) * (f / 80) ** 2 / np.sqrt(1 + (f / 80) ** 4)
    coh = np.sinc(2 * f * MIC_SPACING / 343.0)
    spec = np.fft.rfft(noise, axis=1)
    spec = np.stack([spec[0], coh * spec[0] + np.sqrt(1 - coh ** 2) * spec[1]]) * shape
    burst = np.fft.irfft(spec, m, axis=1)
    rise = np.sin(0.5 * np.pi * np.clip(t / KNOCK_RISE_S, 0, 1)) ** 2
    burst *= rise * np.exp(-t / decay_s) / max(np.abs(burst).max(), 1e-12)
    out = np.zeros((2, n))
    out[:, :m] = amp * burst
    return out


def impact_force(im, force, velocity, ff_speed, sr=48000):
    """The hammer's impact on the board (a piano's thump, fit_knock.py): the strike's felt force
    (N, from the strike) × gain × (velocity / ff_speed)^growth, through a 2nd-order Butterworth
    lowpass at cutoff_hz (None: open), as a vertical bridge force. The impact reaches the board
    through the action and the frame, not the string: the string passes only the strike position's
    share below its fundamental, 10–20 dB under what the recordings hold in the treble."""
    from scipy.signal import butter, sosfilt
    y = np.asarray(force, float) * im["gain"] * (velocity / ff_speed) ** im["growth"]
    if im.get("cutoff_hz"):
        y = sosfilt(butter(2, im["cutoff_hz"], fs=sr, output="sos"), y)
    return y


def knock_knee(note, knee):
    """1 from the knee note up, fading out (raised cosine) over the octave below."""
    return float(np.sin(0.5 * np.pi * np.clip((note - knee + 12) / 12, 0, 1)) ** 2)


def knock_level(kn, note, velocity):
    return kn["amp"] * velocity ** kn["q"] * knock_knee(note, kn["knee_note"])


def radiate(profile, y_v, y_h, sr=48000):
    """Bridge forces of both polarizations → (channels, n) through the radiation filters
    (env SOUNDBOARD_MODE, default "full")."""
    mode = os.environ.get("SOUNDBOARD_MODE", "full")
    out = 0
    for y, pol in ((y_v, "v"), (y_h, "h")):
        if mode == "smooth" and pol == "h":   # no fine structure: no separate path either
            pol = "v"
        key = (profile.dataset, mode, profile.board is not None, pol)
        if key not in _ir_cache:
            _ir_cache[key] = make_filter(profile, mode, sr, pol)
        out = out + apply(y, _ir_cache[key])
    return out if len(out) == 2 else np.repeat(out, 2, axis=0)


CORRECTION = os.environ.get("SB_CORRECTION", "1") != "0"


def coupled_modes(profile, note, sr=48000):
    """A note's string modes as the engine runs them (proto/export.py): partial numbers k,
    w (S×K, rad/s), sig (S×K, 1/s), the vertical bridge coupling kappa_v (K), the bridge
    factor (K, with the per-note correction) and per polarization ("vert", "horiz") the
    coupled unison system's eigenvalues lam (K×S) and eigenvectors V (K×S×S)."""
    p = note_params(profile, note)
    k, w, sig = scale.M.mode_table(p, sr)
    K = w.shape[1]
    if p.admittance is None:
        kappa_v = np.full(K, p.kappa * (1 + 1j * p.kappa_reactive))
    else:
        y = p.admittance(w.mean(axis=0) / (2 * np.pi))
        kappa_v = p.kappa * ((y if p.admittance_reactive else y.real) + 1j * p.kappa_reactive)
    bridge = p.T * (k * np.pi / p.L) * (-1.0) ** k
    if p.mode_gain is not None:
        bridge = bridge * p.mode_gain(w.mean(axis=0) / (2 * np.pi))
    out = {"p": p, "k": k, "w": w, "sig": sig, "kappa_v": kappa_v, "bridge": bridge}
    w_h = w * 2 ** (p.horiz_detune_cents / 1200)
    for name, ww, kappa in (("vert", w, kappa_v[:, None, None]), ("horiz", w_h, p.horiz_kappa)):
        out[name] = scale.M.coupled_eigenmodes(ww, sig, kappa)
    return out


def note_params(profile, note):
    """scale.note_params plus the per-note transfer correction C(f, note) and the fitted room's
    compensation as a gain per partial (its magnitude at the partial's frequency; a real-time engine applies it
    that way, so the prototype does too, instead of a minimum-phase FIR per note)."""
    p = scale.note_params(profile, note)
    sb = (profile.felt_fit or {}).get("soundboard") or {}
    room = (profile.felt_fit or {}).get("room")
    if (CORRECTION and "correction" in sb) or (room and "comp_db" in room and sb):
        p = replace(p, mode_gain=lambda f: 10 ** (partial_gain_db(sb, room, note, f) / 20))
    return p


def render(profile, note, velocity, duration, sr=48000, stereo=False, knock=True, dampers=None, core=None, felt=None):
    """Render a note through the profile's radiation filter, plus the action knock
    (fit_knock.py) if fitted; mono (first channel) unless stereo. Bridge force only if
    no H identified. dampers: [(t_s, on), …] (damper.py rates); info["bridge_force"]
    holds the vertical bridge force (for the sympathetic resonance)."""
    p = scale.with_felt(note_params(profile, note), felt)
    yp, info = scale.M.render(scale.with_core(p, core) if core else p, velocity, sr=sr, duration=duration,
                              split=True, dampers=dampers,
                              damper_sigma=lambda f: damper.sigma(note, f, profile.dataset))
    if (profile.felt_fit or {}).get("soundboard") is None:
        y = yp.sum(axis=0)
        return (np.stack([y, y]) if stereo else y), info
    im = (profile.felt_fit or {}).get("impact") if knock else None
    if im:
        f = np.zeros(yp.shape[1])
        f[:len(info["force"])] = info["force"]
        yp = yp.copy()
        yp[0] = yp[0] + impact_force(im, f, velocity, profile.hammer_speeds()["ff"], sr)
    info["bridge_force"] = yp[0]
    out = radiate(profile, yp[0], yp[1], sr)
    kn = (profile.felt_fit or {}).get("knock") if knock else None
    if kn:
        out = out + knock_burst(note, knock_level(kn, note, velocity), kn["decay_s"], kn["cutoff_hz"],
                                out.shape[1], sr)
    return (out if stereo else out[0]), info


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True, choices=FH.D.ids())
    args = ap.parse_args()
    sb, (f, h) = identify(args.profile)
    FH.P.update_fit(args.profile, {"soundboard": sb})
    print(f"wrote soundboard transfer function to {FH.P.fit_path(args.profile)}")
    # the radiation's draw: none with a fundamental in a notch (board.py pick_rad_seed)
    profile = scale.Profile(args.profile)
    fundamentals = []
    for note in range(21, 109):
        p = scale.note_params(profile, note)
        fundamentals.append(p.f0 * np.sqrt(1 + p.B))
    seed, worst, first = pick_rad_seed(lambda k: scale.make_board(args.profile, rad_seed=k), sb["grid_hz"], sb["H_db"], fundamentals)
    FH.P.update_fit(args.profile, {"radiation": {"seed": seed, "deepest_notch_db": round(worst, 1)}})
    print(f"radiation draw {seed}: the deepest notch at a fundamental {worst:+.1f} dB re its surroundings (draw 0: {first:+.1f})")

    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    fig, ax = plt.subplots(figsize=(11, 5))
    # identify() normalized H to max 0 dB; shift the raw estimates by the same offset
    off = np.median(h - np.interp(np.log2(f), np.log2(sb["grid_hz"]), sb["H_db"]))
    ax.scatter(f, h - off, s=2, alpha=0.15, color="gray", label="per-partial estimates (note gains removed)")
    ax.plot(sb["grid_hz"], sb["H_db"], color="k", lw=2, label="H(f)")
    ax.axvspan(*sb["data_range_hz"], color="tab:green", alpha=0.05, label="covered by data")
    ax.set(xscale="log", xlabel="Hz", ylabel="dB", ylim=(np.min(sb["H_db"]) - 15, 15),
           title=f"{args.profile}: bridge force → recording, transfer magnitude")
    ax.grid(alpha=0.3, which="both")
    ax.legend(fontsize=8)
    out = os.path.join(HERE, "out", f"soundboard_{args.profile}.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=100, bbox_inches="tight")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
