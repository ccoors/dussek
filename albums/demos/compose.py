#!/usr/bin/env python3
"""Composes the demo pieces and writes them as Standard MIDI files: four short original pieces
that show the damper pedal (legato changes, half-pedaling, half changes, touches), the soft pedal
(full and partial) and the sostenuto. The same files each run (seeded).

    python3 albums/demos/compose.py      # writes albums/demos/midi/NN-name.mid

The music is written in a small text notation (`Piece.seq`): tokens `PITCHES:DUR[MODS]`, where
PITCHES is a note (`Eb5`) or a chord (`E4+G4+C5`), `~` before a note adds a grace note a
semitone below, `r` is a rest; DUR is in beats (`1`, `.5`, `1/3`); MODS: `'` staccato, `>`
accent, `<` softer, `_` held to the next note. `|` separates bars for the reader."""
import math
import os
import random
import re
from fractions import Fraction

import mido

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "midi")
TPQ = 480
LEAD = 4                 # an empty bar before the music (beats; every piece is in 4/4)
COPYRIGHT = "CC0 1.0: dedicated to the public domain"

PC = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
SUSTAIN, SOSTENUTO, SOFT = 64, 66, 67


def pitch(name):
    m = re.fullmatch(r"([A-G])(#|b)?(-?\d)", name)
    if not m:
        raise ValueError(f"not a note: {name}")
    return 12 * (int(m[3]) + 1) + PC[m[1]] + {"#": 1, "b": -1, None: 0}[m[2]]


class Piece:
    def __init__(self, title, bpm, seed, swing=0.0, jitter_s=0.006):
        self.title = title
        self.tempo = [(0.0, float(bpm))]
        self.notes = []          # (start, end, pitch, velocity), in beats
        self.ccs = []            # (beat, controller, value)
        self.rng = random.Random(seed)
        self.swing = swing       # 0 straight … 1 triplet eighths
        self.jitter_s = jitter_s

    def bpm_at(self, t):
        bpm = self.tempo[0][1]
        for tt, b in sorted(self.tempo):
            if tt > t:
                break
            bpm = b
        return bpm

    def beats(self, t, secs):
        """secs as beats at t"""
        return secs * self.bpm_at(t) / 60

    def swung(self, t):
        if not self.swing:
            return t
        i = math.floor(t + 1e-9)
        f = t - i
        s = 0.5 + self.swing / 6
        f = f * s / 0.5 if f < 0.5 else s + (f - 0.5) * (1 - s) / 0.5
        return i + f

    # notes

    def note(self, t, dur, p, vel, swing=True, jitter=True):
        t0, t1 = (self.swung(t), self.swung(t + dur)) if swing else (t, t + dur)
        if jitter:
            t0 += self.beats(t0, self.rng.gauss(0, self.jitter_s))
            vel += self.rng.gauss(0, 3)
        self.notes.append((max(t0, 0.0), max(t1, t0 + 0.05), p, int(round(min(max(vel, 1), 127)))))

    def chord(self, t, dur, names, vel, swing=True, top=4):
        ps = sorted(pitch(n) for n in names.split("+"))
        for i, p in enumerate(ps):
            self.note(t, dur, p, vel + (top if i == len(ps) - 1 and len(ps) > 1 else 0), swing)

    def seq(self, t, text, vel, legato=0.92, swing=True, octave=0, top=4):
        """Plays the notation from beat t; vel is a number or a function of the beat. Returns the end."""
        for tok in text.split():
            if tok == "|":
                continue
            spec, _, rest = tok.partition(":")
            m = re.fullmatch(r"([\d./]+)([>'<_]*)", rest)
            if not m:
                raise ValueError(f"bad token: {tok}")
            dur = float(Fraction(m[1]))
            if spec != "r":
                mods = m[2]
                v = (vel(t) if callable(vel) else vel) + 14 * mods.count(">") - 12 * mods.count("<")
                lg = 0.45 if "'" in mods else 1.0 if "_" in mods else legato
                parts = sorted(((pitch(s.lstrip("~")), s.startswith("~")) for s in spec.split("+")))
                for i, (p, grace) in enumerate(parts):
                    vv = v + (top if i == len(parts) - 1 and len(parts) > 1 else 0)
                    if grace:
                        ts = self.swung(t) if swing else t
                        self.note(ts - self.beats(ts, 0.07), self.beats(ts, 0.06), p - 1, vv - 10, swing=False)
                    self.note(t, dur * lg, p, vv, swing)
                    if octave:
                        self.note(t, dur * lg, p + octave, vv - 10, swing)
            t += dur
        return t

    def gliss(self, t, beats, lo, hi, v0, v1):
        """A glissando over the white keys from lo to hi."""
        keys = [p for p in range(pitch(lo), pitch(hi) + 1) if p % 12 in (0, 2, 4, 5, 7, 9, 11)]
        for i, p in enumerate(keys):
            x = i / (len(keys) - 1)
            self.note(t + beats * x, beats / len(keys) * 2, p, v0 + (v1 - v0) * x, swing=False, jitter=False)

    # pedals

    def cc(self, t, ctl, val):
        self.ccs.append((t, ctl, int(round(min(max(val, 0), 127)))))

    def ramp(self, t, ctl, v0, v1, secs):
        """A pedal moved from v0 to v1 over secs (smoothstep, a value every ~12 ms)."""
        n = max(2, round(secs / 0.012))
        span = self.beats(t, secs)
        for i in range(n + 1):
            x = i / n
            self.cc(t + span * i / n, ctl, v0 + (v1 - v0) * x * x * (3 - 2 * x))

    def pedal(self, t, v0, v1, secs=0.08):
        self.ramp(t, SUSTAIN, v0, v1, secs)

    def change(self, t, down=127, up=0):
        """A legato change at a harmony struck at t: up just after the strike, down again. With up
        above 0 a half change: the dampers touch the strings only briefly, the bass keeps ringing."""
        self.ramp(t + self.beats(t, 0.03), SUSTAIN, down, up, 0.07)
        self.ramp(t + self.beats(t, 0.15), SUSTAIN, up, down, 0.10)

    def touch(self, t, hold=0.3, depth=127):
        """A short pedal on an accented chord (t in straight beats, swung here)."""
        ts = self.swung(t)
        self.ramp(ts + self.beats(ts, 0.01), SUSTAIN, 0, depth, 0.04)
        self.ramp(ts + self.beats(ts, hold), SUSTAIN, depth, 0, 0.06)

    def soft(self, t, v0, v1, secs=0.25):
        self.ramp(t, SOFT, v0, v1, secs)

    def sostenuto(self, t, on):
        self.cc(t, SOSTENUTO, 127 if on else 0)

    def rit(self, t0, t1, bpm1, steps=12):
        bpm0 = self.bpm_at(t0)
        for i in range(1, steps + 1):
            self.tempo.append((t0 + (t1 - t0) * i / steps, bpm0 + (bpm1 - bpm0) * i / steps))

    # the file

    def events(self, name="Piano", program=0, channel=0):
        """The part's track: (tick, order, message), a key struck again ending the sound before."""
        notes = sorted(self.notes, key=lambda n: (n[2], n[0]))
        ev = [(0, 0, mido.MetaMessage("track_name", name=name)),
              (0, 0, mido.Message("program_change", program=program, channel=channel))]
        for i, (t0, t1, p, v) in enumerate(notes):
            if i + 1 < len(notes) and notes[i + 1][2] == p:
                t1 = min(t1, notes[i + 1][0] - 1 / TPQ)
            if tick(t1) > tick(t0):
                ev.append((tick(t0), 3, mido.Message("note_on", note=p, velocity=v, channel=channel)))
                ev.append((tick(t1), 1, mido.Message("note_off", note=p, velocity=64, channel=channel)))
        for t, ctl, val in self.ccs:
            ev.append((tick(t), 2, mido.Message("control_change", control=ctl, value=val, channel=channel)))
        return ev

    def write(self, path, name="Piano", program=0):
        write_tracks(path, self.title, self.tempo, [self.events(name, program)])


def tick(t):
    """A beat of the music as a tick of the file, after the empty bar."""
    return int(round((t + LEAD) * TPQ))


def write_tracks(path, title, tempo, tracks):
    """A type 1 file: the tempo map, then the given tracks."""
    mid = mido.MidiFile(type=1, ticks_per_beat=TPQ)
    meta = [(0, 0, mido.MetaMessage("track_name", name=title)),
            (0, 0, mido.MetaMessage("copyright", text=COPYRIGHT)),
            (0, 0, mido.MetaMessage("time_signature", numerator=4, denominator=4))]
    for t, bpm in sorted(tempo):
        # the first tempo from the file's start: the empty bar in the music's tempo
        meta.append((tick(t) if t > 0 else 0, 1, mido.MetaMessage("set_tempo", tempo=mido.bpm2tempo(bpm))))
    for events in [meta] + tracks:
        track = mido.MidiTrack()
        now = 0
        for t, _, msg in sorted(events, key=lambda e: (e[0], e[1])):
            track.append(msg.copy(time=t - now))
            now = t
        track.append(mido.MetaMessage("end_of_track", time=0))
        mid.tracks.append(track)
    mid.save(path)


def boogie():
    """A 12-bar boogie-woogie in C: four choruses. The second drops to a quiet one on the soft
    pedal, the melody under a half-pressed damper pedal; the third has tremolos in half pedal and
    touches of full pedal on the shouts; the fourth starts in stop time."""
    P = Piece("Back Porch Boogie", 138, seed=1, swing=0.75)
    ROOT = {"C": 36, "F": 41, "G": 43}
    WALK = [0, 4, 7, 9, 10, 9, 7, 4]
    ROCK = [(0, 7), (0, 7), (0, 9), (0, 9), (0, 10), (0, 10), (0, 9), (0, 9)]
    # the right hand's riff chords per harmony: 3rd, 5th, root, 7th, 6th
    V = {"C": ("E4", "G4", "C5", "Bb4", "A4", "Eb5", "E5"),
         "F": ("A4", "C5", "F5", "Eb5", "D5", "Ab5", "A5"),
         "G": ("B4", "D5", "G5", "F5", "E5", "Bb5", "B5")}
    BLUES = "C C C C F F C C G F C G".split()

    def lh(bar, ch, vel, kind="walk"):
        r = ROOT[ch]
        for i in range(8):
            ps = [r + WALK[i]] if kind == "walk" else [r + x for x in ROCK[i]]
            for p in ps:
                P.note(4 * bar + i / 2, 0.45, p, vel + (6 if i % 2 == 0 else 0))

    def riff1(bar, ch, vel):
        a, b, c, b7, six, _, _ = V[ch]
        P.seq(4 * bar, f"~{a}+{b}+{c}:.5' {a}+{b}+{c}:1' {a}+{b}+{b7}:1' {a}+{b}+{six}:1.5", vel)

    def riff2(bar, ch, vel):
        a, b, c, _, _, m3, M3 = V[ch]
        P.seq(4 * bar, f"r:.5 {a}+{b}+{c}:.5' {a}+{b}+{c}:.5' {a}+{b}+{c}:1> r:.5 {m3}:.5 {M3}:.5", vel)

    def trem(t, beats, lo, hi, vel):
        for i in range(int(beats * 4)):
            P.chord(t + i / 4, 0.24, lo if i % 2 == 0 else hi, vel + (4 if i % 4 == 0 else 0), swing=False, top=0)

    # intro: the left hand alone, a pickup into the first chorus
    lh(0, "C", 62)
    lh(1, "C", 68)
    P.seq(4, "r:2 G4:.5 A4:.5 Bb4:.5 B4:.5", 70)

    # chorus 1: riffs, dry
    for i, ch in enumerate(BLUES):
        bar = 2 + i
        lh(bar, ch, 72)
        if i == 11:
            P.seq(4 * bar, "D5+F5+B5:.5' D5+F5+B5:1>' r:2.5", 82)
        elif i % 2 == 0:
            riff1(bar, ch, 80)
        else:
            riff2(bar, ch, 78)

    # chorus 2: quiet, una corda, a melody under half pedal (the dampers half on the strings)
    t = 4 * 14
    P.soft(t - 0.5, 0, 127)
    melody = ("r:.5 G4:.5 Bb4:.5 C5:1 r:.5 Eb5:.5 E5:.5 | G5:1.5 E5:.5 C5:1 r:1 | "
              "r:.5 C5:.5 Eb5:.5 E5:.5 G5:.5 A5:.5 G5:.5 E5:.5 | Bb5:2 A5:.5 G5:.5 E5:1 | "
              "Eb5:1.5 C5:.5 A4:1 C5:1 | Eb5:.5 D5:.5 C5:.5 A4:.5 F4:2 | "
              "r:.5 G4:.5 C5:.5 E5:.5 G5:1 E5:1 | Eb5:.5 C5:.5 Bb4:.5 G4:.5 C5:2 | "
              "D5:1 F5:1 B5:1.5 A5:.5 | A5:1.5 F5:.5 Eb5:1 C5:1 | E5:.5 G5:.5 C6:1.5 G5:.5 E5:1 | "
              "D5:.5 B4:.5 G4:1 G4:.5> A4:.5> Bb4:.5> B4:.5>")
    P.seq(t, melody, lambda b: 56 + 10 * (b - t) / 48, legato=0.98)
    P.pedal(t, 0, 58)
    for i, ch in enumerate(BLUES):
        lh(14 + i, ch, 46 + i)
        if i:
            P.change(t + 4 * i, down=58)
    P.soft(t + 45, 127, 0, 0.6)          # back up during the pickup
    P.pedal(t + 47.8, 58, 0)

    # chorus 3: loud, rocking left hand, tremolos in half pedal, full-pedal touches on the shouts
    t = 4 * 26
    for i, ch in enumerate(BLUES):
        lh(26 + i, ch, 82, "rock")
    trem(t, 7, "G5", "C6+E6", 84)
    P.pedal(t, 0, 64)
    P.pedal(t + 7, 64, 0)
    P.seq(t + 7, "r:.5 C6+E6+G6:.5'>", 92)
    P.seq(t + 8, "C6+E6:.5' C6+E6:.5' Bb5+E6:.5' C6+E6:1> r:1.5 | G5:.5 Bb5:.5 C6:.5 Eb6:.5 E6:.5 G6:.5 Bb6:1>", 90)
    trem(t + 16, 7, "A5", "C6+Eb6", 86)
    P.pedal(t + 16, 0, 64)
    P.pedal(t + 23, 64, 0)
    P.seq(t + 23, "r:.5 A5+C6+F6:.5'>", 94)
    P.seq(t + 24, "~E6+G6+C7:.5'> r:.5 E6+G6+C7:.5'> r:1 E6+G6+Bb6:.5'> r:1", 94)
    for x in (0, 1, 2.5):
        P.touch(t + 24 + x, 0.25)
    P.seq(t + 28, "C6:1/3 Bb5:1/3 G5:1/3 E5:1/3 Eb5:1/3 C5:1/3 Bb4:1/3 G4:1/3 E4:1/3", 88, swing=False)
    P.seq(t + 32, "B5+D6+G6:1.5> B5+D6+F6:.5' r:2 | A5+C6+F6:1.5> A5+C6+Eb6:.5' r:2", 94)
    P.touch(t + 32, 0.5)
    P.touch(t + 36, 0.5)
    P.seq(t + 40, "G5+C6+E6:1> F#5+C6+Eb6:1 F5+B5+D6:1 E5+Bb5+C#6:1 | D5+G5+B5:1> r:2 G4:.5 B4:.5", 90)

    # chorus 4: stop time, then the riffs again, a chromatic descent and the ending
    t = 4 * 38
    stops = ["E4+G4+Bb4+C5:1' G5:.5 E5:.5 Eb5:.5 C5:.5 r:1",
             "E4+G4+Bb4+C5:1' r:.5 C6:.5 Bb5:.5 G5:.5 Eb5:.5 E5:.5",
             "E4+G4+Bb4+C5:1'",
             "E4+G4+Bb4+C5:1' G4:.5 A4:.5 Bb4:.5 B4:.5 C5:.5 C#5:.5"]
    for i, s in enumerate(stops):
        P.chord(t + 4 * i, 0.45, "C2+C3", 96)
        P.seq(t + 4 * i, s, 90)
        P.touch(t + 4 * i, 0.22)
    P.seq(t + 9, "A5:1/3 G5:1/3 E5:1/3 G5:1/3 E5:1/3 C5:1/3 Eb5:1/3 C5:1/3 A4:1/3", 90, swing=False)
    for i, ch in enumerate(BLUES[4:10]):
        bar = 42 + i
        lh(bar, ch, 86 + i)
        (riff1 if i % 2 == 0 else riff2)(bar, ch, 92 + i)
    lh(48, "C", 90)
    P.seq(4 * 48, "C6+E6:.5 B5+Eb6:.5 Bb5+D6:.5 A5+C#6:.5 Ab5+C6:.5 G5+B5:.5 Gb5+Bb5:.5 F5+A5:.5", 94)
    P.rit(4 * 48, 4 * 50, 104)
    P.chord(4 * 49, 0.45, "C2+C3", 98)
    P.chord(4 * 49 + 1.5, 0.45, "Db2+Db3", 98)
    P.seq(4 * 49, "E5+G5+C6:1.5'> Db5+F5+B5:.5'> r:2", 98)
    P.touch(4 * 49, 0.2)
    P.touch(4 * 49 + 1.5, 0.2)
    end = 4 * 50
    P.pedal(end - 0.1, 0, 127)
    for i, n in enumerate(["C1", "C2", "G2"]):
        P.note(end + 0.02 * i, 6, pitch(n), 96, swing=False)
    P.seq(end + 0.08, "~E4+A4+D5+G5+C6:6", 92, swing=False)
    P.seq(end + 3, "C7:.5'", 52, swing=False)
    P.pedal(end + 7, 127, 0, 0.5)
    return P


def ballad():
    """A pop ballad in D-flat: intro, verse, chorus, verse, a big chorus, the intro again as
    the outro. Legato changes at every harmony; half changes in the big chorus; the soft pedal
    full in the intro and outro, half in the first verse."""
    P = Piece("Streetlights", 68, seed=2)
    ARP = {
        "Db": "Db2 Ab2 Db3 Eb3 F3 Eb3 Db3 Ab2",
        "Ab/C": "C2 Ab2 C3 Eb3 Ab3 Eb3 C3 Ab2",
        "Bbm": "Bb1 F2 Bb2 C3 Db3 C3 Bb2 F2",
        "Gb": "Gb1 Db2 Gb2 Ab2 Bb2 Ab2 Gb2 Db2",
        "Gbmaj7": "Gb1 Db2 F2 Ab2 Bb2 Ab2 F2 Db2",
        "Db/F": "F2 Ab2 Db3 Eb3 F3 Eb3 Db3 Ab2",
        "Ebm7": "Eb2 Bb2 Db3 F3 Gb3 F3 Db3 Bb2",
        "Absus": "Ab1 Eb2 Ab2 Bb2 Db3 Bb2 Ab2 Eb2",
        "Ab": "Ab1 Eb2 Ab2 Bb2 C3 Bb2 Ab2 Eb2",
    }

    def bars(t0, chords, vel, up=0, low=False):
        """The left hand's broken chords and a pedal change at each harmony."""
        t = t0
        for bar in chords:
            beats = 4 / len(bar)
            for ch in bar:
                ps = [pitch(n) for n in ARP[ch].split()][: int(beats * 2)]
                for i, p in enumerate(ps):
                    if i == 0:
                        P.note(t, beats * 0.98, p, vel + 8)
                        if low and p - 12 >= 28:
                            P.note(t, beats * 0.98, p - 12, vel)
                    else:
                        P.note(t + i / 2, 0.9, p, vel - 4 + (3 if i in (3, 4) else 0))
                if t > 0:
                    P.change(t, up=up)
                t += beats
        return t

    VERSE = [["Db"], ["Ab/C"], ["Bbm"], ["Gb"], ["Db"], ["Ab/C"], ["Bbm"], ["Gb", "Ab"]]
    CHORUS = [["Gb"], ["Db/F"], ["Ebm7"], ["Absus", "Ab"], ["Gb"], ["Db/F"], ["Bbm"], ["Ab"]]
    motif = ("F5:.5 Ab5:.5 Db6:1 Eb6:.5 Db6:.5 Ab5:1 | F5:.5 Ab5:.5 Db6:1 Eb6:.5 F6:.5 Db6:1 | "
             "F5:.5 Ab5:.5 Db6:1 Eb6:.5 Db6:.5 Ab5:1 | Eb5:.5 Ab5:.5 Db6:1 C6:2")
    verse = ("F5:1.5 Eb5:.5 Db5:1 Eb5:1 | F5:1 Ab5:1.5 F5:.5 Eb5:1 | Db5:2 r:.5 Db5:.5 Eb5:.5 F5:.5 | "
             "Gb5:1.5 F5:.5 Eb5:2 | F5:1.5 Eb5:.5 Db5:1 Ab4:1 | C5:1 Eb5:1.5 Db5:.5 C5:1 | "
             "Db5:1 Bb4:1 F5:1 Eb5:1 | Db5:2 Eb5:1 C5:1")
    chorus = ("r:.5 Bb5:.5 Bb5:.5 Ab5:.5 Bb5:1 Db6:1 | Ab5:2 r:.5 F5:.5 Ab5:.5 Bb5:.5 | "
              "Gb5:1.5 F5:.5 Eb5:1 F5:1 | Eb5:2 Db5:1 C5:1 | r:.5 Bb5:.5 Bb5:.5 Ab5:.5 Bb5:1 Db6:1 | "
              "Eb6:1.5 Db6:.5 Ab5:2 | r:.5 F5:.5 Gb5:.5 F5:.5 Db5:1 Bb4:1 | C5:2 Eb5:2")

    def swell(t0, v0, v1, beats):
        return lambda b: v0 + (v1 - v0) * min(max((b - t0) / beats, 0), 1)

    # intro: una corda, pp, the bass and fifth held
    P.soft(0, 127, 127, 0.01)
    P.pedal(0, 0, 127, 0.05)
    intro_lh = "Db2+Ab2:4 | Gb1+Db2:4 | Db2+Ab2:4 | Ab1+Eb2:2 Ab1+Eb2:2"
    P.seq(0, intro_lh, 40, legato=0.98)
    for t in (4, 8, 12, 14):
        P.change(t)
    P.seq(0, motif, 46, legato=1.0)

    # verse 1: the soft pedal half down, then up by the chorus
    t = 16
    P.soft(t - 0.3, 127, 64, 0.3)
    bars(t, VERSE, 42)
    P.seq(t, verse, swell(t, 54, 62, 32))
    P.soft(t + 28, 64, 0, 2.0)

    # chorus 1
    t = 48
    bars(t, CHORUS, 52)
    P.seq(t, chorus, swell(t, 66, 74, 24))

    # interlude: the intro's motif over the broken chords
    t = 80
    bars(t, [["Db"], ["Gbmaj7"]], 48)
    P.seq(t, motif.split("|")[0] + "|" + motif.split("|")[1], 58, legato=1.0)

    # verse 2: the melody doubled an octave down
    t = 88
    bars(t, VERSE, 50)
    P.seq(t, verse, swell(t, 60, 68, 32), octave=-12)

    # chorus 2: in octaves, the bass doubled, half changes keep the low strings ringing
    t = 120
    bars(t, CHORUS, 62, up=45, low=True)
    P.seq(t, chorus, lambda b: 82 + (8 if 140 <= b < 144 else 0), octave=-12)

    # outro: una corda again, the motif, a held D-flat add9
    t = 152
    P.change(t)
    P.soft(t - 1, 0, 127, 0.6)
    P.seq(t, intro_lh.replace("Ab1+Eb2:2 Ab1+Eb2:2", "Gb1+Db2:4"), 38, legato=0.98)
    for x in (4, 8, 12):
        P.change(t + x)
    P.seq(t, motif.rsplit("|", 1)[0] + "| F5:.5 Ab5:.5 Db6:1 Eb6:2", lambda b: 46 - (b - t) / 2, legato=1.0)
    P.rit(t + 8, t + 16, 56)
    end = t + 16
    P.change(end)
    P.seq(end, "Db2+Ab2:8", 36)
    P.seq(end + 0.12, "F4+Ab4+Db5+Eb5+Ab5:8", 38)
    P.pedal(end + 7.5, 127, 0, 0.8)
    P.soft(end + 8, 127, 0)
    return P


def minimal():
    """A minimalist piece in A minor that grows layer by layer: a three-note cell in sixteenths
    runs against the 4/4 bar; a low A held by the sostenuto rings under every harmony while the
    damper pedal, half pressed, changes at each bar, and the soft pedal lifts in steps. Two builds
    lead to climaxes in broken octaves over a driving bass (full pedal, full changes); between
    them the drone returns, and the coda turns the cell to A major."""
    P = Piece("Glass Rain", 96, seed=3)
    CELL3 = {"Am": "A4 E5 C5", "F": "A4 F5 C5", "C": "G4 E5 C5", "G": "G4 D5 B4", "E": "G#4 E5 B4",
             "A": "A4 E5 C#5", "D/A": "A4 F#5 D5"}
    CELL4 = {"F": "F4 C5 F5 A5", "G": "G4 D5 G5 B5", "E": "G#4 B4 E5 G#5"}
    BASS = {"Am": "A2+E3", "F": "F2+C3", "C": "C3+G3", "G": "G2+D3", "E": "E2+B2", "A": "A2+E3", "D/A": "A2+D3"}
    ROOT = {"Am": 45, "F": 41, "C": 36, "G": 43, "E": 40, "Asus": 45, "A": 45}
    TONES = {"Am": (9, 0, 4), "F": (5, 9, 0), "C": (0, 4, 7), "G": (7, 11, 2), "E": (4, 8, 11),
             "Asus": (9, 2, 4), "A": (9, 1, 4)}
    RING = ["Am", "F", "C", "G"] * 2
    phase = [0]

    def cells(bar, ch, vel, shift=0, per_bar=16):
        """The three-note cell, its phase running on across the bars."""
        notes = [pitch(n) + shift for n in CELL3[ch].split()]
        step = 4 / per_bar
        for i in range(per_bar):
            accent = 6 if i % (per_bar // 4) == 0 else 0
            P.note(4 * bar + i * step, step * 1.6, notes[phase[0] % 3], vel + accent)
            phase[0] += 1

    def cells4(bar, ch, vel, grow=0, n=16):
        """A four-note cell in time with the bar, its top accented."""
        ns = [pitch(x) for x in CELL4[ch].split()]
        for k in range(n):
            v = vel + grow * k / 16 + (4 if k % 4 == 3 else -6 if k % 4 else 0)
            P.note(4 * bar + k / 4, 0.4, ns[k % 4], v)

    def pulse(t, ch, vel, beats=4):
        """The left hand's driving eighths: the root in octaves on the one, fifths and octaves after."""
        r = ROOT[ch]
        P.note(t, beats, r - 12, vel + 6)
        for i in range(int(beats * 2)):
            P.note(t + i / 2, 0.45, (r, r + 7, r + 12, r + 7)[i % 4], vel + (4 if i % 2 == 0 else -2))

    def waves(t, ch, mels, vel, octave=False):
        """The climax's right hand: per half note a melody note on each beat and the chord falling
        below it in sixteenths; with octave the melody in octaves, the chord inside them."""
        for h, name in enumerate(mels.split()):
            m = pitch(name)
            inside = [p for p in range(m - 11, m) if p % 12 in TONES[ch]]
            c1, c2 = inside[0], inside[-1]
            for k in range(8):
                tk = t + 2 * h + k / 4
                j = k % 4
                if j == 0:
                    P.note(tk, 0.45, m, vel + 12 - (4 if k else 0))
                    if octave:
                        P.note(tk, 0.45, m - 12, vel)
                else:
                    P.note(tk, 0.3, (c2, c1, c2 if octave else m - 12)[j - 1], vel - 6)

    def drone(t, vel):
        P.chord(t, 2, "A1+A2", vel, top=0)
        P.sostenuto(t + 0.5, True)

    melody = ("E5:3 D5:.5 C5:.5 | C5:2 A4:2 | G4:1.5 C5:.5 E5:2 | D5:4 | "
              "E5:3 G5:1 | A5:2 F5:2 | E5:1.5 D5:.5 C5:2 | B4:2 D5:2")

    # intro: the drone, the cell alone, first in eighths; una corda, half pedal changed shallowly
    P.soft(0, 127, 127, 0.01)
    drone(0, 40)
    P.pedal(0.6, 0, 64, 0.2)
    for i in range(4):
        cells(i, "Am", 30 + 2 * i, per_bar=8 if i < 2 else 16)
        if i:
            P.change(4 * i, down=64, up=30)

    # A: the harmonies under the cell
    for i, ch in enumerate(RING):
        bar = 4 + i
        cells(bar, ch, 40 + i)
        P.chord(4 * bar, 3.9, BASS[ch], 36 + i // 2)
        P.change(4 * bar, down=64, up=30)

    # B: the cell moves to the left hand, the right hand sings; the soft pedal lifts by steps
    t = 4 * 12
    P.seq(t, melody, lambda b: 54 + (b - t) / 3, legato=1.0)
    for i, ch in enumerate(RING):
        cells(12 + i, ch, 44 + i, shift=-12)
        P.change(4 * (12 + i), down=84, up=20)
    P.soft(t, 127, 80, 1.0)
    P.soft(t + 8, 80, 40, 1.0)
    P.soft(t + 16, 40, 0, 1.0)

    # B': the melody an octave up, in octaves; the cell below, the drone under both
    t = 4 * 20
    P.seq(t, "E6:3 D6:.5 C6:.5 | C6:2 A5:1 C6:1 | G5:1.5 C6:.5 E6:2 | D6:3 B5:1 | "
             "E6:3 G6:1 | A6:2 F6:2 | E6:1.5 D6:.5 C6:1 E6:1 | D6:2 B5:1 D6:1",
          lambda b: 62 + (b - t) / 3, legato=1.0, octave=-12)
    for i, ch in enumerate(RING):
        cells(20 + i, ch, 52 + i, shift=-12)
        P.change(4 * (20 + i), down=84, up=20)

    # build: the drone let go, the bass in the left hand, then four-note cells over driving eighths
    t = 4 * 28
    P.sostenuto(t - 0.2, False)
    for i, ch in enumerate(["F", "G", "Am", "C", "F", "G", "E", "E"]):
        bar = 28 + i
        v = 60 + 4 * i
        if i < 4:
            cells(bar, ch, v)
            P.chord(4 * bar, 3.9, BASS[ch], v - 6)
        else:
            cells4(bar, ch, v, grow=12 if i == 7 else 0)
            pulse(4 * bar, ch, v - 4)
        P.change(4 * bar)

    # climax 1: broken chords under the melody, the bass driving, full changes
    t = 4 * 36
    for i, (ch, m) in enumerate(zip(["Am", "F", "C", "G", "Am", "F", "G", "E"],
                                    ["E5 A5", "C6 A5", "G5 E5", "D5 G5", "E5 A5", "C6 F6", "D6 B5", "G#5 B5"])):
        v = 88 + min(i, 4) * 2
        pulse(t + 4 * i, ch, v - 6)
        waves(t + 4 * i, ch, m, v)
        P.change(t + 4 * i)

    # breakdown: pp at once, una corda, the drone again; the cell high, the melody low in the left hand
    t = 4 * 44
    P.change(t, down=64, up=0)
    P.soft(t - 0.4, 0, 127, 0.3)
    drone(t, 38)
    phase[0] = 0
    for i, ch in enumerate(RING):
        cells(44 + i, ch, 34, shift=12)
        if i:
            P.change(4 * (44 + i), down=64, up=30)
    low = re.sub(r"[A-G][#b]?\d", lambda m: str_pitch(pitch(m[0]) - 12), melody)
    P.seq(t, low, lambda b: 44 + (b - t) / 4, legato=1.0)

    # build 2: the drone and the soft pedal let go, four bars of crescendo, a breath before the top
    t = 4 * 52
    P.sostenuto(t - 0.2, False)
    P.soft(t, 127, 0, 2.5)
    for i, ch in enumerate(["F", "G", "E", "E"]):
        bar = 52 + i
        v = 56 + 12 * i
        last = i == 3
        cells4(bar, ch, v, grow=12, n=12 if last else 16)
        pulse(4 * bar, ch, v - 4, beats=3 if last else 4)
        P.change(4 * bar)
    P.pedal(4 * 55 + 3, 127, 0, 0.1)

    # climax 2: the melody in octaves, a step higher; A0 under the first chord; the end in A major
    t = 4 * 56
    P.pedal(t + P.beats(t, 0.05), 0, 127, 0.08)
    P.note(t, 4, pitch("A0"), 104)
    for i, (ch, m) in enumerate(zip(["Am", "F", "C", "G", "F", "G", "Asus", "A"],
                                    ["A5 E6", "C6 F6", "E6 G6", "D6 B5", "A5 C6", "D6 G6", "E6 D6", "C#6 E6"])):
        v = 98 + min(i, 5) * 2
        pulse(t + 4 * i, ch, v - 4)
        waves(t + 4 * i, ch, m, v, octave=True)
        if i:
            P.change(t + 4 * i)

    # coda: a rolled A major over the whole keyboard, left to ring; the cell returns in major
    t = 4 * 64
    P.change(t)
    roll = "A0 A1 E2 A2 C#3 E3 A3 C#4 E4 A4 C#5 E5 A5".split()
    for i, n in enumerate(roll):
        P.note(t + 0.07 * i, 8, pitch(n), 108 - 2 * i, swing=False)
    phase[0] = 0
    cells(65, "A", 30, shift=12)
    P.soft(4 * 66 - 0.4, 0, 127, 0.3)
    P.change(4 * 66, down=64, up=0)
    drone(4 * 66, 34)
    for i, ch in enumerate(["A", "D/A", "A", "E", "A", "D/A"]):
        bar = 66 + i
        cells(bar, ch, 40 - 2 * i)
        P.chord(4 * bar, 3.9, BASS[ch], 32 - i)
        if i:
            P.change(4 * bar, down=64, up=30)
    P.seq(4 * 66, "E6:3 F#6:1 | D6:4 | C#6:2 A5:2 | B5:4", 40, legato=1.0)
    P.rit(4 * 71, 4 * 74, 72)
    P.change(4 * 72, down=64, up=30)
    cells(72, "A", 28, per_bar=8)
    end = 4 * 73
    P.pedal(end - 0.05, 64, 0, 0.05)
    P.pedal(end + 0.1, 0, 127, 0.1)
    P.seq(end, "A3+E4+B4+C#5+E5:8", 32, top=6)
    P.seq(end + 2, "E6:6", 28)
    P.pedal(end + 9, 127, 0, 0.8)
    P.sostenuto(end + 10, False)
    P.soft(end + 10, 127, 0)
    return P


def montuno():
    """A son montuno in C: the right hand's syncopated octaves over C-F-G-F, the left hand's
    anticipated bass. Dry in the grooves; the coro on the soft pedal with half pedal; touches on
    the mambo's clave hits; a glissando into the last chord in full pedal."""
    P = Piece("Ferry Montuno", 176, seed=4, jitter_s=0.004)
    MON = {"C": ("E4+E5", "G4+C5", "G4+G5", "C5+E5"),
           "F": ("A4+A5", "C5+F5", "F4+F5", "A4+C5"),
           "G": ("B4+B5", "D5+G5", "G4+G5", "B4+D5")}
    BASSN = {"C": (36, 31), "F": (41, 36), "G": (43, 38)}
    CYCLE = ["C", "F", "G", "F"]

    def transpose(names, k):
        return "+".join(str_pitch(pitch(n) + k) for n in names.split("+"))

    def guajeo(bar, cycles, vel, up=0, octaves=True):
        """The right hand's pattern: per harmony an anticipated octave, a dyad, an octave, a dyad."""
        for c in range(cycles):
            for k, ch in enumerate(CYCLE):
                tc = 4 * bar + 8 * c + 2 * k
                o1, d1, o2, d2 = MON[ch]
                if not octaves:
                    o1, o2 = o1.split("+")[1], o2.split("+")[0]
                for x, names, v in ((-0.5, o1, vel + 10), (0, d1, vel - 6), (0.5, o2, vel), (1.0, d2, vel - 6)):
                    P.chord(tc + x, 0.3, transpose(names, up), v, top=2)

    def tumbao(bar, cycles, vel, octaves=False):
        for c in range(cycles):
            for k, ch in enumerate(CYCLE):
                tc = 4 * bar + 8 * c + 2 * k
                root, fifth = BASSN[ch]
                for p, x, d in ((root, -0.5, 1.4), (fifth, 1.0, 0.45)):
                    P.note(tc + x, d, p, vel + (6 if x < 0 else 0))
                    if octaves:
                        P.note(tc + x, d, p + 12, vel - 8)

    def comp(bar, cycles, vel):
        """The left hand under the coro: the bass anticipated, a dyad on the upbeat."""
        for c in range(cycles):
            for k, ch in enumerate(CYCLE):
                tc = 4 * bar + 8 * c + 2 * k
                P.note(tc - 0.5, 1.4, BASSN[ch][0], vel + 6)
                P.chord(tc + 1.0, 0.4, transpose(MON[ch][3], -12), vel - 6)
                P.change(tc, down=56, up=10)

    coro = ("E5:1 G5:.5 E5:.5 F5:1.5 A5:.5 | G5:1.5 D5:.5 F5:1 E5:1 | "
            "E5:1 G5:.5 C6:.5 A5:1.5 F5:.5 | D5:1 G5:1 C5:2 | "
            "E5:.5 E5:.5 G5:.5 E5:.5 F5:.5 A5:1 C6:.5 | B5:1 G5:1 A5:2 | "
            "G5:.5 E5:.5 C5:.5 E5:.5 F5:1 A5:1 | G5:1.5 B4:.5 C5:2")

    # intro (the pickup in bar 0): the pattern alone
    guajeo(1, 2, 70)
    # A
    guajeo(5, 4, 74)
    tumbao(5, 4, 78)
    # B: the coro on the soft pedal, half pedal changed at each harmony
    P.soft(4 * 13 - 0.9, 0, 127, 0.2)
    P.pedal(4 * 13 - 0.4, 0, 56)
    comp(13, 4, 56)
    P.seq(4 * 13, coro, 64, legato=0.95)
    P.pedal(4 * 21 - 0.6, 56, 0)
    P.soft(4 * 21 - 1.2, 127, 0, 0.3)
    # A again: the bass in octaves
    guajeo(21, 4, 82)
    tumbao(21, 4, 84, octaves=True)
    # mambo: the pattern an octave up, then a break on the clave in G7 with full-pedal touches
    guajeo(29, 3, 88, up=12)
    tumbao(29, 3, 90, octaves=True)
    t = 4 * 35
    for x in (0, 1.5, 3, 5, 6):
        P.chord(t + x, 0.4, "G2+G3", 100, top=0)
        P.chord(t + x, 0.4, "F4+B4+D5+G5", 98)
        P.touch(t + x, 0.18)
    P.gliss(t + 6.9, 0.55, "G4", "G6", 50, 86)
    # B again: louder, the coro in octaves, no soft pedal
    P.pedal(4 * 37 - 0.4, 0, 56)
    comp(37, 4, 70)
    P.seq(4 * 37, coro, 82, legato=0.95, octave=-12)
    P.pedal(4 * 45 - 0.6, 56, 0)
    # the last round, three hits, a glissando into the last chord
    guajeo(45, 1, 88)
    tumbao(45, 1, 90, octaves=True)
    t = 4 * 47
    for x in (0, 1.5, 3):
        P.chord(t + x, 0.4, "C2+C3", 102, top=0)
        P.chord(t + x, 0.4, "E4+A4+D5+G5", 100)
        P.touch(t + x, 0.15)
    end = t + 5
    P.gliss(end - 0.8, 0.75, "C4", "C7", 46, 84)
    P.pedal(end - 0.05, 0, 127, 0.05)
    P.chord(end, 6, "C1+C2+G2", 106, top=0)
    P.chord(end, 6, "E5+G5+C6+E6+G6", 104)
    P.pedal(end + 6, 127, 0, 0.5)
    return P


PC_NAMES = {"C": 0, "C#": 1, "D": 2, "Eb": 3, "E": 4, "F": 5, "F#": 6, "G": 7, "Ab": 8, "A": 9, "Bb": 10, "B": 11}

# the ensemble's parts: (stem, track name, General MIDI program)
PARADE_PARTS = [("piano", "Piano", 0), ("vibraphone", "Vibraphone", 11), ("marimba", "Marimba", 12),
                ("harp", "Harp", 46), ("kalimba", "Kalimba", 108), ("bells", "Glockenspiel", 9),
                ("chimes", "Tubular Bells", 14)]


def parade():
    """An ensemble piece in D: a kalimba riff on a drone tine, a marimba groove, vibraphone pads
    in pedal, piano melody and harp arpeggios; a quiet piano interlude on the soft pedal in half
    pedal, a build, everyone together with bells and tubular bells, the riff alone at the end.
    One part per instrument, each kept in its instrument's recorded range."""
    parts = {stem: Piece("Tin Roof Parade", 112, seed=10 + i) for i, (stem, _, _) in enumerate(PARADE_PARTS)}
    tempo = parts["piano"].tempo
    for part in parts.values():
        part.tempo = tempo
    pn, vb, mb, hp, kl, bl, tb = (parts[s] for s, _, _ in PARADE_PARTS)

    # each harmony: its chord tones (root first) and the bass's pitch class
    CH = {"D": ("D F# A", "D"), "A/C#": ("A C# E", "C#"), "A": ("A C# E", "A"), "Bm": ("B D F#", "B"),
          "G": ("G B D", "G"), "F#m": ("F# A C#", "F#"), "Em": ("E G B", "E")}
    # the kalimba's riff per harmony: a middle tine, A4 the drone tine, a high tine, an added one
    RIFF = {"D": "D5 F#5 E5", "A/C#": "C#5 E5 B4", "A": "C#5 E5 B4", "Bm": "D5 F#5 C#5",
            "G": "D5 G5 B4", "F#m": "C#5 F#5 E5", "Em": "E5 G5 B4"}
    INTRO = ["D", "A/C#", "Bm", "G"] * 2
    VERSE = ["D", "A/C#", "Bm", "G", "D", "A/C#", "G", "A"]
    BRIDGE = ["G", "A", "F#m", "Bm", "G", "A", "Bm", "D"]
    QUIET = ["Bm", "G", "D", "A", "Bm", "G", "Em", "A"]
    BUILD = ["G", "A", "Bm", "A"]
    OUTRO = ["D", "G", "D", "A"]

    def pcs(ch):
        return [PC_NAMES[n] for n in CH[ch][0].split()]

    def low(pc, lo):
        """The lowest key at or above lo of pitch class pc."""
        return lo + (pc - lo) % 12

    def stack(ch, lo, n):
        """n chord tones upward from lo."""
        return [p for p in range(lo, lo + 36) if p % 12 in pcs(ch)][:n]

    def bass(ch, lo):
        return low(PC_NAMES[CH[ch][1]], lo)

    def riff(bar, ch, vel):
        mid, high, add = (pitch(n) for n in RIFF[ch].split())
        for i, p in enumerate([mid, 69, high, 69, add, 69, high, 69]):
            kl.note(4 * bar + i / 2, 0.9, p, vel + (6 if i % 4 == 0 else 0) - (4 if p == 69 else 0))

    def groove(bar, ch, vel):
        """The marimba's bass line: the root, its octave and the fifth, syncopated."""
        r = bass(ch, 38)
        fifth = next(p for p in range(r + 1, r + 12) if p % 12 == (pcs(ch)[0] + 7) % 12)
        for x, p in ((0, r), (0.5, r + 12), (1.5, fifth), (2, r + 12), (3, fifth), (3.5, r + 12)):
            mb.note(4 * bar + x, 0.4, p, vel + (8 if x == 0 else 0))

    def sixteenths(bar, ch, vel, grow=0):
        ns = stack(ch, bass(ch, 50), 6)
        order = ns + ns[-2:0:-1]
        for k in range(16):
            mb.note(4 * bar + k / 4, 0.24, order[k % len(order)], vel + grow * k / 16 + (6 if k % 4 == 0 else 0))

    def pads(bar, ch, vel, n=4):
        vb.chord(4 * bar, 3.9, "+".join(str_pitch(p) for p in stack(ch, 55, n)), vel, top=2)
        vb.change(4 * bar)

    def arpeggio(bar, ch, vel, lo=43, step=0.5):
        ns = stack(ch, bass(ch, lo), int(4 / step))
        for i, p in enumerate(ns):
            hp.note(4 * bar + i * step, 4 - i * step, p, vel + (6 if i == 0 else 0))
        hp.change(4 * bar)

    def rolled(bar, ch, vel):
        for i, p in enumerate([bass(ch, 38)] + stack(ch, bass(ch, 50), 5)):
            hp.note(4 * bar + 0.08 * i, 4, p, vel, swing=False)
        hp.change(4 * bar)

    def harp_gliss(t, beats, lo, hi, v0, v1):
        """A glissando over the D major strings."""
        scale = [p for p in range(pitch(lo), pitch(hi) + 1) if p % 12 in (2, 4, 6, 7, 9, 11, 1)]
        for i, p in enumerate(scale):
            x = i / (len(scale) - 1)
            hp.note(t + beats * x, 2, p, v0 + (v1 - v0) * x, swing=False, jitter=False)

    def comp(bar, ch, vel, octave_bass=True):
        """The piano's left hand: the bass in octaves, a chord on the and of 2 and on 4."""
        b = bass(ch, 38)
        pn.note(4 * bar, 3.9, b, vel + 6)
        if octave_bass:
            pn.note(4 * bar, 3.9, b + 12, vel)
        chord = "+".join(str_pitch(p) for p in stack(ch, 53, 3))
        pn.chord(4 * bar + 1.5, 1, chord, vel - 8, top=0)
        pn.chord(4 * bar + 3, 1, chord, vel - 10, top=0)
        pn.change(4 * bar)

    def broken(bar, ch, vel):
        """The piano's left hand in the interlude: a broken chord in eighths."""
        r = bass(ch, 38)
        up = [p for p in range(r + 12, r + 30) if p % 12 in pcs(ch)]
        for i, p in enumerate([r, r + 7 if (r + 7) % 12 in pcs(ch) else up[0]] + up[:3] + up[1::-1] + [r + 12]):
            pn.note(4 * bar + i / 2, 0.9, p, vel + (6 if i == 0 else 0))

    melody = ("F#5:1.5 E5:.5 D5:1 A4:1 | E5:1.5 F#5:.5 E5:1 C#5:1 | D5:1 F#5:1 B5:1.5 A5:.5 | "
              "G5:2 r:1 F#5:.5 G5:.5 | A5:1.5 F#5:.5 D5:1 F#5:1 | E5:1 C#5:1 E5:1 A5:1 | "
              "B5:1.5 A5:.5 G5:1 E5:1 | A5:1 G5:.5 F#5:.5 E5:2")
    counter = ("B4:2 D5:1 G5:1 | E5:2 C#5:2 | C#5:1 E5:1 A5:2 | F#5:4 | B4:2 D5:1 G5:1 | "
               "A5:2 E5:2 | F#5:2 E5:2 | D5:4")
    quiet = ("F#5:2 D5:1 B4:1 | D5:1.5 B4:.5 G4:2 | A4:1 D5:1 F#5:1 A5:1 | G5:2 E5:2 | "
             "F#5:2 B5:1 A5:1 | G5:2 D5:2 | E5:1 G5:1 B5:1 G5:1 | D5:2 C#5:2")

    def up(text, k):
        return re.sub(r"[A-G][#b]?\d", lambda m: str_pitch(pitch(m[0]) + k), text)

    # intro: the kalimba alone, then the marimba, the vibraphone's first chord on the last bar
    for i, ch in enumerate(INTRO):
        riff(i, ch, 70)
        if i >= 4:
            groove(i, ch, 60 + 2 * i)

    # verse: the piano's melody, the vibraphone's pads in pedal
    t = 4 * 8
    vb.pedal(t - 0.1, 0, 127, 0.05)
    for i, ch in enumerate(VERSE):
        bar = 8 + i
        riff(bar, ch, 66)
        groove(bar, ch, 72)
        pads(bar, ch, 50)
        comp(bar, ch, 56, octave_bass=False)
    pn.seq(t, melody, 72)

    # bridge: the vibraphone sings, the harp's arpeggios, the bells join the second half
    t = 4 * 16
    for i, ch in enumerate(BRIDGE):
        bar = 16 + i
        groove(bar, ch, 74)
        arpeggio(bar, ch, 58)
        comp(bar, ch, 58)
    vb.seq(t, counter, 74, legato=1.0)
    for i in range(8):
        vb.change(t + 4 * i)
    bl.seq(t + 16, up(counter.split("|", 4)[4], 12), 50, legato=1.0)

    # interlude: the piano almost alone, una corda, in half pedal; harp chords, the riff comes back
    t = 4 * 24
    pn.soft(t - 0.5, 0, 127)
    pn.pedal(t - 0.2, 127, 0, 0.05)
    pn.pedal(t + 0.3, 0, 70)
    for i, ch in enumerate(QUIET):
        bar = 24 + i
        broken(bar, ch, 44)
        rolled(bar, ch, 30)
        if i:
            pn.change(4 * bar, down=70, up=30)
        vb.chord(4 * bar, 3.9, "+".join(str_pitch(p) for p in stack(ch, 62, 3)), 36, top=0)
        vb.change(4 * bar)
        if i >= 4:
            riff(bar, ch, 40 + 4 * (i - 4))
    pn.seq(t, quiet, lambda b: 54 + (b - t) / 4, legato=1.0)
    pn.soft(t + 28, 127, 0, 1.0)

    # build: marimba sixteenths, the piano's octaves pulsing, a harp glissando into the top
    t = 4 * 32
    pn.pedal(t - 0.1, 70, 0, 0.05)
    for i, ch in enumerate(BUILD):
        bar = 32 + i
        v = 62 + 8 * i
        sixteenths(bar, ch, v, grow=8)
        riff(bar, ch, 62 + 4 * i)
        b = bass(ch, 38)
        for k in range(8):
            pn.chord(4 * bar + k / 2, 0.45, f"{str_pitch(b)}+{str_pitch(b + 12)}", v + (6 if k % 2 == 0 else 0), top=0)
        pn.chord(4 * bar, 1.9, "+".join(str_pitch(p) for p in stack(ch, 62, 3)), v)
        pn.chord(4 * bar + 2, 1.9, "+".join(str_pitch(p) for p in stack(ch, 62, 3)), v + 4)
        pn.change(4 * bar)
        vb.chord(4 * bar, 3.9, "+".join(str_pitch(p) for p in stack(ch, 55, 4)), 46 + 8 * i, top=0)
        vb.change(4 * bar)
    harp_gliss(4 * 35 + 1.5, 2.4, "D3", "D6", 50, 90)

    # everyone: the melody in the piano's octaves and the bells, tubular bells on the phrases
    t = 4 * 36
    for i, ch in enumerate(VERSE):
        bar = 36 + i
        riff(bar, ch, 76)
        groove(bar, ch, 84)
        pads(bar, ch, 62)
        arpeggio(bar, ch, 64, lo=50, step=0.5)
        comp(bar, ch, 72)
    pn.seq(t, melody, 88, octave=-12)
    bl.seq(t, up(melody, 12), 66)
    for x, n in ((0, "D5"), (16, "A4"), (24, "G4"), (28, "A4")):
        tb.note(t + x, 4, pitch(n), 80)

    # outro: the riff and the groove fall back, a last chord for everyone
    t = 4 * 44
    for i, ch in enumerate(OUTRO):
        bar = 44 + i
        riff(bar, ch, 70 - 4 * i)
        if i < 2:
            groove(bar, ch, 66 - 6 * i)
    vb.pedal(t - 0.1, 127, 0, 0.1)
    tempo_end = 4 * 48
    pn.rit(4 * 46, tempo_end, 92)
    harp_gliss(tempo_end - 1.2, 1.1, "D4", "A6", 46, 80)
    end = tempo_end
    pn.pedal(end - 0.1, 0, 127, 0.05)
    pn.chord(end, 8, "D1+D2+A2", 86, top=0)
    pn.seq(end + 0.05, "D4+F#4+A4+D5+F#5:8", 80, swing=False)
    vb.chord(end, 8, "F#4+A4+D5+E5", 64, top=0)
    vb.pedal(end + 0.05, 0, 127, 0.05)
    for k in range(24):
        mb.note(end + k / 6, 0.15, pitch("D4") if k % 2 == 0 else pitch("A4"), 66 - 2 * k, swing=False)
    kl.chord(end, 4, "A4+D5+F#5", 70, top=0)
    hp.chord(end, 8, "D3+A3+D4+F#4", 72, top=0)
    hp.pedal(end, 0, 127, 0.05)
    bl.note(end, 6, pitch("D7"), 70)
    tb.note(end, 8, pitch("D5"), 82)
    for part in (pn, vb, hp):
        part.pedal(end + 8, 127, 0, 0.6)
    return parts


def str_pitch(p):
    names = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"]
    return f"{names[p % 12]}{p // 12 - 1}"


PIECES = [("01-back-porch-boogie", boogie), ("02-streetlights", ballad),
          ("03-glass-rain", minimal), ("04-ferry-montuno", montuno), ("05-tin-roof-parade", parade)]


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, make in PIECES:
        piece = make()
        path = os.path.join(OUT, f"{name}.mid")
        if isinstance(piece, dict):
            # an ensemble: one file per part (the stems midi2wav renders) and all parts in one
            os.makedirs(os.path.join(OUT, name), exist_ok=True)
            first = next(iter(piece.values()))
            tracks = []
            for ch, (stem, track, program) in enumerate(PARADE_PARTS):
                piece[stem].write(os.path.join(OUT, name, f"{stem}.mid"), track, program)
                tracks.append(piece[stem].events(track, program, ch))
            write_tracks(path, first.title, first.tempo, tracks)
            notes = sum(len(p.notes) for p in piece.values())
            ccs = sum(len(p.ccs) for p in piece.values())
            title = f"{first.title} ({len(piece)} parts)"
        else:
            piece.write(path)
            notes, ccs, title = len(piece.notes), len(piece.ccs), piece.title
        length = mido.MidiFile(path).length
        print(f"{name}.mid  {title}: {notes} notes, {ccs} pedal events, {length // 60:.0f}:{length % 60:04.1f}")


if __name__ == "__main__":
    main()
