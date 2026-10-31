import numpy as np

import damper
import export
import scale


def test_plucked_instruments_damp_every_key():
    # a harpsichord's jacks carry a damper on every key; a piano's top strings ring free
    assert damper.has_damper(88) and not damper.has_damper(89) and not damper.has_damper(108)
    assert all(damper.has_damper(n, plucked=True) for n in range(21, 109))


def test_the_top_keys_get_a_finite_damper_loss():
    s = damper.sigma(100, [500.0, 2000.0, 8000.0], "harpsichord", plucked=True)
    assert np.all(np.isfinite(s)) and np.all(s > 0)
    assert np.all(damper.sigma(100, [500.0, 2000.0], "grand_a") == 0)


def export_notes(profile, keys):
    prof = scale.Profile(profile)
    blob = export.Blob()
    peaks = export.release.peak_table(prof)
    return [export.export_note(prof, n, blob, prof.hammer_speeds(), peaks, prof.felt_fit.get("knock")) for n in keys]


def test_export_damps_the_harpsichords_top_keys():
    for n in export_notes("harpsichord", (88, 89, 108)):
        assert n["has_damper"] and n["damper_sigma"] is not None, n["note"]
    for n in export_notes("grand_a", (89, 108)):
        assert not n["has_damper"] and n["damper_sigma"] is None
