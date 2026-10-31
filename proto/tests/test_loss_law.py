"""The strings' loss law σ = b1 + b3·ω_ref²·(ω/ω_ref)^q (string_model.string_loss): q = 2 is the law
as it was; a fitted q below 2 lets the bass's high partials ring as the recordings' do (design
2026-10-20-bass-high-band). The engine pins the same table (physics.rs)."""
import numpy as np
import pytest

import scale
import string_model as M

# (b1, b3, f, q) → σ (1/s)
LOSS_TABLE = [((0.5, 1e-9, 500, 2.0), 0.509869604), ((0.5, 1e-9, 500, 1.5), 0.513957728),
              ((0.5, 1e-9, 4000, 1.5), 0.815827341), ((0.2, 2e-9, 8000, 1.0), 0.831654682),
              ((0.2, 2e-9, 1000, 1.3), 0.278956835)]


def test_the_loss_law_follows_its_table():
    for (b1, b3, f, q), want in LOSS_TABLE:
        assert M.string_loss(b1, b3, 2 * np.pi * f, q) == pytest.approx(want, abs=1e-9)


def test_q_two_is_todays_law():
    w = 2 * np.pi * np.array([100.0, 1000.0, 9000.0])
    assert np.allclose(M.string_loss(0.3, 1.5e-9, w), 0.3 + 1.5e-9 * w ** 2, rtol=1e-12)


def test_the_profile_reads_the_loss_anchors(monkeypatch):
    prof = scale.Profile("grand_a")
    b3p = prof.loss(36)[1]
    cp = {**prof.coupling(), "beta3@24": 0.5, "beta3@48": 0.5, "beta3@72": 0.5, "beta3@96": 0.5, "loss_q": 1.5}
    monkeypatch.setattr(prof, "coupling", lambda: cp)
    p = scale.note_params(prof, 36)
    assert p.b3 == pytest.approx(0.5 * b3p) and p.loss_q == 1.5
    assert scale.beta3_factor({}, 36) == 1.0
