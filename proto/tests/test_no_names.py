"""No project name or source id as a string literal in the pipeline's code (the project files
hold every per-instrument choice)."""
import ast
import glob
import os
import re

import project as P

ALLOWED = {("analysis/split_runs.py", "guitar"): "the Iowa guitar runs' file names"}


def test_no_project_names_in_the_code():
    projects, parts, sources = P.index()
    names = set(projects) | set(parts) | set(sources)
    found = []
    for path in sorted(glob.glob(os.path.join(P.REPO, "analysis", "*.py")) + glob.glob(os.path.join(P.REPO, "proto", "*.py"))
                       + glob.glob(os.path.join(P.REPO, "proto", "brass", "*.py"))):
        rel = os.path.relpath(path, P.REPO)
        for node in ast.walk(ast.parse(open(path).read())):
            if isinstance(node, ast.Constant) and isinstance(node.value, str) and node.value in names:
                if (rel, node.value) not in ALLOWED:
                    found.append(f"{rel}:{node.lineno}: {node.value!r}")
    assert not found, "\n".join(found)


# a C/C++ string literal (escapes kept as written)
LITERAL = re.compile(r'"((?:[^"\\\n]|\\.)*)"')


def test_no_instrument_names_in_the_plugin():
    names = {d for d in os.listdir(os.path.join(P.REPO, "instruments"))
             if os.path.isfile(os.path.join(P.REPO, "instruments", d, "instrument.json"))}
    found = []
    for path in sorted(glob.glob(os.path.join(P.REPO, "plugin", "src", "**", "*.[ch]pp"), recursive=True)
                       + glob.glob(os.path.join(P.REPO, "plugin", "src", "*.h"))):
        rel = os.path.relpath(path, P.REPO)
        for n, line in enumerate(open(path), 1):
            if line.lstrip().startswith("//"):
                continue
            for m in LITERAL.finditer(line):
                if m.group(1) in names:
                    found.append(f"{rel}:{n}: {m.group(0)}")
    assert not found, "\n".join(found)


def test_no_names_in_the_editor():
    projects, parts, sources = P.index()
    names = set(projects) | set(parts) | set(sources)
    found = []
    for path in sorted(glob.glob(os.path.join(P.REPO, "editor", "src", "**", "*.rs"), recursive=True)):
        rel = os.path.relpath(path, P.REPO)
        for n, line in enumerate(open(path), 1):
            if line.lstrip().startswith("//"):
                continue
            for m in LITERAL.finditer(line):
                if m.group(1) in names:
                    found.append(f"{rel}:{n}: {m.group(0)}")
    assert not found, "\n".join(found)


def test_no_names_in_the_roll():
    # the roll's rules (engine/src/roll, its binary) hold for any piano: the choices are a rolled
    # project's file
    projects, parts, sources = P.index()
    names = set(projects) | set(parts) | set(sources)
    found = []
    paths = (glob.glob(os.path.join(P.REPO, "engine", "src", "roll", "*.rs"))
             + [os.path.join(P.REPO, "engine", "src", "roll.rs"), os.path.join(P.REPO, "engine", "src", "bin", "roll.rs")])
    for path in sorted(paths):
        rel = os.path.relpath(path, P.REPO)
        for n, line in enumerate(open(path), 1):
            if line.lstrip().startswith("//"):
                continue
            for m in LITERAL.finditer(line):
                if m.group(1) in names:
                    found.append(f"{rel}:{n}: {m.group(0)}")
    assert not found, "\n".join(found)
