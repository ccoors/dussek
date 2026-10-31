"""analysis/project.py: the index of projects, parts and sources, and their files."""
import json
import os

import pytest

import project as P


def write(root, name, text):
    os.makedirs(os.path.join(root, name), exist_ok=True)
    with open(os.path.join(root, name, "project.toml"), "w") as f:
        f.write(text)


@pytest.fixture
def projects(tmp_path, monkeypatch):
    root = str(tmp_path / "projects")
    write(root, "alpha", """
[project]
family = "struck"
body = "grand"
[uses]
dampers = "beta"
[strings]
design = "grand"
[[part]]
id = "alpha"
[[part.source]]
id = "alpha"
root = "set-a"
dynamics = { pp = "v1", ff = "v4" }
""")
    write(root, "beta", """
[project]
family = "bars"
[[part]]
id = "beta"
[[part.source]]
id = "beta_one"
root = "set-b"
[[part.source]]
id = "beta_two"
root = "set-b"
""")
    monkeypatch.setenv("PE_PROJECTS", root)
    P._index.cache_clear()
    yield root
    P._index.cache_clear()


def test_ids_are_global(projects):
    assert P.project_of("alpha") == "alpha" and P.project_of("beta_two") == "beta"
    assert P.part_ids("beta") == ["beta"] and P.source_ids("beta") == ["beta_one", "beta_two"]
    assert P.part_of("beta_one") == "beta" and P.main_part("beta") == "beta"
    with pytest.raises(KeyError):
        P.project_of("gamma")


def test_an_id_used_by_two_projects_is_refused(projects):
    write(projects, "gamma", '[project]\nfamily = "bars"\n[[part]]\nid = "beta"\n')
    P._index.cache_clear()
    with pytest.raises(ValueError, match="beta"):
        P.index()


def test_sections_sources_and_uses(projects):
    assert P.source("alpha")["dynamics"] == {"pp": "v1", "ff": "v4"}
    assert list(P.source("alpha")["dynamics"]) == ["pp", "ff"]          # the file's order
    assert P.section("alpha", "strings") == {"design": "grand"}
    assert P.section("beta", "strings") == {}
    assert P.uses("alpha", "dampers") == "beta" and P.uses("alpha", "felt") is None
    assert P.provider("alpha", "dampers") == "beta" and P.provider("beta", "dampers") == "beta"
    assert P.data_root("beta_two") == os.path.join(P.REPO, "data", "set-b")


def test_fit_documents(projects):
    assert not P.has_fit("alpha") and P.load_fit("alpha") == {}
    P.update_fit("alpha", {"a": 1, "b": [1.5]})
    P.update_fit("alpha", {"c": 2}, drop=("a",))
    path = P.fit_path("alpha")
    assert path == os.path.join(projects, "alpha", "fits", "alpha.json")
    assert json.load(open(path)) == {"b": [1.5], "c": 2}
    assert open(path).read() == json.dumps({"b": [1.5], "c": 2}, indent=1)   # as the old dumps
    P.write_fit("beta", {"keys": []})
    assert P.load_fit("beta") == {"keys": []}


def test_caches(projects):
    assert P.cache_path("beta_one", "analysis") == os.path.join(projects, "beta", "cache", "beta_one", "analysis.json")
    assert not P.has_cache("beta_one", "analysis")
    P.write_cache("beta_one", "analysis", {"samples": []}, indent=1)
    assert P.load_cache("beta_one", "analysis") == {"samples": []}
    assert P.cache_bytes("beta_one", "analysis") == json.dumps({"samples": []}, indent=1).encode()


def test_writes_are_atomic(projects):
    P.update_fit("alpha", {"a": 1})

    def broken(obj, f, **kw):
        f.write("{")
        raise RuntimeError("disk full")
    with pytest.MonkeyPatch.context() as mp:
        mp.setattr(P.json, "dump", broken)
        with pytest.raises(RuntimeError):
            P.update_fit("alpha", {"a": 2})
    assert P.load_fit("alpha") == {"a": 1}
    assert [f for f in os.listdir(os.path.dirname(P.fit_path("alpha"))) if ".tmp" in f] == []


def test_a_part_overrides_a_section(projects):
    write(projects, "gamma", """
[project]
family = "plucked"
[strings]
design = "harpsichord_8"
board_seed = 7
[[part]]
id = "g8"
[[part.source]]
id = "g8"
root = "x"
[[part]]
id = "g4"
strings = { design = "harpsichord_4" }
[[part.source]]
id = "g4"
root = "x"
""")
    P._index.cache_clear()
    assert P.section("g8", "strings") == {"design": "harpsichord_8", "board_seed": 7}
    assert P.section("g4", "strings") == {"design": "harpsichord_4", "board_seed": 7}
    assert P.section("gamma", "strings") == {"design": "harpsichord_8", "board_seed": 7}
