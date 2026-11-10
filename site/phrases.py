#!/usr/bin/env python3
"""The website's phrases as Standard MIDI files (CC0, as the demos): one per kind of instrument for
the menu groups' demos (20–30 s), one per brass instrument for Weidinger's page, and the short ones
the features play plain, then with the feature. Written with the demos' notation (albums/demos/compose.py: `Piece.seq`); seeded, so the
same files each run.

    python3 site/phrases.py      # writes site/midi/<name>.mid

Each phrase stays inside its instrument's recorded keys (site/render.py checks it)."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "albums", "demos"))
from compose import Piece  # noqa: E402

OUT = os.path.join(HERE, "midi")
MOD = 1  # the mod wheel: the clavichord's Bebung


def piano():
    """A lyrical phrase in D: a broken-chord left hand, a singing line, pedal changes, a swell."""
    p = Piece("Piano phrase", 72, seed=1)
    bars = ["D2 A2 D3 F#3 A3 D3 F#3 A2", "B1 F#2 B2 D3 F#3 B2 D3 F#2", "G1 D2 G2 B2 D3 G2 B2 D2",
            "A1 E2 A2 C#3 E3 A2 C#3 E2", "F#1 C#2 F#2 A2 C#3 F#2 A2 C#2", "G1 D2 G2 B2 E3 G2 B2 D2",
            "A1 E2 A2 C#3 G3 A2 C#3 E2", "D2 A2 D3 F#3 A3 D4"]
    for i, b in enumerate(bars):
        t = 4 * i
        swell = 52 + 26 * (1 - abs(i - 4.5) / 4.5)
        p.seq(t, " ".join(f"{n}:.5" for n in b.split()) if i < 7 else " ".join(f"{n}:.5" for n in b.split()[:5]) + " D4:1.5_", swell - 8, legato=1.0)
        p.change(t) if i else p.pedal(t, 0, 127)
    melody = ("F#5:1.5 E5:.5 D5:1 A4:1 | B4:1.5 C#5:.5 D5:1 F#5:1 | G5:1.5 F#5:.5 E5:1 D5:1 | "
              "C#5:1 E5:1 A5:2 | A5:1.5 G5:.5 F#5:1 C#5:1 | D5:1 E5:.5 F#5:.5 G5:2 | "
              "F#5:1 E5:1 D5:.5 C#5:.5 B4:.5 C#5:.5 | D5:4_")
    p.seq(0, melody, lambda t: 62 + 30 * (1 - abs(t / 4 - 4.5) / 4.5), legato=0.96)
    p.pedal(31.5, 127, 0, 0.6)
    return p


def harpsichord():
    """Two voices in A minor, sixteenths over eighths, a cadence."""
    p = Piece("Harpsichord phrase", 100, seed=2, jitter_s=0.003)
    rh = ("A4:.25 C5:.25 E5:.25 A5:.25 G#5:.25 E5:.25 B4:.25 E5:.25 C5:.25 E5:.25 A5:.25 C6:.25 B5:.25 A5:.25 G#5:.25 B5:.25 | "
          "A5:.25 E5:.25 C5:.25 E5:.25 D5:.25 F5:.25 A5:.25 F5:.25 E5:.25 G5:.25 C6:.25 G5:.25 F5:.25 A5:.25 D6:.25 A5:.25 | "
          "G5:.25 B5:.25 D6:.25 B5:.25 A5:.25 C6:.25 E6:.25 C6:.25 B5:.25 G5:.25 E5:.25 G5:.25 F5:.25 D5:.25 B4:.25 D5:.25 | "
          "C5:.25 E5:.25 A5:.25 E5:.25 D5:.25 F5:.25 B5:.25 F5:.25 E5:.25 C5:.25 B4:.25 C5:.25 B4:.25 A4:.25 G#4:.25 B4:.25 | "
          "A4:.25 C5:.25 E5:.25 A5:.25 F5:.25 D5:.25 B4:.25 D5:.25 E5:.5 B4:.5 G#4:.5 B4:.5 | A4+C5+E5+A5:4_")
    lh = ("A2:.5 A3:.5 E3:.5 A3:.5 E2:.5 E3:.5 G#3:.5 E3:.5 | F2:.5 F3:.5 D3:.5 F3:.5 C3:.5 C4:.5 A3:.5 C4:.5 | "
          "G2:.5 G3:.5 B3:.5 G3:.5 C3:.5 C4:.5 E3:.5 C3:.5 | A2:.5 A3:.5 D3:.5 D4:.5 E3:.5 E2:.5 E3:.5 G#2:.5 | "
          "A2:.5 C3:.5 D3:.5 F3:.5 E3:1 E2:1 | A1+A2:4_")
    p.seq(0, rh, 78, legato=0.85)
    p.seq(0, lh, 70, legato=0.8)
    return p


def clavichord():
    """A sighing line in C minor; the long notes trembled with the Bebung (the mod wheel)."""
    p = Piece("Clavichord phrase", 58, seed=3)
    line = "G4:1 Eb5:1.5 D5:.5 | C5:1 B4:1 C5:2 | Ab4:1 F5:1.5 Eb5:.5 | D5:1 C5:.5 B4:.5 C5:2_"
    p.seq(0, line, lambda t: 60 + 14 * (t % 4 < 2), legato=0.95)
    p.seq(0, "C3+Eb3:2 G2+F3:2 | C3+Eb3:4 | F2+C3:2 G2+B2:2 | C3+G3:4_", 46, legato=0.97)
    for t, beats in ((2.5, 1.5), (6, 2), (10.5, 1.5), (14, 2)):
        n = int(beats * 58 / 60 * 5.5)  # about 5.5 trembles a second
        for i in range(n):
            x = t + beats * i / n
            p.cc(x, MOD, 70 if i % 2 == 0 else 0)
        p.cc(t + beats, MOD, 0)
    return p


def harp():
    """Rolled chords in Eb climbing the harp, a glissando, a quiet answer."""
    p = Piece("Harp phrase", 64, seed=4)
    chords = [("Eb2 Bb2 G3 Eb4 G4 Bb4", 0), ("C2 G2 Eb3 C4 Eb4 G4", 4), ("Ab1 Eb2 C3 Ab3 C4 Eb4", 8),
              ("Bb1 F2 D3 Bb3 D4 F4 Bb4", 12)]
    for notes, t in chords:
        for i, n in enumerate(notes.split()):
            p.seq(t + i * 0.18, f"{n}:{3.6 - i * 0.18:.2f}_", 64 + 3 * i, legato=1.0)
        p.seq(t + 2, "Bb5:.5 G5:.5 Ab5:.5 F5:.5" if t < 12 else "D6:.5 C6:.5 Bb5:1", 70)
    p.gliss(16, 2, "Eb4", "Eb7", 50, 84)
    p.seq(18.5, "Eb2+Bb2+G3+Eb4:3.5_", 60)
    p.seq(19, "G5:.5 Bb5:.5 Eb6:2.5_", 58)
    return p


def guitar():
    """Fingerpicking in A minor, then a strummed chord."""
    p = Piece("Guitar phrase", 84, seed=5)
    shapes = [("A2", "E3 A3 C4 E4"), ("F2", "C3 F3 A3 C4"), ("C3", "G3 C4 E4 G4"), ("G2", "D3 G3 B3 D4")]
    t = 0
    for rep in range(2):
        for bass, upper in shapes:
            u = upper.split()
            p.seq(t, f"{bass}:2_", 72)
            p.seq(t, f"r:.5 {u[1]}:.5 {u[2]}:.5 {u[3]}:.5 {u[2]}:.5 {u[1]}:.5 {u[2]}:.5 {u[3]}:.5", 60 + 6 * rep, legato=1.0)
            t += 4
    for i, n in enumerate("A2 E3 A3 C4 E4".split()):
        p.seq(t + i * 0.04, f"{n}:4_", 78, swing=False)
    return p


def zither():
    """A psaltery's tune in G, its long notes tremolo (one string plucked fast)."""
    p = Piece("Zither phrase", 76, seed=6)
    tune = [("B4", 1), ("D5", 1), ("G5", 2), ("F#5", 1), ("E5", 1), ("D5", 2), ("E5", 1), ("G5", 1), ("B5", 2),
            ("A5", 1), ("F#5", 1), ("G5", 4)]
    t = 0
    for n, d in tune:
        if d >= 2:
            p.seq(t, " ".join([f"{n}:.125"] * int(d * 8)), lambda x: 58 + 8 * ((x * 8) % 2 < 1), legato=1.0, swing=False)
        else:
            p.seq(t, f"{n}:{d}", 70)
        t += d
    return p


def vibraphone():
    """A ballad in F: four-mallet chords, a melody over them, the pedal held through each bar."""
    p = Piece("Vibraphone phrase", 84, seed=7)
    chords = ["F3+A3+C4+E4", "D3+F3+A3+C4", "G3+Bb3+D4+F4", "C3+E3+G3+Bb3", "F3+A3+C4+E4"]
    for i, c in enumerate(chords):
        t = 4 * i
        p.chord(t, 3.8 if i < 4 else 6, c, 56)
        p.pedal(t, 0, 127) if i == 0 else p.change(t)
    p.seq(0, "A5:1.5 G5:.5 F5:1 C5:1 | D5:2 C5:1 A4:1 | Bb4:1 D5:1 F5:1.5 E5:.5 | E5:2 G5:2 | F5:4_", 72)
    p.pedal(22, 127, 0, 0.5)
    return p


def kalimba():
    """An ostinato in C, thumbs alternating, a tune on top."""
    p = Piece("Kalimba phrase", 104, seed=8)
    pattern = "C4:.5 G4:.5 E4:.5 C5:.5 D4:.5 A4:.5 F4:.5 B4:.5"
    t = 0
    for rep in range(4):
        p.seq(t, pattern, 60 + 4 * rep, legato=1.0)
        t += 4
    p.seq(8, "E5:1 G5:1 C6:1.5 B5:.5 | A5:1 G5:1 E5:2 | C6:4_", 74)
    p.seq(16, "C4+E5+C5:4_", 66)
    return p


# the features' phrases: short, the same music plain and with the feature

def feature_piano(pedal=True, sostenuto=False):
    """A held chord and a line over it (8 s): the base for the pedal comparisons."""
    p = Piece("Feature phrase", 76, seed=9)
    p.seq(0, "C3+G3+E4:4_ | F2+C3+A3:4_ | G2+D3+B3:4_ | C3+G3+E4:4_", 58, legato=0.98)
    p.seq(0, "G4:1 E4:.5 G4:.5 C5:2 | A4:1 C5:1 F5:2 | D5:.5 C5:.5 B4:.5 A4:.5 G4:2 | E5:4_", 70)
    if sostenuto:  # the first chord caught, the rest played short
        p.sostenuto(0.3, True)
        p.sostenuto(15.5, False)
    elif pedal:
        p.pedal(0, 0, 127)
        for t in (4, 8, 12):
            p.change(t)
        p.pedal(15.5, 127, 0, 0.4)
    return p


def feature_chords():
    """I–IV–V–I in C, held (8 s): a temperament's beating against equal temperament."""
    p = Piece("Feature chords", 60, seed=10)
    p.seq(0, "C3+G3+C4+E4:2 F3+A3+C4+F4:2 G3+B3+D4+G4:2 C3+G3+C4+E4:2_", 62, legato=0.99)
    return p


def feature_mallets():
    """A short vibraphone line (7 s): the motor's tremolo on and off."""
    p = Piece("Feature mallets", 80, seed=11)
    p.chord(0, 8, "F3+A3+C4+E4", 54)
    p.seq(0, "C5:2 A4:1 F5:3 E5:2_", 70)
    p.pedal(0, 0, 127)
    p.pedal(7.5, 127, 0, 0.4)
    return p


def feature_harpsichord():
    """A cadence (5 s) for the stops."""
    p = Piece("Feature harpsichord", 96, seed=12, jitter_s=0.003)
    p.seq(0, "D5:.25 F5:.25 A5:.25 F5:.25 E5:.25 G5:.25 Bb5:.25 G5:.25 A5:.5 F5:.5 E5:.5 C#5:.5 | D5:2_", 74, legato=0.85)
    p.seq(0, "D3:1 F3:1 G3:.5 E3:.5 A2:1 | D2+D3:2_", 66, legato=0.85)
    return p


def feature_clavichord(bebung=True):
    """One long note and a step (6 s), trembled or not."""
    p = Piece("Feature clavichord", 60, seed=13)
    p.seq(0, "C5:3 B4:1 C5:2_", 66, legato=0.97)
    p.seq(0, "Eb3+G3:6_", 48)
    if bebung:
        for i in range(16):
            p.cc(0.3 + 2.6 * i / 16, MOD, 76 if i % 2 == 0 else 0)
        p.cc(3, MOD, 0)
    return p


# ---- Weidinger, the brass: one line each (a slur where the notes overlap, a tongued note where they do not), and the
# features' short phrases

SLUR, TONGUE = 1.06, 0.82


def trumpet():
    """A fanfare in B-flat, tongued; a slurred song; the call again, higher."""
    p = Piece("Trumpet phrase", 96, seed=21)
    t = p.seq(0, "Bb4:.5 Bb4:.25 Bb4:.25 D5:.5 F5:.5 | Bb5:1.5 F5:.5 D5:1 r:1", 100, legato=TONGUE)
    t = p.seq(t, "F4:1 Bb4:1 C5:1 D5:1 | Eb5:1.5 D5:.5 C5:2 | D5:1 C5:.5 Bb4:.5 A4:1 G4:1 | F4:3 r:1",
              lambda b: 64 + 3 * ((b - t) % 4), legato=SLUR)
    p.seq(t, "F4:.5 Bb4:.5 D5:.5 F5:.5 | Bb5:.75 A5:.25 Bb5:3_", 108, legato=TONGUE)
    return p


def horn():
    """A horn call in F, slurred and soft, then answered loud."""
    p = Piece("Horn phrase", 72, seed=22)
    t = p.seq(0, "C4:1 F4:1 A4:1.5 G4:.5 | F4:1 C5:2 A4:1 | Bb4:1 A4:.5 G4:.5 F4:1 E4:1 | F4:3 r:1", 62, legato=SLUR)
    p.seq(t, "C4:.5 F4:.5 A4:.5 C5:.5 | A4:.5 C5:.5 F5:2_", 104, legato=TONGUE)
    return p


def tenor_trombone():
    """A ballad in E-flat, slurred, a swell to its top."""
    p = Piece("Tenor trombone phrase", 76, seed=23)
    line = ("Bb2:1 Eb3:1 G3:1 Bb3:1 | C4:1.5 Bb3:.5 G3:2 | Ab3:1 G3:.5 F3:.5 Eb3:1 F3:1 | G3:3 r:1 | "
            "Bb3:1 C4:1 D4:1 Eb4:1 | F4:2 D4:2 | Eb4:4_")
    p.seq(0, line, lambda b: 58 + 40 * max(0.0, 1 - abs(b - 20) / 12), legato=SLUR)
    return p


def bass_trombone():
    """A low line in G minor: tongued and walking, then slurred down to the bottom."""
    p = Piece("Bass trombone phrase", 92, seed=24)
    t = p.seq(0, "G1:1 Bb1:.5 C2:.5 D2:1 F2:1 | G2:1 F2:.5 D2:.5 C2:1 Bb1:1 | Eb2:1 D2:.5 C2:.5 Bb1:1 A1:1 | G1:2 D2:1 r:1",
              92, legato=TONGUE)
    p.seq(t, "D3:2 C3:1 Bb2:1 | A2:2 F#2:2 | G2:2 D2:2 | G1:4_", 70, legato=SLUR)
    return p


def tuba():
    """An oom-pah in B-flat, short and loud, then the tune slurred."""
    p = Piece("Tuba phrase", 104, seed=25)
    t = p.seq(0, "Bb1:1 r:1 F1:1 r:1 | Bb1:1 r:1 F1:1 r:1 | Eb2:1 r:1 Bb1:1 r:1 | F1:1 r:1 C2:1 r:1", 96, legato=0.5)
    p.seq(t, "Bb1:1 D2:1 F2:1 Bb2:1 | C3:1.5 Bb2:.5 A2:1 F2:1 | G2:1 F2:1 Eb2:1 D2:1 | Bb1:4_", 74, legato=SLUR)
    return p


def feature_brass(vel=80):
    """The trumpet's feature phrase (7 s): four tongued notes, then a slurred turn and a held note."""
    p = Piece("Feature brass", 92, seed=26)
    t = p.seq(0, "F4:.5 Bb4:.5 D5:.5 F5:.5", vel, legato=TONGUE)
    p.seq(t, "Eb5:.5 D5:.5 C5:.5 D5:.5 | Bb4:4_", vel - 6, legato=SLUR)
    return p


def feature_horn():
    """The horn's feature phrase (7 s), a fourth down from the trumpet's."""
    p = Piece("Feature horn", 92, seed=29)
    t = p.seq(0, "C4:.5 F4:.5 A4:.5 C5:.5", 80, legato=TONGUE)
    p.seq(t, "Bb4:.5 A4:.5 G4:.5 A4:.5 | F4:4_", 74, legato=SLUR)
    return p


def feature_brass_chords():
    """Three chords, every key at once: a section, or one player taking the last key."""
    p = Piece("Feature brass chords", 80, seed=27)
    p.seq(0, "Bb4+D5+F5:2 C5+Eb5+G5:2 | D5+F5+Bb5:4_", 84, legato=0.96)
    return p


def feature_glide():
    """The tenor trombone's slurs within a tritone (7 s): by the lips, or by the slide."""
    p = Piece("Feature glide", 76, seed=28)
    p.seq(0, "F3:1 Ab3:1 Bb3:1 C4:1 | Eb4:1.5 C4:.5 Bb3:1 Ab3:1 | Bb3:3_", 70, legato=SLUR)
    return p


def sad_trumpet():
    """Wah-wah-waaah on the trumpet's plunger (CC 4, the plugin's default tie): three notes falling a semitone each, the
    hand shut at each attack and opened during the note, the last one long, opened slowly, its vibrato (CC 70) widening."""
    plunger, vibrato = 4, 70
    p = Piece("Wah-wah-waaah", 60, seed=30)
    p.cc(0, plunger, 8)
    for t, n, dur, opening in ((0.0, "G4", 0.7, 0.45), (0.9, "F#4", 0.7, 0.45), (1.8, "F4", 2.6, 1.4)):
        p.seq(t, f"{n}:{dur}_", 84)
        p.ramp(t + 0.04, plunger, 8, 120, opening)
        if dur < 1:
            p.ramp(t + dur - 0.12, plunger, 120, 8, 0.1)
    p.ramp(2.4, vibrato, 0, 110, 1.4)
    p.ramp(4.3, plunger, 120, 30, 0.3)
    p.cc(4.8, vibrato, 0)
    return p


def silence():
    """No notes, the sustain pedal down: the audio input rings the free strings (with --latch)."""
    p = Piece("Silence", 60, seed=14)
    p.pedal(0, 0, 127)
    return p


PHRASES = {
    "piano": piano, "harpsichord": harpsichord, "clavichord": clavichord, "harp": harp, "guitar": guitar,
    "zither": zither, "vibraphone": vibraphone, "kalimba": kalimba,
    "f-piano": feature_piano, "f-piano-dry": lambda: feature_piano(pedal=False),
    "f-piano-sostenuto": lambda: feature_piano(sostenuto=True),
    "f-chords": feature_chords, "f-mallets": feature_mallets, "f-harpsichord": feature_harpsichord,
    "f-clavichord": feature_clavichord, "f-clavichord-plain": lambda: feature_clavichord(bebung=False),
    "silence": silence,
    "trumpet": trumpet, "horn": horn, "tenor-trombone": tenor_trombone, "bass-trombone": bass_trombone, "tuba": tuba,
    "f-brass": feature_brass, "f-brass-soft": lambda: feature_brass(40), "f-brass-loud": lambda: feature_brass(118),
    "f-brass-chords": feature_brass_chords, "f-glide": feature_glide, "f-brass-horn": feature_horn,
    "f-sad-trumpet": sad_trumpet,
}


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, make in PHRASES.items():
        p = make()
        p.write(os.path.join(OUT, f"{name}.mid"))
        print(f"{name}.mid  {len(p.notes)} notes")


if __name__ == "__main__":
    main()
