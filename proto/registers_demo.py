#!/usr/bin/env python3
"""Harpsichord B's registers for listening: the demo phrase with 8′, 8′ + 8′ II,
8′ + 8′ II + 4′, the lute alone and lute + 8′ (render, RENDER_STOPS / RENDER_PARAMS).

Usage: registers_demo.py --project flemish   (after cargo build --release)
"""
import argparse
import os
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analysis"))
import project as P  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..")
RENDER = os.path.join(REPO, "engine", "target", "release", "render")
OUT = os.path.join(HERE, "out", "registers")
TAKES = [("8", 1, 0), ("8_8ii", 5, 0), ("8_8ii_4", 7, 0), ("lute", 0, 1), ("lute_8", 1, 1)]


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--project", required=True, help="a project with registers (Harpsichord B: flemish)")
    args = ap.parse_args()
    os.makedirs(OUT, exist_ok=True)
    events = P.cache_path(P.main_part(args.project), "demo_events")
    for name, stops, lute in TAKES:
        env = {**os.environ, "RENDER_STOPS": str(stops), "RENDER_PARAMS": f"lute={lute}"}
        out = os.path.join(OUT, f"{args.project}_{name}.wav")
        subprocess.run([RENDER, os.path.join(REPO, "instruments", args.project), events, out], env=env, check=True,
                       stdout=subprocess.DEVNULL)
        print(out)


if __name__ == "__main__":
    main()
