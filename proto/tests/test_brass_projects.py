"""The five brass projects and the cutting of their recordings (analysis/split_brass.py)."""
import json
import os
import subprocess
import sys

import numpy as np
import pytest
import soundfile as sf

import project as P
import split_brass


def chromatic_run(keys, sr=44100):
    """Each key's tone with its harmonics for 1 s, 0.3 s of faint noise between them."""
    rng = np.random.default_rng(1)
    parts = []
    for k in keys:
        t = np.arange(sr) / sr
        f = 440 * 2 ** ((k - 69) / 12)
        parts.append(sum(np.sin(2 * np.pi * h * f * t) / h for h in range(1, 6)) * 0.2)
        parts.append(rng.normal(size=int(0.3 * sr)) * 1e-4)
    return np.concatenate(parts), sr


def test_a_lost_note_shifts_only_that_note():
    y, sr = chromatic_run([60, 61, 63, 64])
    pairs = split_brass.align(y, sr, split_brass.cuts(y, sr), 60, 64)
    assert [m for _, m in pairs] == [60, 61, 63, 64]


def test_a_run_with_short_gaps_keeps_its_quiet_notes_whole():
    """The horn's runs: notes of 2.3 s, 0.2 s between them, the first ones quieter. Under a tenth of a run is silence, so a floor
    at the tenth percentile lies in the notes and only their loudest moments rise above it."""
    sr, parts = 44100, []
    rng = np.random.default_rng(1)
    keys = list(range(60, 72))
    for i, k in enumerate(keys):
        t = np.arange(int(2.3 * sr)) / sr
        f = 440 * 2 ** ((k - 69) / 12)
        wander = 1 + 0.03 * np.sin(2 * np.pi * 0.7 * t)
        parts.append(sum(np.sin(2 * np.pi * h * f * t) / h for h in range(1, 6)) * 0.1 * 10 ** (i * 0.5 / 20) * wander)
        parts.append(rng.normal(size=int(0.2 * sr)) * 1e-5)
    y = np.concatenate(parts)
    segs = split_brass.cuts(y, sr)
    pairs = split_brass.align(y, sr, segs, 60, 71)
    assert [m for _, m in pairs] == keys
    assert all(segs[i][1] - segs[i][0] > 2.0 for i, _ in pairs)


def test_a_faint_sound_before_a_note_does_not_take_its_place():
    """The trumpet's runs: a sound some 60 dB under the note, a second before it, on the note's pitch to the search."""
    sr, parts = 44100, []
    keys = [60, 61, 62]
    for k in keys:
        f = 440 * 2 ** ((k - 69) / 12)
        parts.append(np.zeros(sr))
        t = np.arange(int(0.4 * sr)) / sr
        parts.append(np.sin(2 * np.pi * f * t) * 3e-4)
        parts.append(np.zeros(int(0.6 * sr)))
        t = np.arange(3 * sr) / sr
        parts.append(sum(np.sin(2 * np.pi * h * f * t) / h for h in range(1, 6)) * 0.2)
    parts.append(np.zeros(sr))
    y = np.concatenate(parts) + np.random.default_rng(1).normal(size=sum(map(len, parts))) * 1e-5
    segs = split_brass.cuts(y, sr)
    pairs = split_brass.align(y, sr, segs, 60, 62)
    assert [m for _, m in pairs] == keys
    assert all(segs[i][1] - segs[i][0] > 2.5 for i, _ in pairs)


def test_a_run_s_range_is_in_its_name(tmp_path):
    for n in ("X.ff. Db1B1.aiff", "X.pp.C2B2.aiff", "X.ff.C4.aiff", "X.ff.zip", "Y.ff.C4.aiff"):
        (tmp_path / n).write_bytes(b"")
    got = [(d, lo, hi) for _, d, lo, hi in split_brass.runs(str(tmp_path), "X")]
    assert got == [("ff", 25, 35), ("ff", 60, 60), ("pp", 36, 47)]


def test_a_note_comes_out_on_its_key(tmp_path):
    y, sr = chromatic_run([60, 61, 63])
    sf.write(tmp_path / "X.ff.C4Eb4.aiff", y, sr, subtype="PCM_24")
    kept = split_brass.split(str(tmp_path), "X", 30.0, str(tmp_path / "notes"))
    assert sorted(kept) == [("ff", 60), ("ff", 61), ("ff", 63)]
    x, _ = sf.read(tmp_path / "notes" / "X" / "ff" / "61.wav")
    assert 0.9 * sr < len(x) < 1.1 * sr


def test_a_note_says_where_it_lies_in_its_run(tmp_path):
    """The silence around a note stays in its run: the fits measure the background there."""
    y, sr = chromatic_run([60, 61, 63])
    run = tmp_path / "X.ff.C4Eb4.aiff"
    sf.write(run, y, sr, subtype="PCM_24")
    split_brass.split(str(tmp_path), "X", 30.0, str(tmp_path / "notes"))
    note, _ = sf.read(tmp_path / "notes" / "X" / "ff" / "61.wav")
    where = split_brass.where(str(tmp_path / "notes" / "X" / "ff" / "61.wav"), str(tmp_path))
    assert where["file"] == str(run)
    x, _ = sf.read(run)
    assert 1.2 < where["start"] < 1.4 and 0.9 < where["end"] - where["start"] < 1.1
    assert np.array_equal(x[int(where["start"] * sr):int(where["end"] * sr)], note)


def test_the_fits_read_the_notes_in_their_runs():
    notes = os.path.join(P.REPO, "data", "iowa-mis-brass", "notes", "Horn")
    if not os.path.isdir(notes):
        pytest.skip("the recordings are not here")
    code = "import json, spec; print(json.dumps(spec.notes()))"
    env = {**os.environ, "BRASS_PROJECT": "horn"}
    out = subprocess.run([sys.executable, "-c", code], cwd=os.path.join(P.REPO, "proto", "brass"), env=env, capture_output=True, text=True, check=True)
    got = json.loads(out.stdout)
    assert len(got) == 95
    assert all(n["file"].endswith((".aif", ".aiff")) and n["end"] > n["start"] for n in got.values())
    assert sum(n["start"] > 0 for n in got.values()) > 80


def test_the_projects_keep_the_spec_s_ranges():
    want = {"trumpet": (54, 86), "tenor_trombone": (40, 72), "horn": (38, 77), "bass_trombone": (28, 65), "tuba": (28, 60)}
    for name, (lo, hi) in want.items():
        b = P.config(name)["brass"]
        assert (b["low"], b["high"]) == (lo, hi), name


def test_the_projects_keep_the_spec_s_mutes():
    five = ["straight", "cup", "wah", "wah_stem", "bucket", "plunger"]
    want = {"trumpet": five, "tenor_trombone": five, "horn": [],
            "bass_trombone": ["straight", "cup", "bucket", "plunger"], "tuba": ["straight"]}
    for name, mutes in want.items():
        assert P.config(name)["brass"]["mutes"] == mutes, name
