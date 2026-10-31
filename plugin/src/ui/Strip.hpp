// The player strip's geometry and playing logic (pure; tested in ui_tests): 88 keys
// (key 0 = A0 … 87 = C8), a pedal, hit-testing with velocity, the decoding of the engine's
// key words, the mapping from a frequency to the x of the key it belongs above.
#pragma once
#include "Layout.hpp"

#include <functional>
#include <vector>

namespace pe {

constexpr int kKeys = 88;
constexpr float kPedalWidth = 36;       // the sustain pedal's slot
constexpr float kSostenutoWidth = 28;   // the sostenuto pedal's slot, left of it
constexpr float kSoftWidth = 28;        // the soft pedal's slot, left of that

bool isBlackKey(int key);

struct KeyShape { Rect r; bool black = false; };
struct StripGeometry {
    Rect area, keysArea, pedal, sostenuto, soft;
    std::vector<KeyShape> keys;
};
StripGeometry computeStrip(const Rect& area);

/// the key under (x, y), or -1; `velocity` 1…127: softer at the top of a key, harder at the bottom
int hitKey(const StripGeometry& g, float x, float y, int& velocity);
/// 0 (top of the pedal slot) … 1 (bottom), clamped
float pedalDepthAt(const StripGeometry& g, float y);
/// the same for the soft pedal's lever
float softDepthAt(const StripGeometry& g, float y);

struct KeyLight { bool held = false, ringing = false; };
/// from the engine's 8 key words (as floats, exact to 2^24); out-of-range words read as silence
KeyLight keyLight(const float words[8], int key);

float keyCenterX(const StripGeometry& g, int key);
/// the x where the key coordinate 12·log2(f/27.5) falls (linear between key centers, clamped)
float freqToStripX(const StripGeometry& g, float hz);

/// Plays notes from pointer events: press starts a note, moving to another key ends it and
/// starts the next (glissando), leaving the keys or `release` ends it.
class StripPlayer {
public:
    StripPlayer(std::function<void(int key, int velocity)> on, std::function<void(int key)> off)
        : fOn(std::move(on)), fOff(std::move(off)) {}
    void press(const StripGeometry& g, float x, float y);
    /// Note: without a preceding `press`, `move` over a key starts a note (meant for drag re-entry);
    /// callers must gate `move` on a pressed state, or hovering will sound notes.
    void move(const StripGeometry& g, float x, float y);
    void release();
    int current() const { return fCurrent; }
private:
    void play(const StripGeometry& g, float x, float y);
    std::function<void(int, int)> fOn;
    std::function<void(int)> fOff;
    int fCurrent = -1;
};

} // namespace pe
