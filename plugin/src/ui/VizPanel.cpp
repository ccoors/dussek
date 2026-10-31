#include "VizPanel.hpp"
#include "Theme.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>

namespace pe {
using DGL_NAMESPACE::Color;
using DGL_NAMESPACE::NanoVG;

namespace {
constexpr int kSamples = 24;          // points along a string
constexpr float kRingGain = 2.5f;     // the strings ringing along, drawn this much larger than they move
constexpr float kHammerZone = 58;     // the hammers' row under the strings (px)
constexpr float kFlashS = 0.12f;      // a strike's flash at the string
constexpr float kSounding = 3e-8f;    // a string's level (m) from which it is drawn
constexpr float kTwoPi = 6.28318530718f;

uint64_t nowMs()
{
    return (uint64_t)std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now().time_since_epoch()).count();
}

Color hue(int key, float scale, float alpha)
{
    float c[3];
    keyHue(key, c);
    return Color(std::min(1.0f, c[0] * scale), std::min(1.0f, c[1] * scale), std::min(1.0f, c[2] * scale), alpha);
}

/// the key's displacement per partial with the ring added in power, and its level (m)
float combined(const PeTelemetryKey& k, float* amp)
{
    float sum = 0;
    for (int i = 0; i < PE_VIZ_PARTIALS; ++i) {
        amp[i] = std::hypot(k.amp[i], kRingGain * k.ring[i]);
        sum += amp[i] * amp[i];
    }
    return std::sqrt(0.5f * sum);
}

float brightness(float level) { return std::clamp(std::pow(level / 6e-4f, 0.35f), 0.08f, 1.0f); }

bool heldBit(const PeTelemetryFrame& f, int key) { return (f.keys[key / 12] >> (2 * (key % 12)) & 1u) != 0; }

uint32_t lcg(uint32_t& s) { s = s * 1664525u + 1013904223u; return s; }
}

void VizPanel::open()
{
    if (!fFrame) fFrame.reset(new PeTelemetryFrame());
    fOpen = true;
    fHaveFrame = false;
    fAvailable = fClient.open(fToken);
    if (fAvailable) fClient.setEnabled(true);
}

void VizPanel::close()
{
    fClient.close(); // clears the enabled flag: the engine stops collecting
    fAvailable = false;
    fOpen = false;
    fHaveFrame = false;
}

void VizPanel::setToken(uint32_t token)
{
    if (token == fToken) return;
    fToken = token;
    if (fOpen) { open(); fRepaint(); }
}

void VizPanel::layout(const Rect& area, const StripGeometry& strip)
{
    fArea = area;
    fStrip = strip;
    fPlot = { strip.keysArea.x, area.y + 6, strip.keysArea.w, area.h - 12 };
}

uint64_t VizPanel::engineNow() const
{
    if (!fHaveFrame) return 0;
    const uint64_t ms = std::min<uint64_t>(nowMs() - fFrameAtMs, 200); // a stalled engine: the picture stops
    return fFrame->position + (uint64_t)((double)ms * fRate / 1000.0);
}

void VizPanel::poll()
{
    if (!fAvailable || !fClient.pollFrame(*fFrame)) return;
    fHaveFrame = true;
    fFrameAtMs = nowMs();
    if (fFrame->rate > 0 && std::isfinite(fFrame->rate)) fRate = fFrame->rate;
    fAnyHammer = fSounding = false;
    float amp[PE_VIZ_PARTIALS];
    for (int k = 0; k < kKeys; ++k) {
        const PeTelemetryKey& key = fFrame->key[k];
        HammerTrack& h = fHammers[k];
        if (key.struck_at != h.struckAt) h = { key.struck_at, key.speed, 0 };
        const bool held = heldBit(*fFrame, k);
        // released since the strike (also a key struck and released between two frames)
        if (!held && h.struckAt != 0 && h.upAt <= h.struckAt) h.upAt = std::max(fFrame->position, h.struckAt + 1);
        fAnyHammer = fAnyHammer || (key.flags & PE_KEY_HAMMER) != 0;
        fSounding = fSounding || combined(key, amp) > kSounding;
    }
}

void VizPanel::idle()
{
    if (!fOpen) return;
    const uint64_t before = fFrameAtMs;
    poll();
    // a new frame, a sounding string (its snapshot), or a hammer still moving between frames
    // (struck or released within the second)
    bool moving = fHaveFrame && (fFrameAtMs != before || fSounding);
    const uint64_t now = engineNow(), second = (uint64_t)fRate;
    for (int k = 0; k < kKeys && fHaveFrame && !moving; ++k) {
        const HammerTrack& h = fHammers[k];
        moving = (h.struckAt != 0 && now - h.struckAt < second) || (h.upAt != 0 && now - h.upAt < second);
    }
    if (moving) fRepaint();
}

void VizPanel::draw(NanoVG& vg)
{
    if (!fOpen) return;
    vg.beginPath(); vg.rect(fArea.x, fArea.y, fArea.w, fArea.h); vg.fillColor(Color(12, 14, 24)); vg.fill();
    vg.fontSize(theme::smallFont);
    if (!fAvailable) {
        vg.fillColor(theme::dim);
        vg.textAlign(NanoVG::Align(NanoVG::ALIGN_CENTER | NanoVG::ALIGN_MIDDLE));
        vg.text(fArea.x + fArea.w / 2, fArea.y + fArea.h / 2, "Visualization unavailable: no shared memory with the engine (sandboxed host?)", nullptr);
        return;
    }
    const uint64_t now = engineNow();
    vg.save();
    vg.scissor(fArea.x, fArea.y, fArea.w, fArea.h);
    if (fBars) drawBars(vg, now);
    else {
        drawStrings(vg, now);
        if (fAnyHammer) drawHammers(vg, now);
    }
    vg.restore();
    vg.fillColor(theme::faint);
    vg.textAlign(NanoVG::Align(NanoVG::ALIGN_LEFT | NanoVG::ALIGN_TOP));
    vg.text(fArea.x + 8, fArea.y + 6, fBars ? "the bars as the engine moves them, exaggerated" : "the strings as the engine moves them, exaggerated", nullptr);
}

void VizPanel::drawStrings(NanoVG& vg, uint64_t now)
{
    const float pitch = fPlot.w / kKeys;
    fFront = fPlot.y + fPlot.h - (fAnyHammer ? kHammerZone : 6.0f);
    const float room = fFront - (fPlot.y + 18);
    float longest = 0;
    for (int k = 0; k < kKeys && fHaveFrame; ++k) longest = std::max(longest, fFrame->key[k].length);
    uint32_t seed = ++fSeed;
    for (int k = 0; k < kKeys; ++k) {
        const float x = fPlot.x + pitch * ((float)k + 0.5f);
        const PeTelemetryKey* key = fHaveFrame ? &fFrame->key[k] : nullptr;
        const float len = key ? stringPx(key->length, longest, room) : room * 0.5f;
        const float top = fFront - len;
        // the idle string and its hitch pin
        vg.beginPath(); vg.moveTo(x, fFront); vg.lineTo(x, top);
        vg.strokeColor(Color(70, 74, 88)); vg.strokeWidth(1); vg.stroke();
        vg.beginPath(); vg.circle(x, top, 1.6f); vg.fillColor(Color(110, 100, 80)); vg.fill();
        if (!key) continue;
        float amp[PE_VIZ_PARTIALS];
        const float level = combined(*key, amp);
        if (level > kSounding) {
            const float bright = brightness(level);
            float w[kSamples + 1], widest = 0;
            for (int i = 0; i <= kSamples; ++i) {
                w[i] = pitch * displacementPitches(shapeEnvelope(amp, PE_VIZ_PARTIALS, (float)i / kSamples)) + 0.6f;
                widest = std::max(widest, w[i]);
            }
            // the glow around the band, the band, then a snapshot of the string inside it
            const float feather = 1.5f * widest + 5;
            vg.beginPath(); vg.rect(x - widest - feather, top - feather, 2 * (widest + feather), len + 2 * feather);
            vg.fillPaint(vg.boxGradient(x - widest, top, 2 * widest, len, widest, feather, hue(k, bright, 0.35f * bright), Color(0, 0, 0, 0.0f)));
            vg.fill();
            vg.beginPath();
            for (int i = 0; i <= kSamples; ++i) {
                const float y = fFront - len * (float)i / kSamples;
                if (i == 0) vg.moveTo(x - w[i], y); else vg.lineTo(x - w[i], y);
            }
            for (int i = kSamples; i >= 0; --i) vg.lineTo(x + w[i], fFront - len * (float)i / kSamples);
            vg.closePath();
            vg.fillColor(hue(k, bright * 0.75f, 0.85f)); vg.fill();
            float phase[PE_VIZ_PARTIALS];
            for (float& p : phase) p = (float)(lcg(seed) >> 8) / 16777216.0f * kTwoPi;
            vg.beginPath();
            for (int i = 0; i <= kSamples; ++i) {
                const float u = (float)i / kSamples;
                const float y = shapeSnapshot(amp, phase, PE_VIZ_PARTIALS, u);
                const float dx = (y < 0 ? -1.0f : 1.0f) * pitch * displacementPitches(y);
                if (i == 0) vg.moveTo(x + dx, fFront - len * u); else vg.lineTo(x + dx, fFront - len * u);
            }
            vg.strokeColor(hue(k, 0.4f + 1.1f * bright, 1.0f)); vg.strokeWidth(1.4f); vg.stroke();
        }
        // a pluck's or strike's flash at its point
        const float since = key->struck_at != 0 && now >= key->struck_at ? (float)(now - key->struck_at) / fRate : 1e9f;
        if (since < kFlashS) {
            const float y = (key->flags & PE_KEY_HAMMER) ? fFront : fFront - len * std::clamp(key->point, 0.0f, 1.0f);
            const float a = 1.0f - since / kFlashS;
            vg.beginPath(); vg.circle(x, y, 14);
            vg.fillPaint(vg.radialGradient(x, y, 0, 14, hue(k, 1.2f, 0.9f * a), Color(0, 0, 0, 0.0f)));
            vg.fill();
        }
        // the damper on the strings; lifted it rises (fainter, drawn off the string a little)
        if (key->flags & PE_KEY_DAMPER) {
            const float e = std::clamp(key->engage, 0.0f, 1.0f);
            const float y = fFront - std::max(8.0f, 0.16f * len) - (1 - e) * 6;
            const int shade = (int)(40 + 50 * e);
            vg.beginPath(); vg.roundedRect(x - pitch * 0.38f, y - 5, pitch * 0.76f, 10, 2);
            vg.fillColor(Color(shade, shade - 8, shade - 16, 0.45f + 0.55f * e)); vg.fill();
        }
    }
    vg.beginPath(); vg.moveTo(fPlot.x, fFront); vg.lineTo(fPlot.x + fPlot.w, fFront);
    vg.strokeColor(Color(90, 90, 100)); vg.strokeWidth(1); vg.stroke();
}

void VizPanel::drawHammers(NanoVG& vg, uint64_t now)
{
    const float pitch = fPlot.w / kKeys;
    const float travel = kHammerZone - 14;
    const float bottom = fPlot.y + fPlot.h;
    const uint64_t frame = (uint64_t)(fRate / 60.0f);
    for (int k = 0; k < kKeys; ++k) {
        const float x = fPlot.x + pitch * ((float)k + 0.5f);
        const HammerTrack& h = fHammers[k];
        const float height = hammerHeight(h, now, fRate);
        // a few exposures over the last frame for the motion, then the hammer on its shank
        for (int s = 3; s >= 0; --s) {
            const uint64_t at = now - std::min<uint64_t>(now, (uint64_t)s * frame / 3);
            const float hs = s == 0 ? height : at <= h.struckAt ? 0.0f : hammerHeight(h, at, fRate);
            if (s != 0 && std::fabs(hs - height) < 0.01f) continue;
            const float y = fFront + 3 + (1 - hs) * travel;
            const float wt = s == 0 ? 1.0f : 0.25f + 0.15f * (float)(3 - s);
            if (s == 0) {
                vg.beginPath(); vg.moveTo(x, y + 10); vg.lineTo(x, bottom);
                vg.strokeColor(Color(120, 95, 70)); vg.strokeWidth(1.5f); vg.stroke();
            }
            vg.beginPath(); vg.roundedRect(x - pitch * 0.4f, y, pitch * 0.8f, 10, 3);
            vg.fillColor(Color(225, 215, 195, wt)); vg.fill();
        }
    }
}

void VizPanel::drawBars(NanoVG& vg, uint64_t now)
{
    const float pitch = fPlot.w / kKeys;
    const float mid = fPlot.y + 10 + (fPlot.h - 10) / 2;
    float longest = 0;
    for (int k = 0; k < kKeys && fHaveFrame; ++k) longest = std::max(longest, fFrame->key[k].length);
    uint32_t seed = ++fSeed;
    for (int k = 0; k < kKeys; ++k) {
        const float x = fPlot.x + pitch * ((float)k + 0.5f);
        const PeTelemetryKey* key = fHaveFrame ? &fFrame->key[k] : nullptr;
        const float len = key ? stringPx(key->length, longest, fPlot.h - 24) : (fPlot.h - 24) * 0.5f;
        const float top = mid - len / 2, bw = pitch * 0.72f;
        float amp[PE_VIZ_PARTIALS] = {};
        const float level = key ? combined(*key, amp) : 0.0f;
        const float bright = level > kSounding ? brightness(level) : 0.0f;
        if (bright > 0) {
            const float feather = 10 + 14 * bright;
            vg.beginPath(); vg.rect(x - bw - feather, top - feather, 2 * (bw + feather), len + 2 * feather);
            vg.fillPaint(vg.boxGradient(x - bw / 2, top, bw, len, 3, feather, hue(k, bright, 0.4f * bright), Color(0, 0, 0, 0.0f)));
            vg.fill();
        }
        vg.beginPath(); vg.roundedRect(x - bw / 2, top, bw, len, 2);
        vg.fillColor(Color(Color(58, 56, 60), hue(k, 0.9f, 1.0f), 0.85f * bright)); vg.fill();
        if (bright > 0) { // a snapshot of the bar's bending along it
            float phase[PE_VIZ_PARTIALS];
            for (float& p : phase) p = (float)(lcg(seed) >> 8) / 16777216.0f * kTwoPi;
            vg.beginPath();
            for (int i = 0; i <= kSamples; ++i) {
                const float u = (float)i / kSamples;
                const float y = shapeSnapshot(amp, phase, PE_VIZ_PARTIALS, u);
                const float dx = (y < 0 ? -1.0f : 1.0f) * pitch * 0.5f * displacementPitches(y);
                if (i == 0) vg.moveTo(x + dx, top + len * u); else vg.lineTo(x + dx, top + len * u);
            }
            vg.strokeColor(hue(k, 0.5f + bright, 1.0f)); vg.strokeWidth(1.2f); vg.stroke();
        }
        if (!key) continue;
        const float since = key->struck_at != 0 && now >= key->struck_at ? (float)(now - key->struck_at) / fRate : 1e9f;
        if (since < kFlashS) {
            const float a = 1.0f - since / kFlashS;
            vg.beginPath(); vg.circle(x, mid, 16);
            vg.fillPaint(vg.radialGradient(x, mid, 0, 16, hue(k, 1.2f, 0.9f * a), Color(0, 0, 0, 0.0f)));
            vg.fill();
        }
        // the damper bar's felt under the bar's end (the vibraphone, the chimes)
        if (key->flags & PE_KEY_DAMPER) {
            const float e = std::clamp(key->engage, 0.0f, 1.0f);
            const int shade = (int)(40 + 50 * e);
            vg.beginPath(); vg.rect(x - bw / 2, top + len - 6 + (1 - e) * 5, bw, 4);
            vg.fillColor(Color(shade, shade - 8, shade - 16, 0.45f + 0.55f * e)); vg.fill();
        }
    }
}

} // namespace pe
