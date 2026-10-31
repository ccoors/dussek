"""The public tree: nothing in it points into private/ (the maintainer's private material, not in a
stranger's clone) or names a path on someone's machine."""
import re
import subprocess

import project as P

# what lives in private/ (its own repository, listed in .gitignore)
PRIVATE_PATHS = ("docs/superpowers", "docs/papers", "docs/patents", "albums/1/")
# the files that may name private/ itself: the pointer to its notes and the ignore rule
MAY_NAME_PRIVATE = {"AGENTS.md", ".gitignore"}
# a home directory (not a path that merely ends in one, as a test's fake root "/x/home/")
HOME = re.compile(r"(?<![\w.])/home/|~/dev/")


def tracked_text():
    """(path, lines) of every tracked text file (a NUL in the first 8 KiB: binary, skipped)"""
    out = subprocess.run(["git", "ls-files", "-z"], cwd=P.REPO, capture_output=True, check=True).stdout
    for rel in out.decode().split("\0"):
        if not rel:
            continue
        try:
            raw = open(f"{P.REPO}/{rel}", "rb").read()
        except (FileNotFoundError, IsADirectoryError):
            continue
        if b"\0" in raw[:8192]:
            continue
        yield rel, raw.decode(errors="replace").splitlines()


def test_nothing_points_into_private():
    hits = []
    for rel, lines in tracked_text():
        if rel == "proto/tests/test_public_tree.py":
            continue
        for n, line in enumerate(lines, 1):
            if any(p in line for p in PRIVATE_PATHS) or ("private/" in line and rel not in MAY_NAME_PRIVATE):
                hits.append(f"{rel}:{n}: {line.strip()[:120]}")
    assert hits == [], "\n".join(hits)


def test_no_home_paths():
    hits = []
    for rel, lines in tracked_text():
        if rel == "proto/tests/test_public_tree.py":
            continue
        for n, line in enumerate(lines, 1):
            if HOME.search(line):
                hits.append(f"{rel}:{n}: {line.strip()[:120]}")
    assert hits == [], "\n".join(hits)


def test_the_code_names_one_license():
    # the plugin's metadata (LV2's doap:license and the other formats') and the crates say what LICENSE is
    assert open(f"{P.REPO}/LICENSE").read().split()[:4] == ["GNU", "AFFERO", "GENERAL", "PUBLIC"]
    for path in ("engine/Cargo.toml", "editor/Cargo.toml"):
        assert 'license = "AGPL-3.0-or-later"' in open(f"{P.REPO}/{path}").read(), path
    plugin = open(f"{P.REPO}/plugin/src/PianoPlugin.cpp").read()
    assert re.search(r'getLicense\(\) const override \{ return "AGPL-3\.0-or-later"; \}', plugin)
