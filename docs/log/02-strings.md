# Pianos, harpsichords and harps (steps 22–39)

The buff stop and the derived registers, Grands D to G and Upright C, the fits made robust (fade-
outs, contact sub-steps, register gains beyond the recorded keys), and the harps. Index of every
step: [README.md](README.md).

<a id="step-22"></a>
## 22. The buff stop

The buff stop (both harpsichords):
VCSL's English harpsichord (a Zuckermann kit) is recorded "Normal" and "Lute"; the "Lute"
samples are a buff (same level, t₋₂₀dB about 0.6× in the bass and 0.5× above, darker), not
a nasal lute row. `proto/fit_buff.py` measures each partial's extra decay Δσ(k) on the 26
keys recorded both ways (MIDI 34–88) and fits Δσ ≈ a + g·sin²(kπp): a pad touching the
string at p near the nut (p ≤ 0.15) plus a flat part a, the leather acting as a lossy end
(the recorded buff damps the lowest partials about evenly, which a point pad cannot: a
first fit without a reached only 0.77–0.94× in t₋₂₀dB). Curves over the keyboard, held
beyond the recorded keys: p 0.028 … 0.088 (rising as the strings shorten, a batten ~2 cm
from the nut; the comb counts on 5 keys, the rest is a smooth rise), a 0 … 2.5 /s, g 0.6 …
7.8 /s. `export.py` writes them as an optional top-level `"buff"` block (register 0) for
`harpsichord` and `flemish8`; the engine compiles σ_buff = a + g·sin²(kπp) per partial
(`Physics::buff_sigma`). `Params.buff` acts at a pluck: `Voice::set_buff` shifts the free
and damped λ of every mode of that string (ringing ones included) in place, allocation-free;
a switch while notes ring waits for their next pluck. The 4′, release noise, knock and
calibration are unchanged; buff off renders bit-identically. Validation
(`proto/buff_check.py`, single notes, Buff/Normal per band, against the English keys'
range): t₋₂₀dB ratio Unk 0.62 / 0.50 / 0.65, Flemish 0.78 / 0.66 / 0.80 (bass / middle /
treble; English 0.60 / 0.50 / 0.46); median Δσ (partials 1–12) Unk 1.4 / 2.6 / 1.0 /s,
Flemish 0.8 / 2.3 / 2.3 /s (English 1.1 / 2.4 / 3.5); within the range except Unk's
treble Δσ and the Flemish treble t₋₂₀dB. Both come from the room: its tail (fitted from
Unk's and the Flemish recordings, another room than the English one) does not shorten with
the buff, and the short buffed treble notes sink into it sooner. With the room off all
bands lie within the range (treble Δσ Unk 3.9, Flemish 3.0 /s); with only the
sympathetic strings off Unk's treble Δσ stays at 1.0 /s. The Flemish 4′, unbuffed and free
while its key is held, adds a little (treble t₋₂₀dB 0.80 → 0.74 without sympathetic
resonance), as it would on the instrument. Not tuned for.
Completing it: the harpsichords damp every key (`damper.has_damper(note, plucked)`; a jack
carries a damper on every key, also on 89–108 above the compass, which had rung freely
as on a piano), and the sympathetic resonators of buffed strings take the pads' loss too
(`Sympathetic::buff_sigma`; `Bank::set_buff` switches their poles at once on a toggle,
ringing ones included: the pads rest on every string, the resonators are quiet enough
for an instant change, and unbuffed instruments render bit-identically).

<a id="step-23"></a>
## 23. Harpsichord B's second 8′ and lute stop

Harpsichord B's second 8′ and lute stop: no recordings, so both
are derived from the fitted 8′ I as distances from the nut (β·L of each key's speaking
length L and pluck point β; constants in `export.py`, starting values tuned by ear). The
8′ II (register 2, `export.py --add-choir`): the 8′ I's strings and quill, plucked one jack
row nearer the nut (β_II = max(β_I − D_ROW/L, 0.02), D_ROW = 1.5 cm), each key retuned by a
seeded ±1.5 cents so the two 8′s beat, level ROW2_DB = 0 dB, no knock of its own. The lute:
a second jack row on the 8′ II strings, D_LUTE = 2.5 cm from the nut (β = D_LUTE/L, at most
0.6·β_II on the short treble strings, at least 0.02), exported as a top-level `"lute"`
block (register, β per key, gain dB), loaded into `NoteSpec::lute`. `Params.lute`: at a
note-on the lute register's strings are plucked by `Voice::pluck_with` at the lute's point
instead of their own, with or without stop bit 2 (with both, the lute wins: one row plucks
those strings); a switch acts on the next plucks, release noise follows as register 2. The
plugin's "8′ II" and "Lute" toggles exclude each other in the UI. Cost: 88 more voices
(sounding only when plucked) and register 2's sympathetic resonators; the bench's
pedalled 32 notes/s at 64-frame buffers: load mean 12 % (8′), 15 % (8′ + 4′), 20 % (all three). All other
instruments render bit-identically; the Flemish with its old stops changes only through
the added strings ringing sympathetically (identical with sympathetic resonance off).
Listening: `proto/registers_demo.py` → `proto/out/registers/` (the lute alone is about
3 dB louder in RMS than the 8′ alone, its upper partials stronger).

<a id="step-24"></a>
## 24. Grand D

Grand D (`grand_e`, VCSL "Grand Piano, S Model B 1895"): an 1895 grand, 42 notes
in whole tones, 3 layers (vl2/vl3/vl4 as pp/mf/ff), pedal-up sustains; the samples are
**normalized**, so it is fitted like Salamander (no level target, no `register.py --fit`):
the dynamic range comes from the model. Felt fit: speeds pp 0.6, mf 2.1, ff 5.0 m/s (the
Iowa piano, the same model: 0.6 / 2.1 / 7.0), mean error 5.4 dB (Salamander 5.1; the
level-targeted sets 2.6–3.1). Room decay: the between-partials T60 reads ~40 s in every band
(no consistent estimate), `room_t60`'s fallback gives 0.3–1.2 s. Soundboard residual
12.3 dB (others 10.3–14.2). Rendered (room off) against the recordings: C4's spectral
centroid matches (438/524/615 Hz vs 319/542/594), C2 is darker (173–468 vs 315–624), C6 too
bright (1351–2013 vs 973–1092): C6's fundamental lands in a hole of the synthetic board.
Found on the way, not Grand D's alone: the model's treble fundamentals jump note to note
(notes 72–96, mf, rms of the adjacent differences: Grand D 10.3 dB, Grand K 9.0, Iowa 7.9,
Salamander 5.6) more than a real piano's (the Iowa recordings: 5.5 dB, max 12.8 vs ~20); and
failed partial detections (B confidence < 0.15) stay in the analyses at low weight.

<a id="step-25"></a>
## 25. Fade-outs and the noise floor

Fade-outs and the noise floor (`analyze.fade_cut_s`): the analysis read each partial's
noise floor from the file's last second, and most sets fade their files out over the
last ~2 s (Grand D, Salamander and the harpsichords; short files over ~25 %; Salamander's
fixed 0.6 s cut was too short). Inside the fade the floor read 20–40 dB low, the noise
counted as decay, and the coupling fit answered with strings of almost no internal loss
(Grand D: β1 0 in the treble): aftersound plateaus and upper partials that rang on (the
"resonances" and the flat bands in the spectrogram). The fade is now found per file
(noise in 10–16 kHz falling 3 dB below its plateau), the cut kept in the analysis and
applied by `body_decay` and `fit_coupling` too. Refit of every set with a fade: coupling
errors Grand D 12.4 → 7.5 dB, Harpsichord A 10.4 → 8.0, Flemish 4′ 11.5 → 8.7; held
notes against the recordings (levels 1–3 s re peak, partials 1–10) rms 12.6 → 11.9 dB
over the other instruments, Grand D 17.1 → 14.6. The damper loss (from Salamander's
release samples, which fade too) lost 341 of 1606 partials that only the fade had made
look usable. Not refitted: the buff (fitted as a ratio of two equally faded sets).
Register balance of normalized sets (`datasets.LEVEL_REFERENCE`): Grand D's register fit
follows the Iowa recordings (the same model); without it the bass sat 16 dB (26 dB
after the fade fix) too low re the treble; now bass − treble +7.8 dB (Iowa's recordings
+11.4, Grand D's normalized samples +13.4). Open: C6 decays 12–20 dB too fast on Grand D,
Iowa and Knight; Grand D's felt fit in the bass (error 13–14 dB at notes 24/30).

<a id="step-26"></a>
## 26. Upright C

Upright C (`livingroom`, KeyPleezer's LivingRoom Upright, Micro SFZ; audio CC BY-SA 4.0,
so its export is CC BY-SA 4.0 too: `instruments/livingroom/LICENSE.md`): a large vintage
Swedish upright, 33 notes, p and f (as mf/ff) with real levels, full-length tails, no
fades, no release samples. A generic SFZ loader (`datasets.sfz_files`: the regions that
play with the pedal up). Fits: felt error 2.7 dB (speeds pp 1.13 extrapolated, mf 2.1,
ff 5.0 m/s), soundboard residual 11.5 dB, coupling 6.6 dB, register level error 2.8 dB.
Against the recordings: bass − treble +8.5 dB (recordings +10.2), held notes rms 12.4 dB,
no excess sustain (+0.7 dB). Condition: phantom and hammer wear as on the other uprights
(C6's phantom +11 dB re the note, Knight +5). Seen on the way: Upright Y's model has its
bass 11 dB above its recordings' balance (+17.6 vs +6.5 dB) despite its register fit.

<a id="step-27"></a>
## 27. Grand E

Grand E (`piano162`, Ivy Audio's Piano in 162, SFZ edition): every key,
5 layers × 2 takes, pedal up and down, close and ambient microphones, natural tails to
the floor, no fades. Each layer and take was recorded at a gain of its own (the noise
floor 4–12 dB above the mf layer's, the same in every octave band and within ±0.7 dB
from note to note); the loader undoes it per layer (`datasets.IVY162_GAIN_DB`, take 1;
`analyze` and every reader of recorded levels take a sample's `gain_db`), so the set
keeps real levels: pp −21, mf 0, f +9 dB at the attack. Fitted on pp/mf/f of the close,
pedal-up first takes (264 files, 11 top pp notes failed). Fits: speeds pp 0.4, mf 3.0,
ff 7.0 m/s, felt error 4.0 dB (a hard treble felt, 32–128 N at 0.5 mm from C6 up),
soundboard residual 10.6 dB, coupling 7.3 dB, register level error 4.4/4.5 dB (mf/ff).
Against the recordings: register balance within ±2 dB but C6–G7 −4.8 dB, held notes rms
10.4 dB, no excess sustain (−1.1 dB).

<a id="step-28"></a>
## 28. Contact sub-steps

Contact sub-steps (`string_model.contact_substeps`, `voice.rs` `contact_ratio`): Grand E's
first fit rendered NaN at C7 (its hard treble felt) and the register fit's smoothing
spread it over every register gain. The explicit contact step is stable while
dt·√Λ/2 < 1 (Λ: the felt's stiffness at the compression the hammer's energy reaches on
rigid strings, over the hammer mass and the string's mass at the strike point), but the
contact already grew without bound from about 0.8: Iowa's C7 at ff peaked at 330 N
where its neighbors peak at 100–150, and the hardest felt (+12 dB) made Salamander's top
keys up to 15 dB louder (the backoff only caught outright divergence). A contact over
CONTACT_RATIO_MAX 0.5 now runs ⌈ratio/0.5⌉ sub-steps per sample (its own resonator
coefficients, the amplitudes taken back to the last sample − 1); the rest keep one step
and are unchanged. At ff the old fits rendered unstable treble contacts on Iowa,
Grand K and Knight (C7 ratio 0.83) and on Upright Y from C6 up (1.0): their treble fits
were made against inflated forces and are worth a refit. `register.py --fit` stops on a
non-finite level instead of writing it.

<a id="step-29"></a>
## 29. Refit of every piano after the contact sub-steps

Refit of every piano after the contact sub-steps (step 28; the harpsichords pluck and are
unchanged), 22.7 min for all eight in two lanes (with the faster fits, IDEAS 26). Speeds moved
where the felt fit's landscape is flat: Iowa 0.6/2.1/7.0 → 0.4/1.1/5.0 m/s at the same
2.6 dB (Grand D takes them over: its felt error 5.4 → 6.1 dB), Knight ff 3.5 → 7.0 m/s.
Against the recordings (engine demo, register medians per octave; held notes): Knight's
register rms 4.1 → 2.5 dB (worst octave 10.7 → 6.2), the others within ±0.5 dB of before;
Salamander's held notes +0.8 → +2.9 dB too long; Upright Y's bass-heavy balance did not
come from the contact (unchanged). Golden values (render presets, the C4 baseline)
regenerated.

<a id="step-30"></a>
## 30. Grand F

Grand F (`headroom`, Bengt Nilsson's Headroom Piano; CC BY 4.0, credited in
`instruments/headroom/LICENSE.md`): every third key, 5 layers, close and Decca tree
microphones, one take, no pedal-down or release samples. One gain for every layer (the
noise floor alike); the treble session from A5 up has less rumble, not another gain.
Layers 1, 2, 5 as pp/mf/ff (the engine plays mf at MIDI 60, where layer 2 starts).
Fitted on the close microphones (90 files, 3 top pp failed), chain 3.6 min. Fits: speeds
pp 0.6, mf 1.1, ff 3.5 m/s, felt error 3.8 dB, soundboard residual 12.4 dB, coupling
6.3 dB, register level error 3.1/3.2 dB. Against the recordings: register balance rms
1.7 dB (worst octave 3.5 dB), held notes rms 9.3 dB, no excess sustain (−0.8 dB), the
model no more uneven than the recordings (2.5–2.8 vs 3.2–3.3 dB). Seen on the way: Grand
E's "mf" layer is velocity 81–101 but plays at MIDI 60 (its mf speed 3.0 m/s, 1.1 on the
other grands).

<a id="step-31"></a>
## 31. Grand E's mf

Grand E's mf (step 27): the engine plays pp, mf and ff at MIDI 14, 60 and 124, and Grand E
took Ivy's mf layer (velocity 81–101) for 60: its level rose 21 dB up to MIDI 60 and 8 dB
above (mf at 3.0 m/s). Ivy's p (34–64, the layer that holds 60) is only 5 dB above pp and
25 dB below f: flat to 60, +29 dB above. Its mp (65–80) rises evenly: speeds 0.4/1.1/7.0
m/s as the other grands, −12 … +19 dB re MIDI 60 (Iowa −11 … +15), register balance rms
2.4 → 2.1 dB, felt error 4.0 → 4.6 dB.

<a id="step-32"></a>
## 32. Grand G

Grand G (`osiris`, the Osiris Piano, CC0): a worn grand recorded softly with the soft
pedal down, the white keys, 2 layers ~5 dB apart; microphone A, unfiltered, pedal up.
Fitted freely its felt jumped between 0.5 and 128 N from note to note (two soft layers,
1.5 and 2.5 m/s, barely constrain it) and the register correction swung 35 dB: it takes
Grand F's felt (`datasets.FELT_REFERENCE`, `fit_profile_felt.reference_felts`; the speeds
are fitted alone). Speeds mf 3.0, ff 5.0 m/s, felt error 4.8 dB, coupling 5.6 dB,
register level error 3.4/3.0 dB; against the recordings register rms 2.1 dB, held notes
rms 13.3 dB (C6 11.5 dB short, as on Grand D, Iowa and Knight). Its range over the
velocity is narrow (−3.4 … +4.5 dB re MIDI 60): the recordings' own 5 dB between layers.
Calibrated on its ff like the others, every mf came out as loud as their ff (a piece
peaked at +2.4 dBFS): `output_gain_db` (instrument.json, `datasets.OUTPUT_GAIN_DB`; 0 for
the rest) puts it 10 dB down, its mf with the other grands' (mf − ff median −14.8 dB, its
own −4.4).

<a id="step-33"></a>
## 33. Harpsichords C and D

Harpsichords C and D (`french`, `italian`: VCSL's French and Italian harpsichords, CC0;
one 8′ each, 28 notes D1–C6 and 32 notes F♯1–B5, one layer, release samples, the buff
stop as on the other two). The harpsichord chain, about 2 min each. Fits: pluck-point
notches on 25/28 and 29/32 notes, coupling 6.4 and 6.3 dB (Harpsichords A, B: 7.4–8.7),
the aftersound beats ~2 dB in the recordings and 0.2–0.5 dB in the model (single strings;
the same on A and B). Two changes on the way, both for every instrument:
- Register gain beyond the recorded keys (`register.register_gain`): it was held at the
  outermost recorded note, so the keys above a short range grew louder by the model's
  own trend (the Italian 10–20 dB above B5 at C7–C8). It now carries on with the slope
  of the outermost recorded octave (at most 1 dB/semitone): register balance re the
  recordings French 5.0 → 2.2 dB rms, Italian 7.8 → 4.0 (its top keys still +10 dB:
  the model rises above B5 more steeply than its last octave says). Takes effect for
  the others at their next register fit.
- The output calibration on the recorded keys (`export.calibration_gain_db`, in
  `output_gain_db`): the engine's median ff peak over all 88 keys counted the
  extrapolated ones too. Re-exported everything: the pianos move by −0.95 (Upright Y) …
  +0.13 dB, Harpsichord A +0.44, the French −2.3, the Italian +0.9 dB.
In a piece the French plays ~3 dB and the Italian ~2 dB louder than Harpsichord A (their
middle registers are louder re their range, as recorded); the Italian peaks at +1 dBFS
with the piece's notes above its range.

<a id="step-34"></a>
## 34. Harp A

Harp A (`harp`: VCSL's Concert Harp, CC0; 23 strings E1–F7 in thirds, f and mf as ff and
mf, no SFZ: `datasets.harp_files` reads the file names). The harpsichord chain without
the release noise, about 3 min. Design `harp` (`scale.py`, assumed): one string per key
(the pedals' retuning of 47 strings onto 88 keys is not modeled), G7 0.075 m ×1.6 per
octave, 350 N in the wound bass to 60 N in the treble, a 0.35 m² soundboard; the finger
a pluck with its lowpass at 4 partials, pulled 3 mm. What is new:
- No dampers: a key's release is the player's hand on the string (`damper.hand_sigma`,
  120 dB/s flat over frequency, assumed), the pedal down lets every string ring; no
  mechanism comes back up, so the release noise has no impacts (and its control no
  effect: `INACTIVE_HARP`, the plugin grays it out). No buff stop.
- Two layers fit the pluck's velocity constants (`fit_pluck.fit_velocity`, the
  harpsichords keep Penttinen's): 8.8 dB and 0.75 octave of lowpass opening per step
  (the recordings grow 6.3 dB in RMS and 8.3 in peak from mf to ff). The analysis's
  partial levels are re each file's peak (summed without it, mf and ff had the same
  energy). The two layers cannot place the lowpass (at 1…32 partials, a pad width or
  a fixed frequency the fit's error is 4.96…5.14 dB): the soundboard fit takes it up.
- The pluck point searched in 0.3–0.5 (`fit_pluck.BETA_RANGE`): on the full grid C3 and
  F4 took the comb's ambiguous fractions and pulled the curve to 0.12 around B2; now
  0.43 in the bass to 0.47 from the middle up (odd partials 13–23 dB over the even ones
  at D4–C5).
- Sparse sets fit the coupling on their recorded notes (`fit_coupling.default_notes`:
  the default every 3rd key met only 45, 48, 69, 72 of the thirds, and the bass anchor's
  β1 went to 10.9); Upright Y (6 notes met) keeps the default. Loss fit: 3 partial
  decays per note suffice for the harp (`scale.MIN_LOSS_ROWS`; the treble has 1–3
  partials), the soundboard's low edge at 8 rows (146 → 87 Hz).
- The polarizations beat in the coupling fit's depth term (`fit_coupling.model_depths`:
  they meet at the microphone; added in power, a single string never beat and the
  model's depth stayed at 0.01 dB against the recordings' 2.4; the harpsichords' 0.2–0.5
  against 2.0–2.9 is the same flaw and changes at their next coupling fit). Now 2.4 dB.
- The prompt sound (`fit_coupling.early_excess`, weight `W_EARLY`, the harp only): from
  D4 up the fundamentals start 9–24 dB above their aftersound's line and fall at 40–110
  dB/s for 0.2 s (one polarization coupled strongly to the soundboard); the EDCs start
  50 ms in, where the aftersound outweighs it, and both polarizations decayed slowly.
  The term moves the fit part of the way (D4's first 0.3 s now as recorded; C5's prompt
  still too slow; middle-register T30 2.3 → 1.7× the recordings'): the slow
  polarization's loss is tied to the fast one's through b1′.
- `output_gain_db` −6 (`datasets.OUTPUT_GAIN_DB`): its ff peaks span 24 dB as recorded
  (bass −29, middle −5…−8 dBFS at the common calibration), the middle's chords reached
  +0.2 dBFS in the demo.
Engine against the 42 recorded takes (single notes, first 0.5 s): level bass +1.0,
middle +0.4, treble −3.0 dB (per take ±10, the recordings' own scatter); highs ≥ 4 kHz
+4.2, +0.5, −6.8 dB.
The body (`mic::Body::Harp`, `"body": "harp"`; it had been a grand: a horizontal board,
a lid, the grand's microphone positions). Assumed geometry of a pedal harp: the
soundboard a 1.4 m strip leaning back 30° towards the player, its foot 0.2 m above the
floor; the strings drive it along its center line (C1 near the foot, G7 near the top);
its outer face radiates (one-sided), its inner face through the sound holes in the
soundbox's back towards the player (−6 dB, lowpass 600 Hz); the floor the one
reflector; no lid (the plugin grays it out, the setup's lid is held at 1). Positions:
Player at the harpist's ears (left of the soundbox's top, behind it, 1.25 m up: the
holes directly, the board diffracted), Audience 3.5 m in front, Close 1 a pair 0.4 m in
front of the board, Close 2 0.5 m behind the sound holes (level: along the box's normal
the pair went into the floor), the close pairs spread along the board. The UI draws it
from above and from the side (`PeMicGeometry.front_axis`, `outline_front`), where the
pair is dragged in that plane. Demo levels re the recording's position: Player +1.1,
Audience +0.2, Close 1 −2.5, Close 2 +2.4 dB (the grand's and the upright's −3…+1).

<a id="step-35"></a>
## 35. The harpsichords refitted from the coupling fit on

The harpsichords refitted from the coupling fit on (step 34's beating of single strings):
coupling error Harpsichord A 8.04 → 6.20 dB, French 6.44 → 5.05, Italian 6.33 → 5.23,
Flemish 8′ 7.40 → 5.80, 4′ 8.72 → 6.69; levels within 1 dB. Preferred by ear in an A/B
of the demos. What the depth term measured: the std of a partial's envelope around a
quadratic trend also counts a two-stage decay's bend, not only beats; the recordings'
2.0–2.9 dB are mostly that (their broadband envelope modulates 0.3–0.75 dB, 0.3–3 s).
The fit matched the bend (`horiz` at its bound 0.5 on Harpsichord A and the Flemish 8′),
the engine's broadband modulation moved 0.18–1.27 dB (French closer, Italian 1.25 → 0.24).

<a id="step-36"></a>
## 36. The harp's polarizations measured

The harp's polarizations measured (`fit_polarizations.py`, the `polarizations` block;
step 34's prompt-sound term superseded). Its envelopes are two polarizations' coherent
sum: per partial of the f layer, 20 ms after the onset to −50 dB, fitted as
|e^(−σ_f·t) + 10^(−g/20)·e^(−σ_s·t)·e^(j(2π·Δf·t + φ))| (96 starts; synthetic cases
recovered). C5's fundamental "dropping 24 dB in 0.24 s" is a beat's first minimum: two
components 1.6 dB apart, 2.3 Hz apart, decaying at 13 and 11 dB/s. An earlier try as
two decays added in power read the minima as a 50–300 dB/s first stage and made it worse
(coupling error 16.6 dB). Fundamentals: 11–26 and 3–11 dB/s, the slower 10–16 dB down,
beating at 2–8 cents. Per note, in the model's terms (string_model.free_modes): b1, b3
from the slower rates, κ = σ_f − (b1 + b3·ω²) (the coupling fit's κ = kf·b1′ with one kf),
horiz_gain = 10^(−g/20) and the polarizations' detuning from the beat (both weighted
1/n², a running median over ±5 semitones, then smoothed: they scatter note to note);
the coupling fit keeps them and fits the reactive part. C4: κ 2.3/s, horiz 0.15,
6 cents (the coupling fit had 1.5 cents for all). Engine against the 42 takes (bass /
middle / treble): T30 re the recordings 1.23 / 1.73 / 1.37 → 1.13 / 1.16 / 1.02;
broadband modulation 0.1 / 0.7 / 0.3 → 0.2 / 2.0 / 1.2 dB (recordings 0.7 / 2.7 /
2.4); levels within 2.3 dB. 1:10 min for the step (a pool over the takes).

<a id="step-37"></a>
## 37. Harp B

Harp B (`leverharp`: the Etherealwinds Harp II CE, Jordi Francis's 34-string Iona Celtic
harp sampled with Versilian Studios; chromatic C2–A6, v2/v3 as mf/ff, the first round
robin; `datasets.leverharp_files`, keys an octave above the file names). The harp chain
(3 min). Design `leverharp` (assumed): A6 0.09 m, ×1.68 per octave (C2 1.06 m), 200 → 60 N,
a ~0.2 m² board; the engine's body is the concert harp's at 3/4 (`mic::Body::LeverHarp`,
`HarpDims::LEVER`: board, box, holes, column and the close pairs' spread scaled, the
bridge from C2 at the foot to A6 at the top; the player's ears stay at a seated 1.25 m).
Fits: the pluck point on the full grid (bass 0.39, A3 0.47, treble 0.35; 26 of 35 notes
with the comb), velocity 14.3 dB per step (its soft layer: v3 a median 14.8 dB over v2),
polarizations on 58 notes (two components on 4–6 of 6 partials up to C5). Engine against
the 116 takes: levels per octave within 1.6 dB (single takes ±4–6), T30 re the recordings
1.03 / 1.13 / 0.83, broadband modulation 0.8 / 2.1 / 2.2 dB (recordings 0.9 / 1.4 / 2.3);
no output gain needed (its demo peaks at −4.4 dBFS).
Register gain beyond the recorded keys (`register.register_gain`, for every instrument at
its next register fit): the recordings' level trend over their last octave minus the
model's own change there (`register.model_levels` renders those keys). Carrying the gain's
slope on (step 33) assumed the model flat beyond the range; the lever harp's gain rose
8.5 dB over its top six keys (the model falling there), and above A6 its ff peaks climbed
to +10 dBFS at C8; now −13…−6 dBFS, as its top recorded keys. The Italian's top keys
(still +10 dB, step 33) should follow at its next register fit.

<a id="step-38"></a>
## 38. Register fits rerun on every instrument

Register fits rerun on every instrument for step 37's extrapolation, with one more rule:
beyond a recorded range the extrapolated level holds or falls outward, never rises (the
recordings' top-octave trend rose on the concert harp, Iowa and the LivingRoom and put
their top keys 5–10 dB over their last recorded note). Changes ≥ 1 dB are almost all
beyond the fitted keys (the ones recorded at mf and ff; Iowa's and the LivingRoom's end
below C8); a few keys within 1–3 semitones of the edge follow the smoothed curve (French,
Italian, Flemish), and the concert harp's bass at keys 27–30 is 2–3 dB quieter.
The loudest of the top five keys re the four below, before → after: Harp A −9 → −4 dB, Iowa
−12 → −8, the Italian +2 → −3, the Flemish +2 → −1, the French −5 → −2, the rest within
1 dB. `mic_custom.rs`'s pinned renders regenerated (Knight's energy +0.1 %).
The plugin's instrument menu groups the instruments by type (`instrumentGroups`: grand
pianos, upright pianos, harpsichords, harps); a group opens its instruments beside the
list (`submenuRect`), on hover or click; the host's parameter keeps the flat list.

<a id="step-39"></a>
## 39. Harpsichord C's bell-like bass: a partial near its prediction

Harpsichord C's lowest keys sounded bell-like, with few partials. Its D1 (the lowest
recording, a weak fundamental) had been analyzed at f0 −182 cents and B 0.03: the
global search found 36.7 Hz, but the refinement took a peak 8 % below as the fundamental
and fitted the partials to it. Now a peak further than PEAK_TOL (4 % of f0) from its
prediction is not that partial (`analyze.estimate_partials`): D1 36.6 Hz, B 3.3e-5. The
keys below it inherited the error through the smoothing's extrapolation (A0 B 14,
−328 cents); beyond the recorded keys tuning and B now carry on the edge's trend at
most 2 cents and 0.1 (log B) per semitone (`scale.TUNING_SLOPE_MAX`, `LOGB_SLOPE_MAX`;
unlimited, the lever harp's A0 was +106 cents, Grand D's top keys +46). The tolerance is
of the partial's own frequency (n·f0): of f0 alone, the treble's higher partials, their
B still rough, failed in 2–8 more takes per piano set. Harpsichord C refitted; every
other set reanalyzed and re-exported, its fits kept (takes with a residual ≥ 8 cents or
40 cents off their neighbors: Piano in 162 30 → 17, Grand K 12 → 3, Salamander 21 → 17,
Iowa 11 → 8, Grand D 9 → 6; 1–3 bad takes per set now errors, left out). Tuning moved ≥ 5
cents only at the extrapolated and outermost keys (the lever harp's A0 −82, Grand K's
+46, Grand D's C8 +40, Harp A's C8 +33). Salamander's D♯4 v16 now 311.1 Hz (was 312.6,
+8 cents): the pinned renders (`test_pluck.py`, `mic_custom.rs`) regenerated.
