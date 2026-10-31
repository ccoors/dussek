# Recording a piano: the session sheet

A sheet to print for sampling a piano of one's own (written for an upright in a living room, a
handheld recorder with an XY pair). What the fits need: three or more dynamics, pp among them (a
felt fitted without pp plays too bright when soft, design log step 142), every minor third, each
note rung out; and what the current upright sets lack: key releases (the dampers and the release
noise) and the pedal's noise. Before the session, a test recording (a few notes at pp and ff in the
bass, the middle and the treble, from the planned position) checks clipping, the noise floor and
the room. The recordings are for analysis only, never in git (`docs/getting-data.md`); ask the
owner's permission.

## Setup

- [ ] Recorder: 48 kHz, 24 bit, WAV; auto-level, compressor/limiter and low-cut off; the XY pair
  in plain stereo (not MS), at 90°: from 50–80 cm the strings span about ±45°, each capsule on
  one end and the middle register on axis; 120° takes the middle 60° off axis and aims the
  capsules at the side walls (more of the room), with no gain for the fits.
- [ ] On a stand or tripod, its place marked with tape; not moved during the session.
- [ ] The piano's top lid open; the recorder 50–80 cm in front of the upper front panel, 1.2–1.4 m
  high, a little left of center (the bass strings), aimed at the opening.
- [ ] Gain set by hand once: the loudest ff bass note peaks near −6 dBFS. Not touched again (the
  dynamics' level differences are data).
- [ ] Started by remote, or before sitting down; the recorder not touched while it runs.
- [ ] The room: fridge, heating fans and other hum off; windows closed; phones silent. A heavy
  blanket behind an upright standing at a wall; curtains closed, a rug down.
- [ ] Tuned recently, if possible (a sour unison is recorded as it is).

## The passes

One file per pass, the keys in the table's order, ascending. A slip or a replay goes into the
notes column, not into the recording (speech near an onset confuses the note detection); a
replayed note goes at the end of its pass.

1. **Silence:** 30 s, nothing played (the noise floor).
2. **The dynamics:** one pass per dynamic, all keys, each struck twice; pp, mf and ff at least, p
   and f if time allows. Each note held until it has died into the noise (about 20–30 s in the
   bass, 10 s in the middle, 5 s at the top), then released slowly; about 2 s before the next.
   The dynamics clearly apart (8–12 dB), even across the keyboard; exact touch matters less, the
   export voices every key to one level curve (step 138).
3. **Releases:** mf, all keys once: held about 2 s, then released normally.
4. **The pedal:** the sustain pedal pressed and released about 10 times, no notes; slowly and
   quickly.
5. **Optional:** one note per octave at mf with the sustain pedal down, rung out.

About 1.5–2 hours with breaks for three dynamics.

## Takes

A tick per strike that went well (☐ ☐: the two strikes of a dynamic).

| Key | MIDI | pp | (p) | mf | (f) | ff | Release | Notes |
|---|---|---|---|---|---|---|---|---|
| A0 | 21 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C1 | 24 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| D♯1 | 27 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| F♯1 | 30 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| A1 | 33 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C2 | 36 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| D♯2 | 39 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| F♯2 | 42 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| A2 | 45 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C3 | 48 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| D♯3 | 51 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| F♯3 | 54 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| A3 | 57 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C4 | 60 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| D♯4 | 63 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| F♯4 | 66 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| A4 | 69 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C5 | 72 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| D♯5 | 75 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| F♯5 | 78 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| A5 | 81 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C6 | 84 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| D♯6 | 87 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| F♯6 | 90 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| A6 | 93 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C7 | 96 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| D♯7 | 99 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| F♯7 | 102 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| A7 | 105 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| C8 | 108 | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |

Around the string breaks (where the wound bass strings end, and where the dampers stop near the
top), a few keys more are welcome if time allows:

| Key | MIDI | pp | (p) | mf | (f) | ff | Release | Notes |
|---|---|---|---|---|---|---|---|---|
| | | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| | | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |
| | | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ ☐ | ☐ | |

## Afterwards

The files as recorded, with this sheet (or a photo of it). `analysis/split_session.py` cuts each pass
at its strikes and names the takes after the table (`<key>_<dynamic>_<take>.wav`); a take stopped
early (a mis-strike, replayed), a wrong note, a key touched rather than struck and a clipped take
(where the key has another) go to `discarded/`, each with its reason in the pass's JSON. The two
strikes of each dynamic are ranked by how near their level lies to the neighboring keys' and the more
even one is played. The releases pass gives each take from its damper's fall; the keys around the
breaks, played at various velocities, take the dynamic their level is nearest. Then one SFZ file over
all of it is the set's map, its release regions the dampers' and the release noise's source:

```sh
R=<the recordings>; D=data/<set>
python3 analysis/split_session.py $R/pp.wav $D --dynamic pp      # likewise mf, ff
python3 analysis/split_session.py $R/releases.wav $D --dynamic mf --releases --damped-to 88
python3 analysis/split_session.py $R/breaks.wav $D --dynamic auto --keys 50,88,89
python3 analysis/split_session.py --sfz $D --name <set>.sfz
```

Upright D was recorded so (design log step 204).
