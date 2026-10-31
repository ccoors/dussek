import numpy as np

import soundboard as SB

SR = 48000
FLAT = {"grid_hz": [20.0, 20000.0], "H_db": [0.0, 0.0]}
LATE = np.array([[100.0, 2.0], [10000.0, 2.0]])


def band_db(ir, lo, hi, t, win=0.05):
    """Energy (dB) of [lo, hi) Hz in [t, t + win) s, both channels."""
    seg = ir[:, int(t * SR):int((t + win) * SR)]
    spec = np.abs(np.fft.rfft(seg * np.hanning(seg.shape[1]), axis=1)) ** 2
    f = np.fft.rfftfreq(seg.shape[1], 1 / SR)
    return 10 * np.log10(spec[:, (f >= lo) & (f < hi)].sum())


def test_the_room_decays_at_its_t60():
    ir = SB.room_ir(FLAT, LATE, SR)
    rate = (band_db(ir, 700, 1400, 0.2) - band_db(ir, 700, 1400, 1.0)) / 0.8
    assert abs(rate - 30) < 5, rate


def test_eq_db_tilts_the_room():
    flat = SB.room_ir(FLAT, LATE, SR)
    tilt = SB.room_ir(FLAT, LATE, SR, eq_db=np.array([[500.0, 10.0], [4000.0, -10.0]]))
    ratio = lambda ir: band_db(ir, 3600, 4400, 0.1) - band_db(ir, 450, 550, 0.1)  # noqa: E731
    assert abs(ratio(tilt) - ratio(flat) + 20) < 3, ratio(tilt) - ratio(flat)


class _Prof:
    def __init__(self, fit):
        self.felt_fit = fit
        self.dataset = "none"


def test_the_fitted_room_keeps_its_decay(monkeypatch):
    monkeypatch.setattr(SB, "room_t60", lambda dataset: np.array([[250.0, 0.6], [4000.0, 0.5]]))
    fit = {"soundboard": FLAT, "room": {"eq_db": [[250.0, -3.0]], "gain": 1.0}}
    r = SB.room_params(_Prof(fit))
    assert np.allclose(r["t60"], [[250.0, 0.6], [4000.0, 0.5]])     # the measured decay, as before
    assert r["eq_db"] == [[250.0, -3.0]] and r["gain"] == 1.0
    r = SB.room_params(_Prof({"soundboard": FLAT}))
    assert r["eq_db"] == [] and r["gain"] == SB.ROOM_GAIN


def test_the_correction_carries_the_room_s_compensation():
    sb = {"grid_hz": [100.0, 1000.0, 10000.0], "H_db": [0.0, 0.0, 0.0],
          "correction": {"notes": [21, 60, 108], "C_db": [[1.0, 2.0, 3.0]] * 3}}
    room = {"comp_db": [0.5, -1.0, 2.0]}
    notes, rows = SB.correction_rows(sb, room)
    assert notes == [21, 60, 108] and np.allclose(rows, [[1.5, 1.0, 5.0]] * 3)
    # no correction identified: the compensation alone, on the correction's anchors
    notes, rows = SB.correction_rows({k: v for k, v in sb.items() if k != "correction"}, room)
    assert notes == list(SB.CORR_NOTES) and np.allclose(rows, [[0.5, -1.0, 2.0]] * len(SB.CORR_NOTES))
    assert SB.correction_rows({k: v for k, v in sb.items() if k != "correction"}, None) is None
    # the per-partial gain: correction plus compensation
    assert np.allclose(SB.partial_gain_db(sb, room, 60, np.array([1000.0])), [1.0])



def test_a_cut_takes_energy_away():
    # eq_db is on top of the room's unit energy: a uniform −10 dB is −10 dB, not renormalized away
    flat = SB.room_ir(FLAT, LATE, SR)
    cut = SB.room_ir(FLAT, LATE, SR, eq_db=np.array([[100.0, -10.0], [10000.0, -10.0]]))
    assert abs(10 * np.log10((cut ** 2).sum() / (flat ** 2).sum()) + 10.0) < 0.2
