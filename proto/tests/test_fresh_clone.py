"""What a fresh clone lacks (proto/out/, the caches, data/) a step makes or does without."""
import os
import re
import tomllib

import project as P


def chain_scripts():
    """the scripts the steps of projects/steps.toml run"""
    steps = tomllib.load(open(os.path.join(P.REPO, "projects", "steps.toml"), "rb"))["step"]
    return sorted({a for s in steps.values() for a in s["run"] if a.endswith(".py")})


def test_chain_scripts_make_the_folders_they_write():
    # proto/out/ is not in git: a step that plotted into it crashed a fresh clone's first build
    writes_out = re.compile(r"""os\.path\.join\(HERE, ["']out["']""")
    missing = [s for s in chain_scripts()
               if writes_out.search(src := open(os.path.join(P.REPO, s)).read()) and "os.makedirs(" not in src]
    assert missing == []


def lent_caches():
    """{lender's cache file (relative to the repository): its borrowers}, for every cache a committed
    project reads through a borrowing (uses.<role>:cache.<name>)"""
    import build
    steps = tomllib.load(open(os.path.join(P.REPO, "projects", "steps.toml"), "rb"))["step"]
    read = re.compile(r"uses\.(\w+):cache\.(\w+)$")
    found = {}
    for name in P.index()[0]:
        uses = P.config(name).get("uses", {})
        for inst in build.chain(name):
            for r in steps[inst.split("#")[0]].get("reads", []):
                if (m := read.match(r)) and m[1] in uses:
                    path = os.path.relpath(P.cache_path(P.main_part(uses[m[1]]), m[2]), P.REPO)
                    found.setdefault(path, set()).add(name)
    return found


def test_what_a_lender_lends_is_committed():
    # a fresh clone has no data/ to build a lender from: a borrower (any new project the editor makes
    # from a donor) reads the lender's committed caches instead
    import subprocess
    tracked = set(subprocess.run(["git", "ls-files", "projects"], cwd=P.REPO, capture_output=True, text=True,
                                 check=True).stdout.split())
    lent = lent_caches()
    assert lent and sorted(set(lent) - tracked) == []


def test_the_lent_caches_are_what_the_borrowers_were_built_from():
    # a lender refit and its lent cache left out of the commit: here the file is new and the check
    # passes, but a clone builds new instruments from the old one (a clone's file is the committed one)
    import build
    lent = re.compile(r"uses\.\w+:cache\.\w+$")
    off = []
    for name in P.index()[0]:
        for key, got in build.ledger(name).items():
            kind, id_, inst = key.split(":", 2)
            if not any(lent.match(r) for r in got["inputs"]):
                continue
            now = build.fingerprint(name, inst, kind, id_)
            off += [f"{name} {inst}: {r}" for r in got["inputs"] if lent.match(r) and now.get(r) != got["inputs"][r]]
    assert off == []
