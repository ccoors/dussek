"""The checks of analysis/project.py under a step (PE_STEP), and the record mode (PE_RECORD)."""
import json
import os
import pickle

import pytest

import project as P
from test_project import projects, write  # noqa: F401  (the fixture)

STEPS = """
[step.felt]
scope = "part"
run = ["true"]
reads = ["project.strings", "cache.analysis", "uses.dampers:cache.damper"]
seeds = ["fit.coupling"]
owns = ["fit.felt_p"]

[step.register]
scope = "part"
run = ["true"]
reads = ["fit.*"]
amends = ["fit.soundboard"]

[chain]
struck = ["felt", "register"]
"""


@pytest.fixture
def steps(projects, monkeypatch):  # noqa: F811
    with open(os.path.join(projects, "steps.toml"), "w") as f:
        f.write(STEPS)
    P.registry.cache_clear()
    P._rules_for.cache_clear()
    monkeypatch.setenv("PE_PROJECT", "alpha")
    yield projects
    P.registry.cache_clear()
    P._rules_for.cache_clear()


def test_declared_reads_and_writes_pass(steps, monkeypatch):
    P.write_cache("alpha", "analysis", {"samples": []})
    P.write_cache("beta", "damper", {"rows": []})
    monkeypatch.setenv("PE_STEP", "felt")
    assert P.section("alpha", "strings") == {"design": "grand"}
    assert P.load_cache("alpha", "analysis") == {"samples": []}
    assert P.load_cache("beta", "damper") == {"rows": []}         # borrowed: uses.dampers
    P.update_fit("alpha", {"felt_p": 2.3})
    fit = P.load_fit("alpha")
    assert fit["felt_p"] == 2.3                                   # its own output read back
    assert fit.get("coupling") is None                            # a seed


def test_an_undeclared_read_fails(steps, monkeypatch):
    P.write_cache("beta", "analysis", {"samples": []})
    monkeypatch.setenv("PE_STEP", "felt")
    with pytest.raises(P.Undeclared, match="project.uses"):
        P.uses("alpha", "felt")
    with pytest.raises(P.Undeclared, match="fit.knock"):
        P.load_fit("alpha")["knock"]
    with pytest.raises(P.Undeclared, match="cache.analysis"):
        P.load_cache("beta", "analysis")                          # beta's, not borrowed as dampers' analysis
    with pytest.raises(P.Undeclared):
        list(P.load_fit("alpha").items())                         # every key: needs fit.*


def test_an_undeclared_write_fails(steps, monkeypatch):
    monkeypatch.setenv("PE_STEP", "felt")
    with pytest.raises(P.Undeclared, match="fit.knock"):
        P.update_fit("alpha", {"knock": {}})
    with pytest.raises(P.Undeclared):
        P.write_cache("beta", "damper", {})                       # another project's
    assert not P.has_fit("alpha")


def test_a_second_run_and_amendments(steps, monkeypatch):
    P.update_fit("alpha", {"soundboard": {"correction": {}}, "felt_p": 2.0})
    monkeypatch.setenv("PE_STEP", "register#2")
    doc = P.load_fit("alpha")
    assert dict(doc.items())["felt_p"] == 2.0                     # fit.* reads all
    P.update_fit("alpha", {"soundboard": {"correction": {"g": 1}}})
    with pytest.raises(P.Undeclared):
        P.update_fit("alpha", {"felt_p": 1.0})


def test_without_a_step_nothing_is_checked(steps, monkeypatch):
    monkeypatch.delenv("PE_STEP", raising=False)
    P.update_fit("alpha", {"anything": 1})
    assert type(P.load_fit("alpha")) is dict


def test_a_checked_document_pickles(steps, monkeypatch):
    P.update_fit("alpha", {"felt_p": 2.0})
    monkeypatch.setenv("PE_STEP", "felt")
    doc = pickle.loads(pickle.dumps(P.load_fit("alpha")))
    assert isinstance(doc, P.FitDoc) and doc["felt_p"] == 2.0


def test_record_mode_logs_and_checks_nothing(steps, monkeypatch, tmp_path):
    log = tmp_path / "record.jsonl"
    monkeypatch.setenv("PE_STEP", "felt")
    monkeypatch.setenv("PE_RECORD", str(log))
    P.uses("alpha", "felt")
    P.update_fit("alpha", {"knock": {}})
    P.load_fit("alpha")["knock"]
    rows = [json.loads(line) for line in open(log)]
    assert {(r["op"], r["what"]) for r in rows} == {("read", "project.uses"), ("write", "fit.knock"), ("read", "fit.knock")}
    assert all(r["step"] == "felt" and r["project"] == "alpha" for r in rows)
    P.write_cache("beta", "damper", {})
    last = json.loads(open(log).readlines()[-1])
    assert last["what"] == "cache.damper" and last["as"] == ["uses.dampers:cache.damper"]


def test_a_memo_is_written_by_whichever_step_declares_it(steps, monkeypatch):
    with open(os.path.join(steps, "steps.toml"), "a") as f:
        f.write('\n[step.pedal]\nscope = "part"\nrun = ["true"]\nmemos = ["cache.release_peaks"]\n')
    P.registry.cache_clear()
    P._rules_for.cache_clear()
    monkeypatch.setenv("PE_STEP", "pedal")
    P.write_cache("alpha", "release_peaks", {"key": "k"})
    assert P.load_cache("alpha", "release_peaks") == {"key": "k"}
