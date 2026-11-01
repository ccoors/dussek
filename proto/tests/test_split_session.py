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


def test_a_pedal_pass_tells_down_from_up_by_the_strings_ring():
    # pedal down: a thump, then the strings ring (110 Hz); pedal up: a thump, the ring stops
    t = np.arange(int(8.0 * SR)) / SR
    ring = 10 ** (-45 / 20) * np.sin(2 * np.pi * 110 * t)
    thump = lambda: 10 ** (-30 / 20) * np.random.default_rng(3).standard_normal(int(0.02 * SR))  # noqa: E731
    down = thump()
    x = np.zeros(int(16.0 * SR))
    a, b = int(2.0 * SR), int(9.0 * SR)
    x[a:a + len(down)] += down
    x[a:b] += ring[:b - a]                       # the strings ring from the pedal-down to the pedal-up
    x[b:b + len(down)] += thump()
    x += 10 ** (-85 / 20) * np.random.default_rng(4).standard_normal(len(x))
    ev = SS.split_pedal(np.stack([x, x], axis=1), SR)
    assert [(e["kind"], e["why"]) for e in ev] == [("down", []), ("up", [])], ev
    assert abs(ev[0]["start"] - a) < 0.01 * SR and abs(ev[1]["start"] - b) < 0.01 * SR


def tongue(key, seconds, level_db=-20.0, decay_db_s=3.0, attack_s=0.002, seed=0):
    """A struck tongue: its fundamental and a weak octave, ringing long."""
    t = np.arange(int(seconds * SR)) / SR
    f0 = A.midi_to_hz(key)
    ph = np.random.default_rng(seed).uniform(0, 2 * np.pi, 2)
    x = np.sin(2 * np.pi * f0 * t + ph[0]) + 0.3 * np.sin(2 * np.pi * 2 * f0 * t + ph[1])
    x *= 10 ** (-decay_db_s * t / 20) * np.minimum(t / attack_s, 1)
    return 10 ** (level_db / 20) * x / np.abs(x).max()


def test_a_tongue_drums_pass_is_labeled_by_the_fundamental_leading_each_strike():
    # played in the drum's order (not rising); D5's strike sets D4's tongue ringing louder than
    # itself within half a second, and C4's ring, 30 dB down 5 s in, is touched 15 dB over it
    keys = [60, 72, 62, 74]
    c4 = tongue(60, 8.0, decay_db_s=6.0)
    c4[int(5.0 * SR):int(5.05 * SR)] += tongue(60, 0.05, -35.0, attack_s=0.001)
    d5 = tongue(74, 8.0, -26.0)
    d5[int(0.15 * SR):] += tongue(62, 8.0 - 0.15, -20.0, attack_s=0.4, seed=1)
    x = session([c4, tongue(72, 8.0, -24.0), tongue(62, 8.0), d5])
    takes, _ = SS.split_pass(x, SR, keys, tongues=True)
    assert [(t["key"], t["why"]) for t in takes] == [(60, []), (72, []), (62, []), (74, [])], takes
    assert takes[0]["length_s"] > 7.5


def test_silent_releases_are_labeled_in_order_without_the_notes_struck_by_mistake(tmp_path):
    # a key pressed without its note and released: the thump of its return, the keys every third;
    # before the pass a stray event, between A0 and C1 a note struck by mistake (C1's own, damped),
    # D#1 released twice, and the recorder's stop cut by the file's end
    def thump(level_db, seed):
        n = int(0.6 * SR)
        return 10 ** (level_db / 20) * np.random.default_rng(seed).standard_normal(n) * np.exp(-np.arange(n) / (0.08 * SR))
    gap = np.zeros(int(4.0 * SR))
    parts = [thump(-50, 1), gap, thump(-45, 2), gap, tone(24, 1.2, -25.0, damp_at=0.8), gap,
             thump(-46, 3), gap, thump(-48, 4), np.zeros(int(1.5 * SR)), thump(-44, 5), gap, thump(-40, 6)[:int(0.1 * SR)]]
    x = np.concatenate(parts)
    x = x + 10 ** (-85 / 20) * np.random.default_rng(9).standard_normal(len(x))
    x = np.stack([x, x], axis=1)
    takes = SS.split_silent(x, SR, [21, 24, 27], start_s=2.0)
    kept = [(t["key"], t["take"]) for t in takes if not t["why"]]
    assert kept == [(21, 1), (24, 1), (27, 1), (27, 2)], takes
    why = [t["why"][0] for t in takes if t["why"]]
    assert len(why) == 2 and why[0].startswith("a note") and why[1].startswith("cut by the file's end"), why
    SS.write_silent(x, SR, takes, str(tmp_path))
    assert sorted(os.listdir(tmp_path / "release_silent")) == ["A0_1.wav", "C1_1.wav", "D#1_1.wav", "D#1_2.wav"]
    y, _ = sf.read(tmp_path / "release_silent" / "D#1_1.wav")
    assert len(y) / SR < SS.SILENT_S + 0.1


def test_a_second_pedal_pass_continues_the_numbering(tmp_path):
    (tmp_path / "pedal").mkdir()
    for name in ("down_1.wav", "down_2.wav", "up_1.wav"):
        sf.write(tmp_path / "pedal" / name, np.zeros(10), SR)
    events = [{"start": 100, "end": 200, "kind": "down", "take": 1, "why": []},
              {"start": 300, "end": 400, "kind": "up", "take": 1, "why": []}]
    first = SS.pedal_first(str(tmp_path), None)
    assert first == {"down": 2, "up": 1}
    SS.write_pedal(np.zeros((500, 2)), SR, events, str(tmp_path), "pedal_2", first)
    assert [e["file"] for e in events] == [os.path.join("pedal", "down_3.wav"), os.path.join("pedal", "up_2.wav")]
    # split again, a pass keeps the numbers it had (the files after it not counted); its left-out
    # events carry its name (another pass's are not overwritten)
    assert SS.pedal_first(str(tmp_path), {"first": {"down": 2, "up": 1}}) == {"down": 2, "up": 1}
    assert SS.pedal_first(str(tmp_path), {"takes": []}) == {}
    gone = [{"start": 100, "end": 200, "kind": "unclear", "why": ["the ring changes +0.5 dB"]}]
    SS.write_pedal(np.zeros((500, 2)), SR, gone, str(tmp_path), "pedal_2", {})
    assert gone[0]["file"] == os.path.join("discarded", "pedal", "unclear_pedal_2_event1.wav")


def test_a_pass_at_another_gain_is_written_at_the_sessions_as_float(tmp_path):
    # recorded 4 dB lower (the recorder's gain turned down for the loud takes): written 4 dB up,
    # past full scale where it peaks near it, so as float
    x = session([tone(60, 3.0, -2.0)])
    takes, _ = SS.split_pass(x, SR, [60], gain_db=4.0)
    assert takes[0]["why"] == []
    SS.write_takes(SS.at_gain(x, 4.0), SR, takes, str(tmp_path), "ff")
    info = sf.info(tmp_path / "ff" / "C4_ff_1.wav")
    y, _ = sf.read(tmp_path / "ff" / "C4_ff_1.wav")
    assert info.subtype == "FLOAT" and np.abs(y).max() > 1.0
    assert abs(takes[0]["peak_dbfs"] - (-2.0 + 4.0)) < 0.1


def test_a_clipped_take_plays_after_a_clean_one_of_a_later_pass(tmp_path):
    # the ff pass kept A5's clipped take as its key's only one; a redo pass (auto) gives a clean one
    ff = {"pass": "ff.wav", "dynamic": "ff", "takes": [
        {"key": 81, "rank": 1, "file": "ff/A5_ff_1.wav", "why": [], "note": "clipped, its key's only take"},
        {"key": 84, "rank": 1, "file": "ff/C6_ff_1.wav", "why": []}]}
    redo = {"pass": "redos.wav", "dynamic": "auto", "takes": [
        {"key": 81, "dynamic": "ff", "rank": 1, "file": "ff/A5_ff_redos1.wav", "why": []},
        {"key": 84, "dynamic": "ff", "rank": 1, "file": "ff/C6_ff_redos1.wav", "why": []}]}
    (tmp_path / "ff.split.json").write_text(json.dumps(ff))
    (tmp_path / "redos.split.json").write_text(json.dumps(redo))
    SS.write_sfz(str(tmp_path), "set.sfz")
    first = [line.split()[1] for line in (tmp_path / "set.sfz").read_text().splitlines() if line.endswith("seq_position=1")]
    assert first == ["sample=ff/A5_ff_redos1.wav", "sample=ff/C6_ff_1.wav"]


def test_a_take_flat_at_its_peak_is_clipped_whatever_the_ceiling():
    # the recorder clips at 0.993 (−0.06 dBFS), not at full scale; a take that merely peaks near
    # full scale is not clipped
    loud = tone(52, 4.0, 2.0)
    x = np.clip(session([loud, tone(52, 4.0, -3.0, seed=1), tone(55, 4.0, -0.3)]), -0.993, 0.993)
    takes, _ = SS.split_pass(x, SR, [52, 55], mixed=True)
    assert [t["why"] for t in takes] == [["clipped"], [], []]
