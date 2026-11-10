# Weidinger: the brass

Weidinger is Dussek's second plugin, for breath-driven lines: brass first, woodwinds if they ever come. It is built from the same
sources as Dussek (the engine, the microphones and the room, MIDI learn, presets, the window), in the same four forms: LV2, CLAP,
VST3 and a JACK standalone. **It is a preview**, in the same release files as Dussek: the Linux archive installs both, the
Windows installer and the macOS package offer it as a component that can be left out (its standalone, the plugin formats chosen
for Dussek, and the brass instruments). From source, `just plugin` builds both, `just install` links both, and
`just standalone --weidinger` runs its standalone under PipeWire. Its parameters, presets
(`~/.local/share/dussek/weidinger-presets/`, `.weidinger-preset` files) and MIDI map are its own; it lists the brass instruments
only, and Dussek none of them. A fresh instance and the Default preset play the trumpet. The rest of the window works as
[Dussek's](plugin.md), except the bottom panels: the Analysis panel shows the Spectrogram alone (the players are no voices
of strings: no voice count, no Partials), and there is no Visualization.

## The instruments

Five blown instruments, marked experimental: their lips, bore and bell are modeled, and a virtual player makes from the keys
alone what a keyboard cannot send. No brass player has judged them yet. They play as loud as Dussek's grands: a phrase in each
one's middle register lies at the grands' loudness.

**Playing.** Velocity and overlap: overlapping keys slur, lifting between them tongues. A line is one player, a chord a section of
up to six, each with its seat in the stereo image; a key left down under a slur keeps its note (**Solo** on the Player tab: one
player, the last key wins). Keys outside the range stay quiet and are gray on the strip, a pressed one lights up; amber keys play
under what was fitted, where a player's lips loosen. None play over the highest fitted note: there the lips are at their limit. The sustain, soft and sostenuto pedals do nothing and have no levers here.

**Slurs.** A slur lands on its note, soft or loud, within 20 c on most keys. A few keys still miss on a loud slur from above and
pull in after it:
- the tuba's lowest keys (open E1 to G1, up to 360 c; straight F1 to C2, up to 60 c) and its open G3 (about 150 c flat);
- some of the bass trombone's low keys (20–60 c).

Tongued notes and slurs from below land on all of them.

| Tab | Controls |
|---|---|
| Sound | **Mute**, a button per mute the instrument has, from the next tongued note (kept by kind: a wah stays a wah on the next instrument, one it lacks plays open); **Plunger** and **Plunger by hand** with the plunger in; **Growl**, **Flutter**, **Vibrato** (above 0 it replaces the player's); Room |
| Player | **Virtual player** (off, for wind controllers and DAWs: the air from velocity, nothing else); **Solo**; **Glide**, **Glide held**, **Glide time** on the trombones; **Air** and **Air by hand**; **Air added** (on top of the keys' or the Air's, up to the top at 100 %); **Aftertouch to** growl or flutter |
| Touch | the velocity curve; **Dynamics** (less brings the soft notes up, with more air, so also brighter; more takes them down); **Player's variation**, hidden with the player off |
| Mics | Distance level; the custom pair |

**Controllers.** Ties of MIDI learn, made out of the box on channel 1 (Weidinger's own map,
`~/.config/dussek/weidinger-midi-map.txt`, apart from Dussek's); each shows its tag on its control and is learned again or forgotten
like any other:

| CC | Plays |
|---|---|
| 1 (the mod wheel) | Air added |
| 11 | Air (a breath controller: learn Air on CC2) |
| 70 | Vibrato |
| 21, 23 | Growl, Flutter |
| 4 | Plunger |
| 65 | Glide held |
| 5 | Glide time |

Air and Plunger moved from MIDI switch on their "by hand" switch and take over from the keys; until then soft takeover does not
hold them back, since their slider is not heard. The switch gives them back.

| aftertouch | growl or flutter (Aftertouch to), built in |
| the pitch wheel | ±2 semitones, built in |

**Microphones.** The bell has the positions of the other instruments, in its own places: Audience (the default, 6 m out,
centered on the bell, at the player's height), Player at the player's ears, Close 1 30 cm on the axis, Close 2 30 cm well off it, a custom pair, and Recording, the
fitted sound unchanged, 1.5 m on the axis as Iowa's microphone stood. A position sets the beam by angle, the distance's level and
the room's balance; Lid and Pillow do not apply. Of the distance's delay only the pair's difference is kept, so nothing plays late. Audience is 3 to 4 dB
darker from 4 kHz up than Recording. The spacing is the pair's, as on every instrument; at Player it stops at the ears' 18 cm and at
Close 1 and 2 at 10 cm, and the value reads "(limited)" there. From above, the other positions' markers wait at the view's edge when
they are farther than 5 m (Audience).

Each bell points as it does when played (the export's `pose`): the trumpet and the trombones forward and a little down,
the horn back and to the player's right, the tuba up and to the right. Player and Audience stay where the player is,
Close 1 and Close 2 turn with the bell, so in front the horn and the tuba are darker and lean a little to the right, as
they do in a hall. For production, Recording gives the close, centered sound: the direct sound alike in both channels, the
bell's direction not applied (Room at 0 leaves it dry, to pan and give a room in the mix).

With the microphones, Room, Volume, Dynamics, the variation and the tuning, these are Weidinger's parameters; Dussek's other
controls (hammers, pedals, lid, condition, …) are not there.
