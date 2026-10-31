# The engine, the plugin and the microphones (steps 1–21)

The real-time engine from the first export to the plugin UI: the plan and its first measurements,
the dampers and the pedal, the physics compiler, wear and defects, the microphone positions, the
first instruments after Grand A and B, temperaments and registers. Index of every step:
[README.md](README.md).

## The plan, before step 1

Cost numbers from `proto/cost.py` (Salamander profile, 48 kHz; Iowa is within 15 %).
1 complex multiply-add (cMAC) = 8 flops.

### Where the cost is

| Component | Per-sample work | Load |
|---|---|---|
| Strings: eigenmodes after the hammer contact | 1 cMAC per mode; 335 modes per note on average (804 at E1), 115 after pruning at −80 dB (pruning error −70…−77 dB re the signal) | 32 voices: 5.3 GFLOP/s full, ~1.7–3 pruned (mf…ff) |
| Hammer contact | 25–110 samples × ≤ 400 real resonators, once per note-on | negligible (< 50 k MAC per strike) |
| Sympathetic bank (≤ 5 kHz, unisons merged) | 1 cMAC per free resonator; 2 502 with the pedal down, 33 without | ≤ 1 GFLOP/s |
| Radiation: body 0.5 s + room 2 s, 2 polarizations × 2 channels | partitioned FFT convolution of 4 × 120 k taps | ~0.1–0.3 GFLOP/s |
| Per-note transfer correction C(f, note) | becomes a gain per mode (magnitude at the mode frequency) | free |
| Knock, release noise | per-key buffers, precomputed at load | negligible |

Worst case (32 sounding notes at ff, pedal down): roughly 4–5 GFLOP/s. All resonators are
independent, so the recursions vectorize perfectly across modes (structure of arrays,
AVX2/AVX-512). That should fit into a fraction of one core of a current laptop CPU; the
first Rust step is a benchmark to replace this estimate with a measurement.

### Design

- **Instrument file, exported from Python.** The Python prototype stays the fitting and
  scale-design toolchain. An export script writes everything the engine needs per note:
  mode frequencies and losses, the coupled eigen-decomposition (λ, V, V⁻¹) for both
  polarizations, bridge factors, per-mode correction gains, damper rates, hammer and felt
  parameters; plus the body/room impulse responses, the knock/release-noise parameters and
  the sympathetic bank. The engine only runs it, so it cannot drift from the prototype's
  physics, and it is checked against the prototype's renders.
- **One voice per key**, not per note-on: a re-strike adds the new hammer contact to the
  string's current state (the prototype rendered each strike independently). Bounds the
  voice count at 88 and is closer to the physics.
- **Note-on in the audio thread**: hammer contact (a few ms of real resonators), projection
  onto the eigenmodes (K small S×S solves, V⁻¹ precomputed), pruning at −80 dB. Cheap
  enough to do within one block.
- **Strings**: complex one-pole per kept mode, float32 SoA. Long, lightly damped modes drift
  in float32 over seconds, so every ~4096 samples the states are re-synchronized from the
  closed form (w·e^{λn}, double). Dampers and pedal change the poles at events, as in the
  prototype. A voice is dropped when all its modes are below −100 dB.
- **Sympathetic resonance**: the prototype's bank, driven by the summed vertical bridge
  force, with the same state handover when a key is struck (no reset).
- **Radiation**: non-uniformly partitioned convolution (low latency) for body and room;
  the room is a separate, switchable part (it is the recording room, not the instrument).
- **User parameters** (first set): room level, knock level, release-noise level,
  sympathetic-resonance amount (TRANSFER), velocity curve, profile selection.

### Language and framework

DSP engine in **Rust** (library with a small C ABI: load instrument, note on/off, pedal,
process), host integration with **DPF** in a thin C++ wrapper: LV2, VST3, CLAP and a JACK
standalone from one code base. Pure-Rust frameworks (nih-plug) cover CLAP/VST3 but not
LV2, and the low-level Rust LV2 crates would leave state, worker thread and UI to us.

### Measured (step 2)

AMD Ryzen 7 7840HS, one core, 48 kHz, `engine/target/release/bench` (64-frame buffers;
per-buffer worst case = fastest of three runs per buffer, which removes system noise):

| Scenario | Mean load | Worst buffer | Peak |
|---|---|---|---|
| Pedal down, 32 random ff notes/s over the keyboard | 12 % | 34 % | 87 voices, 13 300 modes, 2 502 sympathetic |
| 8-note mf chords every 0.5 s, no pedal | 3.5 % | 44 % (8 strikes in one buffer) | 54 voices, 4 000 modes |
| Pedalled ff glissando over 88 keys | 12 % | 19 % | 88 voices, 16 800 modes |

Step 1 (f64, per sample, uniform 256-block convolution) ran the first and third at
~125 % with spikes to 3×. What changed:

- **Strings**: float32 complex one-poles, 16 per lane group (AVX2+FMA when available,
  portable otherwise), rendered per sub-block (sub-blocks end at every event and every
  64 samples). The exact state is kept in double precision and re-synchronized every
  64 samples and before every strike or damper change, so float32 rounding never runs
  longer than one block. Modes below −80 dB re the strongest at a strike are not started,
  running modes are dropped below −100 dB. Against the step-1 render: −70 dB overall,
  −105…−123 dB without pruning.
- **Sympathetic bank**: one float32 bank over all keys; only driven or still ringing
  resonators run. Pole rounding to float32 detunes them by < 1 mHz (deterministic,
  inaudible; it is the only difference above −100 dB to the double-precision bank).
- **Radiation**: non-uniformly partitioned convolution, float32: 64-sample blocks over the
  first 960 taps, 1024-sample blocks beyond; the products with older input spectra are
  spread evenly over the 64-sample blocks. **Latency 64 samples** (1.3 ms at 48 kHz).
- **Note-on**: ≤ 215 µs per ff strike (bass), spread over the 2–4 ms of the contact.
- **Sample rates**: impulse responses and noise bursts are resampled at load (spectral
  interpolation; responses scaled by 48 kHz/rate). A digitally driven resonator responds
  θ/sin θ stronger (θ = ω/rate), which the model was fitted with at 48 kHz, so the
  contact keeps the 48 kHz factor at any rate. At 44.1 and 96 kHz the band levels match
  48 kHz within ±0.5 dB (up to 1 dB at 16–20 kHz), overall level within 0.2 dB.
- **Audio thread**: no allocation after construction (fixed contact slots per key, at
  most two concurrent contacts; overlay list capped).

<a id="step-1"></a>
## 1. Export and an offline render

Export script (Python → instrument file) and a Rust engine rendering the demo's event
list offline; compare with `proto/demo.py` (strings sample-accurate, noise parts
statistically).

<a id="step-2"></a>
## 2. The benchmark: float32 strings, partitioned convolution

Benchmark the per-sample engine; settle pruning, block size, float precision. Done: "Measured (step 2)" above.

<a id="step-3"></a>
## 3. The DPF wrapper; the noises synthesized at load

DPF wrapper: MIDI, sustain pedal (CC64), parameters, profile loading in the worker thread.
Done: `engine/src/ffi.rs` (C ABI, `plugin/src/pianoengine.h`), `plugin/` (DPF, pinned
by `fetch-dpf.sh`). The room is exported separately from the body (body + room = the
former full response) and mixed after convolution with the room level. A worker
thread loads engines (instrument × sample rate) and hands them over through two
single-writer atomic slots; the audio thread never blocks or frees. The engine sets
flush-to-zero itself during processing. Switching the instrument while notes sound
cuts them (no crossfade yet).
Later: the noise parts (room response, knock and release bursts) are no longer
exported as sample data but synthesized by the engine at load from their parameters
(`engine/src/noise.rs`, parallel, ~0.1 s); instruments shrank from 25 MB to 3 MB.
Release and knock bursts match the prototype's statistics within the spread between
two prototype seeds (band levels, envelope, L/R coherence); the room's T60 and band
levels likewise. A new room realization changes single partials' levels by a few dB
(like moving the microphone), so individual notes sound slightly different.

<a id="step-4"></a>
## 4. Pedal noise

Pedal noise (done): `analysis/pedal_noise.py`, `proto/pedal.py`. Pedal down is a
thump (resynthesized like the release noise, two takes alternating) plus the strings
ringing as the dampers lift: a displacement kick into the string models of the keys
whose dampers lift, centered on the bass (fitted: keys ~35–38, practically fundamentals
only), with the unison strings' phases summing to zero (the recorded ring has no loud
onset and stays flat for seconds: the slow modes). Pedal up: the dampers landing.
Levels re the median mf note from the sample set's mix; Iowa uses the Salamander
measurements.

<a id="step-5"></a>
## 5. Dampers continuous

Dampers continuous (done): each key has a damper engagement e (0 lifted … 1
resting); the damper loss is scaled by e (strings and sympathetic bank), so the pedal
position (CC64 / 127) half-pedals: dampers leave the strings between depth 0.3 and 0.7
(smoothstep; no measurements, a plausible regulation), e quantized to 1/32 (each
change recomputes the sounding modes' poles). Whenever a key's dampers leave its
strings (e falls through ½: pedal or key press), the strings get the lift kick, so a
silent key press rings too; thumps when the pedal crosses the middle. The kick is
limited to the partials above −80 dB after its rolloff (it runs in the audio thread
at every damper lift: computing all partials made 100 % spikes at a pedal-down).

<a id="step-6"></a>
## 6. Physics compiler in Rust

Physics compiler in Rust (done): the export carries physical parameters only
(string, hammer, damper per note; soundboard modes, transfer correction, damper
tables); `engine/src/physics.rs` derives partials, bridge admittance (from the board
modes directly instead of the prototype's interpolated table), the coupled unison
modes (eigen-decomposition of diag(d) − κ·11ᵀ via its characteristic polynomial,
S ≤ 3) and the sympathetic resonators, at the engine's rate. Against the prototype:
exact to 1e-15 except the bridge coupling (≤ 1 %, the prototype's table) and the
damper rates (≤ 0.7 %, the 0.1-octave table); demo −72/−82 dB. Instruments 3 → 0.5
MB. Runtime: concert pitch (strings retuned by tension) and unison width recompile
(new engine); hammer hardness is live (felt stiffness at the next strike).

<a id="step-7"></a>
## 7. Condition

Condition (done): a macro 0 (as fitted) … 1 (worn) plus per-aspect offsets. Live
(engine.rs): hammers (felt +12 dB stiffer, exponent +0.6 at equal force at 0.5 mm),
voicing (± 4 dB per key, fixed draw), dampers (engagement × (1 − 0.75)), action noise
(+9 dB). Compiled (physics.rs `Wear`, rebuild): unisons (± 3 cents per string),
tuning (± 6 cents per note), strings (partials ± 0.6 cents·√n per string, b3 × 3),
soundboard (b1 × 2.5). Random draws per (variant, note, purpose) streams, so the
amounts only scale them. Magnitudes are estimates without measurements. Measured on a
test passage at 100 %: spectral centroid 476 → 557 Hz (hammers), release decay −39 →
−13 dB/s (dampers), pitch scatter 5 cents (tuning), beat depth 0.8 → 1.9 dB
(unisons), sustain −6.9 → −8.8 dB/s (soundboard); level within ±1.6 dB.
Revised by ear: unison drift σ = 65 cents·a^3.7, tuning drift σ = 40 cents·a³ (5 cents
at half wear each; the extremes far beyond a honky-tonk).

<a id="step-8"></a>
## 8. Optional defects

Optional defects (done, live, not part of the condition): hammer grooves (high-passed
noise on the bridge force in the first 6 ms, scaled by the contact's peak force),
damper buzz (when a damper lands: a dead zone on that string's own bridge force for
250 ms, the voice rendered separately meanwhile), rattle (a loose part on the board:
one-sided contact above 0.35 × an ff E1's peak bridge force, measured at load, on the
summed string force). Intensities from listening to prototype sketches; 100 % = the
most they should be.

<a id="step-9"></a>
## 9. False beats and phantom partials

False beats and phantom partials (done; by ear the strongest prototype sketch ≈ 55 %
wear, beyond it extrapolated). False beats (compiled): partials split, the copy's
modes Δω off with the same contact excitation, share/depth/beat rate growing with
the amount (physics.rs `split`, voice.rs ids after kn·S). Phantom partials (live):
per voice level·HP150(v²/F_ff) on its own bridge force (F_ff: an ff E1's peak
force), so they grow with dynamics and in the bass; the voices render separately
while it is on (~25 % more render time). Both match the sketches within 0.5 dB at
55 %.

<a id="step-10"></a>
## 10. Microphone positions, step 1

Microphone positions, step 1 (done, `engine/src/mic.rs`): the fitted sound is taken
as direct sound at 1 m; a position moves the two microphones around a grand on a
floor with the lid on full stick: per microphone the soundboard's response along the
direct path (1/R, delay, air absorption), the floor reflection and the lid
reflection (mirror sources, the lid only where the reflection point lies on it); the
diffuse room unchanged, total level kept. Player (ears at the keyboard, 1.3 m, no lid
reflection): direct/room −1.8 dB; audience (3.5 m on the curved side, with the lid
reflection): −8 dB.

<a id="step-11"></a>
## 11. Microphone positions, step 2

Microphone positions, step 2 (done): each note drives the board at its bridge point
(bass bridge back left, treble bridge diagonal to the front right), mixed between 8
zones. Per zone the soundboard's response is re-synthesized from the board modes
(Rust reproduces the exported response to −149 dB for the original excitation)
with mode shapes from random plane waves at the board's bending wavenumber (∝ √f,
25 cm at 1 kHz), and the step-1 paths start at the zone's point. The two
microphones' radiation factors are coherent at low frequencies by their spacing
(sinc(2fd/c); the fitted ones are independent, a wide pair): otherwise the left and
right responses are uncorrelated and no arrival-time difference survives. Player:
bass ≈ center/slightly left, treble clearly right (the treble bridge end is front
right). Audience: a 50 cm pair; from 3.5 m the bridge spans only ±11°. Voices
render separately and are split to their zones; the sympathetic bank and rattle
are spread evenly (diffuse); the room convolves the total. Cost: heavy pedalled
passage 13 → 22 % mean, worst 64-frame buffer ~60 % (16 body convolutions finish
their large blocks together; can be staggered). Recording position unchanged.
Mic spacing (0…1000 cm, default 50) for player and audience: the pair centered on the
position across the line of sight; the board radiation's and the room's
inter-channel coherence follow it (the room re-synthesized with the spacing, same
energy). L/R correlation 1.00 (0 cm, mono) → 0.38 (16) → 0.27 (50) → −0.2 (500).
Mic distance (0 = the position's default: player 1.25 m, audience 3.5 m; 1…20 m),
horizontal from the board's center: direct/room +0.5 dB at 1 m, −8 dB at 3.5 m, −17
dB at 10 m, −24 dB at 20 m (audience; the player, without the lid reflection, about
1 dB less). "Distance level": the total kept (default) or the direct sound (the
room then grows with the distance; with the room off and an external reverb the
level stays within ±1.6 dB from 1.5 to 20 m).

<a id="step-12"></a>
## 12. Microphone positions, step 3

Microphone positions, step 3 (first version): the board's directivity per sound
path (departure angle from the board's normal: direct, floor via the underside,
lid), applied like the air absorption. Above coincidence (~1.9 kHz for the 250 m/s
bending speed at 1 kHz) a plate radiates at angles up to sin θ_c = c/c_b(f): a
filled cone around the normal, narrowing with frequency (a uniform plate fills only
its rim; ribs, edges and the drive point's near field fill it: grands send the
treble up broadly, the lid redirects it to the audience); soft edge, blended to
even radiation below coincidence, normalized to the hemisphere mean. The statistical
expectation of the full Rayleigh integral over random-wave modes, without
computing it per surface element. Effect on the board part: player 16 kHz −1.9 dB,
audience +2.1 dB; in the full signal (with the room) < 1 dB of treble balance: the
audience's direct path is grazing (dull), the player's floor reflection leaves the
underside steeply (bright). A first, conservative version.

<a id="step-13"></a>
## 13. Handover

Handover (done): an engine rebuilt for another tuning or microphone setup takes over
from the running one (`Engine::adopt`, `pe_adopt`, called by the audio thread when
it swaps engines; real-time safe, no allocation). It takes keys, pedal, queued
events, the output in flight (FIFO and the partial block), one-shots, the hammer
contacts in progress, and every running mode's exact state, which goes on at the new
pole: phase and output continuous, the strings glide to the new frequencies. Mode
ids map one to one (eigenmode e of partial j; the eigen-decomposition starts each
root near its string, so the order is stable), false-beat copies to the same
partial's copy (or merged into the partial), partials beyond the new count dropped;
the sympathetic bank likewise per key. Radiation: the same setup (equal position,
spacing, distance, level reference; board wear only changes the strings) moves the
old convolvers over with their history; another setup crossfades the input over
100 ms (smoothstep) while the old responses ring out (≤ 4 at a time, room 2 s).
The plugin's worker keeps a cache (loaded instrument, last radiation responses):
a tuning change rebuilds in ~20 ms instead of ~0.6 s, a mic change in ~0.45 s.
A handover with the same tuning matches the uninterrupted render to float rounding
(−138 dB mid-block, exact at a block boundary; `engine/tests/handover.rs`); with
other tunings and mic setups no click (6 kHz high-passed energy within the
reference's spread) and no level gap. `render` takes RENDER_TUNING/RENDER_SWITCH,
`lv2render` timed parameter changes (`@t symbol=value`) and LV2RENDER_PACE.
Instrument or sample-rate changes still cut.

<a id="step-14"></a>
## 14. Close microphone positions

Close microphone positions (done): "close: hammers" (a pair across the piano above
the hammer line, 0.3 m above the soundboard) and "close: tail" (over the rear of
the board, 0.45 m), both under the open lid; the distance control sets the height
(kept under the lid, ≥ 8 cm), the spacing is limited to the case (≤ 1.1 m). A
microphone above the case gets no floor reflection (the case shields it); the lid
reflection where the microphone is under the lid. A bridge zone radiates from an
area: its level follows 1/√(R² + 0.2²) instead of 1/R (no change beyond ~1 m:
≤ 0.2 dB). Chromatic scale (mf, first 0.25 s): hammers pan bass −1 dB … top octave
+11 dB (R−L), mid register 15 % brighter (spectral centroid), note-to-note
unevenness 2.1 dB (recording position 1.7); tail: bass −4…−6 dB (left), treble
centered, 13 % brighter. Here the directivity (step 3) matters: hammers +2…+4 dB above
3 kHz, tail −2 dB above 6 kHz.

<a id="step-15"></a>
## 15. A third instrument, Upright A

A third instrument, Upright A (profile `knight`: the VCSL "Knight" upright, CC0; 45
notes in whole tones, two velocity layers with their recorded levels, 44.1 kHz).
Pipeline generalized: `analysis/datasets.py` (loaders, SFZ mapping, data roots,
each profile's dynamics), any number of dynamics (the two upright layers are mf
and ff; pp's hammer speed log-linear in the MIDI velocity), support notes snapped
to the recorded ones. Where a set keeps its levels, the fundamental's level change
between dynamics is a felt-fit target too (it fixes the speed ratio one layer pair
leaves open): speeds mf 2.1, ff 7.0 (pp 0.88) m/s, error 3.1 dB. Upright design
curves (`scale.py DESIGN`, assumed): speaking length C4 0.57 m / A0 1.15 m, bass
tension 1300 N, hammers −15 %, soundboard modal density 1/26 Hz (smaller board),
singles to D1, bichords to A#2. Room decay: too few consistent bands between the
partials, so the lower quartile per band (smoothed, held above 2.5 kHz): 1.5 s
around 200 Hz, 0.3–0.6 s in the mids. Fits: soundboard residual 13.3 dB, coupling
5.4 dB (beat depth recording 3.18 / model 3.16 dB at the detune scale 1.0), knock
11.1 → 9.6 dB; release noise, damper and the pedal ring from Salamander (as Iowa).
Keyboard check vs the recordings: brightness change 4.9 dB (Iowa 6.3), spectral
shape 8.8 dB (Iowa 7.5), decay ratio 0.99 (Iowa 1.15). The export marks the body
(`"body": "upright"`); the engine then keeps the recording's microphone position (the
positions model a grand with its lid; step 16 lifts this).

<a id="step-16"></a>
## 16. Microphone positions for the upright

Microphone positions for the upright (`mic::Body::Upright`): the geometry became data
(`Body`: bridge points, radiating sources per zone with their normal, gain and
lowpass, flat reflectors, case, microphone placement); the grand is `Body::Grand`,
bit-identical at all five positions. The upright's board stands in the x–z plane
against a wall (0.15 m behind it; 1.2 m for close 2) and radiates from its back face
(lowpass 4 kHz: ribs and back posts); its front face's sound leaves through the open
top (unobstructed) and the upper front panel (lowpass 2 kHz), each −6 dB. One-sided
sources reach their back side by diffraction around the case (−10 dB, lowpass
500 Hz). Reflectors: floor, wall, the top lid opened 95° (leaning back). Positions
share the grand's slots: player (1.0 m from the board), audience (3.5 m), close 1 =
open top (0.3 m above it), close 2 = back (0.4 m behind the board). Measurements
(`proto/mic_check.py`, chromatic scale, mf, first 0.25 s per note, re the recording
position; spectral centroid per register bass / middle / treble):

| position | level | centroid | C50 |
|---|---|---|---|
| player | +0.2 dB | −2.9 / −2.7 / −1.6 % | −3.8 dB |
| audience | +0.1 dB | −9.9 / −3.2 / −1.0 % | −4.7 dB |
| close 1: open top | −2.5 dB | −6.1 / −1.5 / −0.2 % | −3.4 dB |
| close 2: back | −1.0 dB | +4.4 / +2.7 / −1.7 % | −3.5 dB |

(recording C50 −4.1 dB). C50 falls with the distance (player 1 → 10 m: −3.8 → −4.6
dB). Close 2 is drier than the roomy recording position (the upright's room rings
longest around 200 Hz), so brighter in the bass register. A one-sided source's paths
are blocked when the microphone is behind its face (the back face's wall image
reaches the front only around the case), and a reflection whose legs pass through
the lid is dropped (the open top's wall path behind the lid doubled the lid
reflection: a comb from 2.4 kHz). The same metric gives the grand's close: hammers
+7.2 % in the middle register (step 14: +15 % by its own measure).
Cost: close positions 16.3–16.6 % mean load (heavy half-pedal passage, 64-frame
buffers), as the grand's close: hammers (16.3 %); the recording position 8.2 %.

<a id="step-17"></a>
## 17. Grand C and Upright B

Two more instruments from VCSL Keys (CC0): Grand C (profile `grandk`, grand "K": 38
notes A♯0–C8, mostly 4 layers, levels as recorded) and Upright B (profile `uprighty`,
upright "Y": 14 notes about every 6 semitones, 3 layers, levels as recorded). Both are
level-keeping sets (`LEVEL_TARGET`). Their notes lack some layers (Grand K's top
treble has no soft ones), so the dynamics follow the file's layer number (Grand K v1
pp, v2 mf, v4 ff; Upright Y vl1/vl2/vl3). Upright Y's file mapped to C1 sounds B♭0
(partials ~29.5 Hz apart): the loader relabels it.
Pipeline changes (the existing profiles' fits unchanged where they were run):
- felt fit: a support note counts with any of the dynamics pairs (Grand K's bass has
  no clean pp); without pp in the top treble, the level step ff−mf; ff speeds up to
  14 m/s (Grand K's ff is 25–40 dB above its pp; the fit sat at the old 10 m/s edge).
- Upright Y keeps the straight felt line (its neighboring support felts were 10×
  apart) and picks its speeds, among the sets within 0.2 dB of the best (the error
  landscape is flat), by the recorded ff−mf step (10.1 dB; the best set gave 4 dB).
- identification: the low data edge after 8 rows instead of 20 for these sparse
  sets (Grand K's edge was 92 Hz and H held its 0 dB maximum below it: a boomy bass
  with fundamentals ~30 dB too strong).
- room decay: a band needs ¾ of the most estimates any band has, at most 20 (Upright
  Y has 14 notes).
- register gain: the smoothing widens with the note spacing above a whole tone
  (Upright Y 10.5 semitones: it followed single loud recordings, +8 dB at G2, and the
  demo's ff chord clipped).
- the peak table behind the output calibration and the release-noise levels
  (`release.peak_table`) was cached without regard to the profile: Iowa and Knight
  had kept the peaks from before their register and felt fits (Iowa's demo peaked at
  +3.9 dBFS; calibration now −9.4 dB, Knight +2.4 dB, release levels up to 14 dB per
  note). The cache is keyed on the analysis and the profile now; both re-exported.
Fits: Grand C speeds pp 0.30, mf 1.10, ff 7.0 m/s (felt 2–4 N in the middle, ~40 N
at the top; error 2.7 dB), soundboard residual 10.3 dB, coupling 10.9 → 6.9 dB (beat
depth recording 3.0 / model 2.7 dB), string scatter 0.04 cents·√k, knock 9.8 → 9.4
dB; keyboard check brightness change 6.3 dB, spectral shape 7.1 dB, decay ratio 1.06
(Iowa 6.3 / 7.5 / 1.15). Upright B speeds pp 0.6, mf 3.0, ff 10 m/s, felt
10.8 N · 2^((note − 60)/15.7), soundboard 14.2 dB, coupling 10.0 → 8.8 dB (depth 4.4 /
3.8 dB), scatter 0.03; keyboard check 11.1 / 10.4 dB / 0.94 (14 notes: the player's
note-to-note level scatter of ±6 dB stays). Demo levels re C4 (rms) follow the
recordings within 2 dB for Grand C; Upright B's C2 is 7.6 dB too strong.
Upright B's microphone positions are 16–45 % brighter in the middle register than its
recording position (Knight: within 3 %): the recording body and the zones' mode shapes
are independent random draws, and the small board's few modes below 1 kHz make their
brightness vary (open; a different board seed would change it).

<a id="step-18"></a>
## 18. A harpsichord

A harpsichord (profile `harpsichord`, VCSL "Harpsichord, Unk": 61 notes F1–F6, one
layer, release samples; equal temperament at 440 Hz). Its strings are plucked: the
string is released from rest out of a triangle with its apex at the pluck point β,
a_k = A·2/(π²β(1−β))·sin(kπβ)/k²·1/√(1 + (f_k/f_q′)²)·10^(3·s/20), A = 1.5 mm,
f_q′ = 20·f0·hardness·2^(0.5·s), s = log2 of the pluck "speed" (pp 0.5, mf 1 = the
recording, ff 2; Penttinen 2006: harmonics +3 dB and brighter, the knock +10 dB per
dynamic step), the horizontal polarization `horiz_gain` × the vertical. The engine adds
these starting amplitudes onto the string modes (`Voice::pluck`, like the pedal ring):
no contact phase, so a pluck costs less than a strike. Each note's excitation is typed
in the export (format 5, `"excitation": {"type": "hammer" | "pluck", …}`); format-4
exports load as hammer instruments (the pianos' renders are bit-identical, also
re-exported in format 5).
Pipeline: `fit_pluck.py` fits β per note from the recorded notch pattern (smooth
envelope + comb; 2β-ambiguous notes dropped against the running median, twice: they
cluster at A1–C#3): 0.09 in the bass rising to 0.27 at the top (47 notes); f_q is a design value (with one layer it
trades against H and C). `datasets.loudest` gives the one-layer set its "mf" where the
fits used "ff"; single strings store the fitted coupling factor as `kf1`; the knock's
velocity exponent is fixed (one layer: q = 1.66); release noise from its own release
samples (`release_noise.py harpsichord`, −27.3 dB re the note, both SFZ volumes); the damper decay stays
Salamander's (the sustain files end in an edit fade). The register gain is held beyond
the outermost recorded notes (the local-linear fit ran on to −93 dB at A7).
Fits: soundboard residual 5.5 dB (the lowest of the six sets), coupling 12.2 → 10.4 dB,
register gain +11.8 (bass) … −14.6 dB (top), level error 3.0 dB rms, knock 6.7 → 6.5 dB;
keyboard check: spectral shape 5.3 dB, decay ratio 1.06 (both the best of the six).
Engine vs prototype: both equal the closed-form modal sum from m = 2 after the release
(tests `harpsichord_pluck_matches_the_closed_form`, `test_pluck_render_is_the_closed_form_from_m_2`).
Engine demo per dynamic: pp −13.5, mf −9.7, ff −6.2 dBFS peaks. Cost: 7.6 % mean load
(heavy half-pedal passage, 64-frame buffers; Salamander 8.8 %).
Open: the aftersound's envelope modulation is 5.2 dB in the recordings, 0.5 dB in the
model: with single strings only the two polarizations can beat, and the coupling fit
adds them in power (no beating). The highs (≥ 4 kHz re the fundamental's band) come out
+6.8 dB at mf (the quill lowpass at 10 or 6 partials changes it by ~2 dB only). The keys
above F6 (extrapolated) smear into broadband noise at 2–8 kHz. The mic positions are
25–32 % brighter in the middle register than the recording position (as Upright B's).

<a id="step-19"></a>
## 19. Temperaments

Temperaments (`engine/src/temperament.rs`): computed from their definitions, the
narrowing of each of the twelve fifths (cents; they add up to the Pythagorean
comma): equal; meantone 1/4 and 1/6 syntonic comma (eleven fifths E♭…G♯, the wolf
G♯–E♭); Werckmeister III (C–G–D–A, B–F♯ by 1/4 Pythagorean comma); Kirnberger III
(C–G–D–A–E by 1/4 syntonic comma, F♯–C♯ by the schisma); Vallotti (F…B by 1/6
Pythagorean comma); Young II (C…F♯ likewise); Lehman's reading of Bach (F…E by 1/6,
C♯…A♯ by 1/12, A♯–F 1/12 wide). Checked against Aeolus's tables (Fons Adriaensen,
GPL-3; only the numbers are compared, nothing is copied): exact for meantone 1/4,
Werckmeister III and Lehman; its Kirnberger III table is rounded (≤ 0.7 cents) and its
"Vallotti" is a syntonic-comma variant (checked against the published cents instead).
`Tuning::temperament` holds the cents re equal per pitch class, the scale centered on
a key and shifted so that A keeps the concert pitch; `physics::compile` retunes each
note by its class's offset like the concert pitch (tension ∝ f², so the bridge force
follows). Equal temperament is exactly zero: the renders are bit-identical. Plugin:
"Temperament" and "Temperament key" in the tuning group (a change rebuilds the
engine, the notes go on); `PeTuning` gained `temperament`, `temperament_key`, and
`pe_temperament_count`/`pe_temperament_name` list them. Measured through the LV2
plugin: E4 in 1/4-comma meantone on C −3.4 cents re equal (expected −3.42).

<a id="step-20"></a>
## 20. Registers: Harpsichord B

Registers: Harpsichord B (instrument `flemish`, VCSL "Harpsichord, Flemish": an 8′ and
a 4′, keys F♯1–C6 in whole tones, 28 and 26 notes, one layer, release samples; the 4′
files in `Sustains/High` sound an octave above their key). An instrument is a list of
registers, each 88 notes keyed by the key played. Export format 6
(`export.py --profile flemish8 --add-register flemish4 --octave 12 --name flemish`):
`"registers": [{"name", "notes", "correction"?}]`; formats 4/5 load as one register.
The engine flattens them register-major (`notes[r·88 + key − 21]`, `Note.register`),
so voices, the sympathetic bank, the radiation zones, the release bursts and the
handover cover every register unchanged; register r ≥ 1 compiles with its own
correction. The added register plays through the main one's body and room: its
correction is C₄(f, key + 12) + H₄(f) − H₈(f) + balance(key), the balance being the
recorded 4′/8′ peak difference (+ SFZ volumes) minus the models', smoothed over 4
semitones and held beyond the recorded keys (−0.6 … +6.2 dB).
Stops: `Params.stops` (bit r = register r, default 1), live: a note-on excites the
enabled registers; a change leaves ringing strings alone; key up damps the key in
every register, one release noise per register the note-on excited; one knock per
key press (register 0's, none with all stops off); calibration and the pedal thumps
from register 0. A one-register instrument ignores the stops. Plugin: "Stop 8′"
(default on) and "Stop 4′" (off) in the instrument group; `PeParams.stops`;
`RENDER_STOPS` / `BENCH_STOPS` for the offline tools.
Profiles: `flemish8` (the harpsichord design), `flemish4` (analyzed at the sounding
note, `key` kept; design `harpsichord4`: per key a string an octave above the 8′'s,
half its length, 0.6 × its tension; assumed), one board seed. Each identifies its own
H from its own recordings. The soundboard's low data edge took 8 rows (as Grand C):
8′ 165 → 92 Hz, 4′ 369 → 203 Hz (the 4′'s H had been held flat 5 dB above its
neighbors). Profile curves far beyond the data (the 4′ sounds up to C8) are held
where the data still carries them instead of turning NaN.
Fits (8′ / 4′): pluck point 0.11 → 0.50 / 0.08 → 0.41 (26 of 28 / 25 of 25 notes),
soundboard residual 7.6 / 4.9 dB, coupling 9.5 → 7.9 / 12.7 → 11.5 dB, register gain
+11.7 … −10.3 / +4.1 … −24.0 dB, level error 1.7 / 1.7 dB rms, knock 6.3 → 6.1 /
8.6 → 5.5 dB, keyboard shape 5.6 / 4.6 dB, decay ratio 1.06 / 1.03; release noise
−24.2 / −19.9 dB re the note.
Against the recordings (demo, recordings at file level + SFZ volume; 8′+4′ against the
sum of both, as the set's "Full" mix): the 4′/8′ level per key in the chromatic run
(first 0.25 s) differs from the recordings' by +0.0 dB median, within ±3 dB on the
recorded keys but D♯2 (+4.8); above C6 the model's 4′ is 4–11 dB below the
pitch-shifted top samples. Long-term spectra of the run (sustain, matched at
200–1600 Hz): within ±2.4 dB (8′), ±5.7 (4′, the 50–100 Hz band), ±1.3 (8′+4′). The
held chords come out darker in the sustain (3–20 kHz −5 … −15 dB; attacks within
±5 dB): in the spectrograms the 4′'s partials above ~6 kHz die within ~1 s, the
recordings' last ~3 s. The single held notes (C2, C4, C6) differ in the outer bands
as the first harpsichord's do. No stripes or combing. Demo peaks: 8′ −3.1, 4′ −5.7,
8′+4′ −2.3 dBFS. Cost with both stops: 13.1 % mean load (pedalled ff notes, 64-frame
buffers; 8′ alone 10.0 %, Salamander 12.6 %).

<a id="step-21"></a>
## 21. Plugin UI

Plugin UI: DPF/NanoVG, the parameter table shared with the DSP
(`plugin/src/Params.hpp`); pure logic in `plugin/src/ui/` (formatting, per-instrument
capabilities, layout, drawing transforms and dragging; `plugin/tests/ui_tests.cpp`). The
microphone drawing comes from the engine: `Body::placement` is the code that places the
pair for the sound, returning its limits and drag axes too (the grand's close pair: the
treble-side microphone's height, the lid stopping the bass side first), and `pe_mic_geometry`
hands it with the case outlines to the UI, so the drawing cannot disagree with the sound.
Controls without effect per instrument by test (`engine/tests/no_effect.rs`: the
harpsichords' pedal noise, hammer grooves, unison width, stretch and unison wear). The UI's state
(open sections, zoom) is a plugin state key; for LV2 DPF then adds an event output port
(control port indices +1, symbols unchanged) and requires the worker feature.
