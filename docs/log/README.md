# Design log

What changed, step by step, with why and the measured numbers: the project's memory of its
decisions. Step numbers are stable and cited across the code and docs ("design log step 113").
How the system works today is in [../architecture.md](../architecture.md); this log says how it got
there. "IDEAS n" in older steps is an item of the former `IDEAS.md`, now [../../ROADMAP.md](../../ROADMAP.md).

A new step goes at the end of the last file, numbered on: what changed, why, the measured numbers.

## Names in this log

The steps keep the names they were written with. Step 198 renamed the instruments' directories and
re-lettered the grands:

| Directory then | Now | Letter then | Now |
|---|---|---|---|
| `salamander` | `grand_a` | Grand A | Grand A |
| `iowa` | `grand_b` | Grand B | Grand B |
| `headroom` | `grand_c` | Grand F | Grand C |
| `piano162` | `grand_d` | Grand E | Grand D |
| (a maker's name; the log says `grand_e`) | `grand_e` | Grand D | Grand E |
| `grandk` | `grand_f` | Grand C | Grand F |
| `osiris` | `grand_g` | Grand G | Grand G |
| `knight` | `upright_a` | Upright A | Upright A |
| `uprighty` | `upright_b` | Upright B | Upright B |
| `livingroom` | `upright_c` | Upright C | Upright C |
| `baillon` | `harpsichord_e` | Harpsichord E | Harpsichord E |
| `barton` | `harpsichord_f` | Harpsichord F | Harpsichord F |
| `salamander2` | `registers_fixture` | (a test fixture) | |

## [The engine, the plugin and the microphones](01-engine.md)

- [1. Export and an offline render](01-engine.md#step-1): the export script and the Rust engine rendering the demo, checked against the prototype
- [2. The benchmark: float32 strings, partitioned convolution](01-engine.md#step-2): float32 one-poles, pruning at −80 dB, 64-sample latency: 12 % of a core
- [3. The DPF wrapper; the noises synthesized at load](01-engine.md#step-3): C ABI, worker-thread loading, the room separate; knock, release and room synthesized at load (25 → 3 MB)
- [4. Pedal noise](01-engine.md#step-4): the thump and the strings ringing as the dampers lift
- [5. Dampers continuous](01-engine.md#step-5): a damper engagement per key: half-pedaling, the lift kick
- [6. Physics compiler in Rust](01-engine.md#step-6): partials, admittance and unison modes derived at load; instruments 0.5 MB
- [7. Condition](01-engine.md#step-7): a wear macro with per-aspect offsets, live and compiled
- [8. Optional defects](01-engine.md#step-8): hammer grooves, damper buzz, rattle
- [9. False beats and phantom partials](01-engine.md#step-9): split partials and the bridge force's square
- [10. Microphone positions, step 1](01-engine.md#step-10): direct sound, floor and lid reflections; Player and Audience
- [11. Microphone positions, step 2](01-engine.md#step-11): eight bridge zones, the board's response from its modes, spacing and distance
- [12. Microphone positions, step 3](01-engine.md#step-12): a coincidence-cone directivity per path
- [13. Handover](01-engine.md#step-13): a rebuilt engine takes over the sounding notes; crossfaded radiation
- [14. Close microphone positions](01-engine.md#step-14): pairs above the hammers and over the tail
- [15. A third instrument, Upright A](01-engine.md#step-15): VCSL's Knight; any number of dynamics, upright design curves
- [16. Microphone positions for the upright](01-engine.md#step-16): the body as data (`mic::Body`); the upright against its wall
- [17. Grand C and Upright B](01-engine.md#step-17): VCSL's grand K and upright Y; sparse and level-keeping sets
- [18. A harpsichord](01-engine.md#step-18): the pluck, its fit from the notch pattern, export format 5
- [19. Temperaments](01-engine.md#step-19): meantones and well-temperaments from their definitions
- [20. Registers: Harpsichord B](01-engine.md#step-20): an 8′ and a 4′ as registers; live stops, export format 6
- [21. Plugin UI](01-engine.md#step-21): NanoVG UI, the parameter table, the drawing from the engine's placement

## [Pianos, harpsichords and harps](02-strings.md)

- [22. The buff stop](02-strings.md#step-22): fitted from VCSL's English harpsichord with and without its buff
- [23. Harpsichord B's second 8′ and lute stop](02-strings.md#step-23): derived from the 8′ I: a jack row nearer the nut, a lute row
- [24. Grand D](02-strings.md#step-24): VCSL's 1895 grand "S Model B": normalized samples, the range from the model
- [25. Fade-outs and the noise floor](02-strings.md#step-25): the files' fade-outs found per file; every set refitted
- [26. Upright C](02-strings.md#step-26): KeyPleezer's LivingRoom upright, CC BY-SA 4.0
- [27. Grand E](02-strings.md#step-27): Ivy Audio's Piano in 162: every key, the layers' gains undone
- [28. Contact sub-steps](02-strings.md#step-28): stable hammer contact for hard treble felts
- [29. Refit of every piano after the contact sub-steps](02-strings.md#step-29): speeds moved where the felt fit is flat; golden values regenerated
- [30. Grand F](02-strings.md#step-30): Bengt Nilsson's Headroom Piano (CC BY 4.0)
- [31. Grand E's mf](02-strings.md#step-31): Ivy's mp layer as mf: an even rise over the velocity
- [32. Grand G](02-strings.md#step-32): the Osiris Piano: soft and worn, Grand F's felt, an output gain
- [33. Harpsichords C and D](02-strings.md#step-33): VCSL's French and Italian; the register gain carried on, the calibration on recorded keys
- [34. Harp A](02-strings.md#step-34): no dampers, the pluck's velocity fit, the polarizations beating, the harp body
- [35. The harpsichords refitted from the coupling fit on](02-strings.md#step-35): the beating single strings: coupling errors 1.2–2 dB lower
- [36. The harp's polarizations measured](02-strings.md#step-36): two polarizations' coherent sum fitted per partial
- [37. Harp B](02-strings.md#step-37): the Etherealwinds Celtic harp; the model's trend beyond the range
- [38. Register fits rerun on every instrument](02-strings.md#step-38): levels never rise beyond a recorded range; the menu groups the instruments
- [39. Harpsichord C's bell-like bass: a partial near its prediction](02-strings.md#step-39): a peak far from its prediction is not that partial; tuning slopes limited

## [Bars, guitars, zithers and the clavichord](03-percussion-and-more.md)

- [40. The vibraphone](03-percussion-and-more.md#step-40): the first instrument without strings: modes, a mallet, the bar frame
- [41. The vibraphone's motor, harmonics and the bars ringing along](03-percussion-and-more.md#step-41): the motor's discs, the fundamental's harmonics; bars ringing along inaudible
- [42. The marimba](03-percussion-and-more.md#step-42): three mallets, a fourth mode, the Mallet menu
- [43. The xylophone](03-percussion-and-more.md#step-43): untuned treble bars; harmonics and fast decays measured carefully
- [44. The bells](03-percussion-and-more.md#step-44): eight drifting modes, plastic and brass mallets
- [45. The crotales](03-percussion-and-more.md#step-45): bronze discs, eight modes, one mallet
- [46. The bars' stereo image](03-percussion-and-more.md#step-46): a frame's range over the zones, one zone per key; Spread all keys; the recorded range
- [47. The plugin UI as tabs](03-percussion-and-more.md#step-47): five tabs of compact rows, inactive controls hidden, tooltips
- [48. The vibraphone played from a keyboard](03-percussion-and-more.md#step-48): a held key lifts the damper bar
- [49. The guitar and measured board modes](03-percussion-and-more.md#step-49): Iowa's classical guitar; the top's 224 Hz mode measured
- [50. The body response follows H](03-percussion-and-more.md#step-50): the modes at flat weights and H one filter after them; every instrument refitted
- [51. The pedal's strings leave their dampers from rest](03-percussion-and-more.md#step-51): the ring's kick from rest: no click at a pedal press
- [52. Panic](03-percussion-and-more.md#step-52): silence within 20 ms; CC120 and CC123 as specified
- [53. Copy CLI](03-percussion-and-more.md#step-53): a `midi2wav` line for the current settings; one parameter mapping for DSP and UI
- [54. The header menu](03-percussion-and-more.md#step-54): zoom, Copy CLI and Reset in one menu; the header's layout faults
- [55. The radiation's draw](03-percussion-and-more.md#step-55): the radiation drawn so that no fundamental sits in a deep notch
- [56. The clavichord](03-percussion-and-more.md#step-56): Edinburgh's clavichord: the tangent, the Bebung, an assumed room
- [57. The tubular bells](03-percussion-and-more.md#step-57): VCSL's tubes: the strike note, split mode pairs, an assumed damper
- [58. The hand chimes](03-percussion-and-more.md#step-58): 19 tubes, one dynamic, an assumed clapper and damper
- [59. The kalimba](03-percussion-and-more.md#step-59): cantilever tines, the other tines ringing along, the thumb's click
- [60. The balafon](03-percussion-and-more.md#step-60): six bars over gourds, three mallets, the membranes' buzz
- [61. The psaltery and the strumstick](03-percussion-and-more.md#step-61): the harps' chain on new designs; a dry studio assumed
- [62. The đàn tranh, and the pitch wheel](03-percussion-and-more.md#step-62): equal temperament on movable bridges; the pitch wheel bends the strings
- [63. A sparse board's first mode](03-percussion-and-more.md#step-63): the board's grid held at 20 Hz; a mode at 0 Hz refused
- [64. The hand chimes' strike, and VCSL's bars in a dry room](03-percussion-and-more.md#step-64): the strike's short modes and swell; VCSL's dry studio for its bars
- [65. The balafon, less metallic](03-percussion-and-more.md#step-65): upper modes' levels from the spectrum, a flat body, the membrane fitted
- [66. The kalimba's ringing tines as a control, and its drawing](03-percussion-and-more.md#step-66): "Ringing tines" on the sympathetic control; a kalimba drawing
- [67. The balafon's strike](03-percussion-and-more.md#step-67): modes starting from zero over the mallet's contact; Mallet click
- [68. A control for the balafon's buzz](03-percussion-and-more.md#step-68): "Gourd buzz" scales the membrane and the harmonics
- [69. The strumstick's overtones](03-percussion-and-more.md#step-69): b3 from the whole decay: partials 2–5 nearer the recordings
- [70. The kalimba's rattle](03-percussion-and-more.md#step-70): the tine's sizzle as a membrane, measured per tine
- [71. Bodies of their own for the zithers and the strumstick](03-percussion-and-more.md#step-71): the strumstick on the lap, the psaltery and đàn tranh on a table
- [72. The balafon's buzz per key, and its other lines](03-percussion-and-more.md#step-72): the swing as a root sum of squares; per-key buzz; the strike's lines
- [73. "As recorded", a temperament](03-percussion-and-more.md#step-73): each recorded key at its own measured pitch
- [74. The marimba's and the kalimba's onset](03-percussion-and-more.md#step-74): the marimba's and kalimba's modes rise from zero
- [75. The sympathetic strings' drive fades](03-percussion-and-more.md#step-75): the bank's drive ramps over 20 ms: no bursts at damper landings

## [Instrument projects and the editor](04-projects-and-editor.md)

- [76. Instrument projects: the pipeline driven by data](04-projects-and-editor.md#step-76): `project.toml` per instrument, declared reads, the ledger, no names in code
- [77. Instrument discovery: the plugin driven by the exports](04-projects-and-editor.md#step-77): the menu block, the engine's report on a file, the catalog
- [78. Spread all keys on every instrument with a range of its own](04-projects-and-editor.md#step-78): the harps', guitars' and zithers' zones along their own range
- [79. The editor, stage 3a](04-projects-and-editor.md#step-79): the Rust editor over the project files: templates, the suggestion, the build page
- [80. The editor's guided setup, stage 3a.2](04-projects-and-editor.md#step-80): the rule model, the wizard, the folder picker, sortable tables
- [81. SFZ keys by `key=` and `<control>` offsets; the analysis stops on a map with wrong keys](04-projects-and-editor.md#step-81): the user's marimba SFZ: `key=` and octave offsets read
- [82. The bar fits take a set's dynamics through its source's table](04-projects-and-editor.md#step-82): SFZ layers named 1/3, 2/3, 3/3 mapped through the dynamics table
- [83. SFZ keys by note name and the default key center; the wizard's mallet names](04-projects-and-editor.md#step-83): Piano in 162's note names; one mallet name per mallet source
- [84. The editor's deferred minors from stages 3a and 3a.2](04-projects-and-editor.md#step-84): status on a thread, Finish's cleanup, a group's sources, atomic restores
- [85. SFZ `#include` and `#define`](04-projects-and-editor.md#step-85): Ethan Winer's SFZ files: includes and defines expanded
- [86. "How is it played?" and the donor](04-projects-and-editor.md#step-86): ten plain answers, the closest donor, auto tracks
- [87. Glockenspiel, the first instrument made with the editor's wizard](04-projects-and-editor.md#step-87): Ethan Winer's glockenspiel through the wizard
- [88. The final review's minors of step 86](04-projects-and-editor.md#step-88): a typed menu name kept, the donor passed, one-dynamic sets
- [89. The wizard's guess on a second glockenspiel](04-projects-and-editor.md#step-89): a wider bar profile; the donor scored both ways with decays
- [90. Glockenspiel B](04-projects-and-editor.md#step-90): VCSL's glockenspiel through the wizard
- [91. A bar's mode in a notch of the body](04-projects-and-editor.md#step-91): a mode's compensation averaged over ±50 cents
- [92. The hand chimes' A5](04-projects-and-editor.md#step-92): a take's strike capped re its neighbors
- [93. A piano's overtones read "not clearly a bar's"](04-projects-and-editor.md#step-93): harmonic series ruled out of the bar profiles
- [94. An audio input that excites the strings](04-projects-and-editor.md#step-94): two audio inputs drive the free strings through the bank
- [95. The editor compares the instrument with its recordings](04-projects-and-editor.md#step-95): every recording rendered and measured; keys flagged off the medians
- [96. The Compare page per source](04-projects-and-editor.md#step-96): each source judged against its own medians
- [97. The Compare page's A/B in mono or stereo](04-projects-and-editor.md#step-97): the A/B mixed to mono or played at the recorded microphones
- [98. Level corrections per key and dynamic](04-projects-and-editor.md#step-98): `level_corrections` per key and dynamic, a gain per strike
- [99. The Compare page's measurements, fixed by ear](04-projects-and-editor.md#step-99): a broadband length, stopped recordings, dry sets, the spectrum above its noise
- [100. The length as the ear judges it](04-projects-and-editor.md#step-100): the length in the note's band, 50 dB under its first 0.5 s
- [101. The marimba's levels corrected, the first instrument](04-projects-and-editor.md#step-101): two Iowa runs split again; the marimba corrected and heard
- [102. The vibraphone's levels corrected](04-projects-and-editor.md#step-102): the vibraphone's pp bass and treble
- [103. Level corrections by how far the recordings' levels are trusted](04-projects-and-editor.md#step-103): `level_groups`: whole, octave run or file
- [104. Grand A's levels: each key scaled by one gain](04-projects-and-editor.md#step-104): `levels = "per_key"`: the steps between layers recorded, the keys not
- [105. Take checks: flawed takes flagged on the editor's Check page, left out by ear](04-projects-and-editor.md#step-105): decay, late rise and brightness checks; `skip_files`
- [106. The first listening pass over the take checks' flags](04-projects-and-editor.md#step-106): takes left out by ear; two fixes the pass found
- [107. Borrowed reads by name](04-projects-and-editor.md#step-107): borrowing steps declare the keys they read
- [108. The bells' pp, and how far one key sways a level proposal](04-projects-and-editor.md#step-108): the plastic mallet's pp by hand; the sway a key has on a proposal
- [109. Grand D's levels from Grand B's recordings](04-projects-and-editor.md#step-109): Grand D's register and level steps from Grand B

## [Touch, MIDI learn, morphing and rolled pianos](05-playing-and-new-pianos.md)

- [110. The velocity curve editor](05-playing-and-new-pianos.md#step-110): control points, a monotone cubic in the engine, the Touch tab
- [111. MIDI learn](05-playing-and-new-pianos.md#step-111): ties to every control, one map per controller, values reported to the host
- [112. MIDI learn's review leftovers](05-playing-and-new-pianos.md#step-112): the CC tag's room, the map file merged, the tuning watch on the worker
- [113. A faster engine build at a microphone position](05-playing-and-new-pianos.md#step-113): 0.41 → 0.10 s; the last six setups cached
- [114. The coupling fit keeps to the committed fit, and a jump is reported](05-playing-and-new-pianos.md#step-114): the committed fit as a start; a warning on a jump
- [115. An instrument between instruments: the engine morph](05-playing-and-new-pianos.md#step-115): per-key interpolation, boards paired by rank, the level measured
- [116. The morph in the plugin](05-playing-and-new-pianos.md#step-116): the Morph tab's pad, `level.json`, fading out at a rebuild
- [117. Rolled pianos](05-playing-and-new-pianos.md#step-117): where a piano's identity lives; rolls from a recipe; the realism check
- [118. Rolled pianos, faster and chosen by hand](05-playing-and-new-pianos.md#step-118): about 7 s a roll, built as set, the donor table, notches lifted, Finish writes the export
- [119. The dampers measured from release recordings](05-playing-and-new-pianos.md#step-119): the first fall of Grand D's release recordings, 2–3× the old loss; Upright A's for the uprights
- [120. The release noise 12 dB lower, and the room measured in the gaps](05-playing-and-new-pianos.md#step-120): Salamander's key-up thump at −12 dB; the room right in level but tilted (+4 … +12 dB at 1.6–4 kHz)
- [121. The noises from a player's distance](05-playing-and-new-pianos.md#step-121): Salamander's close-miked thumps tilted as a player hears them (−13 dB at 3–8 kHz); a borrower stale only for what it reads
- [122. The pedal's release as a landing](05-playing-and-new-pianos.md#step-122): Salamander's pedal-up rattle replaced by Upright A's landing and bounces
- [123. The room cut where it overfilled](05-playing-and-new-pianos.md#step-123): per band where the strings' onset overfills the gaps (Grand A −7 … −12 dB at 2–8 kHz); a strike at every note dropped (a click)
- [124. One level curve over the velocity](05-playing-and-new-pianos.md#step-124): every piano key struck at the speed that sounds √velocity-linear levels (45 dB, Salamander's layers); the Dynamics control; the per-key level table in level.json
- [125. The knock under the recordings](05-playing-and-new-pianos.md#step-125): the pianos' knock shaped by the attack's spectrum and leveled by its 200–700 Hz peak, an overshoot 3×; the plucked instruments unchanged
- [126. The hammer's thump through the board](05-playing-and-new-pianos.md#step-126): the pianos' knock replaced by each strike's felt force, lowpassed, on the bridge (Grand E 500 Hz); the string's static share tried and dropped
- [127. The aftersound's level along the keyboard](05-playing-and-new-pianos.md#step-127): the pianos' horizontal level at C2, C4, C6 fitted to the recordings' aftersound (C5–C6 2–15 dB short with one level); a longer ring than recorded left to a Sustain control
- [128. The thump trimmed by ear](05-playing-and-new-pianos.md#step-128): Grand A's thump −12 dB, Upright C's −10 dB up to C4 and −6 dB from C6, a project's [impact] trim_db per key
- [129. The Sustain control](05-playing-and-new-pianos.md#step-129): every mode's decay rate divided (50–400 %), frequencies and attack kept; dividing the losses moved the pitch or left the bass
- [130. Each strike slightly different](05-playing-and-new-pianos.md#step-130): per strike the strike point, the unison's timing and felt shares drawn (Grand E's takes), the Strike variation control at 50 %
- [131. The phantom partials, part of the piano](05-playing-and-new-pianos.md#step-131): the energy between the bass partials grows ~1 dB/dB at 0.5–2 kHz (tension modulation); a phantom level per instrument plus the Wear control's, Grand A 0.05 by ear; ruled out on the way: damped strings, split partials, the bridge's mobility
- [132. The strings' loss at high frequencies](05-playing-and-new-pianos.md#step-132): the loss law b3·ω^q, β3 and q fitted to the partials up to the 60th with the coupling in the model; Grand A's bass high band lasts as the recordings' (q 1.55)
- [133. The correction in bands above the tracked partials](05-playing-and-new-pianos.md#step-133): band rows from the recordings' attack above the 60th partial; Grand A barely moved, the gap is the felt's high band per key
- [134. The felt's core](05-playing-and-new-pianos.md#step-134): a stiff layer under the felt's soft one (bright at mf, brighter above 2 kHz at f); a per-key fit tried and dropped (its board refit tilted the levels ±4 dB); Grand A's curve set by ear in `[felt_core]`, the fits never see it
- [135. Which felts want a core: Grand E's](05-playing-and-new-pianos.md#step-135): the grands' bass split by the attack's change from mf to ff: the engine's too wide (Grand G, D, B, A: a lower exponent wanted) or too narrow at 4–6 kHz (Grand E: a core); Grand E's core 2 N, its depth just past each key's compression at mf (measured by sweeping the depth)
- [136. The rail](05-playing-and-new-pianos.md#step-136): the traits plan's first phase: snapshots of any git ref in the plugin, the recordings' noise per band, a report of the grands' traits and their bass attack against the recorded grands' envelope, the listening set
- [137. Lists that scroll](05-playing-and-new-pianos.md#step-137): every list cut to the window and scrolled (the wheel, a bar), opened at its current entry; 33 snapshots made the instrument menu's last rows unreachable
- [138. Even levels](05-playing-and-new-pianos.md#step-138): the board's and the seeded body's level jumps from key to key evened out as a voicer would (`[even_levels]`: a trend through the ff levels, each key's way to it in its level correction at every dynamic, a seeded scatter); variants in the Snapshots group (`just variant`), every key's level in the report; Grand A 1.5 → 0.2 dB rms off the trend
- [139. Condition in zones, the grands mint](05-playing-and-new-pianos.md#step-139): the grands export voiced even (`[even_levels]`, Grand A halfway to the others' shape); "Wear: voicing" scatters each key's level (σ 4.5 dB · √wear); the Condition slider in zones Mint, Normal (the default: the keys' levels alone, about ±1 dB), Worn (the other aspects begin), Broken
- [140. What the board fit absorbed; Grand B's felt and unison by ear](05-playing-and-new-pianos.md#step-140): the per-key correction carries the hammer's spectrum per register (every grand's model treble too bright); band rows only clear of the noise; `[felt]` (exponent, stiffness) and `[unison]` (width) by ear; Grand B 1.7, +2.5 dB, 60 %
- [141. The grands' felts; the room's top octave](05-playing-and-new-pianos.md#step-141): every grand's `[felt]` from its recording at the played velocities, then by ear (Grand A's core at a quarter); a room band without a target holds its neighbor's cut (Grand D's rattle); `[room] trim_db` (Grand F's)
- [142. The felt with pp; the board's tone by ear](05-playing-and-new-pianos.md#step-142): solved from mf and ff alone, the lower exponents made soft playing 10–14 dB too bright (a plucked noise on Grand E); A, B, D, E solved with pp; `[board] trim_db` after the even levels (Grand C's boom); album 1 with the room 4 dB up
- [143. The board's tone per register](05-playing-and-new-pianos.md#step-143): measured re each note's fundamental, the treble's upper partials 4–7 dB over the recordings on Grands A–E (F, heard as right, not), A's and B's bass 3.5 dB full; a `[board] trim_db` row fades in over two keys; the treble heard better on A–D
- [144. The report again; Grand C's bass core](05-playing-and-new-pianos.md#step-144): against `voicing-1` five grands nearer their recordings, Grand C's ff bass 12–20 dB dark above 2 kHz (its felt solved from C2 up); a 6 N core to D♭2, gone by C3, mf unchanged
- [145. The felt as runtime controls](05-playing-and-new-pianos.md#step-145): a Felt tab bends the stiffness, exponent, core force and depth over the keyboard (bends on the shipped felt); every key keeps its loudness (the level rows measured again at the build, ~3 s); Hammer hardness on the pianos the same way
- [146. What the board's correction carries; Grand G's felt](05-playing-and-new-pianos.md#step-146): the correction's patterns predate the by-ear felts; the top octave's cut is no hammer's (a fixed filter per key, kept); the correction is hardly felt-shaped, but it was fitted against the fitted felt at other speeds than the felts were voiced at; a refit against today's felt at the played speeds, tried as variants, changed step 143's measure by about 1 dB either way, unheard: kept as is; Grand G's felt by ear (p 1.3, +3 dB): soft playing gets its highs back
- [147. The lowest octave's fundamentals](05-playing-and-new-pianos.md#step-147): Grand C's 250 Hz gap is one key and the engine's right channel; Grand F's, G's and D's board fits held the response flat below their data (58–77 Hz), their A0–G♯1 fundamentals 11–19 dB too strong; `[board] trim_db` low rows bring them to their recordings
- [148. The inventory of traits; the board's Bass](05-playing-and-new-pianos.md#step-148): the board carries the identity, the report's loss outliers were the fit's slope past its measured keys (now dotted); Bass on the Sound tab: the strings below 180 Hz (on the board's responses it lifted the strike's thud into a pop); Warmth and Air, the references' board directions, tried and taken off
- [149. Xruns beside a build](05-playing-and-new-pianos.md#step-149): a damper lift allocated on the audio thread (now on the stack); a felt's measurement copied 85 MB per note render and filled the memory's bandwidth (the audio thread's IPC 3.1 → 0.8): bursts shared, measuring responses cut to their window; builds on the physical cores but one. Beside felt builds p99 7.4 → 2.9 ms, none over a 256-sample budget
- [150. The strings' loss up to the top key](05-playing-and-new-pianos.md#step-150): the recorded keys past the measured loss give b1' from their fundamentals, past them a straight line that never falls; Grand C's top ×0.48 → ×0.85 of the recording's decay
- [151. The uprights' dampers and rooms](05-playing-and-new-pianos.md#step-151): Grand D's dampers (Upright A's takes hold 0.2 s before they fall), VCSL's dry room for Upright A and C (theirs read 1.1–4.5 s); a release falls 26–43 dB in 0.3 s, as Grand A's
- [152. Upright B's lowest fundamentals](05-playing-and-new-pianos.md#step-152): `[board] trim_db` rows below its board fit's data, B♭0 +28.8 → +13.1 dB re partials 2–4; its "missing bass" a thin tone, not its level
- [153. The uprights' felts by ear](05-playing-and-new-pianos.md#step-153): solved from probes as the grands' (A p 1.92 +9.2 dB, B 1.96 +12, C 1.92 +4.5 and a bass core at 3 N); Upright B's short sustain not the felt: 4–8 dB under its recordings at 1–2 s, heard as wanting 150–200 %
- [154. A ring per key by ear](05-playing-and-new-pianos.md#step-154): a project's `[sustain]` sets the Sustain control's factor per key (the export's `sustain`, the engine's compile); the uprights 150 %, Upright A and B rising to 200 % at C7
- [155. Upright B's room by ear](05-playing-and-new-pianos.md#step-155): its room fitted to its dry takes' gaps, heard as very dry; an assumed living room, uncut (`[room] trim_db` lifts too, up to +6 dB)
- [156. Stems from `midi2wav`](05-playing-and-new-pianos.md#step-156): `--stems` writes the struck strings, the ringing, the room and the noises as files that sum to the render (−131 dB re its peak); a render each, the others muted where they join the mix
- [157. A press is any value but 0](05-playing-and-new-pianos.md#step-157): Next and Previous step at every value on their side (0, or any other), so a + and a − button on one CC carry both and a button without a release steps each time; a switch is on from 1, a pad hit softly included
- [158. Relative knobs](05-playing-and-new-pianos.md#step-158): "Learn MIDI (relative knob)": an endless knob's two's-complement moves (1–63 up, 127 … 65 down) move the control from where it is; a list one entry per 3 steps
- [159. Dragging the microphones](05-playing-and-new-pianos.md#step-159): a handover's new responses take the old ones' input history and crossfade at the outputs over 30 ms (the old ones rang out their 2 s tails, four at a time); unchanged strings swap whole; a drag at 256 frames: 588 blocks of 2250 over budget → 0
- [160. Flipping pads, an instrument knob, soft takeover](05-playing-and-new-pianos.md#step-160): a momentary pad flips a switch at each press; a relative knob of its own for the instrument menu; absolute knobs take a slider over once they reach it
- [161. Scala files in the plugin](05-playing-and-new-pianos.md#step-161): the Temperament menu's "Scala file" (DPF's file browser, the "scale_file" state); any note count, a `.kbm` key map beside it; the engine's tuning per key
- [162. Dragging the morph pad with a felt set](05-playing-and-new-pianos.md#step-162): each point builds at once on the felt's leveling as last measured, the point the pad stops at measured after (each step measured 3–7 s before)
- [163. Instruments kept anywhere](05-playing-and-new-pianos.md#step-163): the instrument menu's "Open instrument folder…"; the "instrument" state holds the folder's path
- [164. A fifth less CPU](05-playing-and-new-pianos.md#step-164): the convolutions' products on AVX2+FMA, their output without a modulo per sample, silent blocks skipped and the diffuse part through one mean response; culls every 4th block; the lanes summed as a tree: `bench` at the Player position 21.6 → 17.5 % (pedalled), 8.7 → 6.1 % (chords)
- [165. The Visualization panel](05-playing-and-new-pianos.md#step-165): the strings, dampers and hammers as the engine plays them, live in a panel beside Analysis; the telemetry frame carries each key's strings, damper and last strike
- [166. A video from midi2wav](05-playing-and-new-pianos.md#step-166): `--video` draws the Visualization panel frame by frame (tiny-skia, every core) into ffmpeg with the sound; `--size`, `--fps`, `--ffmpeg` (AV1 …); the hammers rise before their notes
- [167. midi2wav's command line](05-playing-and-new-pianos.md#step-167): `-h`/`--help` (every `--param` and `--tuning` key), `--version`; every value checked first, suggestions for typos, the files named in the errors
- [168. The uprights voiced as the grands](05-playing-and-new-pianos.md#step-168): even levels (1.4–1.8 → 0.2–0.5 dB rms); Upright A's and B's treble trimmed from C4 to C5 (4 kHz +18 and +11 dB over their recordings at mf), heard "less glassy"; Upright C's lowest fundamentals −10 dB; `tone_check.py` and `just report --uprights`
- [169. The uprights finished](05-playing-and-new-pianos.md#step-169): Upright A's mf/ff gap was the measure's (the fundamental's own band counted; `tone_check.py` leaves it out), its treble at the grands' edge; Upright B's B♭0 the recording's (the weakest of every set)
- [170. Live re-strike](05-playing-and-new-pianos.md#step-170): a re-strike's felt meets the ringing strings where they are (the voice follows the running modes during the contact); a soft repeat into a loud note up to 8× the force, each repeat a little different by the string's phase; a Touch switch, off by default, free
- [171. Live re-strike for the mallets](05-playing-and-new-pianos.md#step-171): a contact cannot keep the bars' recipe (13–16 dB off where the cutoff climbs steeply); the mallet meets the swinging bar at the relative speed, the collision a brake on the running modes plus today's strike (pushed at the relative speed it ran away on light treble bars), the same switch
- [172. The runaway guards](05-playing-and-new-pianos.md#step-172): a voice past 30 dB over its largest strike (or not finite) silenced at the block's end; an output past +20 dBFS clears every sound at once; both counted, neither tripped by hard playing on any instrument
- [173. One loudness for every instrument](05-playing-and-new-pianos.md#step-173): calibrated by their ff peak, held tones played up to 14 dB louder than the grands (the marimba clipped); each instrument's gain now puts a phrase in its middle at the grands' loudness (±0.2 dB), Grand G's −10 dB gone; Harpsichords C and D get step 150's loss
- [174. The Volume to +24 dB](05-playing-and-new-pianos.md#step-174): +12 dB too little for some pieces; the output guard rises with the Volume above 0 dB
- [175. Presets](05-playing-and-new-pianos.md#step-175): microphone presets per body type (the Mics tab) and full presets with the instrument (the Sound tab), shipped mic setups for the grand and the upright, the user's as files in ~/.local/share/pianoengine/presets
- [176. The plugin starts at Player](05-playing-and-new-pianos.md#step-176): Player sounds much better than Recording; midi2wav and the fits keep Recording; the loudness at Player within ±2.6 dB of step 173's
- [177. Spread all keys on, the loudness re-matched](05-playing-and-new-pianos.md#step-177): the plugin starts with all keys spread; each instrument matched at the midpoint of Recording and Player (eight moved by 1–1.5 dB): both positions within ±1.5 dB of the reference
- [178. midi2wav plays the Dynamics](05-playing-and-new-pianos.md#step-178): its velocity-to-speed probe never got --param, so Dynamics changed nothing in renders and Copy CLI lines; fixed, with a test
- [179. Production presets](05-playing-and-new-pianos.md#step-179): measured against FluidR3 (bright) and SGM (warm, even); Production, bright (Grand A close, hard hammers, little room) and Production, warm (Grand B, Dynamics 25 dB), shipped as files and as the plugin's programs (LV2 presets)
- [180. Touch presets: closed](05-playing-and-new-pianos.md#step-180): light/heavy dropped (keyboards have it); Grand A within 2 dB RMS of its set as sfizz plays it from velocity 16 up, the set's floor below not copied
- [181. Grand G's treble](05-playing-and-new-pianos.md#step-181): its board response 38 dB heavier at 100–300 Hz than at 1–1.6 kHz sent the treble's onsets out as a low thump and rumble; 15 dB of it moved into the per-key correction (`[board] transient_db`): the partials kept, key 87's attack on its recording's
- [182. The harpsichords' release noise](05-playing-and-new-pianos.md#step-182): their release samples held the string ringing out, played again as a 0.3–0.5 s whisper; the note's partials taken out before measuring, and every harpsichord's release noise at −38 dB re the note (was −24 … −34)
- [183. The unisons toward the top key](05-playing-and-new-pianos.md#step-183): the recordings' unisons beat faster toward the top (5.6 cents at F♯7–C8, the engine 1.4); `[unison] treble` raises each piano's detune from key 78 (×1.8–12 at 108), the engine's top beats now 4.3; `unison_check.py`
- [184. The board's low modes ring](05-playing-and-new-pianos.md#step-184): the synthetic board's modes all died in ~0.12 s, a measured board rings ~0.5 s; `[board] low_eta` 4 % on the ten pianos by ear (0.45–0.6 s below 200 Hz), their low modes radiated to both microphones in phase; the rolls' steering judged from 250 Hz up
- [185. Program changes follow their switch again](05-playing-and-new-pianos.md#step-185): the JACK standalone loaded program N (Default, the Production presets) on a MIDI program change, the switch unread and the UI unaware; `loadProgram` on the audio thread is ignored
- [186. A concert grand's board ring at the player's seat, measured: 4 % kept](05-playing-and-new-pianos.md#step-186): Plath's anechoic dataset rings ~1 s at 125–250 Hz under treble notes; the body response ends at 0.5 s (1 s: +2–3 points of load); 2.5 % sounded different, not better: kept at 4 %
- [187. Player's bass in both ears alike](05-playing-and-new-pianos.md#step-187): against Plath's dummy head at a concert grand's keyboard, Player's bass sat 6–9 dB right on every grand; the lid reflects from its size up (Rindel), the pair's channels at one level, the right radiation independent of the left again: 63 Hz within ~1 dB of the seat
- [188. The lid taken off](05-playing-and-new-pianos.md#step-188): Lid removed (Mics tab, off by default; `lid=2`): no lid reflection, no closed lid's loss, the open lid's positions; small at Player (≤ 1.7 dB per octave), Audience −3 dB at 8 kHz with the room (−6 dB without)
- [189. The harpsichords' keys reversed](05-playing-and-new-pianos.md#step-189): the menu block's `reversed_keys`: the strip draws dark naturals and light sharps on the four harpsichords
- [190. The phantom partials, a defect in the bass](05-playing-and-new-pianos.md#step-190): the Condition's phantom partials played on every key and put loud treble notes 15–30 dB over their neighbors; now a defect (Extras, off by default), the piano's own and the control's in full to C2, fading out by C3
- [191. The mallet cuts no fundamental deeply](05-playing-and-new-pianos.md#step-191): the bars' fit let the crotales' cutoff fall under their top keys' fundamentals (43 dB at C8, made up by the modes' levels), and +12 dB of hardness put them up to 39 dB over or silenced them; the first mallet's cutoff now cuts a fundamental by at most 3 dB at mf, every bar instrument refitted: +1.6/+2.8 dB at velocity 100
- [192. The bars wear as bars](05-playing-and-new-pianos.md#step-192): the soundboard's wear killed the bars (the balafon silent within 1 s at Broken) and every bar drifted like a string; now no soundboard, strings or false beats on bars (hidden there), metal ones keep their pitch, wooden ones drift σ 10 cents at Broken, the kalimba as strings
- [193. A plucked string's soundboard wears a third](05-playing-and-new-pianos.md#step-193): the soundboard's wear multiplied a plucked string's own loss, most of its damping, as a piano's (guitar −18.5 dB at 1 s, median, at Broken); now a third of it: guitar −7.0, harps −2 to −4, harpsichords −3 to −5, the pianos' range
- [194. The keys' voicing by kind](05-playing-and-new-pianos.md#step-194): a piano's per-key voicing and hammer wear on every instrument; now the bars' keys differ in level alone by half a piano's (±0.5 dB at Normal), the strings played by hand too and without hammer wear (hidden); pianos and harpsichords as before
- [195. Normal at 10 %](05-playing-and-new-pianos.md#step-195): Mint and Normal sounded alike; the zones' edges at 10 % (Normal, the default), 35 % (Worn), 100 % (Broken), the wear at each edge as before: Normal → Worn steeper
- [196. Two harpsichords from Edinburgh, each 8′ choir recorded](05-playing-and-new-pianos.md#step-196): Harpsichords E (Baillon, 1755) and F (Barton, 1709), CC BY 4.0; a register names its stop, their second choirs recorded on the 8′ II's (no 4′, no lute); at the grands' loudness, the 8′ II 1.8 and 3.0 dB louder as recorded
- [197. Three mbiras from VCSL](05-playing-and-new-pianos.md#step-197): VCSL's unused instruments looked at; Mbira A, Mbira B and the nyunga nyunga: the kalimba's tines with buzzers 25 dB stronger (the membrane's level ×10), named an octave low, at the kalimba's loudness; with the kalimba in a group of their own, Thumb pianos
- [198. Neutral names, the grands lettered by sound](05-playing-and-new-pianos.md#step-198): the instruments' directories name no set or maker (`grand_a` …); the grands re-lettered best-sounding first; no sound changed
- [199. Every instrument's license, written from its project](05-playing-and-new-pianos.md#step-199): a `[license]` block per project, `LICENSE.md` for all 38 exports with the lenders' credits; CC BY 4.0, BY-SA where a ShareAlike set is reached; Iowa and Salamander terms checked
- [200. The release peaks follow the code; two checks before a commit](05-playing-and-new-pianos.md#step-200): the release noise's reference peaks, a memo keyed without the code, outlived the model (Grand A's from a working tree); the key covers the code and the project now; `just check` (nothing stale) and `just reproduce` (the exports made again and compared: 13 of 38 differed, by up to 2.8 dB in the release noise; heard, exported again)
- [201. A fresh clone builds, and makes a new instrument from one set](05-playing-and-new-pianos.md#step-201): the repository files (LICENSE AGPL-3.0-or-later, also in the plugin's metadata; CONTRIBUTING, templates, the README, two quick starts, the lessons); DPF a submodule; `proto/out/` made by its steps; the seven lent caches (4.4 MB) in git and a lender without its recordings taken as committed; the acceptance test from one set in 70 s
- [202. Three checks around the lent caches](05-playing-and-new-pianos.md#step-202): a missing lent cache named before the first step; the lent caches compared with their borrowers' ledgers; `just check` fails when the sets are not mounted and nothing is checked
- [203. CI on three platforms; the plugin ported to macOS and Windows](05-playing-and-new-pianos.md#step-203): Linux builds and runs the suites but the engine's (nightly); macOS (arm64, 13.3+) and Windows (MinGW) build the plugin and run its UI tests; tests without their sets or caches skipped and named; user folders per platform, std::filesystem, parseFloat, a Windows file mapping
- [204. Upright D, from the maintainer's own session](05-playing-and-new-pianos.md#step-204): `split_session.py` cuts a session's passes at their strikes (180 of 188 takes kept; mis-strikes, a wrong note, touched keys, clipped takes left out); its own dampers (111 dB/s, Grand E's 111), release noise and room; felt p 1.82 +5.3 dB, the lowest fundamentals and the treble trimmed, unisons ×1.81: measured, not heard yet
- [205. Upright D heard: the release noise borrowed, its own pedal, a felt core](05-playing-and-new-pianos.md#step-205): its release takes held the damped strings' remainder (−29 dB re the note, bright, an edge at the start): Salamander's instead; `pedal_noise.py` reads a set's own pedal files, `split_session.py --pedal` tells down from up: the pedal's ring −27.7 → −39.0 dB re the note (its recordings −37 … −54); Upright C's felt core: the bass's attack at mf −7.9 → −0.9 dB
- [206. Upright D's pedal thumps and its undamped treble](05-playing-and-new-pianos.md#step-206): its thumps played 4.5 dB over its recordings (the export's mf reference) and clicked (a room's floor and blips read into the bands): `[noises] pedal_level_db`, a set's own takes read within 0.35 s, their floor out; every piano's borrowed thumps lie at or under the recordings, not 20 dB over; `[sympathetic] percent`, Upright D 25 %
- [207. Upright D's pedal: the stomps out of the round robin](05-playing-and-new-pianos.md#step-207): the engine plays the pedal-downs in turn whatever the press; every third and fourth press played the pass's two stomps and their clack (highs −37 dB re the peak at the onset, the others −68 … −138): the thumps the two normal presses, the ring still fitted to all four; a warning on a take 6 dB over its kind
- [208. Grand H, from a normalized baby grand](05-playing-and-new-pianos.md#step-208): the City Piano (public domain), every file at −5 dBFS, cut while sounding, mono, its releases key-up noises alone; its layers' timbre its own, its levels Grand B's; felt, board and unisons measured on it; heard "thin" and low-passed: ff 7–9 dB dark at 4–10 kHz, a felt core; the thinness the recording's
- [209. Grand I, from the OLPC sound library's grand piano](05-playing-and-new-pianos.md#step-209): Zenph Studios' reproducing piano for OLPC (CC BY 3.0, in the Wayback Machine): 1212 files intact, mono 16 bit, 30 keys × ~16 strengths × held, 7 s and staccato; the held takes nearest MIDI 14, 60, 124; Grand A's file as donor; its lowest fundamentals −12 dB (+23 → +10 dB re partials 2–4), the room's noise-floor decay above 8 kHz left out (a hiss), the felt solved on the attack (pp +8.6/+13.6 → +0.9/+4.4 dB re the recording, heard as stringy)
- [210. The Pedal noise at −20 dB by default](05-playing-and-new-pianos.md#step-210): the thumps distracted in quiet pieces and were turned down 20 dB on every piano; the control (thumps and ring together) starts at −20 dB, `midi2wav` still as fitted, the Copy CLI line with `pedal=0.1`
- [211. The pedal's thumps and its ring, two controls](05-playing-and-new-pianos.md#step-211): Pedal noise the thumps, Pedal ring the strings' kick as the dampers lift (`pedal_ring`, in the report, the 13 pianos); the noise −20 dB, the ring −10 dB by ear
