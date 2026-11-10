"""The player's setup per note: the lip tuning that sounds the right mode, the slide that
centers the pitch. Cached in tuning.json."""
import os
import numpy as np
import sim, spec

HERE = os.path.dirname(os.path.abspath(__file__))
NL = 1.24e-8 * spec.S["nl"]  # s/Pa: 1 m of narrow bore, half the mouthpiece pressure travelling out; x its length in bore


def pitch(p, sr=sim.SR, near=None):
    """The period by autocorrelation; near: the note it should be, from a fifth below to an octave above."""
    s = p - p.mean()
    ac = np.fft.irfft(np.abs(np.fft.rfft(s, 2 * len(s))) ** 2)[: len(s)]
    lo, hi = int(sr / 1600), int(sr / (3 * spec.S["fmin"]))
    if near is not None:
        lo, hi = max(2, int(sr / (near * 2.0))), min(len(s) - 2, int(sr / (near / 1.5)))
    lag = lo + np.argmax(ac[lo:hi])
    a, b, c = ac[lag - 1], ac[lag], ac[lag + 1]
    return sr / (lag + 0.5 * (a - c) / (a - 2 * b + c))


def play(note, pm_level, ratio, dur=0.6):
    N = int(dur * sim.SR)
    pm = pm_level * sim.envelope(N)
    return sim.simulate([note], np.zeros(N, int), pm, np.full(N, note.f * ratio))


def cents_off(note, pm_level, ratio):
    u, p = play(note, pm_level, ratio)
    s = p[int(0.3 * sim.SR) :]
    if np.std(s) < 100:
        return None
    f = pitch(s)
    if not np.isfinite(f) or f <= 0:
        return None
    return 1200 * np.log2(f / note.f)


def tune(midi, pm_level=3000.0):
    note = sim.Note(midi)
    ratios = np.arange(0.45, 0.96, 0.025)
    ok = [r for r in ratios if (c := cents_off(note, pm_level, r)) is not None and abs(c) < 60]
    if not ok:
        return {"midi": midi, "ok": False}
    ratio = float(np.median(ok))
    for _ in range(2):
        c = cents_off(note, pm_level, ratio)
        if c is None or abs(c) > 60:
            break
        note.retune(c)
    return {"midi": midi, "ok": True, "ratio": ratio, "band": [float(min(ok)), float(max(ok))],
            "extra": note.extra, "cents_after": cents_off(note, pm_level, ratio)}


def load():
    return {}


def note_for(midi, tuning):
    t = tuning[midi]
    note = sim.Note(midi)
    note.extra = t["extra"]
    note.retune(0.0)
    return note, t["ratio"]


