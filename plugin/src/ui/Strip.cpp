#include "Strip.hpp"
#include "Working.hpp"

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

DiagBar diagBar(float width, float y, float h, bool open, const StripGeometry& strip)
{
    DiagBar b;
    b.keyboard = { 8, y + 3, 96, h - 6 };
    b.analysis = { 112, y + 3, 84, h - 6 };
    b.viz = { 204, y + 3, 108, h - 6 };
    const Rect& last = open ? b.viz : b.keyboard;
    const float x = last.x + last.w + 8, w = textWidth(kWorkingLabel) + 8;
    // the pedals' names are centered over their columns: clear of the leftmost pedal's
    float end = width;
    if (open)
        for (const Rect* r : { &strip.soft, &strip.sostenuto, &strip.pedal })
            if (r->w > 0)
                end = std::min(end, r->x);
    end -= 8;
    if (x + w <= end)
        b.working = { x, y + 3, w, h - 6 };
    return b;
}

StripGeometry computeStrip(const Rect& area, Pedals pedals)
{
    StripGeometry g;
    g.area = area;
    // from the right edge: each pedal present takes its slot
    float right = area.x + area.w;
    const auto slot = [&](bool present, float width) {
        if (!present) return Rect { right, area.y + 2, 0, area.h - 4 };
        right -= width;
        return Rect { right + 4, area.y + 2, width - 8, area.h - 4 };
    };
    g.pedal = slot(pedals.sustain, kPedalWidth);
    g.sostenuto = slot(pedals.sostenuto, kSostenutoWidth);
    g.soft = slot(pedals.soft, kSoftWidth);
    // a pedal missing right of the others stands at the keys' edge too
    for (Rect* r : { &g.pedal, &g.sostenuto, &g.soft })
        if (r->w == 0) r->x = right;
    g.keysArea = { area.x, area.y, right - area.x, area.h };
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
