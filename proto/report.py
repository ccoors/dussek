#!/usr/bin/env python3
"""The report: the grands' (or the uprights') traits along the keyboard, their bass attack against
what the recorded pianos of their kind do, and their tone per register against their own recordings
(design 2026-10-21-traits-plan, the rail).

- The traits: the exports' parameters per key (the felt's force at 0.5 mm, its exponent, the core,
  the hammer, the strings).
- The envelope: per key A0–B2 and band, the range of its kind's recordings of the attack's change
  from mf to ff (bass_check's bands re 200–500 Hz: the microphones and rooms cancel). A band counts
  only where both of a key's recordings lie CLEAN_DB over their noise; a project leaves the envelope
  with `[report] envelope = false`. A piano's engine beyond it is listed: a warning, not a veto.
- The engine against its own recording, per dynamic; a snapshot's (proto/snapshot.py) lines beside.
- The tone per register (tone_check.py): within a note, octave bands re the fundamental's, and the
  lowest octaves' fundamentals, the engine − its recordings.

Usage: report.py [--uprights] [--snapshot REF] [--out DIR]
"""
import argparse
import datetime
import json
import os
import subprocess
import sys

import numpy as np  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, ".."))
sys.path.insert(0, os.path.join(REPO, "analysis"))
import bass_check  # noqa: E402
import even_levels as EL  # noqa: E402
import project as P  # noqa: E402
import scale  # noqa: E402
import snapshot as SN  # noqa: E402
import tone_check  # noqa: E402

INSTRUMENTS = os.path.join(REPO, "instruments")
PROJECTS = os.path.join(REPO, "projects")
PAGE = os.path.join(HERE, "report_page.html")
KINDS = {"grand": "Grand pianos", "upright": "Upright pianos"}   # a body: its menu group
CLEAN_DB = 10.0
BANDS = bass_check.ATTACK_BANDS[:4]
TRAITS = ("felt05", "felt_p", "core05", "core_depth", "hammer_mass", "strike_pos", "B", "b1", "detune", "kappa")
measure = bass_check.measure


def _json(d):
    with open(os.path.join(d, "instrument.json")) as fh:
        return json.load(fh)


def pianos(kind="grand", instruments=INSTRUMENTS, projects=PROJECTS):
    """The exports of a kind (KINDS) by their data (body, menu group, shown) that have a project; by menu name."""
    out = []
    for d in sorted(os.listdir(instruments)):
        path = os.path.join(instruments, d, "instrument.json")
        if not os.path.isfile(path) or not os.path.isfile(os.path.join(projects, d, "project.toml")):
            continue
        j = _json(os.path.join(instruments, d))
        menu = j.get("menu") or {}
        if j.get("body") == kind and menu.get("group") == KINDS[kind] and not menu.get("hidden"):
            out.append((menu.get("name") or d, d))
    return [d for _, d in sorted(out)]


def traits(instrument_dir):
    """Per trait [[note, value], …] over the hammer notes."""
    t = {k: [] for k in TRAITS}
    for n in _json(instrument_dir)["notes"]:
        e, s, k = n.get("excitation") or {}, n.get("string") or {}, n["note"]
        if e.get("type") != "hammer" or not s:
            continue
        p = e["felt_p"]
        t["felt05"].append([k, e["felt_K"] * 5e-4 ** p])
        t["felt_p"].append([k, p])
        t["core05"].append([k, e.get("core_K", 0.0) * 5e-4 ** p])
        if e.get("core_K"):
            t["core_depth"].append([k, e["core_depth"] * 1e3])
        t["hammer_mass"].append([k, e["mass"] * 1e3])
        t["strike_pos"].append([k, e["strike_pos"]])
        t["B"].append([k, s["B"]])
        t["b1"].append([k, s["b1"]])
        d = s.get("detune_cents") or [0.0]
        t["detune"].append([k, max(d) - min(d)])
        t["kappa"].append([k, s.get("kappa")])
    return t


def _clean(row):
    return [a - n >= CLEAN_DB for a, n in zip(row["rec"]["attack"], row["rec"]["noise"])]


def _by_note(rows):
    by = {}
    for r in rows:
        if "eng" in r and r["dynamic"] in ("mf", "ff"):
            by.setdefault(r["note"], {})[r["dynamic"]] = r
    return by


def spread(rows, which):
    """Per note ff − mf of the attack's bands; for the recording a band only where both lie clear of their noise."""
    out = {}
    for n, d in sorted(_by_note(rows).items()):
        if "mf" not in d or "ff" not in d:
            continue
        lo, hi = d["mf"][which]["attack"], d["ff"][which]["attack"]
        ok = [a and b for a, b in zip(_clean(d["mf"]), _clean(d["ff"]))] if which == "rec" else [True] * len(BANDS)
        out[n] = [round(hi[i] - lo[i], 2) if ok[i] else None for i in range(len(BANDS))]
    return out


def error(rows):
    """Per dynamic and note the engine − the recording; a band only where the recording lies clear of its noise."""
    out = {}
    for n, d in sorted(_by_note(rows).items()):
        for dyn, r in d.items():
            ok = _clean(r)
            out.setdefault(dyn, {})[n] = [round(r["eng"]["attack"][i] - r["rec"]["attack"][i], 2) if ok[i] else None
                                          for i in range(len(BANDS))]
    return out


def _interp(curve, band, lo, hi):
    pts = sorted((k, v[band]) for k, v in curve.items() if v[band] is not None)
    out = {}
    for (k0, v0), (k1, v1) in zip(pts, pts[1:]):
        for k in range(max(k0, lo), min(k1, hi) + 1):
            out[k] = v0 + (v1 - v0) * (k - k0) / (k1 - k0)
    if len(pts) == 1 and lo <= pts[0][0] <= hi:
        out[pts[0][0]] = pts[0][1]
    return out


def envelope(curves, band, lo=21, hi=47):
    """Per key (note, min, max, count) over the curves (each linear between its own keys) where two give a value."""
    per = [_interp(c, band, lo, hi) for c in curves]
    out = []
    for k in range(lo, hi + 1):
        vs = [p[k] for p in per if k in p]
        if len(vs) >= 2:
            out.append((k, min(vs), max(vs), len(vs)))
    return out


def outside(env, curve, band):
    """(note, value, lo, hi) where the curve lies beyond the envelope."""
    at = {e[0]: e for e in env}
    return [(k, v[band], at[k][1], at[k][2]) for k, v in sorted(curve.items())
            if k in at and v[band] is not None and not at[k][1] <= v[band] <= at[k][2]]


LEVEL_TOOL = os.path.join(REPO, "engine", "target", "release", "level")


def levels_of(level_doc, speeds):
    """{"ff": [[key, dB re the piano's median], …]}: each key's ff level, which sets its level at every
    velocity (the engine plays a velocity at the speed where the key sounds a set level under its
    own ff), from a level with each key's ff level (the `level` tool's); {} without one."""
    try:
        ff = EL.key_levels(level_doc, speeds)["ff"]
    except ValueError:
        return {}
    return {"ff": [[k, round(float(v - np.median(ff)), 2)] for k, v in zip(EL.KEYS, ff)]}


def levels(instrument_dir):
    """Every key's level (levels_of) as the engine plays the instrument: its level tool (~2.5 s), which
    prints the level and writes nothing."""
    out = subprocess.run([LEVEL_TOOL, instrument_dir], check=True, capture_output=True, text=True).stdout
    doc = json.loads(out)
    return levels_of(doc, _json(instrument_dir)["velocity"]["speed"])


def board(name):
    """The project's main part's fitted soundboard: its response H(f) on the fit's grid (normalized
    to its maximum), the frequency range its recordings cover, and the per-key correction rows."""
    sb = P.load_fit(P.main_part(name))["soundboard"]
    c = sb.get("correction") or {"notes": [], "C_db": []}
    r = lambda xs, n=2: [round(float(x), n) for x in xs]  # noqa: E731
    return {"grid_hz": r(sb["grid_hz"], 1), "H_db": r(sb["H_db"]), "range_hz": r(sb["data_range_hz"], 0),
            "corr_notes": list(c["notes"]), "C_db": [r(row, 1) for row in c["C_db"]]}


def measured_ranges(loss_notes, sample_notes):
    """The keys a fitted trait rests on, [first, last]: the strings' loss (and the board coupling, its
    multiple) from the keys with enough decays measured, the inharmonicity from the recorded keys;
    the report draws a curve past them dotted (an edge there is the fit's slope, not the piano)."""
    out = {}
    if loss_notes:
        out["b1"] = out["kappa"] = [min(loss_notes), max(loss_notes)]
    if sample_notes:
        out["B"] = [min(sample_notes), max(sample_notes)]
    return {k: list(v) for k, v in out.items()}


def traits_measured(name):
    """`measured_ranges` of the project's main part ({} without its analysis)."""
    try:
        prof = scale.Profile(P.main_part(name))
    except FileNotFoundError:  # no analysis cache: nothing to mark
        return {}
    return measured_ranges(prof.loss_notes, [s["note"] for s in prof.samples])


def in_envelope(name):
    """The project's `[report] envelope` (default true)."""
    return bool(P.section(name, "report").get("envelope", True))


def _describe():
    r = subprocess.run(["git", "-C", REPO, "describe", "--always", "--dirty", "--tags"], capture_output=True, text=True)
    return r.stdout.strip()


def tone(name, instrument_dir, workdir):
    """tone_check's summary of the export against its project's recordings."""
    return tone_check.summary(tone_check.measure(name, instrument_dir, workdir))


def collect(names, snapshot_refs, workdir, instruments=INSTRUMENTS, snapshots=None, kind="grand"):
    """The page's data: per piano its traits, spreads, errors, tone and levels (and each existing snapshot's
    or variant's), the envelopes per band, and per piano the keys its engine leaves them."""
    snapshots = snapshots or SN.snapshots_dir()
    os.makedirs(workdir, exist_ok=True)
    out = []
    for name in names:
        d = os.path.join(instruments, name)
        rows = measure(name, d, "", workdir)
        g = {"id": name, "name": (_json(d).get("menu") or {}).get("name") or name, "in_envelope": in_envelope(name),
             "traits": traits(d), "rec": spread(rows, "rec"), "eng": spread(rows, "eng"), "errors": error(rows),
             "tone": tone(name, d, workdir), "levels": levels(d), "board": board(name), "measured": traits_measured(name),
             "snapshots": {}}
        for ref in snapshot_refs:
            sd = os.path.join(snapshots, f"{name}@{SN.safe_ref(ref)}")
            if os.path.isfile(os.path.join(sd, "instrument.json")):
                srows = measure(name, sd, "", workdir)
                g["snapshots"][ref] = {"traits": traits(sd), "eng": spread(srows, "eng"), "errors": error(srows),
                                       "tone": tone(name, sd, workdir), "levels": levels(sd)}
        out.append(g)
    envs = {b: envelope([g["rec"] for g in out if g["in_envelope"]], b) for b in range(len(BANDS))}
    for g in out:
        g["outside"] = {b: outside(envs[b], g["eng"], b) for b in range(len(BANDS))}
    return {"described": _describe(), "snapshots": list(snapshot_refs), "date": datetime.date.today().isoformat(),
            "bands": [f"{a // 1000}–{b // 1000} kHz" for a, b in BANDS], "kind": kind, "pianos": out, "envelopes": envs}


def unknown_refs(names, refs, snapshots=None):
    """The refs (or variants' labels) no piano has a snapshot of: a typo or a ref never snapshotted
    would draw nothing beside today's, silently."""
    snapshots = snapshots or SN.snapshots_dir()
    return [ref for ref in refs
            if not any(os.path.isfile(os.path.join(snapshots, f"{n}@{SN.safe_ref(ref)}", "instrument.json")) for n in names)]


def write_page(data, out_dir):
    """The page with the data in it; its path."""
    os.makedirs(out_dir, exist_ok=True)
    with open(PAGE) as fh:
        page = fh.read()
    path = os.path.join(out_dir, "index.html")
    with open(path, "w") as fh:
        fh.write(page.replace("/*DATA*/", json.dumps(data)))
    return path


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--snapshot", action="append", default=[],
                    help="a ref (just snapshot REF) or a variant's label (just variant P LABEL) drawn beside today's; repeatable")
    ap.add_argument("--uprights", action="store_true", help="the uprights instead of the grands")
    ap.add_argument("--out", help="the page's folder (default proto/out/report, proto/out/report-uprights)")
    args = ap.parse_args()
    kind = "upright" if args.uprights else "grand"
    out = args.out or os.path.join(HERE, "out", "report-uprights" if args.uprights else "report")
    names = pianos(kind)
    missing = unknown_refs(names, args.snapshot)
    if missing:
        ap.error(f"no {kind} has a snapshot of {', '.join(missing)} in {SN.snapshots_dir()} "
                 "(just snapshot REF, or just variant P LABEL)")
    data = collect(names, args.snapshot, os.path.join(HERE, "out", "bass_check_renders"), kind=kind)
    print(write_page(data, out))
    for g in data["pianos"]:
        if g["in_envelope"]:
            print(f"  {g['name']}: keys out of the envelope " + ", ".join(
                f"{band} {len(g['outside'][b])}" for b, band in enumerate(data["bands"])))
        else:
            print(f"  {g['name']}: not in the envelope")


if __name__ == "__main__":
    main()
