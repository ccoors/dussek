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
