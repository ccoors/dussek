"""The exports measure each instrument's level with the engine's `level` tool and keep it in
instrument.json (`level`), beside the tie to their data.bin (`data`)."""
import hashlib
import json
import subprocess
import types

import export


def test_measure_level_runs_the_engine_tool(monkeypatch):
    calls = []

    def run(cmd, check, stdout, text):
        calls.append((cmd, check))
        return types.SimpleNamespace(stdout='{"ff_db": -15.5, "keys": [48, 60, 72]}\n')

    monkeypatch.setattr(subprocess, "run", run)
    assert export.measure_level("/x/instruments/foo") == {"ff_db": -15.5, "keys": [48, 60, 72]}
    assert len(calls) == 1 and calls[0][1] is True
    cmd = calls[0][0]
    assert cmd[-1] == "/x/instruments/foo" and cmd[cmd.index("--bin") + 1] == "level"


def test_the_level_and_the_data_tie_live_in_instrument_json(tmp_path):
    blob = types.SimpleNamespace(parts=[b"\x00\x01", b"\x02"])
    doc = {"format": 5}
    export.write_instrument(str(tmp_path), str(tmp_path / "prev"), doc, blob)
    export.store_level(str(tmp_path), doc, {"ff_db": -15.5})
    written = json.loads((tmp_path / "instrument.json").read_text())
    assert written["data"] == {"bytes": 3, "sha256": hashlib.sha256(b"\x00\x01\x02").hexdigest()}
    assert list(written)[-1] == "level" and written["level"] == {"ff_db": -15.5}
    assert not (tmp_path / "level.json").exists()
    # the same export again: its level aside, nothing changed, so nothing is kept as the previous one
    export.write_instrument(str(tmp_path), str(tmp_path / "prev"), {"format": 5}, blob)
    assert not (tmp_path / "prev").exists()
