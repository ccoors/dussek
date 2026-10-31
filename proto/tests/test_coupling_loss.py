"""The coupling fit's loss stage (fit_coupling.fit_high_loss): the strings' high-frequency loss (β3 at the
β1 anchors, the loss law's exponent q) fitted to the partials' energy decay up to the 60th, the
board's coupling in the model (design 2026-10-20-bass-high-band)."""
import numpy as np
import pytest

import fit_coupling as FC
import scale


def test_global_to_note_takes_the_beta3_anchors():
    g = {**scale.Profile("grand_a").coupling(), "beta3@24": 0.5, "beta3@48": 1.0}
    x = FC.global_to_note(g, 36, "grand")
    assert np.exp(x[1]) == pytest.approx(np.sqrt(0.5))
    assert np.exp(FC.global_to_note({k: v for k, v in g.items() if not k.startswith("beta3@")}, 36, "grand")[1]) == 1.0


def test_only_opted_in_hammer_pianos_fit_the_loss(monkeypatch):
    real = FC.scale.strings
    monkeypatch.setattr(FC.scale, "strings", lambda ds: {**real(ds), "high_loss": True})
    assert FC.fits_high_loss("grand_a") and not FC.fits_high_loss("harp")
    monkeypatch.setattr(FC.scale, "strings", lambda ds: {k: v for k, v in real(ds).items() if k != "high_loss"})
    assert not FC.fits_high_loss("grand_a")


def test_the_loss_stage_needs_high_partials():
    # rows that stop at the 12th partial: nothing to fit, the law as it was
    t = np.arange(0.05, 2.0, 0.01)
    cache = {36: [(n, 65.4 * n, t, -10.0 * t, 2.0, None, None, 0.05, None, -20.0) for n in range(1, 13)]}
    factors, before, after = FC.fit_high_loss(None, [36], {"loss_q": 2.0}, {}, cache=cache)
    assert factors == {"beta3@24": 1.0, "beta3@48": 1.0, "beta3@72": 1.0, "beta3@96": 1.0, "loss_q": 2.0}
    assert before == after


def test_note_params_takes_the_exponent_from_g():
    prof = scale.Profile("grand_a")
    g = {**prof.coupling(), "loss_q": 1.4}
    p = FC.note_params(prof, 36, FC.global_to_note(g, 36, prof.design), g)
    assert p.loss_q == 1.4


def test_the_stage_alone_hands_its_rows_to_the_fit(monkeypatch):
    # the rows live in the workers' _g (the pool's initializer), not in the caller's: the caller
    # passes them (the stage once saw none and kept the law as it was)
    seen = {}

    def fake_fit(pool, notes, g, old, cache=None):
        seen["cache"] = cache
        return {}, None, None

    class NoPool:
        def __init__(self, *a, **k): pass
        def __enter__(self): return self
        def __exit__(self, *a): return False
        def map(self, f, xs): return [f(x) for x in xs]

    monkeypatch.setattr(FC, "fit_high_loss", fake_fit)
    monkeypatch.setattr(FC, "Pool", NoPool)
    monkeypatch.setattr(FC.FH.P, "update_fit", lambda *a, **k: None)
    FC.main_high_loss("grand_a", [36])
    assert seen["cache"] is not None and max(r[0] for r in seen["cache"][36]) > FC.N_MAX
