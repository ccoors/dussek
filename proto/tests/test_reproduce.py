import reproduce as R


def test_identical_exports_differ_in_nothing():
    doc = {"notes": [{"note": 21, "level_db": [[1.0, 2.0, 3.0]], "release": {"peak": [2.0, 3.0, 4.0]}}]}
    assert R.differences(doc, doc) == []


def test_differences_name_the_field_in_db():
    a = {"notes": [{"release": {"peak": [1.0, 2.0]}, "level_db": [[0.5]]}], "ff_db": -15.0}
    b = {"notes": [{"release": {"peak": [1.0, 2.0 * 10 ** (1.5 / 20)]}, "level_db": [[0.7]]}], "ff_db": -15.0}
    rows = {path: (n, d) for path, unit, n, d in R.differences(a, b)}
    assert set(rows) == {"/notes/release/peak", "/notes/level_db"}
    assert rows["/notes/release/peak"][0] == 1 and abs(rows["/notes/release/peak"][1] - 1.5) < 1e-9
    assert abs(rows["/notes/level_db"][1] - 0.2) < 1e-9


def test_a_missing_or_changed_key_is_a_difference():
    rows = R.differences({"a": 1, "knock": None}, {"a": 1, "knock": {"q": 2}, "extra": True})
    assert {r[0] for r in rows} == {"/knock", "/extra"}


def test_rounding_is_no_difference():
    # the peaks' sums come back in another order now and then: 1e-14 dB is the same export
    a = {"notes": [{"release": {"peak": [1.0]}, "level_db": [[0.5]]}], "gain": 2.0}
    b = {"notes": [{"release": {"peak": [1.0 + 1e-15]}, "level_db": [[0.5 + 1e-9]]}], "gain": 2.0 * (1 + 1e-12)}
    assert R.differences(a, b) == []


def test_a_project_not_built_here_is_not_reproduced(monkeypatch, capsys):
    # a fresh clone has no caches but the lent ones: its exports cannot be made again there, which is
    # no difference (as `just check` passes over it)
    import build
    monkeypatch.setattr(build, "_built_here", lambda n: n == "grand_a")
    checked = []
    monkeypatch.setattr(R, "check", lambda name, scratch: checked.append(name) or [])
    monkeypatch.setattr("sys.argv", ["reproduce.py", "grand_a", "grand_b"])
    try:
        R.main()
    except SystemExit as e:
        assert e.code == 0
    out = capsys.readouterr().out
    assert checked == ["grand_a"]
    assert "grand_b: not built here" in out and "1 of 1 exports reproduce" in out
