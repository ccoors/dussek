"""How much air is between the harmonics, recording against model (usage: BRASS=<name> air_noise.py [off]): over a held note's
sustain, per band, the floor in the middle of the gaps between harmonics, dB per kHz re the note's whole level, a recording's
own background beside it. With off: the model with its noise source silent, to see what the source itself adds."""
import json, os
import numpy as np
import soundfile as sf
from concurrent.futures import ProcessPoolExecutor
import live, spec

SR = live.SR
BANDS = [(500, 1000), (1000, 2000), (2000, 4000), (4000, 8000), (8000, 16000)]
AIR = {"pp": 40, "mf": 75, "ff": 114}


def floors(x, sr, f0):
    """Per band: the median of the gaps' middle bins, dB per kHz re the whole level."""
    w = np.hanning(len(x))
    sp = np.abs(np.fft.rfft(x * w)) ** 2 / (w ** 2).sum() / sr * 2  # power per Hz
    f = np.fft.rfftfreq(len(x), 1 / sr)
    tot = np.mean(x ** 2) + 1e-30
    out = []
    for a, b in BANDS:
        vals = []
        for k in range(1, int(b / f0) + 1):
            lo, hi = (k + 0.35) * f0, (k + 0.65) * f0
            if lo >= a and hi <= b:
                vals.append(np.median(sp[(f >= lo) & (f <= hi)]))
        if not vals and b - a > 0:  # a gap wider than the band
            vals = [np.median(sp[(f >= a) & (f <= b)])]
        out.append(float(10 * np.log10(np.median(vals) * 1000 / tot + 1e-30)))
    return out


def model(job):
    key, dyn, off = job
    lv = live.Live("open", False, 1)
    if off:
        lv.noise = np.zeros_like(lv.noise)  # the noise is the seed's, shared: a silent one of its own
    ev = [(0, "cc", 11, AIR[dyn]), (0.02, "on", key, 100)]
    n = int(2.2 / live.DT)
    out = np.zeros(n * live.CTRL)
    j = 0
    for k in range(n):
        while j < len(ev) and ev[j][0] <= k * live.DT:
            lv.handle(ev[j][1:]); j += 1
        out[k * live.CTRL:(k + 1) * live.CTRL] = lv.step()
    return key, dyn, floors(out[int(0.6 * SR):int(2.0 * SR)], SR, 440 * 2 ** ((key - 69) / 12))


def rec_files():
    """(key, dyn, path, start, end) of the recordings."""
    out = []
    for info in spec.notes().values():
        out.append((info["midi"], info["dyn"], info["file"], info["start"], info["end"]))
    return out


def recording(job):
    key, dyn, p, a, b = job
    x, sr = sf.read(p)
    x = x.mean(axis=1) if x.ndim > 1 else x
    if a is None:
        e = np.array([np.sqrt(np.mean(x[i:i + 2205] ** 2)) for i in range(0, len(x) - 2205, 2205)])
        i0 = int(np.argmax(e > 0.3 * e.max()) * 2205 + 0.4 * sr)
        s = x[i0:i0 + int(1.2 * sr)]
        whole = x
    else:
        s = x[int((a + 0.5) * sr):int(min(a + 1.7, b - 0.3) * sr)]
        whole = x[int(max(a - 2, 0) * sr):int((b + 2) * sr)]
    f0 = 440 * 2 ** ((key - 69) / 12)
    if len(s) < sr // 2 or np.std(s) < 1e-6:
        return None
    # the background: the quietest half second near the note, on the note's own scale
    n = sr // 2
    q = min(range(0, max(len(whole) - n, 1), n // 2), key=lambda i: np.mean(whole[i:i + n] ** 2))
    bg = whole[q:q + n]
    w = np.hanning(len(bg))
    sp = np.abs(np.fft.rfft(bg * w)) ** 2 / (w ** 2).sum() / sr * 2
    f = np.fft.rfftfreq(len(bg), 1 / sr)
    tot = np.mean(s ** 2)
    back = [float(10 * np.log10(np.median(sp[(f >= a_) & (f < b_)]) * 1000 / tot + 1e-30)) for a_, b_ in BANDS]
    return key, dyn, floors(s, sr, f0), back


def table(rows, name):
    print(f"{name:16s}" + "".join(f"{a}-{b}".rjust(11) for a, b in BANDS))
    for dyn in ("pp", "mf", "ff"):
        r = [q[2] for q in rows if q[1] == dyn]
        if r:
            print(f"  {dyn} ({len(r):2d})        " + "".join(f"{v:11.1f}" for v in np.median(r, axis=0)))


if __name__ == "__main__":
    os.environ.setdefault("OMP_NUM_THREADS", "1")
    lo, hi = spec.S.get("low", 55), spec.S.get("high", 88)
    keys = list(range(lo, hi + 1, 3))
    print(spec.NAME, "keys", keys)
    with ProcessPoolExecutor() as ex:
        rec = [r for r in ex.map(recording, rec_files()) if r]
        table(rec, "recording")
        back = [(r[0], r[1], r[3]) for r in rec]
        table(back, "its background")
        m = list(ex.map(model, [(k, d, False) for k in keys for d in AIR]))
        table(m, "model")
        o = list(ex.map(model, [(k, d, True) for k in keys for d in AIR]))
        table(o, "model, no noise")
    json.dump(dict(rec=rec, model=m, off=o), open(os.path.join(spec.OUT, f"air_noise_{spec.NAME}.json"), "w"))
