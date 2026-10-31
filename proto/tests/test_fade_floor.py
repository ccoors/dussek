import numpy as np
import soundfile as sf

import analyze as A

SR = 44100


def note(fade_s=0.0, zeros_s=0.0, seconds=8.0, seed=1):
    """A decaying C4 (12 partials, all in the noise floor within 4 s, so that a file cut
    by the fade still ends in the floor) over a steady noise floor; optionally the file's last
    fade_s fade out (as sample sets do), then zeros_s of digital silence."""
    rng = np.random.default_rng(seed)
    t = np.arange(int(seconds * SR)) / SR
    f0 = 261.6
    x = sum(10 ** (-n / 10) * np.sin(2 * np.pi * n * f0 * t) * np.exp(-t * (2.0 + 0.4 * n)) for n in range(1, 13))
    x = 0.5 * x / np.abs(x).max() + 10 ** (-70 / 20) * rng.standard_normal(len(t))
    if fade_s:
        k = int(fade_s * SR)
        x[-k:] *= 10 ** (np.linspace(0, -40, k) / 20)
    if zeros_s:
        x = np.concatenate([x, np.zeros(int(zeros_s * SR))])
    return np.column_stack([x, x])


def analyzed(tmp_path, name, x):
    path = tmp_path / f"{name}.wav"
    sf.write(path, x, SR, subtype="FLOAT")
    return A.analyze({"path": str(path), "note": 60, "layer": 1, "layers": 1, "dynamic": "ff", "tail_cut_s": 0.0})


def test_fade_length_is_detected():
    assert abs(A.fade_cut_s(note(fade_s=2.0), SR) - 2.0) < 0.3
    assert abs(A.fade_cut_s(note(fade_s=2.0, zeros_s=0.05), SR) - 2.05) < 0.3


def test_no_fade_cuts_nothing():
    assert A.fade_cut_s(note(), SR) < 0.2


def test_a_faded_end_leaves_the_noise_floor_where_it_is(tmp_path):
    # the floor comes from the file's last second: inside a fade it read 20–40 dB low, the
    # noise then counted as the partials' decay (flat tails, strings that barely lose energy)
    clean = {p["n"]: p["floor_db"] for p in analyzed(tmp_path, "clean", note())["partials"]}
    r = analyzed(tmp_path, "faded", note(fade_s=2.0, zeros_s=0.05))
    faded = {p["n"]: p["floor_db"] for p in r["partials"]}
    common = sorted(set(clean) & set(faded))
    assert len(common) >= 6
    assert max(abs(faded[n] - clean[n]) for n in common) < 3.0
    assert abs(r["tail_cut_s"] - 2.05) < 0.3


def test_short_files_fade_too():
    # treble files of 2–4 s fade over their last ~25 % (0.4–1.5 s): a plateau taken a fixed
    # 2.5–4 s before the end missed them
    assert abs(A.fade_cut_s(note(fade_s=0.8, seconds=3.0), SR) - 0.8) < 0.25
    assert A.fade_cut_s(note(seconds=3.0), SR) < 0.2
