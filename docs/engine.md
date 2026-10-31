# The engine and its tools

`engine/` runs an exported instrument in real time. The plugin plays it; these tools play it
offline. How it works inside: [architecture.md](architecture.md).

```sh
just engine                                   # cargo build --release
python3 proto/demo.py --profile grand_a    # a reference render and its event list
engine/target/release/render instruments/grand_a \
    projects/grand_a/cache/grand_a/demo_events.json proto/out/demo_salamander_engine.wav [rate]
engine/target/release/bench instruments/grand_a 64       # the load per 64-frame buffer
just midi2wav instruments/grand_f song.mid song.wav          # a MIDI file into a WAV
```

The plugin's "Copy CLI" (in its header menu) puts a `pianoengine render` line for its current
settings on the clipboard.

## midi2wav

`midi2wav` plays a Standard MIDI file through the engine. The standalone does the same as
`pianoengine render` with the same arguments (`pianoengine render --help`): one code
(`engine/src/midi2wav/`, through the C interface's `pe_cli`), the same file byte for byte, so a
release needs no separate tool. `midi2wav -h` lists its options,
`midi2wav --help` describes each and every key of `--param` and `--tuning`. Every value is checked
before anything renders; an unknown option or key gets the nearest known one as a suggestion. It reads:

- notes, their release velocity included (how fast the damper falls, how loud the release noise);
- CC64, half-pedaling included; sostenuto CC66; the soft pedal CC67;
- CC120 (silence) and CC123;
- the mod wheel CC1 and aftertouch (the clavichord's Bebung; `--param bebung=30`, its depth in
  cents);
- the pitch wheel (the đàn tranh's bend; `--param bend=2`, its range in semitones);
- tempo changes.

| Option | What |
|---|---|
| `--rate R` | the sample rate |
| `--tail S` | seconds rendered after the last event |
| `--velocity-curve 1:1,64:80,127:127` | the Touch tab's curve, as in:out points |
| `--latch` | the sustain pedal down throughout, as the plugin's Sustain latch |
| `--felt TEXT` | the Felt tab's felt, as the plugin saves it (`s:21=6,60=0;p:;c:;d:`). On a hammer piano it and `--param hardness` go through the build, every key's loudness kept (a few seconds) |
| `--param room=0.5,pedal=1,pedal_ring=1,damper_fall=2,dynamics=45,…` | live parameters (`dynamics`: the pianos' level range in dB, 10–70). `prep`: 0 none, 1 plucked, 2 muted, 3 harmonic, 4 bolt, 5 paper; with `pluck_pos`, `pluck_bright`, `mute`, `harmonic`, `paper` |
| `--tuning a4=442,stretch=1,mic=1,temperament=3,key=2,…` | compiled settings. `temperament` is the plugin's menu index (0 equal), `key` its key (0 C … 11 B); `stretch` is a tuner's stretch on top of the recording's own (1 adds it, −1 removes it, 0 as recorded); the bolt is `bolt=0.5,bolt_pos=0.31`; `lid` 0 closed … 1 open, 2 taken off; `pillow` 0 … 1 |
| `--scl file.scl [--key N]` | a Scala scale (any note count) as the temperament, its first degree on pitch class N; a key map beside it (`file.kbm`) lays it on the keys |
| `--mic X,Y,Z[,PATTERN[,YAW[,PITCH]]]` | twice: left, then right; a free microphone pair (see [plugin.md](plugin.md#microphones)). It aims at the board unless YAW is given, and overrides `--tuning mic=…` |
| `--wall Y` | the upright's wall, for `--mic` setups |
| `--morph DIR=W` | repeatable: an instrument between instruments (below) |
| `--bits 16\|24\|32` | the output's sample format |
| `--normalize [dBFS]` | one gain for the whole file, to a peak of −1 dBFS or the level given. Without it the engine's calibration stands |
| `--jobs N`, `--warmup S`, `--maxwarm S` | the parallel render (below) |
| `--stems` | the parts as their own files too (below) |
| `--video OUT [--size WxH] [--fps N] [--ffmpeg ARGS]` | a video of the piece with its sound (below) |

`midi2wav` starts where the fits do: the Recording position, spread keys off, no wear. The plugin
starts at Player with spread keys on, its Condition at Normal (10 %: `voicing=0.05`); Copy CLI
writes what it plays (`mic=1`, `spread=1` on the bars, the Condition as its wear keys). The wear
keys are the Condition tab's aspects, 0 … 1: `hammer`, `voicing`, `damper`, `action` live
(`--param`), `unisons`, `tuning`, `strings`, `board`, `falsebeats` compiled (`--tuning`, `seed` the
draw). They act by kind as in the plugin: no soundboard or strings on bars, a third of the board's
on plucked strings ([plugin.md](plugin.md#condition)).

The engine is sequential, so by default `midi2wav` cuts the piece into one time segment per core.
Each segment is rendered by its own engine, which first replays the music before its segment,
from the last moment all dampers were down: at least 12 s (`--warmup`), at most 40 s
(`--maxwarm`). A 4½-minute piece takes 5 s instead of 20 s. It differs from the one-go render
(`--jobs 1`) only in the sympathetic resonance's wash, about 45 dB below the music, and there is
no seam at the cuts.

`--stems` writes the parts beside the output, each a file of its own (`song.strings.wav`, …):

- `strings`: the struck strings as the board radiates them directly (the hammer's thump on the board
  with them);
- `ringing`: the strings ringing along (the sympathetic resonance) and the rattle, the same way;
- `room`: the room of both;
- `noises`: the knock, the release and the pedal's noises.

They sum to the output to rounding (−131 dB re its peak), with its gain (`--normalize` sets it from
the output). The engine is linear from the bridge on and nothing heard drives the strings back, so
each part is a render with the others muted where they join the mix (`Mix`); each costs a render.

`--video OUT` also writes a video of the piece, with the output as its sound (needs `ffmpeg`): the
plugin's Visualization panel, frame by frame. Each key's strings from above, a sounding string a
glowing band as wide as its partials move it (exaggerated; the strings ringing along drawn larger),
a fresh snapshot of the string inside it every frame; the dampers on the strings; a piano's hammers
from the front; the keys, held ones lit; under them the soft, sostenuto and sustain pedals as bars
filled to their depth (where the instrument has dampers). A bar instrument shows its bars. The score is known ahead, so each hammer rises before its note at its own
speed (the plugin's cannot). Its state comes from a render of its own (one engine through the whole
piece, the audio's state to the sample); the frames are drawn on every core and piped into ffmpeg.

- `--size WxH`: default 1920x1080, even sides; the picture scales with the height.
- `--fps N`: default 60.
- `--transparent`: no background, for compositing. The strings' light keeps an alpha as bright as
  it is (it adds over what lies beneath), the keys, hammers and dampers are solid. The default
  options become ProRes 4444 with its alpha (`-c:v prores_ks -profile:v 4444 -pix_fmt yuva444p10le
  -c:a pcm_s16le`, a `.mov`, read by Resolve, Premiere, Final Cut, Kdenlive); VP9 keeps it too, in a
  `.webm` browsers play: `--ffmpeg "-c:v libvpx-vp9 -pix_fmt yuva420p -crf 32 -b:v 0 -c:a libopus"`.
- `--ffmpeg ARGS`: ffmpeg's output options, split at spaces, in place of the default
  `-c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p -c:a aac -b:a 256k -movflags +faststart`.
  AV1 with Opus, into a `.webm`: `--ffmpeg "-c:v libsvtav1 -crf 30 -preset 6 -pix_fmt yuv420p -c:a libopus -b:a 192k"`.

A 30 s piece at 1080p60 takes 18 s with x264 and 42 s with SVT-AV1 (preset 6) on 16 threads; the
drawing alone runs at about 190 frames a second.

## Morphing

`--morph DIR=W` (repeatable; `midi2wav` and `render`) plays an instrument between instruments,
made from their files without fitting. Each `--morph` mixes in the instrument in DIR at weight
W, and the instrument given first keeps the rest (1 − ΣW):

```sh
just midi2wav instruments/grand_a song.mid song.wav --morph instruments/grand_d=0.5
```

This one is halfway between Grand A and Grand D. A morph takes the hammer-strung pianos, the
grands and uprights in any mix, and refuses others. What it blends:

- per key, the strings' and hammers' numbers are interpolated (scales geometrically);
- the soundboards' modes are paired by relative rank;
- the measured responses are mixed;
- the noises and the room are interpolated.

The morph plays at its instruments' mean level at ff. Each instrument's ff level is measured and
stored in its `level.json`, which the export writes with the engine's `level` tool. A piano's
`level.json` also holds its level over the hammer speed on every key (`speed_db`), through which
its velocity plays (the Dynamics); a morph blends its pianos' tables. `engine/tests/level_files.rs`
fails when the engine plays one 0.1 dB off (a table 0.2 dB); this writes it again (about 4 s for
a piano):

```sh
cargo run --release --manifest-path engine/Cargo.toml --bin level -- instruments/<dir>
```

A morph takes about 0.25 s to build; a plain load takes 0.13 s.

## Accuracy and cost

- **Against the prototype**: the engine reproduces the prototype's render to about −70 dB; string
  modes below −80 dB are not started. It deliberately differs in two ways:
  - one voice per key: a re-strike acts on the still-vibrating string, and the damper lifts for
    its remainder too (with `--param restrike=1`, Live re-strike, the felt meets the strings where
    they swing);
  - no fixed render length for undamped notes.
- **Sample rates**: any rate works. The exported buffers are resampled at load, and the hammer
  contact keeps the reference rate's behavior.
- **Latency**: 64 samples.
- **Load**: about 12 % of one laptop core for a heavily pedaled ff passage (88 voices) at the
  Recording position, about 17 % at Player (`bench`, 256-frame buffers).
- **Output**: calibrated to full scale (the median ff single note peaks at −15 dBFS), then each
  instrument's output gain (`output_gain_db`) puts a phrase in its middle register at the grands'
  loudness. The prototype's renders are in model units.

## Environment variables

| Variable | For | What |
|---|---|---|
| `RENDER_STOPS`, `BENCH_STOPS` | `render`, `bench` | the registers' bits (7: all three of Harpsichord B's) |
| `RENDER_PARAMS` | `render` | live parameters, as `--param` (`lute=1`, `buff=1`) |
| `RENDER_TUNING`, `RENDER_SWITCH` | `render` | a tuning (as `--tuning`); `t:spec;…`: at t seconds a new engine for that spec takes over |
| `RENDER_PREROLL`, `RENDER_BLOCK` | `render` | silence before t = 0, frames per block (tests) |
| `BENCH_MIC`, `BENCH_STRIKE`, `BENCH_REPEAT`, `BENCH_TOP`, `BENCH_TELEMETRY` | `bench` | the microphone position, the strike cost per key, repeats, the slowest buffers, a diagnostics run |
| `BENCH_ONLY`, `BENCH_SECONDS`, `BENCH_DRAG` | `bench` | the scenarios whose name holds the text, each cut to S seconds (a profiler's run), the microphones dragged as the plugin does it |
| `PIANOENGINE_INSTRUMENTS` | the plugin | an extra instrument search path, searched first |
| `PIANOENGINE_CACHE` | `roll`, the editor's rolls | where the reference measurements are kept |
| `PIANOENGINE_NO_DIRECTIVITY` | the engine | the board's directivity off (checks) |
