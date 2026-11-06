# Changelog

What each release changes for a player. From 1.0 on a break needs a major version; before it, a
break is allowed but listed under "Breaking" (`docs/compatibility.md`).

## Unreleased

### Changed

- The window shows only the tabs and pedals that act on the instrument, and the keyboard and its panels
  take the width. `midi2wav --video` draws the same pedals.

## 0.3.2 — 2026-11-02

### New

- "updating…" in the bottom bar while a change is still on its way into the sound (Hammer hardness,
  the felt, another instrument: up to about 2 s on a piano).
- The Morph tab morphs the harpsichords among themselves and the two harps with each other: the
  chosen harpsichord keeps its registers and stops, each mixed with the partners' of the same pitch.
- Dussek is on itch.io too (https://ccoors.itch.io/dussek), where the itch app keeps it up to date.

### Breaking

- Hammer hardness reaches +24 dB (was +12 dB). Its automation recorded in a VST3 or CLAP host
  before maps to other values (those hosts store it as a share of the range); LV2 sessions and
  presets keep theirs.

### Changed

- A morph plays each key at the mean of its partners' levels: halfway between Grand A and Grand B
  had a bass up to 2.6 dB louder than either.
- Release noise starts at −10 dB, as the Pedal ring does (chosen by ear); saved sessions and
  presets keep their own value.

## 0.3.1 — 2026-11-01

### New

- A tongue drum (Percussion): a steel tongue drum's 15 tongues E3–E5, from new recordings at three
  dynamics; the other tongues ring along ("Ringing tongues").

### Changed

- Upright D: its own key release noise, from new recordings of its keys released without a
  note (it borrowed Grand A's): a low thud, at about the old noise's level. Its A5 at ff from a new,
  unclipped recording.
- Harpsichord B: the lute stop plays a little under the 8′ II instead of 7 dB over it (its quill
  pulls as hard as the row's: near the nut the string gives way less).

### Fixed

- Linux: the standalone links its outputs to the default output when nothing else does, so it
  plays when started on its own under PipeWire (it was silent there); `DUSSEK_AUTOCONNECT=0` turns
  this off.
- The microphone drawings keep clear of their captions ("from above" ran into the left microphone
  with a bottom panel open; the floor line crossed "from the front").
- Grand H: no hiss in its room (its room rang 1.4 s from 1.6 kHz up: the strings in its recording's
  gaps read as a room).
- Linux: instruments installed with `install.sh --system` are found when `$XDG_DATA_DIRS`
  leaves out `/usr/local/share` (NixOS, some desktops).
- Linux: a failed `install.sh` lists what it copied, so `uninstall.sh` or the next install removes
  it; a relative `$XDG_DATA_HOME` is ignored, as the XDG spec says.
- `dussek render` and midi2wav refuse an `--input` that is no WAV file at once, not after the render.
- Windows: the installer keeps Dussek in its fixed folder (a plugin's Copy CLI looks for the
  standalone there) and removes the Start menu entry when an update leaves the standalone out.

## 0.3.0 — 2026-11-01

The first release.

### New

- Nine grands, four uprights, six harpsichords, a clavichord, harps, guitars, zithers and
  percussion, each physically modeled from freely licensed recordings, shipped as numbers.
- A standalone and LV2, CLAP and VST3 plugins for Linux, Windows and macOS (Apple silicon).
- Installers: a setup program on Windows, a package on macOS, `install.sh` on Linux; the
  instruments installed once for every plugin format.
