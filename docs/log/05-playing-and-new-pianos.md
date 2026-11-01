# Touch, MIDI learn, morphing and rolled pianos (steps 110 on)

The velocity curve, MIDI learn, faster rebuilds, the morph between instruments and rolled pianos.
New steps are appended here. Index of every step: [README.md](README.md).

<a id="step-110"></a>
## 110. The velocity curve editor

The velocity curve editor.
MIDI files whose touch is off (too loud, too soft, too even, too uneven) had no fix but the
exponent slider, which bends the whole range one way. The curve now has control points (2–8
at whole velocities, the ends at 1 and 127 moving up and down only) on a Touch tab after Sound:
a monotone cubic through them (Fritsch–Carlson, Butland's weights: it never falls nor
overshoots), velocity in → velocity played, in the engine (`engine/src/velocity.rs`; the C API
`pe_set_velocity_curve`, its text through `pe_velocity_curve_parse/format`, `pe_adopt` keeping
it), so midi2wav (`--velocity-curve`) and a later standalone play the same. Velocity to velocity
rather than to hammer speed: one curve means the same on every instrument (it is kept across
switches). The plugin saves it as the `velocity_curve` state; a lock-free sequence lock hands
it to the audio thread, which sets it on every new engine (a switch or a session restore builds
one without `pe_adopt`). The incoming velocities reach the UI through the telemetry region (a
64-slot ring, written whether or not the diagnostics are on; a reader gets the newest 63), and
show as dots on the curve (4 s) and bars of the last 30 s in 32 bins. The exponent slider is
gone (69 parameters). Four notes at velocities 60–72 (Grand A's C4, peaks) span 2.3 dB with the
straight curve, 11.8 dB with a steep middle (55 → 30, 75 → 100; `engine/tests/velocity_spread.rs`
holds it at twice at least). The final review found the audio thread's take of the curve could
wait on a publish half done (now one attempt per block), a new engine at a freed one's address
missing the curve (the slot forgets its engine on every swap), and the notes' reader reopening
the region, and losing notes, each time an LV2 host re-sent the token unchanged.

<a id="step-111"></a>
## 111. MIDI learn

MIDI learn. A host's MIDI
learn reaches only the automatable parameters (not the ones that rebuild the engine, nor the
instrument, a state), and the JACK standalone has no host at all. The plugin now ties CCs to
every control itself. The map (`plugin/src/MidiMap.*`: ties by parameter symbol, the program
change switch) lives in one file every instance shares (`$XDG_CONFIG_HOME/pianoengine/midi-map.txt`):
it describes the controller, not a sound. The worker reads it into a lock-free table (an atomic
per channel × CC); `run()` asks a router (`plugin/src/MidiRoute.*`) what each CC does: caught
for the control that waits (the UI sets `kDiagLearnIn`; the DSP answers in `kDiagLearnedOut`;
the pedals and CC120/123 pass), a tie's value (sliders over their travel, switches at 64, lists
in bands over the menu the instrument shows, published by the worker at each load), a step of
the instrument (once per rise through 64), or the built-in handling. A value is set as the
user's edit would set it (the 8' II and the lute exclude each other) and reported with DPF's
`requestParameterValueChange` (the JACK standalone, LV2 hosts with the
control-input-port-change-request feature, optional in the manifest); where the host takes
none, the DSP writes it to a value mirror in the telemetry region (128 slots, one atomic word
each: a sequence and the value; region version 3) and the UI passes it on to the host.
Instrument steps and program changes go to the worker, which sets the "instrument" state and
tells the UI in `kDiagInstrumentOut` (cleared when another choice replaces it). The ties work
while no engine plays. Rebuild sweeps needed nothing new: the worker already drops a build that
a newer request superseded. A 1 s sweep of 100 values: the lid at the Player position (a build
0.4–0.55 s) 3 builds, ending on the last value; Unison width (a few ms each) 29, the sound
following the knob. Checked under Xvfb with MIDI from a virtual port in the JACK standalone and
in Carla (learn, values, Next instrument once per pad press, program change with the switch
off and on, the ties surviving a restart and Reset). jalv never delivers the later output
ports (the diagnostics token among them) to its UI, so the mirror's way to the host is tested
at the region and client level only. The seven tabs' padding shrinks (20 → 14 px) so they keep
one row at the default width. The final review found "Learn again" on the same control waiting
forever when no CC came in between (the router now sees the wait at every block), and a UI
opened after MIDI moved a control showing the host's old value (a mirror slot now holds a value
the host has not heard of, NaN once the host sends a newer one, and a UI that just opened
applies what is left). Two layout tests had written past their arrays all along (the Instrument
row's index counted into parameter-sized arrays); the new parameters moved the write onto live
heap data, and ui_tests crashed one run in twenty until AddressSanitizer showed where.

<a id="step-112"></a>
## 112. MIDI learn's review leftovers

A tied control's label column leaves room for its CC tag (it
covered the end of long labels: "Hammer hard…"); a header slider's tag sits after its label, a
header button's on its corner. A change to the map is made to the file as it is now (another
instance's new ties stay) and kept only when the file is written (a tie the DSP never heard of
showed a tag and did nothing). kDiagLearnIn is clamped to the targets (a host restores it with a
session: anything else is no wait), and a UI that sees a wait it did not start ends it. A tie
reload makes the router forget Next's and Previous's last values (a pad moved to another CC
missed its first press). The tuning comparison moved to the worker (`TuningWatch`): through MIDI
learn the audio thread sets parameters too, and the two threads raced on the last tuning, which
could miss a rebuild; a tuning change now waits up to the worker's 50 ms poll.

<a id="step-113"></a>
## 113. A faster engine build at a microphone position

A faster engine build at a microphone position (IDEAS 28). A build took 0.40–0.42 s at a
position other than the recording's (12 ms there), Grand A at 48 kHz: the plugin's position
changes and the custom pair's drags waited that long. Timed by phase: the soundboard's
synthesis (`mic::synth_body`: 1198 modes into 32 zone × polarization × microphone rows, 24000
samples) spent its 16 threads remaking every mode's oscillator per thread, a serial chain of
complex products; sending the zones along their paths (`along_paths`) built its FFT plans and
its directivity table (64 cones per bin) on every call, and ran the 8 zones one after another,
0.26–0.28 s. Now the threads take spans of the time axis (each mode starts a span at its exact
value e^{t·ln z}; to rounding, 1e-9 of a row normalized to energy 1), the plans, the directivity
and the air absorption are made once per FFT size, the zones and the two polarizations' rooms
run in parallel (the same bits). A build: 0.10 s (the vibraphone 0.09 s). The radiation cache
keeps the last six setups (the plugin's position menu has six; a few MB each): switching back
to a position recently used rebuilds in about 12 ms. The engine suite's time stays (≈ 2 min;
its slowest tests are renders: `no_effect` 84 s).

<a id="step-114"></a>
## 114. The coupling fit keeps to the committed fit, and a jump is reported

The coupling fit keeps to the committed fit, and a jump is reported (IDEAS 27). Its surface has
minima of nearly equal error far apart: two Grand A takes left out moved it 0.07 dB lower into
another one (reactive 0.99 → 2.70, 3–6 cents flatter, more ring), unnoticed by the build. A
refit now starts from the committed fit as well as from its four fixed starts, and keeps the
committed start's result unless another start is better by more than 0.1 dB
(`fit_coupling.choose`); a parameter that moved twofold from the committed fit prints a
`warning:` line, which `proto/build.py` shows (a `warning` event with `--json`). Grand A refit
in a scratch copy: every parameter within 0.5 % of the committed fit, error 7.948 dB both times
(the fixed starts still found the 8.02 dB minimum, now passed over). From a committed reactive
part set to 10 the fit stopped at 9.56 (hdetune 0.021 → 0.146) at 7.953 dB, as low as at 2.70:
the decay curves alone cannot tell those apart, although 9.6 plays far flat; the pitch term in
the error (IDEAS 27) is what would.

<a id="step-115"></a>
## 115. An instrument between instruments: the engine morph

An instrument between instruments: the engine morph (IDEAS 30). `Instrument::morph` takes
instruments and weights and builds a playable instrument from their files, as the throwaway
spike's halfway Grand A–E did, which the user judged a plausible third piano. Loading is now
two steps, decode (the files parsed) and build (the instrument, its noises and room
synthesized); the morph works between them, so the noise and room synthesis see interpolated
parameters. Hammer-strung pianos only (the grands and uprights; others are refused by name).
The rules: per key the strings' and hammers' numbers interpolated, scales geometrically and
offsets arithmetically, the string count from the heaviest side; the boards' modes paired by
relative rank (by rank where equal in size), frequency and damping geometric, shape and
radiation the magnitudes' geometric mean with the heaviest board's sign (a signed mean would
thin the board halfway), a larger board's unpaired modes faded by its weight (an upright
going to 0 restores a grand's 1 220 modes); the measured responses mixed at unit energy; the
noise impacts all kept, each side's quieter by its weight; the room's reverberation on every
side's frequencies; the level corrections averaged with 0 dB for a side without them. Two
jumps are accepted where the heaviest side changes: the paired modes' signs, and the string
count at a few keys between a grand and an upright. The level is measured: the stored per-key
peaks are means and missed what the morphed physics radiates (halfway A–E played 3.1 dB
above both its ends at ff: 1.1 dB from the median of the blended peaks, the rest the
physics), and the calibration level is no reference (the pianos sit up to 3 dB off it, A
+2.2 dB, E −3.1 dB at their median keys, so a piano morphed with itself would jump). Keys 48,
60 and 72 are rendered at ff on the morph and on each side, and the morph's peaks scaled to
the weighted dB mean of the sides' (halfway: −15.12 dB against A −16.86, E −13.37). A morph
of two grands builds in 0.55 s against a plain load's 0.13 s (best of three, `midi2wav`'s
load time); most of the difference is the level measurement (each side built and rendered),
which the plugin can cache per instrument. `midi2wav` and `render` take `--morph DIR=W`. The
profile names the recipe (`morph:salamander=0.5000,piano162=0.5000`), so each morph has its own
radiation cache entry.

<a id="step-116"></a>
## 116. The morph in the plugin

The morph in the plugin.
The Morph tab lists the chosen instrument and up to four partners (the `morph` state) and draws
them as a regular polygon; Morph X and Morph Y (parameters) place a point whose Wachspress
coordinates, rounded to 1 %, weigh them (`Morph.hpp`). The worker builds through
`pe_create_morph`, whose cache keeps the last four instruments by recipe, so a tuning change or
a point dragged back to a recent mix morphs nothing. The morph measured its sides' levels on
every build (most of step 115's 0.55 s); each instrument's ff level now comes with its export
(`level.json`, written by the engine's `level` tool at the end of `export.py` /
`export_bars.py`, so it goes stale exactly with the export: the export step declares no
outputs a separate step could follow), checked by `engine/tests/level_files.rs` (0.1 dB), and
two grands morph in 0.25 s (0.56 s without the files). The body comes from the chosen
instrument at any weight (the UI's microphones and controls follow it; an upright past 50 %
no longer moves them). Sounding notes do not carry over to another mix: carrying the strings'
state over to other strings (first tried, for engines of one chosen instrument) played Grand
G's held chord 21 dB too loud in Grand E (its strings swing far more for the same loudness),
and the other way it rose 8 dB, a burst of ringing notes when the point jumped; the handover
takes notes over within the same instrument only, as before. Where it cannot, the plugin
fades the old engine out over 30 ms beside the new one (`FadeOut`; before, an instrument
switch cut its notes dead, a click at every build while the pad was dragged). A build that a newer request
superseded is still played when only the point or the tuning moved on, so a drag is heard as
it goes, one engine per build, and the last point is always built. The tabs are narrower
(padding 14 → 8 px) so eight fit the default window.

<a id="step-117"></a>
## 117. Rolled pianos

Rolled pianos. A throwaway spike in six blind listening rounds (neutral names,
loudness matched per register, a real piano hidden among the candidates, named correctly twice)
found where a piano's identity lives: in its body, the radiation responses' coarse color
(1/6 octave) with the per-note correction; the strings and hammers add character (Grand G's
carry it alone); the fine structure is "another microphone", and a random one is not heard as
synthetic; the correction alone moves no identity. The ten hammer pianos' bodies differ along
few directions (60, 16, 8 and 6 %); heard on Grand A, the first is warm and round against cold
and clear, the second airy against dry, the others barely audible. Rolls with blended strings
(three grands, none over 55 %), a body from the knobs and a fine structure mixed from real
responses were heard as none of the ten; "a bit more bass" on the user's favorite was the
warmth knob. The engine's `roll` module makes them from a recipe (a project of family
`rolled`, one step); the editor rolls and adjusts; the plugin reloads an instrument whose
files change. Three faults of the spike are tested against: a body changed without its board's
radiation lost 13 dB at Player (20 dB in the bass), since away from the recording's position
the responses are made from the board's modes (now moved with the responses, then calibrated
per octave to the references' Player against Recording); a fine structure mixed per
microphone put the bass out of phase between the two (the frame's own response now kept below
100–200 Hz); noise as the fine structure smeared each band's onset into a long ring. The
realism check was calibrated on the user's verdicts: spectra from 0.05 s and of the mono sum,
as the spike measured, the references' range widened by a quarter of its span, so the roll the
user approved (3 dB brighter in the treble than the brightest reference) passes and the two
they heard as wrong (dull bass notes, a ring 10 dB too long) fail. A roll takes about 12.5 s per
attempt (the Player calibration's renders 6 s, widening 2.3 s, the check 1.5 s), the references
16 s once per process; the design's 5 s needs the board's radiation fitted instead of rendered.
The review found the plugin's reload hollow: a candidate keeps its name and profile, and the
new engine adopted the old one's radiation responses (the new body unheard until the
microphones moved); instruments now carry a fingerprint of their files (a morph: of its
sides' at their weights) and an engine adopts another only for the same one, else the old one
fades out. A roll also goes stale when an export it reads changes (`uses.*:export`), the
preview is written by the newest roll only, and a leftover preview no longer counts as a
committed instrument in the plugin's tests.

<a id="step-118"></a>
## 118. Rolled pianos, faster and chosen by hand

Rolled pianos, faster and chosen by hand (the user: "the UX is terrible, and the speed is also an
issue"). A roll took 12.5 s an attempt, up to 75 s pulled back. The Player calibration ran all
6 passes every time: the engine sets the radiation's level to the recording's (each zone's
responses at the fitted responses' energy), so the uniform part of Player against Recording
(2.5 dB on the test roll) never moved; it now fixes the shape (the mean removed) and stops when
a pass gains under 0.15 dB, since where Player is not the board's radiation alone (the treble
above 4 kHz) a pass moves a tenth of a dB (six passes took a ±10 dB board change's 2.4 dB to
1.7). Recording is rendered once (the board's radiation does not touch it; the same bits), the
realism check reuses the last Player render, widening runs the responses in parallel (2.3 →
0.9 s, the same bits), and a render builds its 15 notes' engines in their threads from a copy
of the filled radiation cache (one after another their convolvers took 0.6 s: Player 0.95 →
0.58 s, Recording 0.33 → 0.18 s, the same numbers). The references are measured per piano,
once per program and export, and kept on disk; the blended strings and fine structure are kept
for the next roll of the same donors. The test roll: 38 s (pulled back once) → 7.1 s, 19 s
when the program first measures the references. The sliders are built as set, the realism
check named in the page ("a guide: your ear decides"); a draw (Roll again) is pulled back, its
knobs to 0.01 and the sliders set to them (before, the page showed other numbers than its
sliders), and a pull-back that mends less than a quarter of what is off (each statistic's and
octave's excess as a share of the range, each fault one) ends it, the best try kept: a draw
was pulled back 4× for a fault and the treble at Player that the knobs barely move. The fault
type does not tell: warmth 3.0 puts Player 7 dB off at 177 Hz and the bass out of phase, which
pulling back mends. The test roll's knobs are now what the first generator built (it pulled
the user's favorite back once: the treble 0.4 dB brighter than the references' range). The
editor's page: a table of the grands (a share of the strings and hammers each, the others
keeping their proportions; the fine structure's boxes), a change stops the running roll at its
next stage and rolls once it settles (400 ms, no mouse button held), where each change had
queued a whole roll behind the running one. From the review: donors must be grands at the
frame's sample rate, the release noise's ratios blend with the hammer speeds, a correction of
another layout is refused, `level.json` is written before `instrument.json`, Save keeps a
project file's other lines, a missing instrument is not watched, the roll's code holds no
instrument's name. The exports' test for a fundamental in a notch of the radiation (Grand C's
D♯7 once 31 dB under the response around it) failed on a leftover preview (key 83, −27.8 dB):
summed responses put some key's fundamental below −15 dB for 117 of 140 donor sets, and the
body's decay and the widening dig new ones (the test roll: key 86, −25.7 dB). Weighing the
mix's donors (signs and gains, the best of 32 draws) mended 112 of the 117 but moved the bands'
color and the bass's phase; a roll now lifts each fundamental deeper than −12 dB with a narrow
peak (Q 100) after the widening (the bands within 1 dB, the energy kept), and the realism
check names one left as a fault. The preview is skipped by that test, as by the plugin's.
The first rolled project the user finished took minutes to build and to show its status
(`build.py status` 55 s for it, 0.14 s for one of its references): the status read every
borrowed project's anew per role, per read and recursively (799 status reads, 10 231
fingerprints), and a stale build read the whole status before each step of each borrowed
project (13 s with nothing to build). Each borrowed project's status is now read once per call
and a build reads it again only after a step ran: the status 0.55 s, a finished roll's build
10 s. "Rolled pianos" joins the menu's known groups after the uprights, and the plugin's tests
take the instruments git tracks (a rolled piano the user has not committed is not theirs).
Finish now writes the preview's export as the project's when the preview plays the recipe (its
recipe as built, a pulled-back draw's knobs included, kept beside it; the name and the menu
entry may differ): the instrument is in the plugin's menu at once, and the build that follows
writes the same files (a test compares them byte for byte). A name with an export is taken.

<a id="step-119"></a>
## 119. The dampers measured from release recordings

A blind listening round on nine stimuli (loudness-matched, onsets aligned, neutral names) heard
our Grand E against Grand E's own samples (played by sfizz). What the user heard on ours: notes
ringing on after their release (staccato, repetition, broken chords running into the next), the
release noise too loud, the treble short, uneven velocity steps, every repeated strike the same.
The first, measured on a staccato run: 0.1–0.2 s after a key's release ours rang on from G2 to C4,
about half of it with our room off.

The damper loss came from Salamander's "harm" samples: a release-triggered resonance layer of its
SFZ (they peak 75–250 ms into the file), not damped notes, read as the energy-decay slope over
20 dB, which the room after the damper dominates: 30–60 dB/s. Two grands' own release recordings
(VCSL, Grand C and Grand D: the release regions of their SFZ files) fall 22–40 dB in the
fundamental's band within 0.2 s; their renders fell 4–7 dB. `damper_decay.py` now reads each
partial's first fall, the slope of its envelope over its first 12 dB from its peak (windows of two
periods, 2.5 ms hops; only partials 22 dB above their floor, so the fall stays 10 dB clear of the
room under it; a synthetic release of 100 dB/s under a room 25 dB down reads within 15 %), from a
source's `dampers` (the release regions of an SFZ). On Grand D's release recordings (close
microphones, 99 files): 90–170 dB/s for the fundamentals from A1 to A5. Rendered with it and no
room, Grand D falls −5/−12/−15 dB in the fundamental's band at 50/100/200 ms after the release
(its recordings −8/−13/−22; with the old loss −1/−3/−7); with the room on −1/−7/−15. Grand D lends
it to the grands, the harpsichords, the clavichord and the plucked strings (their sympathetic
strings); Upright A's own release recordings fall only 20–50 dB/s (its renders and recordings both
−5 dB at 200 ms, before and after), and it lends that to the uprights. A uniform scale of the old
loss was tried first: no scale matches the recordings' first 50 ms, the room and the release noise
hold the level up after it.

Every borrower refitted from the soundboard on: the fits moved little (the transfer within 1.5 dB;
the pedal ring's shape, fitted on Salamander, spread 6.5 → 9; the clavichord's coupling 15–30 %,
its takes end in the listing cloth's damping, and kf3, which acts on no key of a double-strung
instrument, ran to its bound). On the staccato run, the level 0.25 s after the onset (the key up at
0.12 s): Grand E C4 −8 → −16 dB, A4 −6 → −12 dB; Grand A C4 −2 → −7 dB. What is left of the
ringing on is the room, 12 dB above Grand D's release recordings' after the strings are stopped,
and the release noise (Salamander's, at its sampler's mix): the next step.

<a id="step-120"></a>
## 120. The release noise 12 dB lower, and the room measured in the gaps

With the dampers measured (step 119), the key-up thump stood out: Salamander's release noise,
which every instrument borrows, at its sample author's mix (the release group at −37 dB, by ear);
the user heard it as too much on C2 and on a chord. Rendered against Grand D's release recordings
(the fall at 50/100/200 ms in the fundamental's band, broadband and above the fourth partial), the
release noise at 0, −6, −12 and −18 dB fitted best at −12 dB. Salamander's project now carries
`[release_noise] level_db = -12.0`, which `release_noise.py` adds to every ratio; only that step and
the exports read it, so the instruments were exported again, not refitted. Grand D's renders now fall
−1/−7/−16 dB in the fundamental's band and −3/−7/−14 dB broadband (its recordings −8/−13/−22 and
−7/−11/−16), 4.5 dB rms over the three bands (5.1 before, 9.3 before step 119).

The room was the other half of what held the level up after a release: with it off, Grand D falls
as its recordings do. A flat cut was measured first and not taken. Over a held note's gaps between
the partials, re the partials' own level, the engine's attack (0–50 ms) and room (0.15–0.6 s) lie
within a few dB of the recordings overall (Grand A −1/+1, Grand C −3/−1, Grand D −5/−6, Grand E
−6/−6 dB), but tilted: +4 … +12 dB at 1.6–4 kHz, −8 … −25 dB below 600 Hz. A room 12 dB lower would
deepen the bass's shortfall. Both the room's spectrum (it follows the fitted transfer H) and the
attack noise's fit are next, measured this way against every piano's own recordings.

A two-way borrowing (Grand B lends Grand D its levels, Grand D lends Grand B its dampers) ran both
chains again on the export rebuild with unchanged inputs: Grand B's fits came out identical, Grand
D's moved 0.02 dB at most (the felt fit seeds from the coupling's last run).

<a id="step-121"></a>
## 121. The noises from a player's distance

The pedal noise stayed distinct in the A/B files after step 120 ("clicky", the user had said of the
Schubert excerpt). Its level was not it: against Upright A's pedal noises (VCSL, recorded at the
player's position at their real level beside its notes: the thump down 15 dB, up 6.5 dB under the
median mf note's peak), ours peaked lower (Upright A −20/−26, Grand E −26/−33 dB). Its spectrum was:
re the mf notes, −14 dB at 125 Hz and +6 dB at 8 kHz against the recording (pedal up −23 and +4).
Our thump reproduces Salamander's own pedal recordings (their spectral shapes within a few dB);
those were made 12 cm over the strings, where the mechanism clicks, and every piano borrows them,
most recorded from a player's distance or further. Per 1/3 octave, Upright A's pedal-down thumps
against Salamander's, both normalized below 500 Hz: −4 … −7 dB at 0.5–1.3 kHz, −8 dB at 2–2.5 kHz,
−13 dB at 3–8 kHz (below 400 Hz Upright A's hum, above 10 kHz its noise). Salamander's project
carries it, smoothed, as `[noises] perspective_db` (0 dB at 400 Hz … −13 dB from 6.4 kHz), and
`release_noise.py` and `pedal_noise.py` tilt every thump's band spectrum by it (the release noise is
the same material from the same microphones). After it, re the mf notes against Upright A's
recordings, pedal down −15 … −7 dB over 125 Hz–8 kHz with no slope left (before −14 … +6), pedal up
−23 … +1 (before −23 … +9): the click gone, the level left a few dB under the real thump's (heard
as too present before). The instruments were exported again; the pedal fits, which read the same
cache, came out unchanged.

The pedal ring's test (step 51) judged the ring's onset above 2 kHz re the thump's part there, which
the tilt lowered 13 dB: it now judges it re the thump's whole peak. Measured on the way: the kick the
dampers' refit widened (Salamander's pedal fit, every piano's shape: center 38 → 36, spread 6.5 →
9) put Grand E's ring onset above 2 kHz 24 dB higher, at 85 dB under an mf note: inaudible.

The driver had marked a borrower stale whenever any step of its lender that writes a fit or a cache
was stale: with Grand D's pedal fit stale (it borrows Salamander's pedal noise), every grand that
borrows Grand D's dampers looked stale from its soundboard on, and step 120's export rebuild ran
Grand B's and Grand D's chains again. A borrower is now stale only for the lender's steps that write
what it reads (its `uses.<role>:<key>` reads; `uses.<role>:*` reads all of them).

<a id="step-122"></a>
## 122. The pedal's release as a landing

After step 121 the user found the pedal's release still unlike a piano's: their spectrogram of the
Schubert excerpt showed five separate broadband clicks about 60 ms apart. They are Salamander's
own: its pedal-up takes are a rattle of near-equal impacts (take 2: five within 1.4 dB, 50–70 ms
apart; take 1 within 6.7 dB), its mechanism heard from 12 cm. Upright A's pedal-up takes, recorded
at the player's position, are one landing and its bounces, 8–18 dB under it. Salamander's project
now carries two of them (`[noises] pedal_up_impacts`, as (time, level, decay), the landing at
40 ms), which `pedal_noise.py` puts in place of its own takes' impacts; the spectrum stays the
grand's (tilted as step 121). The pedal down's impacts are alike on both (several thumps within a
few dB: pedal, trapwork, damper rail) and stay. Grand E's pedal-up envelope (room and sympathetic
strings off), every 15 ms re its peak: before −12, −14, −11, −10, −2 at 30–165 ms; now −1 at
45 ms, then −12 … −23. The instruments were exported again, the pedal fits unchanged.

<a id="step-123"></a>
## 123. The room cut where it overfilled

The listening round's next item (steps 119–120): the room against the recordings in the gaps between a held
note's partials. Measured by the median of the gap bins (a string ringing there does not count),
per octave band re the partials, notes from 400 Hz up at the loudest dynamic, at 0–0.48 s: the
recordings' gap sound falls fast for ~0.15 s, then slowly. The engine's is the room's response to
the strings' onset, one decay measured over its first 15 dB: against the recordings 8–15 dB short at
the strike in the bass, 6–17 dB loud at 2–4 kHz around 0.1–0.3 s (Grand A, Grand D), 10–40 dB short
after 0.5 s. The room was the only source: without it the gaps fell 30–79 dB under the recordings.

A `room` step (proto/fit_room.py) between coupling and register now cuts the room per octave band
where the strings' onset drives it past the recordings: three renders (as is, without the room, then
refined over three) bring the room's part to what the recording leaves over the strings and board
alone, at most 30 dB; where the gaps without the room are over the recording already, the room is not
the cause and loses at most 6 dB; a band whose recorded course decays slower than 15 dB/s is the
recording's noise floor and sets no target. The cut (`eq_db`) sits on top of the room's unit energy:
a cut on the envelope before the normalization only moved the room's energy into the other bands (up
to 10 dB more room in the bass). The partials go through the room too: the per-note correction gets
back what the cut takes from them (`comp_db`, the full filter's smoothed spectrum without the cuts
re with them, −3.6 … +5.4 dB), so the cuts change the gaps only. The cuts (dB at 250 Hz … 8 kHz):

| | 250 | 500 | 1k | 2k | 4k | 8k |
|---|---|---|---|---|---|---|
| Grand A | | | | −7 | −12 | −11 |
| Grand C | | | | −11 | −17 | −11 |
| Grand D | | | | −9 | −20 | |
| Grand E | | | | | −3 | |
| Grand F | | | | −3 | −13 | |
| Grand G | −6 | | | | | |
| Upright B | | | −10 | −16 | −25 | |
| Upright C | | | −15 | −19 | −27 | −15 |

(Grand B and Upright A: none.) The user heard the room better on Grand A, C, D, E and Upright C.

At the other microphone positions the engine matches the board's level to the recording's total,
body and room: with the room cut that total dropped and the board was raised up to 7 dB (Upright C's
Player 7–12 dB over its Recording). The match now takes the room's energy before its cuts
(`RadiationParts::room_kept`): the board's Player against its Recording stays (a test).

The knock is fitted after the room, on the attack's first 15 ms: where the cuts took the room's
treble out of it, the knock came out louder (Grand D 5.7×, 4 ms; Grand C 11×, cutoff 1 kHz, which
helped it). The user found the knocks perhaps a bit loud; the Action knock control sets them.

What was tried and dropped, on the way to the bass's shortfall at the attack (Grand A 9 dB, Grand E
29 dB short at 250 Hz, still open):

- the room louder re the body: the partials pass through it too, the gaps' ratio to them saturates
  (Grand D: the gain at its limit, 20×, the renders still 2–9 dB short);
- an early part in the room (its own decay and level per band) with its own level: the gaps matched
  within 2 dB, but the room no longer like H colored the partials at every microphone position but
  Recording (compensations ±5–12 dB, Player against Recording off by 5 dB);
- the room with the recordings' slower late decay: far too much room at its energy (12–15 dB over
  the recordings with every band cut 12 dB);
- a strike, one sound at every note fitted to the gaps: diffuse noise with its own decays and levels
  (a "tock" around 950 Hz from a +50 → −60 dB cliff between two octaves; with a 16–26 ms rise and a
  smooth spectrum still a click), then a half-sine force pulse into the board (1–2 ms): the same
  sound on every key, as the first knock was ("the same ping-pong ball on every key"), since every
  key shares the board's response at Recording; and its first 15 ms threw the knock fit (Grand E's
  knock 90× louder, heard as "two stones").

What the recordings hold there depends on the note: the shortfall needs a source per key (roadmap).

Tests: the roll's fixture pulled back three more times (warmth −0.09, air −0.35: its treble 1.9 dB
over the references, which the cuts made darker), the realism check's dulled bass −30 dB (−20 was
inside the wider range), the roll's calibration up to 5 passes (Grand C's Player 16 dB over its
Recording at 2 kHz in the treble widens the references' spread), a morph halfway within 2.5 dB of its
ends at Player (it averages the sides' cuts and compensations apart), Upright C's Compare ceiling 4
(three early decays).

<a id="step-124"></a>
## 124. One level curve over the velocity

The listening round's next item (steps 119–120): the velocity steps. The pianos played the recordings' pp, mf
and ff at velocities 14, 60 and 124, ln speed straight between them, so each set's own layers set
the steps. Peak level re velocity 127, the mean of C3, C4, C5: Grand E and Grand C rose +7.6 / +7.2
dB from velocity 32 to 64 and +14.7 / +14.4 dB from 64 to 112, and from 16
to 127 the pianos spanned 7.6 dB (Osiris) to 31 dB (Grand C).

The target comes from a recording: Salamander's 16 layers played through their own set (its velocity
map, amp_veltrack 73) fit a level linear in √velocity within 1.2 dB, 45 dB from velocity 1 to 127
(`velocity::dynamics_db`). Headroom's 5 layers fit the same shape at 54 dB. A
piano now strikes each key at the speed at which that key's note sounds the curve re its own ff,
its ff staying at velocity 124 at the recorded speed and level. The speed comes from a table of the
key's level over its hammer speed, as the engine plays it (`Instrument::level_curve`: every key at 8
speeds an octave apart, from 1/64 of the ff speed to twice it, the peak over 0.2 s at Recording,
the knock off), stored in level.json by the `level` tool, about 4 s a piano; below the table the
level keeps falling at the end slope (8–10 dB per ln unit). The timbre stays the hammer's: a louder
strike is a faster, brighter one. A morph blends its sides' tables (within 1.3 dB of the curve
halfway between Grand E and Grand A), a roll measures its own. The Dynamics control (Touch tab,
10–70 dB, `--param dynamics=`) sets the range; the other instruments keep their anchors and do not
show it.

On the way:

- one table for the whole keyboard, at C3–C5, left the treble off: Grand C's C6 6–10 dB under the
  curve below velocity 64, its upper middle's level falling fastest with the speed. The recordings
  do not do that (Salamander's and Headroom's C6 layers span their middle's range within 2–4 dB);
- a row every fourth key missed it too: a key's curve scatters by 1–1.7 dB from its neighbors'
  (Grand C at 1.25 m/s: C4 −18 dB between −22 and −21, C6 −28 between −24 and −22), so the table
  holds every key;
- with the knock in the measured peak, the uprights' treble levels flattened at soft strikes (Knight's
  C7 at −32 dB re ff on the knock, its string at −68 dB): the curve drove those notes down to the
  table's floor, the knock alone left. The table measures the note; the knock grows with the speed
  as fitted, so a soft note on Knight still peaks up to 12 dB over the curve on its knock.

Peak level re velocity 127 (dB, the range over C2, C3, C4, C5, C6), before → now:

| velocity | 16 | 32 | 64 | 96 | 112 |
|---|---|---|---|---|---|
| the curve | −31.9 | −24.6 | −14.3 | −6.4 | −3.0 |
| Grand E | −34.3 … −28.2 → −31.9 … −31.7 | −30.0 … −24.8 → −24.6 … −24.5 | −21.1 … −17.7 → −14.5 … −14.3 | −11.0 … −8.3 → −6.5 … −6.3 | −5.5 … −3.9 → −3.1 … −2.9 |
| Grand C | −42.9 … −28.5 → −31.9 … −31.3 | −38.3 … −25.6 → −24.7 … −24.4 | −28.4 … −19.3 → −14.3 … −14.1 | −12.4 … −9.4 → −6.7 … −6.4 | −6.3 … −4.6 → −3.4 … −2.9 |
| Grand A | −27.8 … −22.9 → −31.9 … −31.1 | −22.3 … −18.9 → −24.6 … −23.9 | −12.5 … −10.9 → −14.4 … −14.0 | −6.1 … −5.5 → −6.6 … −6.4 | −3.0 … −2.6 → −3.1 … −3.0 |

The music (Schubert, Debussy) plays Grand E 3 dB louder, Grand C 4–6 dB (its treble at mf), Grand A
2–3 dB softer, Upright B 3–5 dB softer. The user heard the curve better than the anchors, most on
Grand E.

<a id="step-125"></a>
## 125. The knock under the recordings

After step 124 the user played with the Action knock at about −16 dB: above A4 it stood out, most
at soft strikes. The knock (step 3's action noise, fitted by `fit_knock.py`) matched the spectrum of
each note's first 15 ms re its strongest band, rms over all bands and dynamics. In the treble the
recordings hold 6–11 dB more than the strings at 250 Hz – 1 kHz, below the fundamentals (Salamander
at pp, notes from A4): the board's thump at the strike, which our strings do not drive. The knock, a
noise burst flat to its 1–1.5 kHz cutoff, filled those bands and overshot the recordings by up to
10 dB at 1.6 kHz, where they fall away: a click. It improved the fit's error by 0.4–2 dB of ~9, its
level spanned 39 dB across the pianos, and two had a q of 0.5 (the knock falling slower than the
note at soft strikes).

The 15 ms spectrum cannot tell a click from a thump: a burst decaying in 4 ms and one over 15 ms
hold the same energy. The attack analysis now also measures the attack's peak at 200–700 Hz re the
note's (`attack_noise.low_peak_db`, version 2): from C6 up no partial lies there, the transient
alone. In Salamander's recordings it falls from −7 dB at its softest layer to −16 dB at its loudest
(C6–E7); Grand E's −7 to −15 dB. The knock was 2–6 dB over that.

The fit on the hammer pianos (version 2): the spectrum sets the knock's shape (decay, cutoff from
500 Hz, the knee), an overshoot over a recording's band counting 3×; the level and growth then
follow the low-band peak, each dynamic's median over C6 and up against the recordings', an
overshoot 3× again and q from 1 (the note's own growth at soft strikes: the knock never grows
relatively louder there, though the recordings' does by 7–10 dB from ff to pp). The plucked and
tangent instruments keep the first criterion: under the new one their fits jumped (the guitar's
knock +63 dB, the lever harp's gone), and no one had heard them against it.

The user heard each round: the first refit (overshoots weighted, the pianos' Salamander −3 dB, Grand
E −9 dB) better than before; then the level by the peak (Grand E back near its old level) worse on
Grand E, better on the others, so overshoots there count 3× too; with every note in the rms one
recording without a transient pulled a knock to nothing (Headroom's off, Iowa 10–15 dB under), so it
compares the medians per dynamic. The last version better on Salamander and Grand E than every one
before. Low-band peak re the note (dB, C6–E7; the recordings soft / medium / loud, ours velocity
20 / 64 / 115):

| | recordings | before | now |
|---|---|---|---|
| Grand A | −9.9 / −15.9 / −19.2 | −6.1 / −7.6 / −10.3 | −13.5 / −16.1 / −18.5 |
| Grand E | −7.2 / −12.9 / −14.4 | −7.4 / −10.2 / −11.1 | −11.0 / −11.4 / −12.4 |
| Grand C | −10.4 / −13.6 / −18.3 | −8.0 / −11.9 / −13.3 | −16.6 / −19.6 / −19.7 |
| Headroom | −16.6 / −19.5 / −20.9 | −10.1 / −13.2 / −13.9 | −13.1 / −18.6 / −21.4 |
| Upright A | −7.6 / −14.3 / −14.6 | −7.1 / −7.4 / −7.9 | −10.1 / −12.6 / −14.6 |

On the way: an added register (Salamander twice, the registers' fixture) lacked the room's
compensation since step 123: it plays through the main register's body and room, whose cuts take from
its partials too, but its own correction (`register_correction`) left the compensation out; its first
re-export since showed its registers 2.3 dB apart. It gets the main register's now.

The rest stays: in the treble the strings alone fall 6–25 dB short at 250–600 Hz at the strike
(Grand E the most), a thump a noise burst cannot give without a click. It is the source per note
step 123 found missing in the bass: the hammer's contact force into the board (roadmap).

<a id="step-126"></a>
## 126. The hammer's thump through the board

The knock of step 125, under the recordings now, still was a noise burst beside the note. Two
sources were tried for the treble's thump below the fundamentals (200–700 Hz at the strike, 10–20 dB
over our strings in the recordings):

- the string's static share: below its fundamental a string passes the strike position's share β of
  the felt force to the bridge. A full modal sum does (Σ 2(−1)ᵏ sin(kπβ)/(kπ) = −β), and our
  truncated modes with physical weights do within 0.5 dB; the fitted per-partial output gains throw
  it −19 … +15 dB off (Grand E's treble −4 … −19 dB, Salamander's +6 … +11 dB). Corrected to β (a
  static correction during contact), Grand E's treble gained up to 6 dB, still 10–20 dB under the
  recordings: the thump does not come through the string. Not kept;
- the hammer's impact on the board: each strike's own felt force (5 ms and dark in the bass, 2.4 ms
  and bright at C7, shorter at ff: a bandwidth 8× apart across the keyboard), × a gain, on the bridge
  with the strings' force, through the board and the room (unlike step 123's half-sine, one shape for
  every key). Heard "part of the note".

Through Salamander's board the felt force landed on the recordings' treble peak at a plausible gain
(1.4); through Grand E's, bright at the attack, only at 22.6 and 6–10 dB over at 1.25–2 kHz. So the
impact gets a lowpass, fitted like the knock's shape (the attack frame from A4 up, an overshoot 3×:
Grand E 500 Hz, which the user chose over 1 and 1.5 kHz by ear; Salamander open), and its level by
the treble's 200–700 Hz peak, then the user's choice: 6 dB under and softer below ff,
× (speed / ff speed)^0.5 (Salamander's thump −6 dB at ff, −11 dB at mf, −17 dB at pp re the fit).
The fit (`fit_knock.py`, version 3) writes `impact` for the hammer pianos instead of a knock; the
plucked and tangent instruments keep theirs. The engine sends each strike's felt force through the
lowpass (`voice::ImpactFilter`, the prototype's `butter(2, …)`) onto the voice's bridge force; the
Action knock control and the action's wear set it from the next note, and a morph blends it.

| | gain (× felt force) | lowpass |
|---|---|---|
| Grand A | 0.71 | 3 kHz |
| Grand B | 0.25 | open |
| Grand C | 0.25 | open |
| Grand E | 0.50 | 500 Hz |
| Iowa | 0.35 | 1.5 kHz |
| Osiris | 0.35 | 2 kHz |
| Headroom | 0.022 | open |
| Upright A | 0.25 | open |
| Upright B | 0.35 | open |
| Upright C | 5.7 | 300 Hz |

In the engine the thump sits 6–12 dB under the recordings' treble peak (by the user's choice), the
knock had 0–4 dB. The engine's and the prototype's thump agree within 1 dB on Grand E; its strings
peak up to 3 dB under the prototype's at C6 (older), so there its thump stands out a little more.
The user heard the thump better than the knock on Grand A, C and E, the bass included.

<a id="step-127"></a>
## 127. The aftersound's level along the keyboard

The treble was heard too short (C6, step 119's listening round). The C6 fundamental falls
18–20 dB/s over its first 2 s in Grand A's recording, 12–16 in Grand E's; ours 14 (Grand A) and 22
(Grand E). Grand A's C6 already followed its own (the decay curve 20 dB down at 0.74 s in both). The engine reproduces
the coupling fit's model (each partial's decay curve within ~0.05 s), so the shortfall is the fit's.

What it falls short in is the aftersound. With one horizontal level for every key (the slow
polarization that carries the aftersound), the recordings' partials 1–3 lay over the model 1–2.5 s
after the strike (re their peak over the first 0.3 s, median over an octave's notes) by 2–8 dB in
C5–B5 and 5–15 dB in C6–B6, on all six pianos measured; in C2–B2 they lay under it by 1–10 dB on
five (there the aftersound is the unison's strings against each other, not the horizontal
polarization). Three ways to lengthen Grand E's treble, by ear: a stronger aftersound (the
horizontal level ×2.5 from C4 up, "quite good"), less string loss, half the bridge coupling. The
user chose to stay with the recordings; a Sustain control for a longer ring than recorded comes
separately.

The coupling fit gets a second stage on the pianos: the horizontal level × a factor at C2, C4 and
C6 (log-interpolated, held beyond), the other parameters held, against the mean EDC error plus the
late gap (|the median late difference| per octave, C1–B6, averaged; 1 dB of each weighs the
same). Tried on the way:

- the horizontal excitation falling over the partials (n^−γ): the fit took γ 0.49 for 0.06 dB, the
  fundamentals 2–3 dB up; partial 3 was as short as the fundamental;
- the harp's early-excess term: the bass jumped (β1 at A0 ×0.2, the bichords' coupling ×3), the
  error 7.5 → 11.1 dB; the beating term off: the fit stayed where it was;
- the late level re the mean of the first 0.3 s, the loudest take alone: ±10 dB per note, Grand A's
  C5 went down; re the peak, every take (pp, mf, ff), the medians agree with the ear;
- a factor per octave: up and down between neighbours (Grand E ×0.3 at C2, ×5.0 at C3); one at C7,
  the fit's only note in its octave: to the bound (its tails in their noise, the undamped strings'
  ring). C7 up is left out of the late gap.

| | C2 | C4 | C6 | late gap (dB) | EDC error (dB) |
|---|---|---|---|---|---|
| Grand A | 0.99 | 0.90 | 1.60 | 4.71 → 3.79 | 7.95 → 7.98 |
| Grand B | 0.99 | 1.09 | 5.03 | 3.83 → 1.99 | 6.50 → 6.82 |
| Grand C | 0.34 | 2.85 | 2.51 | 3.87 → 1.67 | 7.40 → 7.94 |
| Grand D | 1.41 | 0.59 | 4.25 | 5.65 → 3.32 | 7.85 → 7.88 |
| Grand E | 0.78 | 5.66 | 1.93 | 5.74 → 2.75 | 7.49 → 8.84 |
| Grand F | 1.75 | 1.58 | 0.81 | 2.97 → 1.30 | 7.00 → 7.55 |
| Grand G | 1.04 | 0.86 | 2.50 | 3.01 → 2.27 | 6.79 → 7.03 |
| Upright A | 1.17 | 0.67 | 2.39 | 3.99 → 2.48 | 5.91 → 6.03 |
| Upright B | 1.55 | 1.16 | 1.96 | 2.67 → 1.91 | 8.76 → 8.95 |
| Upright C | 0.76 | 1.42 | 3.74 | 3.92 → 1.96 | 6.71 → 6.95 |

Recording − model, partials 1–3 over 1–3 s, the median per register: C5–B5 Grand E +6.4 → −0.4 dB,
Grand C +7.9 → +3.1, Grand B +3.9 → +0.6; C6–B6 Grand D +14.5 → +5.8, Upright A +12.8 → +7.2,
Grand B +9.1 → +4.2, Grand E +4.7 → +1.5. C2–B2 stays (−6 … −10 dB on Grands A, B, D, E). By ear
(the factors on the exports): Grand E's fit as good as ×2.5 from C4, Grand A's better than before.

The thump's fit (step 126) runs after the coupling and moved with the louder aftersound, whose
onset adds to its 200–700 Hz band below C6: Upright A's to none, Grand G's halved, Grand E's and
Upright B's +3 dB, Grand A's lowpass open. The thumps stay at their committed values (heard in step
126). The plucked instruments' coupling fit is unchanged (no second stage); their steps were
adopted. `fit_coupling.py` version 2.

<a id="step-128"></a>
## 128. The thump trimmed by ear

Playing every piano again after step 126, the user heard the thump better on almost all of them,
and far too intense on two: Grand A (12 dB too loud on every key) and Upright C (10 dB up to C4,
6 dB from C6 up). Grand A's thump was the one prototyped and approved on single notes; played, the
thumps come at every note, in chords and runs, over the whole velocity range, and add up: the
demos had not shown it.

A project's `[impact] trim_db` holds such a choice: (key, dB) points, linear between and held
beyond. The export writes each note's `impact_db` (left out at 0), the engine multiplies the
strike's thump by it (`Note::impact_gain`), and a morph blends it in dB over the sides with a
thump, as their gains. The fitted level (`fit.impact`) stays as fitted, so a refit of the knock
step keeps the trim on top. Grand A's ff level (level.json, which a morph matches) is 0.85 dB lower,
its peaks at ff partly the thump; the per-key level table, measured without the thump, is unchanged.

<a id="step-129"></a>
## 129. The Sustain control

A treble ringing longer than the recordings was asked for (step 127). Two ways were heard on Grand E,
less string loss and half the bridge coupling, and the user could not tell them apart much. As
controls, each failed a check:

- the internal loss (b1, b3) divided: the pitch stays exactly (all strings shift alike), but the
  bass barely moves (C2 +2.5 dB at 1.5–2.5 s even at 8×): there the coupling sets the decay;
- the coupling's real part divided too (its reactive part, the pitch pull, kept): the whole
  keyboard rings longer, but the modes move, C5 −5.3 cents at 4×. The unison's strings lock by
  the real coupling; weaker, they part, and the bridge's pull lands on them differently.

The Sustain (`Tuning::sustain`, 50 … 400 % in the plugin, 0.25 … 8 in the engine) divides each
compiled mode's decay rate instead, after the unison is solved: every mode keeps its frequency
and its share of the strike and rings that many times as long, the strings' coupled modes, a
bar's modes and the sympathetic resonators alike; the dampers and the contact keep theirs. The
sounding pitch over a note's first seconds still follows its in-phase mode for longer (Grand E's
C5 holds 523.3 Hz rather than gliding to 524.9 Hz within half a second), as a longer prompt sound
would.

| level at 1.5–2.5 s re 100 %, velocity 80 | C2 | C4 | C6 | C7 |
|---|---|---|---|---|
| Grand E, 50 % | −8.0 dB | −6.9 | −21.3 | −32.0 |
| Grand E, 200 % | +4.9 | +8.1 | +11.5 | +22.2 |
| Grand E, 400 % | +8.1 | +15.3 | +21.0 | +39.2 |
| Grand A, 200 % | +4.6 | +11.1 | +11.2 | +12.0 |

The attack's peak stays within 0.6 dB at 200 % (0.9 dB at Grand A's C6 at 400 %). On the Sound
tab, compiled like the Unison width (a rebuild in the background); `midi2wav --tuning sustain=2`.

<a id="step-130"></a>
## 130. Each strike slightly different

In the blind listening round every repeated strike of ours sounded the same ("machine-gun"). Measured,
strike against the previous one over its first 200 ms, the residual after the best gain (dB re the
strike; Grand E, velocity 80):

| | C3–C4 | C5–C6 |
|---|---|---|
| ours, from rest and at 2/s | −126 … −66 (the same) | −127 … −84 |
| Grand E's two recorded takes per key and dynamic (mp–f) | −15 … −18 | −12 … −16 |

The takes differ by 1–2 dB in level (the player) and vary the waveform. By octave band they
repeat their partials around the fundamental within about −20 dB and are unrelated from 2–4 kHz up
(−7 … +2 dB). At 6
and 10 strikes per second ours already differed (−10 … −35 dB), the previous strike's ring under
the next.

The hammer never meets the strings quite the same way twice. Each strike now draws its strike
point (σ 2 mm, the partials' shapes sin(kπβ) of its own), the moment it meets each string of the
unison (σ 40 µs, as a later contact by the distance the hammer travels meanwhile, the first string
at the hammer's arrival) and each string's share of the felt (σ 5 %). The timing does most of it;
2 mm and 5 % alone stay 10–30 dB under the takes. The draws come from a stream per key, so a render
is deterministic, and the bars' mallets and the plucks have none. At 100 %, ours: −27 … −29 dB at
C3–C4 and −21 … −27 dB at C5–C6, the fundamental's band within a few dB of the takes from C5 up,
C3's 1–4 kHz more variable and C3–C4 above 8 kHz steadier (the takes' top holds the hammer's
noise and the recording's).

The user heard 100 % as more realistic and more than they like, wanted it per piece, and strikes
that are not always the same bother them: the Strike variation (Touch tab, 0–400 %,
`Params::variation`, `--param variation=`) is off by default, far beyond 200 % "interesting in some
edge cases". The level tables, the ff level and the roll checks render
without it.

<a id="step-131"></a>
## 131. The phantom partials, part of the piano

The user still heard the recordings' bass attack as "really powerful", most of all at f, and ours as
lacking it, a different sound from the thump. On the way to its cause (design
2026-10-20-bass-high-band, `proto/bass_check.py` the measure) these were ruled out: lengthwise
vibrations as a missing transverse brightness (the engine's partials at f as bright as the
recordings' up to 2 kHz), the damped strings ringing along (≤ 1 dB at 100–400 Hz and ≤ 5 dB between
the partials at 2–8 kHz even at 30× their coupling), the partials split in two (a prototype of the
measured split, heard as no better), and the bridge's mobility per key (design
2026-10-20-bridge-mobility: a fitted factor on the coupling below 500 Hz moved the partials that
matter little and was heard as little; its fit stage stays in `fit_coupling.py`, opted in by no
project). A roll blends the loss exponent (step 132) with the b3 it belongs to.

What did measure: the energy between the bass partials (outside ±6 Hz of each) re the partials, over
Grand A's 16 layers, grows +0.5 … +1.0 dB per dB of the note at 0.5–2 kHz in A0–A1 (quadratic in the
strings' motion: their tension modulation, the phantom partials), and not above 2 kHz. The engine
without phantoms lay 20–40 dB under the recordings there. The phantom term existed as a Wear control
(step 9); it is now also part of the instrument: `[export] phantom` per project, the engine playing
the instrument's level plus the control's (which means wear beyond the instrument's own), a morph
the weighted mean. Grand A by ear of 0, 0.025, 0.05, 0.1, 0.15, 0.2, 0.25 against its recordings:
0.05. Its energy between the partials at 0.5–2 kHz at ff rose 7–10 dB (A0 −32 → −24, A1 −27 → −25,
C2 −26 → −23 dB; the recordings −11, −18, −21); at 0.2 it added 6–7 dB at 2.5–5 kHz.

An instrument's own phantom partials play below C3 (`PHANTOM_OWN_TOP`), where they were measured: a
voice with phantom partials leaves the shared accumulators, and on all 88 keys Grand A's mean load
rose 30–50 % (bench); gated, 8–13 % in the dense passages (pedalled ff at 32 notes/s 14.3 → 15.6 %,
the pedalled glissando 13.5 → 15.2 %). The Wear control's phantom partials still reach every key.
Grand A's level table, measured at C3, C4 and C5, is the one without phantoms. The tests of a linear
relation (a strike's 6 dB doubling, the thump's linearity) leave the phantom term out: it is
quadratic by design.

<a id="step-132"></a>
## 132. The strings' loss at high frequencies

The bass's high partials died early: C2's 4.5–6.5 kHz band at 0.3 / 0.6 / 1.5 s after the strike,
re its start, the recording −6 / −11 / −25 dB, the engine −6 / −13 / −36 (−16 / −22 / −34 with the
room off). The strings' loss b3′ is measured from each partial's total decay, which holds what the
board takes; the model adds its coupling on top, and the coupling fit saw partials 1–12 only.
Halving b3 matched 4.5–6.5 kHz and overshot 3–4.5 kHz: the loss rises less steeply than ω².

The loss law is now b1 + b3·ω_ref²·(ω/ω_ref)^q (ω_ref = 2π·1 kHz; q = 2 is the law as it was), in
`string_model.string_loss` and `physics::string_loss` (one table pins both), `loss_q` per note in the
export where it is not 2. A stage of the coupling fit (`[strings] high_loss`) fits β3 at the β1
anchors and one q to the energy decay of every partial up to the 60th, the coupling in the model.
Grand A: β3 1.05 / 1.58 / 2.42 / 2.07 at C1 / C3 / C5 / C7, q 1.55, the error over partials 1–60
7.94 → 7.09 dB. At 4.5–6.5 kHz (0.3 / 0.6 / 1.5 s, ff):

| | the recording | before | after |
|---|---|---|---|
| A1 | −4 / −10 / −24 | −7 / −14 / −35 | −6 / −10 / −24 |
| C2 | −6 / −11 / −25 | −6 / −13 / −36 | −5 / −9 / −23 |
| F♯2 | −6 / −9 / −20 | −6 / −13 / −31 | −4 / −10 / −21 |

A2 still falls short at 1.5 s (−28 against −18); A0's first values now last too long. The middle and
treble move little, toward the recordings. Heard as slightly closer. The body-change calibration
test's bound below 4 kHz is 3 dB now: Grand A's octave 3 came back to −2.1 dB before, −2.7 after,
against +0.1.

<a id="step-133"></a>
## 133. The correction in bands above the tracked partials

The analysis tracks 60 partials, which end at 2.2 kHz on A0, 3.8 on A1, 4.5 on C2 and 6.2 on F♯2;
the per-key correction had no data above them. In the attack (0.02–0.15 s, re 200–500 Hz, ff) A1
and C2 lay 12–24 dB under the recordings at 4–16 kHz, while F♯2, whose partials reach 6.2 kHz,
matched there. With `[strings] band_correction` the transfer's identification adds band rows: per
note and dynamic the recording's attack in third-octave bands from 1.12 × its highest tracked partial
to 16 kHz, minus the synthesized bridge force's in the same bands; they stay out of H's fit and go
to the correction.

Grand A (246 band rows) moved little where it matters: A1 at 4–6 / 6–8 / 8–11 / 11–16 kHz −58 /
−75 / −86 / −121 → −57 / −71 / −78 / −89 dB (the recording −37 / −51 / −67 / −94); B0 came out
9–11 dB too bright at 2–6 kHz. The fit closed its own residual (A1's band rows +6.5 → +0.2 dB);
the gap lies in the strings' bridge force itself: at 4–6 kHz A1 and C2 ~17 dB short, B0 ~11 dB over,
and the correction, smooth along the keyboard, does not follow that key to key. The hammer felt's
high band was never fitted (the felt fit sees partials 1–20, and only their change between
dynamics): next. Heard as barely audible, kept.

<a id="step-134"></a>
## 134. The felt's core

The recordings' bass attack is already bright at mf and keeps brightening above 2 kHz at f; ours
was dull at both (the user: "at m they also sound a lot duller"). Grand A's attack (0.02–0.15 s,
dB re 200–500 Hz), recording / engine: A1 at mf −14 / −32 at 1–2 kHz and −28 / −52 at 2–4 kHz; at f
−37 / −57 at 4–6 kHz, C2 −39 / −59. Below 1.3 kHz C2's partials 15–20 brighten 30–40 dB from pp to
mf and 4 dB more to f; above 2 kHz the recording brightens 15–27 dB more from mf to f. One power law
brightens its whole tilt steadily with the hammer's speed and cannot do both; the felt fit, which
sees partials 2–20 and their change between dynamics only, had made A1–C2 the softest felts on the
keyboard (3.1–3.7 N at 0.5 mm).

A spike tried two laws at A1 and C2: hysteresis, F = Kδ^p(1 + α·dδ/dt), reached the bands only at
10 dB more error on the partials; a felt with a stiff core under its soft layer,
F = Kδ^p + K_c·max(δ − d_c, 0)^p, improved both (bands 15.6 → 6.7 dB rms, partials 4.3 → 3.0 dB),
and was heard as "much better". The prototype and the engine have the law (`felt_force`, a table
both pin; the contact's sub-steps count the core; a morph or roll blends its stiffness, its depth
weighted by it); without a core every strike computes as before.

A fit per key, scored on the partials plus the attack bands' change between adjacent dynamics, was
built and refit Grand A three times; it taught more than it gave:
- the sets fade their files to digital silence, so a noise read from the last 0.3 s counted pp's
  hiss as signal (−167 against −143 dB at 4–6 kHz); the noise is now the median of the file's middle;
- a felt factor per key smoothed apart from the core broke the pair (A1's 0.35 N felt with a 15 N core
  became 9 N with 24 N, and mf stayed as dull);
- the board's refit with the core in its renders moved its transfer by 4 dB and its per-key gains by
  up to 7 dB, though the core changes the strings' level by 0.3 dB: a tilt of +3–5 dB in the bass and
  −1–4 dB in the treble, heard as the bass too loud;
- the per-key cores scattered (8–28 N between neighbors), and the user heard F♯2–C3 at f too bright.

So the core is set by ear as a curve in the part's project (`[felt_core]`: keys, the force at
0.5 mm past the depth, the depth), read by the export only; the fits never see it. Grand A: 0.2 mm
everywhere, 8 N at A0, 17 at A1, 20 at C2, 13 at F♯2, 10 at A2, 9 at C3, 5 at F♯3, none from C4;
the levels stay main's within +1.6 dB (C2–C3 at soft velocities). The user, on single keys: A1 at f
"very close", C2 at f "almost perfect", mf better but not quite as bright; on Schubert's Ständchen
"the bass sounds right/very plausible". The attack bands' measurement (`felt_bands`) stays for a
later fit of a few parameters.


<a id="step-135"></a>
## 135. Which felts want a core: Grand E's

The grands' bass attack (A0–B2, 0.02–0.15 s, dB re 200–500 Hz), measured as its change from mf to
ff, recording against engine, splits two ways (medians over the keys, A0–B1 / C2–B2):
- the engine brightens more than the recording at 2–4 kHz: Grand G −5 / −4 recorded against
  +29 / +31, Grand D +9 / +8 against +24, Grand B +20 / +15 against +25 / +23, and Grand A
  +16 / +12 against +27 / +18 (before its core +32 / +28: its shallow core acts at mf too and
  narrowed the gap). Matching ff leaves mf dull, and a deep core, which acts more the harder the
  strike, widens the gap. What sounds overdone on Grand A is another thing (found with the traits
  spike, step 136): its C2–B2 now lies over its recording at both mf (+5 to +16 dB at 1–8 kHz) and
  ff (+9 to +20), while A0–B1 at ff matches and mf came from 18 dB dull to about 5. Grand G's recordings are bright at both soft layers and on
  the partials (C2–A2: the share of the 2–4 kHz energy within ±12 Hz of them −0.6 to −2.5 dB,
  noise would give −4 to −7); its borrowed felt rises 33 dB at 2–4 kHz from 3 to 5 m/s. A lower exponent narrows it
  (1.1–1.3 at twice the force at 0.5 mm: both layers within a few dB at 2–4 kHz, 5–10 dB bright
  above 4 kHz): a by-ear felt over the borrowed one, not built yet;
- the recording brightens more at 4–6 kHz: Grand E +37 / +35 against +13 / +16, ff 10–15 dB short
  there, mf right. That is what a core does;
- Grand C's mf bass lies 0–15 dB over its hiss, mostly under 10 above 2 kHz: no judgement.

A core for Grand E acts at ff only when its depth lies past the felt's compression at mf. Sweeping
the depth per key (the deepest that still changes 4–6 kHz) gives that compression: at mf 1.4 mm at
A0, 1.0 at A1, 0.9 at C2, 0.7 at A2, 0.6 at A3; at ff 2.0 down to 1.2 (the rigid-string bound,
δ = ((p + 1)·m·v²/(2K))^(1/(p+1)), says 0.6–1.1 and 1.9–3.3: the strings give way). A uniform
1.4 mm never reached A2. Grand E's `[felt_core]`: the depth 0.2 mm past mf's (1.6 mm at A0, 1.3
at D1, 1.1 at C2, 0.9 at A2, 0.8 at C4), 2 N at 0.5 mm past it to B2, none from C4 (its felt gives
1.9 N at 0.5 mm, 24 at 1.5). At ff, engine − recording, A0–B1 / C2–B2:

| | 1–2 kHz | 2–4 kHz | 4–6 kHz |
|---|---|---|---|
| before | +11 / +8 | −1 / +6 | −15 / −10 |
| 2 N | +12 / +10 | +2 / +9 | −12 / −10 |
| 4 N | +13 / +10 | +6 / +11 | −7 / −3 |

The core cannot lift 4–6 kHz without 2–4 kHz. Single keys and pieces (the Ständchen's bass stays
under velocity 92, below the core) told the variants barely apart; the user heard them on repeated
loud notes from an arpeggiator, switching instruments while it ran, and chose 2 N. The export's
calibration lowers the output 0.29 dB (ff 0.24 dB louder). The user's next wish: the felt's
traits (the core's force and depth, the exponent) as runtime controls, a curve over the keyboard.

<a id="step-136"></a>
## 136. The rail

Steps 134–135 set the felt's core by ear, away from the per-key fits, and left nothing to check a
change against: Grand A's core, right on the day, was heard a day later as "overdone in some
pieces". A throwaway page of graphs showed why at once (its C2–B2 now over its recording at mf and
ff, step 135) and corrected our own reading of it. The plan (design 2026-10-21-traits-plan) keeps
the recordings as the rail, in a new role: the edges of what real grands do, not each piano's exact
answer. Its first phase:
- snapshots: `just snapshot <ref>` writes an instrument at a git ref beside the working copy, in the
  plugin's "Snapshots" group (a search path after the instruments), its name and profile marked
  with the ref; the engine has to load it or nothing is kept;
- `bass_check` reads each recording's noise per attack band (attack-long frames in the file's
  middle): Grand C's mf bass lies at its hiss above 4 kHz (C1 −0.4 dB over it at 4–6 kHz);
- the report (`just report`, a page in `proto/out/report`): the grands, picked by their data, with
  their traits along the keyboard and their bass attack against the envelope of the grands'
  recordings (the change from mf to ff per band, re 200–500 Hz, bands within 10 dB of a recording's
  hiss left out), per grand the keys its engine leaves it, each engine against its own recording,
  a snapshot's lines beside today's; Grand G leaves the envelope (`[report] envelope = false`: its
  two layers are both soft);
- the listening set written down (`docs/building-instruments.md`): Ständchen (soft), Jingle Bell
  Rock (loud bass), loud repeated notes from an arpeggiator while switching instruments, and a
  sound confirmed with fresh ears the day after.

The first report, keys of A0–B2 whose engine leaves the envelope at 1–2 / 2–4 / 4–6 / 6–8 kHz:
Grand A 1 / 3 / 1 / 3 of 9, Grand B 4 / 10 / 6 / 19 of 24, Grand C 0 / 4 / 0 / 4 of 8, Grand D
1 / 5 / 0 / 0 of 13, Grand E 6 / 7 / 16 / 5 of 27, Grand F 3 / 2 / 1 / 2 of 9. Grand B and E's
counts are the next look (B at 6–8 kHz up to 20 dB over the range, E at 4–6 kHz under it).

<a id="step-137"></a>
## 137. Lists that scroll

With step 136's snapshots of `voicing-1` (33 instruments) the instrument menu's "Snapshots" group
ran past the window: the plugin draws its lists inside its own window (DPF has no popup windows),
and below 600 px only the first 20 of the 33 rows could be reached. Every list (the groups, a
group's instruments, Mallet, the ⋯ menu's zoom steps, the MIDI learn and morph menus) is now cut to
the window and scrolls:

- a dropdown goes below its control, above where only that side fits, else on the roomier side cut
  to the view; a submenu is cut to the view's height (`dropdownListRect`, `submenuRect`);
- the wheel scrolls the list under the pointer a row per step; a bar along the right edge (10 px)
  drags (a press beside the thumb centers it there first);
- a list opens with its current entry centered as far as it goes (the chosen instrument in its
  group, the zoom step, the mallet);
- the pure part (the row under a point, the scroll's range, the thumb, the wheel) is in
  `Layout.cpp` with a test each (`a_list_taller_than_the_view_is_cut_to_it_and_scrolls`,
  `a_dropdown_that_fits_nowhere_takes_the_roomier_side_and_scrolls`).

On the way: hovering the group last open when the menu closed opened nothing until the next repaint
(the list's hot row was kept from the last time); it now opens at once.

<a id="step-138"></a>
## 138. Even levels

The traits plan's experiment asked which of a piano's per-key values are physics and which the
fits absorbed. For Grand A the answer was plainer than expected: the felt is already one smooth
curve (only its bass rise is in question, a matter for the felt as a trait); the soundboard fit's
per-key gains (jumps of 6–8 dB between sampled keys) never reach the engine, they take up the
Salamander set's per-key leveling inside the fit; and what the engine plays is uneven anyway. Each
key's partials pass through the board's response and the seeded body, and where they meet a peak
or a notch the key comes out louder or quieter: the sampled keys and the others alike (2.1 and 2.6
dB rms around a smooth trend at mf, the first 0.5 s). The drawn fine structure is right for the
tone; its level jumps are not Grand A's, and a voicer evens them out at the hammers.

So the level tool keeps each key's own ff level beside its rows over the hammer speed
(`level.json` `key_ff_db`, from the renders it made already), and a project's `[even_levels]`
voices the export to one level curve: a trend through the keys' ff levels (a running median over 9
keys, then ¼ ½ ¼, the ends extended on the outer keys' line without its outliers), each key's way to
it added to its level correction at every dynamic, a seeded scatter if asked, measured again. Only
the ff level counts: since step 124 the engine plays a velocity at the speed where the key sounds a
set level under its own ff, so a key's ff level sets its level at every velocity, and a different
correction at pp or mf would only move its speed there, its tone (the review found the first cut
doing that: C4's pp and ff corrections 3.6 dB apart). Grand A's by-ear pp corrections (step 104,
the Compare page) act the same way since step 124: they move the soft keys' speeds, not their level. Variants go
into the plugin's Snapshots group (`just variant`, project tables overridden with `--with`), and
the report draws every key's level.

Grand A, the keys' ff levels' rms off the trend (the level table's peak over 0.2 s): today 1.5 dB
(the largest 4.1 dB); "even" (`window_keys = 9`) 0.2 dB, its corrections −3.8 … +4.1 dB; "even,
scattered" (`scatter_db = 1`, `seed = 1`) 0.85 dB. Without `[even_levels]` an export is the same
(Grand A's instrument.json and data.bin unchanged; level.json gains `key_ff_db`). The user listens
next: `voicing-1`, even, even with scatter.

The user, at once: "even" sounds "a lot cleaner … really mint condition", "even, scattered" like
"another (very plausible) permutation" of Grand A: today's 1.5 dB were a random pattern, not Grand
A's character, and the scatter's size is the condition a runtime control could set. The report's
level graph then showed the shape itself: every other grand falls toward the treble (C8 4–20 dB
under its median), Grand A rises to +6 dB at C7–C8. The Salamander set scaled each key's loudest
take to one peak, so its recordings say nothing about the level along the keyboard; that shape is
the model's. `[even_levels]` takes a target shape (`target_keys`, `target_db`, re the piano's
median) instead of the piano's own trend: the variant "even-register" has the median of the other
six grands' trends (+1.3 dB at A0, 0 in the middle, −3.9 at A6, −13.1 at C8), its corrections
−20.4 … +5.6 dB. For the user's listening round.


<a id="step-139"></a>
## 139. Condition in zones, the grands mint

Step 138's listening settled how to land it (spec `2026-10-21-condition-zones-design.md`). The user
heard the even grands as "a lot cleaner … mint condition" on all seven, and preferred Grand A's
shape halfway between its own and the other grands' ("even-register-half"). A piano that clean is
unusual, so the default keeps a little unevenness, with the clean one a move away. The unevenness
is what a voicer leaves and time adds: it belongs to the condition, not to the export.

- **The grands export mint.** Every grand's project has `[even_levels]`: Grand B–G their own trend
  (`window_keys = 9`), Grand A the halfway shape (`target_keys`, `target_db`, every third key, dB re
  its median: +2.8 at A0, about 0 in the middle, +1.0 at A6, −2.9 at C8). The corrections: Grand A −11.0 … +5.0 dB (rms 3.4 dB, mostly
  the shape: its treble down), Grand B–G rms 1.4–2.0 dB (the largest −5.7 and +7.7 dB). After the
  export each grand's ff levels lie 0.05–0.31 dB rms off its curve (at most 0.79 dB, Grand C).
- **The voicing scatters each key's level.** "Wear: voicing" gave each key a seeded hardness
  offset; it now also offsets each key's strike gain by a seeded draw from its own stream, σ =
  4.5 dB · √wear: 1.0 dB at wear 0.05, 2.0 dB at 0.2, 4.5 dB at 1. The square root lets Normal
  reach its ±1 dB at a small wear (the voicing's hardness offset there about 0.2 dB). At wear 0 the strike is the export's
  bit for bit; another variant is another permutation (Grand A, wear 0.05, seed 1: σ 0.85 dB over
  the 88 keys, B0 drawn +2.74 dB and heard +2.75 dB in its peak over 0.2 s).
- **Condition in zones.** The slider maps onto each aspect's wear through four points, linear
  between, in whole percents: the voicing Mint 0 % → 0, Normal 20 % → 0.05, Worn 50 % → 0.2 (what
  the old slider gave at 20 %), Broken 100 % → 1; every other aspect stays at 0 through Normal, then
  0.2 at 50 % and 1 at 100 %. The aspects' offsets add after it, on the wear scale. The default is
  20 % (Normal), shown "20 % · Normal"; a zone's name typed sets its upper edge. midi2wav's default
  stays mint (wear 0), so the plugin's default command line carries `voicing=0.05`.

The first cut gave every aspect 0.05 at Normal, against the user's description of the zones
(Normal: "loudness variation between keys", the detuning in Worn). The review and a timing found
it: the Ständchen through midi2wav took 20–42 % longer at Normal than at Mint (Grand A, Upright A,
the harpsichord, the marimba), nearly all of it the phantom partials (Upright A: 6.8 s → 10.2 s with
their wear 0.05 alone). Normal is now the voicing alone, at Mint's cost. A dragged value between
whole percents showed one zone and played another (20.3 % shown "20 % · Worn"); the slider takes
whole percents.

The points and σ are starting values; the user tunes them by ear in the plugin.

<a id="step-140"></a>
## 140. What the board fit absorbed; Grand B's felt and unison by ear

The report's board sections (step 136) showed corrections of 29–49 dB, Grand E's response dipping
at 202 Hz and Grand F's and G's response trading shape with their mean correction. The
investigation (scripts, no change to the sound) found:

- **Grand E's dip** is 6–8 dB against its neighbors (the −32.5 dB was H's absolute value), and the
  middle keys' correction adds 10–15 dB back where their low partials lie: board plus correction,
  what they play, has none. Its close pair does dip ~5 dB around 220 Hz against the ambient pair
  (both channels, 27 of 35 keys; the analysis never mixes to mono), but the correction does not
  follow the close pair's coloration key by key (correlation −0.04).
- **The trade** between H and the correction's mean over the keys is in every grand (−0.57 …
  −0.84): the fit's freedom; only their sum plays.
- **The large corrections** are the hammer more than the board: within each key's own partials,
  every grand's correction cuts the upper partials by 3–15 dB per octave of partial number from
  about D♯6 (the model's treble strike too bright on seven recordings), the bass's upper partials
  raised on some; a per-key tilt explains 24–59 % of a correction. The cells below a key's
  fundamental (−36 dB at 63 Hz on A7) never play.
- **The bass above the measured partials** carried the last measured correction on (Grand E, F, G:
  +8 … +19 dB at 2–8 kHz). Grand E refitted with band rows (`band_correction`, as Grand A):
  the first refit put the bass 12–28 dB over the recording at 11–16 kHz at f, its pp and mp bands
  being the recording's noise against a nearly silent model; band rows now count only 10 dB over
  the band's noise (`soundboard.clear_band_levels`). The refit brings A0's 11–16 kHz at f from
  −27.9 to +5.2 dB re the recording; the user heard no difference (those bands lie 35–80 dB under
  200–500 Hz). Its fit is committed, its export left as it was.

Grand B's felt then came by ear. Measured at the velocities the export plays a layer at (the
engine picks each key's speed from its level table; a first comparison at fixed speeds overstated
Grand E's swing from mp to f by 11–22 dB), Grand B opened ~8 dB more from mf to ff at 2–4 kHz than
its recording ("very hard and jagged"). `[felt] p` (the exponent, the force at 0.5 mm kept) at 1.7
matched the swing, but sat darker at ff than every recorded grand (2–4 kHz −35.7 dB re 200–500 Hz
against their −17 … −26; the darker uprights −37 … −41): "more like an upright". `[felt]
stiffness_db` scales the felt as the Hammer hardness control does; the user's +2.5 dB brought the
grand back (+5.5 dB heard as excessive).

The softer felt uncovered a "worn wobble": the sustain's beating, the same in every felt variant
(partials 1–6 within 0.05 dB). Over 0.3–12 s Grand B's engine beat 25–60 % faster than its
recording in partials 1–6 (the 3rd partial 0.95 Hz against 0.60) and partials 4–6 ~1 dB deeper;
not more regular (one beat line holds 66 % of the engine's wobble, 79 % of the recording's). Its
`[strings] detune` was 1.8 cents where the other grands have 0.7–1.0; the user heard the Unison
width at 60 % as right: `[unison] width = 0.6` (1.08 cents). The other grands' beat rates match
their recordings within ±10 %; all wobble 0.5–1.3 dB deeper in partials 4–6 than recorded
(open). Grand B exported with both; the user: "a clear improvement".

<a id="step-141"></a>
## 141. The grands' felts; the room's top octave

Step 140's measure, each grand's attack at 1–2 and 2–4 kHz re 200–500 Hz at the velocities the
export plays its mf and ff layers at, against its recording, found every grand off in its own way:
Grand A, D and F opened too much from mf to ff (D +3.8/+10.2 dB re its recording), C was too bright
at both and opened too little, E too bright at both. Per grand, two probe exports (the exponent
−0.4, the stiffness +3 dB) gave how each `[felt]` value moves the four numbers, a least-squares step
the pair, a third export its check: C 2.16 −5.8 dB, D 1.65 +4.2 dB, E 1.88 −1.3 dB, F 1.99
+5.5 dB, all within about ±3 dB of the recordings where they were up to 9–10 dB off. Grand A did
not solve with its core: split by key, the core (step 134, by ear) put its bass at ff 5–14 dB over
the recording at 1–2 kHz and 10–24 dB at 2–4 kHz, mf right; without it mf sat 8–10 dB dark. The
felt solved without the core, p 1.87 +4.4 dB (within ±3.6 dB). By ear: all better; Grand A's bass
"a little brighter" → the core back at a quarter ("very nice"); Grand E 3 dB softer and its unison
at 75 %; Grand F "extremely bright" → +3.0 dB ("feels right").

Grand D and F rattled with the room on, dry not. Their rooms left 8 kHz uncut (0 dB) beside a cut of
19.9 and 12.6 dB at 4 kHz, their decay held long up there (D 1.16 s above 2.5 kHz): over 0.1–1.5 s
after a strike D's room carried the top octave 4.4 dB over the dry sound (4–8 kHz −15.5). Cutting
8 kHz as 4 kHz cured both by ear. For Grand D a fit bug: a band where the recording shows only its
noise floor sets no target and kept 0 dB; it now holds the cut of its nearest targeted band
(`fit_room.hold_untargeted`); Grand D refitted: 2 kHz −9.6, 4 and 8 kHz −16.4 dB. Grand F's 8 kHz had
a target, its room's level there within ~2 dB of the recording's between the partials: the level
matched, the synthetic tail's grain did not; `[room] trim_db` cuts it by ear (−12.6 dB).

Grand E's band refit (step 140) also ran its knock fit, whose gain moved one grid step (1.0 →
1.414 before the −6 dB): its thump 3 dB louder, heard in every Grand E variant since without a
remark. The suites' pins moved with the grands: Grand A's golden render (+0.3 … +0.4 dB), the
roll's position calibration 6 passes (was ≤ 5), its far draw now warmth 4.5 (at 3 it fails only
Player against Recording, which no pull-back mends), the morph tests on pairs that still differ.

<a id="step-142"></a>
## 142. The felt with pp; the board's tone by ear

Album 1 re-rendered with the new grands (and at the plugin's default condition, Normal) showed what
the listening rounds had missed. On Grand E a soft Schubert passage carried a "string-like", plucked,
harp-like noise. Seven renders of the passage bisected it: the even levels changed nothing (−0.2 …
−0.3 dB per octave), the band refit nothing (0.0), the felt everything: −3.8/−5.2 dB at 1–2 kHz,
+1.8/+4.6 dB at 4–8 kHz against the album's version. Step 141 solved the felts from the mf and ff
layers only; a lower exponent, the force at 0.5 mm kept, makes the felt relatively stiffer for light
strikes, so soft playing came out bright: at pp 10.6 (Grand A), 10.7 (D) and 14 dB (E) over the
recording at 2–4 kHz (keys C2–C5), where B and F, dark at pp before, improved. Solved against pp, mf
and ff (two probes, six numbers): A 2.09 +3.4 dB, D 1.9 +2.2 dB, E 2.32 −5.7 dB (its exponent back
near the fit's 2.3, the felt softer), B 1.88 +6.1 dB (its by-ear pair had left mf 8 dB dark at 1–2 kHz
and the bass and middle 7–9 dB heavy below 1 kHz, heard as "boomy"); every layer within about 4 dB,
pp within 4. The user: "fixed it", "much better".

Grand C was boomy too, its felt fine: at mf (keys C2–C4, re 1 kHz) 63 Hz +4.5, 125 Hz +3.0, 250 Hz
+6.1 dB over the recording. `[board] trim_db` trims the partials per octave at the export. A first cut
moved little: the even levels, measured with the trim, lifted the trimmed bass back to their curve;
they are now measured without it and the trimmed correction swapped in after. The room holds most of
the 63 Hz excess (the partials' trim cannot reach it); 63 −4, 125 and 250 Hz −3 dB brought 125 and
250 Hz to −0.4 and +2.7 dB, heard as better, the 250 Hz step small. Most grands carry about +5 dB at
250 Hz re their recordings (open).

The album then: track 5 stays on Grand E; it sounded "too clean, even at Player": a synthetic concert
hall (2.0 s, mixed at −10 and −6 dB) sounded worse than the instruments' own rooms 4 dB up, which the
album now plays (`room=1.585`).

<a id="step-143"></a>
## 143. The board's tone per register

Step 140 found every grand's model treble too bright and step 142 most grands' low end full; both
were measured again on the exports, against each grand's recording: every recorded key at pp, mf and
ff, played at the export's velocities, octave bands over the first second, each band re the band
of the note's fundamental (a note's levels re its total hide a cut on a treble fundamental: it only
makes the note quieter). At mf (ff), within a treble note (C5 up), the 4 kHz octave sat over the
recording on five grands: A +6.8 (+1.6), B +3.7 (+7.1), C +6.1 (+1.8), D +4.1 (+5.1), E +3.8 (+0.4) dB;
Grand F +0.7 (−1.2), the grand whose brightness the user had called right. Within a bass note (below
B2) Grand A's 250 Hz sat +3.3 (+3.8) dB, Grand B's 125 Hz +3.5 (+3.3); C, D, E at most 2.5 dB. The
middle register's same bands were not over (A's 4 kHz −7.8 dB at mf): a cut in frequency alone takes
the middle's partials and the fundamentals around C4 with it.

A row of `[board] trim_db` now takes two keys, fading it in from 0 at the first to full at the
second, per anchor of the correction (every 6 keys). The treble's rows fade in from C4 to C5 (A 4 and
8 kHz −4, B −5, C −4, D −4.5, E −2.5 dB), the bass's from E3 to E2 (A 250 Hz −3.5, B 125 Hz −3.5 dB).
By ear, as snapshots: the treble better on A, B, C and D; E's treble and both basses not heard, kept
as nearer the recordings. The snapshots cut the top octave's fundamentals too (they lie at 2–4 kHz):
keys 98–108, already 12–29 dB under Grand E's recording, lost 1–3 dB more (two more flags in the
compare corpus); a row with keys now spares each key's own fundamental (0 up to √2·f0, full from
2·f0), the top keys within 0.4 dB of before. The treble at mf A +3.5, B 0.0, C +2.6, D +1.6, E +1.8 dB,
the bass A +1.5, B +1.2 dB, the middle within 1.1 dB of before.

<a id="step-144"></a>
## 144. The report again; Grand C's bass core

The report against `voicing-1` (A0–B2, the bass attack re 200–500 Hz, engine − recording, median):
five grands nearer their recordings at mf and ff (A at ff 10.2 → 6.9 dB, F at mf 7.4 → 5.5), Grand C's
mf nearer (8.4 → 2.9) but its ff twice as far (5.8 → 11.8): at 2–4, 4–6, 6–8 kHz −11.8, −20.0,
−12.8 dB (at `voicing-1` −4.0, −9.2, −7.9). Its `[felt]` (step 141) was solved on C2–C5; the softer
stiffness left the bass's ff short while its mf stayed right. Grand B's and D's growth from mf to ff
at 4–8 kHz lies outside the recorded grands' envelope on 6–19 keys but within 1–3 dB of their own
recordings, which sit at the envelope's edge: left.

A `[felt_core]` for Grand C, probed by export: its depth Grand E's mf compressions scaled by a quarter
for the softer felt (1.9 mm at A0 to 1.2 at B2), past mf's compression (mf's 1–2 kHz unchanged, +6.2 dB
with or without a core). The force against the bass at ff: 2 N −6.5, −16.0, −6.6 dB; 4 N −2.3, −9.9,
−2.5; 6 N −0.8, −4.5, +0.6; 8 N +0.2, −0.9, +3.1. Faded out by C4, 6 N overshot F♯2–G♯3 at ff by 5–13 dB
at 2–4 kHz, where the felt alone was near the recording, and lifted their mf 1.5–3 dB; heard as a
change up to C4. Full to D♭2 and gone by C3: −1.3, −10.2, −1.7 dB, C3 up unchanged. The user heard
the two clearly apart, chose the faithful one and saw the longer one's appeal: the core's reach is a
curve for the felt's runtime controls (the traits plan's phase 3).

<a id="step-145"></a>
## 145. The felt as runtime controls

The traits plan's phase 3 (design 2026-10-22-felt-controls): the felt, voiced by ear through the
projects so far (steps 141–144), becomes the player's. A Felt tab on the hammer pianos holds four
curves over the keyboard, each up to 8 points of bends on the shipped felt: the stiffness (dB re the
force at 0.5 mm), the exponent, the core's force (N at 0.5 mm into it) and its depth (mm), linear
between points and held beyond them. The bends are stored, not the values, so a moved morph point
keeps them over the new blend; a key's shipped detail stays under a bend.

The user ranked the loudness above hearing a change at once: every key keeps its loudness at every
velocity. The build bends each key's felt, measures the level table and each key's ff level again
(`level_rows`) and shifts each key's level by its shipped ff level minus its new one. On Grand C a
far edit (the stiffness +12 dB in the bass, a 6 N core added at C4–C6, the exponent +0.5) moves no
key's level more than 1 dB at velocity 32, 64 or 112 (unleveled: over 2 dB), Hammer hardness +9 and
−12 dB alike, while the bass's 2–4 kHz re 200–500 Hz rises at least 3 dB. A measurement takes about
3.3 s (16 threads), the first on an instrument about 6.6 s with the shipped rows; the worker's cache
keeps the last four edited instruments, so the Original toggle switches back unmeasured. Hammer
hardness on a hammer piano goes through the same build (its live hardness stays 1), the other
instruments keep it live. Every limit at once (stiffness +24 dB, exponent 5, a 100 N core at 0 mm;
then −36 dB and 1.0) plays finite at velocity 127 on every hammer piano.

The plugin sends the `felt` state when a point is let go, added or removed, never during a drag;
the tab shows "measuring…" while the worker builds. The shipped felt the graphs draw is read on a
thread of the UI's own (`FeltFetch`), the newest request winning. Nine tabs keep one row at the
default width with 4 px of padding beside each label.

<a id="step-146"></a>
## 146. What the board's correction carries; Grand G's felt

The roadmap's next step after the felt controls was to move the hammer's share of each grand's
per-key board correction C(f, key) (step 140) into the felt. Measured first (scripts, no change):

- **The patterns predate the felt's by-ear changes.** Grand G, whose felt never changed, has the
  strongest. C was fitted against the fitted felt (exponent 2.3 on every grand) at the fit's hammer
  speeds.
- **Within each note C cuts the upper partials more the higher the key:** about 0–2 dB per octave of
  partial number around C4, 4–10 dB around C6, 10–14 dB in the top keys, on all seven grands. Around
  the middle the felt's stiffness and exponent reproduce 50–80 % of it on Grand F and G at the fit's
  speeds (a stiffness rising too steeply into the treble).
- **The top octave is not the hammer.** At the attack the recordings' partial 2 lies 10–27 dB further
  under the fundamental than the model's (after the board's shared response). This holds on both
  channels (1–3 dB apart) and about equally at mf and ff, and the model brightens from mf to ff as
  the recordings do. The model's upper partials decay no slower, neither from 50 ms on nor in the
  first 150 ms. Softening the felt by 18 dB moves partial 2 only 2–11 dB; a heavier hammer (×1.6),
  a strike point nearer the end or a contact width as long as the strings do not close it. Most
  likely the board radiates less at high frequencies at the bridge's treble end: a fixed filter per
  key, the correction's own job.
- **The board fit and the voicing compare at different speeds.** The board fit renders each layer
  at the felt fit's estimate of its recorded speed (pp 0.4–0.6, mf 1.1–2.1, ff 3.5–10 m/s; Grand G
  2.1, 3 and 5). The felts were voiced at the layers' velocities as the export plays them (steps
  141–142), where the level curve the keys share gives pp 0.1–0.3 and mf 0.6–1.2 m/s (default
  Dynamics), ff its recorded speed. Measured against today's felt (538 takes, every other recorded
  key), at either set of speeds:
  - The residual's part that is the same at every dynamic is hardly felt-shaped: a felt bend fitted
    to it per octave explains 0–20 % on most grands (up to about 40 % on C's and F's middle; the
    takes' scatter of several dB per partial weighs in here).
  - The part that changes with the dynamic is, but it is small: on A–F 10–40 % of it, with bends
    of ±1–5 dB.
  - A refit against today's felt would mostly cut the treble further, by 2–10 dB per octave of
    partial number from about A5 up on A, B, D, F (and G): the direction of step 143's by-ear trims.
    At the played speeds, Grand C's correction would lift its middle by 2–4 dB per octave instead.

So the correction stays a correction: the cleanup refits it against today's felt at the played
speeds, where the felts were voiced, instead of moving it into the felt.

Grand G was the exception: on it the felt explained 60–90 % of the dynamic part (at the played
speeds). Its two layers are
both soft (the soft pedal down, 4.4 dB apart), read by its fit as 3 and 5 m/s, which the engine plays
at velocities 103 and 124. Compared there, its borrowed felt (exponent 2.3) left the soft layer 5 and
10 dB dark at 2–4 and 4–6 kHz (re 200–500 Hz), the louder one 4 and 5 dB bright. Soft playing had
almost no highs: at velocities 14–40, 2–4 kHz sat 34–50 dB under Grand F's (the felt's donor). In
the bass its louder layer is the darker one (−4.3 dB at 2–4 kHz), which no felt follows. Probed with
`midi2wav --felt`, the exponent and the stiffness barely move that growth (−0.4 on the exponent: −1 dB
of it). An exponent of 1.3 at +6 dB came nearest (within 2 dB at 1–4 kHz, the louder layer 9 dB
bright at 4–6 kHz), at +3 dB a little darker (the soft layer 6 dB dark at 2–4 kHz). Soft playing
came back to 3–17 dB (+6 dB) and 12–20 dB (+3 dB) under Grand F's. The user found the recording
itself unnatural to compare against and chose +3 dB as "a more useful default, an acceptable
compromise": Grand G's `[felt]` p 1.3, stiffness +3 dB.

The refit itself, tried as variants (scripts, nothing kept): each grand's C fitted again against
today's felt at the played speeds (Grand G at its 3 and 5 m/s), H, the anchors, the smoothing and the
register gain kept. On the fit's own measure (the partials' levels at the attack) every grand came
nearer its recording: rms 0.4–2.2 dB under today's C, which at those speeds sat over no correction at
all on B–F. On step 143's measure (octave bands over the first second re the fundamental's band, at
the layers' velocities, up to 4 kHz) the change was small and mixed, rms of engine − recording:

| | A | B | C | D | E | F | G |
|---|---|---|---|---|---|---|---|
| today | 4.4 | 3.9 | 8.8 | 6.0 | 5.4 | 7.7 | 5.1 |
| refit | 4.1 | 5.0 | 7.8 | 5.2 | 5.4 | 7.5 | 4.7 |
| refit, step 143's treble trims off | 4.3 | 4.6 | 7.8 | 5.3 | 5.4 | | |

The refit does not replace step 143's trims (without them Grand A's treble goes from 3.1 to 4.7 dB);
on Grand B the two cut the treble twice. The user heard no difference between the refit variants
and today's: the corrections stay as they are. What the measure leaves furthest off lies elsewhere:
Grand C's middle 16–18 dB low at 250 Hz, Grand F's bass 11–13 dB low at 125 Hz, every grand's pp
rows.

<a id="step-147"></a>
## 147. The lowest octave's fundamentals

Step 146's measure left Grand C's middle 16–18 dB low at 250 Hz and Grand F's bass 11–13 dB low at
125 Hz, each re the band of the note's fundamental. Taken apart per key and partial (the channels'
power averaged, not summed to mono):

- **Grand C's 250 Hz is no voicing problem.** Three recorded keys feed it. D3's recording carries
  its partial 2 about 10 dB over its neighbors' (the correction, smooth over 6 keys, follows no
  single key). At the Recording position the engine's right channel dips near 300 Hz (the board's
  response is drawn per channel; left and right 11–12 dB apart there, the recording's 2 dB), and a
  mono sum cancels further.
- **The lowest fundamentals are too strong on three grands.** A board fit sees partials only from
  the frequency its rows reach (`data_range_hz`) and holds the board's response flat below it:
  Grand F from 77 Hz (at −10.7 dB, where the other grands' response falls 15–35 dB by 30–45 Hz),
  Grand G from 64, Grand D from 58. The fundamental re partials 2–4 (first second, the layers'
  velocities), engine − recording, median over the keys:

| | A0–G♯1 | A1–G♯2 |
|---|---|---|
| Grand F | +18.0 dB | +7.3 dB |
| Grand G | +19.0 | +5.2 |
| Grand D | +10.7 | +4.6 |
| Grand A, B, E | −3.3 … +3.4 | |
| Grand C | −17.2 (6 takes) | +2.2 |

  Per key the excess lies at the fundamentals' frequencies: Grand F's C1–C2 (33–65 Hz) 14–25 dB,
  Grand G's 29–49 Hz 17–36 dB, Grand D's 29–37 Hz 10–21 dB.

`[board] trim_db` rows without keys (a frequency's cut on every partial, as the board's missing
fall below the data would give), solved by least squares over the keys within the rows' ±12 dB:
Grand F 31.5 Hz −12, 45 Hz −12, 63 Hz −8 dB; Grand G 31.5 and 45 Hz −12 dB; Grand D 31.5 Hz −12,
45 Hz −2 dB. The two octaves then sit at −2.1/−1.0 dB (F), +3.1/+0.3 (G), +0.1/+4.4 (D) re their
recordings; partial 2 of the lowest keys comes down a little too, and the lowest keys get quieter
(their loudness is set before the trim, as Grand C's in step 142). Heard as "a very clear difference
for all of them … more realistic". The user liked Grand F's heavy bass as well: an EQ after the
plugin brings it back (a bell at about 45 Hz, +20 dB, two octaves wide; the trim acts on the
strings, an EQ also lifts the room's and the thump's lowest part); a knob for the board's bass is a
candidate for the inventory of traits.

<a id="step-148"></a>
## 148. The inventory of traits; the board's Bass

The traits plan's phase 4, the inventory of traits (design 2026-10-23-board-knobs): which of a
piano's traits are worth a control. The board carries a piano's identity (step 117: grafted onto
other strings it sounds like its own piano), and nothing reached it; the felt has the Felt tab
(step 145); the strings' and hammers' traits add character but nothing heard asks for a control
yet. The report's outliers among them were the fits': the strings' loss (and the board coupling,
its multiple) rests on the keys with at least 8 well-measured decays, past which the smoothing
carries the last keys' slope on. Only Grand A is measured to C8; Grand C turned down from 96,
D from 92, F climbed from 99. The top octave's decay against the recordings (fundamental, mf,
from D♯7) does not follow those curves: D and F ×1.0, B ×0.8, E ×1.5 and A ×2 too fast, Grand G
×0.45 (its top octave rings 2–4× long; measured to 95). The report now draws a trait dotted past
the keys its fit measured.

**Bass** on the Sound tab, on the hammer pianos, set and then played (a rebuild and its
crossfade): −12 … +24 dB on the strings, every note, in their per-note correction at the compile
(`board_bass.rs`), as Grand F's step 147 trims; shaped as those trims summed, its top widened by
ear half an octave: full at 45 Hz, 0.8 below and at 63 Hz, 0.5 at 90 Hz, 0.25 at 125 Hz, none from
180 Hz. Loudness follows, as with an equalizer. +20 dB on Grand F gives a little more than its
bass before step 147 (below 120 Hz 4–5 dB more from A0 to E2, A2 +5.5 dB over the whole note at
Player); a bass alone rebuilds no response, the strings' decay stays bit for bit, a morph takes
it; every hammer piano at +24 and −12 dB, at Recording and Player, plays finite, under its boost
+ 6 dB, and dies away.

Not on the board's responses, which carry the strike's thud as well as the strings. The first
version put it there, as an equalizer on every body response; by ear, +20 dB on Grand F made the
attack pop in the bass, "really noticeable and quite unpleasant": under the notes above the shape,
whose own partials take none of it, the thud below 120 Hz rose 10–14 dB in the attack's first
30 ms (Grand F at Player, mf: C3 −10.7 → +0.3 dB re the whole note's first 0.3 s, as loud as the
note; G3 +13.5 dB; the knock off, the same). On the strings, the thud under G3 … C6 stays as
shipped (to 0.1 dB); with the first shape +20 dB gave the old Grand F's bass within 1–2 dB, and
125 Hz was heard as a little too low a top.

Tried and taken off: **Warmth** and **Air**, the board along the ten references' first and second
directions (the rolled pianos' basis: +1σ warmth +12.3 dB at 63 Hz, −7.4 dB at 2 kHz; +1σ air
88 dB/s slower above 1 kHz), each band's decay and level changed in every finished body response
(0.12 s a rebuild at Recording, 0.4 s at Player). "A huge effect", but neither named what the user
hears: each step moves the bass, the treble, the board's ring and a per-key part at once (warmth −2,
"cold", made Grand K's C1 12 dB louder at Player, its upper partials up to +15 dB), and warmth's
bass on the board popped as the first Bass did. What the coloring needed, should a knob on the
board's responses come back (the branch board-knobs, commit 73be458): a minimum-phase level change
(a zero-phase one rang before the strike, and cutting that off leaked a +12 dB bass boost into
every band); a slower decay stopping where a band has died 60 dB and at a quarter of its own rate,
the response's last 20 ms faded (+2 air raised the late floor by up to 88 dB, a ghost 0.5 s after
a staccato note); both microphones of a response one operation; FFT lengths padded to 2·3·5-smooth
sizes (an FFT length with a large prime factor took 3.2 s a rebuild at Player).

<a id="step-149"></a>
## 149. Xruns beside a build

Audio dropouts while morphing, heard during builds. A probe played a dense pedalled passage in
256-sample blocks (5.33 ms) on a SCHED_FIFO thread through the C API, as a host does, while a
worker built morphs between Grands A and C at Player every 100 ms (Ryzen 7 7840HS, 8 cores, 16
threads). Idle the blocks took 1.2 ms (p50), 1.8 ms (p99); beside the morph builds p99 3.8 ms,
max 5.5 ms; with a felt set (each build measuring the level table) p50 5.4, p99 9.1 ms, a third of
the blocks over the budget. Three causes:
- **An allocation on the audio thread**: `Voice::kick` collected a sign per partial at every damper
  lift, about 50 allocations in the block a pedal goes down, and every morph step replays the pedal
  on the new engine; beside a build that waits on the allocator's lock. The signs (n^−8: three
  above −80 dB) are on the stack now, the output bit for bit as before. `engine/tests/realtime.rs`
  counts the allocations of a pedal press and of a dense passage with an adoption: none.
- **The memory**: the audio thread executed the same instructions per second beside a felt's
  measurement but took 4× the cycles (IPC 3.1 → 0.8; its L3 misses +70 %): at a microphone position
  it misses the 16 MB L3 by design, and the measurement filled the memory's bandwidth. Each of
  its 704 renders (0.2 s each) copied every note's knock and release burst into its engine and
  its instrument's clone (85 MB a render, 59 GB in 2 s). The bursts are shared now (`Arc`, the
  instrument's own at its rate); a measuring engine's responses end with its window (the same
  table to 1.4e−6 dB): 19 MB a render, the measurement 3.1 → 2.2 s, a felt build in the probe
  5.0 → 2.3 s. Plain renders are bit for bit as before (48, 44.1 and 96 kHz, a morph).
- **Every core**: a build on all 16 threads, the audio core's sibling among them, slowed the blocks
  more than on 7 (p99 3.8 vs 2.6 ms beside morph builds; the felt's measurement no faster past 5
  threads, bound by memory). The plugin's builds take the physical cores but one
  (`pe_set_build_threads`; the offline tools keep every core), at nice 10 (no effect on a
  real-time audio thread, but below the host's other work).

Beside felt builds on 7 threads now: p50 1.7 ms, p99 2.9, max 3.8, none over the budget (was
4.1–4.4, 6.8–7.4, 8.5–9.1 ms and 500–600 blocks over in 25 s); beside plain morph builds p99 2.7,
max 3.9 ms.


<a id="step-150"></a>
## 150. The strings' loss up to the top key

The strings' loss b1' rests on the keys with at least 8 well-measured partial decays; a grand's top
4–10 recorded keys have fewer (the fundamental and a partial or two), and past the measured keys
the smoothing carried the edge's local slope on: Grand C's turned down from 96 and its top keys rang
about twice as long as recorded, Grand D's from 92 (step 148). Two changes in `scale.py`:

- **The recorded keys past the measured range join the loss's curve**: each with b1' from its
  fundamental's decays (the confidence-weighted mean of σ − b3·ω², b3 from the measured keys'
  curve), weighted by their count as the measured keys are. The fundamental alone: past the
  measured keys the b3 curve is no measurement, and on the upper partials σ − b3·ω² fell below zero
  (Grand C's key 102, its third partial: b3·ω² 190 dB/s against a recorded 89), which first made
  Grand C worse. Their fundamentals count down to a confidence of 0.05 (the measured keys' rows 0.3):
  a top key's fit is short and its range small by nature (Grand G from C7: 0.04–0.37 at 12–17 dB),
  its rate no worse; that matters on Grand G and Upright B alone.
- **Past the last key with any decay** the curve carries on in a straight line at the edge's slope,
  never falling toward the treble (`_smooth(…, rising=True)`; the bass end as before).

The loss's curve against the recorded fundamentals above each piano's old measured range (log2 of
the model's rate over the recording's, the median; 0 is right, below it the model rings long):

| | Measured to | Before | After |
|---|---|---|---|
| Grand B | 94 → 104 | −0.40 | −0.11 |
| Grand C | 96 → 108 | −0.80 | −0.19 |
| Grand D | 92 → 104 | −0.28 | +0.14 |
| Grand E | 96 → 106 | +0.04 | −0.01 |
| Grand F | 99 → 108 | +0.73 | +0.05 |
| Grand G | 95 → 108 | −0.50 | −0.22 |
| Upright A | 87 → 105 | +0.03 | +0.06 |
| Upright B | 91 → 103 | −0.38 | +0.02 |
| Upright C | 89 → 102 | +0.01 | −0.11 |
| Harpsichord A | 84 → 89 | +0.33 | +0.04 |

Grand A (measured to C8), the harp and the other plucked strings keep their loss; the clavichord's
and Harpsichord B's top keys move with Harpsichord A's (b1' ×1.3–3.3 at their top key). Below the
keys that moved, b1' changed by less than 1 %. In the engine (renders at the Recording position, mf,
the analysis's own estimator, the engine's envelope fitted over the recording's range: Grand G's
soft layers show 25 dB of the top keys' decay, its clean renders 100 dB), the fundamental's decay
from D♯7 re the recording, the median: Grand C ×0.48 → ×0.85, D ×0.90 → ×1.06, B ×0.83 → ×0.93,
G ×0.59 → ×0.73, E ×1.24 → ×1.23, F ×0.93 → ×0.82, Upright A ×1.11 → ×1.05, Upright C ×0.99 → ×0.76
(two keys). Per key the engine's top decays scatter 0.4–3× round that even on Grand A, measured to
C8: the per-key board correction and the unisons set them more than the loss's trend. Re-exported
(the export alone; the fits upstream saw the old loss on those keys), the level tables measured again
(up to 2.7 dB on Grand F's top keys, their loudness kept).

<a id="step-151"></a>
## 151. The uprights' dampers and rooms

The uprights by ear (the user, 2026-10-24): Upright A and C "not bright enough, at any velocity", their
room "way too much reverb"; Upright B "broken": little sustain, little bass, dampers that "don't work
right". Measured against the recordings and against Grand A (heard as right), the first of four
findings was the release: 0.3 s after a key's release Grand A has fallen 23–27 dB (100 Hz–5 kHz,
Recording position, mf), the uprights 6–17 dB.

**The dampers.** The uprights borrowed Upright A's (step 119), measured from its release recordings at
16–38 dB/s for the fundamentals against Grand D's 91–239 dB/s. Its takes hold their level for 0.2 s
(−2 … −6 dB) before they fall (−15 … −21 dB by 0.4 s): a late landing or takes begun before the
key's release, and the first fall over 12 dB from the peak read mostly the plateau; the engine has no
landing delay. Upright B's own release recordings (its set's `Releases`) are cut at 0.3 s, the key-up
thump on their peak: 370–640 dB/s, no fundamental readable. The uprights borrow Grand D's dampers now.

**The rooms.** With Grand D's dampers and the room off, all three fall as Grand A (−28 … −39 dB at
0.3 s); with it on, Upright C held at −9 … −13 dB. Upright C's room, measured between its recordings'
partials, rings 1.1–4.5 s from 250 Hz to 3.2 kHz (the grands 0.2–1.4 s; a living room about 0.5 s),
the same from note to note (at 1.3 kHz 3.8–4.9 s over 19 notes): strings ringing in the gaps or the
take's own reverb, not a room. Upright A's reads 1.2–1.6 s at 126–252 Hz (its hum). Both take VCSL's
dry studio (`[strings] room_t60 = "vcsl_dry"`, 0.2–0.45 s), as four plucked and struck sets do;
Upright B's (0.24–1.2 s) stays. Heard as variants (`--with`, the export alone): Upright A dry, Upright
B with the dampers alone, Upright C dry over an assumed living room (0.6 s at 250 Hz to 0.3 s at
16 kHz) best. A variant can now override what a project borrows (`analysis/project.py` `uses` reads
the command line's `[uses]`).

Refitted from the soundboard on (the felts kept: their reads changed in keys they do not use, the
outputs adopted). The fits moved little (the coupling's err within 0.1 dB), but the thump's: Upright
A's to none (gain 0.35 → 0.000008), Upright C's lowpass 300 → 1500 Hz; the thumps are held at the
values heard (as in step 127), and Upright C's 10 dB further down by ear ("too loud now", −10 dB on
Action knock: `[impact] trim_db` −20 dB to C4, −16 dB from C6). After the release now, re the level
before it (0.1 / 0.3 s, keys A1, C3, C4, C5, C6):

| | Before | Now |
|---|---|---|
| Grand A | | −7/−24, −5/−23, −6/−32, −5/−30, −8/−16 |
| Upright A | −2/−8, −6/−5, −2/−12, −11/−17, −9/−16 | −6/−26, −11/−26, −6/−32, −16/−38, −13/−43 |
| Upright B | −4/−12, −6/−14, −7/−11, −0/−3, −6/−14 | −8/−28, −9/−31, −11/−32, −5/−29, −12/−31 |
| Upright C | −3/−9, −2/−7, −2/−7, −2/−9, +3/−1 | −7/−28, −7/−28, −4/−26, −5/−35, −11/−29 |

Upright C's room, refitted in level for the shorter
tail, holds 5–10 dB more at C5–C6 than its variant did. The dry rooms darkened Upright A's and C's bass in the rolls' realism
measure (Upright C −76.5 → −84.1 dB, the darkest reference), so the range takes a darker bass:
`roll_measure`'s dull bass now darkens all five of its bass notes (on four, the fifth held it at
−88.4 dB whatever the cut; now −100.3, 6.5 dB past the edge). Left from the same measurements: Upright B's
two lowest fundamentals 20–30 dB over their recordings (step 147's flaw); Upright A's and C's attack
dark at mf below C4 (4–8 kHz: Upright A −17 … −21 dB, 8–16 kHz −26 … −46 dB on both; their felts were
never set by ear); Upright B's held notes from C4 up 7–13 dB short at 1–2 s (its loss: the upper
partials' fast decay in b1', 2–3× the low partials' from C3).

<a id="step-152"></a>
## 152. Upright B's lowest fundamentals

Step 147's flaw on Upright B: its board fit's data start at 65 Hz and hold the board's response flat
below, and its two lowest recorded keys' fundamentals sat over their recordings re partials 2–4 (mf,
the analysis's estimator on both): B♭0 +28.8 dB, G1 +21.1, C2 +3.4. `[board] trim_db` rows at
22.5, 31.5 and 45 Hz, solved by least squares within their −12 dB bound (all three at it): B♭0
+13.1, G1 +8.2, C2 −2.4, G2 unchanged (−8.4). Heard as the right direction. The "missing bass" the
user heard was not its level: every key of Upright B plays within −5 … +3 dB of C4 (A-weighted, mf,
0.05–1 s), its recorded bass keys 1–9 dB over their recordings re its middle, its share below 250 Hz
as the other uprights'. It sounds thin: "everything below about C3 not bright enough" (G1, G2 heard in
the editor against the recordings), its lowest keys' attack at mf 15–25 dB under the recordings at
1–8 kHz: its felt, never set by ear (next).

<a id="step-153"></a>
## 153. The uprights' felts by ear

The uprights' felts were the fit's (p 2.30), never voiced: the attack at 1–2 and 2–4 kHz re 200–500 Hz
(bass_check's bands, the export's velocities per layer, engine − recording, the median over the
recorded keys) lay 5–14 dB dark at mf on C2–C5 (Upright A), 5–21 dB at pp and mf (Upright B), 4–7 dB
(Upright C), their bass at mf 5–17 dB. Solved as the grands' (steps 141–142: two probe exports, the
exponent −0.4 and the stiffness +3 dB, a least-squares step, a check):

| | `[felt]` | C2–C5 after | A0–B2 after |
|---|---|---|---|
| Upright A | p 1.92, +6.2 dB, then +3 dB by ear | within ±3.3 (by ear: +1 … +7) | −3 … +3 |
| Upright B | p 1.96, +12.0 dB (the bound) | within ±2.2 | mf and ff within ±3; pp +13/+26 |
| Upright C | p 1.92, +4.5 dB, a core in the bass | within ±3.3 | mf 2–4 kHz +1.5, ff +3.4/+0.5 |

Upright B's pp recordings scatter from key to key (−7 … +37 dB at 2–4 kHz against one felt); solved
from mf and ff alone (p 2.52, +10.5 dB) it was heard as worse. Upright C's bass stayed dark with the
felt (mf 1–2 kHz −8.9, ff 2–4 kHz −9.1): a `[felt_core]` at 3 N to D♯2, none from C3, its depth at about
half each key's rigid-string bound of the compression at mf (A0 1.66 mm: 0.8, D♯2 0.55, C3 0.45 mm),
so that it reaches mf, heard better than a deeper one at 2 N (ff alone); its mf at 1–2 kHz stayed at
−8.6 dB in every probe, not the felt's. Upright A's golden render +0.3 … +0.6 dB.

Upright B then sounded short again. The felt did not take its sustain (the attack over the held part,
A-weighted, within 0.7 dB of the felt before): the brighter attack bares what was there. Its held
notes from C3 up lie 4–8 dB under its recordings at 1–2 s (100 Hz–5 kHz re the attack, mf, per key
where both last: −9 … +7 dB), within about −5 … +3 dB at 4 s; the treble no shorter than the middle
(a first count, 7–16 dB in the treble, compared medians over different keys: its treble takes last
2–6 s). Its partials' decays over their whole length (EDC) match or are slower than the recordings'
(C4–B5: partials 1–2 ×1.01, 3–6 ×0.66), and the coupling fit's model (which the engine follows,
key 72 and 84 compared) holds partials 1–3's late level within ±4 dB per octave at its error's
minimum (the trichords' kf3 ≈ 1.0: scanned 0.25–2.5); C6's take, a hold and then one fall at about
16 dB/s, is a shape the model does not make (10 dB whatever kf3). Holding the model's EDC at the floor
the measured curves are cut at (−40 dB) took C6's error 24.6 → 10.1 dB (two partials at 11.6 and
14.6 kHz, −47 and −80 dB in the model against −26 and −15) but rang the treble shorter in the engine
(β1@96 0.40 → 0.48): not kept. Heard with the Sustain control, the middle wants 150 %, the treble
about 200 %: a longer ring than recorded, as the grands have (Grand A's late level 4–7 dB over its
recordings in the middle); a by-ear sustain per register, which the export cannot carry yet.

<a id="step-154"></a>
## 154. A ring per key by ear

Upright B's held notes lay 4–8 dB under its recordings at 1–2 s (step 153), and no refit lengthens
them: the coupling fit sits at its minimum. Heard with the Sustain control (step 129), its middle
wanted 150 %, its treble about 200 %: a longer ring than recorded, as the grands have (Grand A's late
level 4–7 dB over its recordings in the middle). The control is one number at runtime; a project's
`[sustain]` now sets it per key by ear: `percent`, (key, %) points (25–800 %), linear between and held
beyond. The export writes each note's `sustain` (left out at 1), the engine multiplies the control's
value by it when it compiles the note (`physics::compile`: every mode's decay rate divided, the
frequencies and the strike's shares kept, the dampers and the contact as they were), so the control's
100 % is the instrument's own ring; a morph mixes it geometrically, a roll keeps its frame's. A key's
factor of 2 compiles exactly as the control at 2, the factor 2 under the control at 2 as 4
(`engine/tests/sustain.rs`); every export without the table is unchanged.

By ear: Upright B and A 150 % to C5, rising to 200 % at C7; Upright C 150 % over the keyboard (the
rise heard as too much on it). Upright B's held level re the attack (mf) at 0.5/1/2 s then: C3–B4
−8.6/−17.4/−20.6 dB (its recordings −10.5/−16.0/−23.5, before −10.7/−19.5/−23.9), from C5 up
−15.2/−22.1/−30.0 (−16.6/−26.9/−33.6; before −19.7/−31.0/−40.2). Heard as better on all three.

<a id="step-155"></a>
## 155. Upright B's room by ear

With its dampers, felt and ring set, Upright B sounded "very dry". Its room was its recordings': the
decay between their partials 0.24–0.44 s below 640 Hz and about 1 s at 1.6–6.4 kHz, and its level
fitted 10–25 dB down above 1 kHz (1 kHz −10.05, 2 kHz −16.29, 4 kHz −25.13 dB) to the dry gaps of
VCSL's close takes. What the room added over the dry sound (100 Hz–5 kHz, mf, held 0.5–1.5 s, keys C2,
C3, C4, C5, C6): −1.1 … +3.9 dB, against Upright A's +4.4 … +6.2 and Grand A's +6.9 … +11.0. `[room]
trim_db` now lifts as well as cuts, a band at most +6 dB over the room's unit level. Three variants:
half the cuts undone (+0.5 … +6.6 dB), all undone (+2.1 … +11.9), all undone with an assumed living
room's decay (`[strings] room_t60`, 0.6 s at 250 Hz to 0.3 s at 16 kHz; +5.1 … +12.5): the last heard
best. Refitted from the room on with it (its level then cut at 1 kHz alone, −5.82 dB, which the trim
undoes; the thump held as heard, the knock's refit +3 dB), the room adds +5.0 … +12.7 dB held, within
1 dB of the variant.

<a id="step-156"></a>
## 156. Stems from `midi2wav`

To hear one part of a passage alone (the plugin's controls only mute one at a time) and to check one
part between two versions, `midi2wav --stems` writes the parts beside the render: the struck strings
and the ringing (the sympathetic bank with the rattle) as the board radiates them directly, the room
of both, and the noises (knock, release, pedal). The engine is linear from the bridge on, and the bank
listens to the strings without driving them back, so each stem is a whole render with the other parts
muted where they join the mix (`engine::Mix`: the strings' bridge force after it has driven the bank,
the diffuse part, the radiation's direct and room outputs, the one-shots), the parallel segments the
same for each. Grieg's Valse-Impromptu on Grand A (196 s, room 1.585, pedal noise 0.5): the stems' sum
within −131 dB of the render's peak; their levels re the render: strings −6.9 dB, ringing −19.1, room
−1.1, noises −44.1 (rms). Five renders instead of one (5 s each here). `engine/tests/stems.rs` checks
the sum at the recording's position and at a zoned one (the Player's, where each string feeds its
bridge zones directly).

<a id="step-157"></a>
## 157. A press is any value but 0

Two of the controller's (the MPK mini IV's) parts played badly with MIDI learn. Its + and − buttons
send one CC, + 127 and − 0, at each push and nothing at the release: tied to Next instrument, the
first push stepped and the next ones did not (a step needed a rise through 64, and nothing fell
back), and Previous could not share the CC. Its pads send a CC value as hard as they are hit: a
normal hit stays under 64, which neither stepped nor switched a switch on.

A press is now any value but 0. A Next or Previous tie steps at every value on the side it was
learned on (the value 0, or any other): a pad steps however softly it is hit and its release does
nothing, a button that sends the same value at every push steps at each, and one CC carries Next on
one side and Previous on the other (the map's line "… previous_instrument zero", its tag "CC 40 at
0"; a control's tie still takes the whole CC). The learned value tells the side (`packLearned`: one
more bit, `kDiagLearnedOut` up to 4096). A switch is off at 0 and on at 1–127. Gone with it: the
step's memory of the last value (a knob tied to Next now steps at every value it sends, where it
stepped once per pass through 64).

<a id="step-158"></a>
## 158. Relative knobs

The MPK mini IV's knobs are endless; in relative mode they send moves: about 1–10 turning up and
117–127 turning down, more the faster the turn (two's complement, the commonest scheme). Tied as
absolute, such a knob jumped between the two ends. A control's right-click menu now also offers
"Learn MIDI (relative knob)": the tie (`Tie::relative`, "… relative" in the map's file, the tag
"CC 21 ±") reads each value as steps (`relativeSteps`: 1–63 up, 128 − v down, 0 one up for the
controllers that send it at the slowest turn) and moves the control from where it is
(`nudgeValue`, applied by the DSP side to its current value): a slider 1/127 of its travel per
step, a list and an integer slider one entry per 3 steps (gathered by the router per target, a turn
back starting over so that it answers at once), a switch on turning up and off turning down. Since it
never jumps, a relative knob needs no soft takeover. A relative tie takes the whole CC (both sides);
in the file a Next or Previous one steps the instrument the same way.

<a id="step-159"></a>
## 159. Dragging the microphones

The user heard xruns while dragging the microphones. `bench` with `BENCH_DRAG=1` plays it as the
plugin does (the C API: a worker building engines through a cache at the plugin's build threads, the
free pair moved 1 cm per build; an audio thread paced in real time, a pedalled passage, taking each
over): Grand A at 256 frames, the pair held still, block time re the budget median 25 %, p99 57 %;
dragged (90 handovers in 12 s) median 86 %, p99 181 %, 588 of 2250 blocks over. The builds alone
(no handover) were harmless (median 30 %, none over): the handovers were. At a handover the old
responses kept their input crossfading out and rang out their tails, the room's 2 s, up to four at a
time beside the new: at a position with bridge zones 16 board convolutions and 2 room ones each, all
of them running through a drag.

Both setups are fed the same bridge forces; only their responses differ. So the new convolvers now
take the old ones' input history (`Convolver::take_history`: the stored input spectra, the block
being collected, the output on its way; the products ahead with the new responses caught up over
the rest of each long block's period, `push`): from the handover on they sound as if they had always
played at the new position, and the two crossfade at their outputs over 30 ms (`OUT_FADE_S`), after
which the old ones stop. Where the layouts differ (another number of zones) or the old history still
holds a ramp, the handover is as before. The handover itself copied every sounding mode into the new
voices (0.5 ms with the pedal down); strings compiled alike (`Instrument::strings_print`, a hash of
every note's physics, a destructuring that names each field) now swap the voices and the bank whole.

Dragged, Grand A at 256 frames: median 35 %, p99 72 %, max 97 %, none over (the handover itself 9 %
of a block, 25 % before); Grand D 3 and Upright A 1 blocks over of 2250. At 128 frames 36 of 4500
over (21 in a handover's block: its copy of 2.5 MB of input history; the builds alone 3).
`engine/tests/handover.rs` checks that a moved pair sounds as the engine that played there all along
(within 10⁻⁴ of the peak from 0.1 s after the handover); `convolution.rs` the same for one convolver.

<a id="step-160"></a>
## 160. Flipping pads, an instrument knob, soft takeover

Three of MIDI learn's leftovers. The controller's pad Toggle is one setting for all its pads, so
toggled stops and momentary Next/Previous pads could not coexist: a switch's right-click menu now
offers "Learn MIDI (pad flips it)", a tie (`Tie::flip`, "… flip" in the map's file, the tag "CC 30
flip") whose every press (a value but 0, step 157) flips the switch and whose release passes; the
pads can stay momentary. Turning through the instruments with a relative knob (step 158) took Next's
tie away from its button: the MIDI tab has a third target, "Instrument (relative knob)"
(`kTargetKnob`, "instrument_knob"; always relative, steps either way, one per 3 clicks). Soft
takeover (the MIDI tab's switch, on by default; "soft_takeover on|off" in the file): after a slider
moved otherwise, its value no longer the one MIDI set last, an absolute tie moves it only once the CC
comes within 2 % of its travel or passes it between two values (`Takeover`, on the DSP side beside the
router); lists, switches and relative ties take every value.

<a id="step-161"></a>
## 161. Scala files in the plugin

The engine read twelve-note Scala files for `midi2wav` only. The Temperament menu now ends in
"Scala file": DPF's file browser (`USE_FILE_BROWSER`) picks a `.scl`, the plugin keeps its path (the
"scale_file" state, a file path to the hosts) and builds with it (`PeTuning::scale_file`, read on the
worker). Any note count: `temperament::parse_scale` keeps every degree and the period the scale
repeats at; a Scala key map beside the file of the same name (`.kbm`: the keys retuned, the middle
key for degree 0, the reference key and frequency, the period's degree, a map with keys left out) is
read too (`parse_kbm`). `scale_keys` gives each of the 88 keys its pitch re equal temperament, the
tuning's new `keys` (used in place of the twelve pitch classes' `temperament`); without a key map one
key per degree, degree 0 on the temperament key's pitch class in middle C's octave and A4 kept, so a
twelve-note octave scale gives exactly the menu temperaments' offsets (tested on Werckmeister III,
rotated), and the temperament key rotates a Scala scale as it does the built-in ones (the "rotation of
the wolf fifth" on the roadmap: a meantone's wolf moves with its key). A key is held within two
octaves of its own pitch. The row shows the file's name; Copy CLI writes `--scl '…' --key N`.

<a id="step-162"></a>
## 162. Dragging the morph pad with a felt set

With a felt edit, every point of a dragged morph pad measured its levels twice (the shipped felt's
rows for the new recipe, then the edited instrument's: `level_rows`, 528 renders, 3–7 s), so a drag
was heard seconds late, point after point. A felt edit's leveling is now its own value
(`FeltLeveling`: the level table played and each key's shift; `Instrument::felt_leveling` measures
it, `with_felt_leveling` applies it), and the C API's cache keeps the last one measured per felt.
Quick builds (`pe_cache_set_quick`) take it for any point not yet measured; the plugin's worker builds
quick first and, when the build stood one in (`pe_cache_provisional`) and nothing newer is asked, the
same settings again measured: each point of a drag plays within a morph's build (0.21 s), the point
the pad stops at is measured after. Between Grand C and Grand D at 30 % → 40 %, the stand-in's key
levels lay within 3 dB of the measured ones (`ffi::morph_tests`). The roadmap's other morph cost, a
radiation cache shared by the level renders, measured small now: the level step is 57 ms of a morph's
0.21 s (its renders already in parallel, each thread's responses computed once), the build's room
synthesis 66 ms and noise bursts 96 ms; left as they are.

<a id="step-163"></a>
## 163. Instruments kept anywhere

An instrument played only from a search path. The instrument menu now ends in "Open instrument
folder…": a file browser picks a folder's `instrument.json`, and the "instrument" state holds the
folder's absolute path instead of a directory's name. `Catalog::outside` makes such a path an entry
the first time it is asked (the search paths' test: both files, the engine's report; the group
"Opened") and keeps it for the process, so every lookup by id (`Catalog::find`, `instrumentDir`, the
worker's build and file watch, the Copy CLI line) takes it as it takes a listed one. Morph partners
stay the menu's instruments.

<a id="step-164"></a>
## 164. A fifth less CPU

Looking for more headroom after step 159. `perf` is not allowed on this machine (kernel.perf_event_paranoid 2,
left as it is); callgrind on `bench` (now `BENCH_ONLY` and `BENCH_SECONDS` for a short run), counting only
`Engine::process_input`, at the Player position (bridge zones): the convolutions' complex
multiply-accumulates 18 %, the strings' resonators 15 %, the modes' per-block sync and cull 18 %, the
convolver's output copy 9.5 % (a modulo per sample), the lanes' sums 9 % (an f64 sum in lane order, which
cannot vectorize). Changed, each measured:

- `kernel::cmac`: the products on AVX2+FMA (`fmaddsub` on interleaved spectra), dispatched as the
  resonators' kernel is; one radiation-sized convolver (2 channels, 26 386 taps) 13.2 → 9.8 ms per 8 s;
- the output ring copied by slices (its position steps by whole base blocks): 9.8 → 7.7 ms;
- a stage skips an input block of exact zeros (no transform, its products skipped; the output the
  same, `convolution::tests`), and with zones the diffuse part (the sympathetic bank, the rattle),
  which was spread over every zone, goes through one convolver of the zones' mean vertical response
  (Σ h_z ∗ d/Z = h̄ ∗ d): a zone no string feeds stays silent;
- a voice's modes are culled every 4th block, each voice at its own (a mode 100 dB down rings a few
  ms longer), the sympathetic bank's likewise;
- the lanes summed as a pairwise tree in f64 (vectorized), the whole-block sync by lane chunks.

`bench` (Grand A, 256 frames, mean load), the recording's position / the Player's:

| scenario | before | after |
|---|---|---|
| pedal, 32 ff notes/s | 14.4 / 21.6 % | 11.7 / 17.5 % |
| 8-note mf chords, no pedal | 3.8 / 8.7 % | 2.9 / 6.1 % |
| pedalled ff glissando | 13.6 / 20.7 % | 11.1 / 16.6 % |
| half-pedal sweeps | 8.1 / 14.3 % | 6.8 / 11.7 % |

The golden renders moved by about 10⁻⁶ (the summation's order), regenerated. The silence skip rarely
fires in these scenarios (struck strings ring on through their zones, each key feeds two); it pays
where a zone is idle.
With step 159, a microphone drag (`BENCH_DRAG=1`, Grand A): at 256 frames median 26 %, max 64 % of the
budget; at 128 frames 1 block of 4500 over (36 after step 159 alone).


<a id="step-165"></a>
## 165. The Visualization panel

The spike's main video (branch `viz-spike`, roadmap notes "Live visualizations") drawn live in the
plugin: a "Visualization" toggle beside "Keyboard" and "Analysis" opens a panel in the Analysis
panel's place (one at a time; the saved `diag` level 3). The strings above the strip's keys, evenly
spaced, their lengths the instrument's own (compressed: a 5 cm treble string beside a 2 m bass one
stays visible); a sounding one is a glowing band as wide as its partials' displacement (each partial
k as sin(kπu) along the string, summed in power, the width compressed as a square root), the
sympathetic bank's ring on it drawn 2.5× larger, and inside it a snapshot of the string at random
phases drawn anew at every repaint (the frames' 30 Hz set only the bands' width). The dampers sit on
the strings, lifted ones raised and faint; a piano's hammers move in a row under the strings; a pluck
flashes at its point, a bar instrument draws its bars (and the damper bar) instead.

The engine's telemetry frame carries the keys (`telemetry::KeyFrame`, 88 × 288 bytes, frame version 4,
44 kB): per key the first 32 partials' displacement (every register's vertical modes over their bridge
weights, summed in power; m), the bank's ring on the same scale, the damper's engagement, the last
strike's position and speed, the speaking length and the strike or pluck point. A bar's modes are in
model units: scaled by 3 mm over its reference amplitude, they land within the strings' range. Grand A's
C4 peaks at 2.4 mm (every partial) at velocity 110, 0.2 mm at 30; the other string instruments' strings
at 1–5 mm at velocity 110; building a frame with a pedalled 12-note chord costs 28 µs instead of 19 (30 times
a second, only while a panel is open).

The hammers (`Viz.hpp`, tested in `ui_tests`): at the string for 30 ms from the engine's strike (a
contact of 1–2 ms no frame would show), back at half the strike's speed over the 47 mm blow, caught by
the backcheck at 0.45 while the key is held, from there to rest at 6 heights/s after the release; a key
released on the way back goes on at its rebound speed. No rise is drawn: a live note-on comes when the
hammer arrives. The spike's hammers sometimes went through the strings: its rise, started up to 40 ms
before a note-on, read the previous strike's hammer before that strike happened when a key was struck
again quickly (a height above 1); the live model draws only from strikes that happened and clamps
every height to 0 … 1.

<a id="step-166"></a>
## 166. A video from midi2wav

The Visualization panel as a video, for a piece: `midi2wav … out.wav --video out.mp4` (with `--size`,
`--fps` and `--ffmpeg` for ffmpeg's output options, e.g. AV1 with Opus). With the plugin's Copy CLI
line it films the setup being played. After out.wav is written, one engine renders the whole piece
again (the engine is deterministic: the sound's own state to the sample) and stops at every frame's
moment (its latency added) for `Engine::viz_keys`; the frames are drawn with `tiny-skia` (pure Rust)
on every core, a batch at a time, and a writer thread pipes them as raw RGBA into ffmpeg, which takes
out.wav as the sound.

The picture is the spike's (`tools/viz/draw.py`) in Rust: the strings at their compressed lengths, the
bands and the glow (drawn at a quarter of the size, box-blurred three times, added back bilinearly), a
snapshot of the string at fresh phases each frame, the dampers, the hammers with four exposures over a
frame, the keys lit by their key's color, the three pedals as bars under the keys' middle. By eye
(the user): the hammers' row halved to 110 px at 1080p (they travelled too far), the pedals moved
from a sustain bar at the top right to the three under the keys.

`--transparent` leaves the background out, for compositing (ProRes 4444 with its alpha by default).
Light needs an alpha of its own: drawn as it is, a band over nothing would be a dull opaque color.
Each added color c goes in premultiplied as (c, max(c)), so where it lies on a key or a string it adds
as before (the alpha saturates), and where it lies on nothing it composites over a picture as light;
the glow's upscale raises the alpha the same way, and the frames go to ffmpeg with straight alpha.
Over a bright gradient the keys, hammers and dampers stay solid and the strings glow over it; 30 s at
1080p60 in ProRes take 47 s. By eye (the user): a ringing string no longer shows the idle string's gray line
inside its band, and the hammers' earlier exposures are see-through instead of darkened (over a
picture they had shown as dark smears). Offline the score is known, so a
hammer rises before its note over twice its blow at its speed (at most 0.2 s), from wherever the key's
earlier strikes left it and never before the previous strike: the spike's glitch (a rise begun before
the previous strike, which read that hammer above the strings) is a test now.

The cost at 1080p60, a 30 s excerpt on 16 threads: the drawing first took 71 ms a frame (each thread),
of which tiny-skia's scaled draw of the glow took 21 ms twice; the glow's upscale written as one
bilinear pass (8.5 ms) and the partials' sines tabulated: 34 ms. The writes moved to their own
thread: 87 → 194 frames a second without encoding, 119 with x264 (18 s for the 30 s), 52 with
SVT-AV1 at preset 6 (42 s). A 4½-minute piece took 221 s before these changes.

<a id="step-167"></a>
## 167. midi2wav's command line

`midi2wav` had no `--help`, and its errors were Rust's debug output: `Error: ParseFloatError { kind:
Invalid }` for `--rate abc`, `Error: Os { code: 2, … }` for a missing MIDI file, the whole usage line
for a wrong `--bits`; a missing output folder failed after the render. Its options now go through
`clap`: `-h` the summary by heading (Rendering, Playing, Instrument and tuning, Microphones, Output,
Parallel render, Video), `--help` each option in full with every key of `--param` and `--tuning`
(the tables `spec::PARAM_KEYS` and `spec::TUNING_KEYS`; a test holds them to the parser) and the
temperaments, `--version`. Every value is checked before anything is loaded: an unknown option gets
clap's suggestion (`--parm` → `--param`), an unknown key the nearest known one (`roomm` → `room`,
`mic_patern1` → `mic_pattern1`), a value what it should be (`--bits`: 16, 24 or 32); `--size`, `--fps`,
`--ffmpeg` and `--transparent` need `--video`, `--key` needs `--scl`, `--wall` needs `--mic`. Then the
files: a missing instrument directory, a folder without `instrument.json`, an unreadable MIDI file and
a missing output folder are named before the render. Usage errors exit with 2, the others with 1, each
as one `error:` line. The options' names stay (the plugin's Copy CLI writes them).

<a id="step-168"></a>
## 168. The uprights voiced as the grands

After steps 151–155 the uprights sounded "pretty okay" (the user, 2026-10-26); what the grands
taught was applied to them in turn, measured on the exports against each upright's recordings
(`proto/tone_check.py`, the measure of steps 143 and 147: every recorded key at the layers'
velocities, the two channels' power averaged, within a note its octave bands over the first second re
the band of its fundamental, and the lowest octaves' fundamentals re partials 2–4; on Grand A it
reads the treble's 4 kHz at +1.8 dB at mf, step 143 +3.5 by its own count).

- **Even levels** (step 138): the keys' ff levels lay 1.7, 1.4 and 1.8 dB rms around their trend on
  Upright A, B and C (the largest −6.0 and +5.8 dB, Upright B's top keys), as the grands' did before.
  `[even_levels] window_keys = 9`: after the export 0.32, 0.49 and 0.16 dB (at most 1.9).
- **The treble** (step 143): within a treble note (C5 up) Upright A's 4 kHz sat +18.0 dB over its
  recordings at mf (+8.7 at ff), Upright B's 2, 4 and 8 kHz +8.1, +11.1, +19.8 (ff +7.3, +7.2, +10.9),
  more than any grand's before its trims (+4–7); Upright C's +1.2 (+5.5), where the grands' trims
  stopped. `[board] trim_db` rows from C4 to C5: Upright A 4 kHz −10, 8 kHz −5 dB, to +11.9 (+1.1) at
  4 kHz; Upright B 2, 4, 8 kHz −5, −7, −12, to +5.1, +5.3, +9.3 (ff +4.4, +1.3, +1.9). Heard as "less
  glassy … definitely an improvement". What Upright A keeps at mf, 11 dB more than at ff, read as
  the felt's (step 146): the measure's, step 169.
- **The lowest octave** (step 147): Upright C's board fit holds its response flat below 55 Hz, and
  its fundamentals of A0–G♯1 sat 9.9 dB over partials 2–4 (F1 16 dB). 31.5 Hz −12 dB (least squares
  over the keys; a 45 Hz row would also cut B1–G♯2, already 6 dB under) brings them to −0.2 dB.
  Hardly heard; kept, as the grands' unheard trims, nearer the recordings. Upright A's lie at −1.7
  dB; Upright B's B♭0 stays +15–24 dB, its rows at the bound since step 152.

The report covers the uprights too (`just report --uprights`, `proto/out/report-uprights/`: their
envelope from the three upright sets, thin), and both kinds' pages carry the tone per register,
today's and each snapshot's or variant's, beside the bass attack. The exports equal the variants
heard (data.bin and level.json byte for byte).

The rolled pianos' realism ranges include the uprights: with them voiced, a roll drawn at warmth 4.5
fails only what no pull-back mends (Player against Recording at 0.7–1.4 kHz, the ring at 60 ms), and
the pull-back's test draws at warmth 10 (pulled back to 7.5). Upright A's golden renders moved
−0.14 … +0.16 dB in energy.

<a id="step-169"></a>
## 169. The uprights finished

Step 168 left two items: Upright A's treble at mf 11 dB over its ff against the recordings, and
Upright B's B♭0 at its trims' bound. Neither needs a change.

- **The measure counted the fundamental's own band.** Within a note each band is re the band holding
  the fundamental, which reads 0 by definition; in the treble the top keys' fundamentals fill the 2
  and 4 kHz bands, and their zeros pulled the register's median toward 0, at ff more than at mf (ff
  recorded on 4 keys more, 99–105). `tone_check.py` leaves that band out. Per key, Upright A's mf lies
  1.6 dB over its ff at 4 kHz (the median over the 13 keys recorded at both), both about +7 dB over the
  recordings: no change with the dynamic, nothing for the felt.
- **Against the grands**, on the corrected measure (engine − recording, mf / ff): the treble's 4 kHz
  −0.9 … +7.5 dB on the seven grands, Upright A +11.9 / +3.8 (per key about +7); its 1 kHz (partial
  2 of C5–F♯5) −3 … +12.5 on the grands (Grand F, heard as right, +11.6 / +12.5), Upright A +11.1 /
  +4.9, C +11.8 / +13.4; the middle's 250 Hz −12 … +5 (Grand C −11 / −12), Upright A −14.6 / −15.6.
  The uprights lie at the grands' edges, where the grands were left. Grand A's treble at 4 kHz reads
  +4.3 / +3.0 (step 168's +1.8 counted the zeros).
- **Upright B's B♭0 is the recording's.** Its fundamental re partials 2–4 lies at −49 dB (mf) and −59
  dB (ff) in the recording, the weakest of every set (A0–B0 elsewhere −30 … −49 dB, Upright A
  −30 … −41, Grand A −43 … −45), and no other set's ff falls 10 dB under its mf there; the engine's
  −34 / −36 dB lie within the other recordings. Cutting further would follow a recut, normalized
  file and thin a bass the user once heard as missing.

The uprights' roadmap item closes.

<a id="step-170"></a>
## 170. Live re-strike

The hammer's contact ran against strings at rest, its result added to the ringing ones: a re-strike
was a strike from silence on top of the ring. The strings are linear and only the felt is not, so
the one thing that shortcut gets wrong is the felt's compression: it measured the hammer against a
string at rest while the string at the strike point was still swinging.

Measured first in the prototype (Grand A, its felt as exported; C2, C4, C6; every pair of pp, mf,
ff at 0.4, 1.5 and 5 m/s; 50–400 ms apart; 8 phases over a period; the damper up), against a contact
whose felt sees the strings' displacement from the running eigenmodes:

| re-strike | string speed at the strike point ÷ hammer's | peak force re today | its own sound re a fresh strike | the total after (old + new) re today |
|---|---|---|---|---|
| softer into louder (pp into ff) | 2–10 | 2–12× | +8 … +20 dB | about −0.5 dB, ±2 by phase; octaves of partials up to 5–12 dB |
| the same dynamic | 0.3–0.8 | 0.5–1.8× | −5 … +4.5 dB | ±1–3 dB by phase; up to 5–11 dB at 50 ms |
| louder into softer | ≤ 0.2 | about 1× | ±1 dB | within 1 dB |

Less with the gap and up the keyboard (the treble decays sooner). No player times where the string
is when the hammer arrives (a period of C4 lasts 3.8 ms): each repeat comes out a little different.

**The engine** (`voice.rs`): the note keeps the unison's eigenvectors (`Polarization::v`); per mode
and string the voice holds state → displacement, v_ie / (bridge_j · colsum_je). While a contact runs
it follows the running modes from their exact state (`Modes::ring_start`, `ring_step`) and adds each
string's displacement at the strike point to the felt's compression, over the strike gain the modes
carry. The hammer starts at the rest plane as from silence, or at the nearest string where one
swings towards it (else it would start compressed); the contact's longest time counts from its
first touch. Its result is added to the running modes as before. The first cut started the hammer
short of the strings' whole reach (Σ |·| over the modes): a soft C2 into a ringing ff one started
2.6 mm out, touched after 14.6 ms and was cut at the contact's 20 ms, the repeats lost.

Checked: off, every strike as before bit for bit (the engine suite); on, a strike into silence the
same bit for bit; Grand A's C4, ff then pp 100 ms later over 16 phases, the felt's peak force
2.4–8.3× a fresh pp (the prototype 1.2–7.7×); single re-strikes on C2 and C4 within ±1–2 dB of today
by phase. Rendered (Grand A): soft C4s into a loud chord, the pedal down, the attacks ±0.5 dB (the
chord's ring dominates); a trill and a repeated A4 at 12 per second up to +1.9 dB; a C2 struck ever
softer every 200 ms, each strike adding a little less than today's sum, −2.6 dB by the tenth. No
clicks. The cost: Liszt's Faust Waltz (MAESTRO, 9.4 min) 8.47–8.93 s off and 8.49–8.90 s on.

In the renders the user heard a difference only "listening very closely"; playing, clearly, and it
can get annoying: useful in some pieces, so a switch, off by default: **Live re-strike** on the
Touch tab (`restrike`), shown where a hammer strikes (the pianos); `midi2wav --param restrike=1`. The bars' mallets and the plucks start from rest: a mallet's
contact or a re-pluck would be their own work.

<a id="step-171"></a>
## 171. Live re-strike for the mallets

The bars start each strike from a recipe (`physics::pluck_amplitude` with `bar`): per mode an amplitude
∝ the speed through a lowpass whose cutoff rises with it. A contact simulation was tried when the bars
were fitted and left their upper modes 20–30 dB off (the note in `proto/bars.py`); fitted again now
against the recipe at the anchor speeds, a mallet on a felt law came within 13–16 dB rms where the
cutoff climbs steeply (the vibraphone 2.6, the marimba's cord 3.0 and rubber 2.5 octaves per doubling
of the speed: a contact gives less than one), 4–6 dB with a core under the felt, 9 dB the balafon.
Replacing the recipe would change the bars' sound from rest.

So the strike stays the recipe's and the mallet meets the bar at the speed between the two: a mallet
(tens of grams) against its bar (0.08–0.96 kg, `bar_mass`), in contact briefly, is set by the
relative speed first. The bar's modes are its velocity at the strike (a strike from rest gives each
mode a velocity ∝ ω·a): one factor per strike maps the running modes to the surface's displacement and
velocity, so that today's strike at the mallet's speed v gives the surface c·v, c = (1 + e)·m /
(m + M) (m 35 g, e 0.5, M the bar's modal mass); a bar with a resonator adds its bank's fundamental.
The mallet arrives at the rest plane at the key's moment: a bar on its side meets it then, one swung
away it chases for up to 20 ms. At the relative speed u = v − ẏ the collision changes the surface's
velocity by c·u = c·v − c·ẏ: the running modes lose c of their velocity there (displacement kept),
and today's strike at v pushes, as bright as one at u (4 × v at most, 5 % at least). Left out: the
contact's shape (a double bounce, the bar giving way) and the meeting's few ms (the strike sounds at
the key's moment). From rest, the strike as before bit for bit (a test); the kalimba's tines are
plucked by the thumb and keep the recipe.

The first cut played today's strike at u (its push c·u) and took M alone (a treble bar of 39 g modal
mass weighs about what the mallet does: its surface then left faster than the mallet came). The user
found it running away with an arpeggiator and the sustain latch, until a MIDI panic. Reproduced: a
bar struck 12 and 20 times a second, the dampers up, ran to infinity on the vibraphone's and the
crotales' treble; with m + M and the push bounded by the momentum it climbed 9–18 dB over the roll
without the switch, and capped at twice the speed still 15.5 dB after 10 s (the vibraphone's C8): the
recipe puts a bright strike's push into the upper modes and left the swinging fundamental unslowed,
so every hit into it was hard and paid for nothing. With the brake: over 20 s the vibraphone's C8
0.8 dB, the crotales' C7 2.1–2.8 dB over the roll without it, settled; a test strikes six bar
instruments at four keys, soft, loud and in turn, 12 and 20 times a second (within 6 dB).

The marimba's C4, ff then pp 100 ms later: the mallet meets the bar at 0.54–1.58× its own speed (the
surface's speed right after a strike lies mostly in the upper modes, decayed by then). Rendered: a
marimba roll on one bar and on a chord 1.6 dB quieter than without (each hit takes some swing out),
the difference −10 dB under the render; the vibraphone's soft repeats into a pedalled chord and the
xylophone's repeats at 14 a second −30 and −21 dB, their levels within 0.2 dB; no clicks.

The same switch, Live re-strike (`restrike`), now shown for the mallet instruments too.

<a id="step-172"></a>
## 172. The runaway guards

The mallets' first re-strike (step 171) ran away until a MIDI panic: "some really unpleasant sounds"
(the user). A fault like it should silence itself. Two guards:

- **A voice's.** Each bank of modes keeps a ceiling, 30 dB over the largest single strike it got
  since it was last silent (`voice::RUNAWAY`; strikes in random phase pile up about √N, a roll of 20 a
  second on a long-ringing bar ~12 dB, all in phase ~23 dB). At each block's end, after the
  synchronization that touches every state anyway, a state past it or not finite silences the voice
  (its modes, contacts, noises) and counts it. A state blowing up between strikes, as a numerical
  fault would, trips it; a runaway the strikes themselves feed raises the ceiling with them.
- **The output's.** A sample past +20 dBFS or not finite clears every sound at once (a panic without
  its fade, which would play the burst out) and counts it. What is played stays well under it: the
  loudest instrument peaks near −1 dBFS, about +11 at the Volume's +12 dB.

Tested: a voice's state set to twice its ceiling, to infinity, to NaN, silenced at that block's end;
a Volume of +60 dB standing in for a runaway, cleared at the first sample past the guard and silent
after; every exported instrument struck hard (soft and loud in turn, 16 a second on four keys, the
pedal down, at the Volume's top), Live re-strike on and off, trips neither. Free: one comparison per
mode and block, and one per output sample.

<a id="step-173"></a>
## 173. One loudness for every instrument

Switching from a grand to the vibraphone, the marimba or the harpsichords always took the Volume
down (the user). The engine calibrates every instrument by its ff peak (the median key at −15
dBFS), and a struck string's peak is a short transient over a falling tone while a bar or a
harpsichord holds its level: at one peak the held tones sound louder. The hand-set gains (the
bells, the glockenspiels, the crotales, Harp A, Grand G) came from demos' peaks.

Measured (every exported instrument at its defaults, EBU R128 through ffmpeg's ebur128): single
notes every third recorded key at velocities 64, 100 and 127, and a phrase in the middle register
(a two-octave arpeggio at 80, four chords at 95, the notes released). The phrase against the grands'
median (−28.9 LUFS): the marimba +14.3 dB (its peak +1.7 dBFS, clipping), the vibraphone +12.4, the
harpsichords +3.9 … +9.9, the hand chimes +9.2, the kalimba +8.6, the psaltery +5.9, Harp A +5.2, the
clavichord +4.0, Grand F +3.2; Grand G −8.2, Glockenspiel A −8.1, the crotales −6.0, the strumstick
−4.2, the guitar −3.1, the balafon −2.9; the other pianos within 2 dB. The single notes agree but for
the dynamics: at velocity 64 a piano sits low in its 45 dB, a harpsichord plucks as at 127.

Each instrument more than 1 dB off moved by its offset in 0.5 dB steps (26 projects, `output_gain_db`
or a bar's `gain_db` in `[export]`, the reason as a comment). Grand G's −10 dB came from its mf lying
only 4.4 dB under its ff; since the velocity curve (step 124) each piano plays through its own level
table, and it had left it 10 dB under the other grands at every velocity: now −2. After the
re-export every phrase lies within ±0.2 dB of the grands' median, the phrases' peaks −17.2 … −6.3 dBFS
(before −21.8 … +1.7). Only the gain changed (and the level tool's ff level with it; the pianos'
level tables within 0.0001 dB), except Harpsichords C and D: step 150's loss past the measured keys
had not reached their exports, and now does (C's top octave's loss down to a quarter, D's up to
+36 % from C6).

The editor's comparison (`compare_one`) flagged three of the marimba's pp treble notes 11–17× long
after its −14.5 dB, its renders exactly that much quieter: a measure's floors are absolute (−100
dBFS for a note's fall, −60 for an unmatched partial), and those notes' heads fell under them. A
render's shape is now measured scaled to its recording's level, its level as rendered; the corpus
flags as many or fewer pairs than before (Grand B 46 → 37, Upright A 13 → 9, Upright C 4 → 1, the
marimba 5).

<a id="step-174"></a>
## 174. The Volume to +24 dB

With every instrument at the grands' loudness (step 173), +12 dB was too little for some pieces (the
user): the Volume now reaches +24 dB. The output guard (step 172) rises with it: +20 dBFS × the
Volume above 0 dB, so its margin over the instrument's own level stays (a phrase's peaks −6 … −17
dBFS at 0 dB). Every instrument struck hard at +24 dB trips neither guard; an instrument 60 dB over
its calibration stands in for a runaway in the guard's test.

<a id="step-175"></a>
## 175. Presets

The roadmap's presets, agreed with the user (spec 2026-10-27-presets-design): one mechanism, a preset
holding the values of what it covers, picking one changing only that. Two kinds:

- **Microphone presets** (in the Mic position list, after the positions; first a row of their own on
  the Mics tab, which the user found less intuitive than the positions' list): the microphones only (the position, its distance
  and spacing, the Custom pair, Distance level), not the lid, the pillow, spread keys or the room, which
  other instruments lack. A Custom position is in the body's own coordinates, so a microphone preset
  belongs to a body type: one saved on a grand is offered for every grand. Shipped (as data in the
  plugin, `Presets.cpp`), from the engine's own pairs as anchors (the player's ears at (0, −1.25,
  0.45) m on the grand, (0, −1.0, 0.45) on the upright): the grand's ORTF at the player's seat, one
  jazz-style cardioid near the tail (both microphones at one point), a spaced omni pair over the
  strings, a concert pair at 4 m; the upright's pair over the open top, pair behind the board (the
  wall at 1.2 m), ORTF. Each is a Custom pair the engine's clamp keeps as it is (a test).
- **Full presets** (the Sound tab's first row): every parameter and the sound-carrying states (the
  instrument, the velocity curve, the morph, the felt, the Scala file); picking one switches the
  instrument. None ships yet: the first is the Production piano, to be exported as a DPF program too
  (DPF writes every parameter and state into an LV2 preset, the instrument included).

The user's presets are files in `~/.local/share/pianoengine/presets/`, `key = value` lines,
parameters by their symbol (a reordered or added parameter reads back right, an unknown one is
skipped): every host and the standalone share them. Save as… types the name in the row (Enter); a
Delete for the user's preset shown arms with a first click (3 s, as the menu's Reset). A row names the
preset the controls still match, the user's before a shipped one (saved under a name, it shows that
name), else None. A short note confirms a save or a delete, or says why a save failed.

On screen (Xvfb): the ORTF preset sets Custom and its pair; Save as… writes the file with every
microphone setting; it lists after a restart; its delete asks twice; a full preset saved on Grand A
brings Grand A back from the vibraphone with its settings.

<a id="step-176"></a>
## 176. The plugin starts at Player

Player (the pianist's ears at the keyboard) sounds much better than Recording (the user), so the
plugin's Mic position starts there; `midi2wav`, the fits and the tests keep Recording (the fitted
sound, what the recordings are compared with), and Copy CLI writes `mic=1` for it.

The loudness of step 173 was set at Recording. The phrase at Player against Recording (EBU R128):
within ±1 dB for 21 of 33 instruments; Harp A +2.6 dB, Harp B +2.5, Upright C +2.2, Grand D +2.0,
Grand G +1.4, Harpsichord B +1.0; the vibraphone −2.1, the dan tranh −2.1, Glockenspiel B −1.8,
Grand C −1.7, Grand F −1.5, the clavichord, the kalimba and the psaltery −1.2. Left as it is for now.

<a id="step-177"></a>
## 177. Spread all keys on, the loudness re-matched

The plugin now starts with Spread all keys on (the user); `midi2wav` keeps it off. It barely moves
the loudness: the phrase at Player within ±0.4 dB but for the hand chimes (+2.3 dB, their middle keys
spread nearer the microphones), the xylophone (−1.3), the bells (−0.9), Glockenspiel B (+0.9) and the
crotales (−0.8).

Step 173 matched the instruments at Recording; at the plugin's default (Player, spread on) the phrase
lay −1.9 … +3.0 dB from the reference (−29.1 LUFS, the grands' median of the midpoint below), 18 of 32
instruments within ±1 dB. Each instrument is now matched at the midpoint of the two positions (the
mean of their LUFS): the eight whose midpoint lay 0.75 dB or more off moved in 0.5 dB steps (Harp A,
Harp B, Upright C, Grand D and the hand chimes −1.5 dB, Grand B −1.0; the vibraphone and the dan
tranh +1.0), the rest within ±0.5 dB left alone. After the re-export: Recording −1.3 … +1.2 dB (26
within ±1 dB), Player with spread −1.5 … +1.5 (22), the midpoint −0.6 … +0.7 (all 32). Only the gains
changed (the pianos' level tables within 0.0001 dB).

<a id="step-178"></a>
## 178. midi2wav plays the Dynamics

`midi2wav` turns each velocity into a hammer speed with a probe engine built before the render, and
that engine never got the `--param` values: `--param dynamics` changed nothing (a render at 15 dB and
one at 45 dB were identical to the sample), so a Copy CLI line rendered the default 45 dB whatever the
plugin's Dynamics. The probe now gets the params; a test renders Dynamics 15 against the default (the
velocity-90 notes' peak up more than 2 dB). The plugin was never affected. Found while measuring the
production presets (step 179).

<a id="step-179"></a>
## 179. Production presets

The roadmap's production piano: the pianos sound like concert pianos, and pieces in an arrangement
work badly on them. Measured against two General MIDI soundfonts' pianos (program 0, fluidsynth
without reverb and chorus): FluidR3 and SGM (the user's favorite), on single notes (every sixth key,
velocities 16 … 127) and on four pieces (a Mozart sonata and a Chopin ballade from MAESTRO, the
boogie and the pop ballad of the demos), our grands at the plugin's defaults.

- The per-note dynamics are not the gap: from velocity 16 to 127 ours span 29–32 dB, FluidR3 36 and
  SGM 35. Both soundfonts keep one timbre at every velocity (FluidR3's centroid the same to the hertz;
  SGM's bass 339 … 406 Hz), where ours turn dull when soft.
- FluidR3 is bright: on the pop ballad 2–4 kHz lie −14.2 dB re the total, above 4 kHz −21.7 (Grand A
  −24.4 and −40.2, the other grands down to −50); ours match their own recordings there within a few dB
  at mf and ff (`tone_check.py`), so FluidR3's piano is recorded closer and brighter. Grand A at Close 1
  with Hammer hardness +12 dB and Room −12 dB lands on it: centroid 485 Hz against 469, 2–4 kHz −14.0,
  above 4 kHz −22.2.
- SGM is dark (above 4 kHz −41 … −47 dB) with fuller low mids (200–500 Hz −3.0 … −4.2 dB, ours down
  to −8) and a narrower loudness range (EBU R128 LRA 13–16 LU, ours 14–20). Grand B at the default
  position with Dynamics 25 dB lands on it: the pop ballad's LRA 13.9 LU against 14.0, 2–4 kHz −24.8
  against −24.7, above 4 kHz −47.1 against −46.9; the ballade 15.3 LU against 16.1. A quieter room moved
  it away (brighter, less bass).

Two shipped full presets, by ear (the user: fitting): **Production, bright** (the Grand A setting)
and **Production, warm** (the Grand B one). They are files in the repository (`plugin/presets/`, the
user's preset format; they name instruments, which the plugin's code may not), embedded by the build,
and list only what differs: a full preset now sets every other parameter and state to its default (a
user's full preset carries every value anyway). A **Default** comes first (every control and state at its default, the instrument
the menu's first: Reset keeps the instrument; the user wanted it in the list rather than only in the
menu), and the row names it while everything is at its default. The three are also the plugin's DPF
programs, Default the first (the JACK standalone loads program 0 at its start, which first started it
as Production, bright), so the LV2 bundle's `presets.ttl` lists them and Ardour shows them in its
preset menu. DPF writes every state into those presets, the window's (tab, zoom)
at its default: the UI now ignores an incoming default window state and hands its own back, so loading
a host's preset leaves the window as it is.

In passing: Grand G's treble keys 87 and 93 have most of their energy below their fundamentals (the
centroid 257 and 457 Hz at velocity 32); left for its listening pass.

<a id="step-180"></a>
## 180. Touch presets: closed

The roadmap's touch presets: "light/heavy action" curves and a "Grand A as recorded" curve from its
16 layers. The first are dropped: master keyboards have a light/heavy setting of their own (the user),
and a second one in the plugin would fight it.

For the second, Grand A's set as it plays itself (its SFZ through sfizz: 16 layers, `amp_veltrack`
73) against the engine at the Recording position, every sixth key C2–C6, the level over the first
second re velocity 127 (median over the keys):

| velocity | 4 | 16 | 24 | 32 | 48 | 64 | 80 | 104 | 120 |
|---|---|---|---|---|---|---|---|---|---|
| the set | −31.5 | −31.1 | −30.7 | −25.4 | −16.8 | −14.1 | −10.4 | −6.6 | −1.7 |
| the engine | −38.3 | −29.7 | −25.8 | −22.5 | −17.0 | −12.5 | −9.0 | −4.2 | −1.2 |

From velocity 16 up the engine lies within 2.0 dB RMS of the set, and Dynamics 45 dB fits the set
best (RMS 1.5 dB; 40 dB 2.8, 50 dB 1.8). sfizz reads `amp_veltrack` as a mix of linear amplitudes,
step 124 as a scale on the dB; above velocity 32 both give the 45 dB. The set floors at about −31 dB
below velocity 26 (its softest layer, 20 dB under its loudest as recorded, with the tracking's least
gain), where the engine keeps falling. A curve that matched would copy the set's floor rather than a
piano, for about 2 dB elsewhere: not made. The engine's spectral centroid lies under the layers' at
every velocity (0.72 of it at velocity 4, 0.92 at 127; the recordings' noise floor may account for
part of it), which no velocity curve changes.

<a id="step-181"></a>
## 181. Grand G's treble: its board's hump moved into the correction

Step 179 found Grand G's treble keys 87 and 93 with most of their energy below their fundamentals
(the spectral centroid 257 and 457 Hz at velocity 32). By key, the share below the fundamental in
the first 50 ms of the strings' part rose from −22 dB re the total at G5 to about 0 dB from D6 up
(Grand A and Grand E −21 … −32 dB throughout); the room then carried it for a second (the room's
part at key 87: 87 % of its energy below the fundamental).

What it was not: the hammer's impact (its gain at 0 changed nothing), the felt (Grand A's on the
treble changed nothing), the strings' data (Grand A's changed nothing), the room's long decay at
160 Hz alone (2.2 s; the recordings ring there 1.6–2.2 s consistently over 28 notes, measured over
10 or 15 dB of the decay curve: their own resonance, not the noise floor), nor the transfer
correction's gains on the contact's static share (the string's own gains below the fundamental, tried
as an engine switch: 1 dB; reverted).

What it was: Grand G's fitted board response lies 100–300 Hz 38 dB over 1–1.6 kHz (Grand A +10,
Grand B −8), with a hole from 800 Hz to 6 kHz down to −28 dB re its median; the per-key correction
lifts the treble's partials back by 8–26 dB. The correction acts on the partials only, so whatever
else a treble note sends to the bridge (the onset of its partials, the felt's push, the impact) came
out through the hump. Cutting the board's and the room's response 15 dB at 63–400 Hz in a copy put key
86's attack on its recording's (100–300 Hz re the total in the first 50 ms −5.8 dB, the recording
−6.3; the note band −3.8 against −2.7).

The export now moves part of a response into the correction (`[board] transient_db`, Grand G −15 dB
at 40–320 Hz): the radiation gets it (the body's response; the room on its own level `eq_db`, since
the room scales to unit energy after its spectrum; the body at the microphone positions on a new
`body_H_db`, which the engine reads for `mic::zones`, else `H_db`) and every key's correction its
opposite. The body's response comes at unit energy: without the hump it rose 1–8 kHz by `lift`, which
the correction takes off too and the room's gain gets, so the body's and the room's partials keep
their gains. Against the export without it, the partials of keys A0–C6 within ±1.5 dB (each key's
even level up to +1.3 dB: they were measured on peaks that held the thump).

Key 87 at velocity 60, re the total (the note band 1–1.6 kHz, then 100–300 Hz):

| | first 50 ms | 0.05–0.2 s | 0.2–0.5 s |
|---|---|---|---|
| Recording position, before | −13.1, −1.6 | −6.7, −4.1 | −1.7, −7.3 |
| Recording position, now | −2.8, −6.3 | −0.8, −13.4 | −0.2, −21.1 |
| Player, before | −11.9, −1.1 | −11.1, −3.6 | −5.9, −3.9 |
| Player, now | −2.9, −6.8 | −2.0, −9.7 | −0.6, −13.3 |
| the recording of key 86 | −2.7, −6.3 | −0.7, −9.9 | −0.2, −16.5 |

The phrase's loudness moved by +0.2 (Recording), −0.6 (Player), −0.8 (Audience) and +0.1 dB (Close 1).
Grand G's body is one of the synthetic pianos' references: their basis's first direction now holds
54 % of the spread (60 % before), and a mix of Grand G's with three others' responses puts its
deepest fundamental at −14.6 dB (−18), still lifted to −12 (the roll tests follow).

<a id="step-182"></a>
## 182. The harpsichords' release noise

The user: the harpsichords' release noise "pretty terrible", too loud, several clicks, on some keys
"more like a ghost whisper" (Harpsichord B's G4, Harpsichord D's F4).

The four harpsichords take their release noise from their sets' own release samples (`vcsl_keys`):
bursts at the envelope's peaks with their decays, a third-octave spectrum, a level re the note from
the sample's peak. The samples hold the string ringing out under the damper: Harpsichord D's for E4
95–99 % on the note's partials after 0.15 s. Measured as noise, it gave bursts decaying over
0.24–0.50 s colored like the note, which the engine played on top of the note's own damped decay (the
whisper), and its peak set the noise's level. The analysis now takes the note's partials out first
(STFT bins within 2.5 bins of the measured partials, then f0 and B's model above them, zeroed; frames
of 8 periods of the lowest), step version 3.

The sample for D's E4: peak −17.8 → −27.7 dBFS, five bursts (decays 15–391 ms) → two (5, 52 ms); B's
F♯4: decays up to 499 ms → 4–14 ms. As the engine plays them (velocity 80 at Player, the noise's peak
re the note's, its length to −30 dB, its share on the note's partials): B's G4 −24.8 → −25.3 dB, 601 →
259 ms, 35 → 0 %; D's F4 −26.3 → −37.8 dB, 601 → 292 ms; A's C4 −31.3 → −37.3 dB; C's samples held no
ring (unchanged). The clicks that stay (3–5 per release on A and C: the jack, the quill passing the
string, the damper) are the recordings' own, heard as fine.

Still too loud by default (the user): the median over C3–F5 lay at −24.4 (B), −25.4 (C), −25.5 (A) and
−33.7 dB (D) re the note's peak, the grands' at −40 … −44 (their trims). Each project's
`[release_noise] level_db` (−14, −12.5, −12.5, −4.5 dB) puts all four at −38: a harpsichord's action a
little more present than a piano's. The Release noise control adds up to +24 dB.

<a id="step-183"></a>
## 183. The unisons toward the top key

Two new references: Plath's dissertation measured a concert grand's unisons 1–2
cents apart up to F6 and up to 5 cents above; Tominaga & Sato's simulation detunes 0.3 / 1.5 / 2.5
cents (bass, middle, treble). Our pianos' unison detune is one scale per piano, measured from the
beats of keys A2–D♯6 (`[strings] detune`), and held above them: their top octaves at 0.9–1.4 cents
of spread.

`unison_check.py` measures the beats: per key from C4 up, the loudest layer's partials 1–3, each
envelope's strongest modulation (0.25–25 Hz, a quadratic trend removed) in cents of the partial,
against the engine's render of the key (room off). Pooled over the ten pianos (medians, cents):

| | C4–B4 | C5–B5 | C6–B6 | C7–F7 | F♯7–C8 |
|---|---|---|---|---|---|
| the recordings | 1.5 | 1.9 | 2.3 | 3.6 | 5.6 |
| the engine, before | 2.2 | 1.6 | 1.4 | 1.5 | 1.4 |
| the engine, now | 2.2 | 1.5 | 1.9 | 3.0 | 4.3 |

Each piano on its own rises the same way. The recordings' top keys carry a fast, shallow shimmer
(10–20 Hz, 3–6 dB, well over their noise); the engine's swelled slowly instead (1–2 Hz, dips of
10–20 dB): its top keys' unison beat lay under what the measurement sees, the slow swell was
something else.

`[unison] treble` (the export's; the fits never see it) is a factor on the detune along the keys,
log-interpolated: 1 at key 78, rising to the factor at 108. The first round took the ratio of the
beats from key 78 up (a line in log) and gave ×1.4–9.5; measured again, the engine's beats had moved
little: three strings beat at their neighbors' gaps, about half the spread, and before the ramp the
engine's top beat had not been the unison's. The second round multiplied by the new ratio: Grand B
×8.8, C ×6.7, D ×11.6, E ×10.9, F ×1.8 (its recordings end at key 101), G ×8.8, Grand A and the
three uprights at the bound, ×12 (Grand A's spread at C8 11 cents). Left after it: the recordings
over the engine ×0.85–2.3 at key 108 per piano (single keys beat ±50 % around their octave's line).
The keys' measured levels at ff moved by up to −3.4 dB at the top key (Upright A; Grand A −0.1, below
key 78 within 0.06 dB).

The engine's beats around C4 run faster than the recordings' (2.2 against 1.5 cents) on most
pianos: not the unison's detune (measured there), likely the polarizations' beat; not changed here.

Re-exported, the registers fixture (Grand A twice, stale since step 143) showed an old gap: an added
register's correction never took the main register's `[board]` trim and transient shift, though it
plays through that body; it takes both now (the trim after the even levels, as the main one's). Only
the fixture has a register on a trimmed or shifted body. The editor's comparison flags Grand C 23
pairs (21) and Grand E 58 (52): their top keys at mf 1–2 dB quieter re the recordings, over the
−12 dB line, and pairs at the edges as their medians moved.

<a id="step-184"></a>
## 184. The board's low modes ring

Plath's dissertation and Bader & Plath (2020) measured a grand's board glued into its
rim at T60 ≈ 0.5–0.6 s (Schroeder over its whole response; 0.3 s raw), before the frame and strings.
Our synthetic board's modes all decay at one rate, set by a constant modal overlap of 1 (fitted to
how irregularly the recorded partials decay): ~0.12 s each. A trial gave the board's modes a loss
factor where it rings them longer (`Board(eta_low)`; below overlap / (density·η): 450 Hz at 4 %,
900 Hz at 2 %). The board's own response, Grand A (Schroeder T60 per band):

| | 50–100 Hz | 100–200 Hz | 200–400 Hz | 400–800 Hz | 800 Hz up |
|---|---|---|---|---|---|
| the overlap alone | 0.15 s | 0.17 | 0.12 | 0.12 | 0.12 |
| η 4 % | 0.59 | 0.45 | 0.21 | 0.12 | 0.12 |
| η 2 % | 1.26 | 1.03 | 0.40 | 0.20 | 0.12 |

The partials' decay spread under 1 kHz (A2–A4, each partial's rate re its neighbors', median |log2|),
which the overlap was fitted to, stayed under the recordings' (Grand A 0.26 → 0.32 at 4 %, the
recording 0.44; Grand B 0.21 → 0.18, 0.41). After a treble strike at Player, the 60–400 Hz band re
the note over 0.05–0.5 s rose 5–17 dB. The user heard both as different, "maybe a bit better", and
could not decide between them; 4 % went in: it matches the measured board, which the frame and the
strings can only damp further.

`[board] low_eta` (the export's; the fits never see it), 0.04 on the ten pianos. Two consequences:

- A ringing board's band below 200 Hz holds one mode, and its two microphones' independent radiation
  draws set the pair's phase there by chance (Grand B's 63 Hz octave: the mono sum 12.4 dB under the
  channels' power; the rolls flagged their bass out of phase). Its low modes now radiate to both in
  phase (`coherent_low`: fully below 100 Hz, independent from 400 Hz): every piano's 63 Hz octave
  −0.5 … 0.0 dB (before −9.7 … −0.2).
- Narrower low modes dig deeper notches between them: a fundamental deeper than −14 dB in one gets
  the draw of 64 whose deepest is least deep (the export's `ringing_draw`): Upright A (−17.3 →
  −12.9 dB); the others keep theirs.

C2 on Grand A now carries its board's 59 Hz mode under it at −36 dB re the fundamental over its
first second (−55 before). The phrase's loudness (the midpoint of Recording and Player, step 177)
moved 0.0 … +0.7 dB per piano, the grands' median +0.25: no gains changed.

The rolls' body steering sets bands by smooth filters, which a band holding less than one sharp mode
defeats (Grand A's body moved fully to Grand E's: 3–8 dB off at 80–230 Hz): it now judges levels and
rates from 250 Hz up (`body::STEER_FROM_HZ`); the rolled pianos' lowest octaves' body color is
approximate. The pull-back test's donors' fault (the bass apart) is gone with the coherent bass.
Tests rebaselined: Player against Recording on the references ±5 dB (Grand A's 8 kHz octave −4.7),
the cardioid pair turned away 0.85 of the level toward the board (0.81), the audio input's tail
measured 0.8–1.3 s after the burst (the board's ring was in the old window), the editor's
comparison Grand A 13 pairs (C2's early decay; F♯3 mf's pitch read +25 cents with its partials
unmoved), the golden renders (−0.7 … +0.8 dB in energy by position).

<a id="step-185"></a>
## 185. Program changes follow their switch again

The user: the plugin reacted to MIDI program changes with "Program change selects the instrument"
off, and the UI showed nothing of it. Since the shipped presets became the plugin's programs (step
179), DPF's JACK standalone catches a program change N on channel 1 below the program count itself
and calls `loadProgram(N)` on the audio thread, before the plugin's MIDI route reads the message:
0 loaded Default (every parameter and state reset), 1 and 2 the Production presets; the standalone
tells its UI only of parameters a CC changed, so the window kept the old values. With the switch on,
the route then also chose instrument N.

`loadProgram` now ignores a call on the audio thread (`run()` records its thread): there it is the
wrapper's MIDI path, and the route answers the message as the switch says. A host's preset menu (an
LV2 host loads the bundle's presets itself; VST3 and CLAP hosts call it off the audio thread) and the
standalone's Default at its start (before the first block) load as before.

<a id="step-186"></a>
## 186. A concert grand's board ring at the player's seat, measured: 4 % kept

Plath's dataset (Zenodo, doi:10.5281/zenodo.3274772, CC BY 4.0): a concert grand, anechoic, a dummy head at the
player's seat, the bridge's acceleration, 88 keys × 5 takes at forte (a key's takes within 1.5 dB,
~80 dB SPL), before and after a year in a hall. Under a treble note (keys 68–88) nothing of the
strings lies below the fundamental, so what sounds there after the strike is the board (with the
frame, the case and the action's knock). The bridge's accelerometer sees none of it (below 400 Hz
70 dB under its peak, in its noise); the ears do: lines at 66, 94, 134, 193 Hz under C7, 18–24 dB
under the note's peak.

Per octave band, Hann-windowed spectra (band filters on the engine's noiseless renders leaked the
fundamental into the low bands' tails), the band re the note's total over its first 0.1 s (dB,
medians; the engine at velocity 100, Player, the room off):

| 125 Hz | 0–0.1 s | 0.1–0.3 | 0.3–0.6 | 0.6–1.0 |
|---|---|---|---|---|
| the concert grand (after the year, 105 takes) | −23 | −33 | −44 | −53 |
| Grand D before step 184 | −37 | −96 | −152 | −152 |
| Grand D at 4 % (step 184) | −27 | −41 | −64 | −150 |
| Grand D at 2.5 % (a 1 s response) | −27 | −35 | −50 | −70 |
| Grand A at 4 % | −44 | −61 | −88 | −152 |

The grand's ring decays ~55 dB/s at 125 Hz, ~70 at 250 Hz, ~150 at 500 Hz (T60 ~1.1, 0.9, 0.4 s; loss
factors ~1.6, 1.0, 1.1 %; Schroeder on the bands gave 0.9–1.4 s at 125–250 Hz, longer after the year
than before). The engine's body response ends at 0.5 s (`Board.impulse_response`), where a 4 % board's
lowest modes are 50–60 dB down; longer damping needs a longer response: 1 s cost 2–3 points of mean
load at Player (64 frames), the worst blocks unchanged; 2 s 6–8 points and blocks to 73–95 % of the
budget.

By ear, variants of Grands A and D at a 1 s response with each mode's T60 at most that: 1.5 %
"pretty terrible" (its first 0.1 s 10 dB quieter: the unit-energy response gave the ring more of it),
2.5 % clearly different but neither better nor worse over several pieces. 4 % stays: the difference
does not pay for the load and the re-export; the measurement is one piano, not ours, and neither
setting matches it at 250–500 Hz.

<a id="step-187"></a>
## 187. Player's bass in both ears alike

Plath's dataset (step 186) holds every key of a concert grand at the player's seat, a dummy head in
an anechoic room. Measured against the seven grands at Player (velocity 100, the room off; per key
the first second, octave bands, medians over the keys and the grands), only what the seat sets
carries over: the two ears' balance, how alike they are, the treble's fall-off.

The right ear's level over the left's (dB):

| | 63 Hz | 125 Hz | 250 Hz | 500 Hz | 1 kHz |
|---|---|---|---|---|---|
| the concert grand, keys C2–B3 | +1.3 | +1.4 | +1.6 | +1.2 | +0.3 |
| Player before, C2–B3 | +6.4 | +8.0 | +4.9 | +2.9 | −3.6 |
| Player now, C2–B3 | +1.2 | +2.7 | +1.3 | +3.7 | −2.6 |
| the concert grand, A0–B1 / C4–B5 / C6–C8 at 63 Hz | −0.1 / +1.3 / +1.0 | | | | |
| Player before | +6.0 / +6.4 / +3.7 | | | | |
| Player now | −0.3 / +1.3 / +2.0 | | | | |

The bass sat 6–9 dB to the right on every grand and key (the treble keys' low bands: the strike's
thud and the board's ring). Three causes, each found by mirroring the pair and closing the lid on
Grand A:

- The open lid reflected at every frequency, to the right microphone only (it is over the
  treble). A panel reflects from its size up: below Rindel's characteristic frequency
  c·d*/(2·(a·cos θ)²) (d* from the two legs, a the smaller side, θ the incidence) the reflection
  falls 6 dB per octave. At Player that is ~120 Hz for the treble keys (the lid overhead) to
  1.2 kHz for the bass (grazing). About 4 dB of the 125 Hz excess.
- Each channel was scaled to its fitted channel's energy, and each synthesized channel to unit
  energy first. Both channels share the board's bass (the radiation coherent by the spacing), so
  the recording's balance between its own microphones (Grand A's right one ~3 dB up in the mids)
  and the right channel's leaner mids each lifted the whole right bass. The pair's channels now sit
  at their mean level, normalized together.
- The right microphone's radiation, ρ·r₀ + √(1−ρ²)·r₁, keeps the energy only for an r₁ independent
  of r₀; since step 184 the export leans r₁ on r₀ below 400 Hz, and the mix summed to up to 1.4×
  (+3 dB). Signs alternating along the modes in frequency order make r₁ independent again at its
  size; drawn at random, they tipped a band of a few modes to one side, on every piano alike.

`mic::tests::a_players_pair_hears_the_bass_alike` (the old scaling: +7.3 dB below 160 Hz) and
`the_open_lid_reflects_from_its_size_up` hold them. The positions' golden summaries moved −1.0 …
+1.6 dB in the two keys' energy (the draws), each position's level still the recording's total
over the zones. Recording is unchanged.

What stays: 125 Hz on the middle keys at +5 dB (the lid at ~120–250 Hz there, a flat 1.45 m
rectangle; whether Plath's lid was open the data does not say). How alike the two ears are:
0.99/0.91 at 125/250 Hz at the dummy head, 0.9–0.99/0.5–0.7 at our 0.5 m pair, as a spaced pair
gives (the head's 18 cm and shadow). Above 500 Hz the head leans right 2–3 dB (shadow and lid); our
pair wanders ±3 dB. The treble at 4 and 8 kHz re 0.5–2 kHz: the concert grand's seat sits with
Grands A and F; Player is within a few dB of Recording on every grand, and without a close
microphone on the concert grand the seat's own fall-off cannot be told from the piano's.

In a piece with the room on (the boogie at Player), the bass's balance is mostly the room's. Its
draw is every instrument's (seeds 11 and 21) and puts the right channel 2.5–4 dB under the left at
125–250 Hz where the piece's notes lie, as a room's modes differ between two points; balancing the
draw's octaves moved that ~1 dB and was not kept. Below 250 Hz (63/125/250 Hz, dB), Grand A
+1.5/−1.3/−2.8 before, −1.5/−2.6/−2.4 now; Grand D +2.2/+2.5/+0.6 before, −0.6/−3.2/+0.7 now.
By ear on headphones (the boogie at Player, before against now), the bass sits better, most
clearly on Grand D: its 125 Hz band moved 5.7 dB, Grand A's 1.3 dB.

<a id="step-188"></a>
## 188. The lid taken off

A grand's lid is taken off in studios (the microphones go where they like), in small rooms and
jazz clubs (an even sound) and on stages (the player sees the band). The engine had the lid from
closed to full stick only. **Lid removed** (Mics tab, off by default) takes it off: in the engine a
lid above 1 is `mic::LID_OFF` (2; `lid=2` for midi2wav), which drops the lid from the reflectors
(it comes last, so the floor's and the wall's indices stay), leaves out the closed lid's loss on
the paths and the room, draws no lid and lets a free microphone hang anywhere above a grand's
case. The positions stay the open lid's. The plugin's switch sets the engine's lid to 2 and leaves
the Lid slider's value alone (`engineLid`). It shows where the Lid does: the grands, the uprights,
the harpsichords, the clavichord.

The change with the lid off, the boogie, the level kept (Distance level: total; dB per octave
band, 63 Hz … 8 kHz, both channels):

| | room | Player | Audience |
|---|---|---|---|
| Grand A | off | +0.2 −0.2 −1.4 +0.9 −0.5 −0.2 −0.1 −0.4 | −0.7 −1.7 −2.8 +0.4 +0.4 −0.8 +0.1 −6.2 |
| Grand A | on | +0.2 +0.1 0.0 +0.5 −0.2 +0.1 −0.1 −0.2 | +0.5 +0.4 +0.4 +0.4 +0.2 +0.3 +0.1 −2.6 |
| Grand D | off | +0.2 −0.2 −1.7 +0.1 −0.8 −0.6 −0.9 −0.7 | −0.7 −1.8 −2.6 −1.1 −0.3 −1.5 −2.3 −5.8 |
| Grand D | on | +0.1 0.0 −0.5 +0.4 −0.3 −0.5 −0.6 −0.5 | +0.4 −0.1 +0.2 +0.3 +0.4 −0.3 −0.7 −3.3 |

Audience loses the treble the lid sends its way; Player, beside the lid's hinge, little. Upright
A's open top lid leans back, away from Player: nothing changes there, and at Audience ≤ 0.3 dB.
`a_lid_taken_off_reflects_nothing` checks the reflectors, the paths at every preset, the leak, the
drawing and the clamp.

<a id="step-189"></a>
## 189. The harpsichords' keys reversed

Many harpsichords have dark naturals and light sharps (ebony and bone, most of all the French
ones). The menu block's `reversed_keys` (the engine's report: `Menu::reversed_keys`,
`PeInstrumentInfo::reversed_keys`) has the plugin's strip draw them so; the four harpsichords set
it. Their naturals are a dark brown a little lighter than the panel, so the gaps between them
stay visible. The exports changed by that key alone.

<a id="step-190"></a>
## 190. The phantom partials, a defect in the bass

Turning up Condition made some keys much louder than their neighbors. The phantom partials
(`level·v²/F`, the bridge force squared over an ff E1's) were the Wear control's on every key, while
a piano's own stop at C3 (step 131); a treble key's force is far above the E1's, so its partials
swamped it. All keys at velocity 100 and 127 (the room off), the level over the first 0.5 s, phantom
wear 1 (Broken) against 0, dB, median/max:

| | vel | A0–B1 | C2–B2 | C3–B3 | C4–B5 | C6–C8 |
|---|---|---|---|---|---|---|
| Grand A | 127 | +3.8/+6.2 | +7.7/+11.7 | +8.9/+11.4 | +9.1/+13.8 | +4.2/+16.4 |
| Grand D | 127 | +4.1/+8.1 | +6.6/+10.5 | +12.5/+17.0 | +13.4/+19.9 | +4.4/+15.7 |
| Upright A | 127 | +5.0/+11.2 | +9.2/+18.2 | +13.4/+20.6 | +14.8/+21.3 | +20.9/+33.1 |

Worn (wear 0.2) already put single treble keys of Upright A 12–19 dB up at velocity 100–127.

The control leaves the Condition: **Phantom partials** is a defect (Extras, 0–100 %, off by
default; the engine's `Params::phantom`, `phantom` for midi2wav). The instrument's and the
control's partials play in full up to C2 and fade to none at C3 (`PHANTOM_BASS`), not a cut, so
B2 → C3 makes no step at 100 %. At 100 % now (velocity 127, dB, median/max): Grand A A0–B1
+3.8/+6.2, C2–B2 +3.1/+7.3; Grand D +4.1/+8.1, +2.5/+8.4; Upright A +5.0/+11.2, +4.4/+7.2; nothing
from C3. At the defaults nothing changes but Grand A's own partials on C2–B2 (≤ 0.1 dB). Condition at
Worn or Broken no longer adds them. Grand A's level table moved on C♯2–B2 with it (up to 0.8 dB at
the hardest strike; the table makes up for it when played).

The visibility table also caught the Condition tab's short names, a list in enum order: without
the phantom its last entry named the soundboard "Phantom partials"; a `static_assert` holds the
list to the enum.

<a id="step-191"></a>
## 191. The mallet cuts no fundamental deeply

The Condition's measurements per instrument (all keys at velocity 100, each wear aspect alone at
full) found the crotales' top keys blowing up under a harder mallet: D♯7–C8 rose up to 39 dB with
the hardness +12 dB (Mallet hardness at its top, or the hammer wear at Broken), and at velocity
100 the engine went silent on D7–C8. The bars' fit (`proto/bars.py` `fit_mallet`) finds the
mallet's lowpass from the level changes between the recorded dynamics, a few hand-played takes;
on the crotales it let the cutoff fall on the lighter discs (`beta` −0.8) to 2.1 kHz at C8, under
its 4.2 kHz fundamental: a 43 dB cut at mf, which the modes' levels made up. A harder mallet moves
the cutoff up, the cut goes, the make-up stays. The tubular bells did the same above their
recorded keys (−0.32, a 58 dB cut at C8), the vibraphone's yarn mallet within its recorded ones
(17 dB at F6).

The first mallet's cutoff on each bar is now raised where it would cut the fundamental at mf by
more than 3 dB (`FUNDAMENTAL_CUT_DB`, `cutoff_scale`; the other mallets scaled alike, as the engine
scales them by the note's f_q), in the fit, the levels and the export alike; every bar
instrument refitted (`bars`, `export_bars`). Bounding `beta` at 0 instead (a lighter bar, a shorter
contact) moved the tubular bells' fit to another of equal error (6.2 dB) whose soft notes came out
18 dB quieter and an octave duller; the floor alone leaves their recorded keys as they were. The
fits' errors moved ≤ 0.9 dB (the crotales 7.8 → 8.0, the vibraphone 8.5 → 8.8, the marimba
6.6 → 6.3).

What +12 dB of hardness adds to a recorded key's early level (0–0.3 s), median/max, dB:

| | vel 40, before | vel 40, now | vel 100, before | vel 100, now |
|---|---|---|---|---|
| Crotales | +21.4/+39.1 | +7.0/+9.7 | silent from D7 | +1.6/+2.8 |
| Vibraphone | +10.2/+20.0 | +5.4/+9.1 | +2.3/+6.4 | +0.5/+1.0 |
| Marimba | +1.9/+8.0 | +2.9/+4.8 | +0.8/+2.9 | +1.0/+1.4 |
| Balafon | +0.7/+13.4 | +2.1/+8.6 | +0.6/+11.9 | +2.1/+8.7 |
| Tubular bells, above the recorded keys | +2.4/+52.1 | +2.4/+6.3 | +0.4/+35.5 | +0.4/+1.5 |

At the fitted hardness, the recorded keys against before (median, largest): the crotales +1.7 dB
(+3.6 at C8) at velocity 100, +2.3 (+9.4 at C8) at 40; the vibraphone −0.4 (−2.4 at F6) and +0.4
(+5.8 at F6); the balafon, marimba and bells within 2.8 dB; the glockenspiels, xylophone, hand
chimes, kalimba and the tubular bells' recorded keys unchanged. The tubular bells' keys above
F5 no longer fade to silence at soft strikes (C8 at velocity 40: −104 → −54 dB, as their
recorded keys). The bells, glockenspiels and xylophone, whose cutoffs lay well above their
fundamentals, still gain up to 9–17 dB on single keys: brightness in their upper modes.

<a id="step-192"></a>
## 192. The bars wear as bars

The Condition's wear was a piano's on every instrument. On the bars, discs, tubes and tines the
soundboard's wear multiplied each mode's own loss by 2.5 (`BOARD_LOSS`): the balafon died within
1 s at Broken (decay at 1 s, median −105 dB re Mint) and lost a median 16.5 dB already at Worn;
the marimba's and xylophone's worst keys lost 94–97 dB at Broken. The tuning drift was a string's
on all of them (σ 40 cents at Broken, a median 31 cents per key on the metal bars too), and the
strings' and false beats' wear scattered and split the modes.

The wear now goes by the instrument's kind (`Wear::on`, from the body): bars have no soundboard,
strings or false beats to wear; metal ones (vibraphone, glockenspiels, bells, crotales, tubular
bells, hand chimes) keep their pitch; wooden bars (marimba, xylophone, balafon) drift a quarter
as far as strings (σ 10 cents at Broken: the wood ages, an old marimba is retuned); a kalimba's
tines slide in their bridge and drift as strings do. The engine's report says which aspects do
nothing (from `Wear::on`), so the plugin hides them there.

At Broken now (all keys at velocity 100, the room off; against Mint, median [5th percentile]):
the balafon's decay −1.4 [−7.6] dB (before −105 [−125]), the marimba's −0.1 [−3.3] (−20 [−97]),
the xylophone's −0.3 [−2.6] (−8 [−94]); what is left comes from the mallets' and the voicing's
wear. The drift per key: the wooden bars 8 cents median, the kalimba 30, the metal ones none.
The dampers' wear (the vibraphone, tubular bells and hand chimes ring on 6–8 dB after the
release at Broken), the mallets' and the voicing's stay as they were.

<a id="step-193"></a>
## 193. A plucked string's soundboard wears a third

The soundboard's wear multiplies a string's own loss b1 by 1 + 1.5·wear. A piano's strings lose
most of their energy through the bridge, so on the pianos that made the decay at 1 s a median
3–6 dB faster at Broken; a plucked string's own loss is most of its damping, and the same factor
took the guitar's and the đàn tranh's sustain (−18 dB median, −51 to −80 dB on their worst keys)
and the harps' (−7 and −12, worst −77 and −84). A plucked string's soundboard wear now adds a
third of it (`PLUCKED_BOARD_LOSS`): ×1.5 at Broken.

The soundboard's wear alone at Broken (all keys at velocity 100, the room off; the decay at 1 s
against Mint, median [5th percentile], dB):

| | before | now |
|---|---|---|
| Guitar | −18.5 [−80.2] | −7.0 [−20.0] |
| Strumstick | −10.6 [−39.9] | −3.6 [−13.2] |
| Harp A, B | −6.6 [−76.7], −12.2 [−83.8] | −2.2 [−28.1], −4.1 [−26.3] |
| Harpsichords A–D | −7.6 … −16.0 [−21.2 … −51.5] | −2.6 … −5.4 [−7.2 … −17.0] |
| Đàn tranh, psaltery | −18.2 [−51.1], −4.5 [−17.8] | −6.2 [−17.3], −1.5 [−6.0] |

The pianos, unchanged, lie at −2.8 … −6.3 [−8.4 … −28.4]. At Broken with every aspect the guitar's
decay is now −10.6 [−55.4] (before −22.8 [−90.9]): its worst keys' loss is the strings' wear now
(b3 × 3, −50 dB on its 5th percentile alone).

<a id="step-194"></a>
## 194. The keys' voicing by kind

The Condition's voicing (the keys a little uneven: each key's hardness ±4 dB × wear and its
level σ 4.5 dB × √wear, 1 dB at Normal) and the hammers' wear (+12 dB of hardness at Broken) were a
piano's on every instrument. A piano's hammers and a harpsichord's quills are voiced key by key;
a vibraphone's or a glockenspiel's bars are cut and tuned even at the factory, and a harp's,
a guitar's or a kalimba's strings and tines are played by hand: nothing hardens, nothing is
voiced.

The live wear now goes by the instrument's kind (`LiveWear::of`, from the body):

| | hammers' wear | per-key hardness | per-key level |
|---|---|---|---|
| pianos, clavichord, harpsichords | as before | as before | as before |
| bars, discs, tubes (the mallets harden) | as before | none | half |
| harps, guitar, strumstick, zithers, kalimba | none (hidden) | none | half |

The keys' level spread (σ over the keys of the level change against Mint, all keys at velocity
100, the room off), dB, before → now:

| | Normal | Worn | Broken |
|---|---|---|---|
| Vibraphone | 1.0 → 0.5 | 2.2 → 0.9 | 5.8 → 2.1 |
| Marimba | 1.0 → 0.5 | 2.0 → 0.9 | 4.6 → 2.1 |
| Glockenspiel A | 1.0 → 0.5 | 2.1 → 1.2 | 5.6 → 3.4 |
| Harp A | 1.0 → 0.5 | 2.0 → 1.0 | 4.8 → 3.6 |
| Guitar | 1.0 → 0.5 | 1.9 → 1.0 | 4.2 → 2.4 |
| Kalimba | 1.0 → 0.5 | 2.0 → 1.0 | 4.0 → 2.1 |
| Harpsichord A | 1.0 | 2.0 | 5.3 |

What is left at Broken comes from the other aspects (the harps' and zithers' tuning and strings,
the bars' mallets).

<a id="step-195"></a>
## 195. Normal at 10 %

Mint and Normal (the default, 20 %) sounded alike by ear: Normal's keys differ in level by about
1 dB. Normal's upper edge is now 10 % (and the default), Worn's 35 %, Broken's 100 % as before; the
wear at each edge is unchanged (the voicing 0.05, 0.2, 1; every other aspect 0, 0.2, 1). From
Normal to Worn the wear rises over 25 points of the slider instead of 30, from Worn to Broken over
65 instead of 50. At the old default, 20 %, the slider now lies within Worn (voicing 0.11, every
other aspect 0.08); saved sessions keep their number and move with it.

<a id="step-196"></a>
## 196. Two harpsichords from Edinburgh, each 8′ choir recorded

Edinburgh DataShare published two more instruments of St Cecilia's Hall, recorded like the
clavichord (CC BY 4.0, 48 kHz / 24 bit): Luigi Baillon's French double of 1755 (F1–F6, each manual
recorded on its own: one 8′ choir each, every key) and Thomas Barton's English single of 1709
(G1–D6, its two 8′ choirs recorded alone and together). An earlier search had passed them over for
their noise reduction ("not an accurate scientific record of the actual decay", their readme).
Measured: the floors at −55 … −74 dBFS, never digital silence, the fundamentals reaching the floor
25–95 dB down; the Baillon also high-passed (the floor falls about 20 dB per octave below
100–150 Hz, its lowest fundamentals 70–110 dB under partials 2–4), the Barton only below about 50
Hz. Both are tuned at A = 415 Hz; the Baillon in an unequal temperament, the same on both manuals
(C, F, G, B♭ near 0 cents, C♯, E, A, B 12–16 cents low).

They are Harpsichords E and F (`baillon`, `barton`), each choir a part of its own (the Baillon's
lower and upper manual, the Barton's right and left choir). Until now only Harpsichord B had a
second 8′, derived from its first; these two are the first recorded ones. A register in format 6
now names its stop (`stop`, a bit of `Params::stops`; without it, its place in the list): their
second choir sits on the 8′ II's stop, with no 4′ and no lute. The engine plays a register by its
stop, the report says which stops play one (`stops`, checked against renders by
`no_effect.rs`), and the plugin shows those stops alone (the header: Stop 8′, 8′ II, Buff). The
export's added register takes `name` and `stop` (`registers = [{ part, octave = 0, name = "8′
II", stop = 2 }]`); the editor's comparison plays each part on its register's stop.

Neither set has release samples: both borrow Harpsichord A's release noise (`[uses] release`).
Their levels: a phrase in the middle lay +8.2 (E) and +7.4 dB (F) over the grands' median (−28.9
LUFS); `output_gain_db` −8.0 and −7.5 put the 8′ at −28.8 and −29.1. The second choir plays 1.8 and
3.0 dB louder (the recordings' balance, −2.0 … +1.8 and −4.8 … +1.2 dB over the keys), both
together +5 dB. Against the recordings (`tone_check.py`, the 8′): the middle within 3.4 dB per
octave band, the treble within 2.4 (E) and 7.4 dB (F, at 8 kHz; 2.1 below it). Over the first 30
dB the decays scatter against the recordings as Harpsichord A's do against its own; the editor's
comparison reads them ×0.71–0.77 long (Harpsichords A–D ×1.0), deeper in the noise-reduced tails,
and flags 23 (E) and 4 (F) pairs (their ceilings). Their bass against the high-passed recordings
says nothing; against the other harpsichords' engines the fundamental lies −13 … +7 dB re partials
2–4 (A–D −21 … +4). The recordings end after 4 s (E) and 6 s (F), so the bass's decay is seen over
its first 30 dB: C2 falls 30 dB in 4.4 and 4.6 s in the engine, 3.8 and 5.7 s recorded (A–C 8–10 s,
D 4.3).

Left open: the Barton's staccato takes hold its own release noise (the dampers landing), its
both-choirs takes the longer bass decays (8.8–18.3 s) and the two choirs' beating; the Baillon's 4′,
lute and buff were not recorded.

<a id="step-197"></a>
## 197. Three mbiras from VCSL

VCSL's unused instruments, looked at (CC0): of those with a pitch per key, the three mbiras are new
to the collection; its marimba, vibraphone and xylophone (8–14 bars each) are sparse beside Iowa's
complete ones, its tubular glockenspiel has 9 tubes, its slit drum 2 tongues, its folk harp is a part
of Harp B's set; the rest is unpitched (triangles, cowbells, gongs, …) or rubbed (wine glasses).

The mbiras are the kalimba's steel tines with buzzers on their boards (bottle caps or shells): Mbira
A (a Zimbabwean mbira dzaVadzimu, nyamaropa tuning, 24 tines B2–C♯6), Mbira B (a Zimbabwean mbira,
mavembe or gandanga tuning, 24 tines A♯2–A5) and the nyunga nyunga (Mozambique, 19 tines F3–F5).
All three are named an octave low (the energy of every take an octave above its name, as the
Tanzanian kalimba's); Mbira A's lowest file, named A♯1, sounds B2 + 17 cents (the set's "Low B"):
relabeled. Their projects are the kalimba's with their own sources: the modes 1 : ~6.5 : ~18, the
other tines ringing along (`halo`), the thumb's click, played at equal temperament (tuned −51 …
+55 cents off the names), no dampers.

The buzzers measured as the kalimba's rattle (the band between the lines re the fundamental): −26 …
−55 dB, the kalimba's −53 … −76. Its membrane at the kalimba's level (0.2) held the per-key gain at
its limit (100) on the loudest buzzers; at 2.0 every recorded tine fits (the limit only far above
the recorded range). The buzz stays on no control, as the kalimba's rattle (the Sympathetic control
is "Ringing tines").

Their level against the kalimba's (the tines' family, set by step 173), the phrase: −2.1 (A), +0.6
(B), −3.8 dB (the nyunga nyunga); `gain_db` −6.5, −9.0 and −4.5 put them at −27.1, −26.9 and −26.8
LUFS against the kalimba's −27.0.

Hammered at the Volume's top (`runaway.rs`), Mbira A's top keys ran away (62 voices silenced): beyond
the recorded keys the export fitted each key's membrane to the edge tine's rattle, and a fainter
extrapolated tine took the gain's limit (100 × 2.0). The export now holds the edge key's gain
beyond the recorded keys (`export_bars.py`), which also moves the kalimba's 33 and the balafon's 59
keys outside their recordings (the kalimba's below G2 from 0.08 to 0.48, above C♯7 to 5.31; the
balafon's 1.43–2.07); none within them. The notch test (`test_radiation_notches.py`) takes a flat
body's response as flat (the nyunga nyunga's mode at 9.3 kHz read as raised against the generic
board's peak there). The wizard's tines answer (`projects/kinds.toml`) lists the mbiras as donors:
the kalimba and Mbira A now read as tines, Mbira B and the nyunga nyunga as the mallets' (a fellow
bar answer). Against their recordings (the editor's comparison) the lengths read ×0.92–0.98, 3–4
pairs flagged each (the kalimba 9).

By ear the mbiras sound much like the kalimba (the user): the four tine instruments moved from
Percussion to a group of their own, "Thumb pianos" (after Percussion in the menu).

<a id="step-198"></a>
## 198. Neutral names, the grands lettered by sound

Before the repository goes public, the instruments' directories (`instruments/`, `projects/`, the
`"instrument"` state) stop naming a sample set or a maker: `grand_a` … `grand_g`, `upright_a` …
`upright_c`, `harpsichord_e`, `harpsichord_f` and `registers_fixture` (the table at the top of the
log's index). The directories that name a kind (`flemish`, `marimba`, …) stay; the parts and
sources inside a project that named a set take the new stem (`harpsichord_e_lower`); the `data/`
folders keep the sets' names, as `docs/getting-data.md` does.

The grands are re-lettered best-sounding first (the user's ear): today's A, B, F, E, D, C, G read A
… G. The log keeps the old names and letters (the table translates); everything else uses the new.

The rename changes no sound: the 13 renamed instruments render a test phrase sample for sample as
before (the exports are the committed ones with the profile and the menu name changed: a fresh
export from today's caches differs, Grand A's release peaks by up to 20 % and its levels by 0.4 dB,
a drift left for its own step). The ledgers record the new names as built (`build.py adopt`), except
the steps that were already stale before (Grands D, F and G's pedal, Grand D's felt), which stay so.
A test that loads through `common::load` now fails when its committed instrument is missing instead of
returning early, and `proto/tests/test_instrument_paths.py` checks that every instrument the engine's
code and tests name exists (many load directly and return early).

<a id="step-199"></a>
## 199. Every instrument's license, written from its project

Nine instruments carried a hand-written `LICENSE.md`, the others none. Now every project with
recordings has a `[license]` block (the set, its author, a link, its terms quoted where the source
words them, and `source_license`: CC0, public domain, free use, CC BY 4.0 or CC BY-SA 4.0), and
`proto/license.py` writes each instrument's `LICENSE.md` from it and from the blocks of every project
reached through `[uses]`: a borrowed part carries its recordings' credit along (Grand G's file credits
the Osiris Piano, the Headroom Piano for its felt, Grand A's set for its noises, Grand E's for its
dampers and Grand B's through Grand E's levels). The instrument itself is CC BY 4.0, or CC BY-SA 4.0
where a ShareAlike set is reached (today Upright C alone); every file says that music made with it is
the player's, free to use without credit. A rolled piano states its own license the same way, the
stricter of its sources'.

The terms were checked at their sources: the Iowa MIS page says of the whole collection that the
recordings "may be downloaded and used for any projects, without restrictions" (the docs had read the
piano's and the guitar's as "none stated: analysis only"); the Salamander set's author made it public
domain on 4 March 2022 (its README and mirrors still say CC BY 3.0).

No step reads the block (the driver leaves it out of a project's fingerprint, as the editor's take
checks): a changed credit makes nothing stale and needs no re-export, which would be a sound change.
The exports write the file; `python3 proto/license.py --all` rewrites them all, and
`proto/tests/test_license.py` checks each against its project. The licensor is this repository's
author unless the block says `by`; a project without a block (one the editor made from a donor,
which no longer copies the donor's block) names no license and no licensor, only what its borrowed
parts carry. Rolled pianos keep the file the engine writes.

<a id="step-200"></a>
## 200. The release peaks follow the code; two checks before a commit

After the rename (step 198), Grand A exported again did not match the committed export: its release
noise peaks moved by up to 2.8 dB, its key levels at C♯2–B2 by up to 0.24 dB. The cause is a memo.
The export scales each key's release noise by a reference peak: the prototype's render of the note at
pp, mf and ff (`release.peak_table`), cached per part and keyed by the analysis, the fit and the felt.
The key did not cover the code, nor the project's other tables. Grand A's table was rendered on
2026-10-27 at 14:47 from a working tree in the middle of step 184 (the board's low modes), a state
no commit reproduces; that afternoon's export and every one after it took the cached table. The
rename changed the fit file's bytes, so the key, and the table was rendered again with today's model.
The key now covers the body bands, the project's tables and the source of every module the render
imports (`release._render_sources`): a model change renders the peaks again at the next export,
about 10 s a piano. No other cache is keyed by content; the rest are steps' outputs, which the
ledger follows.

The levels have a second cause: step 190 changed the engine's phantom partials and measured only
Grand A's `level.json` again, so its even-level corrections (step 138) still came from the engine
before it.

Two checks now say whether the committed files are what the tree makes:
- `just check` (`build.py status --check --all`, ~6 s): the stale steps, exit 1 if any; a project
  without caches (never built in this checkout) is passed over.
- `just reproduce` (`proto/reproduce.py`): every export made again into a scratch folder (`--out` on
  both exporters) and compared field by field, half the cores' worth of projects at a time (a bar
  instrument's export takes ~40 s on one core); 13 min the first time, one after another (every peak
  table rendered again), 2.2 min after.

Run over all 38 exports with the new key, 19 differ: 6 by rounding alone (1e-14 dB, the peaks'
sums in another order; below 1e-6 dB is no difference), 13 by the stale peak tables. Largest release
peak change: Grand A 2.8 dB (35 keys above 0.5 dB, A0–E4), French 2.2, Grand E 1.7, Flemish 1.5,
Grand B and C 1.3, Harpsichord A 1.25, Grand F 0.9, Italian 0.6, Clavichord 0.4, Grand D 0.35,
Grand G 0.14; on Grands B, C, E and F only the top 3–5 keys move by more than 0.5 dB. The output
gains of the harpsichords follow by 0.01–0.05 dB, the Flemish 4′'s balance against the 8′ (from
the same peaks) by −0.09 dB on average, 0.78 dB at most. The uprights, the bars and Harpsichords
E and F reproduce. Exported again, they are a sound change: heard as variants (Grand A, the French
and Flemish harpsichords, Grand D) and found fine by the user (2026-10-30), all 13 were exported
again; `just reproduce` gives 38 of 38 and `just check` nothing stale.

`build.py status` also showed four steps stale on main since 2026-10-20/21: Grand D's felt (a key
added to `[strings]` that only the board reads; run again, the felt comes out identical) and the
pedal of Grands D, F and G (Grand A's pedal, which they borrow, refit by 0.02 dB with its strings;
Grand D's board refit; run again, the pedal's level moves by 0.03 dB at most). They ran again with
today's code before the export: Grand D's chain from the felt on moved its board correction by up to
0.37 dB, its room EQ by 0.16 dB, its top keys' gains by up to 0.25 dB (the board's code had changed
since 2026-10-21).

<a id="step-201"></a>
## 201. A fresh clone builds, and makes a new instrument from one set

The repository for strangers: a `LICENSE` (AGPL-3.0-or-later; the plugin's metadata said
"Proprietary", now the SPDX id, which LV2 gets as the spdx.org URL), `CONTRIBUTING.md`, issue and pull
request templates, a README that starts from what the project is, the plugin's and the editor's quick
starts and [the lessons](../lessons.md). DPF is a git submodule at `plugin/dpf` (the justfile cloned
it at a commit it kept): `git clone --recursive` and `just build` work in a fresh clone, and a fresh
worktree's `just test-plugin` fetches it.

The editor's quick start, tried in a fresh clone with only Harp A's set in `data/`, failed three ways:

- The soundboard step crashed saving its plot: `proto/out/` is not in git. The soundboard and
  register steps make it now; a test checks every chain script that writes there.
- A new instrument borrows from other projects' caches (Grand E's dampers, Salamander's release and
  pedal noise; levels and felt for three projects), which were not in git, so the build rebuilt every
  lender from its recordings first: three more sets (Salamander, the Iowa piano, VCSL Keys). With them
  linked in it still failed after 236 s, on the lenders' borrowing among themselves (Grand B's pedal
  step before Grand A's pedal noise existed), and it had refit Grand B on the way (its fit rewritten
  in the 6th digit).
- `just check` judged any project with a `cache/` folder as built here.

Now the seven lent caches (4.4 MB, Grand B's analysis 2.5 MB of it) are tracked (`git add -f`;
`proto/tests/test_fresh_clone.py` fails on one a committed project reads and git lacks). A lender
whose recordings are not in `data/`, or that was built elsewhere (its ledger came with the clone,
nothing of it was built here: someone who makes a new grand and has Salamander's set), is not
rebuilt, and its borrowers are not stale on its account: they read what is committed of it. The
editor's build page lists only the lenders it would build. `just check --all` judges a project only where a step ran (its
cache holds a file git does not track) and its recordings are here. In a fresh clone the acceptance
test (a new project from Harp A's set, built) passes in 70 s with only that set, no committed file
changed and `just check` clean; this checkout still judges all 39 projects. The acceptance test now
deletes its project and export after it passes.

<a id="step-202"></a>
## 202. Three checks around the lent caches

Step 201's review left three gaps, closed here:

- A lender taken as committed whose cache a borrower reads is not in git (the editor's Uses list
  can name any project as a lender, and only seven caches are lent): the build died inside the
  borrower's step on a missing file. It now fails before the first step and names the read
  (`uses.<role>:cache.<name>`).
- A lender refit and its lent cache left out of the commit passed `just check` here (the file in
  the tree is the new one) while clones built from the old one. `proto/tests/test_fresh_clone.py`
  now compares every borrower's ledger fingerprint of a lent cache with the file; in a clone, as in
  CI, the file is the committed one.
- With `data/` not mounted, every project built here was passed over and `just check` exited 0. Such
  a project is now named ("built here, but its recordings are not in data/"), and the check fails
  when it leaves nothing checked.

<a id="step-203"></a>
## 203. CI on three platforms; the plugin ported to macOS and Windows

The repository's CI (GitHub Actions) builds on every push and pull request:

- **Linux** (Ubuntu 24.04): the engine, the plugin and the editor; the plugin's UI tests (with a
  comma-decimal locale), the prototype's and the editor's suites, `just check`. The engine's suite
  (565 tests, 6 min on every core of a desktop) runs nightly and by hand (`engine-tests.yml`).
- **macOS** (arm64, macOS 13.3 and up) and **Windows** (MinGW-w64, for DPF's WASAPI fallback in the
  standalone): the plugin and its UI tests.

While the repository is private its Actions minutes count (macOS 10×, Windows 2×; a day of pushes to
branches used half a month's): CI runs on `main` and pull requests only, macOS and Windows
(`platforms.yml`) only where the plugin or the engine changed, the engine's suite by hand; Windows
builds in MSYS2's UCRT64 with MSYS2's own Rust (MINGW64 is deprecated).

A fresh clone holds no datasets and no caches but the lent ones (step 201). 44 prototype tests read
a project's analysis cache or a set directly; a test that dies on a missing file under `data/`, or
on a cache of a project whose recordings are not here either, is now skipped and names what it
needs (`proto/tests/conftest.py`; with every set here the suite is as before, 478 passed). The
editor's two corpus tests check their recorded floor of hits only with every set present.

The port, all behind the same code on Linux (its UI tests unchanged, 266):

- The user folders come from one function per platform (`plugin/src/Paths.cpp`): Linux's as
  before, a relative `XDG_*` ignored as the XDG spec says; macOS's Application Support; Windows'
  `%APPDATA%`.
- Folder scans, file stamps, folder making and replacing a saved file go through `std::filesystem`
  (`std::rename` does not replace an existing file on Windows: a second save of a preset failed).
- `std::from_chars` for floats is missing from macOS's libc++: `pe::parseFloat` reads a number's
  extent first and parses only that (libc++'s stream took the letters of a hex float, "1.5x").
- The telemetry region is a named file mapping on Windows, freed with its last handle; the
  stale-region sweep and its tests stay Linux's.
- An instrument opened from a folder accepts Windows' absolute paths ("C:\…", "D:/…").
- On Windows the plugin links the system libraries Rust's standard library needs.

<a id="step-204"></a>
## 204. Upright D, from the maintainer's own session

The maintainer recorded his own upright on 2026-10-31 after the session sheet
(`docs/recording-a-piano.md`): a handheld recorder's XY pair 64 cm in front, 48 kHz / 24 bit, one
file per pass (pp, mf, ff over every third key, rung out; the keys at the breaks; releases; C1–C8
with the pedal down; the pedal alone). The piano was last tuned years ago.

**The splitter** the sheet promised, `analysis/split_session.py`, cuts a pass at its strikes and
labels them in the sheet's order. A strike is a broadband rise (15 dB over the 0.3 s before) with a
harmonic series rising with it: each key's comb over the spectrum after the rise re the one before,
less the comb between the partials (a bass note's dense spectrum matched a treble key, its odd
partials an octave). Its sample is the steepest rise of a 2 ms level near it: the smoothed rise
peaked up to 0.17 s after the hammer, a key's thump before it, and the first split's files began
after their attacks and ended in the next one's. Labels: the in-order assignment with the largest
total score. Left out, each with its reason: a take falling 15 dB under its key's and neighbors'
median decay for a second within the first half of it (a mis-strike stopped and replayed; the
treble's prompt fall and a take stopped near its end are no cut: a fall alone, or how long a take
stays over the noise, flagged half the treble), one ending early while still 15 dB over the floor,
a wrong note (a neighbor within two semitones 6 dB stronger), a key touched (20 dB under its
loudest take, or a rise within 2 s of a strike and 20 dB under it), a clipped take where its key has
another. The kept takes are ranked by how near their level lies to the neighbors' (the sheet's "more
even one"), and one SFZ file over all passes is the set's map (`seq_position` 1 the first rank, so
`skip_files` falls back to the next). The releases pass gives each take from its damper's fall; the
keys around the breaks, at various velocities, take the dynamic whose level the sheet's passes put
nearest (within 4 dB). Its tests on synthetic sessions: `proto/tests/test_split_session.py`.

**What the passes hold.** pp, mf and ff: 188 strikes, 180 takes kept, two per key and dynamic. Left
out: F♯1's first pp and mf (stopped at 3.4 s and replayed), an F2 for D♯2 (pp), C3 pp and D♯6 ff
touched, a C8 pp cut by the file's end, and the ff takes of A5 and A6 that clip (A5's other clips
too, 9 samples, kept as its key's only take). The breaks pass labeled "D2" sounds D3 (146 Hz, its odd
partials 53–60 dB down: the last wound string on the treble bridge); it adds D3 at pp, mf and ff, E6
at mf and ff, F6 at ff. The releases: 23 takes A0–E6 (the keys above have no damper). The noise
floor lies at −79 dBFS (30 Hz up), −80 dB per octave band at 31.5 Hz to −91 at 2–4 kHz, no hum; a
note's first 0.5 s 27–36 dB over it at pp (the treble least), 47–50 at mf, 55–59 at ff. D♯7–C8 at pp
and A7, C8 at mf hold too few partials over it for the analysis. The two takes of a key lie 3.7, 1.9
and 0.9 dB apart (median; pp, mf, ff). The tuning: A4 −6 cents, the bass flat to −30 at A0, the treble
−11 … −1 to A7 (F♯7 +16, C8 +42), 7.7 cents rms around a smooth curve (Upright A 4.1, B 7.6, C 3.5);
the unisons beat 1.2–2.0 cents per half octave from C4 up, 10 at A7.

**Its own measurements.** The first set this repository has with release recordings at a player's
distance and all three dynamics:

- the dampers (`damper_decay.py` on the releases' SFZ regions): 197 partials, 111 dB/s median at
  100 Hz–1.5 kHz (Grand E's, which every other piano borrows: 111), the bass's weaker (A0–C1 15–60);
- the release noise: 18–27 dB under the mf note's peak (Salamander's, which the others borrow, lies
  at −40 … −48 after its −12 dB of step 120). `release_noise.py` measured a set of several dynamics
  against its loudest take; it now takes the mf one, as the releases are struck (a one-layer set
  unchanged);
- the room between the partials: 0.65 s at 250 Hz, 0.26–0.36 s from 800 Hz to 12.8 kHz, a living
  room's; below 250 Hz 2.9–5.6 s (strings ringing in the gaps), held at 250 Hz (`room_t60`).

The pedal's noise and ring stay Salamander's (`pedal_noise.py` reads its files alone); the pedal
pass (16 events, the loudest 6–15 dB under the mf note's peak) is kept for later. A `[license]`
without a `url` (recordings not published) is credited without one.

**The fit** (made as the wizard makes it, from Upright A's file, the closest key range): the
coupling's error 9.7 → 6.4 dB (the pianos' 5.9–8.8), the register's level error 4.4 / 4.6 dB rms (mf
/ ff). Then what the other uprights got (steps 142, 143, 147, 168, 183), measured, not yet heard:

- the felt: the fitted one (p 2.30) left the attack at 1–4 kHz re 200–500 Hz on C2–C5 12–17 dB dark
  at pp, 3–4 at mf, right at ff; solved from two probes against all three dynamics: p 1.82, +5.3 dB,
  within ±2.8 dB at each. A0–B2 stays dark at 1–2 kHz (−4 … −8 dB) and at ff 2–4 kHz (−9);
- the lowest fundamentals: the board fit's data start at 92 Hz, and A0–G♯1's sat +19.1 dB over
  partials 2–4 re the recordings (C1 +35 … +47); rows at 22.5 and 31.5 Hz −12 dB, 45 Hz −6: +2.3;
- the treble's 2 and 4 kHz (C5 up) +11.8 / +10.1 dB over the recordings at mf, +14.8 / +10.2 at ff;
  rows from C4 to C5, 2 kHz −8 and 4 kHz −6: +6.1 / +3.1 and +8.6 / +4.7 (the same measure today:
  Upright A +8.4 / +2.7 at mf, B +11.7 / +2.5, C +1.0 / +3.5);
- the unisons: the recording's beats over the engine's from key 78 up, ×1.81 at 108 (`[unison]
  treble`); measured again ×1.22;
- even levels (window 9): 0.27 dB rms around the trend (A 0.35, B 0.46, C 0.18);
- the ring by ear as the other uprights' (150 % to C5, 200 % at C7), not heard on this one.

Its phrase (step 173's: an arpeggio at 80, chords at 95) at the midpoint of Recording and Player lies
−27.4 LUFS, the grands' median −28.2: no gain. The bass attack (`just report --uprights`) leaves the
uprights' envelope on 3, 2, 1 and 0 keys at 1–2, 2–4, 4–6 and 6–8 kHz (A 7/1/6/0, B 1/0/1/0, C
11/1/5/0). The editor's Compare flags 17 of its 96 pairs (Upright A 13, B and C 4): A0's early decay
×0.1–0.2 at every dynamic, F♯2 pp's and C3's lengths ×0.1–0.4; its renders ring a median ×1.74 its
recordings' length (the ring by ear). Upright D joins the rolls' references (the editor's candidates:
every exported hammer piano); its range lets a draw at warmth 2.5 pass, and the pull-back's test draws
at 1.0.

Open, for the ear: the felt and the trims above, the ring, its own release noise (some 20 dB over the
borrowed one), the bass's attack (a felt core as Upright C's?). The recordings are CC BY 4.0 (the
maintainer's choice).

<a id="step-205"></a>
## 205. Upright D heard: the release noise borrowed, its own pedal, a felt core

Heard (the maintainer, 2026-10-31): "pretty good"; the pedal "way too intense, it really makes the
strings sound on their own quite intensely"; the key release noise "way too loud and clicky"; Upright
C's felt core, set in the plugin's Felt tab, improved it.

**The release noise.** Its own release takes, the note's partials taken out, held the damped strings'
remainder 20–30 dB over the floor for 0.15 s and no distinct thump: 15–35 dB under the mf note's peak
(median −29; Salamander's, which the other pianos borrow, −32 … −49 after step 120's −12 dB, median
−42), its bands at 6.4–12.8 kHz −13 … −16 dB re its strongest (Salamander's −68 … −85), the first
impact read 4–17 ms into the take on most keys: the take's edge, the damper's fall at 50 ms. A fade-in
and a 0.7 s window took the highs 15 dB down, not the level. Not clean: Upright D borrows Salamander's,
as the other pianos do; its own dampers stay.

**Its own pedal.** `pedal_noise.py` read Salamander's four files by name; a source's `pedal = { kind
= "files", down = […], up = […] }` now names a set's own (levels as recorded, re the median mf note's
peak), Salamander's path unchanged (its cache rewritten byte for byte). `split_session.py --pedal` cuts
a pedal pass at its events and tells a pedal-down from a pedal-up by the strings' ring (56–280 Hz,
0.5–1.5 s after re 1 s before: +3 dB a down, −3 dB an up; a down wants 3 s to the next event): four
downs, three ups of its 16 events. The pedal's ring (the same band over 0.5–3 s after a pedal-down,
no notes, re the median mf note's peak):

| | the thump | the ring |
|---|---|---|
| Upright D's recordings (four downs) | −22, −13, −10, −5 dB | −54, −47, −41, −37 dB |
| Upright A's recordings (VCSL, four) | −24 … −18 | −58 … −56 |
| Upright D, Salamander's pedal fitted through its board | −16.7 | −27.7 |
| Upright D, its own | −17.3 | −39.0 |
| Upright A, C (Salamander's) | −18.0, −14.9 | −25.0, −29.0 |

The fit sets the ring to the downs' mean energy (−40.0 dB, the hardest press the loudest): the kick
12 dB lower (amp 0.00042 → 0.00010), centered on key 40 (spread 6.5; Salamander's shape is fitted on
Grand A), its line error 5.9 dB. The other pianos borrowing Salamander's pedal ring 25–30 dB over
Upright A's recordings by this measure; left for the ear.

**The felt core:** Upright C's (3 N to D♯2, none from C3; depths 0.8, 0.55, 0.45 mm). The attack at
1–2 / 2–4 kHz re 200–500 Hz on A0–B2, engine − recording: mf −7.9 / +0.2 → −0.9 / +0.2 dB, ff −4.2 /
−9.3 → +3.2 / −2.9, pp −5.0 / −0.3 → −3.8 / −0.4; C2–C5 as before (within ±2.7 dB).

The chain refit with the new sources moved nothing measured: the treble and the middle as in step
204, the lowest fundamentals +0.2 dB, the phrase's loudness −27.8 LUFS (the grands' −28.2).

<a id="step-206"></a>
## 206. Upright D's pedal thumps and its undamped treble

Heard (the maintainer): the pedal's ring right now, its thumps "still too loud and sometimes a bit
clicky" (the pedal noise he turns down about 20 dB on the other pianos too); the felt right; the
undamped treble "ring quite intensely on their own", the Sympathetic resonance control at about 25 %
right.

**The thumps** (each event at the Recording position, peak re the median mf note's peak over the 30
recorded keys at the mf velocity; the recording's takes re its mf takes):

| | downs | ups | highs of an up (peak over 3 kHz re its peak) |
|---|---|---|---|
| its recordings | −22.2, −12.8, −10.2, −4.6 | −31.4, −22.1, −23.9 | −22, −33, −30 |
| step 205's export | −17.4, −8.3, −5.6, −0.1 | −26.7, −17.7, −19.2 | −16, −16, −24 |
| now | −21.8, −12.7, −10.1, −4.6 | −30.6, −21.8, −23.5 | −45, −70, −40 |

They played 3.6–4.8 dB over the recordings: a take's level is re the export's reference, the model's
radiated mf peak, which lies over the engine's played mf notes (Grand A's borrowed thumps play
+2.5 … +4.4 dB over Salamander's ratios alike). `[noises] pedal_level_db = -4.5` (a level on every
thump, not on the ring's target). The onsets match (the first impact's rise 0.7–2.8 ms recorded,
0.5–3.3 in the engine); the click was the highs: the thump's bands were read over the take to its
last impact, and in a room's recording the later "impacts" (0.36–0.66 s) were the room's blips and
the band's highs its floor (−25 … −35 dB re the strongest band at 1.6–12.8 kHz), which the
resynthesis put under every impact. Within the first 0.3 s the thump's highs lie under the floor (a
low thud, 100–400 Hz). For a set's own takes `pedal_noise.py` now reads the impacts within 0.35 s and
takes each take's floor (its last 0.5 s) out of its bands; Salamander's path unchanged (its cache
rewritten byte for byte). The ring: −41.4 dB (step 205 −39.0; the recordings −37 … −54).

**The other pianos' borrowed thumps**, the same measure (peaks, the two takes each):

| | downs | ups |
|---|---|---|
| Grands A, B, C, E | −18 … −20, −22 … −25 | −23 … −26, −27 … −31 |
| Grand D | −31, −36 | −38, −41 |
| Grand F | −29, −34 | −37, −38 |
| Grand G | −11, −17 | −19, −22 |
| Uprights A, B, C | −15 … −19, −21 … −24 | −19 … −23, −25 … −29 |
| Upright D's recordings | −22 … −5 (median −11.5) | −31 … −22 (−23.9) |
| Salamander's raw takes re its raw mf (v8) notes | −11.3, −17.2 | −18.8, −24.5 |
| Salamander's mix (its SFZ: −20/−19 dB) | −21.8, −27.7 | −28.3, −34.0 |

Against both recordings the engines' thumps lie at or under their level, not 20 dB over: the
maintainer's 20 dB by ear is a cut below what a microphone at the player's distance records (as
Salamander's sample author mixed his takes 10 dB under their raw level, steps 120–121). For a later
round on every piano.

**The undamped treble.** Keys C3–C5 of the releases pass (held 2.3 s, released) and their renders, the
band of the undamped strings' fundamentals (F6–C8, 1.4–4.3 kHz) 0.5–2 s after the release re the
first 0.2 s of the attack: the recording −50.2 dB, its floor (−50.3 before the strike: no ring above
it); the engine −48.3 at the control's 100 %, −54.2 at 50 %, −60.2 at 25 %, −99 at 0; at 2–3.5 s
after the release −65.4 / −77.5 (100 / 25 %). The engine's undamped strings ring at least to where the
recording shows nothing; 25 % puts them 10 dB under its floor. A project's `[sympathetic] percent`
(0–300) scales the strings' sympathetic drive (the export's `transfer`) as the control scales it on a
string instrument, so the control's 100 % is the instrument's own; without it the transfer is as
before (every other export unchanged). Upright D: 25 %.

<a id="step-207"></a>
## 207. Upright D's pedal: the stomps out of the round robin

Heard live in the plugin (the maintainer): "Sometimes pressing the pedal it's clicky, sometimes it
isn't." The engine plays a pedal-down's take in turn (`Engine::set_pedal`: the take index advances at
each pedal-up, the downs and the ups modulo their counts); a press's speed and depth are not read (a
CC64 jump and a 0.4 s ramp play the same take at the same level, the thump where the dampers' engage
crosses half, about CC64 64; a half press under that plays none). Each press at the Recording position
(peak dBFS; the highs over 4 kHz re its peak in the first 5 ms):

| press | 1 | 2 | 3 | 4 |
|---|---|---|---|---|
| Upright D, step 206 (down_1 … down_4) | −46.6, −138 | −37.7, −68 | −35.3, −37 | −30.6, −38 |
| Upright A (Salamander's two) | −42.7, −113 | −46.9, −109 | as 1 | as 2 |

No press starts on a step (the largest sample step −28 … −33 dB re its peak), the onsets are the
recording's; every third and fourth press played down_3 and down_4, the pass's two stomps, whose clack
the recording holds (their highs at the onset −31 / −35 dB re the peak recorded, the normal presses'
−39 / −32 over the take but none at the onset), 16 dB over the softest. Upright D's thumps are now its
two normal presses (−46.8 / −37.8 dBFS, the highs −57 / −68); the ring keeps its target from all four
downs (heard right: −41.4 dB re the mf note as before), a source's `pedal.ring`, read by `pedal.py`
in place of the downs (Salamander's cache has none: its path unchanged). `pedal_noise.py` warns of a
take 6 dB over its kind's median (down_4 +6.9 dB). Choosing the take by the press (its speed) would
be the engine's; not here.

<a id="step-208"></a>
## 208. Grand H, from a normalized baby grand

The City Piano (Big Cat Instruments, 2014, public domain), a baby grand recorded close: every key,
four velocity ranges (1–48, 49–96, 97–110, 111–127), a release per key, 44.1 kHz / 16 bit. Downloaded
on 2026-10-03 and set aside for its normalization and its cut files; measured before fitting:

- **Levels.** Every file peaks at −5 dBFS: no level per velocity or key. The noise above 4 kHz in a
  file's last second lies 77–82 dB under its peak in every layer, so the noise floor gives no layer's
  gain back either.
- **The layers' timbre is the recordings' own.** The attack's 2–4 kHz re 200–500 Hz (0–0.3 s), from
  the softest range up: −41/−23/−21/−17 dB in A0–E2, −43/−31/−25/−20 in F2–E4, −16/−8/+3/+5 in
  F4–E6: real strikes, only their levels flattened. The softest, the middle and the loudest range
  are pp, mf and ff (the wizard's proposal); 97–110 is left out.
- **Decays.** The files end while the notes still sound (20 s in the bass, 10–15 s in the middle,
  4 s in the treble), at 60–72 dB under the peak. The fundamental's decay is measurable over a
  median 24–28 dB in the bass, 53–57 dB in the middle, 64–66 dB from F4 to E6 (Grand B's mf 26/58/80,
  Grand C's loudest 30/63/81); the analysis finds too few partials in 16 of 264 takes, all from D♯7
  up (Grand B 22).
- **Tuning.** A = 440 Hz with a stretch (−22 cents at A0, +10 … +15 from F6), ±4 cents rms off a
  smooth curve; no mislabeled note.
- **Microphones.** Close and in effect mono: the channels' difference lies 43–44 dB under their sum.
- **Releases.** 0.3–0.8 s each, normalized too, and key-up noises alone: no string rings before them,
  so no damper's fall to read (step 119), and no level re the notes. The dampers are borrowed (Grand
  E's), the release and pedal noise (Grand A's), as on the other grands.

The project starts from Grand E's file (normalized as this set is, its levels from Grand B's
recordings), its by-ear values then measured again on Grand H. The levels come from Grand B too
(`[uses] levels`): of the grands recorded at real levels, its layers' brightness lies nearest this
set's (the attack's 2–4 kHz within 1–3 dB at mf and ff from F2 up; Grand C's 8–14 dB brighter at mf
and ff from F2 to E6). The register step reads Grand B's recordings, so building Grand H needs
`data/iowa-mis-piano/` too. Against Grand B the register fit leaves 4.6/4.7 dB rms per key at mf/ff,
the even levels 0.5 dB from key to key (the other grands 0.2–0.6).

What the export measured, as steps 141–147 and 183 did for the other grands (none of it heard yet):

- **The felt.** At the played pp, mf and ff velocities, the attack's 1–2/2–4 kHz re 200–500 Hz (keys
  C2–C5) against the recording: with Grand E's felt pp −5.8/−7.3, mf +2.5/+2.3, ff +0.9/+5.9 dB; two
  probes and a least-squares step, p 1.41 and −0.8 dB: pp −2.3/+2.4, mf +1.3/+0.5, ff −0.5/−1.6 dB.
- **The board's tone.** Untrimmed, the treble's upper partials lie under the recording (4 kHz pp/mf/ff
  +1.0/−0.3/−6.6 dB; the other grands at mf −1.5 … +7.5), so no treble row. The board fit's data begin
  at 41 Hz, and the fundamentals of A0–G♯1 sat +6.6 dB over partials 2–4 re the recording (D1–E1
  +14 … +24); 31.5 Hz −8 and 45 Hz −1.5 dB: +0.5 dB.
- **The unisons.** The recording's beats grow to 3.4–6.1 cents from key 90 up, the engine's stayed
  at 1.5–2.4: ×3.20 at key 108, measured again ×2.79, so ×8.93 (then ×1.29 left).
- **Its loudness.** The step-173 phrase at the midpoint of Recording and Player: −28.4 LUFS against
  the grands' median −28.0 (measured the same way): Grand E's gain kept.

Against the other grands (the report and the editor's Compare): the bass attack (A0–B2, each band re
200–500 Hz) lies within a few dB of its own recording (mf +1.3/−0.8/+3.9/+3.8, ff +0.7/−1.9/−6.1/+0.3
dB at 1–2/2–4/4–6/6–8 kHz), but 13–20 keys per band lie under the recorded grands' envelope of the
change from mf to ff: the recording's own change is that narrow (its mf bright, the 2–4 kHz in A0–E2
−23 dB re 200–500 Hz against Grand B's −38). The Compare page flags 11 of 264 pairs (Grand E 15),
lengths and top-key pitches; 94 lengths stopped by the cut files. Open: at ff the engine is darker
than the recording above C3 (the middle's 4 kHz −7.5, the treble's 2 and 4 kHz −6.3/−6.6 dB within a
note), where the felt solve (C2–C5, the attack alone) did not look.

Heard: "a bit thin and a bit like through a low-pass filter". Measured over whole notes (third
octaves 50 Hz–10 kHz, every second key, each note at the engine's level, Recording position): at ff
the engine lay 6.8–9.4 dB under the recording at 4–10 kHz (the middle 9–13 dB), within ±3 dB below
4 kHz at every dynamic; Grand E within −5 … +3 dB there. Above 4 kHz at pp and mf the whole notes'
deficit is the recording's hiss (its 4.5–6.5 kHz lies at the file-end level from 1.5 s on). The room
was not it (Grand E's room cuts 2–8 kHz more), nor the felt alone: the exponent and stiffness that
solve pp, mf and ff at 1–4 kHz leave ff's attack 4–10 dB short at 4–8 kHz. A felt core does it
(`[felt_core]`): depth 1.15× each key's compression at mf, 60 N to B5, 25 N from C♯6. Then ff at
4–10 kHz −2.4 … +3.3 dB over whole notes; within a note (tone_check) the middle's 4 kHz −7.5 → +2.3,
the treble's 2 kHz −6.3 → +3.5 dB; the attack's 2–4 kHz at ff now +3.7 … +9.3 dB over. On the two
pieces: +2 dB at 4 kHz, +3.4 … +4.8 at 5 kHz, +6 … +9 at 6.3–8 kHz, below 2 kHz within 1 dB.

"Thin" is the recording's: at 62–250 Hz the engine lies 0 … +5 dB over it, but the recording itself
lies 5–13 dB under Grand E's at 50–125 Hz (the pieces: Grand H 4–11 dB under Grand E at 62–125 Hz).
At Player Grand H also gains 500 Hz (+4.7 dB re Recording, Grand E −2.7). Adding body past the
recording is a choice by ear: open.

Heard (v2): the brightness "works in its favor", the change from mf to ff fine, and the Bass control
at +4 dB "is also helping". Grand H now carries that bass: `[board] trim_db` rows (22.5 Hz +3.6,
31.5 Hz −0.77, 45 Hz +4.37, 90 Hz +1.97 dB, beside the lowest octave's own) make the Bass control's
shape × 4 dB within 0.04 dB from 27 to 600 Hz; both act on every key's correction. On the two pieces
the export at Bass 0 equals v2 at Bass +4 dB within 0.1 dB in every third octave (24 Hz–10 kHz).
<a id="step-209"></a>
## 209. Grand I, from the OLPC sound library's grand piano

The One Laptop per Child project's sound library (2008, "released under a CC BY license", CC BY
3.0) holds a grand piano recorded by Zenph Studios from a reproducing piano played by computer. Its
host is gone; the Wayback Machine keeps its 44.1 kHz folder (1212 FLAC files, 125 MB). Checked:
every file decodes (`flac -t`), all 44.1 kHz / 16 bit mono (the size), 0.8–23.5 s. Every third key
from A0 (30 keys); up to E♭6 each key struck at about 16 strengths, each three times: held to the
floor (15–23 s in the bass, 10–15 s in the middle), released at about 7 s, and staccato (released
0.2–0.4 s after the strike, a fall of 20–30 dB in 0.15 s, 40–50 dB over the floor in the middle);
from F♯6 up 12–16 held notes and at most four staccato ones. The names give the MIDI velocity
played, which differs from key to key; four files repeat another's audio (unmapped). Levels as
recorded (v1 → v127 about +35 dB at the peak, one floor at about
−92 dBFS under a gate), tuned about 6 cents over A = 440 Hz, A7 +26 and C8 about +50 cents (the
analysis finds no partials on C8). The keys' levels step 3–5 dB from one sampled key to the next.

`grand_i` (Grand I) is made as the editor's wizard makes a grand: Grand A's project file (the
closest key range), its sources the set's, the release and pedal noise Salamander's (through Grand
A) and the dampers Grand E's; its own synthetic board (seed 31). Per key the held take nearest MIDI
14, 60 and 124, the velocities the export plays pp, mf and ff at (`dynamic_names` over a dynamic
group that holds the key and the velocity: 90 layers of one file each). The editor's Decay and
Brightness take checks compare a take with its layer's other keys and find none here; the late-rise
check flags 1 of 90.

The fit against the other grands' (their last builds): the soundboard's rms residual 9.2 dB with the
per-note correction (others 9.6–12.7), the coupling fit's EDC error 10.9 dB (6.8–8.8), worst from A4
to C6 (11–20 dB per key; at F♯5 the recording's fundamental falls about 6 dB/s, the prototype's
about 15). The room fit only cuts: the recording's gaps at 250 Hz–1 kHz lie 24–39 dB over the
engine's late (others up to +22). Even levels on its own trend (`window_keys = 9`; Grand A's target
had lifted its top octave by up to 15 dB): −5.3 … +4.0 dB per key, rms 1.7 dB; its own top octave
falls 10–18 dB re its middle, inside the grands' −3 … −24 dB at C8.

Then as steps 141–147 did for the grands:

- The lowest fundamentals (`tone_check.py`): the board's data start at 66 Hz, the fundamentals sat
  over partials 2–4 against the recording by +23.4 dB in A0–G♯1 (the recording's D♯1 fundamental
  51 dB under its partials 2–4) and +7.7 in A1–G♯2. `[board] trim_db` rows by least squares (31.5,
  45 Hz −12 dB, 63 Hz −8.5): +10.4 and −1.8 dB. The bass's bands re its fundamental, −12 … −19 dB
  under the recording's before, now −7 … +3 dB at 125 Hz–2 kHz at pp and mf (63 Hz −12).
- The felt: Grand A's core by ear left out (with it the bass at ff +6.7/+8.1 dB at 2/4 kHz over
  the recording, without +3.6/+4.3); the pair solved as step 142's (below).
- The loudness: the middle phrase at the grands' median (−28.3 LUFS) at the midpoint of Recording and
  Player, `output_gain_db` +1.0.

Left as they are, numbers for the ear: within a note the middle's 250 Hz band +10 … +12 dB and its
8 kHz band +13 … +16 dB over the recording at mf and ff, the treble's 1 kHz +6 … +8 dB (the
hammer's thump 8 dB lower moved neither); the unisons beat 1.3–1.6 times the recording's from C4 to
F5 and 0.7–0.9 times from F♯5 to C♯7, 0.75 at the top (Grand A's ×12 kept, the bound). The 7 s and
staccato takes, unused, hold this piano's own dampers and release noise; measuring them needs the
release found inside a note (the damper step reads release recordings that start at the release).

The first listening (the user): "the Room adds a really weird high-pitched hiss", gone with Room down;
"up to C6 the single note listening tests of the pp versions sound very stringy. Same up to about C4
for mf"; the borrowed dampers fine.

- The hiss: the room's decay came from `body_decay.py`'s bands, and its 12.8 kHz band, the recording's
  floor (T60 41 s; 10 and 16 kHz 44 and 32 s), was consistent enough over the notes to be taken: the
  synthetic room rang for its whole 2 s at 6–16 kHz, 60–80 dB over Grands A and B's room output
  2.5–5.5 s after a strike (third-octave bands, Room on − Room off). `[strings] room_t60` takes the
  measured bands up to 8 kHz, held above (a refit of Grand I's steps): its late output at 6–16 kHz now
  −127 … −138 dB (A −139 … −150); in a piece the energy at 8–16 kHz −13 dB.
- Stringy: the beating against the recording (partials 1–12 over 0.3–3 s, keys C2–C6) lay where Grand
  A's and B's lie against theirs: the depth (median 2.9 dB, recorded 2.9), the jump of the beat
  rate from partial to partial (0.31, recorded 0.34) and the envelope's periodicity (0.36, recorded
  0.26; Grand A 0.36 against 0.31). The soft attack did not: at pp the 1–2 and 2–4 kHz re 200–500 Hz
  sat +8.6/+13.6 dB over the recording (C2–C6, medians; Grand A +0.9/+7.4), step 142's
  string-like, plucked soft playing. The first felt was solved from the tone within a note over 1
  s; solved on the attack at pp, mf and ff as step 142's (C2–C5, two probes): p 2.2, −0.2 dB. Now
  pp +0.9/+4.4, mf +2.1/−1.9, ff +8.2/+7.0 dB (C2–C6). The unisons (C4–F5 1.2–1.6 times the
  recording's beat) left as they were: a width of 0.75 did not slow them in `unison_check.py` (n =
  2 per half octave).

<a id="step-210"></a>
## 210. The Pedal noise at −20 dB by default

Heard in the plugin (the maintainer), on every piano: the pedal's thumps distract in quiet pieces
(the Ständchen), and the control was turned down about 20 dB each time; the ring the lifted dampers
start in the low strings is wanted. Step 206 measured every piano's thumps at or under what their
recordings hold: as recorded is not what a player wants to hear from the bench. The Pedal noise
control now starts at −20 dB (×0.1); it scales the thumps and the ring together, and at −20 dB the
ring is still heard starting. The engine keeps its "1 = as fitted" (`midi2wav` plays the noise as
recorded); the plugin's Copy CLI line carries `pedal=0.1`, so a render sounds as the plugin does.
Saved sessions keep their own value. A thump apart from the ring would be a second control; not
now.

<a id="step-211"></a>
## 211. The pedal's thumps and its ring, two controls

Step 210's control scaled the thumps and the ring together: at −20 dB the ring the lifted dampers
start (the maintainer: wanted) was 20 dB down with the thumps (not wanted). Now two: **Pedal
noise** the thumps going down and up (the engine's `pedal`, the one-shots), **Pedal ring** the
strings' kick as their dampers lift (`pedal_ring`, `Engine::lift_kick`; a `--param` key and a
control in the instrument report, inert where the export has no kick: the 13 pianos show it, as
the Pedal noise). The noise starts at −20 dB, the ring at −10 dB: chosen by ear (the maintainer:
"even if it's not entirely realistic. Sounds best to me"), 10 dB over the ring as step 210 played
it beside the thumps. Either at 0 leaves the other bit for bit as it plays alone
(`engine/tests/pedal.rs`), and the ring follows its gain (half the gain, half the peak within 1 %).
<a id="step-212"></a>
## 212. The About tab

Asked for by the maintainer: a page in the plugin with its name, version and commit, the project's
link, and what the loaded instrument is, mostly its `LICENSE.md`. It is the last tab, after MIDI.

- **The version** has one source: `plugin/CMakeLists.txt`'s `project(… VERSION 0.3.0)`, read by
  `BuildInfo.cpp` for the tab and for `getVersion()` (the hosts'), which named its own numbers
  before.
- **The commit** is read at every build, not at configure time (a configure-time value goes stale
  with each commit): `plugin/cmake/BuildCommit.cmake`, run by `cmake -P` (portable, no shell),
  writes `git describe --always --dirty` into a generated header, and rewrites it only when it
  changes, so a build without a new commit compiles nothing again. Without a `.git` in the
  repository's root (a source tarball, or the tree inside another checkout) or without git: "unknown".
- **The link** (`kProjectUrl`, the repository until its site is up) goes to the clipboard with
  **Copy link**, the path the menu's Copy CLI takes: DPF has no portable way to open a URL (only a
  web view), and a dependency for it was not wanted.
- **The instrument**: its menu name, its folder (the catalog entry's, so an instrument opened from a
  folder elsewhere reads that folder's `LICENSE.md`) and the file as plain text. `markdownLines`
  (pure, `About.cpp`) drops the heading marks, `**` and backticks, joins a paragraph's lines and
  keeps the list items and blank lines; `wrapLines` wraps them to the tab's width with the layout's
  text estimate, breaking a word longer than a line (the sets' URLs at narrow widths) and indenting
  a list item's further lines. The file is read when the instrument changes, the wrap done with the
  layout. A folder without the file says "No LICENSE.md in its folder: its terms are unknown."
- **The tab bar**: the ten tabs need 429 px on one row and the default window's panel had 389. The
  drawing takes 55 % of the width as before, down to half where that keeps the tabs on one row:
  at the default 900 px the drawing is 435 px wide (475 before), from 990 px on it is as before;
  windows under 895 px wrap the tabs onto a second row, as they did.

`ui_tests` checks the cleaning and the wrapping (every line within the width, the words kept in
order, a URL broken, a list item indented), every committed export's `LICENSE.md` read and wrapped,
the tab's lines inside the content area at the minimum, default and a wide window and a tiny one,
the license scrolling in a small window, and the plain lines for a missing file or instrument.

<a id="step-213"></a>
## 213. The name: Dussek

The project's working name, PianoEngine, gives way to its own before the first release fixes the
identifiers: Dussek, after Jan Ladislav Dussek (1760–1812), the pianist who had the piano's keyboard
extended to five and a half octaves. Renamed:

| What | Before | Now |
|---|---|---|
| the plugin's name / maker | PianoEngine / PianoEngine | Dussek / Christian Friedrich Coors (homepage: the project's link) |
| LV2 URI, CLAP ID, VST3 IDs | `urn:pianoengine:piano`, `local.pianoengine.piano`, `PnEn`/`PnoA` | `https://ccoors.de/plugins/dussek`, `de.ccoors.dussek`, `Ccrs`/`Dsek` |
| binaries and bundles | `pianoengine`, `pianoengine.lv2` …, `pianoengine-editor` | `dussek`, `dussek.lv2` …, `dussek-editor` |
| the engine crate, the C header | `pianoengine`, `pianoengine.h` | `dussek`, `dussek.h` |
| user folders | `~/.local/share/pianoengine`, `~/.config/pianoengine`, `PianoEngine` (macOS, Windows) | `dussek`, `dussek`, `Dussek` |
| environment variables | `PIANOENGINE_INSTRUMENTS`, `_CACHE`, `_NO_DIRECTIVITY` | `DUSSEK_…` |
| presets | `.pepreset` | `.dussek-preset` |
| the instruments' credit | "made with PianoEngine", "Grand A from PianoEngine" | "made with Dussek", "Grand A from Dussek" |

The short `pe` of the code (the C interface, the namespace, the test switches) stays: it names the
engine, not the project. The design log's earlier steps keep the name they were written under;
`proto/tests/test_the_name.py` finds the old one anywhere else. A DAW session saved before this
step does not find the plugin (its URI changed; pre-release, no alias), and presets, MIDI maps and
instruments in the old folders are not read until moved. The instruments' data are unchanged (41
of 41 reproduce); only their `LICENSE.md` reads the new name. The parameter table stands: no
reordering was asked for before the release.

<a id="step-214"></a>
## 214. What a player saves stays loadable

Before the first release fixes what players will save, each kind gets a record and a test that
fails when it is broken (`docs/compatibility.md`, from the going-public design's compatibility
section):

- **One instrument file, tied to its data.** `level.json` held the engine's measurements of an
  export (the ff level a morph reads, a hammer piano's level table over the velocity): it changes
  the sound, yet a copy without it played differently without a word, and the files' fingerprint
  left it out. It is now `instrument.json`'s last key, `level` (the `level` tool prints it, the
  export stores it; refreshing it is a re-export). `instrument.json` names its `data.bin` (`data`:
  size and SHA-256): another export's, or a cut one, is refused; a file without the tie loads as
  before. All 41 re-exported without a refit: no `data.bin` changed, Grand A's ff level within
  0.01 dB. Snapshots of older refs carry their `level.json` over.
- **The parameter lock** (`plugin/tests/params.lock`, 78 rows): index, symbol, range, default, unit,
  flags. DPF's VST3 and CLAP wrappers save values by symbol, as LV2 hosts do, and automate by index;
  the diagnostics after the table move when it grows, so none may be automatable (a test pins it).
- **The state's version**: `_state_version` (1), restored first: declared first for LV2, and first
  in key order for DPF's VST3 and CLAP wrappers, which keep the state in a `std::map` (the review
  found `state_version` replayed fifth there); every key through `migrateState`, in the plugin and
  in its UI. The version is sticky (no "restore ended" signal; live changes use the same path), so
  a migration leaves a value already in today's form unchanged.
- **Versions in the player's files**: presets (`version = 1`, the state version of their states)
  and MIDI maps (`version 1`); without one, the first version (not today's); a newer file loads
  what it understands.
- **Instrument aliases** (`instruments/aliases.txt`, empty for now): the catalog and the DSP's
  folder lookup follow them (8 hops, cycles end), a real folder first, also a player's own folder
  of an alias's old name in a later search path; nothing found stays missing.
- **The frozen formats** (`engine/tests/formats/`: `5-bars` Kalimba, `5-strings` Grand A, `6-registers`
  Harpsichord B), loaded and played by a test; an export of a format without a frozen copy fails,
  and a hammer piano's format needs a frozen hammer piano.
- **`render`'s lock** (`engine/tests/cli.lock`, 89 items: the arguments, options and `--param` /
  `--tuning` keys).

AGENTS.md's "no backward compatibility" rule gives way to these.

<a id="step-215"></a>
## 215. The standalone's window: its scale kept, its title, a black window explained

From the maintainer's tries of the test builds:

- **The zoom was lost at each start.** It lives in the `ui` state, which a host keeps with its
  session; DPF's JACK standalone keeps none (and never hands the UI the state). The zoom last
  picked is now also written to `window.txt` in the config folder, and a new window opens at it; a
  host's saved `ui` state still wins. The DSP side's default stays plain: DPF writes the default
  state into the LV2 bundle's presets at the build, which would carry the build machine's zoom.
- **The title read "Christian Friedrich Coors: Dussek"**: DPF's standalone titles its window
  "maker: name". The UI sets the standalone's title to the name alone; the maker hosts show stays.
- **Over Remote Desktop the window stayed black** (Windows' own OpenGL 1.1: no `glActiveTexture`,
  no NanoVG context, a DPF assertion on the console). Without a context the UI now says why, on
  standard error and on Windows in a message box, once: the sound still plays.
- The Windows `TRY-IT.txt` says the "Failed to load JACK DLL" line is expected (DPF tries JACK
  first, then the system's audio).

<a id="step-216"></a>
## 216. Opening an instrument folder on Windows

Under Wine the maintainer picked an instrument's `instrument.json` in "Open instrument folder…"
and got "no instrument": the UI cut the picked file to its folder at the last `/`, and a Windows
path has `\` only (`Z:\home\…\chimes\instrument.json`), so the file itself was taken for the
folder. `folderOf` and `fileNameOf` (`plugin/src/Paths.hpp`) split at either separator on Windows
and keep a drive's root; the Scala file's start folder and its label in the Temperament menu
used the same `/` split.

<a id="step-217"></a>
## 217. Going public

The repository on GitHub was made private under its final name; its history is the one prepared
commit (the tree of the archive's step 202) and the work since (CI, the ports, Uprights and Grands
D to I, the rename, the locks), one author identity, no attribution lines, no brand names outside
the credits (a word scan on every commit). Before its visibility changes, the text a stranger
reads first no longer assumes a private repository or Linux alone: the README names Linux as the
tested platform and macOS (Apple silicon) and Windows as built by CI and untested, points at the CI
test builds until the first release, and gives the clone's address; the workflows' comments drop
the private repository's minutes.

The repository went public on 2026-11-01. With it: a first-time contributor's pull request waits
for approval before its workflows run, the workflows' token is read-only, a ruleset keeps `main`
from force pushes and deletion (both refused on a private repository of a free account), and CI
runs the engine's suite on `main` and every pull request, beside the Linux job (about 40 min). The old
private repository stays, read-only, as the archive of the history before the first publish, its
README pointing here. Next: releases (ROADMAP).

<a id="step-218"></a>
## 218. The plugins tested on Linux and Windows

The maintainer tried the CI's Windows build on Windows 11 with graphics acceleration: the
standalone plays, and LV2, CLAP and VST3 load in a host, play, come back from a saved project,
follow the host's parameters and automation both ways and show the diagnostics' telemetry. One
gap stays: the shipped presets appear in the host's own preset menu for LV2 only. DPF hands VST3
hosts the programs as a hidden program-change parameter without a program list, and CLAP hosts
none (no preset-load or preset-discovery); the plugin's Preset menu reaches them in every format.
On Linux CLAP and VST3 play in a host too. A host's generic controls for the VST3 list DPF's
2080 hidden MIDI parameters ("MIDI Ch. 1 CC 64" …) before the plugin's own: VST3 carries a
controller only as a parameter, and the sustain pedal reaches the plugin that way. In a host
whose HiDPI scaling does not reach the plugin, the window is larger than the UI inside it, for
every CLAP and VST3 plugin alike. The Analysis and Visualization panels stay empty in the VST3
in two Linux hosts: with a UI, DPF's VST3 runs the sound and the editor as two objects, and the
region's token (an output parameter) reaches the editor only through a host that forwards output
parameters; one host passes no output list at all, and DPF then prints an assertion at every block.
Both are DPF's and left as they are. The VST3 stays for hosts that load nothing else; the README,
`docs/plugin.md` and `TRY-IT.txt` recommend LV2 or CLAP wherever a host offers them.
The README, `docs/plugin.md`, `docs/architecture.md` and `TRY-IT.txt` say what is tested where.

<a id="step-219"></a>
## 219. Nothing built before the host activates

With a UI, DPF's VST3 runs the sound and the editor as two instances of the plugin, and the editor's
one never plays: it loaded the instrument and built an engine all the same, and built again at
every change (a felt takes seconds). The worker now waits for the host's first `activate()`. An LV2
instance created and left inactive holds +5 MB after 4 s, was +115 MB (an offline probe, lilv);
activated, a note renders as before. `plugin/tools/lv2render.c` connects the plugin's audio
inputs (step 94) to silence: it took the first two audio ports for its outputs and crashed.

<a id="step-220"></a>
## 220. Releases with installers

A tag `v<version>` makes `.github/workflows/release.yml` publish a release with an installer per
platform, so nothing needs copying by hand (the maintainer: "manually installing all the files is
a bit fiddly"):

- **Windows**: a setup program (Inno Setup, `packaging/windows/dussek.iss`): the standalone in the
  Start menu and each plugin format as choices, an uninstaller, a fixed id so an update installs over
  the version before.
- **macOS**: a package (`packaging/macos/build-pkg.sh`): the app in Applications (`Dussek.app`,
  made around DPF's plain executable by `make-app.sh`), the plugins in `/Library/Audio/Plug-Ins`,
  each bundle signed ad hoc (the LV2's libraries one by one: an LV2 bundle is a plain folder) and
  not relocatable.
- **Linux**: the archive with `install.sh` (for you, or `--system`) and `uninstall.sh`, a desktop
  entry and the icon (`packaging/icon`); built on Ubuntu 22.04 (glibc 2.35), its direct
  dependencies limited to what every desktop has.

The instruments are installed once, in a folder for every user that the plugin now reads before
the user folder (`pe::systemDataDirs`: `%PROGRAMDATA%\Dussek`, `/Library/Application Support/Dussek`,
`$XDG_DATA_DIRS` or `/usr/local/share` and `/usr/share`): an old copy from a test build in the user
folder does not hide them. Each installer empties its own instruments first (an instrument dropped
since does not linger) and never touches the player's presets, MIDI maps or own instruments; the
Linux script lists what it installed, as it shares the user folder with them. `dussek list` prints
what the standalone finds; the plugin's Copy CLI also looks for the standalone where the installers
put it (`Program Files\Dussek`, `/Applications`). On each runner the installer is installed, updated over a stale
instrument, listed and (Windows, Linux) uninstalled before anything is published. The release
text is the version's section of `CHANGELOG.md` and a fixed footer (`packaging/release-notes.sh`);
a tag that is not the code's version, or has no section, publishes nothing.

<a id="step-221"></a>
## 221. The website

`site/` builds the website at ccoors.github.io/dussek from scripts, so a sound or UI change renews it
(`site/README.md`); `.github/workflows/site.yml` renders and deploys it, started by the release
workflow once a release is published (a release made with the workflow's token starts no workflow by
its event, and Pages deploys from `main` only) and by hand. Nothing rendered is kept in git:

- **The clips** (`clips.toml`, `render.py`): a demo per menu group on the maintainer's choice of
  its best (Grand A, Upright D, …), phrases composed per kind (`phrases.py`, CC0, each inside its
  instrument's recorded keys), the four original pieces and the Schubert on Grand A, the Scarlatti
  and the Bach on Grands C and B (MAESTRO, CC BY-NC-SA 4.0, credited), nine features as steps of
  one phrase, the audio input fed the maintainer's own falling sound (CC0, `midi2wav --input`).
  A clip names plugin controls; `plugin/build/settings_line` turns them into the plugin's own Copy
  CLI line, so the site plays what the plugin plays. Each clip at -18 LUFS, a feature's steps
  sharing one gain; Ogg Opus. A card's picture is a still of the instrument's demo from `--video`.
- **The screenshots** (`screenshots.sh`): the standalone on a virtual display, its audio on an
  unconnected JACK client; one with a chord ringing in the Visualization, also in the README.
- **The pages** (`build.py`): chapters in the Visualization's string colours, bass to treble; the
  features switch steps while playing and keep the place; the download picks the visitor's system;
  the credits from every instrument's `LICENSE.md`. The fonts are served from the site (Inter,
  Source Serif 4 renamed under its license); no external script or font. The docs stay on GitHub.

The first listening: the Schubert moved from Grand D to Grand A, fewer feature steps (the
microphones dropped), and two levels for the roadmap: the lute stop 9 dB above the plain stop,
the morph halfway 4.6 dB above Grand A. The ensemble piece is now "Ensemble Demo 1".

<a id="step-222"></a>
## 222. A second session: Upright D's own release noise, and a steel tongue drum

The maintainer's second session (2026-11-01, `data/SOURCES.md`): Upright D's silent releases, a
second pedal pass, three redos, and a steel tongue drum (15 tongues, C major, E3–E5) at pp, mf and
ff. `split_session.py` reads each:

- **Silent releases** (`--silent`): each key pressed without its note and released: the key's
  return and the damper landing on still strings, nothing else. No pitch to label them by: in the
  sheet's order, an event within 3 s of the one before its key's second take, a note struck by
  mistake (a series as a strike's) and the recorder's stop left out, the events before `--from`
  skipped; a count that does not match the keys stops the split. 30 keys (D♯4 twice); the two notes
  struck by mistake (A1, D♯3) lie where the order puts them.
- **A second pedal pass**: numbered after the first (each pass keeps its numbers when split again),
  its left-out events named by the pass. Four pedal-downs, quieter than the first pass's (10–25 dB
  over a busier room); its ups not told from the room's noises, by ear either (the maintainer):
  unused.
- **Redos at another gain** (`--gain-db`): A5 and A6 at ff (clipped in the first session), C8 at pp,
  at the recorder's level 50 instead of 60. The step from the floor of the same morning's passes:
  4.2–4.5 dB from 1 to 20 kHz, 4.3 taken; the takes measured and written that much louder, as float
  where that passes full scale, clipping judged on the samples as recorded. A clipped take plays after
  a clean one of a later pass: A5 ff plays the redo (its level 1 dB under the clipped take's fit).
- **A tongue drum** (`--tongues`): the comb over eight partials labeled C4's strikes C5 and D5's D4.
  Its tongues' upper modes are no series (25–45 dB under the fundamental), and the others ring in
  sympathy within half a second (a D5 strike sets D4 ringing louder than itself after 0.3 s): a
  strike is labeled by its fundamental alone over 0.03–0.2 s, the comb's floor band widened by the
  window's shortening (a 0.15 s window's peaks are 7× wider: the 150 Hz median sat on the strike's
  own skirt), a rise 20 dB under a take that still rings no strike. Played in the drum's order (not
  rising): 109 strikes, 88 takes kept over 15 tongues × 3 dynamics.
- **Clipping at the recorder's own ceiling**: the drum's loudest E3 lay 61 samples flat at 0.9935;
  a run of three equal samples at a channel's peak within 1 dB of full scale is clipped (no
  tolerance: a low note's rounded peak repeats within 1e-3).

**Upright D's release noise, its own.** Its notes' release takes (step 205) held the damped strings'
remainder; the silent releases hold the thump alone, 25–35 dB over the floor for 0.3–1 s
(`release_noise.py`, a source's `release = { kind = "silent" }`: trimmed where the level last lies
10 dB over the file's floor). Its level re the note as recorded at one gain, now per dynamic (the
key's return is the player's, not the hammer's: one noise after any strike): median −12.3 dB re the
pp note's peak, −29.9 re mf, −40.1 re ff; Salamander's, borrowed until now, −32, −42, −50. Its bands
at 6.4–12.8 kHz −30 dB re its strongest (Salamander's −76).

As played (velocity 64 and 110, keys C1–C6, the noise alone: the render with `release=1` less the one
with `release=0`, identical to −200 dB before the release), the noise lay 1–7 dB (median 4.5) over
the recordings re the note's peak, as the pedal's thumps did (step 205): `[release_noise] level_db =
−4.5`, then within ±2 dB. Against Salamander's as played before: −22 … −32 dB re the note's peak at
velocity 64 (was −36 … −43), about 10 dB louder, and brighter. For the ear.

The refit (the source changed; the A5 redo): A5's gain −1.0 dB, the board's and the coupling's fits
moved a little (the horizontal coupling at C4 1.40 → 2.41, a fit known to wander), the license now
the maintainer's recordings alone.
