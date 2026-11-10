"""A brass export: the accepted tables in 32 bits, an ordinary instrument directory (proto/export_brass.py)."""
import json
import os
import subprocess
import sys

import numpy as np
import pytest

import project as P

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(P.REPO, "proto", "export_brass.py")
BRASS = os.path.join(P.REPO, "proto", "brass")


@pytest.fixture(scope="module")
def horn(tmp_path_factory):
    if not os.path.exists(os.path.join(P.project_dir("horn"), "cache", "horn", "tables_open.pkl")):
        pytest.skip("the horn's tables are not here")
    out = str(tmp_path_factory.mktemp("export") / "horn")
    subprocess.run([sys.executable, SCRIPT, "--project", "horn", "--out", out], check=True, capture_output=True, text=True)
    return out


def read(out, ref):
    with open(os.path.join(out, "data.bin"), "rb") as f:
        f.seek(ref["o"])
        return np.fromfile(f, "<f8" if ref["t"] == "f64" else "<f4", ref["n"])


def test_an_export_holds_the_accepted_tables(horn, tmp_path):
    """Every filter and mode as rt_tables has them, within f32's rounding for the filters."""
    dump = tmp_path / "tables.npz"
    code = ("import numpy as np, rt_tables; t = rt_tables.load('open')['notes']; "
            "np.savez(%r, **{f'{k}_{d}_h': n['h'][d] for k, n in t.items() for d in n['h']}, "
            "**{f'{k}_{d}_sp': n['modes'][d][0] for k, n in t.items() for d in n['modes']}, "
            "**{f'{k}_{d}_rc': n['modes'][d][1] for k, n in t.items() for d in n['modes']}, "
            "**{f'{k}_r': n['r'] for k, n in t.items()})" % str(dump))
    subprocess.run([sys.executable, "-c", code], cwd=BRASS, env={**os.environ, "BRASS_PROJECT": "horn"}, check=True, capture_output=True)
    want = np.load(dump)
    doc = json.load(open(os.path.join(horn, "instrument.json")))
    notes = doc["brass"]["mutes"]["open"]["notes"]
    assert len(notes) == len(doc["brass"]["table"]) == 40
    for key, n in notes.items():
        assert np.allclose(read(horn, n["r"]), want[f"{key}_r"], rtol=1e-6, atol=1e-5 * np.abs(want[f"{key}_r"]).max())
        for d in ("pp", "mf", "ff"):
            h = want[f"{key}_{d}_h"]
            assert np.abs(read(horn, n["h"][d]) - h).max() <= 1e-6 * np.abs(h).max()
            m = n["modes"][d]
            assert np.array_equal(read(horn, m["sp"]["re"]) + 1j * read(horn, m["sp"]["im"]), want[f"{key}_{d}_sp"])
            assert np.array_equal(read(horn, m["rc"]["re"]) + 1j * read(horn, m["rc"]["im"]), want[f"{key}_{d}_rc"])


def test_an_export_names_no_recording(horn):
    """Its license is written, no audio goes with it."""
    names = os.listdir(horn)
    assert "LICENSE.md" in names
    assert not [n for n in names if n.lower().endswith((".wav", ".aif", ".aiff", ".flac", ".mp3", ".ogg"))]


def test_an_export_has_no_notes(horn):
    """The brass block is the sound: no silent keys (the engine reads an instrument with a brass block and no notes)."""
    doc = json.load(open(os.path.join(horn, "instrument.json")))
    assert doc["format"] == 5 and doc["body"] == "bell"
    assert "notes" not in doc
    assert doc["menu"]["group"] == "Brass" and doc["menu"]["experimental"] is True


def test_the_playable_keys_end_at_the_table_s_top():
    """From `below` under the table (6 unless the project says) to `above` over it (none unless it says): over
    the highest note the lips are at their limit, and the trumpet's two keys there barely sounded."""
    from importlib import util
    spec = util.spec_from_file_location("export_brass", SCRIPT)
    m = util.module_from_spec(spec)
    os.environ.setdefault("BRASS_PROJECT", "horn")
    spec.loader.exec_module(m)
    assert m.playable([38, 77], {}) == [32, 77]
    assert m.playable([28, 65], {"below": 0, "above": 2}) == [28, 67]


def test_an_export_carries_the_bell_as_held(horn):
    """The project's `[pose]` goes into the brass block as the engine reads it: degrees and meters."""
    doc = json.load(open(os.path.join(horn, "instrument.json")))
    want = P.config("horn")["pose"]
    assert doc["brass"]["pose"] == {"azimuth": want["azimuth"], "elevation": want["elevation"], "ears": want["ears"]}


def test_a_pose_is_checked():
    """A bell's direction in degrees and the ears' place in meters; an unknown key or a missing one is an error."""
    from importlib import util
    spec = util.spec_from_file_location("export_brass", SCRIPT)
    m = util.module_from_spec(spec)
    os.environ.setdefault("BRASS_PROJECT", "horn")
    spec.loader.exec_module(m)
    assert m.pose({}) is None
    assert m.pose({"azimuth": 60, "elevation": 75, "ears": [-0.2, 0, -0.4]}) == {"azimuth": 60.0, "elevation": 75.0, "ears": [-0.2, 0.0, -0.4]}
    for bad in ({"azimuth": 60, "elevation": 75}, {"azimuth": 60, "elevation": 75, "ears": [0, 0]},
                {"azimuth": 60, "elevation": 95, "ears": [0, 0, 0]}, {"azimuth": 0, "elevation": 0, "ears": [0, 0, 0], "x": 1}):
        with pytest.raises(ValueError):
            m.pose(bad)
