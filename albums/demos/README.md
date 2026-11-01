# Demos

Five short original pieces for demonstrating the engine, written for it here and free of any
license worry: the MIDI files and `compose.py` are dedicated to the public domain
([CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)), and so are renders of them.
The piano pieces show the pedals: legato changes, half-pedaling and half changes on the damper
pedal (CC64), the soft pedal full and part way (CC67), the sostenuto (CC66).

```sh
python3 albums/demos/compose.py      # writes midi/*.mid (the same files each run)
python3 albums/demos/mix.py          # the ensemble: renders its stems, mixes them
python3 albums/demos/mix.py --video  # also a video: the stems' panels on a grid, the mix as the sound
mkdir -p albums/demos/out
for f in albums/demos/midi/0[1-4]-*.mid; do      # the piano pieces
    engine/target/release/midi2wav instruments/grand_b "$f" "albums/demos/out/$(basename "$f" .mid).wav"
done
```

| # | Piece | Style | Length | The pedals |
|---|---|---|---|---|
| 1 | Back Porch Boogie | 12-bar boogie-woogie in C, swung, 138 bpm | 1:34 | chorus 1 dry; chorus 2 quiet on the soft pedal, the melody under a half-pressed damper pedal (CC 58) changed each bar; chorus 3 tremolos in half pedal (CC 64), full-pedal touches on the shouts; stop time with touches; the last chord in full pedal |
| 2 | Streetlights | pop ballad in D-flat, 68 bpm | 2:41 | a legato change at every harmony; the soft pedal full in the intro, half (CC 64) in the first verse, lifted for the choruses, full again in the outro; the big chorus with half changes (up to CC 45 only: the bass keeps ringing) |
| 3 | Glass Rain | minimalist ostinato in A minor, 96 bpm, growing layer by layer to two climaxes | 3:14 | a low A held by the sostenuto under every harmony while the damper pedal, half pressed (CC 64), changes shallowly each bar; the soft pedal lifts in steps (127, 80, 40, 0); the builds and climaxes (broken octaves over a driving bass) in full pedal with full changes; a breakdown back to the drone on una corda; a breath (pedal up, silence) before the second climax; a rolled A major over the keyboard left to ring, the coda's cell in major |
| 4 | Ferry Montuno | son montuno in C, 176 bpm | 1:10 | dry grooves; the coro on the soft pedal with half pedal (CC 56) changed at each harmony; full-pedal touches on the mambo's clave hits; a glissando into the last chord in full pedal |
| 5 | Ensemble Demo 1 | ensemble in D, 112 bpm: piano, vibraphone, marimba, harp, kalimba, bells, tubular bells | 1:51 | the piano in legato changes, an interlude on the soft pedal in half pedal (CC 70, changed shallowly); the vibraphone's and the harp's pedal changed each bar; a kalimba riff on a drone tine, a marimba groove, the melody in the piano's octaves and the bells at the top |

- Every file starts with an empty bar in the piece's tempo (1.7 s for the boogie, 3.5 s for the
  ballad), so nothing starts at once; the lengths include it.
- The playing is humanized from a fixed seed: each note a few ms off the grid, its velocity ±3,
  a chord's top note a little louder. Pedal moves are ramps (a value every ~12 ms), as from a
  continuous pedal, not on/off switches.
- The engine's dampers move over CC 38–89 (pedal depth 0.3–0.7, `LIFT_RANGE` in
  `engine/src/engine.rs`): CC 64 holds them half on the strings.
- The ensemble piece is one file per instrument in `midi/05-ensemble-demo-1/` (each within its
  instrument's recorded range, for `midi2wav` or the plugin one at a time) and all of them in
  `midi/05-ensemble-demo-1.mid`, a track and channel each with its General MIDI program, for a
  DAW. `mix.py` renders the stems (`out/05-ensemble-demo-1/`), sets each one's gain and pan (set
  against the stems' levels in the tutti) and puts the mix's peak at −1 dBFS. Every stem plays at
  the player's position, the bars spread over the 88 keys' frame (`--tuning mic=1,spread=1`).
- `mix.py --video` renders each stem's Visualization panel too and sets them on a grid,
  1920 × 1080: the piano large at the top left (1440 × 810), the other instruments as 480 × 270
  tiles down the right and along the bottom, each labeled, the title in the corner; the mix is
  the sound. About 1.5 min.
- The piano pieces: two tracks, the tempo map, the piano on channel 1 (program 0). Any General
  MIDI piano plays them; one without CC67/CC66 misses only the soft pedal's color and the sostenuto's drone.
