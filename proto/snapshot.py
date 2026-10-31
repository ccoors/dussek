#!/usr/bin/env python3
"""Snapshots: an instrument's export at a git ref, beside the working copy (design
2026-10-21-traits-plan, the rail).

`instruments/<name>/` at the ref goes to `<snapshots>/<name>@<ref>` (a `/` in the ref becomes `-`),
its menu name marked with the ref, its menu group "Snapshots", its profile `<profile>@<ref>#<commit>`
(the engine caches the board's and the room's responses by profile). The plugin lists the folder after
the instruments (Catalog::searchPaths). A snapshot is the old data played by today's engine: an
export the engine no longer loads is refused with the engine's message, and nothing is kept.

Usage: snapshot.py REF [NAME …] [--dest DIR] [--remove]
"""
import argparse
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, ".."))
RENDER = os.path.join(REPO, "engine", "target", "release", "render")
GROUP = "Snapshots"


class SnapshotError(Exception):
    pass


def snapshots_dir():
    """The data directory's snapshots folder, as the plugin searches it."""
    data = os.environ.get("XDG_DATA_HOME") or os.path.join(os.environ.get("HOME", "~"), ".local", "share")
    return os.path.join(data, "dussek", "snapshots")


def safe_ref(ref):
    return ref.replace("/", "-")


def engine_loads(instrument_dir):
    """None when the engine loads and plays the instrument, else its message."""
    if not os.path.exists(RENDER):
        raise SnapshotError("build the engine first: just engine")
    with tempfile.TemporaryDirectory() as tmp:
        events = os.path.join(tmp, "events.json")
        with open(events, "w") as fh:
            json.dump({"sr": 48000, "t_end": 0.05, "pedal": [], "events": []}, fh)
        r = subprocess.run([RENDER, instrument_dir, events, os.path.join(tmp, "out.wav")], capture_output=True, text=True)
    return None if r.returncode == 0 else (r.stderr.strip() or f"render exited with {r.returncode}")


def _git(repo, *args, binary=False):
    r = subprocess.run(["git", "-C", repo, *args], capture_output=True, text=not binary)
    return r.returncode, r.stdout


def _names_at(repo, ref):
    code, out = _git(repo, "ls-tree", "--name-only", f"{ref}:instruments")
    if code:
        return []
    names = []
    for n in out.split():
        if _git(repo, "cat-file", "-e", f"{ref}:instruments/{n}/instrument.json")[0] == 0:
            names.append(n)
    return names


def _extract(repo, ref, name, into):
    code, data = _git(repo, "archive", "--format=tar", ref, f"instruments/{name}", binary=True)
    if code:
        raise SnapshotError(f"no instruments/{name} at {ref}")
    prefix = f"instruments/{name}/"
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        for m in tar.getmembers():
            if m.isfile() and m.name.startswith(prefix) and "/" not in m.name[len(prefix):]:
                with open(os.path.join(into, m.name[len(prefix):]), "wb") as fh:
                    fh.write(tar.extractfile(m).read())


def _mark(instrument_dir, name, tag, commit):
    path = os.path.join(instrument_dir, "instrument.json")
    try:
        with open(path) as fh:
            j = json.load(fh)
        menu = dict(j.get("menu") or {})
    except (ValueError, AttributeError) as e:
        raise SnapshotError(f"{name}: instrument.json unreadable: {e}") from e
    menu["name"] = f"{menu.get('name') or name} @ {tag}"
    menu["group"] = GROUP
    j["menu"] = menu
    # the commit too: a moving ref's next snapshot is another board and room for the engine's caches
    j["profile"] = f"{j.get('profile') or name}@{tag}#{commit[:9]}"
    # an export before step 214 kept its measured level in level.json: the engine reads instrument.json's
    level = os.path.join(instrument_dir, "level.json")
    if os.path.exists(level):
        with open(level) as fh:
            j.setdefault("level", json.load(fh))
        os.remove(level)
    with open(path, "w") as fh:
        json.dump(j, fh)


def snapshot(ref, names, dest, repo=REPO, check=engine_loads):
    """Write the exports of `names` (None: every instrument at the ref) at `ref` into `dest`; the
    directories written. All or nothing: an unknown ref or name, or an export the check refuses,
    leaves `dest` as it was."""
    code, commit = _git(repo, "rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}")
    if code:
        raise SnapshotError(f"no such ref: {ref}")
    available = _names_at(repo, ref)
    names = available if names is None else list(names)
    missing = [n for n in names if n not in available]
    if missing:
        raise SnapshotError(f"no instrument {', '.join(missing)} at {ref}")
    tag = safe_ref(ref)
    os.makedirs(dest, exist_ok=True)
    staged = []
    try:
        for name in names:
            tmp = tempfile.mkdtemp(prefix=f".tmp-{name}@{tag}-", dir=dest)
            staged.append((name, tmp))
            _extract(repo, ref, name, tmp)
            problem = check(tmp)            # the export as committed: the engine's own message
            if problem:
                raise SnapshotError(f"{name} at {ref}: {problem}")
            _mark(tmp, name, tag, commit.strip())
        written = []
        for name, tmp in staged:
            final = os.path.join(dest, f"{name}@{tag}")
            if os.path.exists(final):
                shutil.rmtree(final)
            os.chmod(tmp, 0o755)            # mkdtemp's 0700
            os.rename(tmp, final)
            written.append(final)
        return written
    finally:
        for _, tmp in staged:
            if os.path.exists(tmp):
                shutil.rmtree(tmp)


def remove(ref, dest, names=None):
    """Remove the snapshots of `ref` (of `names` only, if given) from `dest`; the directories removed."""
    suffix = "@" + safe_ref(ref)
    gone = []
    for d in sorted(os.listdir(dest)) if os.path.isdir(dest) else []:
        if d.endswith(suffix) and os.path.isdir(os.path.join(dest, d)) and (not names or d[:-len(suffix)] in names):
            shutil.rmtree(os.path.join(dest, d))
            gone.append(os.path.join(dest, d))
    return gone


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("ref")
    ap.add_argument("names", nargs="*", help="instrument directories (default: every one at the ref)")
    ap.add_argument("--dest", default=snapshots_dir())
    ap.add_argument("--remove", action="store_true", help="remove the ref's snapshots (of the names, if given) instead")
    args = ap.parse_args()
    try:
        done = remove(args.ref, args.dest, args.names) if args.remove else snapshot(args.ref, args.names or None, args.dest)
    except SnapshotError as e:
        sys.exit(f"snapshot: {e}")
    for d in done:
        print(("removed " if args.remove else "") + d)


if __name__ == "__main__":
    main()
