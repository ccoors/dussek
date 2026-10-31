"""Snapshots: an instrument's export at a git ref, beside the working copy in the plugin's Snapshots
group, its name and profile marked with the ref (design 2026-10-21-traits-plan, the rail)."""
import json
import os
import shutil
import subprocess

import pytest

import snapshot as SN

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def _git(repo, *args):
    # no signing, whatever the user's configuration asks of commits and tags
    subprocess.run(["git", "-C", repo, "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", *args],
                   check=True, capture_output=True)


@pytest.fixture
def repo(tmp_path):
    """A repository with two exports, committed and tagged t/1."""
    r = str(tmp_path / "repo")
    for name, menu in (("harp", "Harp A"), ("bells", "Bells A")):
        d = os.path.join(r, "instruments", name)
        os.makedirs(d)
        with open(os.path.join(d, "instrument.json"), "w") as fh:
            json.dump({"format": 5, "profile": name, "menu": {"name": menu, "group": "Harps"}, "notes": []}, fh)
        with open(os.path.join(d, "data.bin"), "wb") as fh:
            fh.write(bytes(range(256)) * 4)
    _git(r, "init", "-q")
    _git(r, "-c", "user.name=t", "-c", "user.email=t@t", "add", ".")
    _git(r, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "exports")
    _git(r, "tag", "t/1")
    return r


def ok(_):
    return None


def test_a_snapshot_is_marked_with_its_ref(repo, tmp_path):
    dest = str(tmp_path / "snap")
    assert SN.snapshot("t/1", ["harp"], dest, repo, check=ok) == [os.path.join(dest, "harp@t-1")]
    j = json.load(open(os.path.join(dest, "harp@t-1", "instrument.json")))
    assert j["menu"]["name"] == "Harp A @ t-1" and j["menu"]["group"] == "Snapshots"
    sha = subprocess.run(["git", "-C", repo, "rev-parse", "t/1^{commit}"], capture_output=True, text=True).stdout.strip()
    assert j["profile"] == f"harp@t-1#{sha[:9]}"
    with open(os.path.join(dest, "harp@t-1", "data.bin"), "rb") as fh:
        assert fh.read() == bytes(range(256)) * 4


def test_names_default_to_every_instrument_at_the_ref(repo, tmp_path):
    dest = str(tmp_path / "snap")
    got = SN.snapshot("t/1", None, dest, repo, check=ok)
    assert sorted(os.path.basename(d) for d in got) == ["bells@t-1", "harp@t-1"]


def test_an_unknown_ref_writes_nothing(repo, tmp_path):
    dest = str(tmp_path / "snap")
    os.makedirs(dest)
    with pytest.raises(SN.SnapshotError, match="nope"):
        SN.snapshot("nope", ["harp"], dest, repo, check=ok)
    assert os.listdir(dest) == []


def test_an_unknown_name_writes_nothing(repo, tmp_path):
    dest = str(tmp_path / "snap")
    os.makedirs(dest)
    with pytest.raises(SN.SnapshotError, match="nonesuch"):
        SN.snapshot("t/1", ["nonesuch"], dest, repo, check=ok)
    assert os.listdir(dest) == []


def test_what_the_engine_refuses_is_not_kept(repo, tmp_path):
    dest = str(tmp_path / "snap")
    with pytest.raises(SN.SnapshotError, match="format 9 unsupported"):
        SN.snapshot("t/1", ["harp"], dest, repo, check=lambda d: "format 9 unsupported")
    assert os.listdir(dest) == []


def test_a_second_snapshot_replaces_the_first(repo, tmp_path):
    dest = str(tmp_path / "snap")
    os.makedirs(os.path.join(dest, "harp@t-1"))
    open(os.path.join(dest, "harp@t-1", "stale.txt"), "w").close()
    SN.snapshot("t/1", ["harp"], dest, repo, check=ok)
    assert sorted(os.listdir(os.path.join(dest, "harp@t-1"))) == ["data.bin", "instrument.json"]


def test_remove_takes_only_that_ref(repo, tmp_path):
    dest = str(tmp_path / "snap")
    for d in ("harp@t-1", "harp@main", "bells@t-1"):
        os.makedirs(os.path.join(dest, d))
    assert sorted(os.path.basename(d) for d in SN.remove("t/1", dest)) == ["bells@t-1", "harp@t-1"]
    assert os.listdir(dest) == ["harp@main"]


def test_snapshots_live_in_the_data_directory(monkeypatch):
    monkeypatch.setenv("XDG_DATA_HOME", "/x/data")
    assert SN.snapshots_dir() == "/x/data/dussek/snapshots"
    monkeypatch.delenv("XDG_DATA_HOME")
    monkeypatch.setenv("HOME", "/x/home")
    assert SN.snapshots_dir() == "/x/home/.local/share/dussek/snapshots"


@pytest.mark.skipif(not os.path.exists(os.path.join(REPO, "engine", "target", "release", "render")), reason="no render binary")
def test_the_engine_loads_a_committed_instrument(tmp_path):
    src = os.path.join(REPO, "instruments", "grand_a")
    if not os.path.exists(os.path.join(src, "instrument.json")):
        pytest.skip("no export")
    assert SN.engine_loads(src) is None
    bad = tmp_path / "bad"
    shutil.copytree(src, bad)
    (bad / "instrument.json").write_text("{}")
    assert SN.engine_loads(str(bad))


def test_remove_with_names_takes_only_those(repo, tmp_path):
    dest = str(tmp_path / "snap")
    for d in ("harp@t-1", "bells@t-1"):
        os.makedirs(os.path.join(dest, d))
    assert [os.path.basename(d) for d in SN.remove("t/1", dest, ["harp"])] == ["harp@t-1"]
    assert os.listdir(dest) == ["bells@t-1"]


def test_a_refusal_keeps_the_whole_run_unwritten(repo, tmp_path):
    # one instrument refused: the others are not written either, an older snapshot stays as it was
    dest = str(tmp_path / "snap")
    os.makedirs(os.path.join(dest, "bells@t-1"))
    open(os.path.join(dest, "bells@t-1", "old.txt"), "w").close()
    with pytest.raises(SN.SnapshotError, match="harp"):
        SN.snapshot("t/1", None, dest, repo, check=lambda d: "refused" if "harp" in d else None)
    assert os.listdir(dest) == ["bells@t-1"] and os.listdir(os.path.join(dest, "bells@t-1")) == ["old.txt"]


def test_a_broken_export_is_refused_with_the_engines_message(repo, tmp_path):
    d = os.path.join(repo, "instruments", "broken")
    os.makedirs(d)
    with open(os.path.join(d, "instrument.json"), "w") as fh:
        fh.write("{oops")
    open(os.path.join(d, "data.bin"), "wb").close()
    _git(repo, "-c", "user.name=t", "-c", "user.email=t@t", "add", ".")
    _git(repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "broken")
    dest = str(tmp_path / "snap")
    with pytest.raises(SN.SnapshotError, match="EOF while parsing"):
        SN.snapshot("HEAD", ["broken"], dest, repo, check=lambda d: "EOF while parsing")
    assert os.listdir(dest) == []


def test_an_older_refs_level_json_moves_into_instrument_json(repo, tmp_path):
    # exports before step 214 kept their level in level.json, which the engine no longer reads
    d = os.path.join(repo, "instruments", "harp")
    with open(os.path.join(d, "level.json"), "w") as fh:
        json.dump({"ff_db": -15.5, "keys": [48, 60, 72]}, fh)
    _git(repo, "-c", "user.name=t", "-c", "user.email=t@t", "add", ".")
    _git(repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "level")
    _git(repo, "tag", "t/2")
    dest = str(tmp_path / "snap")
    SN.snapshot("t/2", ["harp"], dest, repo, check=ok)
    j = json.load(open(os.path.join(dest, "harp@t-2", "instrument.json")))
    assert j["level"] == {"ff_db": -15.5, "keys": [48, 60, 72]}
    assert not os.path.exists(os.path.join(dest, "harp@t-2", "level.json"))
