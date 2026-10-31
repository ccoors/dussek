# Data sources

Recordings are used for **analysis only** (parameter extraction). No audio from
these sets is embedded in or distributed with the engine.

## Salamander Grand Piano V3 (48 kHz / 24 bit)

- Location: `salamander-grand-v3/` (original folder name `SalamanderGrandPianoV3_48khz24bit`)
- Instrument: Yamaha C5 grand
- Author: Alexander Holm
- License: public domain: "As of 4.3.2022, this is now public domain!" (its author's page,
  https://rytmenpinne.wordpress.com/sounds-and-such/salamander-grandpiano/); CC-BY 3.0 before
  (http://creativecommons.org/licenses/by/3.0/), as its README and mirrors still say
- Recording: 2× AKG C414, AB pair ~12 cm above the strings
- Content: 16 velocity layers, sampled in minor thirds from A0 (30 notes, 480 files);
  release/hammer-noise and string-resonance samples, pedal noises
- Notes from inspection:
  - Each key scaled by one gain: its v16 peaks at −1.5 dBFS on every key, its layers keep their
    steps (v1 peaks around −15…−34 dBFS) and share one noise floor; the keys' floors lie up to
    15 dB apart, so the level along the keys is not the recorded one (`levels = "per_key"`)
  - The Accurate-Salamander remaster (Chisato Yamauchi, CC BY 3.0;
    https://www.ir.isas.jaxa.jp/~cyamauch/AccurateSalamander/, its scripts and measurements at
    https://github.com/cyamauch/NoctSalamanderGrandPiano) is filtered, re-leveled and rebuilds two
    notes: not for fitting. Its soundbank #0 (V6.2a, FLAC 48 kHz / 24 bit, 784 MB:
    `AccurateSalamanderGrandPianoV6.2a_48khz24bit_flac.zip` from
    https://archive.org/download/httpswww.ir.isas.jaxa.jpcyamauchaccuratesalamander/) lies in
    `accurate-salamander/` for comparison: `48khz24bit/NNN_<key>v<layer>.flac`, each output layer
    taken from the original layer its `src/assign.txt` names (A1's v16 and D#2's v8 unused). Its notes name flawed takes: F#1 and C3 an abnormal tone (a defective
    hammer or string), C4v9 struck twice, C4v15 footsteps after 4 s, F#2v1/v2 a "bom-bom-bom" at the
    strike, A2v1 noise after 14.2 s and at 3848 Hz in the attack, F#7v1 noise after 1 s
  - Durations 2.6–25.9 s; tails have a short fade-out applied
    (~20 dB drop in the last 0.5 s) → exclude the last ~0.6 s from decay fits

## University of Iowa MIS — Piano

- Location: `iowa-mis-piano/` (download with `fetch_iowa.sh`, URL list in `iowa-mis-piano-links.txt`)
- Source: https://theremin.music.uiowa.edu/MISpiano.html
- Instrument: Steinway & Sons model B, recorded Nov 2001, Voxman Music Building
- Recording: 2× Neumann KM 84, 8" above center bass / treble strings; DAT,
  16 bit / 44.1 kHz stereo, **non-anechoic**
- Content: every note A0/Bb0/B0–C8 at pp, mf, ff (260 files; pp starts at Bb0, mf at B0)
- License: the MIS page (https://theremin.music.uiowa.edu/MIS.html), for the whole collection: "Since 1997,
  these recordings have been freely available on this website and may be downloaded and used for
  any projects, without restrictions." No audio of it is redistributed here.
- Notes from inspection:
  - Untrimmed: silence before onset, full decay into the noise floor (~35 s in the bass)
  - Recording noise floor is high relative to pp notes (only ~30 dB of usable decay
    range at pp in the bass) → noise-floor estimation per file is required
  - Some files are zero-padded (digital silence) before the onset and/or at the end
  - **Mislabeled files** (verified by matching upper partials against other files):
    - `Piano.ff.A0.aiff` is actually Bb0
    - `Piano.ff.Bb0.aiff` and `Piano.ff.B0.aiff` are both actually C1
    - `Piano.pp.C1.aiff` is actually B0 (same note as `Piano.pp.B0.aiff`)
    - → there is no real A0 recording in this set; the analysis flags these as `label_mismatch`

## University of Iowa MIS — Percussion (vibraphone, marimba, xylophone, bells, crotales)

- Location: `iowa-mis-percussion/` (downloaded 2026-10-11 from
  https://theremin.music.uiowa.edu/MISvibraphone.html: `Vibraphone.<sustain|dampen>.<pp|mf|ff>.<C3B3|C4B4|C5B5|C6F6>.aif`,
  24 files, 1.1 GB; also one octave each of xylophone and bells for a first look), and from
  https://theremin.music.uiowa.edu/MISmarimba.html `Marimba.<yarn|cord|rubber>.<pp|mf|ff>.<C2B2|…|C6B6|C7>.aif`
  (normal strokes with three kinds of mallet, 54 files, 1.1 GB; the rolls and dead strokes not downloaded),
  and from https://theremin.music.uiowa.edu/MISxylophone.html `xylophone.<rosewood|hardrubber>.<pp|mf|ff>.<F4B4|C5B5|C6B6|C7B7|C8>.aif`
  (30 files; the rolls and glissandi not downloaded), and from
  https://theremin.music.uiowa.edu/MISbells.html `bells.plastic.<pp|mf|ff>.<C5B5|C6B6|C7B7|C8E8>.aif` and
  `bells.brass.<C5B5|…|C8E8>.aif` (16 files, 0.6 GB; the brass runs name no dynamic), and from
  https://theremin.music.uiowa.edu/MIScrotales.html `crotale.<C6|Db6|…|C8>.<pp|ff>.aif` (one note
  per file, 50 files, 119 MB)
- Recording: 2013 (the marimba March 20, 2013, Andrew Thierauf), anechoic chamber, 5 ft from the
  instrument, Earthworks QTC40, 24 bit stereo: the vibraphone 44.1 kHz, the marimba 96 kHz; no motor
- License: the MIS page (https://theremin.music.uiowa.edu/MIS.html), for the whole collection: "Since 1997,
  these recordings have been freely available on this website and may be downloaded and used for
  any projects, without restrictions." No audio of it is redistributed here.
- Content: chromatic octave runs C3–F6 (42 notes) per dynamic; "sustain" with the pedal down
  (each note ~15–30 s, until the next strike: decays end 20–40 dB down), "dampen" with the
  pedal up (the damper bar on the bars, ~2.5 s per note)
- Notes from inspection: `analysis/split_runs.py` cuts the runs per note (at each note's own
  fundamental); the pp sustain run C4–B4 keeps an uneven pace: its notes 61–64 come out a note
  off and the analysis flags them. The set is tuned ~10 cents sharp (A ≈ 442.5 Hz).
- The marimba: C2–C7 (61 notes) per mallet and dynamic, no dampers (each note ~2–7 s until the
  next strike, decayed by then); tuned 12–19 cents sharp. The rubber pp run C2–B2 keeps an uneven
  pace: its notes 37–41 come out off and the analysis flags them.
- The xylophone (June 10, 2013, 44.1 kHz): F4–C8 (44 notes) per mallet and dynamic, no dampers
  (~1–3 s per note); tuned 5–18 cents sharp; the hard rubber's G♯7 and C8 are flagged.
- The bells (orchestra bells, a glockenspiel; June 10, 2013, 44.1 kHz): C5–E8 (41 notes), plastic
  mallets at pp, mf and ff, brass at one dynamic (its fundamentals 12 dB under the plastic's mf:
  `split_runs.py` takes it as mf), no dampers (~5–25 s per note until the next strike); they
  sound as written (C5 = 523 Hz); tuned 9–31 cents sharp, rising to the top. The plastic ff
  run C5–B5 starts on its first strike (~20 ms in: the splitter pads such a run in front).
  Peaks a semitone or a whole tone off the fundamental, ~20 dB under it, in some mf and pp
  takes only (the same keys at ff: 50–70 dB under): a bar touched in those runs, not modeled.
- The crotales (March 29, 2013, 44.1 kHz; C8 pp mono): C6–C8 (25 notes), one mallet (not named)
  at pp and ff, ~5–17 s per note; they sound as named, tuned 4–32 cents sharp. The pp takes lie
  ~35 dB under the ff, with the room's low rumble and noise bursts as loud in broadband.

## University of Iowa MIS — Guitar

- Location: `iowa-mis-guitar/` (download with `fetch_iowa.sh guitar`, URL list in
  `iowa-mis-guitar-links.txt`: the 45 stereo runs of https://theremin.music.uiowa.edu/MISguitar.html,
  1.6 GB; the page's zips hold the same notes)
- Instrument: Raimundo 118 (a classical guitar, nylon strings), Brian Penkrot, December 11, 2011,
  anechoic chamber, 5 ft, Earthworks QTC40, 24 bit / 96 kHz stereo
- License: the MIS page (https://theremin.music.uiowa.edu/MIS.html), for the whole collection: "Since 1997,
  these recordings have been freely available on this website and may be downloaded and used for
  any projects, without restrictions." No audio of it is redistributed here.
- Content: one chromatic run per string and dynamic, `Guitar.<pp|mf|ff>.<sulE|sulA|sulD|sulG|sulB|sul_E>.<range>.stereo.aif`
  (strings 6 … 1: E2–B3, A2–E4, D3–A♭4, G3–D♭5, B3–G♭5, E4–B5; the ff high E ends at B♭5), 352
  notes: each pitch on up to five strings
- Notes from inspection: the runs are edited, not played as runs: every note rings out (9–15 s,
  55–90 dB over the floor) and the notes are joined by digital silence, some by a splice instead
  (the previous note faded within ~0.15 s, the next 40–70 dB louder at once); `split_runs.py
  Guitar` cuts both and labels each note by its pitch. The pp sulE C3–B3 run has no D3. Tuned
  ~20 cents flat (the frets ±10 cents note to note). The background is rumble below 10 Hz.
  The top's main mode (~224 Hz) takes the energy of every partial near it: G3–B♭3's fundamentals
  decay at 50–135 dB/s on every string, their neighbors at 15–30 (`proto/fit_body_modes.py`).

## Versilian Community Sample Library — Keys (VCSL Keys)

- Location: `vcsl-keys/` (https://versilian-studios.com/Distro/VCSL_Keys.zip, 680 MB; repository
  https://github.com/sgossner/VCSL)
- License: CC0 1.0 ("as a derivative of VCSL"), from Versilian Studios' own (commercial) libraries
- Content: 44.1 kHz FLAC, SFZ mappings (the file names use differing octave numbers: map by
  `pitch_keycenter`); pianos (besides harpsichords):
  - Grand "S Model B 1895": 42 notes (whole tones), 3 layers, pedal down (`Sus`) and up
    (`NoSus`), releases; **normalized** per sample (NORMALIZED.txt). Profile `grand_e`
    (Grand E): pedal-up sustains, vl2/vl3/vl4 as pp/mf/ff, no level target (the levels are gone)
  - Grand "K": ~38 notes (whole tones), mostly 4 layers, releases; levels as recorded (the SFZ
    `volume` evens them out). Profile `grand_f` (Grand F); the top treble lacks the soft
    layers, the dynamics follow the file's layer number (v1 pp, v2 mf, v4 ff)
  - Grand "K – Legacy": 19 notes, 4 layers, normalized, "funky tuning" (its own note)
  - Upright "Knight": 45 notes (whole tones), 2 layers, releases, pedal on/off noises; levels
    as recorded. By Ivy Audio for VSCO 2 Pro (Info.txt)
  - Upright "Y": 14 notes (~every 6–7 semitones), 3 layers, 2 round robins, releases;
    levels as recorded. Profile `upright_b` (Upright B). Its lowest file, mapped to C1, sounds
    B♭0 (partials ~29.5 Hz apart): the loader relabels it
  - Harpsichord "Unk": 61 notes F1–F6 (chromatic), one layer (`amp_veltrack=0`), one
    release sample per note; levels as recorded; equal temperament at A = 440 Hz. Profile
    `harpsichord`. Its sustain files end in an edit fade (−60 … −78 dB), not a damper landing
  - Harpsichord "Flemish": 8′ (28 notes) and 4′ (26, each sounding an octave above its
    key, files in `Sustains/High`), whole tones F♯1–C6, one layer, release samples; levels
    as recorded (the set's "Full" mix layers both at their file levels). Profiles
    `flemish8`, `flemish4`; instrument `flemish` (Harpsichord B)
  - Harpsichord "French": 28 notes D1–C6 (whole tones, D0 the first file), one layer, 2 round
    robins (the first fitted), release samples. Profile `french` (Harpsichord C)
  - Harpsichord "Italian": 32 notes F♯1–B5 (every 1–2 semitones; files in `Sustains/stop1`),
    one layer, release samples. Profile `italian` (Harpsichord D)
  - Harpsichord "English" (a Zuckermann kit): 28 notes A♯1–E6 (MIDI 34–88; the file names
    say an octave lower) recorded "Normal" and "Lute" (the "Lute" is a buff: damped and darker
    at the same level), release samples per note; profiles `english`, `english_buff`: the
    source of the buff stop's fit (`proto/fit_buff.py`), not an instrument
- Survey: `analysis/survey.py vcsl-* data/vcsl-keys`

## Versilian Community Sample Library (VCSL, the whole library)

- Location: `vcsl/` (https://github.com/sgossner/VCSL); `vsco-2-ce/` holds VSCO 2 CE
  (https://github.com/sgossner/VSCO-2-CE), whose harp is the same session with fewer files
- License: CC0 1.0 (`vcsl/LICENSE`)
- Concert Harp (`Chordophones/Composite Chordophones/Concert Harp`, not mapped in an SFZ):
  `KSHarp_<note><octave>_<dynamic><take>.wav`, C4 = 60; 23 strings E1–F7 in thirds,
  44.1 kHz stereo, full decays in the middle and treble (the bass is cut while still
  ringing: A2 −66 dB at 11 s). f and mf on most notes: E1 has f only, G1's second take is
  mp, D7's and F7's p (20–30 dB below f), C3's takes are f2 and mf3. Tuned about 12 cents
  below A = 440 Hz, string by string 0 … −36 cents. Profile `harp` (Harp A): f and mf as ff
  and mf (`datasets.harp_files`)
- Tubular bells (`Idiophones/Struck Idiophones/Tubular Bells 1`, `2`, `3 - Legacy`): three
  recordings of the same 18 tubes (their modes agree within 1 Hz). "Tubular Bells 2"
  (`TB_hit_<note>_v<2|4>_<take>.wav`, named at the strike note): the white keys C4–F5 at two
  dynamics, 44.1 kHz stereo, the floor at −88 dB, decays of 10–44 s, the strike note 2–13 cents
  sharp; profile `chimes` (v2, v4 as mf, ff; `datasets.chimes_files`). "3 - Legacy" (`chimes_<note>.ogg`,
  named three octaves low): all 18 chromatic, lossy, one dynamic, cut at 5–11 s: `chimes_check`,
  the black keys' check, not fitted. "1" (`chimes_<note>_<dyn>_rr<n>.wav`, named an octave low): whole
  tones, noisy (floor −60 dB, mains hum at 120 Hz): unused. No damped takes. The modes, re the
  fourth (the strike note's double, where no mode is below): 0.31, 0.61, 1, 1.48 → 1.45, 2.04 →
  1.97, 2.67 → 2.53, 3.37 → 3.13, 4.13 → 3.76, 4.94 → 4.53 from C4 to F5; they beat slowly (split
  pairs: the two lowest 1.8–2.5 dB at 0.3 Hz, the others 0.5–0.75 dB at 0.3–1.2 Hz)
- Hand Chimes (`Idiophones/Struck Idiophones/Hand Chimes`, `sus_<note>_r01_main.wav`, named an
  octave low: `sus_C3` sounds at C4): 19 tubes C4–C7 a whole tone apart, one take each at one
  dynamic, 44.1 kHz stereo, every file normalized to −15 dBFS peak, C4–G♯4 cut while ringing
  (at 8–16 s). The fundamental rings 2–22 dB/s; its harmonics 2, 3 and 5 (−24 to −80 dB, from
  take to take) decay 2–4 × faster; a mode at 1.49–1.57 and one at 5.8–6.0 up to E5, fast.
  Profile `handchimes` (`datasets.handchimes_files`)
- Kalimba, Tanzania (`Idiophones/Plucked Idiophones/Kalimba, Tanzania`,
  `MBira3_pluck_Main_<note>_k<tine>_50_100_rr2.wav`, named an octave low): 27 tines G2–C♯7 (D♯5 and
  F5 twice), one pluck each, 48 kHz stereo, normalized to −2 dBFS peak, 1.1–7.5 s, floors at −77 to
  −125 dB. Tuned up to 65 cents off the names. The tines' cantilever modes 1 : 6.3–6.8 : ~18 (the
  second strongest at the pluck in the bass, gone within 0.1 s); 13–20 of the other tines ring along
  in every take (−10 to −45 dB re the plucked fundamental, the treble's plucked tines outlasted by
  them). Profile `kalimba` (`datasets.kalimba_files`). The other kalimba (Kenya, 15 tines B3–B5):
  unused
- The three mbiras (`Idiophones/Plucked Idiophones/`, all named an octave low, one pluck per tine at
  one dynamic, 48 kHz / 24 bit stereo, buzzers on their boards: between the lines −26 … −55 dB re
  the fundamental, the kalimba's −53 … −76):
  - Mbira dzaVadzimu Nyamaropa, Zimbabwe, Low B (`MBira4_pluck_Main_<note>_<tine>_50_100_rr<n>.wav`):
    24 tines B2–C♯6 (B4 and C♯5 twice), the recorded levels kept (peaks −32 … −13 dBFS), 1.9–6.9 s,
    tuned −20 … +55 cents off the names; its lowest file, named A♯1, sounds B2 + 17 cents (relabeled).
    Project `mbira` (Mbira A)
  - Mbira Mavembe (Gandanga), Zimbabwe, Low G (`Mbira5_Normal_MainSpirit_<note>_k<tine>_vl2_rr1.wav`):
    24 tines A♯2–A5 (G4 and G♯4 twice), normalized (−2 … −3.4 dBFS), 3.2–8.3 s, −51 … +19 cents.
    Project `mavembe` (Mbira B)
  - Nyunga Nyunga, Mozambique, Low F (`Mbira2_Normal_MainSpirit_<note>_k<tine>_vl3_rr<n>.wav`): 19
    tines F3–F5 (five pitches twice), normalized (−2 … −2.8 dBFS), 2.6–4.8 s, −27 … +29 cents.
    Project `nyunga` (Nyunga nyunga)
- Balafon (`Idiophones/Struck Idiophones/Balafon/<Hard|Soft|Traditional> Mallet`,
  `EthnicXylo_<mallet>M_<note>_vl<1-3>_rr<n>_Mid.wav`, named an octave low): 6 wooden bars C♯4,
  F4, C5, F5, C6, F6 with three mallets at three dynamics (vl1–vl3, 7–15 dB apart, the recorded
  levels kept), one or two takes each, 44.1 kHz stereo, 0.5–2.2 s (the bars die in 0.3–1.4 s),
  tuned within ±15 cents. Modes 1 : 3.87 → 2.30 : 8.10 → 3.93 (C♯4 → F6), a fourth at 12.3 and
  11.0 on the two lowest; the fundamental the weakest of them in the bass; the buzz: harmonics up
  to the 7th at −19 to −40 dB. Profile `balafon` (`balafon_trad`, `balafon_hard`, `balafon_soft`;
  `datasets.balafon_files`)
- Psaltery, Bowed and Plucked (`Chordophones/Zithers/Psaltery, Bowed and Plucked`, named an
  octave low): its `Pluck` takes, `BowedPsaltery_<note>_Main_Pluck_rr<n>.wav`, 11 notes A♯4–F♯6 a
  whole tone apart, one take each at one dynamic (the levels kept: peaks −12 to −24 dBFS), 3.6–5.9 s,
  44.1 kHz stereo; tuned within 3 cents; all its strings undamped (they ring along between the
  partials). Profile `psaltery` (`datasets.psaltery_files`); the bowed and spiccato takes unused
- Strumstick (`Chordophones/Composite Chordophones/Strumstick/Finger`,
  `Strumstick_Finger_Str<s>_Main_<note>_vl<1-3>_rr<n>.wav`, named an octave low): a three-string
  fretted dulcimer (open D3, A3, D4), 19 notes D3–A5 on the string each is played on (the bass to G3,
  the middle to D4, the melody string above, diatonic in D) at three dynamics (12 dB apart, the levels
  kept), 1.9–12.5 s; the bass string's fundamental 25–50 dB under its partials. Profile `strumstick`
  (`datasets.strumstick_files`)
- Đàn tranh (`Chordophones/Zithers/Dan Tranh/Normal`, `<note>_<mf|f|ff>_<n>.wav`, named an octave
  low): 16 steel strings B2–B5, pentatonic (B, C♯, D♯, F♯, G♯ in the names), at three dynamics,
  44.1 kHz mono, every file normalized to −10 dBFS, 2.2–6.6 s. Five of the lower strings sound a
  semitone above their names (C♯3 at D3, D♯3 at E3, F♯3 at G3, G♯3 at A3, C♯4 at D4: relabeled,
  `datasets.DANTRANH_RELABEL`), the bridges −43 … +49 cents off. Profile `dantranh`
  (`datasets.dantranh_files`; mf, f, ff as pp, mf, ff). Its `Gliss` and `FX` takes unused
- Glockenspiel (`Idiophones/Struck Idiophones/Glockenspiel`, `glock_<soft|medium|loud>_<note>_<take>.wav`,
  named an octave low: `glock_medium_C5` sounds at C6): seven bars G5–C8 (G5, C6, G6, C7, G7, G♯7,
  C8; G7 soft and medium only, G♯7 loud only), three dynamics (medians −10.5, −4.2 and 0 dB, the
  levels kept), one take each (soft C6 and C8: takes 2 and 3), 44.1 kHz stereo, 2.8–7.5 s, most
  faded to digital silence (G5 cut at −60 dB). The second mode 3.04 → 2.68 times the fundamental
  (G5 → C8). Project `glockenspiel_b` (Glockenspiel B), made with the editor's wizard
- Not used (looked at 2026-10-29): the Folk Harp (EWHarp, 29 notes in whole tones, a subset of
  Etherealwinds Harp II CE: Harp B); VCSL's marimba (10 bars), vibraphone (14) and xylophone (8),
  sparse beside Iowa's complete ones; the tubular glockenspiel (9 tubes, a G major scale, "non-standard
  pitch"); the slit drum (2 tongues) and the Nepalese hand bells (3); the Kenyan kalimba (15 tines,
  beside the Tanzanian's 27); the unpitched or friction ones (triangles, cowbells, agogo bells, gongs,
  bell tree, mark trees, wine glasses)

## Etherealwinds Harp II: Community Edition (SFZ, raw)

- Location: `etherealwinds-harp/` (from https://versilian-studios.com/etherealwinds-harp/,
  `EWHarp2CE_SFZ-Raw.zip`, 364 MB extracted)
- Instrument: Jordi Francis's 34-string Iona Celtic harp (a lever harp), sampled with
  Versilian Studios
- License: the included note (`HarpNote_SFZ.pdf`): "You are more than welcome to use this
  library both commercially and non-commercially in your work"; Versilian's page: CC BY 4.0
  (attribution)
- Content: `Harp_Normal.sfz` (keys an octave above the file names; `default_path=Samples\`),
  chromatic C2–A6 (MIDI 36–93, the levers' semitones sampled too), 2 layers (v2 to velocity
  80, v3 from 81; the SFZ adds 14 and 2 dB), 2 round robins (`lorand`); 44.1 kHz/16 bit
  stereo; levels as recorded (v3 a median 14.8 dB over v2), full decays (floor 56–70 dB
  under the peak, C2 rings 20–30 s); tuned at A = 440 Hz (+3.5 cents median). Also vocal
  phrases and effects (`Vocal Phrases & FX/`, unused). VCSL's "Folk Harp" is a subset
- Profile `leverharp` (Harp B): the first round robin, v2/v3 as mf/ff (`datasets.leverharp_files`)

## KeyPleezer LivingRoom Upright, Micro SFZ

- Location: `livingroom-upright/` (https://downloads.keypleezer.com/dl-free-version-micro-sfz/v100/livingroom-upright-piano-micro-sfz.zip,
  91 MB; page https://keypleezer.com/livingroom-upright-piano/free-edition/micro-sfz/)
- Instrument: a large vintage upright by a Swedish local piano maker (no name given); close mics
- Author: Anders Eklöv (KeyPleezer)
- License: audio **CC BY-SA 4.0**, SFZ files public domain (`_License/License.md` in the zip).
  The fitted export `instruments/upright_c/` is CC BY-SA 4.0 too (its `LICENSE.md`)
- Content: 33 notes (every 2–3 semitones), 2 layers (p vel 0–86, f 87–127) × pedal off/on,
  no release samples; 44.1 kHz / 16-bit FLAC, full-length tails (up to 27 s), no fades; levels
  as recorded (f 2–10 dB above p). File names count octaves one lower: map by `pitch_keycenter`
- Profile `upright_c` (Upright C): the pedal-up regions (`datasets.sfz_files`), p/f as mf/ff

## The maintainer's upright (own recordings, not published)

- Location: `own-upright/`, split from the session's files by `analysis/split_session.py`
  (`docs/recording-a-piano.md`); not downloadable: the maintainer's own recordings, not published
- Instrument: an upright piano in a living room, last tuned years ago
- Author: Christian Friedrich Coors (recorded 2026-10-31)
- License: CC BY 4.0 (the maintainer's)
- Recording: a handheld recorder's XY pair at 90°, 64 cm in front of the piano, a little left,
  1.2 m high; 48 kHz / 24 bit; the gain set once
- Content: one file per pass: pp, mf, ff (A0–C8, every third key, two takes or more, rung out),
  the keys around the breaks (D3, the last wound string; E6 and F6, where the dampers end) at
  various velocities, releases (mf, held a few seconds), C1–C8 with the pedal down, the pedal alone
- Notes from inspection (design log step 204):
  - Split: the three passes 188 strikes, 180 takes kept (two per key and dynamic), and from the
    breaks pass D3 (pp, mf, ff), E6 (mf, ff) and F6 (ff); left out: two mis-strikes stopped and
    replayed (F♯1 pp and mf), a wrong note (F2 for D♯2, pp), two keys touched rather than struck
    (C3 pp, D♯6 ff), a C8 pp cut by the file's end, two ff takes clipped (A5, A6: one of each
    pair; A5's other clips too, 9 samples, kept as its key's only take); the breaks pass's three
    loudest D3 lie between or past the dynamics. Its "D2" sounds D3 (146 Hz, the odd partials
    53–60 dB down)
  - The treble's softest takes hold too few partials over the noise for the analysis: D♯7, F♯7,
    A7 and C8 at pp, A7 and C8 at mf
  - The noise floor −79 dBFS (5 ms, 30 Hz up), −80 dB at 31.5 Hz to −91 dB at 2–4 kHz per
    octave band, no hum line over 5 dB; the notes' first 0.5 s 27–36 dB over it at pp (the
    treble least), 47–50 at mf, 55–59 at ff
  - Tuning: A4 −6 cents; the bass flat to −30 at A0, the treble within −11 … −1 to A7 (F♯7 +16,
    C8 +42 cents)
  - The two takes of a key lie 3.7, 1.9 and 0.9 dB apart (median; pp, mf, ff)
  - The pedal pass: 16 events, 17–50 dB over the floor; four pedal-downs and three pedal-ups
    (`split_session.py --pedal`), the pedal's noise and ring (steps 205–206): the thumps 5–22 dB
    (downs) and 22–31 dB (ups) under the median mf note's peak, a low thud (100–400 Hz) whose highs
    lie under the room's floor; the later blips in a take are the room's;
    down_3 and down_4 are stomps (their clack at the onset), the thumps played from down_1 and down_2,
    the ring fitted to all four (step 207); the C1–C8 pass with the
    pedal down splits into its 8 notes (unused so far)
  - Releases: the dampers' first fall 111 dB/s (median, 100 Hz–1.5 kHz; Grand E's 111), A0–E6;
    the release noise not clean (the damped strings' remainder, an edge at each take's start):
    Salamander's borrowed (design log step 205)
- Profile `upright_d` (Upright D): the SFZ the splitter writes, seq_position 1 (the more even take)

## Ivy Audio Piano in 162 (SFZ)

- Location: `piano-in-162/` (the SFZ edition from https://www.ivyaudio.com, 4.7 GB: the
  folder `Piano in 162 Samples/` and two SFZ files, close and ambient)
- Instrument: a Steinway B (Ivy Audio; SFZ files by Marcelo Colina)
- License: none in the download (`Readme.txt` has none); announced as free "for personal or
  commercial use" (https://bedroomproducersblog.com/2014/05/11/piano-in-162/). We ship no
  audio, only parameters fitted to it, and credit Ivy Audio (`instruments/grand_d/LICENSE.md`)
- Content: every key (`NN-Pedal<Off|On><Dynamic><take>Close.flac`, NN 01–88 = A0–C8), 5 layers
  (Pianissimo, Piano, MezzoPiano, MezzoForte, Forte; SFZ velocities to 33, 64, 80, 101, 127),
  2 takes, pedal off and on, close and ambient microphones; 44.1 kHz / 24-bit FLAC, natural
  tails to the noise floor (up to 34 s), no fades, floor −77 to −90 dB
- Notes from inspection: each layer (and each take) was recorded at a **gain of its own**: its
  noise floor is a constant 4–12 dB above the mf layer's in every octave band and within
  ±0.7 dB from note to note. Undone in the loader (`datasets.IVY162_GAIN_DB`, take 1), the
  attack levels re mf are pp −21, p −16, mp −8, f +9 dB (raw −17, −4, −1, +15; the SFZ
  evens them out with volume +6/+3/+1/+1/−12)
- Profile `grand_d` (Grand D): close, pedal off, take 1; pp/mf/f as pp/mf/ff
  (`datasets.ivy162_files`)

## Edinburgh DataShare — Unfretted Clavichord, ca. 1740

- Location: `edinburgh-clavichord/`: `Unfretted_Clavichord_ca1740s.zip` (256 MB) from
  https://doi.org/10.7488/ds/8090 (https://datashare.ed.ac.uk/handle/10283/9170), unpacked to
  `raw/`; `readme.txt` beside it
- Instrument: an unfretted clavichord of the 1740s (St Cecilia's Hall, University of Edinburgh;
  restored 2003), C2–D6, double strung, pitched at A = 415 Hz; played by Jenny Nex, recorded in
  April 2025 in the museum with a pair of Schoeps microphones, 48 kHz / 24 bit
- License: **CC BY 4.0** (the item's rights field; `license.txt` is the depositor's agreement).
  Export credited in `instruments/clavichord/LICENSE.md`
- Content: four folders, without and with vibrato (Bebung), each without and with noise
  reduction; one take per key, one dynamic, 2–3 s per note, some keys missing (without vibrato
  43 of the 51: no D#2, F4, G4, C5, F#5, A#5, B5, D6). The file names' first token is the key
  (`Cs4_…`); the rest, and the set's SFZ mapping, are a pitch detector's guesses (the SFZ maps
  key 34 to G#5's file)
- Notes from inspection: the museum's air conditioning is in every take (the notes 21–32 dB over
  it broadband; per partial 25–40 dB from the second up, the bass fundamentals 6–17 dB); it fills
  the gaps between the partials, so the room cannot be measured (`body_decay.py` reads 4–45 s;
  the profile's room is assumed, `soundboard.ROOM_T60_ASSUMED`). The decays are short (about
  1–3 s to −60 dB, 20 dB/s in the bass to 50 in the treble) and fall into the noise smoothly.
  C3 and C4 sound an octave above their names (even partials only) and are left out; B2's file
  holds a second strike 4.8 s in (cut). The vibrato takes swing 10–30 cents at 4–5 Hz (middle and
  treble; the bass tracks too noisily). The noise-reduced versions are "not an accurate record of
  the decay" (their readme): unused
- Profile `clavichord`: without vibrato, without noise reduction, at its own A
  (`datasets.clavichord_files`, `"a4": 415`: the analysis measures the tuning re 415, the
  instrument plays at the engine's concert pitch)

## Edinburgh DataShare — Double-Manual Harpsichord, Luigi Baillon, 1755

- Location: `edinburgh-baillon/`: `Harpsichord_Double_Manual_Luigi-Baillon-1755.zip` (151 MB) from
  https://doi.org/10.7488/ds/8091 (https://datashare.ed.ac.uk/handle/10283/9171), unpacked to
  `raw/`; `readme.txt` and `license.txt` beside it
- Instrument: a double-manual French harpsichord by Luigi Baillon, 1755 (St Cecilia's Hall,
  University of Edinburgh), F1–F6, 2 × 8′, 4′, a non-original lute, coupler and non-original buff;
  recorded April 14, 2025 in the museum by Martin Parker, strings played by Jenny Nex, a pair of
  Schoeps MK4 on a Sound Devices MixPre6, 48 kHz / 24 bit stereo
- License: **CC BY 4.0** (the item's rights field; `license.txt` is the depositor's agreement).
  Export credited in `instruments/harpsichord_e/LICENSE.md`
- Content: `Lower_Manual/` and `Upper_Manual/`, 61 keys each (`<key>_<midi>_…wav`, the names right
  at A = 415 Hz), one take per key, one dynamic, 3.2–7.5 s; `Top_octave_upperManual/` 13 more takes
  of the upper manual's F5–F6 (unused); an auto-generated SFZ per folder. Each manual sounds one 8′
  choir (no 4′ in the partials' pattern); the 4′, the lute and the buff are not recorded
- Notes from inspection: the museum's air conditioning was taken out by the set's authors' noise
  reduction ("not an accurate scientific record of the actual decay", the readme): the floors at
  −55 (upper) … −74 dBFS, never digital silence, and a high-pass: the floor falls ~20 dB per octave
  below 100–150 Hz, the lowest fundamentals 70–110 dB under partials 2–4 (the engine's bass is
  judged against the other harpsichords', not these). The files end after ~4 s: the bass's decay is
  seen over its first 30 dB. Tuned in an unequal temperament, the same on both manuals: C, F, G, B♭
  near 0 cents re A = 415 Hz, C♯, E, A, B 12–16 cents low. Projects `harpsichord_e_lower`,
  `harpsichord_e_upper` (Harpsichord E: the 8′ and the 8′ II)

## Edinburgh DataShare — Single-Manual Harpsichord, Thomas Barton, 1709

- Location: `edinburgh-barton/`: `Harpsichord_Single_Manual_Thomas-Barton-1709.zip` (613 MB) from
  https://doi.org/10.7488/ds/8088 (https://datashare.ed.ac.uk/handle/10283/9168), unpacked to
  `raw/` (its `__MACOSX` removed); `readme.txt` and `license.txt` beside it
- Instrument: a single-manual English harpsichord by Thomas Barton, 1709 (St Cecilia's Hall), 54
  keys G1/B1–D6 (a short octave; C♯1 and D♯1 split), 2 × 8′; recorded December 2024 by Martin
  Parker, strings played by Callum Campbell, Schoeps MK4 on a Sound Devices MixPre6, 48 kHz / 24 bit
- License: **CC BY 4.0** (the item's rights field). Export credited in
  `instruments/harpsichord_f/LICENSE.md`
- Content: each choir alone (`RightString_Long_articulation/`, `Left_string_long_articulation/`:
  held notes cut at 6.0 s; `…_short_articulation_take_0n/`: staccato, the damper's landing in each
  file, two takes each) and both together (`BothStrings_long_articulation/`, 8.8–18.3 s, its files
  named an octave low; `BothStrings_short_articulation_take_0n/`, four takes of 40 keys). Both
  single-choir folders name their files `…right_hand_string…`; their takes differ. The names right at
  A = 415 Hz; the left choir's D6 does not read as D6 (left to the analysis)
- Notes from inspection: noise reduction as the Baillon's, cut lower (the fundamentals kept; the
  floor falls below ~50 Hz). Projects `harpsichord_f_right`, `harpsichord_f_left` (Harpsichord F: the 8′ and the
  8′ II); the short takes (real release noise) and the both-strings takes (the two choirs' beating,
  the longer bass decays) unused so far

## Headroom Piano (SFZ)

- Location: `headroom-piano/` (`git clone https://github.com/sfzinstruments/BengtNilsson.HeadroomPiano`,
  284 MB)
- Instrument: a Yamaha C3 grand, recorded by Bengt Nilsson (originally for Kontakt; SFZ by kinwie)
- License: **CC BY 4.0** (`LICENSE` in the repository); the author asks to keep the name
  "Headroom Piano". Export credited in `instruments/grand_c/LICENSE.md`
- Content: every third key A0–C8 (30 notes, file names `HEADROOM PIANO LEVEL<1–5> <CLOSE|DECCA>
  <midi>.flac`), 5 layers (SFZ velocities 1–59, 60–89, 90–105, 106–119, 120–127), close and
  Decca tree microphones, one take, no pedal-down or release samples; 44.1 kHz FLAC, tails to
  the floor (up to 34 s), no fades
- Notes from inspection: levels as recorded (one gain: the noise floor alike in every layer,
  layer 1 → 5 about +20 dB at the attack); a noisy bass (floor about −65 dB); from A5 up a
  quieter session below 500 Hz (15–20 dB less rumble, the same floor above)
- Profile `grand_c` (Grand C): close, layers 1/2/5 as pp/mf/ff (`datasets.headroom_files`)

## Osiris Piano (SFZ)

- Location: `osiris-piano/` (`git clone https://github.com/sfzinstruments/Osiris_Piano`, 908 MB)
- Instrument: a worn Yamaha C2 grand at half stick, soft pedal down, played softly (Versilian
  Studios and Karoryfer Samples, for the KVRDC 2026; `OsirisPiano_Manual.pdf`)
- License: **CC0 1.0** (`LICENSE`)
- Content: the white keys (52), 2 layers (velocity to 63, from 64; B3 lacks the soft one), no
  round robins, pedal up (`UC`) and down (`UC-Sus`), each noise-reduced and as recorded
  (`-noisy`), 3 microphones (A inside the piano, B at its rim, C outside: gated to digital
  silence); 48 kHz FLAC. File names count octaves one lower than the keys (`C3` is middle C;
  map by the SFZ `Programs/modules/mappings-noisy/`). The dampers end at F#6
- Notes from inspection: every file fades out over its last 2–2.5 s, and from the middle up
  the files end while the note still rings (C7: 1.4 s); one gain for both layers (their
  floors alike), the layers only ~5 dB apart
- Profile `grand_g` (Grand G): microphone A, unfiltered, pedal up; the two layers as mf/ff
  (`datasets.osiris_files`); the felt is the Headroom C3's (`datasets.FELT_REFERENCE`: the
  two soft layers could not fix it)

## City Piano

- Location: `city-piano/` (http://www.mediafire.com/download/4vmjle2acsy1xd2/City_Piano_Close_Samples.zip)
- Instrument: a baby grand (Big Cat Instruments, 2014)
- License: "It is free and public domain. Do anything you like with it." (the author's page,
  https://bigcatinstruments.blogspot.com/2015/09/all-keyboard-instruments.html)
- Content: every note, 4 velocity ranges (`<note>-<lovel>-<hivel>.wav`), 88 releases (`<note>.wav`);
  44.1 kHz / 16 bit
- Notes from inspection: every file **normalized** to −5 dBFS (no level per velocity or key); files
  **cut** while still sounding (20 s bass, 10–15 s mid, 4 s treble), so the tail is no noise
  floor estimate; the files start at the strike (no lead-in)
- Measured (2026-10-31, design log step 208): **mono** in effect (the channels' difference 43–44 dB
  under their sum; C♯1 ff −14 dB); the noise above 4 kHz in each file's last second 77–82 dB under
  its peak, alike in every layer; the fundamental's decay measurable over a median 24–28 dB in the
  bass, 53–57 dB in the middle, 64–66 dB from F4 to E6 (Grand B's mf 26/58/80, Grand C's loudest
  30/63/81); tuned at A = 440 Hz with a stretch (−22 cents at A0, +10 … +15 from F6), ±4 cents rms
  off a smooth curve; no mislabeled note. The layers differ in timbre (the attack's 2–4 kHz re
  200–500 Hz in F2–E4: −43, −31, −25, −20 dB from the softest up), so the strikes are real and
  only the levels were flattened. The releases (0.3–0.8 s, normalized as well) are key-up noises
  with no ringing string before them: no damper's fall to read, and no level re the notes
- Profile `grand_h` (Grand H): layers 1–48, 49–96 and 111–127 as pp, mf and ff (97–110 unused); its
  levels from Grand B's recordings (`[uses] levels`), the dampers Grand E's, the release and pedal
  noise Grand A's

## OLPC sound library: Zenph Studios' grand piano multisamples

- Location: `olpc-grand/`, 1212 FLAC files (125 MB): `pno<key>v<velocity><leg|sta>.flac` and one
  `pno096v115leg-click.flac` (unused). The original host (freepats.zenvoid.org's
  `olpc-sounds/yamahaGrandPiano44/`) is gone; the files are in the Wayback Machine's copy of it, its
  snapshot of 2016-09-10, under
  https://web.archive.org/web/20160910064755if_/http://freepats.zenvoid.org/olpc-sounds/yamahaGrandPiano44/<file>
  (the file names in `olpc-grand-files.txt`). The whole OLPC library, version 2.7,
  is at https://archive.org/details/olpc-sound-samples-v2.7z (4.6 GB, 7z; whether it holds this
  set was not checked)
- Instrument: "The Zenph Studios Yamaha Disklavier Pro Piano Multisamples for OLPC": a reproducing
  grand piano played by computer, recorded for the One Laptop per Child project's sound library by
  Zenph Studios (Raleigh, North Carolina), 2008
- License: **CC BY 3.0**: the library's announcement (https://creativecommons.org/2008/03/27/85-gb-of-cc-licensed-samples-for-the-olpc/,
  "released under a CC BY license", linking CC BY 3.0) and the archive.org item's license; the OLPC
  wiki's page for the library (wiki.laptop.org/go/Sound_samples, offline) listed this set as CC BY. Export
  credited in `instruments/grand_i/LICENSE.md`
- Content: every third key from A0 (30 keys), mono, 44.1 kHz / 16 bit, every file intact (`flac -t`).
  Up to E♭6 each key at 16 strengths, each three times: a held note (to the floor: 15–23 s in the
  bass, 10–15 s in the middle), a note released at about 7 s and a staccato note released after
  0.2–0.4 s (the held and the 7 s take of a strength peak within 0.8 dB of each other); from F♯6 up
  12–16 held notes and at most four staccato ones. The names give the MIDI velocity played, which differs
  from key to key. Four files repeat another's audio (the same MD5): `pno021v36leg` is `v34leg`,
  `pno048v4leg` `v2leg`, `pno078v7leg` `v5leg`, and `pno021v8leg` is C1's `pno024v6leg` (its pitch
  C1's); none of them is mapped
- Notes from inspection: levels as recorded (v1 to v127 about +35 dB at the peak; every file at
  one noise floor, about −92 dBFS, a gate to near silence under it); mono and 16 bit, hence the
  size; tuned about 6 cents over A = 440 Hz, A7 +26 and C8 about +50 cents (the analysis finds no
  partials on C8, nor on A7 at pp). The keys' levels step 3–5 dB
  from one sampled key to the next (F♯3, C4 and C5 low), the same at every velocity
- Project `grand_i` (Grand I): per key the held note nearest MIDI 14, 60 and 124 as pp, mf, ff; the
  7 s and staccato takes unused so far (the dampers' fall after a release is in them)

## K18 Upright Piano (tried, not used)

- Source: https://archive.org/details/K18UprightPiano (940 MB), by Rudi Fiasco, **CC BY 4.0**
  (`readme.txt`; the SFZ's header links CC BY 3.0); not kept in `data/`
- Content: an upright (no maker given), 2× Samson C02 in AB ~30 cm from the strings; every third
  key from A0 (30 notes), 6 layers (`K18/<note>.<pp|p|mf|f|ff|sf>.wav`), a release sample per key
  (`<note>.rel.wav`: SFZ regions with no `pitch_keycenter`), pedal noises; 48 kHz / 24 bit
- Notes from inspection: **one strike per key**: the layers are the sf file low-passed (the same
  filter on every key: 0 dB below 200 Hz; −20 dB at ~800 Hz for pp, ~6 kHz for ff; ff and sf
  correlate at 1.000, one sample apart), so they hold no dynamics; noise-reduced (the tails at
  −100 to −120 dBFS); the treble dies fast (C6 −45 dB in 1 s). Fine as an SFZ, which plays the
  recording back
- Tried 2026-10-15 as Upright D: the sf alone as ff, the felt and hammer speeds borrowed from
  Upright B. It sounded poor by ear, and the decay fit was the pianos' worst (12.1 dB rms, the
  others 5.9–8.8); dropped. It needed a felt fit for one recorded dynamic (the wizard writes such
  projects, `fit_profile_felt.py` fails on them) and showed the coupling fit's pitch pull
  (`IDEAS.md` 27)

## Ethan Winer's SoundFonts (SFZ conversion)

- Location: `ethanwiner-soundfonts/`, a clone of https://github.com/sfzinstruments/EthanWiner.Soundfonts
  (commit 173e3f8, 17 MB): the folder `Ethan Winer Collection/` with 15 SFZ files, their regions
  in `Data/*.txt` (`#include`, the samples' extension a `#define $EXT`) and `Samples/`
- Instruments: Ethan Winer's own sample sets (https://ethanwiner.com/ewsf2.html, there as SF2):
  African slit drum, bassoon, cello, claves, glockenspiel, shakers, sleigh bells, slide whistle,
  brushed snare, tambourine, temple blocks, triangle, tubular bells, turtle drum, wind chimes;
  converted to SFZ with FLAC samples by kinwie ("some fixes and modifications")
- License: Ethan Winer's page: "These files are made available for you to use in any way you'd
  like, royalty free, including for commercial projects, with no attribution required." The
  conversion's repository has no license of its own and points to that page
- Content (the glockenspiel): 10 samples `EGlock <name>.flac`, every second or third bar, one
  layer, stereo, 44.1 kHz / 16 bit; "recorded in a fairly live room", the release "simulates
  damping the bars with your hand". The names are the written pitch: the SFZ puts them two
  octaves higher (G2 on key 79), where they sound
- Project `glockenspiel` (Glockenspiel A): the glockenspiel's SFZ file, made with the editor's
  wizard
