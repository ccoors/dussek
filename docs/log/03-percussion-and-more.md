# Bars, guitars, zithers and the clavichord (steps 40–75)

The instruments without strings (vibraphone to balafon), the guitar and its measured body mode, the
zithers, the clavichord; on the way the UI's tabs, Panic, Copy CLI and the radiation's draw. Index
of every step: [README.md](README.md).

<a id="step-40"></a>
## 40. The vibraphone

The vibraphone (Iowa MIS, anechoic, C3–F6, pp/mf/ff, pedal down and up; no motor):
the first instrument without strings. `analysis/split_runs.py` cuts Iowa's octave runs
at each note's own strike (the energy at its fundamental, the run's strikes chosen
together, refined on a 2 ms envelope); `analysis/modes.py` finds a bar's modes by peak
picking and fits each one's decay (`analyze`'s envelope and decay fit). Per bar three
bending modes, 1 : 4.00 : 10.0 (the third tuned only up to about F♯4, then drifting
flat to ~8); the other peaks are the fundamental's harmonics (step 41; read here as
intermodulation, 2·f1, f2 ± f1, f3 ± f1, and other bars ringing along: D3's second and
third modes lie at D4's 2·f1 and 5·f1). Tuned ~10 cents sharp.
`proto/bars.py` fits per key and mode the ratio, the loss (pedal down: the fundamental
2.4–5.7 dB/s, the second mode 10–75), the damper's extra loss (pedal up, the
fundamental 65–190 dB/s in total) and the level, smoothed and carried on beyond the
recorded keys (levels never rising outward), and one mallet: the modes start directly
(the pluck path with `bar`, `physics::pluck_amplitude`), amplitude ∝ speed, through a
lowpass of order 2.0 whose cutoff (575 Hz at mf) rises 2.5 octaves per doubling of the
speed (the third mode's share grows ~40 dB from pp to ff in the bass; the Hardness slider
moves the cutoff). A contact simulation (the hammer's mass on a felt) fitted the same
changes only with a 2–4 ms contact, which left the upper modes 70–100 dB down in its
force spectrum: the engine's contact at 48 kHz then came out 20–30 dB off the fit.
Engine: `StringSpec::modes` (`BarModes`: ratio, σ, φ, output weight per mode, in place of
the string's partials), `"damper": "pedal"` (the damper bar followed the pedal alone:
a key never lifted it; since step 48 a held key lifts it), `mic::Body::Vibraphone` (37 bars on a 1.4 m frame at 0.85 m,
naturals nearer the player, the resonators below; player standing behind the bars,
close 1 above them, close 2 in front of the resonators). The body response is the
synthetic board's, flat and damped to ~15 ms (the bars radiate directly; the board's
machinery serves the microphone positions), the room a generic one (T60 0.7 … 0.35 s).
Renders analyzed as the recordings: the fundamental's decay as fitted, the modes'
levels re the fundamental and their change from pp to ff within a few dB of the
recordings' medians (the takes scatter ±8 dB: played by hand). The plugin: profile
"Vibraphone" in a "Mallets" group; inactive: sympathetic, pedal noise, knock, release,
grooves, action wear, unison width, stretch, unison wear (`no_effect.rs`
INACTIVE_VIBRAPHONE), no lid.
By ear the strikes clicked and the extrapolated low keys were quiet, their overtones on top.
The click: the modes started as a string's pluck, from their displacement (cos), all at
once a step; a struck bar starts from rest (sin, `Voice::pluck_with` with `bar`): the
energy above 4 kHz in the first 4 ms re the next 36 ms −31 → −45 dB at mf (recordings −44),
ff −29 → −38 (−33). The low keys: beyond the recorded ones the fundamental's level at ff
and the modes' balance at mf are held as at the nearest recorded key (`Fit.out_db`;
carried on, the fundamental fell towards the bass while the upper modes held, and held
at mf the top keys came out 12 dB over at ff). And each mode's weight is divided by the
body response's gain at its frequency (`export_bars.compensated`): the synthetic board's
random coloring had put ±4 dB on the keys' levels.

<a id="step-41"></a>
## 41. The vibraphone's motor, harmonics and the bars ringing along

The vibraphone's motor, harmonics and the bars ringing along (no recording with the motor).
Other bars ringing along, measured in the runs (pedal down, every bar free; `proto/bars.py`'s
docstring): an unrelated bar's fundamental rises 1–1.8 s after a strike by a median 6 dB
over the floor at ff (0 dB with the dampers on), 80–90 dB below the struck bar's
fundamental: inaudible, summed over all 37 bars too. The bar two octaves up has its
fundamental within 2 cents of the struck bar's second mode, but that band decays into
the floor at the second mode's rate with no tail at the upper bar's own (~5 dB/s); the
octave band decays at twice the fundamental's rate (a square of it, not the octave bar).
Not modeled (κ = 0). The other peaks are harmonics of the fundamental: where the second
mode stands off 4 (the treble's 3.9) the peaks sit at 3·f1 and 5·f1, f2 ± f1 50 dB and
more below; in the bass (second mode 4.004) a 3 s spectrum on each take's own f1 puts
them at 3.000 and 5.000 too. The bass leads with the odd ones (C3 ff: 3·f1 −33 dB, 5·f1
−44, 7·f1 −50, 2·f1 −51 re f1: the closed tube's resonances), the treble has 2·f1 (−42)
and little else. With the pedal down they grow as squares of the fundamental (re f1 +1 dB
per dB of the recorded f1) and decay at ~2× its rate; with the dampers resting they are
stronger (2·f1 ~−30 dB) and hardly grow (the felt's contact). `analysis/modes.py`
measures each harmonic's band (`harmonic_bands`), `bars.fit_harmonics` fits per key the
level at ff (smoothed, held beyond the recorded keys; none where a mode of the fit or of
the take is within 1.2 %, nor beyond a harmonic's measured span: 4·f1 only from A♯5 up)
and per harmonic the growth (dB per dB of the engine's fundamental from mf to ff, whose
mallet gives ~6 dB to the recordings' ~9) and decay, free and damped; harmonics 2…7.
Engine: a bar's fundamental runs in a bank of its own (`Voice`'s resonator bank,
`Modes::tube`) with the harmonics, set at each strike from the fundamental's new state z
(|z|·10^(L/20) at phase h·arg z, L = level + growth·20·log10(|z|/a_ref), a_ref the
fundamental after an ff strike from rest; free and damped by the engagement) and then
free (λ_h = k·Re λ1 + i·h·Im λ1). Renders analyzed as the recordings: 2·f1 within 1–2 dB
(median per key), 3·f1 and 5·f1 within 1–5 at ff, pedal down and up.
Found on the way, fixed: in the first 0.3 s the bass keys' fundamental reads up to
8 cents low (C3; C4 1), at every dynamic, and settles by 0.3 s (the resonator's own mode
beside it): `modes.py` reads it on 0.3–3 s where the take still rings (the bass up to
7.5 cents higher, the second mode's frequency unchanged: its ratio 4.022 → 4.004). The
second mode's track took the 4th harmonic from A5 up (at 4.000, often stronger than the
falling mode, 3.94 at A♯5 to 3.58 at F6; the fit had averaged the two to 3.96…3.88):
a whole-number peak that decays like a harmonic (< 2.5 × the fundamental's rate) is
skipped when there is another; the ratios' outlier test has a floor of 1 % (the middle,
tuned to 0.04 %, had made the treble's scatter all outliers). And `Modes::apply` started
zero modes (a bar's horizontal polarization, `horiz_gain` 0) that `cull` never dropped:
a struck bar's voice never went inactive.
The motor (`engine/src/motor.rs`): discs on one shaft at the resonators' tops, two dips
per turn; the resonator bank's sum × the disc's gain per sample (1 − d·(1 − 10^(−12/20))·
(1 − u^0.4), u = 1 − |cos θ| the opening: narrow dips, broad peaks), its modes detuned per
block (−3 cents closed); spin-up 0.4 s, fading in or out 0.15 s, off it rests open (the
output bit-identical). The upper modes and the strike radiate from the bars and are left
alone. Starting values by reasoning, no measurement found (the depth, the dip's shape,
the pitch). Plugin: Motor (header toggle, on by default), Motor speed (1–12 Hz, default 7)
and Motor depth (%, default 70: by ear) with the core controls, the vibraphone only.

<a id="step-42"></a>
## 42. The marimba

The marimba (Iowa MIS, the vibraphone's session: anechoic, 96 kHz, C2–C7, 61 bars, pp/mf/ff
with yarn, cord and rubber mallets; no dampers). `analysis/split_runs.py Marimba` and
`modes.py` per mallet (`marimba_yarn`, `_cord`, `_rubber`; the rubber pp run C2–B2 loses
C♯2–F2 to an uneven pace, flagged). The bars: 1 : 4 : 10 tuned to about C4 (the third mode
flattening above, 9.3 at C4), a fourth mode near 18.2–18.7 in the bass (−9 to −30 dB re f1);
the fundamental's loss 17 dB/s at C2 to 175 dB/s at C7; the fundamental's harmonics (2, 3,
5, 7·f1) at −20 to −35 dB; tuned 12–19 cents sharp. `proto/bars.py` takes an instrument
(`INSTRUMENTS`: its sets, mode tracks, damped set or none): the bars from all the sets
together, per set (mallet) its speeds and lowpass in one fit re the first set's mf, the
lowpass's order shared (per mallet it traded off against the cutoff: yarn came out with
a higher cutoff than cord). The cutoff scales with the bar, (f1/C4)^0.40
(`bars.cutoff_scale`: in the same 1–2 kHz the treble's fundamentals rise 6 dB from mf to
ff, the middle's second modes 20; a lighter bar, a shorter contact); with it the levels'
rms 7.5 → 6.7 dB. A mode found on part of the recorded keys only fades beyond them, 6 dB
per semitone (`bars.MODE_FADE_DB`; by ear: carried on, the fourth mode at 18·f1 rang
−23 dB re the fundamental at C4 where the recordings have none, a line rising through
the runs), and the third mode's track keeps to ±10 % of 10 (±25 % took 2·f2 at 7.98 near
C4 and pulled the ratio there to 9.26; now 9.96, recorded 9.95). At mf yarn 970 Hz, cord 1.25 kHz, rubber 8 kHz (order 2.06). Left: the
middle's second and third modes rise more with the strike than the lowpass gives (the
strike point: the center is the second mode's node), the treble's fundamentals at pp
~10 dB under the recordings re the middle (weighting the fundamentals more took the yarn's
lowpass away: worse). Renders analyzed as the recordings: the decays as fitted, the modes
re the fundamental mostly within a few dB, ff levels within ±4 dB of the recordings' over
the octaves (the top octave lowest). `export_bars.py --profile marimba` (`EXPORTS`): body
`marimba` (`mic::BarFrame` MARIMBA: 2.3 m of bars at 0.9 m, the bass resonators 0.85 m;
the vibraphone's geometry is the same struct), no dampers, no motor (the motor now needs
the instrument's `"motor": true`: the plugin's default on would have pulsed the marimba's
resonators), "mallets" (name, lowpass, speeds; the notes' own the first's). Engine:
`Params::mallet` picks one from the next strike (its lowpass, the note's cutoff × its re
the first's; its speeds the velocity curve's anchors). Plugin: profile "Marimba" in the
Mallets group, body 5, the Mallet menu (Yarn, Cord, Rubber) before Mallet hardness;
inactive: the vibraphone's set, damper buzz and wear, damper fall, the sustain latch (no
dampers), no lid, no motor (`no_effect.rs` INACTIVE_MARIMBA). Not modeled: the treble's
second peak 5–9 % below the fundamental (the resonator's own mode, beating with it), rolls,
dead strokes.

<a id="step-43"></a>
## 43. The xylophone

The xylophone (Iowa MIS, June 2013, the same setup: F4–C8, 44 bars, pp/mf/ff with rosewood
and hard rubber mallets; no dampers). Up to D♯5 the second mode is tuned to 3.00 (the closed
resonator's own second resonance), the third near 6 (6.7 at F4 falling to 5.7), a fourth near
10 on a few keys (too short to fit: three modes); above, untuned bars (2.6–2.9 and 5.5 falling
to 4.3); the fundamental's loss 48 dB/s at F4 to ~280 at A6; tuned 5–18 cents sharp.
`bars.py --profile xylophone` (tracks 1 : 3 : 6, rosewood first: the mallet's cutoff
6.8 kHz, hard rubber 3.6 kHz at mf, × (f1/C4)^0.24). Fixed on the way, for all three
instruments where it applied: a harmonic's level is taken only from takes whose correction
back to the strike (its faster decay over the early window) is at most 10 dB
(`H_MAX_CORR_DB`: the xylophone's top keys fall 300–400 dB/s, +50 dB, 2·f1 came out −11 dB
re the fundamental), and held above its measured keys (the marimba's top octave: 2·f1 −52
instead of −41); an upper mode never outlasts the fundamental and keeps its decay's ratio
to it above the keys it was measured on (the vibraphone's extrapolated top keys had the
second mode outlasting the fundamental); an upper mode's decay measured slower than its own
take's fundamental is a failed fit (`modes.fast_decay`, 8 ms windows every 2 ms, for the
xylophone in its place: its treble's upper modes fall 550–1600 dB/s, they read 70–300).
Checked note by note (each key at ff alone, the first 80 ms): the keys with an engine
peak over −30 dB the recording lacks 29–33 of 44 → 8–10 (left: the ratios' smoothing, 2–4 %
off the recordings' where they bend). A decay-corrected spectral level for the upper modes
was tried and dropped: it hangs on the decays it was to stand in for. Body
`mic::BarFrame` XYLOPHONE (1.5 m of bars at 0.85 m, resonators to 0.3 m), body 6. Plugin:
profile "Xylophone" in the Mallets group; the Mallet menu holds every instrument's mallets
(Yarn, Cord, Rubber, Rosewood, Hard rubber: the hosts' list is fixed) and shows each
instrument only its own (`profileMallets`, `engineMallet`; a value the instrument has not
plays its first); inactive as the marimba's. The demos transposed an octave up (written an
octave below, a xylophone part sounds there).

<a id="step-44"></a>
## 44. The bells

The bells (Iowa MIS's orchestra bells, a glockenspiel, June 2013, the xylophone's setup:
C5–E8, 41 steel bars, plastic mallets at pp/mf/ff and brass at one dynamic; no dampers, no
resonators). They sound as written, tuned 9–31 cents sharp. Each bar has a richer set of
modes than the other bars, and their ratios drift over the keys: one from 3.3·f1 (C5) to
2.85 (C7 up); the strongest from 7.2 to 5.4 (to G♯7: at C5 3–15 dB over the fundamental at
the strike, gone within 0.1 s); one from 12.0 to 9.5 (to D6); a torsional one, its ratio
∝ f1^−½ (a shorter bar: 11.3 at C5, 4.0 at C8); one from 16.4 to 12.4 (to A6; above it lies
over 20 kHz); and two quiet ones (4.9 to 3.35, as slow as the fundamental, 25–35 dB under
it; 4.1 to 3.2). The fundamental decays 5–7 dB/s up to B6, 12 at C7, 28 at C8. A mode track
can now be a list of (key, ratio) anchors, interpolated in log ratio (`bars.nominal`), as
well as a fixed ratio. A set recorded at one dynamic (the brass) takes its other dynamics'
spacing and its cutoff's opening from another set (`tied`): the plastic's cutoff 3.5 kHz
at C4, × (f1/C4)^0.31, the brass's 19 kHz; the brass takes come out at 0.17 of the plastic's
speed (its fundamentals 12 dB under the plastic's mf), and the engine plays them so (the
Mallet menu's Brass at a given velocity is quieter and brighter than Plastic). The fit's
rms is 12 dB (the medians per mode, octave and dynamic within ~5 dB: single takes scatter;
the pp fundamentals lie near the floor). The splitter pads a run in front with noise at its
own starting level, a whole number of hops (the plastic ff C5–B5 starts on its first strike
~20 ms in, inside the first frame: it lost that note and shifted the rest by one; the other
instruments' splits are unchanged). Peaks a semitone or a whole tone off the fundamental,
~20 dB under it, appear in some mf and pp takes only (at ff the same keys' neighbors lie
50–70 dB down): a bar touched in those runs, not ringing along; not modeled. The bells
ring 5–14 s, so a piece's notes pile up: `export_bars.py` trims their output 8 dB under the
calibration (`gain_db`; the demo folded into their three octaves peaked at +4.6 dBFS
without it). Checked against the recordings (each key alone, `bars_check`): the fundamental's
decays within 1–4 dB/s, the modes' levels re the fundamental mostly within 5 dB (single
cells 7–11 dB off); the top octave's pp ~11 dB too loud (the marimba's limit too). Body
`mic::BarFrame` BELLS (0.72 m of bars in a case 8 cm deep on a stand at 0.9 m), body 7.
Plugin: profile "Bells" in the Mallets group, the Mallet menu's Plastic and Brass;
inactive as the marimba's. Keys below C5 (the plugin plays all 88) carry the modes on
with limited slopes. The demos fold each note by octaves into C5–C8.

<a id="step-45"></a>
## 45. The crotales

The crotales (Iowa MIS, March 2013, the same setup: C6–C8, 25 bronze discs, one mallet (the
page names none) at pp and ff, one file per note; no dampers). They sound as named, tuned
4–32 cents sharp. Eight modes, every ratio drifting down the keys: 2.14 (C6) to 1.90 (C8),
3.74 to 3.18 (the strongest, at ff 15–20 dB over the fundamental in the low octave), 5.71
to 4.68, 6.6 to 5.8, 8.0 to 6.8, 9.1 to 8.5, 10.6 to 9.7 (the last ones end where they pass
~16 kHz); a peak at the second mode + 1 (3.04 at C♯7) is the two modes' sum, not a mode
(left out). Many modes are split pairs (the disc's degenerate modes: `cluster` 2–6); the
engine has one per track. The fundamental decays ~6 dB/s through C7, 13–16 in the second
octave, 22–26 at C8. `split_runs.py` cuts files of one note (`<instrument>.<note>.<dynamic>`)
at their strike, searched in the first second above half the fundamental: the pp takes lie
~35 dB under the ff (in broadband 22 of the 25 pp files are loudest at a noise burst or the
room's rumble, not the strike): searched over the whole file the cut fell seconds late on
4 of them (A7, A♭7, B♭7, E♭7), within the first second but over the whole band on E♭7. `bars.py`: a set without mf takes its mf
midway between pp and ff in log speed (speed 1; the mallet fit's level changes re its ff,
the harmonics' growth from pp to ff): pp 0.26, ff 3.9; the cutoff 19 kHz at C4 ×
(f1/C4)^−0.80 (6 kHz at C6, 2 kHz at C8: the output weights rise into the treble to
match), its order at the fit's bound (6); the fit's rms 7.8 dB, the ff levels per mode
within ±2 dB. The other bar instruments' profiles are unchanged. Checked against the
recordings (`bars_check`): the fundamental's decays within 3 dB/s, the modes re the
fundamental mostly within 4 dB. Output trimmed 3 dB under the calibration (they ring
3–12 s; the demo folded into their two octaves peaked at +1.5 dBFS at mic 3). Body
`mic::BarFrame` CROTALES (0.84 m, two rows of discs on a bar at 0.9 m), body 8. Plugin:
profile "Crotales" in the Mallets group, no Mallet menu (one mallet); inactive as the
marimba's. The demos fold each note by octaves into C6–C8.

<a id="step-46"></a>
## 46. The bars' stereo image

The bars' stereo image (reported on the crotales at Close 1: centered over the top
octave, the rest from the left). The radiation's eight bridge zones sat at the piano's
keys (`ZONE_NOTES`, 24…104): on a bar frame the keys beyond it are placed just past its ends,
so the crotales' C6–C8 had six zones at their bass end, the bells', the xylophone's and the
vibraphone's several. Now `Body::zone_key` spreads a frame's own range over the zones
(`zone_note` places them; the pianos and harps as before). Measured per key (R re L at the
four positions), three more things made a bar's few partials jump from key to key by
±10 dB: each microphone saw the stand-in board's modes independently (its channels fitted
for a piano's wide pair), each zone had its own random mode shapes (±4 dB on a key's modes
in level), and a key mixed two zones whose responses arrive at two delays per microphone
(cancelling in one channel and not the other). For bars: both microphones see the board
alike (only the paths differ), every zone has the fitted excitation (the coloring
`export_bars.compensated` levels the modes for), and a key sounds from its nearest zone
alone (`Body::zone_weights`); `export_bars.py` writes the body response's channels alike
(Iowa's are one microphone: identical for the crotales and the marimba, 93–97 % correlated
for the vibraphone and the bells), so the Recording position is centered. The crotales at
Close 1 now run −8 dB (C6) to +10 (C8) smoothly (before: neighbors up to 13 dB apart,
−18 to +22); what is left is the floor reflection's comb per microphone (a pure partial
near a notch in one channel, ±3–7 dB at the low close positions). Outputs re-calibrated
(the modes now levelled on the one channel: −0.7 to −1.6 dB). Also: every export writes
its recorded range (`"recorded": [lowest, highest]` after `"profile"`; the existing files
patched to the same values), and the plugin's player strip tints the keys beyond it amber
as extrapolated (`profileRecorded`, a table ui_tests checks against the files).
The plugin's "Spread all keys" (bars, at a microphone position; `Tuning::spread_keys`,
PeTuning `spread_keys`, tuning key `spread`): all 88 keys spread over the frame's zones
instead of its own range (off: the keys beyond it sound from its ends, the crotales' lower
five octaves from the left). The responses stay (the same microphones); `Engine::adopt`
now also compares the keys' zones, so a switch cross-fades to the new positions (it kept
the old radiator with the old ones).

<a id="step-47"></a>
## 47. The plugin UI as tabs

The plugin UI as tabs. The
controls had grown to full-width 30 px rows in collapsible sections, with the inapplicable
ones grayed and the wheel adjusting whatever slider was under the pointer. Now: the header
and the microphone drawing as before, and a panel of five tabs (Sound, Mics, Tuning,
Condition, Extras; `Layout.cpp` `tabGroups`) of compact 22 px rows (label, control, value on
one line; switches for toggles). What does not act is hidden (`paramVisible`: `paramActive`
with the effective preparation); the Preparation menu lists only the preparations that act
on the instrument (`profilePreparations`, pinned by `no_effect.rs`
`preparations_act_where_the_plugin_offers_them`: plucked only on the pianos, muted not on
the bars without dampers, paper on the strings, the vibraphone and the crotales), a stored
one the instrument lacks plays as None (`effectivePrep`, also for the engine). The wheel
only scrolls the tab's content (the header, the tab bar and the wide drawing stay put); a
tooltip (`Tooltip.cpp`, after 0.6 s) gives each parameter's description, all of which now
exist. The selected tab replaces the open sections in the UI state (`tab=`).

<a id="step-48"></a>
## 48. The vibraphone played from a keyboard

Its damper bar followed the pedal alone, as on the
real instrument (a key never lifted it), so without pedal every note was damped at once
(65–190 dB/s on the fundamental), however long its key was held; by ear the instrument only
sounded right with the Sustain latch. Now a held key lifts the damper off its bar, as a
piano's key lifts its strings' (the way a vibraphone is mapped to a keyboard); the key's
release lets it fall unless the pedal holds it up, and the sostenuto holds the keys down at
its press. The engine's `pedal_damper` and the instrument.json key `"damper": "pedal"` are gone
(`vibraphone.rs` `a_held_key_rings_and_the_damper_bar_takes_it_at_the_release`).

<a id="step-49"></a>
## 49. The guitar and measured board modes

The guitar (Iowa MIS's Raimundo 118, a classical guitar recorded anechoic at 5 ft) and
measured board modes. The set is one run per string and dynamic; its notes ring out (9–15 s)
and are joined by digital silence or by splices (`split_runs.py Guitar`: the silences, then
the strongest level jumps where the pitch changes across, then labels by pitch, rising; the
pp low-E run C3–B3 has no D3, which labeling by order had shifted onto five notes). A key
plays its note in first position (`datasets.guitar_files`, `scale.guitar_string`: the
string with the lowest fret; 650 mm scale, each note's length from its fret, the string's
tension, 54–73 N), E2–B5 recorded. The harp chain fits it (a finger's pluck, the
polarizations measured, a hand for a damper, the release without impacts; `scale.FINGERED`).
The body (`mic::Body::Guitar`): the top upright facing the audience, the strings' points on
the bridge, the sound hole (−3 dB, lowpass 500 Hz), the floor; Player at the guitarist's ears
above the upper bout, Close 1 at the sound hole and the 12th fret, Close 2 at the lower bout.
Plugin: "Guitar" in a group of its own; its inactive controls and preparations are the harps'
(`no_effect.rs`).
What the guitar showed: its top's main mode takes the energy of every partial near it.
Partials 1–6 of every note at 214–240 Hz decay at a median 72 dB/s, their neighbors at
10–20 (G3–B♭3's fundamentals on every string, B♭2's second partial: 60–135 dB/s; the air's
mode near 105 Hz much weaker). The random board (`board.py`: one statistical draw, constant
modal density) cannot place such a mode. Now a profile may carry a few measured modes
(`board.Board(modes=…)`, {f, q, g = φ²} after the random ones, their draws unchanged;
`proto/fit_body_modes.py`): per partial of 50–600 Hz the loudest layer's EDC rate (first
20 dB) against the model's analytic one (`string_model.free_modes`, the same window; the
engine's renders match it within 3 %, median over 1438 of Iowa's partials), r = ln(measured/model);
bands where r is high over many notes (smoothed over 1/24 octave, ×1.25 at least, p < 0.01
against r shuffled over the partials) start a mode each, fitted together through the model
(soft-L1 least squares; q 10–100), kept where it lowers the cost by 3 %. A mode at the vertical
polarization alone made the EDCs slower: with the vertical gone in milliseconds, the slow
horizontal one carried the first 20 dB, where the recording falls at 72 dB/s with no slow part
above −20 dB. So the horizontal polarization feels a share of the measured modes
(`horiz_share`, fitted: κ_h = horiz_kappa + κ·share·Re Y_measured; engine `Board::h_share`,
`measured_from`); the random board stays vertical only, so nothing changes where a profile has
no measured modes. The loss and polarization fits leave the partials within three bandwidths
of a mode out (`scale.in_mode_band`: else b1′ and κ take the mode in and the model counts it
twice), so the chain runs twice around the mode fit. The guitar: one mode, 224.3 Hz, q 42,
g 1.57, share 0.76; 96 partials' cost 132 → 110, rms ln 0.62 → 0.52. The engine against the
ff recordings in the band (EDC dB/s): B♭2's 2nd partial 91 (83 recorded), A2's 35 (39), B2's
31 (27), A♭3 62 (53); A3 47 (106) and G♯2's 2nd partial at 204 Hz 15 (55) stay too slow:
on its strings the mode reads broader than one mode holds (A2's 2nd partial at 215 Hz decays
at 9 dB/s at mf, 39 at ff). The fits: soundboard residual 11.5 dB, coupling 10.4 dB.
The pianos, by the same test (each partial's rate re its note's trend over partials 1–8,
1/6-octave bands, shuffled): the three grands of one model (Grands B, D and E) decay faster at 101–127 Hz, Iowa ×1.58 and
Piano in 162 ×1.62 (14–15 notes each, p < 0.001), Grand D ×1.35 (p 0.02); Grand C, Grand F and
Upright A at 202–254 Hz ×1.45 (p ≤ 0.002); the engine's renders of Iowa and Piano in 162 ×1.12–1.17
(not significant). Not applied: on Iowa the mode fit finds 81 and 122 Hz but gains 3 %, as the
model's rates in the first 20 dB lie ×1.4 faster than the recording's throughout (measured/model
median 0.73 over 1463 partials, as much for those 50 dB over the floor), which the coupling
fit (the whole EDC to −40 dB) leaves. Also tried and dropped: b3 from each note's loss fit
over all its partials instead of the polarization fit's six (the guitar's wound bass rings
2–3× too long in partials 13–42): median |ln| error over all partials 0.38 → 0.60 for the
guitar, 0.30 → 0.28 and 0.32 → 0.33 for the harps.

<a id="step-50"></a>
## 50. The body response follows H

The guitar's top mode made the whole difference of a few dB
(rendered with and without it, the demo differed by 13 dB under the music at most, 34 at the
median), where the recordings' G♯2–B♭2 and A3 die 18–36 dB within 0.5 s: the bass rang on its
fundamental instead (E2's second partial 7 dB under it, 17 over in the recording). The
transfer H was right (25 dB lower at 82 Hz than at 220), but the body response realized it
only to ~20–30 dB: the board's modes were summed with H as their weights, and the strong
treble modes' tails (each ~50 dB under its peak, summed over hundreds) filled the bass where
H spans 60 dB (the guitar's, rising to 0 dB at 16 kHz against the finger pluck's lowpass):
in third-octave bands 12–22 dB over H below 125 Hz. Neither narrower modes (η 0.03–0.05),
nor a mode shape with a zero at DC, nor a lower treble changed that. Now
(`board.impulse_response`, the engine's `synth_body` alike: `noise::body_filter`) the modes
are summed at flat weights and H is one minimum-phase filter after them (4096 taps at
48 kHz, the room's filter likewise); the bass follows H on average (third-octave bands
within 4 dB, single bands scattering by their few modes on a sparse board), and the engine's
synthesis reproduces the exported response (`body_synthesis.rs`, correlation > 0.999; the
measured modes at unit weight there too). Every instrument refitted from its analysis on
(the README's chains); partial levels, engine − recording re each note's median, rms below
300 Hz: guitar 9.9 → 6.2 dB (100–125 Hz from +8/+14 to −5/−2), lever harp 8.2 → 6.5 (80–100 Hz
from +9/+14 to 0/+4), harp 9.6 → 8.7, Grand E 7.9 → 5.2, Grand G 8.7 → 7.7, Upright C 6.8 → 5.7,
Grand A 7.7 → 6.7, Grand B 6.9 → 6.3; Grand C 7.0 → 8.3, Upright A 5.9 → 6.4, Upright B
7.1 → 7.4, Grand D 7.1 → 7.3, the harpsichords within 0.6 (Harpsichord A keeps +8 dB at
63–80 Hz, Harpsichord D 15 dB rms: other causes). The bars are unchanged (their H is flat:
the filter an impulse; their refit reproduced them to 1e-15). The guitar now: E2's second
partial 16 dB over the fundamental (17 recorded); the mode (224 Hz, q 38, g 1.52, share 0.56)
takes A3 25 dB down in 0.5 s (12 without it, 36 recorded), A2 21 (12; 27), B♭2 22 (9; 30);
the demo with and without it differs by 5.5 dB under the music at most. The presets render
0.4–1.4 dB less energy (`mic_custom.rs` re-pinned); a pillow at 0.7 with the lid at 0.3 is
now about −16 dB (−11: the bass passed their lowpasses).

<a id="step-51"></a>
## 51. The pedal's strings leave their dampers from rest

After the refit (step 50) pressing the
pedal clicked on Grands B and E and Upright C (less on Grand C): above 2 kHz the press rose
14–21 dB (Grand B −82 → −63 dBFS). The ring's kick (`Voice::kick`, `proto/pedal.py`) started
the strings displaced, at a pseudo-random phase per partial and 2π·i/S apart across the
unison: the bridge force jumped at the lift, a broadband step, cancelled in the unisons but
not on single strings. The refit had moved the kick there: with less bass radiated, the pedal
fit (its target is Salamander's recorded ring) bent the bell onto the low bass to make up
for it (centers 25–27, spread 9, the level up to ×4.3; before 32–36 and 6.5). Now the strings
leave the dampers moving, from rest (sine phase), the unison's strings against each other
(real weights summing to zero, (S−1, −1, …)/√(S−1)), a random sign per partial: the bridge
force starts at zero, and in a unison with zero slope too. Alone that took the ring's onset
above 2 kHz from 8–33 dB over its own later level to 13–38 under (Grands B, E, Upright C); a
single string keeps a kink in its velocity. And the kick's shape (center 35, spread 6.5, n^−8) is Salamander's for
every profile, only the level fitted through each one's own radiation (`SHAPE_FROM`): the
recording is Salamander's ring through Salamander's radiation, and fitting the shape bent it
to another instrument's radiation (the line errors with the shape fixed: 5.8–9.8 dB, free
5.9–7.4 on the four). The ring's onset above 2 kHz is now at least 20 dB under the thump's on every
piano (`pedal.rs`; Grands A, B, E and Upright C 33–55; before the fix 18 dB over it on Grand B); the ring's level after a second
within about 1 dB of the earlier fits'.

<a id="step-52"></a>
## 52. Panic

A button in the header (left of the zoom; `Layout::panic`) silences everything at
once: `Engine::silence` (`pe_silence`) releases every key and pedal as `all_keys_up` does, fades
the output over SILENCE_S = 20 ms (a raised cosine on the samples as they leave the output FIFO:
it acts on the next sample, and the fade itself has no step: `silence.rs`), then clears what
sounds without allocating (the voices' modes, contacts, buzz and grooves; the sympathetic bank's
states; every convolver's stored input and pending output, their block timing kept, so the
radiation's and the room's tails end there too; the one-shots); notes played during the fade go
with it, and a handover (`adopt`) carries the fade on. After it the output is exactly zero (2 s
of a pedalled ff chord with the room on, Grand A, Harp A, the guitar) and a note plays as from
a fresh engine. The UI cannot send MIDI: it flips a hidden input (`kDiagPanicIn`, as its pedal
levers), the audio side silences at any change (`PanicTrigger`; activation takes the value as
it is, so a restored session does not fire). MIDI CC120 is now all sound off as specified
(silence; `midi.rs` `Action::SoundOff`, `Event::Silence` in a queued score), CC123 all notes off
(the releases, as both were). The wide header now wraps onto two rows where one would leave
Volume under 140 px (Harpsichord B's four stops and Buff just above the breakpoint).

<a id="step-53"></a>
## 53. Copy CLI

A button in the header (left of Panic; since step 54 in the header menu) puts the `midi2wav` command
line for the current settings on the clipboard (DPF's `setClipboard`, text/plain), so that an
offline render plays what the plugin plays, without presets. The plugin's mapping of its
parameters to the engine's values (dB to gains, percentages, the condition plus each wear
offset, the stops' bits, the instrument's mallets, the preparation it has) moved out of the
DSP into `EngineValues` (pe_common), which the DSP and the UI both call: `engineParams`,
`engineTuning`. The tuning leaves at their defaults what does nothing at the microphone
position (the lid and the spread keys at the recording's, the free pair elsewhere) and the
bolt's position without a bolt, so a change there builds no new engine (as the DSP's
conditions did before) and stays out of the line. The worker now builds its tuning from a
snapshot of the parameter values (`fShared`, one atomic each) instead of an atomic per
tuning field; `sameTuning` decides when a change needs a new engine. The line is formatted in
Rust, beside the parser that reads it (`spec::format`, `pe_format_spec`): only the keys that
differ from the engine's defaults, the numbers in their shortest exact form; 2000 random
settings read back equal through `spec::params` and `spec::tuning`. `midi2wav` gained what the
plugin had and it lacked: `--tuning temperament=N,key=K` (the named temperaments, as the
plugin's menu) and `--latch` (the pedal down from the start, the file's CC64 then changes
nothing, the deeper counts; down again after a CC120); `--curve` is read in single precision,
as the plugin's. `ui_tests` changes every parameter in every context where the UI shows it
(22 instruments × 6 positions × their preparations) and checks that the line changes: a new
parameter cannot be left out of it silently. The plugin's defaults give `--param motor=1
--tuning seed=1` (its motor on, its wear variant 1; the engine's are off and 0).

<a id="step-54"></a>
## 54. The header menu

The header's right end had grown to Copy CLI, Panic, the zoom and Reset:
about 310 px of the row the instrument, the stops and Volume need. Panic stays a button (it
is wanted at once, mid-performance, whatever tab is open); the zoom, Copy CLI and Reset, set
up now and then, went into one menu beside it (⋯, `Layout::menu`, `menuItems`): the zoom's
steps open beside its item as the instrument menu's groups do, Copy CLI closes the menu and a
note under it says the line is on the clipboard for 1.5 s, Reset's first click arms it ("click
again to confirm", 3 s) with the menu kept open. The header gets back about 200 px. Two old
layout faults went with it. At 420 px Harpsichord B's second row (four stops, Buff, the
latch) left Volume 15 px: now the last toggles go up beside the instrument while it keeps
120 px, and where even that does not do Volume takes a row of its own. Just above the
breakpoint the one-row header narrowed the toggles to 36 px and the latch to a stop's width,
under their labels: now the toggles keep their width (only the instrument menu narrows, by up
to 60 px) and the header wraps instead. `ui_tests` sweeps every instrument from 420 to 1600 px:
Volume ≥ 140 px, the instrument menu ≥ 120, the toggles ≥ 48 (the latch 96), nothing overlapping.
A tooltip without gestures no longer leaves a blank line for them.

<a id="step-55"></a>
## 55. The radiation's draw

A complaint: on most grands D#7 had "a really dominant sharp partial"
(Grand C most of all; Grand E fine). Grand C's D#7 radiated its second partial (5.04 kHz)
11 dB over its fundamental at velocity 110 in the left channel, 28 dB under it in the right;
the recording has it 18–20 dB under. The radiation (board.py `impulse_response`: the board's
modes, each radiating to a channel with a random amplitude r) had a notch there: the
fundamental 31 dB under the response around it (re the median over a third of an octave),
the second partial 11 dB over. A draw has such notches by chance: per grand two to four
fundamentals 15–36 dB deep in one channel (Grand B's F7 −36 dB), Grand E none. The radiation
amplitudes now come from a draw of their own (`rad_seed`; the modes, their damping and their
coupling to the strings stay as they were), and per instrument soundboard.py keeps the one of
64 whose deepest notch at a fundamental (88 keys, both channels) is the least deep (the
profile's "radiation" block): −12.0 … −12.5 dB on the grands (the first draws −16 … −28),
−5 … −12 dB on the others (`test_radiation_notches`; the bars' mono radiation, export_bars.py,
−5 … −6 dB, a new draw without the pick). Grand C's D#7: the second partial at velocity 110
now −4.5 dB re the fundamental (left), −12.4 (right); at 80 −13 / −21. What is left comes
from the modes, which both channels share: the fundamental lies in a valley of the bridge
admittance (Re Y_n 0.13, mean 1), so it radiates 8–9 dB under its surroundings in both
channels and decays slowly (−28 dB at 1 s where the recording is at −46). Every instrument
refitted: the chain moved more than the radiation, its fits one pass further than step 50's
(Grand A's felt f05 at C4 12.7 → 16, stable on another pass; its knock; the treble's note
gains by up to −8.6 dB). The presets' render energy (mic_custom golden): −0.8 … +2.4 dB.

<a id="step-56"></a>
## 56. The clavichord

A new instrument from Edinburgh's unfretted clavichord of the 1740s (CC BY
4.0; C2–D6, double strung, A = 415 Hz): 43 of its 51 keys without vibrato and without noise
reduction, one dynamic, 2–3 s each, the museum's air conditioning 25–40 dB under the partials
(the bass fundamentals 6–17 dB). 41 are used: C3 and C4 sound an octave above their names,
B2's take is cut before its second strike. The analysis measures each set's tuning re its own
A (`"a4"` per item; at 440 every note read −100 cents and was dropped as mislabeled), so the
instrument plays at the concert pitch with the set's own deviations (±10 cents). The tangent
strikes the string at the end of its speaking length and stays there: the pluck's limit
β → 0, a ramp from its apex, 2·amp/(πk) per partial (no comb), through the quill's lowpass
(`PluckSpec::tangent`, string_model `"tangent"`; 1 mm, the lowpass at 20 partials, the
harpsichords' 3 dB and half an octave per dynamic step: one layer, starting values). The
design (assumed): C5 0.27 m, ×1.9 per octave, foreshortened below C3 to 1.25 m at C2; brass
strings at 55–35 N; a 0.12 m² board 3 mm thin; the listing cloth damps a released key's
strings with the harpsichords' damper. The first fit decayed too slowly (the model's rates a
median 0.61 of the recordings', 0.3–0.4 in the treble): its partials fell 10–20 dB in 0.7 s
where the recordings' fall 25–40. Not the strings (rendered without the room they fall as the
recordings do) but the room: `body_decay.py` measures it between the partials, where these
takes hold the air conditioning, and read 4–5.6 s in every band. The clavichord's room is now
assumed (`soundboard.ROOM_T60_ASSUMED`: 1.2 s at 250 Hz to 0.4 s at 16 kHz, a museum gallery;
the few clean estimates 0.6–2 s): decay rate model / recording, median over notes and partials
1–12, 1.02 (|log2| 0.43; the recordings' noise much of it). The fits: soundboard residual 4.3 dB,
coupling 6.0 → 4.6 dB, the register's mf levels 4.0 dB rms, the knock 9.4 → 7.6 dB. The takes
are too short for `fine_structure.py`'s 6 s beat fit: the unisons' scatter is the pianos'
median, 0.1 cents. The Bebung: the tangent pressed harder raises the string's tension. The
vibrato takes swing 10–30 cents at 4–5 Hz; `Params::bebung` (default 30 cents, the plugin's
Bebung 0–100) is the rise at full pressure, the highest of the mod wheel (CC1), the channel
pressure and the key's own (polyphonic aftertouch); per block each held key's rise approaches
depth × pressure (10 ms), a released key's falls back, and its strings' modes follow from their
state (`Voice::set_detune`, the motor's `Modes::set_detune` on the string banks; a strike while
pressed starts at the raised pitch). The other instruments ignore it, bit for bit
(`no_effect.rs`). In the plugin: a Clavichords group, the Bebung with the core controls, the
hardness as "Tangent brightness"; inactive the pedal noise, the hammer grooves and the stretch
(double strung, the unisons act); the harpsichords' preparations. Open: the amplitude's swell
in the vibrato takes (a few dB), the held tangent's slight muting, the pitch rise at a hard
strike, the tangent's own release noise (Salamander's damper noise for now), the room's level
(the generic one), the clavichord's body (the grand's microphone geometry), the vibrato takes
as a check of the depth.

<a id="step-57"></a>
## 57. The tubular bells

VCSL's three chime sets are the same 18 tubes (their modes agree within
1 Hz); "Tubular Bells 2" is the clean one (the floor at −88 dB, decays of 10–44 s): the 11
white keys C4–F5 at two dynamics (as mf and ff; pp mirrored below mf). A tube's heard pitch,
the strike note, lies at half its fourth bending mode, where no mode is: the mode analysis
takes that mode as its reference (`ref_ratio` 2, the "fundamental" of the bar fit, its
tuning re twice the label: +2 … +13 cents) and the ratios re it, the two modes below it
included (0.31 and 0.61, which ring longest: their T60 up to 30–50 s in the treble, a lower
mode may outlast the reference, `lower_free`), six above (1.48 → 1.45 … 4.94 → 4.53 from C4
to F5, flatter than a solid bar's). The legacy set's 18 (lossy, cut at 5–11 s) check the
black keys: their ratios lie within 0.002 of the curves. Model against recording, per mode
over the 22 takes: decay rates a median 0.95–1.05 (the quiet lowest mode 1.48), levels re
the reference within ±1.5 dB. The tubes beat slowly (split pairs): the two lowest modes'
envelopes swing 1.8–2.5 dB at 0.3 Hz, the others 0.5–0.75 dB at 0.3–1.2 Hz; the model's did
not at all. Each mode now has a partner (`BarModes::split`: Δf and its share, the false
beats' mechanism with the bar's own values; a swing of d dB std is a partner at d / 6.1 of
the mode's amplitude): per recorded key and mode the measured one, elsewhere the track's
median swing at a rate drawn about its median (seeded). A bar's fundamental ran in the
resonator's bank, which takes no copies: a split fundamental now runs with the other modes
(the chimes have no harmonics or motor; the other bars unchanged). The renders' swings: C4's
reference 0.9 dB at 0.24 Hz (recording 0.9 at 0.26), E4's 0.61-mode 3.6 dB at 0.61 (4.2 at
0.57). The damper bar (no damped takes) is assumed: 40 dB/s on every mode, lifted by a held
key or the pedal as the vibraphone's. The body: a rack 1.3 m wide, two rows, each tube a
source radiating all round at its middle, 1.3 m up (`mic::CHIMES`, assumed). In the plugin:
"Tubular bells" in the Mallets group; the vibraphone's inactive controls and preparations.
Open: the damper's loss, the side peaks at 1.39–2.25 (20–40 dB under), the rack's geometry.

<a id="step-58"></a>
## 58. The hand chimes

VCSL's set: 19 tubes C4–C7 a whole tone apart (named an octave low, as its
kalimba's and balafon's), one take each at one dynamic, normalized. A chime is nearly its
fundamental alone (2 dB/s at C4 to 20 at C7), with the harmonics 2, 3 and 5 (the fit's,
smoothed: −43 to −57 dB re it; single takes scatter from −24 to −80) and two fast modes up to
E5 (1.49 → 1.57 and 5.99 → 5.80, 25–190 dB/s). One dynamic says nothing about the clapper: it
is assumed (`bars.INSTRUMENTS` `"mallet"`: 6 dB per doubling of the speed, the cutoff opening
half an octave per doubling, pp and ff at 0.25 and 3), and the harmonics' levels are carried
from mf to ff as a free bar's grow (1 dB per dB). A damper (the ringer's shoulder, assumed
60 dB/s) that a held key or the pedal lifts, as the vibraphone's. Model against recording over
the 19 takes: the fundamental's decay a median 0.99 (single keys 0.6–2.6: the tubes differ,
the fit is smooth), the fast modes −0.8 and −3.4 dB, their decays 1.23 and 0.99. The body: a
row of ringers behind a table, the tines 1.2 m up (`mic::HANDCHIMES`, assumed). The engine's
bar bodies are now one pattern (`bar_bodies!`), the instrument's body one field
(`Body::from_name`) and the C interface's bodies a table. In the plugin: "Hand chimes" in the
Mallets group; the vibraphone's inactive controls and preparations. Open: the upper keys'
slow modes at 1.2–1.4 (−18 to −33 dB, 9–33 dB/s; their ratios scatter from key to key), the
lowest keys' 13.8 and 15.9 modes, a second dynamic for the clapper.

<a id="step-59"></a>
## 59. The kalimba

VCSL's Tanzanian kalimba: 27 steel tines G2–C♯7, one pluck each, normalized,
tuned up to 65 cents off the names (the mode analysis's `fund_cents` widens the search; the
keys play at equal temperament, `equal_tuning`: keyboard playability). A tine is a cantilever:
1 : 6.47 (6.3–6.8) : 18 (theory 1 : 6.27 : 17.55), the second mode the strongest at the pluck in
the bass, gone within 0.1 s, the third in the bass only; the fundamental 5–10 dB/s in the bass,
25–40 in the middle, 90–180 at the top. The other tines ring along (the pluck's kick through the
bridge): 13–20 of them in every take, −10 to −45 dB re the plucked fundamental; in the treble
they outlast it. The analysis measures them (`halo`: per take, at every other tine's own
fundamental, the peak over 0.05–0.55 s re the take's fundamental, or the level it would have
needed where none rings); the fit takes both back to the onset by their fitted decays and
splits them into a level per tine (how readily it rings: the middle tines C♯4–G4 most) plus a
smooth term over the plucked key, L = H_j + g(i) (alternating medians; a tine not heard counts
6 dB under its threshold: from the heard takes alone the treble's tines, heard in 6 of 26, came
out the loudest). Each key then carries the ringing tines as extra modes (at their pitches,
with their own losses: up to 24 per key, those above −60 dB). Model against recording: the
fundamental's decay a median 0.95, the second mode −1.1 dB (decay 1.10), the third −0.3 dB;
the ringing tines heard in both a median −2.0 dB apart (±6 per tine and take: the H + g model),
where the recording heard none the engine 3–6 dB under its threshold. One dynamic: an assumed
thumb (6 dB a doubling, the cutoff opening half an octave). No dampers. The body: a small board
held at 1.0 m (`mic::KALIMBA`, assumed). The thumb's click: the recordings' 4–12 kHz band lies
3–20 dB over the modes' in the first 10 ms and falls over 30–80 ms; a knock (the engine's noise
burst, 80 Hz–12 kHz, 30 ms; `export_bars` EXPORTS "knock", at −4 dB re the keys' median radiated
peak at mf) puts the onset within −3.1 … +2.8 dB of them (1–4 kHz within −0.5 … +3.4) over the
first 200 ms. In the plugin: "Kalimba" with "Thumb brightness" and "Thumb click" (the knock, and
the action's wear on it); the menu's "Mallets" group is now "Percussion" (bars, tubes and
tines); the marimba's inactive controls but those two, the crotales' preparations. Open: the
tine's rattle, the ringing as a coupling that follows what was played (here each key carries
its own copy), the mbiras' buzzers.

<a id="step-60"></a>
## 60. The balafon

VCSL's balafon: 6 wooden bars (C♯4, F4, C5, F5, C6, F6) over gourds, with
traditional, hard and soft mallets at three dynamics (the recorded levels kept: the mallets'
fit as the marimba's, one set each). The bars' modes run 1 : 3.87 → 2.30 : 8.10 → 3.93 from
C♯4 to F6, a fourth at 12.3 and 11.0 on the two lowest; in the bass the fundamental is the
weakest of them; all fast, 45–200 dB/s (the bars die in 0.3–1.4 s). The gourds' membranes
buzz: the fundamental's harmonics up to the 7th at −19 to −40 dB, growing 0.1–0.5 dB per dB
(the bars' squares grow 1): the fit's harmonics carry them. Six bars leave the losses' curves
unanchored beyond them, where carried on the treble's slowed again (F6 106 dB/s, C8 52): they
hold beyond the recorded keys (`hold_sigma`). Model against recording, per mallet: the
fundamental's decay a median 0.97, 0.98 and 0.91, the second mode within ±2.6 dB, the third
±1.6; the fundamental's level per dynamic within 2 dB (the soft mallet's pp 3). The body: a
low frame at 0.6 m, a gourd under each bar (`mic::BALAFON`, assumed). The buzz's noise: between
the modes' lines the recordings hold noise 14–26 dB under their total over 50–300 ms (the
model's 42–72), flat from 250 Hz to 4 kHz and 9 dB down at 4–8 kHz. A membrane
(`Voice::membrane`, instrument.json "membrane"): Gaussian noise high-passed at 200 Hz through
two one-pole lowpasses at 1.6 kHz, times the bar's own rectified level (its resonator's
fundamental included, over 5 ms; the noise's density held to 48 kHz's at any rate: its band
within 1.4 dB at 44.1, 48 and 96 kHz): the buzz follows each strike and dies with it. At level 0.67
the noise between the lines lies −3.6, −0.6 and +1.9 dB re the recordings' (250 Hz–1 kHz,
1–4 kHz, 4–8 kHz; per take ±5 dB); the modes' fit unchanged. In the plugin: "Balafon" in the
Percussion group, the Mallet menu's Traditional, Hard and Soft; the marimba's inactive
controls and preparations (the paper preparation is inert on instruments with several
mallets: their strikes take the other-mallet path, which starts no paper). Open: a control for
the membranes, each bar's own gourd peak beside the fundamental (1.06–1.67, no track over the
keys), only 6 bars recorded.

<a id="step-61"></a>
## 61. The psaltery and the strumstick

VCSL's plucked psaltery (11 steel strings A♯4–F♯6 a whole
tone apart, one dynamic, the levels kept) and its strumstick (a three-string fretted dulcimer:
open D3, A3, D4, 19 notes D3–A5 at three dynamics, the levels kept), both named an octave low:
the harps' chain with designs of their own (`scale.py`: the psaltery's strings 0.32 m at A♯4
shortening ×2.15 per octave, music wire at 70 N, a pick's lowpass; the strumstick's 0.6 m scale,
each note on the string the recordings play it on, the finger's lowpass), plucked by fingers or
a pick (`FINGERED`: no dampers but the hand at a key's release). Their gaps between the partials
hold their strings ringing (the psaltery's are all undamped): body_decay read rooms of 5–20 s,
so VCSL's dry studio is assumed (the harp's measured 0.2–0.5 s; `VCSL_DRY_ROOM`). The
strumstick's fundamentals ring 6–12 dB/s while its partials 2–6 fall 25–150 dB/s, unevenly: the
polarization fit's plain median over six partials (its fallback where b1 + b3·ω² will not fit)
ran the melody string's fundamentals 2–7 × too fast. For it (`LOW_WEIGHTED`) b1 and κ are the
partials' medians weighted 1/n² (a one-stage partial's κ excess 0), b3 the upper partials'
excess over b1, κ through a running median before the smoothing (the few two-stage
fundamentals, F♯4, A4, D5, carried theirs into the one-stage notes beside them): the
fundamental's decay ratio model/recording a median 1.42, IQR 1.04–1.62 (was 1.43, 1.14–2.93;
the guitar's 1.52, 1.11–1.82), the worst A4–E5 at 1.8–2.1 (were 2.4–7.5); partials 2–3 now
ring a little long (0.54, 0.62). The psaltery: the fundamental a median 1.26, partials 2–8
1.07–1.27.
Both on the guitar's body (`scale.ENGINE_BODY`: no geometry of their own yet). Levels per key
flat within ±6 dB across the keyboard (the strumstick's peaks below D3 rise by their attack:
their rms holds). In the plugin: "Strumstick" in the Guitars group, "Psaltery" in a new Zithers
group ("Pick brightness"); the harps' inactive controls and preparations. Open: bodies of their
own, the psaltery's undamped strings ringing along (as the kalimba's tines), the strumstick's
two-stage fundamentals.

<a id="step-62"></a>
## 62. The đàn tranh, and the pitch wheel

VCSL's đàn tranh: 16 steel strings over movable bridges,
B2–B5 pentatonic at three dynamics (mf, f, ff, taken as pp, mf, ff), every file normalized.
Five lower strings sound a semitone above their names (relabeled); the bridges lie −43 … +49
cents off, so the keys play at equal temperament (`scale.EQUAL_TUNING`). The harps' chain on a
design of its own (0.85 m at B2 shortening ×1.5 per octave, 70 → 45 N, a pick's lowpass). With
normalized takes the model's own balance rose 47 dB from A0 to C8; the register fit now follows
the concert harp's recorded levels at the nearest notes (`LEVEL_REFERENCE`): peaks within
−31 … −18 dBFS over the keyboard. Model against recording: the fundamental's decay a median 1.61
(the guitar's 1.52), partials 2–8 1.01–1.45, levels re the fundamental ±3–7 dB. Its character is
the left hand pressing the string behind its bridge: the pitch wheel (`Event::PitchBend`, −1 … 1;
MIDI's 14 bits re the middle) bends the sounding notes by ± `Params::bend_range` semitones
(default 2, at most 12) on instruments whose strings bend (`Instrument::bend`, instrument.json
"bend": the đàn tranh; others ignore it). The bend moves every key's strings through the
Bebung's path (`Engine::pitch_step`, the two summed; the same 10 ms settling, a key struck while
bent sounds bent): the test bends A4 by +200.0 and −100 cents within 3, a note struck at +300
within 5 (the board's reactive coupling pulls a mode a few cents, differently at the bent pitch).
The C interface `pe_pitch_bend` and `PeParams::bend_range`; the plugin reads 0xE0 and shows
"Bend range" for the đàn tranh alone (`profileBends`); midi2wav reads the wheel, `--param bend`;
`make_test_midi.Track.bend`. In the plugin: "Dan tranh" in the Zithers group ("Pick
brightness"), the harps' inactive controls and preparations, the guitar's body (no long zither's
geometry yet). Open: a body of its own, the bend on the struck string alone (MIDI's wheel moves
every sounding note; the left hand presses one), the glissandi and FX takes.

<a id="step-63"></a>
## 63. A sparse board's first mode

The synthetic board's modes lie on a jittered grid from 35 Hz at
the board's density; the strumstick's (one mode per ~200 Hz) put its first at −0.9 Hz, and
every microphone position but "Recording" panicked in the body response's inverse transform
(on the audio thread, in the plugin). The grid is now held at 20 Hz at least (`board.F_MIN`;
every other instrument's lowest mode lies at 26 Hz or above: their boards unchanged), and the
engine refuses a board mode at or below 0 Hz at load. The strumstick refitted on its board
(its fit's numbers unchanged within 0.1 dB); every new instrument renders at every
microphone position.

<a id="step-64"></a>
## 64. The hand chimes' strike, and VCSL's bars in a dry room

The recordings' attack: in the first
0.2 s a dozen of each tube's modes ring at 1–16 kHz (the clapper's hit; 40–3700 dB/s), and in
the low tubes the fundamental swells in after the click (its first 8 ms 20–38 dB under its top,
within 3 dB after 20–50 ms at C4–E5; from G♯5 up at once). The engine had the three tracked
modes alone: C4's first 30 ms 17–39 dB short above 500 Hz. Now each take's other modes are
the key's (`bars.Fit.fit_strike`, the nearest recorded tube's; ratios kept): those of the
analysis's spectrum (their level from its peak, `spec_level_db`: the decay fit's `level_db`
read weak modes beside a strong fundamental 20–30 dB high, G♯5's 1.24 and 1.34 at −25 and
−15 dB where the spectrum has −54 and −47; the open point of step 58, the slow modes at
1.2–1.4, was that) and the strike's short-lived peaks, which a 0.3 s spectrum misses
(analysis/modes.py `strike`: the first 60 ms, each peak's level re the fundamental and its
`fast_decay`; the fastest, gone within a few milliseconds, make the click): 8–26 per key.
The swell (`rise`, the fundamental's level in 8 ms windows over 0.1 s): e^{−σ1·t}·(1 −
e^{−σ·t})², two partners at the fundamental's frequency, −2 and +1 × its weight at σ1 + σ and
σ1 + 2σ (a negative output weight: `out`'s sign, the export's `sign`); σ by a grid search,
29–167 /s, rms 0.5–1.1 dB (C5 2.3: its first 4 ms at −38 dB, then a jump; 1 − e^{−σ·t} missed
by up to 3.4). In the engine the swell came out at C4 −3 dB at 76 ms where the recording has
28: the room. The bars' generic room (T60 0.35–0.7 s) at the body's energy, the body being the
direct sound (~15 ms), builds up under the fundamental, and held the fast modes up (their decay
0.69 × the recordings' on the hand chimes, 0.64 on the balafon). VCSL's sets are close
recordings in its dry studio: the hand chimes, the kalimba and the balafon now take its T60
(`soundboard.VCSL_DRY_ROOM`, as its harp measures it) at 0.01 of the body's energy
(`export_bars.VCSL_BARS_ROOM_GAIN`): the fast modes 0.93, 0.96 and 0.73 × (without any room
0.92, 0.96, 0.71), C4 −3 dB at 28 ms. Against the recordings over the first 30 ms, C4 −3 …
−12 dB per octave band from 500 Hz up, G♯5 −5 … +2. Open: the treble's fundamental, which
falls 4–12 dB in the first 40 ms on some keys and not on others (no beating: swings within
1.5 dB); the broadband haze under the strike (60–80 dB down).

<a id="step-65"></a>
## 65. The balafon, less metallic

Over all 50 takes (octave bands re each one's total over
100–300 ms, engine − recording, medians) the engine was bright where wood is dull: 2–4 kHz
+5 … +7.5 dB over 30–300 ms, 8–16 kHz +7 … +13; the soft mallet and every pp 10–30 dB over at
2–4 kHz. Three causes. (1) The upper modes' levels: the decay fit's `level_db` takes a line
through their spike at the strike (700–1200 dB/s over the first 30 ms) and their ring after it
(80–120 dB/s) and read them 5–20 dB over the spectrum: C5's third mode at the fundamental's
level, where 10–50 ms after the strike it lies 13–48 dB under. With `spec_levels` an upper
mode's level is its spectrum's peak re the fundamental's, both taken back by their decays
(`bars.Fit.mode_level`, `spec_correction`); the mallets' dynamics tied to the traditional's
(`tied`: the fit traded a soft mallet's cutoff at its bound against speeds of 2–3.6; single
takes stray 10–20 dB, the mallet fit's rms 8.2 dB, the hard mallet's cutoff at its 20 kHz
bound). (2) The body: the bars' synthetic body swings ±25 dB peak to peak at 4–8 kHz (std 7.5;
the modes are compensated one by one, so only noise shows it): the membrane's noise came out
in stripes. The balafon's body at "Recording" is now flat (`export_bars` `flat_body`; the other
microphone positions keep the board's). (3) The membrane's noise: re the fundamental the
recordings' is flat from 1 to 2 kHz and falls 16 dB per octave above, 8 dB under at 250–500 Hz,
and grows with the strike (pp → ff: 1 kHz +2 dB, 2 kHz +12, 4 kHz +18); the engine's was the
same at every dynamic. `instrument::Membrane` now carries its filters and growth (`hp_hz`,
`lp_hz`, `poles`, `growth`, `tilt`: × r^growth and the lowpass × r^tilt, r the strike's level
re the key's own at speed 1, whichever mallet), fitted by a grid search: high-passed at 700 Hz,
three poles at 2.5 kHz, growth 0.4, tilt 0.2, level 0.5: 3.3 dB rms over 1–8 kHz and the three
dynamics (the fixed filters: 5.3). Now: 250 Hz–8 kHz within ±4 dB over 30–300 ms, per mallet
within ±5 but the hard one's 4 kHz band (−9 … −11). A knock (a noise burst) for the strike's
first 30 ms, 6–10 dB short at 2–8 kHz, lifted only 8 kHz (and the soft mallet's 10 dB over):
the shortfall is the modes' spike, tonal. Open: that spike, the noise's tail after 0.3 s
(5–25 dB short: the membrane dies with the bar, the recordings' noise more slowly), a hum at
160–195 Hz in every take (15–25 dB under the fundamental, not decaying: left out).

<a id="step-66"></a>
## 66. The kalimba's ringing tines as a control, and its drawing

In the treble the other tines
ring as strongly as the played one, or more (the analysis's halo summed over the lower tines:
C♯6–C♯7 +1 … +13 dB re the played fundamental in the recordings, the engine within a few dB
of them): the bass tines outlast the fast treble. As the pianos' sympathetic resonance, it is
now a control: the halo's modes are marked (instrument.json `halo_from`, `BarModes::halo_from`,
`Physics::halo_from` among the partials kept at the rate) and `Params::sympathetic` scales
their start at each strike (`Voice::set_halo`; at 0.5 G3's tine in C♯6's ring 6 dB down, at 0
gone, the played fundamental alike). In the plugin "Ringing tines" (the sympathetic resonance's
slider, active on the kalimba, its own tooltip: `displayDescription`, also the thumb click's).
The drawing: the bars' frame stood the kalimba on legs at 1 m. Now from the front a seated
player behind it, the board at the chest, the tines' row (the bass's long ones left, as the
keys) and the two hands holding it; from above the board with its corners cut and the tines
towards the player (`Body::drawing`; at true scale beside the positions' pairs it stays small).

<a id="step-67"></a>
## 67. The balafon's strike

It clicked where the recordings do not: a bar's modes are released all
at once (the pluck path's `bar`: velocity from rest, cosine phase in the bridge force), so
their sum stepped from silence to a quarter of the strike's peak in one sample (C♯4: two
thirds), a broadband click; the recordings rise from silence over 1–3 ms (in 1 ms frames
−23, −4, 0 dB re the first 50 ms) under a burst of noise above 6 kHz in their first 3 ms (re
the note over 5–100 ms: −18 dB with the traditional mallet, −24 hard, −31 soft) that the
engine's step matched in level (−26 … −29) but not in kind. Now each mode starts from zero
over the mallet's contact: a partner at its frequency in antiphase decaying 1/τ faster
(`export_bars.with_extras` `onset_s`, τ = 1 ms; 0.3–2 ms tried, 1 ms nearest the recordings'
rise: −26, −3, +1 dB), and the contact's noise is the engine's knock, 3 ms, to 16 kHz, at each
key's level (`per_key`: its knee the key's radiated peak re the median) and per mallet
(`Mallet::knock`, instrument.json `knock_db`: 0, −3 and −12 dB): the burst −18.9, −23.6 and
−30.7 dB (spread over the keys 2 dB). In the plugin "Mallet click" (the knock) and the
action's wear act on the balafon. Its first 0.2 ms now under a fifth of the peak (the test;
before: 0.25–0.66).

<a id="step-68"></a>
## 68. A control for the balafon's buzz

Its bars do not ring along (κ = 0), so the sympathetic
resonance's control is free on it: "Gourd buzz" (`Params::sympathetic`, 100 % as recorded)
scales the membrane's noise (its level) and the fundamental's harmonics at each strike (the
resonator bank's `harm_gain`, `Voice::set_harmonics_gain`; the harmonics are the buzz's
periodic part, the fit's up to the 7th). At 50 % C5's second harmonic and the noise between
its 4th and 5th fall 6 dB, the fundamental stays (the test). Instruments without a membrane
keep their harmonics on every setting.

<a id="step-69"></a>
## 69. The strumstick's overtones

Partials 2 and 3 rang long: their rates a median 0.54 and 0.62 of
the recordings' (energy decay curves), the fundamental 1.42 × fast. The loss fit
(`fit_polarizations` LOW_WEIGHTED) took b3 from the upper partials' slow stage, a two-stage
partial's tail 8–16 dB under its start; it now takes their whole decay (one line through both
stages, `note_rates`' last field), and b1 net of b3·ω² (the low partials' slow rate less it;
two rounds). Model against recording now: the fundamental 1.34, partials 2–5 0.64, 0.79, 0.97,
1.03, 6–8 1.31–2.27 (before 0.82–1.64). One b3·ω² cannot hold them all: in the recordings
partials 2–3 fall faster than the law allows re the higher ones (the body's modes, which the
synthetic board does not place); the fit centers it on partials 2–5, which carry the sound.

<a id="step-70"></a>
## 70. The kalimba's rattle

Between the lines the recordings hold a sizzle from 250 Hz to 8 kHz that
follows each tine (G3's for the whole second, F5's half a second) and dies about twice as fast
as it; the engine had only the thumb's click (gone by 0.25 s), 12–54 dB under after 50 ms.
It is the balafon's membrane (Voice::membrane) with two additions: `Membrane::power`, the noise
on the bar's level to this power re its peak since the strike (a rattle buzzes while the tine
swings hard; 2 here, the balafon 1), and `sympathetic` (the balafon's on its control, the
kalimba's not: its control is its ringing tines). High-passed at 300 Hz, two poles at 6 kHz,
level 0.2 by a grid search (4.5 dB rms over 500 Hz–8 kHz and 50–500 ms; 2 kHz 4–9 dB over, the
recordings dip there). Each tine rattles its own way (re its fundamental −45 … −81 dB over
1–4 kHz and 50–300 ms, the two takes of a pitch up to 15 dB apart, no trend over the keys):
the analysis measures it (modes.py `rattle_db`), the fit carries the nearest tine's to each
key (bars.Fit.rattle), and the export predicts the engine's per key (`export_bars.rattle_db`:
the key's modes, the follower, the noise, the same measure) and sets its gain
(`rattle_gain`, instrument.json notes `membrane_gain`, `Note::membrane_gain`): the engine
within −1.3 … +2.4 dB of each tine (the doubled pitches ±6–9 around their two takes). The
kalimba's body at "Recording" is flat too (its noise striped as the balafon's).

<a id="step-71"></a>
## 71. Bodies of their own for the zithers and the strumstick

Bodies of their own for the zithers and the strumstick (they were on the guitar's, which only
the microphone positions other than "Recording" and the drawing use). The strumstick
(`Body::Strumstick`): the guitar's machinery through `GuitarDims` (the classical guitar's, and
the strumstick's: its outline at 0.75 along and 0.42 across, 0.37 × 0.15 m, 6 cm deep, its
three strings D3, A3, D4 on the bridge by the notes each plays, a 0.6 m scale), held on the lap
as the guitar. The zithers (`Body::Psaltery`, `Body::DanTranh`, `ZitherDims`): a box flat on a
table, the strings' drive points on its top from the bass's to the treble's, the top radiating
upwards (its underside on the table) and the psaltery's rose (the guitar's hole's −3 dB and
500 Hz lowpass); the bars' placements and clamping on a frame of the box (the player behind
and above it, the audience in front, close 1 over its middle, close 2 beyond its far edge).
The psaltery: a triangle 0.5 m at its base near the player, 0.4 m deep, on a table at 0.75 m,
the bridges along the base. The đàn tranh: 1.1 m on a stand at 0.6 m, 0.26 m wide at the head
(the player's right) and 0.16 at the tail, the movable bridges on a diagonal by their
speaking lengths, the bass farthest from the player. All assumed. Every position renders
(levels within 6 dB of "Recording"); the C interface's bodies 14–16, the plugin's `BodyKind`;
`scale.ENGINE_BODY` is empty (every design its own).

<a id="step-72"></a>
## 72. The balafon's buzz per key, and its other lines

Some notes still clicked and all sounded
metallic: between the lines the engine held a broadband wash to 16 kHz for 0.5 s on some
keys and none on others (hard C♯4 at mf −82 dB re the recordings' −37, F6 +24 over; the
analysis's `rattle_db`, 1–4 kHz over 50–300 ms re the fundamental). The cause: the membrane's
level and lowpass follow the strike's level re the key's own at speed 1, both the modes' sum
at the bridge, and since step 67 each mode has an antiphase partner, so the sum is zero by
design and the ratio was rounding's. Now the strike's swing is the modes' root sum of squares
(`voice.rs` `swing`): every key buzzes more the harder it is struck (the test, all keys and
mallets). Each gourd buzzes its own way (−38 … −56 dB at mf): the analysis measures the
balafon's takes too (`rattle`), the fit carries the first set's mf per key (bars.Fit.rattle)
and the export sets each key's gain (`membrane_gain`, the kalimba's path): the traditional
mallet at mf within 1.6 dB per key. Per mallet a gain for the same swing (`Mallet::buzz`,
instrument.json `buzz_db` −2, −5, −10 dB: the engine had buzzed a median +2.3, +4.8, +10.2 dB
over the recordings with the traditional, hard and soft mallets). The membrane refitted on
the new swing (octave bands 250 Hz–16 kHz, 50–300 ms, all 54 takes; per point an export):
high-passed at 0.7 × the key's fundamental (`Membrane::hp_ratio`: the recordings' buzz starts
at each bar's pitch; 700 Hz fixed left the treble's 250–1000 Hz 10 dB over), three poles at
2.5 kHz, growth 0.3, no tilt (0.2 opened hard strikes' 8–16 kHz 9–11 dB over; 4 or 6 poles at
2.5–4 kHz: 4.7–15 dB rms): 4.2 dB rms; per mallet over the 18 takes a median −0.5, −1.2,
+1.0 dB, 4.6–5.0 rms. The lines: four modes per bar left out lines the recordings ring for
the whole note (C♯4's 10.5, 13.2 and 23.0 × f1, F5's 6.4, 7.6, 10.6, each bar's gourd beside
the fundamental: F5 1.13, F6 1.15). The bar fit takes the strike's modes (`strike`, the hand
chimes' path) from every take (`fit_strike_all`: their levels taken to the traditional
mallet's mf by the mallets' lowpasses, a line in two takes or more, the medians), only lines
15 dB over the spectrum around them (the analysis's `line_db`; the buzz's own peaks stand
2–13 dB over its median, real lines 15–42); they too start from zero (their onset partners).
The click unchanged: the burst −18.3, −23.7, −30.1 dB (recordings −18.4, −24.2, −30.7).

<a id="step-73"></a>
## 73. "As recorded", a temperament

Every instrument plays the menu's temperament (Equal by
default) on its smoothed measured tuning: the strings' curve (`scale.Profile._tuning`, at most
2 cents per semitone), the bars' (`bars.Fit._tuning`), or equal temperament where the recorded
instrument lies far off it (the kalimba's tines up to 65 cents, the đàn tranh's bridges). The
last entry, "As recorded" (`temperament::AS_RECORDED`), adds each recorded key's own
deviation from that curve: per note `recorded_cents` (instrument.json; `NoteSpec`, added at
`Tuning::as_recorded`), its takes' median measured tuning minus the curve, within ±100 cents
(`scale.recorded_cents`), 0 for keys between recorded ones; a register's at its sounding note.
Equal's offsets otherwise, the temperament key without effect. Over the instruments the
median deviation is 0.1–3.6 cents (the kalimba's 37, the đàn tranh's 10.5); the largest are
a piano's top keys (C8 +33 … +70 cents on three pianos, where the strings' fit has no
inharmonicity to go by) and the kalimba's tines (+64). The exports write it; the committed
instruments got it added in place (an export of the kalimba and of the psaltery afterwards
identical to them).

<a id="step-74"></a>
## 74. The marimba's and the kalimba's onset

As the balafon's (step 67) their modes were released
all at once, a step at the strike: in the first 0.2 ms after it the marimba reached 0.19 of
the strike's peak (its yarn mallet's recordings 0.07; in 1 ms frames already as recorded), the
kalimba 0.64 (0.02: its recordings rise over ~6 ms, the tine coming in at 7 ms, where the
engine's thumb click and tine start together). The metal bars (vibraphone, bells, crotales,
xylophone) are struck as sharply in the recordings (0.35–0.99) and keep their step; the
tubular bells (0.35 against 0.17) and the hand chimes (0.32 against 0.04, under their
clapper's click) were heard as they are. Now each mode starts from zero (`onset_s`): over
0.5 ms on the marimba (0.05; 1 ms put its first millisecond 7 dB under the recordings') and
4 ms on the kalimba (0.03; 8 ms only 2–4 dB nearer in the first frames, later behind at
7 ms); preferred by ear in an A/B against the step. The kalimba's ringing tines come last with
their own partners (`with_extras`: `Params::sympathetic` scales the modes from `halo_from`
on; behind them the tine's own partners followed "Ringing tines" and it no longer started
from zero off 100 %): at 100, 50 and 0 % its first 0.2 ms under a tenth of the peak (the
tests); the rattle per tine unchanged (−1.3 … +2.0 dB).

<a id="step-75"></a>
## 75. The sympathetic strings' drive fades

Half-pedalled Bach on Grand E (the first 10 s of the
Partita No. 1 in MAESTRO) clicked as the pedal moved: broadband bursts to 16 kHz, 0.2–0.4 s
long, at every landing and lift of the dampers; gone without the sympathetic resonance or
without the sustain pedal, there with knock, release and pedal noise off and with an instant
damper fall. The bank's share (the render minus the one without it) held them, 35–50 dB over
its 6–16 kHz level before, where its input, the strings' bridge force, held no transient (a
replay of the bank on its recorded input and key states matched it within −102 dB). The bank
drives a key's resonators only while its dampers are off and its own voice is silent, and it
switched that drive in one sample: as 63 dampers landed or lifted at once each resonator's
forced response to the sounding strings stopped or started and it rang on at its own
frequency. Kept driven throughout (dampers only adding their loss) the bursts were gone, but
a string sounding by its own voice must leave the bank, and driving resting strings would
change the pedal-up sound. So the drive fades in and out linearly sample by sample over
`resonance::DRIVE_RAMP_S` = 20 ms (`kernel::driven_ramp` on the lane chunks with a fading
resonator; in 16-sample steps it rang at their 3 kHz rate, worse than before): the excerpt's
bursts −4, +1, +2 dB (the one at 3.08 s +23, a strike's: +20 with every damper lifted), the
bank's level unchanged (−54.3 → −54.4 dB). 10 ms left +14 dB at a lift, 40 and 80 ms no
better than 20. The test: a legato line under a pedal between half and up, the bank's
largest 20 ms at 6–16 kHz re its median 33 dB switched, 10 faded. The cost within the
benchmark's noise (mean load 11.9–12.4 % pedalled, 4.0–4.2 % chords without pedal).
