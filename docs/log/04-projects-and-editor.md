# Instrument projects and the editor (steps 76–109)

The pipeline driven by project files, the plugin driven by the exports, the editor (wizard, Compare,
level corrections, take checks), and the listening passes that used them. Index of every step:
[README.md](README.md).

<a id="step-76"></a>
## 76. Instrument projects: the pipeline driven by data

Instrument projects: the pipeline driven by data (stage 1 of a Rust editor and port). Every instrument was
made by editing Python: a loader per sample set in `analysis/datasets.py`, rows in about two
dozen tables keyed by the profile's name (roots, dynamics, designs, board seeds, detune, the
soundboard fit's limits, the bars' mode tracks, the exports' constants), and a chain of
commands in the README with per-instrument exceptions. Now each instrument is a project,
`projects/<name>/project.toml` (32: the 31 instruments and the buff stop's English sets),
which holds those choices with their reasons as comments: its sources and how their files
map to notes and dynamics (`analysis/mapping.py`: an SFZ's regions ranked by velocity or one
file per layer, or a glob and a regex with named groups), the scale model and traits of its
strings (the models keep their formulas under names of their own, `pedal_harp`,
`harpsichord_4`, …: as tables they would change the exports in the last bits), the fits'
settings, what it borrows (`[uses]`: Salamander's dampers, release and pedal noise, another
set's levels or felt, the buff stop) and the export's options. The fits move to
`fits/<part>.json`, the analysis results and intermediate files to the projects' caches.
The steps declare what they read and write (`projects/steps.toml`, filled from a recording of
every access while eight projects' chains ran: one per family, Harpsichord B and the buff
stop); under `proto/build.py` `analysis/project.py` refuses any other access, and
`ledger.json` keeps per step the fingerprint of every declared read, its version, the commit
and a run sequence, so `status` names what makes a step stale. No instrument is named in
the code any more (`test_no_names.py`). The checks: the new mapper gives all 41 sources the
old loaders' items exactly (in order); all 31 exports byte-identical, by the old commands
after each migration step and through the driver with the checks on; refits of the eight
projects from the same start, the old commands against the driver: 271/271 files equal
(fits and caches as parsed JSON, the instruments byte for byte; the old pipeline itself
reproduced its refit exactly, so it is deterministic). The raw `city` and `vcsl-*` sets,
which no fit used, are gone; the guitar's files are read in a fixed order (glob's was the
file system's).

<a id="step-77"></a>
## 77. Instrument discovery: the plugin driven by the exports

Instrument discovery: the plugin driven by the exports (stage 2). The engine played any
export already, but the plugin knew its 30 instruments by name: their directories and menu
names in two tables, the Instrument parameter an LV2 enum written at build time and saved as
an index, and about 25 checks of the form "is it the kalimba" deciding the menu group, the
visible controls, the labels, the preparations, the mallets and the soft pedal. Now the export
carries a menu block from the project's `[menu]` (name, group, labels in the instrument's own
words, the mallets' display names, `hidden` for the registers fixture), and the engine reports
from `instrument.json` alone what an instrument has (`engine/src/info.rs`, `pe_instrument_info`):
its body, its exciter (hammer, quill, finger, pick, tangent, mallet, thumb), its stops, mallets
and recorded range, the preparations that act and the controls without effect, by rules over
the data the engine branches on (Release does nothing without release impacts, the damper
controls without dampers, the stretch on plucked strings, Paper on bars with a choice of
mallets …). `engine/tests/no_effect.rs` no longer pins lists per instrument: for every
exported directory the report must equal what the renders measure (all passed on the first
run; 150 s for the 31). The plugin's catalog (`plugin/src/Catalog.*`) scans the search paths
once per process (14 ms for the 31), every directory with an `instrument.json` the engine's
report accepts and a `data.bin`, the first path winning (the worker loads an id where the
catalog lists it; one that does not load silences the engine playing); the menu lists the known groups in
their order, then others, each by name. The choice is the `"instrument"` state holding the
directory's name (a session keeps its instrument across menu changes; a missing one stays
named and plays nothing), the Mallet parameter the instrument's own index. The UI recorded
before the change (every instrument's menu entry, body, range, preparations, mallets, labels
and visible controls at every microphone position and preparation: 3300 lines,
`plugin/tests/visibility.txt`) came out the same but for two intended differences: the groups
sorted by name (Percussion and Zithers reorder), and no soft-pedal lever on the bars and the
clavichord, where the engine ignores the soft pedal (it acts on struck notes only: measured).
A renamed copy of the harp dropped into `$PIANOENGINE_INSTRUMENTS` is listed, shows the harp's
controls and plays. No instrument is named in `plugin/src` (`test_no_names.py`). The renders
are bit-identical for all 31 exports. The driver no longer counts a borrowed project's menu
(or its stale export) against the borrowers' fits: adding `[menu]` made 21 fits stale.

<a id="step-78"></a>
## 78. Spread all keys on every instrument with a range of its own

The bars' zones lay along
their frame's range and the switch spread the keyboard's 88 over it (step 46's crotales);
the harps, the guitars and the zithers lay their notes along their own bridge or strings too
(`HarpDims`, `GuitarDims`, `ZitherDims`), but their eight zones stood at the piano's notes
(24 … 104) and were clamped to the range's ends: the psaltery (A♯4–F♯6) had two zones inside
its range, the đàn tranh three, the lever harp five, the rest piled at the ends. Now every
body with a range of its own (`Body::own_range`: the frame, the bridge, the guitar's E2–E4 and
the strumstick's D3–E4 strings) lays its zones along it, and the switch acts on all of them
(the report's `spread`, checked by `no_effect.rs`; the UI shows it there). The strings still
mix each note between its two nearest zones, so the old layout already ran from bass to
treble (the balance's rank correlation with the key at Close 1, before and after: psaltery
−0.73 / −0.74, đàn tranh −0.75 / −0.70, lever harp −0.56 / −0.45, guitar +0.61 / +0.60); a key
now mixes two near zones instead of the range's ends and middle, its left–right balance moved
by up to 4.6 dB at Player and 6.6 dB at Close 1 on the psaltery (10 dB on the lever harp at
Close 1; the guitars' strings within 2.4 dB), the zones' board shapes with it. The Recording
position (one zone) and the pianos, the harpsichords and the clavichord (the keyboard's 88
along a grand's or upright's bridge) are unchanged.

<a id="step-79"></a>
## 79. The editor, stage 3a

The editor, stage 3a (`editor/`):
a Rust and egui program over the project files and `build.py`. A project starts from a template
(an existing project of the same kind, its comments kept); a source is a folder under `data/`
(one elsewhere linked in); Suggest proposes the map; the grid and the table show the items, a
row plays its file; the build page shows what is stale and why, runs the chain, cancels it.
`project.toml` stays the only record: toml_edit keeps every committed file byte for byte, an
edit changes only its key's line. The mapper is ported to Rust (the view needs the items as a
rule is typed): on all 41 sources with data its 3750 items equal `analysis/mapping.py --dump`'s
field for field and in order. The suggestion, run on every committed source with the
hand-made choices taken over (skips, relabels, the subset of layers, an SFZ set's folders),
gives the same files on the same keys in the same layer order on 33 of 41 (28 at first: the
corpus showed a WAV form symphonia refuses, a set tuned to a = 415 Hz, `LEVEL` as a layer
prefix, Italian dynamics behind a shared prefix, harmonics files among the notes, an SFZ set
that needs its folders chosen). The misses: the balafon's three mallets, the hand chimes and
the kalimba (inharmonic: the pitch does not settle the octave, the plain reading is kept where
the projects use +12; the balafon's best reading was −12), the legacy chimes' +36, four
clavichord files with extra words in their names, the kalimba's `alt` files, and Grand G's
two SFZ files for two layers. The pitch score is conservative on recordings (Harp A 0.39,
the guitar 0.43, the bars 0.27: the plain reading is kept below 0.5), so it decides only
where one reading clearly wins (the đàn tranh, the psaltery and the strumstick at +12, Grand E's
index offset 20). `build.py --json` reports the status and a build's events; SIGINT and
SIGTERM (also when started with SIGINT ignored) restore the running step's files, and a
build outliving a killed editor gets SIGTERM from the kernel. Two conventions of the pipeline
became checks before a build: a string part's main source takes the part's id (`fit_pluck.py
--profile {part}`), and a source's dynamics name layers its items have. Acceptance: Harp A's
set made into a new project from the harp template and the suggestion (with Harp A's choice
of the mf and f layers) gives Harp A's items; its chain built in 55 s and the plugin listed it.

<a id="step-80"></a>
## 80. The editor's guided setup, stage 3a.2

The editor's guided setup, stage 3a.2:
the user's trial of 3a found the editor usable only by someone who knows `project.toml` (a
typed path; the map as key-value text fields; a switch from the file names to an SFZ file that
left the regex, the note and the takes behind; tables that do not sort). A model of a source's
rule (`editor/src/rule.rs`) now sits between the pages and the file: the parts of the files'
names, each with a role and its regex fragment as written (a role picks the fragment only when
it is changed), or an SFZ file's folders and filters; the layers with whether each is kept and
which is pp, mf and ff, in place of `layers`, `dynamic_names` and the source's `dynamics`; the
levels. Reading a source and applying the rule back gives it unchanged, and only keys whose
values change are rewritten: on all 41 committed sources with data the rule applies back
JSON-equal (also after its layers are synced with what the files hold), and all 41 show as
parts or an SFZ, none as a custom map (all 25 regexes the projects use split into parts and
join back to the same text). A wizard (New instrument…) walks through the kind and name, the
folder (the system's folder picker, the folders under `data/`), how the files are read, the
names, the dynamics, the check and the menu, with the suggestion's answers filled in, and writes
the project at Finish (one source per value of a group part); the same pages are a source's
tabs, with Advanced for every key as text. Switching between the SFZ file and the file names
stashes the keys the suggestion writes and restores them on switching back. The tables sort
by keys' pitch, dynamics softest first, file names with their numbers in order, measurements
by value (the unmeasured last). The suggestion is unchanged: 33 of 41 hits. Acceptance: Harp
A's set made into a new project through the wizard's Finish (with Harp A's choice of the mf
and f layers) gives Harp A's items; its chain built in 53 s.

<a id="step-81"></a>
## 81. SFZ keys by `key=` and `<control>` offsets; the analysis stops on a map with wrong keys

The
user's first instrument made with the wizard, a marimba set with an SFZ file, failed in the bar
fit with a numpy error. Its SFZ names each region's key with `key=` (lokey, hikey and
pitch_keycenter at once) and shifts every key by `octave_offset=2`; both mappers read only
`pitch_keycenter` or `lokey`, so the SFZ gave no items, and the suggestion's advice ("choose the
sample folders") pointed elsewhere. The file names then gave keys two octaves low ("24 p.wav"
sounds at 131.9 Hz, MIDI 48.1; "72 m.wav" at MIDI 96.0), which neither the suggestion nor the
editor's pitch column can see on a bar: its partials near 1, 4 and 10 × the fundamental let a
reading two octaves low find the true fundamental as its "fourth partial". The modes analysis
flagged 134 of 147 files ("no fundamental near the nominal pitch"), and the bar fit met an empty
set. Now both parsers (`analysis/mapping.py`, `editor/src/sfz.rs`, kept equal) take `key=` for
the keys a region does not give and add `<control>`'s 12 × `octave_offset` + `note_offset` to
every key; no committed SFZ uses either, so no committed item changes. The set's SFZ now gives
147 items on keys 48–96 in three layers. The suggestion advises choosing folders only where
regions share a key. `analysis/modes.py` stops with the reason when more than half its files are
flagged (the committed sets flag at most 3 %: 5 of 183 for one marimba mallet).

<a id="step-82"></a>
## 82. The bar fits take a set's dynamics through its source's table

With its keys right (step 81), the
marimba set's build failed again in the mallet fit: `bars.py` compared each take's dynamic with
"pp", "mf" and "ff" literally, as every committed bar set names its layers, while an SFZ set's
are "1/3", "2/3", "3/3" and the source's `dynamics` table (pp → "1/3" …) says which is which (the
string fits read the table already). Now `bars.samples` maps each take through the table (by the
layer's number with `layer_key = "layer"`) and leaves out a layer the table does not name. Every
bar set the fits read has the identity table, and rerunning all nine bar projects' fits gives
their committed values unchanged (four files differ in key order only, as they do with the old
code). The marimba set: 147 takes, none flagged, the mallet fit's rms 5.3 dB; its chain builds.

<a id="step-83"></a>
## 83. SFZ keys by note name and the default key center; the wizard's mallet names

Piano in 162's SFZ
(the Close file) gave the wizard no items: it names its keys (`pitch_keycenter=a0`), which both
mappers read as numbers only, and its 20 regions on middle C leave `pitch_keycenter` out, the
SFZ default being 60. Both parsers now turn a key by note name into its number (c4 = 60, as the
format counts) before `<control>`'s offsets, and the ranked mapping takes 60 where a region has
no `pitch_keycenter` (the one-file-per-layer mapping keeps its `lokey` fallback, which Grand G
relies on). The Close file now gives 440 items: 88 keys in 5 layers, pedal up, the first round
robin. No committed SFZ uses either form: the parity test's items are unchanged. The wizard's
Finish copied the template's `[menu] mallets` (the marimba made from the marimba template listed
Yarn, Cord and Rubber for its one mallet); it now writes one name per mallet source (its `mallet`
capitalized) and none for a single mallet, as the committed projects do, and Add a source…
appends the new sources' names to the list as it stands.

<a id="step-84"></a>
## 84. The editor's deferred minors from stages 3a and 3a.2

The editor's deferred minors from stages 3a and 3a.2 (13 of them, the ones a user meets). The build
status (`build.py status`, 0.8–1.2 s) is read on a thread of its own, and what blocks a build is
found again only when the project's text changes, not on every frame (up to 7 ms); the wizard
reads every project's ids once when it opens instead of on every frame. Finish removes only what
it wrote when it fails (a `projects/<name>/` that was there before stays), no longer deletes the
saved project when copying the measurements fails, and copies only the chosen folder's entries;
the measurement cache drops files that are gone. A taken id stops the check step instead of
Finish. With a group, the check page picks the source shown, and its skips and relabels are its
own (they went to every source unseen); a source of the group that reads no file stops the step
(the balafon's folder, suggested with the mallet folders as the group but `hardM` in the names
left as fixed text, made two sources with no files, and only the first source was ever shown). Cancel unlinks a folder the wizard linked into `data/`.
In a source's tabs the last pp/mf/ff can be cleared (the layers are proposed again only when the
rows change), an SFZ set's layers are listed softest first (by the regions' velocities, else by
the items' places among their key's layers: Grand K v1–v4, was v2, v3, v4, v1), the dynamics
page says "measuring…" while it measures, the Folder tab waits for a running job, a suggestion
that finishes during a build waits for it, a reload leaves a source page whose source is gone,
the files are measured again when they change (not only their count), and ticking a skipped step
keeps the skips the chain does not list. `build.py` restored a step's files by copying over them,
so a reader could see an empty fit for a moment (`driver.rs`'s orphan test failed 2 times in 60
runs under load); the copy is renamed over the file now (0 in 90).

<a id="step-85"></a>
## 85. SFZ `#include` and `#define`

Ethan Winer's sets keep their regions in included files
(`#include "Data/glockenspiel.txt"`) and name the samples' extension by a variable
(`#define $EXT flac`, `sample=EGlock G2.$EXT`): both mappers found no region, and the wizard
waited at "the suggestion is still running", the error only in the status line. Both parsers
now expand the includes (relative to the top file's folder, as sfizz does; a missing one is an
error naming it) and replace each `$NAME` after its `#define` (the longest name first) before
reading the regions; the suggestion reads `default_path` from the expanded text. A failed
suggestion is now the step's reason not to go on. No committed SFZ uses either: the parity test's
items are unchanged.

<a id="step-86"></a>
## 86. "How is it played?" and the donor

"How is it played?" and the donor.
A glockenspiel made from the hand chimes' template kept their `[bars] tracks` (1.5 and 5.9
times the fundamental, where it has 2.7 and 5.3) and the bars fit found no take on the first.
The wizard now asks how the instrument is played, after the files are read, in ten plain
answers (`projects/kinds.toml`), pre-selecting one when the recordings' overtones are a bar's;
the new project starts from the closest committed instrument of that kind (bars: the tracks
that explain the recordings' peaks best; strings: the key range), and what the recordings
decide is set from them: bars get `tracks = "auto"` (the fit derives them from the modes
analysis) and the assumed mallet or `pp_mirror` their dynamics need; strings analyze their own
release samples or borrow them and always borrow the dampers and pedal noise; a piano with one
dynamic borrows the felt. The derived tracks against the nine committed bars instruments' hand-set
ones: 25 of 36 found (the first rules found 9: the analysis's confidence rates the decay fit,
not the mode, and a bar's high modes leave its range in the treble); every first upper track
but the balafon's (6 keys) and the tubular bells' (whose first two lie below the reference).
The guess on the donors' own recordings, each left out of its answer: the string sets' bar
shares at most 0.50 (Osiris; the threshold two thirds), none pre-selected; four of the bars
sets pre-select their own answer, the xylophone (0.66) none, the kalimba, the hand chimes and
the balafon a fellow bar answer (their own left without a fitting donor), the tubular bells no
key (a tube's pitch is no partial). A donor's track counts a peak within 5 % at most: the
xylophone's tolerances (0.18, 0.30) made it the closest to any bar set. The glockenspiel of
`data/ethanwiner-soundfonts`, through the wizard touching only the folder, the SFZ file and the
name: set up like the Bells (0.25 against the xylophone's 0.11), its tracks derived from its
takes and built in its chain.

<a id="step-87"></a>
## 87. Glockenspiel, the first instrument made with the editor's wizard

Glockenspiel, the first instrument made with the editor's wizard: Ethan Winer's glockenspiel
(`data/ethanwiner-soundfonts`, ten bars G5–B7, one dynamic), through the wizard touching only
the folder, the SFZ file and the name. Its donor is the Bells (the closest overtones: 0.25
against the xylophone's 0.11), so their body, fit settings and level (8 dB under the
calibration); its tracks derived from its takes (2.74 → 2.66 times the fundamental, 5.3 →
4.9, 8.7 → 8.1, 12.7 → 12.1, 17.5 in the bass, and a short one 7.1 → 5.9 at D6–G6); one
dynamic, so an assumed mallet; no Mallet menu, so the Paper preparation acts (the engine's
rule, as for the crotales). The donor's comments were rewritten for this set by hand; it is
credited in `instruments/glockenspiel/LICENSE.md` though its author asks for none.

<a id="step-88"></a>
## 88. The final review's minors of step 86

A name typed on the wizard's last step keeps a menu name
typed by hand (the menu follows the name only while it was the name's). Finish is given its
donor instead of reading it back from the new file's first line. A set whose source names no
dynamics counts as one recorded (an assumed mallet), not as three. A bar share under two thirds
reads "not clearly a bar's" (from a third up; "a string's" below), and the bars fit's message
for auto tracks without an upper mode states its rule (a fifth of the keys, three at least).
Tests now cover a donor that borrows (Grand C's lenders, Osiris's felt from Grand F pass on),
a harpsichord donor (its own release samples analyzed, its buff stop kept) and two and three
bars dynamics (pp_mirror; neither key): no fault found there.

<a id="step-89"></a>
## 89. The wizard's guess on a second glockenspiel

The wizard's guess on a second glockenspiel (VCSL's, CC0: seven bars G5–C8 at three dynamics,
named an octave low). The names step kept the octave as written: its bars' second mode runs
3.04 → 2.68 times the fundamental (G5 → C8; Ethan Winer's 2.74 → 2.66, the Bells' 3.29 → 2.9),
outside the free-bar profile's 2.76 ± 40 cents, so no reading scored 0.5. The profile now takes
the committed bars' spread (the second mode 2.66 … 3.29, the third 4.9 … 7.2) and asks for the
fundamental and no mode at the octave (a string's third harmonic lies in the band, its second
at 2; a reading 19 semitones below a plate's partials put them in the bands). The string sets'
bar shares move by 0.02 at most (Osiris 0.52, Knight 0.50; two thirds pre-select); the
suggestion corpus keeps its 33 of 41.
With the octave set by hand, the donor came out as the Xylophone (0.50 against the Bells'
0.11): the score counted the share of a donor's tracks found within the donor's own tolerance,
3–4 % for the Bells, so their seven tracks lost to the Xylophone's two. The tracks are derived
from the recordings (`tracks = "auto"`); the donor gives the kind: its body (the frame, the
microphones, the tubes) and its level. The score now matches both ways at 8 % (each key's four
strongest upper modes explained by a track, weighted 1/rank, and the donor's main upper mode
found among them), times the share of keys whose fundamental decays as the donor's within a
factor 3. The measurements gain each strong peak's decay rate (50 ms frames over the first
2.4 s; the cache's version 3); the donors' come from their fits (`keys[].sigma[0]`: at C6 the
steel bars 0.5–1.4/s, the wooden ones 15–18/s). On the glockenspiel's keys: Bells 0.72,
Crotales and Vibraphone 0.23, the wooden donors 0; Ethan Winer's glockenspiel stays with the
Bells (0.50, the Crotales 0.22). On the donors' own recordings five bars sets pre-select their
own answer (the balafon too, before the tubes), two a fellow one (the kalimba, the hand chimes).
The findings, the octave's hint and the dynamics' note wrap at the window's visible edge (a
wide table above made the page wider than the window and cut them).

<a id="step-90"></a>
## 90. Glockenspiel B

Glockenspiel B: VCSL's glockenspiel (CC0; seven bars G5–C8 at three dynamics), through the
wizard touching only the folder and the name: the octave read from the pitch (+12, no
finding), soft, medium and loud as pp, mf and ff at their recorded levels, the mallet answer
pre-selected and the Bells as donor (the closest overtones and decay), built from the editor's
Build page (modes 1.9 s, bars 0.6 s, export 34 s). Its tracks derived from its takes: 3.03 →
2.68 times the fundamental, near 4.1, 5.1, 5.7 → 5.3, 9.6 → 8.3, 12.8 → 12.5; the fundamental
decays at 0.7/s (G5) to 4.9/s (C8). Its three dynamics lie 10 dB apart (medians −10.5, −4.2,
0 dB): the mallet's speeds come out 0.78, 1 and 1.15, the brightness carrying the rest, and the
lowpass's fit sits at two of its bounds (3 octaves per step, order 6). As recorded: at C6 the
takes' peaks span 17 dB soft to loud, their share above 4 kHz −21 to −2 dB; the render from
velocity 16 to 127 spans 16.7 dB, −17 to −2 dB (after step 91; the Bells 28 dB, Ethan Winer's
assumed mallet 35 dB). The UI shows it as the
other glockenspiel (no Mallet menu; recorded to C8). The donor's comments were rewritten for
this set by hand; CC0, so no `LICENSE.md`.

<a id="step-91"></a>
## 91. A bar's mode in a notch of the body

Glockenspiel B's C♯6 sounded 30 dB over its neighbors (its
peak +4.6 dBFS at velocity 100, the others −24 to −28): its mode at 5.52 times the fundamental,
6168 Hz, lies in the bars' board's deepest notch (−39 dB at 6167 Hz, −23 dB 7 Hz to either
side, −5 dB at 70 Hz). The export divides each mode's weight by the body's gain at its fitted
frequency, so it raised the mode 37 dB; the engine plays the key in tune (the set is 12 cents
sharp), 40 Hz below the notch, and the boost came out whole. Every bars export shares that
board: rendered at velocity 100, keys stood 6–15 dB over their neighbors on the Bells (E5), the
Crotales (C♯7, G7), Ethan Winer's glockenspiel (D6, G♯6, F♯7), the marimba and the vibraphone
(C♯6). The gain is now the response's power averaged over ±50 cents, the band a mode moves
with the tuning (`GAIN_BAND_CENTS`; `export_bars` version 2): no key of the eleven stands 6 dB
over its neighbors but the hand chimes' A5 (+11 dB, as before: not a notch), the median peaks
within 0.5 dB of before. A broad dip stays compensated (the Bells' 4.5 kHz, −14 dB over the
band). `proto/tests/test_radiation_notches.py`: the notch's gain over the band, and no audible
mode raised 6 dB past the band's response.

<a id="step-92"></a>
## 92. The hand chimes' A5

Its file (`sus_A4`, named an octave low as all of them: 880.8 Hz) is the
right one: the set has an A tube where the whole tones would have A♯ (G♯5, A5, C6). Its take's
clapper hit holds a line at 1151 Hz as loud as the fundamental in the first 40 ms (the other
takes' strongest 12–15 dB under it); VCSL normalized every file to its peak, which hid it.
The fit kept that strike (its loudest mode −10.8 dB re the fundamental, the tubes around it
−26 to −38 dB) over the tubes' smooth level curve: A5 stood 11 dB over its neighbors, A♯5 (no
tube, the nearest take's strike) 6 dB. A take's strike is now capped (`bars.cap_strike`,
`bars` version 2): its loudest mode at most STRIKE_OUTLIER_DB (10 dB) over the median of its
four nearest takes' (two each side), its modes lowered alike; the bass's louder strikes follow
the register and stay (C4 8 dB over its neighbors). A5 −11.9 dB, C5 −2.6 dB; rendered at
velocity 100 A5 stands 2 dB over its neighbors, A♯5 0 dB. No other set changes.

<a id="step-93"></a>
## 93. A piano's overtones read "not clearly a bar's"

A piano's overtones read "not clearly a bar's" on the wizard's "How is it played?": the grands'
bar shares were 0.28–0.45 (two thirds pre-select, a third reads "not clearly"). Two causes. Every
harmonic series holds 1, 4 and 10, the tuned bar's profile: the bar profiles now rule out a
harmonic series (peaks at twice and three times the fundamental; a bar's mode may lie at the
octave alone, the crotales' 2.15 → 1.91 on their middle keys, with nothing at 3). And a string's
treble, the fundamental and a weak octave, is explained by no profile (0.13–0.53), a bar's three
partials least poorly: a key now tells by its profile where one explains it (0.5, the pitch
check's threshold), else by its upper peaks at −30 dB or more (one more than 40 cents off every
whole number a bar's, whole numbers with the octave a string's), else nothing; the share counts
the keys that tell, and fewer than three read "the overtones do not tell". The string sets'
shares now 0 to 0.16, every one read as a string's (`kind_corpus` asserts it); the bars sets as
before (five pre-select their own answer, the kalimba and the hand chimes a fellow one); the
xylophone's keys tell nothing (its upper modes gone 50 ms after the strike, where the
measurement starts). The suggestion corpus 35 of 41 (was 33: the guitar, Glockenspiel B).
The Check page's table fits a 1600 px window: the file column takes what the others leave (the
name truncated, the path on hover), the pitch's and level's units moved to their titles' hover,
its scroll bar beside the last column; the key grid clear of the page's scroll bar.

<a id="step-94"></a>
## 94. An audio input that excites the strings

The plugin has two audio inputs now; summed, they
drive the sympathetic bank as a force on the bridge besides the strings' own (`Params::input`,
`Engine::process_input`, `pe_process`'s input buffer), and are not radiated themselves. The
bank already drives just the keys whose dampers are up and which have no voice of their own:
with the pedal down a noise burst leaves every string ringing, with it up only the 33
resonators of the undamped treble ring (nothing below the lowest undamped key's fundamental,
`engine/tests/audio_input.rs`). The input's path, by reciprocity: a pressure at the instrument
moves the bridge as the soundboard's response from the bridge to the microphones over jω (the
vertical body at the recording position, its channels averaged, through a leaky integrator at
20 Hz); the bridge admittance cancels against κ, which the prototype takes from it. Measured
against a flat drive on the ring's tail (white noise, pedal down, octave bands 44 Hz–5.6 kHz):
the body response alone moves the bands by ±5–11 dB, the response over jω by ±15–20 dB (bass up,
treble down): it matters, and stays. Normalized to a mean power of 1 over 100 Hz–4 kHz; level 1
(0 dB in the plugin) is the input in output model units × 2, the median over the pianos of
their strings' bridge force over the output it makes (an ff chord: −3 … +20 dB per piano, 7 dB
the median; the guitar 45 dB: its input drives its strings far less than its own sound
suggests). A −20 dBFS white noise at 0 dB, pedal down: the pianos ring at −28 … −35 dBFS
during it, −33 … −45 dBFS 0.1–0.6 s after it; pedal up −46 … −72 dBFS. The filtered input
drives the next block (64 samples late, 1.3 ms at 48 kHz: the sound's way across a meter takes
3 ms). Cost: one more convolution, mono, with the body's length: 4.2 → 4.4 % of a core with
the pedal down and 64-frame buffers (Grand A, C, D). The response is kept in the
`RadiationCache` by instrument and rate: an engine rebuilt for a tuning or a microphone setup
reuses it, and the rebuild takes as long as before within 0.5 ms (12–16 ms on Grand A, Grand D
and Harpsichord A); the engine taking over carries the input's state on. Level 0, the default,
leaves the output bit-identical (the convolution does not run). The report (`info.rs`) names the control inert
where no string has a coupling κ (`no_effect.rs` renders every export with a noise input);
the plugin hides it there (the bars, the kalimba, the balafon). The Copy CLI line leaves it
out: the MIDI file carries no input.

<a id="step-95"></a>
## 95. The editor compares the instrument with its recordings

The editor compares the instrument with its recordings (stage 3b). The editor links the engine (a
path dependency; `editor/build.rs` hashes its sources into the cache's key) and renders every
mapped recording of a fitted dynamic: the key played (a 4′ register's key, not its sounding
note), at the fit's speed for that dynamic (a mallet source at its mallet's own speeds, with its
index), a register's stop, held from silence for as long as the recording sounds (to its noise
floor, the quietest 0.2 s from 0.5 s after the onset, or a damper landing: every strong partial
decaying 4× faster than over the second before, placed at its knee; at most 30 s). Render and
recording are measured alike (`audio::measure_long` / `measure_like`): the level over 0.5 s, the
fundamental, per partial (from 0.7× the note's frequency up: a room's rumble is none; matched
after the fundamentals' ratio) an early (0.05–1 s) and a late decay rate and its level. Why: the
fits' own errors are uneven (per note for a plucked fit, one number for a piano, none per key for
bars) and measure an intermediate fit. The first corpus run flagged 80–100 % of the pairs
against zero: every instrument differs from its recordings as a whole (the render brighter by
a median 5–13 dB, the decay off by a steady factor, each dynamic's level by the velocity curve:
Grand D's pp 36 dB under its recording, its ff 10 dB), so a key is flagged where it
differs from its instrument's medians (the level per dynamic) by more than 12 dB, 25 cents, ×4
or 12 dB: 0–5 % of the pairs on most instruments (Harpsichord A 0/61, Harp A 1/42, Grand A
8/90), more on the Iowa sets (marimba 75/549, xylophone 58/264, Grand B 43/260, vibraphone
32/126); `editor/tests/compare_corpus.rs` holds each instrument's count under a ceiling. Over
all pairs re the medians, the 99th percentiles: level 17.5 dB, pitch 62 cents, early ×4.9, late
×5.9, spectrum +23.8 dB. Found, not changed here: the xylophone's render dies 9.4× faster than
its recordings after the first second (its median), the clavichord's is far darker (its upper
harmonics 20–50 dB under the recording's) and pitched 86 cents up (the set is at A = 415), the
vibraphone's pp sits 13 dB under its mf and ff re the recordings. Not caught: a key between the
recordings (Glockenspiel B's C♯6 of step 91 is none of its seven bars; its old export flags G5's
level). Rendering: building an engine took 0.13 s of a pair's time and rendering 2 s 0.03 s, so a
worker silences and reuses its engine (`Engine::silence`; a key struck again sounds as from a
fresh engine within −60 dB); one worker per physical core (16 on 8 cores ran slower than 8: the
engines' convolution buffers share the cache). The marimba's 549 pairs take 49 s, the corpus's
3 800 212 s the first time and 4 s from the cache. Recordings outside A0–C8 (16) are counted,
not rendered (a NoteOn outside them indexed past the engine's key arrays; the C interface
filters them). The whole-branch review's fixes: a worker that panics fails its pair or ends the
run with an error, the cache kept (it hung the page before); Space and Tab are taken out of the
input on the page (Tab moved egui's focus to the top bar, Space then clicked it); a recording cut
while still ringing (its last second still falling over 1 dB/s) is compared to its end, not to
where its quietest window lies; the page watches both export files and waits out a build. Not
caught: a key whose render rings on (a render's partial with no decay is left out, as one that
rises or beats: Grand C's and Upright B's treble unisons beat over the late window).

<a id="step-96"></a>
## 96. The Compare page per source

With several sources (the marimba's yarn, cord and rubber
mallets, a harpsichord's registers) the plot was crowded and every key of a mallet that differs
as a whole was flagged. Each source is now judged against its own medians
(`compare::medians_by_source`), every lane plots a pair re its source's medians (pitch and
spectrum too: one band fits all sources) and the titles list each shown source's medians;
checkboxes show or hide the sources beside the dynamics, and Flagged only hides the rest. On the
corpus the counts moved little (marimba 75 → 77, xylophone 58 → 56: their mallets' medians lie
within 1 dB and 0.8 dB of spectrum of each other); the ceilings were recorded again.

<a id="step-97"></a>
## 97. The Compare page's A/B in mono or stereo

The render played was the two outputs' mean, the
recording its file's channels: a stereo recording against a mono render. Mono (on by default)
mixes both sides to mono, as they are measured; off, both play in stereo, the render as the
engine's two outputs at the recorded microphones (`Renderer::render_stereo`; `render` is their
mean, `a_stereo_render_averages_to_the_measured_one`).

<a id="step-98"></a>
## 98. Level corrections per key and dynamic

Level corrections per key and dynamic (stage 3c). The user listened to
ten pairs the Compare page flagged: the level difference was right on all five level findings,
at the range's ends and at pp (the marimba's treble at pp too quiet, the vibraphone's bass at pp
too loud, Grand D's treble at ff too loud, Grand A's top key at mf and pp too quiet, the bells at
pp too quiet); nothing could correct one key at one dynamic. A source may carry
`[part.source.level_corrections]` (dB per recorded key at pp, mf, ff); the exports interpolate
it to every key (`proto/level_corrections.py`: linear, the ends held) and write each note's
`level_db`, one triple per mallet in mallet order, each register its own part's; the engine
interpolates a strike's gain in log₂ speed between the mallet's (or the velocity curve's) anchor
speeds and scales that strike's contact and new modes only (`Contact::level`, `pluck_with`):
+6 dB doubles a strike within 0.05 dB on the hammer and the mallet path, what a correction adds to
a re-strike on a ringing bar equals what it adds to the strike alone within −60 dB, no table
renders bit-identically. The Compare page's Correct levels proposes per source and dynamic the
correction the export carries (each recorded key's note's `level_db`, what the renders contain)
minus the residual's running median over five keys (clamped to ±24 dB): a table written but not
built yet is never counted twice. The corrections are a read of their own
(`project.level_corrections`), left out of `project.sources`, `project.*` and a borrowed
project's configuration, and no wildcard read covers it (a step reading it names it: the
export of a project that exports another's part names `uses.main:project.level_corrections`): a
table made every analysis and fit stale before (a refit of minutes). Found on the way: a reused render engine matched a fresh one only within −60 dB (its
strike at another phase of the convolution's 2048-sample ring), which moved the corpus's
near-threshold flags by which pairs a worker rendered first; it now strikes on a multiple of
4096 samples and renders sample for sample as a fresh one. No instrument is corrected yet: the
Compare page's measurements come first.

<a id="step-99"></a>
## 99. The Compare page's measurements, fixed by ear

The Compare page's measurements, fixed by ear. In the listening round
the level and pitch held; the decay misled: stopped recordings (Grand G's D7 cut, a bell stopped
by hand) read as renders ringing too long, the anechoic sets were compared against a render with
a room tail, the late per-partial rate overstated (Guitar's B4: ×9.7 measured, about 0.5 s
heard); and the Clavichord's noise read as brightness. The decay is now the early rate plus a
broadband length: the time to fall 30 dB from the first 0.1 s (0.5 s lay deep in a xylophone's
decay), or as far as the recording shows 10 dB above its quietest window (at least 10 dB; the
render to the same depth). A stop is a drop 10 dB under the line of the second before within
0.5 s that does not come back within 6 dB for 2 s: the spec's test (the fall over 0.25 s twice
as steep as the second before) marked about 2600 of 3300 recordings stopped (a guitar's level
jitters 5–10 dB frame to frame, a slow piano bass's wobble passes 2 dB/s); now 1–17 per
instrument are stopped before their fall, most stops found being the libraries' fade-outs in
the last 0.5 s. A noise floor is no stop (with it, the xylophone's 256 of 264 recordings and
Grand B's 219 of 260 were lower bounds). A stopped recording flags only a shorter render. The
spectrum counts the partials 20 dB above their own noise (before the onset, else the quietest
window that holds sound: a tail faded to digital zero is none; averaged in power), each side re
its own strongest heard partial (a rumble under an Iowa pp note was the recording's 0 dB: 273 of
3124 recordings had their strongest partial 10 dB under it). A file counts as cut while ringing
only when its last second falls along a clean line (r² ≥ 0.6): a floor wobbling a few dB read
as cut, its noise unread, on about half the marimba's and xylophone's recordings. `[project] recorded_dry` renders the Iowa percussion and guitar without a room (Grand B
is not anechoic). Each export whose output changes keeps its predecessor
(`projects/<name>/cache/previous-export/`); the A/B plays it as Original beside the recording
and the render. The renderer starts each render from the engine's parameters (a reused engine
kept the previous pair's mallet). A render runs to twice ×2.5 the recording's length (at twice
it, a render ringing on read ×2 at most: never flagged on about 300 pairs). The corpus (5 min
21 s): re the medians the length's p95 ×2.64, p99 ×5.46; at ×2.5 158 of 2641 pairs flagged on
it (×1.5: 687, ×3: 94), the threshold agreed with the user; the spectrum's p99 +11.8 dB (+25.7
before the noise and reference fixes); 349 pairs flagged in all; the round's Guitar B4 still flags (×0.37), Grand G's D7 no longer, the xylophone's E7
pp and the bells' C5 pp have no length (18 and 8 dB above their noise). The medians show the
pianos' renders ringing 20–25 % shorter than their recordings and the guitar's 2.4× shorter.

<a id="step-100"></a>
## 100. The length as the ear judges it

A second listening round on ten pairs (recording against
render) checked the length: right on the cut Grand G D7, the harp's E5 dying sooner, the bells'
A#6 ringing longer; wrong on the guitar's B4 (×0.37 flagged, the render right by ear), Grand A's
A4 ff (×2.70 longer, by ear a bit shorter) and Grand B's G#5 (×0.09: 24 s to fall 30 dB);
exaggerated on Grand A's A5 pp (×0.27, by ear about 1 s). Two causes in the tracks: a rumble at
20–100 Hz held the broadband level up in quiet tails (the guitar's strongest peak at 1.8 s at
20 Hz, the note's 494 Hz not among the six; Grand B's 22–102 Hz as strong as its 834 Hz from 4 s
to 24 s), and 30 dB under the first 0.1 s fell within the strike and a piano's first drop (Grand
A's A4 ff falls 33 dB in 1 s and rings on at −40 to −47 dB for 12 s; the render falls slower,
then faster). The length, the floor, the end and the stop now read the note's band (high-passed
at 0.7 × its frequency, 24 dB per octave), and the length is the last time the band is above 50 dB
under its level over the first 0.5 s (what Level match plays at), or as far as the recording
shows 10 dB above its floor; the render to the same depth under its own first 0.5 s. The ten
pairs now: the guitar's B4 ×0.85 re its median (not flagged), Grand A's A5 pp 6.1 s against
4.3 s, its A4 ff ×0.75, Grand B's G#5 ×0.92, the harp's E5 ×0.53 (by ear ×0.64: 4.5 s against
7 s). The corpus (7 min 11 s): the length's p95 ×2.14, p99 ×3.56 (×2.64, ×5.46 before); the
threshold set with the user at ×2 (173 of 3089 pairs, 5.6 %; ×2.5 87, ×1.75 274), 352 pairs
flagged in all. The medians: the pianos' renders ring 8–18 % shorter than their recordings
(20–25 % read before, partly the strike), the guitar's ×0.62. More recordings stop before a
50 dB fall (the libraries' fade-outs: Grand B and Grand E 54 each): lower bounds. Three of the
ten takes were broken (a key half released, a string touched, a bar badly hung): the measure was
right about the recording.

<a id="step-101"></a>
## 101. The marimba's levels corrected, the first instrument

The marimba's levels corrected, the first instrument (stage 3c's tables, step 98). The user
listened with the A/B's new **Level: Instrument** (each render with its source's median for the
dynamic taken off): with Level match off the marimba played 9 dB under its recordings at every
dynamic (the export's output gain), with it on the corrections were matched away. Two of the
Iowa percussion runs were split wrong: the marimba's rubber pp C2–B2 took a noise in its
silence for C#2 (C#2 silent, D2–F2 holding C#2–E2, F2's strike lost) and the vibraphone's pp
C4–B4 one for C#4 (61–64 holding 60–63); the first correction cut the rubber's pp bass from the
wrong recordings. `split_runs.py` now checks each cut's fundamental against its label (±2
semitones; the bars are not harmonic) and, where one holds its neighbor's note, chooses the
strikes again with each candidate scored by its fundamental over its neighbors' over 0.3 s (the
analysis frames' bins, 23 Hz at 96 kHz, hold several bass semitones); of the 1279 cuts only the
11 in the two runs changed (the others sample for sample). The two instruments refitted from the
per-note analysis on (the driver fingerprints no audio): the marimba's joint bars fit moved the
treble's second-mode damping 10–13 % (keys 77–87) and the rubber's pp speed −9 %, the
vibraphone hundredths of a cent; the pitch flags gone (the vibraphone's pitch p99 100 → 4
cents). The corrections from the clean recordings: the yarn's pp treble +10 to +16.5 dB (round
one's D6 pp "too quiet": +13.5), the rubber's pp bass −3 to −5.6 dB, mf and ff mostly within
±3 dB; the marimba's flags 20 → 5, its level p99 15.1 → 9.0 dB. By ear (recording, original,
render): the treble matches, the alternating top keys sit in line, the rubber's bass and the
quiet cord D5 are fine, the vibraphone's refit inaudible. Found on the way: a strike with an
unknown mallet number played as the first mallet but took the last mallet's correction.

<a id="step-102"></a>
## 102. The vibraphone's levels corrected

The vibraphone's levels corrected (the clean recordings of step 101). The Compare page's
proposal: the pp bass −3.8 to −7.5 dB (round one's C#3 pp "too loud": −5.7), the pp treble
+3.4 to +12.6 dB from C5 up (the marimba's pattern), mf and ff within ±2.6 dB; flags 8 → 4,
none on level, the level's p99 12.8 → 7.8 dB. By ear with Level: Instrument: the bass and the
deepest cut (A3 pp) match, the turn from cut to boost around C5 is smooth, the treble matches
its neighbors better; C#4 pp's early-decay flag (×14) is a wobble in the recording, the render
sounds as it should.

<a id="step-103"></a>
## 103. Level corrections by how far the recordings' levels are trusted

A pp scale on the corrected
marimba stepped 11 dB from B5 to C6 (the user heard it): the Iowa runs are not peak-normalized
(pp runs ~20 dB under ff) but each octave run has its own gain or touch (the yarn pp runs' peaks
−27.5, −26.4, −19.9, −26.7, −17.5, −25.6 dBFS from C2 up), and step 101's corrections copied the
C6–B6 run's 12 dB into the instrument (round one's "D6 pp too quiet" was heard against it).
Grand D's samples are normalized each to its peak: its own recordings say nothing of its levels.
A source's `level_groups` (else its `levels`: normalized → none) now decides: whole, octave or
file. With octave groups each run's median is taken off and the runs are joined where they meet
(the mean of the two keys either side): centered alone, a tilt repeated in each run left steps
of 3–5 dB at the boundaries; joined, the offsets stay within ±5.5 dB over the range. A ¼ ½ ¼
pass after the running median: the marimba's C5–B5 run alternates 4–7 dB key by key (alternating
sticking), which three of five kept. Files normalized: no correction, no level flag; octave
groups: a key's level judged re its run (`Medians::level_ref`), the A/B's Instrument too. The
marimba and the vibraphone corrected again from no table: within ±4 dB on most of the range
(the yarn ff −8.8, the vibraphone pp −6.0 … +8.3 as their runs' slopes join), every run
boundary within about 2 dB of the uncorrected render's step (B5→C6 −1.0 dB as before); flags
5 and 4. `level_groups` is left out of the driver's project.sources (the editor's alone): no
fit or export goes stale.

<a id="step-104"></a>
## 104. Grand A's levels: each key scaled by one gain

Its v16 peaks at −1.5 dBFS on every key; within a
key its layers share one noise floor (14–24 kHz late in the files, within 0.1–0.5 dB), the keys'
floors lie up to 15 dB apart (−113 to −128 dB). So the steps between a key's layers are recorded
(v1 15–33 dB under v16), its level along the keys is not (both labels said one half: the project's
`normalized`, data/SOURCES.md's "not normalized per layer"). A third label, `levels = "per_key"`:
the felt fit takes the level steps (`datasets.level_steps_recorded`: recorded or per_key) and the
two treble keys fitted to the step alone; `register` (levels along the keys) stays skipped; the
Compare page's level group `"key"` judges a key's pp and mf as the step under its own ff, re the
median step (ff neither flagged nor corrected). Grand A refit: hammer speeds pp 0.20 → 0.40, mf
1.10 → 1.50, ff 5.0 m/s; the felt an anchor curve, softer in the middle (C4 22.6 → 11.3 N), the
top hard (C7 91 N); the felt error 3.4 dB. The steps re the recordings (first 0.5 s): pp − ff
median −10.8 → −2.5 dB, rms 12.7 → 3.9 (from A6 up −16 … −25 → within ±3, F#7 −9.6: a take the
Accurate-Salamander remaster flags); mf − ff −5.7 → −2.8, rms 5.9 → 2.9: round one's "C8 pp and
mf too quiet". The borrowing projects' exports unchanged. Found with the Accurate-Salamander
remaster's measurements (github.com/cyamauch/NoctSalamanderGrandPiano: its gains per key and
layer track ours, r 0.83 at v1).
By ear (Level: Instrument): C8 mf right, C8 pp better but still quiet, the pp range and the
mid's mf fine. The Compare page's proposal with key groups written for pp only: within ±1 dB
from C1 to F#7 (C#2 … B1 +1.8 … +3.1 at the bottom), A7 +2.6, C8 +5.9 dB; its mf proposal (−3 dB
from C7 up) left out, C8 mf heard as right.

<a id="step-105"></a>
## 105. Take checks: flawed takes flagged on the editor's Check page, left out by ear

Flawed takes had
been found late, by ear (round two's Grand E D#2 pp, Guitar C#3 pp, Bells A#6 ff; the
Accurate-Salamander notes on Grand A), and a project could only leave out a whole key. A
source's `skip_files` drops files in both mappers before the take choice: where a map keeps one
take per key and layer (a `prefer` on a take number, an SFZ's `seq_position` or `lorand` filter,
read as the take rule) the next take takes the place; every committed source maps as before.
The editor's measurement keeps the whole decay (50 ms frames, up to 30 s) and a 1/6-octave
spectrum over 0.5 s. Three checks, each scored in robust spreads (1.4826 MAD, floor 2 dB or
2 dB/s) above the source's median, so one threshold serves every kind: the decay slope over
0.1–1.5 s against the same layer at two keys either side; the largest rise from 1 s above a
Theil–Sen line through the decay (0.25 s mean) that is also a climb (below); the brightness
(partials 4–16 re 1–2.5) against the line through the key's other layers, with three or more of
them (through two the line is extrapolated: Grand F flagged 11 of its 90). A spike on the known takes chose them: the decay
put Guitar C#3 pp first of 131 and Bells A#6 ff second of 123, the late rise Grand A's F#2 v2
second of 471; a second strike (C4 v9), footsteps (C4 v15) and non-partial peaks separated in no
measure (3–7 dB, the unison's beating; bell modes and phantom partials off the partials). The
first defaults (decay 4.5, rise 3.5, brightness 4.0) caught every known take, but the final
review found 14 of the 67 rise flags at 1.1 s, the window's start: a decay falling fast, then
slowly, lies above the line through its tail without ever rising (Grand A's F#2 v1 to v5 alike,
the two known ones among them). The rise now counts only as far as the level climbs above its
lowest of the 0.5 s before (and above the line): 3 flags left before 1.2 s, each a climb; a burst
reads ~1 dB lower (from the level just before it); F#2 v1 and v2 score 1.5 and 0.3, their flaw
in the strike ("bom-bom-bom"), left to the ear. The defaults, agreed with the user: decay 4.5,
rise 4.5, brightness 4.0 (Bells A#6 ff 4.6, Guitar C#3 pp 14.6, Grand A F#7 v1 4.3): 120 flags
over 3778 takes, 3.2 per hundred (`editor/tests/takes_corpus.rs` holds each source under its
ceiling). The grid's states by
shape (Okabe–Ito colors as a second cue), a legend, the sliders saved as `[take_checks]`, which
the driver leaves out of every fingerprint.

<a id="step-106"></a>
## 106. The first listening pass over the take checks' flags

Left out by ear: Grand A's C4 v14 and
v15 (footsteps; v15 ends early), Grand B's ff A0, B♭0 and B0 (late starts, the wrong notes) and
pp C1 (a B0), Grand C's C7 v1 (the file stops at 3.0 s while it sounds) and A7 v1 (cut at
1.4 s, played quieter); the brightness flags on Grand A's v1 kept (the layer's own color), the
guitar's late rises kept (its spares are the same pitches on lower strings, not other takes).
Two fixes the pass found: an SFZ set ranked by velocity renumbered the layers above a take left
out (Grand C's C7 v2 moved to layer 1; the fits read its dynamics from the file names and were
right, the grid and the same-layer comparisons not), so the ranks now come from the key's takes
before any is left out; and the late rise read one-step blips out of digital silence as rises
(Grand C's D4 v2: 17 dB to −97 dBFS), so it reads nothing below −90 dBFS. Without those blips
the VCSL sets' rises spread less: the harp, the strumstick and Upright Y flag one or two more
real climbs (12–17 dB, up to −54 … −77 dBFS). The rebuild: Grand B's lowest register gain
−0.04 → −0.84 dB; Grand A's global coupling fit, which reads the ff layer alone, landed in
another minimum of its rugged surface (8.02 → 7.95 dB; reactive 0.99 → 2.70, horizontal detune
0.22 → 0.02 cents, its bound) from the analysis and a 0.3 dB shift of the radiation's notch: the
fit is that sensitive to small changes upstream. The borrowers rebuilt (their pedal fits read
Grand A's: amplitude and error 9–23 % off), and Grand D (it reads Grand B's levels) failed in
its soundboard step: `Profile.felt` reads the felt line's `felt_bass_factor`, `felt_f05_at_c4`
and `felt_log2_slope_per_semitone` where a project has no `felt_curve` (Grand D, Upright Y),
undeclared since the driver checks reads. The seven steps that read the felt curve declare
them now; the other projects were adopted (they read none of them, or values unchanged since
their runs) and Grand D rebuilt from its soundboard step (corrections within 0.3 dB, the
knock's cutoff 3 → 2 kHz). Rendered, Grand A's new coupling rings longer (+1.1 … +4.8 dB after
2 s, the most at C2) and 3–6 cents flatter below C6; heard against the old one, clearer, the
aftersound better, not flat beside the other grands. The other changes were not audible.

<a id="step-107"></a>
## 107. Borrowed reads by name

The borrowing steps declared whole lenders (`uses.dampers:*`: the
lender's project file, every fit and cache), so any refit of Grand A marked the 21 projects
that borrow from it stale and rebuilt their pedal fits and exports for nothing (steps 104 and
106). A recorded run (`--record`) of felt, register, pedal and export on every borrower logged
615,907 reads; the steps declare what they read: the pedal fit and pedal noise of the `pedal`
lender, the damper and release caches of theirs, the buff's four keys, the felt lender's felt
line, body modes, analysis and strings, the levels lender's hammer speeds, analysis and
sources. The pedal step had named the `dampers` lender for the pedal noise (right only while
every borrower takes both from Grand A). `salamander2` keeps `uses.main:*` (it exports Grand A
again, its every fit). Every recorded read passes the new declarations, and so does a run under
the check; the projects were adopted (no value changed). `proto/tests/test_borrowed_reads.py`:
a change of Grand A's coupling leaves Knight's pedal and export fresh, one of its pedal fit
does not. The recorded run also showed register settle further when it runs again alone (its
knock and string scatter are seeds, written after it in the chain): ≤ 0.1 dB, outputs kept.
The take checks' thresholds are now written as shown (3.7, not 3.700000047683716) and in place.

<a id="step-108"></a>
## 108. The bells' pp, and how far one key sways a level proposal

The Compare page's level medians put
the plastic mallet's pp 1.6 dB under its mf against the recordings, steadily across the runs
(−1.0, −1.1, −2.3 dB): `level_corrections` pp +1.6 dB throughout, by hand. Its proposal along the
keys was not taken: the keys scatter ±3–4 dB within their runs (struck unevenly; the pianos
scatter as much, the marimba 2.7–3.1, the vibraphone 1.9), and one key struck hard at a run's
edge (B6 ff, +9.7 dB) moved every run above it through the joins (the mean of the two keys either
side), the ff curve reaching +10.7 dB at the bottom. Two other joins were tried and dropped: a
Theil–Sen line through each run (its slopes are noise at this scatter: plastic mf −10.7 … +8.9)
and the running medians at the run's ends (cut short there, one key flips them: the marimba's
sway 3.4–4.6 → 5.4–7.2 dB). Instead the page measures each source's sway: the table recomputed
with each recorded key left out, the largest change and the key (marimba 3.4–4.6 dB, its tables
approved by ear; vibraphone 2.5; bells brass 5.0, plastic 7.4), marked from 5 dB. The bells' C5,
the first note of every run, renders ~6 dB under its recordings with the plastic mallet and
4.7 dB over with brass (the runs' starts, not corrected); A7 renders ~10 dB over in every
dynamic with both mallets (likely a weak bar) and G7 ~5 dB under: left to the ear. Grand A's 16
layers against the engine (IDEAS 3) found three anchors enough for the fits.

<a id="step-109"></a>
## 109. Grand D's levels from Grand B's recordings

Its files are normalized each to its peak, so it
borrows Grand B's levels ([uses] levels: the same model, every note at real levels), but only
the felt fit's hammer speeds read them: the register step had been skipped since the project
files began, and the felt had no level steps to follow. Against Grand B's recordings (the
register's measure, 0.05–0.5 s), Grand D's model sat 2–3 dB low in the bass and 9–13 dB high in
the treble at mf and ff (the user had heard G♯6 ff too loud), and its treble pp fell 8–24 dB
further under its mf than Grand B's (D7 −22.6, E7 −24.3 dB; bass and middle within 1 dB). Now
the register step runs for Grand D against Grand B (gain +5.9 … −21.8 dB), and the felt fit
takes its steps between dynamics from the levels reference where the set has none of its own
(`fit_profile_felt.level_steps_from`), with the top treble's support keys fitted to them: the
treble felt came out harder (C7 22.6 N, F♯7 64 N; mean error 3.5 dB). Against Grand B: the
balance along the keys within ±2.4 dB smoothed at mf and ff; the treble pp step 7.8 → 6.6 dB
short in the median (G♯6 −7.8 → −2.0, D7 −22.6 → −10.3, E7 −24.3 → −14.5 dB), the top mf step
fixed (E7 −9.8 → −1.3 dB). The other pianos keep their own steps (adopted, their fits
unchanged). The Compare page's length flags on the quietest treble pp renders (peaks down to
−76 dBFS) were the level measure's −120 dBFS floor, gone with the louder pp.
