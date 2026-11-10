# The itch.io page

What the project's page on itch.io says, field by field (Dashboard → Create new project, later
Edit project). The description is plain paragraphs and lists: it pastes into itch's editor as they are.

## The fields

- **Title:** Dussek
- **Project URL:** `<user>.itch.io/dussek` (the ITCH_TARGET variable is then `<user>/dussek`)
- **Short description or tagline:** A free piano, harpsichord, harp and percussion instrument: every note simulated, none recorded.
- **Classification:** Tools
- **Kind of project:** Downloadable
- **Release status:** Released
- **Pricing:** No payments, or Donate (the default amount 0); the downloads stay free either way
- **Uploads:** the five channels `packaging/itch.sh` pushes; their display names on the page:
  - `windows-setup`: Windows installer (64 bit)
  - `windows-portable`: Windows, portable
  - `mac-pkg`: macOS installer (Apple silicon, untested)
  - `mac-portable`: macOS, portable (Apple silicon, untested)
  - `linux`: Linux (x86-64; unpack, run ./install.sh)
- **Genre:** none (tools have none)
- **Tags** (10 at most): piano, synthesizer, audio-plugin, vst, lv2, clap, virtual-instrument, harpsichord, music-production, open-source
- **App store links:** none
- **Community:** Comments
- **Visibility:** Draft until the uploads and the screenshots are in, then Public
- **Cover image:** `cover.png` (630 × 500)
- **Screenshots** (`just screenshots` renders them into `site/out/screenshots/`), in this order:
  `grand-visualization.png`, `grand-sound.png`, `grand-felt.png`, `grand-mics.png`, `vibraphone.png`
- **Gameplay video or trailer:** none (the website plays the demos)
- **Theme** (Edit theme): background #0b0e15, text #ffffff, links and buttons #f6e15a

## The description

Dussek is a free piano, harpsichord, harp and percussion instrument for your computer, as an LV2, CLAP and VST3 plugin and a standalone application.

None of it plays back a recording. The engine simulates the hammer (or plectrum, or mallet), stiff strings with coupled unisons, the soundboard, the dampers and the pedals in real time. Each instrument's parameters were measured from freely licensed recordings of a real one, so the instruments ship as numbers, without any audio: the whole download, every instrument in it, is 30 to 45 MB.

Listen to every kind of instrument on the website: https://ccoors.github.io/dussek/

The instruments (41):

- Grand pianos: Grand A to I (nine)
- Upright pianos: Upright A to D
- Harpsichords: Harpsichord A to F (Harpsichord B with 8′, 4′, a second 8′ and the lute stop)
- Clavichord, with Bebung
- Harps: a concert harp and a lever harp
- Guitars: Guitar, Strumstick
- Zithers: Psaltery, Dan tranh (with the pitch wheel's bend)
- Percussion: vibraphone, marimba, xylophone, balafon, bells, tubular bells, crotales, two glockenspiels, hand chimes, tongue drum
- Thumb pianos: kalimba, two mbiras, nyunga nyunga

Weidinger, in the same downloads: a second plugin from the same sources for the brass (trumpet, horn, tenor and bass trombone, tuba), as its own standalone and LV2, CLAP and VST3 plugins. A preview: the installers offer it as a component that can be left out, and its sound and controls may still change.

What it does:

- A continuous response to velocity, fast repetition, half pedaling, sympathetic resonance and low latency
- The lid, the microphones and their distance; a Visualization that draws each string as the engine moves it
- The felt of each hammer, key by key; tunings from Scala files
- An editor (built from source) that makes new instruments from your own recordings

Install:

- Windows (10 or newer, 64 bit): the installer puts the standalone in the Start menu and the LV2, CLAP and VST3 plugins in place, with an uninstaller. It is not signed: SmartScreen warns about an unknown publisher ("More info", then "Run anyway").
- macOS (Apple silicon, 13.3 or newer): the package installs Dussek in Applications and the plugins in /Library/Audio/Plug-Ins. Untested so far: reports welcome. It is not signed by a registered developer: right-click it and choose Open.
- Linux (x86-64, glibc 2.35 or newer): unpack the archive and run ./install.sh.
- The portable downloads hold the same files to copy by hand (their README.txt says where).

Where a host offers LV2 or CLAP, prefer them to the VST3.

Free software: the code is under the AGPL-3.0-or-later, each instrument under CC BY 4.0 or CC BY-SA 4.0 (the credits: https://ccoors.github.io/dussek/credits.html). The source, the documentation and the issue tracker: https://github.com/ccoors/dussek

Named after Jan Ladislav Dussek (1760–1812), the pianist who had the piano's keyboard extended to five and a half octaves.
