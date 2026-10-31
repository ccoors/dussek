#!/usr/bin/env python3
"""The pipeline's driver: runs a project's chain (projects/steps.toml), records every run in
projects/<name>/ledger.json and tells which steps are stale and why.

  build.py status <project>… | --all
  build.py build <project>… | --all [--from S | --only S[,S…] | --stale] [--no-uses] [--record FILE]
  build.py adopt <project>… | --all        the present outputs taken as fresh (no runs)
  build.py … --json            status as JSON, a build's events as JSON lines (the editor)

A step is stale when it never ran, its version changed, one of its declared reads changed
(the ledger names it), an earlier step it reads from ran after it or is stale, a borrowed
project's step is stale, or one of its outputs is missing."""
import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from functools import lru_cache

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "analysis"))
import project as P  # noqa: E402

JSON = False   # --json: status as JSON, a build's progress as JSON events (the editor's)


def say(text, **event):
    """A line for the shell, or with --json the event as one JSON line (text None: JSON only)."""
    if JSON:
        print(json.dumps(event), flush=True)
    elif text is not None:
        print(text, flush=True)


def _interrupt(signum, frame):
    """SIGTERM (and SIGINT, also when started with it ignored: a shell's background job) as
    Ctrl-C: the running step killed, its files restored (run_step). The signals after it are
    ignored: a second Cancel during the restore would abandon it half way."""
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    raise KeyboardInterrupt


def chain(name):
    cfg = P.config(name)["project"]
    steps = P.registry()["chain"][cfg["family"]]
    skip = set(cfg.get("skip", []))
    if skip - set(steps):
        raise SystemExit(f"{name}: skip names steps not in its chain: {sorted(skip - set(steps))}")
    return [s for s in steps if s not in skip]


def scopes(name, instance):
    scope = P.step_entry(instance)["scope"]
    if scope == "project":
        return [("project", name)]
    if scope == "part":
        return [("part", p) for p in P.part_ids(name)]
    return [("source", s) for s in P.source_ids(name)]


def _parts(name, kind, id_):
    return P.part_ids(name) if kind == "project" else [id_ if kind == "part" else P.part_of(id_)]


def _sources(name, kind, id_):
    return [id_] if kind == "source" else [s for p in _parts(name, kind, id_) for s in P.source_ids(p)]


def _canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def _fit(pid):
    path = P.fit_path(pid)
    return json.load(open(path)) if os.path.exists(path) else {}


def _writes(instance):
    """What a step may write: its outputs, its amendments and its memos (caches keyed by their own
    inputs, cache.release_peaks, rebuilt by whichever step needs them)."""
    e = P.step_entry(instance)
    return set(e.get("owns", [])) | set(e.get("amends", [])) | set(e.get("memos", []))


def _feeds(instance):
    """the step owns or amends a fit or a cache (what a borrower reads)"""
    e = P.step_entry(instance)
    return any(w.startswith(("fit.", "cache.")) for w in e.get("owns", []) + e.get("amends", []))


@lru_cache(maxsize=None)
def _owned_caches():
    """The caches some step owns: a developer tool's output beside them (demo_events.json) is no
    step's input."""
    return {w[len("cache."):] for e in P.registry()["step"].values() for w in e.get("owns", []) if w.startswith("cache.")}


# the exports' folder ($PE_INSTRUMENTS for tests)
INSTRUMENTS = os.environ.get("PE_INSTRUMENTS") or os.path.join(P.REPO, "instruments")

# project sections only the export reads (the plugin's menu entry): a borrower does not depend on them
PRESENTATION = {"menu"}


def value(read, name, kind, id_, exclude=()):
    """The bytes a declared read stands for in this scope (keys in `exclude`, the step's own
    outputs, left out)."""
    if read.startswith("uses."):
        role, what = read[len("uses."):].split(":", 1)
        if role == "*":
            # every role's (a rolled piano reads every project it names), in the roles' order
            uses = P.config(name).get("uses", {})
            return b"".join(r.encode() + b"=" + value(f"uses.{r}:{what}", name, kind, id_, exclude) for r in sorted(uses))
        other = P.config(name).get("uses", {}).get(role)
        if other is None:
            return b"<none>"
        if what == "*":
            # the borrowed project's configuration without its presentation (no fit reads the menu)
            # and its level corrections (only its export reads them)
            cfg = {k: v for k, v in _without_corrections(P.config(other)).items() if k not in PRESENTATION}
            return _canon(cfg) + b"".join(value(w, other, "project", other) for w in ("fit.*", "cache.*"))
        return value(what, other, "project", other)
    if read == "export":
        # the project's exported files (what a rolled piano reads: a re-export after a change of the
        # export alone, its level corrections or the engine's needs no fit to change)
        d = os.path.join(INSTRUMENTS, name)
        files = [os.path.join(d, f) for f in ("instrument.json", "data.bin")]
        return b"".join(open(f, "rb").read() if os.path.exists(f) else b"<none>" for f in files)
    kind_, key = read.split(".", 1)
    if kind_ == "project":
        cfg = P.config(name)
        if key == "*":
            return _canon(_without_corrections(cfg))
        if key == "sources":
            # a source's level corrections are their own read: a change to them leaves the
            # analyses and fits fresh, only the export goes stale
            return _canon([{k: v for k, v in P.index()[2][s][2].items() if k not in NOT_SOURCES} for s in _sources(name, kind, id_)])
        if key == CORRECTIONS:
            return _canon([P.index()[2][s][2].get(CORRECTIONS) for s in _sources(name, kind, id_)])
        if key == "part":
            return _canon([{k: v for k, v in P.index()[1][p][1].items() if k != "source"} for p in _parts(name, kind, id_)])
        # a section with the scope's parts' own tables of it (P.section merges them)
        return _canon([cfg.get(key)] + [P.index()[1][p][1].get(key) for p in _parts(name, kind, id_)])
    if kind_ == "fit":
        docs = [_fit(p) for p in _parts(name, kind, id_)]
        if key == "*":
            return _canon([{k: v for k, v in d.items() if f"fit.{k}" not in exclude} for d in docs])
        return _canon([d.get(key, "<missing>") for d in docs])
    if kind_ == "cache":
        h = hashlib.sha256()
        for i in dict.fromkeys(_sources(name, kind, id_) + _parts(name, kind, id_)):
            d = os.path.join(P.project_dir(P.project_of(i)), "cache", i)
            names = sorted(f[:-5] for f in os.listdir(d) if f.endswith(".json")) if os.path.isdir(d) else []
            for n in ([n for n in names if n in _owned_caches()] if key == "*" else [key] if key in names else []):
                if f"cache.{n}" in exclude:
                    continue
                h.update(f"{i}/{n}".encode())
                h.update(open(os.path.join(d, f"{n}.json"), "rb").read())
        return h.digest()
    raise ValueError(f"read {read!r}")


def _in_chain(name, instance):
    """An instance's reads in its project's chain: (the reads its fingerprint covers, the reads that
    link it to earlier steps, the keys left out of its wildcard reads).
    - A seed is the previous run's value only while no earlier step of the chain writes it: the
      guitar's second polarization fit reads the body modes just fitted.
    - A key a later step writes is not this step's input any more once the chain has run (the first
      body-mode fit's polarizations, which the second polarization fit replaces; coupling's
      soundboard, which register amends): the run sequence links it to its earlier writer instead.
    - Its amendments link it to their owner (register follows soundboard)."""
    steps = chain(name)
    i = steps.index(instance)
    e = P.step_entry(instance)
    before = set().union(*(_writes(s) for s in steps[:i]))
    after = set().union(*(_writes(s) for s in steps[i + 1:]))
    own = _writes(instance)
    seeds = {x for x in e.get("seeds", []) if x not in before}
    reads = (set(e.get("reads", [])) | set(e.get("seeds", []))) - seeds
    exclude = own | after | seeds
    return reads - exclude, reads | set(e.get("amends", [])), exclude


#: a source's level corrections (from the editor's Compare page): read by the exports only
CORRECTIONS = "level_corrections"
#: a source's keys no step's fingerprint covers through project.sources: its corrections (a read of
#: their own) and how its recordings' levels group (the editor's alone)
NOT_SOURCES = (CORRECTIONS, "level_groups")
# project tables no step reads: the editor's take checks' thresholds, the recordings' credit
# (proto/license.py writes it beside the export; a changed credit is no reason to export again)
NOT_PROJECT = ("take_checks", "license")


def _without_corrections(cfg):
    """A project's configuration with its sources' level corrections and groups, and the editor's
    take checks, left out."""
    cfg = {k: v for k, v in cfg.items() if k not in NOT_PROJECT}
    parts = [{**p, "source": [{k: v for k, v in s.items() if k not in NOT_SOURCES} for s in p.get("source", [])]}
             for p in cfg.get("part", [])]
    return {**cfg, "part": parts} if "part" in cfg else cfg


def fingerprint(name, instance, kind, id_):
    fp_reads, _, exclude = _in_chain(name, instance)
    return {r: hashlib.sha256(value(r, name, kind, id_, exclude)).hexdigest()[:16] for r in sorted(fp_reads)}


def ledger_path(name):
    return os.path.join(P.project_dir(name), "ledger.json")


def ledger(name):
    path = ledger_path(name)
    return json.load(open(path)) if os.path.exists(path) else {}


def _covers(writes, reads):
    """Whether something written is among the reads (a wildcard read covers its kind)."""
    for w in writes:
        kind = w.split(".")[0]
        if w in reads or f"{kind}.*" in reads:
            return True
    return False


def _outputs_missing(name, instance, kind, id_):
    # a step's own fit keys may be optional (the felt fit writes felt_curve only where the levels are
    # recorded): its output is missing when none of them is there
    owned = [w[len("fit."):] for w in P.step_entry(instance).get("owns", []) if w.startswith("fit.")]
    for p in _parts(name, kind, id_) if owned else []:
        if not any(k in _fit(p) for k in owned):
            return f"fit.{owned[0]}"
    for w in sorted(_writes(instance) - set(P.step_entry(instance).get("memos", []))):
        kind_, key = w.split(".", 1)
        if kind_ == "cache":
            ids = [id_] if kind != "project" else P.part_ids(name)[:1]
            if any(not os.path.exists(P.cache_path(i, key)) for i in ids):
                return w
    return None


def status(name, led=None, _seen=None, _memo=None):
    """[(instance, kind, id, reason or None)] in chain order. `_memo`: the borrowed projects' rows
    already read in this call (a rolled piano names each reference in several roles: read anew per
    role, read and level, one rolled project's status took 55 s)."""
    led = ledger(name) if led is None else led
    _memo = {} if _memo is None else _memo
    rows, by = [], {}
    steps = chain(name)
    for i, inst in enumerate(steps):
        e = P.step_entry(inst)
        _, reads, _ = _in_chain(name, inst)
        for kind, id_ in scopes(name, inst):
            key = f"{kind}:{id_}:{inst}"
            got = led.get(key)
            reason = None
            if got is None:
                reason = "never run"
            elif got["version"] != e.get("version", 1):
                reason = f"version {got['version']} → {e.get('version', 1)}"
            else:
                fp = fingerprint(name, inst, kind, id_)
                changed = [r for r in fp if got["inputs"].get(r) != fp[r]]
                if changed:
                    reason = "changed: " + ", ".join(changed)
            if reason is None:
                for up in steps[:i]:
                    if not _covers(_writes(up), reads):
                        continue
                    for ukind, uid in scopes(name, up):
                        if set(_parts(name, ukind, uid)) & set(_parts(name, kind, id_)) or kind == "project":
                            u = by.get(f"{ukind}:{uid}:{up}")
                            if u and u[1]:
                                reason = f"{up} stale"
                            elif led.get(f"{ukind}:{uid}:{up}", {}).get("seq", 0) > got["seq"]:
                                reason = f"{up} ran after it"
                        if reason:
                            break
                    if reason:
                        break
            if reason is None:
                uses = P.config(name).get("uses", {})
                # per lender, what is read of it ("*": all of it)
                borrowed = {}
                for r in reads:
                    if r.startswith("uses."):
                        role, _, what = r[5:].partition(":")
                        for b in list(uses.values()) if role == "*" else [uses.get(role)]:
                            if b and b not in (_seen or set()):
                                borrowed.setdefault(b, set()).add(what or "*")
                for other, whats in borrowed.items():
                    if _lends_as_committed(other):
                        continue
                    if other not in _memo:
                        _memo[other] = status(other, None, (_seen or set()) | {name}, _memo)
                    # a stale step that writes no fit or cache (the export) leaves the borrower as it
                    # is, and so does one that writes nothing it reads (Grand E's pedal fit, stale, for
                    # a grand that reads Grand E's dampers)
                    if any(x[3] and _feeds(x[0]) and ("*" in whats or _covers(_writes(x[0]), whats))
                           for x in _memo[other]):
                        reason = f"borrowed {other} stale"
                        break
            if reason is None:
                missing = _outputs_missing(name, inst, kind, id_)
                if missing:
                    reason = f"output missing: {missing}"
            by[key] = (inst, reason)
            rows.append((inst, kind, id_, reason))
    return rows


def status_json(name):
    """status() as the editor reads it: the project's rows, then its borrowed projects' rows."""
    memo = {}

    def rows(n):
        if n not in memo:
            memo[n] = status(n, None, None, memo)
        return [{"step": s, "kind": k, "id": i, "reason": r} for s, k, i, r in memo[n]]
    uses = sorted(o for o in set(P.config(name).get("uses", {}).values()) if not _lends_as_committed(o))
    return {"project": name, "rows": rows(name), "uses": {o: rows(o) for o in uses}}


def _commit():
    run = lambda *a: subprocess.run(["git", *a], cwd=P.REPO, capture_output=True, text=True).stdout.strip()  # noqa: E731
    head = run("rev-parse", "--short", "HEAD") or "none"
    return head + ("+dirty" if run("status", "--porcelain", "--", "analysis", "proto") else "")


def _backup_paths(name, inst, kind, id_):
    paths = [P.fit_path(p) for p in _parts(name, kind, id_)]
    for w in _writes(inst):
        if w.startswith("cache."):
            paths += [P.cache_path(i, w[6:]) for i in (_sources(name, kind, id_) + _parts(name, kind, id_))]
    if kind == "project":
        d = os.path.join(P.REPO, "instruments", name)
        paths += [os.path.join(d, f) for f in ("instrument.json", "data.bin")]
    return list(dict.fromkeys(paths))


def run_step(name, inst, kind, id_, led, record=None):
    e = P.step_entry(inst)
    fill = {"project": name, "part": id_ if kind == "part" else "", "source": id_ if kind == "source" else ""}
    cmd = [a.format(**fill) for a in e["run"]]
    env = {**os.environ, "PE_STEP": inst, "PE_PROJECT": name}
    if record:
        env["PE_RECORD"] = record
    inputs = fingerprint(name, inst, kind, id_)
    log = os.path.join(P.project_dir(name), "cache", "logs", f"{kind}-{id_}.{inst}.log")
    os.makedirs(os.path.dirname(log), exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        saved = {}
        for i, path in enumerate(_backup_paths(name, inst, kind, id_)):
            if os.path.exists(path):
                saved[path] = os.path.join(tmp, str(i))
                shutil.copy2(path, saved[path])
            else:
                saved[path] = None
        what = {"project": name, "step": inst, "kind": kind, "id": id_, "log": log}
        say(f"{name}: {inst} ({kind} {id_})", event="start", **what)
        t0 = time.monotonic()
        def restore():
            for path, copy in saved.items():
                if copy:
                    # copied beside it, then renamed over it: a reader never sees it half written
                    shutil.copy2(copy, path + ".restore")
                    os.replace(path + ".restore", path)
                elif os.path.exists(path):
                    os.remove(path)
        try:
            with open(log, "w") as f:
                r = subprocess.run(cmd, cwd=P.REPO, env=env, stdout=f, stderr=subprocess.STDOUT)
        except BaseException:          # Ctrl-C (or anything) mid-step: its files as they were
            restore()
            raise
        if r.returncode != 0:
            restore()
            if not JSON:
                print(open(log).read()[-4000:])
            say(f"{name}: {inst} failed (exit {r.returncode}); outputs restored, log {log}", event="failed",
                exit=r.returncode, seconds=round(time.monotonic() - t0, 3), **what)
            return False
    # a step's "warning: …" lines (a fit that jumped: worth a listen) are shown, not left in its log
    with open(log) as f:
        for line in f:
            if line.startswith("warning: "):
                message = line[len("warning: "):].rstrip("\n")
                say(f"{name}: {inst}: warning: {message}", event="warning", message=message, **what)
    seq = max((v["seq"] for v in led.values()), default=0) + 1
    led[f"{kind}:{id_}:{inst}"] = {"version": e.get("version", 1), "inputs": inputs, "commit": _commit(), "seq": seq}
    P.write_json(ledger_path(name), led, indent=1, sort_keys=True)
    say(None, event="done", seconds=round(time.monotonic() - t0, 3), **what)
    return True


def build(name, start=None, only=None, stale_only=False, uses=True, record=None, _done=None):
    done = _done if _done is not None else set()
    if name in done:
        return True
    done.add(name)
    if uses:
        for other in sorted(set(P.config(name).get("uses", {}).values())):
            if _lends_as_committed(other):
                if other not in done:
                    done.add(other)
                    say(f"{other}: not built here or its recordings not in data/; lends what is committed",
                        project=other, event="lends_as_committed")
                missing = _missing_lent(name, other)
                if missing:
                    print(f"{name}: {other} lends what is committed, and git lacks what {name} reads of it: "
                          f"{', '.join(missing)} (its recordings in data/ would let it be built)",
                          file=sys.stderr, flush=True)
                    return False
                continue
            if not build(other, stale_only=True, record=record, _done=done):
                return False
    steps = chain(name)
    if start:
        steps = steps[steps.index(start):]
    if only:
        steps = [s for s in steps if s.split("#")[0] in only or s in only]
    led = ledger(name)
    # the status read again only after a step ran (before each step it took 13 s for a rolled piano
    # with nothing to build)
    rows = None
    for inst in steps:
        for kind, id_ in scopes(name, inst):
            if stale_only:
                rows = status(name, led) if rows is None else rows
                row = next(r for r in rows if r[:3] == (inst, kind, id_))
                if row[3] is None:
                    continue
            if not run_step(name, inst, kind, id_, led, record):
                return False
            rows = None
    return True


def adopt(name):
    led = ledger(name)
    seq = max((v["seq"] for v in led.values()), default=0)
    for inst in chain(name):
        for kind, id_ in scopes(name, inst):
            seq += 1
            led[f"{kind}:{id_}:{inst}"] = {"version": P.step_entry(inst).get("version", 1),
                                           "inputs": fingerprint(name, inst, kind, id_),
                                           "commit": _commit() + " adopted", "seq": seq}
    P.write_json(ledger_path(name), led, indent=1, sort_keys=True)


def _missing_lent(name, other):
    """The caches `name` reads of the lender `other` (uses.<role>:cache.<name>) that are not there"""
    roles = {r for r, o in P.config(name).get("uses", {}).items() if o == other}
    missing = []
    for inst in chain(name):
        for r in P.step_entry(inst).get("reads", []):
            role, _, what = r[len("uses."):].partition(":") if r.startswith("uses.") else ("", "", "")
            if role in roles and what.startswith("cache.") and not P.has_cache(P.main_part(other), what[6:]) \
                    and r not in missing:
                missing.append(r)
    return missing


def _lends_as_committed(name):
    """Whether a lender is taken as committed rather than rebuilt (and its staleness not passed on to
    its borrowers): its recordings are not in data/, or it was built elsewhere (its ledger came with a
    clone, nothing of it was built here). A fresh clone would otherwise refit it."""
    if not P.has_recordings(name):
        return True
    return not _built_here(name) and bool(ledger(name))


@lru_cache(maxsize=None)
def _built_here(name):
    """Whether a step ran on the project in this checkout: its cache holds a file git does not track
    (the tracked ones, what it lends, come with a clone)."""
    cache = os.path.join(P.project_dir(name), "cache")
    files = [os.path.realpath(os.path.join(d, f)) for d, _, fs in os.walk(cache) for f in fs]
    if not files:
        return False
    r = subprocess.run(["git", "ls-files", "-z"], cwd=cache, capture_output=True, text=True)
    tracked = {os.path.realpath(os.path.join(cache, f)) for f in r.stdout.split("\0") if f} if r.returncode == 0 else set()
    return any(f not in tracked for f in files)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=("status", "build", "adopt"))
    ap.add_argument("projects", nargs="*")
    ap.add_argument("--all", action="store_true")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--from", dest="start")
    g.add_argument("--only")
    g.add_argument("--stale", action="store_true")
    ap.add_argument("--no-uses", action="store_true", help="leave the borrowed projects as they are")
    ap.add_argument("--record", help="log every access (JSON lines) instead of checking")
    ap.add_argument("--json", action="store_true",
                    help="status as JSON; a build's progress as JSON events, one per line (the editor's)")
    ap.add_argument("--check", action="store_true",
                    help="status: only the stale steps, exit 1 if any (with --all, a project without caches is "
                         "passed over: never built in this checkout)")
    args = ap.parse_args()
    names = sorted(P.index()[0]) if args.all else args.projects
    if not names:
        ap.error("name projects or --all")
    global JSON
    JSON = args.json
    signal.signal(signal.SIGTERM, _interrupt)
    signal.signal(signal.SIGINT, _interrupt)
    ok = True
    try:
        judged, unmounted = 0, 0
        for name in names:
            if args.command == "status" and args.check:
                if args.all and not _built_here(name):
                    print(f"{name}: not built here (no caches), not checked")
                    continue
                if args.all and not P.has_recordings(name):
                    print(f"{name}: built here, but its recordings are not in data/ (a set not mounted?), "
                          "not checked")
                    unmounted += 1
                    continue
                judged += 1
                for inst, kind, id_, reason in status(name):
                    if reason:
                        print(f"{inst:16s} {kind}:{id_:16s} {reason}")
                        ok = False
            elif args.command == "status":
                if JSON:
                    print(json.dumps(status_json(name)), flush=True)
                else:
                    for inst, kind, id_, reason in status(name):
                        print(f"{inst:16s} {kind}:{id_:16s} {reason or 'fresh'}")
            elif args.command == "adopt":
                adopt(name)
            else:
                only = set(args.only.split(",")) if args.only else None
                ok = build(name, args.start, only, args.stale, not args.no_uses, args.record) and ok
    except KeyboardInterrupt:
        say("interrupted: the running step's outputs restored", event="finished", ok=False, cancelled=True)
        sys.exit(130)
    if args.command == "build":
        say(None, event="finished", ok=ok)
    if args.command == "status" and args.check and unmounted and not judged:
        print(f"nothing checked: the {unmounted} built here lack their recordings in data/")
        ok = False
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
