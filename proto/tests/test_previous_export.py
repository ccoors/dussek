"""An export keeps its predecessor (the Compare page's Original): differing output moves the old
files to the snapshot, identical output leaves it, a first export makes none."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, os.path.join(HERE, "..", "..", "analysis"))

import export  # noqa: E402


class Blob:
    def __init__(self, data):
        self.parts = [data]


def read(d):
    return open(os.path.join(d, "instrument.json")).read(), open(os.path.join(d, "data.bin"), "rb").read()


def written(doc, data):
    """the files an export of `doc` and `data` leaves: instrument.json names its data.bin (`data`)"""
    return json.dumps({**doc, "data": export.data_tie(data)}), data


def test_a_first_export_makes_no_snapshot(tmp_path):
    out, snap = tmp_path / "out", tmp_path / "snap"
    export.write_instrument(str(out), str(snap), {"a": 1}, Blob(b"x"))
    assert read(out) == written({"a": 1}, b"x")
    assert not snap.exists()


def test_differing_output_moves_the_old_to_the_snapshot(tmp_path):
    out, snap = tmp_path / "out", tmp_path / "snap"
    export.write_instrument(str(out), str(snap), {"a": 1}, Blob(b"x"))
    export.write_instrument(str(out), str(snap), {"a": 2}, Blob(b"y"))
    assert read(out) == written({"a": 2}, b"y")
    assert read(snap) == written({"a": 1}, b"x")


def test_identical_output_keeps_the_snapshot(tmp_path):
    out, snap = tmp_path / "out", tmp_path / "snap"
    export.write_instrument(str(out), str(snap), {"a": 1}, Blob(b"x"))
    export.write_instrument(str(out), str(snap), {"a": 2}, Blob(b"y"))
    export.write_instrument(str(out), str(snap), {"a": 2}, Blob(b"y"))
    assert read(snap) == written({"a": 1}, b"x"), "a rebuild that changes nothing keeps the real before"
