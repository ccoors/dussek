# Weidinger: the brass and the clarinet

Weidinger is Dussek's second plugin, for breath-driven lines: brass first, and one woodwind, the clarinet, the first of the reeds. It is built from the same
sources as Dussek (the engine, the microphones and the room, MIDI learn, presets, the window), in the same four forms: LV2, CLAP,
VST3 and a JACK standalone. **It is a preview**, in the same release files as Dussek: the Linux archive installs both, the
Windows installer and the macOS package offer it as a component that can be left out (its standalone, the plugin formats chosen
for Dussek, and the brass instruments). From source, `just plugin` builds both, `just install` links both, and
`just standalone --weidinger` runs its standalone under PipeWire. Its parameters, presets
(`~/.local/share/dussek/weidinger-presets/`, `.weidinger-preset` files) and MIDI map are its own; it lists the brass instruments
only (the clarinet among them), and Dussek none of them. A fresh instance and the Default preset play the trumpet. The rest of the window works as
[Dussek's](plugin.md), except the bottom panels: the Analysis panel shows the Spectrogram alone (the players are no voices
of strings: no voice count, no Partials), and there is no Visualization.

## The instruments

Six blown instruments, marked experimental: their lips, bore and bell are modeled, and a virtual player makes from the keys
alone what a keyboard cannot send. No brass player has judged them yet, nor a clarinetist the clarinet. They play as loud as Dussek's grands: a phrase in each
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
| Sound | **Mute**, a button per mute the instrument has, from the next tongued note (kept by kind: a wah stays a wah on the next instrument, one it lacks plays open); **Plunger** and **Plunger by hand** with the plunger in; **Growl**, **Flutter** (not on the clarinet), **Vibrato** (above 0 it replaces the player's); Room |
| Player | **Virtual player** (off, for wind controllers and DAWs: the air from velocity, nothing else); **Solo**; **Glide**, **Glide held**, **Glide time** on the trombones; **Air** and **Air by hand**; **Air added** (on top of the keys' or the Air's, up to the top at 100 %); **Aftertouch to** growl or flutter (not on the clarinet) |
| Touch | the velocity curve; **Dynamics** (less brings the soft notes up, with more air, so also brighter; more takes them down); **Player's variation**, hidden with the player off |
| Mics | Distance level; the custom pair |

**Controllers.** Ties of MIDI learn, made out of the box on channel 1 (Weidinger's own map,
`~/.config/dussek/weidinger-midi-map.txt`, apart from Dussek's); each shows its tag on its control and is learned again or forgotten
like any other:

| Controller | Plays |
|---|---|
| CC 1 (the mod wheel) | Air added |
| CC 11 | Air (a breath controller: learn Air on CC 2) |
| CC 70 | Vibrato |
| CC 21, 23 | Growl, Flutter (the brass's; the clarinet ignores them) |
| CC 4 | Plunger |
| CC 65 | Glide held |
| CC 5 | Glide time |
| aftertouch | growl or flutter (Aftertouch to), built in; nothing on the clarinet |
| the pitch wheel | ±2 semitones, built in |

Air and Plunger moved from MIDI switch on their "by hand" switch and take over from the keys; until then soft takeover does not
hold them back, since their slider is not heard. The switch gives them back.

**Microphones.** The bell has the positions of the other instruments, in its own places: Audience (the default, 6 m out,
centered on the bell, at the player's height), Player at the player's ears, Close 1 30 cm on the axis, Close 2 30 cm well off it, a custom pair, and Recording, the
fitted sound unchanged, 1.5 m on the axis as Iowa's microphone stood. A position sets the beam by angle, the distance's level and
the room's balance; Lid and Pillow do not apply. Of the distance's delay only the pair's difference is kept, so nothing plays late. Audience is 3 to 4 dB
darker from 4 kHz up than Recording. The spacing is the pair's, as on every instrument; at Player it stops at the ears' 18 cm and at
Close 1 and 2 at 10 cm, and the value reads "(limited)" there. From above, the other positions' markers wait at the view's edge when
they are farther than 5 m (Audience). The views draw the mute that plays in the bell's mouth (a straight mute's cone, a
cup's cone and cup, a wah's bowl, its stem, a bucket over the rim, a plunger in front): a drawing, not the tables' shapes.

Each bell points as it does when played (the export's `pose`): the trumpet and the trombones forward and a little down,
the horn back and to the player's right, the tuba up and to the right. Player and Audience stay where the player is,
Close 1 and Close 2 turn with the bell, so in front the horn and the tuba are darker and lean a little to the right, as
they do in a hall. For production, Recording gives the close, centered sound: the direct sound alike in both channels, the
bell's direction not applied (Room at 0 leaves it dry, to pan and give a room in the mix).

With the microphones, Room, Volume, Dynamics, the variation and the tuning, these are Weidinger's parameters; Dussek's other
controls (hammers, pedals, lid, condition, …) are not there.

## The clarinet

A B-flat clarinet in a Woodwinds group of the menu, D3 to B-flat 5 (MIDI 50 to 82): the chalumeau and the clarion, the
break between them at A4 (key 69, where the register hole opens; the fitted break is the hole's, not a throat note's).
Its **reed** is the exciter (a mass on a spring that the mouth's pressure closes against the lay), in a **cylindrical
bore** of the same lattice as the brass's, so only the
odd harmonics are strong in the chalumeau and the clarion's twelfth is the register's. The sound is the flow through the reed,
radiated through each note's own bore and bell; a clarinet has no mutes, no plunger and no slide. The altissimo, over B-flat 5, is
not fitted (it needs cross fingerings).

**Playing.** As the brass: overlapping keys slur, lifting between them tongues, a chord is a section. What the reed adds:
- **Starts at the key.** The tongue rests on the reed for 10 ms and lets go; the note speaks 17 to 38 ms after the key, as the
  recordings' notes do.
- **Release.** A key up fades the breath over 160 ms (the spike's 80 ms brightened the end of a note 13 to 15 dB over
  6 kHz where the recordings darken). A key during the fade is tongued: the note before is stopped by the tongue, not left ringing,
  and the level eases from its level to the new note's.
- **Slurs** glide the modes from one fingering to the next (the paired glide, the notes' modes moving together) and the pressure moves
  over 100 ms from the key.
- **Velocity.** Velocity 1, 64 and 110 give the fitted pp's, mf's and ff's air (25.4, 69.85 and 114.3 of 127, the project's
  `vel_air`; the listening round adjusts it by ear), on every key alike: the reed's pressures are fitted per key, so the brass's
  higher-notes-more offset does not apply.
- **Under pp.** Below pp's air (25.4) the reed keeps pp's pressure and the output fades with the air instead (6 dB down at half of
  it). The pp sits just over the reed's threshold: less pressure stopped the note within a step, 39 dB down at air 24.
- **No growl or flutter.** They are the lips'; on the reed they sounded wrong (growl also took the clarion's pp 12 to 26 dB
  down), so the clarinet ignores them, their controllers and the aftertouch, and hides their rows. **Vibrato** is the knob's
  alone: the virtual player makes none, a clarinet's vibrato being a choice.

| Tab | What differs from the brass's |
|---|---|
| Sound | no Mute, no Plunger, no Plunger by hand, no Growl, no Flutter; Vibrato, Room |
| Player | no Glide, Glide held, Glide time, no Aftertouch to; Virtual player, Solo, Air, Air by hand, Air added |
| Touch, Mics | as the brass's |

**Microphones.** The same positions as the brass, the bell as a clarinet is held (the export's `pose`); Recording is the fitted sound as Iowa's
microphone heard it. Audience is the default, and a phrase in the middle register lies at the grands' loudness, within 1 dB.

**Known gaps.**
- The tongued breath varies from note to note by 2.87 dB (key 65 low), 0.3 dB over the spike's.
- A note change is brighter over 6 kHz than the steady notes: by 9.3 dB (slurred) and 9.1 dB (tongued) at most.
- A pitch wheel at +2 semitones plays about 13 dB down.
- Key 76 at pp is an outlier among the starts (its rise is 20 ms against the recording's 250, its level anchor 12.4 dB over its mf).
- The chalumeau's lowest keys' bore response is up to 2.2 dB off.
- The low keys' ff has mf's timbre, as in the recordings.
- The chalumeau at pp lacks the reed-colored breath at 2 to 4 kHz.
- The Vibrato knob at full swings the level 12 to 18 dB within each cycle, at pp and mf alike.
