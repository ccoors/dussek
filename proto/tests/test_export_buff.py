import export
import project as P


def test_harpsichords_get_the_block_on_register_0():
    b = P.load_fit("harpsichord_buff")
    want = {"register": 0, "notes": [int(n) for n in b["notes"]], "flat": [float(a) for a in b["flat"]],
            "gain": [float(g) for g in b["gain"]], "pos": [float(p) for p in b["pos"]]}
    for prof in ("harpsichord", "flemish8"):
        assert export.buff_doc(prof) == want


def test_other_profiles_and_a_missing_fit_get_none(monkeypatch):
    for prof in ("grand_a", "upright_a", "flemish4"):        # flemish4: not the main register
        assert export.buff_doc(prof) is None
    monkeypatch.setattr(P, "has_fit", lambda pid: False)
    assert export.buff_doc("harpsichord") is None
