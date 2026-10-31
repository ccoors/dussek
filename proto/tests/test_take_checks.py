"""The editor's take checks in the project file: the thresholds ([take_checks]) are the editor's
alone (nothing goes stale), a source's skip_files changes what the analysis reads."""
import os
import shutil
import tomllib

import pytest

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "..", "projects")
# a project whose export reads project.* (the string export)
NAME = "grand_a"


def _lenders(name, seen=None):
    """name and every project it borrows from, and theirs"""
    seen = seen if seen is not None else set()
    if name not in seen:
        seen.add(name)
        with open(os.path.join(SRC, name, "project.toml"), "rb") as f:
            for other in tomllib.load(f).get("uses", {}).values():
                _lenders(other, seen)
    return seen


def _copy(tmp, name, edit):
    """projects/ with NAME (its project text through `edit`), its lenders and the steps"""
    root = tmp / name
    for p in _lenders(NAME):
        (root / p).mkdir(parents=True)
        text = open(os.path.join(SRC, p, "project.toml")).read()
        (root / p / "project.toml").write_text(edit(text) if p == NAME else text)
    for f in ("steps.toml", "kinds.toml"):
        shutil.copy(os.path.join(SRC, f), root / f)
    return root


def _prints(monkeypatch, root, steps):
    import build
    monkeypatch.setenv("PE_PROJECTS", str(root))
    return tuple(build.fingerprint(NAME, s, k, NAME) for s, k in steps)


@pytest.fixture(autouse=True)
def _need_project():
    if not os.path.exists(os.path.join(SRC, NAME)):
        pytest.skip(f"no {NAME} project")


def test_the_take_checks_make_nothing_stale(tmp_path, monkeypatch):
    steps = [("analyze", "source"), ("felt", "part"), ("export", "project")]
    plain = _prints(monkeypatch, _copy(tmp_path, "plain", lambda t: t), steps)
    moved = _prints(monkeypatch, _copy(tmp_path, "moved", lambda t: t.replace(
        "[menu]\n", "[take_checks]\nrise = 6.0\n\n[menu]\n", 1)), steps)
    assert plain == moved


def test_skip_files_make_the_analysis_stale(tmp_path, monkeypatch):
    steps = [("analyze", "source")]
    plain = _prints(monkeypatch, _copy(tmp_path, "plain", lambda t: t), steps)
    # one more file left out (the project may leave out some already)
    left = _prints(monkeypatch, _copy(tmp_path, "left", lambda t: t.replace("skip_files = [", 'skip_files = ["C4v9.wav", ', 1)
                                      if "skip_files = [" in t else t.replace(
        "[[part.source]]\n", '[[part.source]]\nskip_files = ["C4v9.wav"]\n', 1)), steps)
    assert plain != left
