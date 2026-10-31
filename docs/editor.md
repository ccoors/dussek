# The editor

`editor/` is a program over the project files and `proto/build.py`. With it you can:

- make an instrument of an existing kind from a sample set, without a text editor;
- compare an instrument with its recordings and correct its levels;
- roll new pianos from the existing ones.

It needs the Python environment of [building-instruments.md](building-instruments.md), since it
runs the chains through `build.py`.

```sh
just run-editor        # or: just editor, then editor/target/release/dussek-editor
```

The projects are listed on the left, under **New instrument…** and **New rolled piano…**. An open
project's pages (Project, one per source, Build, Compare) sit along the top, beside **Save** and
**Discard edits**.

## New instrument…

The wizard walks through the steps of a new project, and each page says what it is for. The steps
are listed on the left; a step done can be clicked to go back. Nothing is written before Finish;
Cancel also unlinks a folder it linked in.

1. **Folder.** **Choose…** opens the system's folder picker. The folders under `data/` are
   listed beside it, and a path can be typed; **Use this folder** takes it. A folder elsewhere is
   linked in as `data/<name>`, with a stub in `data/SOURCES.md` for its origin and license.
2. **How to read the files.** From an SFZ file, or from the file names. The suggestion runs:
   - From an SFZ file it reads:
     - the filters: attack regions, pedal up, the first round robin;
     - the release samples;
     - a region's key: from `pitch_keycenter` (60 where it is left out), `lokey` or `key`, as a
       number or a note name, moved by `<control>`'s `octave_offset` and `note_offset`. The
       `#include`s and `#define`s are expanded first.
   - From file names it reads the note (a name or a number), the dynamic, the takes, and a word
     that splits the files into groups. Files whose names begin with "rel" are the release
     samples.
   - The pitch of each file decides between the readings the names allow: the octave
     convention, a MIDI number or an index, a set tuned away from a = 440 Hz. Where it cannot
     decide (bells, bars whose partials fit no profile), the plain reading is kept and a finding
     says so.
3. **The file names.** Each part of an example name gets its role: fixed text, a note name, a
   MIDI number, a number counted from a key, the dynamic, the take, a group, any digits or
   letters. The regex is written from the parts. The page also sets the octave, the key the
   numbers count from, and whether only the lowest take is kept. A group makes one source per
   value at Finish (a mallet each). For an SFZ file it shows the folders its regions use, the
   one preferred, and its region filters.
4. **Dynamics** (skipped when the set has one layer). One row per layer, with its files and
   measured level. The page sets which layers are kept and which is pp, mf and ff (at least one);
   **Order by the measured level** and **Propose pp, mf and ff again** redo the proposal. It also
   says whether the levels are:
   - as recorded;
   - normalized per key: one gain per key, the steps between its layers as recorded;
   - normalized per file.
5. **How is it played?** Ten plain answers: grand or upright piano, harpsichord, clavichord,
   harp, zither, guitar or lute, bars struck with a mallet, bells and tubes, plucked tines.
   - The recordings pre-select one when their overtones are a bar's. The test uses one file per
     key from the mf layer, on two thirds of the keys that tell: a pure tone tells nothing, nor
     does a string's treble octave.
   - A string's overtones are a piano's as much as a harp's, so for strings nothing is
     pre-selected.
   - The new project is set up like the closest instrument of that kind, its donor: its body,
     its family's fit settings, its traits and its export. For bars the donor is the one whose
     modes match the recordings' both ways within 8 % and whose fundamental decays alike (steel
     or wood). For strings it is the one with the closest key range.
   - A collapsed list offers every donor.
6. **Check and listen.** The grid and the table (below). With a group, a menu picks the source
   shown, and each source's skipped keys and relabels are its own. A source of the group that
   reads no file stops here.
7. **Name and menu.** The name (lower case, digits, underscores) becomes the folder under
   `projects/`; a name or id that is taken stops here. The page also sets the name and group in
   the plugin's menu. Finish writes the project from the donor's file (its first line says which
   donor and why), with what the recordings decide:
   - bars get `tracks = "auto"`; with one dynamic recorded they get an assumed mallet, with two
     `pp_mirror`;
   - strings analyze their own release samples or borrow them;
   - a piano always borrows the dampers (the donor's lender: Grand E's, measured from its release
     recordings) and the pedal noise (Salamander's material), and with one dynamic the hammer
     felt;
   - strings skip the register step unless their levels are as recorded or borrowed
     (`[uses] levels`);
   - the menu's mallet names: one per mallet source (none for a single mallet).

   With **Build it now** ticked (the default), Finish then builds the whole chain.

## A source's page

The same pages show as tabs: Folder, Reading, Names, Dynamics, Check, Source settings. Advanced
shows every key as TOML text, for what the tabs do not show. Source settings holds the role, the
mallet's name, the tail cut and the octave, and the lists the Check tab fills (with Remove).

- A tab's change is applied at once, and only the keys it changes are rewritten. Save writes
  the file.
- **Rename** gives the source a new id. **Suggest again** proposes the whole rule from the
  folder's files again, in place of the current one.
- Switching between the SFZ file and the file names keeps the other way's choices until another
  folder is chosen, the edits are discarded or the file is reloaded.
- A map the parts cannot show (an alternative at the top of its regex, an anchor) stays custom:
  Advanced edits it, or **Reset to the parts** proposes them from the file names again.
- **Add a source…** (on the project page) runs the steps from the folder to the check on the
  open project; the folder's page also takes the new source's id. Save keeps the sources added.
- A string project's main source takes the project's name: the fits take one id for a part and
  its main source.

## Check

The grid shows each key and layer, its state by shape. A legend sits above it, and the colors
(color-blind-safe) are a second cue:

| Shape | State |
|---|---|
| solid | one file |
| split | several files |
| hollow | pitch suspect: more than 50 cents off the set's pitch |
| striped | a flagged take |
| dashed | a take left out |
| dotted | a spare take in use |

Three take checks flag suspects; they never leave one out:

- **Decay**: a decay unlike the neighboring keys' at its layer (the slope over 0.1–1.5 s).
- **Late rise**: a noise, a buzz or a second strike. It counts as far as the level climbs above
  its lowest of the 0.5 s before and above the decay's robust line. A decay that falls fast,
  then slowly, never climbs, and nothing below −90 dBFS counts.
- **Brightness**: off the line through its key's other layers (with four layers or more).

Each check is scored in robust spreads above its source's own median and flagged past a slider.
The defaults (4.5, 4.5 and 4.0; **Reset** restores them) flag a few per hundred takes, and every
known flawed take of the sets. The sliders are saved as the project's `[take_checks]`, which no
build step reads.

A click on a key in the grid lists its files in the table (again: every key). In the table:

- A row plays its file. Spare takes are listed dimmed under their take, and left-out ones at
  their key.
- The Check column says why a take is flagged.
- A click on a column's title sorts by it in its own order (again: the other way): keys by
  pitch, dynamics softest first, file names with their numbers in order, measurements by value.
- "Leave this key out", "Leave this take out" (or "Use the spare take", where the map keeps one
  take per key and layer and has another), "Put this take back" and "It is really" write `skip`,
  `skip_files` and `relabel`.

Below the table are the files the names do not fit, and what the map left out with the reason. A
take left out whose file is no longer in the set can be forgotten (**Forget**).

## The project page and the build page

**The project page** shows:

- the menu's name and group;
- each borrowing (`[uses]`), as a menu of the projects;
- the family's steps to skip, as checkboxes;
- every key as text, under Advanced;
- the sources: each opens its page; **Up**, **Down**, **Remove** and **Add a source…**.

**The build page** shows each step of the chain, fresh or why it is stale, read in the
background (**Refresh status** reads it again). A borrowed project's steps are listed below; its
stale ones are built first.

- **Build stale**, **From here** and **Only this** run steps; the edits are saved first.
- The page shows the running step and its log. **Cancel** stops the step and restores its files.
- A project the chain would fail on is not started: a source that maps nothing, dynamics that
  name no layer, a source without dynamics or levels, a string part without a source of its name.
- While a build runs the project is read-only; a suggestion that finishes meanwhile waits for
  it.

## Compare

Compare is available once the project is exported. It renders each recorded key and dynamic
with the instrument, with the engine linked into the editor:

- the key played, at the fit's speed for that dynamic;
- a mallet source with its mallet, a register with its stop.

Each render is measured as its recording is, for as long as the recording sounds: to its noise
floor or to where it was stopped, at most 30 s. Its shape is measured scaled to its recording's
level (the measures' floors are absolute), its level as rendered. The renders leave out the strike
variation. A dry set renders without the room. Recordings outside the keys A0–C8, and layers no
fitted dynamic names, are counted, not rendered; keys between recordings are not compared. While
the build page shows a stale step, the page says the export compared is from before it.

**Four lanes** along the keys:

| Lane | Measure | Flagged past |
|---|---|---|
| Level | over the first 0.5 s | 12 dB |
| Pitch | the fundamental's | 25 cents |
| Decay | the partials' early rate to 1 s (circles); the length (diamonds) | ×4 (early), ×2 (length) |
| Spectrum | the partials' levels re each side's strongest, only those 20 dB above their own noise | 12 dB |

The length is the time the note's band (from 0.7 × its frequency up) takes to fall 50 dB under
its level over the first 0.5 s, or as far as the recording shows above its noise. The render is
measured to the same depth. This is the ring the ear judges, not the strike, and no rumble under
the note counts. A noisy recording's hiss is not brightness.

A recording stopped before its fall is a lower bound, drawn as a hollow diamond, and only a
shorter render is flagged. A stop can be a damper, a hand or a fade-out: the level drops 10 dB
under its trend within 0.5 s and stays down.

A key is flagged where it differs by more than the thresholds above from how the whole
instrument differs. That reference is each measure's median, for the level per dynamic. A render
brighter or faster everywhere flags nothing. With several sources, each mallet or register is
judged against its own medians.

- The lanes show each pair re its medians; the titles show the medians themselves.
- The flagged pairs are listed, the worst first (or by key).
- Checkboxes show or hide the dynamics and the sources (the mallets by their menu names).
  **Flagged only** hides the rest.

**Listening.** A click plays the pair. Which sounds play:

- the recording;
- **Original**: the instrument before the last build. Each export whose output changes keeps
  the one it replaces in `projects/<name>/cache/previous-export/`;
- the render;
- **A then B**: recording then render;
- **All three**;
- **Swap**: the next side at the same time since the onset. Tab cycles recording, original,
  render; Space plays the recording or stops.

**Level** sets the renders' levels:

- **Off**: as rendered.
- **Pair** (the default): each render at its recording's level.
- **Instrument**: the source's median difference for the dynamic is taken off. What a level
  correction changes is heard, and the instrument's output gain is not.

Everything is mixed to mono, as it is measured, unless **Mono** is off. Then it plays in stereo,
the renders at the recorded microphones.

**Correct levels…** (once every pair is measured from the current export) proposes a correction
per source and dynamic with three recorded keys or more, from the level lane:

- each recorded key's difference from its median, smoothed by a running median over five keys
  and a ¼ ½ ¼ pass (a run played with alternating sticking, every other key louder, is not
  copied);
- taken off the correction already in place, at most ±24 dB;
- shown as a curve along the keys.

How far a source's recorded levels are trusted follows its `level_groups` (else its `levels`):

| Group | Trusted | Example |
|---|---|---|
| whole | as recorded | most sets |
| `"octave"` | within each octave run C…B: each run's median taken off and the runs joined where they meet, so the slope within a run is followed but no run's gain, nor a step at its boundary | the Iowa percussion, each run recorded at its own gain or touch |
| `"key"` (from `levels = "per_key"`) | within each key: a key's softer dynamics judged as the step under its loudest, re the median step; the loudest neither flagged nor corrected | Grand A, each key scaled by one gain |
| `"file"` (from `levels = "normalized"`) | not at all: no correction, no level flag | normalized files |

The level lane and its flags judge a key against the same reference.

- Each source also shows how far one recorded key alone moves its proposal (the table
  recomputed with each key left out), and which key. From 5 dB it is marked, to be checked by
  ear before writing: a key struck hard at a run's edge moves every run above it.
- **Write to project** sets the proposal as the source's `level_corrections`; Save, then Build,
  applies it. A piano plays each velocity through its level table (design log steps 124, 138):
  there only the ff correction sets a key's level, and a pp or mf one moves the soft keys' speeds.
- **Clear corrections** removes them.
- A proposal starts from the corrections the export carries (what the renders contain), never
  from a table written but not built yet, so none is counted twice.
- The table itself is edited in a text editor, as `[strings]`, `[bars]` and `[export]` are.

The results are cached per export and engine (`projects/<name>/cache/compare.json`). A whole
instrument takes under a minute the first time.

## New rolled piano…

This page makes a piano no recording holds, from the exported hammer pianos (design log steps 117–118):

- each key's strings and hammers are blended from three grands, none over 55 %;
- the body is set by two knobs, measured on the ten real pianos' radiation responses:
  - **Warmth**: cold and clear … warm and round, the body's bass against its treble;
  - **Air**: dry … airy, how long the body rings;
- the responses' fine structure is mixed from three other grands. Their own bass is kept, so
  both microphones' bass stays in phase;
- each register's loudness is the real pianos' average.

**Choosing.** **Roll again**, **Re-roll strings** and **Re-roll fine structure** draw anew. A
table of the grands sets each one's share of the strings and hammers by hand (the others keep
their proportions), and which grands give the fine structure. The heaviest share is the frame,
which gives the rest of the strings' piano. **References and sources** chooses the pianos the
knobs and the realism check measure against, and the grands the draws come from.

**Rolling.** A change stops the roll running, and the page rolls again in the background once the
change settles (for a slider, when it is let go).

- The candidate is written to `instruments/rolled_preview/` (git-ignored). The plugin lists it
  as "Rolled piano (preview)" and reloads it within a second: play it in the standalone, at every
  microphone position.
- A roll takes about 7 s. The first one also measures the references, about 15 s more. They are
  kept in `~/.cache/dussek/roll` (`$DUSSEK_CACHE/roll` when it is set) until the
  program or an export changes.
- Only the newest roll writes the preview. Cancel, or choosing another project, removes it.

**The realism check** measures each candidate as the real pianos are measured:

- the brightness and the decay per register;
- the body's ring;
- Player against Recording;
- the two microphones' bass in phase, and their colors within 3 dB of each other from 60 Hz up;
- no key's fundamental in a notch of the radiation. A mix of responses puts one there for most
  donors, and a roll lifts it out.

The check names what falls outside the real pianos' range. The sliders are built as set; the
check is a guide. Roll again's draw is pulled back toward the middle until it passes: each knob
× 0.75, up to four times, as long as each try mends a quarter of what is off. The sliders then
show what was built.

**Finish** writes `projects/<name>/project.toml`: family `rolled`, the recipe under `[roll]`,
and every project it uses under `[uses]`.

- When the preview plays the recipe, Finish also writes the preview's export as
  `instruments/<name>/`, and the plugin lists it at once.
- It then builds the project. Its one step, `roll`, runs the engine's `roll` binary into
  `instruments/<name>/` (the same files), with a `LICENSE.md` crediting every source and
  stating its own license (CC BY 4.0, or CC BY-SA 4.0 when a source is ShareAlike).

A rolled project's page shows the same controls. Save writes them into its file and keeps its
other lines.

## Edited in a text editor

`[strings]`, `[bars]` and `[export]` are shown as written; edit them in a text editor. The
editor reloads the file when it changes on disk, and asks first when it has unsaved edits of its
own.
