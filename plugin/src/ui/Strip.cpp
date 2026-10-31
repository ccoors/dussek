#include "Strip.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>

namespace pe {

namespace {
constexpr float kBlackWidth = 0.62f, kBlackHeight = 0.62f;
}

bool isBlackKey(int key)
{
    switch ((21 + key) % 12) { // MIDI pitch class
    case 1: case 3: case 6: case 8: case 10: return true;
    default: return false;
    }
}

StripGeometry computeStrip(const Rect& area)
{
    StripGeometry g;
    g.area = area;
    g.pedal = { area.x + area.w - kPedalWidth + 4, area.y + 2, kPedalWidth - 8, area.h - 4 };
    g.sostenuto = { g.pedal.x - kSostenutoWidth, area.y + 2, kSostenutoWidth - 8, area.h - 4 };
    g.soft = { g.sostenuto.x - kSoftWidth, area.y + 2, kSoftWidth - 8, area.h - 4 };
    g.keysArea = { area.x, area.y, area.w - kPedalWidth - kSostenutoWidth - kSoftWidth, area.h };
    int whites = 0;
    for (int k = 0; k < kKeys; ++k) whites += !isBlackKey(k);
    const float w = g.keysArea.w / (float)whites;
    g.keys.resize(kKeys);
    int white = 0;
    for (int k = 0; k < kKeys; ++k) {
        if (!isBlackKey(k)) {
            // edges from the index, so neighbors share an identical edge (half-open contains: no gap, no overlap)
            const float x0 = g.keysArea.x + white * w, x1 = g.keysArea.x + (white + 1) * w;
            g.keys[k] = { { x0, area.y, x1 - x0, area.h }, false };
            ++white;
        }
    }
    for (int k = 0; k < kKeys; ++k) {
        if (isBlackKey(k)) { // over the boundary between its two white neighbors
            const float boundary = g.keys[k + 1].r.x; // the next key is always white
            const float bw = w * kBlackWidth;
            g.keys[k] = { { boundary - bw / 2, area.y, bw, area.h * kBlackHeight }, true };
        }
    }
    return g;
}

int hitKey(const StripGeometry& g, float x, float y, int& velocity)
{
    int hit = -1;
    for (int pass = 0; pass < 2 && hit < 0; ++pass) // black keys first: they lie on top
        for (int k = 0; k < (int)g.keys.size(); ++k)
            if (g.keys[k].black == (pass == 0) && g.keys[k].r.contains(x, y)) { hit = k; break; }
    if (hit < 0) return -1;
    const Rect& r = g.keys[hit].r;
    const float t = std::min(1.0f, std::max(0.0f, (y - r.y) / r.h));
    velocity = std::min(127, std::max(1, (int)std::lround(20 + 107 * t)));
    return hit;
}

float pedalDepthAt(const StripGeometry& g, float y)
{
    return std::min(1.0f, std::max(0.0f, (y - g.pedal.y) / g.pedal.h));
}

float softDepthAt(const StripGeometry& g, float y)
{
    return std::min(1.0f, std::max(0.0f, (y - g.soft.y) / g.soft.h));
}

KeyLight keyLight(const float words[8], int key)
{
    KeyLight l;
    if (key < 0 || key >= kKeys) return l;
    const float f = words[key / 12];
    if (!(f >= 0.0f && f < 16777216.0f)) return l; // also rejects NaN
    const uint32_t bits = ((uint32_t)f >> (2 * (key % 12))) & 3u;
    l.held = bits & 1u;
    l.ringing = bits & 2u;
    return l;
}

float keyCenterX(const StripGeometry& g, int key)
{
    key = std::min(kKeys - 1, std::max(0, key));
    return g.keys[key].r.x + g.keys[key].r.w / 2;
}

float freqToStripX(const StripGeometry& g, float hz)
{
    const float kf = hz > 0 ? 12.0f * std::log2(hz / 27.5f) : 0.0f;
    const float c = std::min((float)(kKeys - 1), std::max(0.0f, kf));
    const int i = std::min(kKeys - 2, (int)std::floor(c));
    const float t = c - (float)i;
    return keyCenterX(g, i) * (1 - t) + keyCenterX(g, i + 1) * t;
}

void StripPlayer::play(const StripGeometry& g, float x, float y)
{
    int v = 0;
    const int k = hitKey(g, x, y, v);
    if (k == fCurrent) return;
    if (fCurrent >= 0) fOff(fCurrent);
    fCurrent = k;
    if (k >= 0) fOn(k, v);
}

void StripPlayer::press(const StripGeometry& g, float x, float y) { release(); play(g, x, y); }
void StripPlayer::move(const StripGeometry& g, float x, float y) { play(g, x, y); }
void StripPlayer::release()
{
    if (fCurrent >= 0) fOff(fCurrent);
    fCurrent = -1;
}

} // namespace pe
