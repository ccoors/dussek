# Getting the data

The datasets are not in git (`data/` holds only their documentation, `data/SOURCES.md`, and the
download lists). The committed instruments play without them; they are needed to fit an
instrument again, to compare it with its recordings, or to make a new one.

Each dataset goes into its own folder under `data/`. The editor also takes a folder elsewhere
and links it in as `data/<name>`.

## Pianos

| Dataset | License | Size | Into | Projects |
|---|---|---|---|---|
| Salamander Grand Piano V3, 48 kHz/24 bit | public domain (CC BY 3.0 before) | | extracted into `data/salamander-grand-v3/` | `grand_a` (Grand A) |
| University of Iowa MIS piano | free for any project (the MIS page) | 1.4 GB | `just fetch-iowa` → `data/iowa-mis-piano/` | `grand_b` (Grand B) |
| [VCSL Keys](https://versilian-studios.com/Distro/VCSL_Keys.zip) | CC0 | 680 MB | `data/vcsl-keys/` | `grand_f` (grand "K"), `grand_e` ("S Model B"), `upright_a` (upright "Knight"), `upright_b` (upright "Y"); the harpsichords below |
| [Piano in 162](https://www.ivyaudio.com), SFZ edition (Ivy Audio) | free for personal or commercial use | 4.7 GB | `data/piano-in-162/` | `grand_d` (Grand D; its layers' recording gains are undone in `analysis/datasets.py`) |
| [Headroom Piano](https://github.com/sfzinstruments/BengtNilsson.HeadroomPiano) (Bengt Nilsson) | CC BY 4.0 | 284 MB | `git clone` into `data/headroom-piano/` | `grand_c` (Grand C, credited) |
| [Osiris Piano](https://github.com/sfzinstruments/Osiris_Piano) (Versilian Studios, Karoryfer) | CC0 | 908 MB | `git clone` into `data/osiris-piano/` | `grand_g` (Grand G) |
| [LivingRoom Upright, Micro SFZ](https://downloads.keypleezer.com/dl-free-version-micro-sfz/v100/livingroom-upright-piano-micro-sfz.zip) (KeyPleezer) | audio CC BY-SA 4.0 | 91 MB | `data/livingroom-upright/` | `upright_c` (Upright C; its export is CC BY-SA 4.0 too) |

## Harpsichords, clavichord, harps and plucked strings

| Dataset | License | Size | Into | Projects |
|---|---|---|---|---|
| VCSL Keys (above) | CC0 | | `data/vcsl-keys/` | `harpsichord` ("Unk"), `flemish8` and `flemish4` (the Flemish registers), `french`, `italian` |
| [VCSL](https://github.com/sgossner/VCSL), the whole library | CC0 | | `git clone` into `data/vcsl/` | `harp` (Concert Harp), `psaltery`, `strumstick`, `dantranh`; the percussion below |
| [Etherealwinds Harp II CE](https://versilian-studios.com/etherealwinds-harp/), `EWHarp2CE_SFZ-Raw.zip` | free for any use, CC BY 4.0 on its page | 364 MB | `data/etherealwinds-harp/` | `leverharp` (Harp B, credited) |
| [Unfretted Clavichord, ca. 1740](https://doi.org/10.7488/ds/8090) (University of Edinburgh), `Unfretted_Clavichord_ca1740s.zip` | CC BY 4.0 | 256 MB | unpacked into `data/edinburgh-clavichord/raw/` | `clavichord` (credited) |
| [Double-Manual Harpsichord, Luigi Baillon, 1755](https://doi.org/10.7488/ds/8091) (University of Edinburgh), `Harpsichord_Double_Manual_Luigi-Baillon-1755.zip` | CC BY 4.0 | 151 MB | unpacked into `data/edinburgh-baillon/raw/` | `harpsichord_e_lower`, `harpsichord_e_upper` (Harpsichord E, credited) |
| [Single-Manual Harpsichord, Thomas Barton, 1709](https://doi.org/10.7488/ds/8088) (University of Edinburgh), `Harpsichord_Single_Manual_Thomas-Barton-1709.zip` | CC BY 4.0 | 613 MB | unpacked into `data/edinburgh-barton/raw/` | `harpsichord_f_right`, `harpsichord_f_left` (Harpsichord F, credited) |
| University of Iowa MIS guitar, anechoic | free for any project (the MIS page) | 1.6 GB | `just fetch-iowa guitar`, then `python3 analysis/split_runs.py Guitar` | `guitar` |

## Percussion

The University of Iowa MIS percussion was recorded anechoic and "may be downloaded and used for
any projects, without restrictions". Its files are octave runs; `analysis/split_runs.py` cuts
them into one file per note under `data/iowa-mis-percussion/notes/` once.

| Dataset | Files from its page | Then | Projects |
|---|---|---|---|
| [Vibraphone](https://theremin.music.uiowa.edu/MISvibraphone.html), 1.1 GB | the 24 runs `Vibraphone.<sustain\|dampen>.<pp\|mf\|ff>.<range>.aif` | `python3 analysis/split_runs.py` | `vibraphone` |
| [Marimba](https://theremin.music.uiowa.edu/MISmarimba.html), 1.1 GB | the 54 runs `Marimba.<yarn\|cord\|rubber>.<pp\|mf\|ff>.<range>.aif` | `python3 analysis/split_runs.py Marimba` | `marimba` |
| [Xylophone](https://theremin.music.uiowa.edu/MISxylophone.html) | the 30 runs `xylophone.<rosewood\|hardrubber>.<pp\|mf\|ff>.<range>.aif` | `python3 analysis/split_runs.py xylophone` | `xylophone` |
| [Bells](https://theremin.music.uiowa.edu/MISbells.html) | the 16 runs `bells.plastic.<pp\|mf\|ff>.<range>.aif` and `bells.brass.<range>.aif` | `python3 analysis/split_runs.py bells` | `bells` |
| [Crotales](https://theremin.music.uiowa.edu/MIScrotales.html) | the 50 files `crotale.<note>.<pp\|ff>.aif` | `python3 analysis/split_runs.py crotale` | `crotales` |

All into `data/iowa-mis-percussion/`.

From VCSL (above): the tubular bells (`chimes`), the hand chimes (`handchimes`), the Tanzanian
kalimba (`kalimba`), the mbiras (`mbira`, `mavembe`, `nyunga`), the balafon (`balafon`) and the
glockenspiel (`glockenspiel_b`).

[Ethan Winer's SoundFonts](https://github.com/sfzinstruments/EthanWiner.Soundfonts), the SFZ
conversion (royalty free, any use, no attribution required, 17 MB): `git clone` into
`data/ethanwiner-soundfonts`. Its glockenspiel is `glockenspiel`, credited.

## A set of one's own

Sampling a piano oneself: the session sheet [recording-a-piano.md](recording-a-piano.md) (the
setup, the passes, a table of takes to tick).

## Test MIDI files

`python3 data/make_test_midi.py` writes test files into `data/midis/test/`:

- velocity ramps (on one key, and across the keyboard);
- repeated notes, speeding up to 30 per second, with and without the pedal;
- crescendos;
- notes struck again while held.
