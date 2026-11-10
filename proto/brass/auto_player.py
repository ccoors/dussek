"""The virtual player: the controllers a keyboard player cannot send, made from the keys alone.

It writes the same CC11, CC1, CC19, CC27 and pitch bend a programmer would draw, for midi_play to
play. It knows only what has happened: never a note's length or the next note before they arrive
(`causal()` checks this), so it could run live. Its shapes come from eight demo files' controllers
over 291 notes: velocity hardly says how much air (r 0.17), higher notes get more (r 0.39); a slur
glides between levels; a tongued note has a small accent (+4 of 127, gone in 150 ms); vibrato on 41
of 55 long notes, by about 0.2 s, to about 98 of 127, about 5 Hz; a long note swells 3 and sags 5 over 2 s.
What the player does send (expression pedal, mod wheel, pitch wheel) wins over the shapes."""
import math
import numpy as np
import midi_play, spec

STEP = 0.005  # s, the grid of the curves it writes


def causal_rng(seed, i):
    return np.random.default_rng([seed, i])


def level_of(v, m):
    """Air for a key: how hard it was struck, and higher notes a little more."""
    if "vel_air" in spec.S:  # the whole of the dynamics from the keys; the higher notes' more goes to nothing at the top, so the top stays one velocity's
        b = float(np.interp(v, *spec.S["vel_air"]))
        return b + np.clip(m - spec.S["center"], -12, 12) * (1 - b / 127.0)
    return 45.0 + 75.0 * v / 127.0 + 1.0 * np.clip(m - spec.S["center"], -12, 12)


class Shape:
    """One note's curves as functions of the time since its key went down. Everything in it was known
    the moment the key went down: the key, how hard, whether a slur, the level so far."""

    def __init__(self, i, m, v, slur, fresh, level, last, seed=1):
        r = causal_rng(seed, i)
        keep = 0.0 if fresh else (0.6 if slur else 0.3)  # legato keeps more of the last note's level
        target = level_of(v, m)
        self.level = target if level is None else keep * level + (1 - keep) * target
        self.level += r.normal(0, 1.5)
        self.v, self.slur = v, slur
        self.from_last = (last - self.level) if slur and last is not None else None
        self.on = 0.3 * r.uniform(0.8, 1.3)
        self.depth = 70 * r.uniform(0.8, 1.1)
        self.rate0 = r.uniform(34, 44)
        self.off = float(np.clip(r.normal(0, 6), -12, 12))
        self.scoops = not slur and r.uniform() < 0.15

    def air(self, t):
        b = self.level + 3 * min(1.0, t / 0.4) - max(0.0, min(10.0, 3 * (t - 1.2)))
        if self.from_last is not None:
            b += self.from_last * math.exp(-t / 0.05)
        elif not self.slur:
            b += 5 * (self.v / 100) * math.exp(-t / 0.08)
        return max(1.0, min(127.0, b))

    def vibrato(self, t):
        return self.depth * max(0.0, min(1.0, (t - self.on) / 0.8))

    def vibrato_rate(self, t):
        return self.rate0 + 5 * min(1.0, t / 2)

    def bend_cents(self, t):
        return self.off + (-60 * math.exp(-t / 0.035) if self.scoops else 0.0)


def perform(notes, seed=1, cc=None, pb=None, shapes=True):
    """notes: (start, end, midi, velocity) in time order. shapes False: the air from the key and nothing
    else (what a keyboard gives without a virtual player)."""
    notes = sorted(notes)
    cc11, cc1, cc19, cc27, bend = [], [], [], [], []
    level, last = None, None
    for i, (s, e, m, v) in enumerate(notes):
        nxt = notes[i + 1][0] if i + 1 < len(notes) else None
        prev = notes[i - 1] if i else None
        slur = prev is not None and prev[1] >= s - 0.001
        fresh = prev is None or s - prev[1] > 1.0
        if not shapes:
            cc11.append((s, min(127.0, level_of(v, m))))
            continue
        sh = Shape(i, m, v, slur, fresh, level, last, seed)
        level = sh.level
        end = min(e, nxt) if nxt is not None and nxt <= e else e  # the next key cuts this one when it comes
        for t in np.arange(0, max(end - s, STEP), STEP):
            t = float(t)
            cc11.append((s + t, sh.air(t)))
            cc1.append((s + t, sh.vibrato(t)))
            cc19.append((s + t, sh.vibrato_rate(t)))
            bend.append((s + t, sh.bend_cents(t) / 200 * 8192))
        last = cc11[-1][1]
        cc27.append((e, 60.0 if e - s > 0.8 else 40.0))  # a long note is let go of more slowly; known once the key comes up
    out = {11: cc11, 1: cc1, 19: cc19, 27: sorted(cc27)}
    for k, ev in (cc or {}).items():
        if ev:  # what the player sends wins
            out[k] = ev
    return midi_play.Track(notes, out, pb or bend)


def causal(notes, seed=1):
    """True when a prefix of the notes gives the same curves up to the next note's start."""
    full = perform(notes, seed)
    for k in range(1, len(notes)):
        part = perform(notes[:k], seed)
        t = sorted(notes)[k][0]
        for c in (11, 1, 19):
            a = [x for x in full.cc[c] if x[0] < t]
            b = [x for x in part.cc[c] if x[0] < t]
            if a != b:
                return False, k, c
        if [x for x in full.pb if x[0] < t] != [x for x in part.pb if x[0] < t]:
            return False, k, "pb"
    return True, None, None
