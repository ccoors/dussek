# Changelog

What each release changes for a player. From 1.0 on a break needs a major version; before it, a
break is allowed but listed under "Breaking" (`docs/compatibility.md`).

## Unreleased

### Fixed

- macOS: in GarageBand, the lower half of the on-screen keyboard from D4 to A5 did not respond
  (the host's resize handle covers the bottom center of a plugin's window). The AudioUnit's window
  is 24 points taller and keeps the keyboard above it. Contributed by
  [@Neon-White](https://github.com/Neon-White).

## 0.5.0 — 2026-11-28

### New

- Without a JACK server, the standalone's MIDI tab picks the audio engine (WASAPI, ASIO or
  DirectSound on Windows) and the output device, with the ASIO driver's control panel; the choice
  and the buffer size are kept for the next start. ASIO works with no extra download. The MIDI
  inputs row names the connected inputs. Contributed by [@deliosiret](https://github.com/deliosiret).
- macOS: the build runs on Intel Macs too (one universal build), and both plugins come as an
  AudioUnit as well, for Logic Pro and GarageBand (in the package, each a choice of its own).
  The macOS build is tested by hand: the standalone, LV2, CLAP, VST3 and the AudioUnit.
- Mic presets for every other body (the harps, the bars, the guitars, the zithers, the kalimba,
  the tongue drum): ORTF at the player's seat and ORTF in front at 1.5 m. Only the grand and the
  upright had them; none for a bell yet.
- Weidinger's microphone views draw the mute that plays in the bell (straight, cup, wah, bucket,
  plunger).
- The editor's wizard asks for the recordings' credit (name, author, terms, license) on its last
  page, so a new instrument's LICENSE.md credits them.

### Fixed

- macOS: the standalone could crash while opening its window (a resize arrived during its first
  MIDI scan). Contributed by [@Neon-White](https://github.com/Neon-White).
- The instrument editor builds on macOS. Contributed by [@Neon-White](https://github.com/Neon-White).
- Every download carries THIRD-PARTY-NOTICES.txt: the licenses of the libraries built into the
  plugins and standalones (DPF and what it builds in, RtAudio, RtMidi, the engine's Rust crates),
  which earlier releases left out. The About tab and the website's Credits page point to it.
- macOS: with Dussek and Weidinger loaded in one host, their windows ran one plugin's window code
  for both (DPF's window classes, of the same name in each); each plugin now has its own.

### Changed

- The standalone starts where it was left: the instrument, every parameter, the velocity curve,
  morph, felt and Scala scale, the selected tab, and the window's size and place (on first start
  the window opens centered). Contributed by [@deliosiret](https://github.com/deliosiret).

## 0.4.1 — 2026-11-17

### New

- A clarinet in Weidinger, in a new Woodwinds group (experimental): a B-flat clarinet from D3 to
  B♭5, its reed and its bore simulated as you play. Overlapping keys slur, the others are tongued;
  key-up lets the breath fade. Growl and flutter do not apply to a reed, so their rows and the
  aftertouch's choice of them are hidden; the Vibrato stays.
- Weidinger has its own icon: a trumpet on Dussek's plates, on its window (Linux, Windows), its
  Start menu entry, its desktop entry and its macOS app (it had Dussek's).

### Fixed

- A plugin in a sandboxed host (a DAW installed as a Flatpak) finds the instruments
  `install.sh` put in `~/.local/share/dussek`: it also looks where its own place implies and, in a
  Flatpak or Snap, in the real home's folder.
- Finding no instruments, the instrument row reads "No instruments found" and its tooltip lists
  the folders looked in (it read "Missing").

## 0.4.0 — 2026-11-10

### New

- Weidinger, a second plugin for the brass (trumpet, horn, tenor and bass trombone, tuba), as a
  preview in the same release files: its own standalone and LV2, CLAP and VST3 plugins. The Windows
  installer and the macOS package offer it as a component that can be left out.
- Analysis and Visualization open with or without the keyboard strip.
- An instrument folder that is no instrument, or a file that is no Scala scale, says why in the window.
- The standalone's window has Dussek's icon on Linux and Windows (it had the system's default).

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
