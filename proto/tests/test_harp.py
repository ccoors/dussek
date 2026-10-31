import os

import numpy as np
import pytest

import damper
import datasets as D
import fit_coupling as FC
import fit_pluck as FP
import scale
import string_model as M

HAVE_HARP = bool(D.items("harp"))


@pytest.mark.skipif(not HAVE_HARP, reason="VCSL's Concert Harp not present")
def test_the_harp_loads_f_and_mf_by_file_name():
    items = D.items("harp")
    notes = sorted({i["note"] for i in items})
    assert len(notes) == 23 and (notes[0], notes[-1]) == (28, 101)          # E1–F7
    assert {i["dynamic"] for i in items} == {"mf", "f"} and len(items) == 42
    # E1 has f only; G1's other take is mp, D7's and F7's p: no mf
    assert {i["note"] for i in items if i["dynamic"] == "mf"}.isdisjoint({28, 31, 98, 101})
    c3 = [i for i in items if i["note"] == 48]                               # C3: takes f2, mf3
    assert sorted(os.path.basename(i["path"]) for i in c3) == ["KSHarp_C3_f2.wav", "KSHarp_C3_mf3.wav"]
    assert D.dynamics("harp") == {"mf": "mf", "ff": "f"} and D.levels_recorded("harp")


def test_harp_design_single_plucked_strings():
    assert scale.design_of("harp") == "pedal_harp" and "pedal_harp" in scale.PLUCKED
    assert all(scale.n_strings(n, "pedal_harp") == 1 for n in range(21, 109))
    L = [scale.speaking_length(n, "pedal_harp") for n in (24, 60, 103)]
    assert abs(L[2] - 0.075) < 1e-9 and 0.35 < L[1] < 0.45 and 1.5 < L[0] < 1.8
    assert scale.tension(103, "pedal_harp") < scale.tension(60, "pedal_harp") < scale.tension(24, "pedal_harp")


def test_the_hand_damps_flat_over_frequency():
    s = damper.hand_sigma([50.0, 500.0, 5000.0])
    assert np.allclose(s, s[0]) and abs(s[0] * damper.DB_PER_NEPER - damper.HAND_DB_S) < 1e-9


def test_velocity_fit_recovers_gain_and_opening():
    rng = np.random.default_rng(3)
    x = np.tile(np.arange(1, 21) / 4.0, 15)                       # f / f_q, K_Q_HARP = 4
    d = 8.0 + FP.velocity_delta_db(x, 1.0, 0.75) + rng.normal(0, 1.0, len(x))
    v = FP.fit_velocity(list(zip(x, d, np.ones(len(x)))))
    assert abs(v["gain_db_per_step"] - 8.0) < 0.6 and abs(v["fq_oct_per_step"] - 0.75) < 0.2


def test_sparse_sets_fit_the_coupling_on_their_recorded_notes(monkeypatch):
    class P:
        def __init__(self, notes):
            self.samples = [{"note": n} for n in notes]
    thirds = [28, 31, 35, 38, 41, 45, 48, 52, 55, 59, 62, 65, 69, 72]
    monkeypatch.setattr(FC.scale, "Profile", lambda d, board=False: P(thirds if d == "sparse" else range(21, 109)))
    assert FC.default_notes("dense") == list(range(24, 97, 3))
    assert FC.default_notes("sparse") == thirds                  # meets 45, 48, 69, 72 only


def test_early_excess_measures_a_two_stage_decay():
    t = np.arange(0, 3, 0.01)
    late = -10.0 * t                                             # aftersound 10 dB/s
    fast = 15.0 - 100.0 * t                                      # 15 dB above, 100 dB/s
    e = 10 * np.log10(10 ** (late / 10) + 10 ** (fast / 10))
    assert abs(FC.early_excess(t, e) - (10 * np.log10(10 ** -0.05 + 10 ** 1.0) + 0.5)) < 0.3
    assert abs(FC.early_excess(t, late)) < 1e-6


def test_the_models_early_excess_reads_the_measured_rows_fields():
    # a measured row: (n, freq, t, EDC, end, depth, depth times, window, early excess, late level);
    # the late level (step 127) came after the excess, and a read from the end took it for the excess
    p = single_string()
    t = np.arange(0.05, 3.0, 0.01)
    row = (1, 196.0, t, -10.0 * t, 3.0, None, None, 0.05, 6.0, -20.0)
    got = FC.model_excess(p, 2.0, [row, row[:8] + (None, -20.0)])
    fm = M.free_modes(p, 2.0)
    tg = np.arange(0, FC.LATE_S[1] + 0.3, 0.005)
    pw = sum(np.abs(c) ** 2 for c in (np.exp(np.outer(tg, fm["lam"][0, :1])) @ fm["weight"][0, :1],
                                      np.exp(np.outer(tg, fm["lam"][0, 1:])) @ fm["weight"][0, 1:]))
    hw = np.hanning(10)
    expect = FC.early_excess(tg, 10 * np.log10(np.convolve(pw, hw / hw.sum(), mode="same") + 1e-30))
    assert got[0] == pytest.approx(expect, abs=1e-9)
    assert got[1] is None                       # no excess in the recording: none in the model


def single_string(**kw):
    return M.NoteParams(f0=196.0, B=2e-4, L=0.5, T=190.0, string_mass=190.0 / (2 * 0.5 * 196.0) ** 2 * 0.5,
                        n_strings=1, detune_cents=(0.0,), b1=1.0, b3=0.0, kappa=1.0, horiz_gain=0.5,
                        excitation="pluck", pluck_beta=0.47, pluck_fq=4 * 196.0, pluck_amp=3e-3, note=55, **kw)


def test_a_single_strings_polarizations_beat_in_the_depth():
    """They meet at the microphone: detuned, they modulate the envelope (added in power,
    the depth stayed ~0 whatever the detuning)."""
    tm = np.arange(0.3, 5.0, 0.01)
    meas = [(1, 196.0, None, None, None, 1.0, tm, 0.04, None)]
    flat = FC.model_depths(single_string(horiz_detune_cents=0.0), 1.0, meas)[0]
    beat = FC.model_depths(single_string(horiz_detune_cents=4.0), 1.0, meas)[0]
    assert flat < 0.3 < 1.5 < beat


def coherent_envelope(t, s_f, s_s, g, df, ph=0.4, a=-10.0):
    z = np.exp(-s_f * t) + 10 ** (-g / 20) * np.exp(-s_s * t) * np.exp(1j * (2 * np.pi * df * t + ph))
    return a + 20 * np.log10(np.abs(z))


def test_the_coherent_fit_recovers_two_polarizations():
    import fit_polarizations as FPol
    t = np.arange(0.02, 3.0, 0.02)
    rng = np.random.default_rng(5)
    for s_f, s_s, g, df in ((2.0, 0.6, 10.0, 0.6), (3.5, 1.2, 2.0, 2.3), (1.2, 0.3, 15.0, 1.0)):
        e = coherent_envelope(t, s_f, s_s, g, df) + rng.normal(0, 0.3, len(t))
        f_, s_, g_, df_, res, res1, _ = FPol.coherent(t, e)
        assert abs(f_ - s_f) < 0.25 * s_f and abs(s_ - s_s) < 0.25 * s_s + 0.05, (s_f, s_s, f_, s_)
        assert abs(g_ - g) < 2.0 and abs(df_ - df) < 0.1 and res < res1 - 0.5, (g, g_, df, df_)


def test_partials_map_to_the_models_polarizations():
    import fit_polarizations as FPol
    hk = M.NoteParams.__dataclass_fields__["horiz_kappa"].default
    # σ_s − horiz_kappa = b1 + b3·ω², σ_f = b1 + b3·ω² + κ; level and beat weighted to the fundamental
    rows = [(n, 2 * np.pi * 200 * n, 0.5 + 1e-7 * (2 * np.pi * 200 * n) ** 2, 0.5 + 1e-7 * (2 * np.pi * 200 * n) ** 2 + 2.0,
             12.0 if n == 1 else 30.0, 3.0 if n == 1 else 9.0) for n in (1, 2, 3, 4)]
    v = FPol.per_note(rows)
    assert abs(v["b1"] - 0.5) < 1e-6 and abs(v["b3"] - 1e-7) < 1e-12 and abs(v["kappa"] - 2.0) < 1e-9
    assert 10 ** (-30 / 20) < v["horiz"] < 10 ** (-12 / 20) and 3.0 < v["hdetune"] < 9.0
    assert v["horiz"] > 10 ** (-21 / 20) and v["hdetune"] < 6.0   # the fundamental counts most
    one = FPol.per_note([(1, 1000.0, 0.4, None, None, None)])
    assert one["kappa"] == 0.0 and one["horiz"] is None and hk > 0


def test_measured_polarizations_set_the_note_params():
    p = scale.Profile.__new__(scale.Profile)
    p.traits = {"fingered"}
    p.felt_fit = {"polarizations": {"notes": [21, 108], "b1": [0.2, 4.0], "b3": [0.0, 0.0], "kappa": [0.1, 5.0],
                                    "horiz": [0.3, 0.2], "hdetune": [2.0, 3.0]}}
    v = p.polarizations(64.5)
    assert abs(v["kappa"] - 2.55) < 1e-9 and abs(v["hdetune"] - 2.5) < 1e-9
    p.felt_fit = {}
    assert p.polarizations(60) is None


def test_low_weighted_losses_take_the_upper_partials_whole_decay():
    import fit_polarizations as FPOL
    # a fundamental at 1 /s, partials 2–4 whose slow stage is slow but whose line (both stages) is
    # b1 + b3·ω² with b3 = 1e-7: the fit's b3 from the line, its b1 net of b3·ω1²
    w1, b1, b3 = 2 * np.pi * 440.0, 1.0, 1e-7
    rows = [(1, w1, b1 + b3 * w1 ** 2, None, None, None, b1 + b3 * w1 ** 2)]
    for n in (2, 3, 4):
        w = n * w1
        rows.append((n, w, 0.3 * (b1 + b3 * w ** 2), 2.0 * (b1 + b3 * w ** 2), 10.0, 1.0, b1 + b3 * w ** 2))
    r = FPOL.per_note(rows, low_weighted=True)
    assert abs(r["b3"] / b3 - 1) < 0.05 and abs(r["b1"] - b1) < 0.05
