"""The harmonics a note should have (usage: BRASS=<name> harm_fit.py): per key and dynamics, each harmonic's level against the
recording's, the sampled notes interpolated, smoothed over neighboring keys and held to +-10 dB; only where the recording's harmonic
stands clear of its floor. A low note's twelve stop far below the treble's: at mf and ff it goes on to REACH, smoothed along the harmonics, down as far as it takes;
where the recording is at its own floor it only says "at most this" (pp keeps its twelve, the air noise accepted there). Numbers only. Writes inst/<name>/residual5.json; rt_tables.py folds it into the bell's output filter."""
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import air_noise, even_odd, live, rt_tables, spec

SR = live.SR
DYNS = ("pp", "mf", "ff")
K, KMAX, REACH = 12, 400, 16000.0
LIMIT, DOWN, CLEAR, SMOOTH = 10.0, 40.0, -60.0, 2


def peaks(x, sr, f0):
    w = np.hanning(len(x))
    sp = np.abs(np.fft.rfft(x * w)) ** 2
    f = np.fft.rfftfreq(len(x), 1 / sr)
    return np.array([sp[(f > k * f0 * 0.97) & (f < k * f0 * 1.03)].max(initial=0.0) if (k <= K and k * f0 < min(sr / 2 - 500, 18000)) or k * f0 <= REACH else 0.0 for k in range(1, KMAX + 1)])


def harmonics(x, sr, f0):
    h = peaks(x, sr, f0)
    return 10 * np.log10(h / h.sum() + 1e-14)


def floor_of(y, s, sr, f0):
    """The file's quietest 100 ms, on the note's scale, in the harmonics' terms."""
    n = int(0.1 * sr)
    q = int(np.argmin([np.std(y[j:j + n]) for j in range(0, len(y) - n, n)])) * n
    h = peaks(y[q:q + n], sr, f0) * len(s) / n
    return 10 * np.log10(h / peaks(s, sr, f0).sum() + 1e-14)


def along(v, top):
    """Past the twelfth: the median of its neighbors, a band's worth; one harmonic there says little."""
    out = v.copy()
    for j in range(K, top):
        w = max(1, (j + 1) // 8)
        out[j] = np.median(v[max(K, j - w):min(top, j + w + 1)])
    return out


def model(job):
    key, dyn = job
    x = live.render_events([(0, "cc", 11, air_noise.AIR[dyn]), (0.02, "on", key, 100), (1.9, "off", key)], 2.0, "open", False, 1)
    return key, dyn, harmonics(x[int(0.6 * SR):int(1.8 * SR)], SR, 440 * 2 ** ((key - 69) / 12)).tolist()


def rec(job):
    key, dyn, p, a, b = job
    x = even_odd.sf.read(p)
    x, sr = x
    x = x.mean(axis=1) if x.ndim > 1 else x
    if a is None:
        e = np.array([np.sqrt(np.mean(x[i:i + 2205] ** 2)) for i in range(0, len(x) - 2205, 2205)])
        i0 = int(np.argmax(e > 0.3 * e.max()) * 2205 + 0.4 * sr)
        s = x[i0:i0 + int(1.2 * sr)]
    else:
        s = x[int((a + 0.5) * sr):int(min(a + 1.7, b - 0.3) * sr)]
    if len(s) <= sr // 2:
        return None
    f0 = 440 * 2 ** ((key - 69) / 12)
    return key, dyn, harmonics(s, sr, f0).tolist(), floor_of(x, s, sr, f0).tolist()


if __name__ == "__main__":
    keys = rt_tables.TABLE
    with ProcessPoolExecutor() as ex:
        R = [r for r in ex.map(rec, air_noise.rec_files()) if r]
        M = {(k, d): np.array(h) for k, d, h in ex.map(model, [(k, d) for k in keys for d in DYNS])}
    delta = {}
    for d in DYNS:
        have = sorted(((r[0], np.array(r[2]), np.array(r[3])) for r in R if r[1] == d), key=lambda q: q[0])
        ks = [q[0] for q in have]
        hs, fs = np.array([q[1] for q in have]), np.array([q[2] for q in have])
        raw = {}
        for k in keys:
            target = np.array([np.interp(k, ks, hs[:, j]) for j in range(KMAX)])
            diff = target - M[(k, d)]
            # a harmonic the recording barely has says nothing; one the model has not at all is capped by LIMIT
            low = np.where((target > CLEAR) & (M[(k, d)] > -130), diff, 0.0)
            floor = np.array([np.interp(k, ks, fs[:, j]) for j in range(KMAX)])
            tail = np.where((target > -130) & (M[(k, d)] > -130), np.where(target < floor + 6, np.minimum(diff, 0.0), diff), 0.0)
            raw[k] = np.concatenate([low[:K], tail[K:]])
        for k in keys:
            near = [raw[q] for q in keys if abs(q - k) <= SMOOTH]
            f0 = 440 * 2 ** ((k - 69) / 12)
            top = K if d == "pp" else max(K, min(KMAX, int(REACH / f0)))
            v = along(np.median(near, axis=0), top)[:top]
            up = spec.S.get(f"{d}_cap", LIMIT)
            delta[(k, d)] = np.concatenate([np.clip(v[:K], -LIMIT, up), np.clip(v[K:], -DOWN, up)])
    out = {f"{k}_{d}": v.tolist() for (k, d), v in delta.items()}
    spec.save("residual", out)
    print(spec.NAME, "residual, dB, harmonics 1 to 12: mean and largest by register at mf")
    for k in keys[::4]:
        print(f"{k:4d} " + " ".join(f"{v:5.1f}" for v in delta[(k, "mf")][:K]))
