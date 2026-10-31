import os

import numpy as np
import pytest
import soundfile as sf

import analyze as A
import modes as MO
import split_runs as SR


def bar_note(f1, sr=44100, dur=4.0, ratios=(1.0, 4.0, 10.0), levels=(1.0, 0.1, 0.3), decays=(3.0, 15.0, 60.0)):
    t = np.arange(int(dur * sr)) / sr
    x = sum(a * 10 ** (-d * t / 20) * np.sin(2 * np.pi * r * f1 * t) for r, a, d in zip(ratios, levels, decays))
    return np.column_stack([x, x]) * 0.5


def test_note_ranges_of_the_iowa_runs():
    assert SR.note_range("C3B3") == (48, 59) and SR.note_range("C6F6") == (84, 89) and SR.note_range("C7") == (96, 96)


def test_the_mode_finder_takes_no_string_template(tmp_path):
    sr = 44100
    f1 = A.midi_to_hz(62) * 2 ** (10 / 1200)                    # D4, 10 cents sharp
    x = np.concatenate([np.zeros((int(0.1 * sr), 2)), bar_note(f1, sr)])
    path = tmp_path / "62.wav"
    sf.write(path, x, sr, subtype="FLOAT")
    r = MO.analyze_modes({"path": str(path), "note": 62, "layer": 3, "layers": 3, "dynamic": "ff"})
    assert "error" not in r and "label_mismatch" not in r and abs(r["tuning_cents"] - 10) < 1
    found = {round(m["ratio"]): m for m in r["modes"]}
    assert set(found) >= {1, 4, 10}
    for k, d in ((1, 3.0), (4, 15.0), (10, 60.0)):
        assert abs(found[k]["ratio"] - k) < 0.002 and abs(found[k]["decay_db_s"] - d) < 0.1 * d


def test_a_run_splits_at_each_notes_own_strike(tmp_path):
    sr = 22050
    gaps = (3.0, 2.0, 3.5, 2.5)                                    # an uneven pace
    x = np.zeros((int(sum(gaps) * sr) + sr, 2))
    t0 = 0.5
    for k, g in enumerate(gaps):
        n = bar_note(A.midi_to_hz(60 + k), sr, dur=g + 1.0)      # each rings into the next strike
        a = int(t0 * sr)
        x[a:a + len(n)] += n[: len(x) - a]
        t0 += g
    ons = SR.onsets(x, sr, 60, len(gaps))
    want = 0.5 + np.concatenate([[0], np.cumsum(gaps[:-1])])
    assert np.allclose(np.array(ons) / sr, want, atol=0.02), np.array(ons) / sr


def test_the_mallet_lowpass_rises_with_the_speed():
    import bars
    lo = bars.mallet_db([260.0, 2600.0], -1.0, 600.0, 2.0, 2.0)
    hi = bars.mallet_db([260.0, 2600.0], 1.0, 600.0, 2.0, 2.0)
    assert hi[0] - lo[0] > 12.0 - 1e-9                       # 2 doublings: at least 12 dB, more past the cutoff
    assert (hi[1] - hi[0]) > (lo[1] - lo[0]) + 20.0          # the upper mode's share grows
    assert abs(bars.mallet_db([1.0], 1.0, 600.0, 2.0, 2.0)[0] - bars.GAIN_DB_PER_STEP) < 1e-4


def test_curves_carry_on_limited_and_never_rise_outward():
    import bars
    notes = np.arange(48, 90)
    up = bars._curve(notes, 0.5 * (notes - 48.0), np.ones(len(notes)), slope=0.2, no_rise=True)
    assert abs(up(70) - 11.0) < 1e-6 and up(100) == up(89)  # rising at the top: held
    assert abs(up(40) - (up(48) - 0.2 * 8)) < 1e-9           # falling towards the bass: limited
    track = {"modes": [{"ratio": 1.0, "level_db": -3.0}, {"ratio": 3.0, "level_db": -20.0},
                       {"ratio": 4.02, "level_db": -25.0}, {"ratio": 9.0, "level_db": -12.0}, {"ratio": 10.1, "level_db": -8.0}]}
    assert bars.track(track, 1)["ratio"] == 4.02 and bars.track(track, 2)["ratio"] == 10.1


def test_the_export_writes_a_bar_per_key():
    import json
    import os
    path = os.path.join(os.path.dirname(__file__), "..", "..", "instruments", "vibraphone", "instrument.json")
    if not os.path.exists(path):
        return
    doc = json.load(open(path))
    # no "damper" key: the damper bar is each note's damper, lifted by a held key or the pedal
    assert doc["body"] == "vibraphone" and "damper" not in doc and doc["pedal"] is None and doc["knock"] is None
    assert [n["note"] for n in doc["notes"]] == list(range(21, 109))
    for n in doc["notes"]:
        m = n["string"]["modes"]
        assert len(m["ratio"]) == len(m["sigma"]) == len(m["out"]) == 3 and m["ratio"][0] == 1.0
        assert n["excitation"]["type"] == "pluck" and n["excitation"]["bar"] and n["has_damper"]
        assert n["string"]["kappa"] == 0.0 and n["release"]["impacts"] == []
        hm = m["harmonics"]
        assert hm["h"][0] == 2 and all(len(hm[k]) == len(hm["h"]) for k in hm) and all(d >= 1.0 for d in hm["decay"])


def test_harmonic_bands_read_the_fundamentals_harmonics(tmp_path):
    sr = 44100
    f1 = A.midi_to_hz(55)
    t = np.arange(int(4.0 * sr)) / sr
    # the fundamental at 4 dB/s, its octave 40 dB down at twice the rate, a mode at 4.02
    x = (np.exp(-4 / 8.686 * t) * np.sin(2 * np.pi * f1 * t) + 0.01 * np.exp(-8 / 8.686 * t) * np.sin(4 * np.pi * f1 * t)
         + 0.03 * np.exp(-15 / 8.686 * t) * np.sin(2 * np.pi * 4.02 * f1 * t))
    x = np.column_stack([x, x]) * 0.5
    hb = {h["h"]: h for h in MO.harmonic_bands(x, sr, 0, f1)}
    assert abs(hb[2]["level_db"] + 40) < 1.0 and abs(hb[2]["decay_db_s"] / hb[2]["f1_decay_db_s"] - 2) < 0.15
    assert hb[3]["level_db"] < -80


def test_the_fundamental_is_read_where_it_has_settled(tmp_path):
    sr = 44100
    f1 = A.midi_to_hz(48) * 2 ** (8 / 1200)
    t = np.arange(int(4.0 * sr)) / sr
    # beside it early on a component 0.6 % low, gone within 0.2 s (the resonator's)
    x = np.exp(-0.5 * t) * np.sin(2 * np.pi * f1 * t) + 0.8 * np.exp(-30 * t) * np.sin(2 * np.pi * 0.994 * f1 * t)
    x = np.concatenate([np.zeros((int(0.1 * sr), 2)), np.column_stack([x, x]) * 0.5])
    path = tmp_path / "48.wav"
    sf.write(path, x, sr, subtype="FLOAT")
    r = MO.analyze_modes({"path": str(path), "note": 48, "layer": 3, "layers": 3, "dynamic": "ff"})
    assert abs(r["tuning_cents"] - 8) < 0.5, r["tuning_cents"]


def test_the_second_mode_is_not_the_fourth_harmonic():
    import bars
    f1 = {"ratio": 1.0, "harmonic": True, "level_db": -3.0, "decay_db_s": 5.0}
    h4 = {"ratio": 4.000, "harmonic": True, "level_db": -30.0, "decay_db_s": 8.0}
    m2 = {"ratio": 3.79, "harmonic": False, "level_db": -36.0, "decay_db_s": 50.0}
    assert bars.track({"modes": [f1, h4, m2]}, 1) is m2
    tuned = {**h4, "decay_db_s": 60.0}                       # a second mode tuned to 4.00 decays as a mode
    assert bars.track({"modes": [f1, tuned, m2]}, 1) is tuned
    assert bars.track({"modes": [f1, h4]}, 1) is h4         # nothing else: the peak there


def test_the_marimba_export_has_mallets_and_no_dampers():
    import json
    import os
    path = os.path.join(os.path.dirname(__file__), "..", "..", "instruments", "marimba", "instrument.json")
    if not os.path.exists(path):
        return
    doc = json.load(open(path))
    assert doc["body"] == "marimba" and "damper" not in doc and doc["motor"] is False
    names = [m["name"] for m in doc["mallets"]]
    assert names == ["yarn", "cord", "rubber"] and doc["velocity"]["speed"] == doc["mallets"][0]["speed"]
    # four modes, each with its onset partner (export_bars `onset_s`: the same mode in antiphase)
    assert all(not n["has_damper"] and n["damper_sigma"] is None and len(r := n["string"]["modes"]["ratio"]) == 8 and r[4:] == r[:4]
               for n in doc["notes"])
    # the notes' cutoff: the first mallet's, scaled to the bar (higher on the treble's lighter bars)
    fq = [n["excitation"]["f_q"] for n in doc["notes"]]
    assert fq[-1] > fq[0] and all(n["excitation"]["fq_oct_per_step"] == doc["mallets"][0]["fq_oct_per_step"] for n in doc["notes"])


def test_mallets_are_fitted_together_re_the_first_sets_mf():
    import bars
    cfg = bars.config("marimba")
    assert cfg["sets"][0] == "marimba_yarn" and cfg["damp"] is None and len(cfg["tracks"]) == 4
    assert bars.cutoff_scale(bars.F_REF, 0.7) == 1.0 and bars.cutoff_scale(2 * bars.F_REF, 1.0) == 2.0
    # a scaled cutoff: a lower level at the same frequency on a heavier (lower) bar
    lo = bars.mallet_db([2000.0], 0.0, 600.0, 1.0, 2.0, bars.cutoff_scale(130.0, 0.5))
    hi = bars.mallet_db([2000.0], 0.0, 600.0, 1.0, 2.0, bars.cutoff_scale(1000.0, 0.5))
    assert lo[0] < hi[0]


def test_the_xylophone_export_tunes_a_twelfth():
    import json
    import os
    path = os.path.join(os.path.dirname(__file__), "..", "..", "instruments", "xylophone", "instrument.json")
    if not os.path.exists(path):
        return
    doc = json.load(open(path))
    assert doc["body"] == "xylophone" and doc["motor"] is False and [m["name"] for m in doc["mallets"]] == ["rosewood", "hardrubber"]
    ratio = {n["note"]: n["string"]["modes"]["ratio"] for n in doc["notes"]}
    assert abs(ratio[70][1] - 3.0) < 0.01 and ratio[100][1] < 2.8 and all(len(r) == 3 for r in ratio.values())


def test_a_harmonic_needs_a_small_correction_back_to_the_strike():
    import bars
    # a take decaying 400 dB/s, its harmonic at twice the rate: +56 dB back to the strike: left out
    assert (2.0 - 1) * 400 * bars.H_EARLY_MID_S > bars.H_MAX_CORR_DB > (2.0 - 1) * 6 * bars.H_EARLY_MID_S


def test_a_run_that_starts_on_its_first_strike_keeps_it(tmp_path):
    # the bells' plastic ff C5–B5: the first strike ~20 ms in, inside the first analysis frame
    sr = 22050
    gaps = (2.0, 2.5, 2.0)
    x = np.zeros((int(sum(gaps) * sr) + sr, 2))
    t0 = 0.02
    for k, g in enumerate(gaps):
        n = bar_note(A.midi_to_hz(72 + k), sr, dur=g + 1.0)
        a = int(t0 * sr)
        x[a:a + len(n)] += n[: len(x) - a]
        t0 += g
    x += 1e-5 * np.random.default_rng(1).standard_normal(x.shape)
    ons = SR.onsets(x, sr, 72, len(gaps))
    want = 0.02 + np.concatenate([[0], np.cumsum(gaps[:-1])])
    assert np.allclose(np.array(ons) / sr, want, atol=0.02), np.array(ons) / sr


def test_a_track_follows_its_anchors_over_the_keys():
    import bars
    t = ((72, 11.3), (108, 3.96))
    assert bars.nominal(4.0, 90) == 4.0
    assert abs(bars.nominal(t, 72) - 11.3) < 1e-9 and abs(bars.nominal(t, 108) - 3.96) < 1e-9
    assert abs(bars.nominal(t, 90) - np.sqrt(11.3 * 3.96)) < 1e-6           # halfway in log ratio
    assert bars.nominal(t, 60) == bars.nominal(t, 72)                        # held beyond them
    s = {"note": 90, "modes": [{"ratio": 1.0, "level_db": 0.0, "decay_db_s": 5.0},
                               {"ratio": 6.70, "level_db": -20.0, "decay_db_s": 60.0},
                               {"ratio": 7.5, "level_db": -10.0, "decay_db_s": 60.0}]}
    assert bars.track(s, 1, (1.0, t), (0.03, 0.03))["ratio"] == 6.70


def test_the_bells_export_has_drifting_modes_and_two_mallets():
    import json
    import os
    path = os.path.join(os.path.dirname(__file__), "..", "..", "instruments", "bells", "instrument.json")
    if not os.path.exists(path):
        return
    doc = json.load(open(path))
    assert doc["body"] == "bells" and doc["motor"] is False and [m["name"] for m in doc["mallets"]] == ["plastic", "brass"]
    assert all(not n["has_damper"] for n in doc["notes"])
    ratio = {n["note"]: n["string"]["modes"]["ratio"] for n in doc["notes"]}
    assert ratio[72][2] > 7.0 and ratio[96][2] < 5.7 and ratio[72][4] > 11.0 and ratio[108][4] < 4.2
    # the brass recorded at one dynamic: its speeds spaced as the plastic's
    p, b = (np.array(m["speed"]) for m in doc["mallets"])
    assert np.allclose(b / b[1], p / p[1])


def test_a_file_of_one_note_is_cut_at_its_strike_not_at_later_noise(tmp_path, monkeypatch):
    # the crotales: one strike near the start, rumble and a noise burst seconds later
    sr = 22050
    x = np.zeros((int(4.0 * sr), 2))
    n = bar_note(A.midi_to_hz(96), sr, dur=3.9) * 0.01            # a pp strike
    a = int(0.05 * sr)
    x[a:a + len(n)] += n[: len(x) - a]
    t = np.arange(len(x)) / sr
    x += (0.05 * np.sin(2 * np.pi * 45 * t) * (1 + np.sin(2 * np.pi * 0.7 * t)))[:, None]
    b = int(2.5 * sr)
    x[b:b + 200] += 0.2 * np.random.default_rng(2).standard_normal((200, 2))
    sf.write(tmp_path / "crotale.C7.pp.aif", x, sr, format="AIFF")
    monkeypatch.setattr(SR, "ROOT", str(tmp_path))
    r = SR.split(str(tmp_path / "crotale.C7.pp.aif"))
    assert abs(r["notes"][0]["note"] - 96) == 0 and abs(r["notes"][0]["start_s"] - 0.05) < 0.02, r
    assert (tmp_path / "notes" / "crotale" / "pp" / "96.wav").exists()


def test_a_set_without_mf_has_it_midway_between_pp_and_ff():
    import project as P
    if not P.has_fit("crotales"):
        return
    sp = P.load_fit("crotales")["hammer_speeds"]
    assert sp["mf"] == 1.0 and abs(np.log2(sp["pp"]) + np.log2(sp["ff"])) < 1e-6 and sp["ff"] > 2.0


def test_the_crotales_export_has_drifting_modes_and_one_mallet():
    import json
    import os
    path = os.path.join(os.path.dirname(__file__), "..", "..", "instruments", "crotales", "instrument.json")
    if not os.path.exists(path):
        return
    doc = json.load(open(path))
    assert doc["body"] == "crotales" and doc["motor"] is False and "mallets" not in doc
    assert all(not n["has_damper"] for n in doc["notes"])
    ratio = {n["note"]: n["string"]["modes"]["ratio"] for n in doc["notes"]}
    assert ratio[84][2] > 3.7 and ratio[108][2] < 3.25 and ratio[84][1] > 2.1 and ratio[108][1] < 1.95


def test_a_cut_holding_its_neighbors_note_is_found():
    # a run C4–D4 whose cuts are shifted by one (a stray strike before the second note): the
    # cuts labeled C#4 and D4 hold C4 and C#4
    sr = 22050
    x = np.zeros((int(7.5 * sr), 2))
    for k, t0 in enumerate((0.5, 3.0, 5.5)):
        n = bar_note(A.midi_to_hz(60 + k), sr, dur=2.0)
        a = int(t0 * sr)
        x[a:a + len(n)] += n
    good = [int(0.5 * sr), int(3.0 * sr), int(5.5 * sr)]
    assert SR.pitch_mismatches(x, sr, good, 60) == []
    shifted = [int(0.5 * sr), int(1.5 * sr), int(3.0 * sr)]
    assert SR.pitch_mismatches(x, sr, shifted, 60) == [1, 2]


@pytest.mark.parametrize("run", ["Marimba.rubber.pp.C2B2.aif", "Vibraphone.sustain.pp.C4B4.aif"])
def test_a_run_with_a_stray_strike_splits_by_pitch(tmp_path, monkeypatch, run):
    # the marimba's rubber pp C2–B2 took a noise for C#2 (C#2–F2 held C2–E2), the vibraphone's
    # pp C4–B4 one for C#4: every cut holds its own note
    path = os.path.join(SR.ROOT, run)
    if not os.path.exists(path):
        return
    monkeypatch.setattr(SR, "ROOT", str(tmp_path))
    res = SR.split(path)
    assert "error" not in res, res
    x, sr = sf.read(path, always_2d=True)
    ons = [round(n["start_s"] * sr) for n in res["notes"]]
    assert SR.pitch_mismatches(x, sr, ons, res["notes"][0]["note"]) == [], res
