#!/usr/bin/env python3
"""Composes the brass pieces for a sextet (two trumpets, horn, tenor and bass trombone, tuba) and
writes them as Standard MIDI files, one per stem, for midi2wav one at a time, and one per piece with
a track each, for a DAW. The same files each run (seeded).

    python3 albums/brass/compose.py      # writes albums/brass/midi/NN-name/<stem>.mid and NN-name.mid

A stem is one part in one mute: the mute is a setting of the render (`mix.py`), the rest is in the
MIDI as the plugin's default ties play it: CC1 air added over the keys', CC11 air by hand, CC70 vibrato,
CC21 growl, CC23 flutter, CC4 the plunger, CC65 the glide held.

The lines use the demos' notation (`albums/demos/compose.py`), one note at a time, with brass
modifiers: `_` slurs into the next note (the keys overlap by 40 ms), `'` staccato, `-` tenuto,
`>` accent, `<` softer; any other note is tongued, its key lifted before the next."""
import importlib.util
import os
import re
from fractions import Fraction

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "midi")
_spec = importlib.util.spec_from_file_location("demos_compose", os.path.join(HERE, "..", "demos", "compose.py"))
demos = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(demos)
pitch = demos.pitch

AIR_PLUS, AIR, VIBRATO, GROWL, FLUTTER, PLUNGER, GLIDE = 1, 11, 70, 21, 23, 4, 65
SLUR_S = 0.04
# a stem's General MIDI program in the DAW file
PROGRAM = {"trumpet": 56, "horn": 60, "tenor_trombone": 57, "bass_trombone": 57, "tuba": 58}


class Part(demos.Piece):
    """One stem: one player's line in one mute."""

    def __init__(self, band, name, instrument, seed):
        super().__init__(band.title, band.bpm, seed, swing=band.swing, jitter_s=0.008)
        self.band, self.name, self.instrument = band, name, instrument

    def line(self, t, text, vel, legato=0.85):
        """Plays the line from beat t; vel is a number or a function of the beat. Returns the end."""
        for tok in text.split():
            if tok == "|":
                continue
            spec, _, rest = tok.partition(":")
            m = re.fullmatch(r"([\d./]+)([>'<_-]*)", rest)
            if not m:
                raise ValueError(f"bad token: {tok}")
            dur = float(Fraction(m[1]))
            if spec != "r":
                mods = m[2]
                v = (vel(t) if callable(vel) else vel) + 14 * mods.count(">") - 12 * mods.count("<")
                if "_" in mods:
                    length = dur + self.beats(t, SLUR_S)
                else:
                    length = dur * (0.45 if "'" in mods else 0.95 if "-" in mods else legato)
                self.note(t, length, pitch(spec), v)
            t += dur
        return t

    def shape(self, ctl, points):
        """A controller through (beat, value) points, a ramp between each two."""
        for (t0, v0), (t1, v1) in zip(points, points[1:]):
            self.ramp(t0, ctl, v0, v1, max(0.02, (t1 - t0) * 60 / self.bpm_at(t0)))

    def notes_in(self, t0, t1):
        return sorted(n for n in self.notes if t0 - 0.1 <= n[0] < t1)

    def wahs(self, t0, t1, closed=12, opened=115):
        """The plunger on each note from t0 to t1: shut, opened as it speaks, shut again as a long note
        ends ("ya", "ya-oo")."""
        self.cc(t0 - 0.25, PLUNGER, closed)
        for n0, n1, _, _ in self.notes_in(t0, t1):
            self.ramp(n0, PLUNGER, closed, opened, 0.12)
            if n1 - n0 > 0.7:
                self.ramp(n1 - self.beats(n1, 0.2), PLUNGER, opened, closed, 0.18)
            else:
                self.ramp(n1, PLUNGER, opened, closed, 0.05)

    def glide(self, t, beats=0.6):
        """The glide held over the slur arriving at beat t (a smear)."""
        self.cc(t - 0.3, GLIDE, 127)
        self.cc(t + beats, GLIDE, 0)

    def swell(self, ctl, t, beats, peak, hold=0.0):
        """A controller up to peak over beats from t and back to 0 after hold beats."""
        self.shape(ctl, [(t, 0), (t + beats, peak), (t + beats + hold, peak), (t + beats + hold + 0.5, 0)])


class Band:
    def __init__(self, title, bpm, seed, swing=0.0):
        self.title, self.bpm, self.swing, self.seed = title, bpm, swing, seed
        self.parts = {}
        self.tempo = demos.Piece(title, bpm, seed).tempo

    def part(self, name, instrument):
        if name not in self.parts:
            self.parts[name] = Part(self, name, instrument, self.seed + len(self.parts))
        return self.parts[name]

    def rit(self, t0, t1, bpm1, steps=12):
        bpm0 = self.bpm_at(t0)
        for i in range(1, steps + 1):
            self.tempo.append((t0 + (t1 - t0) * i / steps, bpm0 + (bpm1 - bpm0) * i / steps))

    def bpm_at(self, t):
        bpm = self.tempo[0][1]
        for tt, b in sorted(self.tempo):
            if tt > t:
                break
            bpm = b
        return bpm

    def write(self, name):
        folder = os.path.join(OUT, name)
        os.makedirs(folder, exist_ok=True)
        tracks = []
        for i, (stem, part) in enumerate(self.parts.items()):
            part.tempo = self.tempo
            part.write(os.path.join(folder, f"{stem}.mid"), stem, PROGRAM[part.instrument])
            # channel 10 is a General MIDI drum kit
            tracks.append(part.events(stem, PROGRAM[part.instrument], channel=i if i < 9 else i + 1))
        demos.write_tracks(os.path.join(OUT, f"{name}.mid"), self.title, self.tempo, tracks)
        end = max(n[1] for p in self.parts.values() for n in p.notes)
        print(f"{name}: {len(self.parts)} stems, {end / 4:.0f} bars")


def bars(n):
    return 4 * n


def third_below(line, scale):
    """The line a diatonic third below: the note three or four semitones down that lies in the scale."""
    out = []
    for tok in line.split():
        spec, _, rest = tok.partition(":")
        if spec in ("r", "|") or tok == "|":
            out.append(tok)
            continue
        p = pitch(spec)
        q = next(p - d for d in (3, 4) if (p - d) % 12 in scale)
        out.append(f"{name(q)}:{rest}")
    return " ".join(out)


def shift(line, semitones):
    out = []
    for tok in line.split():
        spec, _, rest = tok.partition(":")
        out.append(tok if spec in ("r", "|") or tok == "|" else f"{name(pitch(spec) + semitones)}:{rest}")
    return " ".join(out)


NAMES = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"]


def name(p):
    return f"{NAMES[p % 12]}{p // 12 - 1}"


def major(tonic):
    return {(tonic + d) % 12 for d in (0, 2, 4, 5, 7, 9, 11)}


def afterbeats(part, t, chords, voice, vel, voicing):
    """Beats 2 and 4 of each bar, staccato: the march's afterbeats."""
    for c in chords:
        n = voicing[c][voice]
        part.line(t, f"r:1 {n}:1' r:1 {n}:1'", vel)
        t += 4
    return t


def two_beat(part, t, chords, vel, voicing, root=3, fifth=4):
    """Beats 1 and 3 of each bar: the root, then the fifth."""
    for c in chords:
        part.line(t, f"{voicing[c][root]}:1 r:1 {voicing[c][fifth]}:1 r:1", vel, legato=0.6)
        t += 4
    return t


# ---------------------------------------------------------------------------------------------
# 1. Harbor Parade: a march in B-flat, its trio in E-flat

# chord: (trumpet 1, trumpet 2, horn, the bass's root, its fifth) for the stabs; afterbeats read 2 and 3
MARCH = {
    "Bb": ("D5", "Bb4", "F4", "Bb1", "F2"), "F7": ("Eb5", "C5", "A4", "F2", "C2"),
    "Bb7": ("D5", "Ab4", "F4", "Bb1", "F2"), "Eb": ("Eb5", "Bb4", "G4", "Eb2", "Bb1"),
    "Gm": ("D5", "Bb4", "G4", "G2", "D2"), "D7": ("C5", "A4", "F#4", "D2", "A2"),
    "Cm": ("Eb5", "C5", "G4", "C2", "G2"), "Eb7": ("Db5", "Bb4", "G4", "Eb2", "Bb1"),
    "Ab": ("C5", "Ab4", "Eb4", "Ab2", "Eb2"), "G7": ("F5", "D5", "B4", "G2", "D2"),
    "Fm": ("Ab5", "F5", "C5", "F2", "C2"),
}
# the afterbeats' notes in the strains: (horn, tenor trombone) per chord
OFF = {"Bb": ("D4", "F3"), "F7": ("Eb4", "A3"), "Bb7": ("D4", "Ab3"), "Eb": ("Eb4", "G3"),
       "Gm": ("D4", "Bb3"), "D7": ("C4", "F#3"), "Cm": ("Eb4", "G3"), "Eb7": ("Db4", "G3"), "Ab": ("C4", "Ab3")}


def march():
    b = Band("Harbor Parade", 116, 201)
    t1, t2, hn = b.part("trumpet1", "trumpet"), b.part("trumpet2", "trumpet"), b.part("horn", "horn")
    tb, bt, tu = b.part("trombone", "tenor_trombone"), b.part("bass_trombone", "bass_trombone"), b.part("tuba", "tuba")
    bb = major(10)

    # intro: a fanfare in octaves, ff
    fan = ("F4:.5' F4:.5' Bb4:1> D5:1> F5:1> | Eb5:.5 D5:.5 C5:.5 Bb4:.5 C5:2> | "
           "F4:.5' F4:.5' Bb4:1> D5:1> F5:1> | G5:1> Eb5:1> F5:1'> r:1")
    t1.line(0, fan, 104)
    t2.line(0, third_below(fan, bb), 98)
    hn.line(0, shift(fan, -12), 100)
    tb.line(0, shift(fan, -12), 100)
    bt.line(0, shift(fan, -24), 100)
    tu.line(0, shift(fan, -24), 100)

    # first strain: the melody in the trumpets, afterbeats in the horn and trombone, two-beat bass
    t = bars(4)
    a_chords = ["Bb", "Bb", "F7", "F7", "F7", "F7", "Bb", "Bb", "Bb", "Bb7", "Eb", "Eb", "Bb", "F7", "Bb", "Bb"]
    a = ("D5:1 F5:.5 D5:.5 Bb4:1 F4:1 | G4:.5 A4:.5 Bb4:.5 C5:.5 D5:2 | C5:1 Eb5:.5 C5:.5 A4:1 F4:1 | "
         "G4:.5 A4:.5 Bb4:.5 A4:.5 C5:2 | Eb5:1 D5:.5 C5:.5 Bb4:1 A4:1 | C5:.5 Bb4:.5 A4:.5 G4:.5 F4:2 | "
         "Bb4:1 D5:.5 F5:.5 Bb5:2> | A5:.5 G5:.5 F5:.5 D5:.5 Bb4:2 | D5:1 F5:.5 D5:.5 Bb4:1 F4:1 | "
         "Ab4:1 Bb4:1 D5:1 F5:1 | G5:1.5 F5:.5 Eb5:1 Bb4:1 | C5:.5 Eb5:.5 G5:.5 Eb5:.5 C5:2 | "
         "F5:1 D5:1 Bb4:1 D5:1 | C5:1 A4:1 F4:.5 G4:.5 A4:1 | Bb4:1 F4:1 D4:1 F4:1 | Bb4:2 r:2")
    t1.line(t, a, 90)
    t2.line(t, third_below(a, bb), 82)
    off = {c: (OFF[c][0], OFF[c][0], OFF[c][1]) for c in OFF}
    afterbeats(hn, t, a_chords, 1, 74, off)
    afterbeats(tb, t, a_chords, 2, 74, off)
    two_beat(tu, t, a_chords, 82, MARCH)
    two_beat(bt, t, a_chords, 72, {c: (0, 0, 0, shift(f"{MARCH[c][3]}:1", 12).split(":")[0],
                                       shift(f"{MARCH[c][4]}:1", 12).split(":")[0]) for c in MARCH})

    # second strain, in G minor: the melody low in the trombones, stabs above, ff
    t = bars(20)
    b_chords = ["Gm", "Gm", "D7", "D7", "Gm", "Gm", "Cm", "D7", "Eb", "Eb", "Bb", "Bb", "Cm", "F7", "Bb", "Bb"]
    bm = ("G3:1.5 A3:.5 Bb3:1 G3:1 | D4:2 C4:1 Bb3:1 | A3:1.5 Bb3:.5 C4:1 A3:1 | F#3:2 D3:2 | "
          "G3:1 Bb3:1 D4:1 G4:1> | F4:.5 Eb4:.5 D4:.5 C4:.5 Bb3:2 | Eb4:1 C4:1 G3:1 Eb3:1 | D3:1 F#3:1 A3:1 C4:1 | "
          "Bb3:1.5 C4:.5 Bb3:1 G3:1 | Eb3:2 G3:1 Bb3:1 | D4:1.5 C4:.5 Bb3:1 F3:1 | D3:2 F3:2 | "
          "Eb3:1 G3:1 C4:1 Eb4:1 | D4:.5 C4:.5 A3:.5 F3:.5 Eb3:2 | D3:1 F3:1 Bb3:1 D4:1> | Bb3:2 r:2")
    tb.line(t, bm, 106, legato=0.92)
    bt.line(t, shift(bm, -12), 104, legato=0.92)
    for part, voice, vel in ((t1, 0, 96), (t2, 1, 90), (hn, 2, 92)):
        afterbeats(part, t, b_chords, voice, vel, MARCH)
    two_beat(tu, t, b_chords, 100, MARCH)

    # trio in E-flat, softer: the horn's melody slurred, the trumpets in straight mutes holding the
    # harmony, afterbeats in the trombone, the tuba two-beat
    t = bars(36)
    trio_chords = ["Eb", "Eb", "Bb7", "Bb7", "Bb7", "Bb7", "Eb", "Eb", "Eb", "Eb7", "Ab", "Ab", "Eb", "Bb7", "Eb", "Eb"]
    trio = ("G4:2_ Bb4:1.5_ G4:.5_ | Eb4:3 G4:1 | F4:2_ Ab4:1.5_ F4:.5_ | D4:3 Eb4:1 | F4:1.5_ G4:.5_ Ab4:1_ C5:1_ | "
            "Bb4:2_ Ab4:1_ F4:1 | G4:1.5_ F4:.5_ Eb4:1_ G4:1_ | Bb4:4 | G4:2_ Bb4:1.5_ G4:.5_ | Db5:2_ Bb4:1_ G4:1 | "
            "C5:2_ Ab4:1.5_ C5:.5_ | Eb5:3 C5:1 | Bb4:1.5_ G4:.5_ Eb4:1_ G4:1_ | F4:1.5_ G4:.5_ Ab4:1_ D4:1_ | Eb4:3 r:1 | r:4")
    hn.line(t, trio, 74)
    s1, s2 = b.part("trumpet1_straight", "trumpet"), b.part("trumpet2_straight", "trumpet")
    pads = {"Eb": ("Bb4", "G4"), "Bb7": ("Ab4", "F4"), "Eb7": ("Db5", "Bb4"), "Ab": ("C5", "Ab4")}
    tt = t
    for c in trio_chords:
        s1.line(tt, f"{pads[c][0]}:4-", 58)
        s2.line(tt, f"{pads[c][1]}:4-", 54)
        tt += 4
    afterbeats(tb, t, trio_chords, 1, 62, {"Eb": (0, "G3"), "Bb7": (0, "Ab3"), "Eb7": (0, "Db4"), "Ab": (0, "C4")})
    two_beat(tu, t, trio_chords, 70, MARCH)

    # breakstrain, in C minor: the trombones' smears on the glide, the band answering, then a Bb7 swelled
    # with the trumpets flutter-tonguing it
    t = bars(52)
    smear = {"Cm": ("C3", "G3"), "G7": ("G2", "D3")}
    answer = {"Cm": ("G5", "Eb5", "C5"), "G7": ("F5", "D5", "B4"), "Fm": ("Ab5", "F5", "C5")}
    runs = {"Cm": "C2:.5 D2:.5 Eb2:.5 F2:.5 G2:.5 F2:.5 Eb2:.5 D2:.5",
            "G7": "G2:.5 A2:.5 B2:.5 C3:.5 D3:.5 C3:.5 B2:.5 A2:.5",
            "Fm": "F2:.5 G2:.5 Ab2:.5 Bb2:.5 C3:.5 Bb2:.5 Ab2:.5 G2:.5"}
    for i, (call, resp) in enumerate((("Cm", "Cm"), ("G7", "G7"), ("Cm", "Fm"))):
        tc = t + bars(2 * i)
        lo, hi = smear[call]
        tb.line(tc, f"{lo}:.5_ {hi}:1.5> r:1 {lo}:1'", 102)
        tb.glide(tc + 0.5)
        bt.line(tc, shift(f"{lo}:.5_ {hi}:1.5> r:1 {lo}:1'", -12), 100)
        bt.glide(tc + 0.5)
        tu.line(tc, f"{shift(f'{lo}:1', -12)} r:3", 96)
        for part, k in ((t1, 0), (t2, 1), (hn, 2)):
            n = answer[resp][k]
            part.line(tc + 4, f"r:1 {n}:.5'> {n}:.5' {n}:1'> r:1", 100)
        tu.line(tc + 4, runs[resp], 92, legato=0.8)
    tc = t + bars(6)
    for part, n, v in ((t1, "D5", 70), (t2, "Ab4", 66), (hn, "F4", 66), (tb, "Bb3", 66), (bt, "D3", 66), (tu, "Bb1", 70)):
        part.line(tc, f"{n}:7- r:1", v)
        part.swell(AIR_PLUS, tc, 6.5, 100)
    for part in (t1, t2):
        part.swell(FLUTTER, tc + 3, 3.5, 90)

    # the trio again, grandioso: the melody in the first trumpet, a third under it in the second, the
    # horn and trombone on a countermelody, the bass ff; a swell on the last chord and a stinger
    t = bars(60)
    grand = trio.replace("Eb4:3 r:1 | r:4", "G4:1 Bb4:1 Eb5:5")
    t1.line(t, grand, 102)
    t2.line(t, third_below(grand, major(3)), 96)
    counter = {"Eb": ("Bb3", "G3"), "Bb7": ("Ab3", "D3"), "Eb7": ("Db4", "G3"), "Ab": ("C4", "Eb3")}
    tt = t
    for c in trio_chords[:14]:
        hn.line(tt, f"{counter[c][0]}:2- {counter[c][0]}:2-", 92)
        tb.line(tt, f"{counter[c][1]}:4-", 92)
        tt += 4
    hn.line(tt, "Bb3:2 G4:5", 96)
    tb.line(tt, "G3:2 Eb4:5", 96)
    two_beat(bt, t, trio_chords[:14], 98, {c: (0, 0, 0, shift(f"{MARCH[c][3]}:1", 12).split(":")[0],
                                                MARCH[c][4]) for c in MARCH})
    two_beat(tu, t, trio_chords[:14], 104, MARCH)
    bt.line(tt, "Bb2:2 Eb2:5", 100)
    tu.line(tt, "Bb1:2 Eb2:5", 104)
    for part in b.parts.values():
        part.swell(AIR_PLUS, tt + 2, 4, 90, hold=0.5)
    t1.swell(VIBRATO, tt + 3, 3, 70)
    ts = t + bars(16)
    for part, n in ((t1, "Eb5"), (t2, "Bb4"), (hn, "G4"), (tb, "Eb4"), (bt, "Eb3"), (tu, "Eb2")):
        part.line(ts, f"r:1 {n}:1'> r:2", 112)
    b.write("01-harbor-parade")


# ---------------------------------------------------------------------------------------------
# 2. Evening Chorale: a hymn in E-flat, three verses and an Amen

# the hymn in four voices, half notes mostly (soprano, alto, tenor, bass)
HYMN = [
    "G4:2_ Ab4:2_ | Bb4:3_ G4:1_ | C5:2_ Bb4:2_ | Ab4:2_ G4:2 | F4:2_ G4:2_ | Ab4:2_ C5:2_ | Bb4:2_ Ab4:1_ F4:1_ | Eb4:4",
    "Eb4:2 Eb4:2 | Eb4:2 Eb4:2 | Eb4:2 Eb4:2 | F4:2_ Eb4:2 | D4:2_ Eb4:2 | Eb4:2 Eb4:2 | G4:2_ D4:2_ | Bb3:4",
    "Bb3:2_ C4:2_ | Bb3:2_ G3:2_ | Ab3:2_ Bb3:2_ | C4:2_ Bb3:2 | Ab3:2_ Bb3:2_ | C4:2_ Ab3:2_ | Eb4:2_ Ab3:2_ | G3:4",
    "Eb3:2_ C3:2_ | G2:2_ C3:2_ | Ab2:2_ G2:2_ | F2:2_ Bb2:2 | Bb2:2_ Eb3:2_ | Ab2:2_ F2:2_ | Bb2:2_ Bb2:2 | Eb3:4",
]
AMEN = ["Ab4:4 | G4:6", "Eb4:4 | Eb4:6", "C4:4 | Bb3:6", "Ab2:4 | Eb3:6"]


def tuba_line(bass):
    """The bass an octave down, kept at the bass where that would reach under A1."""
    out = []
    for tok in bass.split():
        spec, _, rest = tok.partition(":")
        if tok == "|" or spec == "r":
            out.append(tok)
            continue
        p = pitch(spec) - 12
        out.append(f"{name(p if p >= pitch('A1') else p + 12)}:{rest}")
    return " ".join(out)


def chorale():
    b = Band("Evening Chorale", 60, 202)
    s, a, t_, bs = HYMN
    verse = bars(8)
    phrase_swells = [(0, 6, 40), (4 * 4, 6, 50)]   # (beat, length, peak) of CC1 per verse

    # verse 1, open, mf: the tune in the first trumpet, its vibrato coming late on the long notes
    t1, t2, hn = b.part("trumpet1", "trumpet"), b.part("trumpet2", "trumpet"), b.part("horn", "horn")
    tb, bt, tu = b.part("trombone", "tenor_trombone"), b.part("bass_trombone", "bass_trombone"), b.part("tuba", "tuba")
    t1.line(0, s, 80)
    t2.line(0, a, 70)
    hn.line(0, t_, 72)
    tb.line(0, shift(a, -12), 68)
    bt.line(0, bs, 72)
    tu.line(0, tuba_line(bs), 70)
    for n0, n1, _, _ in t1.notes_in(0, verse):
        if n1 - n0 >= 2.5:
            t1.shape(VIBRATO, [(n0, 0), (n0 + 1.0, 0), (n0 + 2.0, 55), (n1, 55), (n1 + 0.2, 0)])
    for part in b.parts.values():
        for t0, length, peak in phrase_swells:
            part.swell(AIR_PLUS, t0 + 1, length / 2, peak, hold=0.5)

    # verse 2, p: the tune in the horn, the trumpets in cup mutes on the alto and tenor, the bass
    # trombone in its bucket, the tuba resting
    v = verse
    hn.line(v, s, 70)
    c1, c2 = b.part("trumpet1_cup", "trumpet"), b.part("trumpet2_cup", "trumpet")
    c1.line(v, a, 62)
    c2.line(v, t_, 60)
    tb.line(v, shift(a, -12), 56)
    bb = b.part("bass_trombone_bucket", "bass_trombone")
    bb.line(v, bs, 62)
    for part in (hn, c1, c2, tb, bb):
        for t0, length, peak in phrase_swells:
            part.swell(AIR_PLUS, v + t0 + 1, length / 2, peak - 15, hold=0.5)

    # verse 3, f, open again: the tune's dynamics on CC11 (air by hand, a breath controller's way), the
    # horn's vibrato with it; a ritardando into the Amen
    v = 2 * verse
    air = b.part("trumpet1_air", "trumpet")
    air.line(v, s, 100)
    air.shape(AIR, [(v - 0.5, 70), (v + 4, 95), (v + 10, 118), (v + 14, 100), (v + 18, 92),
                    (v + 24, 120), (v + 28, 105), (v + 32, 88)])
    t2.line(v, a, 96)
    hn.line(v, t_, 96)
    tb.line(v, shift(a, -12), 94)
    bt.line(v, bs, 100)
    tu.line(v, tuba_line(bs), 100)
    for n0, n1, _, _ in hn.notes_in(v, v + verse):
        if n1 - n0 >= 2.5:
            hn.shape(VIBRATO, [(n0, 0), (n0 + 0.5, 40), (n1, 40), (n1 + 0.2, 0)])
    for part in (t2, hn, tb, bt, tu):
        part.swell(AIR_PLUS, v + 22, 3, 70, hold=1)
    b.rit(v + 24, v + 32, 48)

    # Amen, pp, the first trumpet open again: Ab, then Eb held and faded
    v = 3 * verse
    for part, line, vel in ((t1, AMEN[0], 58), (t2, AMEN[1], 52), (hn, AMEN[2], 54), (tb, shift(AMEN[1], -12), 50),
                            (bt, AMEN[3], 54), (tu, tuba_line(AMEN[3]), 52)):
        part.line(v, line, vel, legato=0.97)
        part.shape(AIR_PLUS, [(v, 0), (v + 4, 30), (v + 6, 30), (v + 9.5, 0)])
    b.write("02-evening-chorale")


# ---------------------------------------------------------------------------------------------
# 3. Back Alley Strut: a 12-bar blues in F, swung

BLUES = ["F7", "Bb7", "F7", "F7", "Bb7", "Bb7", "F7", "D7", "Gm7", "C7", "F7/D7", "Gm7/C7"]
# per chord: the bass's root and fifth, the riffs' notes (lead, second, horn), the trombone's smear
BLUE = {
    "F7": ("F2", "C2", "F5", "C5", "A4", ("C3", "A3")), "Bb7": ("Bb1", "F2", "F5", "D5", "Ab4", ("D3", "Ab3")),
    "D7": ("D2", "A2", "F#5", "D5", "C5", ("D3", "F#3")), "Gm7": ("G2", "D2", "F5", "D5", "Bb4", ("D3", "Bb3")),
    "C7": ("C2", "G2", "E5", "C5", "Bb4", ("C3", "Bb3")),
}


def halves(c):
    return c.split("/") if "/" in c else [c, c]


def blues():
    b = Band("Back Alley Strut", 126, 203, swing=0.75)
    chorus = bars(12)
    tu, hn = b.part("tuba", "tuba"), b.part("horn", "horn")

    def bass(t, walking=False, vel=86):
        for c in BLUES:
            h = halves(c)
            if walking:
                # root, third, fifth, sixth; a bar of two chords their roots and thirds
                r0, r1 = pitch(BLUE[h[0]][0]), pitch(BLUE[h[1]][0])
                third = lambda c: 3 if c.startswith("Gm") else 4
                steps = ([r0, r0 + third(h[0]), r0 + 7, r0 + 9] if h[0] == h[1]
                         else [r0, r0 + third(h[0]), r1, r1 + third(h[1])])
                tu.line(t, " ".join(f"{name(p)}:1" for p in steps), vel, legato=0.75)
            else:
                tu.line(t, f"{BLUE[h[0]][0]}:2- {BLUE[h[1]][1] if h[0] == h[1] else BLUE[h[1]][0]}:2-", vel)
            t += 4

    # intro: two bars of the tuba alone on C7, a trombone pickup
    tb = b.part("trombone", "tenor_trombone")
    tu.line(0, "C2:1 E2:1 G2:1 Bb2:1 | C3:1 Bb2:1 G2:1 E2:1", 84, legato=0.75)
    tb.line(bars(1) + 2, "C3:.5_ E3:.5_ G3:1>", 90)
    tb.glide(bars(1) + 2.5)

    # choruses 1 and 2: the head in the first trumpet, plunger; the second in a wah mute with its stem
    # (and its stem in), riffs; the trombone's tailgate smears on the glide; the tuba two-beat
    t0 = bars(2)
    head = ("A4:.5 C5:.5 D5:.5 Eb5:1 D5:.5 C5:1 | D5:1.5 C5:.5 Ab4:1 F4:1 | A4:.5 C5:.5 D5:.5 Eb5:1 D5:.5 C5:1 | F5:2 r:2 | "
            "D5:.5 F5:.5 Ab5:.5 F5:1 D5:.5 F5:1 | Bb4:2 r:2 | A4:.5 C5:.5 D5:.5 Eb5:1 D5:.5 C5:1 | F#5:2 D5:1 A4:1 | "
            "G5:1.5 F5:.5 D5:1 Bb4:1 | E5:1 G5:1 Bb5:1 G5:1 | A5:1 F5:1 F#5:1 D5:1 | G5:1 Bb4:1 E5:1 C5:1")
    lead = b.part("trumpet1_plunger", "trumpet")
    harmon = b.part("trumpet2_wah_stem", "trumpet")
    for k in range(2):
        t = t0 + k * chorus
        lead.line(t, head, 92 + 6 * k)
        lead.wahs(t, t + chorus)
        tt = t
        for c in BLUES:
            h = halves(c)
            harmon.line(tt, f"r:2 {BLUE[h[1]][3]}:.5' {BLUE[h[1]][3]}:.5'> r:1", 76 + 6 * k)
            lo, hi = BLUE[h[0]][5]
            tb.line(tt, f"r:1 {lo}:.5_ {hi}:2.5", 80 + 6 * k)
            tb.glide(tt + 1.5)
            tt += 4
        bass(t)
    hn.line(t0 + chorus, " ".join(f"{BLUE[halves(c)[0]][4]}:4-" for c in BLUES), 62)

    # chorus 3: the trombone's solo in the bucket mute, slurred, smeared into its long notes; the
    # trumpets in straight mutes and the horn behind it, soft
    t = t0 + 2 * chorus
    solo = ("C4:1.5_ A3:.5_ F3:1_ Eb3:1 | D3:1_ F3:1_ Ab3:1_ Bb3:1 | C4:.5_ D4:.5_ C4:.5_ A3:.5_ F3:2 | r:1 F3:.5_ G3:.5_ A3:1_ C4:1 | "
            "D4:2_ Ab3:1_ F3:1 | Bb3:1.5_ Ab3:.5_ F3:1_ D3:1 | C3:1_ F3:1_ A3:1_ C4:1 | F#3:2_ A3:1_ C4:1 | "
            "Bb3:1.5_ A3:.5_ G3:1_ D3:1 | E3:1_ G3:1_ Bb3:1_ C4:1 | A3:2_ F#3:2 | G3:2_ E3:2")
    bucket = b.part("trombone_bucket", "tenor_trombone")
    bucket.line(t, solo, 84)
    for bar in (0, 4, 7, 10):
        bucket.glide(t + bars(bar) + (1.5 if bar == 0 else 0), 0.4)
    st1, st2 = b.part("trumpet1_straight", "trumpet"), b.part("trumpet2_straight", "trumpet")
    pads = {"F7": ("A4", "Eb4"), "Bb7": ("Ab4", "D4"), "D7": ("F#4", "C4"), "Gm7": ("Bb4", "F4"), "C7": ("Bb4", "E4")}
    tt = t
    for c in BLUES:
        for i, h in enumerate(halves(c)):
            st1.line(tt + 2 * i, f"{pads[h][0]}:2-", 56)
            st2.line(tt + 2 * i, f"{pads[h][1]}:2-", 52)
        tt += 4
    bass(t, vel=78)

    # chorus 4, the shout: all open, a riff on one note per chord in every part, the trombones
    # growling their smears, a plain wah mute in the second trumpet, the tuba walking
    t = t0 + 3 * chorus
    shout = b.part("trumpet1", "trumpet")
    wah = b.part("trumpet2_wah", "trumpet")
    bt = b.part("bass_trombone", "bass_trombone")
    riff = "{0}:.5> {0}:.5 r:.5 {0}:.5'> r:1 {0}:1>"
    tt = t
    for c in BLUES:
        h = halves(c)
        for part, k, vel in ((shout, 2, 108), (wah, 3, 100), (hn, 4, 98)):
            part.line(tt, riff.format(BLUE[h[0]][k]), vel)
        lo, hi = BLUE[h[0]][5]
        for part, d in ((tb, 0), (bt, -12)):
            part.line(tt, f"r:2 {shift(lo + ':.5', d).split(':')[0]}:.5_ {shift(hi + ':1', d).split(':')[0]}:1.5>", 108)
            part.glide(tt + 2.5)
            part.shape(GROWL, [(tt + 2.5, 0), (tt + 3, 90), (tt + 3.8, 90), (tt + 4, 0)])
        tt += 4
    bass(t, walking=True, vel=96)

    # ending: two hits, then the last chord held, the first trumpet's flutter growing on it, the
    # trombones falling off it on the glide
    t = t0 + 4 * chorus
    end = (("trumpet1", "A5"), ("trumpet2_wah", "F5"), ("horn", "D5"), ("trombone", "C4"), ("bass_trombone", "F2"), ("tuba", "F2"))
    for stem, n in end:
        part = b.parts[stem]
        part.line(t, f"{n}:1'> r:1 {n}:1'> r:1 | {n}:6", 110)
        part.swell(AIR_PLUS, t + 4, 4, 80, hold=1.5)
    shout.swell(FLUTTER, t + 5, 3, 100, hold=1)
    for stem, n in (("trombone", "C4"), ("bass_trombone", "F2")):
        part = b.parts[stem]
        part.notes[-1] = part.notes[-1][:1] + (t + 9.5,) + part.notes[-1][2:]
        drop = name(pitch(n) - 7)
        part.line(t + 9.4, f"{drop}:.8", 70)
        part.glide(t + 9.4, 0.8)
    b.write("03-back-alley-strut")


def main():
    march()
    chorale()
    blues()


if __name__ == "__main__":
    main()
