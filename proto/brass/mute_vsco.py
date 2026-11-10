"""The mutes against VSCO's muted recordings (straight and wah-wah; same player, room and
microphones as its open notes): per note and dynamics the muted note's octave bands and
harmonics minus the open note's, and the model's same difference."""
import glob, os, re
import numpy as np
import soundfile as sf
from concurrent.futures import ProcessPoolExecutor
import sim, muted, spec, sustain

HERE = os.path.dirname(os.path.abspath(__file__))
_REC = spec.S.get("mute_recordings", {})
V = os.path.join(spec.P.REPO, "data", _REC["root"], _REC["folder"]) if _REC else None
NAMES = {"C": 0, "C#": 1, "D": 2, "D#": 3, "E": 4, "F": 5, "F#": 6, "G": 7, "G#": 8, "A": 9, "A#": 10, "B": 11}
DYN = {"v1": "pp", "v3": "ff"}


def midi_of(name):
    m = re.match(r"([A-G]#?)(-?\d)", name)
    return 12 * (int(m.group(2)) + 2) + NAMES[m.group(1)]  # the set's names run an octave low: its C3 is MIDI 60


def harm(x, f, sr, n=8):
    X = np.abs(np.fft.rfft(x * np.hanning(len(x))))
    F = np.fft.rfftfreq(len(x), 1 / sr)
    return np.array([20 * np.log10(X[(F > f * k * 0.97) & (F < f * k * 1.03)].max() + 1e-12) if f * k * 1.03 < sr / 2 else np.nan for k in range(1, n + 1)])


def rec(folder, name, vel):
    fs = glob.glob(os.path.join(V, folder, f"*_{name}_{vel}_rr1.wav"))
    if not fs:
        return None
    x, sr = sf.read(fs[0])
    if x.ndim > 1:
        x = x.mean(axis=1)
    e = np.convolve(np.abs(x), np.ones(2000) / 2000, "same")
    on = np.argmax(e > 0.2 * e.max())
    a = on + int(0.35 * sr)
    b = min(a + int(1.2 * sr), np.nonzero(e > 0.3 * e.max())[0][-1])
    return x[a:b], sr


def measure(s, f, sr):
    return np.array(sustain.bands(s, sr)[:5]) + 20 * np.log10(np.std(s)), harm(s, f, sr), 20 * np.log10(np.std(s))


def model(a):
    m, d, mute = a
    x = muted.model(m, d, mute)[int(0.6 * sim.SR):int(1.8 * sim.SR)]
    return a, measure(x, 440 * 2 ** ((m - 69) / 12), sim.SR)


if __name__ == "__main__":
    pairs = {"straightM-sus": ["straight"], "harmonM-sus": ["wah", "wah_stem"]}
    for folder, mutes_ in pairs.items():
        names = sorted({re.search(r"sus_([A-G]#?-?\d)_", os.path.basename(p)).group(1) for p in glob.glob(os.path.join(V, folder, "*.wav"))}, key=midi_of)
        names = [n for n in names if str(midi_of(n)) in muted.T5["table"]]
        print(folder, "notes", [(n, midi_of(n)) for n in names])
        for vel, d in DYN.items():
            R, RH = [], []
            lev = []
            for n in names:
                a, o = rec(folder, n, vel), rec("sus", n, vel)
                if a is None or o is None:
                    continue
                f = 440 * 2 ** ((midi_of(n) - 69) / 12)
                A, O = measure(a[0], f, a[1]), measure(o[0], f, o[1])
                R.append(A[0] - O[0]); RH.append((A[1] - A[1][0]) - (O[1] - O[1][0])); lev.append(A[2] - O[2])
            print(f" {d}: recording, bands <1k..8-16k re open {np.round(np.nanmedian(R, axis=0)).astype(int)}  harmonics re open, re h1 {np.round(np.nanmedian(RH, axis=0)).astype(int)}  level {np.median(lev):+.0f} dB ({len(R)} notes)")
            ms = [midi_of(n) for n in names]
            with ProcessPoolExecutor(8) as ex:
                res = dict(ex.map(model, [(m, d, k) for m in ms for k in ["open"] + mutes_]))
            for k in mutes_:
                B = [res[(m, d, k)][0] - res[(m, d, "open")][0] for m in ms]
                H = [(res[(m, d, k)][1] - res[(m, d, k)][1][0]) - (res[(m, d, "open")][1] - res[(m, d, "open")][1][0]) for m in ms]
                L = [res[(m, d, k)][2] - res[(m, d, "open")][2] for m in ms]
                print(f"    model {k:9s}: bands {np.round(np.nanmedian(B, axis=0)).astype(int)}  harmonics {np.round(np.nanmedian(H, axis=0)).astype(int)}  level {np.median(L):+.0f} dB")
