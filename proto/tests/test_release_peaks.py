import os

import pytest

import project as P
import release

HAVE = P.has_cache("grand_a", "analysis")


def test_the_render_sources_are_the_model_the_peaks_render():
    # the peaks are the prototype's renders: its code is part of what they depend on (Grand A's table
    # came from a working tree no commit holds and outlived the model it was rendered with, step 200)
    names = {os.path.basename(f) for f in release._render_sources()}
    assert {"soundboard.py", "scale.py", "release.py", "project.py"} <= names
    assert not {"export.py", "build.py", "report.py"} & names


@pytest.mark.skipif(not HAVE, reason="no analysis")
def test_the_peak_key_follows_the_code(monkeypatch):
    before = release._peak_key("grand_a")
    monkeypatch.setattr(release, "_code_digest", lambda: b"another model")
    assert release._peak_key("grand_a") != before


@pytest.mark.skipif(not HAVE, reason="no analysis")
def test_the_peak_key_follows_the_project(monkeypatch):
    # a table the fit never sees (the unisons' spread at the top keys) changes the renders too
    before = release._peak_key("grand_a")
    cfg = P.config("grand_a")
    monkeypatch.setattr(P, "config", lambda name: {**cfg, "unison": {"top_cents": 9.9}} if name == "grand_a" else cfg)
    assert release._peak_key("grand_a") != before
