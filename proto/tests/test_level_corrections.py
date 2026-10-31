"""A source's level corrections ([part.source.level_corrections]): checked, interpolated to every
key and held beyond the recorded ones, one triple per mallet in source order; each part's own."""
import os
import re
import shutil
import sys

import pytest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, os.path.join(HERE, "..", "..", "analysis"))

import level_corrections as LC  # noqa: E402

T = {"keys": [60, 64, 72], "pp": [2.0, 4.0, 0.0], "mf": [0.0, 1.0, -1.0]}


def test_the_table_is_interpolated_and_held():
    assert LC.at(T, 60) == [2.0, 0.0, 0.0]
    assert LC.at(T, 62) == [3.0, 0.5, 0.0]          # halfway between 60 and 64; ff left out: 0
    assert LC.at(T, 40) == [2.0, 0.0, 0.0]          # below the first key: held
    assert LC.at(T, 100) == [0.0, -1.0, 0.0]        # above the last: held


def test_only_some_mallets_have_a_table():
    assert LC.note_levels([None, T, None], 64) == [[0.0, 0.0, 0.0], [4.0, 1.0, 0.0], [0.0, 0.0, 0.0]]
    assert LC.note_levels([None, None], 64) is None


@pytest.mark.parametrize("bad, what", [
    ({"keys": [64, 60], "pp": [0.0, 0.0]}, "ascending"),
    ({"keys": [60, 64], "pp": [0.0]}, "pp"),
    ({"keys": [60, 64], "mf": [0.0, "x"]}, "mf"),
    ({"pp": [0.0]}, "keys"),
])
def test_a_bad_table_names_its_source(bad, what):
    with pytest.raises(ValueError) as e:
        LC.check(bad, "marimba_yarn")
    assert "marimba_yarn" in str(e.value) and what in str(e.value)


def test_each_register_takes_its_parts_table(tmp_path, monkeypatch):
    """A copy of the two-register project with a table on its 4′ source only."""
    src = os.path.join(HERE, "..", "..", "projects", "flemish", "project.toml")
    if not os.path.exists(src):
        pytest.skip("no flemish project")
    d = tmp_path / "flemish"
    d.mkdir()
    text = open(src).read()
    # the 4′ source's table, right after its map (the map is the last table of that source)
    i = text.index('[[part.source]]\nid = "flemish4"')
    j = text.find("\n[[", i)
    j = len(text) if j < 0 else j
    text = text[:j] + '\n\n[part.source.level_corrections]\nkeys = [40, 60]\nmf = [3.0, 3.0]\n' + text[j:]
    (d / "project.toml").write_text(text)
    shutil.copy(os.path.join(HERE, "..", "..", "projects", "steps.toml"), tmp_path / "steps.toml")
    monkeypatch.setenv("PE_PROJECTS", str(tmp_path))
    assert LC.tables("flemish8", ("main",)) == [None]
    t4 = LC.tables("flemish4", ("main",))
    assert t4 == [{"keys": [40, 60], "mf": [3.0, 3.0]}]
    assert LC.note_levels(t4, 50) == [[0.0, 3.0, 0.0]]


def _copy(tmp, table, groups=False):
    """projects/ with glockenspiel_b and the steps, its source given a table or not (and a
    level_groups key or not)"""
    root = tmp / (("with" if table else "without") + ("-groups" if groups else ""))
    (root / "glockenspiel_b").mkdir(parents=True)
    src = os.path.join(HERE, "..", "..", "projects")
    shutil.copy(os.path.join(src, "steps.toml"), root / "steps.toml")
    shutil.copy(os.path.join(src, "kinds.toml"), root / "kinds.toml")
    text = open(os.path.join(src, "glockenspiel_b", "project.toml")).read()
    if table:
        text += '\n[part.source.level_corrections]\nkeys = [79, 108]\nmf = [6.0, 6.0]\n'
    if groups:
        text = text.replace("[[part.source]]\n", '[[part.source]]\nlevel_groups = "octave"\n', 1)
    (root / "glockenspiel_b" / "project.toml").write_text(text)
    return root


def test_a_level_correction_makes_only_the_export_stale(tmp_path, monkeypatch):
    """the fits do not read the corrections: a table changes the export's fingerprint only"""
    if not os.path.exists(os.path.join(HERE, "..", "..", "projects", "glockenspiel_b")):
        pytest.skip("no glockenspiel_b project")
    import build
    prints = {}
    for table in (False, True):
        monkeypatch.setenv("PE_PROJECTS", str(_copy(tmp_path, table)))
        prints[table] = (build.fingerprint("glockenspiel_b", "bars", "part", "glockenspiel_b"),
                         build.fingerprint("glockenspiel_b", "export_bars", "project", "glockenspiel_b"))
    assert prints[False][0] == prints[True][0], "the bars fit must not go stale"
    assert prints[False][1] != prints[True][1], "the export must"


def test_the_level_groups_make_nothing_stale(tmp_path, monkeypatch):
    """only the editor reads a source's level_groups (how its recordings' levels group): neither
    the fits nor the export go stale"""
    if not os.path.exists(os.path.join(HERE, "..", "..", "projects", "glockenspiel_b")):
        pytest.skip("no glockenspiel_b project")
    import build
    prints = {}
    for groups in (False, True):
        monkeypatch.setenv("PE_PROJECTS", str(_copy(tmp_path, False, groups)))
        prints[groups] = (build.fingerprint("glockenspiel_b", "bars", "part", "glockenspiel_b"),
                          build.fingerprint("glockenspiel_b", "export_bars", "project", "glockenspiel_b"))
    assert prints[False] == prints[True]


def test_a_borrowed_parts_correction_makes_the_borrowing_export_stale(tmp_path, monkeypatch):
    """registers_fixture exports Salamander's part ([export] main): a table on Salamander's source changes
    registers_fixture's export, so its fingerprint must change too"""
    src = os.path.join(HERE, "..", "..", "projects")
    if not all(os.path.exists(os.path.join(src, p)) for p in ("grand_a", "registers_fixture", "grand_e", "grand_b")):
        pytest.skip("no grand_a projects")
    import build
    prints = {}
    for table in (False, True):
        root = tmp_path / ("with" if table else "without")
        # the two and their lenders (Grand A's dampers are Grand E's, Grand E's levels Grand B's)
        for p in ("grand_a", "registers_fixture", "grand_e", "grand_b"):
            (root / p).mkdir(parents=True)
            text = open(os.path.join(src, p, "project.toml")).read()
            # without Salamander's own table, if it has one (each run then differs by ours alone)
            text = re.sub(r"\n(#[^\n]*\n)?\[part\.source\.level_corrections\]\n([a-z][^\n]*\n?)*", "\n", text)
            if table and p == "grand_a":
                i = text.index("[[part.source]]")
                j = text.find("\n[[", i + 1)
                j = len(text) if j < 0 else j
                text = text[:j] + '\n\n[part.source.level_corrections]\nkeys = [60, 72]\nmf = [3.0, 3.0]\n' + text[j:]
            (root / p / "project.toml").write_text(text)
        for f in ("steps.toml", "kinds.toml"):
            shutil.copy(os.path.join(src, f), root / f)
        monkeypatch.setenv("PE_PROJECTS", str(root))
        prints[table] = build.fingerprint("registers_fixture", "export", "project", "registers_fixture")
    assert prints[False] != prints[True]


def test_a_wildcard_does_not_cover_the_corrections():
    """project.* and <role>:* are fingerprinted without the corrections: a step reading them must
    declare them by name"""
    import project as P
    assert not P._declared(["project.level_corrections"], {"project.*"})
    assert not P._declared(["uses.main:project.level_corrections"], {"uses.main:*", "uses.main:project.*"})
    assert P._declared(["project.level_corrections"], {"project.level_corrections"})
    assert P._declared(["uses.main:project.level_corrections"], {"uses.main:project.level_corrections"})
