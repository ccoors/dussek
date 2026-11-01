"""site/clips.toml and site/render.py's check: every clip names what exists (an instrument, its MIDI
files within the instrument's recorded keys, plugin controls, sound files), every menu group has
one demo, and a bad clip stops the render before anything is rendered."""
import glob
import json
import os
import subprocess
import sys
import tomllib

import pytest

import project as P

RENDER = f"{P.REPO}/site/render.py"
TABLE = f"{P.REPO}/site/clips.toml"
SETTINGS_LINE = f"{P.REPO}/plugin/build/settings_line"
needs_tool = pytest.mark.skipif(not os.path.exists(SETTINGS_LINE), reason="needs plugin/build/settings_line (just plugin)")


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
