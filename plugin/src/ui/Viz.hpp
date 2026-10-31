// The Visualization panel's model (pure; tested in ui_tests): the hammers' heights from the
// engine's strikes and the keys' releases, a string's drawn length, its shape along the length
// from the partials' displacement, the keys' colors.
#pragma once
#include <cstddef>
#include <cstdint>

namespace pe {

/// the hammer at the string this long after the engine's strike (s): a strike lasts a few
/// milliseconds, which no frame would show
constexpr float kVizContactS = 0.03f;
/// the backcheck catches a held key's hammer this far up (0 rest … 1 at the string)
constexpr float kVizCheck = 0.45f;
/// the hammer's travel from rest to the string (m) and its fall after a release (heights per second)
constexpr float kVizBlowM = 0.047f, kVizDrop = 6.0f;

/// One key's hammer: its last strike (engine samples; 0: none), that strike's speed (m/s), and the
/// key's release after it (0: still held, or released before the strike)
struct HammerTrack { uint64_t struckAt = 0; float speed = 0; uint64_t upAt = 0; };

/// The hammer's height (0 rest … 1 at the string) at engine position `now` (`rate` Hz): at the
/// string from the strike for kVizContactS, then falling back at half its speed, caught by the
/// backcheck while the key is held, falling to rest after the release; never above the string.
/// No rise is drawn before a strike: a live note-on comes when the hammer arrives.
float hammerHeight(const HammerTrack& h, uint64_t now, float rate);

/// the drawn length of a string `length` m long, where the longest is `longest` m and `px` the
/// most there is: compressed (a 5 cm treble string beside a 2 m bass one stays visible)
float stringPx(float length, float longest, float px);

/// The shape along a string from its partials' displacement amp[0..n) (m): at u in 0 … 1, the
/// envelope sqrt(Σ amp_k² sin²(kπu)) (√2 × the RMS), and a snapshot Σ amp_k cos(φ_k) sin(kπu)
/// at the phases `phase` (rad, n of them)
float shapeEnvelope(const float* amp, size_t n, float u);
float shapeSnapshot(const float* amp, const float* phase, size_t n, float u);

/// a displacement (m) as drawn: a half width in key pitches, compressed (a millimeter about half a
/// pitch: the band as wide as the strings' spacing), at most kVizWidest
constexpr float kVizWidest = 0.75f;
float displacementPitches(float metres);

/// key 0 = A0 … 87 = C8: warm in the bass to cool in the treble, as 0 … 1 components
void keyHue(int key, float rgb[3]);

} // namespace pe
