#!/usr/bin/env python3
"""Export a brass instrument for the engine: the tables of proto/brass/rt_tables.py in 32 bits and what the
players read (engine/src/brass/). An instrument directory without notes (the engine reads one with a brass block
and none), a one-sample body and the generic room; the sound is the `brass` block's.

Usage: export_brass.py --project trumpet [--out DIR]
"""
import argparse
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "brass"))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import numpy as np  # noqa: E402

import project as P  # noqa: E402

DYNS = ("pp", "mf", "ff")
RELEASE_BANDS = (25.0 * 2 ** (np.arange(0, 30) / 3)).tolist()
H_GRID_HZ = [20.0, 20000.0]
ROOM_T60 = [[125.0, 0.7], [250.0, 0.65], [500.0, 0.6], [1000.0, 0.6], [2000.0, 0.55], [4000.0, 0.45], [8000.0, 0.35]]
SOFT = 1e-4  # the lips' opening's rounding at shut, rt_process's soft in live.py


def playable(table_range, brass):
    """The keys that play: from `below` under the table to `above` over it. Under it a player's lips loosen and the
    notes hold; over the highest they are at their limit, so none by default."""
    return [int(table_range[0] - brass.get("below", 6)), int(table_range[1] + brass.get("above", 0))]


def pose(table):
    """The bell as the player holds it, from the project's `[pose]`: the axis's azimuth (clockwise from the player's
    front, degrees) and elevation (up, degrees), the ears' place from the bell's mouth (right, front, up, m). None
    without one: the engine's default, forward and a little down."""
    if not table:
        return None
    if set(table) != {"azimuth", "elevation", "ears"}:
        raise ValueError(f"[pose] takes azimuth, elevation and ears, not {sorted(table)}")
    az, el, ears = float(table["azimuth"]), float(table["elevation"]), [float(x) for x in table["ears"]]
    if not -180.0 <= az <= 360.0 or not -90.0 <= el <= 90.0 or len(ears) != 3 or max(map(abs, ears)) > 3.0:
        raise ValueError(f"[pose] out of range: {table}")
    return {"azimuth": az, "elevation": el, "ears": ears}


def note_doc(n, blob):
    """One key under one mute: its modes in f64, its filters in f32."""
    modes = {d: {"sp": {"re": blob.add(n["modes"][d][0].real), "im": blob.add(n["modes"][d][0].imag)},
                 "rc": {"re": blob.add(n["modes"][d][1].real), "im": blob.add(n["modes"][d][1].imag)}} for d in DYNS}
    doc = {"f": float(n["f"]), "settings": {d: [float(x) for x in n["settings"][d]] for d in DYNS}, "modes": modes,
           "h": {d: blob.add(n["h"][d], "f32") for d in DYNS}, "r": blob.add(n["r"], "f32")}
    if "open" in n:  # the plunger's: "h" shut, "open" the open bell
        doc["open"] = {d: blob.add(n["open"][d], "f32") for d in DYNS}
    return doc


def room_feed(table, radius):
    """The direct sound's level over what the room's filter (the bell's power) leaves of it, on three played notes: darker
    instruments lose less to it, and the room stands the same distance under the direct sound on all of them."""
    import live
    import mutes
    a, b = [], []
    for key in (table[len(table) // 4], table[len(table) // 2], table[3 * len(table) // 4]):
        x = live.render_events([(0.05, "on", key, 80), (1.2, "off", key)], 1.5, "open", True, 1) * live.Live("open", True, 1).gain
        x = np.tanh(x)
        f = np.maximum(np.fft.rfftfreq(len(x), 1 / live.SR), 1.0)
        p = np.fft.irfft(np.fft.rfft(x) * 10 ** ((mutes.piston_gain_db(np.full_like(f, 1.0), radius) - mutes.piston_gain_db(f, radius)) / 20), len(x))
        a.append(np.mean(x * x))
        b.append(np.mean(p * p))
    return float(np.sqrt(sum(a) / sum(b)))


def brass_doc(name, blob):
    """What the players read: each value as live.py and rt_tables.py take it."""
    import fit4
    import fit5
    import live
    import player
    import rt_tables
    import spec
    from scipy.signal import firwin
    S = spec.S
    table = rt_tables.TABLE
    every = ["open", *S["mutes"]]
    _, lips = fit4.lips(1.0, fit5.Q)
    doc = {
        "table": table, "range": playable((table[0], table[-1]), S), "fitted": [S["low"], S["high"]],
        "slide": bool(S.get("slide")), "gain_db": float(S.get("gain_db", 0.0)),
        "width": S["width"], "y0": S["y0"], "soft": SOFT, "rho": float(live.sim.RHO),
        "nl": float(player.NL * live.T5["nl"] * live.SR * live.OS),
        "mu": S["mu"], "mu_f": S.get("mu_f", 262.0), "mu_p": S.get("mu_p", 0.0),
        "center": S["center"], "vel_air": S.get("vel_air"), "scoop_span": S.get("scoop_span"), "noise": S.get("noise"),
        "bell_radius": S["bell"][1], "room_feed": room_feed(table, S["bell"][1]),
        "lips": {k: [float(l[i]) for l in lips] for i, k in enumerate(("ratio", "q", "sign", "comp"))},
        "dec": firwin(96, 20000, fs=live.SR * live.OS).tolist(),
        "makeup_db": {m: live.MAKEUP[m] for m in every}, "order": every,
        "speak": spec.load("speak", {}), "level": spec.load("level", {}), "cells": spec.load("cells", {}),
        "mutes": {},
    }
    # the attack's two settings, written only where the project sets them (an export without them plays as before)
    if S.get("attack_rise_periods"):
        doc["attack_rise_periods"] = float(S["attack_rise_periods"])
    if S.get("attack_rise_hz"):
        doc["attack_rise_hz"] = float(S["attack_rise_hz"])
    if "attack_lean" in S:
        doc["attack_lean"] = bool(S["attack_lean"])
    if S.get("attack_from_rest"):
        doc["attack_from_rest"] = True
    if S.get("attack_rise_from"):
        doc["attack_rise_from"] = float(S["attack_rise_from"])
    for m in every:
        t = rt_tables.load(m)
        doc["at"] = {str(k): [[float(x) for x in a[0]], [float(x) for x in a[1]], float(a[2])] for k, a in t["at"].items()}
        doc["mutes"][m] = {"notes": {str(k): note_doc(t["notes"][k], blob) for k in table}}
    return doc


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--project", required=True)
    ap.add_argument("--out", metavar="DIR", help="write the export into DIR instead of instruments/<project> (proto/reproduce.py's)")
    ap.add_argument("--with", dest="overrides", action="append", default=[], metavar="TABLE.KEY=VALUE",
                    help="override a key of the project's top-level table (a TOML literal), for this export")
    ap.add_argument("--variant", metavar="LABEL",
                    help="write into the snapshots folder as <project>@LABEL (menu group Snapshots), not instruments/")
    args = ap.parse_args()
    # before the brass modules read the project
    for item in args.overrides:
        try:
            table, key, value = P.parse_with(item)
        except ValueError as e:
            ap.error(str(e))
        P.override(args.project, table, {key: value})
    os.environ["BRASS_PROJECT"] = args.project
    sys.path.insert(0, HERE)
    import license as LIC
    import soundboard as SB
    from export import Blob, DAMPER_LOG2, SR, write_instrument
    blob = Blob()
    brass = brass_doc(args.project, blob)
    held = pose(P.section(args.project, "pose"))
    if held:
        brass["pose"] = held
    one = np.zeros((2, 1))
    one[:, 0] = 1.0
    ir = {"channels": 2, "len": 1, "ir": blob.add(one, "f32")}
    radiation = {pol: {"body": ir, "room_seed": seed} for pol, seed in (("v", 11), ("h", 21))}
    radiation["room"] = {"t60": ROOM_T60, "grid_hz": H_GRID_HZ, "H_db": [0.0, 0.0], "gain": SB.ROOM_GAIN, "length_s": 2.0}
    one_mode = blob.add(np.array([100.0]))
    import resonance
    import release
    profile = P.main_part(args.project)
    doc = {
        "format": 5, "profile": profile, "recorded": brass["fitted"], "body": "bell",
        "menu": P.section(args.project, "menu"), "motor": False, "reference_sr": SR, "mic_spacing": SB.MIC_SPACING,
        "velocity": {"midi": [14, 60, 124], "speed": [1.0, 2.0, 4.0]},
        "contact": {"max_s": 0.02, "separation_s": 0.002}, "knock": None,
        "release": {"bands_hz": RELEASE_BANDS, "length_s": release.LENGTH_S, "rise_s": release.RISE_S},
        "pedal": None, "sympathetic": {"transfer": resonance.TRANSFER, "f_max": resonance.F_MAX},
        "board": {"f": one_mode, "sigma": one_mode, "phi": one_mode, "density": 1.0, "r": one_mode, "r_h": one_mode},
        "correction": None, "damper_log2": list(DAMPER_LOG2), "radiation": radiation,
        "brass": brass,
    }
    # the project's own level re the fit's: the grands' loudness (step 243)
    gain = float(P.section(args.project, "export").get("output_gain_db", 0.0))
    if abs(gain) >= 0.05:
        doc["output_gain_db"] = gain
    unused = P.unused_overrides()
    if unused:  # a variant that would differ in nothing
        ap.error("--with read by nothing in this export: " + ", ".join(f"{t}.{k}" for _, t, k in unused))
    if args.variant:
        import shutil
        import snapshot
        from export import variant_target
        outdir, doc["menu"], doc["profile"] = variant_target(args.project, args.variant, doc.get("menu"), doc["profile"],
                                                             snapshot.snapshots_dir())
        if os.path.exists(outdir):
            shutil.rmtree(outdir)
        snapdir = os.path.join(outdir, ".previous")
    else:
        outdir = args.out or os.path.join(HERE, "..", "instruments", args.project)
        snapdir = os.path.join(outdir, ".previous") if args.out else os.path.join(P.project_dir(args.project), "cache", "previous-export")
    os.makedirs(outdir, exist_ok=True)
    write_instrument(outdir, snapdir, doc, blob)
    print(f"wrote {outdir}: {blob.offset / 1e6:.2f} MB data")
    LIC.write(args.project, outdir)
    if args.variant:
        print(f"variant {outdir}")


if __name__ == "__main__":
    main()
