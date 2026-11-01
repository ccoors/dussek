# Dussek

A physically modeled piano, as an LV2, CLAP and VST3 plugin and a standalone. It is tested on Linux
and Windows, in all four forms (where a host offers LV2 or CLAP, prefer them to VST3:
[docs/plugin.md](docs/plugin.md)); the macOS build (Apple silicon) comes from CI and is untested:
reports welcome. It plays nine grands, four uprights, six harpsichords, a clavichord, harps,
guitars, zithers and percussion. None of them plays back a recording: the engine simulates the
hammer (or plectrum, or mallet), stiff strings with coupled unisons, the soundboard, the dampers and
the pedals, in real time. Each instrument's parameters were measured from freely licensed recordings
of a real one, so the instruments ship as numbers, without any audio. The aim is an instrument that
plays well (a continuous response to velocity, fast repetition, half pedaling, sympathetic
resonance, low latency) and has a character of its own. It is named after Jan Ladislav Dussek
(1760–1812), the pianist who had the piano's keyboard extended to five and a half octaves.

<!-- a screenshot: docs/img/plugin-grand.png, the plugin on a grand, taken by a script -->

## The instruments

| Menu group | Instruments |
|---|---|
| Grand pianos | Grand A, B, C, D, E, F, G, H, I |
| Upright pianos | Upright A, B, C, D |
| Harpsichords | Harpsichord A, B (8′, 4′, a second 8′ and the lute stop), C, D, E, F |
| Clavichords | Clavichord (with Bebung) |
| Harps | Harp A (a concert harp), Harp B (a lever harp) |
| Guitars | Guitar, Strumstick |
| Zithers | Psaltery, Dan tranh (with the pitch wheel's bend) |
| Percussion | Vibraphone, Marimba, Xylophone, Balafon, Bells, Tubular bells, Crotales, Glockenspiel A, B, Hand chimes |
| Thumb pianos | Kalimba, Mbira A, B, Nyunga nyunga |

The editor makes more: new instruments from a sample set, pianos morphed between two others, and
rolled pianos that no recording holds (a "Rolled pianos" group).

## Download and install

From the [releases](https://github.com/ccoors/dussek/releases), each with:

- **Windows** (10 or newer, 64 bit): `dussek-<version>-windows-setup.exe` installs the standalone
  (in the Start menu), the LV2, CLAP and VST3 plugins (each one optional) and the instruments, with
  an uninstaller. It is not signed: SmartScreen warns about an unknown publisher; "More info", then
  "Run anyway".
- **macOS** (Apple silicon, 13.3 or newer; untested, reports welcome): `dussek-<version>-macos.pkg`
  installs Dussek in Applications and the plugins in `/Library/Audio/Plug-Ins`. It is not signed
  by a registered developer: right-click it and choose Open, or allow it in System Settings →
  Privacy & Security ("Open Anyway").
- **Linux** (x86-64, glibc 2.35 or newer): unpack `dussek-<version>-linux.tar.gz` and run
  `./install.sh` (for you, under your home folder) or `sudo ./install.sh --system` (for every
  user); `./uninstall.sh` (or `sudo ./uninstall.sh --system`) removes it.

If you tried a test build before, remove the bundles you copied by hand first (`dussek.lv2`,
`dussek.clap`, `dussek.vst3` in `%APPDATA%\LV2` or `~/Library/Audio/Plug-Ins`): hosts prefer those
to the installed ones, and they read the instruments copied with them.

Each installer puts the instruments in one place that the standalone and every plugin format
read. Where a host offers LV2 or CLAP, prefer them to the VST3 ([docs/plugin.md](docs/plugin.md)).
The `-portable` archives hold the same files to copy by hand; between releases, the CI workflows'
runs (the Actions tab, signed in to GitHub; kept 3 days) carry test builds of the same kind.

## Build from source

You need Rust (with `cargo-nextest` for the tests), CMake, a C++17 compiler, Python 3.11 or newer
with the packages of `requirements.txt` (`pip install -r requirements.txt`: numpy, scipy,
soundfile, matplotlib, mido, and pytest for the tests), and [just](https://github.com/casey/just).
On Linux the plugin needs the development packages of OpenGL, X11, JACK, ALSA and PulseAudio
(`.github/workflows/ci.yml` lists Ubuntu's).

```sh
git clone --recursive https://github.com/ccoors/dussek.git && cd dussek
just build         # the engine, the plugin (DPF comes as a git submodule) and the editor
just install       # links the plugin into ~/.lv2, ~/.clap, ~/.vst3; the instruments into ~/.local/share
just standalone    # the JACK standalone under PipeWire, with an ALSA MIDI port
just midi2wav instruments/grand_a song.mid song.wav   # a MIDI file rendered offline
just run-editor    # the instrument editor
just test          # all four test suites
just               # every recipe
```

The instruments come with the repository and play without any recordings. To fit instruments
yourself, get the datasets first ([docs/getting-data.md](docs/getting-data.md)).

## Quick starts

- [docs/quick-start.md](docs/quick-start.md): the plugin. Installing, picking an instrument, the
  controls that matter.
- [docs/quick-start-editor.md](docs/quick-start-editor.md): a new instrument from a sample set,
  with the editor (experimental).

## Licenses

- The code: [AGPL-3.0-or-later](LICENSE).
- The instruments: each carries `instruments/<dir>/LICENSE.md` with its license and the credits
  of the recordings it was measured from. They are CC BY 4.0, except Upright C, which is
  CC BY-SA 4.0. An instrument made with a ShareAlike part (a borrowing, or a rolled piano's
  donor) takes CC BY-SA 4.0 too; its license file says so.
- The demo pieces (`albums/demos/`): CC0.
- Music you make with the instruments is yours, free to use without credit.

The recordings the instruments were measured from:

| Menu name | Measured from | The recordings' license |
|---|---|---|
| Grand A | Salamander Grand Piano V3 | public domain (CC BY 3.0 before) |
| Grand B | University of Iowa MIS piano | any project, without restrictions |
| Grand C | Bengt Nilsson's Headroom Piano | CC BY 4.0 |
| Grand D | Ivy Audio's Piano in 162 | free for personal or commercial use |
| Grand E | VCSL "S Model B 1895", normalized (its levels from Grand B's recordings) | CC0 |
| Grand F | VCSL grand "K" | CC0 |
| Grand G | the Osiris Piano: worn, recorded softly with the soft pedal down | CC0 |
| Grand H | the City Piano (Big Cat Instruments), a baby grand: normalized (its levels from Grand B's recordings) | public domain |
| Grand I | the OLPC sound library's grand piano multisamples (Zenph Studios): a reproducing piano played by computer | CC BY 3.0 |
| Upright A, B | VCSL uprights "Knight" and "Y" | CC0 |
| Upright C | KeyPleezer's LivingRoom Upright | CC BY-SA 4.0 (the export too) |
| Upright D | an upright in a living room, the maintainer's own recordings (not published) | CC BY 4.0 |
| Harpsichord A | VCSL harpsichord "Unk" | CC0 |
| Harpsichord B | VCSL Flemish harpsichord: 8′ and 4′, a derived 8′ II and lute | CC0 |
| Harpsichord C, D | VCSL French and Italian harpsichords | CC0 |
| Harpsichord E | Edinburgh's double-manual French harpsichord, Luigi Baillon, 1755: both manuals' 8′ | CC BY 4.0 |
| Harpsichord F | Edinburgh's single-manual English harpsichord, Thomas Barton, 1709: both 8′ choirs | CC BY 4.0 |
| Clavichord | Edinburgh's unfretted clavichord, ca. 1740 | CC BY 4.0 |
| Harp A | VCSL Concert Harp | CC0 |
| Harp B | Etherealwinds Harp II CE, a Celtic lever harp | CC BY 4.0 |
| Guitar | University of Iowa MIS classical guitar | any project, without restrictions |
| Strumstick | VCSL strumstick | CC0 |
| Psaltery, Dan tranh | VCSL plucked psaltery and đàn tranh | CC0 |
| Vibraphone, Marimba, Xylophone, Bells, Crotales | University of Iowa MIS percussion | any project, without restrictions |
| Tubular bells, Hand chimes, Kalimba, Mbira A, Mbira B, Nyunga nyunga, Balafon, Glockenspiel B | VCSL | CC0 |
| Glockenspiel A | Ethan Winer's SoundFonts | any use, no attribution required |

The recordings are used for analysis only: no third-party audio is in the repository or the
engine.

## Contributing

Bug reports and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) says how the
project works: the tests, the commit messages, the design log, and listening before a sound
change goes in.

## Documentation

- [docs/plugin.md](docs/plugin.md): the plugin. Covers building, the controls, presets, microphones,
  the touch curve, morphing, tuning, MIDI and MIDI learn.
- [docs/engine.md](docs/engine.md): the engine's command-line tools (`render`, `bench`,
  `midi2wav`) and how close the engine is to the prototype.
- [docs/editor.md](docs/editor.md): the instrument editor. Covers new instruments from a sample
  set, comparing with the recordings, level corrections and rolled pianos.
- [docs/building-instruments.md](docs/building-instruments.md): projects, the fitting chains and
  `build.py`.
- [docs/getting-data.md](docs/getting-data.md): where each dataset comes from and where it goes.
- [docs/recording-a-piano.md](docs/recording-a-piano.md): a session sheet for sampling a piano.
- [docs/architecture.md](docs/architecture.md): how the system works, layer by layer.
- [docs/compatibility.md](docs/compatibility.md): what stays loadable across releases, and how.
- [docs/lab/](docs/lab/): the engine's ideas as six small experiments.
- [docs/lessons.md](docs/lessons.md): what the project learned about building and judging a
  modeled instrument.
- [docs/log/](docs/log/README.md): the design log, every change with its reasons and numbers.
- [ROADMAP.md](ROADMAP.md): what is next ([docs/roadmap-notes.md](docs/roadmap-notes.md): the
  background).
- [AGENTS.md](AGENTS.md): conventions for working on the code, for people and coding agents.

## Layout

| Path | What |
|---|---|
| `projects/` | one project per instrument (`project.toml`, `fits/`, `ledger.json`); `steps.toml`: the fitting chains |
| `analysis/` | measurements from the recordings (Python) |
| `proto/` | the offline model, its fits and the exports (Python) |
| `instruments/` | the exported instruments |
| `engine/` | the real-time engine (Rust) and its tools |
| `plugin/` | the DPF plugin (C++) and its UI; `plugin/dpf`: DPF, a git submodule |
| `editor/` | the instrument editor (Rust, egui) |
| `albums/demos/` | original demo pieces (CC0) and the scripts that compose and mix them |
| `data/` | the datasets (not in git; `data/SOURCES.md`) and test MIDI files |
| `docs/` | the documentation and the design log |
