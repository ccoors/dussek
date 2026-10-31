"""A borrower's steps read named keys of the project they borrow from (projects/steps.toml), not
all of it: a refit of the lender's coupling leaves the borrowers' pedal and export fresh, a
change to the pedal fit they read does not."""
import json
import os
import shutil
import tomllib

import pytest

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "..", "projects")
LENDER, BORROWER = "grand_a", "upright_a"     # Upright A borrows Grand A's release and pedal


@pytest.fixture(autouse=True)
def _need_projects():
    for n in (LENDER, BORROWER):
        if not os.path.exists(os.path.join(SRC, n, "fits")):
            pytest.skip(f"no {n} project")


def _others():
    """The other projects the borrower borrows from (Grand E's dampers)."""
    with open(os.path.join(SRC, BORROWER, "project.toml"), "rb") as fh:
        uses = tomllib.load(fh).get("uses", {})
    return sorted(set(uses.values()) - {LENDER, BORROWER})


def _copy(tmp, name, fit_edit):
    """projects/ with the lender, the borrower and what else it borrows from (project files and
    fits), the lender's fit through `fit_edit`"""
    root = tmp / name
    for f in ("steps.toml", "kinds.toml"):
        root.mkdir(parents=True, exist_ok=True)
        shutil.copy(os.path.join(SRC, f), root / f)
    for n in (LENDER, BORROWER, *_others()):
        shutil.copy2(os.path.join(SRC, n, "project.toml"), (root / n).mkdir() or root / n / "project.toml")
        shutil.copytree(os.path.join(SRC, n, "fits"), root / n / "fits")
    path = root / LENDER / "fits" / f"{LENDER}.json"
    fit = json.loads(path.read_text())
    fit_edit(fit)
    path.write_text(json.dumps(fit))
    return root


def _prints(monkeypatch, root):
    import build
    monkeypatch.setenv("PE_PROJECTS", str(root))
    return {s: build.fingerprint(BORROWER, s, k, i) for s, k, i in
            (("pedal", "part", BORROWER), ("export", "project", BORROWER))}


def test_a_lenders_coupling_leaves_the_borrowers_fresh(tmp_path, monkeypatch):
    plain = _prints(monkeypatch, _copy(tmp_path, "plain", lambda f: None))
    refit = _prints(monkeypatch, _copy(tmp_path, "refit", lambda f: f["coupling"].update(reactive=2.7)))
    assert plain == refit


def test_a_lenders_pedal_fit_makes_the_borrowers_pedal_stale(tmp_path, monkeypatch):
    plain = _prints(monkeypatch, _copy(tmp_path, "plain", lambda f: None))
    moved = _prints(monkeypatch, _copy(tmp_path, "moved", lambda f: f["pedal"].update(amp=1.0)))
    assert plain["pedal"] != moved["pedal"]
