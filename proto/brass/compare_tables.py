#!/usr/bin/env python3
"""A rebuild's tables against the accepted ones (usage: compare_tables.py <project> --other <checkout>): the bands of
the output filter more than 1 dB apart, with the output's gain and its level at the dynamics' air on top; a table's
own overall level is the level fit's. A difference is for the ear, not a failure: the exit status stays 0. Both sides
have run the chain's brass_tables step."""
import argparse
import json
import os

import digest

REPO = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
LIMIT_DB = 1.0
ANCHOR = {"pp": 0, "mf": 1, "ff": 2}  # the level's anchors of the air (live.LEVEL_X) at the dynamics


def load(checkout, project):
    """A checkout's tables' digests and what levels them."""
    with open(os.path.join(checkout, "projects", project, "cache", project, "tables_manifest.json")) as f:
        tables = json.load(f)
    with open(os.path.join(checkout, "projects", project, "fits", f"{project}.json")) as f:
        fit = json.load(f)
    return {"tables": tables, "level": fit.get("level", {}), "gain_db": fit.get("gain_db", 0.0), "leveled": fit.get("leveled", {})}


def _output_db(side, mute, key, dyn):
    level = side["level"].get(mute, side["level"].get("open", {}))
    anchors = level.get(str(key), [0.0] * 4)
    return side["gain_db"] + anchors[ANCHOR[dyn]] + level.get(f"{key}_{dyn}", 0.0)


def differences(a, b):
    """[(mute, key, dynamics, the band's lower edge in Hz, dB of b over a)] over LIMIT_DB, with inf for a key only one has."""
    out = []
    for mute in a["tables"]:
        rows_a, rows_b = a["tables"][mute], b["tables"].get(mute, {})
        for key in sorted(set(rows_a) | set(rows_b), key=int):
            if key not in rows_a or key not in rows_b:
                out.append((mute, int(key), "*", 0.0, float("inf")))
                continue
            for dyn in ANCHOR:
                up = _output_db(b, mute, key, dyn) - _output_db(a, mute, key, dyn)
                for edge, x, y in zip(digest.EDGES, rows_a[key][dyn]["bands"], rows_b[key][dyn]["bands"]):
                    if abs(y + up - x) > LIMIT_DB:
                        out.append((mute, int(key), dyn, edge, y + up - x))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("project")
    ap.add_argument("--other", required=True, help="the checkout with the rebuild")
    args = ap.parse_args()
    a, b = load(REPO, args.project), load(args.other, args.project)
    diffs = differences(a, b)
    bands = sum(len(row[d]["bands"]) for m in a["tables"].values() for row in m.values() for d in ANCHOR)
    print(f"{args.project}: {len(diffs)} of {bands} bands over {LIMIT_DB} dB; gain {a['gain_db']} -> {b['gain_db']} dB")
    for mute in a["tables"]:
        d = [x for x in diffs if x[0] == mute and x[4] != float("inf")]
        keys = sorted({x[1] for x in d})
        worst = sorted(d, key=lambda x: -abs(x[4]))[:5]
        print(f"  {mute}: {len(d)} bands on {len(keys)} keys; worst " + ", ".join(f"{k} {dy} {e:.0f} Hz {v:+.1f} dB" for _, k, dy, e, v in worst))
    print(json.dumps(diffs if len(diffs) < 40 else f"{len(diffs)} rows")[:400])
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "out", f"compare_{args.project}.json"), "w") as f:
        json.dump(diffs, f)


if __name__ == "__main__":
    main()
