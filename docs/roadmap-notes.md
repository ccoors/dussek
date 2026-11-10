# Roadmap notes

Background for the items of [ROADMAP.md](../ROADMAP.md): measurements, options, what was tried.
"Step N" is a step of the [design log](log/README.md).

<a id="user-guide"></a>
## The user guide

The highest-value item that is not DSP. It starts from [quick-start.md](quick-start.md), which
has a line on each of the controls that matter. The topics collected so far, each a line there
now and a section in the guide:

- far microphones: what happens to the sound as the distance grows (at Audience, little: the
  room's level stays and only the direct sound's share falls);
- Distance level: the total kept, or the direct sound kept for an external reverb (Room off).

<a id="listening-round"></a>
## The sound in a blind listening round

On 2026-10-18 the user listened blind, loudness-matched, onsets aligned: our Grand D against
Grand D's own samples played by sfizz. Nine stimuli: C2, C4 and C6 at three dynamics, staccato,
a chord, a pedaled passage, repetition, and two excerpts from MAESTRO. Ours beat its own samples
on eight. The scripts are throwaway. What the user heard on ours, with what was measured:

- notes ringing on after their release: dampers 2–3× too weak (step 119), then the room's tail
  and the release noise;
- the release noise too loud (C2, a chord) and the pedal noise too loud and clicky: both
  Salamander's, at its sampler's mix and from microphones 12 cm over the strings (the release
  12 dB lower, step 120; both tilted as a player hears them, step 121; the pedal's release a
  landing, step 122). The pedal down could be more subtle still: for now the Pedal noise control
  sets it, −20 dB by default, the ring apart at −10 dB (steps 210–211; most MIDI sources send the pedal as 0
  or 127 alone);
- the velocity steps uneven: peak level +7–8 dB from velocity 30 to 64, +12–15 dB from 64 to 110;
  "the first chord too weak" (step 124: one curve over the velocity on
  every key, its range the Dynamics control);
- the treble (C6) too short, its attack "a flat pop", not resonating through the
  instrument;
- repetition "machine-gun": every strike the same, and an overtone that sounds synthetic;
- the knock too loud above A4, most at soft strikes (the user played it at −16 dB): fitted to fill
  the treble's thump below the fundamentals, it overshot the recordings by up to 10 dB at 1.6 kHz
  (step 125: under the recordings; step 126: the hammer's thump through the board instead, its
  felt force lowpassed per piano);
- broadband noise at each attack, the treble dominating a pedaled passage, C4 "more distant",
  C2 "spatially flat", the Debussy melody far away with something like "bit-noise".

No longer heard as issues (2026-10-20, by ear, after steps 124–130): the treble's attack ("a flat
pop"), the bass's aftersound (C2–B2, 6–10 dB over the recordings) and the broadband noise at each
attack. They leave the open list; the measurements above stand.

The room (step 123): between a held note's partials the recordings hold a sound that falls fast for
~0.15 s, then slowly. The engine's gaps hold only the room's response to the strings' onset: it
overfilled the middle and treble (Grand A +9 … +12 dB at 2–4 kHz) and left the bass short at the
attack. The room is now cut per band where it overfills, the partials keeping their level. Open: the
bass at the attack, 9 dB (Grand A) to 29 dB (Grand D) short at 250 Hz. What did not work:

- a louder room: the partials pass through it too, so the gaps' ratio to them saturates;
- the room with an early part, or with the recordings' slower late decay: it colored the partials
  at every microphone position but Recording, or was far too much room;
- one sound added at every strike, fitted to the gaps: as diffuse noise (with a rise, with a smooth
  spectrum) and as a force pulse into the board, both heard as a click on every note. At the
  Recording position every key shares one board response, so it is the same sound on every key;
  the first knock (a pulse through the board, "the same ping-pong ball on every key") failed the
  same way.

What the recordings hold there depends on the note. Candidates for a source per note: the hammer
meeting the string (a broadband force at the key's own bridge point, the contact's shape), or the
board's response per bridge zone at the Recording position as the microphone positions have.

Also seen on the way: one second after a strike Grand A holds its level 10–12 dB higher re its
attack's peak than Salamander's samples played by a sampler: the attack against the sustain.

The treble's sustain (step 127): a longer C6 was asked for (the recordings' fundamental falls
18–20 dB/s on Grand A, 12–16 on Grand D); ours was short of the
recordings only in the aftersound's level, now fitted per two octaves. Heard on Grand D as ways to
a longer ring: less string loss (B2) and half the bridge coupling (B3, quieter and longer),
for a Sustain control. The thump's fit (step 126) moved with the aftersound on five pianos (Upright
A's to none): its level reads the 200–700 Hz band below C6, where the strings' onset adds; the
thumps are held at their values heard in step 126, a refit of the knock step would move them again.

The bass above 1 kHz (2026-10-20, steps 131–133; designs 2026-10-20-bridge-mobility and
2026-10-20-bass-high-band): the fits see only the first partials (the analysis tracks 60, the
correction used ≤ 40, the felt ≤ 20, the coupling ≤ 12), which in the bass end below the range the
user heard missing. Measured with `proto/bass_check.py`, recording against the engine at the layers'
own velocities. Also seen: the low partials of C2–C3 start 10–25 dB above their aftersound and fall
to it within 0.5 s in the recordings; the model's fast mode decays 2–5× slower at 90–280 Hz there
(Grand A C2 partial 3: 3.2 dB/s), and the room at 200 Hz holds the partial up for half a second
(room off: −3 dB at 0.5 s, the recording −20). A factor on the coupling per key (the bridge's
mobility) did not reach it.

The strings' loss past its measured keys (2026-10-23, after step 148): the fit's loss against the
recordings' own fundamental decays on the keys above each grand's measured range, model over
recording (above 1: the model decays faster):

| Grand | Measured to | Above it |
|---|---|---|
| B | 94 | 0.4–1.2×, about 0.5× near the top |
| C | 99 | 1.2–1.8× |
| D | 96 | 1.7× at first, 0.6× at the top |
| E | 92 | 1.8× at first, 0.55× at the top |
| F | 96 | 0.45–0.64×: rings about twice as long |
| G | 95 | two keys (0.7×, 1.65×) |

A straight line from the edge at its slope that never turns down brings the median from −0.28 to
−0.07 log2 (18 % too slow to 5 %), each grand still off by up to about 35 %. The other way: the top
keys fitted from their recorded fundamentals, the loss's high-frequency part kept from the smoothing.

Done in step 150, both: the top recorded keys' fundamentals join the curve, past them a straight
line that never falls. The medians above came to −0.22 … +0.14 log2 (Grand G −0.50 → −0.22). What is
left is per key: the engine's top keys decay 0.4–3× the recording's around the trend, Grand A's too.

Closed 2026-10-24, measured but not heard (the grands' closing round): the bass's aftersound in
C2–C3 6–10 dB over the recordings and A2's high band at 1.5 s; the board fit's leftovers (every
grand's pp furthest off step 143's measure, a fit falling below its data, the band rows for the
other grands' bass); the top keys' decay per key around the trend. The report against `voicing-1`
(A0–B2, engine − recording re 200–500 Hz, the median): within ±8 dB but Grand A's ff at 2–4 and
6–8 kHz (+7 … +8, its core, heard as right), Grand F's ff at 4–6 kHz (−10), Grand D's ff at 1–2 kHz
(+10) and 4–6 kHz (−8), Grand G's mf at 2–4 kHz (−12, was −29); the bass of Grand B, D and E
brightens from mf to ff 3–6 dB less above 4 kHz than any recorded grand (16–19 keys on Grand B):
the felts by ear.

The listening round's items closed 2026-10-24: Grand D's C4 ("more distant") and C2 ("spatially flat")
from the blind round heard as fine after steps 119–150, Grand G's top octave too.

The coupling fit's EDC error compares the model to the measured curve at the measured curve's times,
which stop at −40 dB, but does not hold the model there: a partial the model kills (Upright B's C6 at
11.6 and 14.6 kHz, −47 and −80 dB) weighs 20–65 dB in one note's error and bends the global fit (step
153). Holding the model at the floor is right by the fit's own description; tried on Upright B it
rang the treble shorter in the engine, so it waits for a refit of every piano with a listen.

<a id="rolled-pianos"></a>
## Rolled pianos

Rolled pianos (steps 117, 118) are new pianos made from the measured ones in the editor:

- the strings blended from three grands;
- a body set by Warmth and Air;
- a fine structure mixed from real responses;
- a realism check calibrated by ear.

They play in the standalone, which reloads changed instruments. A roll takes about 7 s.

What the listening found: a piano's identity rides with its body, the radiation responses'
coarse color with the per-note correction. The strings and hammers add character; Grand G's carry
it alone. A random fine structure is heard as "another microphone", not as synthetic. Blind,
rolls were heard as none of the ten real pianos. The warm side of Warmth sounded unbalanced or
reverberant.

What is open:

- Most of a roll is now the Player renders. Fitting the board's radiation instead of rendering it
  would make rolls faster.
- Player's level cannot be calibrated through the board's radiation, because the engine sets
  each zone's responses to the fitted responses' energy. Above 4 kHz, Player barely follows the
  board.
- The hammers stay together with the strings (the user's choice, step 118).

<a id="morphing"></a>
## Morphing

Done (steps 115, 116): the weighted morph of the hammer-strung pianos, in the engine and in the
plugin. Done (step 232): the harpsichords morph among themselves, and the harps; the chosen
instrument keeps its registers and stops, each blended with the others' of the same pitch. An instrument file is ~120 000 numbers:

- 96 000 are the two fitted radiation responses (the recording's fine structure);
- 8 500 are the soundboard's 1 220 modes;
- ~6 900 are the 78 numbers per key (strings 18, hammer 7, release noise 48).

Making one is a reshuffle of numbers in milliseconds, then an ordinary engine build.

What the first listening (2026-10-16) found:

- **Halfway between Grand A and Grand D: a plausible third piano.**
- **The average of Grands A–F: plausible.** Grand G, a special case, pulled the average of all
  seven darker and its sustain apart. A board cannot be averaged mode by mode: a new one is
  drawn from the averaged statistics and its radiation made from its modes. That is plausible
  but anonymous. The board's draw alone moved the sustain (the chord's drop over 1.9 s) from
  9.4 to 16.5 dB with G in: where the modes sit against the partials sets how fast the bridge
  drains each string.
- **Random grands** (Grand A's numbers varied smoothly along the keys): clearly different, but
  still recognizably Grand A at the recording's position, whose measured responses and per-note
  correction carry it. At a microphone position far less so. With a new random board and its
  radiation made from it: clearly new pianos, somewhat more synthetic sounding.

Which exports morph easily, by their structure (checked 2026-10-16; all span keys 21–108):

- **the grands (A–G)**: identical (hammer, the same 1/2/3-string layout, 1 220 board modes);
- **grands with uprights**: the same hammer and strings. The uprights' boards have 844 modes
  (rank pairing takes 844), a few bass keys change string count, and the body (it places the
  microphones) comes from the chosen side;
- **the harpsichords** (done, step 232): one body type, one plucked string per key. Harpsichord B
  has 3 registers, E and F 2, the others 1: the chosen one's registers, each with the others' of
  its pitch;
- **all plucked strings** (the harpsichords, the harps, the guitar, the strumstick, the
  psaltery, the đàn tranh): the same kind of note in the format, but their bodies differ
  (107–1 015 board modes). What one side has alone (measured polarizations, the bend) fades in
  or out. The two harps morph with each other (step 232; 251 and 141 modes, paired by rank);
- **the clavichord with them**: stored as a pluck, but two strings per key, its own body, the
  Bebung, a tangent that stays on the string;
- **the percussion**: explicit modes and a mallet throughout, but 3 (vibraphone, xylophone) to 8
  (marimba, bells, crotales) to 13–29 (hand chimes) and 42–56 (kalimba) modes per key. They would
  be paired by index, the extra ones fading. The motor and the damper bar exist on one side only;
- **hard**: hammer against pluck (one exciter needed). **Hardest**: strings against explicit
  modes. A piano and a bars instrument are both notes of modes in the same format: a bar's
  (ratio, damping, shape, output) per mode, a string's from f0 and B. A morph would need both as
  explicit modes and one exciter: an instrument that cannot exist, but can be synthesized.

Technically possible is not sounding good: a pair across a group (harpsichord to guitar) wants a
listen through scripts first, as Grands A, B, D, E and F got.

<a id="presets"></a>
## Presets

The Custom microphone position stores a setup as 13 numbers, so a named microphone preset is
small. Classic setups to ship: a close pair, ORTF at the player's seat, audience, a jazz-style
single microphone near the tail. Whole-configuration presets and snapshots would follow, with a
morph between two settings.

<a id="production"></a>
## A production piano

The user's observation (2026-10-19): Debussy's *Images I* sounds good on every grand and upright;
other pieces work only on Grands A and D (close-miked, brighter recordings); multichannel MIDI files
(even their piano tracks alone) work on none, and nothing sounds like a GM soundfont's "Acoustic
Grand Piano". The engine aims at a concert piano as recorded (wide dynamics, timbre following the
velocity, long sustain with the 200–500 Hz ring, a room); a GM piano is a production piano, voiced
and mixed to sit in an arrangement (a narrow, often compressed dynamic range, bright at every
velocity, shorter sustain, close and dry). MIDI files are also written against a GM synth's velocity
response: through our 45 dB curve their soft notes turn dull and quiet, their accents harsh, their
overlaps and pedaling ring longer than intended.

The comparison: the same MIDI through ours and FluidR3 (`/usr/share/soundfonts/FluidR3_GM.sf2`,
program 0), measured on
- the dynamic range per key from pp to ff (as the velocity curve's tables), and EBU R128's loudness
  range (LRA) over pieces;
- the brightness: the spectral centroid per register and velocity;
- the low mids ("mud"): the energy at 200–500 Hz re the total over the first second;
- the sustain: T60 per register, or the energy left after 0.5 s;
- the perspective: the direct-to-reverberant ratio or the clarity (C50/C80).

The knobs exist already: the Dynamics (around 20–25 dB instead of 45), the velocity curve, the
hammer hardness, the room and the microphone position; a preset sets them together.

Done (design log step 179): measured against FluidR3 and SGM, the gap is the brightness or the
loudness range, not the per-note dynamics; two shipped presets, Production, bright and Production, warm.

<a id="touch"></a>
## Touch

The velocity curve is done (step 110). Aftertouch is out of scope for the pianos and
harpsichords: once the key is down they no longer respond to pressure (the hammer has escaped,
the quill has plucked). The release velocity is already used (the damper fall).

A preset from the recordings: Grand A's 16 layers against the engine at their SFZ velocities
(2026-10-16, no change made).

- The tone is right at equal loudness: the middle within ±0.2 dB in brightness for v3–v16, the
  bass ~1 dB darker throughout, v1–v2 1.6–3.7 dB too bright. The treble is too noisy to read.
  Three anchors are therefore enough for the fits.
- But between pp and mf (v4–v6) the engine plays ~2 dB softer than the recordings' steps, beyond
  the anchors' own ~2 dB. That is the ln-speed line through three anchors against the set
  author's velocity map.
- A "Grand A as recorded" curve from the 16 steps would close it. Since step 124 the pianos play
  one level curve over the velocity, fitted to those 16 layers as their set maps them (45 dB from
  velocity 1 to 127, within 1.2 dB).

Closed (design log step 180): measured again against the set as sfizz plays it, the engine's Grand A
lies within 2 dB RMS from velocity 16 up (45 dB fits the set best); it plays 3–5 dB louder at
velocities 24–32 and keeps falling below 16 (−38 dB at 4) where the set's softest layer floors at
about −31 dB. Not worth a shipped curve; "light/heavy action" curves were dropped too, since
keyboards have that setting themselves.

<a id="listening-passes"></a>
## Each instrument checked by ear

The editor's stages are done:

- the project files (step 76);
- discovery (step 77);
- the editor and its wizard (steps 79, 80);
- Compare (steps 95–100);
- level corrections (step 98);
- take checks (step 105).

Done by ear so far:

- the marimba's and the vibraphone's levels (steps 101–103, by octave run);
- Grand A's levels per key (step 104);
- Grand E's levels from Grand B's recordings (step 109);
- the bells' pp by hand (step 108);
- a first pass over the take checks' flags (step 106).

Still to do: the listening pass over each remaining instrument's flags (and the takes the
Accurate-Salamander notes name on Grand A), the corrections each one needs, then each instrument
rebuilt.

<a id="coupling-pitch"></a>
## The coupling fit's pitch pull

The bridge's reactive part (`[coupling] reactive`) is fitted by `fit_coupling.py` to the decay
curves alone. It moves each unison's in-phase mode, the prompt sound the pitch is heard from, by
S·κ·reactive rad/s; the other modes do not move. The analysis's pitch already holds the real
bridge's pull, so the model's is a detuning.

- **The effect**: by that formula every piano plays 1–13 cents flat, most in the bass (Grand C
  C4 −9.6, Grand E −10.8, Upright B −1.3). This matches the Compare page's pitch medians (−2 to
  −7 cents; Upright B and C 0).
- **The K18 trial** (`data/SOURCES.md`): it took a reactive part of 9.1 and played ~28 cents
  flat. A pitch term in the fit's error (1 dB per 3 cents) brought K18 to +1 cent, its reactive
  part to the bound (0.01), at 2.8 dB more EDC error. Not kept.
- **Next**: try it on a piano with clean recordings (Grand C) and judge by ear, pitch against
  decay, before the pianos are refitted. Raising `f0` instead would put the other modes sharp of
  the in-phase one.
- **Also open from the trial**: a felt fit for one recorded dynamic: copy the felt reference's
  speeds and felt, with a line-shaped reference felt written as a curve and the keys its readers
  declare.

- **The beats around middle C** (2026-10-27): the in-phase mode's pull also sets the middle keys'
  beat: the engine's run faster than the recordings' (C4–B4, over the ten pianos 2.2 against 1.5
  cents), Grand A's 7.3 cents on the first partial at C4 with the unisons in tune (`width=0`), as the
  formula gives (3 · 0.83 · 2.7 rad/s, 1.07 Hz). A pull is a constant number of Hz, so its beat in
  cents falls with the partial number, a detune's does not: the recording beats 1.8 / 1.0 / 0.8 cents
  on partials 1 / 2 / 3 (keys 60–77), the engine 5.2 / 2.4 / 2.5; with reactive 0.99 2.7 / 1.4 / 1.2
  and pitch −2.9 cents (now −5.9, the recording −0.2), with 0.3 2.1 / 1.5 / 1.2 and −1.0, the late
  level within ±3 dB. The user heard no difference between the three: no refit for it. The beats per
  partial are the better term for the fit than the pitch alone: they tell the pull from the detune.

The fit's surface has minima of nearly equal error far apart (step 106: two takes left out moved
it 8.02 → 7.95 dB, reactive 0.99 → 2.70, 3–6 cents flatter). Since step 114 a refit keeps to the
committed fit unless another start is better by more than 0.1 dB, and warns at a twofold jump.
The decay curves alone cannot tell the minima apart; the pitch term would.

<a id="rust-port"></a>
## The fitting pipeline in Rust

A full refit of all instruments took about 75 minutes (2026-10-10), most of it Python rendering
notes and evaluating models in loops. The engine already has the string, hammer and soundboard
models in Rust. Rendering the fits through it would also remove the risk of the two drifting
apart.

Done in Python first (2026-10-11): the free decay summed per block (13× faster), the knock fit's
grid vectorized, the board's admittance table shared per process, `free_modes` once per coupling
error. With these, Grand D's chain went from 17.5 to 4.8 min, the results bit-identical.

What is left:

- the coupling fit: 2 min, ~6500 Nelder–Mead evaluations of 20 ms, half of it the per-sample
  contact loop;
- the demo and the fine structure: 40 s each;
- the felt fit: 33 s.

The felt fit renders with the previous run's coupling: a felt ↔ coupling iteration, by design.

Every fit is a step (`projects/steps.toml`). Its declared reads, seeds, outputs and amendments
are the contract a Rust port must honor, and its outputs can be compared with the Python step's
on the same inputs (the ledger's fingerprints).

<a id="prepared-piano"></a>
## The prepared piano

A reference: Kurt James Werner's prepared-piano samples (*Samples and Such*, CCRMA, Stanford, 2011;
CC BY 3.0; 282 MB, not in `data/` yet): a grand prepared for Cage's *Sonatas and Interludes*, 43 of
its 45 prepared notes (D1, D2, D3, G3–F7), one take each at one unknown dynamic, 25 also with the
pedal down; 24 bit, 44.1 kHz; no unprepared reference and no labels of what sits where (the table
is in the score). Measured on 2026-10-31 against Grand A and Grand D's renders of the same keys:

- **Near-unprepared** (15 keys, most above F5): harmonic within ±10 cents, decays like a grand's:
  the set's own reference.
- **Light masses** (8 keys): the fundamental 100–170 cents lower. The engine's bolt (a point mass,
  first order) holds the first two partials: D4 at mass 0.3, position 0.34 renders −116/−98 cents
  against the recording's −127/−78, t20 0.50 against 0.54 s; from the third partial up the curves
  part.
- **Heavy masses** (11 keys): the fundamental 700–900 cents lower, the second partial 260–430, from
  the fourth on back near the series; only the exact point-mass solution (the roots of
  cot(xp) + cot(x(1−p)) = R·x, mass ratio R 0.4–2.2) fits, the first-order bolt explains 1–55 % of
  the peaks' energy. Most keys keep one string of the unison unloaded at its pitch.
- **Treble screws** (7 keys): the free string at its pitch and a low component at 0.22–0.30 f0
  (R 4.5–20).
- **Rubber** (9 keys): t20 0.14–0.42 s against a grand's 0.74–2.28 (70–130 dB/s against 10–17), the
  pitch hardly moved: loss without mass.
- **Buzz and metal**: 4–12 kHz 10–30 dB over the grand at 0.1 s, the 5 ms envelope chattering 2–4×
  (nuts); one key's attack a metal clank (spectral centroid 3.7 kHz against 0.5).

What the engine lacks for it: a heavy mass (the exact solution), a preparation on some strings of a
unison, loss apart from mass (rubber), a rattle driven by the string's motion, an object's own ring.

A proposal: preparations per key in a preset and a project (the key, the object, which strings,
the position, the mass ratio, extra loss, buzz), compiled per key so a change rebuilds one note;
Plucked, Muted and Harmonic stay global, Bolt and Paper become per-key entries ("all keys" a
shortcut); in the UI, a key clicked on the player strip takes its object; a shipped "Prepared"
preset from our own fits, not the score's text. The set gives each key's fit (mass ratio, position,
loaded strings, loss, buzz) and a check (the first six modes within 25 cents, t20 within 30 %, the
high band within 3 dB); the rattle's color, the clank and the inches-to-fraction step stay by ear.
Its license needs `CC-BY-3.0` among `proto/license.py`'s source licenses and a credit line.

<a id="instruments"></a>
## The instruments' open points

**Pianos**:

- The three grands of one model (Grands B, D and E): their band at 101–127 Hz decays faster in the recordings (×1.6; the engine ×1.1).
  Grand F, Grand C and Upright A show the same at 202–254 Hz (×1.45). Both could be measured body
  modes (step 49), after the mismatch in the first 20 dB of decay is resolved (Iowa's model is
  ×1.4 too fast there).
- C6 decays 12–20 dB too fast on Grand E, Iowa and Knight (step 25).

**Harpsichords**: Edinburgh's Baillon double (1755) and Barton single (1709) are Harpsichords E
and F (step 196, CC BY 4.0), each with its second 8′ recorded. Harpsichord B's derived 8′ II could
be checked against them (the pluck point one jack row nearer the nut, the ±1.5 cents). Not yet
used: the Barton's staccato takes (its own release noise, the dampers landing) and its takes of
both choirs together (the bass decays to 18 s, the choirs' beating). Neither set recorded a 4′ or
a lute; both are noise-reduced, the Baillon also high-passed below 100–150 Hz.

**Clavichord**:

- the vibrato's amplitude swell;
- the held tangent's slight muting;
- the pitch rise at a hard strike;
- the tangent's own release noise (Salamander's damper noise for now);
- the room's level (a generic one);
- a body of its own (it uses the grand's microphone geometry);
- the vibrato takes as a check of the Bebung's depth.

**Harps**:

- The pedals' retuning (47 strings on 88 keys) is not modeled.
- The hand's sound at a release is not modeled.
- The soundbox has no air mode of its own.
- The body's geometry is assumed, not measured.
- Harp B's levers (a lever's semitone shortens the string at a fret) are not modeled: every key
  is its own string.

**Guitars and zithers**:

- the guitar: per-string coupling (the mode reads broader on the G string than on the A), the
  strings ringing along, a Position control for the other strings' recordings;
- the psaltery's undamped strings ringing along;
- the strumstick's two-stage fundamentals;
- the đàn tranh: the bend on the struck string alone (MIDI's wheel moves every sounding note,
  the left hand presses one), and the glissandi and FX takes.

**Percussion**:

- the marimba's rolls and dead strokes, and the treble's second peak beside the fundamental;
- the bells' brass mallet's dynamics (recorded at one);
- the crotales' split mode pairs (one mode per track);
- the tubular bells' damper loss (assumed), the 1.39–2.25 side peaks, the rack's geometry;
- the hand chimes' lowest keys' 13.8 and 15.9 modes, and a second dynamic for the clapper;
- the kalimba's ringing as a coupling that follows what was played (each key carries its own
  copy), and the mbiras' buzzers;
- the balafon's strike spike, its noise's tail after 0.3 s, each bar's own gourd peak, only 6
  bars recorded.

<a id="visualizations"></a>
## Visualizations

The spike (the archive repository's branch `viz-spike`, 2026-10-25) renders videos offline, each a binary in `engine/src/bin/`
that writes the engine's state and a script in `tools/viz/` that draws it (Python, PIL, ffmpeg):

- `midi2viz` + `draw.py`: a MIDI file with its audio; per frame and key the damper's engagement,
  the strings' displacement per partial (the vertical modes over their bridge weights) and the
  sympathetic bank's ring; the hammers' flight from the note-ons. The grand's strings from above, the
  hammers from the front, the keys.
- `strike2viz` + `strike.py`: one key's contact traced at 192 kHz, 2000× slower.
- `unison2viz` + `unison.py`: the unison's coupled modes from the strike, through a stroboscope, in
  three scenarios (the width as fitted, at 0 %, the soft pedal).
- `partials2viz` + `partials.py`: every key's partials against exact harmonics.
- `mics2viz` + `mics.py`: the grand's paths (direct, lid, floor) to the preset pairs, drawn in one
  slice through the strike point (the delays and levels in 3D).

Live is in reach: the engine's side is cheap (34 s of music with its frames in 2.7 s); the drawing
was the slow part (53 s on 8 cores in Python at 1080p), work a GPU does in under a millisecond a
frame. The plugin's telemetry (30 frames a second, the voices' partials) would carry each key's
displacement per partial, its damper and the bank's ring. Live, a hammer jumps to the string at
its note-on: the videos start its rise up to 40 ms before (pp), which a live note cannot.

What the views showed (Grand A's C4):
- the contact: 1.7 ms and 2.3 N at velocity 30, 1.1 ms and 32 N at 110; at ff the force peaks
  twice, the wave reflected from the agraffe (0.12 of the length away) meeting the hammer again;
  C7 and E1 are single contacts;
- the unison: the bridge splits each partial's modes about 1.1 Hz apart at any width, the strings
  moving together 1 Hz below the others (T60 6 s against 17 s); at width 0 a strike excites almost
  only that mode in partials 1–2 (the others 55–85 dB down), from partial 3 up the strings' scatter
  excites the others, within 0–5 dB after 3 s in partials 5–6: the beating heard at 0 %;
- the soft pedal: the spared string, pulled up through the bridge, is louder than the struck ones
  within 0.5 s.

The live panel and the video landed (steps 165–166); the one-note views stay in the spike, for a
visualization project of their own or the docs (2026-10-26).

<a id="more-instruments"></a>
## More instruments

Licenses checked 2026-10-10:

- No free set with a usable license was found for a celesta, a fortepiano or a Rhodes
  (jRhodes3d is CC BY-NC).
- Not worth it:
  - Splendid Grand: Akai's public-domain piano from around 2000, 4 layers, irregular spacing, no
    releases;
  - Scarypiano: CC0, but deliberately processed samples;
  - Maestro Concert Grand: its own license;
  - a church grand (Pianobook): Pianobook terms, no license stated.
- VSCO 2 CE's two uprights are Upright B and Upright A from other sessions: nothing new.

A clavichord was the one place for aftertouch (done: the Bebung).

<a id="key-range"></a>
## The key range: 88 keys

Decided 2026-11-02: every instrument keeps 88 keys (A0–C8). Considered and declined:

- 108 keys (C0–B8, as one maker's nine-octave grand): above C8 (4.2 kHz) pitch is hard to hear (beyond
  about 5 kHz), no music is written for those keys, and the model would play B8 with two partials,
  MIDI 127 with one (a sine and the thump).
- All 128 MIDI notes: strings no piano has (MIDI 0 at 8 Hz: a 2.9 m string, a growl).
- The bass to C0 (97 keys, as one maker's largest concert grand; Busoni's Bach transcriptions reach G0)
  is the one extension with a use. If it is wanted: a menu entry per grand as an export choice
  (the extra strings ring along under the pedal and change the 88's sound), the bridge extended a
  few centimeters past A0 so the 88 keep their places. "88 keys" is assumed in about 350 places
  (the engine, its C interface, the fits, the UI, the editor): about two weeks.

## Brass

**The soft notes' speech** (measured 2026-11-10). From each one's own first sound the engine's notes speak as fast as the
recordings' (the pitch within 80 c and the level within 6 dB of the note's own, held 100 ms: `proto/brass/speech.py`). What is
late is the wait from the key to the first sound, which the recordings cannot show: taking their first sound as the key, at pp
and mf from the middle of the range up the trumpet is 20–25 ms late, the horn 20–45 ms, the tuba 13–35 ms, the tenor
trombone's top third 60 ms at pp; at ff 0–10 ms. Near their threshold the lips build slowly (the horn's A♯4 at mf: −25 dB
at 16 ms, the breath, then −10 dB at 54 ms, −3 dB at 72 ms; at ff −5 dB at 28 ms).

`speak_fit.py` pushes the attack only for a note that takes over 60 ms (`TARGET`), and lets the push overshoot by up to 6 dB
(`OVER`). On the horn's G4–D5 at mf a push of 1.3 speaks in 14–34 ms for +2.3 to +3.7 dB at the attack (none: 29–64 ms,
+0.9 to +2.4 dB). The way, if the wait is heard: the target and the cap as `[brass]` settings (30 ms, 4 dB), the chain from
`brass_speak` refit, the horn first, as a variant for the ear: a refit changes every step after it, and the push trades the
wait for some of the burst steps 244–247 took out.

**The trumpet's low notes overshoot** (left as they are by ear, 2026-11-10). F♯3–F4 peak 4.5–10 dB over their sustain
(the recordings about 0). The cause is the scoop: the lips start up to 300 c slack (`compare5.attack_for`; `scoop_span`
caps it only where the next mode down is close) and lips under the note blow the bore harder: without the scoop +2.1 to +3.2 dB,
without the kick about 1 dB less, the burst and the virtual player's accent change nothing. The scoop gives the attack's pitch
glide, which the recordings have (−68, −34, −14 c at 20, 40, 80 ms at pp; the player −53, −13, −4 c; without the scoop −22,
+7, +2 c); a slower scoop matches the glide and overshoots more (+8 dB), less air while the lips are slack takes 1.5 dB off.
Not heard as explosive: left.

**A Visualization for the brass** (the maintainer's note, 2026-11-10). The plugin offers none on a bell, and
`midi2wav --video` refuses one (it drew a piano's keys and strings, which never moved). What it would show is open: the bell
and the air through it, the lips' motion, the bore's modes, the mute. The engine's telemetry carries strings and bars
(`telemetry.rs`, `VIZ_PARTIALS`); a bell needs its own frame.

**The mute in the microphone drawing** (the same note). The drawing shows the bell as held (`[pose]`, `mic.rs`
`bell_outlines`) but not the mute in it; a straight, cup, wah or bucket outline in the bell's mouth would show which one plays.

## Signing

What was found on 2026-11-10, to be looked up again when the work starts (prices, terms and eligibility change):

- **Windows, SignPath Foundation** (signpath.org): free for open source; needs an OSI-approved license (AGPL-3.0 is),
  automated builds on GitHub, multi-factor authentication on both accounts, a release before. The certificate is the
  foundation's: Windows names "SignPath Foundation" as the publisher. Official GitHub Actions submit the artifacts and the
  service checks they come from the repository's build; each project applies on its own. The likely choice.
- **Windows, Certum Open Source Code Signing:** about €49 a year (the key in Certum's cloud, SimplySign) or €69 with a
  card; the publisher the developer's own name, one certificate for all one's open-source projects. The cloud key's one-time
  codes make an unattended CI run awkward, and certificates are now limited to about 15 months.
- **Windows, Azure Artifact Signing:** about $10 a month, but an individual developer must be in the USA or Canada; in the
  EU only organizations.
- Either way SmartScreen may warn until a signed file has built a reputation.
- **macOS, Apple's Developer Program** ($99 a year): Developer ID Application for the apps and plugin bundles, Developer
  ID Installer for the package, notarization (`notarytool`, an App Store Connect API key); one account for every project.
  In the workflow it replaces `packaging/macos/build-pkg.sh`'s ad hoc signatures.
