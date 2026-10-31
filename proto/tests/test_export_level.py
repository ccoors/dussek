"""The exports measure each instrument's ff level with the engine's `level` tool (level.json)."""
import subprocess

import export


def test_measure_level_runs_the_engine_tool(monkeypatch):
    calls = []
    monkeypatch.setattr(subprocess, "run", lambda cmd, check: calls.append((cmd, check)))
    export.measure_level("/x/instruments/foo")
    assert len(calls) == 1 and calls[0][1] is True
    cmd = calls[0][0]
    assert cmd[-1] == "/x/instruments/foo" and cmd[cmd.index("--bin") + 1] == "level"
