#!/usr/bin/env python3
"""A brass project's steps (usage: steps.py <step> <project>): the order is the chain's (projects/steps.toml). Each
module holds its instrument in module state and reads its fit keys once, on import, so each stage runs in a process
of its own and reads what the last one wrote. RESIDUAL=0: the tables without the harmonics' correction. A step that
corrects what an earlier one left starts from that step's output, never from its own of the last build: a build is the
same from the same inputs (each build had added its passes to the last one's, and two in a row differed by up to 224 c)."""
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "analysis"))
import project as P  # noqa: E402


def run(module, *args, plain=False, capture=False):
    env = {**os.environ, "BRASS_PROJECT": NAME, "OMP_NUM_THREADS": "1"}
    if plain:
        env["RESIDUAL"] = "0"
    cmd = [sys.executable, os.path.join(HERE, f"{module}.py"), *args]
    print("---", module, *args, flush=True)
    if capture:
        return subprocess.run(cmd, env=env, check=True, capture_output=True, text=True).stdout
    subprocess.run(cmd, env=env, check=True)
    return None


def mutes():
    """What goes in the bell (the plunger is an output filter on the open note: nothing to tune or repair)."""
    return [m for m in P.section(NAME, "brass")["mutes"] if m != "plunger"]


def every():
    return ["open", *P.section(NAME, "brass")["mutes"]]


# what the steps after the fit write: the live player, the tables and the mutes read them, so a step read the last
# build's level, speak, residual and mutes' correction where this one's had not been written yet (two builds in a row
# differed in every one of them); the mutes' shapes and their correction go in their own step, a lender's
LATER = ("tuning", "cells", "speak", "residual", "cells_mutes", "tuning_mutes", "level", "gain_db")


def brass_fit():
    """From the recordings alone: the later steps' keys go first, so each step reads only what this build wrote before it."""
    P.update_fit(P.main_part(NAME), {}, drop=LATER)
    if P.config(NAME)["project"]["family"] == "brass_lender":
        run("fit5")
        run("fit_other", "recording")
    else:
        run("fit_other")
    run("phrase3")


def brass_retune():
    """Two passes from the table's own tuning: each moves every note's slide by what it plays off."""
    P.update_fit(P.main_part(NAME), {"tuning": {}})
    run("compare5", "retune")
    run("compare5", "retune")


def brass_repair():
    """The cells from none: this step's and the later ones' (the mutes', the landing's, the slurs') of this build only."""
    P.update_fit(P.main_part(NAME), {"cells": {}})
    run("rt_stability", "open", "repair")
    run("mode_fix", "apply", "open")
    run("mode_fix", "lips", "open")


def brass_speak():
    run("speak_fit", plain=True)


def brass_harmonics():
    run("harm_fit", plain=True)


def brass_mute_shapes():
    P.update_fit(P.main_part(NAME), {}, drop=("mute_shapes", "mute_residual"))  # the muted notes read the correction
    run("fit_mutes")
    run("fit_mutes", "vsco")


def brass_mutes():
    P.update_fit(P.main_part(NAME), {"cells_mutes": {}, "tuning_mutes": {}})  # from none; with nothing in the bell, empty
    if not mutes():
        return
    run("muted", "retune", "repair", *mutes())
    for m in mutes():
        run("rt_stability", m, "repair")
        run("mode_fix", "apply", m)
        run("mode_fix", "lips", m)
        run("rt_stability", m, "repair")  # what the mode fix left is what a player gets


def brass_mute_residual():
    run("mute_residual")


def brass_land():
    run("land_fix", "open", *mutes())


def brass_slurs():
    for m in ("open", *mutes()):
        run("slur_fix", "repair", m)


def brass_level():
    run("level_fit")


def brass_gain():
    run("chain_fit", "gain")


def brass_tables():
    run("rt_tables", *every())
    os.environ["BRASS_PROJECT"] = NAME
    import digest
    import rt_tables
    manifest = {}
    for m in every():
        t = rt_tables.load(m)
        manifest[m] = {str(k): digest.of_entry(e) for k, e in t["notes"].items()}
    P.write_cache(NAME, "tables_manifest", manifest)


def brass_report():
    """What a person checks a rebuild by: which notes do not hold, which land off, the level's range."""
    rows = {}
    for m in every():
        lines = []
        for module, pattern in (("rt_sweep", "virtual="), ("rt_stability", "not holding"), ("level_check", "range v20|worst|v80 over")):
            if m == "plunger" and module != "level_check":
                continue
            out = run(module, m, capture=True)
            lines += [line[:200] for line in out.splitlines() if re.search(pattern, line)]
        rows[m] = lines
    P.write_cache(NAME, "report", rows)
    print(json.dumps(rows, indent=1))


if __name__ == "__main__":
    step, NAME = sys.argv[1], sys.argv[2]
    globals()[step.split("#")[0]]()
