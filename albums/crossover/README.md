# Crossover

A piece for Dussek's and Weidinger's instruments together, written here and free of any license worry:
the MIDI files and the scripts are dedicated to the public domain
([CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)), and so are renders of them. Not on the
website: a demo of the two plugins in one band.

```sh
python3 albums/crossover/compose.py   # writes midi/01-the-crossing/<stem>.mid and midi/01-the-crossing.mid
python3 albums/crossover/mix.py       # renders the stems (out/01-the-crossing/), mixes them (out/01-the-crossing.wav)
python3 albums/crossover/mix.py --mix-only
```

| # | Piece | Style | Length | The players |
|---|---|---|---|---|
| 1 | The Crossing | in E-flat, 88 bpm: an intro, the theme twice, a groove in C minor, a brass chorale, a build, the theme in full, a coda | 3:33 | Dussek: piano (Grand C), harp, vibraphone, marimba, bells, tubular bells; Weidinger: two trumpets (one also in a cup mute), horn, tenor and bass trombone, tuba |

- The sections: the piano alone, the harp and vibraphone joining, the horn's call far off (bars 1–8); the theme
  on the piano over harp and held trombones, then on the trumpet with the horn a third below and the bells
  doubling its long notes (9–24); a groove in C minor, the marimba's sixteenths and the tuba's bass under the
  trumpet's riffs and the cup-muted answers, then the piano's solo over it (25–40); the brass sextet's chorale
  alone, the bells between its chords (41–48); a build, everyone joining, the trombone sliding down into bar 55,
  the trumpet's flutter, a harp glissando into the top (49–56); the theme in full, trumpets in thirds, the
  horn and trombones in chords, the growl at its height, the piano in octaves, tubular bells on the downbeats
  (57–64); a quiet coda, the horn's call again, the last chord with everyone (65–74).
- The keyboard parts use the demos' notation and pedals (`albums/demos/compose.py`), the brass the brass
  album's (`albums/brass/compose.py`): a stem is one part in one mute, its controllers as the plugin's default
  MIDI ties play them. Every part stays in its instrument's recorded keys.
- `mix.py` renders Dussek's stems at the player's position and Weidinger's at Audience, as their own albums do.
