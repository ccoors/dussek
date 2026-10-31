#!/usr/bin/env python3
"""Whether the committed exports are what an export makes today: each project exported again into a
scratch folder (export.py / export_bars.py --out, from the fits and caches as they are, with today's
code and engine) and compared with instruments/<project>. Exit 1 when any differs.

A difference is a sound change waiting for the next export: a cache that outlived the code that
filled it (Grand A's release peaks, step 200), an engine change measured into an instrument's level without
the export that depends on it. Before merging a change to proto/ or engine/, or committing an
export: `just reproduce` (about 2 min, every core; `just reproduce grand_a` one project).

    python3 proto/reproduce.py [project …] [--keep DIR]
"""
import argparse
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import project as P  # noqa: E402

import build  # noqa: E402

FILES = ("instrument.json", "LICENSE.md")
# below these a difference is rounding (a sum in another order): no sound change
TOLERANCE = {"dB": 1e-6, "rel": 1e-9}


def _unit(path):
    """How a field's difference reads: dB fields in dB, a peak (linear) as the dB of its ratio, any other
    number relative."""
    leaf = path.rsplit("/", 1)[-1]
    if leaf.endswith("db") or leaf.endswith("_db") or "_db/" in path + "/":
        return "dB"
    return "peak" if leaf == "peak" else "rel"


def differences(a, b, path=""):
    """[(path, unit, count, largest)] where two JSON documents differ: numbers by `_unit` (lists'
    elements gathered under one path), anything else counted with largest 0."""
    rows = {}

    def walk(x, y, at):
        if isinstance(x, dict) and isinstance(y, dict):
            for k in list(x) + [k for k in y if k not in x]:
                if k not in x or k not in y:
                    rows.setdefault(f"{at}/{k}", ["key", 0, 0.0])[1] += 1
                else:
                    walk(x[k], y[k], f"{at}/{k}")
        elif isinstance(x, list) and isinstance(y, list) and len(x) == len(y):
            for u, v in zip(x, y):
                walk(u, v, at)
        elif (isinstance(x, (int, float)) and isinstance(y, (int, float))
              and not isinstance(x, bool) and not isinstance(y, bool)):
            if x == y:
                return
            unit = _unit(at)
            if unit == "dB":
                d = abs(y - x)
            elif unit == "peak" and x > 0 and y > 0:
                d, unit = abs(20 * math.log10(y / x)), "dB"
            else:
                d, unit = abs(y - x) / max(abs(x), abs(y)), "rel"
            if d < TOLERANCE[unit]:
                return
            r = rows.setdefault(at, [unit, 0, 0.0])
            r[1] += 1
            r[2] = max(r[2], d)
        elif x != y:
            rows.setdefault(at, ["value", 0, 0.0])[1] += 1

    walk(a, b, path)
    return sorted(((p, u, n, d) for p, (u, n, d) in rows.items()), key=lambda r: -r[3])


def exporter(name):
    """the export script of a project with a committed export (None: none, or a rolled piano's,
    which the editor writes)"""
    if P.config(name).get("project", {}).get("family") == "rolled":
        return None
    if not os.path.isfile(os.path.join(P.REPO, "instruments", name, "instrument.json")):
        return None
    steps = build.chain(name)
    return "export.py" if "export" in steps else "export_bars.py" if "export_bars" in steps else None


def check(name, scratch):
    """[(file, rows)] where the project's committed export and today's differ ([] when the same)"""
    out = os.path.join(scratch, name)
    script = exporter(name)
    assert script, name
    r = subprocess.run([sys.executable, os.path.join(HERE, script), "--project", name, "--out", out],
                       capture_output=True, text=True)
    if r.returncode:
        return [("export", [(r.stderr.strip().splitlines()[-1] if r.stderr.strip() else "failed", "error", 1, 0.0)])]
    committed = os.path.join(P.REPO, "instruments", name)
    found = []
    with open(os.path.join(committed, "data.bin"), "rb") as f, open(os.path.join(out, "data.bin"), "rb") as g:
        if f.read() != g.read():
            found.append(("data.bin", [("", "bytes", 1, 0.0)]))
    for fn in FILES:
        a, b = os.path.join(committed, fn), os.path.join(out, fn)
        if not (os.path.exists(a) and os.path.exists(b)):
            if os.path.exists(a) != os.path.exists(b):
                found.append((fn, [("", "only one side", 1, 0.0)]))
            continue
        if fn.endswith(".json"):
            rows = differences(json.load(open(a)), json.load(open(b)))
        else:
            rows = [] if open(a).read() == open(b).read() else [("", "text", 1, 0.0)]
        if rows:
            found.append((fn, rows))
    return found


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("projects", nargs="*", help="default: every project with a committed export")
    ap.add_argument("--keep", metavar="DIR", help="leave today's exports in DIR (default: a scratch folder, removed)")
    ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 4) // 2),
                    help="exports at a time (default: half the cores; a bar instrument's takes ~40 s on one)")
    args = ap.parse_args()
    names = args.projects or [n for n in sorted(P.index()[0]) if exporter(n)]
    without = [n for n in names if not exporter(n)]
    if without:
        ap.error("no committed export to reproduce (or a rolled piano's): " + ", ".join(without))
    # an export needs its project's caches: a project never built in this checkout (a fresh clone)
    # cannot be exported again here, which is no difference (as `just check` passes over it)
    for n in names:
        if not build._built_here(n):
            print(f"{n}: not built here (no caches), not reproduced", flush=True)
    names = [n for n in names if build._built_here(n)]
    scratch = args.keep or tempfile.mkdtemp(prefix="dussek-reproduce-")
    bad = 0
    try:
        with ThreadPoolExecutor(args.jobs) as pool:
            results = pool.map(lambda n: (n, check(n, scratch)), names)   # in order, as they finish
            for name, found in results:
                print(f"{name}: {'same' if not found else 'DIFFERS'}", flush=True)
                for fn, rows in found:
                    for path, unit, n, d in rows[:8]:
                        size = f"largest {d:.3g} {unit}" if unit in ("dB", "rel") else unit
                        print(f"    {fn}{path}: {n} value{'s' if n != 1 else ''}, {size}", flush=True)
                bad += bool(found)
    finally:
        if not args.keep:
            shutil.rmtree(scratch, ignore_errors=True)
    print(f"{len(names) - bad} of {len(names)} exports reproduce" + (f"; {bad} differ" if bad else "")
          + ("" if names else " (nothing built here)"))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
