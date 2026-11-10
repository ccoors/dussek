#!/usr/bin/env python3
"""Composes "The Crossing": Dussek's and Weidinger's instruments in one piece, in E-flat, 88 bpm, about
three and a half minutes. Six of Dussek's (piano, harp, vibraphone, marimba, bells, tubular bells) and a
brass sextet of Weidinger's (two trumpets, horn, tenor and bass trombone, tuba); one Standard MIDI file per
stem, for midi2wav one at a time (`mix.py`), and one with a track each, for a DAW. The same files each run
(seeded).

    python3 albums/crossover/compose.py      # writes albums/crossover/midi/01-the-crossing/<stem>.mid and .mid

The keyboard parts use the demos' notation (`albums/demos/compose.py`), the brass the brass album's
(`albums/brass/compose.py`: `_` slurs into the next note, `'` staccato, `-` tenuto, `>` accent); a brass
stem is one part in one mute (`trumpet2_cup`), its other controls in the MIDI as the plugin's default ties
play them (CC70 vibrato, CC21 growl, CC23 flutter, CC65 the glide held).

The sections: an intro (the piano alone, the harp and vibraphone joining, a horn call far off); the theme
twice (the piano, then the trumpet with the horn a third below); a groove in C minor (marimba, tuba,
cup-muted riffs, then the piano's solo over it); a brass chorale with bells; a build; the theme in full;
a quiet coda, the horn's call again."""
import importlib.util
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "midi")


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


demos = load("demos_compose", os.path.join(HERE, "..", "demos", "compose.py"))
brass = load("brass_compose", os.path.join(HERE, "..", "brass", "compose.py"))
pitch, name = demos.pitch, brass.name

TITLE, BPM, SEED = "The Crossing", 88, 301
# Dussek's stems: (instrument, General MIDI program in the DAW file)
KEYS = {"piano": ("grand_c", 0), "harp": ("harp", 46), "vibraphone": ("vibraphone", 11), "marimba": ("marimba", 12),
        "bells": ("bells", 9), "chimes": ("chimes", 14)}
# Weidinger's: (part, instrument); a stem `<part>_<mute>` plays the part in that mute
BRASS = {"trumpet1": "trumpet", "trumpet2": "trumpet", "trumpet2_cup": "trumpet", "horn": "horn",
         "trombone": "tenor_trombone", "bass_trombone": "bass_trombone", "tuba": "tuba"}

NOTE = {"C": 0, "Db": 1, "D": 2, "Eb": 3, "E": 4, "F": 5, "F#": 6, "Gb": 6, "G": 7, "Ab": 8, "A": 9, "Bb": 10, "B": 11}
# a chord: its tones (pitch classes, root first) and its bass
CHORDS = {"Eb": "Eb G Bb", "Cm": "C Eb G", "Ab": "Ab C Eb", "Bb": "Bb D F", "Bb7": "Bb D F Ab", "Gm": "G Bb D",
          "Fm": "F Ab C", "Fm7": "F Ab C Eb", "G7": "G B D F", "Bbsus": "Bb Eb F", "Ab/Eb": "Ab C Eb/Eb",
          "Bb/D": "Bb D F/D", "Eb/G": "Eb G Bb/G", "Ab/C": "Ab C Eb/C", "Cm/G": "C Eb G/G", "Dm7b5": "D F Ab C"}


def tones(ch):
    return [NOTE[n] for n in CHORDS[ch].split("/")[0].split()]


def bass_pc(ch):
    parts = CHORDS[ch].split("/")
    return NOTE[parts[1]] if len(parts) > 1 else tones(ch)[0]


def low(pc, lo):
    """The lowest key at or above lo of pitch class pc."""
    return lo + (pc - lo) % 12


def stack(ch, lo, n):
    """n chord tones upward from lo."""
    return [p for p in range(lo, lo + 48) if p % 12 in tones(ch)][:n]


def names(ps):
    return "+".join(name(p) for p in ps)


# the theme, eight bars in E-flat, and its chords
THEME = ("G4:1 Bb4:1 Eb5:1.5 D5:.5 | C5:1 Eb5:1 G5:2 | F5:1 Eb5:.5 D5:.5 C5:1 Ab4:1 | Bb4:3 r:1 | "
         "G4:1 Bb4:1 Eb5:1 G5:1 | Ab5:1.5 G5:.5 F5:1 Eb5:1 | D5:1 F5:1 Bb4:1 D5:1 | Eb5:4")
THEME_CHORDS = ["Eb", "Cm", "Ab", "Bb", "Eb/G", "Ab", ("Fm7", "Bb7"), "Eb"]
INTRO = ["Eb", "Ab/Eb", "Eb", "Bb/D", "Cm", "Ab", "Bb", "Bb7"]
GROOVE = ["Cm", "Ab", "Fm", "G7"] * 4
CHORALE = ["Ab", "Eb/G", "Fm", "Bb", "Cm", "Ab", "Bbsus", "Eb"]
BUILD = ["Ab", "Bb", "Cm", "Bb/D", "Eb/G", "Ab", "Bb", "Bb7"]
CODA = ["Ab", "Eb", "Ab", "Eb", "Cm", "Ab", "Bb7", "Eb"]
# the horn's call, far off in the intro and again in the coda
CALL = "Bb3:1_ Eb4:1_ G4:2 | F4:1_ Eb4:1_ Bb3:2 | C4:1_ D4:1_ Eb4:1_ G4:1_ | F4:4"


def halves(bar):
    """A bar's chords on its two halves (one chord: both)."""
    return bar if isinstance(bar, tuple) else (bar, bar)


def compose():
    band = brass.Band(TITLE, BPM, SEED)
    keys = {stem: demos.Piece(TITLE, BPM, seed=SEED + 20 + i) for i, stem in enumerate(KEYS)}
    for p in keys.values():
        p.tempo = band.tempo
    pn, hp, vb, mb, bl, ch = (keys[s] for s in KEYS)
    bp = {stem: band.part(stem, inst) for stem, inst in BRASS.items()}
    t1, t2, t2c, hn, tb, bt, tu = (bp[s] for s in BRASS)

    # ---- the keyboard's figures
    def arpeggio_bar(t, chord, vel, lo=39):
        """The piano's left hand: the bass, then the chord's tones broken upward in eighths; a legato change."""
        b = low(bass_pc(chord), lo)
        ns = [b] + stack(chord, b + 7, 7)
        for i, p in enumerate(ns[:8]):
            pn.note(t + i / 2, 4 - i / 2, p, vel - (0 if i == 0 else 8) + (4 if i == 4 else 0))
        pn.change(t)

    def harp_roll(t, chord, vel, lo=40, n=6):
        for i, p in enumerate([low(bass_pc(chord), lo)] + stack(chord, lo + 12, n - 1)):
            hp.note(t + 0.07 * i, 4, p, vel, swing=False)
        hp.change(t)

    def harp_arp(t, chord, vel, lo=43):
        ns = stack(chord, low(bass_pc(chord), lo), 8)
        for i, p in enumerate(ns):
            hp.note(t + i / 2, 4 - i / 2, p, vel + (6 if i == 0 else 0))
        hp.change(t)

    def pad(t, chord, vel, beats=4):
        vb.chord(t, beats - 0.1, names(stack(chord, 55, 4)), vel, top=2)
        vb.change(t)

    def comp(t, chord, vel):
        """The piano's syncopated chords in the groove: the bass on 1, the chord on the and of 2 and on 4."""
        pn.note(t, 0.9, low(bass_pc(chord), 36), vel + 4)
        pn.note(t, 0.9, low(bass_pc(chord), 36) + 12, vel - 4)
        for x in (1.5, 3):
            pn.chord(t + x, 0.45, names(stack(chord, 55, 4)), vel - 6)

    def marimba_groove(t, chord, vel, grow=0):
        ns = stack(chord, low(bass_pc(chord), 48), 6)
        order = ns + ns[-2:0:-1]
        for k in range(16):
            mb.note(t + k / 4, 0.24, order[k % len(order)], vel + grow * k / 16 + (6 if k % 4 == 0 else 0))

    # ---- A. intro (bars 0-7): the piano alone, then the harp, the vibraphone; the horn's call far off
    T = 0
    for i, c in enumerate(INTRO):
        arpeggio_bar(T + 4 * i, c, 52 + 2 * i)
        if i >= 2:
            pad(T + 4 * i, c, 40 + 2 * i)
        if i >= 4:
            harp_roll(T + 4 * i, c, 48)
    hn.line(T + 16, CALL, 60)
    hn.shape(brass.VIBRATO, [(T + 16, 0), (T + 28, 40), (T + 32, 0)])

    # ---- B. the theme twice (bars 8-23)
    T = 32
    pn.seq(T, demos_theme(12), lambda b: 70 + 10 * (1 - abs((b - T) / 4 - 4) / 4), legato=0.96)
    for i, bar in enumerate(THEME_CHORDS):
        a, b = halves(bar)
        t = T + 4 * i
        pn.chord(t, 1.9, names([low(bass_pc(a), 36), low(bass_pc(a), 36) + 7]), 56)
        pn.chord(t + 2, 1.9, names(stack(b, 55, 3)), 50)
        pn.change(t)
        harp_arp(t, a, 52)
        pad(t, a, 44)
        tb.note(t, 3.8, stack(a, 53, 1)[0], 40)  # held under the piano's melody, softer than it
        bt.note(t, 3.8, stack(a, 46, 1)[0], 38)
        tu.line(t, f"{name(low(bass_pc(a), 34))}:2 {name(low(bass_pc(b), 34))}:2", 58)
    T = 64
    t1.line(T, THEME, 82, legato=0.9)
    t1.shape(brass.VIBRATO, [(T + 12, 0), (T + 15, 50), (T + 16, 0), (T + 28, 0), (T + 31.5, 55), (T + 32, 0)])
    hn.line(T, brass.third_below(THEME, brass.major(NOTE["Eb"])), 44)  # under the trumpet
    for i, bar in enumerate(THEME_CHORDS):
        a, b = halves(bar)
        t = T + 4 * i
        for k in range(4):
            arp = stack(a if k < 2 else b, low(bass_pc(a), 51), 4)
            for j, p in enumerate(arp):
                pn.note(t + k + j / 4, 1 - j / 4, p, 48 + (6 if j == 0 else 0))
        pn.note(t, 3.9, low(bass_pc(a), 36), 58)
        pn.change(t)
        harp_roll(t, a, 46)
        tb.note(t, 3.8, stack(a, 53, 1)[0], 44)
        bt.note(t, 3.8, stack(a, 46, 1)[0], 42)
        tu.line(t, f"{name(low(bass_pc(a), 34))}:2 {name(low(bass_pc(b), 34))}:2", 62)
    # the bells doubling the theme's long notes, two octaves up, from its second half
    for bar, n in ((4, "G6"), (5, "Ab6"), (7, "Eb6")):
        bl.note(T + 4 * bar, 2, pitch(n), 58)

    # ---- C. the groove in C minor (bars 24-39): marimba, tuba, cup-muted riffs; the piano's solo
    T = 96
    RIFF = {"Cm": "C5:.5 Eb5:.5 G5:1' F5:.5 Eb5:.5 C5:1'", "Ab": "Ab4:.5 C5:.5 Eb5:1' D5:.5 C5:.5 Ab4:1'",
            "Fm": "F4:.5 Ab4:.5 C5:1' Bb4:.5 Ab4:.5 F4:1'", "G7": "G4:.5 B4:.5 D5:1' F5:.5 D5:.5 B4:1'"}
    ANSWER = {"Cm": "r:2 G4:.5' G4:.5' Bb4:1-", "Ab": "r:2 Ab4:.5' Ab4:.5' C5:1-",
              "Fm": "r:2 F4:.5' F4:.5' Ab4:1-", "G7": "r:2 G4:.5' B4:.5' D5:1>"}
    TUBA = {"Cm": "C2:1 r:.5 C2:.5 G1:1 Bb1:1", "Ab": "Ab1:1 r:.5 Ab1:.5 Eb2:1 C2:1",
            "Fm": "F1:1 r:.5 F1:.5 C2:1 Ab1:1", "G7": "G1:1 r:.5 G1:.5 D2:1 B1:1"}
    for i, c in enumerate(GROOVE):
        t = T + 4 * i
        marimba_groove(t, c, 58)
        tu.line(t, TUBA[c], 80, legato=0.7)
        if i < 8:
            t1.line(t, RIFF[c], 74 + 2 * (i % 4), legato=0.8)
            t2c.line(t, ANSWER[c], 72, legato=0.85)
            pad(t, c, 46, beats=4)
        else:
            comp(t, c, 64)
            tb.line(t, f"r:1.5 {name(stack(c, 53, 1)[0])}:.5' r:1.5 {name(stack(c, 53, 1)[0])}:.5'", 72)
            bt.line(t, f"r:1.5 {name(stack(c, 46, 1)[0])}:.5' r:1.5 {name(stack(c, 46, 1)[0])}:.5'", 70)
    # the piano's solo over the second half: a line in C minor, its right hand alone
    SOLO = ("G5:.5 Ab5:.5 G5:.5 Eb5:.5 C5:1 Eb5:.5 G5:.5 | Ab5:1 G5:.5 F5:.5 Eb5:1 C5:1 | "
            "F5:.5 G5:.5 Ab5:.5 C6:.5 Bb5:1 Ab5:.5 F5:.5 | G5:.25 Ab5:.25 G5:.25 F5:.25 D5:1 B4:1 G4:1 | "
            "C5:.5 Eb5:.5 G5:.5 C6:.5 Eb6:1 D6:.5 C6:.5 | Ab5:1 C6:1 Eb6:1.5 C6:.5 | "
            "Ab5:.5 F5:.5 C5:.5 F5:.5 Ab5:.5 C6:.5 F6:1 | D6:.5 B5:.5 G5:.5 F5:.5 D5:.5 B4:.5 G4:1")
    pn.seq(T + 32, SOLO, lambda b: 74 + 8 * ((b - T - 32) % 4 < 1), legato=0.9)

    # ---- D. the brass chorale (bars 40-47), the bells answering at each bar's end
    T = 160
    players = [t1, t2, hn, tb, bt, tu]
    for i, c in enumerate(CHORALE):
        t = T + 4 * i
        vel = 62 + 12 * (1 - abs(i - 4.5) / 4.5)
        tops = stack(c, 53, 8)
        for part, p in zip(players[:5], [tops[4], tops[3], tops[2], tops[1], low(tones(c)[0], 46)]):
            part.note(t, 3.92, p, vel)
        tu.note(t, 3.92, low(bass_pc(c), 31), vel)
        if i % 2 == 1:
            bl.note(t + 3, 1.5, stack(c, 84, 1)[0], 54)
    for part in (t1, t2, hn, tb):
        part.shape(brass.VIBRATO, [(T, 0), (T + 12, 30), (T + 20, 30), (T + 32, 0)])

    # ---- E. the build (bars 48-55): everyone joins, louder each bar; a harp glissando into the top
    T = 192
    for i, c in enumerate(BUILD):
        t = T + 4 * i
        vel = 58 + 4 * i
        marimba_groove(t, c, vel - 6, grow=8)
        pad(t, c, vel - 10)
        pn.chord(t, 1.9, names([low(bass_pc(c), 31), low(bass_pc(c), 31) + 12]), vel + 4)
        pn.chord(t + 2, 1.9, names(stack(c, 60, 4)), vel)
        pn.change(t)
        harp_arp(t, c, vel - 8)
        hn.note(t, 3.9, stack(c, 58, 1)[0], vel)
        tb.note(t, 1.9, stack(c, 50, 1)[0], vel)
        # into bar 55 the trombone slides down (a slur, the glide held: a smear)
        tb.note(t + 2, 2.04 if i == 5 else 1.9, stack(c, 50, 2)[1], vel + 2)
        if i == 5:
            tb.glide(t + 4, 0.6)
        bt.note(t, 3.9, low(tones(c)[0], 40), vel)
        tu.line(t, f"{name(low(bass_pc(c), 31))}:1' r:1 {name(low(bass_pc(c), 31))}:1 r:1", vel + 6)
        if i >= 4:
            t1.note(t, 3.9, stack(c, 67, 1)[0], vel + 4)
            t2.note(t, 3.9, stack(c, 63, 1)[0], vel)
    t1.swell(brass.FLUTTER, T + 28, 3, 90)
    ch.note(T + 16, 6, pitch("Eb4"), 70)
    scale = [p for p in range(pitch("Bb3"), pitch("Bb6") + 1) if p % 12 in brass.major(NOTE["Eb"])]
    for i, p in enumerate(scale):
        hp.note(T + 30 + 2 * i / len(scale), 2, p, 50 + 30 * i / len(scale), swing=False, jitter=False)

    # ---- F. the theme in full (bars 56-63): trumpets in thirds, the horn and trombones in chords
    T = 224
    t1.line(T, THEME, 104, legato=0.9)
    t2.line(T, brass.third_below(THEME, brass.major(NOTE["Eb"])), 96, legato=0.9)
    t1.shape(brass.VIBRATO, [(T + 26, 0), (T + 29, 60), (T + 32, 0)])
    t1.swell(brass.GROWL, T + 20, 1.5, 70, hold=1)
    pn.seq(T, " ".join(f"{n.split(':')[0]}+{name(pitch(n.split(':')[0]) + 12)}:{n.split(':')[1]}" if n not in ("|", "r:1") else n
                       for n in shift_tokens(THEME, 12).split()), 96, legato=0.95)
    for i, bar in enumerate(THEME_CHORDS):
        a, b = halves(bar)
        t = T + 4 * i
        pn.chord(t, 1.9, names([low(bass_pc(a), 28), low(bass_pc(a), 28) + 12]), 92)
        pn.chord(t + 2, 1.9, names([low(bass_pc(b), 28), low(bass_pc(b), 28) + 12]), 86)
        pn.change(t)
        harp_arp(t, a, 66)
        pad(t, a, 58)
        hn.note(t, 1.9, stack(a, 58, 1)[0], 82)  # the chords under the trumpets' thirds
        hn.note(t + 2, 1.9, stack(b, 58, 1)[0], 80)
        tb.note(t, 1.9, stack(a, 52, 2)[1], 82)
        tb.note(t + 2, 1.9, stack(b, 52, 2)[1], 80)
        bt.note(t, 1.9, stack(a, 43, 1)[0], 90)
        bt.note(t + 2, 1.9, stack(b, 43, 1)[0], 88)
        tu.line(t, f"{name(low(bass_pc(a), 31))}:1 r:1 {name(low(bass_pc(b), 31))}:1 r:1", 96, legato=0.8)
        ch.note(t, 3, stack(a, 63, 1)[0] if stack(a, 63, 1)[0] <= 77 else stack(a, 60, 1)[0], 76)
        for k, n in enumerate(stack(a, 84, 3)):
            bl.note(t + 2 + k / 3, 1, n, 60)

    # ---- G. the coda (bars 64-71): quiet, the horn's call again, the last chord with everyone
    T = 256
    for i, c in enumerate(CODA):
        t = T + 4 * i
        arpeggio_bar(t, c, 52 - 2 * i)
        pad(t, c, 44 - i)
        if i % 2 == 0:
            harp_roll(t, c, 44)
    hn.line(T + 4, CALL, 58)
    t2.line(T + 20, "Eb5:2_ D5:2 | Eb5:4", 54)
    end = T + 32
    last = [("piano", "Eb2+Bb2+Eb3+G3+Bb3+Eb4+G4"), ("vibraphone", "G4+Bb4+Eb5"), ("harp", "Eb2+Bb2+G3+Eb4")]
    for stem, notes in last:
        keys[stem].chord(end, 8, notes, 56)
    pn.change(end)
    ch.note(end, 8, pitch("Eb4"), 60)
    bl.note(end + 0.5, 4, pitch("Bb6"), 44)
    for part, n in ((t1, "G4"), (t2, "Eb4"), (hn, "Bb3"), (tb, "G3"), (bt, "Bb2"), (tu, "Eb2")):
        part.line(end, f"{n}:8", 50)
    for part in (pn, vb, hp):
        part.pedal(end + 8, 127, 0, 0.6)
    return keys, band


def shift_tokens(line, semitones):
    return brass.shift(line, semitones)


def demos_theme(semitones):
    """The theme an octave up (the piano's)."""
    return brass.shift(THEME, semitones)


def write(keys, band):
    folder = os.path.join(OUT, "01-the-crossing")
    os.makedirs(folder, exist_ok=True)
    tracks = []
    for i, (stem, piece) in enumerate(keys.items()):
        _, program = KEYS[stem]
        piece.write(os.path.join(folder, f"{stem}.mid"), stem, program)
        tracks.append(piece.events(stem, program, channel=i))
    for j, (stem, part) in enumerate(band.parts.items()):
        part.tempo = band.tempo
        program = brass.PROGRAM[part.instrument]
        part.write(os.path.join(folder, f"{stem}.mid"), stem, program)
        ch = len(keys) + j
        tracks.append(part.events(stem, program, channel=ch if ch < 9 else ch + 1))
    demos.write_tracks(os.path.join(OUT, "01-the-crossing.mid"), TITLE, band.tempo, tracks)
    end = max(n[1] for p in [*keys.values(), *band.parts.values()] for n in p.notes)
    print(f"01-the-crossing: {len(keys)} keyboard and {len(band.parts)} brass stems, {end / 4:.0f} bars")


if __name__ == "__main__":
    write(*compose())
