"""Every committed instrument the engine's tests and code name exists: many of them load it with
`Instrument::load(...) else { return }` (a missing dataset skips a test), so a renamed or mistyped
name would pass silently."""
import os
import re
import subprocess

import project as P

PATTERNS = [re.compile(r"instruments/([a-z][a-z0-9_]*)\b(?![.\-])"),     # ".../instruments/grand_a"
            re.compile(r"\{ROOT\}([a-z][a-z0-9_]*)"),                      # format!("{ROOT}upright_a")
            re.compile(r"\b(?:dir|ir|impact|check|with_trim|find)\(\"([a-z][a-z0-9_]*)\"")]  # the tests' helpers
# names that are meant to be missing (the guard's own test)
MISSING = {"no_such"}


def test_every_named_instrument_exists():
    files = subprocess.run(["git", "ls-files", "engine"], cwd=P.REPO, capture_output=True, text=True, check=True).stdout.split()
    missing = []
    for rel in files:
        if not rel.endswith((".rs", ".toml")):
            continue
        text = open(os.path.join(P.REPO, rel)).read()
        for pat in PATTERNS:
            for m in pat.finditer(text):
                name = m.group(1)
                if name not in MISSING and not os.path.isdir(os.path.join(P.REPO, "instruments", name)):
                    missing.append(f"{rel}: {name}")
    assert missing == []


def test_the_scan_sees_a_stale_name(tmp_path):
    # the patterns catch the forms the tests use
    text = 'concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/salamander")\nformat!("{ROOT}knight")\ndir("iowa")'
    found = {m.group(1) for pat in PATTERNS for m in pat.finditer(text)}
    assert found == {"salamander", "knight", "iowa"}
