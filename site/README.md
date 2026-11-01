# The website

The pages at https://ccoors.github.io/dussek/, built from this folder by scripts, so that a sound
change or a UI change renews them. Nothing rendered is kept in git.

```sh
python3 site/phrases.py          # the phrases (site/midi/*.mid, committed)
python3 site/render.py --check   # the clip table: instruments, keys, controls, files
python3 site/render.py           # every clip into site/out/audio/*.ogg and site/out/clips.json
python3 site/render.py --only grand,pedals
python3 site/render.py --video   # also the ensemble piece's video (site/out/video)
site/screenshots.sh site/out/screenshots   # the standalone on a virtual display (Xvfb, xdotool)
```

- `clips.toml`: every clip, by section (the menu groups' demos, the original pieces, the real
  performances, the features); its comments say each key. A clip names plugin controls by symbol:
  `plugin/build/settings_line` (`just plugin`) turns them into the plugin's own Copy CLI line.
- `phrases.py`: the demos' and the features' phrases (CC0), within each instrument's recorded keys.
- `midi/maestro/`: three performances from the MAESTRO dataset, CC BY-NC-SA 4.0 with their
  citation (`LICENSE.md`); their renders carry the same license.
- `sounds/`: the audio input's sound (CC0, `LICENSE.md`).
- Each clip sits at −18 LUFS (true peak at most −1 dBFS); a feature's steps share one gain, so
  their differences in level stay, unless the clip says `match = "each"`.
