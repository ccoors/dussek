import os

import numpy as np
import pytest
import soundfile as sf
from scipy.signal import butter, sosfilt

import analyze as A
import datasets as D

ROOT = D.root("grand_d")


def floor_db(path):
    """The file's noise floor 1–8 kHz: the quietest 0.5 s of its second half."""
    x, sr = sf.read(path)
    m = sosfilt(butter(4, [1000, 8000], "bandpass", fs=sr, output="sos"), x.mean(1))
    w = int(0.5 * sr)
    return 10 * np.log10(min(np.mean(m[i:i + w] ** 2) for i in range(len(m) // 2, len(m) - w, w // 4)))


pytestmark = pytest.mark.skipif(not os.path.isdir(ROOT), reason="Piano in 162 not in data/")


def test_piano162_loads_every_key_close_pedal_up_first_take():
    items = D.items("grand_d")
    assert sorted({i["note"] for i in items}) == list(range(21, 109))
    assert sorted({i["dynamic"] for i in items}) == ["f", "mp", "pp"] and len(items) == 3 * 88
    names = {os.path.basename(i["path"]) for i in items}
    assert "01-PedalOffPianissimo1Close.flac" in names and "88-PedalOffForte1Close.flac" in names
    assert all("PedalOffClose" in i["path"] and i["path"].endswith("1Close.flac") for i in items)
    assert all(os.path.exists(i["path"]) for i in items)
    # the engine plays pp, mf and ff at MIDI 14, 60, 124: Ivy's pp (to 33), mp (65–80), f (102–127),
    # the layers that rise evenly over the velocity (p is 5 dB above pp and 25 dB below f)
    assert D.dynamics("grand_d") == {"pp": "pp", "mf": "mp", "ff": "f"} and D.levels_recorded("grand_d")


def test_the_layer_gains_bring_every_noise_floor_to_the_mf_layers():
    # each layer was recorded at its own gain (its floor 4–12 dB above mf's in every band,
    # constant within the layer): the gains undo that, so the levels between layers are real
    items = D.items("grand_d")
    by = {}
    for it in items:
        if it["note"] % 7 == 0:
            by.setdefault(it["dynamic"], []).append(floor_db(it["path"]) - it["gain_db"])
    med = {d: np.median(v) for d, v in by.items()}
    assert max(med.values()) - min(med.values()) < 0.5


def test_analyze_applies_the_gain(tmp_path):
    rng = np.random.default_rng(1)
    t = np.arange(3 * 44100) / 44100
    x = 0.3 * np.sin(2 * np.pi * 261.6 * t) * np.exp(-2 * t) + 1e-4 * rng.standard_normal(len(t))
    path = tmp_path / "n.wav"
    sf.write(path, np.column_stack([x, x]), 44100, subtype="FLOAT")
    item = {"path": str(path), "note": 60, "layer": 1, "layers": 1, "dynamic": "mf", "tail_cut_s": 0.0}
    plain, quieter = A.analyze(item), A.analyze({**item, "gain_db": 6.0})
    assert abs(plain["peak_dbfs"] - quieter["peak_dbfs"] - 6.0) < 0.01 and quieter["gain_db"] == 6.0
