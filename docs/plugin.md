# The plugin

A DPF wrapper (C++) around the engine's C interface: LV2, CLAP, VST3 and a JACK standalone.
LV2 and the standalone are the main targets; all four are tested on Linux and Windows.

Where a host offers LV2 or CLAP, use them: the VST3 is kept for hosts that load nothing else.
In VST3 the shipped presets are missing from the host's own preset menu (the plugin's Preset menu
has them), a host's generic controls list 2080 hidden MIDI parameters before the plugin's own (a
VST3 receives MIDI controllers, the sustain pedal among them, only as parameters), and the
Analysis and Visualization panels work only in hosts that pass the plugin's output values on to
its editor. All three come from DPF, the framework the plugin is built on.

## Building and running

```sh
just plugin        # DPF (a git submodule, plugin/dpf) at its pinned commit; cmake into plugin/build (builds the engine too)
just install       # links: ~/.lv2, ~/.clap, ~/.vst3; instruments/ into ~/.local/share/dussek
just uninstall     # removes those links
just standalone    # the JACK standalone under PipeWire
```

`just install` makes links, so a rebuild updates the installed plugin.

`just standalone` (`plugin/tools/run-standalone.sh`) runs the JACK standalone as a desktop app,
through `pw-jack`:

- Its outputs are linked to the default sink's front left and right.
- An ALSA MIDI port named "Dussek MIDI" (`a2jmidi_bridge`, from a2jmidid) is linked to its
  MIDI input, for programs that play to ALSA MIDI devices: `aplaymidi -p "Dussek MIDI:0"
  file.mid`.
- It finds the standalone's ports by its process, since PipeWire lets two clients share the name
  "Dussek". It ends the bridge when the standalone closes.
- Keyboards are not linked: connect them in a patchbay.

Without the script: `plugin/build/bin/dussek` (or the installed `dussek`). On Linux the standalone
links its outputs itself when nothing else does: half a second after its ports appear, still
unlinked, they go to the first playback device JACK lists (under PipeWire the default output, under
JACK `system:playback_1` and `_2`; a mono device takes both). A session manager or your own routing
that links them first wins; `DUSSEK_AUTOCONNECT=0` turns it off.

Without a JACK server the standalone plays on its own audio (DPF's fallback: CoreAudio on macOS,
WASAPI on Windows, PulseAudio or ALSA on Linux) to the default output. It opens every MIDI input
present at start; the MIDI tab then has an **Audio and MIDI** row: the buffer size (128 … 1024
frames) and **Rescan**, which opens the MIDI inputs plugged in since (it reopens them all, so a
held note stops). In a host and under JACK the row is not shown. On Linux the fallback does not
play yet (RtAudio rejects PulseAudio's default output); JACK, or PipeWire's JACK layer, is the
way there.

LV2 hosts need the worker feature, as Ardour, Carla and Jalv have. Without it, the velocity
curve's edits do not reach the sound.

## Instruments

The plugin lists every instrument directory it finds in these paths:

1. `$DUSSEK_INSTRUMENTS`;
2. the bundle's resources;
3. the standalone's own folder's `instruments` (an unpacked archive plays without installing; on
   macOS the folder holding `Dussek.app`), only when the running program is the standalone;
4. the system folder's `instruments`, where an installer puts them for every user: on Linux
   each entry of `$XDG_DATA_DIRS` + `/dussek`, then `/usr/local/share/dussek` and
   `/usr/share/dussek` where the list leaves them out, on macOS
   `/Library/Application Support/Dussek`, on Windows `%PROGRAMDATA%\Dussek`;
5. `~/.local/share/dussek/instruments` (`just install` links `instruments/` there;
   `$XDG_DATA_HOME` replaces `~/.local/share`);
6. `~/.local/share/dussek/snapshots`: instruments from a git ref (`just snapshot <ref>`), in
   the menu's "Snapshots" group.

A name found in an earlier path hides the same name in the later ones: an installed instrument
hides an old copy of it in the user folder (from a test build); the instruments you add keep
names of their own. Put them in the user folder, not the system folder: the Windows and macOS
installers empty the system folder's `instruments` on every update.
`dussek list` (the standalone) prints what it finds: a line per instrument (its name, menu
group and entry, folder), then the hidden copies; it exits with 1 when it finds none.

The user folder (`~/.local/share/dussek` above, with the presets beside the instruments)
is the platform's own: on Linux `$XDG_DATA_HOME/dussek`, else `~/.local/share/dussek`
(the MIDI maps in `$XDG_CONFIG_HOME/dussek`, else `~/.config/dussek`); on macOS
`~/Library/Application Support/Dussek`; on Windows `%APPDATA%\Dussek`
(`plugin/src/Paths.cpp`).

- **Menu entry**: from the file's `"menu"` block. Without one, the instrument is listed under
  "Other" by its directory's name.
- **Controls shown**: from the engine's report on the file (`engine/src/info.rs`): the controls
  without effect, the preparations that act, the stops, the mallets.
- **Adding one**: a new instrument of a kind the engine has (any body in `mic.rs`) needs no
  rebuild. Copy its directory into one of the paths; `$DUSSEK_INSTRUMENTS` keeps it out of
  the repository, which the user data directory links to.
- **Kept anywhere**: the instrument menu's last entry, "Open instrument folder…", opens a file
  browser: choosing a folder's `instrument.json` plays that folder, listed under "Opened" while
  it plays, whether or not it lies in a search path.
- **Saved by name**: a session saves the instrument by its directory's name (one opened from a
  folder by its path). One it cannot find shows as "Missing: <name>" and plays nothing.
- **Reload**: an instrument whose files change while the plugin runs plays anew about a second
  after they last changed. This covers a re-export and the editor's rolled preview. The old
  sound fades out over 30 ms, as at a change of instrument.
- **New directories** show in the menu once their `instrument.json` is there, without a restart.

## The window

The UI is NanoVG and scalable: the host's HiDPI factor × a zoom of 100–400 %.

- **The header**:
  - the instrument menu: a menu of groups (grand pianos, upright pianos, rolled pianos,
    harpsichords, clavichords, harps, guitars, percussion, thumb pianos, zithers, and any other an instrument
    names), each opening its instruments beside it, by name. A list longer than the window is cut
    to it and scrolls (the wheel, or its bar at the right edge), opened at the chosen instrument;
    every list does the same (Mallet, the MIDI learn menus, …);
  - the stops, the motor, the sustain latch and Volume. They go on two rows where one leaves
    Volume too little room, or the toggles less than their labels;
  - **Panic**;
  - the **⋯** menu.
- **The microphone drawing**, from above and from the front:
  - click a marker to choose a position;
  - drag the pair to set its distance and spacing; from the front, drag the height for the
    close positions.
  The drawing shows where the engine really puts the microphones, its limits included. The
  position, distance and spacing are under it.
- **Ten tabs**: Sound, Morph, Touch, Felt, Mics, Tuning, Condition, Extras, MIDI, About. Only the controls
  that act on the instrument, at its microphone position and with its preparation, are shown
  (`engine/tests/no_effect.rs`).
- **Layout**: wide windows put the drawing beside the panel and flow the panel into up to three
  columns. Narrow ones show one column, with the drawing in the Mics tab.
- **The bottom**: **Keyboard** opens the player strip (keys to play with the mouse, the pedals).
  With it open, one panel above it at a time:
  - **Analysis**: a spectrogram of the output with the engine's partials, or the partials as bars
    over their keys;
  - **Visualization**: the instrument as the engine plays it. Each key's strings from above, a
    sounding string a glowing band as wide as its partials move it (exaggerated; the strings ringing
    along drawn larger), a fresh snapshot of the string inside it at every redraw; the dampers on the
    strings, lifted ones raised; a piano's hammers from the front, at the string from the strike and
    falling back, caught by the backcheck while the key is held. A pluck flashes at its point; a
    bar instrument shows its bars and their damper bar instead. The engine sends it 30 times a
    second; a live note-on comes as the hammer arrives, so no hammer rises before it. The Copy
    CLI line plus `--video out.mp4` renders the same picture as a video ([engine.md](engine.md)).

  Both panels read what the sound side shares with the window. In VST3 that needs a host that
  passes the plugin's output values on to its editor (DPF runs the two as separate objects); where
  the host does not, the panels stay empty and the sound is unaffected.

Using the controls:

- The mouse wheel scrolls the panel and never changes a value.
- Drag a slider (Shift: finer); double-click it for the default; click the value to type it.
- Right-click a control to tie it to a MIDI CC (MIDI learn, below).
- Resting the pointer on a control shows what it does.

The header's buttons:

- **Panic** (top right) silences everything within 20 ms: every note, the strings ringing along
  and the room's tail. Every key and pedal is released; the sustain latch stays as set. Should the
  sound ever run away (a fault), the engine silences it by itself: the voice that blows up, or
  everything once the output passes +20 dBFS.
- **⋯** holds the zoom, Copy CLI and Reset.
  - **Copy CLI** puts a `dussek render` command line (the standalone's `render`, which is
    `midi2wav`: either takes it) for the current settings on the clipboard. It names the standalone
    by its full path: its own in the standalone, in a plugin the standalone next to the plugin's
    bundle (a release's folder; `plugin/build/bin` behind `just install`'s links), else the one on
    `PATH`, else plain `dussek`. Then the
    instrument's directory, the sample rate, `--velocity-curve`, `--param`, `--tuning` with
    every setting that differs from the engine's default, `--latch`, and `--felt` with the Felt tab's
    points (midi2wav builds the felt and a piano's Hammer hardness as the plugin does, the loudness
    kept). What does nothing (such
    as the lid at the recording's position) is left out. Replace `in.mid` and `out.wav`; the
    render then plays as the plugin does, since the plugin and the line share one mapping of the
    parameters to the engine's values.
  - **Reset** (click twice within 3 s; the menu stays open for the second click) sets every
    parameter but the instrument back to its default, and the velocity curve straight. The felt's
    points, the morph's partners and the Scala file stay (the Default preset clears them too).

The selected tab, the zoom and the bottom's panel are saved with the session. The zoom last picked
is also kept for the next window (`window.txt` in the config folder, beside the MIDI map): the
standalone keeps no session. `plugin/build/ui_tests` tests the UI's logic without a window. The
parameter table only grows from release to release ([compatibility.md](compatibility.md)).

## The header's controls

| Control | What |
|---|---|
| Instrument | the menu above |
| Stops 8′, 4′, 8′ II, Lute | The registers an instrument has (Harpsichord B all four; Harpsichords E and F the 8′ and the 8′ II). Live, from the next notes; 8′ II and Lute pluck the same strings and exclude each other |
| Buff | the harpsichords' buff stop: pads that make the notes shorter and darker |
| Motor | the vibraphone's motor, on by default |
| Sustain latch | the pedal held down while on, together with CC64 and the on-screen pedal: for MIDI files without pedal, e.g. on the vibraphone and the harps; hidden on the instruments without dampers (the marimba, the kalimba, …) |
| Volume | the output level, off … +24 dB |

## Sound

| Control | What |
|---|---|
| Preset | whole configurations: the instrument and every setting (the velocity curve, the felt, the morph, the tuning file among them); picking one switches to it. Save as… keeps the current one under a name (typed in place, Enter); your own preset shown gets a Delete (a second click within 3 s confirms). Shipped: **Default** (every control and state at its default, the instrument the menu's first; Reset keeps the instrument, the felt, the morph's partners and the Scala file), **Production, bright** (Grand A at Close 1, Hammer hardness +12 dB, Room −12 dB: present and dry, like a GM soundfont's bright piano) and **Production, warm** (Grand B with Dynamics 25 dB: full low mids and even levels); each sets every other control to its default. LV2 hosts list them too, in their own preset menus (DPF gives CLAP and VST3 hosts no preset list) |
| Mallet | the instrument's mallets (the marimba's yarn, cord, rubber; the xylophone's rosewood, hard rubber; …) |
| Hammer hardness | −24 … +24 dB, from the next note. Below 0 a softer felt, down to old, dull hammers; harder, compressed felt is the condition's hammer wear. On the hammer pianos every key keeps its loudness: the change is measured in the background, as the Felt tab's (below), and heard once that is done. On a harpsichord the quill's lowpass. Named for what strikes or plucks: "Quill brightness" (the harpsichords), "Tangent brightness" (the clavichord), "Finger brightness" (the harps, the guitars), "Pick brightness" (the zithers), "Mallet hardness" (the bars), "Thumb brightness" (the kalimba) |
| Sustain | 50 … 400 %: how long the notes ring, every partial's decay that much slower or faster (200 %: twice as long), its pitch and the attack kept; on top of an instrument's own ring per key (the uprights 150–200 %); the strings, the bars and the sympathetic ring alike, the dampers not. Rebuilds the instrument in the background, as the Tuning tab's settings. Not the sustain pedal |
| Bass | −12 … +24 dB: the strings below about 180 Hz, as an equalizer on them, louder with it; the hammer's thud through the board stays as it is: full at 45 Hz, 0.8 of it below and at 63 Hz, 0.5 at 90 Hz, 0.25 at 125 Hz, none from 180 Hz. +20 dB gives Grand C a little more than the bass it had before it was trimmed to its recording (design log steps 147, 148). The hammer pianos; a quick rebuild (the board's responses stay) |
| Motor speed, depth | the vibraphone's motor: 1–12 pulses per second (default 7), depth default 70 % |
| Bebung | the clavichord: a held key's pitch rises with the pressure on it, up to this depth in cents at full pressure (0–100, default 30). From the mod wheel (CC1) or aftertouch, channel or per key; the highest counts |
| Bend range | the đàn tranh: the pitch wheel bends the sounding notes, as the left hand pressing behind the bridge. Default 2 semitones |
| Room | the room's level, off … +12 dB (off: the dry instrument, for an external reverb) |
| Condition | in zones: Mint (0 %), Normal (to 10 %, the default), Worn (to 35 %), Broken (to 100 %); the Condition tab's aspects add to it |

## Morph

The Morph tab plays an instrument between the chosen one and up to four others, made from their
files without fitting (the engine's morph, [engine.md](engine.md#morphing)). It covers the
grands and uprights so far; on another instrument the tab says it does not morph.

- **The list** starts with the chosen instrument. "+ Add" offers the ones that may join, and each
  partner row moves up, down or goes (×). Every row shows its share.
- **The pad** draws the list as a regular polygon, the chosen instrument at the top; with two
  instruments it is a line. The point picks the mix:
  - a corner is that instrument alone;
  - an edge mixes its two ends;
  - inside, every instrument has a share (generalized barycentric coordinates). With four or
    more, the order decides which pairs an edge mixes.
- **Building**: drag or click the point. The instrument rebuilds in the background ("building…"
  under the pad, about 0.3 s), and each build plays as it lands, so a drag is heard as it goes.
- **Notes across a build**: notes held across a build fade out over 30 ms. Another mix is
  another instrument, whose strings cannot take over the old ones' swing (a change of instrument
  fades the same way). Notes played after the build sound in the new mix.
- **Joining and leaving**: a new partner joins at 0 %; the point starts on the chosen
  instrument's corner. Removing one puts the point in the middle.
- **Saving**: the point is two parameters, Morph X and Morph Y. They are automatable; right-click
  the pad to tie them to two knobs. The partners are the `morph` state. Both are saved with the
  session.
- **Display**: while partners play, the Instrument row reads e.g. "Grand A + 2 (morph)", and
  Copy CLI adds their `--morph` flags.
- **The body** (where the microphones go, the lid) is always the chosen instrument's.

## Touch

**Dynamics** (the pianos): how much softer the softest key is than the hardest, the level range
from velocity 1 to 127, 10–70 dB (45 dB, as Salamander's sample set plays its 16 recorded layers).
Every key follows one curve: from velocity 32 to 64 to 112 about +10 dB and +11 dB at 45 dB. The
loudest notes stay as recorded; a smaller range strikes the soft notes harder (louder, and brighter
with it), a larger one softer. The other instruments play their recordings' dynamics.

**Strike variation** (the pianos): how much each strike differs from the last, as a real hammer
never meets the strings quite the same way twice: its strike point (±2 mm), the moment it meets
each string of a unison (±40 µs) and each string's share of the felt (±5 %), drawn anew per strike.
The upper partials change most, the fundamental and the level hardly. 100 % is the spread of Grand
E's two recorded takes per key, 0 % every strike alike, the default: whether a piece wants it is a
choice (a sequencer's repeated notes sound mechanical without it). Up to 400 %. From the next note.

**Live re-strike** (the pianos and the mallet instruments, off by default): a note struck again
while it still rings. On, the hammer meets the strings where they are in their swing: towards it,
the felt is hit harder and shorter (a soft repeat into a loud note up to several times the force,
brighter, its knock louder); away, softer. A mallet meets the bar the same way: a bar is far
heavier than its strings' counterpart, so a repeat changes less: brighter or duller by the bar's
phase, and each hit takes some of the bar's swing out (a roll on one bar about 1.5 dB quieter). No player times that (a period of C4 lasts 3.8 ms), so each
repeat comes out a little different. Off, every strike is as into still strings or a still bar, its
sound added to the ring. Heard mostly in soft repeats into loud notes, trills, rolls and fast
repetitions; it costs nothing measurable. From the next note.

The velocity curve reshapes the incoming velocities before the instrument plays them, whether
they come from the keyboard or from a MIDI file whose touch is off. Across: the velocity in; up:
the velocity played. It acts before the Dynamics.

- **Editing**: the curve starts as a straight line. Click to add a point (at most 8), drag one to
  move it, double-click to remove it. The ends move up and down only: the left end raised gives
  a minimum loudness, the right end lowered a cap.
- **Shape**: the curve runs smoothly through the points and never falls. Steep over a range
  spreads the notes there apart (for a file that plays too evenly); flat squeezes them together.
- **Feedback**:
  - the last notes show as dots on the curve at their incoming velocity, fading over 4 s;
  - the bars under it count where the notes of the last 30 s came in, to find the range to bend;
  - a double-click on the bars clears them and the dots, to start over with the next file.
- **Saving**: "Reset curve" and the menu's Reset make it straight. The curve is saved with the
  session (the `velocity_curve` state) and kept when the instrument changes.

## Felt

The hammer pianos' felt as curves over the keyboard (design 2026-10-22-felt-controls), bent from
the felt the instrument ships with: to hear what the felt does, out to the extremes, and to adjust
a sound now and then (a metallic bass, a treble too glassy).

- **Four quantities**, picked by the chips above the graph:
  - **Stiffness**: the felt's force at 0.5 mm of compression, dB re as shipped;
  - **Exponent**: how much brighter it gets struck harder (its force at 0.5 mm kept);
  - **Core force**: the stiff core under the soft felt, N at 0.5 mm into it (raised from 0 N where
    the instrument has none);
  - **Core depth**: mm of felt before the core.
- **The graph**: the shipped felt dashed (the depth dotted where a key has no core), the felt played
  solid over it. The other three show small under it.
- **Editing**: click to add a point (at most 8 per quantity), drag one to move it, double-click to
  remove it. A point bends the felt at its key; between points the bend changes linearly with the
  key, beyond the outer ones it holds, so one point bends the whole keyboard. Each key keeps its
  own shipped detail under the bend. The graph's range grows to fit the points when one is let go.
- **Loudness**: every key keeps its loudness at every velocity; only the tone changes. After each
  change (a point let go, added or removed, Reset) the engine measures the instrument again in the
  background, about 2 s (longer the first time on an instrument; on the physical cores but one, at a
  lower priority than the host's other work), "measuring…" under the graph; the change is heard when that is
  done, the felt and its leveling together.
- **Original** plays the shipped felt, the points kept (switching back is quick: the last four
  felts are kept); it is off when a session opens. **Reset** removes the points.
- **Saving**: the points are saved with the session (the `felt` state) and belong to the
  instrument: switching to another one starts from its shipped felt. With a morph, the bends apply
  over the felt at the pad's point. Dragging the pad plays each point at once, its loudness as last
  measured for the felt; the point where the pad stops is measured afterwards, as above.
- The condition's hammer wear and voicing apply on top. Instruments without felt hammers (every
  one but the grand and upright pianos) show a note instead.

## Mics

**Mic presets** (in the Mic position list, after the positions): named microphone setups for this
kind of instrument (a grand's for every grand, an upright's for every upright: a Custom position is in
the instrument body's own coordinates). Picking one sets the microphones only: the position, its distance and spacing, the
Custom pair, Distance level; the lid, the pillow, spread keys and the room stay. Shipped for the
grand: ORTF at the player's seat (two cardioids 17 cm apart, 110° between them, at head height), Jazz:
one mic near the tail (both microphones at one point, a cardioid aimed at the board: mono), Spaced
pair over the strings (two omnis 60 cm apart, 30 cm over the strings), Concert pair at 4 m (two omnis
on the curved side); for the upright: Pair over the open top, Pair behind the board (the piano 1.2 m
from the wall), ORTF at the player's seat. Save as… (at the list's end) keeps the current microphones
under a name for every instrument of this kind, typed in the box (Enter); your own preset shown gets a
Delete (a second click within 3 s confirms). The box names the preset the microphones still match
(your own before a shipped one), else the position (Custom once a microphone moves).

Your presets, both kinds, are files in `~/.local/share/dussek/presets/` (`$XDG_DATA_HOME`
replaces `~/.local/share`), one `.dussek-preset` file per preset: every host and the standalone see
the same list. A host's own preset menu (Ardour's) keeps working beside them.

**Positions** (under the drawing; the plugin starts at Player, `midi2wav` and the fits at
Recording):

| Position | Grand | Upright | Harp |
|---|---|---|---|
| Recording | the fitted sound | | |
| Player (the default) | at the keyboard, 1.25 m | 1.0 m from the board | at the harpist's ears behind the soundbox, 0.55 m |
| Audience | in front of the open lid, 3.5 m | 3.5 m from the board | 3.5 m in front |
| Close 1 | hammers: a pair under the open lid above the hammer line, 0.3 m above the board | open top: 0.3 m above it | 0.4 m in front of the soundboard |
| Close 2 | tail: over the rear of the board, 0.45 m | back: 0.4 m behind the board, the piano pulled 1.2 m from the wall | 0.5 m behind the sound holes |
| Custom | a free pair (below) | | |

- The paths: the Audience and Player positions get the distance, the floor and lid reflections,
  air absorption, and notes radiating from their bridge points. The lid reflects from its size up:
  below ~120 Hz (overhead) to ~1.2 kHz (grazing) its reflection falls 6 dB per octave.
- The two microphones get the same level, whatever the recording's balance between its own pair
  was, and hear the board's bass alike (as Plath's dummy head at a concert grand's keyboard does:
  within ~1–2 dB below 250 Hz).
- The upright otherwise stands against a wall. Its sound leaves through the back, the open top
  and the front panel.
- The harp's close pairs spread along the board, and it is drawn from above and from the side.
- The other bodies (bars, guitars, zithers) have the same positions in their own geometry.

| Control | What |
|---|---|
| Spacing | the pair's spacing, 0…1000 cm (close positions: up to 110 cm) |
| Distance | 0 = the position's default (above); up to 20 m. At the close positions, the height above the soundboard |
| Distance level | the level kept over the distance either for the total (direct + room) or for the direct sound (for an external reverb) |
| Lid | 0–100 %: below |
| Lid removed | the lid taken off: below |
| Pillow | 0–100 %: below |
| Spread all keys | below |

A change of position, distance or spacing rebuilds the engine (~0.1 s, about 12 ms for a
position used recently). The new position sounds at once as if it had always been there, its
reverb included, and the sound crossfades to it over 30 ms; a drag plays as it goes.

### Microphones

**Custom** gives both microphones a free x, y, z, in meters from the board's center. The tuning
keys are `mic=5`, `mic_x1 mic_y1 mic_z1` and `…2`, and `mic_wall`.

**Dragging**:

- A plain drag of a preset's microphone changes its distance and spacing, as before.
- An Alt-drag of one microphone switches to Custom, seeded from the preset's geometry so nothing
  jumps, and moves that microphone freely: top view x/y, front view x/z.
- While the position is Custom, every drag is free.

Numeric sliders for the coordinates (and the upright's "Wall distance") appear under Mics, and
Distance and Spacing are hidden.

**Clamping**: every move goes through the engine's clamp (`Body::clamp_mic`), which keeps the
microphones out of the board, the lid and the floor:

- grand: over the case between the board and the lid, never above the lid (with the lid
  removed, anywhere above the board);
- upright: not inside the case (above the open top is fine), and in front of the wall;
- harp: not inside the soundbox; the drawing's second view is from the side, y/z.

**Patterns and aim**: each microphone has a pattern, a yaw and a tilt.

- The pattern: 0 omni, 50 cardioid, 75 hypercardioid, 100 figure-8 (key `mic_pattern1`/`2`,
  0…1).
- The yaw: 0 along +y, from the player's seat into the piano, positive towards +x
  (`mic_yaw1`/`2`).
- The tilt: positive up (`mic_pitch1`/`2`).
- Arrows and a small pattern glyph appear on the drawing when a pattern is above 0. Dragging an
  arrow's tip aims it.

How the patterns behave:

- The patterns are not level-normalized (the on-axis sensitivity is 1), so a microphone aimed
  away is quieter.
- The diffuse room is scaled by the pattern's diffuse sensitivity. Its inter-channel coherence
  follows the patterns: exact for a coincident pair, approximate for a spaced one. The room is
  isotropic, so a cardioid aimed away from the piano is only a few dB quieter in a typical setup;
  the direct sound is suppressed, the room is not.
- The room's coherence follows the effective distance between the two microphones (a grand's
  close pair held under the lid sits a little closer together than its spacing says).
- There are no frequency-dependent patterns and no proximity effect.

**Time**: no latency is added. Time zero is the earliest arrival over all zones, microphones and
paths, so the relative delays between paths and between microphones are real, and aiming never
moves time zero.

### Lid and pillow

**Lid** (0–100 %; tuning key `lid`, 0 … 1):

- Grand: 100 % is the full stick (40°), about 50 % a half stick, 0 % closed. Upright: 100 % is
  the top lid open (95°), 0 % the top lid down.
- The lid-reflection geometry follows the angle. The grand's close pairs sit at the minimum
  height above the board (8 cm) when the lid is closed.
- Closing the lid muffles the sound outside the case. What leaves through the lid is up to 9 dB
  quieter and lowpassed; the cutoff falls to about 1.2 kHz when shut. The diffuse room is reduced
  and darkened the same way for the shielded microphones (half of the room's excitation is taken
  to come through the lid).
- Not affected: the microphones under the grand's lid (the close pairs), and the underside sound
  that reaches you via the floor.
- The slider is hidden at the Recording position, which stands for the open lid. The
  harpsichords and the clavichord use it too.
- A change rebuilds the radiation (a crossfade), like a microphone move.

**Lid removed** (off by default; `lid=2` in the tuning key) takes the lid off, as for a studio's
microphones or an even sound in a small room:

- No lid reflection and no closed lid's loss; the Lid slider does nothing while it is on.
- The positions stay those of the open lid; a free microphone may go anywhere above a grand.
- With the level kept (Distance level: total), the change is small at Player (within ~1.7 dB per
  octave). Audience loses the treble the lid sends its way: −6 dB at 8 kHz without the room,
  −3 dB with it. An upright's open top lid leans back, away from Player: no change there.
- Hidden where the Lid is (the Recording position, instruments without a lid).

**Pillow** (0–100 %; key `pillow`, 0 = off) is a pillow or blanket against the soundboard.

- The body response gets duller, quieter and shorter (a lowpass, a level, extra decay). The room
  gets a lowpass and a level change, with no extra decay.
- The effect is steeper in the lower half of the slider: q = 1 − (1 − p)². A full pillow gives a
  600 Hz cutoff, −12 dB and a 25 ms decay.
- It works at every position, Recording included.

Example: `midi2wav --tuning lid=0.5,pillow=0.3` (lid 1 = open, the default). All amounts are
starting values tuned by ear, since no recording exists to base them on: the constants are in
`engine/src/mic.rs` (`LID_LEAK_DB`, `LID_LEAK_HZ`, `LID_ROOM_SHARE`) and `engine/src/pillow.rs`.

### Spread all keys

This applies to the bars, the harps, the guitars and the zithers, at a microphone position
(tuning key `spread`, 0/1).

- **On** (the default): all 88 keys spread along it.
- **Off**: the instrument's own range lies along the instrument in front of the microphones, as on
  the real one (the bars on their frame, a harp's or a zither's bridge, a guitar's strings). The
  keys beyond the range sound from its ends; for example, the crotales' lower five octaves come
  from the left. (`midi2wav` keeps off as its default.)

The pianos, the harpsichords and the clavichord hold the keyboard's 88 keys along their bridge
already. A change hands over to the new key positions with a crossfade.

## What each instrument does its own way

The instruments and their recordings are listed in the [README](../README.md#the-instruments).
What they do beyond strings struck or plucked:

| Instrument | Its own behavior |
|---|---|
| Grand D | every key recorded; its layers' recording gains undone |
| Grand E | normalized samples: the dynamic range comes from the model |
| Grand G | a worn grand recorded softly with the soft pedal down: a quiet piano, narrow dynamics |
| Grand H | a baby grand recorded close and in mono, every file normalized: its levels from Grand B's recordings, its timbre per layer its own |
| Grand I | recorded from a reproducing piano at named MIDI velocities: its pp, mf and ff are the takes nearest the velocities the engine plays them at (14, 60, 124); recorded in mono |
| Harpsichord B | 8′, 4′ and 8′ II registers and a lute row (the header's stops); the 8′ II and the lute derived from the 8′. The lute's quill pulls as hard as the 8′ II's, near the nut: nasal and quieter |
| Harpsichord E | a French double of 1755: the lower manual's 8′ and the upper manual's (the 8′ II), each recorded; F1–F6 |
| Harpsichord F | an English single of 1709: its two 8′ choirs, each recorded (the 8′ and the 8′ II); G1–D6, the bass a short octave in the original, chromatic here |
| Harpsichords | the buff stop; the hardness is the quill's lowpass |
| Clavichord | C2–D6 recorded, double strung, struck by tangents that stay on the strings while the keys are held. A key's release lets the listing cloth damp its strings. The Bebung (Sound tab); the hardness is the tangent's brightness |
| Harp A | plucked near the middle of the string, no dampers: a key's release is the player's hand on the string, and the pedal down lets every string ring |
| Harp B | a 34-string Celtic lever harp, chromatic C2–A6; its body is the concert harp's at 3/4 |
| Guitar | each key in first position, on the string where it lies lowest on the neck. No dampers: a key's release is the player's hand, as on the harps. The top's main mode near 224 Hz is measured, so G3–B♭3 die quickly, as they do on the guitar |
| Strumstick | a three-string fretted dulcimer held on the lap like a smaller guitar; no dampers but the hand |
| Psaltery | steel strings a whole tone apart, a triangular box on a table |
| Dan tranh | 16 steel strings over movable bridges, pentatonic, normalized. Plays at equal temperament (the recorded bridges lie −43 … +49 cents off; "As recorded" plays them so). The pitch wheel bends the sounding notes (Bend range). A long box on a stand |
| Vibraphone | three tuned modes per bar, the mallet brighter the harder it strikes, the fundamental's harmonics. A held key lifts the damper bar off its bar, as played from a keyboard (the real bar follows the pedal alone); the pedal lifts it off every bar. The motor: Motor in the header, on by default, its speed and depth on the Sound tab |
| Marimba | four modes per bar in the bass, each strike rising from zero over 0.5 ms, no dampers; yarn, cord or rubber mallets |
| Xylophone | F4–C8, the second mode tuned to a twelfth; rosewood or hard rubber |
| Bells | an orchestra glockenspiel, C5–E8 recorded, eight modes per bar, long ringing, no dampers; plastic or brass |
| Crotales | bronze discs C6–C8, eight modes, one mallet |
| Glockenspiel A | G5–B7, every second or third bar recorded, one dynamic (an assumed mallet), six modes per bar derived from the recordings; made with the editor's wizard |
| Glockenspiel B | G5–C8, seven bars recorded at three dynamics only 10 dB apart, seven modes per bar; made with the editor's wizard |
| Tubular bells | 18 tubes C4–F5, the white keys recorded at two dynamics. Nine modes per tube, the strike note at half the fourth. The modes beat slowly (split pairs 0.2–1.5 Hz apart). A damper bar that a held key or the pedal lifts, its loss assumed |
| Hand chimes | 19 tubes C4–C7 a whole tone apart, one dynamic (an assumed clapper). The fundamental, its harmonics and two fast modes; the strike's short-lived modes; the low tubes' fundamental swells in over 20–50 ms after the clapper's click. A damper on the ringer's shoulder that a held key or the pedal lifts |
| Tongue drum | a steel tongue drum, 15 tongues E3–E5 in C major recorded at three dynamics with its rubber mallet. Each tongue nearly a pure tone, with the shell's own ring at 758 Hz (and 684 Hz in the bass) on every strike; the other tongues ring along, as recorded ("Ringing tongues"). Each strike rises from zero over 1 ms; no dampers |
| Kalimba | 27 steel tines G2–C♯7, one dynamic (an assumed thumb). Each pluck rises from zero over 4 ms. Plays at equal temperament: the recorded tines lie up to 65 cents off, and "As recorded" plays them so. The other tines ring along, as on the recordings ("Ringing tines"); each tine rattles on the bridge, as recorded; the thumb's click is the knock ("Thumb click"); no dampers |
| Mbiras | Mbira A (a Zimbabwean mbira dzaVadzimu, 24 tines B2–C♯6), Mbira B (a Zimbabwean mbira, 24 tines A♯2–A5) and the nyunga nyunga (Mozambique, 19 tines F3–F5): the kalimba's tines and its behavior, with the buzzers on their boards at each tine's recorded level |
| Balafon | wooden bars over gourds, six recorded C♯4–F6 at three dynamics, four modes per bar and each bar's other clear lines. The gourds' buzz: the fundamental's harmonics up to the 7th and the membranes' noise following each strike, each gourd at its own level, more the harder the strike and the mallet ("Gourd buzz"). Each strike rises from zero over the mallet's contact; its noise on the wood is "Mallet click". Traditional, hard or soft mallets; no dampers |

The microphone positions apply to every instrument.

## Tuning

These settings are compiled into the strings. Changing one rebuilds the engine in the background
(~20 ms), and the new engine takes over the sounding notes: they glide to the new tuning without
a break.

| Control | What |
|---|---|
| Concert pitch | 415–466 Hz; A keeps it under every temperament |
| Temperament | equal (the default); meantone 1/4 and 1/6 comma, Werckmeister III, Kirnberger III, Vallotti, Young II, Lehman's Bach; "As recorded"; "Scala file" (below) |
| Temperament key | the key a temperament or a Scala scale is centered on (a meantone's wolf moves with it) |
| Stretch | −1 … +10: a tuner's stretch computed from the strings' inharmonicity, on top of the recording's own. 0 as recorded, −1 pure octaves; bass flatter, treble sharper. The pianos only: the other instruments are tuned without a stretch |
| Unison width | 0–400 %: how far a note's strings are detuned against each other, 100 % as recorded, 0 all alike. Where notes have more than one string (the pianos, the clavichord) |

"As recorded" plays each recorded key at its own measured pitch, around the instrument's
smoothed tuning. The kalimba's tines and the đàn tranh's bridges play as recorded; keys between
recorded ones follow the curve.

"Scala file" opens a file browser for a Scala scale (`.scl`, any number of notes; the row then
shows the file's name, "unreadable" when it cannot play). Without a key map each key plays the
next degree, degree 0 on the temperament key in middle C's octave and A4 at the concert pitch: a
twelve-note octave scale is a temperament, a 19-note one spreads over fewer octaves. A key map of
the same name beside it (`name.kbm`) lays the scale on the keys its own way (its middle key, its
reference key and frequency, keys it leaves out stay equal). A key goes at most two octaves from
its own pitch: the strings are the recorded keys'. The file is kept as a path (the "scale_file"
state); Copy CLI writes it as `--scl`.

## Condition

**Condition** (on the Sound tab) runs through four zones, its value shown with the zone's name
("10 % · Normal"; typing a zone's name sets its upper edge):

| Zone | Slider | Voicing | Every other aspect |
|---|---|---|---|
| Mint | 0 % | none: the instrument as exported (the grands voiced even, every key on one level curve) | none |
| Normal | to 10 %, the default | to 0.05: the keys a little uneven (their levels about ±1 dB) | none |
| Worn | to 35 % | to 0.2 | to 0.2 |
| Broken | to 100 % | to 1 | to 1 |

The wear rises linearly within a zone, in whole percents. Normal costs nothing beyond Mint: the
voicing is a gain and a hardness per strike; the false beats add modes, and they begin in Worn. Each aspect here adds an offset of −100…+100 % (of the
wear, 0 … 1) on top of it:

- live: hammers, voicing evenness (each key's hardness and level: σ 4.5 dB × √wear, 1 dB at
  Normal), dampers, action noise;
- compiled (a rebuild): unisons, tuning, strings, soundboard, false beats.

On the bars, discs, tubes and tines only what they have wears: no soundboard, strings or false
beats (those aspects are hidden there). Metal ones (vibraphone, glockenspiels, bells, crotales,
tubular bells, hand chimes) keep their pitch (Tuning hidden); wooden bars (marimba, xylophone,
balafon) drift a quarter as far as strings (σ 10 cents at full wear); a kalimba's tines drift as
strings do. On plucked strings (harpsichords, harps, guitars, zithers) the soundboard's wear adds a third of
what it adds on struck ones: their own loss is most of their damping.
The live wear goes by kind too: a piano's hammers, the clavichord's tangents and a harpsichord's
quills are voiced key by key; the bars are cut even, so their keys differ in level alone, by half
a piano's (±0.5 dB at Normal), and their mallets harden as hammers do; the harps, guitars, zithers
and kalimba, played by hand, differ in level by half a piano's and have no hammers to wear (hidden
there). Elsewhere too an aspect with nothing to wear is hidden: Unisons where every note has one
string, Dampers without dampers, Action noise without an action's knock.

**Variant** picks the random draws: one worn piano per variant.

## Extras

**Noises**:

| Control | What |
|---|---|
| Action knock, Release noise, Pedal noise, Pedal ring | dB, −40 = off. On the pianos the Action knock is the hammer's thump through the soundboard, part of the note, from the next note. The Pedal noise is the pedal's thump going down and up, the Pedal ring the strings the lifted dampers set ringing (mostly the bass). The noise is −20 dB by default (as recorded, 0 dB, the thumps distract in quiet pieces), the release noise and the ring −10 dB (chosen by ear) |
| Sympathetic resonance | % of the free strings ringing along. On the kalimba, "Ringing tines"; on the balafon, "Gourd buzz" |
| Audio input | dB, −40 = off (the default), up to +12 dB (below) |
| Damper fall | % of the normal ~25 ms a damper takes to land when a key is released (below); 0 = instant |

The audio input: the plugin's two audio inputs, summed, drive the free strings through the
sympathetic resonators, as a loud sound in the room does on a grand.

- With the pedal down every string rings along with it; with it up, only the strings without
  dampers.
- It is filtered by the soundboard's response by reciprocity, so the bass strings take up more
  than the treble.
- The input itself is not played back.
- At 0 dB it drives the strings about as hard as their own sound at that level does.
- Beware of acoustic feedback with a microphone near the speakers.
- It is hidden on the instruments without sympathetic strings: the bars, the kalimba, the
  balafon.

Damper fall: the release velocity of the keyboard's note-off speeds the damper up or slows it
down. It also sets the loudness of the release thump, which is smaller for a key held only a
moment: 40 % at once, full from 120 ms. A quick re-strike catches the damper in mid-fall.

**Defects** are independent of the condition: hammer grooves, damper buzz, rattle, phantom
partials. Each runs from 0 (off) to 100 % (the most it should be); the hammer grooves show on the
pianos only, the damper buzz where there are dampers. The phantom partials (the extra partials of
old, stiff strings in loud bass notes) add to the piano's own (a piano's export may carry some, as
Grand A does); they play in full up to C2 and fade out by C3. At 100 % a
fortissimo bass note gets about 3–5 dB louder.

**Preparation**: one preparation for all keys. The menu lists those that act on the instrument.

| Preparation | Controls |
|---|---|
| Plucked: a finger inside the piano | the position along the string, the brightness |
| Muted: a hand or felt | how hard; not on the highest, damperless notes |
| Harmonic: a finger at the node of the 2nd to 8th partial while the hammer strikes | the partial |
| Bolt on the strings | its mass and position: a point mass that lowers each partial by where it sits. Compiled like the tuning, so a change rebuilds the engine |
| Paper on the strings | how much it buzzes |

## MIDI

| Message | Effect |
|---|---|
| Notes | any channel |
| CC64 sustain | continuous: half-pedaling (on/off pedals work too) |
| CC66 sostenuto | on/off at 64: holds up the dampers that are lifted when it goes down |
| CC67 soft pedal | continuous, for the notes struck from then on. On a grand una corda: the hammers miss one string of each unison and strike with softer felt. On an upright the hammer rail: slower hammers. The pianos only |
| CC1 mod wheel, channel pressure, polyphonic key pressure | the clavichord's Bebung; the others ignore them |
| Pitch wheel | the đàn tranh's bend: ± its Bend range on the sounding notes |
| CC120 all sound off | silence within 20 ms, as Panic |
| CC123 all notes off | every key and pedal released; the dampers fall |
| Any other CC | only through a MIDI learn tie |
| Program change | only with its switch on (below) |

The player strip has three pedal levers: soft, sostenuto and sustain, from left to right. They
spring back when released and follow the MIDI pedals; the soft one is dead on every instrument
but the pianos.
Keys beyond the instrument's recorded range are tinted amber on the strip (extrapolated by the
model). The harpsichords' strip has dark naturals and light sharps, as many harpsichords do.

The instrument loads in a worker thread. The instruments play at one loudness: a phrase in each
one's middle register (an arpeggio at velocity 80, chords at 95) measures as loud as on the grands
(about −29 LUFS), so switching needs no Volume change; at soft or hard velocities they differ as
their dynamics do (a harpsichord plucks the same at any velocity, a piano spans 45 dB). Only
switching the instrument (or the sample rate) cuts notes.

### MIDI learn

MIDI learn ties a controller's knobs, faders and pads to the controls, every one of them. That
includes the ones that rebuild the instrument, which a host's own MIDI learn cannot reach.

- **Learning**: right-click a control and choose "Learn MIDI" ("Learn MIDI (relative knob)" for an
  endless knob in relative mode, below; on a switch "Learn MIDI (pad flips it)" instead). It shows
  "learning…" until the first CC arrives, which ties it. Esc, a right-click or a click elsewhere
  gives up.
- **Tied controls**: a tied control carries a small tag ("CC 21"). Its right-click menu then
  offers "Learn again" (either way) and "Forget CC 21".
- **How a CC drives a control**:
  - a slider follows the CC over its travel;
  - a switch is off at 0 and on at any other value, so a pad hit softly switches it too. A
    momentary pad learned with "Learn MIDI (pad flips it)" (the tag "CC 30 flip") flips the switch
    at each press instead, its release passed over, so the pads can stay momentary for Next and
    Previous;
  - a list is split into equal bands, one per entry in the menu's order (Mallet and Preparation:
    the instrument's own).
- **Relative knobs**: an endless knob in relative mode sends moves, not places: 1–63 up by that
  many steps, 127 down by one, 117 by eleven (faster turns send more; 0 counts as one up). Tied
  with "Learn MIDI (relative knob)" (its tag "CC 21 ±"), it moves the control from wherever it is,
  so it never jumps after a change with the mouse. A slider moves 1/127 of its travel per step, a
  list and an integer slider one entry per 3 steps (a turn back answers at once), a switch (tied
  so only in the map's file) goes on turning up and off turning down. In the map's file such a tie
  ends in `relative`.
- **Soft takeover** (the MIDI tab's switch, on by default): after a slider moved otherwise (the
  mouse, the host, a preset), a knob or fader tied to it takes over only once it comes within 2 %
  of the slider's value or passes it, so the slider never jumps. Lists, switches and relative knobs
  take every value.
- **One to one**: one CC drives one control, and a control has one CC; learning a CC that is
  tied already moves it. A tie keeps the channel it was learned on.
- **Reserved CCs**: the pedals (CC64, 66, 67) and CC120 and 123 keep their meaning and never
  tie. The mod wheel (CC1) can be tied, and then no longer plays the Bebung.
- **As if moved by hand**: a value from MIDI acts as if the control was moved (the 8′ II still
  switches the lute off). Two exceptions: picking a microphone position does not reset the
  distance, and a Custom pair's coordinate does not switch to Custom.
- **Rebuilds**: a knob sweeping a control that rebuilds the instrument (the lid at a microphone
  position, the tuning) rebuilds it for where the knob stops. A sweep over 1 s gave three
  builds.

The MIDI tab lists:

- the ties (× removes one);
- "Next instrument" and "Previous instrument" (learn them there): each press steps through the
  instrument menu once, starting over past the end. A press is any value but 0, so a pad steps
  however softly it is hit and its release (0) does nothing; a button that sends the same value at
  every push steps at every push. A controller's + and − buttons that share one CC (+ sends 127,
  − sends 0) carry both: learn Next with one button and Previous with the other, and the tie
  shows "CC 40" and "CC 40 at 0";
- "Instrument (relative knob)": an endless knob in relative mode steps through the instrument menu
  either way, one instrument per 3 clicks; it leaves Next and Previous to the buttons;
- "Program change selects the instrument": program 1 is the menu's first, on any channel. It is
  off by default, since many MIDI files start with a program change;
- "Soft takeover", above.

The ties live in one file for every instance and session:
`$XDG_CONFIG_HOME/dussek/midi-map.txt` (`~/.config/dussek/midi-map.txt`). They describe
the controller, not a sound, so Reset, presets and saved sessions leave them alone. They also
work while no instrument plays, so a knob can step away from one that does not load. Hosts that
take a plugin's own parameter changes (the JACK standalone, Carla) save the values MIDI set;
others hear of them only through the open UI.

## About

The last tab says what plays:

- **The plugin**: its name and version, the commit it was built from (`git describe --always
  --dirty` at build time, "unknown" when built from a source tarball) and its license
  (AGPL-3.0-or-later, `LICENSE`).
- **The project's link**, with **Copy link** (the plugin opens no browser: DPF has no portable way
  to, so the link goes to the clipboard).
- **The instrument**: its name, its folder and its `LICENSE.md` (every export carries one; an
  instrument opened from a folder elsewhere has it in that folder). The Markdown shows as plain
  text, wrapped to the tab's width and scrolling where it is longer; the file is read when the
  instrument changes. A folder without the file says so.

## For tests

`plugin/tools/lv2render.c` is an offline LV2 host for tests. It reads MIDI from a text file and
uses random block sizes, with timed parameter changes (`@t symbol=value`, `LV2RENDER_PACE`). The
plugin matches the engine's own render bit for bit, up to float32 FFT rounding when the pre-roll
shifts the block grid.
