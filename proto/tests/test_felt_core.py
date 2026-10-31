"""The felt's core (string_model.felt_force): F = K·δ^p + K_c·max(δ − d_c, 0)^p, a stiff core under the
felt's soft layer; the recordings' bass is already bright at mf and keeps brightening above 2 kHz at
ff, which one power law cannot do (design 2026-10-20-felt-core). The engine pins the same table
(physics.rs)."""
from dataclasses import replace

import numpy as np
import pytest

import bass_check as BC
import scale
import string_model as M

# (K, p, core_K, core_depth, delta) → F (N)
FELT_TABLE = [((1.2e8, 2.3, 0.0, 0.0, 1e-3), 15.1071049), ((1.2e8, 2.3, 5.9e8, 4e-4, 1e-3), 38.0474469),
              ((1.2e8, 2.3, 5.9e8, 4e-4, 3e-4), 0.947458359), ((3e8, 2.5, 1e9, 2e-4, 2e-3), 191.12719),
              ((1.2e8, 2.3, 5.9e8, 4e-4, -1e-4), 0.0)]


def test_the_felt_law_follows_its_table():
    for args, want in FELT_TABLE:
        assert M.felt_force(*args) == pytest.approx(want, rel=1e-8)


def _c2():
    # C2 without a core, whatever the fit holds
    return replace(scale.note_params(scale.Profile("grand_a"), 36), core_K=0.0, core_depth=0.0)


def _cored(p):
    return dict(core_K=15 / 5e-4 ** p.felt_p, core_depth=4e-4)


def test_no_core_renders_as_before():
    p = _c2()
    a, _ = M.render(p, 5.0, duration=0.3)
    b, _ = M.render(replace(p, core_K=0.0, core_depth=4e-4), 5.0, duration=0.3)
    assert np.array_equal(a, b)


def test_the_core_brightens_the_attack():
    # the spike: C2's 4–6 kHz at ff about 22 dB up with a core past 0.4 mm, 15 N at 0.5 mm past it
    p = _c2()
    a, _ = M.render(p, 5.0, duration=0.3)
    b, _ = M.render(replace(p, **_cored(p)), 5.0, duration=0.3)
    assert BC.attack_bands(b, 48000, 0)[2] > BC.attack_bands(a, 48000, 0)[2] + 10


def test_contact_ratio_counts_the_core():
    p = _c2()
    assert M.contact_ratio(replace(p, **_cored(p)), 5.0, 48000) > M.contact_ratio(p, 5.0, 48000)


TABLE = {"notes": [24, 36, 48], "f05": [20.0, 11.0, 0.0], "depth_mm": [0.2, 0.3, 0.3]}


def test_the_core_curve_of_a_project():
    # [felt_core] in the part's project, set by ear: linear between its keys, held beyond
    import export
    d, f = scale.core_at(TABLE, 30)
    assert d == pytest.approx(2.5e-4) and f == pytest.approx(15.5)
    assert scale.core_at(TABLE, 60) == (pytest.approx(3e-4), 0.0)
    assert scale.core_at({}, 36) == (0.0, 0.0)


def test_the_export_plays_the_core():
    import export
    p = _c2()
    q = scale.with_core(p, TABLE)
    assert q.core_K == pytest.approx(11.0 / 5e-4 ** p.felt_p) and q.core_depth == pytest.approx(3e-4)
    assert scale.with_core(p, {}) == p


def test_the_fits_never_see_the_core():
    # the board's fit with the core in its renders tilted Grand A's levels by ±4 dB across the keys
    # (its per-key gains moved up to 7 dB for a 0.3 dB change in the strings' level)
    assert scale.note_params(scale.Profile("grand_a"), 36).core_K == 0.0


def test_the_export_writes_the_core_only_where_there_is_one():
    import export
    p = _c2()
    assert not {"core_K", "core_depth"} & set(export.excitation_doc(p))
    doc = export.excitation_doc(replace(p, core_K=5e8, core_depth=4e-4))
    assert doc["core_K"] == 5e8 and doc["core_depth"] == 4e-4


def test_the_release_peaks_see_the_core():
    # the export scales each note's release noise by its radiated peak: with the core the engine
    # plays, as the approved export had it (C2 at ff: 41 against 27 without)
    import release
    plain = release._peak(("grand_a", 36, 5.0, None))
    cored = release._peak(("grand_a", 36, 5.0, TABLE))
    assert cored > plain * 1.1
    assert release._peak_key("grand_a", TABLE) != release._peak_key("grand_a", None)


def test_a_bad_core_table_is_refused():
    # [felt_core] is typed by hand: a negative depth would push without contact
    for bad in ({"notes": [36, 24], "f05": [1.0, 2.0], "depth_mm": [0.2, 0.2]},
                {"notes": [24, 36], "f05": [1.0], "depth_mm": [0.2, 0.2]},
                {"notes": [24, 36], "f05": [1.0, 2.0], "depth_mm": [-0.1, 0.2]},
                {"notes": [24, 36], "f05": [-1.0, 2.0], "depth_mm": [0.2, 0.2]}):
        with pytest.raises(ValueError):
            scale.core_at(bad, 30)


def test_the_release_peaks_keep_a_table_per_core(monkeypatch):
    # the export asks with the core, the pedal fit and the tests without: neither redoes the other's
    import release
    store, calls = {}, []
    has, load, write = release.P.has_cache, release.P.load_cache, release.P.write_cache
    peaks = "release_peaks"
    monkeypatch.setattr(release.P, "has_cache", lambda d, n: (d, n) in store if n == peaks else has(d, n))
    monkeypatch.setattr(release.P, "load_cache", lambda d, n: store[(d, n)] if n == peaks else load(d, n))
    monkeypatch.setattr(release.P, "write_cache",
                        lambda d, n, v: store.__setitem__((d, n), v) if n == peaks else write(d, n, v))
    monkeypatch.setattr(release, "_peak", lambda a: calls.append(a) or 1.0)

    class Sync:
        def __init__(self, *a): pass
        def __enter__(self): return self
        def __exit__(self, *a): return False
        def map(self, f, xs): return [f(x) for x in xs]
    monkeypatch.setattr(release, "Pool", Sync)
    prof = scale.Profile("grand_a")
    release.peak_table(prof, TABLE)
    release.peak_table(prof)
    n = len(calls)
    release.peak_table(prof, TABLE)
    release.peak_table(prof)
    assert len(calls) == n == 2 * 88 * 3
