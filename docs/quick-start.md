# Quick start: the plugin

From nothing to playing a grand, and the few controls that change the sound most.
[plugin.md](plugin.md) has every control.

## Install

The [releases](https://github.com/ccoors/dussek/releases) have an installer per platform (the
README's [Download and install](../README.md#download-and-install) says which file and what the
unsigned installers ask). From source (the README's [Build from source](../README.md#build-from-source)
lists what you need):

```sh
git clone --recursive <this repository's URL>    # then, in the clone:
just plugin        # the engine and the plugin, into plugin/build/bin
just install       # links the plugin into ~/.lv2 (and ~/.clap, ~/.vst3), the instruments into ~/.local/share/dussek
```

`just install` makes links, so a later `just plugin` updates the installed plugin.

- **In an LV2 host** (Ardour, Carla, Jalv, …): rescan the plugins and add "Dussek" to an
  instrument track. The host needs LV2's worker feature (these three have it).
- **As a JACK standalone**: `just standalone` starts it under PipeWire (`pw-jack`), links its
  outputs to the default output and gives it an ALSA MIDI port. Connect your keyboard to its MIDI
  input in a patchbay (qpwgraph, Helvum, Carla). Started on its own (`plugin/build/bin/dussek`, under
  PipeWire or a JACK server), it links its outputs to the default output itself.

## Pick an instrument

The instrument menu at the top left lists the instruments by group: grand pianos, upright pianos,
harpsichords, and so on. Grand A is a good start. Switching cuts the sounding notes; the
instruments are leveled, so the volume stays where it was.

## The controls that matter

Resting the pointer on any control says what it does; double-click resets it.

- **Volume** (the header): the output level, off to +24 dB.
- **The pedals**: sustain (CC64) is continuous, so half pedaling works; sostenuto (CC66); soft
  (CC67, the pianos only). The Keyboard strip at the bottom has the three pedals on screen.
  **Sustain latch** (the header) holds the sustain pedal down, for MIDI files without pedaling.
- **The microphones** (the drawing, and the Mics tab): **Player** (the default) hears the
  instrument from the keyboard, close and wide. **Audience** hears it from 3.5 m in front of the
  open lid, with more room. Moving further away at Audience changes little: the room's sound
  stays about the same and only the direct sound's share falls. **Distance level** set to
  "Direct sound" keeps the direct sound's level instead, so the room grows with the distance:
  meant for an external reverb, with **Room** (the Sound tab) off.
- **Condition** (the Sound tab): how new or worn the instrument is. Mint (0 %) plays it as
  measured, voiced even; Normal (up to 10 %, the default) makes the keys a little uneven, as a
  real instrument is; Worn and Broken add hardened hammers, false beats, a drifting tuning,
  leaky dampers and action noise. The Condition tab shifts each of these on its own.
- **Concert pitch** (the Tuning tab): A at 415–466 Hz, 440 Hz by default.
- **Temperament** (the Tuning tab): equal by default; historical ones (meantone, Werckmeister,
  Vallotti, …) make the keys near the **Temperament key** purer and the far ones rougher.
  "Scala file" plays a scale from a `.scl` file.

Two more worth a try: **Hammer hardness** (the Sound tab), softer or brighter felt with every
key's loudness kept; and the **Touch** tab, where the velocity curve fits the instrument to your
keyboard.

## Rendering a MIDI file

The ⋯ menu's **Copy CLI** puts a command line on the clipboard that renders a MIDI file offline
with the current settings: replace `in.mid` and `out.wav` and run it. The standalone renders it
(`dussek render`; the line names the standalone by its path where the plugin finds it beside
itself). The plain form:

```sh
plugin/build/bin/dussek render instruments/grand_a song.mid song.wav
```

Music you make with the instruments is yours, free to use without credit.
