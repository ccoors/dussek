"""The hammer's impact on the board (the pianos' thump): each strike's felt force, lowpassed and
scaled, on the bridge with the strings' force (soundboard.impact_force, fit_knock.py)."""
import numpy as np

import soundboard as SB

SR = 48000


def pulse(ms=3.0, n=4800):
    t = np.arange(n) / SR
    return np.where(t < ms / 1000, np.sin(np.pi * t / (ms / 1000)) ** 2, 0.0)


def test_the_impact_is_the_felt_force_scaled_by_the_strike():
    f = pulse()
    im = {"gain": 0.5, "cutoff_hz": None, "growth": 0.5}
    y = SB.impact_force(im, f, velocity=1.25, ff_speed=5.0)
    assert np.allclose(y, 0.5 * (1.25 / 5.0) ** 0.5 * f)


def test_its_lowpass_keeps_the_low_band_and_cuts_above():
    f = np.zeros(1 << 14)
    f[0] = 1.0
    y = SB.impact_force({"gain": 1.0, "cutoff_hz": 500.0, "growth": 0.0}, f, velocity=1.0, ff_speed=1.0)
    spec = np.abs(np.fft.rfft(y))
    hz = np.fft.rfftfreq(len(y), 1 / SR)
    at = lambda x: spec[np.argmin(np.abs(hz - x))]
    assert abs(20 * np.log10(at(50.0))) < 0.5
    assert abs(20 * np.log10(at(500.0)) + 3.0) < 0.5
    assert 20 * np.log10(at(2000.0)) < -20


def test_a_render_carries_the_felt_force():
    import scale
    import os
    os.environ["SOUNDBOARD_MODE"] = "full"
    p = SB.note_params(scale.Profile("grand_a"), 84)
    _, info = scale.M.render(p, 2.0, duration=0.1)
    f = info["force"]
    assert f.max() > 0 and np.all(f >= 0) and len(f) < 0.02 * SR


def test_the_projects_trim_is_read_per_key():
    # [impact] trim_db: (key, dB) anchors set by ear, linear between, held beyond (step 128)
    import export as X
    trim = {"trim_db": [[60, -10.0], [84, -6.0]]}
    assert X.impact_trim_db(trim, 21) == -10.0 and X.impact_trim_db(trim, 60) == -10.0
    assert np.isclose(X.impact_trim_db(trim, 72), -8.0)
    assert X.impact_trim_db(trim, 108) == -6.0
    assert X.impact_trim_db({"trim_db": [[21, -12.0]]}, 90) == -12.0
    assert X.impact_trim_db({}, 60) == 0.0


def test_the_export_writes_the_trim_only_where_there_is_one():
    import export as X
    import project as P
    import scale
    prof = scale.Profile("grand_a")
    trim = X.impact_trim_db(P.section("grand_a", "impact"), 84)
    doc = X.export_note(prof, 84, X.Blob(), prof.hammer_speeds(), {84: {"pp": 1, "mf": 1, "ff": 1}}, None)
    assert doc.get("impact_db", 0.0) == trim
