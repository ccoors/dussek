"""Did the model play what the MIDI asks? Per note, the pitch in its middle against the note plus
the bend, and the level; prints the notes that missed. usage: midi_check.py file.mid track out.wav"""
import sys
import numpy as np
import soundfile as sf
import midi_play, sim


def pitch_near(s, f, sr=sim.SR):
    """The autocorrelation's best lag within a fifth around f."""
    s = s - np.mean(s)
    c = np.fft.irfft(np.abs(np.fft.rfft(s, 2 * len(s))) ** 2)[:len(s)]
    lo, hi = int(sr / (f * 1.5)), int(sr / (f / 1.5))
    if hi >= len(c) or c[0] <= 0:
        return np.nan, 0.0
    k = lo + int(np.argmax(c[lo:hi]))
    d = c[k - 1] - 2 * c[k] + c[k + 1]
    lag = k + (0.5 * (c[k - 1] - c[k + 1]) / d if d < 0 else 0.0)  # between samples: a high note's period is few of them
    return sr / lag, float(c[k] / c[0])


def check(tr, x, sr=sim.SR):
    rows = []
    for s, e, m, v in tr.notes:
        if e - s < 0.08:
            continue
        a, b = int((s + 0.25 * (e - s)) * sr), int((s + 0.85 * (e - s)) * sr)
        if b - a < 2048 or b > len(x):
            continue
        tm = np.array([(a + b) / 2 / sr])
        bend = midi_play.curve(tr.pb, 0, tm)[0] / 8192 * midi_play.PB_RANGE
        f = 440 * 2 ** ((m + bend - 69) / 12)
        p, r = pitch_near(x[a:b], f)
        rows.append((round(s, 2), m, round(float(bend), 2), round(1200 * np.log2(p / f)) if p == p else None, round(r, 2),
                     round(20 * np.log10(np.std(x[a:b]) + 1e-12), 1)))
    off = [r for r in rows if r[3] is None or abs(r[3]) > 40 or r[4] < 0.5]
    return rows, off


if __name__ == "__main__":
    tr = midi_play.read(sys.argv[1])[sys.argv[2]]
    x, _ = sf.read(sys.argv[3])
    rows, off = check(tr, x)
    print(f"{len(rows)} notes checked, {len(off)} off")
    print("t, midi, bend, cents, clarity, dB")
    for r in off[:60]:
        print(r)
    c = np.array([r[3] for r in rows if r[3] is not None])
    print("cents median abs", np.median(np.abs(c)), "p90", np.percentile(np.abs(c), 90))
