#!/usr/bin/env python3
"""Export a struck-bar instrument (the vibraphone, the marimba, the xylophone, the bells, the crotales, the tubes) for the engine: the fit of
bars.py (the part's fit document) as format 5 notes with a mode table in place of a string's
partials (engine/src/physics.rs `BarModes`). [export] (the project file): per instrument its body, its dampers
(the vibraphone's damper bar on the pedal; the marimba has none), whether it has a motor and
a trim on the output calibration (gain_db).
An instrument with several mallets lists them ("mallets": the engine's Mallet parameter
picks one; each its lowpass and speeds, the notes' own the first's); a mallet's cutoff
on each bar × bars.cutoff_scale (in each note's f_q: the engine scales another mallet's by
the note's f_q re the first mallet's).

Per key one bar (S = 1): its modes' ratios, losses σ and output weights (the fitted
levels; the kalimba's other tines ringing along and the hand chimes' strike modes appended, bars.py `halo`, `strike`); the mallet as the pluck path's `bar` excitation (the modes started directly
through the mallet's lowpass, physics::pluck_amplitude); a damper
on every key, its loss per frequency from the modes' (pedal up minus pedal down,
interpolated in log f, held beyond them): the vibraphone's damper bar, lifted by a held key
or the pedal (`damper` in [export] says which instruments have one). No knock, release noise, pedal noise or
sympathetic coupling (κ = 0: other bars do not ring along audibly in the recordings, bars.py); the
kalimba has a knock, the thumb's click ([export] knock: its level re the keys' radiated peak).

The recordings are anechoic and the bars radiate directly: the body response is the
synthetic board's with a flat spectral envelope and heavily damped modes (BOARD_OVERLAP:
~15 ms), which gives the microphone positions (mic.rs zones) their usual machinery (the balafon's
flat at "Recording", [export] flat_body: the board's ±25 dB at 4–8 kHz striped its membranes'
noise, which no per-mode compensation reaches); the
room a generic one (ROOM_T60), at the body's energy (soundboard.ROOM_GAIN). VCSL's sets (the hand
chimes, the kalimba, the balafon) were recorded close in its dry studio ([export] room: its T60,
soundboard.VCSL_DRY_ROOM, and its energy re the body's).

The fundamental's harmonics (bars.py's `harmonics`: per key the levels re the fundamental
at ff, free and damped, their growth and decay) go with the modes (`BarModes::harmonics`,
the engine runs them in the resonator's bank); those the fit leaves out on a key (H_NONE_DB)
are not written.

Each mode's output weight is divided by the body response's gain at its frequency, and
each harmonic's level by the gain at its frequency re the fundamental's (`compensated`): a sum of random modes colors every frequency at random, and a bar's
few modes would take that as their level. The gain is the response's over ±50 cents (GAIN_BAND_CENTS):
a notch a few cents wide under a fitted mode (39 dB at 6167 Hz) was undone at full depth, and the
engine, playing the key in tune, sounded the mode beside it (Glockenspiel B's C♯6: 30 dB over its
neighbors). The level: per key and dynamic the radiated peak of the mode sum at that speed (the
engine's calibration and release-level reference), the calibration on the recorded keys.

Usage: export_bars.py --project vibraphone
"""
import argparse
import json
import os

import numpy as np

import bars
import level_corrections as LC
import license as LIC
import project as P  # noqa: E402  (on the path through bars)
import release
import resonance
import soundboard as SB
from board import Board, DENSITY
from export import Blob, DAMPER_LOG2, SR, calibration_gain_db, measure_level, write_instrument

HERE = os.path.dirname(os.path.abspath(__file__))
BOARD_SEED = 17
BOARD_OVERLAP = 8.0
H_GRID_HZ = [20.0, 20000.0]
ROOM_T60 = [[125.0, 0.7], [250.0, 0.65], [500.0, 0.6], [1000.0, 0.6], [2000.0, 0.55], [4000.0, 0.45], [8000.0, 0.35]]
RELEASE_BANDS = (25.0 * 2 ** (np.arange(0, 30) / 3)).tolist()
OUTPUT_GAIN_DB = 0.0
# each instrument's export settings, [export] in its project file (the body, the damper, the
# motor, the onset, the room, the click, the membrane and the mallets' levels, with their reasons);
# a room's t60 "vcsl_dry" is VCSL's dry studio (soundboard.VCSL_DRY_ROOM)
def config(project):
    cfg = dict(P.section(project, "export"))
    if cfg.get("room", {}).get("t60") == "vcsl_dry":
        cfg["room"] = {**cfg["room"], "t60": [list(p) for p in SB.VCSL_DRY_ROOM]}
    return cfg


def with_extras(key, onset_s=None):
    """The key with its extra modes appended (bars.py: `halo`, the kalimba's ringing tines;
    `strike`, the hand chimes' tubes' own modes; `rise`, the fundamental's swell: two partners at
    its frequency, −2 and +1 × its weight, at its loss + σ and + 2σ), under the fundamental's
    damper loss; with `onset_s` each mode's partner in antiphase (from zero over the contact).
    `sign`: per mode its output weight's sign; `halo_from`: the halo's first mode (the engine's
    `Params::sympathetic` scales the modes from there on: the halo last, its partners with it)."""
    key = {**key, "sign": key.get("sign", [1.0] * len(key["ratios"]))}
    halo = key["halo"] if key.get("halo") and key["halo"]["ratio"] else None
    def append(key, h):
        n = len(h["ratio"])
        return {**key, "ratios": key["ratios"] + h["ratio"], "sigma": key["sigma"] + h["sigma"], "out_db": key["out_db"] + h["out_db"],
                "damper_sigma": key["damper_sigma"] + [key["damper_sigma"][0]] * n, "sign": key["sign"] + h.get("sign", [1.0] * n)}
    def onset(key, a, b):
        # modes a … b − 1 from zero over the mallet's contact: partners in antiphase, decaying
        # 1/onset_s faster
        return append(key, {"ratio": key["ratios"][a:b], "sigma": [sg + 1.0 / onset_s for sg in key["sigma"][a:b]],
                            "out_db": key["out_db"][a:b], "sign": [-x for x in key["sign"][a:b]]}) if onset_s else key
    s1, o1 = key["sigma"][0], key["out_db"][0]
    if key.get("strike") and key["strike"]["ratio"]:
        key = append(key, key["strike"])
    key = onset(key, 0, len(key["ratios"]))
    if key.get("rise"):
        sg = key["rise"]
        key = append(key, {"ratio": [key["ratios"][0]] * 2, "sigma": [s1 + sg, s1 + 2 * sg],
                           "out_db": [o1 + 20 * np.log10(2.0), o1], "sign": [-1.0, 1.0]})
    # the ringing tines last, with their own partners: `Params::sympathetic` scales the modes from
    # `halo_from` on (the tine's own partners among them, it no longer started from zero off 100 %)
    if halo:
        h0 = len(key["ratios"])
        key = {**append(key, halo), "halo_from": h0}
        key = onset(key, h0, len(key["ratios"]))
    return key


def damper_table(key):
    """Damper loss (1/s) on DAMPER_LOG2's grid: the modes' (log σ over log f), held beyond."""
    f = key["f1"] * np.array(key["ratios"])
    sig = np.array(key["damper_sigma"])
    o = np.argsort(f)
    grid = DAMPER_LOG2[0] + DAMPER_LOG2[1] * np.arange(DAMPER_LOG2[2])
    return np.exp(np.interp(grid, np.log2(f[o]), np.log(sig[o])))


# the band a mode's compensation averages the body's response over: a mode moves by tens of
# cents with the tuning (concert pitch, A4, the temperament), and a notch a few cents wide under
# its fitted frequency (39 dB at 6167 Hz) is no gain it is played at
GAIN_BAND_CENTS = 50.0


def body_gain(ir, f, sr=SR):
    """The body response's gain at frequencies f: power over the channels, averaged over
    ±GAIN_BAND_CENTS (a sum of random modes colors each frequency at random, ±4 dB from key to
    key in the peaks)."""
    nfft = 1 << 20
    H = np.abs(np.fft.rfft(ir, nfft, axis=1)) ** 2
    grid = np.fft.rfftfreq(nfft, 1 / sr)
    cum = np.concatenate([[0.0], np.cumsum(H.mean(axis=0))])
    f = np.asarray(f, float)
    r = 2 ** (GAIN_BAND_CENTS / 1200)
    a = np.searchsorted(grid, f / r)
    b = np.maximum(np.searchsorted(grid, f * r, side="right"), a + 1)
    return np.sqrt((cum[b] - cum[a]) / (b - a))


def compensated(key, ir):
    """The key with its modes' output weights divided by the body's gain there, its
    harmonics' levels by the gain there re the fundamental's: at the recording's position
    each mode and harmonic as fitted."""
    f = key["f1"] * np.array(key["ratios"])
    g = body_gain(ir, np.minimum(f, 0.5 * SR - 1))
    out = {**key, "out_db": (np.array(key["out_db"]) - 20 * np.log10(g)).tolist()}
    if "harmonics" in key:
        hm = dict(key["harmonics"])
        gh = body_gain(ir, np.minimum(key["f1"] * np.array(hm["h"], float), 0.5 * SR - 1)) / g[0]
        for tag in ("", "damped_"):
            hm[tag + "level_db"] = [lv - 20 * np.log10(x) if lv > bars.H_NONE_DB else lv for lv, x in zip(hm[tag + "level_db"], gh)]
        out["harmonics"] = hm
    return out


def harmonics_doc(key):
    """The engine's harmonics block: those the key has (free or damped), else None."""
    hm = key.get("harmonics")
    if not hm:
        return None
    keep = [i for i in range(len(hm["h"])) if max(hm["level_db"][i], hm["damped_level_db"][i]) > bars.H_NONE_DB]
    if not keep:
        return None
    return {k: [hm[k][i] for i in keep] for k in ("h", "level_db", "growth", "decay", "damped_level_db", "damped_growth", "damped_decay")}


def note_fq(key, mallet):
    """The mallet's cutoff on this key's bar (at mf)."""
    return float(mallet["f_q"] * bars.cutoff_scale(key["f1"], mallet.get("beta", 0.0), mallet["f_q"], mallet["order"]))


def note_doc(key, mallet, speeds, blob, damped=True, knock=False):
    ratios = key["ratios"]
    return {
        "note": key["note"],
        # the recorded bar's own pitch re the curve (the engine's "As recorded")
        "recorded_cents": key.get("recorded_cents", 0.0),
        "string": {"f0": key["f1"], "B": 0.0, "L": float(bars.BAR_LEN_F3 * np.sqrt(174.61 / key["f1"])), "T": 1.0,
                   "mass": key["bar_mass"], "S": 1, "detune_cents": [0.0], "b1": 0.0, "b3": 0.0,
                   "kappa": 0.0, "kappa_reactive": 0.0, "horiz_gain": 0.0, "horiz_kappa": 0.0,
                   "horiz_detune_cents": 0.0, "f_max": 20000.0, "scatter_cents": 0.0,
                   "modes": {"ratio": ratios, "sigma": key["sigma"], "phi": [1.0] * len(ratios),
                             "out": [float(sg * 10 ** (d / 20)) for sg, d in zip(key.get("sign", [1.0] * len(ratios)), key["out_db"])],
                             **({"harmonics": h} if (h := harmonics_doc(key)) else {}),
                             # the other tines ringing along from this mode on (the kalimba's halo)
                             **({"halo_from": key["halo_from"]} if "halo_from" in key else {}),
                             # the modes' slow beating (the tubes): per mode Δf (Hz) and the partner's share
                             **({"split_hz": key["split_hz"], "split_share": key["split_share"]} if "split_hz" in key else {})}},
        "excitation": {"type": "pluck", "bar": True, "beta": 0.5, "f_q": note_fq(key, mallet), "amp": 1.0,
                       "gain_db_per_step": mallet["gain_db_per_step"], "fq_oct_per_step": mallet["fq_oct_per_step"],
                       "order": mallet["order"]},
        "has_damper": damped,
        "damper_sigma": blob.add(damper_table(key)) if damped else None,
        "knock": {"knee": 1.0} if knock else None,
    }


def peak(key, mallet, speed, ir, sr=SR, length_s=0.15):
    """Radiated peak of a key struck at `speed`: its modes, started at their levels through
    the mallet (as the engine's pluck path), through the body response (channel 0)."""
    f = key["f1"] * np.array(key["ratios"])
    sig = np.array(key["sigma"])
    keep = f < 0.45 * sr
    a = 10 ** ((np.array(key["out_db"]) + bars.mallet_db(f, np.log2(speed), note_fq(key, mallet), mallet["fq_oct_per_step"],
                                                         mallet["order"])) / 20) * np.array(key.get("sign", [1.0] * len(f)))
    t = np.arange(int(length_s * sr)) / sr
    # a struck bar starts from rest (sine phase, as the engine's voice)
    y = (a[keep, None] * np.exp(-sig[keep, None] * t) * np.sin(2 * np.pi * f[keep, None] * t)).sum(axis=0)
    return float(np.abs(np.convolve(y, ir)).max())


# the rattle's measure (analysis/modes.py rattle_db): the band's median power over the window re the
# fundamental's peak over the first 0.1 s
RATTLE_BAND = (1000.0, 4000.0)
RATTLE_S = (0.05, 0.3)
# the engine's membrane follower (voice.rs MEMBRANE_ENV_S)
MEMBRANE_ENV_S = 0.005


def rattle_db(key, mallet, speed, membrane, gain, sr=SR, seed=7):
    """The engine's rattle at a key (dB re its fundamental, as the analysis measures the
    recordings'): its modes started as `peak` starts them, the membrane's noise (Voice::membrane:
    high-passed, `poles` one-pole lowpasses, × the modes' rectified level over MEMBRANE_ENV_S to
    `power` re its peak) at `level` × gain; no body (flat), no room."""
    from scipy.signal import butter, lfilter
    t = np.arange(int(0.35 * sr)) / sr
    f = key["f1"] * np.array(key["ratios"])
    keep = f < 0.45 * sr
    a = 10 ** ((np.array(key["out_db"]) + bars.mallet_db(f, np.log2(speed), note_fq(key, mallet), mallet["fq_oct_per_step"],
                                                         mallet["order"])) / 20) * np.array(key.get("sign", [1.0] * len(f)))
    y = (a[keep, None] * np.exp(-np.array(key["sigma"])[keep, None] * t) * np.sin(2 * np.pi * f[keep, None] * t)).sum(axis=0)
    ae = 1.0 - np.exp(-1.0 / (MEMBRANE_ENV_S * sr))
    env = lfilter([ae], [1.0, -(1.0 - ae)], np.abs(y))
    p = membrane.get("power", 1.0)
    if p != 1.0:
        env = env * (env / np.maximum(np.maximum.accumulate(env), 1e-30)) ** (p - 1.0)
    n = np.random.default_rng(seed).standard_normal(len(t))
    hp = membrane["hp_ratio"] * key["f1"] if membrane.get("hp_ratio") else membrane.get("hp_hz", 200.0)
    n = lfilter(*butter(2, hp, "hp", fs=sr), n)
    al = 1.0 - np.exp(-2 * np.pi * membrane.get("lp_hz", 1600.0) / sr)
    for _ in range(membrane.get("poles", 2)):
        n = lfilter([al], [1.0, -(1.0 - al)], n)
    x = y + membrane["level"] * gain * env * n
    nfft = 1 << 15
    fr = np.fft.rfftfreq(nfft, 1 / sr)
    s0 = x[:int(0.1 * sr)]
    ref = (np.abs(np.fft.rfft(s0 * np.hanning(len(s0)), nfft)) ** 2)[(fr > 0.95 * key["f1"]) & (fr < 1.05 * key["f1"])].max()
    s1 = x[int(RATTLE_S[0] * sr):int(RATTLE_S[1] * sr)]
    pw = np.abs(np.fft.rfft(s1 * np.hanning(len(s1)), nfft)) ** 2
    return float(10 * np.log10(np.median(pw[(fr >= RATTLE_BAND[0]) & (fr < RATTLE_BAND[1])]) / ref))


def rattle_gain(key, mallet, speed, membrane, rounds=3):
    """This key's membrane gain that brings its rattle to the recorded tine's (`rattle_db`), within
    [0.01, 100]: the noise's power scales with the gain squared where it lies over the lines."""
    g = 1.0
    for _ in range(rounds):
        g = float(np.clip(g * 10 ** ((key["rattle_db"] - rattle_db(key, mallet, speed, membrane, g)) / 20), 0.01, 100.0))
    return g


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--project", required=True, help="the instrument's project (projects/<name>)")
    ap.add_argument("--out", metavar="DIR",
                    help="write the export into DIR instead of instruments/<project> (proto/reproduce.py's)")
    args = ap.parse_args()
    profile = P.main_part(args.project)
    cfg = config(args.project)
    fit = bars.load(profile)
    mallet, speeds = fit["mallet"], fit["hammer_speeds"]
    # the mallet sources' level corrections, in mallet order (the fit's)
    level_tabs = LC.tables(profile, ("mallet", "main"))
    blob = Blob()
    board = Board(seed=BOARD_SEED, density=DENSITY, overlap=BOARD_OVERLAP)
    h_db = [0.0, 0.0]
    # both channels alike: Iowa's are one microphone (the crotales', the marimba's: identical; the
    # vibraphone's, the bells': 93–97 % correlated), and a bar is a small source radiating directly
    # (the board's independent channels put a key's few modes ±10 dB apart left and right)
    irs = {pol: np.repeat(board.impulse_response(H_GRID_HZ, h_db, SR, polarization=pol)[:1], 2, axis=0) for pol in ("v", "h")}
    if cfg.get("flat_body"):
        for ir in irs.values():
            ir[:] = 0.0
            ir[:, 0] = 1.0
    peaks = {}
    notes = []
    for key in fit["keys"]:
        key = compensated(with_extras(key, cfg.get("onset_s")), irs["v"])
        doc = note_doc(key, mallet, speeds, blob, damped=cfg.get("damper") is not None, knock="knock" in cfg)
        if cfg.get("membrane", {}).get("per_key") and key.get("rattle_db") is not None:
            doc["membrane_gain"] = rattle_gain(key, mallet, speeds["mf"], cfg["membrane"])
        pk = {d: peak(key, mallet, speeds[d], irs["v"][0]) for d in ("pp", "mf", "ff")}
        peaks[key["note"]] = pk
        doc["release"] = {"impacts": [], "band_db": [0.0] * len(RELEASE_BANDS), "peak": [pk[d] for d in ("pp", "mf", "ff")],
                          "ratio_db": [[float(np.log(speeds[d])), -120.0] for d in ("pp", "mf", "ff")]}
        lv = LC.note_levels(level_tabs, key["note"])
        if lv is not None:
            doc["level_db"] = lv
        notes.append(doc)
    # beyond the recorded keys the membrane keeps the edge key's gain: fitted there to the edge tine's
    # rattle, a fainter extrapolated tine took up to 100× (an mbira's top keys ran away when hammered)
    lo, hi = fit["recorded"]
    edge = {d["note"]: d.get("membrane_gain") for d in notes if d["note"] in (lo, hi)}
    for d in notes:
        if "membrane_gain" in d and not lo <= d["note"] <= hi and edge.get(lo if d["note"] < lo else hi) is not None:
            d["membrane_gain"] = edge[lo if d["note"] < lo else hi]
    gain = round(OUTPUT_GAIN_DB + cfg.get("gain_db", 0.0) + calibration_gain_db(peaks, tuple(fit["recorded"])), 2)
    knock = None
    if "knock" in cfg:
        k = cfg["knock"]
        lo, hi = fit["recorded"]
        ref = float(np.median([pk["mf"] for n, pk in peaks.items() if lo <= n <= hi]))
        knock = {"amp": 10 ** (k["level_db"] / 20) * ref, "q": k["q"], "decay_s": k["decay_s"], "cutoff_hz": k["cutoff_hz"],
                 "rise_s": k["rise_s"]}
        if k.get("per_key"):
            # each key's knock at its own level (its radiated peak at mf re the median)
            for doc in notes:
                doc["knock"]["knee"] = float(peaks[doc["note"]]["mf"] / ref)
    ir = lambda h: {"channels": int(h.shape[0]), "len": int(h.shape[1]), "ir": blob.add(h, "f32")}  # noqa: E731
    radiation = {pol: {"body": ir(irs[pol]), "room_seed": seed} for pol, seed in (("v", 11), ("h", 21))}
    room = cfg.get("room", {"t60": ROOM_T60, "gain": SB.ROOM_GAIN})
    radiation["room"] = {"t60": room["t60"], "grid_hz": H_GRID_HZ, "H_db": h_db, "gain": room["gain"], "length_s": 2.0}
    doc = {
        "format": 5, "profile": profile, "recorded": [int(k) for k in fit["recorded"]],
        "body": P.section(args.project, "project")["body"],
        # the plugin's menu entry (the project's [menu]: name, group, labels, mallets; the engine ignores it)
        **({"menu": P.section(args.project, "menu")} if P.section(args.project, "menu") else {}),
        "motor": cfg["motor"], "reference_sr": SR,
        "mic_spacing": SB.MIC_SPACING,
        **({"output_gain_db": gain} if abs(gain) >= 0.05 else {}),
        "velocity": {"midi": [14, 60, 124], "speed": [speeds[d] for d in ("pp", "mf", "ff")]},
        **({"mallets": [{"name": m["name"], "f_q": m["f_q"], "fq_oct_per_step": m["fq_oct_per_step"], "order": m["order"],
                         "speed": [m["speeds"][d] for d in ("pp", "mf", "ff")],
                         **({"knock_db": cfg["mallet_knock_db"][i]} if "mallet_knock_db" in cfg else {}),
                         **({"buzz_db": cfg["mallet_buzz_db"][i]} if "mallet_buzz_db" in cfg else {})}
                        for i, m in enumerate(fit["mallets"])]}
           if len(fit.get("mallets", [])) > 1 else {}),
        "contact": {"max_s": 0.02, "separation_s": 0.002},
        "knock": knock,
        **({"membrane": {k: v for k, v in cfg["membrane"].items() if k != "per_key"}} if "membrane" in cfg else {}),
        "release": {"bands_hz": RELEASE_BANDS, "length_s": release.LENGTH_S, "rise_s": release.RISE_S},
        "pedal": None,
        "sympathetic": {"transfer": resonance.TRANSFER, "f_max": resonance.F_MAX},
        "board": {"f": blob.add(board.f), "sigma": blob.add(board.sigma), "phi": blob.add(board.phi),
                  "density": board.density, "r": blob.add(board.r), "r_h": blob.add(board.r_h)},
        "correction": None,
        "damper_log2": list(DAMPER_LOG2),
        "radiation": radiation,
        "notes": notes,
    }
    outdir = args.out or os.path.join(HERE, "..", "instruments", args.project)
    snapdir = os.path.join(outdir, ".previous") if args.out else os.path.join(P.project_dir(args.project), "cache", "previous-export")
    write_instrument(outdir, snapdir, doc, blob)
    print(f"wrote {outdir}: {blob.offset / 1e6:.2f} MB data, output gain {gain:+.2f} dB")
    measure_level(outdir)
    LIC.write(args.project, outdir)


if __name__ == "__main__":
    main()
