"""analysis/pedal_noise.py: a set's own pedal recordings, declared in its source, or Salamander's."""
import pedal_noise as PN


def test_a_sets_own_pedal_recordings_are_read_as_recorded(monkeypatch):
    monkeypatch.setattr(PN.P, "source", lambda pid: {"pedal": {"kind": "files", "down": ["pedal/down_1.wav", "pedal/down_2.wav"],
                                                               "up": ["pedal/up_1.wav"]}})
    monkeypatch.setattr(PN.D, "root", lambda pid: "/set")
    assert PN.takes("own") == [("down", "down_1", "/set/pedal/down_1.wav", 0.0), ("down", "down_2", "/set/pedal/down_2.wav", 0.0),
                               ("up", "up_1", "/set/pedal/up_1.wav", 0.0)]


def test_without_them_the_takes_are_salamanders_in_its_mix(monkeypatch):
    monkeypatch.setattr(PN.P, "source", lambda pid: {})
    monkeypatch.setattr(PN.D, "root", lambda pid: "/set")
    assert PN.takes("lender") == [("down", "pedalD1", "/set/pedalD1.wav", -20.0), ("down", "pedalD2", "/set/pedalD2.wav", -20.0),
                                  ("up", "pedalU1", "/set/pedalU1.wav", -19.0), ("up", "pedalU2", "/set/pedalU2.wav", -19.0)]


def test_a_take_far_over_its_kinds_median_is_named():
    out = {"down": [{"name": n, "ratio_db": r} for n, r in (("d1", -22.1), ("d2", -12.7), ("d3", -10.1), ("d4", -4.5))],
           "up": [{"name": "u1", "ratio_db": -31.4}, {"name": "u2", "ratio_db": -22.0}, {"name": "u3", "ratio_db": -23.8}]}
    assert [(k, n) for k, n, _ in PN.loud_takes(out)] == [("down", "d4")]
    # Salamander's two takes each way, and the two normal presses alone: none
    assert PN.loud_takes({"down": [{"name": "a", "ratio_db": -21.8}, {"name": "b", "ratio_db": -27.7}],
                          "up": [{"name": "c", "ratio_db": -28.3}, {"name": "d", "ratio_db": -34.0}]}) == []
    assert PN.loud_takes({"down": out["down"][:2], "up": []}) == []
