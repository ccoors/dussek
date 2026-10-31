"""The report (proto/report.py): the grands' (or uprights') traits along the keyboard, their attack's
change from mf to ff against the envelope of their kind's recordings, their tone per register
(design 2026-10-21-traits-plan, the rail)."""
import json
import os

import pytest

import report as R


def _export(root, name, body="grand", group="Grand pianos", hidden=False, notes=None, project=True, projects=None):
    d = os.path.join(root, name)
    os.makedirs(d)
    menu = {"name": name.upper(), "group": group, **({"hidden": True} if hidden else {})}
    with open(os.path.join(d, "instrument.json"), "w") as fh:
        json.dump({"format": 5, "profile": name, "body": body, "menu": menu, "notes": notes or []}, fh)
    open(os.path.join(d, "data.bin"), "wb").close()
    if project and projects:
        os.makedirs(os.path.join(projects, name))
        open(os.path.join(projects, name, "project.toml"), "w").close()
    return d


def _note(k, K=2e8, p=2.3, core_K=0.0, core_depth=0.0):
    e = {"type": "hammer", "mass": 0.005, "felt_K": K, "felt_p": p, "strike_pos": 0.12}
    if core_K:
        e.update(core_K=core_K, core_depth=core_depth)
    return {"note": k, "excitation": e,
            "string": {"B": 1e-4, "b1": 0.2, "detune_cents": [-0.5, 0.0, 0.7], "kappa": 0.4}}


def _row(note, dyn, rec, eng, noise=-100.0):
    return {"note": note, "dynamic": dyn, "rec": {"attack": list(rec), "noise": [noise] * 6},
            "eng": {"attack": list(eng)}}


def test_traits_of_a_note(tmp_path):
    d = _export(str(tmp_path), "g", notes=[_note(36, core_K=4e8, core_depth=2e-4), _note(60)])
    t = R.traits(d)
    assert set(t) == set(R.TRAITS)
    assert t["felt05"][0] == [36, pytest.approx(2e8 * 5e-4 ** 2.3, rel=1e-3)]
    assert t["core05"][0] == [36, pytest.approx(4e8 * 5e-4 ** 2.3, rel=1e-3)] and t["core05"][1] == [60, 0.0]
    assert t["core_depth"] == [[36, pytest.approx(0.2)]]
    assert t["hammer_mass"][0] == [36, pytest.approx(5.0)]
    assert t["detune"][0] == [36, pytest.approx(1.2)]


def test_pianos_are_picked_by_their_data(tmp_path):
    inst, proj = str(tmp_path / "instruments"), str(tmp_path / "projects")
    os.makedirs(proj)
    _export(inst, "yes", projects=proj)
    _export(inst, "upright", body="upright", group="Upright pianos", projects=proj)
    _export(inst, "upright_elsewhere", body="upright", projects=proj)
    _export(inst, "hidden", hidden=True, projects=proj)
    _export(inst, "noproject", projects=None)
    assert R.pianos("grand", inst, proj) == ["yes"]
    assert R.pianos("upright", inst, proj) == ["upright"]


def test_spread_drops_bands_near_the_hiss():
    rows = [_row(36, "mf", [-10, -20, -60, -80], [-12, -25, -70, -90]),
            _row(36, "ff", [-5, -10, -40, -60], [-6, -12, -45, -66])]
    rows[0]["rec"]["noise"] = [-100, -100, -65, -100, -100, -100]     # mf 4–6 kHz 5 dB over its hiss
    rec, eng = R.spread(rows, "rec"), R.spread(rows, "eng")
    assert rec[36][:2] == [pytest.approx(5), pytest.approx(10)] and rec[36][2] is None
    assert eng[36][2] == pytest.approx(25)


def test_error_is_the_engine_against_its_recording():
    rows = [_row(36, "mf", [-10, -20, -60, -80], [-12, -25, -70, -90])]
    rows[0]["rec"]["noise"] = [-100, -100, -65, -100, -100, -100]
    e = R.error(rows)
    assert e["mf"][36][:2] == [pytest.approx(-2), pytest.approx(-5)] and e["mf"][36][2] is None


def test_envelope_spans_the_grands_between_their_keys():
    env = {e[0]: e for e in R.envelope([{21: [0.0], 27: [6.0]}, {21: [2.0], 24: [4.0]}], 0)}
    assert env[21] == (21, 0.0, 2.0, 2) and env[24] == (24, pytest.approx(3.0), 4.0, 2)
    assert sorted(env) == [21, 22, 23, 24]       # 25–27: one grand only


def test_envelope_skips_missing_values():
    env = R.envelope([{21: [None], 24: [1.0]}, {21: [2.0], 24: [3.0]}, {21: [5.0], 24: [None]}], 0)
    assert env == [(21, 2.0, 5.0, 2), (24, 1.0, 3.0, 2)]


def test_outside_lists_only_keys_beyond_the_band():
    env = [(21, 0.0, 2.0, 2), (24, 3.0, 4.0, 2)]
    assert R.outside(env, {21: [1.0], 24: [6.0], 27: [9.0]}, 0) == [(24, 6.0, 3.0, 4.0)]


def _measure(project, instrument, params, workdir, notes=None):
    shift = 3.0 if "@" in os.path.basename(instrument) else 0.0
    return [_row(21, "mf", [-10, -20, -60, -80], [-10 + shift, -20, -60, -80]),
            _row(21, "ff", [-5, -10, -40, -60], [-2, -5, -30, -50]),
            _row(24, "mf", [-10, -20, -60, -80], [-10, -20, -60, -80]),
            _row(24, "ff", [-4, -12, -45, -61], [-4, -12, -45, -61])]


def test_collect_without_a_snapshot_for_every_grand(tmp_path, monkeypatch):
    inst, proj, snaps = str(tmp_path / "instruments"), str(tmp_path / "projects"), str(tmp_path / "snaps")
    os.makedirs(proj)
    for n in ("a", "b"):
        _export(inst, n, notes=[_note(21)], projects=proj)
    _export(snaps, "a@t-1", notes=[_note(21, K=3e8)])
    monkeypatch.setattr(R, "measure", _measure)
    monkeypatch.setattr(R, "levels", lambda d: {})
    monkeypatch.setattr(R, "tone", lambda name, d, w: {"registers": {}, "lowest": {}, "dir": d})
    monkeypatch.setattr(R, "board", lambda name: {})
    monkeypatch.setattr(R, "in_envelope", lambda name: True)
    monkeypatch.setattr(R, "traits_measured", lambda name: {})
    data = R.collect(["a", "b"], ["t/1"], str(tmp_path / "work"), instruments=inst, snapshots=snaps)
    g = {x["id"]: x for x in data["pianos"]}
    assert list(g["a"]["snapshots"]) == ["t/1"] and g["b"]["snapshots"] == {}
    assert g["a"]["snapshots"]["t/1"]["eng"][21][0] != g["a"]["eng"][21][0]
    assert data["snapshots"] == ["t/1"]
    assert data["envelopes"][0][0][3] == 2


def test_a_grand_out_of_the_envelope_still_has_its_lines(tmp_path, monkeypatch):
    inst, proj = str(tmp_path / "instruments"), str(tmp_path / "projects")
    os.makedirs(proj)
    for n in ("a", "b", "c"):
        _export(inst, n, notes=[_note(21)], projects=proj)
    monkeypatch.setattr(R, "measure", _measure)
    monkeypatch.setattr(R, "levels", lambda d: {})
    monkeypatch.setattr(R, "tone", lambda name, d, w: {"registers": {}, "lowest": {}, "dir": d})
    monkeypatch.setattr(R, "board", lambda name: {})
    monkeypatch.setattr(R, "in_envelope", lambda name: name != "c")
    monkeypatch.setattr(R, "traits_measured", lambda name: {})
    data = R.collect(["a", "b", "c"], [], str(tmp_path / "work"), instruments=inst, snapshots=str(tmp_path / "none"))
    g = {x["id"]: x for x in data["pianos"]}
    assert g["c"]["in_envelope"] is False and g["c"]["rec"] and g["c"]["eng"]
    assert all(e[3] == 2 for e in data["envelopes"][0])


def test_the_page_carries_the_data(tmp_path):
    path = R.write_page({"pianos": []}, str(tmp_path / "out"))
    text = open(path).read()
    assert path.endswith("index.html") and '"pianos": []' in text and "/*DATA*/" not in text


def test_two_snapshots_one_missing(tmp_path, monkeypatch):
    inst, proj, snaps = str(tmp_path / "instruments"), str(tmp_path / "projects"), str(tmp_path / "snaps")
    os.makedirs(proj)
    for n in ("a", "b"):
        _export(inst, n, notes=[_note(21)], projects=proj)
    _export(snaps, "a@t-1", notes=[_note(21)])
    _export(snaps, "b@even", notes=[_note(21)])
    monkeypatch.setattr(R, "measure", _measure)
    monkeypatch.setattr(R, "levels", lambda d: {"ff": [[21, 0.0]]})
    monkeypatch.setattr(R, "tone", lambda name, d, w: {"registers": {}, "lowest": {}, "dir": d})
    monkeypatch.setattr(R, "board", lambda name: {"H_db": [0.0]})
    monkeypatch.setattr(R, "in_envelope", lambda name: True)
    monkeypatch.setattr(R, "traits_measured", lambda name: {})
    data = R.collect(["a", "b"], ["t/1", "even"], str(tmp_path / "work"), instruments=inst, snapshots=snaps)
    g = {x["id"]: x for x in data["pianos"]}
    assert list(g["a"]["snapshots"]) == ["t/1"] and list(g["b"]["snapshots"]) == ["even"]
    assert g["b"]["snapshots"]["even"]["levels"] == {"ff": [[21, 0.0]]} and g["a"]["levels"] == {"ff": [[21, 0.0]]}
    assert g["a"]["board"] == {"H_db": [0.0]}
    assert g["b"]["snapshots"]["even"]["tone"]["dir"].endswith("b@even") and g["b"]["tone"]["dir"].endswith(os.path.join("instruments", "b"))


def test_levels_are_re_the_median():
    doc = {"key_ff_db": [float(k) for k in range(88)],
           "speed_db": {"keys": list(range(21, 109)), "speed": [0.078, 0.156, 0.3125, 0.625, 1.25, 2.5, 5.0, 10.0],
                        "db": [[0.0] * 8 for _ in range(88)]}}
    lv = R.levels_of(doc, [0.4, 1.5, 5.0])
    assert set(lv) == {"ff"}
    for rows in lv.values():
        assert [r[0] for r in rows] == list(range(21, 109))
        assert sorted(r[1] for r in rows)[44] == pytest.approx(0.5, abs=0.51)
    assert R.levels_of({"ff_db": -20.0}, [0.4, 1.5, 5.0]) == {}


def test_the_board_from_the_fit():
    # a committed grand's soundboard fit: the response over its grid, the data range, the correction rows
    try:
        b = R.board("grand_a")
    except FileNotFoundError:
        pytest.skip("no fit")
    assert len(b["grid_hz"]) == len(b["H_db"]) and len(b["range_hz"]) == 2
    assert len(b["C_db"]) == len(b["corr_notes"]) and all(len(r) == len(b["grid_hz"]) for r in b["C_db"])
    assert max(b["H_db"]) == pytest.approx(0.0, abs=1e-6)


def test_traits_mark_where_the_fit_extrapolates():
    # the loss (and the coupling, its multiple) measured on keys up to 96; B on every recorded key
    assert R.measured_ranges([21, 30, 96], [21, 60, 105]) == {"b1": [21, 96], "kappa": [21, 96], "B": [21, 105]}
    assert R.measured_ranges([], []) == {}


def test_traits_measured_only_forgives_a_missing_analysis(monkeypatch):
    monkeypatch.setattr(R.P, "main_part", lambda name: name)

    def missing(dataset):
        raise FileNotFoundError(dataset)

    monkeypatch.setattr(R.scale, "Profile", missing)
    assert R.traits_measured("a") == {}

    def broken(dataset):
        raise KeyError("loss_notes")

    monkeypatch.setattr(R.scale, "Profile", broken)
    with pytest.raises(KeyError):
        R.traits_measured("a")


def test_collect_carries_the_measured_ranges(tmp_path, monkeypatch):
    inst, proj = str(tmp_path / "instruments"), str(tmp_path / "projects")
    os.makedirs(proj)
    _export(inst, "a", notes=[_note(21)], projects=proj)
    monkeypatch.setattr(R, "measure", _measure)
    monkeypatch.setattr(R, "levels", lambda d: {})
    monkeypatch.setattr(R, "tone", lambda name, d, w: {"registers": {}, "lowest": {}, "dir": d})
    monkeypatch.setattr(R, "board", lambda name: {})
    monkeypatch.setattr(R, "in_envelope", lambda name: True)
    monkeypatch.setattr(R, "traits_measured", lambda name: {"b1": [21, 96]})
    data = R.collect(["a"], [], str(tmp_path / "work"), instruments=inst, snapshots=str(tmp_path / "none"))
    assert data["pianos"][0]["measured"] == {"b1": [21, 96]}
    page = open(R.write_page(data, str(tmp_path / "out"))).read()
    assert "g.measured" in page, "the page draws the measured ranges"


def test_a_ref_no_grand_has_is_named(tmp_path):
    snaps = str(tmp_path)
    _export(snaps, "a@v1")
    assert R.unknown_refs(["a", "b"], ["v1", "v2", "feature/x"], snaps) == ["v2", "feature/x"]
    _export(snaps, "b@feature-x")
    assert R.unknown_refs(["a", "b"], ["v1", "feature/x"], snaps) == []
