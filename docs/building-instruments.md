# Building instruments

Every instrument is a project. A project's chain of steps measures its recordings, fits the
model to them and exports the instrument. The editor ([editor.md](editor.md)) does all of this
without a text editor; this page describes the files and the commands underneath. The datasets
come first: [getting-data.md](getting-data.md).

## A project's files

`projects/<name>/` holds:

| File | What |
|---|---|
| `project.toml` | hand-written (or written by the editor): every choice for this instrument, each with its reason as a comment |
| `fits/<part>.json` | the fitted values. A part is fitted on its own: most projects have one, Harpsichord B two |
| `ledger.json` | which steps ran on which inputs |
| `cache/` | not in git: the analysis results, the fits' intermediate files, the logs. Except what a lender lends (below): tracked, so a fresh clone can build its borrowers |

What `project.toml` holds:

- **The sample sets** (sources): which file is which note and dynamic, as an SFZ map or a
  file-name pattern.
- **The model**: the scale model and traits of its strings, or the settings of its bars.
- **The fits' choices** for this instrument.
- **Borrowings** (`[uses]`): what it takes from another project. Examples:
  - the dampers (Grand E's, measured from its release recordings: every other string
    instrument's), Salamander's release noise (all but the harpsichords and Upright D, which measure their own)
    and pedal noise;
  - another set's levels: a normalized set's balance along the keys and the steps between its
    dynamics then follow that set's recordings;
  - another set's felt;
  - the harpsichords' buff stop.
- **The export's options and its menu entry** (`[menu]`): the plugin's name and group for it,
  its own labels for controls whose generic words do not fit, its mallets' display names,
  `reversed_keys` for a keyboard with dark naturals and light sharps (the harpsichords),
  `first` for the plugin's default choice (Weidinger's trumpet: a fresh instance and the Default
  preset play it; the menu's order stays), and `hidden` for a test fixture.
- **The recordings' credit** (`[license]`): `set`, `author`, `url` (left out for recordings not
  published), `terms` (quoted where the source words them) and `source_license` (`CC0`,
  `public-domain`, `free-use`, `CC-BY-3.0`, `CC-BY-4.0` or `CC-BY-SA-4.0`); `by` names who made the
  instrument when it is not this repository's author.
  `proto/license.py` writes `instruments/<project>/LICENSE.md` from it and from
  the blocks of every project reached through `[uses]`: the instrument is CC BY 4.0, or CC BY-SA
  4.0 where any of those recordings are ShareAlike, and music made with it is the player's;
  without a block it names no license (a project the editor makes starts without one). The
  exports write it; no step reads it, so a changed credit makes nothing stale: `python3
  proto/license.py --all` rewrites the files (`proto/tests/test_license.py` checks them).
- **The loudness** (`[export]`): `output_gain_db` (a bar instrument's: `gain_db`), dB on the
  export's output calibration (the median key's ff peak), so that a phrase in the middle register
  plays at the grands' median loudness (EBU R128, the mean of Recording and Player; steps 173,
  177). Only the export reads it.
- **Trims by ear** (`[impact]`): `trim_db`, (key, dB) points on the hammer's thump, linear
  between and held beyond (Grand A −12 dB, Upright C −20 dB up to C4 and −16 dB from C6). The
  export writes each note's `impact_db` and the engine multiplies its thump by it; the fit's
  own level stays as fitted, so a refit keeps the trim. Only the export reads it.
- **The felt's core by ear** (`[felt_core]`): `notes`, `f05` (N at 0.5 mm past the core's depth)
  and `depth_mm`, linear between the keys and held beyond (Grand A: 2–5 N in A0–C2, 3.25–1.25 N
  in F♯2–F♯3, none from C4, 0.2 mm, a quarter of its first curve, step 141; Grand F: 6 N to E♭2,
  none from C3, the depth just past mf's compression so mf stays). Only the export reads it: the
  fits never see the core (the board's refit with it tilted Grand A's levels by ±4 dB).
- **The felt by ear** (`[felt]`): `p`, the felt's exponent (1–5), the force at 0.5 mm kept and its
  growth past it changed (lower: less brightening from a soft strike to a hard one); `stiffness_db`
  (±12 dB), the felt's K scaled as the plugin's Hammer hardness scales it (a core keeps its own
  f05). Only the export reads it, before the core. Set from the recording at the velocities the
  export plays its pp, mf and ff layers at (the attack's 1–2 and 2–4 kHz re 200–500 Hz), then by
  ear; solved from mf and ff alone, a lower exponent makes soft playing too bright (step 142).
- **The board's tone by ear** (`[board]`): `trim_db`, rows of a band center (Hz) and dB (±12) over
  the octave around it, linear in log frequency to the next octave (adjacent rows join flat), added
  to every key's correction, so to the partials (Grand F: 63 Hz −4, 125 and 250 Hz −3 dB). Two more
  numbers on a row, keys, fade it in from 0 at the first to full at the second, so it holds a
  register (`[4000, -4, 60, 72]` the treble's upper partials from C5 up, `[250, -3.5, 52, 40]` the
  bass's from E2 down) and leaves the fundamentals beside it alone; such a row also spares each key's
  own fundamental (0 up to √2·f0, full from 2·f0), so the top octave, whose fundamentals lie in the
  treble's band, keeps its loudness. It plays
  after the even levels, which are measured without it (else they lift a trimmed bass back). Only the
  export reads it.
- **The board's response moved into the correction** (`[board]`): `transient_db`, rows as
  `trim_db`'s (±24 dB), added to the radiation (the body's response, the room's level, the body at the
  microphone positions) and taken off every key's correction, so each partial keeps its gain and only
  the sound between the partials changes: a strike's onset, the hammer's impact, the room's input. For
  a fitted response that tilts far over the keyboard while the correction lifts the treble back key
  by key (Grand G: 100–300 Hz 38 dB over 1–1.6 kHz; −15 dB at 40–320 Hz; step 181). The export
  measures how much the unit-energy body rises elsewhere and gives that to the correction and the
  room's gain too. Only the export reads it.
- **The board's ring** (`[board]`): `low_eta`, a loss factor (0.005–0.2) for the synthetic board's
  modes where it rings them longer than the fitted overlap does (its modes otherwise die in ~0.12 s
  each; at 4 % those below 450 Hz ring longer, 0.45–0.6 s below 200 Hz, as a grand's board glued into
  its rim measures). It acts on the bridge's admittance and the body's response alike; such a board
  radiates its modes below 100 Hz to both microphones in phase (independent from 400 Hz), and a
  radiation draw that now puts a fundamental deeper than −14 dB in a notch is drawn again. The
  pianos 4 % by ear (step 184), kept against a concert grand measured at the player's seat (step
  186). Only the export reads it.
- **The room by ear** (`[room]`): `trim_db`, rows of a band (250 … 8000 Hz) and dB added to the
  fitted room's own level per band, a cut or a lift up to +6 dB over the room's unit level (Grand C:
  8 kHz −12.6 dB: its synthetic tail carried the top octave over the dry sound, heard as a rattle;
  Upright B: 1 kHz +5.82 dB, its room uncut over its dry recordings). The partials' compensation for
  the room stays as fitted. Only the export reads it. A set whose gaps hold no room, or a room by ear,
  takes an assumed decay instead of the measured one: `[strings] room_t60`, rows of (Hz, s), or
  `"vcsl_dry"` (VCSL's dry studio, 0.2–0.45 s).
- **The ring by ear** (`[sustain]`): `percent`, (key, %) points (25–800 %), linear between and held
  beyond: each key's ring re the fitted one, every mode's decay rate divided by it as the plugin's
  Sustain control divides it (Upright A and B 150 % to C5, 200 % at C7; Upright C 150 %). The export
  writes each note's `sustain`; the engine multiplies the control's value by it, so the control's
  100 % is the instrument's own ring. Only the export reads it.
- **The ringing along by ear** (`[sympathetic]`): `percent` (0–300), the strings' sympathetic drive
  (the export's `transfer`) re the fitted one, as the plugin's Sympathetic resonance control scales it
  on a string instrument, so the control's 100 % is the instrument's own (Upright D 25 %: its undamped
  treble rang on its own, step 206). Only the export reads it.
- **The unison** (`[unison]`): `width`, the fitted detune of each key's strings scaled as the
  plugin's Unison width scales it, set by ear (0.05–2; Grand B 0.6: its beats ran 25–60 % faster
  than its recording's); `treble`, a factor on it along the keys (`[[key, factor], …]`,
  log-interpolated, held beyond its ends, 0.2–12): the fitted detune comes from beats below the top
  octaves, where the recordings' beats grow (`unison_check.py` measures both and the factor at the
  top key; the pianos ×1.8–12 at key 108, step 183). Only the export reads it.
- **Per source**:
  - `level_corrections` (from the editor's Compare page): `keys` (the keys played) and `pp`,
    `mf`, `ff` in dB per key. The export interpolates them to every key, holding the ends, as
    each note's `level_db`. The engine plays it as a gain on each strike, interpolated in log
    speed between the dynamics' anchor speeds. Only the export reads it, so a change leaves the
    fits fresh.
  - `level_groups` (`"octave"`, `"key"`, `"file"`, `"whole"`): how far its recordings' levels
    are trusted. Only the editor reads it; nothing goes stale.
  - `skip_files`: the files left out, as the editor's Check page writes them. They drop out
    before the take choice, so where the map keeps one take per key and layer (a `prefer` on a
    take number, an SFZ's `seq_position` or `lorand` filter), the next take takes their place.
    A layer left out whole stays empty, and the layers above it keep their numbers.
  - `pedal` (`{ kind = "files", down = […], up = […], ring = […] }`): the set's own pedal recordings,
    as recorded (`pedal_noise.py`; Salamander's, its four files, without it). The engine plays the
    downs in turn whatever the press, so a take far over the others (a stomp) sounds on every n-th
    press: the build warns of one 6 dB over its kind's median. `ring`, if given, are the pedal-downs
    the ring is fitted to (the downs without it).
  - `release` and `dampers` (`{ kind = "sfz", file = … }`): the release regions of an SFZ
    file; `release` also `{ kind = "rel_files" }`, the set's `rel1…rel88.wav` (Salamander's), or
    `{ kind = "silent" }`, a session's silent releases (`release_silent/`, keys released without
    their note: `split_session.py --silent`; Upright D), its level re the notes at each dynamic.
    `release` gives the release noise; `dampers` gives the damper loss, each partial's first
    fall after the key's release (`damper_decay.py`, the `dampers` step). A set without them
    borrows them (`[uses]`).
- **`[noises] perspective_db`** (`[[Hz, dB], …]`): a tilt for the release and pedal noises' band
  spectra, where the lender's microphones were closer than its borrowers' (Salamander's: −13 dB
  from 6.4 kHz up, design log step 121). Only those two steps and what reads their caches read it.
  Its `pedal_level_db` moves every pedal thump's level (not the ring's target: Upright D −4.5 dB, its
  thumps played over its recordings, step 206). Its `pedal_up_impacts` replaces the pedal-up takes' impacts (time, level, decay) with another
  recording's (Salamander's rattle with Upright A's landing, step 122).
- **`[release_noise] level_db`**: a level for the release noise the `release_noise` step writes,
  re the sample author's mix (Salamander's: −12 dB, design log step 120; the harpsichords' −4.5 …
  −14 dB put each at −38 dB re the note, step 182; Upright D's −14.5 dB, its recordings' level as
  played less 10 dB by ear, steps 222 and 224). Only that step and the exports read it.
- **`[take_checks]`** (`decay`, `rise`, `brightness`): the Check page's thresholds, only those
  moved from the defaults. Only the editor reads it.
- **`[project] recorded_dry = true`** on a set recorded anechoic (the Iowa percussion and
  guitar): the Compare page renders it without a room, as it was recorded.

`projects/kinds.toml` lists the editor wizard's answers to "How is it played?" and, for each,
the committed instruments a new one starts from.

`[bars] tracks = "auto"` (what the wizard writes) has the bars fit derive the modes' tracks from
the takes:

- each key's modes with a decay are followed from key to key;
- those found on a fifth of the keys that are not the fundamental's harmonics count, the six
  strongest.

The fit prints them and writes them into its fit document (`tracks`). Numbers in its place pin
them by hand.

## Running a chain

`proto/build.py` runs a project's chain of steps, the stale steps of the projects it borrows from
first. A lender whose recordings are not in `data/`, or that was built elsewhere (its committed
ledger, nothing of it built in this checkout: a fresh clone), is not rebuilt or judged: its
borrowers read what is committed of it. So the caches a lender lends (`uses.<role>:cache.<name>`:
Grand E's dampers, Salamander's release and pedal noise, Harpsichord A's release noise, the levels
and felt three projects borrow) are tracked in git, added with `git add -f`;
`proto/tests/test_fresh_clone.py` lists them and fails on one left out. A step that rewrites one
leaves it changed in `git status`, to commit with its fits.

```sh
just status grand_f          # each step: fresh, stale (and why), never run
just fit grand_f             # the whole chain: a refit, so a sound change
just fit grand_f --from soundboard
just fit grand_f --stale     # only what is stale
just fit grand_f --only knock,export   # only these steps
just export grand_f          # export again without refitting (just export: every instrument)
```

The recipes call `python3 proto/build.py status|build …`. Its other options: `--no-uses` leaves
the borrowed projects as they are, `--all` takes every project, `--json` gives the status as JSON
and a build's progress as JSON events (the editor's). `build.py adopt <project>` runs nothing: it
takes the present outputs as fresh (after a change known to leave them as they are).

Before a commit two checks say whether the committed files are what the tree makes:

```sh
just check                   # nothing stale in any project built here (~6 s; exit 1 with the stale steps)
just reproduce [grand_a …]   # each export made again into a scratch folder and compared (~2 min)
```

`just check` is `build.py status --check --all`: it passes over a project never built in this
checkout (no cache file git does not track). One built here whose recordings are not in `data/` is
named and passed over too; if that leaves nothing checked (a disk with the sets not mounted), the
check fails. A build whose lender is taken as committed fails before its first step when git lacks
a cache it reads of that lender, naming the read (`uses.<role>:cache.<name>`), and
`proto/tests/test_fresh_clone.py` fails when a committed lent cache is not the one its borrowers'
ledgers were built from (a lender refit, its cache left out of the commit). `just reproduce` (`proto/reproduce.py`) passes over a project never built in this checkout
(an export needs its caches) and exports each other project with `--out` into a
scratch folder (half the cores at a time, `--jobs`), from the fits and caches as they are and with today's code and engine, and lists
per file the fields that differ and by how much (dB fields in dB, release peaks as the dB of their
ratio). A difference is a sound change waiting for the next export: run it before merging a change
to `proto/` or `engine/`, and before committing an export. The release noise's reference peaks are
the one cache that is not a step's output: a memo of prototype renders, keyed by the analysis, the
body bands, the fit, the project's tables and the render's code (`release._render_sources`), so a
model change renders them again at the next export (~10 s a piano).

A step's `warning:` lines show in the build; the editor gets them as `warning` events. The
coupling fit prints one when a parameter moved twofold from the committed fit: a refit worth a
listen before it is committed. The coupling fit starts from the committed fit too, and keeps to
it unless another start is better by more than 0.1 dB. Its surface has minima of nearly equal
error far apart (design log step 114).

## The chains

The chains are in `projects/steps.toml`, by family. A project's `skip` leaves steps out: the
pianos borrow Salamander's release and pedal noise and Grand E's dampers and skip those steps (the
lenders run their own), and Salamander, scaled per key, skips `register`. A normalized set fits its
register against another set's levels instead (`[uses] levels`: Grand E and Grand H against Grand B's, the đàn
tranh against Harp A's).

| Family | Instruments | Steps |
|---|---|---|
| `struck` | the pianos | analyze, release_noise, dampers, pedal_noise, felt, body_decay, attack_noise, soundboard, coupling, room, register, knock, fine_structure, pedal, export |
| `plucked` | the harpsichords | analyze, pluck, body_decay, attack_noise, soundboard, coupling, register, knock, release_noise, export |
| `tangent` | the clavichord | as `plucked`, without release_noise (it borrows Salamander's) |
| `fingered` | the harps, the psaltery, the strumstick, the đàn tranh | analyze, pluck, polarizations, body_decay, attack_noise, soundboard, coupling, register, knock, export |
| `guitar` | the guitar | as `fingered`, with the measured body modes twice around the polarizations |
| `bars` | the vibraphone, the marimba, … | modes (per set), bars, export_bars |
| `buff` | the English harpsichord with and without its buff | fit_buff |
| `derived` | the registers test fixture `registers_fixture` | export |
| `rolled` | rolled pianos | roll (the engine's `roll` binary) |
| `brass` | the horn, the trombones, the tuba | brass_fit, brass_retune, brass_repair, brass_speak, brass_harmonics, brass_mutes, brass_land, brass_slurs, brass_level, brass_gain, brass_tables, brass_report, export_brass |
| `brass_lender` | the trumpet | as `brass`, with brass_mute_shapes before brass_mutes and brass_mute_residual after it |

A step reads and writes only what its entry declares. Under the driver, `analysis/project.py`
checks every access, and the ledger keeps a fingerprint of every declared read. A borrowed read
names its key: `uses.pedal:fit.pedal` is the pedal fit of the project lent as `pedal`. A refit
of the lender therefore leaves its borrowers fresh unless it changed what they read.
`build.py build … --record FILE` logs every access instead of checking it.

The Iowa percussion and guitar sets hold runs of notes per file. `python3 analysis/split_runs.py
<Instrument>` splits them once into `data/<set>/notes/`, a preparation of the data.

### The brass

A brass project fits a model, not a set of notes: the bore, the lips and the bell (`proto/brass/`), per key a grid over the lips
and the air, then the tuning, the speech, the harmonics, the mutes, the slurs and the level (design log step 234). Its fits
render through a player of their own (`proto/brass/live.py`), so the attack and the slur they judge are the ones the engine
plays; `proto/brass/port_check.py` holds the engine's players to it (step 235). The tables:

- `[brass]`: the model and the fits' settings: the range (`low`, `high`), the bore and bell, the lips, the fits' limits
  (`ff_cap`, `mf_cap`), the mutes the instrument takes, and the attack (`attack_rise_periods`, `attack_rise_hz`, `attack_lean`,
  `attack_rise_from`, `attack_from_rest`; steps 244–247). The driver fingerprints the whole table: a change makes every step
  stale. A value only the export reads (the attack's) is exported with `--only export_brass` and the chain adopted
  (`build.py adopt <project>`), as the ledger then says.
- `[uses] mutes`: the trumpet fits the mutes' shapes and the filter for what a shape cannot reach (VSCO's muted notes); the
  others borrow them, scaled with their bell.
- `[pose]`: where the bell points as played and where the player's ears are (the microphones' places, `docs/brass.md`).
- `[export] output_gain_db`: the level against the grands' (step 243).

The runs: `python3 analysis/split_brass.py [project …]` cuts Iowa's brass runs once into one file per note
(`data/iowa-mis-brass/notes/`), with a `<midi>.json` beside each: where it lay in its run, for the fits' background.

A rebuild from nothing took 19 min (the horn, no mutes) to 83 min (the tenor trombone) at nice 19, before step 241 made
the later steps five to ten times faster; `brass_report` writes what a person checks it by (keys not holding, landings, levels), and
`python3 proto/brass/compare_tables.py <project> --other <checkout>` compares two builds band by band. A rebuild is not a
reproduction: the mutes' search lands on other shapes, so the accepted fits stay until the ear prefers a rebuilt one.

## Developer tools

These tools take a part (`--profile`) or a project (`--project`) and write to `proto/out/`:

| Tool | What |
|---|---|
| `demo.py` | the listening demo, and the event list `render` plays (`projects/<name>/cache/<part>/demo_events.json`) |
| `compare_note.py` | one note: synthesis against the measurements |
| `spectro_compare.py` | whole-demo spectrograms, recordings against synthesis |
| `keyboard.py` | all notes: brightness change, centroid, energy decay against the recordings |
| `mic_check.py` | the microphone positions measured |
| `buff_check.py` | the buff stop against the recordings |
| `sample_demo.py` | the same demo from the original recordings (local A/B only) |
| `registers_demo.py` | Harpsichord B's register combinations |

## Listening, snapshots and the report

By-ear changes need a rail (design 2026-10-21-traits-plan): something fixed to hear and measure a
change against.

- **The listening set.** Schubert's Ständchen (`data/midis/staendchen.mid`): soft, its bass stays
  under velocity 92, so a change that acts only on loud strikes does not reach it. Jingle Bell Rock
  (`data/midis/jbr.mid`): its bass to velocity 114. Loud repeated notes from an arpeggiator while
  switching between instruments in the plugin: the clearest test of a change in the strike (Grand
  E's 2 and 4 N cores sounded alike in rendered notes and pieces, apart this way).
- **Fresh ears.** A sound decided one day is confirmed the next before it merges: long A/B days
  dull the ear, and Grand A's felt core, right on the day, was heard as overdone a day later.
- **Snapshots.** `just snapshot <ref> [names]` writes `instruments/<name>/` at a git ref (`main`, a
  tag such as `voicing-1`, a commit) into `~/.local/share/dussek/snapshots/<name>@<ref>` (`/`
  in a ref becomes `-`); the plugin lists them in the menu group "Snapshots", named "Grand A @
  voicing-1", their profile marked with the ref and its commit too (the engine caches the board's
  and the room's responses by profile: a refreshed `main` is another board for it). A snapshot is the old data played by today's engine: right for voicing
  changes, blind to changes in the engine's code; an export the engine no longer loads is refused.
  A snapshot run is all or nothing; snapshotting a ref again replaces its snapshots; `just snapshot
  <ref> [names] --remove` deletes them. `$XDG_DATA_HOME` replaces `~/.local/share` if set, as for
  the plugin; `--dest` writes elsewhere.
- **The report.** `just report [--snapshot <ref>]` writes `proto/out/report/index.html`: the
  grands (by their data: body, menu group; `--uprights` the uprights, into
  `proto/out/report-uprights/`, their envelope from the upright sets) with their traits along the keyboard (the felt's force
  at 0.5 mm, its exponent, the core, the hammer, the strings), and their bass attack (A0–B2,
  `bass_check`) against the envelope of the grands' recordings: the attack's change from mf to ff
  per band, a recording's band counted only where it lies 10 dB over its noise. Per grand and band
  the keys its engine leaves the envelope: a warning, not a veto. Then each engine against its own
  recording per dynamic, and a snapshot's lines beside today's. The tone per register
  (`tone_check.py`, alone: `python3 proto/tone_check.py <project>… [--instrument DIR]`): within a
  note its octave bands over the first second re its fundamental's band, the engine − its
  recordings per register and dynamic (a treble too bright, step 143), and the lowest octaves'
  fundamentals re partials 2–4 (a board fit held flat below its data, step 147). The unisons' beats
  (`unison_check.py`, not in the report, run alone as `tone_check.py`): per key from C4 up the
  beat of partials 1–3 in cents, the engine (room off) against its loudest layer, and the line
  their ratio follows from key 78 up (step 183). A trait fitted past the keys its
  fit measured (the strings' loss and the board coupling from the keys with enough decays and,
  above them, the recorded keys' fundamentals; the inharmonicity from the recorded keys) is drawn
  dotted there: the curve carries the last keys' slope on (the loss never falling toward the
  treble), an edge there is the fit's, not the piano's. A project whose recordings say
  nothing about the envelope leaves it with `[report] envelope = false` and its reason (Grand G: two
  soft layers). The first run renders every grand (~4 min); the renders are cached by content.
  `--snapshot` is repeatable and also takes a variant's label; a level section draws each key's
  ff level re the piano's median (the level it plays at every velocity), so the keys' evenness shows;
  and the board: each grand's fitted response H(f) (its color) and its correction per key as a map
  (key against frequency), where large or patterned corrections show what the fit absorbed.
- **Variants.** `just variant <project> <label> [--with table.key=value …]` exports a project into
  the snapshots folder as `<project>@<label>` ("Grand A (even)"), its project tables overridden for
  that export only (the value a TOML literal; `uses.<role>=…` changes what it borrows);
  `instruments/` and the previous export stay as they are. The route for trying a trait's setting
  by ear before it goes into the project file. Only the strings' export (`export.py`) takes it.
- **Even levels.** A project's `[even_levels]` voices the piano to one level curve along the
  keyboard: the export measures every key's level as the engine plays it (its stored level's
  `key_ff_db`), takes a trend through the keys' ff levels (a running median over `window_keys` keys,
  default 9, then ¼ ½ ¼; the keyboard's slow shape kept), and adds each key's way to the trend to
  its level correction at every dynamic, plus a seeded scatter (`scatter_db`, `seed`); then it
  measures again. Only the ff level counts: the engine plays each velocity at the speed where the
  key sounds a set level under its own ff, so the correction shifts the key's level at every
  velocity and leaves its speeds, its tone, as they were. `target_keys` and `target_db` (anchors,
  dB re the piano's median) set the shape from outside instead of the piano's own trend (Grand A's
  register from the other grands'). The pianos export so (the grands step 139, the uprights step
  168): mint, the unevenness a played piano has comes back at runtime with the plugin's
  Condition. A `--with` that no read takes (a mistyped table) is refused. The board, the body and
  the strings stay as they are: a key that meets a resonance or a notch of the board keeps its tone, not its
  jump in level.

## The modules

`analysis/` measures the recordings:

| Module | What |
|---|---|
| `analyze.py` | partial frequencies, inharmonicity, decays and tuning per take, with a confidence per measurement |
| `modes.py` | a bar's, tube's or tine's modes by peak picking, their decays, the strike and the buzz |
| `split_runs.py` | the Iowa runs cut into one file per note |
| `split_session.py` | a recording session's passes (`recording-a-piano.md`) cut into one file per strike, the takes checked and ranked, the set's SFZ; a pedal pass into pedal-downs and pedal-ups |
| `body_decay.py` | the decay between the partials: soundboard and room |
| `attack_noise.py` | the attack transient (hammer and action noise) per band |
| `release_noise.py`, `pedal_noise.py` | from a set's release and pedal samples (Salamander's, or a set's own pedal files; the harpsichords' own release samples, their notes' partials taken out; a session's silent releases): the release noise, the pedal's thump and ring |
| `damper_decay.py` | the dampers: each partial's first fall in a set's release recordings (a source's `dampers`) |
| `datasets.py`, `mapping.py`, `project.py` | the sets, a source's files → notes and dynamics, the projects' files and the steps' checks |
| `survey.py`, `summarize.py` | a set's quality before fitting it (lengths, decays, endings, noise, mislabeled notes); an analysis's label mismatches and overview plots |

`proto/` is the offline model and its fits:

| Module | What |
|---|---|
| `string_model.py` | hammer (or pluck) + coupled modal strings, one note |
| `scale.py` | per-note parameters across the keyboard (measured + design curves) |
| `board.py` | the synthetic soundboard: bridge admittance + radiation modes |
| `soundboard.py` | the transfer H(f) identified; body and room radiation filters |
| `fine_structure.py` | the partial-to-partial irregularity of decays and levels against the recordings |
| `fit_profile_felt.py`, `fit_hammer.py` | the felt and hammer speeds against the spectrum change between dynamics |
| `fit_coupling.py` | string loss and unison coupling against the full per-partial decay curves; on the pianos then the horizontal polarization's level at C2, C4 and C6 against the aftersound's level (partials 1–3 over 1–2.5 s re their peak, every take, an octave's median) |
| `fit_pluck.py`, `fit_polarizations.py`, `fit_body_modes.py` | the pluck point from the notch pattern; the two polarizations per note; a body's few strong low modes |
| `fit_knock.py`, `attack_compare.py` | the action knock (the attack transient): its shape from the attack's first 15 ms, on the pianos its level from the treble's attack peak at 200–700 Hz (both erring under the recordings); and its check |
| `fit_room.py` | the pianos' room cut per octave band where the strings' onset drives it past the sound between the recorded partials (the partials keep their level) |
| `fit_buff.py` | the buff stop's loss |
| `damper.py`, `release.py`, `pedal.py`, `resonance.py` | the dampers, the key release noise, the pedal, the sympathetic resonance |
| `register.py` | the radiated level and band shape per note against the recordings; `--fit`: the register gain |
| `bars.py` | the bars' modes, mallets and harmonics |
| `level_corrections.py` | a source's level corrections interpolated to every key |
| `export.py`, `export_bars.py`, `export_brass.py` | a project → `instruments/<name>/` |
| `even_levels.py` | the even levels' trend and per-key corrections (`[even_levels]`) |
| `snapshot.py`, `report.py` | the rail: snapshots of a git ref, the report |
| `tone_check.py`, `unison_check.py`, `bass_check.py`, `early_drop.py`, `felt_bands.py` | the engine against its recordings: the tone per register, the unisons' beats, the bass above 1 kHz, the low partials' early drop, the attack above 2 kHz |
| `build.py` | runs a project's chain and tracks what is stale |
| `cost.py` | the real-time cost estimate per component |

## Harpsichord B's registers

Harpsichord B has three registers and a lute row. The parts `flemish8` and `flemish4` each run
the harpsichord chain; the 4′ is analyzed at its sounding note.

The export then joins them (format 6) and derives the second 8′ and the lute from the 8′. There
are no recordings of those two: their values are constants in `export.py`, tuned by ear. The
project's `[export]` names the main part (`main`), the added register (`registers`: the 4′, an
octave up) and `choir = true` for the 8′ II and the lute.

- The 4′ plays through the 8′'s body. Its transfer and the recorded balance are folded into its
  correction.
- The 8′ II is the 8′ plucked one jack row nearer the nut, detuned by up to ±1.5 cents.
- The lute plucks the 8′ II strings 2.5 cm from the nut.

Harpsichords E and F have two recorded 8′ choirs instead: Edinburgh's Baillon recorded per
manual, its Barton per choir. Each choir is a part of its own (`harpsichord_e_lower` and `harpsichord_e_upper`,
`harpsichord_f_right` and `harpsichord_f_left`), and the added register names its stop: `registers = [{ part =
"harpsichord_e_upper", octave = 0, name = "8′ II", stop = 2 }]`. The engine plays a register by the stop
its file names (format 6's `stop`, its place in the list without one), and the plugin shows the
stops an instrument has. No 4′ and no lute: neither set recorded them. A project's id and its
parts' ids differ (a part named as its project hides the other parts from the build).

To hear them:

- `RENDER_STOPS=7` (for `render`) and `BENCH_STOPS=7` (for `bench`) play all three registers;
  `RENDER_PARAMS=lute=1` plays the lute.
- `python3 proto/registers_demo.py --project flemish` renders the combinations.

## The buff stop

The harpsichords have a buff stop (the "Buff" toggle): pads on the strings that make the notes
shorter and darker. Its loss is fitted once, from VCSL's English harpsichord recorded with and
without its buff (the project `harpsichord_buff`). It is exported into the instruments that
borrow it.

- `python3 proto/buff_check.py` compares it with the recordings.
- To render with it: `RENDER_PARAMS=buff=1` for `render`, `--param buff=1` for `midi2wav`.
