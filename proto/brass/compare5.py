"""Round 3 against Iowa: two-mode lips, the player leaning into each note, breath and wander,
the slide per note, the microphone correction. Stats per note and dynamics, and
out/compare_r3.wav (per note: Iowa pp, model pp, Iowa ff, model ff, level-matched)."""
import os
import numpy as np
import soundfile as sf
import sim, player, spec, sustain, fit4, fit5

HERE = os.path.dirname(os.path.abspath(__file__))
T5 = spec.load("table", {"table": {}, "eq": [], "nl": 0.0})
LIFE = dict(breath=0.015, wander=0.05, lipwander=0.003)
CENTERS_HZ = [500, 1414, 2828, 5657, 11314, 18762]


def eq_curve(freqs, eq):
    lf = np.log(np.maximum(freqs, 1.0))
    return 10 ** (np.interp(lf, np.log(T5.get("eq_hz", CENTERS_HZ)), eq) / 20)


def apply_eq(x, eq, sr=sim.SR):
    X = np.fft.rfft(x)
    return np.fft.irfft(X * eq_curve(np.fft.rfftfreq(len(x), 1 / sr), eq), len(x))


def attack_for(dyn):
    """The player's attack by dynamics (0 pp .. 1 ff): pp pushes the breath (130 %) and starts
    the lips slack (300 cents), ff leans into the note (70 % first) with less slack (120 c);
    fitted to Iowa's attack pitch curves (pp from ~100 cents below in 60–80 ms)."""
    return dict(first=1.3 - 0.6 * dyn, scoop=300.0 - 180.0 * dyn, kick=0.5, burst=BURST)


BURST = 5.0  # the attack's rush of air: the breath noise x (1 + 5 e^(-t / 30 ms)), +15 dB at the start


DYN = {"pp": 0.0, "mf": 0.55, "ff": 1.0}


def kick_for(pm):
    """The tongue's release in proportion to the breath behind it (a fixed kick clicked at pp)."""
    return 0.5 * min(1.0, pm / 2000.0)


TUNE = spec.load("tuning", {})


def tuned_note(midi, dyn="mf"):
    """The note with its slide moved so the cell of this dynamics plays in tune (a player
    corrects each note's intonation by ear; the lips' setting differs by dynamics), plus the
    correction measured on the full player (tuning5.json)."""
    note = fit4.note_of(midi)
    note.retune(T5["table"][str(midi)][dyn]["cents"] + TUNE.get(f"{midi}_{dyn}", 0.0))
    return note


def _measure_pitch(a):
    m, d = a
    x = model(m, d, dur=2.0)
    f = 440 * 2 ** ((m - 69) / 12)
    s = x[int(0.6 * sim.SR):int(1.8 * sim.SR)]
    ps = np.array([1200 * np.log2(player.pitch(s[k:k + int(0.05 * sim.SR)]) / f)
                   for k in range(0, len(s) - int(0.05 * sim.SR), int(0.025 * sim.SR))])
    return a, float(np.median((ps + 600) % 1200 - 600))


def retune_all():
    """One pass: render every note and dynamics with the full player, move its slide by what
    it plays off."""
    from concurrent.futures import ProcessPoolExecutor
    jobs = [(int(m), d) for m in T5["table"] for d in ("pp", "mf", "ff")]
    with ProcessPoolExecutor(8) as ex:
        res = dict(ex.map(_measure_pitch, jobs))
    new = {f"{m}_{d}": TUNE.get(f"{m}_{d}", 0.0) + c for (m, d), c in res.items()}
    spec.save("tuning", new)
    return res


def model(midi, dyn, dur=2.5, seed=1, life=True, kw=None, scoop=None, scoop_t=0.04, first=None, kick=None):
    c = T5["table"][str(midi)][dyn]
    a = attack_for(DYN[dyn])
    scoop = a["scoop"] if scoop is None else scoop
    first = a["first"] if first is None else first
    kick = a["kick"] if kick is None else kick
    (x,), p = fit5.render(tuned_note(midi, dyn), c["center"], c["pm"], nls=(T5["nl"],), dur=dur, kick=kick_for(c["pm"]) if kick is None else kick,
                          life=(kw or LIFE) if life else None, seed=seed, blow=True,
                          scoop=scoop, scoop_t=scoop_t, first=first, burst=a["burst"] if life else 0.0)
    return apply_eq(x, np.array(T5["eq"]))


def job(a):
    m, d = a
    x = model(m, d, dur=2.0)
    f = 440 * 2 ** ((m - 69) / 12)
    xx = np.concatenate([np.zeros(int(0.05 * sim.SR)), x])
    st = sustain.stats(x[int(0.6 * sim.SR):int(1.8 * sim.SR)], sim.SR, f)
    return a, x, st, sustain.bands(x[int(0.6 * sim.SR):int(1.8 * sim.SR)], sim.SR), sustain.rises(xx, sim.SR), sustain.click(xx, sim.SR)


def iowa_clip(m, d, dur=2.5, pre=0.0):
    info = spec.notes()[f"{m}_{d}"]
    x, sr = sf.read(info["file"])
    x = sustain.highpass(x)
    a = int((info["start"] - pre) * sr)
    return x[a:a + int(dur * sr)].copy()


if __name__ == "__main__":
    import sys
    if "retune" in sys.argv:
        r = retune_all()
        print("off before:", sorted(round(v) for v in r.values()))
