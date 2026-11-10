# Brass

Three short original pieces for a brass sextet (two trumpets, horn, tenor trombone, bass trombone,
tuba), written here to hear the brass family through its settings, as no freely licensed brass
arrangements turned up. The MIDI files and the scripts are dedicated to the public domain
([CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)), and so are renders of them.

```sh
python3 albums/brass/compose.py      # writes midi/NN-name/<stem>.mid and midi/NN-name.mid (the same files each run)
python3 albums/brass/mix.py          # renders the stems (out/NN-name/), mixes them (out/NN-name.wav)
python3 albums/brass/notes.py        # the notes that sound off their written pitch, settled (250 ms in)
python3 albums/brass/mix.py --variant attack   # the same with the instruments' variant exports <instrument>@attack
                                               # (proto/export_brass.py --variant): out/NN-name@attack.wav
```

| # | Piece | Style | Length | The settings |
|---|---|---|---|---|
| 1 | Harbor Parade | march in B-flat, its trio in E-flat, 116 bpm | 2:44 | a fanfare in octaves; the first strain's melody in the trumpets over afterbeats and a two-beat bass; the second strain's in the trombones in octaves, ff; the trio's in the horn, slurred, the trumpets in straight mutes; a breakstrain of trombone smears on the glide (CC65), the band's answers, a B-flat 7 swelled on CC1 with the trumpets flutter-tonguing (CC23); the trio again ff, its last chord swelled with vibrato (CC70), a stinger |
| 2 | Evening Chorale | hymn in E-flat, 60 bpm | 1:57 | four voices slurred; verse 1 open, mf, the trumpet's vibrato coming late on its long notes; verse 2 p, the tune in the horn, the trumpets in cup mutes, the bass trombone in its bucket; verse 3 f, the tune's dynamics on CC11 (air by hand), a ritardando; an Amen, pp |
| 3 | Back Alley Strut | 12-bar blues in F, swung, 126 bpm | 1:46 | choruses 1 and 2: the head in the first trumpet with the plunger opened on each note (CC4), the second in the wah mute with its stem, the trombone's tailgate smears on the glide, a two-beat tuba; chorus 3: the trombone's solo in the bucket mute, the trumpets in straight mutes behind it; chorus 4: a riff in every part, the trombones growling (CC21), the second trumpet in the wah mute, the tuba walking; two hits and a last chord, the trumpet's flutter growing on it, the trombones falling off it on the glide |

- A stem is one part in one mute (`trumpet1_straight`, `bass_trombone_bucket`): the mute is a setting
  of the render, everything else is in the MIDI, as the plugin's default ties play it (CC1 air added
  over the keys', CC11 air by hand, CC70 vibrato, CC21 growl, CC23 flutter, CC4 the plunger, CC65 the glide
  held). Overlapping keys slur (40 ms), the others are tongued.
- Each piece is also one file with a track and channel per stem (`midi/NN-name.mid`), for a DAW.
- `mix.py` renders every stem at Audience (Weidinger's default), the trumpets left, the trombones
  right, the horn and tuba near the middle, the lead trumpet 1–2 dB over the others; the mix's peak at
  −1 dBFS.
- Every file starts with an empty bar in the piece's tempo. The playing is humanized from a fixed seed
  (each note a few ms off the grid, its velocity ±3).
- The lines stay within each instrument's fitted keys and mostly off the keys whose loud slurs still
  miss (`docs/brass.md`).
