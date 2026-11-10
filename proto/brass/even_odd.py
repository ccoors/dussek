"""Clarinet or trumpet? (usage: BRASS=<name> even_odd.py): the harmonics 1 to 8 of a held note, recording against model; even re odd:
the mean level of harmonics 2, 4, 6 less that of 3, 5, 7, dB. A clarinet's low notes hold it far below zero."""
import json, os
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import air_noise, live, spec, soundfile as sf

SR = live.SR


def harm(x, sr, f0):
    w = np.hanning(len(x))
    sp = np.abs(np.fft.rfft(x * w)) ** 2
    f = np.fft.rfftfreq(len(x), 1 / sr)
    h = np.array([sp[(f > k * f0 * 0.97) & (f < k * f0 * 1.03)].max() if k * f0 < sr / 2 - 500 else 1e-30 for k in range(1, 9)])
    return 10 * np.log10(h / h.max() + 1e-12)


def index(h):
    return float(np.mean(h[[1, 3, 5]]) - np.mean(h[[2, 4, 6]]))


def model(job):
    key, dyn = job
    x = live.render_events([(0, "cc", 11, air_noise.AIR[dyn]), (0.02, "on", key, 100), (1.9, "off", key)], 2.0, "open", False, 1)
    return key, dyn, harm(x[int(0.6 * SR):int(1.8 * SR)], SR, 440 * 2 ** ((key - 69) / 12)).tolist()


def rec(job):
    key, dyn, p, a, b = job
    x, sr = sf.read(p)
    x = x.mean(axis=1) if x.ndim > 1 else x
    if a is None:
        e = np.array([np.sqrt(np.mean(x[i:i + 2205] ** 2)) for i in range(0, len(x) - 2205, 2205)])
        i0 = int(np.argmax(e > 0.3 * e.max()) * 2205 + 0.4 * sr)
        s = x[i0:i0 + int(1.2 * sr)]
    else:
        s = x[int((a + 0.5) * sr):int(min(a + 1.7, b - 0.3) * sr)]
    return (key, dyn, harm(s, sr, 440 * 2 ** ((key - 69) / 12)).tolist()) if len(s) > sr // 2 else None


if __name__ == "__main__":
    lo, hi = spec.S.get("low", 55), spec.S.get("high", 88)
    keys = list(range(lo, hi + 1))
    with ProcessPoolExecutor() as ex:
        R = [r for r in ex.map(rec, air_noise.rec_files()) if r]
        M = list(ex.map(model, [(k, d) for k in keys for d in ("pp", "mf", "ff")]))
    json.dump(dict(rec=R, model=M), open(os.path.join(spec.OUT, f"even_odd_{spec.NAME}.json"), "w"))
    print(spec.NAME, "even re odd, dB: model pp mf ff / nearest recording pp mf ff; then mf harmonics 1 to 8 model / recording")
    for k in keys:
        m = {d: h for key, d, h in M if key == k}
        line = f"{k:4d} " + " ".join(f"{index(np.array(m[d])):6.1f}" for d in ("pp", "mf", "ff")) + "  /"
        for d in ("pp", "mf", "ff"):
            c = [r for r in R if r[1] == d]
            n = min(c, key=lambda r: abs(r[0] - k)) if c else None
            line += f" {index(np.array(n[2])):6.1f}" if n and abs(n[0] - k) <= 2 else "     --"
        c = [r for r in R if r[1] == "mf" and abs(r[0] - k) <= 2]
        if c:
            n = min(c, key=lambda r: abs(r[0] - k))
            line += "   " + " ".join(f"{v:4.0f}" for v in m["mf"]) + " / " + " ".join(f"{v:4.0f}" for v in n[2])
        print(line)
