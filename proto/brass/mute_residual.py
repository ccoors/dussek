"""What the mutes' shapes cannot reach: per mute and dynamics, in third octaves, the recording's difference to
its open notes less the model's difference to its open notes. One fixed filter on the way out, as the
microphone's correction is for the open trumpet; it does not change how a note plays. Also the open model's
difference to VSCO's open notes, for hearing the mutes without that difference (mutes_compare.py).
Writes mute_residual.json."""
import os
import numpy as np
from concurrent.futures import ProcessPoolExecutor
from scipy.signal import resample_poly
import mute_vsco, muted, sim, spec

HERE = os.path.dirname(os.path.abspath(__file__))
SR = sim.SR
CENTERS = 1000 * 2.0 ** (np.arange(-12, 14) / 3)
PLAN = spec.S["mute_recordings"]["plan"]
CAP = 25.0


def envelope(s, f):
    """The harmonics' peaks (dB), interpolated in log frequency at the third octaves."""
    X = np.abs(np.fft.rfft(s * np.hanning(len(s)))) / len(s) * 4
    F = np.fft.rfftfreq(len(s), 1 / SR)
    hf, hl = [], []
    for k in range(1, int(18000 / f) + 1):
        m = (F > f * k * 0.97) & (F < f * k * 1.03)
        if m.any():
            hf.append(f * k)
            hl.append(20 * np.log10(X[m].max() + 1e-12))
    return np.interp(np.log(CENTERS), np.log(hf), hl, left=np.nan, right=np.nan)


def recording(folder, name, vel):
    r = mute_vsco.rec(folder, name, vel)
    if r is None:
        return None
    return r[0] if r[1] == SR else resample_poly(r[0], SR, r[1])


def model(a):
    m, d, k = a
    return a, envelope(muted.model(m, d, k, dur=2.0, raw=True)[int(0.7 * SR):int(1.8 * SR)], 440 * 2 ** ((m - 69) / 12))


def smooth(x):
    """Filled where a note has no harmonics, then eased over three bands."""
    k = ~np.isnan(x)
    y = np.interp(np.arange(len(x)), np.nonzero(k)[0], x[k])
    return np.convolve(np.pad(y, 1, mode="edge"), [0.25, 0.5, 0.25], "valid")


if __name__ == "__main__":
    out = {"centers": CENTERS.tolist()}
    table = {str(m) for m in muted.T5["table"]}
    jobs = sorted({(mute_vsco.midi_of(n), d, k) for _, mute, ns in PLAN for n in ns for d in ("pp", "ff") for k in (mute, "open") if str(mute_vsco.midi_of(n)) in table})
    with ProcessPoolExecutor(10) as ex:
        M = dict(ex.map(model, jobs))
    for folder, mute, names in PLAN:
        out[mute] = {}
        out.setdefault("open_to_recording", {})
        for vel, d in mute_vsco.DYN.items():
            res, opn = [], []
            for n in names:
                m = mute_vsco.midi_of(n)
                if str(m) not in table:
                    continue
                a, o = recording(folder, n, vel), recording("sus", n, vel)
                if a is None or o is None or len(a) < SR // 2 or len(o) < SR // 2:
                    continue
                f = 440 * 2 ** ((m - 69) / 12)
                ea, eo = envelope(a, f), envelope(o, f)
                res.append((ea - eo) - (M[(m, d, mute)] - M[(m, d, "open")]))
                opn.append((eo - 20 * np.log10(np.std(o))) - (M[(m, d, "open")]))
            out[mute][d] = np.clip(smooth(np.nanmedian(res, axis=0)), -CAP, CAP).tolist()
            out["open_to_recording"].setdefault(d, []).extend(opn)
            print(mute, d, f"{len(res)} notes", np.round(out[mute][d][6:]).astype(int).tolist())
    for d, rows in out["open_to_recording"].items():
        out["open_to_recording"][d] = smooth(np.nanmedian(rows, axis=0)).tolist()
    spec.save("mute_residual", out)
