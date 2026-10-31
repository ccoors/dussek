"""proto/build.py on a synthetic project whose steps are small scripts using project.py."""
import json
import os
import signal
import subprocess
import sys
import time

import pytest

HERE = os.path.dirname(os.path.abspath(__file__))
BUILD = os.path.join(HERE, "..", "build.py")

STEP = r'''
import os, sys
sys.path.insert(0, {analysis!r})
import project as P
part, mode = sys.argv[1], sys.argv[2]
k = P.section(part, "knobs") if mode in ("one", "two", "sneak", "pol", "bm") else {{}}
if mode == "one":
    P.update_fit(part, {{"a": k["x"] * 2, "noise": P.load_fit(part).get("c")}})   # c: a seed
elif mode == "two":
    P.update_fit(part, {{"b": P.load_fit(part)["a"] + 1}})
    if k.get("warn"):
        print("warning: b jumped, listen before committing")
        print("  warnings: (indented: not one)")
    if k.get("fail"):
        raise SystemExit("asked to fail")
elif mode == "three":
    P.update_fit(part, {{"c": P.load_fit(part)["b"] * 10}})
elif mode == "sneak":
    P.section(part, "secret")
elif mode == "lean":
    P.update_fit(part, {{"d": P.load_fit(P.provider(part, "base")).get("a")}})
elif mode == "leancache":                              # a borrowed cache (the dampers' shape)
    P.update_fit(part, {{"e": P.load_cache(P.provider(part, "base"), "lent")["v"]}})
elif mode == "pol":                                    # the guitar's shape: pol, bm, pol#2, bm#2, sb
    P.update_fit(part, {{"pol": k["x"] + (P.load_fit(part).get("bm") or 0)}})
elif mode == "bm":
    P.update_fit(part, {{"bm": P.load_fit(part)["pol"] + k["y"]}})
elif mode == "sb":
    P.update_fit(part, {{"sb": P.load_fit(part)["bm"] + 1000}})
elif mode == "amender":
    P.update_fit(part, {{"a": P.load_fit(part)["a"] + 100}})
elif mode == "slow":                                   # a long step: the cancel tests
    P.update_fit(part, {{"s": "half"}})
    import time
    time.sleep(60)
elif mode == "tell":                                   # the export's shape: reads the menu, owns no fit
    P.section(part, "menu")
elif mode == "gather":                                 # the roll's shape: every borrowed project, owns no fit
    pass
'''

STEPS = """
[step.one]
scope = "part"
run = [{py}, {step}, "{{part}}", "one"]
reads = ["project.knobs"]
seeds = ["fit.c"]
owns = ["fit.a", "fit.noise", "fit.maybe"]   # maybe: written only on some projects (felt_curve)

[step.two]
scope = "part"
run = [{py}, {step}, "{{part}}", "two"]
version = {v2}
reads = ["project.knobs", "fit.a"]
owns = ["fit.b"]

[step.three]
scope = "part"
run = [{py}, {step}, "{{part}}", "three"]
reads = ["fit.b"]
owns = ["fit.c"]

[step.sneak]
scope = "part"
run = [{py}, {step}, "{{part}}", "sneak"]
reads = ["project.knobs"]

[step.lean]
scope = "part"
run = [{py}, {step}, "{{part}}", "lean"]
reads = ["project.uses", "uses.base:*"]
owns = ["fit.d"]

[step.pol]
scope = "part"
run = [{py}, {step}, "{{part}}", "pol"]
reads = ["project.knobs"]
seeds = ["fit.bm"]
owns = ["fit.pol"]

[step.bm]
scope = "part"
run = [{py}, {step}, "{{part}}", "bm"]
reads = ["project.knobs", "fit.pol"]
owns = ["fit.bm"]

[step.sb]
scope = "part"
run = [{py}, {step}, "{{part}}", "sb"]
reads = ["fit.bm"]
owns = ["fit.sb"]

[step.leanc]
scope = "part"
run = [{py}, {step}, "{{part}}", "leancache"]
reads = ["project.uses", "uses.base:cache.lent"]
owns = ["fit.e"]

[step.picka]
scope = "part"
run = [{py}, {step}, "{{part}}", "lean"]
reads = ["project.uses", "uses.base:fit.a"]
owns = ["fit.d"]

[step.amender]
scope = "part"
run = [{py}, {step}, "{{part}}", "amender"]
amends = ["fit.a"]

[step.tell]
scope = "project"
run = [{py}, {step}, "{{project}}", "tell"]
reads = ["project.menu"]

[step.slow]
scope = "part"
run = [{py}, {step}, "{{part}}", "slow"]
owns = ["fit.s"]

[step.gather]
scope = "project"
run = [{py}, {step}, "{{project}}", "gather"]
reads = ["project.uses", "uses.*:*"]

[step.gatherx]
scope = "project"
run = [{py}, {step}, "{{project}}", "gather"]
reads = ["project.uses", "uses.*:export"]

[chain]
toy = ["one", "two", "three"]
gathering = ["gather"]
exporting = ["gatherx"]
told = ["one", "two", "three", "tell"]
guitarish = ["pol", "bm", "pol#2", "bm#2", "sb"]
amending = ["one", "amender"]
sneaky = ["sneak"]
leaning = ["lean"]
picking = ["picka"]
cachy = ["leanc"]
slowy = ["one", "slow"]
"""


@pytest.fixture
def toy(tmp_path):
    root = tmp_path / "projects"
    (root / "p").mkdir(parents=True)
    step = tmp_path / "step.py"
    step.write_text(STEP.format(analysis=os.path.join(HERE, "..", "..", "analysis")))

    def setup(x=3, fail=False, v2=1, family="toy", y=5, menu="A", warn=False):
        (root / "steps.toml").write_text(STEPS.format(py=json.dumps(sys.executable), step=json.dumps(str(step)), v2=v2))
        (root / "p" / "project.toml").write_text(
            f'[project]\nfamily = "{family}"\nexport = false\n[knobs]\nx = {x}\ny = {y}\nfail = {"true" if fail else "false"}\n'
            f'warn = {"true" if warn else "false"}\n'
            f'[menu]\nname = "{menu}"\n[secret]\ny = 1\n[[part]]\nid = "p"\n')
    setup()

    def run(*args):
        env = {**os.environ, "PE_PROJECTS": str(root)}
        return subprocess.run([sys.executable, BUILD, *args], env=env, capture_output=True, text=True)

    def fit():
        return json.load(open(root / "p" / "fits" / "p.json"))

    def ledger():
        return json.load(open(ledger.path))
    ledger.path = root / "p" / "ledger.json"
    return setup, run, fit, ledger


def stale(run):
    r = run("status", "p")
    return {line.split()[0] for line in r.stdout.splitlines() if "fresh" not in line and line.strip()}


def test_build_runs_the_chain_and_records_it(toy):
    setup, run, fit, ledger = toy
    r = run("build", "p")
    assert r.returncode == 0, r.stderr
    assert fit() == {"a": 6, "noise": None, "b": 7, "c": 70}
    led = ledger()
    assert [led[f"part:p:{s}"]["seq"] for s in ("one", "two", "three")] == [1, 2, 3]
    assert stale(run) == set()                    # one never wrote its optional fit.maybe


def test_status_check_fails_on_a_stale_step(toy):
    # the check before a commit: silent and 0 when everything is fresh, the stale rows and 1 when not
    setup, run, fit, ledger = toy
    run("build", "p")
    r = run("status", "--check", "p")
    assert (r.returncode, r.stdout) == (0, "")
    setup(x=4)
    r = run("status", "--check", "p")
    assert r.returncode == 1 and "project.knobs" in r.stdout and "fresh" not in r.stdout


def test_status_check_of_all_passes_over_a_project_never_built_here(toy):
    # a checkout without the datasets has no caches: nothing in it can be judged
    setup, run, fit, ledger = toy
    r = run("status", "--check", "--all")
    assert r.returncode == 0 and "not built here" in r.stdout
    (ledger.path.parent / "cache").mkdir()
    (ledger.path.parent / "cache" / "x.json").write_text("{}")   # a step ran here
    assert run("status", "--check", "--all").returncode == 1


def test_a_lender_with_its_recordings_but_built_elsewhere_lends_what_is_committed(toy):
    # a fresh clone with a lender's set in data/ (someone made a new grand and has Salamander): its
    # committed ledger and fits came with the clone, its caches did not; rebuilding it would refit it
    import shutil
    setup, run, fit, ledger = toy
    q = ledger.path.parent.parent / "q"
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "picking"\n[uses]\nbase = "p"\n[[part]]\nid = "q"\n')
    assert run("build", "q").returncode == 0
    shutil.rmtree(ledger.path.parent / "cache")  # the clone: p's ledger and fits, no cache
    setup(x=4)
    p = ledger.path.parent / "project.toml"
    p.write_text(p.read_text() + '[[part.source]]\nid = "p_set"\nroot = "."\n')   # data/ itself: present
    assert "stale" not in run("status", "q").stdout
    r = run("build", "q")
    assert r.returncode == 0 and fit()["a"] == 6  # p not refit
    assert "p: " in r.stdout and "lends what is committed" in r.stdout
    assert "uses" in (s := json.loads(run("status", "--json", "q").stdout)) and "p" not in s["uses"]


def test_a_lender_taken_as_committed_without_the_cache_a_borrower_reads_fails_first(toy):
    # a lent cache git lacks (a lender the editor's Uses list picked, whose cache is not lent by any
    # committed project): the build says which, before a step dies on the missing file
    setup, run, fit, ledger = toy
    q = ledger.path.parent.parent / "q"
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "cachy"\n[uses]\nbase = "p"\n[[part]]\nid = "q"\n')
    p = ledger.path.parent / "project.toml"
    p.write_text(p.read_text() + '[[part.source]]\nid = "p_set"\nroot = "no-such-set"\n')
    r = run("build", "q")
    assert r.returncode != 0 and "uses.base:cache.lent" in r.stderr and "q: leanc" not in r.stdout
    lent = ledger.path.parent / "cache" / "p" / "lent.json"
    lent.parent.mkdir(parents=True)
    lent.write_text('{"v": 7}')
    assert run("build", "q").returncode == 0 and json.load(open(q / "fits" / "q.json")) == {"e": 7}


def test_status_check_of_all_passes_over_a_lender_without_its_recordings(toy):
    # a fresh clone holds a lender's lent caches (committed) but not its recordings
    setup, run, fit, ledger = toy
    root = ledger.path.parent.parent
    (ledger.path.parent / "cache").mkdir()
    (ledger.path.parent / "cache" / "x.json").write_text("{}")
    git = lambda *a: subprocess.run(["git", *a], cwd=root, capture_output=True, check=True)
    git("init", "-q")
    git("add", "-f", str(ledger.path.parent / "cache" / "x.json"))
    p = ledger.path.parent / "project.toml"
    p.write_text(p.read_text() + '[[part.source]]\nid = "p_set"\nroot = "no-such-set"\n')
    r = run("status", "--check", "--all")
    assert r.returncode == 0 and "not built here" in r.stdout


def test_status_check_of_all_fails_when_every_build_here_lacks_its_recordings(toy):
    # data/ not mounted: every project built here is passed over, and the check would judge nothing
    setup, run, fit, ledger = toy
    assert run("build", "p").returncode == 0     # built here: its logs in cache/
    p = ledger.path.parent / "project.toml"
    p.write_text(p.read_text() + '[[part.source]]\nid = "p_set"\nroot = "no-such-set"\n')
    r = run("status", "--check", "--all")
    assert r.returncode == 1 and "built here, but its recordings are not in data/" in r.stdout
    q = ledger.path.parent.parent / "q"            # one project still judged: the check stands on it
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "toy"\n[knobs]\nx = 1\ny = 1\n[[part]]\nid = "q"\n')
    assert run("build", "q").returncode == 0
    r = run("status", "--check", "--all")
    assert r.returncode == 0 and "built here, but its recordings are not in data/" in r.stdout


def test_status_check_of_all_passes_over_a_project_with_only_its_committed_caches(toy):
    # a fresh clone with a lender's set in data/ (the one it builds from): its lent caches, tracked,
    # are not a build made here
    setup, run, fit, ledger = toy
    root = ledger.path.parent.parent
    lent = ledger.path.parent / "cache" / "p" / "damper.json"
    lent.parent.mkdir(parents=True)
    lent.write_text("{}")
    git = lambda *a: subprocess.run(["git", *a], cwd=root, capture_output=True, check=True)
    git("init", "-q")
    git("add", "-f", str(lent))
    r = run("status", "--check", "--all")
    assert r.returncode == 0 and "not built here" in r.stdout
    (lent.parent / "analysis.json").write_text("{}")   # a step ran here
    assert run("status", "--check", "--all").returncode == 1


def test_a_step_whose_outputs_are_all_gone_is_stale(toy):
    setup, run, fit, ledger = toy
    run("build", "p")
    path = ledger.path.parent / "fits" / "p.json"
    doc = json.load(open(path))
    json.dump({k: v for k, v in doc.items() if k not in ("a", "noise")}, open(path, "w"))
    assert "output missing" in run("status", "p").stdout


def test_a_changed_input_makes_the_step_and_its_followers_stale(toy):
    setup, run, fit, ledger = toy
    run("build", "p")
    setup(x=4)
    assert stale(run) == {"one", "two", "three"}       # two reads knobs too; three follows two
    assert "project.knobs" in run("status", "p").stdout
    assert run("build", "p", "--stale").returncode == 0
    assert fit()["c"] == 90 and stale(run) == set()


def test_a_rerun_upstream_makes_the_followers_stale(toy):
    setup, run, fit, ledger = toy
    run("build", "p")
    run("build", "p", "--only", "two")
    assert stale(run) == {"three"}


def test_a_version_bump(toy):
    setup, run, fit, ledger = toy
    run("build", "p")
    setup(v2=2)
    assert stale(run) == {"two", "three"}


def test_a_seed_does_not_make_a_step_stale(toy):
    setup, run, fit, ledger = toy
    assert run("build", "p").returncode == 0
    assert run("build", "p", "--only", "one").returncode == 0      # one read c (70) as its seed
    assert fit()["noise"] == 70
    assert run("build", "p", "--from", "two").returncode == 0
    assert stale(run) == set()


def test_a_failing_step_leaves_outputs_and_ledger(toy):
    setup, run, fit, ledger = toy
    run("build", "p")
    before, led = fit(), ledger()
    setup(x=5, fail=True)
    r = run("build", "p")
    assert r.returncode != 0 and "asked to fail" in r.stdout + r.stderr
    after = fit()
    assert after["b"] == before["b"] and after["a"] == 10      # one ran; two restored
    assert ledger()["part:p:two"] == led["part:p:two"]


def test_an_undeclared_read_fails_the_step(toy):
    setup, run, fit, ledger = toy
    setup(family="sneaky")
    r = run("build", "p")
    assert r.returncode != 0 and "Undeclared" in r.stdout + r.stderr and "project.secret" in r.stdout + r.stderr


def test_status_reports_a_missing_borrowed_cache(toy):
    setup, run, fit, ledger = toy
    q = ledger.path.parent.parent / "q"
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "leaning"\n[uses]\nbase = "p"\n[[part]]\nid = "q"\n')
    r = run("status", "q")                       # p never built: no fits, no caches, no ledger
    assert r.returncode == 0 and "never run" in r.stdout
    assert run("build", "q").returncode == 0      # builds p first (its uses), then q
    assert json.load(open(q / "fits" / "q.json")) == {"d": 6}
    assert stale(lambda *a: run(a[0], "q")) == set()
    setup(x=4)                                    # p's input changes: q borrows a stale project
    out = run("status", "q").stdout               # (uses.base:* covers p's configuration too)
    assert "changed: uses.base:*" in out or "borrowed p stale" in out


def test_a_lender_without_its_recordings_lends_what_is_committed(toy):
    # a fresh clone has no data/: a lender's steps cannot run there, and what it lends is committed
    setup, run, fit, ledger = toy
    q = ledger.path.parent.parent / "q"
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "picking"\n[uses]\nbase = "p"\n[[part]]\nid = "q"\n')
    assert run("build", "q").returncode == 0
    setup(x=4)                                    # p changes, and its set is not in data/
    p = ledger.path.parent / "project.toml"
    p.write_text(p.read_text() + '[[part.source]]\nid = "p_set"\nroot = "no-such-set"\n')
    out = run("status", "q").stdout
    assert "stale" not in out and "fresh" in out
    r = run("build", "q")
    assert r.returncode == 0 and fit()["a"] == 6  # p's steps did not run
    assert "p: not built here or its recordings not in data/; lends what is committed" in r.stdout


def test_adopt_records_the_present_state(toy):
    setup, run, fit, ledger = toy
    run("build", "p")
    os.remove(ledger.path)
    assert stale(run) == {"one", "two", "three"}
    assert run("adopt", "p").returncode == 0 and stale(run) == set()


def test_record_mode(toy, tmp_path):
    setup, run, fit, ledger = toy
    log = tmp_path / "rec.jsonl"
    setup(family="sneaky")
    r = run("build", "p", "--record", str(log))
    assert r.returncode == 0
    rows = [json.loads(line) for line in open(log)]
    assert {"step": "sneak", "op": "read", "what": "project.secret"} in [{k: x[k] for k in ("step", "op", "what")} for x in rows]


def test_a_step_run_twice_in_a_chain(toy):
    # the guitar's polarizations and body modes run twice: the second polarizations fit reads the
    # body modes just fitted (not a seed there), and the first body-mode fit's read of the
    # polarizations is overwritten by the second run (not part of its fingerprint)
    setup, run, fit, ledger = toy
    setup(family="guitarish")
    assert run("build", "p").returncode == 0
    assert stale(run) == set()
    setup(family="guitarish", y=10)                     # an input only bm reads
    r = run("build", "p", "--stale")
    assert r.returncode == 0
    led = ledger()
    assert led["part:p:pol#2"]["seq"] > led["part:p:bm"]["seq"]      # pol#2 reran after bm
    assert stale(run) == set()
    f = fit()
    assert f["bm"] == f["pol"] + 10 and f["sb"] == f["bm"] + 1000


def test_an_amendment_follows_its_owner(toy):
    setup, run, fit, ledger = toy
    setup(family="amending")
    assert run("build", "p").returncode == 0
    assert run("build", "p", "--only", "one").returncode == 0      # one rewrites a: the amendment is gone
    assert stale(run) == {"amender"}


def test_a_tools_cache_makes_nothing_stale(toy):
    setup, run, fit, ledger = toy
    q = ledger.path.parent.parent / "q"
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "leaning"\n[uses]\nbase = "p"\n[[part]]\nid = "q"\n')
    assert run("build", "q").returncode == 0
    demo = ledger.path.parent / "cache" / "p" / "demo_events.json"   # a developer tool's output
    demo.parent.mkdir(parents=True, exist_ok=True)
    demo.write_text("{}")
    assert stale(lambda *a: run(a[0], "q")) == set()


def test_an_interrupted_step_restores_its_outputs(toy, monkeypatch):
    setup, run, fit, ledger = toy
    assert run("build", "p").returncode == 0
    before = fit()
    import build
    import project as P
    monkeypatch.setenv("PE_PROJECTS", str(ledger.path.parent.parent))
    for f in (P._index, P.registry, P._rules_for):
        f.cache_clear()

    def interrupted(cmd, **kw):
        P.write_json(P.fit_path("p"), {"a": "half"}, indent=1)
        raise KeyboardInterrupt
    monkeypatch.setattr(build.subprocess, "run", interrupted)
    with pytest.raises(KeyboardInterrupt):
        build.run_step("p", "one", "part", "p", build.ledger("p"))
    monkeypatch.undo()
    for f in (P._index, P.registry, P._rules_for):
        f.cache_clear()
    assert fit() == before


def test_a_borrowed_menu_makes_nothing_stale(toy):
    # the menu is presentation (only the export reads it): a borrower's fits do not depend on it,
    # and the borrowed project's export, stale after a menu edit, writes nothing a borrower reads
    setup, run, fit, ledger = toy
    setup(family="told")
    q = ledger.path.parent.parent / "q"
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "leaning"\n[uses]\nbase = "p"\n[[part]]\nid = "q"\n')
    assert run("build", "q").returncode == 0
    assert stale(lambda *a: run(a[0], "q")) == set()
    setup(family="told", menu="B")
    assert stale(run) == {"tell"}
    assert stale(lambda *a: run(a[0], "q")) == set()


def test_a_borrower_follows_only_the_lenders_steps_it_reads(toy):
    # q reads p's fit.a alone: p's later steps gone stale (a version bump of two) leave q fresh, as
    # a grand that borrows Grand E's dampers is no older when Grand E's pedal fit is stale
    setup, run, fit, ledger = toy
    setup()
    q = ledger.path.parent.parent / "q"
    q.mkdir()
    (q / "project.toml").write_text('[project]\nfamily = "picking"\n[uses]\nbase = "p"\n[[part]]\nid = "q"\n')
    assert run("build", "q").returncode == 0
    assert stale(lambda *a: run(a[0], "q")) == set()
    setup(v2=2)
    assert stale(run) == {"two", "three"}
    assert stale(lambda *a: run(a[0], "q")) == set()
    # one gone stale (its input changed): q reads its output, so q is stale too
    setup(x=4, v2=2)
    assert "picka" in stale(lambda *a: run(a[0], "q"))


def test_what_feeds_a_borrower_follows_the_registry(toy, monkeypatch):
    # _feeds reads the registry: within one process, a changed registry changes its answer
    setup, run, fit, ledger = toy
    import build
    import project as P
    monkeypatch.setenv("PE_PROJECTS", str(ledger.path.parent.parent))
    root = ledger.path.parent.parent

    def fresh():
        for f in (P._index, P.registry, P._rules_for):
            f.cache_clear()
    fresh()
    assert build._feeds("one") and not build._feeds("tell")
    steps = (root / "steps.toml").read_text()
    (root / "steps.toml").write_text(steps.replace('owns = ["fit.a", "fit.noise", "fit.maybe"]', 'owns = []'))
    fresh()
    assert not build._feeds("one")
    monkeypatch.undo()
    fresh()


def events(r):
    return [json.loads(line) for line in r.stdout.splitlines() if line.strip()]


def test_json_events_of_a_build(toy):
    setup, run, fit, ledger = toy
    r = run("build", "--json", "p")
    assert r.returncode == 0, r.stderr
    ev = events(r)
    assert [(e["event"], e.get("step")) for e in ev] == [
        ("start", "one"), ("done", "one"), ("start", "two"), ("done", "two"), ("start", "three"), ("done", "three"),
        ("finished", None)]
    assert ev[-1] == {"event": "finished", "ok": True}
    first = ev[0]
    assert (first["project"], first["kind"], first["id"]) == ("p", "part", "p")
    assert first["log"].endswith(os.path.join("cache", "logs", "part-p.one.log"))
    assert ev[1]["seconds"] >= 0


def test_json_events_of_a_failing_build(toy):
    setup, run, fit, ledger = toy
    setup(fail=True)
    r = run("build", "--json", "p")
    assert r.returncode == 1
    ev = events(r)
    failed = [e for e in ev if e["event"] == "failed"]
    assert [(e["step"], e["exit"]) for e in failed] == [("two", 1)]
    assert ev[-1] == {"event": "finished", "ok": False}
    assert "asked to fail" not in r.stdout, "the log stays in its file"


def test_a_steps_warnings_reach_the_build(toy):
    # a step's "warning: " lines (a fit that jumped) are shown, not left in its log
    setup, run, fit, ledger = toy
    setup(warn=True)
    r = run("build", "p")
    assert r.returncode == 0, r.stderr
    assert "p: two: warning: b jumped, listen before committing" in r.stdout
    assert "(indented: not one)" not in r.stdout
    r = run("build", "--json", "--from", "two", "p")
    warned = [e for e in events(r) if e["event"] == "warning"]
    assert [(e["step"], e["message"]) for e in warned] == [("two", "b jumped, listen before committing")]


def test_status_json(toy):
    setup, run, fit, ledger = toy
    s = json.loads(run("status", "--json", "p").stdout)
    assert (s["project"], s["uses"]) == ("p", {})
    assert [(r["step"], r["kind"], r["id"], r["reason"]) for r in s["rows"]] == [
        ("one", "part", "p", "never run"), ("two", "part", "p", "never run"), ("three", "part", "p", "never run")]
    assert run("build", "p").returncode == 0
    s = json.loads(run("status", "--json", "p").stdout)
    assert [r["reason"] for r in s["rows"]] == [None, None, None]


@pytest.mark.parametrize("sig,ignored", [(signal.SIGINT, False), (signal.SIGTERM, False), (signal.SIGINT, True)])
def test_a_cancelled_build_restores_the_running_step(toy, sig, ignored):
    # ignored: started with SIGINT ignored, as a shell's background job (and its children) is
    setup, run, fit, ledger = toy
    setup(family="slowy")
    env = {**os.environ, "PE_PROJECTS": str(ledger.path.parent.parent)}
    pre = (lambda: signal.signal(signal.SIGINT, signal.SIG_IGN)) if ignored else None
    p = subprocess.Popen([sys.executable, BUILD, "build", "--json", "p"], env=env, stdout=subprocess.PIPE, text=True,
                         preexec_fn=pre)
    for line in p.stdout:
        e = json.loads(line)
        if e["event"] == "start" and e["step"] == "slow":
            break
    deadline = time.monotonic() + 20
    while "half" not in (ledger.path.parent / "fits" / "p.json").read_text():
        assert time.monotonic() < deadline, "the slow step never wrote"
        time.sleep(0.05)
    p.send_signal(sig)
    rest = [json.loads(line) for line in p.stdout]
    assert p.wait(timeout=30) == 130
    assert rest[-1] == {"event": "finished", "ok": False, "cancelled": True}
    assert "s" not in fit()
    assert set(ledger()) == {"part:p:one"}


def test_an_interrupt_ignores_the_signals_after_it():
    # a second Cancel (or the editor closing) during run_step's restore would abandon it half way
    import build
    old = {s: signal.getsignal(s) for s in (signal.SIGINT, signal.SIGTERM)}
    try:
        with pytest.raises(KeyboardInterrupt):
            build._interrupt(signal.SIGTERM, None)
        assert signal.getsignal(signal.SIGINT) == signal.SIG_IGN
        assert signal.getsignal(signal.SIGTERM) == signal.SIG_IGN
    finally:
        for s, h in old.items():
            signal.signal(s, h)


def test_a_wildcard_borrow_reads_every_role(toy):
    setup, run, fit, ledger = toy
    root = ledger.path.parent.parent
    for name in ("q", "z"):
        (root / name).mkdir()
        (root / name / "project.toml").write_text(f'[project]\nfamily = "toy"\n[knobs]\nx = 1\ny = 1\n[[part]]\nid = "{name}"\n')
    (root / "r").mkdir()
    (root / "r" / "project.toml").write_text('[project]\nfamily = "gathering"\n[uses]\na = "p"\nb = "q"\n')
    assert run("build", "r").returncode == 0      # builds p and q first
    assert stale(lambda *a: run(a[0], "r")) == set()
    (root / "z" / "project.toml").write_text('[project]\nfamily = "toy"\n[knobs]\nx = 9\ny = 1\n[[part]]\nid = "z"\n')
    assert stale(lambda *a: run(a[0], "r")) == set()   # z is not borrowed
    (root / "q" / "project.toml").write_text('[project]\nfamily = "toy"\n[knobs]\nx = 2\ny = 1\n[[part]]\nid = "q"\n')
    out = run("status", "r").stdout
    assert "changed: uses.*:*" in out or "borrowed q stale" in out


def test_a_borrowed_export_read_follows_the_files(toy, tmp_path):
    setup, run, fit, ledger = toy
    root = ledger.path.parent.parent
    inst = tmp_path / "instruments"
    for name in ("p", "q"):
        (inst / name).mkdir(parents=True)
        (inst / name / "instrument.json").write_text("{}")
        (inst / name / "data.bin").write_bytes(b"1234")
    (root / "q").mkdir()
    (root / "q" / "project.toml").write_text('[project]\nfamily = "toy"\n[knobs]\nx = 1\ny = 1\n[[part]]\nid = "q"\n')
    (root / "r").mkdir()
    (root / "r" / "project.toml").write_text('[project]\nfamily = "exporting"\n[uses]\na = "p"\nb = "q"\n')
    env = {**os.environ, "PE_PROJECTS": str(root), "PE_INSTRUMENTS": str(inst)}
    go = lambda *a: subprocess.run([sys.executable, BUILD, *a], env=env, capture_output=True, text=True)  # noqa: E731
    assert go("build", "r").returncode == 0
    assert "fresh" in go("status", "r").stdout
    (inst / "q" / "data.bin").write_bytes(b"12345")      # q re-exported, its fits unchanged
    assert "changed: uses.*:export" in go("status", "r").stdout


def test_a_borrowed_project_is_read_once(toy):
    # a rolled piano names each reference in several roles: its status read each borrowed project's
    # anew per role and per read, and recursively (55 s for one rolled project, 2026-10-17)
    setup, run, fit, ledger = toy
    root = ledger.path.parent.parent
    (root / "q").mkdir()
    (root / "q" / "project.toml").write_text('[project]\nfamily = "toy"\n[knobs]\nx = 1\ny = 1\n[[part]]\nid = "q"\n')
    (root / "r").mkdir()
    (root / "r" / "project.toml").write_text('[project]\nfamily = "gathering"\n[uses]\na = "p"\nb = "q"\nc = "p"\nd = "q"\n')
    assert run("build", "r").returncode == 0
    probe = (f"import sys, collections, json; sys.path.insert(0, {os.path.join(HERE, '..')!r}); import build\n"
             "n = collections.Counter(); real = build.status\n"
             "def counted(name, *a, **k):\n    n[name] += 1\n    return real(name, *a, **k)\n"
             "build.status = counted\nbuild.status_json('r')\nprint(json.dumps(n))\n")
    env = {**os.environ, "PE_PROJECTS": str(root)}
    out = subprocess.run([sys.executable, "-c", probe], env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    assert json.loads(out.stdout) == {"r": 1, "p": 1, "q": 1}


def test_a_build_reads_the_status_again_only_after_a_step_ran(toy):
    # a stale build looked the whole status up before each step of each borrowed project (13 s for
    # a rolled piano with nothing to build, 2026-10-17)
    setup, run, fit, ledger = toy
    root = ledger.path.parent.parent
    (root / "q").mkdir()
    (root / "q" / "project.toml").write_text('[project]\nfamily = "toy"\n[knobs]\nx = 1\ny = 1\n[[part]]\nid = "q"\n')
    (root / "r").mkdir()
    (root / "r" / "project.toml").write_text('[project]\nfamily = "gathering"\n[uses]\na = "p"\nb = "q"\nc = "p"\n')
    assert run("build", "r").returncode == 0
    probe = (f"import sys, collections, json; sys.path.insert(0, {os.path.join(HERE, '..')!r}); import build\n"
             "n = collections.Counter(); real = build.status\n"
             "def counted(name, *a, **k):\n    n[name] += 1\n    return real(name, *a, **k)\n"
             "build.status = counted\nassert build.build('r', stale_only=True)\nprint(json.dumps(n))\n")
    env = {**os.environ, "PE_PROJECTS": str(root)}
    out = subprocess.run([sys.executable, "-c", probe], env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    assert json.loads(out.stdout) == {"r": 1, "p": 2, "q": 2}
    # a stale step still runs, and the steps after it are looked at anew
    setup(x=4)
    assert run("build", "--stale", "p").returncode == 0
    assert fit()["c"] == ((4 * 2) + 1) * 10
