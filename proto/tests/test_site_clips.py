"""site/clips.toml and site/render.py's check: every clip names what exists (an instrument, its MIDI
files within the instrument's recorded keys, plugin controls, sound files), every menu group has
one demo, and a bad clip stops the render before anything is rendered."""
import glob
import json
import os
import shutil
import subprocess
import sys
import tomllib

import pytest

import project as P

RENDER = f"{P.REPO}/site/render.py"
TABLE = f"{P.REPO}/site/clips.toml"
SETTINGS_LINE = f"{P.REPO}/plugin/build/settings_line"
needs_tool = pytest.mark.skipif(not os.path.exists(SETTINGS_LINE), reason="needs plugin/build/settings_line (just plugin)")
needs_ffmpeg = pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="needs ffmpeg (the site's runner installs it)")


def check(table):
    return subprocess.run([sys.executable, RENDER, "--check", "--table", str(table)], capture_output=True, text=True)


@needs_tool
def test_table_names_only_what_exists():
    r = check(TABLE)
    assert r.returncode == 0, r.stderr


def test_each_menu_group_has_one_demo():
    clips = tomllib.load(open(TABLE, "rb"))["clip"]
    groups = {}
    for f in glob.glob(f"{P.REPO}/instruments/*/instrument.json"):
        menu = json.load(open(f)).get("menu", {})
        if not menu.get("hidden"):
            groups[os.path.basename(os.path.dirname(f))] = menu.get("group", "Other")
    demos = [groups[c["instrument"]] for c in clips if c["section"] == "demo"]
    assert sorted(demos) == sorted(set(groups.values())), "one demo per menu group"


@needs_tool
def test_a_bad_clip_fails_before_rendering(tmp_path):
    bad = tmp_path / "clips.toml"
    bad.write_text('[[clip]]\nid = "oops"\nsection = "demo"\ntitle = "Oops"\ninstrument = "nonesuch"\nmidi = "piano"\n')
    r = check(bad)
    assert r.returncode == 1 and "oops" in r.stderr and "nonesuch" in r.stderr


@needs_tool
def test_notes_outside_the_recorded_keys_are_refused(tmp_path):
    # the psaltery is recorded from key 70 up: the piano phrase reaches far below
    bad = tmp_path / "clips.toml"
    bad.write_text('[[clip]]\nid = "low"\nsection = "demo"\ntitle = "Low"\ninstrument = "psaltery"\nmidi = "piano"\n')
    r = check(bad)
    assert r.returncode == 1 and "low" in r.stderr and "recorded" in r.stderr


@needs_tool
def test_an_unknown_control_is_refused(tmp_path):
    bad = tmp_path / "clips.toml"
    bad.write_text('[[clip]]\nid = "knob"\nsection = "feature"\ntitle = "Knob"\ninstrument = "grand_a"\n'
                   'midi = "piano"\nsteps = [{ label = "x", set = { nonesuch = 1 } }]\n')
    r = check(bad)
    assert r.returncode == 1 and "knob" in r.stderr and "nonesuch" in r.stderr


@needs_tool
def test_an_unknown_key_is_refused(tmp_path):
    # a misspelled key (`second` for `seconds`) or one in the wrong place (`input` on the clip, read
    # only in a step) would be ignored without a word
    bad = tmp_path / "clips.toml"
    bad.write_text('[[clip]]\nid = "typo"\nsection = "demo"\ntitle = "Typo"\ninstrument = "grand_a"\nmidi = "piano"\n'
                   'second = 10\ninput = "site/sounds/falling.ogg"\n'
                   '[[clip]]\nid = "steptypo"\nsection = "feature"\ntitle = "T"\ninstrument = "grand_a"\nmidi = "piano"\n'
                   'steps = [{ label = "x", sett = { condition = 1 } }]\n')
    r = check(bad)
    assert r.returncode == 1
    assert "typo: unknown key `second`" in r.stderr and "typo: unknown key `input`" in r.stderr
    assert "steptypo (x): unknown key `sett`" in r.stderr


def load_render():
    import importlib.util
    spec = importlib.util.spec_from_file_location("site_render", RENDER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@needs_ffmpeg
def test_intermediate_files_are_float(tmp_path):
    # the steps' WAVs between the renders and the encoder: 32-bit float, not ffmpeg's 16-bit default
    import soundfile
    render = load_render()
    c = {"id": "src", "section": "feature", "title": "Src", "steps": [{"label": "s", "source": "site/sounds/falling.ogg"}]}
    wav = tmp_path / "s.wav"
    render.render_step(c, c["steps"][0], str(wav), str(tmp_path))
    assert soundfile.info(str(wav)).subtype == "FLOAT"


@needs_tool
def test_a_failed_render_says_why(tmp_path, monkeypatch):
    # midi2wav's own message reaches the log (CI's), not just its exit status
    render = load_render()
    fake = tmp_path / "midi2wav"
    fake.write_text("#!/bin/sh\necho 'error: the reason' >&2\nexit 1\n")
    fake.chmod(0o755)
    monkeypatch.setattr(render, "MIDI2WAV", str(fake))
    c = {"id": "boom", "section": "demo", "title": "Boom", "instrument": "grand_a", "midi": "piano"}
    with pytest.raises(SystemExit) as e:
        render.render_step(c, render.steps_of(c)[0], str(tmp_path / "b.wav"), str(tmp_path))
    assert "boom" in str(e.value) and "the reason" in str(e.value)
