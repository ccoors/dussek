import numpy as np
import pytest

import register as R


def test_register_gain_does_not_run_away_beyond_the_recorded_notes():
    # a level error falling over the recorded range 29…89
    err = {n: [0.3 * (60 - n)] for n in range(29, 90, 2)}
    anchors = np.arange(21, 109, 6)
    g = R.register_gain(err, anchors, 3.0)
    at = dict(zip(anchors.tolist(), g))
    # beyond the data the edge trend carries on, no steeper (the local-linear smoother had run
    # on to an extrapolated −90 dB treble)
    assert abs((at[105] - at[93]) - (-0.3 * 12)) < 0.5 and abs((at[21] - at[27]) - (0.3 * 6)) < 0.5
    assert np.all(np.isfinite(g)) and np.ptp(g) < 0.3 * (105 - 21) + 1


def test_a_normalized_set_takes_its_register_levels_from_a_reference():
    # Grand E's samples are normalized: its register balance follows the Iowa recordings
    # (the same model, every note at real levels)
    assert R.FH.D.level_reference("grand_e") == "grand_b"
    assert R.can_fit("grand_e") and R.can_fit("upright_a") and not R.can_fit("grand_a")


def test_the_reference_recording_is_the_nearest_note_of_the_same_dynamic():
    ref = {(60, "mf"): "a", (62, "mf"): "b", (61, "ff"): "c", (70, "mf"): "far"}
    assert R.reference_sample(ref, 61, "mf") in ("a", "b")
    assert R.reference_sample(ref, 62, "mf") == "b"
    assert R.reference_sample(ref, 61, "ff") == "c"
    assert R.reference_sample(ref, 66, "mf") is None     # more than 2 semitones away
    assert R.reference_sample(ref, 60, "pp") is None


def test_a_normalized_set_plays_at_its_references_hammer_speeds():
    # without recorded levels the felt fit can trade hammer speed against felt hardness;
    # free, Grand E's ff fell to 3.5 m/s, its soundboard tilted 35 dB to make up for it, and
    # the phantom partials and hammer wear came out 10 dB too strong (Condition)
    import fit_profile_felt as F
    import project as P
    ref = P.load_fit("grand_b")["hammer_speeds"]
    dyns = ["pp", "mf", "ff"]
    assert F.speed_sets("grand_e", dyns) == [tuple(ref[d] for d in dyns)]
    assert len(F.speed_sets("upright_a", ["mf", "ff"])) > 1      # its own levels: searched


def test_non_finite_model_levels_stop_the_fit():
    # one diverged render (Grand D's C7, before the contact's sub-steps) made every register
    # gain NaN through the smoothing, and the profile was written with NaN corrections
    with pytest.raises(SystemExit, match="100"):
        R.check_finite({99: [0.5, 0.1], 100: [float("nan"), 0.2], 101: [0.0]})
    R.check_finite({99: [0.5], 101: [0.0]})


def test_the_register_gain_continues_the_recorded_trend_beyond_the_range():
    # held at the last recorded note, the extrapolated keys above it grew louder by the
    # model's own trend: the Italian harpsichord (to B5) 10–20 dB above its top notes at C7–C8
    notes = list(range(30, 84))
    err = {n: [-0.6 * (n - 60)] for n in notes}
    anchors = [21, 27, 33, 63, 87, 93, 105]
    g = R.register_gain(err, anchors, frac=3.0)
    g0 = R.register_gain(err, [63], frac=3.0)    # (shifted to median 0: compare differences)
    d = dict(zip(anchors, g))
    assert abs((d[105] - d[87]) - (-0.6 * 18)) < 1.0      # the slope carried on
    assert abs((d[21] - d[33]) - (0.6 * 12)) < 1.0
    steep = {n: [-3.0 * (n - 60)] for n in notes}
    s = dict(zip(anchors, R.register_gain(steep, anchors, frac=3.0)))
    assert abs((s[105] - s[87]) - (-R.EXTRAP_SLOPE_MAX * 18)) < 1.0   # clamped
    flat = {n: [0.0] for n in notes}
    assert np.allclose(R.register_gain(flat, anchors, frac=3.0), 0.0, atol=1e-9)


def test_beyond_the_range_the_recordings_trend_minus_the_model():
    # recorded keys 40…80, recorded level flat, the model rising 0.5 dB/semitone above 80:
    # the gain falls by the model's rise there (the level stays flat), not on with its own slope
    notes = list(range(40, 81))
    err = {n: [0.3 * (n - 70) if n > 70 else 0.0] for n in notes}     # the gain rising at the top
    rec = {n: [0.0] for n in notes}
    anchors = [60, 80, 86, 92]
    model = {40: 0.0, 80: 0.0, 86: 3.0, 92: 6.0}
    g = dict(zip(anchors, R.register_gain(err, anchors, 3.0, rec, model)))
    assert abs((g[86] - g[80]) - (-3.0)) < 0.2 and abs((g[92] - g[80]) - (-6.0)) < 0.2, g
    # without the model: the gain's own trend (it carries on rising)
    g0 = dict(zip(anchors, R.register_gain(err, anchors, 3.0)))
    assert g0[92] > g0[80]


def test_beyond_the_range_the_level_never_rises():
    # the recordings rising 0.5 dB/semitone over their top octave, the model flat beyond:
    # the keys above are held at the top recorded key's level, not raised further
    notes = list(range(40, 81))
    err = {n: [0.0] for n in notes}
    rec = {n: [0.5 * (n - 68) if n > 68 else 0.0] for n in notes}
    anchors = [60, 80, 92]
    model = {40: 0.0, 80: 0.0, 92: 0.0}
    g = dict(zip(anchors, R.register_gain(err, anchors, 3.0, rec, model)))
    assert abs(g[92] - g[80]) < 1e-6, g
