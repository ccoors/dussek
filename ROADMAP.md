# Roadmap

What is open, by horizon. Each item says what is left; its background (measurements, options,
what was tried) is in [docs/roadmap-notes.md](docs/roadmap-notes.md). What is done is in the
[design log](docs/log/README.md).

## Now

Nothing in progress; the next item comes from below.

## Next

- **The website.** GitHub Pages: a demo per menu group playable on the page, the ensemble piece's
  video, the original piano pieces, a few real performances, short clips of the features,
  screenshots, the instruments' credits; rendered by a script, so a sound change renews them.
- **Traits, the rest.** The plan (design 2026-10-21-traits-plan): a piano as smooth curves with
  its few breaks plus traits (knobs), its unevenness as statistics with a seed, the tuner's and
  voicer's choices on top. Done: the rail (step 136), the even levels (138–139), the felt as runtime
  curves (145), the inventory (148: the board's Bass), the Condition by kind (191–194: what bars
  and plucked strings can wear), its zones at 10/35/100 % (195; one Condition for all the pianos,
  good enough by ear). Left: the strings' and hammers' traits when something is heard; the product
  layer. [Notes](docs/roadmap-notes.md#listening-round)
- **Repeated strikes.** Each strike varies (step 130); the hammer meets a still-moving string with
  Live re-strike (step 170: a switch, off by default, heard as getting annoying when played), the
  bars' mallets at the relative speed (step 171). Left: a re-pluck for the plucked strings (the
  plucker takes the old motion at its point, then releases: mostly the guitars' tremolo).
- **Morphing beyond the pianos.** The other groups (harpsichords, plucked strings, percussion),
  then across families. [Notes](docs/roadmap-notes.md#morphing)
- **Presets, the rest.** Microphone presets per body type and full presets, shipped and saved
  (step 175), the first two shipped full presets, also in LV2 hosts' preset menus (step 179). Left: shipped
  mic setups for the other bodies; snapshots and a morph between two settings.
  [Notes](docs/roadmap-notes.md#presets)
- **Each instrument checked by ear.** The take checks' flags heard instrument by instrument, the
  level corrections proposed and heard, then each instrument rebuilt.
  [Notes](docs/roadmap-notes.md#listening-passes)
- **The coupling fit's pitch pull.** A pitch term in the fit's error, tried on a clean piano
  (Grand C) and judged by ear, pitch against decay, before the pianos are refitted. Also a felt
  fit for a piano with one recorded dynamic. [Notes](docs/roadmap-notes.md#coupling-pitch)

## Later

- **The user guide.** The [quick start](docs/quick-start.md) covers installing and the controls
  that matter; how to play the instruments well, beyond the reference pages, waits while things
  change quickly. It grows from the quick start. [Notes](docs/roadmap-notes.md#user-guide)
- **The fitting pipeline in Rust.** The fits render through the engine instead of the
  prototype, one step at a time, each against the Python step's outputs on the same inputs.
  The slowest steps come first: coupling, then fine structure and the demo, then felt.
  [Notes](docs/roadmap-notes.md#rust-port)
- **Visualizations of one note.** The live panel (step 165) and `midi2wav --video` (step 166) are
  done; the spike's other views (the strike in slow motion, the unison's strings and their modes,
  the partials against their harmonics, the sound reaching the microphones; the spike in the archive repository, branch `viz-spike`)
  are not for the plugin: a visualization project of their own or material for the docs.
  [Notes](docs/roadmap-notes.md#visualizations)
- **Rolled pianos, the follow-ups.**
  - Faster rolls: most of a roll is now the Player renders; fitting the board's radiation instead
    of rendering it would remove them.
  - The warm end of Warmth, where rolls sounded unbalanced.
  - The treble at Player above 4 kHz, which the board's radiation barely sets.
  - Rolls in the plugin.
  - Strings from a design (scale, string breaks, hammer grading) where blends prove too limiting.
  - Hammers separate from the strings (they stay together for now).
  [Notes](docs/roadmap-notes.md#rolled-pianos)
- **Microphones.**
  - More than two microphones, each with a level and pan.
  - More reflectors (side and back walls, ceiling) and a room size.
  - A diffuse room that depends on the position.
  - Mid-side, the proximity effect, frequency-dependent patterns.
  - The board's dipole-like radiation at low frequencies.
- **The lid and the pillow.**
  - A frequency-dependent or resonant closed lid.
  - An upright with its front board off.
  - The lid's effect on the room's reflections.
  - A pillow on part of the keyboard or on the strings.
- **The prepared piano.**
  - Preparations per key.
  - Felt between the strings.
  - A mallet.
  - Plucking single strings with a pick.
  [Notes](docs/roadmap-notes.md#prepared-piano): a recorded reference measured, what the
  engine's preparations match and what they lack, a proposal.
- **The instruments' open points.** What each instrument still lacks against its recordings
  (the harp's pedals, the guitar's strings ringing along, the bars' split modes, …), and the
  pianos' measured body modes. [Notes](docs/roadmap-notes.md#instruments)
- **More instruments.** A fortepiano, an electric piano with tines, a celesta: no freely
  licensed set has been found yet. [Notes](docs/roadmap-notes.md#more-instruments)
- **Condition.** A seasonal pattern: humidity moves the soundboard's crown, so the middle goes
  sharp or flat together.
- **Expression.**
  - MPE, or per-note pitch and expression (a per-key bend of a plucked string, aftertouch for a
    bowed sustain).
  - Pedal mechanism noise.
  - Scaling the inharmonicity B on its own.
- **Sharing instruments.** A documented instrument format and a published recipe, so that
  others can make instruments from their own sets. The editor is the start of it; its wizard
  should ask for the recordings' credit (the `[license]` block, step 199), which a new project
  starts without.
- **Cost.** A low-CPU mode (fewer partials, a simpler soundboard) and a polyphony or quality
  setting, for weaker machines and the ARM/Raspberry Pi case (and a phone with its screen off: the
  Android app below).
- **Platforms.** macOS and Windows builds through DPF, and CI running the tests and builds on
  each commit.
- **An Android app.** A MIDI player (and later a live instrument) on a phone. The spike (in the archive repository, branch
  `worktree-agent-ab1ed2ffbf05f2af3`, `android/`: the engine unchanged through a small Rust JNI
  library, AAudio, a plain Java app with the instruments bundled) on a Pixel 9a (Tensor G4, 48 kHz):
  live at 256 frames 27–32 % mean load, p99 43–61 %, no underruns for Grand A, Grand B and Upright A
  (64 frames clean too), but only with an ADPF performance hint (target half the block): without it
  the callback wanders onto the little cores and underruns at every block size up to 1024; offline
  7–9× real time on the big core (about 1.8× slower than one core of the Ryzen 7 7840HS). With the
  screen off the work lands on the little cores: listening away from the screen renders first, then
  plays (or the low-CPU mode above). Next: the hint on by default (Oboe has one), render-then-play,
  USB/BLE MIDI input (`android.media.midi`, lock-free into the engine), stream recovery and latency,
  instruments as downloads instead of in the APK (22 MB), a real UI.
- **Output ports per part.** The plugin's soundboard, room and noise bursts on their own
  outputs (separate buffers already, almost free), the sympathetic ring through a second soundboard
  convolution, the bridge force as a debug tap: soloing parts live while playing (offline:
  `midi2wav --stems`).
- **Further out.**
  - Ambisonic output: an Ambisonic microphone is one more kind, a point whose paths (each knows
    its arrival direction) are weighted by spherical harmonics, with a decorrelated diffuse room
    per channel. First in `midi2wav`: 3rd order (16 channels) written as IAMF through ffmpeg
    (its `iamf` muxer, alone or in the `--video` MP4), checked binaurally in the IAMF web demo
    (aomediacodec.github.io/iamf-tools/web_demo). The player's seat on headphones is where it
    pays; the single diffuse room limits it until more reflectors exist. Later: Ambisonic
    output ports in the plugin (the board's responses cost about 8× at 16 channels).
  - Other physically modeled instruments, e.g. the organ, from freely licensed organ samples
    (GrandOrgue, Hauptwerk sets).

Decided against, for now: a live standalone player of our own (`just standalone` runs the JACK
standalone as an app instead); a repetition limit for uprights; a second harpsichord manual with
a coupler; for MIDI learn, 14-bit CCs, NRPN, MIDI output (motorized faders, LED rings) and the
velocity curve and action buttons as learnable controls.
