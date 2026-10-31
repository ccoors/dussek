import types

import numpy as np
import soundfile as sf

import bars
import export_bars as EB
import project as P
import pytest
import modes as MO


def tube(path, sr=48000, f1=261.63, swell=None, fast=None, dur=2.0):
    """A synthetic tube: the fundamental (2 dB/s), swelling as (1 − e^{−σt})² for swell = σ, and a
    fast mode fast = (ratio, level dB, decay dB/s); 50 ms of silence before the strike."""
    t = np.arange(int(dur * sr)) / sr
    x = 10 ** (-2 * t / 20) * np.sin(2 * np.pi * f1 * t)
    if swell:
        x *= (1 - np.exp(-swell * t)) ** 2
    if fast:
        r, lv, d = fast
        x += 10 ** (lv / 20) * 10 ** (-d * t / 20) * np.sin(2 * np.pi * r * f1 * t)
    x = np.concatenate([np.zeros(int(0.05 * sr)), x])
    sf.write(path, np.column_stack([x, x]) * 0.3, sr, subtype="FLOAT")
    return {"path": str(path), "note": 60, "layer": 1, "layers": 1, "dynamic": "mf"}


def test_the_strike_finds_a_fast_high_mode_with_its_level_and_decay(tmp_path):
    item = tube(tmp_path / "c4.wav", fast=(9.3, -15.0, 300.0))
    r = MO.strike(item, {"f0": 261.63})
    near = [p for p in r if abs(p["ratio"] / 9.3 - 1) < 0.01]
    assert len(near) == 1
    assert abs(near[0]["level_db"] + 15.0) < 2.0 and abs(near[0]["decay_db_s"] - 300.0) < 45.0


def test_the_rise_fit_finds_the_swell_and_none_without_one(tmp_path):
    stub = types.SimpleNamespace(sigmas=lambda n: np.array([2.0 / bars.NEPER_DB]))
    found = {}
    for name, sg in (("swell", 60.0), ("flat", None)):
        # the strike's click marks the onset (as the clapper's in the recordings)
        item = tube(tmp_path / f"{name}.wav", swell=sg, fast=(11.7, 0.0, 1500.0))
        s = {"note": 60, "rise": MO.rise(item, {"f0": 261.63})}
        found[name] = bars.Fit.fit_rise(stub, [s])[60]
    assert found["flat"] is None
    assert abs(np.log(found["swell"] / 60.0)) < np.log(1.25)


def test_the_swell_is_two_partners_at_the_fundamental():
    key = {"ratios": [1.0, 1.5], "sigma": [0.3, 5.0], "out_db": [-10.0, -30.0], "damper_sigma": [7.0, 7.0], "rise": 50.0,
           "strike": {"ratio": [9.3], "sigma": [35.0], "out_db": [-25.0]}}
    k = EB.with_extras(key)
    assert k["ratios"] == [1.0, 1.5, 9.3, 1.0, 1.0]
    assert k["sign"] == [1.0, 1.0, 1.0, -1.0, 1.0]
    assert np.allclose(k["sigma"][3:], [50.3, 100.3])
    assert np.allclose(k["out_db"][3:], [-10.0 + 20 * np.log10(2.0), -10.0])
    assert k["damper_sigma"] == [7.0] * 5
    out = EB.note_doc({**k, "note": 60, "f1": 261.63, "bar_mass": 0.5}, {"f_q": 3000.0, "beta": 0.3, "gain_db_per_step": 6.0,
                                                                       "fq_oct_per_step": 0.5, "order": 2.0},
                      None, EB.Blob(), damped=False)["string"]["modes"]["out"]
    assert np.allclose(np.array(out[3:]) / out[0], [-2.0, 1.0])


def test_spec_levels_take_an_upper_mode_from_the_spectrum():
    # a mode whose decay fit read it at the fundamental's level, the spectrum 20 dB under: with
    # spec_levels the spectrum's, both taken back by their decays (equal here: no correction)
    f1 = {"ratio": 1.0, "level_db": -3.0, "spec_level_db": 0.0, "edc_decay_db_s": 100.0}
    m = {"ratio": 5.8, "level_db": -3.0, "spec_level_db": -20.0, "edc_decay_db_s": 100.0}
    s = {"peak_dbfs": -6.0, "modes": [f1, m]}
    stub = types.SimpleNamespace(spec_levels=True, track=lambda s, j: f1)
    assert abs(bars.Fit.mode_level(stub, s, 1, m) - (-29.0)) < 1e-9
    assert bars.Fit.mode_level(types.SimpleNamespace(spec_levels=False, track=None), s, 1, m) == -9.0
    # a faster mode is taken back further: its spectrum's peak lies lower re its onset
    fast = {**m, "edc_decay_db_s": 300.0}
    assert bars.Fit.mode_level(stub, s, 1, fast) > -29.0


def test_the_onset_is_a_partner_per_mode_in_antiphase():
    key = {"ratios": [1.0, 3.9], "sigma": [5.0, 12.0], "out_db": [-10.0, -20.0], "damper_sigma": [0.0, 0.0]}
    k = EB.with_extras(key, onset_s=0.001)
    assert k["ratios"] == [1.0, 3.9, 1.0, 3.9] and k["sign"] == [1.0, 1.0, -1.0, -1.0]
    assert np.allclose(k["sigma"][2:], [1005.0, 1012.0]) and k["out_db"][2:] == [-10.0, -20.0]
    # each mode then starts from zero: (e^{−σt} − e^{−(σ + 1/τ)t}) at t = 0
    assert EB.with_extras(key)["ratios"] == [1.0, 3.9]



def test_a_keys_rattle_gain_brings_it_to_the_recorded_tines():
    # a tine at C5: the fundamental and a mode at 6.4, the rattle on their level squared
    key = {"f1": 523.25, "ratios": [1.0, 6.4], "sigma": [3.0, 30.0], "out_db": [-20.0, -40.0], "rattle_db": -60.0}
    mallet = {"f_q": 2500.0, "fq_oct_per_step": 0.5, "order": 1.0, "beta": 0.0}
    membrane = {"level": 0.2, "hp_hz": 300.0, "lp_hz": 6000.0, "poles": 2, "power": 2.0}
    g = EB.rattle_gain(key, mallet, 1.0, membrane)
    assert abs(EB.rattle_db(key, mallet, 1.0, membrane, g) + 60.0) < 1.0
    # a quieter tine takes less
    assert EB.rattle_gain({**key, "rattle_db": -70.0}, mallet, 1.0, membrane) < g


def test_the_analysis_stops_when_most_files_miss_their_keys():
    # a map whose keys are two octaves low flags nearly every file: the step says why instead of
    # leaving the fits an empty set
    ok = {"file": "a.wav", "f0": 130.8}
    miss = {"file": "b.wav", "error": "no fundamental near the nominal pitch"}
    assert MO.too_many_flagged([ok, ok, miss]) is None
    msg = MO.too_many_flagged([ok, miss, miss, {"file": "c.wav", "label_mismatch": 12}])
    assert msg and "3 of 4" in msg and "keys" in msg


def test_the_takes_reach_the_fits_by_the_dynamics_table(monkeypatch):
    # an SFZ set names its layers 1/3, 2/3, 3/3: the source's dynamics table says which is pp, mf
    # and ff; a layer the table leaves out is not a fit's take
    takes = [{"file": f"{k}.wav", "note": 60, "layer": k, "layers": 4, "dynamic": f"{k}/4", "modes": []} for k in (1, 2, 3, 4)]
    monkeypatch.setattr(bars.P, "load_cache", lambda ds, what: {"samples": takes})
    monkeypatch.setattr(bars.P, "source", lambda ds: {"dynamics": {"pp": "1/4", "mf": "2/4", "ff": "4/4"}})
    bars.samples.cache_clear()
    try:
        assert [(s["file"], s["dynamic"]) for s in bars.samples("zz")] == [("1.wav", "pp"), ("2.wav", "mf"), ("4.wav", "ff")]
        monkeypatch.setattr(bars.P, "source", lambda ds: {"dynamics": {"pp": 1, "ff": 4}, "layer_key": "layer"})
        bars.samples.cache_clear()
        assert [(s["file"], s["dynamic"]) for s in bars.samples("zz")] == [("1.wav", "pp"), ("4.wav", "ff")]
    finally:
        bars.samples.cache_clear()


def test_a_track_no_take_shows_is_named_with_the_takes_modes():
    # a project made from another instrument's template: its tracks miss the set's modes
    # (a glockenspiel's 2.7 and 5.3 under the hand chimes' 1.5 and 5.9)
    import bars
    mode = lambda r: {"ratio": r, "level_db": -20.0, "decay_db_s": 10.0, "conf": 0.9}  # noqa: E731
    takes = [{"note": 79, "modes": [mode(1.0), mode(2.74), mode(5.30)]},
             {"note": 98, "modes": [mode(1.0), mode(2.68), mode(5.06)]}]
    tracks, tols = [1.0, [[62, 1.49], [68, 1.55]], [[60, 5.99], [76, 5.90]]], [0.03, 0.04, 0.03]
    assert bars.missing_tracks(takes, tracks, tols) == [1, 2]
    text = bars.missing_text(takes, tracks, tols)
    assert "track 1" in text and "track 2" in text and "[bars] tracks" in text
    assert "2.74" in text and "5.06" in text, "the takes' own modes, to take the tracks from"
    assert bars.missing_tracks(takes, [1.0, [[79, 2.74], [98, 2.68]]], [0.03, 0.04]) == []


def _mode(r, level=-20.0, conf=0.9, decay=10.0, harmonic=False):
    return {"ratio": r, "level_db": level, "decay_db_s": decay, "conf": conf, "harmonic": harmonic}


def test_derived_tracks_follow_each_mode_from_key_to_key():
    # a glockenspiel's second and third modes drifting down the range, a stray mode on one key
    import bars
    takes = [{"note": n, "modes": [_mode(1.0, -10), _mode(r2), _mode(r3, -25)] + ([_mode(7.7, -30)] if n == 84 else [])}
             for n, r2, r3 in ((79, 2.74, 5.30), (81, 2.72, 5.27), (84, 2.70, 5.20), (86, 2.71, 5.22),
                               (89, 2.69, 5.18), (91, 2.69, 5.16), (95, 2.69, 5.24), (98, 2.68, 5.10))]
    tracks, tols = bars.derive_tracks(takes)
    assert tracks[0] == 1.0 and len(tracks) == 3, tracks
    assert abs(bars.nominal(tracks[1], 79) - 2.74) < 0.01 and abs(bars.nominal(tracks[1], 98) - 2.68) < 0.01
    assert abs(bars.nominal(tracks[2], 79) - 5.30) < 0.03
    assert all(0.03 <= t <= 0.12 for t in tols) and len(tols) == 3
    assert bars.missing_tracks(takes, tracks, tols) == [], "every derived track is in the takes"


def test_derived_tracks_leave_out_harmonics_and_strays():
    import bars
    # a whole-number mode 40 dB under the fundamental, decaying as slowly: its harmonic (the
    # marimba's 2.0); a mode on one key only: a stray. Neither is a track.
    takes = [{"note": n, "modes": [_mode(1.0, decay=5.0), _mode(2.0, level=-60.0, decay=6.0, harmonic=True)]
              + ([_mode(3.3)] if n == 60 else [])} for n in (60, 62, 64, 65)]
    assert bars.derive_tracks(takes) == ([1.0], [0.03])


def test_a_strong_whole_number_mode_is_a_track():
    # the xylophone's tuned 3.0: 18 dB under the fundamental, its decay fit mostly unsure (median
    # conf 0.05; its fast decay known), decaying slowly on one key in five (measured: 20 %): the
    # bar's own mode
    import bars
    takes = [{"note": n, "modes": [_mode(1.0, decay=10.0), {**_mode(3.0, level=-38.0, conf=0.3 if n in (67, 69) else 0.05, decay=16.0 if n == 65 else 60.0,
                                                                   harmonic=True), "fast_decay_db_s": 40.0}]} for n in (65, 67, 69, 71)]
    tracks, _ = bars.derive_tracks(takes)
    assert len(tracks) == 2 and abs(bars.nominal(tracks[1], 65) - 3.0) < 0.01, tracks


def test_derived_tracks_merge_a_keys_takes():
    # three layers on each key: one track per mode, not one per take
    import bars
    takes = [{"note": n, "modes": [_mode(1.0), _mode(4.0 + 0.01 * k, decay=60.0, harmonic=True)]}
             for n in (48, 50, 52, 53) for k in range(3)]
    tracks, _ = bars.derive_tracks(takes)
    assert len(tracks) == 2, "a tuned bar's 4.0 decays fast: a mode, and only one"


def test_auto_tracks_reach_the_fit_document(monkeypatch):
    # the fit with tracks = "auto": the derived tracks are its own and its document's; tied and
    # tol (set by hand for numbered tracks) are not read
    import bars
    takes = [{"note": n, "dynamic": "mf", "tuning_cents": 0.0, "modes": [_mode(1.0, -10), _mode(2.7), _mode(5.3, -25)]}
             for n in (79, 81, 84, 86)]
    cfg = {"tracks": "auto", "tied": {1: 0}, "scale_cutoff": True, "sets": ["s"], "mallets": ["m"], "damp": None,
           "mallet": {"speeds": {"pp": 0.25, "mf": 1.0, "ff": 3.0}, "f_q": 3000.0, "fq_oct_per_step": 0.5, "order": 2.0, "beta": 0.3},
           "damper_db_s": 60.0}
    monkeypatch.setattr(bars, "config", lambda profile: cfg)
    monkeypatch.setattr(bars, "samples", lambda ds: takes)
    tracks, tols = bars.derive_tracks(takes)
    calls = []
    real = bars.derive_tracks
    monkeypatch.setattr(bars, "derive_tracks", lambda t, **kw: calls.append(len(t)) or real(t, **kw))
    try:
        fit = bars.Fit("zz")
    except Exception:  # a later stage may need more than these takes: the tracks are derived before it
        fit = None
    assert calls == [len(takes)], "the fit derived its tracks from its takes before anything else read them"
    if fit is not None:
        assert fit.auto_tracks and fit.tracks == tracks and fit.tols == tols and fit.tied == {}


# the committed tracks the derived ones find: 25 of 36 on 2026-10-14 (the balafon none of 3, the
# chimes not their two below the reference); the test fails below it
RECORDED_FOUND = 25


def test_derived_tracks_find_the_committed_ones():
    # each committed bars instrument's tracks, set by hand, against those derived from its own takes:
    # a committed track is found when one derived track lies within its tolerance on two thirds of
    # the keys where the committed one is in a take (needs the analysis caches: skipped without)
    import bars
    projects, _, _ = P.index()
    found, total, rows = 0, 0, []
    for name in sorted(projects):
        if projects[name].get("project", {}).get("family") != "bars":
            continue
        part = P.main_part(name)
        cfg = bars.config(part)
        if cfg["tracks"] == "auto":
            continue                # derived: nothing set by hand to compare
        try:
            sus = [s for ds in cfg["sets"] for s in bars.samples(ds)]
        except (FileNotFoundError, KeyError):
            continue
        derived, dtol = bars.derive_tracks(sus)
        assert bars.missing_tracks(sus, derived, dtol) == [], f"{name}: a derived track the fit cannot use"
        for s in sus:
            picked = [id(m) for j in range(1, len(derived)) if (m := bars.track(s, j, derived, dtol))]
            assert len(picked) == len(set(picked)), f"{name}: two derived tracks pick one peak at key {s['note']}: {derived}"
        hit = []
        for j in range(1, len(cfg["tracks"])):
            notes = sorted({s["note"] for s in sus if bars._usable(bars.track(s, j, cfg["tracks"], cfg["tol"]))})
            ok = bool(notes) and any(
                sum(abs(bars.nominal(d, n) / bars.nominal(cfg["tracks"][j], n) - 1) < cfg["tol"][j] for n in notes) >= 2 * len(notes) / 3
                for d in derived[1:])
            hit.append(ok)
        found += sum(hit)
        total += len(hit)
        rows.append(f"{name}: {sum(hit)} of {len(hit)} {''.join('+' if h else '-' for h in hit)}, derived {derived[1:]}")
    if not rows:
        pytest.skip("no bars analysis cache")
    print("\n".join(rows))
    assert found >= RECORDED_FOUND, f"{found} of {total} found, {RECORDED_FOUND} recorded\n" + "\n".join(rows)


def test_a_bars_high_modes_count_where_they_can_be_measured():
    # the marimba: its strong third and fourth modes (10 and 18.7) only in the bass and middle,
    # where they lie within the analysis's range; its slow whole-number lines (2, 3) are harmonics
    import bars
    takes = []
    for i, n in enumerate(range(40, 80, 2)):
        modes = [_mode(1.0, -10, decay=5.0), _mode(4.0, -22, decay=40.0, harmonic=True),
                 _mode(2.0, -40, decay=6.0, harmonic=True), _mode(3.0, -34, decay=7.0, harmonic=True)]
        if i < 9:
            modes.append(_mode(10.0, -12, decay=80.0, harmonic=True))
        if i < 4:
            modes.append(_mode(18.7, -15, decay=120.0))
        takes.append({"note": n, "modes": modes})
    tracks, _ = bars.derive_tracks(takes)
    got = sorted(round(bars.nominal(t, 40), 1) for t in tracks[1:])
    assert got == [4.0, 10.0, 18.7], tracks


def test_every_derived_track_is_one_the_fit_can_use():
    # a mode whose decay only the short window caught (conf 0 everywhere): the fit's curves need a
    # decay fit on some keys, so it is no track
    import bars
    takes = [{"note": n, "modes": [_mode(1.0), _mode(2.7), {**_mode(5.3, conf=0.0, decay=None), "fast_decay_db_s": 300.0}]}
             for n in (79, 81, 84, 86)]
    tracks, tols = bars.derive_tracks(takes)
    assert len(tracks) == 2 and bars.missing_tracks(takes, tracks, tols) == [], tracks


def test_a_keys_takes_a_little_apart_are_one_track():
    # two takes per key with the mode at 2.70 and 2.74 (1.5 % apart: the bells' takes differ by up
    # to 6 %): one track, not two that pick the same peak
    import bars
    takes = [{"note": n, "modes": [_mode(1.0), _mode(r)]} for n in (72, 74, 76, 77) for r in (2.70, 2.74)]
    tracks, tols = bars.derive_tracks(takes)
    assert len(tracks) == 2, tracks


def test_no_two_derived_tracks_pick_one_peak():
    # a mode split in two tracks would be rendered twice (the fit takes each track's strongest peak
    # within its tolerance): the glockenspiel's 5.2 and 5.3, 2 % apart
    import bars
    takes = [{"note": n, "modes": [_mode(1.0), _mode(2.7), _mode(5.19, -25), _mode(5.27, -28)]} for n in range(79, 99, 2)]
    tracks, tols = bars.derive_tracks(takes)
    for s in takes:
        picked = [id(m) for j in range(1, len(tracks)) if (m := bars.track(s, j, tracks, tols))]
        assert len(picked) == len(set(picked)), (s["note"], tracks)


def test_modes_below_the_reference_are_tracks_where_they_may_be():
    # the tubular bells' modes at 0.31 and 0.61 of the reference (lower_free, ref_ratio 2): kept for
    # such an instrument, not for a bar's (a stray low peak there is noise)
    import bars
    takes = [{"note": n, "modes": [_mode(1.0), _mode(0.61, -15), _mode(2.7)]} for n in (60, 62, 64, 65)]
    low, _ = bars.derive_tracks(takes, lower=True)
    assert any(abs(bars.nominal(t, 60) - 0.61) < 0.01 for t in low[1:]), low
    plain, _ = bars.derive_tracks(takes)
    assert all(bars.nominal(t, 60) > 1.15 for t in plain[1:]), plain


def test_auto_tracks_without_an_upper_mode_say_the_rule(monkeypatch):
    # tracks = "auto" and no mode held: the message states the rule (a fifth of the keys, three at
    # least) and does not blame a template
    import bars
    takes = [{"note": n, "dynamic": "mf", "tuning_cents": 0.0, "modes": [_mode(1.0)]} for n in (79, 81, 84, 86)]
    cfg = {"tracks": "auto", "scale_cutoff": True, "sets": ["s"], "mallets": ["m"], "damp": None,
           "mallet": {"speeds": {"pp": 0.25, "mf": 1.0, "ff": 3.0}, "f_q": 3000.0, "fq_oct_per_step": 0.5, "order": 2.0, "beta": 0.3}}
    monkeypatch.setattr(bars, "config", lambda profile: cfg)
    monkeypatch.setattr(bars, "samples", lambda ds: takes)
    with pytest.raises(SystemExit) as e:
        bars.Fit("zz")
    text = str(e.value)
    assert "a fifth of the keys" in text and "half" not in text and "template" not in text, text
    assert "key 79" in text, "the takes' modes listed"


def test_a_strike_far_over_its_neighbors_is_capped():
    # the hand chimes' A5 take: its clapper's hit at 1151 Hz as loud as the fundamental (−10.8 dB
    # re it in the fit), the tubes around it −26 to −38 dB; lowered as a whole to its four nearest
    # takes' median plus STRIKE_OUTLIER_DB. The bass's louder strikes (C4 8 dB over its neighbors)
    # follow the register and stay
    out = {60: [(1.3, 20.0, -13.5)], 62: [(1.3, 20.0, -22.1)], 64: [(1.3, 20.0, -19.8)], 66: [(1.3, 20.0, -20.9)],
           68: [(1.3, 20.0, -22.4)], 76: [(2.0, 30.0, -31.4)], 78: [(2.0, 30.0, -30.7)], 80: [(2.0, 30.0, -26.4)],
           81: [(1.29, 7.0, -10.8), (6.54, 46.0, -21.8)], 84: [(2.0, 30.0, -38.3)], 86: [(2.0, 30.0, -28.3)], 90: []}
    capped = bars.cap_strike(out)
    line = float(np.median([-30.7, -26.4, -38.3, -28.3])) + bars.STRIKE_OUTLIER_DB
    assert capped[81][0][2] == pytest.approx(line)
    # the take's modes lowered alike, their ratios and losses kept
    assert capped[81][1] == pytest.approx((6.54, 46.0, -21.8 - (-10.8 - line)))
    assert {n: rows for n, rows in capped.items() if n != 81} == {n: rows for n, rows in out.items() if n != 81}


def test_the_mallet_cuts_no_fundamental_deeply():
    """The cutoff's floor: at most FUNDAMENTAL_CUT_DB off the fundamental at mf, on every bar of
    every exported bar instrument (a deeper cut, made up by the modes' levels, came back as
    +40 dB under a harder mallet)."""
    import json
    import os
    f1 = np.array([200.0, 2000.0, 4000.0])
    sc = bars.cutoff_scale(f1, 0.0, 3000.0, 6.0)
    cut = -bars.mallet_db(f1, 0.0, 3000.0, 0.0, 6.0, sc)
    assert sc[0] == 1.0 and sc[2] > 1.0 and np.all(cut <= bars.FUNDAMENTAL_CUT_DB + 1e-9)
    root = os.path.join(os.path.dirname(__file__), "..", "..", "instruments")
    seen = 0
    for d in sorted(os.listdir(root)):
        path = os.path.join(root, d, "instrument.json")
        if not os.path.exists(path):
            continue
        doc = json.load(open(path))
        for n in doc.get("notes", []):
            e = n["excitation"]
            if e.get("type") == "pluck" and e.get("bar"):
                seen += 1
                c = 10 * e["order"] * np.log10(1 + (n["string"]["f0"] / e["f_q"]) ** 2)
                assert c <= bars.FUNDAMENTAL_CUT_DB + 1e-6, (d, n["note"], c)
    if seen == 0:
        pytest.skip("no bar instruments exported")
