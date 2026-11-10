"""The brass model against what the ear accepted: tables built from the committed fits hold the accepted
numbers (proto/tests/data/brass_accepted.json, from the prototype's tables). The modules keep an
instrument in module state, so each check runs in a process of its own."""
import json
import os
import subprocess
import sys

import project as P
import pytest

BRASS = os.path.join(P.REPO, "proto", "brass")
ACCEPTED = json.load(open(os.path.join(os.path.dirname(__file__), "data", "brass_accepted.json")))


def run(project, code):
    """`code` with the project selected, in proto/brass; what it printed, as JSON."""
    env = {**os.environ, "BRASS_PROJECT": project, "OMP_NUM_THREADS": "1"}
    env.pop("PE_RECORD", None)
    r = subprocess.run([sys.executable, "-c", code], cwd=BRASS, env=env, capture_output=True, text=True)
    assert r.returncode == 0, r.stderr[-2000:]
    return json.loads(r.stdout.splitlines()[-1])


# a read under a project's cache fails the check: a fresh clone has none
NO_CACHE = """
import sys
def _no_cache(event, args):
    if event == "open" and isinstance(args[0], str) and "/cache/" in args[0]:
        raise RuntimeError("reads a cache: " + args[0])
sys.addaudithook(_no_cache)
"""


def close(got, want, path=""):
    """Where two digests differ: 0.001 dB on a band, 0.001 % on anything else."""
    if isinstance(want, dict):
        return [e for k in want for e in close(got[k], want[k], f"{path}/{k}")]
    if isinstance(want, list):
        return [e for i, (a, b) in enumerate(zip(got, want)) for e in close(a, b, f"{path}/{i}")]
    ok = abs(got - want) <= 0.0011 if "bands" in path else abs(got - want) <= 1e-5 * abs(want)
    return [] if ok else [f"{path}: {got} accepted {want}"]


@pytest.mark.parametrize("project", sorted(ACCEPTED))
def test_tables_hold_the_accepted_values(project):
    # a key takes seconds: the middle key, open, and a mute on the lender (all of them were held to the
    # prototype's, design log step 234)
    center = P.config(project)["brass"]["center"]
    jobs = [("open", center)] + ([("straight", center)] if project == "trumpet" else [])
    code = NO_CACHE + f"""
import json, digest, rt_tables
print(json.dumps({{f"{{m}}_{{k}}": digest.of_entry(rt_tables.build_note((k, m))[1]) for m, k in {jobs!r}}}))
"""
    got = run(project, code)
    wrong = [e for m, k in jobs for e in close(got[f"{m}_{k}"], ACCEPTED[project][m][str(k)], f"{m}/{k}")]
    assert wrong == [], "\n".join(wrong[:12])


def test_tables_need_only_the_fits():
    # the whole `at` of a table comes from the fits; the audit hook refuses a read under a cache
    code = NO_CACHE + """
import json, rt_tables
print(json.dumps(sorted(rt_tables.levels_at())[:1]))
"""
    assert run("tuba", code) == [28]


def test_the_c_builds_once_under_races(tmp_path):
    # four processes, no library yet: each loads a whole one
    code = """
import sys, json, ctypes
import spec
ctypes.CDLL(spec.build_c("lips.c", ["-O2"], so=sys.argv[1]))
print(json.dumps(True))
"""
    env = {**os.environ, "BRASS_PROJECT": "tuba"}
    ps = [subprocess.Popen([sys.executable, "-c", code, str(tmp_path / "_lips.so")], cwd=BRASS, env=env,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for _ in range(4)]
    for p in ps:
        out, err = p.communicate()
        assert p.returncode == 0 and out.strip().endswith("true"), err[-800:]
    assert sorted(os.listdir(tmp_path)) == ["_lips.so"]


def test_a_borrower_reads_the_lender_s_mutes():
    code = """
import json, spec, mutes
print(json.dumps([spec.load("mute_shapes") is None, spec.lender("mute_shapes")["straight"]["length"], mutes.MUTES["straight"].length]))
"""
    own, length, scaled = run("tuba", code)
    assert own and scaled > length  # the tuba's bell is larger than the trumpet's: the shape scaled up
