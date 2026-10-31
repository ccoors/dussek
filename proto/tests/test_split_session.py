"""analysis/split_session.py: a session's pass cut at its strikes, labeled in the sheet's order."""
import json
import os

import numpy as np
import soundfile as sf

import analyze as A
import split_session as SS

SR = 16000


def tone(key, seconds, level_db=-20.0, decay_db_s=18.0, damp_at=None, seed=0):
    """A struck string's partials (slightly stretched), decaying at decay_db_s; damped at damp_at
    (s) by 150 dB/s."""
    t = np.arange(int(seconds * SR)) / SR
    f0 = A.midi_to_hz(key)
    rng = np.random.default_rng(seed)
    x = np.zeros_like(t)
    for n in range(1, 13):
        f = n * f0 * np.sqrt(1 + 2e-4 * n * n)
        if f > 0.45 * SR:
            break
        x += 10 ** (-(n - 1) * 1.5 / 20) * np.sin(2 * np.pi * f * t + rng.uniform(0, 2 * np.pi)) \
            * 10 ** (-decay_db_s * (1 + 0.1 * n) * t / 20)
    env = np.ones_like(t)
    if damp_at is not None:
        env[t > damp_at] = 10 ** (-150 * (t[t > damp_at] - damp_at) / 20)
    x *= env * np.minimum(t / 0.002, 1)
    return 10 ** (level_db / 20) * x / np.abs(x).max()


def session(parts, seed=1):
    """The tones one after another (each followed by its own silence), over a low noise floor."""
    x = np.concatenate([np.zeros(int(1.0 * SR))] + [np.r_[p, np.zeros(int(0.5 * SR))] for p in parts])
    x = x + 10 ** (-85 / 20) * np.random.default_rng(seed).standard_normal(len(x))
    return np.stack([x, x], axis=1)


def test_a_pass_is_labeled_in_the_sheets_order_with_its_mis_strikes_and_wrong_notes_left_out(tmp_path):
    keys = [45, 48, 51]
    x = session([tone(45, 5.0), tone(45, 5.0, -24.0, seed=1),
                 tone(48, 3.0, damp_at=0.8),                        # a mis-strike, stopped
                 tone(48, 5.0, -22.0), tone(48, 5.0, -21.0, seed=2),
                 tone(50, 5.0),                                     # a wrong note
                 tone(51, 5.0), tone(51, 5.0, -30.0, seed=3)])
    takes, _ = SS.split_pass(x, SR, keys)
    assert len(takes) == 8
    wrong = [t for t in takes if t["why"] and t["why"][0].startswith("wrong")]
    assert len(wrong) == 1 and wrong[0]["why"] == ["wrong: sounds D3"], takes
    kept = [t for t in takes if t is not wrong[0]]
    assert [t["key"] for t in kept] == [45, 45, 48, 48, 48, 51, 51]
    assert kept[2]["why"] and kept[2]["why"][0].startswith("cut"), kept[2]
    assert all(not t["why"] for t in kept if t is not kept[2]), kept
    # ranked by evenness: the take nearer its neighbors' level first
    assert [t.get("rank") for t in kept[5:]] == [1, 2]

    SS.write_takes(x, SR, takes, str(tmp_path), "mf")
    doc = {"pass": "mf.wav", "dynamic": "mf", "releases": False, "takes": takes}
    (tmp_path / "mf.split.json").write_text(json.dumps(doc))
    assert len(os.listdir(tmp_path / "mf")) == 6 and len(os.listdir(tmp_path / "discarded" / "mf")) == 2
    assert "C3_mf_1.wav" in os.listdir(tmp_path / "discarded" / "mf")
    y, sr = sf.read(tmp_path / "mf" / "A2_mf_1.wav")
    assert sr == SR and 4.5 < len(y) / SR < 6.0

    n_att, n_rel = SS.write_sfz(str(tmp_path), "set.sfz")
    assert (n_att, n_rel) == (6, 0)
    regions = [line for line in (tmp_path / "set.sfz").read_text().splitlines() if line.startswith("<region>")]
    first = kept[5]["file"]
    assert f"<region> sample={first} pitch_keycenter=51 lokey=51 hikey=51 lovel=51 hivel=100 seq_position=1" in regions


def test_a_noise_is_no_strike_and_a_ringing_note_is_not_struck_again():
    noise = 10 ** (-40 / 20) * np.random.default_rng(5).standard_normal(int(0.05 * SR))
    x = session([tone(57, 5.0), np.r_[noise, np.zeros(int(2.0 * SR))], tone(60, 5.0)])
    takes, _ = SS.split_pass(x, SR, [57, 60])
    assert [(t["key"], t["why"]) for t in takes] == [(57, []), (60, [])]


def test_a_release_is_found_at_the_dampers_fall():
    x = session([tone(60, 4.0, damp_at=1.5, decay_db_s=6.0), tone(63, 4.0, damp_at=2.0, decay_db_s=6.0)])
    takes, _ = SS.split_pass(x, SR, [60, 63], release=True)
    assert [t["key"] for t in takes] == [60, 63]
    for t, at in zip(takes, (1.5, 2.0)):
        assert abs((t["release"] - t["start"]) / SR - at) < 0.03, t


def test_keys_from_the_command_line():
    assert SS.keys_arg("21:30:3") == [21, 24, 27, 30]
    assert SS.keys_arg("38,88,89") == [38, 88, 89]
    assert SS.key_name(39) == "D#2" and SS.key_name(108) == "C8"


def test_keys_at_various_velocities_take_the_nearest_dynamic(tmp_path):
    # the sheet's passes put key 45 at −40/−30/−20 dB and key 51 at −42/−32/−22 dB (pp/mf/ff)
    for d, lv in (("pp", -40.0), ("mf", -30.0), ("ff", -20.0)):
        doc = {"pass": f"{d}.wav", "dynamic": d, "releases": False,
               "takes": [{"key": 45, "level_db": lv, "why": []}, {"key": 51, "level_db": lv - 2.0, "why": []},
                         {"key": 51, "level_db": lv - 30.0, "why": ["soft"]}]}
        (tmp_path / f"{d}.split.json").write_text(json.dumps(doc))
    levels = SS.sheet_levels(str(tmp_path))
    assert levels == {"pp": {45: -40.0, 51: -42.0}, "mf": {45: -30.0, 51: -32.0}, "ff": {45: -20.0, 51: -22.0}}
    takes = [{"key": 48, "level_db": v, "why": []} for v in (-31.5, -20.0, -21.5, -10.0)] + \
            [{"key": 60, "level_db": -30.0, "why": []}]
    SS.assign_dynamics(takes, levels)
    assert [(t.get("dynamic"), t.get("rank")) for t in takes] == [("mf", 1), ("ff", 2), ("ff", 1), (None, None), (None, None)]
    assert takes[3]["why"][0].startswith("level: +11.0 dB from ff") and takes[4]["why"][0].startswith("level: no sheet")


def test_a_strike_is_placed_at_its_hammer_not_where_the_smoothed_level_peaks():
    # a key's thump 0.12 s before the hammer, then the note
    thump = 10 ** (-55 / 20) * np.random.default_rng(7).standard_normal(int(0.01 * SR))
    note = np.r_[thump, np.zeros(int(0.11 * SR)), tone(48, 4.0)]
    x = session([note])
    _, found = SS.strikes(x, SR)
    strikes = [s for s in found if s["note"]]
    assert len(strikes) == 1
    hammer = int(1.0 * SR) + len(thump) + int(0.11 * SR)
    assert abs(strikes[0]["at"] - hammer) < 0.006 * SR, (strikes[0]["at"] - hammer) / SR
